<script setup lang="ts">
import type { TvDetail } from "@/types/media.ts";
import {
  findCatalogItem,
  getTvDetail,
  imageUrl,
  refToItem,
  versionsFor,
} from "@/service/modules/tmdb.ts";
import type { PlaybackEpisode } from "@/stores/playback.ts";
import { usePlaybackStore } from "@/stores/playback.ts";

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
const id = computed(() => Number(route.params.id));

const item = computed(() => findCatalogItem("tv", id.value));
const detail = ref<TvDetail>();

watch(
  id,
  async (v) => {
    detail.value = await getTvDetail(v);
  },
  { immediate: true },
);

const versions = computed(() => versionsFor(id.value));

const meta = computed(() => {
  const it = item.value;
  if (!it) return [];
  const d = detail.value;
  return [
    String(it.year ?? "—"),
    d ? `${d.seasons_count} 季` : "",
    d ? `${d.episodes_count} 集` : "",
    ...it.genres,
  ].filter(Boolean);
});

const creators = computed(() => detail.value?.creators.map((c) => c.name).join("、") ?? "");
const similar = computed(() => detail.value?.similar.map(refToItem) ?? []);

// 演示「继续观看」定位：偶数 ID 模拟有历史，奇数 ID 模拟无历史，以便能展示/测试季列表和直达集列表两种状态
const defaultSeason = computed(() => {
  if (id.value % 2 !== 0) return undefined;
  const seasons = detail.value?.seasons ?? [];
  if (!seasons.length) return undefined;
  return seasons[id.value % seasons.length]?.season_number;
});
const watchedEpisode = computed(() => {
  if (defaultSeason.value == null) return undefined;
  const s = detail.value?.seasons.find((x) => x.season_number === defaultSeason.value);
  if (!s) return undefined;
  return (id.value % s.episode_count) + 1;
});

function playTv() {
  const tv = item.value;
  if (!tv) return;

  playback.open({
    type: "tv",
    id: String(tv.id),
    title: tv.title,
    subtitle: meta.value.join(" · "),
    poster: imageUrl(tv.poster_path, "w500"),
    backdrop: imageUrl(tv.backdrop_path, "w1280"),
    tags: versions.value[0]?.tags,
    duration: 45 * 60,
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
  const tv = item.value;
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
    poster: episode.poster ?? imageUrl(tv.poster_path, "w500"),
    backdrop: episode.backdrop ?? imageUrl(tv.backdrop_path, "w1280"),
    tags: versions.value[0]?.tags,
    duration: episode.runtime * 60,
    playlist,
  });
}
</script>

<template>
  <main v-if="item" class="detail-view">
    <PageHeader :title="item.title" fallback="/library/series" />

    <DetailHero
      :title="item.title"
      :poster="imageUrl(item.poster_path, 'w500')"
      :backdrop="imageUrl(item.backdrop_path, 'w1280')"
      :meta="meta"
      :tagline="detail?.tagline"
      :overview="item.overview"
      :rating="item.rating"
      :versions="versions"
      @play="playTv"
    >
      <template #extra>
        <dl class="facts">
          <div v-if="creators" class="fact">
            <dt>主创</dt>
            <dd>{{ creators }}</dd>
          </div>
          <div v-if="detail && detail.original_title !== item.title" class="fact">
            <dt>原名</dt>
            <dd>{{ detail.original_title }}</dd>
          </div>
        </dl>
      </template>
    </DetailHero>

    <SeasonEpisodes
      v-if="detail?.seasons.length"
      :seasons="detail.seasons"
      :series-id="id"
      :default-season="defaultSeason"
      :watched-episode="watchedEpisode"
      :show-title="item.title"
      :backdrop="imageUrl(item.backdrop_path, 'w1280')"
      :rating="item.rating"
      @play-episode="playEpisode"
    />

    <CastRow v-if="detail" :cast="detail.cast" />
    <SimilarRow :items="similar" />
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

.detail-missing {
  min-height: 100vh;
  display: grid;
  place-content: center;
  gap: var(--fbz-space-3);
  text-align: center;
  color: var(--fbz-color-text-muted);
}
</style>
