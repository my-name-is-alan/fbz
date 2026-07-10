<script setup lang="ts">
import {
  createMarketSource,
  deleteMarketSource,
  getMarketCatalog,
  installMarketPlugin,
  listMarketSources,
  setMarketSourceEnabled,
  syncMarketSource,
} from "@/service/modules/admin.ts";
import { useUiStore } from "@/stores/ui.ts";
import type { PluginMarketCatalogItem, PluginMarketSource } from "@/types/admin.ts";

const router = useRouter();
const uiStore = useUiStore();

const sources = ref<PluginMarketSource[]>([]);
const catalog = ref<PluginMarketCatalogItem[]>([]);
const loading = shallowRef(false);
const catalogLoading = shallowRef(false);
const loadingAction = shallowRef("");
const error = shallowRef("");

// 新增市场源表单
const newSourceName = ref("");
const newSourceUrl = ref("");
const addingSource = shallowRef(false);

// catalog 过滤
const filterSourceId = ref<string>("");
const searchQuery = ref("");

const sourceFilterOptions = computed(() => [
  { label: "全部来源", value: "" },
  ...sources.value.map((s) => ({ label: s.name, value: s.id })),
]);

function formatTime(value: string | null | undefined): string {
  if (!value) return "从未同步";
  return new Intl.DateTimeFormat("zh-CN", {
    month: "2-digit",
    day: "2-digit",
    hour: "2-digit",
    minute: "2-digit",
  }).format(new Date(value));
}

/** 安装按钮文案：未装 / 可更新 / 已是最新。 */
function installLabel(item: PluginMarketCatalogItem): string {
  if (item.hasUpdate) return "更新";
  if (item.isInstalled) return "已安装";
  return "安装";
}

function installDisabled(item: PluginMarketCatalogItem): boolean {
  if (item.isInstalled && !item.hasUpdate) return true;
  return loadingAction.value === `install:${item.sourceId}:${item.pluginId}:${item.version}`;
}

onMounted(() => {
  void refreshSources();
  void refreshCatalog();
});

async function refreshSources() {
  loading.value = true;
  error.value = "";
  try {
    sources.value = await listMarketSources();
  } catch {
    error.value = "市场源加载失败，请确认后端已就绪且当前账号具备管理员权限。";
  } finally {
    loading.value = false;
  }
}

async function refreshCatalog() {
  catalogLoading.value = true;
  try {
    catalog.value = await getMarketCatalog({
      sourceId: filterSourceId.value || undefined,
      q: searchQuery.value.trim() || undefined,
    });
  } catch {
    uiStore.showToast("市场插件目录加载失败。", "error");
  } finally {
    catalogLoading.value = false;
  }
}

async function handleAddSource() {
  const name = newSourceName.value.trim();
  const url = newSourceUrl.value.trim();
  if (!name || !url) {
    uiStore.showToast("请填写市场源名称与地址。", "warning");
    return;
  }
  addingSource.value = true;
  try {
    await createMarketSource({ name, url });
    uiStore.showToast(`已添加市场源 ${name}。`, "success");
    newSourceName.value = "";
    newSourceUrl.value = "";
    await refreshSources();
  } catch {
    uiStore.showToast("添加市场源失败，请检查地址是否有效。", "error");
  } finally {
    addingSource.value = false;
  }
}

async function handleDeleteSource(source: PluginMarketSource) {
  loadingAction.value = `del:${source.id}`;
  try {
    await deleteMarketSource(source.id);
    uiStore.showToast(`已删除市场源 ${source.name}。`, "success");
    if (filterSourceId.value === source.id) filterSourceId.value = "";
    await refreshSources();
    await refreshCatalog();
  } catch {
    uiStore.showToast("删除市场源失败。", "error");
  } finally {
    loadingAction.value = "";
  }
}

async function handleSyncSource(source: PluginMarketSource) {
  loadingAction.value = `sync:${source.id}`;
  try {
    const result = await syncMarketSource(source.id);
    uiStore.showToast(`市场源 ${source.name} 已同步，拉取 ${result.synced} 个条目。`, "success");
    await refreshSources();
    await refreshCatalog();
  } catch {
    uiStore.showToast("同步市场源失败。", "error");
  } finally {
    loadingAction.value = "";
  }
}

async function handleToggleSource(source: PluginMarketSource) {
  loadingAction.value = `toggle:${source.id}`;
  try {
    await setMarketSourceEnabled(source.id, !source.enabled);
    uiStore.showToast(`市场源 ${source.name} 已${source.enabled ? "停用" : "启用"}。`, "success");
    await refreshSources();
    await refreshCatalog();
  } catch {
    uiStore.showToast("更新市场源启用状态失败。", "error");
  } finally {
    loadingAction.value = "";
  }
}

