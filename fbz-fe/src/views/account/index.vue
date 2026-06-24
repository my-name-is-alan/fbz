<script setup lang="ts">
import { useThemeStore } from "@/stores/theme.ts";
import { useLibraryStore } from "@/stores/library.ts";
import { useUiStore } from "@/stores/ui.ts";
import { libraryCovers } from "@/service/modules/tmdb.ts";

const route = useRoute();
const themeStore = useThemeStore();
const libraryStore = useLibraryStore();
const uiStore = useUiStore();

const pageMap: Record<string, { title: string; desc: string }> = {
  "admin-dashboard": {
    title: "控制面板",
    desc: "媒体库状态总览、系统资源监控和最近入库动态。",
  },
  "admin-profile": {
    title: "个人信息",
    desc: "更新您的账户昵称、登录账号、密码以及多媒体点播默认语言偏好设置。",
  },
  "admin-theme": {
    title: "主题设置",
    desc: "个性化系统界面主题、自定义品牌高亮主题色，配置系统视觉交互样式。",
  },
  "admin-lib-sort": {
    title: "媒体库排序",
    desc: "调整媒体库在前台首页和导航中的显示顺序。",
  },
  "admin-metadata": {
    title: "元数据设置",
    desc: "配置搜刮引擎优先级、元数据语言偏好和海报抓取策略。",
  },
  "admin-libraries": {
    title: "媒体库管理",
    desc: "配置搜刮引擎、配置磁盘文件路径，或调整自动扫码与入库通知任务。",
  },
  "admin-transcode": {
    title: "转码设置",
    desc: "配置硬件加速、转码质量和流媒体输出参数。",
  },
  "admin-users": {
    title: "用户管理",
    desc: "管理系统用户账户、权限和访问控制。",
  },
  "admin-users-create": {
    title: "添加用户",
    desc: "创建自托管系统的新访问账号及媒体库浏览权限。",
  },
  "admin-users-edit": {
    title: "编辑用户控制权限",
    desc: "修改系统用户基本信息、系统控制权限组和授权媒体库。",
  },
  "admin-plugins": {
    title: "插件设置",
    desc: "安装、配置和管理系统插件扩展。",
  },
  "admin-metadata-mgr": {
    title: "元数据管理",
    desc: "批量管理、修复和刷新媒体元数据与图片缓存。",
  },
  "admin-logs": {
    title: "系统日志",
    desc: "实时显示视频文件监控、元数据入库详情及后台扫描任务日志。",
  },
  "admin-about": {
    title: "关于",
    desc: "系统版本信息、开源许可和技术支持。",
  },
};

const page = computed(() => {
  const name = String(route.name ?? "admin-dashboard");
  return pageMap[name] ?? { title: "系统控制台", desc: "" };
});

// 预设品牌色选项
const presetColors = [
  { label: "经典绿", value: "#1ed760" },
  { label: "爱奇艺红", value: "#e50914" },
  { label: "天空蓝", value: "#0063e5" },
  { label: "芒果黄", value: "#ff9900" },
  { label: "优雅紫", value: "#8b5cf6" },
  { label: "科技青", value: "#00f5d4" },
];

/* ---------- Admin: Library Settings Section ---------- */
const libraryTypeOptions = [
  { label: "电影 (Movie)", value: "movie" },
  { label: "电视剧 (TV Series)", value: "series" },
  { label: "动漫 (Anime)", value: "anime" },
  { label: "纪录片 (Documentary)", value: "documentary" },
  { label: "音乐 (Music)", value: "music" },
];

/** Library type → icon SVG path data and accent color */
const libTypeVisuals: Record<string, { icon: string; accent: string }> = {
  movie: {
    icon: "M2 2h20v20H2z M7 2v20 M17 2v20 M2 12h20 M2 7h5 M2 17h5 M17 17h5 M17 7h5",
    accent: "#0ea5e9",
  },
  series: {
    icon: "M2 7h20v15H2z M17 2l-5 5-5-5",
    accent: "#8b5cf6",
  },
  anime: {
    icon: "M12 2l3.09 6.26L22 9.27l-5 4.87L18.18 21 12 17.77 5.82 21 7 14.14l-5-4.87 6.91-1.01L12 2z",
    accent: "#f43f5e",
  },
  documentary: {
    icon: "M12 2a10 10 0 1 0 0 20 10 10 0 0 0 0-20z M2 12h20 M12 2a15.3 15.3 0 0 1 4 10 15.3 15.3 0 0 1-4 10 15.3 15.3 0 0 1-4-10 15.3 15.3 0 0 1 4-10z",
    accent: "#10b981",
  },
  music: {
    icon: "M9 18V5l12-2v13 M6 18a3 3 0 1 0 0-6 3 3 0 0 0 0 6z M18 16a3 3 0 1 0 0-6 3 3 0 0 0 0 6z",
    accent: "#f59e0b",
  },
};

function getLibTypeName(kind: string) {
  return libraryTypeOptions.find((o) => o.value === kind)?.label.split(" ")[0] ?? "未知";
}

function getLibVisuals(kind: string) {
  return libTypeVisuals[kind] ?? libTypeVisuals.movie;
}

/** 各库封面剧照（取最新入库的前 4 张 backdrop） */
const coverMap = computed(() => libraryCovers());

