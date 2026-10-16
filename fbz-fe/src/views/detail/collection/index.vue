<script setup lang="ts">
import type { CollectionDetail, MediaItem } from "@/types/media.ts";
import { fetchPlaybackSource, loadCollectionDetail } from "@/service/modules/detail.ts";
import { usePlaybackStore } from "@/stores/playback.ts";
import { useUiStore } from "@/stores/ui.ts";
import { itemRoute } from "@/utils/itemType.ts";

/** 合集（BoxSet）详情：成员网格 + 播放首部可播成员。 */
const route = useRoute();
const router = useRouter();
const playback = usePlaybackStore();
const uiStore = useUiStore();

const id = computed(() => String(route.params.id ?? ""));
const collection = ref<CollectionDetail>();
const parts = ref<MediaItem[]>([]);
const loading = ref(true);
const playing = ref(false);

const sortBy = ref<"default" | "year" | "rating">("default");

const meta = computed(() => {
  if (!collection.value) return [];
  const years = parts.value.map((p) => p.year).filter((y): y is number => y != null);
  const span = years.length ? `${Math.min(...years)}–${Math.max(...years)}` : "";
  return [`${parts.value.length} 部作品`, span].filter(Boolean);
});

const playedCount = computed(() => parts.value.filter((p) => p.isPlayed).length);

const sortedParts = computed(() => {
  const list = [...parts.value];
  if (sortBy.value === "year") list.sort((a, b) => (a.year ?? 0) - (b.year ?? 0));
  if (sortBy.value === "rating") list.sort((a, b) => (b.rating ?? 0) - (a.rating ?? 0));
  return list;
});

/** 可直接起播的成员（电影 / 视频）；剧集进详情由续看逻辑接管。 */
const firstPlayable = computed(() =>
  sortedParts.value.find((p) =>
    ["Movie", "Video"].includes(p.itemType ?? (p.detailType === "tv" ? "Series" : "Movie")),
  ),
);

watch(
  id,
  async (v) => {
    loading.value = true;
    collection.value = undefined;
    parts.value = [];
    if (!v) {
      loading.value = false;
      return;
    }
    const detail = await loadCollectionDetail(v);
    if (id.value !== v) return;
    collection.value = detail?.collection;
    parts.value = detail?.parts ?? [];
    loading.value = false;
  },
  { immediate: true },
);

async function playCollection() {
  if (playing.value) return;
  const movie = firstPlayable.value;
  if (movie) {
    playing.value = true;
    try {
      const source = await fetchPlaybackSource(String(movie.id));
      if (!source?.uri) {
        uiStore.showToast("该条目暂无可播放媒体源。", "warning");
        return;
      }
      playback.open({
        type: "movie",
        id: String(movie.id),
        title: movie.title,
        subtitle: movie.meta,
        poster: movie.poster,
        duration: source.duration,
        startPositionSeconds: 0,
        source,
        server: source.server,
        mediaSources: source.mediaSources,
        chapters: source.chapters,
        subtitleTracks: source.subtitleTracks,
        mediaInfo: source.mediaInfo,
      });
    } catch {
      uiStore.showToast("无法开始播放，请稍后重试。", "error");
    } finally {
      playing.value = false;
    }
    return;
  }
  const first = sortedParts.value[0];
  if (first) {
    void router.push(
      itemRoute(first.itemType ?? (first.detailType === "tv" ? "Series" : "Movie"), first.id),
    );
    return;
  }
  uiStore.showToast("合集中暂无可播放成员。", "warning");
}
</script>

<template>
  <main v-if="loading" class="c-page">
    <p class="c-state"><BaseIcon name="loading" :size="16" class="c-spin" />正在打开合集…</p>
  </main>

  <main v-else-if="collection" class="c-page detail-page">
    <DetailHero
      :title="collection.title"
      :poster="collection.poster_path ?? undefined"
      :backdrop="collection.backdrop_path ?? undefined"
      :meta="meta"
      :overview="collection.overview"
      type-label="合集 · BoxSet"
      type-icon="collection"
      :show-actions="parts.length > 0"
      :busy="playing"
      @play="playCollection"
    >
      <template #header>
        <PageHeader
          :title="collection.title"
          :crumbs="[{ label: '合集', to: '/collections', icon: 'collection' }]"
          fallback="/collections"
        />
      </template>
    </DetailHero>

    <section class="parts">
      <header class="parts-head">
        <h2 class="c-section-title">
          <BaseIcon name="collection" :size="16" class="lead" />包含作品
          <span class="n">{{ parts.length }}</span>
          <span v-if="playedCount" class="n">· 已看 {{ playedCount }}</span>
        </h2>
        <div class="seg" role="group" aria-label="排序">
          <button type="button" :class="{ on: sortBy === 'default' }" @click="sortBy = 'default'">
            默认
          </button>
          <button type="button" :class="{ on: sortBy === 'year' }" @click="sortBy = 'year'">
            年份
          </button>
          <button type="button" :class="{ on: sortBy === 'rating' }" @click="sortBy = 'rating'">
            评分
          </button>
        </div>
      </header>
      <div v-if="parts.length" class="grid">
        <MediaCard
          v-for="(item, i) in sortedParts"
          :key="item.id"
          :item="item"
          layout="poster"
          :variant="(i % 2) as 0 | 1"
        />
      </div>
      <BaseEmptyState
        v-else
        compact
        icon="collection"
        title="该合集暂无可见成员"
        description="成员可能不在你有权限的媒体库中。"
      />
    </section>
  </main>

  <main v-else class="c-page">
    <BaseEmptyState
      icon="search-x"
      title="未找到该合集"
      description="合集可能已被删除，或后端尚未提供该合集详情。"
    >
      <RouterLink to="/collections" class="du-btn du-btn-ghost"
        ><BaseIcon name="collection" :size="15" />返回合集列表</RouterLink
      >
    </BaseEmptyState>
  </main>
</template>

<style scoped lang="scss">
.parts {
  display: flex;
  flex-direction: column;
  gap: 12px;
}

.parts-head {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 12px;

  .lead,
  .n {
    color: var(--ui-text-subtle);
  }

  .n {
    font-size: 12px;
    font-weight: 500;
  }
}

.seg {
  display: inline-flex;
  padding: 2px;
  gap: 2px;
  border: 1px solid var(--ui-border);
  border-radius: 9px;
  background: var(--ui-surface);

  button {
    height: 26px;
    padding: 0 10px;
    border: 0;
    border-radius: 6px;
    background: transparent;
    color: var(--ui-text-muted);
    font-size: 12.5px;
    cursor: pointer;

    &.on {
      color: var(--ui-text);
      background: rgb(var(--ui-accent-channel) / 14%);
      box-shadow: inset 0 0 0 1px rgb(var(--ui-accent-channel) / 30%);
    }
  }
}

.grid {
  display: grid;
  grid-template-columns: repeat(auto-fill, minmax(150px, 1fr));
  gap: 20px 16px;
}

@media (max-width: 600px) {
  .grid {
    grid-template-columns: repeat(auto-fill, minmax(104px, 1fr));
  }
}
</style>
