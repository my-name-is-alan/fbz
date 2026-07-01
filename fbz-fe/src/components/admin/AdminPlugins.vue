<script setup lang="ts">
import {
  activatePluginPackage,
  approvePluginPackage,
  getPluginConfig,
  listPluginDispatches,
  listPluginPackages,
  listPlugins,
  rejectPluginPackage,
  replayPluginDispatch,
  setPluginEnabled,
} from "@/service/modules/admin.ts";
import { useUiStore } from "@/stores/ui.ts";
import type {
  PluginConfig,
  PluginDispatch,
  PluginPackageSummary,
  PluginSummary,
} from "@/types/admin.ts";

const uiStore = useUiStore();

const activeSubTab = shallowRef<"plugins" | "packages" | "dispatches">("plugins");
const plugins = ref<PluginSummary[]>([]);
const packages = ref<PluginPackageSummary[]>([]);
const dispatches = ref<PluginDispatch[]>([]);
const selectedConfig = ref<PluginConfig | null>(null);
const loading = shallowRef(false);
const loadingAction = shallowRef("");
const error = shallowRef("");

const activePlugins = computed(() => plugins.value.filter((plugin) => plugin.enabled).length);
const pendingPackages = computed(
  () => packages.value.filter((pkg) => pkg.approvalStatus === "pending_approval").length,
);
const failedDispatches = computed(
  () => dispatches.value.filter((dispatch) => dispatch.status === "failed").length,
);

onMounted(() => {
  void refreshAll();
});

function pluginName(plugin: PluginSummary): string {
  return plugin.name ?? plugin.pluginId;
}

function formatTime(value: string | null | undefined): string {
  if (!value) return "-";
  return new Intl.DateTimeFormat("zh-CN", {
    month: "2-digit",
    day: "2-digit",
    hour: "2-digit",
    minute: "2-digit",
  }).format(new Date(value));
}

async function refreshAll() {
  loading.value = true;
  error.value = "";
  try {
    const [pluginPage, packagePage, dispatchPage] = await Promise.all([
      listPlugins({ limit: 200 }),
      listPluginPackages({ limit: 200 }),
      listPluginDispatches({ limit: 20 }),
    ]);
    plugins.value = pluginPage.items;
    packages.value = packagePage.items;
    dispatches.value = dispatchPage.items;
  } catch {
    error.value = "插件管理数据加载失败，请确认后端已初始化且当前用户具备管理员权限。";
  } finally {
    loading.value = false;
  }
}

async function handleTogglePlugin(plugin: PluginSummary) {
  loadingAction.value = `plugin:${plugin.pluginId}`;
  try {
    await setPluginEnabled(plugin.pluginId, !plugin.enabled);
    uiStore.showToast(
      `插件 ${pluginName(plugin)} 已${plugin.enabled ? "停用" : "启用"}。`,
      "success",
    );
    await refreshAll();
  } catch {
    uiStore.showToast("更新插件启用状态失败。", "error");
  } finally {
    loadingAction.value = "";
  }
}

async function handlePackageAction(
  pkg: PluginPackageSummary,
  action: "approve" | "reject" | "activate",
) {
  loadingAction.value = `${action}:${pkg.packageId}`;
  try {
    if (action === "approve") {
      await approvePluginPackage(pkg.packageId);
      uiStore.showToast(`插件包 ${pkg.name} 已审批通过。`, "success");
    } else if (action === "reject") {
      await rejectPluginPackage(pkg.packageId);
      uiStore.showToast(`插件包 ${pkg.name} 已拒绝。`, "success");
    } else {
      await activatePluginPackage(pkg.packageId);
      uiStore.showToast(`插件包 ${pkg.name} 已激活。`, "success");
    }
    await refreshAll();
  } catch {
    uiStore.showToast("插件包操作失败，请检查包状态和审批状态。", "error");
  } finally {
    loadingAction.value = "";
  }
}

async function handleLoadConfig(plugin: PluginSummary) {
  loadingAction.value = `config:${plugin.pluginId}`;
  try {
    selectedConfig.value = await getPluginConfig(plugin.pluginId);
  } catch {
    uiStore.showToast("该插件暂无可编辑配置或当前包未激活。", "warning");
  } finally {
    loadingAction.value = "";
  }
}

