<script setup lang="ts">
import type { DetailViewModel, EpisodeSummary } from "@/service/modules/detail.ts";
import type { MediaSourceInfo } from "@/service/modules/mediaInfo.ts";
import {
  fetchEpisodes,
  fetchMediaSources,
  fetchPlaybackSource,
  fetchSeriesContext,
  loadTvDetail,
} from "@/service/modules/detail.ts";
import { useDetailActions } from "@/composables/useDetailActions.ts";
import { useAuthStore } from "@/stores/auth.ts";
import type { PlaybackEpisode } from "@/stores/playback.ts";
import { usePlaybackStore } from "@/stores/playback.ts";
import { useUiStore } from "@/stores/ui.ts";
import { episodeCode, itemTypeMeta } from "@/utils/itemType.ts";

/**
 * 剧集层级：Series → Season → Episode（ParentIndexNumber / IndexNumber / SeriesName）。
 * - Series：季 / 集导航 + 续看定位
 * - Season：转到所属剧集（季集导航内定位该季）
 * - Episode：单集详情（SxxExx、剧集面包屑、媒体信息、同季播放列表）
 */
interface EpisodePlayPayload {
  episode: EpisodeSummary;
  episodes: EpisodeSummary[];
}

const route = useRoute();
const router = useRouter();
const playback = usePlaybackStore();
const uiStore = useUiStore();
const authStore = useAuthStore();
const routeId = computed(() => String(route.params.id));

const detail = ref<DetailViewModel>();
const loading = ref(true);
const resolvingPlay = shallowRef(false);
const versionId = ref("");
const subtitleOpen = ref(false);
const series = ref<{ seriesId?: string; seriesName?: string }>({});

/** 分集媒体信息弹窗。 */
const infoOpen = ref(false);
const infoTitle = ref("");
const infoSources = ref<MediaSourceInfo[]>([]);
const infoActiveId = ref("");
const infoLoading = ref(false);

const { toggleFavorite, togglePlayed, openMore, editMetadata } = useDetailActions(
  detail,
  "/library",
);

const isEpisode = computed(() => detail.value?.itemType === "Episode");
const typeMeta = computed(() => itemTypeMeta(detail.value?.itemType));
const seriesId = computed(() => (isEpisode.value ? series.value.seriesId : detail.value?.id));
const showTitle = computed(() =>
  isEpisode.value ? (series.value.seriesName ?? "") : (detail.value?.title ?? ""),
);
const epCode = computed(() =>
  isEpisode.value ? episodeCode(detail.value?.seasonNumber, detail.value?.episodeNumber) : "",
);

watch(
  routeId,
  async (id) => {
    loading.value = true;
    series.value = {};
    const loaded = await loadTvDetail(id);
    if (routeId.value !== id) return;
    if (loaded?.itemType === "Season" && loaded.parentId) {
      await router.replace({ path: `/tv/${loaded.parentId}`, query: { season: loaded.id } });
      return;
    }
    detail.value = loaded;
    loading.value = false;
    if (loaded?.itemType === "Episode") {
      const ctx = await fetchSeriesContext(loaded.id);
      if (routeId.value === id) series.value = ctx;
    }
  },
  { immediate: true },
);

const crumbs = computed(() => {
  if (!isEpisode.value || !series.value.seriesId) return [];
  return [
    { label: series.value.seriesName ?? "剧集", to: `/tv/${series.value.seriesId}`, icon: "tv" },
  ];
});

/** 分集标题：「剧名 SxxExx」。 */
function episodeLabel(episode: EpisodeSummary, index: number): string {
  const code = episodeCode(episode.seasonNumber, episode.episodeNumber ?? index + 1);
  return `${showTitle.value} ${code}`.trim();
}

function toPlaybackPlaylist(episodes: EpisodeSummary[]): PlaybackEpisode[] {
  return episodes.map((episode, index) => ({
    id: episode.id,
    seasonNumber: episode.seasonNumber ?? 0,
    episodeNumber: episode.episodeNumber ?? index + 1,
    title: episodeLabel(episode, index),
    subtitle: episode.name,
    duration: episode.runtimeSeconds ?? 0,
    poster: episode.poster,
  }));
}

