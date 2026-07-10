<script setup lang="ts">
import type { DetailViewModel, EpisodeSummary } from "@/service/modules/detail.ts";
import { fetchEpisodes, fetchPlaybackSource, loadTvDetail } from "@/service/modules/detail.ts";
import { setFavorite } from "@/service/modules/userData.ts";
import { useAuthStore } from "@/stores/auth.ts";
import type { PlaybackEpisode } from "@/stores/playback.ts";
import { usePlaybackStore } from "@/stores/playback.ts";
import { useUiStore } from "@/stores/ui.ts";

/** SeasonEpisodes 抛出的播放载荷：命中集 + 同季全集。 */
interface EpisodePlayPayload {
  episode: EpisodeSummary;
  episodes: EpisodeSummary[];
}

const route = useRoute();
const playback = usePlaybackStore();
const uiStore = useUiStore();
const authStore = useAuthStore();
const routeId = computed(() => String(route.params.id));

const detail = ref<DetailViewModel>();
const togglingFavorite = shallowRef(false);
const resolvingPlay = shallowRef(false);

watch(
  routeId,
  async (v) => {
    detail.value = await loadTvDetail(v);
  },
  { immediate: true },
);

/**
 * 剧集主播放键：解析「继续观看」目标集（有进度且未看完的最靠后一集 →
 * 第一个未看完集 → 第一集）后直接播放，避免打开没有流的空播放器。
 */
async function playTv() {
  const tv = detail.value;
  if (!tv || resolvingPlay.value) return;

  resolvingPlay.value = true;
  try {
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

/** 收藏切换：乐观更新，失败回滚并提示。 */
async function toggleFavorite() {
  const tv = detail.value;
  const userId = authStore.userId;
  if (!tv || !userId || togglingFavorite.value) return;

  const next = !tv.isFavorite;
  togglingFavorite.value = true;
  detail.value = { ...tv, isFavorite: next };
  try {
    await setFavorite(userId, tv.id, next);
  } catch {
    detail.value = { ...tv, isFavorite: !next };
    uiStore.showToast("更新收藏状态失败。", "error");
  } finally {
    togglingFavorite.value = false;
  }
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
    isFavorite: tv.isFavorite,
  });
}

/** 同季分集 → 播放列表（id 用真实集 public_id，供 store 切集时重取流地址）。 */
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

/** 分集标题：优先真实季/集号，缺省用顺序兜底。 */
function episodeLabel(episode: EpisodeSummary, index: number): string {
  const seasonNo = episode.seasonNumber;
  const episodeNo = episode.episodeNumber ?? index + 1;
  const prefix = seasonNo != null ? `S${seasonNo} ` : "";
  return `${detail.value?.title ?? ""} ${prefix}E${episodeNo}`.trim();
}

/** 播放某一集：取真实流地址后连同同季播放列表一并打开播放器（带续播位置）。 */
async function playEpisode({ episode, episodes }: EpisodePlayPayload) {
  const tv = detail.value;
  if (!tv) return;

  const playlist = toPlaybackPlaylist(episodes);
  const index = episodes.findIndex((entry) => entry.id === episode.id);
  const source = await fetchPlaybackSource(episode.id);
  // 分集续播：有进度且未看完时从上次位置继续。
  const startPositionSeconds =
    !episode.played && episode.progressPercent && episode.runtimeSeconds
      ? Math.floor((episode.progressPercent / 100) * episode.runtimeSeconds)
      : undefined;

  playback.open({
    type: "episode",
    id: episode.id,
    title: episodeLabel(episode, index < 0 ? 0 : index),
    subtitle: episode.name,
    poster: episode.poster ?? tv.poster,
    backdrop: tv.backdrop,
    tags: tv.versions[0]?.tags,
    duration: episode.runtimeSeconds,
    startPositionSeconds,
    playlist,
    source: source ? { uri: source.uri, mimeType: source.mimeType } : undefined,
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
      :genres="detail.genres"
      :official-rating="detail.officialRating"
      :overview="detail.overview"
      :rating="detail.rating"
      :is-favorite="detail.isFavorite"
      @play="playTv"
      @toggle-favorite="toggleFavorite"
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
      :series-id="detail.id"
      :show-title="detail.title"
      :backdrop="detail.backdrop"
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
