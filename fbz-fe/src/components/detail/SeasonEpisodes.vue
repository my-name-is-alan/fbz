<script setup lang="ts">
import type { SeasonInfo } from "@/types/media.ts";
import { useBodyScrollLock } from "@/composables/useBodyScrollLock.ts";
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

const props = defineProps<Props>();
const emit = defineEmits<{
  playEpisode: [episode: EpisodePlayPayload];
}>();

const modalSeasonNumber = shallowRef<number>();
const activeRangeIndex = shallowRef(0);

const isSingleSeason = computed(() => props.seasons.length === 1);
const modalOpen = computed(() => modalSeasonNumber.value != null);

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

const singleSeason = computed(() => props.seasons[0]);
const modalSeason = computed(() =>
  props.seasons.find((season) => season.season_number === modalSeasonNumber.value),
);

const activeSeason = computed(() => modalSeason.value ?? singleSeason.value);
const activeSeasonCard = computed(() =>
  seasonCards.value.find((season) => season.season_number === activeSeason.value?.season_number),
);

const activeSeasonPoster = computed(() => imageUrl(activeSeason.value?.poster_path, "w500"));
const activeSeasonBackdrop = computed(() => props.backdrop ?? activeSeasonPoster.value);

const activeSeasonMeta = computed(() => {
  const season = activeSeason.value;
  if (!season) return [];
  const airYear = season.air_date ? new Date(season.air_date).getFullYear() : undefined;
  return [airYear ? `${airYear}` : "", `${season.episode_count} 集`].filter(Boolean);
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

const visibleModalEpisodes = computed(() => {
  if (!showRanges.value) return activeEpisodes.value;
  return episodeRanges.value[activeRangeIndex.value]?.episodes ?? activeEpisodes.value.slice(0, RANGE_SIZE);
});

const currentEpisode = computed(() => activeEpisodes.value.find((episode) => episode.current));
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
      current: isHistorySeason && number === props.watchedEpisode,
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
  return `这一集围绕主要人物继续展开，新的线索逐渐浮出水面，角色之间的关系也被推向更紧张的位置。`;
}

function openSeason(seasonNumber: number, episodeNumber?: number) {
  modalSeasonNumber.value = seasonNumber;
  activeRangeIndex.value = episodeNumber ? Math.floor((episodeNumber - 1) / RANGE_SIZE) : 0;
}

function closeSeasonModal() {
  modalSeasonNumber.value = undefined;
  activeRangeIndex.value = 0;
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

useBodyScrollLock(modalOpen);

useEventListener(window, "keydown", (event) => {
  if (event.key === "Escape") closeSeasonModal();
});
</script>

<template>
  <section v-if="props.seasons.length" class="seasons">
    <template v-if="isSingleSeason && singleSeason">
      <header class="section-head">
        <h2 class="section-title">{{ singleSeason.name }}</h2>
      </header>

      <BaseScroller col-width="276px" gap="var(--fbz-space-4)" class="single-episodes">
        <button
          v-for="episode in activeEpisodes"
          :key="episode.id"
          class="episode-tile"
          :class="{ watched: episode.watched, current: episode.current }"
          type="button"
          @click="() => playEpisode(episode)"
        >
          <div class="episode-thumb">
            <MediaPoster
              :src="activeSeasonBackdrop"
              :title="`${props.showTitle} ${episode.title}`"
              ratio="wide"
            />
            <span class="episode-play">▶</span>
          </div>
          <strong>{{ episode.number }}. {{ episode.title }}</strong>
          <span>{{ episode.airDate }}　{{ episode.runtime }}分钟</span>
          <p>{{ episode.summary }}</p>
        </button>
      </BaseScroller>
    </template>

    <template v-else>
      <header class="section-head">
        <h2 class="section-title">播出季</h2>
      </header>

      <BaseScroller col-width="176px" gap="var(--fbz-space-5)" class="season-strip">
        <button
          v-for="season in seasonCards"
          :key="season.season_number"
          class="season-poster-card"
          type="button"
          @click="() => openSeason(season.season_number, season.hasHistory ? props.watchedEpisode : undefined)"
        >
          <div class="season-poster">
            <MediaPoster :src="season.poster" :title="season.name" ratio="poster" />
            <span class="count-badge">{{ season.episode_count }}</span>
          </div>
          <strong>{{ season.name }}</strong>
          <span class="season-rating">豆 {{ season.rating }}</span>
          <span v-if="season.hasHistory" class="history-line">继续 E{{ props.watchedEpisode }}</span>
        </button>
      </BaseScroller>
    </template>

    <Teleport to="body">
      <Transition name="season-modal">
        <section v-if="modalOpen && activeSeason" class="season-modal" aria-modal="true" role="dialog">
          <img
            v-if="activeSeasonBackdrop"
            class="modal-backdrop"
            :src="activeSeasonBackdrop"
            :alt="activeSeason.name"
          />
          <div class="modal-scrim" />

          <button class="modal-close" type="button" aria-label="关闭季详情" @click="closeSeasonModal">
            ‹
          </button>

          <div class="modal-content">
            <header class="modal-hero">
              <button class="modal-poster" type="button" @click="currentEpisode && playEpisode(currentEpisode)">
                <MediaPoster
                  :src="activeSeasonPoster"
                  :title="activeSeason.name"
                  ratio="poster"
                />
                <span class="poster-play">▶</span>
              </button>

              <div class="modal-copy">
                <p class="modal-kicker">{{ props.showTitle }}</p>
                <h2>{{ activeSeason.name }}</h2>
                <div class="modal-meta">
                  <span class="season-rating">豆 {{ activeSeasonCard?.rating ?? props.rating?.toFixed(1) ?? "8.8" }}</span>
                  <span v-for="meta in activeSeasonMeta" :key="meta">{{ meta }}</span>
                </div>
                <div class="modal-actions">
                  <button
                    class="modal-play"
                    type="button"
                    @click="() => currentEpisode && playEpisode(currentEpisode)"
                  >
                    ▶ 播放
                  </button>
                  <button class="round-action" type="button">✓</button>
                  <button class="round-action" type="button">•••</button>
                </div>
                <p class="modal-overview">
                  {{ activeSeason.overview || "本季继续展开故事主线，人物关系、冲突和关键事件逐步推进。" }}
                </p>
              </div>
            </header>

            <div v-if="showRanges" class="modal-ranges" aria-label="集数范围">
              <button
                v-for="(range, index) in episodeRanges"
                :key="range.label"
                class="range-tab"
                :class="{ active: activeRangeIndex === index }"
                type="button"
                @click="() => { activeRangeIndex = index; }"
              >
                {{ range.label }}
              </button>
            </div>

            <div class="episode-list">
              <button
                v-for="episode in visibleModalEpisodes"
                :key="episode.id"
                class="episode-row"
                :class="{ watched: episode.watched, current: episode.current }"
                type="button"
                @click="() => playEpisode(episode)"
              >
                <div class="row-thumb">
                  <MediaPoster
                    :src="activeSeasonBackdrop"
                    :title="`${props.showTitle} ${episode.title}`"
                    ratio="wide"
                  />
                  <span class="row-play">▶</span>
                </div>
                <div class="row-copy">
                  <strong>{{ episode.number }}. {{ episode.title }}</strong>
                  <span>{{ episode.airDate }}　{{ episode.runtime }}分钟</span>
                  <p>{{ episode.summary }}</p>
                </div>
                <div class="row-actions">
                  <span>♡</span>
                  <span>✓</span>
                  <span>•••</span>
                </div>
              </button>
            </div>
          </div>
        </section>
      </Transition>
    </Teleport>
  </section>
</template>

<style scoped lang="scss">
.seasons {
  max-width: 1280px;
  margin: 0 auto;
  padding: 0 var(--fbz-space-8) var(--fbz-space-8);
}

.section-head {
  margin-bottom: var(--fbz-space-3);
}

.section-title {
  margin: 0;
  font-size: 24px;
  line-height: 1.2;
  font-weight: 900;
}

.single-episodes,
.season-strip {
  :deep(.track) {
    align-items: start;
  }
}

.episode-tile,
.season-poster-card {
  min-width: 0;
  padding: 0;
  border: 0;
  background: transparent;
  color: inherit;
  text-align: left;
  cursor: pointer;
}

.episode-tile {
  display: flex;
  flex-direction: column;
  gap: 5px;

  strong {
    margin-top: 2px;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    font-size: var(--fbz-font-size-sm);
    font-weight: 800;
  }

  span {
    color: var(--fbz-color-text-muted);
    font-size: var(--fbz-font-size-sm);
  }

  p {
    margin: 0;
    color: var(--fbz-color-text-soft);
    font-size: var(--fbz-font-size-sm);
    line-height: 1.45;
    display: -webkit-box;
    overflow: hidden;
    -webkit-line-clamp: 2;
    -webkit-box-orient: vertical;
  }

  &.watched {
    opacity: 0.58;
  }

  &.current .episode-thumb {
    border-color: rgba(30, 215, 96, 0.86);
    box-shadow: inset 0 0 0 1px rgba(30, 215, 96, 0.86);
  }
}

.episode-thumb,
.season-poster,
.row-thumb,
.modal-poster {
  position: relative;
  overflow: hidden;
  border-radius: var(--fbz-radius-card);
}

.episode-thumb {
  border: 1px solid rgba(255, 255, 255, 0.14);
  background: rgba(255, 255, 255, 0.05);
  transition:
    border-color var(--fbz-transition-fast),
    box-shadow var(--fbz-transition-fast);
}

.episode-tile:hover .episode-thumb,
.episode-tile:focus-visible .episode-thumb {
  border-color: rgba(255, 255, 255, 0.34);
  box-shadow: inset 0 0 0 1px rgba(255, 255, 255, 0.18);
}

.episode-play,
.poster-play,
.row-play {
  position: absolute;
  inset: 0;
  display: grid;
  place-items: center;
  color: #fff;
  background: rgba(0, 0, 0, 0.18);
  opacity: 0;
  transition: opacity var(--fbz-motion-fast);
}

.episode-tile:hover .episode-play,
.modal-poster:hover .poster-play,
.episode-row:hover .row-play {
  opacity: 1;
}

.season-poster-card {
  display: block;

  strong,
  .season-rating,
  .history-line {
    display: block;
    margin-top: 6px;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    text-align: center;
  }

  strong {
    font-size: var(--fbz-font-size-sm);
    font-weight: 800;
  }

  &:hover .season-poster {
    filter: brightness(1.08);
    transform: translateY(-2px);
  }
}

.season-poster {
  border: 1px solid rgba(255, 255, 255, 0.08);
  transition:
    filter var(--fbz-motion-fast),
    transform var(--fbz-motion-fast);
}

.count-badge {
  position: absolute;
  top: 4px;
  right: 4px;
  min-width: 22px;
  height: 22px;
  display: grid;
  place-items: center;
  padding: 0 5px;
  border-radius: 999px;
  background: var(--fbz-color-brand-500);
  color: #07120a;
  font-size: 11px;
  font-weight: 900;
}

.season-rating {
  color: var(--fbz-color-brand-500);
  font-size: var(--fbz-font-size-sm);
  font-weight: 800;
}

.history-line {
  color: var(--fbz-color-text-muted);
  font-size: var(--fbz-font-size-xs);
}

.season-modal {
  position: fixed;
  inset: 0;
  z-index: calc(var(--fbz-z-overlay) + 40);
  overflow-y: auto;
  background: #030304;
  color: var(--fbz-color-text);
}

.modal-backdrop {
  position: fixed;
  inset: 0;
  width: 100%;
  height: 100%;
  object-fit: cover;
  opacity: 0.24;
}

.modal-scrim {
  position: fixed;
  inset: 0;
  background:
    linear-gradient(90deg, rgba(0, 0, 0, 0.92), rgba(0, 0, 0, 0.68) 48%, rgba(0, 0, 0, 0.82)),
    linear-gradient(180deg, rgba(0, 0, 0, 0.18), #030304 100%);
}

.modal-close {
  position: fixed;
  z-index: 2;
  top: 28px;
  left: 22px;
  width: 42px;
  height: 42px;
  display: grid;
  place-items: center;
  border: 0;
  border-radius: 50%;
  background: rgba(255, 255, 255, 0.08);
  color: #fff;
  font-size: 34px;
  line-height: 1;
  cursor: pointer;
}

.modal-content {
  position: relative;
  z-index: 1;
  max-width: 1580px;
  margin: 0 auto;
  padding: 80px var(--fbz-space-8) 64px;
}

.modal-hero {
  display: grid;
  grid-template-columns: 258px minmax(0, 1fr);
  gap: 36px;
  align-items: start;
  max-width: 1180px;
  min-height: 420px;
}

.modal-poster {
  padding: 0;
  border: 0;
  background: transparent;
  color: inherit;
  cursor: pointer;
  box-shadow: 0 18px 50px rgba(0, 0, 0, 0.42);
}

.poster-play {
  font-size: 28px;
  background: rgba(0, 0, 0, 0.34);
}

.modal-copy {
  padding-top: 8px;
}

.modal-kicker {
  margin: 0 0 8px;
  color: var(--fbz-color-text-soft);
  font-size: var(--fbz-font-size-md);
}

.modal-copy h2 {
  margin: 0 0 var(--fbz-space-3);
  font-size: 32px;
  line-height: 1.1;
  font-weight: 900;
}

.modal-meta {
  display: flex;
  flex-wrap: wrap;
  gap: var(--fbz-space-3);
  align-items: center;
  margin-bottom: var(--fbz-space-4);
  color: var(--fbz-color-text-soft);
}

.modal-actions {
  display: flex;
  align-items: center;
  gap: var(--fbz-space-2);
  margin-bottom: var(--fbz-space-4);
}

.modal-play,
.round-action {
  height: 46px;
  border: 0;
  cursor: pointer;
  font-weight: 800;
}

.modal-play {
  padding: 0 24px;
  border-radius: 999px;
  background: rgba(255, 255, 255, 0.18);
  color: #fff;
}

.round-action {
  width: 46px;
  border-radius: 50%;
  background: rgba(255, 255, 255, 0.16);
  color: #fff;
}

.modal-overview {
  max-width: 920px;
  margin: 0;
  color: var(--fbz-color-text-soft);
  font-size: var(--fbz-font-size-md);
  line-height: 1.7;
}

.modal-ranges {
  position: sticky;
  top: 0;
  z-index: 2;
  display: flex;
  gap: var(--fbz-space-2);
  overflow-x: auto;
  padding: var(--fbz-space-3) 0;
  margin-bottom: var(--fbz-space-2);
  background: linear-gradient(180deg, rgba(3, 3, 4, 0.96), rgba(3, 3, 4, 0.78));
  scrollbar-width: none;

  &::-webkit-scrollbar {
    display: none;
  }
}

.range-tab {
  flex: 0 0 auto;
  height: 32px;
  padding: 0 12px;
  border: 1px solid transparent;
  border-radius: 999px;
  background: rgba(255, 255, 255, 0.08);
  color: var(--fbz-color-text-soft);
  font-size: var(--fbz-font-size-sm);
  cursor: pointer;

  &.active {
    background: var(--fbz-color-brand-500);
    color: #07120a;
    font-weight: 900;
  }
}

.episode-list {
  display: flex;
  flex-direction: column;
  gap: var(--fbz-space-5);
}

.episode-row {
  display: grid;
  grid-template-columns: 232px minmax(0, 1fr) 132px;
  gap: var(--fbz-space-4);
  align-items: center;
  padding: 0;
  border: 0;
  background: transparent;
  color: inherit;
  text-align: left;
  cursor: pointer;

  &.watched {
    opacity: 0.56;
  }

  &.current .row-thumb {
    outline: 2px solid rgba(30, 215, 96, 0.72);
  }
}

.row-thumb {
  background: rgba(255, 255, 255, 0.06);
}

.row-copy {
  min-width: 0;

  strong {
    display: block;
    margin-bottom: 4px;
    font-size: var(--fbz-font-size-md);
    font-weight: 900;
  }

  span {
    display: block;
    margin-bottom: 5px;
    color: var(--fbz-color-text-muted);
    font-size: var(--fbz-font-size-sm);
  }

  p {
    margin: 0;
    color: var(--fbz-color-text-soft);
    line-height: 1.55;
    display: -webkit-box;
    overflow: hidden;
    -webkit-line-clamp: 2;
    -webkit-box-orient: vertical;
  }
}

.row-actions {
  display: flex;
  justify-content: flex-end;
  gap: var(--fbz-space-5);
  color: var(--fbz-color-text-muted);
  font-size: 22px;
}

.season-modal-enter-active,
.season-modal-leave-active {
  transition: opacity var(--fbz-motion-base);
}

.season-modal-enter-from,
.season-modal-leave-to {
  opacity: 0;
}

@media (max-width: 900px) {
  .modal-hero {
    grid-template-columns: 132px minmax(0, 1fr);
    min-height: 0;
  }

  .episode-row {
    grid-template-columns: 148px minmax(0, 1fr);
  }

  .row-actions {
    grid-column: 2;
    justify-content: flex-start;
    font-size: 18px;
  }
}

@media (max-width: 600px) {
  .seasons {
    padding: 0 var(--fbz-space-4) var(--fbz-space-5);
  }

  .modal-content {
    padding: 72px var(--fbz-space-4) var(--fbz-space-5);
  }

  .modal-hero {
    grid-template-columns: 96px minmax(0, 1fr);
    gap: var(--fbz-space-4);
  }

  .modal-copy h2 {
    font-size: 24px;
  }

  .modal-overview {
    grid-column: 1 / -1;
  }

  .episode-row {
    grid-template-columns: 116px minmax(0, 1fr);
    gap: var(--fbz-space-3);
  }
}
</style>
