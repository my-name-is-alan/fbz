<script setup lang="ts">
import type { CollectionDetail, MediaItem } from "@/types/media.ts";
import { fetchPlaybackSource, loadCollectionDetail } from "@/service/modules/detail.ts";
import { usePlaybackStore } from "@/stores/playback.ts";
import { useUiStore } from "@/stores/ui.ts";

const route = useRoute();
const router = useRouter();
const playback = usePlaybackStore();
const uiStore = useUiStore();

const id = computed(() => String(route.params.id ?? ""));
const collection = ref<CollectionDetail>();
const parts = ref<MediaItem[]>([]);
const loading = ref(true);
const playing = ref(false);

const meta = computed(() => (collection.value ? [`${parts.value.length} 部作品`] : []));

/** 可直接起播的电影类成员（剧集进详情由续看逻辑接管）。 */
const firstPlayableMovie = computed(() =>
  parts.value.find((p) => (p.detailType ?? "movie") === "movie"),
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
    collection.value = detail?.collection;
    parts.value = detail?.parts ?? [];
    loading.value = false;
  },
  { immediate: true },
);

/** Hero「播放」：优先播第一部电影；若仅有剧集则打开第一部剧集详情。 */
async function playCollection() {
  if (playing.value) return;
  const movie = firstPlayableMovie.value;
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
      });
    } catch {
      uiStore.showToast("无法开始播放，请稍后重试。", "error");
    } finally {
      playing.value = false;
    }
    return;
  }
  const first = parts.value[0];
  if (first) {
    const kind = first.detailType ?? "movie";
    void router.push(`/${kind}/${first.id}`);
    return;
  }
  uiStore.showToast("合集中暂无可播放成员。", "warning");
}
</script>

<template>
  <main v-if="loading" class="detail-missing">
    <p>正在打开合集…</p>
  </main>

  <main v-else-if="collection" class="detail-view">
    <PageHeader :title="collection.title" fallback="/collections" />

    <DetailHero
      :title="collection.title"
      :poster="collection.poster_path ?? undefined"
      :backdrop="collection.backdrop_path ?? undefined"
      :meta="meta"
      :overview="collection.overview"
      :show-actions="parts.length > 0"
      @play="playCollection"
    />

    <section class="parts">
      <h2 class="section-title">包含作品</h2>
      <div v-if="parts.length" class="grid">
        <MediaCard
          v-for="(item, i) in parts"
          :key="item.id"
          :item="item"
          layout="poster"
          :variant="(i % 2) as 0 | 1"
        />
      </div>
      <p v-else class="empty-state">该合集暂无可见成员。</p>
    </section>
  </main>

  <main v-else class="detail-missing">
    <p>未找到该合集，或后端尚未提供该合集详情。</p>
    <RouterLink to="/collections" class="link">返回合集列表</RouterLink>
  </main>
</template>

<style scoped lang="scss">
.parts {
  max-width: 1280px;
  margin: 0 auto;
  padding: 0 var(--fbz-space-8) 80px;
}

.section-title {
  margin: 0 0 var(--fbz-space-4);
  font-size: var(--fbz-font-size-lg);
  font-weight: 700;
}

.grid {
  display: grid;
  grid-template-columns: repeat(auto-fill, minmax(150px, 1fr));
  gap: var(--fbz-space-5) var(--fbz-space-4);
}

.empty-state {
  margin: 0;
  color: var(--fbz-color-text-muted);
  font-size: var(--fbz-font-size-sm);
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

@media (max-width: 600px) {
  .parts {
    padding: 0 var(--fbz-space-4) 60px;
  }

  .grid {
    grid-template-columns: repeat(auto-fill, minmax(110px, 1fr));
    gap: var(--fbz-space-4) var(--fbz-space-3);
  }
}
</style>
