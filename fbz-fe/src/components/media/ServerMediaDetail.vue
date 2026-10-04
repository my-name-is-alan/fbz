<script setup lang="ts">
import {
  itemDetail,
  seriesEpisodes,
  imageUrl,
  preparePlayback,
  errorMessage,
} from "@/service/modules/server.ts";
import type { ServerItem } from "@/service/modules/server.ts";
import { usePlaybackStore } from "@/stores/playback.ts";
const route = useRoute();
const playback = usePlaybackStore();
const item = ref<ServerItem>();
const episodes = ref<ServerItem[]>([]);
const season = ref(1);
const loading = ref(true);
const busy = ref(false);
const error = ref("");
let generation = 0;
const seasons = computed(() =>
  [...new Set(episodes.value.map((e) => e.ParentIndexNumber ?? 1))].sort((a, b) => a - b),
);
const selectedEpisodes = computed(() =>
  episodes.value.filter((e) => (e.ParentIndexNumber ?? 1) === season.value),
);
const next = computed(
  () =>
    episodes.value.find((e) => (e.UserData?.PlaybackPositionTicks ?? 0) > 0) ||
    episodes.value.find((e) => !e.UserData?.Played) ||
    episodes.value[0],
);
const backdrop = computed(() =>
  item.value ? imageUrl(item.value, "Backdrop") || imageUrl(item.value) : undefined,
);
function episodeTitle(episode: ServerItem) {
  return /S\d+E\d+/i.test(episode.Name) ? `第 ${episode.IndexNumber ?? "—"} 集` : episode.Name;
}
async function load() {
  const current = ++generation;
  loading.value = true;
  error.value = "";
  try {
    const detail = await itemDetail(String(route.params.id));
    let list: ServerItem[] = [];
    if (detail.Type === "Series")
      list = (await seriesEpisodes(detail.Id)).Items.sort(
        (a, b) =>
          (a.ParentIndexNumber ?? 1) - (b.ParentIndexNumber ?? 1) ||
          (a.IndexNumber ?? 0) - (b.IndexNumber ?? 0),
      );
    if (current !== generation) return;
    item.value = detail;
    episodes.value = list;
    season.value = next.value?.ParentIndexNumber ?? seasons.value[0] ?? 1;
  } catch (err) {
    if (current === generation) error.value = errorMessage(err);
  } finally {
    if (current === generation) loading.value = false;
  }
}
async function play(episode?: ServerItem) {
  const chosen = episode || (item.value?.Type === "Series" ? next.value : item.value);
  if (!chosen || busy.value) return;
  busy.value = true;
  error.value = "";
  try {
    const prepared = await preparePlayback(chosen);
    playback.open({
      ...prepared,
      title: item.value?.Type === "Series" ? item.value.Name : prepared.title,
      subtitle:
        chosen.Type === "Episode"
          ? `第 ${chosen.ParentIndexNumber ?? 1} 季 · 第 ${chosen.IndexNumber ?? "—"} 集`
          : prepared.subtitle,
      backdrop: prepared.backdrop || backdrop.value,
      playlist: episodes.value.map((e) => ({
        id: e.Id,
        title: item.value?.Name ?? e.Name,
        subtitle: `第 ${e.ParentIndexNumber ?? 1} 季 · 第 ${e.IndexNumber ?? "—"} 集`,
        seasonNumber: e.ParentIndexNumber ?? 1,
        episodeNumber: e.IndexNumber ?? 0,
        duration: (e.RunTimeTicks ?? 0) / 1e7,
        poster: imageUrl(e) || imageUrl(item.value!),
        backdrop: backdrop.value,
        serverItem: e,
      })),
    });
  } catch (err) {
    error.value = errorMessage(err);
  } finally {
    busy.value = false;
  }
}
watch(() => route.params.id, load, { immediate: true });
useEventListener(window, "fbz-playback-saved", () => {
  void load();
});
</script>
<template>
  <main class="server-detail">
    <div v-if="loading" class="detail-loading"><div class="du-skeleton" /></div>
    <BaseEmptyState v-else-if="!item" title="暂时无法打开这部作品" :description="error" error
      ><button class="du-btn" @click="load">重试</button
      ><RouterLink class="du-btn du-btn-ghost" to="/library">返回媒体库</RouterLink></BaseEmptyState
    ><template v-else
      ><section class="detail-stage" :style="{ '--detail-image': `url(${backdrop || ''})` }">
        <div class="stage-shade" />
        <RouterLink class="du-btn du-btn-ghost detail-back" to="/library"
          ><BaseIcon name="left" :size="17" />媒体库</RouterLink
        >
        <div class="stage-copy">
          <span class="eyebrow">{{ item.Type === "Series" ? "剧集收藏" : "电影收藏" }}</span>
          <h1>{{ item.Name }}</h1>
          <div class="detail-meta">
            <span v-if="item.ProductionYear">{{ item.ProductionYear }}</span
            ><span v-if="seasons.length">{{ seasons.length }} 季 · {{ episodes.length }} 集</span
            ><span v-else-if="item.RunTimeTicks"
              >{{ Math.round(item.RunTimeTicks / 6e8) }} 分钟</span
            ><span v-for="genre in item.Genres?.slice(0, 3)" :key="genre" class="genre-pill">{{
              genre
            }}</span
            ><span v-if="item.CommunityRating" class="rating"
              ><BaseIcon name="star" :size="15" />{{ item.CommunityRating.toFixed(1) }}</span
            >
          </div>
          <p v-if="item.Overview" class="overview">{{ item.Overview }}</p>
          <p v-else class="overview muted">暂时没有这部作品的简介。</p>
          <div class="detail-actions">
            <button
              class="du-btn detail-play"
              :disabled="busy || (item.Type === 'Series' && !next)"
              @click="play()"
            >
              <BaseIcon :name="busy ? 'loading' : 'play'" />{{
                (
                  item.Type === "Series"
                    ? next?.UserData?.PlaybackPositionTicks
                    : item.UserData?.PlaybackPositionTicks
                )
                  ? "继续观看"
                  : "开始播放"
              }}</button
            ><span v-if="item.Type === 'Series' && next" class="next-label"
              >第 {{ next.ParentIndexNumber ?? 1 }} 季 · 第 {{ next.IndexNumber ?? "—" }} 集</span
            >
          </div>
          <p v-if="error" class="du-alert du-alert-error playback-error" role="alert">
            {{ error }}
          </p>
        </div>
      </section>
      <section v-if="item.Type === 'Series'" class="episode-section">
        <div class="episode-heading">
          <h2>选集</h2>
          <div class="du-tabs season-tabs" role="tablist" aria-label="选择季度">
            <button
              v-for="s in seasons"
              :key="s"
              role="tab"
              class="du-tab"
              :class="{ 'du-tab-active': season === s }"
              :aria-selected="season === s"
              @click="season = s"
            >
              第 {{ s }} 季
            </button>
          </div>
        </div>
        <BaseEmptyState
          v-if="!episodes.length"
          title="这一季还没有分集"
          description="完成媒体库扫描后，分集会显示在这里。"
          icon="tv"
        />
        <div v-else class="episode-grid">
          <button
            v-for="episode in selectedEpisodes"
            :key="episode.Id"
            class="episode-card"
            :disabled="busy"
            @click="play(episode)"
          >
            <div class="episode-image">
              <MediaPoster
                :src="imageUrl(episode, 'Backdrop') || imageUrl(episode) || backdrop"
                :title="episodeTitle(episode)"
                ratio="wide"
              /><span class="episode-number">{{
                String(episode.IndexNumber ?? 0).padStart(2, "0")
              }}</span
              ><span class="episode-play"><BaseIcon name="play" :size="18" /></span
              ><span
                v-if="episode.UserData?.PlaybackPositionTicks && episode.RunTimeTicks"
                class="watched-line"
                :style="{
                  width: `${Math.min(100, (episode.UserData.PlaybackPositionTicks / episode.RunTimeTicks) * 100)}%`,
                }"
              />
            </div>
            <h3>{{ episodeTitle(episode) }}</h3>
            <p>
              {{
                episode.RunTimeTicks
                  ? `${Math.round(episode.RunTimeTicks / 6e8)} 分钟`
                  : "点击播放"
              }}<span v-if="episode.UserData?.Played"> · 已看完</span>
            </p>
          </button>
        </div>
      </section></template
    >
  </main>