async function handleInstall(item: PluginMarketCatalogItem) {
  if (item.isInstalled && !item.hasUpdate) return;
  loadingAction.value = `install:${item.sourceId}:${item.pluginId}:${item.version}`;
  try {
    await installMarketPlugin({
      sourceId: item.sourceId,
      pluginId: item.pluginId,
      version: item.version,
    });
    uiStore.showToast(`${item.name} 已下载，请前往插件设置审批并激活后启用。`, "success");
    await refreshCatalog();
    void router.push({ path: "/admin/plugins", query: { tab: "packages" } });
  } catch {
    uiStore.showToast("安装失败，请检查市场源与插件包完整性。", "error");
  } finally {
    loadingAction.value = "";
  }
}
</script>

<template>
  <div class="market-view">
    <!-- 市场源管理 -->
    <section class="market-sources">
      <header class="section-head">
        <h4>市场源</h4>
        <button class="link-btn" type="button" :disabled="loading" @click="refreshSources">
          {{ loading ? "刷新中..." : "刷新" }}
        </button>
      </header>

      <p v-if="error" class="error-text">{{ error }}</p>

      <div class="add-source-form">
        <input
          v-model="newSourceName"
          class="text-input"
          type="text"
          placeholder="来源名称，如 官方市场"
          aria-label="市场源名称"
        />
        <input
          v-model="newSourceUrl"
          class="text-input grow"
          type="url"
          placeholder="市场索引地址 https://…"
          aria-label="市场源地址"
        />
        <button class="add-btn" type="button" :disabled="addingSource" @click="handleAddSource">
          {{ addingSource ? "添加中..." : "添加来源" }}
        </button>
      </div>

      <div v-if="sources.length === 0 && !loading" class="empty-state compact">
        尚未配置任何插件市场源。添加一个来源地址后即可同步并浏览可安装插件。
      </div>
      <ul v-else class="source-list">
        <li v-for="source in sources" :key="source.id" class="source-row">
          <div class="source-main">
            <span class="source-name">
              {{ source.name }}
              <span class="source-state" :class="{ off: !source.enabled }">
                {{ source.enabled ? "启用" : "停用" }}
              </span>
            </span>
            <span class="source-url">{{ source.url }}</span>
            <span class="source-meta">上次同步 {{ formatTime(source.lastSyncedAt) }}</span>
          </div>
          <div class="source-actions">
            <button
              class="mini-btn"
              type="button"
              :disabled="loadingAction === `sync:${source.id}`"
              @click="handleSyncSource(source)"
            >
              {{ loadingAction === `sync:${source.id}` ? "同步中..." : "同步" }}
            </button>
            <button
              class="mini-btn"
              type="button"
              :disabled="loadingAction === `toggle:${source.id}`"
              @click="handleToggleSource(source)"
            >
              {{ source.enabled ? "停用" : "启用" }}
            </button>
            <button
              class="mini-btn danger"
              type="button"
              :disabled="loadingAction === `del:${source.id}`"
              @click="handleDeleteSource(source)"
            >
              删除
            </button>
          </div>
        </li>
      </ul>
    </section>

    <!-- catalog 浏览 -->
    <section class="market-catalog">
      <header class="section-head">
        <h4>可安装插件</h4>
      </header>

      <div class="catalog-filters">
        <BaseSelect
          v-model="filterSourceId"
          class="source-filter"
          :options="sourceFilterOptions"
          ariaLabel="按市场源过滤"
          @update:model-value="refreshCatalog"
        />
        <input
          v-model="searchQuery"
          class="text-input grow"
          type="search"
          placeholder="搜索插件名称 / 关键字"
          aria-label="搜索插件"
          @keyup.enter="refreshCatalog"
        />
        <button class="add-btn" type="button" :disabled="catalogLoading" @click="refreshCatalog">
          {{ catalogLoading ? "搜索中..." : "搜索" }}
        </button>
      </div>

      <div v-if="catalog.length === 0 && !catalogLoading" class="empty-state compact">
        当前来源下没有可安装插件。请确认已同步市场源，或调整搜索条件。
      </div>
      <div v-else class="catalog-grid">
        <article
          v-for="item in catalog"
          :key="`${item.sourceId}:${item.pluginId}:${item.version}`"
          class="catalog-card"
        >
          <div class="catalog-head">
            <img v-if="item.iconUrl" :src="item.iconUrl" :alt="item.name" class="catalog-icon" />
            <div class="catalog-title">
              <span class="catalog-name">
                {{ item.name }}
                <span v-if="item.hasUpdate" class="install-badge update">可更新</span>
                <span v-else-if="item.isInstalled" class="install-badge">已安装</span>
              </span>
              <span class="catalog-sub">
                {{ item.pluginId }} · v{{ item.version
                }}{{ item.author ? ` · ${item.author}` : "" }}
              </span>
              <span v-if="item.isInstalled && item.installedVersion" class="catalog-sub installed">
                本机版本 v{{ item.installedVersion }}
              </span>
            </div>
          </div>
          <p class="catalog-desc">{{ item.description }}</p>
          <div v-if="item.permissions.length" class="perm-list">
            <span
              v-for="perm in item.permissions"
              :key="perm.key"
              class="perm-badge"
              :title="perm.reason ?? undefined"
            >
              {{ perm.key }}
            </span>
          </div>
          <footer class="catalog-foot">
            <span v-if="item.signature" class="sig-badge">已签名</span>
            <span v-else class="sig-badge unsigned">未签名</span>
            <button
              class="install-btn"
              type="button"
              :disabled="installDisabled(item)"
              @click="handleInstall(item)"
            >
              {{
                loadingAction === `install:${item.sourceId}:${item.pluginId}:${item.version}`
                  ? "安装中..."
                  : installLabel(item)
              }}
            </button>
          </footer>
        </article>
      </div>
    </section>
  </div>
