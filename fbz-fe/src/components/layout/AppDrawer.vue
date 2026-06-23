<script setup lang="ts">
import { useLibraryStore } from "@/stores/library.ts";

const open = defineModel<boolean>({ default: false });

const libraryStore = useLibraryStore();
const { libraries } = storeToRefs(libraryStore);

function close() {
  open.value = false;
}

// 路由变化时自动收起
const route = useRoute();
watch(() => route.fullPath, close);
</script>

<template>
  <Teleport to="body">
    <div class="drawer" :class="{ open }">
      <div class="mask" @click="close" />
      <nav class="panel">
        <RouterLink to="/" class="link" @click="close">首页</RouterLink>
        <RouterLink to="/" class="link" @click="close">最近添加</RouterLink>
        <RouterLink to="/library" class="link" @click="close">媒体库总览</RouterLink>

        <div class="sec">全部库</div>
        <RouterLink
          v-for="lib in libraries"
          :key="lib.id"
          :to="`/library/${lib.id}`"
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
