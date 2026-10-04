<script setup lang="ts">
import { fetchHomeData } from "@/service/modules/navigation.ts";
import { useLibraryStore } from "@/stores/library.ts";
import { useUiStore } from "@/stores/ui.ts";
import type { FeaturedItem } from "@/types/media.ts";
import type { HomeRow } from "@/types/navigation.ts";

const libraryStore = useLibraryStore();
const uiStore = useUiStore();

const featured = ref<FeaturedItem[]>([]);
const rows = ref<HomeRow[]>([]);
const loaded = ref(false);
const error = ref<string | null>(null);

const hasLibraries = computed(() => libraryStore.libraries.length > 0);
const hasAnyLibraryItems = computed(() => libraryStore.totalCount > 0);
const emptyStateTitle = computed(() =>
  hasLibraries.value ? "媒体库还没有可展示内容" : "还没有创建媒体库",
);
const emptyStateDescription = computed(() =>
  hasLibraries.value
    ? "当前媒体库存在，但首页还没有扫描出的媒体条目。可以进入媒体库管理检查路径并重新触发扫描。"
    : "先创建一个媒体库，选择 Rust 后端可访问的服务器目录，然后让后端扫描真实媒体文件。",
);

onMounted(async () => {
  try {
    const [data] = await Promise.all([
      fetchHomeData(),
      libraryStore.loaded ? Promise.resolve(true) : libraryStore.loadFromBackend(),
    ]);
    rows.value = data.rows;
    featured.value = data.featured;
    loaded.value = true;
  } catch {
    error.value = "首页数据加载失败，请检查网络、登录状态或服务器运行状态。";
    loaded.value = true;
  }
});
</script>

<template>
  <main class="home-view">
    <HomeHero v-if="featured.length" :items="featured" />

    <div class="content">
      <p v-if="!loaded" class="state-text">正在加载首页数据...</p>
      <p v-else-if="error" class="state-text is-error">{{ error }}</p>
      <section v-else-if="!rows.length || !hasAnyLibraryItems" class="empty-library-panel">
        <div class="empty-copy">
          <span class="eyebrow">Media Library</span>
          <h1>{{ emptyStateTitle }}</h1>
          <p>{{ emptyStateDescription }}</p>
        </div>
        <div class="empty-actions">
          <button class="primary-action" type="button" @click="uiStore.openLibraryEditor(null)">
            新建媒体库
          </button>
          <RouterLink class="secondary-action" to="/admin/libraries">媒体库管理</RouterLink>
        </div>
      </section>
      <MediaRow
        v-for="row in rows"
        :key="row.key"
        :title="row.title"
        :items="row.items"
        :layout="row.layout"
        :size="row.layout === 'wide' ? 'xlarge' : 'normal'"
        :to="row.to"
        :show-resolution="row.layout !== 'wide'"
        :show-rating="row.layout !== 'wide'"
      />
    </div>
  </main>
</template>

<style scoped lang="scss">
.content {
  position: relative;
  z-index: 3;
  padding: var(--fbz-space-6) var(--fbz-space-8) 80px;
}

.state-text {
  color: var(--fbz-color-text-muted);
  font-size: var(--fbz-font-size-md);

  &.is-error {
    color: var(--fbz-color-danger-500);
  }
}

.empty-library-panel {
  min-height: 320px;
  border: 1px solid var(--fbz-color-line-soft);
  border-radius: 8px;
  background:
    linear-gradient(135deg, rgba(45, 212, 191, 0.08), transparent 42%), var(--fbz-color-panel);
  display: flex;
  justify-content: space-between;
  align-items: center;
  gap: var(--fbz-space-8);
  padding: var(--fbz-space-8);
}

.empty-copy {
  max-width: 640px;

  .eyebrow {
    display: inline-block;
    margin-bottom: var(--fbz-space-3);
    color: var(--fbz-color-brand-500);
    font-size: var(--fbz-font-size-xs);
    font-weight: 800;
    letter-spacing: 0;
    text-transform: uppercase;
  }

  h1 {
    margin: 0 0 var(--fbz-space-3);
    color: var(--fbz-color-text);
    font-size: 28px;
    line-height: 1.2;
  }

  p {
    margin: 0;
    color: var(--fbz-color-text-muted);
    font-size: var(--fbz-font-size-md);
    line-height: 1.7;
  }
}

.empty-actions {
  display: flex;
  gap: var(--fbz-space-3);
  flex-shrink: 0;
}

.primary-action,
.secondary-action {
  height: 40px;
  border-radius: var(--fbz-radius-control);
  padding: 0 var(--fbz-space-5);
  display: inline-flex;
  align-items: center;
  justify-content: center;
  font-size: var(--fbz-font-size-sm);
  font-weight: 800;
  text-decoration: none;
  cursor: pointer;
}

.primary-action {
  border: 0;
  background: var(--fbz-color-brand-500);
  color: #07120a;

  &:hover {
    background: var(--fbz-color-brand-600);
  }
}

.secondary-action {
  border: 1px solid var(--fbz-color-line);
  background: var(--fbz-color-panel-strong);
  color: var(--fbz-color-text-soft);

  &:hover {
    background: var(--fbz-color-panel-elevated);
    color: var(--fbz-color-text);
  }
}

@media (max-width: 600px) {
  .content {
    padding: var(--fbz-space-5) var(--fbz-space-4) 60px;
  }

  .empty-library-panel {
    align-items: stretch;
    flex-direction: column;
    padding: var(--fbz-space-5);
  }

  .empty-actions {
    flex-direction: column;
  }
}
</style>
