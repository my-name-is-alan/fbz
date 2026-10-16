<script setup lang="ts">
import { useLibraryStore } from "@/stores/library.ts";
import { useAuthStore } from "@/stores/auth.ts";
import { searchHints, type SearchResultItem } from "@/service/modules/search.ts";
import { libraryRoute } from "@/utils/libraryRoute.ts";
import { libraryTypeMeta } from "@/utils/libraryType.ts";

const emit = defineEmits<{ openDrawer: [] }>();

const libraryStore = useLibraryStore();
const authStore = useAuthStore();
const { libraries } = storeToRefs(libraryStore);
const router = useRouter();
const route = useRoute();

const fmt = new Intl.NumberFormat("zh-CN");

/* ---------- 顶栏内联搜索：回车跳 /search?q=；输入防抖出联想；空关键词进搜索页引导态 ---------- */
const searchTerm = ref("");
const suggestions = ref<SearchResultItem[]>([]);
const suggestOpen = ref(false);
const suggestLoading = ref(false);
let suggestController: AbortController | undefined;

const SUGGEST_ICONS: Record<string, string> = {
  movie: "movie",
  tv: "tv",
  person: "user-round",
  album: "album",
  artist: "artist",
};

const loadSuggestions = debounce(async (term: string) => {
  const trimmed = term.trim();
  if (!trimmed) {
    suggestions.value = [];
    suggestOpen.value = false;
    return;
  }
  suggestController?.abort();
  const controller = new AbortController();
  suggestController = controller;
  suggestLoading.value = true;
  suggestOpen.value = true;
  try {
    const items = await searchHints(trimmed, {
      userId: authStore.userId,
      limit: 8,
      signal: controller.signal,
    });
    if (controller.signal.aborted) return;
    suggestions.value = items;
  } catch (error) {
    if ((error as { code?: string })?.code === "ERR_CANCELED") return;
    suggestions.value = [];
  } finally {
    if (!controller.signal.aborted) suggestLoading.value = false;
  }
}, 220);

function onSearchInput() {
  void loadSuggestions(searchTerm.value);
}

function submitSearch() {
  loadSuggestions.cancel();
  suggestOpen.value = false;
  const q = searchTerm.value.trim();
  void router.push({ name: "search", query: q ? { q } : {} });
}

function pickSuggestion(item: SearchResultItem) {
  suggestOpen.value = false;
  searchTerm.value = item.title;
  void router.push(item.to);
}

/* ---------- 媒体库下拉 ---------- */
const libMenuOpen = ref(false);
const libraryActive = computed(() =>
  ["/library", "/music/", "/books/"].some((p) => route.path.startsWith(p)),
);

/* ---------- 账户菜单 ---------- */
const accountMenuOpen = ref(false);

function logout() {
  accountMenuOpen.value = false;
  authStore.logout();
  void router.push({ name: "login" });
}

watch(
  () => route.fullPath,
  () => {
    libMenuOpen.value = false;
    accountMenuOpen.value = false;
    suggestOpen.value = false;
  },
);

useEventListener(window, "click", (event) => {
  const target = event.target as HTMLElement | null;
  if (!target?.closest(".account-menu")) accountMenuOpen.value = false;
  if (!target?.closest(".header-search")) suggestOpen.value = false;
  if (!target?.closest(".lib-wrap")) libMenuOpen.value = false;
});

useEventListener(window, "keydown", (e) => {
  if (e.key !== "Escape") return;
  accountMenuOpen.value = false;
  suggestOpen.value = false;
  libMenuOpen.value = false;
});
</script>

