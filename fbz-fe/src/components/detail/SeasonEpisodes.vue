<script setup lang="ts">
import { ref, computed, watch, nextTick, onMounted } from "vue";
import type { EpisodeSummary, SeasonSummary } from "@/service/modules/detail.ts";
import { fetchEpisodes, fetchSeasons } from "@/service/modules/detail.ts";
import { episodeCode } from "@/utils/itemType.ts";

/**
 * 季 / 集导航（Series → Season → Episode）。
 * - 单季：直接平铺分集列表
 * - 多季：季海报网格 → 选季进入分集列表（顶部季切换条 + 返回）
 * - 「继续观看」：自动定位到有进度且未看完的最靠后一集所在季与范围
 * - 长季（>50 集）按 E1-E50 分段
 */
interface Props {
  /** 剧集 public_id。 */
  seriesId: string;
  showTitle: string;
  /** 剧集剧照，仅当分集自身无缩略图时用作兜底背景（可缺省）。 */
  backdrop?: string;
  /** 打开时优先展示的季（单集详情 / 季链接）。 */
  initialSeasonId?: string;
  /** 当前正在查看的单集（高亮）。 */
  currentEpisodeId?: string;
}

/** 抛给父级的播放载荷：命中集 + 同季全集（父级据此建播放列表并取流地址）。 */
interface EpisodePlayPayload {
  episode: EpisodeSummary;
  episodes: EpisodeSummary[];
}

interface EpisodeCard {
  id: string;
  displayNumber: number;
  code: string;
  title: string;
  durationText: string;
  airDate: string;
  summary: string;
  poster?: string;
  played: boolean;
  /** 「继续观看」命中集（有进度且未看完）。 */
  current: boolean;
  /** 正在查看的单集。 */
  viewing: boolean;
  progressPercent?: number;
}

interface EpisodeRange {
  label: string;
  episodes: EpisodeCard[];
}

const RANGE_SIZE = 50;

const props = defineProps<Props>();
const emit = defineEmits<{
  playEpisode: [payload: EpisodePlayPayload];
  showInfo: [episode: EpisodeSummary];
}>();

const seasons = ref<SeasonSummary[]>([]);
const episodes = ref<EpisodeSummary[]>([]);
const selectedSeasonId = ref<string>();
const activeRangeIndex = ref(0);
const viewState = ref<"seasons" | "episodes">("seasons");
const sectionRef = ref<HTMLElement | null>(null);

let allowScroll = false;
onMounted(() => {
  void nextTick(() => {
    allowScroll = true;
  });
});

watch(viewState, () => {
  if (!allowScroll) return;
  void nextTick(() => {
    const el = sectionRef.value;
    if (!el || typeof window.scrollTo !== "function") return;
    const headerH =
      parseInt(getComputedStyle(document.documentElement).getPropertyValue("--header-h")) || 64;
    const top = el.getBoundingClientRect().top + window.scrollY - headerH - 16;
    if (top < window.scrollY) window.scrollTo({ top, behavior: "smooth" });
  });
});

/** 在整季集里挑「继续观看」命中集：有进度且未看完，取最靠后的一集。 */
function pickResumeEpisode(list: EpisodeSummary[]): EpisodeSummary | undefined {
  const inProgress = list.filter((ep) => ep.progressPercent != null && !ep.played);
  if (!inProgress.length) return undefined;
  return inProgress.reduce((latest, ep) => {
    const key = (e: EpisodeSummary) => (e.seasonNumber ?? 0) * 10_000 + (e.episodeNumber ?? 0);
    return key(ep) >= key(latest) ? ep : latest;
  });
}

/** 载入剧集：拉季列表；优先 initialSeasonId，其次「继续观看」的季，否则季网格。 */
watch(
  () => props.seriesId,
  async (seriesId) => {
    seasons.value = [];
    episodes.value = [];
    selectedSeasonId.value = undefined;
    viewState.value = "seasons";
    activeRangeIndex.value = 0;
    if (!seriesId) return;

    const seasonList = await fetchSeasons(seriesId);
    if (props.seriesId !== seriesId) return;
    seasons.value = seasonList;
    if (!seasonList.length) return;

    if (seasonList.length === 1) {
      await selectSeason(seasonList[0]!, "episodes");
      return;
    }

    const preferred = props.initialSeasonId
      ? seasonList.find((s) => s.id === props.initialSeasonId)
      : undefined;
    if (preferred) {
      await selectSeason(preferred, "episodes");
      return;
    }

    // 多季：一次性拉全集判断「继续观看」落在哪一季。
    const all = await fetchEpisodes(seriesId);
    if (props.seriesId !== seriesId) return;
    const resume = pickResumeEpisode(all);
    const historySeason =
      resume != null
        ? seasonList.find((s) => s.seasonNumber != null && s.seasonNumber === resume.seasonNumber)
        : undefined;

    if (historySeason) {
      await selectSeason(historySeason, "episodes");
    } else {
      selectedSeasonId.value = seasonList[0]!.id;
      viewState.value = "seasons";
    }
  },
  { immediate: true },
);

