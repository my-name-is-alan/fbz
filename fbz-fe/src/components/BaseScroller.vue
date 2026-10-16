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
        <span class="edge-ic"><BaseIcon name="left" :size="18" /></span>
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
        <span class="edge-ic"><BaseIcon name="right" :size="18" /></span>
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

  .edge-ic {
    width: 32px;
    height: 32px;
    display: grid;
    place-items: center;
    border: 1px solid var(--fbz-color-line-bright);
    border-radius: 50%;
    background: color-mix(in srgb, var(--fbz-color-panel-strong) 88%, transparent);
    box-shadow: 0 6px 18px rgb(0 0 0 / 35%);
    transition:
      color var(--fbz-motion-fast),
      border-color var(--fbz-motion-fast),
      transform var(--fbz-motion-fast);
  }

  &:hover .edge-ic {
    color: var(--fbz-color-brand-500);
    border-color: color-mix(in srgb, var(--fbz-color-brand-500) 60%, transparent);
    transform: scale(1.06);
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
