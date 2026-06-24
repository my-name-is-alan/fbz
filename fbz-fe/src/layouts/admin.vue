<script setup lang="ts">
import { useThemeStore } from "@/stores/theme.ts";
import { useAuthStore } from "@/stores/auth.ts";

const route = useRoute();
const themeStore = useThemeStore();
const authStore = useAuthStore();

const mobileMenuOpen = ref(false);

interface NavLink {
  label: string;
  to: string;
  name: string;
  icon: string;
}

interface NavGroup {
  label: string;
  children: NavLink[];
}

type NavItem = NavLink | NavGroup;

function isGroup(item: NavItem): item is NavGroup {
  return "children" in item;
}

/** 当前路由名是否属于某个 group */
function groupIsActive(group: NavGroup): boolean {
  return group.children.some((c) => c.name === route.name);
}

// SVG icon path 常量（24×24 viewBox, stroke-based）
const ICONS = {
  dashboard: "M3 9l9-7 9 7v11a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2z M9 22V12h6v10",
  theme: "M21 12.79A9 9 0 1 1 11.21 3 7 7 0 0 0 21 12.79Z",
  sort: "M3 6h18 M3 12h12 M3 18h6",
  metadata:
    "M14 2H6a2 2 0 0 0-2 2v16a2 2 0 0 0 2 2h12a2 2 0 0 0 2-2V8z M14 2v6h6 M16 13H8 M16 17H8 M10 9H8",
  library: "M22 19a2 2 0 0 1-2 2H4a2 2 0 0 1-2-2V5a2 2 0 0 1 2-2h5l2 3h9a2 2 0 0 1 2 2z",
  transcode: "M23 7l-7 5 7 5V7z M1 5h15v14H1z",
  users:
    "M17 21v-2a4 4 0 0 0-4-4H5a4 4 0 0 0-4 4v2 M9 11a4 4 0 1 0 0-8 4 4 0 0 0 0 8z M23 21v-2a4 4 0 0 0-3-3.87 M16 3.13a4 4 0 0 1 0 7.75",
  user: "M20 21v-2a4 4 0 0 0-4-4H8a4 4 0 0 0-4 4v2 M12 11a4 4 0 1 0 0-8 4 4 0 0 0 0 8z",
  plugin: "M12 2L2 7l10 5 10-5-10-5z M2 17l10 5 10-5 M2 12l10 5 10-5",
  metaMgr: "M4 7V4h16v3 M9 20h6 M12 4v16",
  log: "M14 2H6a2 2 0 0 0-2 2v16a2 2 0 0 0 2 2h12a2 2 0 0 0 2-2V8z M14 2v6h6 M12 18v-6 M8 18v-4 M16 18v-2",
  info: "M12 22a10 10 0 1 0 0-20 10 10 0 0 0 0 20z M12 16v-4 M12 8h.01",
} as const;

const navItems: NavItem[] = [
  {
    label: "控制面板",
    to: "/admin",
    name: "admin-dashboard",
    icon: ICONS.dashboard,
  },
  {
    label: "个人偏好",
    children: [
      { label: "个人信息", to: "/admin/profile", name: "admin-profile", icon: ICONS.user },
      { label: "主题设置", to: "/admin/theme", name: "admin-theme", icon: ICONS.theme },
      { label: "媒体库排序", to: "/admin/lib-sort", name: "admin-lib-sort", icon: ICONS.sort },
    ],
  },
  {
    label: "媒体设置",
    children: [
      { label: "元数据设置", to: "/admin/metadata", name: "admin-metadata", icon: ICONS.metadata },
      { label: "媒体库管理", to: "/admin/libraries", name: "admin-libraries", icon: ICONS.library },
      { label: "转码设置", to: "/admin/transcode", name: "admin-transcode", icon: ICONS.transcode },
    ],
  },
  {
    label: "系统设置",
    children: [
      { label: "用户管理", to: "/admin/users", name: "admin-users", icon: ICONS.users },
      { label: "插件设置", to: "/admin/plugins", name: "admin-plugins", icon: ICONS.plugin },
      {
        label: "元数据管理",
        to: "/admin/metadata-mgr",
        name: "admin-metadata-mgr",
        icon: ICONS.metaMgr,
      },
      { label: "系统日志", to: "/admin/logs", name: "admin-logs", icon: ICONS.log },
      { label: "关于", to: "/admin/about", name: "admin-about", icon: ICONS.info },
    ],
  },
];

