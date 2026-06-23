<script setup lang="ts">
import type { ContinueItem } from "@/types/media.ts";

interface Props {
  title: string;
  items: ContinueItem[];
  layout?: "poster" | "wide";
  size?: "normal" | "large" | "xlarge";
  /** 「查看全部」跳转目标 */
  to?: string;
  showResolution?: boolean;
  showRating?: boolean;
}

const props = withDefaults(defineProps<Props>(), {
  layout: "poster",
  size: "normal",
  showResolution: true,
  showRating: true,
});

const isLarge = computed(() => props.size === "large");
const isXLarge = computed(() => props.size === "xlarge");

const colWidth = computed(() => {
  if (props.layout === "poster") return "132px";

  if (isXLarge.value) return "252px";
  if (isLarge.value) return "300px";
  if (props.layout === "wide") return "248px";

  return undefined;
});
</script>

<template>
  <section class="media-row">
    <header class="head">
      <h2>{{ props.title }}</h2>
      <RouterLink v-if="props.to" :to="props.to" class="more">查看全部</RouterLink>
    </header>
    <BaseScroller
      class="scroller"
      :class="`is-${props.layout} ${props.size === 'large' ? 'is-large' : ''} ${props.size === 'xlarge' ? 'is-xlarge' : ''}`"
      :col-width="colWidth"
    >
      <MediaCard
        v-for="(item, i) in props.items"
        :key="item.id"
        :item="item"
        :layout="props.layout"
        :show-resolution="props.showResolution"
        :show-rating="props.showRating"
        :variant="(i % 2) as 0 | 1"
      />
    </BaseScroller>
  </section>
</template>

<style scoped lang="scss">
.media-row {
  margin-bottom: var(--fbz-space-8);
}

.head {
  display: flex;
  align-items: baseline;
  justify-content: space-between;
  margin-bottom: var(--fbz-space-4);

  h2 {
    margin: 0;
    font-size: var(--fbz-font-size-lg);
    font-weight: 700;
  }
}

.more {
  font-size: var(--fbz-font-size-md);
  color: var(--fbz-color-text-muted);
  text-decoration: none;
  transition: color var(--fbz-motion-fast);

  &:hover {
    color: var(--fbz-color-text);
  }
}

// 每列宽度按 layout / 尺寸覆盖 BaseScroller 的 --col（通过 col-width prop）。
.scroller.is-poster :deep(.track) {
  --col: 132px;
}
</style>
