<script setup lang="ts">
import type { MediaItem } from "@/types/media.ts";

interface Props {
  title?: string;
  items: MediaItem[];
}

const props = withDefaults(defineProps<Props>(), {
  title: "相似推荐",
});
</script>

<template>
  <section v-if="props.items.length" class="similar-row">
    <h2 class="section-title">{{ props.title }}</h2>
    <BaseScroller class="scroller">
      <MediaCard
        v-for="(item, i) in props.items"
        :key="item.id"
        :item="item"
        layout="poster"
        :variant="(i % 2) as 0 | 1"
      />
    </BaseScroller>
  </section>
</template>

<style scoped lang="scss">
.similar-row {
  max-width: 1280px;
  margin: 0 auto;
  padding: 0 var(--fbz-space-8) var(--fbz-space-8);
}

.section-title {
  margin: 0 0 var(--fbz-space-4);
  font-size: var(--fbz-font-size-lg);
  font-weight: 700;
}

.scroller :deep(.track) {
  --col: 132px;
}

@media (max-width: 600px) {
  .similar-row {
    padding: 0 var(--fbz-space-4) var(--fbz-space-5);
  }

  .scroller :deep(.track) {
    --col: 112px;
  }
}
</style>
