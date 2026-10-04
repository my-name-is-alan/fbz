<script setup lang="ts">
import type { ContinueItem } from "@/types/media.ts";
import { preparePlayback, seriesEpisodes } from "@/service/modules/server.ts";
import { usePlaybackStore } from "@/stores/playback.ts";
import { useUiStore } from "@/stores/ui.ts";
const props = withDefaults(
  defineProps<{
    item: ContinueItem;
    layout?: "poster" | "wide";
    showResolution?: boolean;
    showRating?: boolean;
    variant?: 0 | 1;
    subtitle?: string;
    compact?: boolean;
  }>(),
  { layout: "poster", showResolution: true, showRating: true, variant: 0 },
);
const playback = usePlaybackStore();
const ui = useUiStore();
const busy = ref(false);
const to = computed(() =>
  props.item.serverItem?.Type === "Episode" && props.item.serverItem.SeriesId
    ? `/tv/${props.item.serverItem.SeriesId}`
    : `/${props.item.detailType === "tv" ? "tv" : "movie"}/${props.item.id}`,
);
const subtitle = computed(() => props.subtitle ?? props.item.meta ?? String(props.item.year ?? ""));
async function play() {
  if (busy.value) return;
  busy.value = true;
  try {
    let item = props.item.serverItem;
    if (!item) {
      ui.showToast("此条目尚未连接媒体服务器", "info");
      return;
    }
    if (item.Type === "Series") {
      const episodes = await seriesEpisodes(item.Id);
      item =
        episodes.Items.find((e) => (e.UserData?.PlaybackPositionTicks ?? 0) > 0) ||
        episodes.Items.find((e) => !e.UserData?.Played) ||
        episodes.Items[0];
    }
    if (!item) throw new Error("这个剧集还没有可播放的分集");
    playback.open(await preparePlayback(item));
  } catch {
    ui.showToast("暂时无法打开影片，请检查媒体源或稍后重试", "error");
  } finally {
    busy.value = false;
  }
}
</script>
<template>
  <article
    class="media-card du-card"
    :class="{ 'is-wide': layout === 'wide', 'is-compact': compact }"
  >
    <div class="visual">
      <RouterLink :to="to" class="poster-link" :aria-label="`查看 ${item.title}`"
        ><MediaPoster
          :src="item.poster"
          :title="item.title"
          :ratio="layout === 'wide' ? 'wide' : 'poster'"
          :variant="variant" /></RouterLink
      ><button
        class="du-btn du-btn-circle play-overlay"
        :aria-label="`播放 ${item.title}`"
        :disabled="busy"
        @click="play"
      >
        <BaseIcon :name="busy ? 'loading' : 'play'" :size="21" /></button
      ><span v-if="showResolution && item.resolution" class="du-badge resolution">{{
        item.resolution
      }}</span>
      <div v-if="item.progress != null" class="watch-progress">
        <span :style="{ width: `${Math.min(100, Math.max(0, item.progress))}%` }" />
      </div>
    </div>
    <div class="card-caption">
      <RouterLink :to="to" class="title" :title="item.title">{{ item.title }}</RouterLink>
      <div class="metadata">
        <span>{{ subtitle }}</span
        ><span v-if="showRating && item.rating" class="score"
          ><BaseIcon name="star" :size="12" />{{ item.rating.toFixed(1) }}</span
        >
      </div>
    </div>
  </article>
</template>
<style scoped lang="scss">
.media-card {
  min-width: 0;
  background: transparent;
  border: 0;
  box-shadow: none;
  gap: 0;
  border-radius: 0;
}
.visual {
  position: relative;
  isolation: isolate;
  overflow: hidden;
  border-radius: 12px;
  background: var(--fbz-color-panel-strong);
  transition: transform 0.2s;
}
.poster-link {
  display: block;
  outline-offset: -3px;
}
.play-overlay {
  position: absolute;
  right: 12px;
  bottom: 12px;
  width: 42px;
  height: 42px;
  min-height: 0;
  background: #f6f8f7e8;
  color: #101614;
  border: 0;
  opacity: 0;
  transform: translateY(6px);
  transition:
    opacity 0.2s,
    transform 0.2s;
}
.media-card:hover .play-overlay,
.media-card:focus-within .play-overlay {
  opacity: 1;
  transform: none;
}
.media-card:hover .visual {
  transform: translateY(-3px);
}
.play-overlay:hover {
  background: var(--fbz-color-brand-500);
}
.resolution {
  position: absolute;
  top: 10px;
  left: 10px;
  background: #080a0dbb;
  color: #f5f5f5;
  border: 1px solid #ffffff22;
  height: 22px;
  padding: 0 7px;
  font-size: 10px;
}
.watch-progress {
  position: absolute;
  bottom: 0;
  left: 0;
  width: 100%;
  height: 3px;
  background: #ffffff30;
}
.watch-progress span {
  display: block;
  height: 100%;
  background: var(--fbz-color-brand-500);
}
.card-caption {
  padding: 13px 1px 0;
}
.title {
  display: -webkit-box;
  -webkit-box-orient: vertical;
  -webkit-line-clamp: 2;
  overflow: hidden;
  text-decoration: none;
  line-height: 1.5;
  font-size: 14px;
  font-weight: 550;
  color: var(--fbz-color-text);
  letter-spacing: 0.05px;
}
.metadata {
  display: flex;
  justify-content: space-between;
  gap: 6px;
  margin-top: 5px;
  color: var(--fbz-color-text-muted);
  font-size: 11px;
  line-height: 1.6;
}
.metadata > span:first-child {
  overflow: hidden;
  white-space: nowrap;
  text-overflow: ellipsis;
}
.score {
  display: inline-flex;
  align-items: center;
  gap: 4px;
  color: var(--fbz-color-text-soft);
}
.is-wide .title {
  font-size: 13px;
}
.is-wide .play-overlay {
  opacity: 1;
  transform: none;
  width: 34px;
  height: 34px;
  bottom: 10px;
  right: 10px;
}
@media (pointer: coarse) {
  .play-overlay {
    opacity: 1;
    transform: none;
    width: 34px;
    height: 34px;
  }
}
@media (max-width: 600px) {
  .visual {
    border-radius: 9px;
  }
  .title {
    font-size: 12px;
  }
  .metadata {
    font-size: 10px;
  }
  .card-caption {
    padding-top: 10px;
  }
  .play-overlay {
    right: 8px;
    bottom: 8px;
  }
}
.title {
  min-height: 3em;
}
.is-compact {
  display: grid;
  grid-template-columns: 104px minmax(0, 1fr);
  gap: 12px;
  align-items: center;
}
.is-compact .visual {
  width: 104px;
}
.is-compact .card-caption {
  padding: 0;
}
.is-compact .title {
  min-height: 0;
  font-size: 12px;
  line-height: 1.6;
}
.is-compact .metadata {
  font-size: 10px;
  margin-top: 6px;
}
.is-compact .play-overlay {
  width: 28px;
  height: 28px;
  right: 6px;
  bottom: 6px;
}
@media (max-width: 800px) {
  .is-compact {
    display: block;
  }
  .is-compact .visual {
    width: 100%;
  }
  .is-compact .card-caption {
    padding-top: 9px;
  }
}
</style>
