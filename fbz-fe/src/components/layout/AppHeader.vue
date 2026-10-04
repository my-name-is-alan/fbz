<script setup lang="ts">
import { useAuthStore } from "@/stores/auth.ts";
import { useThemeStore } from "@/stores/theme.ts";
const auth = useAuthStore();
const theme = useThemeStore();
const route = useRoute();
const router = useRouter();
const query = ref("");
const searching = ref(false);
const accountMenu = ref<HTMLDetailsElement>();
watch(
  () => route.fullPath,
  () => {
    if (accountMenu.value) accountMenu.value.open = false;
  },
);
useEventListener(document, "pointerdown", (event) => {
  if (accountMenu.value && !accountMenu.value.contains(event.target as Node))
    accountMenu.value.open = false;
});
defineEmits<{ openDrawer: [] }>();
const links = [
  { label: "发现", to: "/", icon: "home", type: "" },
  { label: "电影", to: "/library?type=Movie", icon: "movie", type: "Movie" },
  { label: "剧集", to: "/library?type=Series", icon: "tv", type: "Series" },
  { label: "媒体库", to: "/library", icon: "library", type: "all" },
  { label: "管理", to: "/admin", icon: "settings", type: "admin" },
];
function active(link: (typeof links)[number]) {
  if (link.type === "admin") return route.path.startsWith("/admin");
  return link.to === "/"
    ? route.path === "/"
    : route.path.startsWith("/library") &&
        (link.type === "all" ? !route.query.type : route.query.type === link.type);
}
function search() {
  searching.value = false;
  void router.push({ path: "/library", query: { search: query.value } });
}
async function logout() {
  await auth.logout().catch(() => {});
  await router.push("/user/login");
}
</script>
<template>
  <header class="site-header">
    <div class="header-inner">
      <RouterLink class="wordmark" to="/" aria-label="FBZ 首页">FBZ<span /></RouterLink>
      <nav class="desktop-navigation" aria-label="主导航">
        <RouterLink
          v-for="link in links.filter((l) => l.type !== 'admin' || auth.isAuthenticated)"
          :key="link.label"
          :to="link.to"
          :class="{ active: active(link) }"
          >{{ link.label }}</RouterLink
        >
      </nav>
      <div class="header-tools">
        <form class="header-search" :class="{ expanded: searching }" @submit.prevent="search">
          <BaseIcon name="search" :size="17" /><input
            v-model="query"
            class="du-input"
            placeholder="搜索电影、剧集…"
            aria-label="全站搜索"
          /><button
            class="du-btn du-btn-ghost du-btn-circle du-btn-sm search-close"
            aria-label="关闭搜索"
            type="button"
            @click="searching = false"
          >
            <BaseIcon name="close" :size="16" />
          </button>
        </form>
        <button
          class="du-btn du-btn-ghost du-btn-circle mobile-search"
          aria-label="打开搜索"
          @click="searching = !searching"
        >
          <BaseIcon name="search" /></button
        ><button
          class="du-btn du-btn-ghost du-btn-circle theme-action"
          aria-label="切换主题"
          @click="theme.setThemeMode(theme.themeMode === 'dark' ? 'light' : 'dark')"
        >
          <BaseIcon :name="theme.themeMode === 'dark' ? 'sun' : 'moon'" :size="18" />
        </button>
        <details
          ref="accountMenu"
          v-if="auth.isAuthenticated"
          class="du-dropdown du-dropdown-end account-dropdown"
        >
          <summary class="du-btn du-btn-ghost du-btn-circle user-avatar" aria-label="账户菜单">
            {{ auth.nickname.slice(0, 1).toUpperCase() }}
          </summary>
          <ul class="du-dropdown-content du-menu account-menu">
            <li class="menu-identity">{{ auth.nickname }}<small>个人媒体空间</small></li>
            <li>
              <RouterLink to="/admin"><BaseIcon name="settings" :size="17" />管理控制台</RouterLink>
            </li>
            <li>
              <RouterLink to="/admin/storage"
                ><BaseIcon name="cloud" :size="17" />云盘挂载</RouterLink
              >
            </li>
            <li>
              <RouterLink to="/admin/profile"
                ><BaseIcon name="user" :size="17" />账户设置</RouterLink
              >
            </li>
            <li>
              <button @click="logout"><BaseIcon name="logout" :size="17" />退出登录</button>
            </li>
          </ul>
        </details>
        <RouterLink v-else to="/user/login" class="du-btn du-btn-sm">登录</RouterLink>
      </div>
    </div>
  </header>
  <nav class="mobile-navigation" aria-label="移动端主导航">
    <RouterLink
      v-for="link in links.filter((l) => l.type !== 'admin')"
      :key="link.label"
      :to="link.to"
      :class="{ active: active(link) }"
      ><BaseIcon :name="link.icon" :size="21" /><span>{{ link.label }}</span></RouterLink
    >
  </nav>
