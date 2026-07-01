<script setup lang="ts">
import type { DetailViewModel } from "@/service/modules/detail.ts";
import { loadTvDetail } from "@/service/modules/detail.ts";
import type { PlaybackEpisode } from "@/stores/playback.ts";
import { usePlaybackStore } from "@/stores/playback.ts";
import { useUiStore } from "@/stores/ui.ts";

interface EpisodePlayEvent {
  seasonNumber: number;
  episodeNumber: number;
  title: string;
  runtime: number;
  poster?: string;
  backdrop?: string;
  episodes: Array<{
    id: string;
    seasonNumber: number;
    episodeNumber: number;
    title: string;
    runtime: number;
    poster?: string;
    backdrop?: string;
  }>;
}

const route = useRoute();
const playback = usePlaybackStore();
const uiStore = useUiStore();
const routeId = computed(() => String(route.params.id));

const detail = ref<DetailViewModel>();

watch(
  routeId,
  async (v) => {
    detail.value = await loadTvDetail(v);
  },
  { immediate: true },
);

// 演示「继续观看」定位：仅 TMDB 占位态（有季列表）下生效，后端态季集为空时不展示。
// 偶数数字 id 模拟有历史、奇数模拟无历史，以便能展示/测试季列表和直达集列表两种状态。
const numericId = computed(() => Number(detail.value?.id));
const defaultSeason = computed(() => {
  const seasons = detail.value?.seasons ?? [];
  if (!seasons.length || Number.isNaN(numericId.value) || numericId.value % 2 !== 0)
    return undefined;
  return seasons[numericId.value % seasons.length]?.season_number;
});
const watchedEpisode = computed(() => {
  if (defaultSeason.value == null) return undefined;
  const s = detail.value?.seasons.find((x) => x.season_number === defaultSeason.value);
  if (!s) return undefined;
  return (numericId.value % s.episode_count) + 1;
});

function playTv() {
  const tv = detail.value;
  if (!tv) return;

  playback.open({
    type: "tv",
    id: tv.id,
    title: tv.title,
    subtitle: tv.meta.join(" · "),
    poster: tv.poster,
    backdrop: tv.backdrop,
    tags: tv.versions[0]?.tags,
    duration: 45 * 60,
  });
}

/** 打开元数据管理弹层：用详情视图模型拼一个最小 MediaItem 传入。 */
function editMetadata() {
  const tv = detail.value;
  if (!tv) return;
  const yearSeg = tv.meta.find((m) => /^\d{4}$/.test(m));
  uiStore.openMetadataManager({
    id: tv.id,
    libraryId: "series",
    detailType: "tv",
    title: tv.title,
    meta: tv.meta.join(" · "),
    poster: tv.poster,
    year: yearSeg ? Number(yearSeg) : undefined,
    rating: tv.rating ?? undefined,
    isFavorite: false,
  });
}

function toPlaybackPlaylist(
  tvTitle: string,
  episodes: EpisodePlayEvent["episodes"],
): PlaybackEpisode[] {
  return episodes.map((episode) => ({
    id: `${tvTitle}-${episode.id}`,
    seasonNumber: episode.seasonNumber,
    episodeNumber: episode.episodeNumber,
    title: `${tvTitle} S${episode.seasonNumber} E${episode.episodeNumber}`,
    subtitle: `${episode.title} · ${episode.runtime}min`,
    duration: episode.runtime * 60,
    poster: episode.poster,
    backdrop: episode.backdrop,
  }));
}

function playEpisode(episode: EpisodePlayEvent) {
  const tv = detail.value;
  if (!tv) return;

  const playlist = toPlaybackPlaylist(tv.title, episode.episodes);
  const episodeId = `${tv.title}-${
    episode.episodes.find(
      (entry) =>
        entry.seasonNumber === episode.seasonNumber &&
        entry.episodeNumber === episode.episodeNumber,
    )?.id ?? `${tv.id}-s${episode.seasonNumber}-e${episode.episodeNumber}`
  }`;

  playback.open({
    type: "episode",
    id: episodeId,
    title: `${tv.title} S${episode.seasonNumber} E${episode.episodeNumber}`,
    subtitle: `${episode.title} · ${episode.runtime}min`,
    poster: episode.poster ?? tv.poster,
    backdrop: episode.backdrop ?? tv.backdrop,
    tags: tv.versions[0]?.tags,
    duration: episode.runtime * 60,
    playlist,
  });
}
</script>

<template>
  <main v-if="detail" class="detail-view">
    <PageHeader :title="detail.title" fallback="/library/series" />

    <DetailHero
      :title="detail.title"
      :poster="detail.poster"
      :backdrop="detail.backdrop"
      :meta="detail.meta"
      :tagline="detail.tagline"
      :overview="detail.overview"
      :rating="detail.rating"
      :versions="detail.versions"
      @play="playTv"
    >
      <template #extra>
        <dl class="facts">
          <div v-if="detail.creators" class="fact">
            <dt>主创</dt>
            <dd>{{ detail.creators }}</dd>
          </div>
          <div v-if="detail.originalTitle" class="fact">
            <dt>原名</dt>
            <dd>{{ detail.originalTitle }}</dd>
          </div>
        </dl>
        <button type="button" class="edit-meta-btn" @click="editMetadata">编辑元数据</button>
      </template>
    </DetailHero>

    <SeasonEpisodes
      v-if="detail.seasons.length"
      :seasons="detail.seasons"
      :series-id="detail.id"
      :default-season="defaultSeason"
      :watched-episode="watchedEpisode"
      :show-title="detail.title"
      :backdrop="detail.backdrop"
      :rating="detail.rating"
      @play-episode="playEpisode"
    />

    <CastRow :cast="detail.cast" />
    <SimilarRow :items="detail.similar" />
  </main>

  <main v-else class="detail-missing">
    <p>未找到该剧集</p>
    <RouterLink to="/" class="link">返回首页</RouterLink>
  </main>
</template>

<style scoped lang="scss">
.facts {
  margin: 0;
  display: flex;
  flex-direction: column;
  gap: var(--fbz-space-2);
}

.fact {
  display: flex;
  gap: var(--fbz-space-3);
  font-size: var(--fbz-font-size-md);

  dt {
    flex: 0 0 64px;
    color: var(--fbz-color-text-muted);
  }

  dd {
    margin: 0;
    color: var(--fbz-color-text-soft);
  }
}

.link {
  color: var(--fbz-color-brand-500);
  text-decoration: none;

  &:hover {
    text-decoration: underline;
  }
}

.edit-meta-btn {
  margin-top: var(--fbz-space-3);
  height: 34px;
  padding: 0 16px;
  border-radius: var(--fbz-radius-control);
  border: 1px solid var(--fbz-color-line);
  background: var(--fbz-color-panel-strong);
  color: var(--fbz-color-text-soft);
  font-size: var(--fbz-font-size-sm);
  font-weight: 700;
  cursor: pointer;
  transition: all var(--fbz-motion-fast);

  &:hover {
    border-color: var(--fbz-color-brand-500);
    color: var(--fbz-color-brand-500);
    background: color-mix(in srgb, var(--fbz-color-brand-500) 6%, transparent);
  }
}

.detail-missing {
  min-height: 100vh;
  display: grid;
  place-content: center;
  gap: var(--fbz-space-3);
  text-align: center;
  color: var(--fbz-color-text-muted);
}
</style>
