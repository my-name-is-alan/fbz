<script setup lang="ts">
import type { ContinueItem } from "@/types/media.ts";
import { usePlaybackStore } from "@/stores/playback.ts";
import { useUiStore } from "@/stores/ui.ts";

interface Props {
  item: ContinueItem;
  layout?: "poster" | "wide";
  showResolution?: boolean;
  showRating?: boolean;
  /** 占位块色块交替 */
  variant?: 0 | 1;
  /** 自定义副标题，若提供则优先显示 */
  subtitle?: string;
}

const props = withDefaults(defineProps<Props>(), {
  layout: "poster",
  showResolution: true,
  showRating: true,
  variant: 0,
});

const router = useRouter();
const playback = usePlaybackStore();

const ratio = computed(() => (props.layout === "wide" ? "wide" : "poster"));
const detailType = computed(
  () => props.item.detailType ?? (props.item.libraryId === "series" ? "tv" : "movie"),
);

// 详情页类型路径：优先用 detailType；缺省时按库 id 推断（剧集库→tv）
const to = computed(() => `/${detailType.value}/${props.item.id}`);

const rating = computed(() => (props.item.rating != null ? props.item.rating.toFixed(1) : null));

// 副标题：优先使用传入的 subtitle，否则用年份或 meta 文案
const subtitle = computed(
  () => props.subtitle ?? (props.item.year != null ? String(props.item.year) : props.item.meta),
);

// 清晰度徽章：统一弱化为黑色半透明小角标
const resolution = computed(() => props.item.resolution);

function goDetail() {
  router.push(to.value);
}

async function goPlayback() {
  // 剧集卡片：播放目标是具体分集，进详情页由「继续观看」逻辑接管。
  if (detailType.value === "tv") {
    goDetail();
    return;
  }

  // 电影卡片：先取真实流地址再开播放器，避免空播放器。
  const { fetchPlaybackSource } = await import("@/service/modules/detail.ts");
  const source = await fetchPlaybackSource(String(props.item.id));
  playback.open({
    type: detailType.value,
    id: String(props.item.id),
    title: props.item.title,
    subtitle: subtitle.value,
    poster: props.item.poster,
    tags: resolution.value ? [resolution.value] : undefined,
    source: source ? { uri: source.uri, mimeType: source.mimeType } : undefined,
  });
}

const uiStore = useUiStore();

function onContextMenu(e: MouseEvent) {
  uiStore.openContextMenu(e.clientX, e.clientY, props.item as any);
}
</script>

<template>
  <article
    class="media-card"
    role="link"
    tabindex="0"
    @click="goDetail"
    @contextmenu.prevent="onContextMenu"
    @keydown.enter.self.prevent="goDetail"
    @keydown.space.self.prevent="goDetail"
  >
    <div class="thumb">
      <MediaPoster
        :src="props.item.poster"
        :title="props.item.title"
        :ratio="ratio"
        :variant="props.variant"
      />

      <button
        class="play-overlay"
        type="button"
        :aria-label="`播放 ${props.item.title}`"
        @click.stop="goPlayback"
      >
        <svg class="play-icon" viewBox="0 0 24 24" width="18" height="18" fill="currentColor">
          <path d="M8 5v14l11-7z" />
        </svg>
      </button>

      <!-- 角标/进度只是卡片叠层，不参与飞渡 -->
      <span v-if="props.showResolution && resolution" class="res-badge">
        {{ resolution }}
      </span>
      <div v-if="props.item.progress != null" class="progress">
        <span :style="{ width: `${props.item.progress}%` }" />
      </div>
    </div>

    <div class="footer">
      <h3 class="title" :title="props.item.title">{{ props.item.title }}</h3>
      <div class="meta">
        <span class="subtitle">{{ subtitle }}</span>
        <span v-if="props.showRating && rating" class="rating">★ {{ rating }}</span>
      </div>
    </div>
  </article>
</template>

