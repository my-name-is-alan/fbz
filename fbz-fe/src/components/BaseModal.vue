<script setup lang="ts">
const props = defineProps<{ open: boolean; title: string; description?: string; wide?: boolean }>();
const emit = defineEmits<{ close: [] }>();
const dialog = ref<HTMLDialogElement>();
watch(
  () => props.open,
  async (open) => {
    await nextTick();
    if (open && !dialog.value?.open) dialog.value?.showModal();
    else if (!open && dialog.value?.open) dialog.value?.close();
  },
  { immediate: true },
);
function onBackdrop(event: MouseEvent) {
  if (event.target === dialog.value) emit("close");
}
</script>
<template>
  <Teleport to="body"
    ><dialog
      ref="dialog"
      class="du-modal app-modal"
      :class="{ 'is-wide': wide }"
      @cancel.prevent="emit('close')"
      @click="onBackdrop"
    >
      <section class="du-modal-box">
        <header>
          <div>
            <h2>{{ title }}</h2>
            <p v-if="description">{{ description }}</p>
          </div>
          <button
            class="du-btn du-btn-ghost du-btn-circle du-btn-sm"
            aria-label="关闭弹窗"
            @click="emit('close')"
          >
            <BaseIcon name="close" />
          </button>
        </header>
        <div class="modal-content"><slot /></div>
        <footer v-if="$slots.actions" class="du-modal-action"><slot name="actions" /></footer>
      </section></dialog
  ></Teleport>
</template>
<style scoped lang="scss">
.app-modal {
  background: transparent;
  color: var(--fbz-color-text);
  z-index: 100;
}
.app-modal::backdrop {
  background: #030507ba;
  backdrop-filter: blur(8px);
}
.du-modal-box {
  width: min(580px, calc(100vw - 32px));
  max-width: none;
  padding: 28px;
  background: var(--fbz-color-panel);
  border: 1px solid var(--fbz-color-line);
  border-radius: 16px;
  color: inherit;
}
.is-wide .du-modal-box {
  width: min(920px, calc(100vw - 32px));
}
header {
  display: flex;
  align-items: flex-start;
  justify-content: space-between;
  gap: 20px;
  margin-bottom: 26px;
}
h2 {
  margin: 0;
  font-size: 22px;
  font-weight: 600;
  letter-spacing: -0.5px;
}
p {
  margin: 8px 0 0;
  color: var(--fbz-color-text-muted);
  font-size: 13px;
  line-height: 1.7;
}
.modal-content {
  min-width: 0;
}
footer {
  padding-top: 20px;
  border-top: 1px solid var(--fbz-color-line-soft);
}
@media (max-width: 600px) {
  .du-modal-box {
    padding: 22px;
    width: 100%;
    max-height: 88dvh;
    border-radius: 18px 18px 0 0;
  }
  .app-modal {
    align-items: end;
  }
  .is-wide .du-modal-box {
    width: 100%;
  }
}
</style>
