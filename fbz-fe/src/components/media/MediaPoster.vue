<script setup lang="ts">
import { computed, ref, watch } from "vue";
import { mediaImageSrcSet } from "@/service/request.ts";

/**
 * 媒体海报 —— 有 src 时显示真实图，加载中展示骨架屏，加载失败或无 src 时渲染精美的 placeholder 反馈。
 * variant 让相邻占位块色块交替，避免一片死板。
 */
interface Props {
  src?: string;
  title: string;
  /** 占位块底色变体，用于相邻卡片交替 */
  variant?: 0 | 1;
  /** 海报宽高比 */
  ratio?: "poster" | "wide";
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
    ? "(max-width: 600px) 100vw, (max-width: 1200px) 80vw, 1280px"
    : "(max-width: 600px) 32vw, (max-width: 1200px) 18vw, 220px",
);

watch(
  () => props.src,
  (newSrc) => {
    isLoaded.value = false;
    hasError.value = false;
    if (!newSrc) {
      hasError.value = true;
    }
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
    <!-- 真实图片，加载成功后淡入显示 -->
    <picture v-if="props.src && !hasError">
      <source :srcset="srcset" :sizes="sizes" />
      <img
        :src="props.src"
        :srcset="srcset"
        :sizes="sizes"
        :alt="props.title"
        loading="lazy"
        @load="onLoad"
        @error="onError"
        :class="{ 'is-hidden': !isLoaded }"
      />
    </picture>

    <!-- 加载中的骨架屏占位 -->
    <div v-if="props.src && !isLoaded && !hasError" class="shimmer-overlay" />

    <!-- 无图片或加载失败时的反馈占位 -->
    <div v-if="!props.src || hasError" class="placeholder-fallback">
      <div class="fallback-icon">
        <svg
          v-if="props.ratio === 'poster'"
          viewBox="0 0 24 24"
          width="36"
          height="36"
          fill="currentColor"
        >
          <path
            d="M18 4v16H6V4h12m0-2H6c-1.1 0-2 .9-2 2v16c0 1.1.9 2 2 2h12c1.1 0 2-.9 2-2V4c0-1.1-.9-2-2-2zM8 6h3v2H8zm5 0h3v2h-3zm-5 4h3v2H8zm5 0h3v2h-3zm-5 4h3v2H8zm5 0h3v2h-3z"
          />
        </svg>
        <svg v-else viewBox="0 0 24 24" width="48" height="48" fill="currentColor">
          <path
            d="M21 3H3c-1.1 0-2 .9-2 2v14c0 1.1.9 2 2 2h18c1.1 0 2-.9 2-2V5c0-1.1-.9-2-2-2zm0 16H3V5h18v14zM8 10l7 4-7 4v-8z"
          />
        </svg>
      </div>
      <span class="fallback-title">{{ props.title }}</span>
      <span class="fallback-tips">{{ hasError ? "图片加载失败" : "暂无封面" }}</span>
    </div>

    <slot />
  </div>
</template>

<style scoped lang="scss">
.media-poster {
  position: relative;
  width: 100%;
  overflow: hidden;
  border-radius: var(--fbz-radius-card);
  background: var(--fbz-color-panel);
  transition: background var(--fbz-motion-base);

  &.is-poster {
    aspect-ratio: 2 / 3;
  }

  &.is-wide {
    aspect-ratio: 16 / 9;
  }

  &.is-alt {
    background: var(--fbz-color-panel-strong);
  }

  picture,
  img {
    display: block;
    width: 100%;
    height: 100%;
  }

  img {
    object-fit: cover;
    opacity: 1;
    transition: opacity 0.3s ease;

    &.is-hidden {
      opacity: 0;
      position: absolute;
      inset: 0;
    }
  }
}

.shimmer-overlay {
  position: absolute;
  inset: 0;
  background: linear-gradient(
    90deg,
    var(--fbz-color-panel) 25%,
    var(--fbz-color-panel-strong) 37%,
    var(--fbz-color-panel) 63%
  );
  background-size: 200% 100%;
  animation: shimmer 1.4s ease infinite;
}

.placeholder-fallback {
  position: absolute;
  inset: 0;
  display: flex;
  flex-direction: column;
  align-items: center;
  justify-content: center;
  padding: var(--fbz-space-3);
  text-align: center;
  background: var(--fbz-color-panel-strong);

  .is-alt & {
    background: var(--fbz-color-panel-elevated);
  }

  .fallback-icon {
    color: var(--fbz-color-text-disabled);
    margin-bottom: var(--fbz-space-2);
    display: flex;
    align-items: center;
    justify-content: center;
  }

  .fallback-title {
    font-family: var(--fbz-font-display);
    font-size: var(--fbz-font-size-xs);
    font-weight: 700;
    color: var(--fbz-color-text-soft);
    margin-bottom: var(--fbz-space-1);
    display: -webkit-box;
    overflow: hidden;
    -webkit-line-clamp: 2;
    -webkit-box-orient: vertical;
    line-height: 1.3;
  }

  .fallback-tips {
    font-size: 10px;
    color: var(--fbz-color-text-muted);
    font-weight: 600;
  }
}

@keyframes shimmer {
  0% {
    background-position: -200% 0;
  }
  100% {
    background-position: 200% 0;
  }
}
</style>
