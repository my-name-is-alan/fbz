<script setup lang="ts">
/**
 * 插件管理页宿主：承载插件通过 manifest `menu` 声明并渲染进后台导航的页面
 * （路径命名空间 `/admin/plugins/{pluginId}/...`）。
 *
 * 后端插件（wasm/http runtime）没有独立前端 bundle，页面内容由宿主统一渲染：
 * 插件概要 + 该插件声明的菜单分区 + manifest 配置 schema 表单（读写
 * `/api/admin/plugins/{pluginId}/config`）。
 */
import {
  getPluginConfig,
  listPluginMenuItems,
  listPlugins,
  updatePluginConfig,
} from "@/service/modules/admin.ts";
import type {
  PluginConfig,
  PluginConfigField,
  PluginMenuItem,
  PluginSummary,
} from "@/types/admin.ts";
import { useUiStore } from "@/stores/ui.ts";

const route = useRoute();
const uiStore = useUiStore();

const pluginId = computed(() => String(route.params.pluginId ?? ""));

const loading = ref(true);
const error = ref("");
const plugin = ref<PluginSummary | null>(null);
const menuItems = ref<PluginMenuItem[]>([]);
const config = ref<PluginConfig | null>(null);
const configValues = ref<Record<string, unknown>>({});
const savingConfig = ref(false);

/** 当前插件声明的菜单项（按 weight 排序），用于页内分区导航。 */
const pluginMenu = computed(() =>
  menuItems.value
    .filter((item) => item.pluginId === pluginId.value)
    .sort((a, b) => a.weight - b.weight || a.itemKey.localeCompare(b.itemKey)),
);

/** 当前路由命中的菜单项（决定页标题）。 */
const activeMenuItem = computed(
  () => pluginMenu.value.find((item) => item.path === route.path) ?? pluginMenu.value[0] ?? null,
);

const pageTitle = computed(() => {
  const name = plugin.value?.name ?? config.value?.pluginName ?? pluginId.value;
  const label = activeMenuItem.value?.label;
  return label ? `${name} · ${label}` : name;
});

function normalizeValue(field: PluginConfigField, raw: unknown): unknown {
  if (field.type === "boolean") return raw === true;
  if (field.type === "number") return typeof raw === "number" ? raw : "";
  if (field.type === "secret" || field.type === "password") return "";
  return raw ?? "";
}

async function loadAll() {
  if (!pluginId.value) return;
  loading.value = true;
  error.value = "";
  config.value = null;
  configValues.value = {};
  try {
    const [items, pluginPage] = await Promise.all([listPluginMenuItems(), listPlugins()]);
    menuItems.value = items;
    plugin.value = pluginPage.items.find((p) => p.pluginId === pluginId.value) ?? null;
    if (!plugin.value) {
      error.value = "未找到该插件，可能已被卸载或尚未启用。";
      return;
    }
    try {
      const loaded = await getPluginConfig(pluginId.value);
      config.value = loaded;
      const values: Record<string, unknown> = {};
      for (const field of loaded.schema) {
        values[field.key] = normalizeValue(field, loaded.values[field.key]);
      }
      configValues.value = values;
    } catch {
      // 插件可以只声明菜单不声明配置 schema；配置读取失败不阻塞页面。
      config.value = null;
    }
  } catch {
    error.value = "插件页面加载失败，请确认后端可用且当前用户具备管理员权限。";
  } finally {
    loading.value = false;
  }
}

async function handleSaveConfig() {
  const current = config.value;
  if (!current) return;
  savingConfig.value = true;
  try {
    const payload: Record<string, unknown> = {};
    for (const field of current.schema) {
      const value = configValues.value[field.key];
      if ((field.type === "secret" || field.type === "password") && value === "") continue;
      if (field.type === "number") {
        payload[field.key] = value === "" ? null : Number(value);
      } else {
        payload[field.key] = value;
      }
    }
    const updated = await updatePluginConfig(current.pluginId, payload);
    config.value = updated;
    uiStore.showToast(`插件 ${updated.pluginName} 配置已保存。`, "success");
  } catch {
    uiStore.showToast("保存插件配置失败，请检查必填项与取值。", "error");
  } finally {
    savingConfig.value = false;
  }
}

watch(pluginId, () => void loadAll(), { immediate: true });
</script>

