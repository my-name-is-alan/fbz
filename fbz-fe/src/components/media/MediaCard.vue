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
.media-card {
  display: block;
  cursor: pointer;
  text-decoration: none;
  color: inherit;
  overflow: hidden;
  border: 1px solid var(--fbz-color-line-soft);
  border-radius: var(--fbz-radius-card);
  background: linear-gradient(180deg, var(--fbz-color-panel) 0%, var(--fbz-color-bg-strong) 100%);
  box-shadow: 0 12px 28px rgba(0, 0, 0, 0.18);
  transition:
    border-color var(--fbz-motion-fast),
    box-shadow var(--fbz-motion-fast),
    transform var(--fbz-motion-fast);

  &:hover,
  &:focus-visible,
  &:focus-within {
    border-color: var(--fbz-color-brand-500);
    box-shadow:
      0 12px 32px color-mix(in srgb, var(--fbz-color-brand-500) 18%, transparent),
      0 4px 12px rgba(0, 0, 0, 0.4);
    transform: translateY(-4px);
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
  background: var(--fbz-color-panel);

  :deep(.media-poster) {
    border-radius: 0;
  }

  &::after {
    position: absolute;
    inset: auto 0 0;
    z-index: 1;
    height: 34%;
    pointer-events: none;
    content: "";
    background: linear-gradient(
      180deg,
      var(--fbz-color-panel-transparent) 0%,
      var(--fbz-color-panel) 100%
    );
  }
}

.play-overlay {
  position: absolute;
  z-index: 3;
  left: 50%;
  top: 50%;
  width: 44px;
  height: 44px;
  display: grid;
  place-content: center;
  border-radius: 50%;
  border: 1px solid rgba(255, 255, 255, 0.22);
  background: color-mix(in srgb, var(--fbz-color-brand-500) 92%, transparent);
  color: #07120a;
  box-shadow: 0 12px 28px rgba(0, 0, 0, 0.32);
  opacity: 0;
  transform: translate(-50%, -50%) scale(0.92);
  transition:
    opacity var(--fbz-motion-fast),
    transform var(--fbz-motion-fast),
    background var(--fbz-motion-fast);

  &:hover {
    background: var(--fbz-color-brand-600);
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
  color: var(--fbz-color-brand-500);
  font-family: var(--fbz-font-display);
  font-size: var(--fbz-font-size-xs);
  font-weight: 700;
  letter-spacing: 0;
}

.progress {
  position: absolute;
  inset: auto 0 0 0;
  z-index: 2;
  height: 3px;
  background: rgba(255, 255, 255, 0.15);

  span {
    display: block;
    height: 100%;
    background: var(--fbz-color-brand-500);
  }
}

.footer {
  position: relative;
  margin-top: -14px;
  padding: 18px 10px 10px;
  background: linear-gradient(
    180deg,
    var(--fbz-color-panel-transparent) 0%,
    var(--fbz-color-panel) 22%,
    var(--fbz-color-bg-strong) 100%
  );
}

.title {
  margin: 0 0 3px;
  font-family: var(--fbz-font-display);
  font-size: 14px;
  font-weight: 700;
  line-height: 1.3;
  text-align: left;
  letter-spacing: 0;
  white-space: nowrap;
  overflow: hidden;
  text-overflow: ellipsis;
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
  font-family: var(--fbz-font-display);
  color: var(--fbz-color-text-muted);
  letter-spacing: 0;
  white-space: nowrap;
  overflow: hidden;
  text-overflow: ellipsis;
}

.res-badge {
  position: absolute;
  z-index: 2;
  top: 6px;
  right: 6px;
  flex: 0 0 auto;
  padding: 2px 6px;
  border-radius: 3px;
  border: 1px solid rgb(255 255 255 / 14%);
  background: rgb(0 0 0 / 62%);
  color: rgb(255 255 255 / 82%);
  box-shadow: 0 4px 12px rgb(0 0 0 / 22%);
  backdrop-filter: blur(8px);
  -webkit-backdrop-filter: blur(8px);
  font-family: var(--fbz-font-display);
  font-size: 10px;
  font-weight: 600;
  letter-spacing: 0;
  line-height: 1.5;
}

// 桌面沿用 HDHive 式紧凑媒体标题，不随容器过度放大
@media (min-width: 768px) {
  .title {
    font-size: 14px;
    font-weight: 700;
  }
}

@media (hover: none) {
  .play-overlay {
    opacity: 1;
    transform: translate(-50%, -50%) scale(1);
  }
}
</style>
