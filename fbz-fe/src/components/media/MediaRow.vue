<script setup lang="ts">
import type { ContinueItem } from "@/types/media.ts";

interface Props {
  title: string;
  items: ContinueItem[];
  layout?: "poster" | "wide";
  size?: "normal" | "large" | "xlarge";
  /** 「查看全部」跳转目标 */
  to?: string;
  /** 标题前的 Lucide 图标 */
  icon?: string;
  showResolution?: boolean;
  showRating?: boolean;
}

const props = withDefaults(defineProps<Props>(), {
  layout: "poster",
  size: "normal",
  showResolution: true,
  showRating: true,
});

const colWidth = computed(() => {
  if (props.layout === "poster") return "var(--row-poster-col)";
  if (props.size === "xlarge") return "var(--row-wide-col-xl)";
  return "var(--row-wide-col)";
});
</script>

<template>
  <section class="media-row">
    <header class="head">
      <component
        :is="props.to ? 'RouterLink' : 'h2'"
        :to="props.to"
        class="c-section-title title-link"
      >
        <BaseIcon v-if="props.icon" :name="props.icon" :size="16" class="lead" />
        {{ props.title }}
        <span class="count">{{ props.items.length }}</span>
        <BaseIcon v-if="props.to" name="right" :size="16" class="chev" />
      </component>
    </header>
    <BaseScroller class="scroller" :col-width="colWidth" gap="16px">
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
  --row-poster-col: 156px;
  --row-wide-col: 260px;
  --row-wide-col-xl: 300px;

  display: flex;
  flex-direction: column;
  gap: 10px;
}

.head {
  display: flex;
  align-items: center;
  justify-content: space-between;
}

.title-link {
  color: var(--ui-text);
  text-decoration: none;

  .lead {
    color: var(--ui-text-subtle);
  }

  .count {
    margin-left: 2px;
    color: var(--ui-text-subtle);
    font-size: 12px;
    font-weight: 500;
    font-variant-numeric: tabular-nums;
  }

  .chev {
    color: var(--ui-text-subtle);
    transition: transform var(--ui-duration-fast);
  }
}

a.title-link:hover {
  color: var(--ui-accent);

  .chev {
    color: var(--ui-accent);
    transform: translateX(2px);
  }
}

// BaseScroller 内留出 hover 位移空间，同时与页面左缘对齐。
.scroller :deep(.track) {
  padding: 6px 2px 10px;
}

@media (max-width: 600px) {
  .media-row {
    --row-poster-col: 118px;
    --row-wide-col: 220px;
    --row-wide-col-xl: 240px;
  }
}
</style>
