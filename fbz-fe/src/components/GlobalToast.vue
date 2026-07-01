<script setup lang="ts">
import { useUiStore } from "@/stores/ui.ts";

const uiStore = useUiStore();
const { toasts } = storeToRefs(uiStore);

function getToastIcon(type: string) {
  switch (type) {
    case "success":
      return "✨";
    case "error":
      return "🚨";
    case "warning":
      return "⚠️";
    case "info":
    default:
      return "💡";
  }
}
</script>

<template>
  <div class="global-toast-container">
    <TransitionGroup name="toast-list">
      <div
        v-for="toast in toasts"
        :key="toast.id"
        class="toast-item"
        :class="toast.type"
        @click="uiStore.removeToast(toast.id)"
      >
        <span class="toast-icon">{{ getToastIcon(toast.type) }}</span>
        <span class="toast-message">{{ toast.message }}</span>
        <button class="toast-close-btn" type="button" aria-label="关闭">✕</button>
      </div>
    </TransitionGroup>
  </div>
</template>

<style scoped lang="scss">
.global-toast-container {
  position: fixed;
  top: 24px;
  right: 24px;
  z-index: 9999;
  display: flex;
  flex-direction: column;
  gap: 12px;
  width: 360px;
  max-width: calc(100vw - 48px);
  pointer-events: none;
}

.toast-item {
  pointer-events: auto;
  display: flex;
  align-items: center;
  gap: 12px;
  padding: 12px 16px;
  /* 用随主题翻转的 panel token，避免白主题下深底 + 深字看不清 */
  background: color-mix(in srgb, var(--fbz-color-panel-elevated) 92%, transparent);
  backdrop-filter: blur(16px);
  border: 1px solid var(--fbz-color-line);
  border-radius: 6px;
  box-shadow: 0 10px 30px rgba(0, 0, 0, 0.3);
  cursor: pointer;
  user-select: none;
  width: 100%;
  box-sizing: border-box;
  transition:
    border-color var(--fbz-motion-fast),
    transform var(--fbz-motion-base) cubic-bezier(0.16, 1, 0.3, 1),
    box-shadow var(--fbz-motion-fast);

  &.success {
    border-color: color-mix(in srgb, var(--fbz-color-brand-500) 40%, transparent);
    .toast-icon {
      color: var(--fbz-color-brand-500);
    }
  }

  &.error {
    border-color: color-mix(in srgb, var(--fbz-color-danger-500) 40%, transparent);
    .toast-icon {
      color: var(--fbz-color-danger-500);
    }
  }

  &.warning {
    border-color: color-mix(in srgb, var(--fbz-color-amber-500) 40%, transparent);
    .toast-icon {
      color: var(--fbz-color-amber-500);
    }
  }

  &.info {
    border-color: color-mix(in srgb, var(--fbz-color-blue-500, #3b82f6) 40%, transparent);
    .toast-icon {
      color: var(--fbz-color-blue-500, #3b82f6);
    }
  }

  .toast-icon {
    font-size: 16px;
    flex-shrink: 0;
  }

  .toast-message {
    flex: 1;
    font-size: 13px;
    font-weight: 600;
    color: var(--fbz-color-text);
    line-height: 1.4;
    word-break: break-word;
  }

  .toast-close-btn {
    background: none;
    border: 0;
    color: var(--fbz-color-text-muted);
    font-size: 11px;
    cursor: pointer;
    padding: 2px;
    opacity: 0;
    transition: opacity var(--fbz-motion-fast);
  }

  &:hover {
    border-color: var(--fbz-color-line-bright);
    transform: translateY(-2px);
    box-shadow: 0 12px 36px rgba(0, 0, 0, 0.4);

    .toast-close-btn {
      opacity: 1;
    }
  }
}

/* Toast Transitions */
.toast-list-enter-from {
  opacity: 0;
  transform: translateX(120px) scale(0.9);
}
.toast-list-leave-to {
  opacity: 0;
  transform: translateX(120px) scale(0.9);
}
.toast-list-enter-active,
.toast-list-leave-active {
  transition: all 0.4s cubic-bezier(0.16, 1, 0.3, 1);
}
.toast-list-leave-active {
  position: absolute;
}
.toast-list-move {
  transition: transform 0.4s cubic-bezier(0.16, 1, 0.3, 1);
}
</style>
