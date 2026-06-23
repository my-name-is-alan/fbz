<script setup lang="ts">
/**
 * 媒体海报 —— 有 src 时显示真实图，无 src 时渲染纯色占位块（设计阶段默认走占位）。
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
</script>

<template>
  <div class="media-poster" :class="[`is-${props.ratio}`, { 'is-alt': props.variant === 1 }]">
    <img v-if="props.src" :src="props.src" :alt="props.title" loading="lazy" />
    <span v-else class="placeholder">{{ props.title }}</span>
    <slot />
  </div>
</template>

<style scoped lang="scss">
.media-poster {
  position: relative;
  width: 100%;
  overflow: hidden;
  // 自带圆角：starport 飞渡时海报会被传送到 carrier（脱离卡片/详情的圆角容器），
  // 没有这层圆角飞行途中会变成直角，落地才变圆
  border-radius: var(--fbz-radius-card);
  background: var(--fbz-color-panel);

  &.is-poster {
    aspect-ratio: 2 / 3;
  }

  &.is-wide {
    aspect-ratio: 16 / 9;
  }

  &.is-alt {
    background: var(--fbz-color-panel-strong);
  }

  img {
    display: block;
    width: 100%;
    height: 100%;
    object-fit: cover;
  }
}

.placeholder {
  position: absolute;
  inset: 0;
  display: grid;
  place-content: center;
  padding: 10px;
  text-align: center;
  font-family: var(--fbz-font-display);
  font-size: var(--fbz-font-size-xs);
  font-weight: 700;
  letter-spacing: 0;
  color: var(--fbz-color-text-muted);
}
</style>