/** 播放某一集：取真实流地址后连同同季播放列表一并打开播放器（带续播位置）。 */
async function playEpisode({ episode, episodes }: EpisodePlayPayload, fromStart = false) {
  const tv = detail.value;
  if (!tv) return;
  const playlist = toPlaybackPlaylist(episodes);
  const index = episodes.findIndex((entry) => entry.id === episode.id);
  const source = await fetchPlaybackSource(episode.id);
  if (!source) {
    uiStore.showToast("该分集暂无可播放媒体源。", "warning");
    return;
  }
  const startPositionSeconds =
    !fromStart && !episode.played && episode.progressPercent && episode.runtimeSeconds
      ? Math.floor((episode.progressPercent / 100) * episode.runtimeSeconds)
      : 0;
  playback.open({
    type: "episode",
    id: episode.id,
    title: episodeLabel(episode, index < 0 ? 0 : index),
    subtitle: episode.name,
    poster: episode.poster ?? tv.poster,
    backdrop: tv.backdrop,
    tags: source.mediaInfo ? undefined : tv.versions[0]?.tags,
    duration: source.duration ?? episode.runtimeSeconds,
    startPositionSeconds,
    playlist,
    source,
    server: source.server,
    mediaSources: source.mediaSources,
    chapters: source.chapters,
    subtitleTracks: source.subtitleTracks,
    mediaInfo: source.mediaInfo,
  });
}

/**
 * 主播放键。Series：解析「继续观看」目标集（有进度且未看完的最靠后一集 → 第一个未看完 →
 * 第一集）。Episode：播放本集（同季为播放列表）。
 */
async function playMain(fromStart = false) {
  const tv = detail.value;
  if (!tv || resolvingPlay.value) return;
  resolvingPlay.value = true;
  try {
    if (isEpisode.value) {
      const sid = series.value.seriesId;
      const seasonEpisodes = sid ? await fetchEpisodes(sid, tv.parentId) : [];
      const self: EpisodeSummary = seasonEpisodes.find((e) => e.id === tv.id) ?? {
        id: tv.id,
        episodeNumber: tv.episodeNumber ?? null,
        seasonNumber: tv.seasonNumber ?? null,
        name: tv.title,
        runtimeSeconds: tv.runtimeSeconds,
        played: tv.played,
        progressPercent:
          tv.resumePositionSeconds && tv.runtimeSeconds
            ? (tv.resumePositionSeconds / tv.runtimeSeconds) * 100
            : undefined,
      };
      await playEpisode(
        { episode: self, episodes: seasonEpisodes.length ? seasonEpisodes : [self] },
        fromStart,
      );
      return;
    }
    const episodes = await fetchEpisodes(tv.id);
    if (!episodes.length) {
      uiStore.showToast("该剧集还没有可播放的分集。", "warning");
      return;
    }
    const inProgress = [...episodes]
      .reverse()
      .find((episode) => !episode.played && (episode.progressPercent ?? 0) > 0);
    const firstUnplayed = episodes.find((episode) => !episode.played);
    const target = inProgress ?? firstUnplayed ?? episodes[0]!;
    const sameSeason = episodes.filter((episode) => episode.seasonNumber === target.seasonNumber);
    await playEpisode({ episode: target, episodes: sameSeason.length ? sameSeason : episodes });
  } finally {
    resolvingPlay.value = false;
  }
}

async function showEpisodeInfo(episode: EpisodeSummary) {
  infoOpen.value = true;
  infoTitle.value = `${episodeCode(episode.seasonNumber, episode.episodeNumber)} · ${episode.name}`;
  infoSources.value = [];
  infoLoading.value = true;
  try {
    infoSources.value = await fetchMediaSources(episode.id);
    infoActiveId.value = infoSources.value[0]?.id ?? "";
  } finally {
    infoLoading.value = false;
  }
}

async function reloadSources() {
  const tv = detail.value;
  if (!tv) return;
  const fresh = await loadTvDetail(tv.id);
  if (fresh && detail.value?.id === fresh.id) detail.value = fresh;
}

const activeSource = computed(
  () =>
    detail.value?.mediaSources.find((s) => s.id === versionId.value) ??
    detail.value?.mediaSources[0],
);

const heroMeta = computed(() => {
  const d = detail.value;
  if (!d) return [];
  return epCode.value ? [epCode.value, ...d.meta] : d.meta;
});

const facts = computed(() => {
  const d = detail.value;
  if (!d) return [];
  return [
    { label: isEpisode.value ? "导演" : "主创", value: d.creators },
    { label: "编剧", value: d.writers },
    { label: "原名", value: d.originalTitle !== d.title ? d.originalTitle : undefined },
    { label: "分级", value: d.officialRating },
    { label: "首播", value: d.premiereDate },
  ].filter((f) => f.value);
});
</script>

