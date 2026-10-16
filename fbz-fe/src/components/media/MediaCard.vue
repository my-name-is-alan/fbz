<script setup lang="ts">
import type { ContinueItem } from "@/types/media.ts";
import { usePlaybackStore } from "@/stores/playback.ts";
import { useUiStore } from "@/stores/ui.ts";
import { detailShellFor, episodeCode, itemRoute, itemTypeMeta } from "@/utils/itemType.ts";

/**
 * 统一媒体卡片（JF / Emby 气质）：海报 + 复合状态角标。
 * - 左上：已看（Lucide Check）/ 收藏（心形）
 * - 右上：分辨率 · 动态范围 · 多版本（只在数据存在时出现，不造假）
 * - 底部：观看进度条
 * 卡片主体进详情；海报播放键直接起播（电影 / 视频 / 单集），容器类条目进详情。
 */
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
const uiStore = useUiStore();

/** 真实 Emby Type 优先；旧数据只有 detailType 时按其推断。 */
const itemType = computed(() => {
  if (props.item.itemType) return props.item.itemType;
  if (props.item.detailType === "tv") return "Series";
  if (props.item.detailType === "collection") return "BoxSet";
  return props.item.libraryId === "series" ? "Series" : "Movie";
});
const typeMeta = computed(() => itemTypeMeta(itemType.value));
const to = computed(() => itemRoute(itemType.value, String(props.item.id)));

/** 可直接起播：电影 / 视频类 / 单集。剧集、合集、专辑等容器进详情。 */
const directPlayable = computed(() =>
  ["Movie", "Video", "MusicVideo", "Trailer", "Episode"].includes(itemType.value),
);

const rating = computed(() =>
  props.item.rating != null && props.item.rating > 0 ? props.item.rating.toFixed(1) : null,
);

const epCode = computed(() =>
  itemType.value === "Episode"
    ? episodeCode(props.item.seasonNumber, props.item.episodeNumber)
    : "",
);

const subtitle = computed(
  () => props.subtitle ?? (props.item.year != null ? String(props.item.year) : props.item.meta),
);

const resolution = computed(() => (props.showResolution ? props.item.resolution : undefined));
const versionCount = computed(() => props.item.versionCount ?? 0);
const resuming = computed(
  () => !props.item.isPlayed && props.item.progress != null && props.item.progress > 0,
);

function goDetail() {
  void router.push(to.value);
}

async function goPlayback() {
  if (!directPlayable.value) {
    goDetail();
    return;
  }
  // 先取真实流地址再开播放器，避免空播放器。
  const { fetchPlaybackSource } = await import("@/service/modules/detail.ts");
  const source = await fetchPlaybackSource(String(props.item.id));
  if (!source) {
    uiStore.showToast("该条目暂无可播放媒体源。", "warning");
    return;
  }
  playback.open({
    type: itemType.value === "Episode" ? "episode" : "movie",
    id: String(props.item.id),
    title: props.item.title,
    subtitle: subtitle.value,
    poster: props.item.poster,
    tags: resolution.value ? [resolution.value] : undefined,
    duration: source.duration,
    // 有进度则交给 server.startTicks 续播；否则显式从头。
    startPositionSeconds: resuming.value ? undefined : 0,
    source,
    server: source.server,
    mediaSources: source.mediaSources,
    chapters: source.chapters,
    subtitleTracks: source.subtitleTracks,
    mediaInfo: source.mediaInfo,
  });
}

function onContextMenu(e: MouseEvent) {
  uiStore.openContextMenu(e.clientX, e.clientY, props.item);
}

const playLabel = computed(() => {
  if (!directPlayable.value) return `打开 ${props.item.title}`;
  return `${resuming.value ? "继续播放" : "播放"} ${props.item.title}`;
});
const shell = computed(() => detailShellFor(itemType.value));
</script>