<template>
  <header class="site-header">
    <div class="header-inner">
      <button
        class="icon-btn hamburger"
        type="button"
        aria-label="打开导航"
        @click="emit('openDrawer')"
      >
        <BaseIcon name="menu" :size="20" />
      </button>

      <RouterLink to="/" class="brand" aria-label="FBZ 首页">FBZ</RouterLink>

      <nav class="nav" aria-label="主导航">
        <RouterLink to="/" class="nav-item" exact-active-class="active">
          <BaseIcon name="home" :size="16" />首页
        </RouterLink>

        <div class="lib-wrap" :class="{ open: libMenuOpen }">
          <button
            type="button"
            class="nav-item"
            :class="{ active: libraryActive }"
            aria-haspopup="menu"
            :aria-expanded="libMenuOpen"
            @click.stop="libMenuOpen = !libMenuOpen"
          >
            <BaseIcon name="library" :size="16" />媒体库
            <BaseIcon name="down" :size="14" class="caret" />
          </button>
          <div class="lib-menu" role="menu" aria-label="媒体库">
            <RouterLink to="/library" class="lib-link overview" role="menuitem">
              <span class="lib-ic"><BaseIcon name="grid" :size="16" /></span>
              <span class="lib-name">媒体库总览</span>
              <span class="lib-count">{{ libraries.length }} 个库</span>
            </RouterLink>
            <div v-if="libraries.length" class="lib-sep" />
            <RouterLink
              v-for="lib in libraries"
              :key="lib.id"
              :to="libraryRoute(lib)"
              class="lib-link"
              role="menuitem"
            >
              <span class="lib-ic"
                ><BaseIcon :name="libraryTypeMeta(lib.libraryType).icon" :size="16"
              /></span>
              <span class="lib-name">
                {{ lib.name }}
                <small>{{ libraryTypeMeta(lib.libraryType).label }}</small>
              </span>
              <span class="lib-count">{{ fmt.format(lib.count) }}</span>
            </RouterLink>
          </div>
        </div>

        <RouterLink to="/playlists" class="nav-item" active-class="active">
          <BaseIcon name="playlist" :size="16" />播放列表
        </RouterLink>
        <RouterLink to="/collections" class="nav-item" active-class="active">
          <BaseIcon name="collection" :size="16" />合集
        </RouterLink>
      </nav>

      <form class="header-search" role="search" @submit.prevent="submitSearch">
        <BaseIcon name="search" :size="15" class="search-icon" />
        <input
          v-model="searchTerm"
          class="header-search-input"
          type="search"
          name="q"
          placeholder="搜索影片、剧集、人物…"
          autocomplete="off"
          aria-label="搜索"
          aria-autocomplete="list"
          :aria-expanded="suggestOpen"
          @input="onSearchInput"
          @focus="onSearchInput"
        />
        <div v-if="suggestOpen" class="search-suggest" role="listbox" aria-label="搜索建议">
          <p v-if="suggestLoading" class="suggest-hint">
            <BaseIcon name="loading" :size="14" class="c-spin" />搜索中…
          </p>
          <p v-else-if="!suggestions.length" class="suggest-hint">没有联想结果，回车查看全部</p>
          <button
            v-for="item in suggestions"
            :key="`${item.kind}-${item.id}`"
            type="button"
            class="suggest-item"
            role="option"
            @mousedown.prevent="pickSuggestion(item)"
          >
            <span class="suggest-thumb">
              <img v-if="item.poster" :src="item.poster" alt="" />
              <BaseIcon v-else :name="SUGGEST_ICONS[item.kind] ?? 'file'" :size="16" />
            </span>
            <span class="suggest-text">
              <strong>{{ item.title }}</strong>
              <small>{{ item.meta }}</small>
            </span>
            <BaseIcon :name="SUGGEST_ICONS[item.kind] ?? 'file'" :size="14" class="suggest-kind" />
          </button>
          <button
            v-if="searchTerm.trim()"
            type="button"
            class="suggest-more"
            @mousedown.prevent="submitSearch"
          >
            <BaseIcon name="search" :size="14" />查看「{{ searchTerm.trim() }}」的全部结果
          </button>
        </div>
      </form>

      <RouterLink to="/search" class="icon-btn mobile-search" aria-label="搜索">
        <BaseIcon name="search" :size="20" />
      </RouterLink>

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
            :size="32"
          />
        </button>

        <div v-if="accountMenuOpen" class="account-dropdown" role="menu" aria-label="用户菜单">
          <div class="account-head">
            <strong>{{ authStore.nickname || authStore.username || "用户" }}</strong>
            <small>{{ authStore.isAdmin ? "管理员" : "普通用户" }}</small>
          </div>
          <RouterLink v-if="authStore.isAdmin" class="account-item" role="menuitem" to="/admin">
            <BaseIcon name="admin" :size="16" />后台管理
          </RouterLink>
          <RouterLink class="account-item" role="menuitem" to="/admin/profile">
            <BaseIcon name="user" :size="16" />个人信息
          </RouterLink>
          <RouterLink
            v-if="authStore.isAdmin"
            class="account-item"
            role="menuitem"
            to="/admin/logs"
          >
            <BaseIcon name="bell" :size="16" />消息中心
          </RouterLink>
          <div class="account-sep" />
          <button class="account-item danger" type="button" role="menuitem" @click="logout">
            <BaseIcon name="logout" :size="16" />退出登录
          </button>
        </div>
      </div>
    </div>
  </header>
</template>

