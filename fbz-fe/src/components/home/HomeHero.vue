<script setup lang="ts">
import type { FeaturedItem } from "@/types/media.ts";

interface Props {
  items: FeaturedItem[];
  /** 自动轮播间隔（ms），0 表示不自动 */
  interval?: number;
}

const props = withDefaults(defineProps<Props>(), {
  interval: 6000,
});

const current = ref(0);
const active = computed(() => props.items[current.value]);

let timer: ReturnType<typeof setInterval> | undefined;

function go(i: number) {
  current.value = (i + props.items.length) % props.items.length;
}

function restart() {
  if (timer) clearInterval(timer);
  if (props.interval > 0 && props.items.length > 1) {
    timer = setInterval(() => go(current.value + 1), props.interval);
  }
}

function select(i: number) {
  go(i);
  restart();
}

onMounted(restart);
onBeforeUnmount(() => timer && clearInterval(timer));

// 蜂巢：拆两列，偶数列错位下移
const colA = computed(() =>
  props.items.map((item, i) => ({ item, i })).filter(({ i }) => i % 2 === 0),
);
const colB = computed(() =>
  props.items.map((item, i) => ({ item, i })).filter(({ i }) => i % 2 === 1),
);
</script>

<template>
  <section class="hero">
    <div class="slides">
      <div
        v-for="(item, i) in props.items"
        :key="item.id"
        class="slide"
        :class="{ active: i === current }"
      >
        <img v-if="item.backdrop" :src="item.backdrop" :alt="item.title" />
        <span v-else class="slide-ph">{{ item.title }}</span>
      </div>
    </div>
    <div class="scrim" />

    <div class="content">
      <p class="eyebrow">最新入库</p>
      <h1 class="title">{{ active.title }}</h1>
      <div class="meta">
        <template v-for="(m, i) in active.meta" :key="m">
          <span>{{ m }}</span>
          <span v-if="i < active.meta.length - 1" class="dot" />
        </template>
        <span class="tags">
          <span v-for="t in active.tags" :key="t" class="tag">{{ t }}</span>
        </span>
      </div>
      <p class="overview">{{ active.overview }}</p>
      <div class="actions">
        <button class="btn btn-play">▶ 播放</button>
        <button class="btn btn-ghost">详情</button>
      </div>
    </div>

    <div class="hive">
      <div class="hive-col">
        <button
          v-for="{ item, i } in colA"
          :key="item.id"
          class="hive-cell"
          :class="{ active: i === current }"
          @click="select(i)"
        >
          <img v-if="item.thumb" :src="item.thumb" :alt="item.title" loading="lazy" />
          <span v-else class="cell-ph" aria-hidden="true" />
        </button>
      </div>
      <div class="hive-col offset">
        <button
          v-for="{ item, i } in colB"
          :key="item.id"
          class="hive-cell"
          :class="{ active: i === current }"
          @click="select(i)"
        >
          <img v-if="item.thumb" :src="item.thumb" :alt="item.title" loading="lazy" />
          <span v-else class="cell-ph" aria-hidden="true" />
        </button>
      </div>
    </div>

    <div class="scroll-hint">向下滚动 ↓</div>
  </section>
</template>

<style scoped lang="scss">
.hero {
  position: relative;
  height: 86vh;
  min-height: 560px;
  display: flex;
  align-items: center;
  padding: 0 var(--fbz-space-8);
  overflow: hidden;
  background: var(--fbz-color-bg-strong);
}

.slides {
  position: absolute;
  inset: 0;
  z-index: 0;
}

.slide {
  position: absolute;
  inset: 0;
  opacity: 0;
  transition: opacity var(--fbz-motion-slow) ease;

  &.active {
    opacity: 1;
  }

  img {
    width: 100%;
    height: 100%;
    object-fit: cover;
    object-position: center 28%;
  }
}

.slide-ph {
  position: absolute;
  inset: 0;
  display: grid;
  place-content: center;
  font-size: 80px;
  font-weight: 800;
  letter-spacing: 4px;
  color: rgba(255, 255, 255, 0.06);
}

.scrim {
  position: absolute;
  inset: 0;
  z-index: 1;
  background:
    linear-gradient(90deg, rgba(0, 0, 0, 0.9) 0%, rgba(0, 0, 0, 0.45) 45%, rgba(0, 0, 0, 0.1) 72%),
    linear-gradient(0deg, var(--fbz-color-bg) 0%, rgba(0, 0, 0, 0.4) 22%, rgba(0, 0, 0, 0) 50%);
}

.content {
  position: relative;
  z-index: 2;
  max-width: 540px;
}

.eyebrow {
  margin: 0 0 var(--fbz-space-3);
  font-size: var(--fbz-font-size-sm);
  font-weight: 700;
  letter-spacing: 2px;
  text-transform: uppercase;
  color: var(--fbz-color-brand-500);
}

.title {
  margin: 0 0 var(--fbz-space-3);
  font-size: 48px;
  line-height: 1.06;
  font-weight: 800;
}