async function handleReplayDispatch(dispatch: PluginDispatch) {
  loadingAction.value = `dispatch:${dispatch.id}`;
  try {
    await replayPluginDispatch(dispatch.id);
    uiStore.showToast("插件 dispatch 已重新入队。", "success");
    await refreshAll();
  } catch {
    uiStore.showToast("重放 dispatch 失败，可能状态不允许重放。", "error");
  } finally {
    loadingAction.value = "";
  }
}
</script>

<template>
  <div class="admin-plugins-view">
    <div class="summary-grid">
      <div class="summary-card">
        <span class="label">插件</span>
        <span class="value">{{ plugins.length }}</span>
        <span class="meta">启用 {{ activePlugins }}</span>
      </div>
      <div class="summary-card">
        <span class="label">插件包</span>
        <span class="value">{{ packages.length }}</span>
        <span class="meta">待审批 {{ pendingPackages }}</span>
      </div>
      <div class="summary-card">
        <span class="label">Dispatch</span>
        <span class="value">{{ dispatches.length }}</span>
        <span class="meta">失败 {{ failedDispatches }}</span>
      </div>
    </div>

    <nav class="sub-tabs-bar" role="tablist" aria-label="插件管理标签">
      <button
        class="sub-tab-btn"
        :class="{ active: activeSubTab === 'plugins' }"
        type="button"
        role="tab"
        :aria-selected="activeSubTab === 'plugins'"
        @click="activeSubTab = 'plugins'"
      >
        插件状态
      </button>
      <button
        class="sub-tab-btn"
        :class="{ active: activeSubTab === 'packages' }"
        type="button"
        role="tab"
        :aria-selected="activeSubTab === 'packages'"
        @click="activeSubTab = 'packages'"
      >
        插件包
      </button>
      <button
        class="sub-tab-btn"
        :class="{ active: activeSubTab === 'dispatches' }"
        type="button"
        role="tab"
        :aria-selected="activeSubTab === 'dispatches'"
        @click="activeSubTab = 'dispatches'"
      >
        运行审计
      </button>
      <button class="refresh-btn" type="button" :disabled="loading" @click="refreshAll">
        {{ loading ? "刷新中..." : "刷新" }}
      </button>
    </nav>

    <p v-if="error" class="error-text">{{ error }}</p>

    <div class="plugins-panel-content">
      <div v-if="activeSubTab === 'plugins'" class="plugins-grid">
        <div v-if="!loading && plugins.length === 0" class="empty-state">
          后端当前没有已注册插件。通过插件包安装接口安装后会显示在这里。
        </div>
        <div
          v-for="plugin in plugins"
          :key="plugin.pluginId"
          class="plugin-card"
          :class="{ inactive: !plugin.enabled }"
        >
          <div class="plugin-main">
            <div class="plugin-header">
              <div class="title-row">
                <span class="plugin-name">{{ pluginName(plugin) }}</span>
                <div class="badge-row">
                  <span class="plugin-ver">{{ plugin.packageVersion ?? "no package" }}</span>
                  <span class="status-badge" :class="{ active: plugin.enabled }">
                    {{ plugin.enabled ? "已启用" : "已停用" }}
                  </span>
                </div>
              </div>
              <span class="author">
                {{ plugin.pluginId }} / {{ plugin.runtime ?? "runtime 未知" }}
              </span>
            </div>
            <p class="plugin-desc">
              包状态 {{ plugin.packageStatus ?? "-" }}，审批 {{ plugin.approvalStatus }}
            </p>
          </div>
          <div class="plugin-actions">
            <button class="plugin-btn secondary" type="button" @click="handleLoadConfig(plugin)">
              配置
            </button>
            <button
              class="plugin-btn secondary"
              type="button"
              :disabled="loadingAction === `plugin:${plugin.pluginId}`"
              @click="handleTogglePlugin(plugin)"
            >
              {{ plugin.enabled ? "停用" : "启用" }}
            </button>
          </div>
        </div>
      </div>

      <div v-else-if="activeSubTab === 'packages'" class="plugins-grid">
        <div v-if="!loading && packages.length === 0" class="empty-state">
          后端当前没有插件包记录。
        </div>
        <div v-for="pkg in packages" :key="pkg.packageId" class="plugin-card store-card">
          <div class="plugin-main">
            <div class="plugin-header">
              <div class="title-row">
                <span class="plugin-name">{{ pkg.name }}</span>
                <span class="plugin-ver">{{ pkg.packageVersion }}</span>
              </div>
              <span class="author">{{ pkg.pluginId }} / {{ pkg.runtime }}</span>
            </div>
            <p class="plugin-desc">
              状态 {{ pkg.packageStatus }}，审批 {{ pkg.approvalStatus ?? "-" }}，签名
              {{ pkg.signaturePresent ? "存在" : "无" }}
            </p>
          </div>
          <div class="plugin-actions">
            <button
              class="plugin-btn secondary"
              type="button"
              :disabled="loadingAction === `approve:${pkg.packageId}`"
              @click="handlePackageAction(pkg, 'approve')"
            >
              审批
            </button>
            <button
              class="plugin-btn secondary"
              type="button"
              :disabled="loadingAction === `activate:${pkg.packageId}`"
              @click="handlePackageAction(pkg, 'activate')"
            >
              激活
            </button>
            <button
              class="plugin-btn danger"
              type="button"
              :disabled="loadingAction === `reject:${pkg.packageId}`"
              @click="handlePackageAction(pkg, 'reject')"
            >
              拒绝
            </button>
          </div>
        </div>
      </div>

      <div v-else class="dispatch-list">
        <div v-if="!loading && dispatches.length === 0" class="empty-state">
          暂无插件 dispatch 审计记录。
        </div>
        <div v-for="dispatch in dispatches" :key="dispatch.id" class="dispatch-row">
          <div class="dispatch-main">
            <span class="dispatch-title">
              {{ dispatch.hookEvent ?? dispatch.aggregateType }}
            </span>
            <span class="dispatch-meta">
              {{ dispatch.pluginId ?? "unknown plugin" }} / {{ dispatch.handler ?? "handler" }}
            </span>
            <span v-if="dispatch.lastError" class="dispatch-error">{{ dispatch.lastError }}</span>
          </div>
          <div class="dispatch-side">
            <span class="status-badge" :class="{ active: dispatch.status === 'delivered' }">
              {{ dispatch.status }}
            </span>
            <span class="dispatch-meta">{{ formatTime(dispatch.createdAt) }}</span>
            <button
              class="plugin-btn secondary"
              type="button"
              :disabled="loadingAction === `dispatch:${dispatch.id}`"
              @click="handleReplayDispatch(dispatch)"
            >
              重放
            </button>
          </div>
        </div>
      </div>
    </div>

    <Transition name="fade">
      <div v-if="selectedConfig" class="config-overlay" @click="selectedConfig = null">
        <section class="config-panel" @click.stop>
          <header>
            <div>
              <h3>{{ selectedConfig.pluginName }}</h3>
              <p>{{ selectedConfig.pluginId }} / {{ selectedConfig.packageId }}</p>
            </div>
            <button type="button" class="close-btn" @click="selectedConfig = null">关闭</button>
          </header>
          <div v-if="selectedConfig.schema.length === 0" class="empty-state compact">
            该插件没有声明可编辑配置项。
          </div>
          <div v-else class="config-list">
            <div v-for="field in selectedConfig.schema" :key="field.key" class="config-field">
              <span class="field-name">{{ field.label }}</span>
              <span class="field-meta">{{ field.key }} / {{ field.type }}</span>
              <p v-if="field.helpText">{{ field.helpText }}</p>
            </div>
          </div>
        </section>
      </div>
    </Transition>
  </div>