/** 选季：拉该季分集并进入分集视图，定位「继续观看」/ 正在查看所在范围。 */
async function selectSeason(
  season: SeasonSummary,
  targetView: "seasons" | "episodes" = "episodes",
) {
  selectedSeasonId.value = season.id;
  viewState.value = targetView;
  activeRangeIndex.value = 0;
  const list = await fetchEpisodes(props.seriesId, season.id);
  if (selectedSeasonId.value !== season.id) return;
  episodes.value = list;

  const focus = list.find((ep) => ep.id === props.currentEpisodeId) ?? pickResumeEpisode(list);
  if (focus) {
    const idx = list.findIndex((ep) => ep.id === focus.id);
    if (idx >= 0) activeRangeIndex.value = Math.floor(idx / RANGE_SIZE);
  }
}

const activeSeason = computed(
  () => seasons.value.find((season) => season.id === selectedSeasonId.value) ?? seasons.value[0],
);

const activeSeasonMeta = computed(() => {
  const season = activeSeason.value;
  if (!season) return [];
  return [
    season.year ? `${season.year}年` : "",
    season.episodeCount != null ? `${season.episodeCount} 集` : "",
  ].filter(Boolean);
});

const resumeEpisode = computed(() => pickResumeEpisode(episodes.value));

const displayEpisodes = computed<EpisodeCard[]>(() =>
  episodes.value.map((ep, index) => {
    const displayNumber = ep.episodeNumber ?? index + 1;
    return {
      id: ep.id,
      displayNumber,
      code: episodeCode(ep.seasonNumber, displayNumber),
      title: ep.name,
      durationText: formatDuration(ep.runtimeSeconds),
      airDate: formatAirDate(ep.premiereDate),
      summary: ep.overview ?? "",
      poster: ep.poster,
      played: ep.played,
      current: resumeEpisode.value?.id === ep.id,
      viewing: props.currentEpisodeId === ep.id,
      progressPercent: ep.progressPercent,
    };
  }),
);

const showRanges = computed(() => displayEpisodes.value.length > RANGE_SIZE);

const episodeRanges = computed<EpisodeRange[]>(() => {
  if (!showRanges.value) return [];
  const ranges: EpisodeRange[] = [];
  for (let start = 0; start < displayEpisodes.value.length; start += RANGE_SIZE) {
    const group = displayEpisodes.value.slice(start, start + RANGE_SIZE);
    ranges.push({
      label: `E${group[0]!.displayNumber}-E${group[group.length - 1]!.displayNumber}`,
      episodes: group,
    });
  }
  return ranges;
});

const visibleEpisodes = computed(() => {
  if (!showRanges.value) return displayEpisodes.value;
  return (
    episodeRanges.value[activeRangeIndex.value]?.episodes ??
    displayEpisodes.value.slice(0, RANGE_SIZE)
  );
});

/** 秒 → "48分钟"，无时长返回空串。 */
function formatDuration(seconds: number | undefined): string {
  if (!seconds) return "";
  return `${Math.max(1, Math.round(seconds / 60))}分钟`;
}

/** 首播日期字符串 → "2024-01-01"，无则空串。 */
function formatAirDate(date: string | undefined): string {
  if (!date) return "";
  const parsed = new Date(date);
  if (Number.isNaN(parsed.getTime())) return "";
  return `${parsed.getFullYear()}-${String(parsed.getMonth() + 1).padStart(2, "0")}-${String(parsed.getDate()).padStart(2, "0")}`;
}

function selectSeasonCard(season: SeasonSummary) {
  void selectSeason(season);
}

function playEpisode(card: EpisodeCard) {
  const episode = episodes.value.find((ep) => ep.id === card.id);
  if (!episode) return;
  emit("playEpisode", { episode, episodes: episodes.value });
}

