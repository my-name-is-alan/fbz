# Layouts

Two route layouts: `default` (front-end media shell) and `admin` (console sidebar + top bar).

## Default Layout

- Path: `src/layouts/default.vue`
- Description: Front-end app shell: AppHeader + AppDrawer + RouterView + MusicPlayerBar.

```vue
<script setup lang="ts">
const drawerOpen = ref(false);
</script>

<template>
  <div class="app-shell">
    <AppHeader @open-drawer="drawerOpen = true" />
    <AppDrawer v-model="drawerOpen" />

    <RouterView />
    <MusicPlayerBar />
  </div>
</template>

<style scoped lang="scss">
.app-shell {
  min-height: 100vh;
}
</style>
```

## Admin Layout

- Path: `src/layouts/admin.vue`
- Description: Admin console: sticky 220px sidebar (grouped nav + plugin menus), mobile drawer, 52px top bar with page title / theme / avatar.

```vue
<script setup lang="ts">
import { useThemeStore } from "@/stores/theme.ts";
import { useAuthStore } from "@/stores/auth.ts";
import { listPluginMenuItems } from "@/service/modules/admin.ts";
import type { PluginMenuItem } from "@/types/admin.ts";

const route = useRoute();
const themeStore = useThemeStore();
const authStore = useAuthStore();

const mobileMenuOpen = ref(false);

// 已启用插件声明的后台菜单项（admin.menu 权限），渲染为独立导航分组。
const pluginMenuItems = ref<PluginMenuItem[]>([]);

onMounted(async () => {
  try {
    pluginMenuItems.value = await listPluginMenuItems();
  } catch {
    // 菜单加载失败不阻塞后台使用（如无管理员权限或后端不可用）。
    pluginMenuItems.value = [];
  }
});

/** 插件菜单顶层项（parentKey 为空），按插件名和 weight 稳定排序。 */
const pluginNavItems = computed(() =>
  pluginMenuItems.value
    .filter((item) => item.parentKey === null)
    .sort(
      (a, b) =>
        a.pluginName.localeCompare(b.pluginName) ||
        a.weight - b.weight ||
        a.itemKey.localeCompare(b.itemKey),
    ),
);

function pluginChildItems(parent: PluginMenuItem): PluginMenuItem[] {
  return pluginMenuItems.value
    .filter((item) => item.pluginId === parent.pluginId && item.parentKey === parent.itemKey)
    .sort((a, b) => a.weight - b.weight || a.itemKey.localeCompare(b.itemKey));
}

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
  schedule:
    "M8 2v4 M16 2v4 M3 10h18 M5 4h14a2 2 0 0 1 2 2v14a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2V6a2 2 0 0 1 2-2z M12 14v4 M10 16h4",
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
        label: "插件市场",
        to: "/admin/plugin-market",
        name: "admin-plugin-market",
        icon: ICONS.plugin,
      },
      {
        label: "任务计划",
        to: "/admin/scheduled-tasks",
        name: "admin-scheduled-tasks",
        icon: ICONS.schedule,
      },
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
  if (route.name === "admin-plugin-config") return "插件配置";
  const pluginItem = pluginMenuItems.value.find((item) => item.path === route.path);
  if (pluginItem) return `${pluginItem.pluginName} · ${pluginItem.label}`;
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

        <!-- 已启用插件声明的管理菜单 -->
        <div v-if="pluginNavItems.length > 0" class="nav-group" role="group" aria-label="插件菜单">
          <div class="group-label">插件菜单</div>
          <template v-for="item in pluginNavItems" :key="item.itemKey">
            <RouterLink
              :to="item.path"
              class="menu-link sub-link"
              :class="{ active: route.path === item.path }"
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
                <path :d="ICONS.plugin" />
              </svg>
              <span class="menu-label">{{ item.label }}</span>
            </RouterLink>
            <RouterLink
              v-for="child in pluginChildItems(item)"
              :key="child.itemKey"
              :to="child.path"
              class="menu-link sub-link plugin-child-link"
              :class="{ active: route.path === child.path }"
            >
              <span class="menu-label">{{ child.label }}</span>
            </RouterLink>
          </template>
        </div>

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

          <!-- 已启用插件声明的管理菜单 -->
          <div
            v-if="pluginNavItems.length > 0"
            class="nav-group"
            role="group"
            aria-label="插件菜单"
          >
            <div class="group-label">插件菜单</div>
            <template v-for="item in pluginNavItems" :key="item.itemKey">
              <RouterLink
                :to="item.path"
                class="menu-link sub-link"
                :class="{ active: route.path === item.path }"
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
                  <path :d="ICONS.plugin" />
                </svg>
                <span class="menu-label">{{ item.label }}</span>
              </RouterLink>
              <RouterLink
                v-for="child in pluginChildItems(item)"
                :key="child.itemKey"
                :to="child.path"
                class="menu-link sub-link plugin-child-link"
                :class="{ active: route.path === child.path }"
                @click="closeMobileMenu"
              >
                <span class="menu-label">{{ child.label }}</span>
              </RouterLink>
            </template>
          </div>

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
            <BaseAvatar
              :user-id="authStore.userId"
              :name="authStore.nickname"
              :version="authStore.avatarVersion"
              :size="24"
            />
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

.plugin-child-link {
  padding-left: 40px;
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
```

