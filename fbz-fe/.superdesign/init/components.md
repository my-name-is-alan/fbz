# Shared UI Primitives

Framework: Vue 3 + Vite. UI: custom Base\* primitives + daisyUI (`du-` prefix) + UnoCSS/Tailwind tokens.

Auto-import via `unplugin-vue-components` from `src/components/`.

## BaseAvatar

- Path: `src/components/BaseAvatar.vue`
- Description: Circular user avatar with API image + nickname-initial fallback color.
- Key props: userId?, name?, size?=34, version?

```vue
<script setup lang="ts">
import { userAvatarUrl } from "@/service/request.ts";

/**
 * 通用用户头像：优先展示 `/api/users/{id}/avatar` 真实图，加载失败（未设置头像→404）
 * 回退为昵称首字母 + 稳定派生色。顶部 header、个人资料、用户管理列表共用同一个。
 */
interface Props {
  userId?: string | null;
  /** 用于首字母与派生底色的名称 */
  name?: string;
  /** 像素直径 */
  size?: number;
  /** 头像缓存版本（改动后传新值可击穿浏览器缓存） */
  version?: string | number | null;
}

const props = withDefaults(defineProps<Props>(), {
  userId: null,
  name: "",
  size: 34,
  version: null,
});

// 加载失败标记：同一 src 变化时重置，允许换头像后重新尝试。
const failed = ref(false);

// 只有明确知道用户设置过头像（version 非空）才请求头像端点：
// 未设置头像时直接渲染首字母占位，避免整站每个头像都打出一条 404 噪音。
const src = computed(() =>
  props.version != null && props.version !== ""
    ? userAvatarUrl(props.userId, props.version)
    : undefined,
);
watch(src, () => {
  failed.value = false;
});

const initial = computed(() => (props.name || "?").trim().charAt(0).toUpperCase() || "?");

// 由名称派生稳定的 HSL 底色（同名同色），保证无图时也有辨识度。
const fallbackColor = computed(() => {
  const source = props.name || props.userId || "?";
  let hash = 0;
  for (let i = 0; i < source.length; i += 1) {
    hash = (hash * 31 + source.charCodeAt(i)) % 360;
  }
  return `hsl(${hash}, 42%, 42%)`;
});

const showImage = computed(() => !!src.value && !failed.value);
</script>

<template>
  <span
    class="base-avatar"
    :style="{
      width: `${size}px`,
      height: `${size}px`,
      fontSize: `${Math.round(size * 0.42)}px`,
      background: showImage ? 'transparent' : fallbackColor,
    }"
    :title="name || undefined"
  >
    <img v-if="showImage" :src="src" :alt="name || '用户头像'" @error="failed = true" />
    <span v-else aria-hidden="true">{{ initial }}</span>
  </span>
</template>

<style scoped lang="scss">
.base-avatar {
  display: inline-grid;
  place-content: center;
  border-radius: 50%;
  overflow: hidden;
  flex: 0 0 auto;
  color: #ffffff;
  font-weight: 700;
  line-height: 1;
  user-select: none;

  img {
    width: 100%;
    height: 100%;
    object-fit: cover;
    display: block;
  }
}
</style>
```

## BaseEmptyState

- Path: `src/components/BaseEmptyState.vue`
- Description: Empty-state panel with icon/title/description and action slot.
- Key props: icon?, title, description?, compact?

```vue
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
    <div class="icon" aria-hidden="true">
      <BaseIcon v-if="/^[a-z-]+$/.test(icon)" :name="icon" :size="30" /><span v-else>{{
        icon
      }}</span>
    </div>
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
```

## BaseIcon

- Path: `src/components/BaseIcon.vue`
- Description: Lucide icon registry wrapper (`name` → @lucide/vue component).
- Key props: name, size?=20