<style scoped lang="scss">
// Emby 式「悬浮海报卡」：卡片本体透明无边框，海报块承担圆角 + 阴影，
// 悬停整体上浮、阴影加深并浮出播放按钮；文字区透明置于海报下方。
.media-card {
  display: block;
  cursor: pointer;
  text-decoration: none;
  color: inherit;
  outline: none;

  &:hover,
  &:focus-visible,
  &:focus-within {
    .thumb {
      box-shadow: var(--fbz-shadow-card-hover);
      transform: translateY(-4px) scale(1.015);
    }

    .thumb :deep(img) {
      transform: scale(1.045);
    }

    .title {
      color: var(--fbz-color-brand-500);
    }
  }

  &:focus-visible .thumb {
    box-shadow:
      var(--fbz-shadow-card-hover),
      0 0 0 2px var(--fbz-color-brand-500);
  }

  &:hover .play-overlay,
  &:focus-within .play-overlay {
    opacity: 1;
    transform: translate(-50%, -50%) scale(1);
  }
}

.thumb {
  position: relative;
  overflow: hidden;
  border-radius: var(--fbz-radius-card);
  background: var(--fbz-color-panel);
  box-shadow: var(--fbz-shadow-card);
  transition:
    box-shadow var(--fbz-motion-base),
    transform var(--fbz-motion-base);

  :deep(.media-poster) {
    border-radius: 0;
  }

  :deep(img) {
    transition: transform var(--fbz-motion-slow) ease;
    will-change: transform;
  }
}

.play-overlay {
  position: absolute;
  z-index: 3;
  left: 50%;
  top: 50%;
  width: 48px;
  height: 48px;
  display: grid;
  place-content: center;
  border-radius: 50%;
  border: 0;
  background: var(--fbz-color-brand-500);
  color: #07120a;
  box-shadow: 0 8px 24px rgba(0, 0, 0, 0.45);
  opacity: 0;
  transform: translate(-50%, -50%) scale(0.88);
  transition:
    opacity var(--fbz-motion-base),
    transform var(--fbz-motion-base),
    background var(--fbz-motion-fast);

  &:hover {
    background: var(--fbz-color-brand-600);
    transform: translate(-50%, -50%) scale(1.08);
  }
}

.play-icon {
  margin-left: 2px;
  display: flex;
}

.rating {
  flex: 0 0 auto;
  display: inline-flex;
  align-items: center;
  gap: 2px;
  color: var(--fbz-color-amber-500);
  font-size: var(--fbz-font-size-xs);
  font-weight: 700;
  font-variant-numeric: tabular-nums;
  letter-spacing: 0;
}

.progress {
  position: absolute;
  inset: auto 0 0 0;
  z-index: 2;
  height: 4px;
  background: rgba(255, 255, 255, 0.18);

  span {
    display: block;
    height: 100%;
    border-radius: 0 2px 2px 0;
    background: var(--fbz-color-brand-500);
  }
}

.footer {
  padding: 10px 2px 2px;
}

.title {
  margin: 0 0 2px;
  font-size: 14px;
  font-weight: 600;
  line-height: 1.35;
  text-align: left;
  color: var(--fbz-color-text);
  white-space: nowrap;
  overflow: hidden;
  text-overflow: ellipsis;
  transition: color var(--fbz-motion-fast);
}

// 副标题 + 评分
.meta {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: var(--fbz-space-2);
}

.subtitle {
  font-size: var(--fbz-font-size-sm);
  color: var(--fbz-color-text-muted);
  font-variant-numeric: tabular-nums;
  white-space: nowrap;
  overflow: hidden;
  text-overflow: ellipsis;
}

.res-badge {
  position: absolute;
  z-index: 2;
  top: 8px;
  right: 8px;
  flex: 0 0 auto;
  padding: 2px 7px;
  border-radius: var(--fbz-radius-round);
  border: 0;
  background: rgb(0 0 0 / 66%);
  color: rgb(255 255 255 / 88%);
  backdrop-filter: blur(8px);
  -webkit-backdrop-filter: blur(8px);
  font-size: 10px;
  font-weight: 700;
  letter-spacing: 0.3px;
  line-height: 1.6;
}

@media (hover: none) {
  .play-overlay {
    opacity: 1;
    transform: translate(-50%, -50%) scale(1);
  }
}
</style>
