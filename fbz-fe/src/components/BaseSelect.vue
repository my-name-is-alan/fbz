<script setup lang="ts" generic="T extends string | number">
/**
 * 自定义下拉选择 —— 替代原生 <select>，统一面板样式 / 间距 / 居中 / 选中态。
 * v-model 绑定值；options 为 { label, value }[]。点击外部 / Esc 关闭，方向键 + 回车选择。
 *
 * 用法：
 *   <BaseSelect v-model="x" :options="[{ label: '名称', value: 'title' }]" />
 */
interface Option {
  label: string;
  value: T;
}

interface Props {
  options: Option[];
  /** 控件尺寸 */
  size?: "sm" | "md";
  /** 无障碍标签 */
  ariaLabel?: string;
  /** 占位（无选中项时显示） */
  placeholder?: string;
}

const props = withDefaults(defineProps<Props>(), {
  size: "md",
  placeholder: "请选择",
});

const model = defineModel<T>();

const root = ref<HTMLElement>();
const open = ref(false);
const activeIndex = ref(-1);

const selected = computed(() => props.options.find((o) => o.value === model.value));
const label = computed(() => selected.value?.label ?? props.placeholder);

function toggle() {
  open.value = !open.value;
  if (open.value) {
    activeIndex.value = props.options.findIndex((o) => o.value === model.value);
  }
}

function close() {
  open.value = false;
}

function pick(opt: Option) {
  model.value = opt.value;
  close();
}

onClickOutside(root, close);

function onKeydown(e: KeyboardEvent) {
  if (!open.value) {
    if (e.key === "Enter" || e.key === " " || e.key === "ArrowDown") {
      e.preventDefault();
      toggle();
    }
    return;
  }
  switch (e.key) {
    case "Escape":
      close();
      break;
    case "ArrowDown":
      e.preventDefault();
      activeIndex.value = Math.min(activeIndex.value + 1, props.options.length - 1);
      break;
    case "ArrowUp":
      e.preventDefault();
      activeIndex.value = Math.max(activeIndex.value - 1, 0);
      break;
    case "Enter":
      e.preventDefault();
      if (props.options[activeIndex.value]) pick(props.options[activeIndex.value]);
      break;
  }
}
</script>

<template>
  <div ref="root" class="base-select" :class="`is-${props.size}`">
    <button
      type="button"
      class="trigger"
      :class="{ open }"
      :aria-label="props.ariaLabel"
      :aria-expanded="open"
      @click="toggle"
      @keydown="onKeydown"
    >
      <span class="value" :class="{ placeholder: !selected }">{{ label }}</span>
      <svg class="caret" viewBox="0 0 24 24" width="16" height="16" aria-hidden="true">
        <path
          d="M6 9l6 6 6-6"
          fill="none"
          stroke="currentColor"
          stroke-width="2"
          stroke-linecap="round"
          stroke-linejoin="round"
        />
      </svg>
    </button>

    <Transition name="panel">
      <ul v-if="open" class="panel" role="listbox">
        <li
          v-for="(opt, i) in props.options"
          :key="String(opt.value)"
          class="option"
          :class="{ active: opt.value === model.value, focus: i === activeIndex }"
          role="option"
          :aria-selected="opt.value === model.value"
          @click="pick(opt)"
          @mouseenter="activeIndex = i"
        >
          <span class="option-label">{{ opt.label }}</span>
          <svg
            v-if="opt.value === model.value"
            class="check"
            viewBox="0 0 24 24"
            width="15"
            height="15"
            aria-hidden="true"
          >
            <path
              d="M5 13l4 4L19 7"
              fill="none"
              stroke="currentColor"
              stroke-width="2.4"
              stroke-linecap="round"
              stroke-linejoin="round"
            />
          </svg>
        </li>
      </ul>
    </Transition>
  </div>
</template>

<style scoped lang="scss">
.base-select {
  position: relative;
  display: inline-block;
  min-width: 0;
}

.trigger {
  display: inline-flex;
  align-items: center;
  justify-content: space-between;
  gap: var(--fbz-space-2);
  width: 100%;
  height: 36px;
  padding: 0 10px 0 12px;
  border-radius: var(--fbz-radius-control);
  border: 1px solid var(--fbz-color-line);
  background: var(--fbz-color-panel);
  color: var(--fbz-color-text);
  font-size: var(--fbz-font-size-sm);
  font-weight: 600;
  line-height: 1;
  transition:
    border-color var(--fbz-motion-fast),
    background var(--fbz-motion-fast);

  &:hover {
    border-color: var(--fbz-color-line-bright);
  }

  &.open {
    border-color: var(--fbz-color-brand-500);
  }
}

.is-md .trigger {
  height: 40px;
}

.value {
  overflow: hidden;
  white-space: nowrap;
  text-overflow: ellipsis;

  &.placeholder {
    color: var(--fbz-color-text-muted);
    font-weight: 400;
  }
}

.caret {
  flex: 0 0 auto;
  color: var(--fbz-color-text-muted);
  transition: transform var(--fbz-motion-fast);

  .trigger.open & {
    transform: rotate(180deg);
    color: var(--fbz-color-brand-500);
  }
}

.panel {
  position: absolute;
  top: calc(100% + 6px);
  left: 0;
  z-index: var(--fbz-z-overlay);
  min-width: 100%;
  max-height: 280px;
  overflow-y: auto;
  margin: 0;
  padding: 6px;
  list-style: none;
  border-radius: var(--fbz-radius-control);
  border: 1px solid var(--fbz-color-line);
  background: rgba(16, 16, 18, 0.97);
  -webkit-backdrop-filter: blur(14px);
  backdrop-filter: blur(14px);
  box-shadow: var(--fbz-shadow-panel);
  scrollbar-width: thin;
}

.option {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: var(--fbz-space-3);
  padding: 8px 10px;
  border-radius: 6px;
  font-size: var(--fbz-font-size-sm);
  color: var(--fbz-color-text-soft);
  white-space: nowrap;
  cursor: pointer;
  transition:
    background var(--fbz-motion-fast),
    color var(--fbz-motion-fast);

  &.focus {
    background: rgba(255, 255, 255, 0.06);
    color: #fff;
  }

  &.active {
    color: var(--fbz-color-brand-500);
    font-weight: 600;
  }
}

.check {
  flex: 0 0 auto;
  color: var(--fbz-color-brand-500);
}

// 面板淡入 + 轻微下移
.panel-enter-active,
.panel-leave-active {
  transition:
    opacity var(--fbz-motion-fast),
    transform var(--fbz-motion-fast);
}

.panel-enter-from,
.panel-leave-to {
  opacity: 0;
  transform: translateY(-4px);
}
</style>
