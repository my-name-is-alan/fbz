<script setup lang="ts">
import { ref, computed, watch, nextTick, onMounted } from "vue";
import type { SeasonInfo } from "@/types/media.ts";
import { imageUrl } from "@/service/modules/tmdb.ts";

interface Props {
  seasons: SeasonInfo[];
  seriesId?: number | string;
  defaultSeason?: number;
  watchedEpisode?: number;
  showTitle: string;
  backdrop?: string;
  rating?: number | null;
}

interface EpisodeItem {
  id: string;
  seasonNumber: number;
  number: number;
  title: string;
  runtime: number;
  airDate: string;
  summary: string;
  watched: boolean;
  current: boolean;
}

interface EpisodeRange {
  label: string;
  episodes: EpisodeItem[];
}

interface SeasonCard {
  season_number: number;
  name: string;
  episode_count: number;
  airYear?: number;
  poster?: string;
  overview: string;
  rating: string;
  hasHistory: boolean;
}

interface EpisodePlayPayload {
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

const RANGE_SIZE = 50;
const PREVIEW_LIMIT = 8;

const props = defineProps<Props>();
const emit = defineEmits<{
  playEpisode: [episode: EpisodePlayPayload];
}>();

const selectedSeasonNumber = ref<number>();
const activeRangeIndex = ref(0);
const viewState = ref<"seasons" | "episodes">("seasons");
const isPopupOpen = ref(false);
const popupContentRef = ref<HTMLElement | null>(null);
const sectionRef = ref<HTMLElement | null>(null);

let allowScroll = false;

onMounted(() => {
  nextTick(() => {
    allowScroll = true;
  });
});

watch(viewState, () => {
  if (!allowScroll) return;
  nextTick(() => {
    if (sectionRef.value) {
      const headerH =
        parseInt(getComputedStyle(document.documentElement).getPropertyValue("--header-h")) || 60;
      const elementPosition = sectionRef.value.getBoundingClientRect().top + window.scrollY;
      const offsetPosition = elementPosition - headerH - 16;

      if (typeof window.scrollTo === "function") {
        window.scrollTo({
          top: offsetPosition,
          behavior: "smooth",
        });
      }
    }
  });
});

let lastSeriesId: number | string | undefined = undefined;
let isInitialized = false;

// Watch for seriesId and seasons to initialize/reset state ONLY when the TV show changes
watch(
  [() => props.seriesId, () => props.seasons],
  ([newId, seasons]) => {
    if (!seasons?.length) return;
    if (!isInitialized || newId !== lastSeriesId) {
      isInitialized = true;
      lastSeriesId = newId;
      const defSeason = props.defaultSeason;
      if (defSeason != null && seasons.some((s) => s.season_number === defSeason)) {
        selectedSeasonNumber.value = defSeason;
        viewState.value = "episodes";
        if (props.watchedEpisode != null) {
          activeRangeIndex.value = Math.floor((props.watchedEpisode - 1) / RANGE_SIZE);
        }
      } else {
        selectedSeasonNumber.value = seasons[0].season_number;
        viewState.value = seasons.length > 1 ? "seasons" : "episodes";
      }
    }
  },
  { immediate: true },
);

// Watch for selectedSeasonNumber changes to reset range index
watch(selectedSeasonNumber, (newVal) => {
  if (newVal === props.defaultSeason && props.watchedEpisode != null) {
    activeRangeIndex.value = Math.floor((props.watchedEpisode - 1) / RANGE_SIZE);
  } else {
    activeRangeIndex.value = 0;
  }
});

const seasonCards = computed<SeasonCard[]>(() =>
  props.seasons.map((season, index) => {
    const airYear = season.air_date ? new Date(season.air_date).getFullYear() : undefined;
    const ratingBase = props.rating ?? 8.8;
    const rating = Math.max(6, Math.min(9.8, ratingBase - (index % 4) * 0.1)).toFixed(1);
    return {
      ...season,
      airYear,
      poster: imageUrl(season.poster_path, "w500"),
      rating,
      hasHistory: season.season_number === props.defaultSeason && props.watchedEpisode != null,
    };
  }),
);

const activeSeason = computed(
  () =>
    props.seasons.find((season) => season.season_number === selectedSeasonNumber.value) ??
    props.seasons[0],
);

const activeSeasonCard = computed(() =>
  seasonCards.value.find((season) => season.season_number === activeSeason.value?.season_number),
);

const activeSeasonPoster = computed(() => imageUrl(activeSeason.value?.poster_path, "w500"));
const activeSeasonBackdrop = computed(() => props.backdrop ?? activeSeasonPoster.value);

const activeSeasonMeta = computed(() => {
  const season = activeSeason.value;
  if (!season) return [];
  const airYear = season.air_date ? new Date(season.air_date).getFullYear() : undefined;
  return [airYear ? `${airYear}年` : "", `${season.episode_count} 集`].filter(Boolean);
});

const activeEpisodes = computed(() => {
  if (!activeSeason.value) return [];
  return buildEpisodes(activeSeason.value);
});

const showRanges = computed(() => activeEpisodes.value.length > RANGE_SIZE);

const episodeRanges = computed<EpisodeRange[]>(() => {
  if (!showRanges.value) return [];

  const ranges: EpisodeRange[] = [];
  for (let start = 0; start < activeEpisodes.value.length; start += RANGE_SIZE) {
    const group = activeEpisodes.value.slice(start, start + RANGE_SIZE);
    const first = group[0]!.number;
    const last = group[group.length - 1]!.number;
    ranges.push({ label: `E${first}-E${last}`, episodes: group });
  }
  return ranges;
});

const visibleEpisodes = computed(() => {
  if (!showRanges.value) return activeEpisodes.value;
  return (
    episodeRanges.value[activeRangeIndex.value]?.episodes ??
    activeEpisodes.value.slice(0, RANGE_SIZE)
  );
});

// Up to 8 episodes for page preview (used in multi-season view)
const pageVisibleEpisodes = computed(() => {
  return visibleEpisodes.value.slice(0, PREVIEW_LIMIT);
});

function buildEpisodes(season: SeasonInfo): EpisodeItem[] {
  return Array.from({ length: season.episode_count }, (_, index) => {
    const number = index + 1;
    const isHistorySeason = season.season_number === props.defaultSeason;
    return {
      id: `${props.seriesId ?? "tv"}-s${season.season_number}-e${number}`,
      seasonNumber: season.season_number,
      number,
      title: `第 ${number} 集`,
      runtime: 38 + ((number * 7) % 18),
      airDate: formatEpisodeDate(season.air_date, number),
      summary: episodeSummary(number),
      watched: isHistorySeason && props.watchedEpisode != null && number < props.watchedEpisode,
      current: isHistorySeason && props.watchedEpisode != null && number === props.watchedEpisode,
    };
  });
}

function formatEpisodeDate(seasonDate: string | null, episodeNumber: number) {
  if (!seasonDate) return "未知日期";
  const date = new Date(seasonDate);
  date.setDate(date.getDate() + episodeNumber - 1);
  return `${date.getFullYear()}年${date.getMonth() + 1}月${date.getDate()}日`;
}

function episodeSummary(episodeNumber: number) {
  return `第 ${episodeNumber} 集故事线继续深入，伴随着新线索逐渐浮出水面，剧中角色们的矛盾纠葛与剧情张力被推向了新的高峰，精彩不容错过。`;
}

function selectSeason(seasonNumber: number) {
  selectedSeasonNumber.value = seasonNumber;
  viewState.value = "episodes";
}

function playEpisode(episode: EpisodeItem) {
  emit("playEpisode", {
    seasonNumber: episode.seasonNumber,
    episodeNumber: episode.number,
    title: episode.title,
    runtime: episode.runtime,
    poster: activeSeasonPoster.value,
    backdrop: activeSeasonBackdrop.value,
    episodes: activeEpisodes.value.map((entry) => ({
      id: entry.id,
      seasonNumber: entry.seasonNumber,
      episodeNumber: entry.number,
      title: entry.title,
      runtime: entry.runtime,
      poster: activeSeasonPoster.value,
      backdrop: activeSeasonBackdrop.value,
    })),
  });
}

function playWatchedEpisode() {
  if (!activeEpisodes.value.length || props.watchedEpisode == null) return;
  const episode = activeEpisodes.value.find((e) => e.number === props.watchedEpisode);
  if (episode) {
    playEpisode(episode);
  }
}

function openEpisodesPopup() {
  isPopupOpen.value = true;
  document.body.style.overflow = "hidden";
  nextTick(() => {
    if (popupContentRef.value) {
      const activeCard = popupContentRef.value.querySelector(".popup-episode-card.current");
      if (activeCard) {
        activeCard.scrollIntoView({ behavior: "smooth", block: "center" });
      }
    }
  });
}

function closeEpisodesPopup() {
  isPopupOpen.value = false;
  document.body.style.overflow = "";
}
</script>

<template>
  <section v-if="props.seasons.length" ref="sectionRef" class="seasons-section">
    <!-- Case 1: Single Season Flat Layout (Only horizontal scrolling, no wrapping) -->
    <div v-if="props.seasons.length === 1" class="single-season-scroller-container">
      <header class="single-season-header">
        <h2 class="section-title">{{ props.seasons[0].name }}</h2>
      </header>