</template>
<style scoped lang="scss">
.site-header {
  position: fixed;
  inset: 0 0 auto;
  z-index: 30;
  height: var(--header-h);
  background: color-mix(in srgb, var(--fbz-color-bg) 90%, transparent);
  border-bottom: 1px solid var(--fbz-color-line-soft);
  backdrop-filter: blur(18px);
}
.header-inner {
  height: 100%;
  max-width: 1560px;
  margin: auto;
  padding: 0 clamp(20px, 4.5vw, 72px);
  display: flex;
  align-items: center;
  gap: 55px;
}
.wordmark {
  font-family: var(--fbz-font-sans);
  font-weight: 800;
  letter-spacing: 4px;
  font-size: 23px;
  text-decoration: none;
  display: flex;
  align-items: center;
  gap: 3px;
}
.wordmark span {
  width: 5px;
  height: 5px;
  background: var(--fbz-color-brand-500);
  border-radius: 50%;
  margin-top: 13px;
}
.desktop-navigation {
  display: flex;
  align-items: center;
  gap: 32px;
  height: 100%;
}
.desktop-navigation a {
  font-size: 12px;
  letter-spacing: 1px;
  text-decoration: none;
  color: var(--fbz-color-text-muted);
  height: 100%;
  display: flex;
  align-items: center;
  border-bottom: 2px solid transparent;
  padding-top: 2px;
}
.desktop-navigation a:hover,
.desktop-navigation a.active {
  color: var(--fbz-color-text);
}
.desktop-navigation a.active {
  border-color: var(--fbz-color-brand-500);
}
.header-tools {
  display: flex;
  align-items: center;
  gap: 12px;
  margin-left: auto;
}
.header-search {
  position: relative;
  display: flex;
  align-items: center;
  color: var(--fbz-color-text-muted);
}
.header-search > svg {
  position: absolute;
  left: 13px;
  pointer-events: none;
}
.header-search input {
  font-size: 12px;
  padding-left: 39px;
  height: 36px;
  width: 230px;
  border-radius: 22px;
  background: var(--fbz-color-panel);
}
.header-tools > .du-btn {
  width: 35px;
  height: 35px;
  min-height: 0;
}
.user-avatar {
  background: var(--fbz-color-panel-elevated);
  border: 1px solid var(--fbz-color-line);
  width: 35px;
  height: 35px;
  min-height: 0;
  font-size: 12px;
}
.account-menu {
  width: 220px;
  margin-top: 16px;
  border: 1px solid var(--fbz-color-line);
  padding: 8px;
  background: var(--fbz-color-panel);
  border-radius: 13px;
}
.account-menu a,
.account-menu button {
  font-size: 12px;
  gap: 12px;
  padding: 11px 13px;
  border-radius: 7px;
}
.menu-identity {
  padding: 12px 13px 16px;
  display: flex;
  flex-direction: column;
  align-items: flex-start;
  gap: 4px;
  border-bottom: 1px solid var(--fbz-color-line-soft);
  margin-bottom: 6px;
  font-size: 13px;
}
.menu-identity small {
  font-size: 10px;
  color: var(--fbz-color-text-muted);
}
.mobile-navigation,
.mobile-search,
.search-close {
  display: none;
}
@media (max-width: 1000px) {
  .header-inner {
    gap: 30px;
  }
  .desktop-navigation {
    gap: 22px;
  }
  .header-search input {
    width: 180px;
  }
  .theme-action {
    display: none;
  }
}
@media (max-width: 650px) {
  .header-inner {
    padding-inline: 20px;
  }
  .desktop-navigation {
    display: none;
  }
  .mobile-search {
    display: inline-flex;
  }
  .header-search {
    display: none;
  }
  .header-search.expanded {
    display: flex;
    position: fixed;
    inset: 0 12px;
    background: var(--fbz-color-bg);
    z-index: 3;
  }
  .header-search.expanded input {
    width: 100%;
    padding-right: 40px;
  }
  .search-close {
    display: inline-flex;
    position: absolute;
    right: 4px;
  }
  .wordmark {
    font-size: 21px;
  }
  .mobile-navigation {
    position: fixed;
    inset: auto 0 0;
    z-index: 40;
    display: flex;
    justify-content: space-around;
    padding: 10px 10px calc(10px + env(safe-area-inset-bottom));
    background: color-mix(in srgb, var(--fbz-color-bg) 94%, transparent);
    backdrop-filter: blur(18px);
    border-top: 1px solid var(--fbz-color-line);
  }
  .mobile-navigation a {
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: 5px;
    padding: 1px 17px;
    text-decoration: none;
    color: var(--fbz-color-text-muted);
    font-size: 9px;
  }
  .mobile-navigation a.active {
    color: var(--fbz-color-brand-500);
  }
}
.account-dropdown summary {
  list-style: none;
}
.account-dropdown summary::marker {
  content: "";
}
.account-dropdown summary::-webkit-details-marker {
  display: none;
}
.header-search > svg {
  z-index: 1;
}
</style>
