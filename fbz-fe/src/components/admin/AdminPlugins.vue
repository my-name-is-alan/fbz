<script setup lang="ts">
import { useUiStore } from "@/stores/ui.ts";

const uiStore = useUiStore();

const activeSubTab = ref("installed");

const installedPlugins = ref([
  {
    id: "p1",
    name: "TMDB Scraper Meta-Engine",
    version: "v1.2.0",
    author: "Fbz Dev Team",
    desc: "自适应匹配搜刮电影与电视剧详情、演职员表和背景原画。",
    active: true,
  },
  {
    id: "p2",
    name: "Local NFO File Parser",
    version: "v1.0.1",
    author: "Fbz Dev Team",
    desc: "智能解析本地已存在的 NFO、XML 等元数据说明文件。",
    active: true,
  },
  {
    id: "p3",
    name: "Discord Rich Presence Client",
    version: "v2.1.0",
    author: "Community",
    desc: "播放影片时，自动将您的 Discord 状态同步为正在观看内容。",
    active: true,
  },
]);

const storePlugins = ref([
  {
    id: "ps1",
    name: "Emby Sync Bridge",
    version: "v1.0.0",
    author: "Community",
    desc: "双向同步 FBZ 与外部 Emby 媒体服务的播放进度、已看记录。",
  },
  {
    id: "ps2",
    name: "Telegram Bot Push Notification",
    version: "v1.1.2",
    author: "Community",
    desc: "媒体库新文件入库、转码报警时，通过 Telegram 发送通知信息。",
  },
  {
    id: "ps3",
    name: "Slack Webhook Notifications",
    version: "v1.0.0",
    author: "Fbz Dev Team",
    desc: "媒体文件入库时触发 Webhook 事件推送到指定的 Slack 频道。",
  },
]);

const loadingPluginId = ref<string | null>(null);

function togglePluginActive(plugin: any) {
  plugin.active = !plugin.active;
  uiStore.showToast(
    `插件【${plugin.name}】已成功${plugin.active ? "激活运行" : "暂停运行"}。`,
    "success",
  );
}

function installPlugin(plugin: any) {
  loadingPluginId.value = plugin.id;
  setTimeout(() => {
    loadingPluginId.value = null;
    installedPlugins.value.push({
      id: plugin.id,
      name: plugin.name,
      version: plugin.version,
      author: plugin.author,
      desc: plugin.desc,
      active: true,
    });
    const idx = storePlugins.value.findIndex((p) => p.id === plugin.id);
    if (idx > -1) storePlugins.value.splice(idx, 1);

    uiStore.showToast(`插件【${plugin.name}】安装成功并已自动激活！`, "success");
  }, 1200);
}

function uninstallPlugin(plugin: any) {
  const idx = installedPlugins.value.findIndex((p) => p.id === plugin.id);
  if (idx > -1) {
    installedPlugins.value.splice(idx, 1);
    storePlugins.value.push({
      id: plugin.id,
      name: plugin.name,
      version: plugin.version,
      author: plugin.author,
      desc: plugin.desc,
    });
    uiStore.showToast(`插件【${plugin.name}】已成功卸载并清理缓存文件。`, "success");
  }
}
</script>

<template>
  <div class="admin-plugins-view">
    <nav class="sub-tabs-bar" role="tablist" aria-label="插件管理标签">
      <button
        class="sub-tab-btn"
        :class="{ active: activeSubTab === 'installed' }"
        type="button"
        role="tab"
        :aria-selected="activeSubTab === 'installed'"
        @click="activeSubTab = 'installed'"
      >
        已安装插件 ({{ installedPlugins.length }})
      </button>
      <button
        class="sub-tab-btn"
        :class="{ active: activeSubTab === 'store' }"
        type="button"
        role="tab"
        :aria-selected="activeSubTab === 'store'"
        @click="activeSubTab = 'store'"
      >
        插件市场 ({{ storePlugins.length }})
      </button>
    </nav>

    <div class="plugins-panel-content">
      <div v-if="activeSubTab === 'installed'" class="plugins-grid">
        <div
          v-for="p in installedPlugins"
          :key="p.id"
          class="plugin-card"
          :class="{ inactive: !p.active }"
        >
          <div class="plugin-main">
            <div class="plugin-header">
              <div class="title-row">
                <span class="plugin-name">{{ p.name }}</span>
                <div class="badge-row">
                  <span class="plugin-ver">{{ p.version }}</span>
                  <span class="status-badge" :class="{ active: p.active }">
                    {{ p.active ? "已启用" : "已停用" }}
                  </span>
                </div>
              </div>
              <span class="author">开发者: {{ p.author }}</span>
            </div>
            <p class="plugin-desc">{{ p.desc }}</p>
          </div>
          <div class="plugin-actions">
            <button class="plugin-btn secondary" type="button" @click="togglePluginActive(p)">
              {{ p.active ? "停用" : "启用" }}
            </button>
            <button class="plugin-btn danger" type="button" @click="uninstallPlugin(p)">
              卸载
            </button>
          </div>
        </div>
      </div>

      <div v-else class="plugins-grid">
        <div v-for="p in storePlugins" :key="p.id" class="plugin-card store-card">
          <div class="plugin-main">
            <div class="plugin-header">
              <div class="title-row">
                <span class="plugin-name">{{ p.name }}</span>
                <span class="plugin-ver">{{ p.version }}</span>
              </div>
              <span class="author">开发者: {{ p.author }}</span>
            </div>
            <p class="plugin-desc">{{ p.desc }}</p>
          </div>
          <div class="plugin-actions">
            <button
              class="plugin-btn primary"
              type="button"
              :disabled="loadingPluginId === p.id"
              @click="installPlugin(p)"
            >
              <span class="spinner" v-if="loadingPluginId === p.id" />
              <span>{{ loadingPluginId === p.id ? "安装中..." : "获取并安装" }}</span>
            </button>
          </div>
        </div>
        <div v-if="storePlugins.length === 0" class="store-empty">
          <span>💡 所有的第三方社区扩展插件已全部安装完毕。</span>
        </div>
      </div>
    </div>
  </div>