/** 获取当前路由对应的页面标题 */
const currentTitle = computed(() => {
  for (const item of navItems) {
    if (isGroup(item)) {
      const child = item.children.find((c) => c.name === route.name);
      if (child) return child.label;
    } else if (item.name === route.name) {
      return item.label;
    }
  }
  return "系统控制台";
});

function toggleTheme() {
  themeStore.setThemeMode(themeStore.themeMode === "light" ? "dark" : "light");
}

function closeMobileMenu() {
  mobileMenuOpen.value = false;
}

useEventListener(window, "keydown", (e) => {
  if (e.key === "Escape" && mobileMenuOpen.value) {
    closeMobileMenu();
  }
});
</script>

<template>
  <div class="admin-shell" :class="`${themeStore.themeMode}-theme`">
    <!-- Desktop Sidebar -->
    <aside class="sidebar">
      <div class="sidebar-brand">
        <span class="brand-logo">F<b>B</b>Z</span>
        <span class="brand-sub">系统控制台</span>
      </div>

      <nav class="sidebar-nav" aria-label="后台管理主要导航">
        <template v-for="item in navItems" :key="isGroup(item) ? item.label : item.to">
          <!-- Top-level link -->
          <RouterLink
            v-if="!isGroup(item)"
            :to="item.to"
            class="menu-link"
            :class="{ active: route.name === item.name }"
          >
            <svg
              class="menu-svg"
              viewBox="0 0 24 24"
              width="16"
              height="16"
              fill="none"
              stroke="currentColor"
              stroke-width="2"
              stroke-linecap="round"
              stroke-linejoin="round"
            >
              <path :d="item.icon" />
            </svg>
            <span class="menu-label">{{ item.label }}</span>
          </RouterLink>

          <!-- Group with children -->
          <div
            v-else
            class="nav-group"
            role="group"
            :aria-label="item.label"
            :class="{ 'group-active': groupIsActive(item) }"
          >
            <div class="group-label">{{ item.label }}</div>
            <RouterLink
              v-for="child in item.children"
              :key="child.to"
              :to="child.to"
              class="menu-link sub-link"
              :class="{ active: route.name === child.name }"
            >
              <svg
                class="menu-svg"
                viewBox="0 0 24 24"
                width="15"
                height="15"
                fill="none"
                stroke="currentColor"
                stroke-width="2"
                stroke-linecap="round"
                stroke-linejoin="round"
              >
                <path :d="child.icon" />
              </svg>
              <span class="menu-label">{{ child.label }}</span>
            </RouterLink>
          </div>
        </template>

        <div class="menu-separator" />

        <RouterLink to="/" class="menu-link back-link">
          <svg
            class="menu-svg"
            viewBox="0 0 24 24"
            width="16"
            height="16"
            fill="none"
            stroke="currentColor"
            stroke-width="2"
            stroke-linecap="round"
            stroke-linejoin="round"
          >
            <line x1="19" y1="12" x2="5" y2="12" />
            <polyline points="12 19 5 12 12 5" />
          </svg>
          <span class="menu-label">返回前台首页</span>
        </RouterLink>
      </nav>

      <div class="sidebar-footer">
        <button class="theme-toggle-btn" type="button" @click="toggleTheme" title="切换主题">
          <svg
            v-if="themeStore.themeMode === 'light'"
            viewBox="0 0 24 24"
            width="14"
            height="14"
            fill="none"
            stroke="currentColor"
            stroke-width="2"
            stroke-linecap="round"
            stroke-linejoin="round"
          >
            <path d="M21 12.79A9 9 0 1 1 11.21 3 7 7 0 0 0 21 12.79Z" />
          </svg>
          <svg
            v-else
            viewBox="0 0 24 24"
            width="14"
            height="14"
            fill="none"
            stroke="currentColor"
            stroke-width="2"
            stroke-linecap="round"
            stroke-linejoin="round"
          >
            <circle cx="12" cy="12" r="5" />
            <line x1="12" y1="1" x2="12" y2="3" />
            <line x1="12" y1="21" x2="12" y2="23" />
            <line x1="4.22" y1="4.22" x2="5.64" y2="5.64" />
            <line x1="18.36" y1="18.36" x2="19.78" y2="19.78" />
            <line x1="1" y1="12" x2="3" y2="12" />
            <line x1="21" y1="12" x2="23" y2="12" />
            <line x1="4.22" y1="19.78" x2="5.64" y2="18.36" />
            <line x1="18.36" y1="5.64" x2="19.78" y2="4.22" />
          </svg>
          <span>{{ themeStore.themeMode === "light" ? "暗黑模式" : "明亮模式" }}</span>
        </button>
        <div class="footer-meta">
          <span class="status-dot" />
          <span class="version-text">Fbz v0.1.0</span>
        </div>
      </div>
    </aside>

    <!-- Mobile Drawer Overlay -->
    <Transition name="fade">
      <div v-if="mobileMenuOpen" class="mobile-overlay" @click="closeMobileMenu" />
    </Transition>

    <!-- Mobile Sidebar -->
    <Transition name="slide">
      <aside
        v-if="mobileMenuOpen"
        class="mobile-sidebar"
        role="dialog"
        aria-modal="true"
        aria-label="移动端导航侧栏"
      >
        <div class="sidebar-brand">
          <span class="brand-logo">F<b>B</b>Z</span>
          <span class="brand-sub">系统控制台</span>
        </div>

        <nav class="sidebar-nav" aria-label="后台管理移动端导航">
          <template v-for="item in navItems" :key="isGroup(item) ? item.label : item.to">
            <RouterLink
              v-if="!isGroup(item)"
              :to="item.to"
              class="menu-link"
              :class="{ active: route.name === item.name }"
              @click="closeMobileMenu"
            >
              <svg
                class="menu-svg"
                viewBox="0 0 24 24"
                width="16"
                height="16"
                fill="none"
                stroke="currentColor"
                stroke-width="2"
                stroke-linecap="round"
                stroke-linejoin="round"
              >
                <path :d="item.icon" />
              </svg>
              <span class="menu-label">{{ item.label }}</span>
            </RouterLink>

            <div
              v-else
              class="nav-group"
              role="group"
              :aria-label="item.label"
              :class="{ 'group-active': groupIsActive(item) }"
            >
              <div class="group-label">{{ item.label }}</div>
              <RouterLink
                v-for="child in item.children"
                :key="child.to"
                :to="child.to"
                class="menu-link sub-link"
                :class="{ active: route.name === child.name }"
                @click="closeMobileMenu"
              >
                <svg
                  class="menu-svg"
                  viewBox="0 0 24 24"
                  width="15"
                  height="15"
                  fill="none"
                  stroke="currentColor"
                  stroke-width="2"
                  stroke-linecap="round"
                  stroke-linejoin="round"
                >
                  <path :d="child.icon" />
                </svg>
                <span class="menu-label">{{ child.label }}</span>
              </RouterLink>
            </div>
          </template>

          <div class="menu-separator" />

          <RouterLink to="/" class="menu-link back-link" @click="closeMobileMenu">
            <svg
              class="menu-svg"
              viewBox="0 0 24 24"
              width="16"
              height="16"
              fill="none"
              stroke="currentColor"
              stroke-width="2"
              stroke-linecap="round"
              stroke-linejoin="round"
            >
              <line x1="19" y1="12" x2="5" y2="12" />
              <polyline points="12 19 5 12 12 5" />
            </svg>
            <span class="menu-label">返回前台首页</span>
          </RouterLink>
        </nav>

        <div class="sidebar-footer">
          <button class="theme-toggle-btn" type="button" @click="toggleTheme">
            <svg
              v-if="themeStore.themeMode === 'light'"
              viewBox="0 0 24 24"
              width="14"
              height="14"
              fill="none"
              stroke="currentColor"
              stroke-width="2"
              stroke-linecap="round"
              stroke-linejoin="round"
            >
              <path d="M21 12.79A9 9 0 1 1 11.21 3 7 7 0 0 0 21 12.79Z" />
            </svg>
            <svg
              v-else
              viewBox="0 0 24 24"
              width="14"
              height="14"
              fill="none"
              stroke="currentColor"
              stroke-width="2"
              stroke-linecap="round"
              stroke-linejoin="round"
            >
              <circle cx="12" cy="12" r="5" />
              <line x1="12" y1="1" x2="12" y2="3" />
              <line x1="12" y1="21" x2="12" y2="23" />
              <line x1="4.22" y1="4.22" x2="5.64" y2="5.64" />
              <line x1="18.36" y1="18.36" x2="19.78" y2="19.78" />
              <line x1="1" y1="12" x2="3" y2="12" />
              <line x1="21" y1="12" x2="23" y2="12" />
              <line x1="4.22" y1="19.78" x2="5.64" y2="18.36" />
              <line x1="18.36" y1="5.64" x2="19.78" y2="4.22" />
            </svg>
            <span>{{ themeStore.themeMode === "light" ? "暗黑" : "明亮" }}</span>
          </button>
        </div>
      </aside>
    </Transition>

    <!-- Main Container -->
    <div class="main-container">
      <header class="top-bar">
        <button
          class="hamburger-btn"
          type="button"
          @click="mobileMenuOpen = true"
          aria-label="打开菜单"
          :aria-expanded="mobileMenuOpen"
        >
          <svg
            viewBox="0 0 24 24"
            width="20"
            height="20"
            fill="none"
            stroke="currentColor"
            stroke-width="2"
            stroke-linecap="round"
            stroke-linejoin="round"
          >
            <line x1="3" y1="12" x2="21" y2="12" />
            <line x1="3" y1="6" x2="21" y2="6" />
            <line x1="3" y1="18" x2="21" y2="18" />
          </svg>
        </button>

        <h2 class="page-title">{{ currentTitle }}</h2>

        <div class="top-actions">
          <button class="header-theme-btn" type="button" @click="toggleTheme" title="切换主题">
            <svg
              v-if="themeStore.themeMode === 'light'"
              viewBox="0 0 24 24"
              width="16"
              height="16"
              fill="none"
              stroke="currentColor"
              stroke-width="2"
              stroke-linecap="round"
              stroke-linejoin="round"
            >
              <path d="M21 12.79A9 9 0 1 1 11.21 3 7 7 0 0 0 21 12.79Z" />
            </svg>
            <svg
              v-else
              viewBox="0 0 24 24"
              width="16"
              height="16"
              fill="none"
              stroke="currentColor"
              stroke-width="2"
              stroke-linecap="round"
              stroke-linejoin="round"
            >
              <circle cx="12" cy="12" r="5" />
              <line x1="12" y1="1" x2="12" y2="3" />
              <line x1="12" y1="21" x2="12" y2="23" />
              <line x1="4.22" y1="4.22" x2="5.64" y2="5.64" />
              <line x1="18.36" y1="18.36" x2="19.78" y2="19.78" />
              <line x1="1" y1="12" x2="3" y2="12" />
              <line x1="21" y1="12" x2="23" y2="12" />
              <line x1="4.22" y1="19.78" x2="5.64" y2="18.36" />
              <line x1="18.36" y1="5.64" x2="19.78" y2="4.22" />
            </svg>
          </button>

          <RouterLink to="/admin/profile" class="user-badge" title="个人信息与偏好设置">
            <span class="user-avatar">{{ authStore.nickname.charAt(0).toUpperCase() }}</span>
            <span class="user-name">{{ authStore.nickname }}</span>
          </RouterLink>
        </div>
      </header>

      <div class="content-view">
        <RouterView />
      </div>
    </div>
  </div>