</template>

<style scoped lang="scss">
.admin-plugins-view {
  display: flex;
  flex-direction: column;
  gap: var(--fbz-space-4);
}

.summary-grid {
  display: grid;
  grid-template-columns: repeat(3, 1fr);
  gap: var(--fbz-space-3);
}

.summary-card {
  background: var(--fbz-color-panel-strong);
  border: 1px solid var(--fbz-color-line-soft);
  border-radius: 6px;
  padding: var(--fbz-space-4);
  display: flex;
  flex-direction: column;
  gap: 5px;

  .label,
  .meta {
    color: var(--fbz-color-text-muted);
    font-size: var(--fbz-font-size-xs);
    font-weight: 700;
  }

  .value {
    color: var(--fbz-color-text);
    font-family: var(--fbz-font-display);
    font-size: 22px;
    font-weight: 800;
  }
}

.sub-tabs-bar {
  display: flex;
  align-items: center;
  gap: var(--fbz-space-2);
  border-bottom: 1px solid var(--fbz-color-line-soft);
  padding-bottom: 8px;
}

.sub-tab-btn,
.refresh-btn {
  height: 32px;
  background: transparent;
  border: 0;
  color: var(--fbz-color-text-muted);
  font-size: var(--fbz-font-size-sm);
  font-weight: 700;
  padding: 0 var(--fbz-space-3);
  cursor: pointer;
  border-radius: var(--fbz-radius-control);
  transition: all var(--fbz-motion-fast);

  &:hover:not(:disabled) {
    color: var(--fbz-color-text);
    background: var(--fbz-color-panel-strong);
  }

  &.active {
    color: var(--fbz-color-brand-500);
    background: color-mix(in srgb, var(--fbz-color-brand-500) 8%, var(--fbz-color-panel-strong));
  }

  &:disabled {
    opacity: 0.5;
    cursor: not-allowed;
  }
}

