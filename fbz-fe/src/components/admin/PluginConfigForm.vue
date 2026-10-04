<script setup lang="ts">
/**
 * 插件配置 schema 表单：按字段类型渲染控件。
 * secret/password 留空表示不修改后端已存值（由调用方在提交时跳过空串）。
 */
import type { PluginConfigField } from "@/types/admin.ts";

const props = defineProps<{
  schema: PluginConfigField[];
  modelValue: Record<string, unknown>;
  saving?: boolean;
  /** 表单控件 id 前缀，避免同页多实例冲突。 */
  idPrefix?: string;
}>();

const emit = defineEmits<{
  "update:modelValue": [value: Record<string, unknown>];
  save: [];
}>();

const prefix = computed(() => props.idPrefix ?? "pcf");

function fieldOptions(field: PluginConfigField): { label: string; value: string }[] {
  return field.options.map((opt) => ({ label: opt.label, value: opt.value }));
}

function setField(key: string, value: unknown) {
  emit("update:modelValue", { ...props.modelValue, [key]: value });
}
</script>

<template>
  <form class="plugin-config-form" @submit.prevent="emit('save')">
    <div v-for="field in schema" :key="field.key" class="config-field">
      <label class="field-name" :for="`${prefix}-${field.key}`">
        {{ field.label }}
        <span v-if="field.required" class="req">*</span>
      </label>

      <label v-if="field.type === 'boolean'" class="cfg-switch" :aria-label="field.label">
        <input
          :id="`${prefix}-${field.key}`"
          type="checkbox"
          :checked="modelValue[field.key] === true"
          @change="setField(field.key, ($event.target as HTMLInputElement).checked)"
        />
        <span class="switch-slide-thumb" />
      </label>

      <BaseSelect
        v-else-if="field.type === 'select'"
        :id="`${prefix}-${field.key}`"
        :model-value="modelValue[field.key] as string"
        :options="fieldOptions(field)"
        :ariaLabel="field.label"
        @update:model-value="setField(field.key, $event)"
      />

      <input
        v-else-if="field.type === 'number'"
        :id="`${prefix}-${field.key}`"
        class="cfg-input"
        type="number"
        :value="modelValue[field.key] as string | number"
        :required="field.required"
        @input="setField(field.key, ($event.target as HTMLInputElement).value)"
      />

      <input
        v-else-if="field.type === 'secret' || field.type === 'password'"
        :id="`${prefix}-${field.key}`"
        class="cfg-input"
        type="password"
        autocomplete="new-password"
        placeholder="留空表示保持已设置的值不变"
        :value="modelValue[field.key] as string"
        @input="setField(field.key, ($event.target as HTMLInputElement).value)"
      />

      <input
        v-else
        :id="`${prefix}-${field.key}`"
        class="cfg-input"
        :type="field.type === 'url' ? 'url' : 'text'"
        :value="modelValue[field.key] as string"
        :required="field.required"
        @input="setField(field.key, ($event.target as HTMLInputElement).value)"
      />

      <p v-if="field.helpText" class="field-help">{{ field.helpText }}</p>
    </div>

    <div class="config-actions">
      <button type="submit" class="save-btn" :disabled="saving">
        {{ saving ? "保存中..." : "保存配置" }}
      </button>
    </div>
  </form>
</template>

<style scoped lang="scss">
.plugin-config-form {
  display: flex;
  flex-direction: column;
  gap: var(--fbz-space-4);
}

.config-field {
  display: flex;
  flex-direction: column;
  gap: 8px;
  max-width: 520px;

  .field-name {
    font-size: var(--fbz-font-size-xs);
    font-weight: 700;
    color: var(--fbz-color-text-soft);

    .req {
      color: var(--fbz-color-danger-500);
    }
  }

  .field-help {
    margin: 0;
    font-size: var(--fbz-font-size-xs);
    color: var(--fbz-color-text-muted);
    line-height: 1.5;
  }
}

.cfg-input {
  height: 38px;
  background: var(--fbz-color-panel);
  border: 1px solid var(--fbz-color-line);
  border-radius: var(--fbz-radius-control);
  padding: 0 var(--fbz-space-3);
  color: var(--fbz-color-text);
  font-size: var(--fbz-font-size-sm);
  transition: all var(--fbz-motion-fast);

  &:focus {
    outline: none;
    border-color: var(--fbz-color-brand-500);
    box-shadow: var(--fbz-shadow-focus);
  }
}

.cfg-switch {
  position: relative;
  display: inline-block;
  width: 44px;
  height: 22px;

  input {
    opacity: 0;
    width: 0;
    height: 0;
  }

  .switch-slide-thumb {
    position: absolute;
    cursor: pointer;
    inset: 0;
    background-color: var(--fbz-color-line-bright);
    border-radius: 22px;
    transition: background-color var(--fbz-motion-fast);

    &::before {
      position: absolute;
      content: "";
      height: 16px;
      width: 16px;
      left: 3px;
      bottom: 3px;
      background-color: white;
      border-radius: 50%;
      transition: transform var(--fbz-motion-fast);
      box-shadow: 0 1px 3px rgba(0, 0, 0, 0.2);
    }
  }

  input:checked + .switch-slide-thumb {
    background-color: var(--fbz-color-brand-500);

    &::before {
      transform: translateX(22px);
    }
  }
}

.config-actions {
  display: flex;
  justify-content: flex-start;
}

.save-btn {
  height: 36px;
  padding: 0 var(--fbz-space-5);
  background: var(--fbz-color-brand-500);
  border: 0;
  color: #07120a;
  font-weight: 700;
  font-size: var(--fbz-font-size-sm);
  border-radius: var(--fbz-radius-control);
  cursor: pointer;
  transition: all var(--fbz-motion-fast);

  &:hover:not(:disabled) {
    background: var(--fbz-color-brand-600);
  }

  &:disabled {
    opacity: 0.6;
    cursor: not-allowed;
  }
}
</style>