function showInfo(card: EpisodeCard) {
  const episode = episodes.value.find((ep) => ep.id === card.id);
  if (episode) emit("showInfo", episode);
}

function playResumeEpisode() {
  const resume = resumeEpisode.value;
  if (resume) emit("playEpisode", { episode: resume, episodes: episodes.value });
}
</script>

<template>
  <section v-if="seasons.length" ref="sectionRef" class="seasons-section">
    <!-- 单季：平铺分集列表 -->
    <div v-if="seasons.length === 1" class="single-season-scroller-container">
      <header class="sec-head">
        <h2 class="c-section-title">
          <BaseIcon name="tv" :size="16" class="lead" />{{ seasons[0]!.name }}
          <span class="n">{{ displayEpisodes.length }} 集</span>
        </h2>
        <button
          v-if="resumeEpisode"
          class="du-btn du-btn-sm du-btn-primary continue-play-btn"
          type="button"
          @click="playResumeEpisode"
        >
          <BaseIcon name="play" :size="14" fill />继续
          {{ episodeCode(resumeEpisode.seasonNumber, resumeEpisode.episodeNumber) }}
        </button>
      </header>
      <ol class="episode-list">
        <li
          v-for="episode in displayEpisodes"
          :key="episode.id"
          class="episode-card"
          :class="{ watched: episode.played, current: episode.current, viewing: episode.viewing }"
          role="button"
          tabindex="0"
          :aria-label="`播放 ${episode.code} ${episode.title}`"
          @click="playEpisode(episode)"
          @keydown.enter.self.prevent="playEpisode(episode)"
        >
          <div class="ep-thumb">
            <MediaPoster
              :src="episode.poster ?? props.backdrop"
              :title="`${props.showTitle} ${episode.title}`"
              ratio="wide"
              icon="tv-minimal"
            />
            <span class="ep-play"><BaseIcon name="play" :size="16" fill /></span>
            <span v-if="episode.played" class="ep-flag"
              ><BaseIcon name="check" :size="12" :stroke-width="3"
            /></span>
            <span v-if="episode.progressPercent != null && !episode.played" class="ep-progress">
              <span :style="{ width: `${episode.progressPercent}%` }" />
            </span>
          </div>
          <div class="ep-info">
            <div class="ep-title-row">
              <span class="ep-code">{{ episode.code }}</span>
              <strong class="episode-title"
                >{{ episode.displayNumber }}. {{ episode.title }}</strong
              >
              <span v-if="episode.current" class="ep-tag">继续观看</span>
              <span v-else-if="episode.viewing" class="ep-tag dim">当前</span>
            </div>
            <div class="ep-meta c-meta">
              <span v-if="episode.durationText">{{ episode.durationText }}</span>
              <span v-if="episode.durationText && episode.airDate" class="c-dot" />
              <span v-if="episode.airDate">{{ episode.airDate }}</span>
            </div>
            <p v-if="episode.summary" class="episode-summary" :title="episode.summary">
              {{ episode.summary }}
            </p>
          </div>
          <button
            class="ep-info-btn"
            type="button"
            title="媒体信息"
            aria-label="媒体信息"
            @click.stop="showInfo(episode)"
          >
            <BaseIcon name="info" :size="16" />
          </button>
        </li>
      </ol>
    </div>

    <!-- 多季 -->
    <div v-else class="multi-seasons-container">
      <!-- 季网格 -->
      <div v-if="viewState === 'seasons'" class="seasons-grid-container">
        <header class="sec-head">
          <h2 class="c-section-title">
            <BaseIcon name="tv" :size="16" class="lead" />季<span class="n">{{
              seasons.length
            }}</span>
          </h2>
        </header>
        <div class="seasons-grid">
          <button
            v-for="season in seasons"
            :key="season.id"
            class="season-card-item"
            type="button"
            @click="selectSeasonCard(season)"
          >
            <div class="season-poster">
              <MediaPoster :src="season.poster" :title="season.name" ratio="poster" icon="tv" />
            </div>
            <div class="season-info">
              <h4>{{ season.name }}</h4>
              <div class="c-meta">
                <span v-if="season.year">{{ season.year }}年</span>
                <span v-if="season.episodeCount != null">{{ season.episodeCount }} 集</span>
              </div>
            </div>
          </button>
        </div>
      </div>

      <!-- 分集列表 -->
      <div v-else class="episodes-container">
        <header class="episodes-view-header">
          <button class="back-to-seasons-btn" type="button" @click="viewState = 'seasons'">
            <BaseIcon name="grid" :size="14" />全部季
          </button>
          <nav class="season-tabs" aria-label="切换季">
            <button
              v-for="season in seasons"
              :key="season.id"
              type="button"
              class="season-tab"
              :class="{ active: season.id === activeSeason?.id }"
              @click="selectSeasonCard(season)"
            >
              {{ season.name }}
            </button>
          </nav>
        </header>

        <div v-if="activeSeason" class="season-banner">
          <div class="season-poster-wrap">
            <MediaPoster
              :src="activeSeason.poster"
              :title="activeSeason.name"
              ratio="poster"
              icon="tv"
            />
          </div>
          <div class="season-meta">
            <h3>{{ activeSeason.name }}</h3>
            <div class="season-stats">
              <span v-for="meta in activeSeasonMeta" :key="meta" class="stat-badge">{{
                meta
              }}</span>
              <span v-if="resumeEpisode" class="stat-badge accent">继续观看</span>
            </div>
            <p v-if="activeSeason.overview" class="season-overview">{{ activeSeason.overview }}</p>
            <button
              v-if="resumeEpisode"
              class="du-btn du-btn-sm du-btn-primary continue-play-btn"
              type="button"
              @click="playResumeEpisode"
            >
              <BaseIcon name="play" :size="14" fill />继续
              {{ episodeCode(resumeEpisode.seasonNumber, resumeEpisode.episodeNumber) }}
            </button>
          </div>
        </div>

        <div v-if="showRanges" class="episode-ranges" aria-label="集数范围">
          <button
            v-for="(range, index) in episodeRanges"
            :key="range.label"
            class="range-tab"
            :class="{ active: activeRangeIndex === index }"
            type="button"
            @click="activeRangeIndex = index"
          >
            {{ range.label }}
          </button>
        </div>

        <ol class="episode-list">
          <li
            v-for="episode in visibleEpisodes"
            :key="episode.id"
            class="episode-card"
            :class="{ watched: episode.played, current: episode.current, viewing: episode.viewing }"
            role="button"
            tabindex="0"
            :aria-label="`播放 ${episode.code} ${episode.title}`"
            @click="playEpisode(episode)"
            @keydown.enter.self.prevent="playEpisode(episode)"
          >
            <div class="ep-thumb">
              <MediaPoster
                :src="episode.poster ?? props.backdrop"
                :title="`${props.showTitle} ${episode.title}`"
                ratio="wide"
                icon="tv-minimal"
              />
              <span class="ep-play"><BaseIcon name="play" :size="16" fill /></span>
              <span v-if="episode.played" class="ep-flag"
                ><BaseIcon name="check" :size="12" :stroke-width="3"
              /></span>
              <span v-if="episode.progressPercent != null && !episode.played" class="ep-progress">
                <span :style="{ width: `${episode.progressPercent}%` }" />
              </span>
            </div>
            <div class="ep-info">
              <div class="ep-title-row">
                <span class="ep-code">{{ episode.code }}</span>
                <strong class="episode-title"
                  >{{ episode.displayNumber }}. {{ episode.title }}</strong
                >
                <span v-if="episode.current" class="ep-tag">继续观看</span>
                <span v-else-if="episode.viewing" class="ep-tag dim">当前</span>
              </div>
              <div class="ep-meta c-meta">
                <span v-if="episode.durationText">{{ episode.durationText }}</span>
                <span v-if="episode.durationText && episode.airDate" class="c-dot" />
                <span v-if="episode.airDate">{{ episode.airDate }}</span>
              </div>
              <p v-if="episode.summary" class="episode-summary" :title="episode.summary">
                {{ episode.summary }}
              </p>
            </div>
            <button
              class="ep-info-btn"
              type="button"
              title="媒体信息"
              aria-label="媒体信息"
              @click.stop="showInfo(episode)"
            >
              <BaseIcon name="info" :size="16" />
            </button>
          </li>
        </ol>
      </div>
    </div>
  </section>