      <BaseScroller class="scroller" col-width="var(--episode-col-width)" gap="var(--fbz-space-4)">
        <button
          v-for="episode in activeEpisodes"
          :key="episode.id"
          class="episode-card"
          :class="{ watched: episode.watched, current: episode.current }"
          type="button"
          @click="() => playEpisode(episode)"
        >
          <div class="episode-thumb-wrap">
            <MediaPoster
              :src="activeSeasonBackdrop"
              :title="`${props.showTitle} ${episode.title}`"
              ratio="wide"
              class="episode-poster"
            />
            <div class="episode-play-overlay">
              <span class="play-icon">▶</span>
            </div>
            <!-- Progress indicator bar for currently active/watching episode -->
            <div v-if="episode.current" class="episode-active-progress">
              <span class="active-bar" />
            </div>
          </div>

          <div class="episode-info">
            <div class="episode-header-row">
              <strong class="episode-title">{{ episode.number }}. {{ episode.title }}</strong>
              <span class="episode-duration">{{ episode.runtime }}分钟</span>
            </div>
            <span class="episode-airdate">{{ episode.airDate }}</span>
            <p class="episode-summary" :title="episode.summary">{{ episode.summary }}</p>
          </div>
        </button>
      </BaseScroller>
    </div>

    <!-- Case 2: Multiple Seasons Layout -->
    <div v-else class="multi-seasons-container">
      <!-- View 1: Season Grid View -->
      <div v-if="viewState === 'seasons'" class="seasons-grid-container">
        <header class="seasons-grid-header">
          <h2 class="section-title">季列表</h2>
        </header>
        <div class="seasons-grid">
          <button
            v-for="season in seasonCards"
            :key="season.season_number"
            class="season-card-item"
            type="button"
            @click="selectSeason(season.season_number)"
          >
            <div class="season-poster-wrap-grid">
              <MediaPoster :src="season.poster" :title="season.name" ratio="poster" />
              <span v-if="season.hasHistory" class="history-badge-grid">继续观看</span>
            </div>
            <div class="season-info-grid">
              <h4>{{ season.name }}</h4>
              <div class="season-meta-grid">
                <span>{{ season.airYear ? season.airYear + "年" : "" }}</span>
                <span>{{ season.episode_count }} 集</span>
              </div>
            </div>
          </button>
        </div>
      </div>