## AppHeader

- Path: `src/components/layout/AppHeader.vue`
- Description: Fixed front-end top nav: brand, library dropdown, search, account menu; frosted on scroll / transparent over hero.

```vue
<script setup lang="ts">
import { useLibraryStore } from "@/stores/library.ts";
import { useAuthStore } from "@/stores/auth.ts";

const emit = defineEmits<{ openDrawer: [] }>();

const libraryStore = useLibraryStore();
const authStore = useAuthStore();
const { libraries } = storeToRefs(libraryStore);
const accountMenuOpen = ref(false);
const router = useRouter();

// 桌面端顶部内联搜索：回车跳 /search?q=；空关键词跳 /search 引导态。
const searchTerm = ref("");
function submitSearch() {
  const q = searchTerm.value.trim();
  void router.push({ name: "search", query: q ? { q } : {} });
}
// 手机端搜索图标：直接进搜索页（输入框在页内）。
function goSearch() {
  void router.push({ name: "search" });
}

const accountLinks = [
  { label: "后台管理", to: "/admin" },
  { label: "个人信息", to: "/admin/profile" },
  { label: "消息中心", to: "/admin/logs" },
] as const;

// 滚动 → header 镂空磨砂
const scrolled = ref(false);
const route = useRoute();
const hasHeroBackdrop = computed(() => {
  const name = String(route.name ?? "");
  const path = route.path;
  return (
    ["home", "movie-detail", "tv-detail", "collection-detail"].includes(name) ||
    path === "/" ||
    path.startsWith("/movie/") ||
    path.startsWith("/tv/") ||
    path.startsWith("/collection/")
  );
});
function onScroll() {
  scrolled.value = window.scrollY > 50;
}
onMounted(() => {
  onScroll();
  window.addEventListener("scroll", onScroll, { passive: true });
});
onBeforeUnmount(() => window.removeEventListener("scroll", onScroll));

useEventListener(window, "click", (event) => {
  const target = event.target as HTMLElement | null;
  if (!target?.closest(".account-menu")) accountMenuOpen.value = false;
});

const fmt = new Intl.NumberFormat("en-US");

function closeAccountMenu() {
  accountMenuOpen.value = false;
}

function logout() {
  accountMenuOpen.value = false;
}

useEventListener(window, "keydown", (e) => {
  if (e.key === "Escape" && accountMenuOpen.value) {
    closeAccountMenu();
  }
});
</script>

<template>
  <header
    class="site-header"
    :class="{ 'is-scrolled': scrolled, 'is-hero-transparent': hasHeroBackdrop && !scrolled }"
  >
    <button class="hamburger" aria-label="打开侧边导航" @click="emit('openDrawer')">☰</button>

    <RouterLink to="/" class="brand">F<b>B</b>Z</RouterLink>

    <nav class="nav" aria-label="主导航">
      <RouterLink to="/" class="nav-item" active-class="active" exact-active-class="active">
        首页
      </RouterLink>

      <div class="lib-wrap">
        <RouterLink to="/library" class="nav-item has-caret" active-class="active"
          >媒体库</RouterLink
        >
        <div class="lib-menu">
          <RouterLink to="/library" class="lib-link strong">
            <span class="lib-ic">▦</span> 媒体库总览
          </RouterLink>
          <div class="lib-group-label">全部库</div>
          <RouterLink
            v-for="lib in libraries"
            :key="lib.id"
            :to="`/library/${lib.id}`"
            class="lib-link"
          >
            <span class="lib-ic">{{ lib.name.charAt(0) }}</span>
            {{ lib.name }}
            <span class="lib-count">{{ fmt.format(lib.count) }}</span>
          </RouterLink>
        </div>
      </div>

      <RouterLink to="/" class="nav-item">最近添加</RouterLink>
    </nav>

    <form class="header-search" role="search" @submit.prevent="submitSearch">
      <span class="search-icon" aria-hidden="true" />
      <input
        v-model="searchTerm"
        class="header-search-input"
        type="search"
        name="q"
        placeholder="搜索影片、人物..."
        autocomplete="off"
        aria-label="搜索"
      />
    </form>
    <button class="icon-search-btn" aria-label="搜索" @click="goSearch">⌕</button>
    <div class="account-menu">
      <button
        class="avatar"
        type="button"
        aria-label="打开用户菜单"
        :aria-expanded="accountMenuOpen"
        @click.stop="accountMenuOpen = !accountMenuOpen"
      >
        <BaseAvatar
          :user-id="authStore.userId"
          :name="authStore.nickname"
          :version="authStore.avatarVersion"
          :size="34"
        />
      </button>

      <div v-if="accountMenuOpen" class="account-dropdown" role="menu" aria-label="用户账户菜单">
        <RouterLink
          v-for="link in accountLinks"
          :key="link.to"
          class="account-item"
          role="menuitem"
          :to="link.to"
          @click="closeAccountMenu"
        >
          {{ link.label }}
        </RouterLink>
        <button class="account-item danger" type="button" role="menuitem" @click="logout">
          退出登录
        </button>
      </div>
    </div>
  </header>
</template>

<style scoped lang="scss">
.site-header {
  position: fixed;
  inset: 0 0 auto 0;
  height: var(--header-h, 60px);
  z-index: var(--fbz-z-overlay);
  display: flex;
  align-items: center;
  gap: var(--fbz-space-6);
  padding: 0 var(--fbz-space-8);
  background: transparent;
  border-bottom: 1px solid transparent;
  backdrop-filter: blur(0px);
  transition:
    background var(--fbz-motion-slow) ease,
    border-color var(--fbz-motion-slow) ease,
    backdrop-filter var(--fbz-motion-slow) ease;

  &.is-scrolled {
    background: color-mix(in srgb, var(--fbz-color-bg) 72%, transparent);
    border-bottom: 1px solid var(--fbz-color-line);
    backdrop-filter: saturate(140%) blur(14px);
    -webkit-backdrop-filter: saturate(140%) blur(14px);
  }
}

.brand {
  font-family: var(--fbz-font-display);
  font-weight: 800;
  font-size: 18px;
  letter-spacing: 2px;
  text-decoration: none;
  color: var(--fbz-color-text);

  b {
    color: var(--fbz-color-brand-500);
  }
}

.nav {
  display: flex;
  align-items: center;
  gap: 2px;
  margin-right: auto;
}

.nav-item {
  position: relative;
  display: inline-flex;
  align-items: center;
  color: var(--fbz-color-text-soft);
  background: none;
  border: 0;
  font-size: var(--fbz-font-size-md);
  text-decoration: none;
  padding: 8px 12px;
  border-radius: var(--fbz-radius-control);
  transition:
    color var(--fbz-motion-fast),
    background var(--fbz-motion-fast);

  &:hover {
    color: var(--fbz-color-text);
    background: var(--fbz-color-panel-strong);
  }

  &.active {
    color: var(--fbz-color-text);

    &::after {
      content: "";
      position: absolute;
      left: 12px;
      right: 12px;
      bottom: 2px;
      height: 2px;
      background: var(--fbz-color-brand-500);
      border-radius: 2px;
    }
  }

  &.has-caret::before {
    content: "▾";
    font-size: 10px;
    margin-right: 6px;
    opacity: 0.6;
    display: inline-flex;
    align-items: center;
    line-height: 1;
  }
}

.lib-wrap {
  position: relative;
  display: inline-flex;
  align-items: center;
}

.lib-menu {
  position: absolute;
  top: calc(100% + 8px);
  left: 0;
  width: 320px;
  background: color-mix(in srgb, var(--fbz-color-panel) 97%, transparent);
  border: 1px solid var(--fbz-color-line);
  border-radius: 8px;
  padding: 8px;
  backdrop-filter: blur(14px);
  -webkit-backdrop-filter: blur(14px);
  box-shadow: var(--fbz-shadow-panel);
  opacity: 0;
  visibility: hidden;
  transform: translateY(-6px);
  transition:
    opacity var(--fbz-motion-base),
    transform var(--fbz-motion-base),
    visibility var(--fbz-motion-base);

  .lib-wrap:hover &,
  .lib-wrap:focus-within & {
    opacity: 1;
    visibility: visible;
    transform: none;
  }
}

.lib-group-label {
  padding: 8px 10px 4px;
  font-size: var(--fbz-font-size-xs);
  color: var(--fbz-color-text-muted);
  text-transform: uppercase;
  letter-spacing: 1px;
}

.lib-link {
  display: flex;
  align-items: center;
  gap: 10px;
  padding: 8px 10px;
  border-radius: 6px;
  color: var(--fbz-color-text-soft);
  font-size: var(--fbz-font-size-md);
  text-decoration: none;

  &:hover {
    background: var(--fbz-color-panel-strong);
    color: var(--fbz-color-text);
  }

  &.strong {
    color: var(--fbz-color-text);
    font-weight: 600;
  }
}

.lib-ic {
  width: 22px;
  height: 22px;
  flex: 0 0 auto;
  display: grid;
  place-content: center;
  border: 1px solid var(--fbz-color-line);
  border-radius: 5px;
  font-size: var(--fbz-font-size-sm);
  color: var(--fbz-color-text-muted);
}

.lib-count {
  margin-left: auto;
  font-size: var(--fbz-font-size-sm);
  color: var(--fbz-color-text-muted);
}

.header-search {
  display: flex;
  align-items: center;
  gap: 8px;
  height: 36px;
  padding: 0 12px;
  border-radius: var(--fbz-radius-control);
  border: 1px solid var(--fbz-color-line);
  background: var(--fbz-color-panel-strong);
  -webkit-backdrop-filter: blur(8px);
  backdrop-filter: blur(8px);
  color: var(--fbz-color-text-muted);
  font-size: var(--fbz-font-size-sm);
  min-width: 200px;
  transition: border-color var(--fbz-motion-fast);

  &:focus-within {
    border-color: var(--fbz-color-brand-500);
  }

  .search-icon {
    flex: 0 0 auto;
    width: 13px;
    height: 13px;
    border-radius: 50%;
    border: 1.5px solid currentColor;
    opacity: 0.7;
  }
}

.header-search-input {
  flex: 1 1 auto;
  min-width: 0;
  height: 100%;
  border: 0;
  background: none;
  outline: none;
  color: var(--fbz-color-text);
  font-size: var(--fbz-font-size-sm);

  &::placeholder {
    color: var(--fbz-color-text-muted);
  }

  &::-webkit-search-cancel-button {
    appearance: none;
  }
}

.account-menu {
  position: relative;
  flex: 0 0 auto;
}

.avatar {
  width: 34px;
  height: 34px;
  flex: 0 0 auto;
  padding: 0;
  border-radius: 50%;
  background: none;
  border: 1px solid var(--fbz-color-line);
  display: grid;
  place-content: center;
  overflow: hidden;
  cursor: pointer;
  transition: border-color var(--fbz-motion-fast);

  &:hover,
  &[aria-expanded="true"] {
    border-color: var(--fbz-color-line-bright);
  }
}

.account-dropdown {
  position: absolute;
  top: calc(100% + 8px);
  right: 0;
  width: 180px;
  padding: 6px;
  border: 1px solid var(--fbz-color-line);
  border-radius: 8px;
  background: color-mix(in srgb, var(--fbz-color-panel) 98%, transparent);
  box-shadow: var(--fbz-shadow-panel);
  backdrop-filter: blur(14px);
  -webkit-backdrop-filter: blur(14px);
  -webkit-backdrop-filter: blur(14px);
}

.account-item {
  width: 100%;
  min-height: 34px;
  display: flex;
  align-items: center;
  padding: 0 10px;
  border: 0;
  border-radius: 6px;
  background: transparent;
  color: var(--fbz-color-text-soft);
  font-size: var(--fbz-font-size-md);
  text-align: left;
  text-decoration: none;

  &:hover {
    background: var(--fbz-color-panel-strong);
    color: var(--fbz-color-text);
  }

  &.danger {
    color: #fca5a5;
  }
}

.hamburger,
.icon-search-btn {
  display: none;
  background: none;
  border: 0;
  color: var(--fbz-color-text);
  width: 36px;
  height: 36px;
  font-size: 20px;
}

.icon-search-btn {
  font-size: 18px;
}

@media (max-width: 600px) {
  .site-header {
    gap: var(--fbz-space-3);
    padding: 0 var(--fbz-space-4);
  }

  .nav,
  .header-search,
  .account-menu {
    display: none;
  }

  .hamburger,
  .icon-search-btn {
    display: grid;
    place-content: center;
  }

  .brand {
    margin-right: auto;
  }
}

.site-header.is-hero-transparent {
  .brand {
    color: #ffffff;
  }

  .nav-item {
    color: rgba(255, 255, 255, 0.7);

    &:hover {
      color: #ffffff;
      background: rgba(255, 255, 255, 0.08);
    }

    &.active {
      color: #ffffff;
    }
  }

  .header-search {
    background: rgba(255, 255, 255, 0.08);
    border-color: rgba(255, 255, 255, 0.15);
    color: rgba(255, 255, 255, 0.5);
  }

  .avatar {
    background: rgba(255, 255, 255, 0.08);
    border-color: rgba(255, 255, 255, 0.15);
    color: rgba(255, 255, 255, 0.75);

    &:hover {
      border-color: rgba(255, 255, 255, 0.3);
      color: #ffffff;
    }
  }

  .hamburger,
  .icon-search-btn {
    color: #ffffff;
  }
}
</style>
```