</template>

<style scoped lang="scss">
.market-view {
  display: flex;
  flex-direction: column;
  gap: var(--fbz-space-5);
}

.section-head {
  display: flex;
  align-items: center;
  justify-content: space-between;
  margin-bottom: var(--fbz-space-3);

  h4 {
    margin: 0;
    font-size: var(--fbz-font-size-sm);
    font-weight: 800;
    color: var(--fbz-color-text);
  }
}

.link-btn {
  background: transparent;
  border: 0;
  color: var(--fbz-color-brand-500);
  font-size: var(--fbz-font-size-xs);
  font-weight: 700;
  cursor: pointer;

  &:disabled {
    opacity: 0.5;
    cursor: not-allowed;
  }
}

.error-text {
  margin: 0 0 var(--fbz-space-3);
  color: var(--fbz-color-danger-500);
  font-size: var(--fbz-font-size-xs);
}

.add-source-form,
.catalog-filters {
  display: flex;
  gap: var(--fbz-space-2);
  margin-bottom: var(--fbz-space-3);
  flex-wrap: wrap;
}

.source-filter {
  min-width: 160px;
}

.text-input {
  height: 36px;
  padding: 0 12px;
  border-radius: var(--fbz-radius-control);
  border: 1px solid var(--fbz-color-line);
  background: var(--fbz-color-panel);
  color: var(--fbz-color-text);
  font-size: var(--fbz-font-size-sm);

  &.grow {
    flex: 1;
    min-width: 200px;
  }

  &::placeholder {
    color: var(--fbz-color-text-muted);
  }

  &:focus-visible {
    outline: none;
    border-color: var(--fbz-color-brand-500);
    box-shadow: var(--fbz-shadow-focus);
  }
}

.add-btn {
  height: 36px;
  padding: 0 var(--fbz-space-4);
  background: var(--fbz-color-brand-500);
  border: 0;
  color: #07120a;
  font-size: var(--fbz-font-size-sm);
  font-weight: 700;
  border-radius: var(--fbz-radius-control);
  cursor: pointer;
  transition: background var(--fbz-motion-fast);

  &:hover:not(:disabled) {
    background: var(--fbz-color-brand-600);
  }

  &:disabled {
    opacity: 0.6;
    cursor: not-allowed;
  }
}

.source-list {
  list-style: none;
  margin: 0;
  padding: 0;
  border: 1px solid var(--fbz-color-line-soft);
  border-radius: 6px;
  overflow: hidden;
}

.source-row {
  display: flex;
  justify-content: space-between;
  align-items: center;
  gap: var(--fbz-space-4);
  padding: var(--fbz-space-3) var(--fbz-space-4);
  border-top: 1px solid var(--fbz-color-line-soft);
  background: var(--fbz-color-panel-strong);

  &:first-child {
    border-top: 0;
  }
}

.source-main {
  min-width: 0;
  display: flex;
  flex-direction: column;
  gap: 3px;
}

.source-name {
  display: flex;
  align-items: center;
  gap: 8px;
  font-size: 13px;
  font-weight: 700;
  color: var(--fbz-color-text);
}

.source-state {
  font-family: var(--fbz-font-display);
  font-size: 9px;
  font-weight: 800;
  padding: 1px 6px;
  border-radius: var(--fbz-radius-control);
  color: var(--fbz-color-brand-500);
  background: color-mix(in srgb, var(--fbz-color-brand-500) 10%, transparent);

  &.off {
    color: var(--fbz-color-text-muted);
    background: var(--fbz-color-panel);
  }
}