```vue
<script setup lang="ts">
import {
  Play,
  Pause,
  Search,
  House,
  Library,
  Film,
  Tv,
  Settings,
  User,
  LogOut,
  ChevronLeft,
  ChevronRight,
  ChevronDown,
  X,
  Plus,
  RefreshCw,
  Folder,
  Cloud,
  SlidersHorizontal,
  Volume2,
  VolumeX,
  Maximize,
  Minimize,
  Captions,
  SkipForward,
  SkipBack,
  RotateCcw,
  RotateCw,
  LoaderCircle,
  Info,
  Check,
  CheckCircle2,
  AlertCircle,
  Star,
  Clock,
  ArrowUpRight,
  Menu,
  Sun,
  Moon,
  Grid2X2,
  List,
  Heart,
  CircleHelp,
} from "@lucide/vue";
const icons = {
  play: Play,
  pause: Pause,
  search: Search,
  home: House,
  library: Library,
  movie: Film,
  tv: Tv,
  settings: Settings,
  user: User,
  logout: LogOut,
  left: ChevronLeft,
  right: ChevronRight,
  down: ChevronDown,
  close: X,
  plus: Plus,
  refresh: RefreshCw,
  folder: Folder,
  cloud: Cloud,
  filter: SlidersHorizontal,
  volume: Volume2,
  mute: VolumeX,
  fullscreen: Maximize,
  minimize: Minimize,
  subtitles: Captions,
  next: SkipForward,
  previous: SkipBack,
  rewind: RotateCcw,
  forward: RotateCw,
  loading: LoaderCircle,
  info: Info,
  check: Check,
  success: CheckCircle2,
  error: AlertCircle,
  star: Star,
  clock: Clock,
  external: ArrowUpRight,
  menu: Menu,
  sun: Sun,
  moon: Moon,
  grid: Grid2X2,
  list: List,
  heart: Heart,
  help: CircleHelp,
};
const props = withDefaults(defineProps<{ name: keyof typeof icons | string; size?: number }>(), {
  size: 20,
});
</script>
<template>
  <component
    :is="icons[props.name as keyof typeof icons] || Info"
    :size="size"
    :stroke-width="1.7"
    aria-hidden="true"
    class="fbz-icon"
  />
</template>
<style scoped lang="scss">
.fbz-icon {
  flex-shrink: 0;
  vertical-align: middle;
}
</style>
```

## BaseModal

- Path: `src/components/BaseModal.vue`
- Description: Native <dialog> modal with daisyUI shell, title/description, default + actions slots.
- Key props: open, title, description?, wide?

```vue
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
```

## BaseScroller

- Path: `src/components/BaseScroller.vue`
- Description: Horizontal scroller with fade edge arrows; grid auto-flow columns.
- Key props: colWidth?=132px, gap?, step?=0.8