</template>

<style scoped lang="scss">
.admin-shell {
  display: flex;
  min-height: 100vh;
  background: var(--fbz-color-bg);
  color: var(--fbz-color-text);
  font-family: var(--fbz-font-sans);
}

// ───── Sidebar ─────
.sidebar {
  width: 220px;
  height: 100vh;
  position: sticky;
  top: 0;
  flex-shrink: 0;
  display: flex;
  flex-direction: column;
  background: var(--fbz-color-panel);
  border-right: 1px solid var(--fbz-color-line-soft);
  z-index: 10;
}

.sidebar-brand {
  padding: 20px 16px;
  display: flex;
  flex-direction: column;
  gap: 4px;
  border-bottom: 1px solid var(--fbz-color-line-soft);

  .brand-logo {
    font-family: var(--fbz-font-display);
    font-weight: 800;
    font-size: 18px;
    letter-spacing: 3px;
    color: var(--fbz-color-text);

    b {
      color: var(--fbz-color-brand-500);
    }
  }

  .brand-sub {
    font-size: 10px;
    color: var(--fbz-color-text-muted);
    letter-spacing: 1.5px;
    text-transform: uppercase;
    font-weight: 600;
  }
}

.sidebar-nav {
  flex: 1;
  padding: var(--fbz-space-2) var(--fbz-space-2);
  display: flex;
  flex-direction: column;
  gap: 1px;
  overflow-y: auto;
}

