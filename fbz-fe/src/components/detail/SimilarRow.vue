<script setup lang="ts">
import type { MediaItem } from "@/types/media.ts";

interface Props {
  title?: string;
  items: MediaItem[];
  icon?: string;
}

const props = withDefaults(defineProps<Props>(), {
  title: "相似推荐",
  icon: "sparkles",
});
</script>

<template>
  <section v-if="props.items.length" class="similar-row">
    <h2 class="c-section-title">
      <BaseIcon :name="props.icon" :size="16" class="lead" />{{ props.title }}
    </h2>
    <BaseScroller class="scroller" col-width="var(--similar-col)" gap="16px">
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
  --similar-col: 160px;

  display: flex;
  flex-direction: column;
  gap: 10px;

  .lead {
    color: var(--ui-text-subtle);
  }
}

.scroller :deep(.track) {
  padding: 6px 2px 10px;
}

@media (max-width: 600px) {
  .similar-row {
    --similar-col: 118px;
  }
}
</style>