function getLibCover(libId: string): string | undefined {
  return coverMap.value[libId]?.[0];
}

function handleEditLibrary(lib: any) {
  uiStore.openLibraryEditor(lib.id);
}

function handleAddLibrary() {
  uiStore.openLibraryEditor(null);
}
</script>

<template>
  <main class="account-view">
    <!-- Header Banner -->
    <div class="panel-header-banner">
      <h1 class="page-heading">{{ page.title }}</h1>
      <p class="description-text">{{ page.desc }}</p>
    </div>
    <!-- 控制面板 -->
    <AdminDashboard v-if="route.name === 'admin-dashboard'" />

    <!-- 媒体库管理 -->
    <div v-else-if="route.name === 'admin-libraries'" class="admin-section">
      <div class="lib-manager-view">
        <div class="section-label">
          <span class="label-text">已挂载影视媒体库</span>
          <span class="label-count">{{ libraryStore.libraries.length }}</span>
        </div>

        <div class="lib-cards-grid">
          <!-- Library cards -->
          <div
            v-for="lib in libraryStore.libraries"
            :key="lib.id"
            class="lib-preview-card"
            :class="{ 'has-cover': getLibCover(lib.id) }"
            @click="handleEditLibrary(lib)"
          >
            <!-- Cover backdrop strip -->
            <div class="card-cover" v-if="getLibCover(lib.id)">
              <img :src="getLibCover(lib.id)" alt="" class="cover-img" loading="lazy" />
              <div class="cover-gradient" />
            </div>
            <div class="card-accent-bar" :style="{ background: getLibVisuals(lib.kind).accent }" />
            <div class="card-content">
              <div class="card-top">
                <span
                  class="lib-icon-container"
                  :style="{ '--icon-accent': getLibVisuals(lib.kind).accent }"
                >
                  <svg
                    viewBox="0 0 24 24"
                    width="18"
                    height="18"
                    fill="none"
                    stroke="currentColor"
                    stroke-width="2"
                    stroke-linecap="round"
                    stroke-linejoin="round"
                  >
                    <path :d="getLibVisuals(lib.kind).icon" />
                  </svg>
                </span>
                <div class="card-title-area">
                  <span class="lib-name">{{ lib.name }}</span>
                  <span class="lib-badge">{{ getLibTypeName(lib.kind) }}</span>
                </div>
                <div class="item-stat">
                  <span class="num">{{ lib.count }}</span>
                  <span class="lbl">条目</span>
                </div>
              </div>
              <div class="card-bottom">
                <svg
                  viewBox="0 0 24 24"
                  width="12"
                  height="12"
                  fill="none"
                  stroke="currentColor"
                  stroke-width="2"
                  stroke-linecap="round"
                  stroke-linejoin="round"
                  class="path-icon"
                >
                  <path
                    d="M22 19a2 2 0 0 1-2 2H4a2 2 0 0 1-2-2V5a2 2 0 0 1 2-2h5l2 3h9a2 2 0 0 1 2 2z"
                  />
                </svg>
                <span class="path-val">{{
                  (lib as any).paths?.[0] || `/media/nas/${lib.name}`
                }}</span>
                <svg
                  viewBox="0 0 24 24"
                  width="14"
                  height="14"
                  fill="none"
                  stroke="currentColor"
                  stroke-width="2"
                  stroke-linecap="round"
                  stroke-linejoin="round"
                  class="edit-icon"
                >
                  <path d="M11 4H4a2 2 0 0 0-2 2v14a2 2 0 0 0 2 2h14a2 2 0 0 0 2-2v-7" />
                  <path d="M18.5 2.5a2.121 2.121 0 1 1 3 3L12 15l-4 1 1-4Z" />
                </svg>
              </div>
            </div>
          </div>

          <!-- Add Library placeholder card -->
          <button class="add-lib-card" type="button" @click="handleAddLibrary">
            <svg
              viewBox="0 0 24 24"
              width="24"
              height="24"
              fill="none"
              stroke="currentColor"
              stroke-width="1.5"
              stroke-linecap="round"
              stroke-linejoin="round"
            >
              <line x1="12" y1="5" x2="12" y2="19" />
              <line x1="5" y1="12" x2="19" y2="12" />
            </svg>
            <span>添加媒体库</span>
          </button>
        </div>
      </div>
    </div>

    <!-- 个人信息 -->
    <AdminProfile v-else-if="route.name === 'admin-profile'" />

    <!-- 主题设置 -->
    <div v-else-if="route.name === 'admin-theme'" class="personalization-section">
      <div class="style-settings-stack">
        <!-- Card 1: Theme selection -->
        <section class="settings-card">
          <div class="card-header">
            <span class="indicator" />
            <h3>系统主题外观</h3>
          </div>
          <div class="card-body">
            <p class="settings-hint">选择您偏好的视觉背景模式。</p>
            <div class="theme-options-grid">
              <button
                class="theme-card dark-opt"
                :class="{ active: themeStore.themeMode === 'dark' }"
                type="button"
                @click="themeStore.setThemeMode('dark')"
              >
                <div class="theme-preview dark-preview">
                  <span class="circle-dot" />
                  <span class="line-bar" />
                </div>
                <span class="label">暗黑模式 (Dark Mode)</span>
              </button>

              <button
                class="theme-card light-opt"
                :class="{ active: themeStore.themeMode === 'light' }"
                type="button"
                @click="themeStore.setThemeMode('light')"
              >
                <div class="theme-preview light-preview">
                  <span class="circle-dot" />
                  <span class="line-bar" />
                </div>
                <span class="label">明亮模式 (Light Mode)</span>
              </button>
            </div>
          </div>
        </section>

        <!-- Card 2: Brand Color selection -->
        <section class="settings-card">
          <div class="card-header">
            <span class="indicator" />
            <h3>全局强调主色调</h3>
          </div>
          <div class="card-body">
            <p class="settings-hint">更改主操作按钮、图标、激活状态和播放进度条的色系。</p>
            <div class="color-options-flex">
              <button
                v-for="color in presetColors"
                :key="color.value"
                class="brand-color-dot"
                :class="{ active: themeStore.brandColor === color.value }"
                :style="{ '--color-val': color.value }"
                type="button"
                :title="color.label"
                @click="themeStore.setBrandColor(color.value)"
              >
                <svg
                  v-if="themeStore.brandColor === color.value"
                  viewBox="0 0 24 24"
                  width="12"
                  height="12"
                  fill="none"
                  stroke="#fff"
                  stroke-width="3"
                  stroke-linecap="round"
                  stroke-linejoin="round"
                >
                  <polyline points="20 6 9 17 4 12" />
                </svg>
              </button>

              <div class="custom-color-picker-wrapper">
                <label class="custom-picker-btn">
                  <input
                    type="color"
                    :value="themeStore.brandColor"
                    @input="(e) => themeStore.setBrandColor((e.target as HTMLInputElement).value)"
                    class="hidden-color-input"
                  />
                  <span
                    class="color-indicator-circle"
                    :style="{ background: themeStore.brandColor }"
                  />
                  <span class="text">自定义色彩</span>
                </label>
              </div>
            </div>
          </div>
        </section>

        <!-- Card 3: Reset / Dev tools -->
        <section class="settings-card dev-card">
          <div class="card-header">
            <span class="indicator dev-indicator" />
            <h3>开发调试与重置</h3>
          </div>
          <div class="card-body">
            <p class="settings-hint">
              您可以清空本地缓存，重新激活首次进入向导流程以测试配置效果。
            </p>
            <button class="relaunch-wizard-btn" type="button" @click="uiStore.resetInitialization">
              <svg
                viewBox="0 0 24 24"
                width="14"
                height="14"
                fill="none"
                stroke="currentColor"
                stroke-width="2"
                stroke-linecap="round"
                stroke-linejoin="round"
              >
                <polyline points="23 4 23 10 17 10" />
                <path d="M20.49 15a9 9 0 1 1-2.12-9.36L23 10" />
              </svg>
              重新拉起初始化向导
            </button>
          </div>
        </section>
      </div>
    </div>

    <!-- 媒体库排序 -->
    <AdminLibSort v-else-if="route.name === 'admin-lib-sort'" />

    <!-- 元数据设置 -->
    <AdminMetadata v-else-if="route.name === 'admin-metadata'" />

    <!-- 转码设置 -->
    <AdminTranscode v-else-if="route.name === 'admin-transcode'" />

    <!-- 用户管理 -->
    <AdminUsers v-else-if="route.name === 'admin-users'" />

    <AdminUserEdit
      v-else-if="route.name === 'admin-users-create' || route.name === 'admin-users-edit'"
    />

    <!-- 插件设置 -->
    <AdminPlugins v-else-if="route.name === 'admin-plugins'" />

    <!-- 元数据管理 -->
    <AdminMetadataMgr v-else-if="route.name === 'admin-metadata-mgr'" />

    <!-- 系统日志 -->
    <AdminLogs v-else-if="route.name === 'admin-logs'" />

    <!-- 关于 -->
    <AdminAbout v-else-if="route.name === 'admin-about'" />
  </main>