      <!-- View 2: Episode List View -->
      <div v-else-if="viewState === 'episodes'" class="episodes-container">
        <header class="episodes-view-header">
          <button class="back-to-seasons-btn" type="button" @click="viewState = 'seasons'">
            <span class="arrow">←</span> 返回季列表
          </button>
          <span class="current-season-title">{{ activeSeason?.name }}</span>
        </header>

        <!-- Selected Season Summary Banner -->
        <div v-if="activeSeason" class="season-banner">
          <div class="season-poster-wrap">
            <MediaPoster :src="activeSeasonPoster" :title="activeSeason.name" ratio="poster" />
          </div>
          <div class="season-meta">
            <div class="season-title-row">
              <h3>{{ activeSeason.name }}</h3>
              <span v-if="activeSeasonCard?.rating" class="season-rating">
                ★ {{ activeSeasonCard.rating }}
              </span>
            </div>
            <div class="season-stats">
              <span v-for="meta in activeSeasonMeta" :key="meta" class="stat-badge">
                {{ meta }}
              </span>
              <span
                v-if="activeSeasonCard?.hasHistory && props.watchedEpisode"
                class="continue-badge"
              >
                上次看到第 {{ props.watchedEpisode }} 集
              </span>
            </div>
            <p class="season-overview">
              {{
                activeSeason.overview ||
                "本季继续展开故事主线，人物关系、冲突 and 关键事件逐步推进。"
              }}
            </p>
            <button
              v-if="activeSeasonCard?.hasHistory && props.watchedEpisode"
              class="continue-play-btn"
              type="button"
              @click="playWatchedEpisode"
            >
              ▶ 继续播放第 {{ props.watchedEpisode }} 集
            </button>
          </div>
        </div>