// ───── Nav groups ─────
.nav-group {
  margin-top: var(--fbz-space-2);

  &:first-child {
    margin-top: 0;
  }
}

.group-label {
  padding: 6px 10px 4px;
  font-size: 10px;
  font-weight: 700;
  color: var(--fbz-color-text-muted);
  text-transform: uppercase;
  letter-spacing: 1px;
  user-select: none;
}

.menu-link {
  display: flex;
  align-items: center;
  gap: 9px;
  padding: 7px 10px;
  border-radius: var(--fbz-radius-control);
  color: var(--fbz-color-text-soft);
  text-decoration: none;
  font-size: 12px;
  font-weight: 600;
  transition:
    background var(--fbz-motion-fast),
    color var(--fbz-motion-fast);

  .menu-svg {
    flex-shrink: 0;
    opacity: 0.5;
    transition: opacity var(--fbz-motion-fast);
  }

  &:hover {
    background: var(--fbz-color-panel-strong);
    color: var(--fbz-color-text);

    .menu-svg {
      opacity: 0.8;
    }
  }

  &.active {
    background: color-mix(in srgb, var(--fbz-color-brand-500) 10%, var(--fbz-color-panel));
    color: var(--fbz-color-brand-500);

    .menu-svg {
      opacity: 1;
      color: var(--fbz-color-brand-500);
    }
  }
}