</template>

<style scoped lang="scss">
.seasons-section {
  display: flex;
  flex-direction: column;
  gap: 12px;
  scroll-margin-top: calc(var(--ui-header-height) + 16px);
}

.sec-head {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 12px;
  margin-bottom: 12px;

  .lead,
  .n {
    color: var(--ui-text-subtle);
  }

  .n {
    font-size: 12px;
    font-weight: 500;
  }
}

.continue-play-btn {
  border-radius: var(--ui-radius-sm);
  font-family: inherit;
}

/* ---------- 分集列表 ---------- */
.episode-list {
  display: flex;
  flex-direction: column;
  margin: 0;
  padding: 0;
  overflow: hidden;
  border: 1px solid var(--ui-border);
  border-radius: var(--ui-radius-lg);
  background: var(--ui-surface);
  list-style: none;
}

.episode-card {
  position: relative;
  display: flex;
  align-items: flex-start;
  gap: 14px;
  padding: 12px;
  border-bottom: 1px solid var(--ui-border);
  cursor: pointer;
  outline: none;
  transition: background var(--ui-duration-fast);

  &:last-child {
    border-bottom: 0;
  }

  &:hover,
  &:focus-visible {
    background: var(--ui-surface-hover);

    .ep-play {
      opacity: 1;
    }

    .episode-title {
      color: var(--ui-accent-hover);
    }
  }

  &:focus-visible {
    box-shadow: inset 0 0 0 2px var(--ui-focus);
  }

  &.current,
  &.viewing {
    background: rgb(var(--ui-accent-channel) / 6%);
    box-shadow: inset 3px 0 0 var(--ui-accent);
  }

  &.watched .episode-title {
    color: var(--ui-text-muted);
  }
}

