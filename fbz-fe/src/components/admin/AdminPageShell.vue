<script setup lang="ts">
/**
 * 后台页面统一外壳：标题横幅（标题 + 描述）+ 内容插槽。
 * 每个 `views/admin/*` 页面用它包裹自己的主体组件，保持横幅样式一致；
 * 自带标题区的页面（如插件菜单页）可省略 title 只用容器。
 */
interface Props {
  title?: string;
  description?: string;
}

defineProps<Props>();
</script>

<template>
  <main class="admin-page">
    <div v-if="title" class="page-banner">
      <h1 class="page-heading">{{ title }}</h1>
      <p v-if="description" class="description-text">{{ description }}</p>
    </div>
    <slot />
  </main>
</template>

<style scoped lang="scss">
.admin-page {
  padding: 0 0 80px;
  max-width: 1200px;
  margin: 0 auto;
}

.page-banner {
  border-bottom: 1px solid var(--fbz-color-line-soft);
  padding-bottom: var(--fbz-space-5);
  margin-bottom: var(--fbz-space-6);

  .page-heading {
    margin: 0 0 var(--fbz-space-2);
    font-size: 22px;
    font-weight: 800;
    letter-spacing: -0.3px;
    color: var(--fbz-color-text);
  }

  .description-text {
    margin: 0;
    color: var(--fbz-color-text-muted);
    font-size: var(--fbz-font-size-sm);
    line-height: 1.6;
  }
}
</style>
