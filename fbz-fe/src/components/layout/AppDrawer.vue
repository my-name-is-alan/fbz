<script setup lang="ts">
import { useLibraryStore } from "@/stores/library.ts";
import { useAuthStore } from "@/stores/auth.ts";
import { libraryRoute } from "@/utils/libraryRoute.ts";
import { libraryTypeMeta } from "@/utils/libraryType.ts";

const open = defineModel<boolean>({ default: false });

const libraryStore = useLibraryStore();
const authStore = useAuthStore();
const { libraries } = storeToRefs(libraryStore);
const router = useRouter();

const fmt = new Intl.NumberFormat("zh-CN");

function close() {
  open.value = false;
}

function logout() {
  close();
  authStore.logout();
  void router.push({ name: "login" });
}

// 路由变化时自动收起
const route = useRoute();
watch(() => route.fullPath, close);

useEventListener(window, "keydown", (e) => {
  if (e.key === "Escape" && open.value) close();
});
</script>

<template>
  <Teleport to="body">
    <div class="drawer" :class="{ open }">
      <div class="mask" @click="close" />
      <nav class="panel" role="dialog" aria-modal="true" aria-label="导航菜单">
        <header class="panel-head">
          <span class="brand">FBZ</span>
          <button class="close" type="button" aria-label="关闭导航" @click="close">
            <BaseIcon name="close" :size="18" />
          </button>
        </header>

        <RouterLink to="/" class="link" exact-active-class="active">
          <BaseIcon name="home" :size="17" />首页
        </RouterLink>
        <RouterLink to="/search" class="link" active-class="active">
          <BaseIcon name="search" :size="17" />搜索
        </RouterLink>
        <RouterLink to="/library" class="link" exact-active-class="active">
          <BaseIcon name="grid" :size="17" />媒体库总览
        </RouterLink>
        <RouterLink to="/playlists" class="link" active-class="active">
          <BaseIcon name="playlist" :size="17" />播放列表
        </RouterLink>
        <RouterLink to="/collections" class="link" active-class="active">
          <BaseIcon name="collection" :size="17" />合集
        </RouterLink>

        <div v-if="libraries.length" class="sec">媒体库</div>
        <RouterLink
          v-for="lib in libraries"
          :key="lib.id"
          :to="libraryRoute(lib)"
          class="link"
          active-class="active"
        >
          <BaseIcon :name="libraryTypeMeta(lib.libraryType).icon" :size="17" />
          <span class="name">{{ lib.name }}</span>
          <span class="count">{{ fmt.format(lib.count) }}</span>
        </RouterLink>

        <div class="sec">账户</div>
        <RouterLink v-if="authStore.isAdmin" to="/admin" class="link">
          <BaseIcon name="admin" :size="17" />后台管理
        </RouterLink>
        <RouterLink to="/admin/profile" class="link">
          <BaseIcon name="user" :size="17" />个人信息
        </RouterLink>
        <button type="button" class="link danger" @click="logout">
          <BaseIcon name="logout" :size="17" />退出登录
        </button>
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
    opacity var(--ui-duration-base),
    visibility var(--ui-duration-base);

  &.open {
    visibility: visible;
    opacity: 1;
  }
}

.mask {
  position: absolute;
  inset: 0;
  background: var(--ui-glass-backdrop);
}

.panel {
  position: absolute;
  top: 0;
  left: 0;
  bottom: 0;
  width: min(300px, 82vw);
  display: flex;
  flex-direction: column;
  gap: 2px;
  padding: 12px;
  overflow-y: auto;
  background: var(--ui-surface);
  border-right: 1px solid var(--ui-border);
  box-shadow: var(--ui-shadow-float);
  transform: translateX(-100%);
  transition: transform var(--ui-duration-base) var(--ui-ease-out);

  .drawer.open & {
    transform: none;
  }
}

.panel-head {
  display: flex;
  align-items: center;
  justify-content: space-between;
  padding: 4px 4px 12px;

  .brand {
    font-size: 18px;
    font-weight: 700;
    color: var(--ui-text);
  }
}

.close {
  width: 32px;
  height: 32px;
  display: grid;
  place-items: center;
  border: 0;
  border-radius: var(--ui-radius-sm);
  background: transparent;
  color: var(--ui-text-muted);

  &:hover {
    background: var(--ui-surface-hover);
    color: var(--ui-text);
  }
}

.link {
  display: flex;
  align-items: center;
  gap: 10px;
  width: 100%;
  min-height: 40px;
  padding: 0 10px;
  border: 0;
  border-radius: var(--ui-radius-sm);
  background: transparent;
  color: var(--ui-text-muted);
  font-size: 14px;
  text-align: left;
  text-decoration: none;

  &:hover,
  &:active {
    background: var(--ui-surface-hover);
    color: var(--ui-text);
  }

  &.active {
    color: var(--ui-accent);
    background: rgb(var(--ui-accent-channel) / 10%);
  }

  &.danger {
    color: var(--ui-danger);
  }

  .name {
    flex: 1;
    min-width: 0;
    overflow: hidden;
    white-space: nowrap;
    text-overflow: ellipsis;
  }

  .count {
    font-size: 12px;
    color: var(--ui-text-subtle);
  }
}

.sec {
  padding: 16px 10px 6px;
  font-size: 11px;
  font-weight: 600;
  letter-spacing: 0.06em;
  color: var(--ui-text-subtle);
}
</style>