.sub-link {
  padding-left: 16px;
}

.menu-separator {
  height: 1px;
  background: var(--fbz-color-line-soft);
  margin: var(--fbz-space-2) var(--fbz-space-2);
}

.back-link {
  color: var(--fbz-color-text-muted);
  font-weight: 500;

  &:hover {
    color: var(--fbz-color-text);
  }
}

.sidebar-footer {
  padding: var(--fbz-space-2);
  border-top: 1px solid var(--fbz-color-line-soft);
  display: flex;
  flex-direction: column;
  gap: var(--fbz-space-2);
}

.theme-toggle-btn {
  width: 100%;
  height: 34px;
  border: 1px solid var(--fbz-color-line);
  background: var(--fbz-color-panel-strong);
  color: var(--fbz-color-text-soft);
  border-radius: var(--fbz-radius-control);
  font-size: 11px;
  font-weight: 600;
  display: flex;
  align-items: center;
  justify-content: center;
  gap: 6px;
  transition:
    border-color var(--fbz-motion-fast),
    color var(--fbz-motion-fast),
    background var(--fbz-motion-fast);

  svg {
    flex-shrink: 0;
    opacity: 0.7;
  }

  &:hover {
    border-color: var(--fbz-color-line-bright);
    color: var(--fbz-color-text);
    background: var(--fbz-color-panel-elevated);
  }
}

