<script setup lang="ts">
import type { DetailViewModel } from "@/service/modules/detail.ts";
import { fetchPlaybackSource, loadMovieDetail } from "@/service/modules/detail.ts";
import { searchRemoteSubtitles } from "@/service/modules/metadata.ts";
import { setFavorite } from "@/service/modules/userData.ts";
import { useAuthStore } from "@/stores/auth.ts";
import { usePlaybackStore } from "@/stores/playback.ts";
import { useUiStore } from "@/stores/ui.ts";

const route = useRoute();
const playback = usePlaybackStore();
const uiStore = useUiStore();
const authStore = useAuthStore();
const routeId = computed(() => String(route.params.id));

const detail = ref<DetailViewModel>();
const togglingFavorite = shallowRef(false);

watch(
  routeId,
  async (v) => {
    detail.value = await loadMovieDetail(v);
  },
  { immediate: true },
);

/** 播放：versionId = 所选版本（MediaSource id）；startAt = 续播位置（秒）。 */
async function playMovie(versionId?: string, startAt?: number) {
  const movie = detail.value;
  if (!movie) return;

  const source = await fetchPlaybackSource(movie.id, versionId);
  const activeVersion = movie.versions.find((v) => v.id === versionId) ?? movie.versions[0];
  playback.open({
    type: "movie",
    id: movie.id,
    title: movie.title,
    subtitle: movie.meta.join(" · "),
    poster: movie.poster,
    backdrop: movie.backdrop,
    tags: activeVersion?.tags,
    duration: source?.duration ?? movie.runtimeSeconds,
    // 显式传入秒数：从头=0，续播=上次位置；避免被 server.startTicks 覆盖。
    startPositionSeconds: startAt ?? 0,
    source: source ?? undefined,
    server: source?.server,
    mediaSources: source?.mediaSources,
    chapters: source?.chapters,
  });
}

/** 续播：从上次位置继续。 */
function resumeMovie(versionId?: string) {
  void playMovie(versionId, detail.value?.resumePositionSeconds);
}

/** 收藏切换：乐观更新，失败回滚并提示。 */
async function toggleFavorite() {
  const movie = detail.value;
  const userId = authStore.userId;
  if (!movie || !userId || togglingFavorite.value) return;

  const next = !movie.isFavorite;
  togglingFavorite.value = true;
  detail.value = { ...movie, isFavorite: next };
  try {
    await setFavorite(userId, movie.id, next);
  } catch {
    detail.value = { ...movie, isFavorite: !next };
    uiStore.showToast("更新收藏状态失败。", "error");
  } finally {
    togglingFavorite.value = false;
  }
}

/** 打开元数据管理弹层：用详情视图模型拼一个最小 MediaItem 传入。 */
const searchingSubs = shallowRef(false);

/** 外挂字幕入口：对接 RemoteSearch/Subtitles；无 provider 时提示空结果。 */
async function searchExternalSubtitles() {
  const movie = detail.value;
  if (!movie || searchingSubs.value) return;
  searchingSubs.value = true;
  try {
    const versionId = movie.versions[0]?.id;
    const results = await searchRemoteSubtitles(movie.id, "chi", versionId);
    if (!results.length) {
      uiStore.showToast("暂无可用的外挂字幕源（服务端未配置字幕 provider）。", "info");
      return;
    }
    uiStore.showToast(`找到 ${results.length} 条外挂字幕候选。`, "success");
  } catch {
    uiStore.showToast("搜索外挂字幕失败，请稍后重试。", "error");
  } finally {
    searchingSubs.value = false;
  }
}

function editMetadata() {
  const movie = detail.value;
  if (!movie) return;
  const yearSeg = movie.meta.find((m) => /^\d{4}$/.test(m));
  uiStore.openMetadataManager({
    id: movie.id,
    libraryId: "movie",
    detailType: "movie",
    title: movie.title,
    meta: movie.meta.join(" · "),
    poster: movie.poster,
    year: yearSeg ? Number(yearSeg) : undefined,
    rating: movie.rating ?? undefined,
    isFavorite: movie.isFavorite,
  });
}
</script>

<template>
  <main v-if="detail" class="detail-view">
    <PageHeader :title="detail.title" fallback="/library/movie" />

    <DetailHero
      :title="detail.title"
      :poster="detail.poster"
      :backdrop="detail.backdrop"
      :meta="detail.meta"
      :genres="detail.genres"
      :official-rating="detail.officialRating"
      :overview="detail.overview"
      :rating="detail.rating"
      :versions="detail.versions"
      :is-favorite="detail.isFavorite"
      :resume-position-seconds="detail.resumePositionSeconds"
      :played="detail.played"
      @play="(versionId) => playMovie(versionId, 0)"
      @resume="resumeMovie"
      @toggle-favorite="toggleFavorite"
    >
      <template #extra>
        <dl class="facts">
          <div v-if="detail.directors" class="fact">
            <dt>导演</dt>
            <dd>{{ detail.directors }}</dd>
          </div>
          <div v-if="detail.originalTitle" class="fact">
            <dt>原名</dt>
            <dd>{{ detail.originalTitle }}</dd>
          </div>
        </dl>
        <div class="hero-extra-actions">
          <button type="button" class="edit-meta-btn" @click="searchExternalSubtitles">
            {{ searchingSubs ? "搜索字幕中…" : "外挂字幕" }}
          </button>
          <button type="button" class="edit-meta-btn" @click="editMetadata">编辑元数据</button>
        </div>
      </template>
    </DetailHero>

    <CastRow :cast="detail.cast" />
    <SimilarRow :items="detail.similar" />
  </main>

  <main v-else class="detail-missing">
    <p>未找到该影片</p>
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

.hero-extra-actions {
  margin-top: var(--fbz-space-3);
  display: flex;
  flex-wrap: wrap;
  gap: var(--fbz-space-2);
}

.edit-meta-btn {
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