</template>

<style scoped lang="scss">
.account-view {
  min-height: 100vh;
  padding: calc(var(--header-h, 60px) + var(--fbz-space-4)) var(--fbz-space-8) 80px;
  max-width: 1200px;
  margin: 0 auto;
}

.panel-header-banner {
  border-bottom: 1px solid var(--fbz-color-line-soft);
  padding-bottom: var(--fbz-space-5);
  margin-bottom: var(--fbz-space-6);

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
  }
}

/* ---------- TAB 1: Media Library Grid Listing ---------- */
.lib-manager-view {
  display: flex;
  flex-direction: column;
  gap: var(--fbz-space-5);
}

.section-label {
  display: flex;
  align-items: center;
  gap: 8px;

  .label-text {
    font-size: 13px;
    font-weight: 700;
    color: var(--fbz-color-text-soft);
    text-transform: uppercase;
    letter-spacing: 0.5px;
  }

  .label-count {
    font-family: var(--fbz-font-display);
    font-size: 11px;
    font-weight: 800;
    color: var(--fbz-color-text-muted);
    background: var(--fbz-color-panel-strong);
    border: 1px solid var(--fbz-color-line-soft);
    padding: 1px 8px;
    border-radius: var(--fbz-radius-round);
  }
}

.lib-cards-grid {
  display: grid;
  grid-template-columns: repeat(auto-fill, minmax(300px, 1fr));
  gap: var(--fbz-space-3);
}