## AppDrawer

- Path: `src/components/layout/AppDrawer.vue`
- Description: Mobile left drawer for home/library navigation; closes on route change / Escape.

```vue
<script setup lang="ts">
import { useLibraryStore } from "@/stores/library.ts";
import { libraryRoute } from "@/utils/libraryRoute.ts";

const open = defineModel<boolean>({ default: false });

const libraryStore = useLibraryStore();
const { libraries } = storeToRefs(libraryStore);

function close() {
  open.value = false;
}

// 路由变化时自动收起
const route = useRoute();
watch(() => route.fullPath, close);

useEventListener(window, "keydown", (e) => {
  if (e.key === "Escape" && open.value) {
    close();
  }
});
</script>

<template>
  <Teleport to="body">
    <div class="drawer" :class="{ open }">
      <div class="mask" @click="close" />
      <nav class="panel" role="dialog" aria-modal="true" aria-label="移动端导航菜单">
        <RouterLink to="/" class="link" @click="close">首页</RouterLink>
        <RouterLink to="/" class="link" @click="close">最近添加</RouterLink>
        <RouterLink to="/library" class="link" @click="close">媒体库总览</RouterLink>

        <div class="sec">全部库</div>
        <RouterLink
          v-for="lib in libraries"
          :key="lib.id"
          :to="libraryRoute(lib)"
          class="link"
          @click="close"
        >
          {{ lib.name }}
        </RouterLink>
      </nav>
    </div>
  </Teleport>
</template>

<style scoped lang="scss">
.drawer {
  position: fixed;
  inset: 0;
  z-index: 60;
  visibility: hidden;
  opacity: 0;
  transition:
    opacity var(--fbz-motion-base),
    visibility var(--fbz-motion-base);

  &.open {
    visibility: visible;
    opacity: 1;
  }
}

.mask {
  position: absolute;
  inset: 0;
  background: rgba(0, 0, 0, 0.6);
}

.panel {
  position: absolute;
  top: 0;
  left: 0;
  bottom: 0;
  width: 76%;
  max-width: 320px;
  padding: 20px 16px;
  background: var(--fbz-color-bg-strong);
  border-right: 1px solid var(--fbz-color-line);
  transform: translateX(-100%);
  transition: transform var(--fbz-motion-base);
  overflow-y: auto;

  .drawer.open & {
    transform: none;
  }
}

.link {
  display: block;
  padding: 12px 10px;
  color: var(--fbz-color-text-soft);
  text-decoration: none;
  font-size: 15px;
  border-radius: 6px;

  &.router-link-active {
    color: var(--fbz-color-brand-500);
    font-weight: 700;
  }

  &:active {
    background: var(--fbz-color-panel-strong);
  }
}

.sec {
  padding: 14px 10px 6px;
  font-size: var(--fbz-font-size-xs);
  color: var(--fbz-color-text-muted);
  text-transform: uppercase;
  letter-spacing: 1px;
}
</style>
```