.ep-thumb {
  position: relative;
  flex: 0 0 auto;
  width: 176px;
  overflow: hidden;
  border: 1px solid var(--ui-border);
  border-radius: var(--ui-radius-sm);
  background: var(--ui-surface-raised);
}

.ep-play {
  position: absolute;
  inset: 0;
  display: grid;
  place-items: center;
  background: rgb(0 0 0 / 38%);
  color: #fff;
  opacity: 0;
  transition: opacity var(--ui-duration-fast);
}

.ep-flag {
  position: absolute;
  top: 5px;
  left: 5px;
  width: 20px;
  height: 20px;
  display: grid;
  place-items: center;
  border-radius: 50%;
  background: var(--ui-accent);
  color: var(--ui-accent-contrast);
}

.ep-progress {
  position: absolute;
  inset: auto 0 0 0;
  height: 3px;
  background: rgb(255 255 255 / 18%);

  span {
    display: block;
    height: 100%;
    background: var(--ui-accent);
  }
}

.ep-info {
  flex: 1;
  min-width: 0;
}

.ep-title-row {
  display: flex;
  align-items: center;
  gap: 8px;
  min-width: 0;
}

.ep-code {
  flex: 0 0 auto;
  height: 19px;
  padding: 0 5px;
  display: inline-flex;
  align-items: center;
  border: 1px solid var(--ui-border-strong);
  border-radius: 4px;
  color: var(--ui-text-muted);
  font-family: var(--ui-font-mono);
  font-size: 10.5px;
  font-weight: 600;
}

.episode-title {
  min-width: 0;
  overflow: hidden;
  color: var(--ui-text);
  font-size: 13.5px;
  font-weight: 600;
  white-space: nowrap;
  text-overflow: ellipsis;
  transition: color var(--ui-duration-fast);
}

.ep-tag {
  flex: 0 0 auto;
  height: 18px;
  padding: 0 6px;
  display: inline-flex;
  align-items: center;
  border-radius: 4px;
  background: var(--ui-accent);
  color: var(--ui-accent-contrast);
  font-size: 10.5px;
  font-weight: 700;

  &.dim {
    background: var(--ui-surface-hover);
    color: var(--ui-text-muted);
  }
}

.ep-meta {
  display: flex;
  align-items: center;
  gap: 6px;
  margin-top: 4px;
}

.episode-summary {
  display: -webkit-box;
  margin: 6px 0 0;
  overflow: hidden;
  color: var(--ui-text-subtle);
  font-size: 12.5px;
  line-height: 1.6;
  -webkit-line-clamp: 2;
  -webkit-box-orient: vertical;
}

.ep-info-btn {
  flex: 0 0 auto;
  width: 32px;
  height: 32px;
  display: grid;
  place-items: center;
  border: 1px solid transparent;
  border-radius: var(--ui-radius-sm);
  background: transparent;
  color: var(--ui-text-subtle);
  cursor: pointer;

  &:hover {
    border-color: var(--ui-border-strong);
    background: var(--ui-surface-raised);
    color: var(--ui-text);
  }
}

/* ---------- 季网格 ---------- */
.seasons-grid {
  display: grid;
  grid-template-columns: repeat(auto-fill, minmax(150px, 1fr));
  gap: 16px;
}