        <!-- Episode Range Selector for long seasons (episodes > 50) -->
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

        <!-- Inline Episode Preview Scroller (up to 8 episodes, horizontal scroll) -->
        <div class="episodes-scroller-wrap">
          <BaseScroller
            class="scroller"
            col-width="var(--episode-col-width)"
            gap="var(--fbz-space-4)"
          >
            <button
              v-for="episode in pageVisibleEpisodes"
              :key="episode.id"
              class="episode-card"
              :class="{ watched: episode.watched, current: episode.current }"
              type="button"
              @click="() => playEpisode(episode)"
            >
              <div class="episode-thumb-wrap">
                <MediaPoster
                  :src="activeSeasonBackdrop"
                  :title="`${props.showTitle} ${episode.title}`"
                  ratio="wide"
                  class="episode-poster"
                />
                <div class="episode-play-overlay">
                  <span class="play-icon">▶</span>
                </div>
                <!-- Progress indicator bar for currently active/watching episode -->
                <div v-if="episode.current" class="episode-active-progress">
                  <span class="active-bar" />
                </div>
              </div>

              <div class="episode-info">
                <div class="episode-header-row">
                  <strong class="episode-title">{{ episode.number }}. {{ episode.title }}</strong>
                  <span class="episode-duration">{{ episode.runtime }}分钟</span>
                </div>
                <span class="episode-airdate">{{ episode.airDate }}</span>
                <p class="episode-summary" :title="episode.summary">{{ episode.summary }}</p>
              </div>
            </button>
          </BaseScroller>
        </div>

        <!-- View All Episodes Trigger -->
        <div v-if="visibleEpisodes.length > PREVIEW_LIMIT" class="view-all-container">
          <button class="view-all-btn" type="button" @click="openEpisodesPopup">
            查看全部 (共 {{ visibleEpisodes.length }} 集)
          </button>
        </div>
      </div>
    </div>
  </section>

  <!-- Bottom-up Popup Drawer for View All -->
  <Teleport to="body">
    <Transition name="drawer-fade">
      <div v-if="isPopupOpen" class="popup-backdrop" @click="closeEpisodesPopup">
        <Transition name="drawer-slide">
          <div class="popup-drawer" @click.stop>
            <header class="popup-header">
              <div class="popup-title-area">
                <h3>{{ activeSeason?.name }} - 全部单集</h3>
                <span class="popup-season-stats">{{ activeSeasonMeta.join(" · ") }}</span>
              </div>
              <button class="popup-close-btn" type="button" @click="closeEpisodesPopup">
                <span class="close-icon">✕</span>
              </button>
            </header>

            <!-- Range tabs if episodes count > 50 inside the popup -->
            <div v-if="showRanges" class="popup-episode-ranges">
              <button
                v-for="(range, index) in episodeRanges"
                :key="'popup-' + range.label"
                class="range-tab"
                :class="{ active: activeRangeIndex === index }"
                type="button"
                @click="activeRangeIndex = index"
              >
                {{ range.label }}
              </button>
            </div>

            <div ref="popupContentRef" class="popup-content">
              <div class="popup-episodes-grid">
                <button
                  v-for="episode in visibleEpisodes"
                  :key="'popup-' + episode.id"
                  class="episode-card popup-episode-card"
                  :class="{ watched: episode.watched, current: episode.current }"
                  type="button"
                  @click="
                    () => {
                      playEpisode(episode);
                      closeEpisodesPopup();
                    }
                  "
                >
                  <div class="episode-thumb-wrap">
                    <MediaPoster
                      :src="activeSeasonBackdrop"
                      :title="`${props.showTitle} ${episode.title}`"
                      ratio="wide"
                      class="episode-poster"
                    />
                    <div class="episode-play-overlay">
                      <span class="play-icon">▶</span>
                    </div>
                    <div v-if="episode.current" class="episode-active-progress">
                      <span class="active-bar" />
                    </div>
                  </div>