```vue
<script setup lang="ts">
/**
 * 横向滚动容器 —— 隐藏原生滚动条，按需在行首/行尾浮出半透明渐变遮罩 + 居中矢量箭头。
 * 自己按 scrollLeft 测两端状态，只在「该方向还有内容可滚」时才显示对应遮罩，
 * 不溢出时两端都不显示。遮罩淡入淡出，触摸设备隐藏（用原生滑动）。
 *
 * 用法：<BaseScroller col-width="132px"><卡片 1 /><卡片 2 />...</BaseScroller>
 * 内部容器是 grid（grid-auto-flow: column），每列宽度由 colWidth 控制。
 */
interface Props {
  /** 每列宽度，如 "132px" / "248px" / "84px" */
  colWidth?: string;
  /** 列间距 */
  gap?: string;
  /** 每次点击箭头滚动的视宽比例 */
  step?: number;
}

const props = withDefaults(defineProps<Props>(), {
  colWidth: "132px",
  gap: "var(--fbz-space-4)",
  step: 0.8,
});

const scroller = ref<HTMLElement>();

const canLeft = ref(false);
const canRight = ref(false);

/**
 * 自己测量两端状态，而不用 useScroll 的 arrivedState ——
 * 后者初始把 right 也置为 true（只在滚动事件后才更新），会导致首屏右箭头不显示。
 * 这里在滚动 / 尺寸变化时直接按 scrollLeft 计算，首屏即正确。
 */
function update() {
  const el = scroller.value;
  if (!el) return;
  const max = el.scrollWidth - el.clientWidth;
  canLeft.value = el.scrollLeft > 1;
  canRight.value = max - el.scrollLeft > 1;
}

useEventListener(scroller, "scroll", update, { passive: true });
useResizeObserver(scroller, update);
onMounted(() => nextTick(update));

function scrollStep(dir: 1 | -1) {
  const el = scroller.value;
  if (!el) return;
  el.scrollBy({ left: dir * el.clientWidth * props.step, behavior: "smooth" });
}
</script>

<template>
  <div class="base-scroller">
    <Transition name="edge">
      <button
        v-show="canLeft"
        class="edge prev"
        type="button"
        aria-label="向左滚动"
        @click="scrollStep(-1)"
      >
        <svg viewBox="0 0 24 24" width="26" height="26" aria-hidden="true">
          <path
            d="M15 5l-7 7 7 7"
            fill="none"
            stroke="currentColor"
            stroke-width="2.2"
            stroke-linecap="round"
            stroke-linejoin="round"
          />
        </svg>
      </button>
    </Transition>

    <div ref="scroller" class="track" :style="{ '--col': props.colWidth, '--gap': props.gap }">
      <slot />
    </div>

    <Transition name="edge">
      <button
        v-show="canRight"
        class="edge next"
        type="button"
        aria-label="向右滚动"
        @click="scrollStep(1)"
      >
        <svg viewBox="0 0 24 24" width="26" height="26" aria-hidden="true">
          <path
            d="M9 5l7 7-7 7"
            fill="none"
            stroke="currentColor"
            stroke-width="2.2"
            stroke-linecap="round"
            stroke-linejoin="round"
          />
        </svg>
      </button>
    </Transition>
  </div>
</template>

<style scoped lang="scss">
.base-scroller {
  position: relative;
}

.track {
  display: grid;
  grid-auto-flow: column;
  grid-auto-columns: var(--col);
  gap: var(--gap);
  overflow-x: auto;
  // 上下 + 左右留白：卡片 hover 上移/放大时边框与阴影不被 overflow 裁掉，
  // 同时让首/尾卡片不贴容器边（横向 padding 在滚动网格里首尾都生效）。
  padding: 6px 14px;
  scroll-behavior: smooth;
  // 隐藏横向滚动条
  scrollbar-width: none;
  -ms-overflow-style: none;

  &::-webkit-scrollbar {
    display: none;
  }
}

// 两端半透明渐变遮罩 + 居中矢量箭头
.edge {
  position: absolute;
  top: 6px;
  bottom: 6px;
  z-index: 2;
  width: 64px;
  padding: 0;
  border: 0;
  display: flex;
  align-items: center;
  color: var(--fbz-color-text);
  cursor: pointer;
  background: transparent;
  transition: opacity var(--fbz-motion-fast);

  svg {
    transition:
      color var(--fbz-motion-fast),
      transform var(--fbz-motion-fast);
    filter: drop-shadow(0 2px 4px rgba(0, 0, 0, 0.3));
  }

  &:hover svg {
    color: var(--fbz-color-brand-500);
    transform: scale(1.12);
  }

  &.prev {
    left: 0;
    justify-content: flex-start;
    padding-left: 4px;
    background: linear-gradient(
      90deg,
      var(--fbz-color-bg) 0%,
      color-mix(in srgb, var(--fbz-color-bg) 72%, transparent) 45%,
      color-mix(in srgb, var(--fbz-color-bg) 0%, transparent) 100%
    );
  }

  &.next {
    right: 0;
    justify-content: flex-end;
    padding-right: 4px;
    background: linear-gradient(
      270deg,
      var(--fbz-color-bg) 0%,
      color-mix(in srgb, var(--fbz-color-bg) 72%, transparent) 45%,
      color-mix(in srgb, var(--fbz-color-bg) 0%, transparent) 100%
    );
  }
}

// 遮罩淡入淡出
.edge-enter-active,
.edge-leave-active {
  transition: opacity var(--fbz-motion-base);
}

.edge-enter-from,
.edge-leave-to {
  opacity: 0;
}

// 触摸设备没有 hover、用原生滑动，隐藏遮罩箭头
@media (hover: none) {
  .edge {
    display: none;
  }
}
</style>
```

## BaseSelect

- Path: `src/components/BaseSelect.vue`
- Description: daisyUI select bound via defineModel; typed options list.
- Key props: options, size?=md, ariaLabel?, placeholder?; v-model

```vue
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
```

## GlobalToast

- Path: `src/components/GlobalToast.vue`
- Description: Fixed top-right toast stack driven by uiStore.toasts.
- Key props: (store-driven)

```vue
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
```

## AdminPageShell

- Path: `src/components/admin/AdminPageShell.vue`
- Description: Admin page wrapper: optional title banner + content slot, max-width 1200px.
- Key props: title?, description?

```vue
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
```
