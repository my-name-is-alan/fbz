<script setup lang="ts">
import type { DetailViewModel } from "@/service/modules/detail.ts";
import { fetchPlaybackSource, loadMovieDetail } from "@/service/modules/detail.ts";
import { useDetailActions } from "@/composables/useDetailActions.ts";
import { useAuthStore } from "@/stores/auth.ts";
import { usePlaybackStore } from "@/stores/playback.ts";
import { useUiStore } from "@/stores/ui.ts";
import { detailShellFor, itemRoute, itemTypeMeta } from "@/utils/itemType.ts";

/**
 * 电影 / 通用视频详情（Movie · Video · MusicVideo · Trailer …）。
 * 其它真实类型落到这里时按 Type 转到对应详情壳，不把一切都当电影。
 */
const route = useRoute();
const router = useRouter();
const playback = usePlaybackStore();
const uiStore = useUiStore();
const authStore = useAuthStore();
const routeId = computed(() => String(route.params.id));

const detail = ref<DetailViewModel>();
const loading = ref(true);
const starting = ref(false);
const versionId = ref("");
const subtitleOpen = ref(false);

const { toggleFavorite, togglePlayed, openMore, editMetadata } = useDetailActions(
  detail,
  "/library",
);

const typeMeta = computed(() => itemTypeMeta(detail.value?.itemType));

watch(
  routeId,
  async (id) => {
    loading.value = true;
    const loaded = await loadMovieDetail(id);
    if (routeId.value !== id) return;
    if (loaded && detailShellFor(loaded.itemType) !== "movie") {
      await router.replace(itemRoute(loaded.itemType, loaded.id, loaded.parentId));
      return;
    }
    detail.value = loaded;
    loading.value = false;
  },
  { immediate: true },
);

/** 播放：versionId = 所选版本（MediaSource id）；startAt = 续播位置（秒）。 */
async function playMovie(selectedVersion?: string, startAt?: number) {
  const movie = detail.value;
  if (!movie || starting.value) return;
  starting.value = true;
  try {
    const source = await fetchPlaybackSource(movie.id, selectedVersion);
    if (!source) {
      uiStore.showToast("该条目暂无可播放媒体源。", "warning");
      return;
    }
    const activeVersion = movie.versions.find((v) => v.id === selectedVersion) ?? movie.versions[0];
    playback.open({
      type: "movie",
      id: movie.id,
      title: movie.title,
      subtitle: movie.meta.join(" · "),
      poster: movie.poster,
      backdrop: movie.backdrop,
      tags: activeVersion?.tags,
      duration: source.duration ?? movie.runtimeSeconds,
      // 显式传入秒数：从头=0，续播=上次位置；避免被 server.startTicks 覆盖。
      startPositionSeconds: startAt ?? 0,
      source,
      server: source.server,
      mediaSources: source.mediaSources,
      chapters: source.chapters,
      subtitleTracks: source.subtitleTracks,
      mediaInfo: source.mediaInfo,
    });
  } finally {
    starting.value = false;
  }
}

function resumeMovie(selectedVersion?: string) {
  void playMovie(selectedVersion, detail.value?.resumePositionSeconds);
}

/** 下载外挂字幕后刷新媒体源（字幕列表 / 规格徽章随之更新）。 */
async function reloadSources() {
  const movie = detail.value;
  if (!movie) return;
  const fresh = await loadMovieDetail(movie.id);
  if (fresh && detail.value?.id === fresh.id) detail.value = fresh;
}

const activeSource = computed(
  () =>
    detail.value?.mediaSources.find((s) => s.id === versionId.value) ??
    detail.value?.mediaSources[0],
);

const facts = computed(() => {
  const d = detail.value;
  if (!d) return [];
  return [
    { label: "导演", value: d.directors },
    { label: "编剧", value: d.writers },
    { label: "原名", value: d.originalTitle !== d.title ? d.originalTitle : undefined },
    { label: "分级", value: d.officialRating },
    { label: "首映", value: d.premiereDate },
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
      :meta="detail.meta"
      :genres="detail.genres"
      :official-rating="detail.officialRating"
      :overview="detail.overview"
      :rating="detail.rating"
      :type-label="typeMeta.label"
      :type-icon="typeMeta.icon"
      :versions="detail.versions"
      :is-favorite="detail.isFavorite"
      :resume-position-seconds="detail.resumePositionSeconds"
      :played="detail.played"
      :busy="starting"
      show-played-toggle
      show-more
      @play="(id) => playMovie(id, 0)"
      @resume="resumeMovie"
      @toggle-favorite="toggleFavorite"
      @toggle-played="togglePlayed"
      @more="openMore"
    >
      <template #header>
        <PageHeader :title="detail.title" fallback="/library" />
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
      v-if="detail.mediaSources.length"
      v-model:active-id="versionId"
      :sources="detail.mediaSources"
      :chapter-count="detail.chapterCount"
    />

    <CastRow :cast="detail.cast" />
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
  </main>

  <main v-else-if="loading" class="c-page">
    <p class="c-state"><BaseIcon name="loading" :size="16" class="c-spin" />正在加载详情…</p>
  </main>

  <main v-else class="c-page">
    <BaseEmptyState
      icon="search-x"
      title="未找到该条目"
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
