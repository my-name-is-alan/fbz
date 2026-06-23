<script setup lang="ts">
interface Props {
  title?: string;
  fallback?: string;
}

const props = withDefaults(defineProps<Props>(), {
  fallback: "/",
});

const router = useRouter();

function goBack() {
  if (window.history.state?.back) {
    router.back();
    return;
  }

  router.push(props.fallback);
}
</script>

<template>
  <div class="page-header">
    <button class="back-btn" type="button" aria-label="返回上一页" @click="goBack">
      <span class="back-icon">‹</span>
      <span class="back-label">返回</span>
    </button>
    <span v-if="props.title" class="page-title">{{ props.title }}</span>
  </div>
</template>

<style scoped lang="scss">
.page-header {
  position: fixed;
  z-index: calc(var(--fbz-z-overlay) + 1);
  top: calc(var(--header-h, 60px) + var(--fbz-space-3));
  left: var(--fbz-space-8);
  right: var(--fbz-space-8);
  display: flex;
  align-items: center;
  gap: var(--fbz-space-3);
  pointer-events: none;
}

.back-btn {
  pointer-events: auto;
  height: 36px;
  display: inline-flex;
  align-items: center;
  gap: 6px;
  padding: 0 12px 0 10px;
  border-radius: var(--fbz-radius-control);
  border: 1px solid rgba(255, 255, 255, 0.12);
  background: rgba(10, 10, 11, 0.68);
  color: var(--fbz-color-text);
  font-size: var(--fbz-font-size-sm);
  font-weight: 700;
  backdrop-filter: blur(14px);
  -webkit-backdrop-filter: blur(14px);
  transition:
    background var(--fbz-motion-fast),
    border-color var(--fbz-motion-fast);

  &:hover {
    border-color: var(--fbz-color-line-bright);
    background: rgba(20, 20, 22, 0.86);
  }
}

.back-icon {
  font-size: 22px;
  line-height: 1;
}

.back-label {
  line-height: 1;
}

.page-title {
  min-width: 0;
  max-width: 48vw;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
  color: var(--fbz-color-text-muted);
  font-size: var(--fbz-font-size-sm);
  text-shadow: 0 1px 8px rgba(0, 0, 0, 0.5);
}

@media (max-width: 600px) {
  .page-header {
    top: calc(var(--header-h, 56px) + var(--fbz-space-2));
    left: var(--fbz-space-4);
    right: var(--fbz-space-4);
  }

  .page-title {
    display: none;
  }
}
</style>