<template>
  <article
    class="media-card"
    :class="`is-${props.layout}`"
    role="link"
    tabindex="0"
    :aria-label="`${typeMeta.label}：${props.item.title}`"
    @click="goDetail"
    @contextmenu.prevent="onContextMenu"
    @keydown.enter.self.prevent="goDetail"
    @keydown.space.self.prevent="goDetail"
  >
    <div class="thumb">
      <MediaPoster
        :src="props.item.poster"
        :title="props.item.title"
        :ratio="props.layout === 'wide' ? 'wide' : 'poster'"
        :variant="props.variant"
        :icon="typeMeta.icon"
      />
      <div class="scrim" />

      <!-- 左上：用户状态 -->
      <div class="corner tl">
        <span v-if="props.item.isPlayed" class="state-badge played" title="已看完">
          <BaseIcon name="check" :size="13" :stroke-width="3" />
        </span>
        <span v-if="props.item.isFavorite" class="state-badge fav" title="已收藏">
          <BaseIcon name="heart" :size="12" fill :stroke-width="2" />
        </span>
      </div>

      <!-- 右上：技术规格（复合角标） -->
      <div class="corner tr">
        <span v-if="resolution" class="spec-badge">{{ resolution }}</span>
        <span v-if="props.item.dynamicRange" class="spec-badge hdr">{{
          props.item.dynamicRange
        }}</span>
        <span v-if="versionCount > 1" class="spec-badge" :title="`${versionCount} 个版本`">
          <BaseIcon name="versions" :size="11" :stroke-width="2.2" />{{ versionCount }}
        </span>
        <span
          v-if="shell !== 'movie' && shell !== 'tv'"
          class="spec-badge kind"
          :title="typeMeta.label"
        >
          <BaseIcon :name="typeMeta.icon" :size="11" :stroke-width="2.2" />
        </span>
      </div>

      <span v-if="epCode" class="ep-badge">{{ epCode }}</span>

      <button class="play-btn" type="button" :aria-label="playLabel" @click.stop="goPlayback">
        <BaseIcon
          :name="directPlayable ? 'play' : 'right'"
          :size="18"
          :fill="directPlayable"
          :stroke-width="2"
        />
      </button>

      <div v-if="props.item.progress != null && !props.item.isPlayed" class="progress">
        <span :style="{ width: `${Math.max(2, props.item.progress)}%` }" />
      </div>
    </div>

    <div class="footer">
      <h3 class="title" :title="props.item.title">{{ props.item.title }}</h3>
      <div class="meta">
        <span class="subtitle">{{ subtitle }}</span>
        <span v-if="props.showRating && rating" class="rating">
          <BaseIcon name="star" :size="11" fill :stroke-width="2" />{{ rating }}
        </span>
      </div>
    </div>
  </article>
</template>

<style scoped lang="scss">
.media-card {
  display: block;
  min-width: 0;
  color: inherit;
  text-decoration: none;
  cursor: pointer;
  outline: none;

  &:hover,
  &:focus-visible {
    .thumb {
      border-color: var(--ui-border-strong);
      transform: translateY(-2px);
      box-shadow: var(--ui-shadow-surface);
    }

    .scrim {
      opacity: 1;
    }

    .play-btn {
      opacity: 1;
      transform: translate(-50%, -50%) scale(1);
    }

    .title {
      color: var(--ui-accent-hover);
    }
  }

  &:focus-visible .thumb {
    outline: 2px solid var(--ui-focus);
    outline-offset: 2px;
  }
}

.thumb {
  position: relative;
  overflow: hidden;
  border: 1px solid var(--ui-border);
  border-radius: var(--ui-radius-md);
  background: var(--ui-surface-raised);
  transition:
    transform var(--ui-duration-base) var(--ui-ease-out),
    box-shadow var(--ui-duration-base),
    border-color var(--ui-duration-fast);
}

