<script setup lang="ts">
import { useLibraryStore } from "@/stores/library.ts";

const emit = defineEmits<{ openDrawer: [] }>();

const libraryStore = useLibraryStore();
const { libraries } = storeToRefs(libraryStore);
const accountMenuOpen = ref(false);

const accountLinks = [
  { label: "后台管理", to: "/admin" },
  { label: "个人信息", to: "/profile" },
  { label: "消息中心", to: "/messages" },
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
</script>

<template>
  <header
    class="site-header"
    :class="{ 'is-scrolled': scrolled, 'is-hero-transparent': hasHeroBackdrop && !scrolled }"
  >
    <button class="hamburger" aria-label="菜单" @click="emit('openDrawer')">☰</button>

    <RouterLink to="/" class="brand">F<b>B</b>Z</RouterLink>

    <nav class="nav">
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

    <button class="header-search" type="button">搜索影片、人物...</button>
    <button class="icon-search-btn" aria-label="搜索">⌕</button>
    <div class="account-menu">
      <button
        class="avatar"
        type="button"
        aria-label="打开用户菜单"
        :aria-expanded="accountMenuOpen"
        @click.stop="accountMenuOpen = !accountMenuOpen"
      >
        A
      </button>

      <div v-if="accountMenuOpen" class="account-dropdown">
        <RouterLink
          v-for="link in accountLinks"
          :key="link.to"
          class="account-item"
          :to="link.to"
          @click="closeAccountMenu"
        >
          {{ link.label }}
        </RouterLink>
        <button class="account-item danger" type="button" @click="logout">退出登录</button>
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

  .lib-wrap:hover & {
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

  &::before {
    content: "";
    width: 13px;
    height: 13px;
    border-radius: 50%;
    border: 1.5px solid currentColor;
    opacity: 0.7;
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
  border-radius: 50%;
  background: var(--fbz-color-panel-strong);
  border: 1px solid var(--fbz-color-line);
  display: grid;
  place-content: center;
  font-size: var(--fbz-font-size-sm);
  font-weight: 700;
  color: var(--fbz-color-text-soft);
  cursor: pointer;
  transition:
    border-color var(--fbz-motion-fast),
    color var(--fbz-motion-fast);

  &:hover,
  &[aria-expanded="true"] {
    border-color: var(--fbz-color-line-bright);
    color: var(--fbz-color-text);
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
