<script setup lang="ts">
import type { FeaturedItem } from "@/types/media.ts";
import { usePlaybackStore } from "@/stores/playback.ts";
import { useUiStore } from "@/stores/ui.ts";

/**
 * 首页英雄区：BFF featured（最新入库主打）轮播。横幅 + 渐变遮罩 + 规格徽章 + 缩略图切换条。
 */
interface Props {
  items: FeaturedItem[];
  /** 自动轮播间隔（ms），0 表示不自动 */
  interval?: number;
}

const props = withDefaults(defineProps<Props>(), {
  interval: 7000,
});

const router = useRouter();
const playback = usePlaybackStore();
const uiStore = useUiStore();

const current = ref(0);
const active = computed(() => props.items[current.value] ?? props.items[0]);
const busy = ref(false);

let timer: ReturnType<typeof setInterval> | undefined;

function go(i: number) {
  if (!props.items.length) return;
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
watch(
  () => props.items.length,
  () => {
    if (current.value >= props.items.length) current.value = 0;
    restart();
  },
);

const detailPath = computed(() =>
  active.value ? `/${active.value.detailType ?? "movie"}/${active.value.id}` : "/",
);

async function playActive() {
  const item = active.value;
  if (!item || busy.value) return;
  busy.value = true;
  try {
    const { itemDetail, preparePlayback } = await import("@/service/modules/server.ts");
    const detail = await itemDetail(String(item.id));
    if (detail.Type === "Series" || detail.Type === "Season" || detail.Type === "BoxSet") {
      await router.push(detail.Type === "BoxSet" ? `/collection/${detail.Id}` : `/tv/${detail.Id}`);
      return;
    }
    playback.open(await preparePlayback(detail));
  } catch {
    uiStore.showToast("无法加载播放源，请稍后重试", "error");
  } finally {
    busy.value = false;
  }
}
</script>

<template>
  <section v-if="active" class="hero" aria-roledescription="carousel" aria-label="最新入库">
    <div class="slides">
      <div
        v-for="(item, i) in props.items"
        :key="item.id"
        class="slide"
        :class="{ active: i === current }"
        :aria-hidden="i !== current"
      >
        <img v-if="item.backdrop" :src="item.backdrop" :alt="item.title" />
        <div v-else class="slide-ph">
          <BaseIcon name="clapperboard" :size="64" :stroke-width="1.2" />
        </div>
      </div>
    </div>
    <div class="scrim" />

    <div class="content">
      <span class="eyebrow"><BaseIcon name="sparkles" :size="12" />最新入库</span>
      <h1 class="title">{{ active.title }}</h1>
      <div class="meta c-meta">
        <template v-for="(m, i) in active.meta" :key="`${m}-${i}`">
          <span v-if="i > 0" class="c-dot" />
          <span>{{ m }}</span>
        </template>
        <span v-for="t in active.tags" :key="t" class="spec">{{ t }}</span>
      </div>
      <p v-if="active.overview" class="overview">{{ active.overview }}</p>
      <div class="actions">
        <button
          class="du-btn du-btn-primary hero-btn"
          type="button"
          :disabled="busy"
          @click="playActive"
        >
          <BaseIcon
            :name="busy ? 'loading' : 'play'"
            :size="16"
            :fill="!busy"
            :class="{ 'c-spin': busy }"
          />
          播放
        </button>
        <RouterLink class="du-btn du-btn-outline hero-btn ghost" :to="detailPath">
          <BaseIcon name="info" :size="16" />详情
        </RouterLink>
      </div>
    </div>

    <div v-if="props.items.length > 1" class="thumbs" role="tablist" aria-label="切换主打">
      <button
        v-for="(item, i) in props.items"
        :key="item.id"
        type="button"
        role="tab"
        class="thumb"
        :class="{ active: i === current }"
        :aria-selected="i === current"
        :aria-label="item.title"
        @click="select(i)"
      >
        <img v-if="item.thumb" :src="item.thumb" alt="" loading="lazy" />
        <BaseIcon v-else name="movie" :size="16" />
      </button>
    </div>
  </section>
</template>

<style scoped lang="scss">
.hero {
  position: relative;
  height: 400px;
  display: flex;
  align-items: flex-end;
  overflow: hidden;
  border: 1px solid var(--ui-border);
  border-radius: var(--ui-radius-lg);
  background: var(--ui-surface-raised);
}

.slides {
  position: absolute;
  inset: 0;
}

.slide {
  position: absolute;
  inset: 0;
  opacity: 0;
  transition: opacity 600ms var(--ui-ease-out);

  &.active {
    opacity: 1;
  }

  img {
    width: 100%;
    height: 100%;
    object-fit: cover;
    object-position: center 25%;
  }
}

.slide-ph {
  width: 100%;
  height: 100%;
  display: grid;
  place-items: center;
  color: var(--ui-border-strong);
}

.scrim {
  position: absolute;
  inset: 0;
  background:
    linear-gradient(
      90deg,
      rgb(var(--ui-scrim) / 96%) 0%,
      rgb(var(--ui-scrim) / 62%) 42%,
      rgb(var(--ui-scrim) / 0%) 100%
    ),
    linear-gradient(0deg, rgb(var(--ui-scrim) / 100%) 0%, rgb(var(--ui-scrim) / 0%) 55%);
}

.content {
  position: relative;
  z-index: 1;
  width: 100%;
  max-width: 640px;
  padding: 32px;
}

.eyebrow {
  display: inline-flex;
  align-items: center;
  gap: 4px;
  height: 20px;
  padding: 0 7px;
  margin-bottom: 12px;
  border-radius: 4px;
  background: var(--ui-accent);
  color: var(--ui-accent-contrast);
  font-size: 11px;
  font-weight: 700;
  letter-spacing: 0.04em;
}

.title {
  margin: 0 0 8px;
  color: var(--ui-text);
  font-size: clamp(26px, 3.4vw, 38px);
  font-weight: 700;
  line-height: 1.15;
  letter-spacing: -0.02em;
}

.meta {
  display: flex;
  flex-wrap: wrap;
  align-items: center;
  gap: 8px;
  margin-bottom: 12px;
}

.spec {
  height: 18px;
  padding: 0 6px;
  display: inline-flex;
  align-items: center;
  border: 1px solid var(--ui-border-strong);
  border-radius: 4px;
  color: var(--ui-text-muted);
  font-size: 10.5px;
  font-weight: 700;
}

.overview {
  display: -webkit-box;
  margin: 0 0 20px;
  overflow: hidden;
  color: var(--ui-text-muted);
  font-size: 13.5px;
  line-height: 1.65;
  -webkit-line-clamp: 3;
  -webkit-box-orient: vertical;
}

.actions {
  display: flex;
  gap: 10px;
}

.hero-btn {
  height: 40px;
  min-height: 40px;
  padding: 0 20px;
  border-radius: var(--ui-radius-md);
  text-decoration: none;

  &.ghost {
    border-color: var(--ui-border-strong);
    color: var(--ui-text);
    background: transparent;

    &:hover {
      background: var(--ui-surface-hover);
      border-color: var(--ui-border-strong);
    }
  }
}

.thumbs {
  position: absolute;
  z-index: 2;
  right: 20px;
  bottom: 20px;
  display: flex;
  gap: 8px;
}

.thumb {
  width: 72px;
  aspect-ratio: 16 / 9;
  display: grid;
  place-items: center;
  overflow: hidden;
  padding: 0;
  border: 1px solid var(--ui-glass-border);
  border-radius: var(--ui-radius-sm);
  background: var(--ui-surface-raised);
  color: var(--ui-text-subtle);
  opacity: 0.55;
  cursor: pointer;
  transition:
    opacity var(--ui-duration-fast),
    border-color var(--ui-duration-fast);

  img {
    width: 100%;
    height: 100%;
    object-fit: cover;
  }

  &:hover {
    opacity: 0.9;
  }

  &.active {
    opacity: 1;
    border-color: var(--ui-accent);
    box-shadow: 0 0 0 1px var(--ui-accent);
  }
}

@media (max-width: 900px) {
  .thumbs {
    display: none;
  }
}

@media (max-width: 600px) {
  .hero {
    height: 340px;
  }

  .content {
    padding: 20px;
  }
}
</style>
