<script setup lang="ts">
import type { CollectionDetail, MediaItem } from "@/types/media.ts";
import { loadCollectionDetail } from "@/service/modules/detail.ts";

const route = useRoute();
const id = computed(() => String(route.params.id ?? ""));
const collection = ref<CollectionDetail>();
const parts = ref<MediaItem[]>([]);

watch(
  id,
  async (v) => {
    const detail = await loadCollectionDetail(v);
    collection.value = detail?.collection;
    parts.value = detail?.parts ?? [];
  },
  { immediate: true },
);

const meta = computed(() => (collection.value ? [`${parts.value.length} 部作品`] : []));
</script>

<template>
  <main v-if="collection" class="detail-view">
    <PageHeader :title="collection.title" fallback="/library/movie" />

    <DetailHero
      :title="collection.title"
      :poster="collection.poster_path ?? undefined"
      :backdrop="collection.backdrop_path ?? undefined"
      :meta="meta"
      :overview="collection.overview"
      :show-actions="false"
    />

    <section class="parts">
      <h2 class="section-title">包含作品</h2>
      <div v-if="parts.length" class="grid">
        <MediaCard
          v-for="(item, i) in parts"
          :key="item.id"
          :item="item"
          layout="poster"
          :variant="(i % 2) as 0 | 1"
        />
      </div>
      <p v-else class="empty-state">该合集暂无可见成员。</p>
    </section>
  </main>

  <main v-else class="detail-missing">
    <p>未找到该合集，或后端尚未提供该合集详情。</p>
    <RouterLink to="/" class="link">返回首页</RouterLink>
  </main>
</template>

<style scoped lang="scss">
.parts {
  max-width: 1280px;
  margin: 0 auto;
  padding: 0 var(--fbz-space-8) 80px;
}

.section-title {
  margin: 0 0 var(--fbz-space-4);
  font-size: var(--fbz-font-size-lg);
  font-weight: 700;
}

.grid {
  display: grid;
  grid-template-columns: repeat(auto-fill, minmax(132px, 1fr));
  gap: var(--fbz-space-5) var(--fbz-space-4);
}

.empty-state {
  margin: 0;
  color: var(--fbz-color-text-muted);
  font-size: var(--fbz-font-size-sm);
}

.link {
  color: var(--fbz-color-brand-500);
  text-decoration: none;

  &:hover {
    text-decoration: underline;
  }
}

.detail-missing {
  min-height: 100vh;
  display: grid;
  place-content: center;
  gap: var(--fbz-space-3);
  text-align: center;
  color: var(--fbz-color-text-muted);
}

@media (max-width: 600px) {
  .parts {
    padding: 0 var(--fbz-space-4) 60px;
  }

  .grid {
    grid-template-columns: repeat(auto-fill, minmax(104px, 1fr));
    gap: var(--fbz-space-4) var(--fbz-space-3);
  }
}
</style>
