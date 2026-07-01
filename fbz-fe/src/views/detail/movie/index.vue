<script setup lang="ts">
import type { DetailViewModel } from "@/service/modules/detail.ts";
import { loadMovieDetail } from "@/service/modules/detail.ts";
import { usePlaybackStore } from "@/stores/playback.ts";
import { useUiStore } from "@/stores/ui.ts";

const route = useRoute();
const playback = usePlaybackStore();
const uiStore = useUiStore();
const routeId = computed(() => String(route.params.id));

const detail = ref<DetailViewModel>();

watch(
  routeId,
  async (v) => {
    detail.value = await loadMovieDetail(v);
  },
  { immediate: true },
);

function playMovie() {
  const movie = detail.value;
  if (!movie) return;

  playback.open({
    type: "movie",
    id: movie.id,
    title: movie.title,
    subtitle: movie.meta.join(" · "),
    poster: movie.poster,
    backdrop: movie.backdrop,
    tags: movie.versions[0]?.tags,
    duration: movie.runtimeSeconds ?? 108 * 60,
  });
}

/** 打开元数据管理弹层：用详情视图模型拼一个最小 MediaItem 传入。 */
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
    isFavorite: false,
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
      :tagline="detail.tagline"
      :overview="detail.overview"
      :rating="detail.rating"
      :versions="detail.versions"
      @play="playMovie"
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
          <div v-if="detail.collectionId" class="fact">
            <dt>所属系列</dt>
            <dd>
              <RouterLink :to="`/collection/${detail.collectionId}`" class="link">
                {{ detail.collectionName }}
              </RouterLink>
            </dd>
          </div>
        </dl>
        <button type="button" class="edit-meta-btn" @click="editMetadata">编辑元数据</button>
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
