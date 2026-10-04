<script setup lang="ts">
withDefaults(
  defineProps<{ title: string; description?: string; icon?: string; error?: boolean }>(),
  { icon: "library" },
);
</script>
<template>
  <section class="empty-state" :role="error ? 'alert' : 'status'">
    <span class="empty-symbol"><BaseIcon :name="error ? 'error' : icon" :size="30" /></span>
    <h2>{{ title }}</h2>
    <p v-if="description">{{ description }}</p>
    <div v-if="$slots.default" class="empty-actions"><slot /></div>
  </section>
</template>
<style scoped lang="scss">
.empty-state {
  display: grid;
  justify-items: center;
  text-align: center;
  padding: 72px 24px;
  border: 1px dashed var(--fbz-color-line);
  border-radius: 16px;
}
.empty-symbol {
  width: 64px;
  height: 64px;
  display: grid;
  place-items: center;
  background: var(--fbz-color-panel-strong);
  color: var(--fbz-color-text-soft);
  border-radius: 20px;
  margin-bottom: 22px;
}
h2 {
  font-size: 21px;
  font-weight: 550;
  margin: 0 0 12px;
}
p {
  max-width: 42ch;
  line-height: 1.8;
  color: var(--fbz-color-text-muted);
  font-size: 14px;
  margin: 0;
}
.empty-actions {
  display: flex;
  gap: 12px;
  flex-wrap: wrap;
  justify-content: center;
  margin-top: 26px;
}
</style>
