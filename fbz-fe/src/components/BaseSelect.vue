<script setup lang="ts" generic="T extends string | number">
const props = withDefaults(
  defineProps<{
    options: { label: string; value: T }[];
    size?: "sm" | "md";
    ariaLabel?: string;
    placeholder?: string;
  }>(),
  { size: "md", placeholder: "请选择" },
);
const model = defineModel<T>();
function select(event: Event) {
  const value = (event.target as HTMLSelectElement).value;
  const option = props.options.find((o) => String(o.value) === value);
  if (option) model.value = option.value;
}
</script>
<template>
  <select
    :value="model"
    class="du-select base-select"
    :class="{ 'du-select-sm': size === 'sm' }"
    :aria-label="ariaLabel"
    @change="select"
  >
    <option v-if="model === undefined" disabled value="">{{ placeholder }}</option>
    <option v-for="option in options" :key="option.value" :value="option.value">
      {{ option.label }}
    </option>
  </select>
</template>
<style scoped lang="scss">
.base-select {
  width: 100%;
  font-size: 13px;
  min-width: 0;
  max-width: none;
}
</style>