                  <div class="episode-info">
                    <div class="episode-header-row">
                      <strong class="episode-title"
                        >{{ episode.number }}. {{ episode.title }}</strong
                      >
                      <span class="episode-duration">{{ episode.runtime }}分钟</span>
                    </div>
                    <span class="episode-airdate">{{ episode.airDate }}</span>
                    <p class="episode-summary" :title="episode.summary">{{ episode.summary }}</p>
                  </div>
                </button>
              </div>
            </div>
          </div>
        </Transition>
      </div>
    </Transition>
  </Teleport>
</template>

<style scoped lang="scss">
.seasons-section {
  position: relative;
  max-width: 1280px;
  margin: 0 auto;
  padding: 0 var(--fbz-space-8) var(--fbz-space-8);
}

/* Single Season flat horizontal layout */
.single-season-scroller-container {
  --episode-col-width: 280px;
  max-width: 1280px;
  margin: 0 auto;
  padding: 0 var(--fbz-space-8) var(--fbz-space-8);

  :deep(.track) {
    padding: 10px 0;
  }
}

.single-season-header {
  border-bottom: 1px solid var(--fbz-color-line-soft);
  padding-bottom: var(--fbz-space-3);
  margin-bottom: var(--fbz-space-4);
}

/* Multi-Season episodes scroller */
.episodes-scroller-wrap {
  --episode-col-width: 280px;
  margin-bottom: var(--fbz-space-4);

  :deep(.track) {
    padding: 10px 0;
  }
}

/* Season Grid View */
.seasons-grid-header {
  border-bottom: 1px solid var(--fbz-color-line-soft);
  padding-bottom: var(--fbz-space-3);
  margin-bottom: var(--fbz-space-4);
}

.seasons-grid {
  display: grid;
  grid-template-columns: repeat(auto-fill, minmax(160px, 1fr));
  gap: var(--fbz-space-5);
}

@media (max-width: 600px) {
  .single-season-scroller-container {
    --episode-col-width: 220px;
    padding: 0 var(--fbz-space-4) var(--fbz-space-5);
  }

  .episodes-scroller-wrap {
    --episode-col-width: 220px;
  }

  .seasons-grid {
    grid-template-columns: repeat(auto-fill, minmax(120px, 1fr));
    gap: var(--fbz-space-3);
  }
}

.season-card-item {
  display: flex;
  flex-direction: column;
  background: var(--fbz-color-panel);
  border: 1px solid var(--fbz-color-line-soft);
  border-radius: var(--fbz-radius-card);
  overflow: hidden;
  cursor: pointer;
  text-align: left;
  color: inherit;
  transition:
    transform var(--fbz-motion-base),
    border-color var(--fbz-motion-base),
    box-shadow var(--fbz-motion-base);
  padding: 0;

  &:hover {
    transform: translateY(-4px);
    border-color: var(--fbz-color-brand-500);
    box-shadow: 0 8px 24px rgba(0, 0, 0, 0.3);
  }
}

.season-poster-wrap-grid {
  position: relative;
  width: 100%;
  aspect-ratio: 2 / 3;
  overflow: hidden;
  background: var(--fbz-color-panel-strong);
}

.history-badge-grid {
  position: absolute;
  top: var(--fbz-space-2);
  left: var(--fbz-space-2);
  background: var(--fbz-color-brand-500);
  color: #07120a;
  font-size: 10px;
  font-weight: 800;
  padding: 2px 6px;
  border-radius: 4px;
  box-shadow: 0 2px 4px rgba(0, 0, 0, 0.4);
}

.season-info-grid {
  padding: var(--fbz-space-3);

  h4 {
    margin: 0 0 var(--fbz-space-1);
    font-size: var(--fbz-font-size-md);
    font-weight: 800;
    color: var(--fbz-color-text);
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }
}

.season-meta-grid {
  display: flex;
  justify-content: space-between;
  font-size: var(--fbz-font-size-xs);
  color: var(--fbz-color-text-muted);
  font-weight: 600;
}

/* Episode View Header */
.episodes-view-header {
  display: flex;
  align-items: center;
  justify-content: space-between;
  margin-top: var(--fbz-space-2);
  margin-bottom: var(--fbz-space-4);
  gap: var(--fbz-space-3);
  border-bottom: 1px solid var(--fbz-color-line);
  padding-bottom: var(--fbz-space-3);
}

.back-to-seasons-btn {
  display: inline-flex;
  align-items: center;
  gap: var(--fbz-space-2);
  background: none;
  border: none;
  color: var(--fbz-color-text-soft);
  font-size: var(--fbz-font-size-sm);
  font-weight: 700;
  cursor: pointer;
  padding: var(--fbz-space-2) 0;
  transition: color var(--fbz-motion-fast);

  &:hover {
    color: var(--fbz-color-brand-500);
  }

  .arrow {
    font-size: 16px;
  }
}

.current-season-title {
  font-size: var(--fbz-font-size-sm);
  font-weight: 800;
  color: var(--fbz-color-text);
}

.section-title {
  margin: 0;
  font-size: 18px;
  font-weight: 800;
  letter-spacing: -0.2px;
}

/* Season Banner styling */
.season-banner {
  display: grid;
  grid-template-columns: 100px 1fr;
  gap: var(--fbz-space-5);
  background: linear-gradient(
    135deg,
    var(--fbz-color-panel) 0%,
    color-mix(in srgb, var(--fbz-color-panel) 60%, transparent) 100%
  );
  border: 1px solid var(--fbz-color-line-soft);
  border-radius: var(--fbz-radius-card);
  padding: var(--fbz-space-4);
  margin-bottom: var(--fbz-space-5);
  box-shadow: 0 4px 14px rgba(0, 0, 0, 0.15);
}

.season-poster-wrap {
  width: 100px;
  border-radius: var(--fbz-radius-card);
  overflow: hidden;
  box-shadow: 0 8px 16px rgba(0, 0, 0, 0.3);
}

.season-meta {
  display: flex;
  flex-direction: column;
  justify-content: center;
}

.season-title-row {
  display: flex;
  align-items: center;
  gap: var(--fbz-space-3);
  margin-bottom: var(--fbz-space-2);

  h3 {
    margin: 0;
    font-size: var(--fbz-font-size-lg);
    font-weight: 800;
    color: var(--fbz-color-text);
  }
}

.season-rating {
  font-family: var(--fbz-font-display);
  font-size: var(--fbz-font-size-sm);
  font-weight: 700;
  color: var(--fbz-color-brand-500);
  background: color-mix(in srgb, var(--fbz-color-brand-500) 8%, transparent);
  padding: 2px 6px;
  border-radius: 3px;
  border: 1px solid color-mix(in srgb, var(--fbz-color-brand-500) 20%, transparent);
}

.season-stats {
  display: flex;
  flex-wrap: wrap;
  align-items: center;
  gap: var(--fbz-space-2);
  margin-bottom: var(--fbz-space-3);
}

.stat-badge {
  font-size: var(--fbz-font-size-xs);
  color: var(--fbz-color-text-muted);
  background: var(--fbz-color-panel-strong);
  padding: 2px 8px;
  border-radius: 4px;
  font-weight: 600;
}

.continue-badge {
  font-size: var(--fbz-font-size-xs);
  color: var(--fbz-color-brand-500);
  font-weight: 700;
}

.continue-play-btn {
  margin-top: var(--fbz-space-3);
  align-self: flex-start;
  height: 36px;
  padding: 0 var(--fbz-space-4);
  border-radius: var(--fbz-radius-control);
  background: var(--fbz-color-brand-500);
  border: none;
  color: #07120a;
  font-size: var(--fbz-font-size-sm);
  font-weight: 800;
  cursor: pointer;
  display: inline-flex;
  align-items: center;
  gap: var(--fbz-space-2);
  transition:
    transform var(--fbz-motion-fast),
    box-shadow var(--fbz-motion-fast);

  &:hover {
    transform: scale(1.02);
    box-shadow: 0 4px 12px color-mix(in srgb, var(--fbz-color-brand-500) 25%, transparent);
  }

  &:active {
    transform: scale(0.98);
  }
}

.season-overview {
  margin: 0;
  font-size: var(--fbz-font-size-sm);
  line-height: 1.6;
  color: var(--fbz-color-text-soft);
  display: -webkit-box;
  overflow: hidden;
  -webkit-line-clamp: 2;
  -webkit-box-orient: vertical;
}

/* Episode Ranges */
.episode-ranges {
  display: flex;
  gap: var(--fbz-space-2);
  overflow-x: auto;
  padding: var(--fbz-space-2) 0;
  margin-bottom: var(--fbz-space-4);
  scrollbar-width: none;

  &::-webkit-scrollbar {
    display: none;
  }
}

.range-tab {
  flex: 0 0 auto;
  height: 28px;
  padding: 0 var(--fbz-space-3);
  border: 1px solid var(--fbz-color-line);
  border-radius: 999px;
  background: var(--fbz-color-panel-strong);
  color: var(--fbz-color-text-muted);
  font-size: var(--fbz-font-size-xs);
  font-weight: 700;
  cursor: pointer;
  transition:
    background var(--fbz-motion-fast),
    color var(--fbz-motion-fast);

  &.active {
    background: var(--fbz-color-brand-500);
    color: #07120a;
    border-color: var(--fbz-color-brand-500);
  }
}

.episode-card {
  display: flex;
  flex-direction: column;
  background: var(--fbz-color-panel);
  border: 1px solid var(--fbz-color-line-soft);
  border-radius: var(--fbz-radius-card);
  padding: 0;
  overflow: hidden;
  text-align: left;
  color: inherit;
  cursor: pointer;
  box-shadow: 0 4px 10px rgba(0, 0, 0, 0.12);
  transition:
    border-color var(--fbz-motion-base),
    box-shadow var(--fbz-motion-base),
    transform var(--fbz-motion-base);
  width: 100%;

  &:hover {
    border-color: var(--fbz-color-line-bright);
    box-shadow: 0 8px 20px rgba(0, 0, 0, 0.24);
    transform: translateY(-2px);
  }

  &.watched {
    opacity: 0.6;
  }

  &.current {
    border-color: var(--fbz-color-brand-500);
    box-shadow: 0 8px 24px color-mix(in srgb, var(--fbz-color-brand-500) 12%, transparent);
    background: color-mix(in srgb, var(--fbz-color-brand-500) 4%, var(--fbz-color-panel));
  }
}

.episode-thumb-wrap {
  position: relative;
  width: 100%;
  aspect-ratio: 16 / 9;
  background: var(--fbz-color-panel-strong);
  overflow: hidden;

  :deep(.media-poster) {
    border-radius: 0;
  }
}

.episode-poster {
  width: 100%;
  height: 100%;
  object-fit: cover;
  transition: transform var(--fbz-motion-slow) ease;

  .episode-card:hover & {
    transform: scale(1.04);
  }
}

.episode-play-overlay {
  position: absolute;
  inset: 0;
  display: grid;
  place-content: center;
  background: rgba(0, 0, 0, 0.3);
  opacity: 0;
  transition: opacity var(--fbz-motion-fast);
}

.play-icon {
  width: 36px;
  height: 36px;
  display: grid;
  place-content: center;
  border-radius: 50%;
  background: var(--fbz-color-brand-500);
  color: #07120a;
  font-size: 13px;
  margin-left: 2px;
  box-shadow: 0 4px 12px rgba(0, 0, 0, 0.3);
}

.episode-card:hover .episode-play-overlay {
  opacity: 1;
}

.episode-active-progress {
  position: absolute;
  bottom: 0;
  left: 0;
  right: 0;
  height: 3px;
  background: rgba(255, 255, 255, 0.15);

  .active-bar {
    display: block;
    height: 100%;
    width: 35%; /* mock progress */
    background: var(--fbz-color-brand-500);
  }
}

.episode-info {
  padding: var(--fbz-space-3);
  display: flex;
  flex-direction: column;
  flex: 1;
}

.episode-header-row {
  display: flex;
  align-items: flex-start;
  justify-content: space-between;
  gap: var(--fbz-space-2);
  margin-bottom: 2px;
}

.episode-title {
  font-size: var(--fbz-font-size-sm);
  font-weight: 800;
  line-height: 1.3;
  color: var(--fbz-color-text);
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
  flex: 1;
}

.episode-duration {
  font-size: var(--fbz-font-size-xs);
  color: var(--fbz-color-text-muted);
  font-weight: 600;
  white-space: nowrap;
}

.episode-airdate {
  font-size: var(--fbz-font-size-xs);
  color: var(--fbz-color-text-muted);
  margin-bottom: var(--fbz-space-2);
}

.episode-summary {
  margin: 0;
  font-size: var(--fbz-font-size-xs);
  line-height: 1.5;
  color: var(--fbz-color-text-soft);
  display: -webkit-box;
  overflow: hidden;
  -webkit-line-clamp: 2;
  -webkit-box-orient: vertical;
}

/* View All Button */
.view-all-container {
  display: flex;
  justify-content: center;
  margin-top: var(--fbz-space-5);
}

.view-all-btn {
  height: 40px;
  padding: 0 var(--fbz-space-6);
  border-radius: var(--fbz-radius-control);
  border: 1px solid var(--fbz-color-brand-500);
  background: transparent;
  color: var(--fbz-color-brand-500);
  font-size: var(--fbz-font-size-sm);
  font-weight: 800;
  cursor: pointer;
  transition:
    background var(--fbz-motion-base),
    color var(--fbz-motion-base),
    box-shadow var(--fbz-motion-base);

  &:hover {
    background: var(--fbz-color-brand-500);
    color: #07120a;
    box-shadow: 0 4px 14px color-mix(in srgb, var(--fbz-color-brand-500) 30%, transparent);
  }
}

/* Bottom Popup Drawer */
.popup-backdrop {
  position: fixed;
  inset: 0;
  background: rgba(0, 0, 0, 0.85);
  backdrop-filter: blur(8px);
  z-index: 1000;
  display: flex;
  align-items: flex-end;
  justify-content: center;
}

.popup-drawer {
  width: 100%;
  max-width: 960px;
  background: var(--fbz-color-bg);
  border-top: 1px solid var(--fbz-color-line);
  border-radius: var(--fbz-radius-card) var(--fbz-radius-card) 0 0;
  max-height: 85vh;
  display: flex;
  flex-direction: column;
  box-shadow: 0 -8px 32px rgba(0, 0, 0, 0.5);
}

.popup-header {
  display: flex;
  align-items: center;
  justify-content: space-between;
  padding: var(--fbz-space-4) var(--fbz-space-6);
  border-bottom: 1px solid var(--fbz-color-line-soft);
  flex-shrink: 0;
}

.popup-title-area {
  h3 {
    margin: 0 0 2px;
    font-size: var(--fbz-font-size-lg);
    font-weight: 800;
    color: var(--fbz-color-text);
  }

  .popup-season-stats {
    font-size: var(--fbz-font-size-xs);
    color: var(--fbz-color-text-muted);
  }
}

.popup-close-btn {
  background: var(--fbz-color-panel);
  border: 1px solid var(--fbz-color-line);
  color: var(--fbz-color-text-muted);
  width: 32px;
  height: 32px;
  border-radius: 50%;
  display: flex;
  align-items: center;
  justify-content: center;
  cursor: pointer;
  transition: all var(--fbz-motion-fast);

  &:hover {
    color: var(--fbz-color-text);
    border-color: var(--fbz-color-line-bright);
    background: var(--fbz-color-panel-strong);
  }
}

.popup-episode-ranges {
  display: flex;
  gap: var(--fbz-space-2);
  overflow-x: auto;
  padding: var(--fbz-space-3) var(--fbz-space-6) var(--fbz-space-1);
  scrollbar-width: none;
  flex-shrink: 0;

  &::-webkit-scrollbar {
    display: none;
  }
}

.popup-content {
  flex: 1;
  overflow-y: auto;
  padding: var(--fbz-space-4) var(--fbz-space-6) var(--fbz-space-6);
  scrollbar-width: thin;
}

.popup-episodes-grid {
  display: grid;
  grid-template-columns: repeat(auto-fill, minmax(280px, 1fr));
  gap: var(--fbz-space-4);
}

/* Drawer Transitions */
.drawer-fade-enter-active,
.drawer-fade-leave-active {
  transition: opacity 0.3s ease;
}

.drawer-fade-enter-from,
.drawer-fade-leave-to {
  opacity: 0;
}

.drawer-fade-enter-active .popup-drawer,
.drawer-fade-leave-active .popup-drawer {
  transition: transform 0.3s cubic-bezier(0.25, 1, 0.5, 1);
}

.drawer-fade-enter-from .popup-drawer,
.drawer-fade-leave-to .popup-drawer {
  transform: translateY(100%);
}

@media (max-width: 900px) {
  .season-banner {
    grid-template-columns: 80px 1fr;
    gap: var(--fbz-space-4);
  }

  .season-poster-wrap {
    width: 80px;
  }
}

@media (max-width: 600px) {
  .seasons-section {
    padding: 0 var(--fbz-space-4) var(--fbz-space-5);
  }

  .season-banner {
    grid-template-columns: 1fr;
    gap: var(--fbz-space-3);
    padding: var(--fbz-space-3);
  }

  .season-poster-wrap {
    display: none;
  }

  .popup-episodes-grid {
    grid-template-columns: 1fr;
  }

  .popup-drawer {
    max-height: 90vh;
  }

  .popup-header {
    padding: var(--fbz-space-3) var(--fbz-space-4);
  }

  .popup-content {
    padding: var(--fbz-space-3) var(--fbz-space-4) var(--fbz-space-4);
  }

  .popup-episode-ranges {
    padding: var(--fbz-space-2) var(--fbz-space-4) 0;
  }
}
</style>