.scrim {
  position: absolute;
  inset: 0;
  z-index: 1;
  background: linear-gradient(180deg, rgb(0 0 0 / 22%) 0%, transparent 30%, rgb(0 0 0 / 38%) 100%);
  opacity: 0.55;
  transition: opacity var(--ui-duration-fast);
  pointer-events: none;
}

.corner {
  position: absolute;
  z-index: 2;
  top: 6px;
  display: flex;
  gap: 4px;
  pointer-events: none;

  &.tl {
    left: 6px;
  }

  &.tr {
    right: 6px;
    flex-direction: column;
    align-items: flex-end;
  }
}

.state-badge {
  width: 22px;
  height: 22px;
  display: grid;
  place-items: center;
  border-radius: 50%;
  box-shadow: 0 2px 8px rgb(0 0 0 / 45%);

  &.played {
    background: var(--ui-accent);
    color: var(--ui-accent-contrast);
  }

  &.fav {
    background: rgb(0 0 0 / 62%);
    color: #fb7185;
    backdrop-filter: blur(6px);
  }
}

.spec-badge {
  display: inline-flex;
  align-items: center;
  gap: 2px;
  height: 18px;
  padding: 0 5px;
  border-radius: 4px;
  background: rgb(0 0 0 / 66%);
  color: #fff;
  font-size: 10px;
  font-weight: 700;
  letter-spacing: 0.02em;
  line-height: 1;
  backdrop-filter: blur(6px);

  &.hdr {
    color: var(--ui-accent-hover);
  }

  &.kind {
    padding: 0 4px;
  }
}

.ep-badge {
  position: absolute;
  z-index: 2;
  left: 6px;
  bottom: 8px;
  height: 18px;
  padding: 0 5px;
  display: inline-flex;
  align-items: center;
  border-radius: 4px;
  background: rgb(0 0 0 / 66%);
  color: #fff;
  font-family: var(--ui-font-mono);
  font-size: 10px;
  font-weight: 600;
}

.play-btn {
  position: absolute;
  z-index: 3;
  left: 50%;
  top: 50%;
  width: 42px;
  height: 42px;
  display: grid;
  place-items: center;
  padding: 0 0 0 2px;
  border: 0;
  border-radius: 50%;
  background: var(--ui-accent);
  color: var(--ui-accent-contrast);
  box-shadow: 0 6px 18px rgb(0 0 0 / 50%);
  opacity: 0;
  transform: translate(-50%, -50%) scale(0.86);
  transition:
    opacity var(--ui-duration-fast),
    transform var(--ui-duration-fast),
    background var(--ui-duration-fast);
  cursor: pointer;

  &:hover {
    background: var(--ui-accent-hover);
  }
}

.progress {
  position: absolute;
  z-index: 2;
  inset: auto 0 0 0;
  height: 3px;
  background: rgb(255 255 255 / 18%);

  span {
    display: block;
    height: 100%;
    background: var(--ui-accent);
  }
}

.footer {
  padding: 8px 2px 0;
}

.title {
  margin: 0;
  overflow: hidden;
  color: var(--ui-text);
  font-size: 13.5px;
  font-weight: 600;
  line-height: 1.35;
  white-space: nowrap;
  text-overflow: ellipsis;
  transition: color var(--ui-duration-fast);
}

.meta {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 6px;
  margin-top: 2px;
}

.subtitle {
  min-width: 0;
  overflow: hidden;
  color: var(--ui-text-muted);
  font-size: 12px;
  font-variant-numeric: tabular-nums;
  white-space: nowrap;
  text-overflow: ellipsis;
}

.rating {
  flex: 0 0 auto;
  display: inline-flex;
  align-items: center;
  gap: 3px;
  color: var(--ui-warning);
  font-size: 11.5px;
  font-weight: 600;
  font-variant-numeric: tabular-nums;
}

@media (hover: none) {
  .play-btn {
    opacity: 1;
    transform: translate(-50%, -50%) scale(0.9);
  }
}
</style>
