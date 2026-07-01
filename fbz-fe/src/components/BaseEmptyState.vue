<script setup lang="ts">
/**
 * 通用空状态：图标 + 标题 + 描述 + 可选操作区（默认插槽放按钮/链接）。
 * 媒体库总览、库详情、首页无库/无结果等场景共用，保证空态视觉一致。
 */
interface Props {
  /** 顶部大图标（emoji 或字符），默认空盒 */
  icon?: string;
  title: string;
  description?: string;
  /** 紧凑模式：用于列表内嵌的小空态（无结果），减少留白 */
  compact?: boolean;
}

withDefaults(defineProps<Props>(), {
  icon: "📁",
  description: "",
  compact: false,
});
</script>

<template>
  <div class="empty-state" :class="{ compact }">
    <div class="icon" aria-hidden="true">{{ icon }}</div>
    <h2 class="title">{{ title }}</h2>
    <p v-if="description" class="desc">{{ description }}</p>
    <div class="actions">
      <slot />
    </div>
  </div>
</template>

<style scoped lang="scss">
.empty-state {
  display: flex;
  flex-direction: column;
  align-items: center;
  justify-content: center;
  text-align: center;
  gap: var(--fbz-space-3);
  padding: 72px var(--fbz-space-6);
  border: 1px dashed var(--fbz-color-line);
  border-radius: 8px;
  background: var(--fbz-color-panel);

  &.compact {
    padding: 48px var(--fbz-space-5);
    border-style: solid;
    border-color: var(--fbz-color-line-soft);
  }
}

.icon {
  font-size: 40px;
  line-height: 1;
  opacity: 0.7;
}

.title {
  margin: 0;
  font-size: var(--fbz-font-size-lg);
  font-weight: 700;
  color: var(--fbz-color-text);
}

.desc {
  margin: 0;
  max-width: 46ch;
  font-size: var(--fbz-font-size-md);
  color: var(--fbz-color-text-muted);
  line-height: 1.6;
}

.actions {
  display: flex;
  flex-wrap: wrap;
  justify-content: center;
  gap: var(--fbz-space-3);
  margin-top: var(--fbz-space-2);

  &:empty {
    display: none;
  }
}
</style>