.lib-preview-card {
  border: 1px solid var(--fbz-color-line-soft);
  background: var(--fbz-color-panel-strong);
  border-radius: 6px;
  cursor: pointer;
  transition: all var(--fbz-motion-base);
  overflow: hidden;
  position: relative;
  height: 160px;

  .card-cover {
    position: absolute;
    inset: 0;
    width: 100%;
    height: 100%;
    overflow: hidden;
    transform: translateZ(0);
    z-index: 0;

    .cover-img {
      width: 100%;
      height: 100%;
      object-fit: cover;
      display: block;
      transition: transform var(--fbz-motion-slow);
      backface-visibility: hidden;
      will-change: transform;
    }

    .cover-gradient {
      position: absolute;
      inset: 0;
      background: linear-gradient(
        to bottom,
        rgba(10, 10, 11, 0.1) 0%,
        rgba(10, 10, 11, 0.5) 45%,
        rgba(10, 10, 11, 0.95) 100%
      );
      z-index: 1;
    }
  }

  &.has-cover {
    .lib-name,
    .num {
      color: #ffffff;
      text-shadow: 0 1px 4px rgba(0, 0, 0, 0.5);
    }
    .lib-badge,
    .lbl,
    .path-val,
    .path-icon {
      color: rgba(255, 255, 255, 0.7);
      text-shadow: 0 1px 4px rgba(0, 0, 0, 0.5);
    }
    .card-bottom {
      border-top-color: rgba(255, 255, 255, 0.1);
    }
    .lib-icon-container {
      background: rgba(10, 10, 11, 0.6);
      backdrop-filter: blur(4px);
      border-color: rgba(255, 255, 255, 0.1);
    }
  }

  &.has-cover .card-accent-bar {
    display: none;
  }

  .card-accent-bar {
    height: 3px;
    width: 100%;
    opacity: 0.6;
    transition: opacity var(--fbz-motion-fast);
  }

  &:hover {
    border-color: var(--fbz-color-brand-500);
    box-shadow: 0 8px 24px rgba(0, 0, 0, 0.25);
    transform: translateY(-1px);

    .cover-img {
      transform: scale(1.04);
    }

    .card-accent-bar {
      opacity: 1;
    }

    .edit-icon {
      opacity: 1;
      color: var(--fbz-color-brand-500);
    }
  }

  .card-content {
    padding: 16px 18px;
    display: flex;
    flex-direction: column;
    justify-content: space-between;
    position: relative;
    z-index: 2;
    height: 100%;
  }

  .card-top {
    display: flex;
    align-items: center;
    gap: 12px;
  }

  .lib-icon-container {
    width: 38px;
    height: 38px;
    background: var(--fbz-color-panel);
    border: 1px solid var(--fbz-color-line-soft);
    border-radius: var(--fbz-radius-control);
    display: grid;
    place-content: center;
    color: var(--icon-accent, var(--fbz-color-text-soft));
    flex-shrink: 0;
    transition: all var(--fbz-motion-fast);
  }

  .card-title-area {
    flex: 1;
    min-width: 0;
    display: flex;
    flex-direction: column;
    gap: 3px;

    .lib-name {
      font-size: 14px;
      font-weight: 700;
      color: var(--fbz-color-text);
      white-space: nowrap;
      overflow: hidden;
      text-overflow: ellipsis;
    }

    .lib-badge {
      font-size: 10px;
      font-weight: 700;
      color: var(--fbz-color-text-muted);
      text-transform: uppercase;
      letter-spacing: 0.5px;
    }
  }

  .item-stat {
    display: flex;
    flex-direction: column;
    align-items: flex-end;
    flex-shrink: 0;

    .num {
      font-family: var(--fbz-font-display);
      font-size: 16px;
      font-weight: 800;
      color: var(--fbz-color-text);
      line-height: 1;
    }

    .lbl {
      font-size: 9px;
      color: var(--fbz-color-text-muted);
      font-weight: 700;
      margin-top: 2px;
    }
  }

  .card-bottom {
    display: flex;
    align-items: center;
    gap: 6px;
    padding-top: 10px;
    border-top: 1px solid var(--fbz-color-line-soft);

    .path-icon {
      color: var(--fbz-color-text-muted);
      flex-shrink: 0;
      opacity: 0.6;
    }

    .path-val {
      flex: 1;
      font-size: 11px;
      color: var(--fbz-color-text-muted);
      white-space: nowrap;
      overflow: hidden;
      text-overflow: ellipsis;
    }

    .edit-icon {
      flex-shrink: 0;
      color: var(--fbz-color-text-muted);
      opacity: 0;
      transition: all var(--fbz-motion-fast);
    }
  }
}

/* Add Library placeholder card */
.add-lib-card {
  border: 1px dashed var(--fbz-color-line-bright);
  background: transparent;
  border-radius: 6px;
  display: flex;
  flex-direction: column;
  align-items: center;
  justify-content: center;
  gap: 10px;
  height: 160px;
  color: var(--fbz-color-text-muted);
  cursor: pointer;
  transition: all var(--fbz-motion-base);

  svg {
    opacity: 0.5;
    transition: all var(--fbz-motion-fast);
  }

  span {
    font-size: 12px;
    font-weight: 600;
  }

  &:hover {
    border-color: var(--fbz-color-brand-500);
    color: var(--fbz-color-brand-500);
    background: color-mix(in srgb, var(--fbz-color-brand-500) 3%, transparent);

    svg {
      opacity: 1;
    }
  }
}