.source-url,
.source-meta {
  font-size: var(--fbz-font-size-xs);
  color: var(--fbz-color-text-muted);
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.source-actions {
  display: flex;
  gap: 6px;
  flex-shrink: 0;
}

.mini-btn {
  height: 28px;
  padding: 0 10px;
  font-size: 11px;
  font-weight: 700;
  border-radius: 4px;
  cursor: pointer;
  background: var(--fbz-color-panel);
  border: 1px solid var(--fbz-color-line);
  color: var(--fbz-color-text-soft);
  transition: all var(--fbz-motion-fast);

  &:hover:not(:disabled) {
    background: var(--fbz-color-panel-elevated);
    color: var(--fbz-color-text);
  }

  &.danger {
    border-color: var(--fbz-color-danger-500);
    color: var(--fbz-color-danger-500);

    &:hover:not(:disabled) {
      background: color-mix(in srgb, var(--fbz-color-danger-500) 8%, transparent);
    }
  }

  &:disabled {
    opacity: 0.55;
    cursor: not-allowed;
  }
}

.catalog-grid {
  display: grid;
  grid-template-columns: repeat(auto-fill, minmax(300px, 1fr));
  gap: var(--fbz-space-3);
}

.catalog-card {
  display: flex;
  flex-direction: column;
  gap: 10px;
  padding: var(--fbz-space-4);
  background: var(--fbz-color-panel-strong);
  border: 1px solid var(--fbz-color-line-soft);
  border-radius: 6px;
  transition: border-color var(--fbz-motion-fast);

  &:hover {
    border-color: var(--fbz-color-brand-500);
  }
}

.catalog-head {
  display: flex;
  align-items: center;
  gap: 10px;
}

.catalog-icon {
  width: 36px;
  height: 36px;
  border-radius: 6px;
  object-fit: cover;
  flex-shrink: 0;
}

.catalog-title {
  min-width: 0;
  display: flex;
  flex-direction: column;
  gap: 2px;
}

.catalog-name {
  display: flex;
  align-items: center;
  flex-wrap: wrap;
  gap: 6px;
  font-size: 13px;
  font-weight: 700;
  color: var(--fbz-color-text);
}

.install-badge {
  font-family: var(--fbz-font-display);
  font-size: 9px;
  font-weight: 800;
  padding: 1px 6px;
  border-radius: var(--fbz-radius-control);
  color: var(--fbz-color-text-muted);
  background: var(--fbz-color-panel);
  border: 1px solid var(--fbz-color-line);

  &.update {
    color: var(--fbz-color-brand-500);
    border-color: color-mix(in srgb, var(--fbz-color-brand-500) 35%, transparent);
    background: color-mix(in srgb, var(--fbz-color-brand-500) 10%, transparent);
  }
}

.catalog-sub {
  font-size: var(--fbz-font-size-xs);
  color: var(--fbz-color-text-muted);

  &.installed {
    color: var(--fbz-color-text-soft);
  }
}

.catalog-desc {
  margin: 0;
  font-size: var(--fbz-font-size-xs);
  color: var(--fbz-color-text-soft);
  line-height: 1.5;
  flex: 1;
}

.perm-list {
  display: flex;
  flex-wrap: wrap;
  gap: 4px;
}

.perm-badge {
  font-size: 10px;
  font-weight: 700;
  padding: 2px 6px;
  border-radius: 4px;
  color: var(--fbz-color-text-muted);
  background: var(--fbz-color-panel);
  border: 1px solid var(--fbz-color-line);
}

.catalog-foot {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: var(--fbz-space-3);
  border-top: 1px solid var(--fbz-color-line-soft);
  padding-top: 10px;
}

.sig-badge {
  font-family: var(--fbz-font-display);
  font-size: 9px;
  font-weight: 800;
  padding: 1px 6px;
  border-radius: var(--fbz-radius-control);
  color: var(--fbz-color-brand-500);
  background: color-mix(in srgb, var(--fbz-color-brand-500) 10%, transparent);

  &.unsigned {
    color: var(--fbz-color-text-muted);
    background: var(--fbz-color-panel);
  }
}

.install-btn {
  height: 28px;
  padding: 0 14px;
  font-size: 11px;
  font-weight: 700;
  border-radius: 4px;
  cursor: pointer;
  background: var(--fbz-color-brand-500);
  border: 0;
  color: #07120a;
  transition: background var(--fbz-motion-fast);

  &:hover:not(:disabled) {
    background: var(--fbz-color-brand-600);
  }

  &:disabled {
    opacity: 0.6;
    cursor: not-allowed;
  }
}

.empty-state {
  text-align: center;
  padding: 40px;
  background: var(--fbz-color-panel-strong);
  border: 1px dashed var(--fbz-color-line-soft);
  border-radius: 6px;
  color: var(--fbz-color-text-muted);
  font-size: var(--fbz-font-size-sm);

  &.compact {
    padding: var(--fbz-space-5);
  }
}

@media (max-width: 600px) {
  .source-row {
    flex-direction: column;
    align-items: flex-start;
  }
}
</style>