.season-card-item {
  display: flex;
  flex-direction: column;
  gap: 8px;
  padding: 0;
  border: 0;
  background: transparent;
  color: inherit;
  text-align: left;
  cursor: pointer;

  &:hover {
    .season-poster {
      border-color: rgb(var(--ui-accent-channel) / 60%);
      transform: translateY(-2px);
    }

    h4 {
      color: var(--ui-accent-hover);
    }
  }
}

.season-poster {
  overflow: hidden;
  border: 1px solid var(--ui-border);
  border-radius: var(--ui-radius-md);
  transition:
    border-color var(--ui-duration-fast),
    transform var(--ui-duration-base) var(--ui-ease-out);
}

.season-info {
  h4 {
    margin: 0;
    color: var(--ui-text);
    font-size: 13.5px;
    font-weight: 600;
  }

  .c-meta {
    display: flex;
    gap: 8px;
    margin-top: 2px;
  }
}

/* ---------- 分集视图头 ---------- */
.episodes-view-header {
  display: flex;
  align-items: center;
  gap: 10px;
  margin-bottom: 14px;
}

.back-to-seasons-btn {
  flex: 0 0 auto;
  display: inline-flex;
  align-items: center;
  gap: 5px;
  height: 30px;
  padding: 0 10px;
  border: 1px solid var(--ui-border);
  border-radius: var(--ui-radius-sm);
  background: var(--ui-surface);
  color: var(--ui-text-muted);
  font-size: 12.5px;
  cursor: pointer;

  &:hover {
    color: var(--ui-text);
    border-color: var(--ui-border-strong);
  }
}

.season-tabs {
  display: flex;
  gap: 4px;
  overflow-x: auto;
  scrollbar-width: none;

  &::-webkit-scrollbar {
    display: none;
  }
}

.season-tab,
.range-tab {
  flex: 0 0 auto;
  height: 30px;
  padding: 0 12px;
  border: 1px solid transparent;
  border-radius: var(--ui-radius-sm);
  background: transparent;
  color: var(--ui-text-muted);
  font-size: 12.5px;
  font-weight: 500;
  white-space: nowrap;
  cursor: pointer;

  &:hover {
    color: var(--ui-text);
    background: var(--ui-surface-hover);
  }

  &.active {
    border-color: rgb(var(--ui-accent-channel) / 50%);
    background: rgb(var(--ui-accent-channel) / 12%);
    color: var(--ui-accent-hover);
  }
}

.season-banner {
  display: flex;
  gap: 16px;
  margin-bottom: 14px;
  padding: 14px;
  border: 1px solid var(--ui-border);
  border-radius: var(--ui-radius-lg);
  background: var(--ui-surface);
}

.season-poster-wrap {
  flex: 0 0 auto;
  width: 96px;
  overflow: hidden;
  border: 1px solid var(--ui-border);
  border-radius: var(--ui-radius-sm);
}

.season-meta {
  flex: 1;
  min-width: 0;
  display: flex;
  flex-direction: column;
  align-items: flex-start;
  gap: 8px;

  h3 {
    margin: 0;
    color: var(--ui-text);
    font-size: 16px;
    font-weight: 600;
  }
}

.season-stats {
  display: flex;
  flex-wrap: wrap;
  gap: 6px;
}

.stat-badge {
  height: 20px;
  padding: 0 7px;
  display: inline-flex;
  align-items: center;
  border: 1px solid var(--ui-border-strong);
  border-radius: 4px;
  color: var(--ui-text-muted);
  font-size: 11px;
  font-weight: 600;

  &.accent {
    border-color: rgb(var(--ui-accent-channel) / 55%);
    color: var(--ui-accent-hover);
  }
}

.season-overview {
  display: -webkit-box;
  margin: 0;
  overflow: hidden;
  color: var(--ui-text-muted);
  font-size: 12.5px;
  line-height: 1.6;
  -webkit-line-clamp: 3;
  -webkit-box-orient: vertical;
}

.episode-ranges {
  display: flex;
  flex-wrap: wrap;
  gap: 4px;
  margin-bottom: 12px;
}

@media (max-width: 600px) {
  .ep-thumb {
    width: 120px;
  }

  .episode-summary {
    display: none;
  }

  .seasons-grid {
    grid-template-columns: repeat(auto-fill, minmax(110px, 1fr));
  }
}
</style>