/* ---------- 2. Library Config Modal Dialog ---------- */
.editor-modal-overlay {
  position: fixed;
  inset: 0;
  z-index: 100;
  background: rgba(0, 0, 0, 0.7);
  backdrop-filter: blur(8px);
  display: grid;
  place-content: center;
}

.editor-modal-container {
  width: 720px;
  max-width: 95vw;
  height: 80vh;
  max-height: 700px;
  background: var(--fbz-color-panel);
  border: 1px solid var(--fbz-color-line-bright);
  border-radius: 8px;
  box-shadow: var(--fbz-shadow-panel);
  display: flex;
  flex-direction: column;
  overflow: hidden;
  color: var(--fbz-color-text);
  font-family: var(--fbz-font-sans);
}

.modal-title-bar {
  padding: var(--fbz-space-4) var(--fbz-space-5);
  border-bottom: 1px solid var(--fbz-color-line-soft);
  display: flex;
  justify-content: space-between;
  align-items: center;
  background: var(--fbz-color-bg-strong);

  .title-left {
    display: flex;
    align-items: center;
    gap: 10px;

    .header-icon {
      color: var(--fbz-color-brand-500);
    }

    h2 {
      margin: 0;
      font-size: 15px;
      font-weight: 700;
    }
  }

  .modal-close-btn {
    background: none;
    border: 0;
    color: var(--fbz-color-text-muted);
    cursor: pointer;
    padding: 4px;
    display: grid;
    place-content: center;
    border-radius: var(--fbz-radius-control);
    transition: all var(--fbz-motion-fast);

    &:hover {
      color: var(--fbz-color-text);
      background: var(--fbz-color-panel-strong);
    }
  }
}

.modal-scroll-body {
  flex: 1;
  overflow-y: auto;
  padding: var(--fbz-space-5);
  background: var(--fbz-color-bg);
}

.settings-cards-stack {
  display: flex;
  flex-direction: column;
  gap: var(--fbz-space-4);
}

.settings-card {
  background: var(--fbz-color-panel-strong);
  border: 1px solid var(--fbz-color-line-soft);
  border-radius: 6px;
  overflow: hidden;

  .card-header {
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

      &.dev-indicator {
        background: var(--fbz-color-amber-500);
      }
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
    display: flex;
    flex-direction: column;
    gap: var(--fbz-space-4);
  }
}

.form-group {
  display: flex;
  flex-direction: column;
  gap: 8px;

  label {
    font-size: var(--fbz-font-size-xs);
    font-weight: 700;
    color: var(--fbz-color-text-soft);
  }

  .control-input {
    height: 38px;
    background: var(--fbz-color-panel);
    border: 1px solid var(--fbz-color-line);
    border-radius: var(--fbz-radius-control);
    padding: 0 var(--fbz-space-3);
    color: var(--fbz-color-text);
    font-size: var(--fbz-font-size-sm);
    transition: all var(--fbz-motion-fast);

    &:focus {
      outline: none;
      border-color: var(--fbz-color-brand-500);
      box-shadow: var(--fbz-shadow-focus);
    }
  }
}

.scrapers-grid {
  display: grid;
  grid-template-columns: repeat(3, 1fr);
  gap: var(--fbz-space-3);
}

.scraper-item-card {
  border: 1px solid var(--fbz-color-line);
  background: var(--fbz-color-panel);
  border-radius: var(--fbz-radius-control);
  padding: var(--fbz-space-4) var(--fbz-space-3);
  cursor: pointer;
  display: flex;
  flex-direction: column;
  align-items: center;
  gap: 8px;
  text-align: center;
  transition: all var(--fbz-motion-base);
  position: relative;

  input {
    position: absolute;
    top: 10px;
    right: 10px;
    accent-color: var(--fbz-color-brand-500);
  }

  .scraper-logo {
    font-family: var(--fbz-font-display);
    font-size: 12px;
    font-weight: 800;
    padding: 2px 8px;
    border-radius: 4px;

    &.tmdb-logo {
      background: #0d253f;
      color: #01b4e4;
    }

    &.imdb-logo {
      background: #f5c518;
      color: #000000;
    }

    &.nfo-logo {
      background: var(--fbz-color-line-bright);
      color: var(--fbz-color-text);
    }
  }

  .scraper-name {
    font-size: 10px;
    font-weight: 600;
    color: var(--fbz-color-text-muted);
  }

  &:hover {
    border-color: var(--fbz-color-line-bright);
    background: var(--fbz-color-panel-elevated);
  }

  &.active {
    border-color: var(--fbz-color-brand-500);
    background: color-mix(in srgb, var(--fbz-color-brand-500) 5%, var(--fbz-color-panel));
  }
}

.paths-stack {
  display: flex;
  flex-direction: column;
  gap: var(--fbz-space-2);
}