<style scoped lang="scss">
.site-header {
  position: sticky;
  top: 0;
  z-index: 40;
  height: var(--ui-header-height);
  border-bottom: 1px solid var(--ui-border);
  background: var(--ui-glass);
  backdrop-filter: blur(16px) saturate(140%);
  -webkit-backdrop-filter: blur(16px) saturate(140%);
}

.header-inner {
  height: 100%;
  max-width: var(--ui-shell-width);
  margin: 0 auto;
  padding: 0 var(--ui-shell-gutter);
  display: flex;
  align-items: center;
  gap: 24px;
}

.brand {
  font-size: 20px;
  font-weight: 700;
  letter-spacing: -0.02em;
  color: var(--ui-text);
  text-decoration: none;
}

.nav {
  display: flex;
  align-items: center;
  gap: 4px;
  margin-right: auto;
}

.nav-item {
  position: relative;
  display: inline-flex;
  align-items: center;
  gap: 6px;
  height: 34px;
  padding: 0 10px;
  border: 0;
  border-radius: var(--ui-radius-sm);
  background: transparent;
  color: var(--ui-text-muted);
  font-size: 13.5px;
  font-weight: 500;
  text-decoration: none;
  cursor: pointer;

  &:hover {
    color: var(--ui-text);
    background: var(--ui-surface-hover);
  }

  &.active {
    color: var(--ui-accent);
  }

  .caret {
    opacity: 0.7;
    transition: transform var(--ui-duration-fast);
  }
}

.lib-wrap {
  position: relative;

  &.open .caret {
    transform: rotate(180deg);
  }
}

.lib-menu {
  position: absolute;
  top: calc(100% + 8px);
  left: 0;
  width: 300px;
  max-height: min(70vh, 520px);
  overflow-y: auto;
  padding: 6px;
  border: 1px solid var(--ui-border);
  border-radius: var(--ui-radius-md);
  background: var(--ui-glass-strong);
  backdrop-filter: blur(16px);
  box-shadow: var(--ui-shadow-float);
  opacity: 0;
  visibility: hidden;
  transform: translateY(-4px);
  transition:
    opacity var(--ui-duration-fast),
    transform var(--ui-duration-fast),
    visibility var(--ui-duration-fast);

  .lib-wrap.open & {
    opacity: 1;
    visibility: visible;
    transform: none;
  }
}

.lib-sep,
.account-sep {
  height: 1px;
  margin: 4px 6px;
  background: var(--ui-border);
}

.lib-link {
  display: flex;
  align-items: center;
  gap: 10px;
  padding: 7px 8px;
  border-radius: var(--ui-radius-sm);
  color: var(--ui-text);
  text-decoration: none;
  font-size: 13px;

  &:hover {
    background: var(--ui-surface-hover);
  }

  &.router-link-active:not(.overview) {
    color: var(--ui-accent);

    .lib-ic {
      color: var(--ui-accent);
      border-color: rgb(var(--ui-accent-channel) / 40%);
    }
  }
}

.lib-ic {
  width: 28px;
  height: 28px;
  flex: 0 0 auto;
  display: grid;
  place-items: center;
  border: 1px solid var(--ui-border);
  border-radius: var(--ui-radius-sm);
  background: var(--ui-surface);
  color: var(--ui-text-muted);
}

.lib-name {
  flex: 1;
  min-width: 0;
  display: flex;
  flex-direction: column;
  line-height: 1.3;
  overflow: hidden;
  white-space: nowrap;
  text-overflow: ellipsis;

  small {
    font-size: 11px;
    color: var(--ui-text-subtle);
  }
}

.lib-count {
  font-size: 12px;
  color: var(--ui-text-subtle);
  font-variant-numeric: tabular-nums;
}

.header-search {
  position: relative;
  display: flex;
  align-items: center;
  width: 280px;
  height: 34px;
  padding: 0 10px;
  gap: 8px;
  border: 1px solid var(--ui-border);
  border-radius: var(--ui-radius-sm);
  background: var(--ui-input);
  color: var(--ui-text-subtle);
  transition:
    border-color var(--ui-duration-fast),
    box-shadow var(--ui-duration-fast);

  &:focus-within {
    border-color: rgb(var(--ui-accent-channel) / 60%);
    box-shadow: 0 0 0 3px rgb(var(--ui-accent-channel) / 12%);
  }
}

.header-search-input {
  flex: 1;
  min-width: 0;
  height: 100%;
  border: 0;
  outline: none;
  background: transparent;
  color: var(--ui-text);
  font-size: 13px;

  &::placeholder {
    color: var(--ui-text-subtle);
  }

  &::-webkit-search-cancel-button {
    appearance: none;
  }
}

