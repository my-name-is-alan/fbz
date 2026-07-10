<script setup lang="ts">
/**
 * 独立插件配置页：`/admin/plugins/:pluginId/config`。
 * 从插件列表「配置」进入；与插件菜单页共用 PluginConfigForm。
 */
import { getPluginConfig, updatePluginConfig } from "@/service/modules/admin.ts";
import type { PluginConfig, PluginConfigField } from "@/types/admin.ts";
import { useUiStore } from "@/stores/ui.ts";

const route = useRoute();
const uiStore = useUiStore();

const pluginId = computed(() => String(route.params.pluginId ?? ""));

const loading = ref(true);
const error = ref("");
const config = ref<PluginConfig | null>(null);
const configValues = ref<Record<string, unknown>>({});
const savingConfig = ref(false);

function normalizeValue(field: PluginConfigField, raw: unknown): unknown {
  if (field.type === "boolean") return raw === true;
  if (field.type === "number") return typeof raw === "number" ? raw : "";
  if (field.type === "secret" || field.type === "password") return "";
  return raw ?? "";
}

async function loadConfig() {
  if (!pluginId.value) return;
  loading.value = true;
  error.value = "";
  config.value = null;
  configValues.value = {};
  try {
    const loaded = await getPluginConfig(pluginId.value);
    config.value = loaded;
    const values: Record<string, unknown> = {};
    for (const field of loaded.schema) {
      values[field.key] = normalizeValue(field, loaded.values[field.key]);
    }
    configValues.value = values;
  } catch {
    error.value = "无法加载插件配置：插件可能未激活，或未声明可编辑配置项。";
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

watch(pluginId, () => void loadConfig(), { immediate: true });
</script>

<template>
  <div class="plugin-config-page">
    <div class="page-banner">
      <h1 class="page-heading">
        {{ config?.pluginName ?? pluginId }}
        <span class="heading-sub">配置</span>
      </h1>
      <p class="description-text">
        编辑插件 <code>{{ pluginId }}</code> 的运行参数。
        <RouterLink to="/admin/plugins" class="back-link">返回插件设置</RouterLink>
      </p>
    </div>

    <p v-if="loading" class="page-hint">配置加载中...</p>
    <p v-else-if="error" class="page-hint error">{{ error }}</p>

    <section v-else-if="config" class="config-card">
      <header class="card-head">
        <span class="indicator" />
        <h3>插件配置</h3>
        <span class="card-meta">{{ config.packageId }}</span>
      </header>
      <div class="card-body">
        <div v-if="config.schema.length === 0" class="empty-hint">该插件没有声明可编辑配置项。</div>
        <PluginConfigForm
          v-else
          v-model="configValues"
          :schema="config.schema"
          :saving="savingConfig"
          id-prefix="apc"
          @save="handleSaveConfig"
        />
      </div>
    </section>
  </div>
</template>

<style scoped lang="scss">
.plugin-config-page {
  display: flex;
  flex-direction: column;
  gap: var(--fbz-space-4);
}

.page-banner {
  border-bottom: 1px solid var(--fbz-color-line-soft);
  padding-bottom: var(--fbz-space-5);

  .page-heading {
    margin: 0 0 var(--fbz-space-2);
    font-size: 22px;
    font-weight: 800;
    letter-spacing: -0.3px;
    color: var(--fbz-color-text);

    .heading-sub {
      margin-left: 8px;
      font-size: 14px;
      font-weight: 600;
      color: var(--fbz-color-text-muted);
    }
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

  .back-link {
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

.config-card {
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

    .card-meta {
      margin-left: auto;
      font-size: var(--fbz-font-size-xs);
      color: var(--fbz-color-text-muted);
      font-family: var(--fbz-font-display);
    }
  }

  .card-body {
    padding: var(--fbz-space-5);
  }
}

.empty-hint {
  margin: 0;
  color: var(--fbz-color-text-muted);
  font-size: var(--fbz-font-size-sm);
}
</style>
