<script setup lang="ts">
import { useLibraryStore } from "@/stores/library.ts";
import { useUiStore } from "@/stores/ui.ts";
import { libraryRoute } from "@/utils/libraryRoute.ts";
import type { MediaLibrary } from "@/types/media.ts";

const libraryStore = useLibraryStore();
const uiStore = useUiStore();
const { libraries, totalCount, loading, error } = storeToRefs(libraryStore);

const fmt = new Intl.NumberFormat("en-US");

onMounted(() => {
  if (!libraryStore.loaded) void libraryStore.loadFromBackend();
});

function onLibraryContextMenu(e: MouseEvent, lib: MediaLibrary) {
  uiStore.openLibraryContextMenu(e.clientX, e.clientY, { id: lib.id, name: lib.name });
}
</script>

<template>
  <main class="library-overview">
    <header class="page-head">
      <h1>媒体库</h1>
      <p class="sub">{{ libraries.length }} 个库 · 共 {{ fmt.format(totalCount) }} 个条目</p>
    </header>

    <p v-if="loading" class="state-text">正在从服务器加载媒体库...</p>
    <p v-else-if="error" class="state-text is-error">{{ error }}</p>
    <BaseEmptyState
      v-else-if="!libraries.length"
      icon="📚"
      title="还没有创建媒体库"
      description="先创建一个媒体库，选择服务器上后端可访问的目录，保存后系统会自动扫描并预提取元数据。"
    >
      <button class="empty-cta" type="button" @click="uiStore.openLibraryEditor(null)">
        新建媒体库
      </button>
      <RouterLink class="empty-cta secondary" to="/admin/libraries">媒体库管理</RouterLink>
    </BaseEmptyState>

    <div v-else class="grid">
      <RouterLink
        v-for="(lib, i) in libraries"
        :key="lib.id"
        :to="libraryRoute(lib)"
        class="lib-card"
        @contextmenu.prevent="onLibraryContextMenu($event, lib)"
      >
        <div class="cover" :class="{ alt: i % 2 === 1 }">
          <span class="cover-name">{{ lib.name }}</span>
        </div>
        <div class="info">
          <span class="name">{{ lib.name }}</span>
          <span class="count">{{ fmt.format(lib.count) }}</span>
        </div>
      </RouterLink>
    </div>
  </main>
</template>

<style scoped lang="scss">
.library-overview {
  padding: calc(var(--header-h, 60px) + var(--fbz-space-8)) var(--fbz-space-8) 80px;
  min-height: 100vh;
}

.page-head {
  margin-bottom: var(--fbz-space-6);

  h1 {
    margin: 0 0 var(--fbz-space-2);
    font-size: var(--fbz-font-size-xl);
    font-weight: 800;
  }

  .sub {
    margin: 0;
    color: var(--fbz-color-text-muted);
    font-size: var(--fbz-font-size-md);
  }
}

.grid {
  display: grid;
  grid-template-columns: repeat(auto-fill, minmax(220px, 1fr));
  gap: var(--fbz-space-5);
}

.state-text {
  margin: var(--fbz-space-8) 0 0;
  color: var(--fbz-color-text-muted);
  font-size: var(--fbz-font-size-md);

  &.is-error {
    color: var(--fbz-color-danger-500);
  }
}

.empty-cta {
  height: 38px;
  padding: 0 18px;
  border-radius: var(--fbz-radius-control);
  border: 1px solid transparent;
  background: var(--fbz-color-brand-500);
  color: #04160b;
  font-size: var(--fbz-font-size-md);
  font-weight: 700;
  text-decoration: none;
  display: inline-flex;
  align-items: center;
  cursor: pointer;
  transition:
    background var(--fbz-motion-fast),
    border-color var(--fbz-motion-fast);

  &:hover {
    background: color-mix(in srgb, var(--fbz-color-brand-500) 88%, white);
  }

  &.secondary {
    background: transparent;
    border-color: var(--fbz-color-line);
    color: var(--fbz-color-text-soft);

    &:hover {
      border-color: var(--fbz-color-line-bright);
      color: var(--fbz-color-text);
    }
  }
}

.lib-card {
  text-decoration: none;
  color: inherit;
}

.cover {
  aspect-ratio: 16 / 9;
  border-radius: var(--fbz-radius-card);
  border: 1px solid var(--fbz-color-line-soft);
  background: var(--fbz-color-panel);
  display: grid;
  place-content: center;
  transition:
    border-color var(--fbz-motion-fast),
    transform var(--fbz-motion-fast);

  &.alt {
    background: var(--fbz-color-panel-strong);
  }

  .lib-card:hover & {
    border-color: var(--fbz-color-brand-500);
    transform: translateY(-3px);
  }
}

.cover-name {
  font-size: var(--fbz-font-size-lg);
  font-weight: 700;
  color: var(--fbz-color-text-muted);
}

.info {
  display: flex;
  align-items: baseline;
  justify-content: space-between;
  margin-top: var(--fbz-space-3);

  .name {
    font-size: var(--fbz-font-size-md);
    font-weight: 600;
  }

  .count {
    font-size: var(--fbz-font-size-sm);
    color: var(--fbz-color-text-muted);
  }
}

@media (max-width: 600px) {
  .library-overview {
    padding: calc(var(--header-h, 56px) + var(--fbz-space-5)) var(--fbz-space-4) 60px;
  }

  .grid {
    grid-template-columns: repeat(auto-fill, minmax(150px, 1fr));
    gap: var(--fbz-space-4);
  }
}
</style>
