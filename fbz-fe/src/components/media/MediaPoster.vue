<script setup lang="ts">
import { computed, ref, watch } from "vue";
import { mediaImageSrcSet } from "@/service/request.ts";

/**
 * 媒体海报 —— 有 src 时显示真实图（加载中骨架），无图 / 失败时渲染 Lucide 类型图标占位。
 * 圆角由使用方（卡片）裁切；自身不加大圆角。
 */
interface Props {
  src?: string;
  title: string;
  /** 占位块底色变体，用于相邻卡片交替 */
  variant?: 0 | 1;
  /** 海报宽高比 */
  ratio?: "poster" | "wide" | "square";
  /** 占位图标（BaseIcon 名称）；缺省按比例选择。 */
  icon?: string;
}

const props = withDefaults(defineProps<Props>(), {
  variant: 0,
  ratio: "poster",
});

const isLoaded = ref(false);
const hasError = ref(false);

const responsiveWidths = computed(() =>
  props.ratio === "wide" ? [480, 768, 1280, 1920] : [185, 342, 500, 780],
);
const srcset = computed(() => mediaImageSrcSet(props.src, responsiveWidths.value));
const sizes = computed(() =>
  props.ratio === "wide"
    ? "(max-width: 600px) 100vw, (max-width: 1200px) 50vw, 640px"
    : "(max-width: 600px) 33vw, (max-width: 1200px) 18vw, 220px",
);
const fallbackIcon = computed(
  () =>
    props.icon ?? (props.ratio === "wide" ? "video" : props.ratio === "square" ? "music" : "movie"),
);

watch(
  () => props.src,
  (newSrc) => {
    isLoaded.value = false;
    hasError.value = !newSrc;
  },
  { immediate: true },
);

function onLoad() {
  isLoaded.value = true;
  hasError.value = false;
}

function onError() {
  isLoaded.value = false;
  hasError.value = true;
}
</script>

<template>
  <div class="media-poster" :class="[`is-${props.ratio}`, { 'is-alt': props.variant === 1 }]">
    <img
      v-if="props.src && !hasError"
      :src="props.src"
      :srcset="srcset"
      :sizes="sizes"
      :alt="props.title"
      loading="lazy"
      decoding="async"
      :class="{ 'is-hidden': !isLoaded }"
      @load="onLoad"
      @error="onError"
    />

    <div v-if="props.src && !isLoaded && !hasError" class="shimmer" />

    <div v-if="!props.src || hasError" class="fallback" :title="props.title">
      <BaseIcon :name="fallbackIcon" :size="props.ratio === 'wide' ? 30 : 34" :stroke-width="1.4" />
      <span class="fallback-title">{{ props.title }}</span>
    </div>

    <slot />
  </div>
</template>

<style scoped lang="scss">
.media-poster {
  position: relative;
  width: 100%;
  overflow: hidden;
  background: var(--ui-surface-raised, var(--fbz-color-panel));

  &.is-poster {
    aspect-ratio: 2 / 3;
  }

  &.is-wide {
    aspect-ratio: 16 / 9;
  }

  &.is-square {
    aspect-ratio: 1 / 1;
  }

  &.is-alt {
    background: var(--ui-surface, var(--fbz-color-panel-strong));
  }

  img {
    display: block;
    width: 100%;
    height: 100%;
    object-fit: cover;
    transition: opacity 240ms ease;

    &.is-hidden {
      position: absolute;
      inset: 0;
      opacity: 0;
    }
  }
}

.shimmer {
  position: absolute;
  inset: 0;
  background: linear-gradient(
    100deg,
    transparent 20%,
    color-mix(in srgb, var(--ui-surface-hover, #20202c) 80%, transparent) 50%,
    transparent 80%
  );
  background-size: 200% 100%;
  animation: shimmer 1.4s ease infinite;
}

.fallback {
  position: absolute;
  inset: 0;
  display: flex;
  flex-direction: column;
  align-items: center;
  justify-content: center;
  gap: 8px;
  padding: 12px;
  color: var(--ui-border-strong, #3a3a50);
  text-align: center;
}

.fallback-title {
  max-width: 100%;
  display: -webkit-box;
  overflow: hidden;
  -webkit-line-clamp: 2;
  -webkit-box-orient: vertical;
  color: var(--ui-text-subtle, #707085);
  font-size: 11.5px;
  font-weight: 500;
  line-height: 1.35;
}

@keyframes shimmer {
  0% {
    background-position: 150% 0;
  }

  100% {
    background-position: -50% 0;
  }
}
</style>
