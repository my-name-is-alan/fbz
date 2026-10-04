<script setup lang="ts">
import { useUiStore } from "@/stores/ui.ts";
const ui = useUiStore();
const icons: Record<string, string> = {
  success: "success",
  error: "error",
  warning: "error",
  info: "info",
};
</script>
<template>
  <div class="du-toast du-toast-top du-toast-end feedback-stack" aria-live="polite">
    <TransitionGroup name="feedback"
      ><div
        v-for="toast in ui.toasts"
        :key="toast.id"
        class="du-alert feedback-item"
        :class="toast.type"
        :role="toast.type === 'error' ? 'alert' : 'status'"
      >
        <BaseIcon :name="icons[toast.type] || 'info'" :size="19" /><span>{{ toast.message }}</span
        ><button
          class="du-btn du-btn-ghost du-btn-circle du-btn-xs"
          aria-label="关闭提示"
          @click="ui.removeToast(toast.id)"
        >
          <BaseIcon name="close" :size="14" />
        </button></div
    ></TransitionGroup>
  </div>
</template>
<style scoped lang="scss">
.feedback-stack {
  position: fixed;
  top: calc(var(--header-h) + 14px);
  right: 22px;
  z-index: 120;
  width: min(380px, calc(100vw - 32px));
  pointer-events: none;
  gap: 10px;
}
.feedback-item {
  pointer-events: auto;
  display: flex;
  gap: 12px;
  border: 1px solid var(--fbz-color-line);
  background: var(--fbz-color-panel);
  color: var(--fbz-color-text);
  padding: 14px 16px;
  font-size: 12px;
  line-height: 1.7;
}
.feedback-item span {
  flex: 1;
}
.feedback-item.success > svg {
  color: var(--fbz-color-brand-500);
}
.feedback-item.error > svg {
  color: var(--fbz-color-danger-500);
}
.feedback-enter-active,
.feedback-leave-active {
  transition:
    opacity 0.2s,
    transform 0.2s;
}
.feedback-enter-from,
.feedback-leave-to {
  opacity: 0;
  transform: translateY(-8px);
}
</style>