</template>
<style scoped lang="scss">
.server-detail {
  padding-top: var(--header-h);
  padding-bottom: 90px;
  min-height: 100dvh;
}
.detail-stage {
  position: relative;
  isolation: isolate;
  min-height: 650px;
  background:
    var(--detail-image) center 30% / cover,
    var(--fbz-color-panel);
  color: #fff;
}
.stage-shade {
  position: absolute;
  inset: 0;
  z-index: -1;
  background:
    linear-gradient(90deg, #090c10f0, #090c1070 60%, #090c1015),
    linear-gradient(0deg, var(--fbz-color-bg), transparent 48%);
}
.detail-back {
  margin: 28px 4.5%;
  color: #dbe0e6;
  font-size: 12px;
  background: #ffffff08;
  border: 1px solid #ffffff20;
}
.stage-copy {
  max-width: 760px;
  padding: 35px clamp(24px, 5vw, 80px) 75px;
}
.eyebrow {
  font-size: 11px;
  letter-spacing: 2px;
  color: #c3c8cf;
}
.stage-copy h1 {
  font-size: clamp(36px, 4.5vw, 66px);
  line-height: 1.1;
  letter-spacing: -2px;
  font-weight: 600;
  margin: 22px 0;
  text-wrap: balance;
}
.detail-meta {
  display: flex;
  align-items: center;
  gap: 13px;
  flex-wrap: wrap;
  font-size: 12px;
  color: #d3d8df;
}
.genre-pill {
  padding: 3px 10px;
  border: 1px solid #ffffff40;
  border-radius: 20px;
  font-size: 11px;
}
.rating {
  display: inline-flex;
  align-items: center;
  gap: 5px;
  color: #e2c989;
}
.overview {
  font-size: 14px;
  line-height: 1.9;
  color: #c9cfd7;
  max-width: 53ch;
  margin: 25px 0 0;
  display: -webkit-box;
  -webkit-line-clamp: 5;
  -webkit-box-orient: vertical;
  overflow: hidden;
}
.detail-actions {
  display: flex;
  align-items: center;
  gap: 20px;
  margin-top: 30px;
}
.detail-play {
  background: #f2f3f4;
  color: #0c1014;
  border: 0;
  padding-inline: 27px;
}
.next-label {
  font-size: 12px;
  color: #c9cfd7;
}
.playback-error {
  margin-top: 20px;
}
.episode-section {
  max-width: 1440px;
  margin: 0 auto;
  padding: 0 clamp(22px, 5vw, 80px);
}
.episode-heading {
  display: flex;
  align-items: center;
  gap: 35px;
  margin-bottom: 25px;
}
.episode-heading h2 {
  font-size: 23px;
  font-weight: 550;
}
.season-tabs {
  gap: 15px;
  overflow: auto;
  flex-wrap: nowrap;
}
.du-tab {
  white-space: nowrap;
  padding: 12px 5px;
  color: var(--fbz-color-text-muted);
  border-bottom: 2px solid transparent;
  font-size: 13px;
}
.du-tab-active {
  color: var(--fbz-color-text);
  border-color: var(--fbz-color-brand-500);
}
.episode-grid {
  display: grid;
  grid-template-columns: repeat(4, minmax(0, 1fr));
  gap: 26px 18px;
}
.episode-card {
  text-align: left;
  background: none;
  border: 0;
  color: var(--fbz-color-text);
  min-width: 0;
  padding: 0;
  cursor: pointer;
}
.episode-image {
  position: relative;
  border-radius: 10px;
  overflow: hidden;
}
.episode-number {
  position: absolute;
  left: 12px;
  bottom: 10px;
  font-size: 13px;
  color: white;
  text-shadow: 0 1px 6px #000;
}
.episode-play {
  position: absolute;
  right: 10px;
  bottom: 10px;
  width: 32px;
  height: 32px;
  display: grid;
  place-items: center;
  border-radius: 50%;
  background: #ffffffdd;
  color: #101214;
  opacity: 0;
}
.episode-card:hover .episode-play,
.episode-card:focus-visible .episode-play {
  opacity: 1;
}
.episode-card h3 {
  font-size: 13px;
  margin: 11px 0 4px;
  font-weight: 550;
}
.episode-card p {
  font-size: 11px;
  color: var(--fbz-color-text-muted);
  margin: 0;
}
.watched-line {
  position: absolute;
  bottom: 0;
  left: 0;
  height: 3px;
  background: var(--fbz-color-brand-500);
}
.detail-loading {
  padding: 35px 5vw;
}
.detail-loading > div {
  height: 620px;
  border-radius: 18px;
}
@media (max-width: 900px) {
  .episode-grid {
    grid-template-columns: repeat(3, minmax(0, 1fr));
  }
  .detail-stage {
    min-height: 580px;
  }
}
@media (max-width: 600px) {
  .detail-stage {
    min-height: 620px;
    background-position: 65% center;
  }
  .stage-shade {
    background: linear-gradient(0deg, var(--fbz-color-bg), #080b1070 80%, #080b1050);
  }
  .stage-copy {
    padding: 140px 24px 48px;
  }
  .stage-copy h1 {
    font-size: 38px;
    letter-spacing: -1px;
    max-width: 15ch;
  }
  .overview {
    -webkit-line-clamp: 4;
    font-size: 13px;
  }
  .detail-back {
    position: absolute;
    top: 0;
    margin: 20px;
  }
  .detail-meta {
    gap: 10px;
  }
  .detail-actions {
    gap: 14px;
  }
  .episode-heading {
    display: block;
  }
  .season-tabs {
    margin-top: 18px;
  }
  .episode-grid {
    grid-template-columns: repeat(2, minmax(0, 1fr));
    gap: 23px 13px;
  }
  .episode-play {
    opacity: 1;
  }
}
</style>