.path-row-item {
  display: flex;
  gap: var(--fbz-space-2);
  align-items: center;

  .input-glow-wrapper {
    flex: 1;
    display: flex;
    align-items: center;
    background: var(--fbz-color-panel);
    border: 1px solid var(--fbz-color-line);
    border-radius: var(--fbz-radius-control);
    padding: 0 10px;
    height: 38px;
    transition: all var(--fbz-motion-fast);

    &:focus-within {
      border-color: var(--fbz-color-brand-500);
      box-shadow: var(--fbz-shadow-focus);
    }

    .path-svg {
      color: var(--fbz-color-text-muted);
      margin-right: 8px;
      flex-shrink: 0;
    }

    input {
      background: transparent;
      border: 0;
      height: 100%;
      width: 100%;
      color: var(--fbz-color-text);
      font-size: var(--fbz-font-size-sm);

      &:focus {
        outline: none;
      }
    }
  }

  .browse-action-btn {
    height: 38px;
    padding: 0 var(--fbz-space-4);
    background: var(--fbz-color-panel);
    border: 1px solid var(--fbz-color-line);
    border-radius: var(--fbz-radius-control);
    color: var(--fbz-color-text-soft);
    font-weight: 700;
    font-size: var(--fbz-font-size-sm);
    cursor: pointer;
    transition: all var(--fbz-motion-fast);

    &:hover {
      background: var(--fbz-color-panel-elevated);
      color: var(--fbz-color-text);
    }
  }

  .remove-action-btn {
    width: 38px;
    height: 38px;
    background: transparent;
    border: 1px solid var(--fbz-color-line);
    border-radius: var(--fbz-radius-control);
    color: var(--fbz-color-danger-500);
    cursor: pointer;
    display: grid;
    place-content: center;
    transition: all var(--fbz-motion-fast);

    &:hover:not(:disabled) {
      background: color-mix(in srgb, var(--fbz-color-danger-500) 8%, transparent);
      border-color: var(--fbz-color-danger-500);
    }

    &:disabled {
      opacity: 0.3;
      cursor: not-allowed;
    }
  }
}

.add-path-action-btn {
  align-self: flex-start;
  background: transparent;
  border: 1px dashed var(--fbz-color-line-bright);
  border-radius: var(--fbz-radius-control);
  color: var(--fbz-color-text-soft);
  height: 34px;
  padding: 0 var(--fbz-space-4);
  font-size: var(--fbz-font-size-xs);
  font-weight: 700;
  cursor: pointer;
  display: flex;
  align-items: center;
  gap: 6px;
  transition: all var(--fbz-motion-fast);

  &:hover {
    border-color: var(--fbz-color-brand-500);
    color: var(--fbz-color-brand-500);
    background: color-mix(in srgb, var(--fbz-color-brand-500) 3%, transparent);
  }
}

.toggles-list {
  display: flex;
  flex-direction: column;
  gap: var(--fbz-space-4);
}

.toggle-row-item {
  display: flex;
  justify-content: space-between;
  align-items: center;
  gap: var(--fbz-space-6);

  .toggle-text {
    display: flex;
    flex-direction: column;
    gap: 3px;

    .title {
      font-size: var(--fbz-font-size-sm);
      font-weight: 700;
      color: var(--fbz-color-text);
    }

    .desc {
      font-size: var(--fbz-font-size-xs);
      color: var(--fbz-color-text-muted);
      line-height: 1.4;
    }
  }
}

.glow-switch {
  position: relative;
  display: inline-block;
  width: 44px;
  height: 22px;
  flex-shrink: 0;

  input {
    opacity: 0;
    width: 0;
    height: 0;
  }

  .switch-slide-thumb {
    position: absolute;
    cursor: pointer;
    inset: 0;
    background-color: var(--fbz-color-line-bright);
    border-radius: 22px;
    transition: background-color var(--fbz-motion-fast);

    &::before {
      position: absolute;
      content: "";
      height: 16px;
      width: 16px;
      left: 3px;
      bottom: 3px;
      background-color: white;
      border-radius: 50%;
      transition: transform var(--fbz-motion-fast);
      box-shadow: 0 1px 3px rgba(0, 0, 0, 0.2);
    }
  }

  input:checked + .switch-slide-thumb {
    background-color: var(--fbz-color-brand-500);

    &::before {
      transform: translateX(22px);
    }
  }
}

.editor-actions-footer {
  padding: var(--fbz-space-3) var(--fbz-space-5);
  border-top: 1px solid var(--fbz-color-line-soft);
  display: flex;
  align-items: center;
  background: var(--fbz-color-bg-strong);

  .spacer {
    flex: 1;
  }

  .action-btn-primary {
    height: 36px;
    padding: 0 var(--fbz-space-5);
    background: var(--fbz-color-brand-500);
    border: 0;
    color: #07120a;
    font-weight: 700;
    font-size: var(--fbz-font-size-sm);
    border-radius: var(--fbz-radius-control);
    cursor: pointer;
    transition: all var(--fbz-motion-fast);

    &:hover {
      background: var(--fbz-color-brand-600);
      box-shadow: 0 0 12px color-mix(in srgb, var(--fbz-color-brand-500) 30%, transparent);
    }
  }

  .action-btn-secondary {
    height: 36px;
    padding: 0 var(--fbz-space-5);
    background: var(--fbz-color-panel-strong);
    border: 1px solid var(--fbz-color-line);
    color: var(--fbz-color-text-soft);
    font-weight: 700;
    font-size: var(--fbz-font-size-sm);
    border-radius: var(--fbz-radius-control);
    cursor: pointer;
    transition: all var(--fbz-motion-fast);
    margin-right: var(--fbz-space-2);

    &:hover {
      background: var(--fbz-color-panel-elevated);
      color: var(--fbz-color-text);
    }
  }

  .action-btn-danger {
    height: 36px;
    padding: 0 var(--fbz-space-4);
    background: transparent;
    border: 1px solid var(--fbz-color-danger-500);
    color: var(--fbz-color-danger-500);
    font-weight: 700;
    font-size: var(--fbz-font-size-sm);
    border-radius: var(--fbz-radius-control);
    cursor: pointer;
    display: flex;
    align-items: center;
    gap: 6px;
    transition: all var(--fbz-motion-fast);

    &:hover {
      background: color-mix(in srgb, var(--fbz-color-danger-500) 8%, transparent);
    }
  }
}