.search-suggest {
  position: absolute;
  top: calc(100% + 6px);
  left: 0;
  right: 0;
  z-index: 50;
  max-height: min(440px, 64vh);
  overflow-y: auto;
  padding: 6px;
  border: 1px solid var(--ui-border);
  border-radius: var(--ui-radius-md);
  background: var(--ui-glass-strong);
  backdrop-filter: blur(16px);
  box-shadow: var(--ui-shadow-float);
}

.suggest-hint {
  display: flex;
  align-items: center;
  gap: 6px;
  margin: 0;
  padding: 10px;
  color: var(--ui-text-muted);
  font-size: 12px;
}

.suggest-item,
.suggest-more {
  width: 100%;
  display: flex;
  align-items: center;
  gap: 10px;
  padding: 6px 8px;
  border: 0;
  border-radius: var(--ui-radius-sm);
  background: transparent;
  color: var(--ui-text);
  text-align: left;
  cursor: pointer;

  &:hover {
    background: var(--ui-surface-hover);
  }
}

.suggest-thumb {
  width: 30px;
  height: 44px;
  flex: 0 0 auto;
  display: grid;
  place-items: center;
  overflow: hidden;
  border-radius: 5px;
  background: var(--ui-surface-raised);
  color: var(--ui-text-subtle);

  img {
    width: 100%;
    height: 100%;
    object-fit: cover;
  }
}

.suggest-text {
  flex: 1;
  min-width: 0;
  display: flex;
  flex-direction: column;
  gap: 2px;

  strong {
    font-size: 13px;
    font-weight: 600;
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }

  small {
    font-size: 11.5px;
    color: var(--ui-text-muted);
  }
}

.suggest-kind {
  color: var(--ui-text-subtle);
}

.suggest-more {
  margin-top: 4px;
  border-top: 1px solid var(--ui-border);
  border-radius: 0 0 var(--ui-radius-sm) var(--ui-radius-sm);
  color: var(--ui-accent);
  font-size: 12.5px;
  font-weight: 600;
}

.icon-btn {
  display: none;
  width: 36px;
  height: 36px;
  place-items: center;
  border: 0;
  border-radius: var(--ui-radius-sm);
  background: transparent;
  color: var(--ui-text);
  cursor: pointer;

  &:hover {
    background: var(--ui-surface-hover);
  }
}

.account-menu {
  position: relative;
  flex: 0 0 auto;
}

.avatar {
  width: 34px;
  height: 34px;
  padding: 0;
  display: grid;
  place-items: center;
  overflow: hidden;
  border: 1px solid var(--ui-border-strong);
  border-radius: 50%;
  background: var(--ui-surface-raised);
  cursor: pointer;

  &:hover,
  &[aria-expanded="true"] {
    border-color: rgb(var(--ui-accent-channel) / 70%);
  }
}

.account-dropdown {
  position: absolute;
  top: calc(100% + 8px);
  right: 0;
  width: 200px;
  padding: 6px;
  border: 1px solid var(--ui-border);
  border-radius: var(--ui-radius-md);
  background: var(--ui-glass-strong);
  backdrop-filter: blur(16px);
  box-shadow: var(--ui-shadow-float);
}

.account-head {
  display: flex;
  flex-direction: column;
  padding: 6px 8px 8px;

  strong {
    font-size: 13px;
    color: var(--ui-text);
  }

  small {
    font-size: 11.5px;
    color: var(--ui-text-subtle);
  }
}

.account-item {
  width: 100%;
  min-height: 34px;
  display: flex;
  align-items: center;
  gap: 10px;
  padding: 0 8px;
  border: 0;
  border-radius: var(--ui-radius-sm);
  background: transparent;
  color: var(--ui-text-muted);
  font-size: 13px;
  text-align: left;
  text-decoration: none;
  cursor: pointer;

  &:hover {
    color: var(--ui-text);
    background: var(--ui-surface-hover);
  }

  &.danger:hover {
    color: var(--ui-danger);
  }
}

@media (max-width: 1100px) {
  .header-search {
    width: 200px;
  }

  .nav-item {
    padding: 0 8px;
  }
}

@media (max-width: 860px) {
  .header-inner {
    gap: 8px;
  }

  .nav,
  .header-search,
  .account-menu {
    display: none;
  }

  .icon-btn {
    display: grid;
  }

  .brand {
    margin-right: auto;
  }
}
</style>