</template>

<style scoped lang="scss">
.admin-plugins-view {
  display: flex;
  flex-direction: column;
  gap: var(--fbz-space-4);
}

.sub-tabs-bar {
  display: flex;
  gap: var(--fbz-space-2);
  border-bottom: 1px solid var(--fbz-color-line-soft);
  padding-bottom: 8px;
}

.sub-tab-btn {
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

  &:hover {
    color: var(--fbz-color-text);
    background: var(--fbz-color-panel-strong);
  }

  &.active {
    color: var(--fbz-color-brand-500);
    background: color-mix(in srgb, var(--fbz-color-brand-500) 8%, var(--fbz-color-panel-strong));
  }
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

  .plugin-header {
    display: flex;
    flex-direction: column;
    gap: 4px;
    margin-bottom: 8px;

    .title-row {
      display: flex;
      justify-content: space-between;
      align-items: center;
      gap: var(--fbz-space-3);

      .plugin-name {
        font-size: 13px;
        font-weight: 700;
        color: var(--fbz-color-text);
      }

      .badge-row {
        display: flex;
        align-items: center;
        gap: 6px;
      }

      .plugin-ver {
        font-family: var(--fbz-font-display);
        font-size: 9px;
        font-weight: 800;
        background: var(--fbz-color-panel);
        border: 1px solid var(--fbz-color-line);
        padding: 1px 6px;
        border-radius: var(--fbz-radius-control);
        color: var(--fbz-color-text-muted);
      }

      .status-badge {
        font-size: 9px;
        font-weight: 700;
        padding: 1px 6px;
        border-radius: 3px;
        line-height: 1.2;

        &.active {
          background: color-mix(in srgb, var(--fbz-color-brand-500) 10%, transparent);
          color: var(--fbz-color-brand-500);
          border: 1px solid color-mix(in srgb, var(--fbz-color-brand-500) 30%, transparent);
        }

        &:not(.active) {
          background: var(--fbz-color-panel);
          color: var(--fbz-color-text-muted);
          border: 1px solid var(--fbz-color-line);
        }
      }
    }

    .author {
      font-size: 10px;
      color: var(--fbz-color-text-muted);
    }
  }

  .plugin-desc {
    margin: 0;
    font-size: var(--fbz-font-size-xs);
    color: var(--fbz-color-text-soft);
    line-height: 1.5;
  }

  .plugin-actions {
    display: flex;
    justify-content: flex-end;
    gap: 8px;
    border-top: 1px solid var(--fbz-color-line-soft);
    padding-top: 10px;

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

      &.primary {
        background: var(--fbz-color-brand-500);
        border: 0;
        color: #07120a;

        &:hover:not(:disabled) {
          background: var(--fbz-color-brand-600);
        }

        &:disabled {
          opacity: 0.6;
          cursor: not-allowed;
        }
      }

      &.secondary {
        background: var(--fbz-color-panel);
        border: 1px solid var(--fbz-color-line);
        color: var(--fbz-color-text-soft);

        &:hover {
          background: var(--fbz-color-panel-elevated);
          color: var(--fbz-color-text);
        }
      }

      &.danger {
        background: transparent;
        border: 1px solid var(--fbz-color-danger-500);
        color: var(--fbz-color-danger-500);

        &:hover {
          background: color-mix(in srgb, var(--fbz-color-danger-500) 8%, transparent);
        }
      }
    }
  }
}

.store-empty {
  grid-column: 1 / -1;
  text-align: center;
  padding: 40px;
  background: var(--fbz-color-panel-strong);
  border: 1px dashed var(--fbz-color-line-soft);
  border-radius: 6px;
  color: var(--fbz-color-text-muted);
  font-size: var(--fbz-font-size-sm);
}

.spinner {
  width: 12px;
  height: 12px;
  border: 2px solid #07120a;
  border-top-color: transparent;
  border-radius: 50%;
  animation: spin 0.8s linear infinite;
}

@keyframes spin {
  to {
    transform: rotate(360deg);
  }
}
</style>