.footer-meta {
  display: flex;
  align-items: center;
  gap: 6px;
  padding: 0 4px;

  .status-dot {
    width: 6px;
    height: 6px;
    border-radius: 50%;
    background: var(--fbz-color-brand-500);
    box-shadow: 0 0 6px var(--fbz-color-brand-500);
  }

  .version-text {
    font-size: 10px;
    color: var(--fbz-color-text-muted);
    letter-spacing: 0.5px;
    font-family: var(--fbz-font-display);
  }
}

// ───── Main Container & Header ─────
.main-container {
  flex: 1;
  min-width: 0;
  display: flex;
  flex-direction: column;
}

.top-bar {
  height: 52px;
  padding: 0 var(--fbz-space-6);
  display: flex;
  align-items: center;
  justify-content: space-between;
  border-bottom: 1px solid var(--fbz-color-line-soft);
  background: var(--fbz-color-panel);
  position: sticky;
  top: 0;
  z-index: 9;
}

.hamburger-btn {
  display: none;
  background: none;
  border: 0;
  color: var(--fbz-color-text-soft);
  cursor: pointer;
  width: 36px;
  height: 36px;
  border-radius: var(--fbz-radius-control);
  place-content: center;
  transition:
    color var(--fbz-motion-fast),
    background var(--fbz-motion-fast);

  &:hover {
    color: var(--fbz-color-text);
    background: var(--fbz-color-panel-strong);
  }
}

.page-title {
  margin: 0;
  font-size: 14px;
  font-weight: 700;
  letter-spacing: -0.2px;
}

.top-actions {
  display: flex;
  align-items: center;
  gap: var(--fbz-space-3);
}

.header-theme-btn {
  background: var(--fbz-color-panel-strong);
  border: 1px solid var(--fbz-color-line);
  width: 32px;
  height: 32px;
  border-radius: var(--fbz-radius-control);
  display: grid;
  place-content: center;
  color: var(--fbz-color-text-soft);
  cursor: pointer;
  transition:
    border-color var(--fbz-motion-fast),
    color var(--fbz-motion-fast);

  &:hover {
    border-color: var(--fbz-color-line-bright);
    color: var(--fbz-color-text);
  }
}

.user-badge {
  display: flex;
  align-items: center;
  gap: 8px;
  padding: 3px 10px 3px 3px;
  background: var(--fbz-color-panel-strong);
  border: 1px solid var(--fbz-color-line);
  border-radius: var(--fbz-radius-round);

  .user-avatar {
    width: 24px;
    height: 24px;
    border-radius: 50%;
    background: var(--fbz-color-brand-500);
    color: #07120a;
    font-weight: 800;
    font-size: 10px;
    display: grid;
    place-content: center;
  }

  .user-name {
    font-size: 11px;
    font-weight: 600;
    color: var(--fbz-color-text-soft);
  }
}

.content-view {
  flex: 1;
  padding: var(--fbz-space-5);
  overflow-y: auto;

  :deep(.account-view) {
    padding: 0 0 80px 0;
  }
}

// ───── Mobile Layouts ─────
.mobile-overlay {
  position: fixed;
  inset: 0;
  z-index: 99;
  background: rgba(0, 0, 0, 0.6);
  backdrop-filter: blur(4px);
}

.mobile-sidebar {
  position: fixed;
  left: 0;
  top: 0;
  bottom: 0;
  width: 260px;
  z-index: 100;
  background: var(--fbz-color-panel);
  box-shadow: 10px 0 40px rgba(0, 0, 0, 0.3);
  display: flex;
  flex-direction: column;
}

// ───── Transitions ─────
.fade-enter-active,
.fade-leave-active {
  transition: opacity var(--fbz-motion-base) ease;
}
.fade-enter-from,
.fade-leave-to {
  opacity: 0;
}

.slide-enter-active,
.slide-leave-active {
  transition: transform var(--fbz-motion-base) cubic-bezier(0.16, 1, 0.3, 1);
}
.slide-enter-from,
.slide-leave-to {
  transform: translateX(-100%);
}

@media (max-width: 768px) {
  .sidebar {
    display: none;
  }

  .hamburger-btn {
    display: grid;
  }

  .top-bar {
    padding: 0 var(--fbz-space-4);
  }

  .content-view {
    padding: var(--fbz-space-4);
  }
}
</style>