## AdminPageShell

- Path: `src/components/admin/AdminPageShell.vue`
- Description: Per-page content shell used by all admin views (title banner + slot).

```vue
<script setup lang="ts">
/**
 * 后台页面统一外壳：标题横幅（标题 + 描述）+ 内容插槽。
 * 每个 `views/admin/*` 页面用它包裹自己的主体组件，保持横幅样式一致；
 * 自带标题区的页面（如插件菜单页）可省略 title 只用容器。
 */
interface Props {
  title?: string;
  description?: string;
}

defineProps<Props>();
</script>

<template>
  <main class="admin-page">
    <div v-if="title" class="page-banner">
      <h1 class="page-heading">{{ title }}</h1>
      <p v-if="description" class="description-text">{{ description }}</p>
    </div>
    <slot />
  </main>
</template>

<style scoped lang="scss">
.admin-page {
  padding: 0 0 80px;
  max-width: 1200px;
  margin: 0 auto;
}

.page-banner {
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
</style>
```

## MusicPlayerBar

- Path: `src/components/playback/MusicPlayerBar.vue`
- Description: Bottom music player bar mounted in default layout.

```vue
<script setup lang="ts">
import { storeToRefs } from "pinia";
import { useMusicPlayerStore } from "@/stores/musicPlayer.ts";
import { formatDuration } from "@/service/modules/music.ts";
import {
  newPlaySessionId,
  reportPlaybackProgress,
  reportPlaybackStart,
  reportPlaybackStopped,
} from "@/service/modules/playbackReport.ts";

/**
 * 底部音乐播放栏 —— 全局挂载，仅在有当前曲目时显示。
 * 用原生 `<audio>` 播放后端直出流（RANGE 支持，可拖动进度）；与视频全屏播放器并存互不干扰。
 * 同时把播放进度上报后端（开始/进度/停止），让曲目进入「继续观看」。
 */
const player = useMusicPlayerStore();
const { current, streamUrl, hasPrevious, hasNext } = storeToRefs(player);

const audioRef = ref<HTMLAudioElement>();
const isPlaying = ref(false);
const currentTime = ref(0);
const duration = ref(0);

/** 当前播放会话：曲目 id + 客户端会话标识，贯穿一次播放的开始→进度→停止。 */
let reportedItemId: string | undefined;
let playSessionId = "";
/** 进度上报节流：最近一次上报的秒数（每 ~10s 上报一次，避免刷屏）。 */
let lastProgressReportedAt = 0;
const PROGRESS_REPORT_INTERVAL_SECONDS = 10;

/** 给上一首（reportedItemId）补一条 stopped（落最终位置），best-effort 不阻断 UI。 */
function reportStoppedForCurrent(positionSeconds: number) {
  if (!reportedItemId) return;
  const itemId = reportedItemId;
  const session = playSessionId;
  reportedItemId = undefined;
  void reportPlaybackStopped({ itemId, playSessionId: session, positionSeconds }).catch(() => {});
}

/** 切歌时（streamUrl 变化）：旧曲 stopped、新曲加载播放 + start。 */
watch(streamUrl, async (url) => {
  // 旧曲先收尾（用切换前的播放位置）。
  reportStoppedForCurrent(currentTime.value);

  if (!url) return;
  await nextTick();
  const el = audioRef.value;
  if (!el) return;
  el.src = url;
  currentTime.value = 0;
  duration.value = 0;

  // 新曲会话：记录 id + 新会话标识，上报 start（best-effort）。
  reportedItemId = current.value?.id;
  playSessionId = newPlaySessionId();
  lastProgressReportedAt = 0;
  if (reportedItemId) {
    void reportPlaybackStart({
      itemId: reportedItemId,
      playSessionId,
      positionSeconds: 0,
    }).catch(() => {});
  }

  try {
    await el.play();
  } catch {
    // 自动播放被浏览器拦截（无用户手势）：保持暂停态，用户可手动点播放。
    isPlaying.value = false;
  }
});

/** 周期上报播放进度（节流到 ~10s 一次）。 */
function onTimeUpdate() {
  const el = audioRef.value;
  if (!el) return;
  currentTime.value = el.currentTime;
  if (!reportedItemId) return;
  if (currentTime.value - lastProgressReportedAt < PROGRESS_REPORT_INTERVAL_SECONDS) return;
  lastProgressReportedAt = currentTime.value;
  void reportPlaybackProgress({
    itemId: reportedItemId,
    playSessionId,
    positionSeconds: currentTime.value,
    isPaused: el.paused,
  }).catch(() => {});
}

/** 暂停/恢复也上报一次进度（携带 isPaused），让后端状态及时。 */
function reportPauseState(isPaused: boolean) {
  if (!reportedItemId) return;
  void reportPlaybackProgress({
    itemId: reportedItemId,
    playSessionId,
    positionSeconds: currentTime.value,
    isPaused,
  }).catch(() => {});
}

function togglePlay() {
  const el = audioRef.value;
  if (!el) return;
  if (el.paused) void el.play();
  else el.pause();
}

/** 拖动进度条 seek。 */
function onSeek(event: Event) {
  const el = audioRef.value;
  const target = event.target as HTMLInputElement;
  if (!el) return;
  el.currentTime = Number(target.value);
}

/** 播完自动下一首；末尾停下并上报 stopped。 */
function onEnded() {
  if (hasNext.value) {
    player.playNext();
  } else {
    reportStoppedForCurrent(currentTime.value);
    isPlaying.value = false;
  }
}

/** 关闭播放栏：先收尾上报，再清空队列。 */
function closeBar() {
  reportStoppedForCurrent(currentTime.value);
  player.close();
}

// 组件卸载（如登出跳转）也收尾上报，避免悬挂的播放会话。
onBeforeUnmount(() => reportStoppedForCurrent(currentTime.value));

const progress = computed(() => (duration.value > 0 ? currentTime.value / duration.value : 0));
</script>

<template>
  <Teleport to="body">
    <Transition name="music-bar-slide">
      <div v-if="current" class="music-bar" role="region" aria-label="音乐播放栏">
        <audio
          ref="audioRef"
          @play="
            isPlaying = true;
            reportPauseState(false);
          "
          @pause="
            isPlaying = false;
            reportPauseState(true);
          "
          @timeupdate="onTimeUpdate"
          @loadedmetadata="duration = audioRef?.duration ?? 0"
          @ended="onEnded"
        />

        <div class="now-playing">
          <MediaPoster class="cover" :src="current.poster" :title="current.title" />
          <div class="meta">
            <span class="title">{{ current.title }}</span>
            <span v-if="current.subtitle" class="subtitle">{{ current.subtitle }}</span>
          </div>
        </div>

        <div class="controls">
          <button
            class="ctl"
            :disabled="!hasPrevious"
            aria-label="上一首"
            @click="player.playPrevious"
          >
            ⏮
          </button>
          <button class="ctl play" :aria-label="isPlaying ? '暂停' : '播放'" @click="togglePlay">
            {{ isPlaying ? "⏸" : "▶" }}
          </button>
          <button class="ctl" :disabled="!hasNext" aria-label="下一首" @click="player.playNext">
            ⏭
          </button>
        </div>

        <div class="progress">
          <span class="time">{{ formatDuration(Math.floor(currentTime)) || "0:00" }}</span>
          <input
            class="bar"
            type="range"
            min="0"
            :max="duration || 0"
            :value="currentTime"
            :style="{ '--progress': `${progress * 100}%` }"
            aria-label="播放进度"
            @input="onSeek"
          />
          <span class="time">{{ formatDuration(Math.floor(duration)) || "0:00" }}</span>
        </div>

        <button class="ctl close" aria-label="关闭播放栏" @click="closeBar">✕</button>
      </div>
    </Transition>
  </Teleport>
</template>

<style scoped lang="scss">
.music-bar {
  position: fixed;
  inset: auto 0 0 0;
  z-index: 60;
  display: grid;
  grid-template-columns: minmax(160px, 1fr) auto minmax(200px, 2fr) auto;
  align-items: center;
  gap: var(--fbz-space-5);
  padding: var(--fbz-space-3) var(--fbz-space-6);
  background: var(--fbz-color-panel-strong);
  border-top: 1px solid var(--fbz-color-line-soft);
}

.now-playing {
  display: flex;
  align-items: center;
  gap: var(--fbz-space-3);
  min-width: 0;

  .cover {
    width: 44px;
    flex: none;
  }

  .meta {
    display: flex;
    flex-direction: column;
    min-width: 0;

    .title {
      font-size: var(--fbz-font-size-sm);
      font-weight: 600;
      white-space: nowrap;
      overflow: hidden;
      text-overflow: ellipsis;
    }

    .subtitle {
      font-size: var(--fbz-font-size-xs);
      color: var(--fbz-color-text-muted);
      white-space: nowrap;
      overflow: hidden;
      text-overflow: ellipsis;
    }
  }
}

.controls {
  display: flex;
  align-items: center;
  gap: var(--fbz-space-2);
}

.ctl {
  display: grid;
  place-content: center;
  width: 36px;
  height: 36px;
  border: none;
  border-radius: 50%;
  background: transparent;
  color: var(--fbz-color-text);
  font-size: var(--fbz-font-size-md);
  cursor: pointer;
  transition:
    background var(--fbz-motion-fast),
    color var(--fbz-motion-fast);

  &:hover:not(:disabled) {
    background: var(--fbz-color-panel);
  }

  &:disabled {
    color: var(--fbz-color-text-muted);
    cursor: default;
    opacity: 0.4;
  }

  &.play {
    background: var(--fbz-color-brand-500);
    color: #0a0a0b;

    &:hover {
      background: var(--fbz-color-brand-500);
      filter: brightness(1.1);
    }
  }
}

.progress {
  display: flex;
  align-items: center;
  gap: var(--fbz-space-3);

  .time {
    font-size: var(--fbz-font-size-xs);
    color: var(--fbz-color-text-muted);
    font-variant-numeric: tabular-nums;
    min-width: 4ch;
    text-align: center;
  }

  .bar {
    flex: 1;
    appearance: none;
    height: 4px;
    border-radius: 2px;
    background: linear-gradient(
      to right,
      var(--fbz-color-brand-500) var(--progress, 0%),
      var(--fbz-color-line-soft) var(--progress, 0%)
    );
    cursor: pointer;

    &::-webkit-slider-thumb {
      appearance: none;
      width: 12px;
      height: 12px;
      border-radius: 50%;
      background: var(--fbz-color-brand-500);
    }

    &::-moz-range-thumb {
      width: 12px;
      height: 12px;
      border: none;
      border-radius: 50%;
      background: var(--fbz-color-brand-500);
    }
  }
}

.music-bar-slide-enter-active,
.music-bar-slide-leave-active {
  transition: transform var(--fbz-motion-base);
}

.music-bar-slide-enter-from,
.music-bar-slide-leave-to {
  transform: translateY(100%);
}

@media (max-width: 600px) {
  .music-bar {
    grid-template-columns: 1fr auto;
    grid-template-rows: auto auto;
    gap: var(--fbz-space-3);
    padding: var(--fbz-space-3) var(--fbz-space-4);
  }

  .progress {
    grid-column: 1 / -1;
    order: 3;
  }

  .close {
    display: none;
  }
}
</style>
```