.confirm-glass-overlay {
  position: absolute;
  inset: 0;
  background: rgba(10, 10, 11, 0.88);
  backdrop-filter: blur(12px);
  display: grid;
  place-content: center;
  z-index: 110;
  padding: var(--fbz-space-5);
}

.confirm-panel-card {
  width: 400px;
  max-width: 90vw;
  background: var(--fbz-color-panel-elevated);
  border: 1px solid var(--fbz-color-line-bright);
  border-radius: 8px;
  padding: var(--fbz-space-5);
  text-align: center;
  box-shadow: var(--fbz-shadow-panel);

  .warn-icon {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    width: 48px;
    height: 48px;
    border-radius: 50%;
    background: color-mix(in srgb, var(--fbz-color-danger-500) 10%, transparent);
    color: var(--fbz-color-danger-500);
    margin-bottom: var(--fbz-space-3);
  }

  h3 {
    margin: 0 0 10px;
    font-size: 15px;
    font-weight: 700;
  }

  p {
    margin: 0 0 var(--fbz-space-4);
    font-size: var(--fbz-font-size-sm);
    color: var(--fbz-color-text-soft);
    line-height: 1.6;
    b {
      color: var(--fbz-color-text);
    }
  }

  .actions-row {
    display: flex;
    gap: var(--fbz-space-3);
  }

  .confirm-btn {
    flex: 1;
    height: 36px;
    border-radius: var(--fbz-radius-control);
    font-size: var(--fbz-font-size-sm);
    font-weight: 700;
    cursor: pointer;
    transition: all var(--fbz-motion-fast);

    &.cancel {
      background: var(--fbz-color-panel-strong);
      border: 1px solid var(--fbz-color-line);
      color: var(--fbz-color-text-soft);

      &:hover {
        background: var(--fbz-color-panel);
      }
    }

    &.confirm {
      background: var(--fbz-color-danger-500);
      border: 0;
      color: #ffffff;

      &:hover {
        background: color-mix(in srgb, var(--fbz-color-danger-500) 84%, black);
      }
    }
  }
}

/* ---------- TAB 2: Appearance & Dev settings ---------- */
.personalization-section {
  display: flex;
  flex-direction: column;
  gap: var(--fbz-space-5);
}

.style-settings-stack {
  display: flex;
  flex-direction: column;
  gap: var(--fbz-space-4);
}

.settings-hint {
  margin: 0 0 var(--fbz-space-3);
  font-size: var(--fbz-font-size-sm);
  color: var(--fbz-color-text-muted);
}

.theme-options-grid {
  display: grid;
  grid-template-columns: repeat(2, 1fr);
  gap: var(--fbz-space-4);
}

.theme-card {
  border: 1px solid var(--fbz-color-line);
  background: var(--fbz-color-panel-strong);
  border-radius: 6px;
  padding: var(--fbz-space-4);
  cursor: pointer;
  display: flex;
  flex-direction: column;
  gap: var(--fbz-space-3);
  align-items: center;
  transition: all var(--fbz-motion-base);

  &:hover {
    border-color: var(--fbz-color-line-bright);
    transform: translateY(-1px);
  }

  &.active {
    border-color: var(--fbz-color-brand-500);
    background: color-mix(in srgb, var(--fbz-color-brand-500) 4%, var(--fbz-color-panel-strong));

    .label {
      color: var(--fbz-color-brand-500);
    }
  }

  .theme-preview {
    width: 100%;
    height: 56px;
    border-radius: var(--fbz-radius-control);
    display: flex;
    align-items: center;
    justify-content: center;
    gap: var(--fbz-space-2);
    border: 1px solid var(--fbz-color-line-soft);
  }

  .dark-preview {
    background: #0a0a0b;
    .circle-dot {
      background: #1ed760;
    }
    .line-bar {
      background: #ffffff;
    }
  }

  .light-preview {
    background: #f5f5f7;
    .circle-dot {
      background: #0063e5;
    }
    .line-bar {
      background: #1c1c1e;
    }
  }

  .circle-dot {
    width: 8px;
    height: 8px;
    border-radius: 50%;
  }

  .line-bar {
    width: 48px;
    height: 5px;
    border-radius: 3px;
    opacity: 0.8;
  }

  .label {
    font-size: var(--fbz-font-size-sm);
    font-weight: 700;
    color: var(--fbz-color-text-soft);
  }
}

.color-options-flex {
  display: flex;
  flex-wrap: wrap;
  align-items: center;
  gap: var(--fbz-space-3);
}