.meta {
  display: flex;
  align-items: center;
  flex-wrap: wrap;
  gap: var(--fbz-space-3);
  margin-bottom: var(--fbz-space-4);
  font-size: var(--fbz-font-size-md);
  color: var(--fbz-color-text-soft);

  .dot {
    width: 3px;
    height: 3px;
    border-radius: 50%;
    background: var(--fbz-color-text-muted);
  }

  .tags {
    display: inline-flex;
    gap: var(--fbz-space-2);
    margin-left: var(--fbz-space-2);
  }

  .tag {
    padding: 2px 7px;
    border: 1px solid var(--fbz-color-line);
    border-radius: 3px;
    font-size: var(--fbz-font-size-xs);
    letter-spacing: 0.5px;
    color: var(--fbz-color-text-soft);
  }
}

.overview {
  margin: 0 0 var(--fbz-space-6);
  max-width: 480px;
  font-size: var(--fbz-font-size-md);
  line-height: 1.65;
  color: var(--fbz-color-text-soft);
  display: -webkit-box;
  -webkit-line-clamp: 2;
  line-clamp: 2;
  -webkit-box-orient: vertical;
  overflow: hidden;
}

.actions {
  display: flex;
  gap: var(--fbz-space-3);
}

.btn {
  height: 44px;
  padding: 0 22px;
  border: 1px solid transparent;
  border-radius: var(--fbz-radius-control);
  font-size: var(--fbz-font-size-md);
  font-weight: 700;
  display: inline-flex;
  align-items: center;
  gap: 9px;
  transition:
    background var(--fbz-motion-fast),
    border-color var(--fbz-motion-fast),
    color var(--fbz-motion-fast);
}

.btn-play {
  color: #07120a;
  background: var(--fbz-color-brand-500);

  &:hover {
    background: var(--fbz-color-brand-600);
  }
}

.btn-ghost {
  color: #fff;
  background: rgba(255, 255, 255, 0.08);
  border-color: var(--fbz-color-line);

  &:hover {
    background: rgba(255, 255, 255, 0.16);
  }
}

.hive {
  position: absolute;
  z-index: 2;
  right: var(--fbz-space-8);
  top: 50%;
  transform: translateY(-50%);
  display: flex;
  gap: 10px;
}

.hive-col {
  display: flex;
  flex-direction: column;
  gap: 10px;

  &.offset {
    margin-top: 46px;
  }
}

.hive-cell {
  position: relative;
  width: 76px;
  height: 84px;
  padding: 0;
  border-radius: var(--fbz-radius-card);
  overflow: hidden;
  border: 1px solid var(--fbz-color-line);
  background: var(--fbz-color-panel);
  opacity: 0.55;
  transition:
    opacity var(--fbz-motion-base),
    border-color var(--fbz-motion-base),
    transform var(--fbz-motion-base);

  &:hover {
    opacity: 0.9;
  }

  &.active {
    opacity: 1;
    border-color: var(--fbz-color-brand-500);
    transform: scale(1.06);
  }

  img {
    display: block;
    width: 100%;
    height: 100%;
    object-fit: cover;
  }
}

.cell-ph {
  position: absolute;
  inset: 0;
  display: block;
  background:
    linear-gradient(145deg, rgba(255, 255, 255, 0.08), rgba(255, 255, 255, 0)),
    var(--fbz-color-panel-strong);
}

.scroll-hint {
  position: absolute;
  left: 50%;
  bottom: 18px;
  transform: translateX(-50%);
  z-index: 2;
  font-size: var(--fbz-font-size-xs);
  letter-spacing: 1px;
  color: var(--fbz-color-text-muted);
  animation: bob 1.8s ease-in-out infinite;
}

@keyframes bob {
  0%,
  100% {
    transform: translate(-50%, 0);
  }
  50% {
    transform: translate(-50%, 7px);
  }
}

@media (max-width: 1024px) {
  .title {
    font-size: 38px;
  }

  .hive-cell {
    width: 64px;
    height: 72px;
  }

  .hive-col.offset {
    margin-top: 40px;
  }
}

@media (max-width: 600px) {
  .hero {
    height: auto;
    min-height: 0;
    padding: calc(var(--header-h, 56px) + 80px) var(--fbz-space-4) var(--fbz-space-6);
    flex-direction: column;
    align-items: stretch;
  }

  .content {
    max-width: 100%;
  }

  .title {
    font-size: 30px;
  }

  .overview {
    -webkit-line-clamp: 3;
    line-clamp: 3;
  }

  .actions .btn {
    flex: 1;
    justify-content: center;
  }

  .hive {
    position: static;
    transform: none;
    margin-top: var(--fbz-space-5);
    justify-content: center;
    gap: 8px;
    width: 100%;
  }

  .hive-col {
    flex-direction: row;
    gap: 8px;

    &.offset {
      margin-top: 0;
    }
  }

  .hive-cell {
    width: 48px;
    height: 64px;
  }

  .scroll-hint {
    display: none;
  }
}
</style>