<template>
  <main v-if="detail" class="c-page detail-page">
    <DetailHero
      v-model:version-id="versionId"
      :title="detail.title"
      :original-title="detail.originalTitle"
      :poster="detail.poster"
      :backdrop="detail.backdrop"
      :poster-ratio="isEpisode ? 'wide' : 'poster'"
      :meta="heroMeta"
      :genres="detail.genres"
      :official-rating="detail.officialRating"
      :overview="detail.overview"
      :rating="detail.rating"
      :type-label="
        isEpisode && series.seriesName ? `${series.seriesName} · ${typeMeta.label}` : typeMeta.label
      "
      :type-icon="typeMeta.icon"
      :versions="detail.versions"
      :is-favorite="detail.isFavorite"
      :played="detail.played"
      :resume-position-seconds="isEpisode ? detail.resumePositionSeconds : undefined"
      :busy="resolvingPlay"
      show-played-toggle
      show-more
      @play="() => playMain(isEpisode)"
      @resume="() => playMain(false)"
      @toggle-favorite="toggleFavorite"
      @toggle-played="togglePlayed"
      @more="openMore"
    >
      <template #header>
        <PageHeader :title="detail.title" :crumbs="crumbs" fallback="/library" />
      </template>
      <template #actions>
        <button type="button" class="du-btn act" @click="subtitleOpen = true">
          <BaseIcon name="subtitles" :size="16" />外挂字幕
        </button>
        <button v-if="authStore.isAdmin" type="button" class="du-btn act" @click="editMetadata">
          <BaseIcon name="edit" :size="15" />编辑元数据
        </button>
      </template>
      <template v-if="facts.length" #facts>
        <dl class="facts">
          <div v-for="fact in facts" :key="fact.label" class="fact">
            <dt>{{ fact.label }}</dt>
            <dd>{{ fact.value }}</dd>
          </div>
        </dl>
      </template>
    </DetailHero>

    <MediaInfoPanel
      v-if="isEpisode && detail.mediaSources.length"
      v-model:active-id="versionId"
      :sources="detail.mediaSources"
      :chapter-count="detail.chapterCount"
    />

    <SeasonEpisodes
      v-if="seriesId"
      :series-id="seriesId"
      :show-title="showTitle"
      :backdrop="detail.backdrop"
      :initial-season-id="isEpisode ? detail.parentId : (route.query.season as string | undefined)"
      :current-episode-id="isEpisode ? detail.id : undefined"
      @play-episode="playEpisode"
      @show-info="showEpisodeInfo"
    />

    <CastRow :cast="detail.cast" :title="isEpisode ? '本集演职人员' : '演职人员'" />
    <SimilarRow :items="detail.similar" />

    <RemoteSubtitleModal
      :open="subtitleOpen"
      :item-id="detail.id"
      :title="detail.title"
      :media-source-id="activeSource?.id"
      :existing="activeSource?.subtitles"
      @close="subtitleOpen = false"
      @downloaded="reloadSources"
    />

    <BaseModal
      :open="infoOpen"
      title="媒体信息"
      :description="infoTitle"
      wide
      @close="infoOpen = false"
    >
      <p v-if="infoLoading" class="c-state">
        <BaseIcon name="loading" :size="15" class="c-spin" />读取 PlaybackInfo…
      </p>
      <MediaInfoPanel
        v-else-if="infoSources.length"
        v-model:active-id="infoActiveId"
        :sources="infoSources"
        compact
      />
      <p v-else class="c-state"><BaseIcon name="info" :size="15" />该分集没有可用的媒体源信息。</p>
    </BaseModal>
  </main>

  <main v-else-if="loading" class="c-page">
    <p class="c-state"><BaseIcon name="loading" :size="16" class="c-spin" />正在加载剧集…</p>
  </main>

  <main v-else class="c-page">
    <BaseEmptyState
      icon="search-x"
      title="未找到该剧集"
      description="条目可能已被删除，或当前账号没有访问该媒体库的权限。"
    >
      <RouterLink to="/" class="du-btn du-btn-ghost"
        ><BaseIcon name="home" :size="15" />返回首页</RouterLink
      >
    </BaseEmptyState>
  </main>
</template>

<style scoped lang="scss">
.facts {
  display: grid;
  grid-template-columns: repeat(auto-fill, minmax(260px, 1fr));
  gap: 8px 32px;
  max-width: 880px;
  margin: 0;
}

.fact {
  display: flex;
  gap: 12px;
  font-size: 13px;

  dt {
    flex: 0 0 48px;
    color: var(--ui-text-subtle);
  }

  dd {
    margin: 0;
    color: var(--ui-text);
  }
}
</style>