.brand-color-dot {
  width: 32px;
  height: 32px;
  border-radius: 50%;
  border: 2px solid var(--fbz-color-line);
  background: var(--color-val);
  cursor: pointer;
  position: relative;
  display: grid;
  place-content: center;
  box-shadow: 0 2px 8px rgba(0, 0, 0, 0.15);
  transition: all var(--fbz-motion-fast) cubic-bezier(0.175, 0.885, 0.32, 1.275);

  &:hover {
    transform: scale(1.12);
  }

  &.active {
    border-color: var(--fbz-color-text);
    transform: scale(1.08);
    box-shadow: 0 0 0 3px color-mix(in srgb, var(--color-val) 35%, transparent);
  }
}

.custom-color-picker-wrapper {
  margin-left: 4px;
}

.custom-picker-btn {
  display: flex;
  align-items: center;
  gap: 8px;
  height: 32px;
  padding: 0 var(--fbz-space-3);
  border-radius: var(--fbz-radius-round);
  border: 1px solid var(--fbz-color-line);
  background: var(--fbz-color-panel-strong);
  font-size: var(--fbz-font-size-xs);
  font-weight: 700;
  color: var(--fbz-color-text-soft);
  cursor: pointer;
  position: relative;
  overflow: hidden;
  transition: all var(--fbz-motion-fast);

  &:hover {
    border-color: var(--fbz-color-line-bright);
    background: var(--fbz-color-panel-elevated);
  }

  .hidden-color-input {
    position: absolute;
    top: 0;
    left: 0;
    opacity: 0;
    width: 100%;
    height: 100%;
    cursor: pointer;
  }

  .color-indicator-circle {
    width: 14px;
    height: 14px;
    border-radius: 50%;
    border: 1px solid var(--fbz-color-line);
  }
}

.relaunch-wizard-btn {
  height: 36px;
  padding: 0 var(--fbz-space-4);
  background: var(--fbz-color-panel-strong);
  border: 1px solid var(--fbz-color-line);
  color: var(--fbz-color-text-soft);
  border-radius: var(--fbz-radius-control);
  font-size: var(--fbz-font-size-sm);
  font-weight: 700;
  cursor: pointer;
  display: inline-flex;
  align-items: center;
  gap: 8px;
  transition: all var(--fbz-motion-fast);

  svg {
    flex-shrink: 0;
  }

  &:hover {
    border-color: var(--fbz-color-brand-500);
    color: var(--fbz-color-brand-500);
    background: color-mix(in srgb, var(--fbz-color-brand-500) 3%, transparent);
  }
}

/* ---------- TAB 3: Notification Log View ---------- */
.messages-log-container {
  border: 1px solid var(--fbz-color-line-soft);
  background: var(--fbz-color-panel-strong);
  border-radius: 6px;
  padding: 40px;
}

.messages-empty-state {
  display: flex;
  flex-direction: column;
  justify-content: center;
  align-items: center;
  text-align: center;
  color: var(--fbz-color-text-muted);

  .empty-svg-icon {
    margin-bottom: var(--fbz-space-3);
    color: var(--fbz-color-text-muted);
    opacity: 0.4;
  }

  h3 {
    margin: 0 0 6px;
    font-size: 15px;
    font-weight: 700;
    color: var(--fbz-color-text-soft);
  }

  p {
    margin: 0;
    font-size: var(--fbz-font-size-sm);
  }
}

/* ---------- General Stub Page / Empty State ---------- */
.stub-container {
  border: 1px dashed var(--fbz-color-line-soft);
  background: var(--fbz-color-panel-strong);
  border-radius: 6px;
  padding: 60px var(--fbz-space-5);
  display: flex;
  justify-content: center;
  align-items: center;
}

.stub-state {
  display: flex;
  flex-direction: column;
  justify-content: center;
  align-items: center;
  text-align: center;
  color: var(--fbz-color-text-muted);

  .stub-icon {
    margin-bottom: var(--fbz-space-4);
    color: var(--fbz-color-text-muted);
    opacity: 0.4;
  }

  h3 {
    margin: 0 0 8px;
    font-size: 16px;
    font-weight: 700;
    color: var(--fbz-color-text-soft);
  }

  p {
    margin: 0;
    font-size: var(--fbz-font-size-sm);
  }
}

/* ---------- Transition Animations ---------- */
.modal-fade-enter-active,
.modal-fade-leave-active {
  transition: opacity var(--fbz-motion-base) ease;

  .editor-modal-container {
    transition: transform var(--fbz-motion-base) cubic-bezier(0.16, 1, 0.3, 1);
  }
}
.modal-fade-enter-from,
.modal-fade-leave-to {
  opacity: 0;

  .editor-modal-container {
    transform: scale(0.96) translateY(8px);
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

@media (max-width: 768px) {
  .account-view {
    padding: calc(var(--header-h, 56px) + var(--fbz-space-3)) var(--fbz-space-4) 60px;
  }

  .lib-cards-grid {
    grid-template-columns: 1fr;
  }

  .scrapers-grid {
    grid-template-columns: 1fr;
    gap: var(--fbz-space-2);
  }

  .theme-options-grid {
    grid-template-columns: 1fr;
  }

  .editor-modal-container {
    width: 100vw;
    height: 100vh;
    max-height: 100vh;
    border-radius: 0;
  }
}
</style>