.refresh-btn {
  margin-left: auto;
  border: 1px solid var(--fbz-color-line);
  background: var(--fbz-color-panel-strong);
}

.plugins-grid {
  display: grid;
  grid-template-columns: repeat(auto-fill, minmax(320px, 1fr));
  gap: var(--fbz-space-3);
  margin-top: var(--fbz-space-2);
}

.plugin-card {
  background: var(--fbz-color-panel-strong);
  border: 1px solid var(--fbz-color-line-soft);
  border-radius: 6px;
  padding: var(--fbz-space-4);
  display: flex;
  flex-direction: column;
  justify-content: space-between;
  gap: 16px;
  transition: all var(--fbz-motion-fast);

  &:hover {
    border-color: var(--fbz-color-brand-500);
    transform: translateY(-1px);
    box-shadow: 0 4px 12px rgba(0, 0, 0, 0.15);
  }

  &.inactive {
    opacity: 0.65;
  }
}

.plugin-header {
  display: flex;
  flex-direction: column;
  gap: 4px;
  margin-bottom: 8px;
}

.title-row {
  display: flex;
  justify-content: space-between;
  align-items: center;
  gap: var(--fbz-space-3);
}

.plugin-name {
  font-size: 13px;
  font-weight: 700;
  color: var(--fbz-color-text);
}

.badge-row,
.plugin-actions {
  display: flex;
  align-items: center;
  gap: 6px;
}

.plugin-ver,
.status-badge {
  font-family: var(--fbz-font-display);
  font-size: 9px;
  font-weight: 800;
  background: var(--fbz-color-panel);
  border: 1px solid var(--fbz-color-line);
  padding: 1px 6px;
  border-radius: var(--fbz-radius-control);
  color: var(--fbz-color-text-muted);
}

.status-badge.active {
  background: color-mix(in srgb, var(--fbz-color-brand-500) 10%, transparent);
  color: var(--fbz-color-brand-500);
  border-color: color-mix(in srgb, var(--fbz-color-brand-500) 30%, transparent);
}

.author,
.plugin-desc,
.dispatch-meta {
  font-size: var(--fbz-font-size-xs);
  color: var(--fbz-color-text-muted);
}

.plugin-desc {
  margin: 0;
  line-height: 1.5;
}

.plugin-actions {
  justify-content: flex-end;
  border-top: 1px solid var(--fbz-color-line-soft);
  padding-top: 10px;
}