<template>
  <div class="plugin-page">
    <div class="plugin-page-banner">
      <h1 class="page-heading">{{ pageTitle }}</h1>
      <p class="description-text">
        由插件 <code>{{ pluginId }}</code> 提供的管理页面。
        <RouterLink to="/admin/plugins" class="back-to-plugins">返回插件设置</RouterLink>
      </p>
    </div>

    <p v-if="loading" class="page-hint">插件页面加载中...</p>
    <p v-else-if="error" class="page-hint error">{{ error }}</p>

    <template v-else>
      <!-- 该插件声明的菜单分区（多入口时展示） -->
      <nav v-if="pluginMenu.length > 1" class="plugin-subnav" aria-label="插件页面分区">
        <RouterLink
          v-for="item in pluginMenu"
          :key="item.itemKey"
          :to="item.path"
          class="subnav-chip"
          :class="{ active: item.path === route.path }"
        >
          {{ item.label }}
        </RouterLink>
      </nav>

      <!-- 插件概要 -->
      <section class="plugin-card">
        <header class="card-head">
          <span class="indicator" />
          <h3>插件状态</h3>
        </header>
        <div class="card-body meta-grid">
          <div class="meta-item">
            <span class="meta-label">名称</span>
            <span class="meta-value">{{ plugin?.name ?? pluginId }}</span>
          </div>
          <div class="meta-item">
            <span class="meta-label">运行时</span>
            <span class="meta-value">{{ plugin?.runtime ?? "-" }}</span>
          </div>
          <div class="meta-item">
            <span class="meta-label">当前版本</span>
            <span class="meta-value">{{ plugin?.packageVersion ?? "-" }}</span>
          </div>
          <div class="meta-item">
            <span class="meta-label">状态</span>
            <span class="status-badge" :class="{ active: plugin?.enabled }">
              {{ plugin?.enabled ? "已启用" : "已停用" }}
            </span>
          </div>
        </div>
      </section>

      <!-- 配置表单（插件声明了 config schema 时） -->
      <section v-if="config && config.schema.length > 0" class="plugin-card">
        <header class="card-head">
          <span class="indicator" />
          <h3>插件配置</h3>
        </header>
        <div class="card-body">
          <PluginConfigForm
            v-model="configValues"
            :schema="config.schema"
            :saving="savingConfig"
            id-prefix="pp"
            @save="handleSaveConfig"
          />
        </div>
      </section>

      <section v-else class="plugin-card">
        <div class="card-body">
          <p class="page-hint">
            该插件没有声明可编辑配置项；运行状态与执行审计请在「插件设置」查看。
          </p>
        </div>
      </section>
    </template>
  </div>
</template>

<style scoped lang="scss">
.plugin-page {
  display: flex;
  flex-direction: column;
  gap: var(--fbz-space-4);
}

.plugin-page-banner {
  border-bottom: 1px solid var(--fbz-color-line-soft);
  padding-bottom: var(--fbz-space-5);

  .page-heading {
    margin: 0 0 var(--fbz-space-2);
    font-size: 22px;
    font-weight: 800;
    letter-spacing: -0.3px;
    color: var(--fbz-color-text);
  }

  .description-text {
    margin: 0;
    color: var(--fbz-color-text-muted);
    font-size: var(--fbz-font-size-sm);
    line-height: 1.6;

    code {
      font-family: var(--fbz-font-display);
      font-size: 11px;
      background: var(--fbz-color-panel-strong);
      border: 1px solid var(--fbz-color-line-soft);
      border-radius: 4px;
      padding: 1px 6px;
    }
  }

  .back-to-plugins {
    margin-left: var(--fbz-space-2);
    color: var(--fbz-color-brand-500);
    text-decoration: none;
    font-weight: 600;

    &:hover {
      text-decoration: underline;
    }
  }
}

.page-hint {
  margin: 0;
  color: var(--fbz-color-text-muted);
  font-size: var(--fbz-font-size-sm);

  &.error {
    color: var(--fbz-color-danger-500);
  }
}

.plugin-subnav {
  display: flex;
  flex-wrap: wrap;
  gap: var(--fbz-space-2);
}

.subnav-chip {
  padding: 6px 14px;
  border-radius: var(--fbz-radius-round);
  border: 1px solid var(--fbz-color-line);
  background: var(--fbz-color-panel-strong);
  color: var(--fbz-color-text-soft);
  font-size: var(--fbz-font-size-xs);
  font-weight: 700;
  text-decoration: none;
  transition: all var(--fbz-motion-fast);

  &:hover {
    border-color: var(--fbz-color-line-bright);
    color: var(--fbz-color-text);
  }

  &.active {
    border-color: var(--fbz-color-brand-500);
    color: var(--fbz-color-brand-500);
    background: color-mix(in srgb, var(--fbz-color-brand-500) 6%, var(--fbz-color-panel-strong));
  }
}

.plugin-card {
  background: var(--fbz-color-panel-strong);
  border: 1px solid var(--fbz-color-line-soft);
  border-radius: 6px;
  overflow: hidden;

  .card-head {
    padding: var(--fbz-space-3) var(--fbz-space-5);
    border-bottom: 1px solid var(--fbz-color-line-soft);
    display: flex;
    align-items: center;
    gap: 10px;

    .indicator {
      width: 3px;
      height: 12px;
      background: var(--fbz-color-brand-500);
      border-radius: 2px;
    }

    h3 {
      margin: 0;
      font-size: 12px;
      font-weight: 700;
      text-transform: uppercase;
      letter-spacing: 0.5px;
      color: var(--fbz-color-text-soft);
    }
  }

  .card-body {
    padding: var(--fbz-space-5);
  }
}

.meta-grid {
  display: grid;
  grid-template-columns: repeat(auto-fit, minmax(160px, 1fr));
  gap: var(--fbz-space-4);
}

.meta-item {
  display: flex;
  flex-direction: column;
  gap: 4px;

  .meta-label {
    font-size: 10px;
    font-weight: 700;
    color: var(--fbz-color-text-muted);
    text-transform: uppercase;
    letter-spacing: 0.5px;
  }

  .meta-value {
    font-size: var(--fbz-font-size-sm);
    font-weight: 600;
    color: var(--fbz-color-text);
    word-break: break-all;
  }
}

.status-badge {
  align-self: flex-start;
  padding: 2px 10px;
  border-radius: var(--fbz-radius-round);
  border: 1px solid var(--fbz-color-line);
  background: var(--fbz-color-panel);
  color: var(--fbz-color-text-muted);
  font-size: 11px;
  font-weight: 700;

  &.active {
    border-color: color-mix(in srgb, var(--fbz-color-brand-500) 40%, transparent);
    color: var(--fbz-color-brand-500);
    background: color-mix(in srgb, var(--fbz-color-brand-500) 8%, transparent);
  }
}
</style>