.plugin-btn {
  height: 26px;
  padding: 0 10px;
  font-size: 11px;
  font-weight: 700;
  border-radius: 4px;
  cursor: pointer;
  display: inline-flex;
  align-items: center;
  gap: 6px;
  transition: all var(--fbz-motion-fast);

  &.secondary {
    background: var(--fbz-color-panel);
    border: 1px solid var(--fbz-color-line);
    color: var(--fbz-color-text-soft);

    &:hover:not(:disabled) {
      background: var(--fbz-color-panel-elevated);
      color: var(--fbz-color-text);
    }
  }

  &.danger {
    background: transparent;
    border: 1px solid var(--fbz-color-danger-500);
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

.dispatch-list {
  background: var(--fbz-color-panel-strong);
  border: 1px solid var(--fbz-color-line-soft);
  border-radius: 6px;
  overflow: hidden;
}

.dispatch-row {
  padding: var(--fbz-space-4);
  border-top: 1px solid var(--fbz-color-line-soft);
  display: flex;
  justify-content: space-between;
  gap: var(--fbz-space-4);

  &:first-child {
    border-top: 0;
  }
}

.dispatch-main {
  min-width: 0;
  display: flex;
  flex-direction: column;
  gap: 4px;
}

.dispatch-title {
  color: var(--fbz-color-text);
  font-size: var(--fbz-font-size-sm);
  font-weight: 800;
}

.dispatch-error,
.error-text {
  margin: 0;
  color: var(--fbz-color-danger-500);
  font-size: var(--fbz-font-size-xs);
}

.dispatch-side {
  display: flex;
  align-items: center;
  gap: var(--fbz-space-2);
  flex-shrink: 0;
}

.empty-state {
  grid-column: 1 / -1;
  text-align: center;
  padding: 40px;
  background: var(--fbz-color-panel-strong);
  border: 1px dashed var(--fbz-color-line-soft);
  border-radius: 6px;
  color: var(--fbz-color-text-muted);
  font-size: var(--fbz-font-size-sm);

  &.compact {
    padding: var(--fbz-space-4);
  }
}

.config-overlay {
  position: fixed;
  inset: 0;
  z-index: 170;
  background: rgba(0, 0, 0, 0.72);
  display: grid;
  place-content: center;
  padding: var(--fbz-space-4);
}

.config-panel {
  width: min(640px, 94vw);
  max-height: 80vh;
  overflow: auto;
  background: var(--fbz-color-panel);
  border: 1px solid var(--fbz-color-line);
  border-radius: 8px;
  box-shadow: var(--fbz-shadow-panel);

  header {
    display: flex;
    justify-content: space-between;
    gap: var(--fbz-space-4);
    padding: var(--fbz-space-4);
    border-bottom: 1px solid var(--fbz-color-line-soft);

    h3,
    p {
      margin: 0;
    }

    p {
      margin-top: 4px;
      color: var(--fbz-color-text-muted);
      font-size: var(--fbz-font-size-xs);
    }
  }
}

.close-btn {
  height: 30px;
  border: 1px solid var(--fbz-color-line);
  border-radius: var(--fbz-radius-control);
  background: var(--fbz-color-panel-strong);
  color: var(--fbz-color-text-soft);
  cursor: pointer;
}

.config-list {
  padding: var(--fbz-space-4);
  display: flex;
  flex-direction: column;
  gap: var(--fbz-space-3);
}

.config-field {
  border: 1px solid var(--fbz-color-line-soft);
  border-radius: 6px;
  padding: var(--fbz-space-3);

  .field-name {
    display: block;
    color: var(--fbz-color-text);
    font-size: var(--fbz-font-size-sm);
    font-weight: 800;
  }

  .field-meta,
  p {
    color: var(--fbz-color-text-muted);
    font-size: var(--fbz-font-size-xs);
  }
}

.fade-enter-active,
.fade-leave-active {
  transition: opacity var(--fbz-motion-fast) ease;
}
.fade-enter-from,
.fade-leave-to {
  opacity: 0;
}

@media (max-width: 760px) {
  .summary-grid {
    grid-template-columns: 1fr;
  }

  .dispatch-row,
  .dispatch-side {
    align-items: flex-start;
    flex-direction: column;
  }
}
</style>
