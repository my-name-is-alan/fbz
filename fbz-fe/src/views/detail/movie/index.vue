<script setup lang="ts">
import type { MovieDetail } from "@/types/media.ts";
import {
  findCatalogItem,
  getMovieDetail,
  imageUrl,
  refToItem,
  versionsFor,
} from "@/service/modules/tmdb.ts";
import { usePlaybackStore } from "@/stores/playback.ts";

const route = useRoute();
const playback = usePlaybackStore();
const id = computed(() => Number(route.params.id));

const item = computed(() => findCatalogItem("movie", id.value));
const detail = ref<MovieDetail>();

watch(
  id,
  async (v) => {
    detail.value = await getMovieDetail(v);
  },
  { immediate: true },
);

const versions = computed(() => versionsFor(id.value));

const meta = computed(() => {
  const it = item.value;
  if (!it) return [];
  const runtime = detail.value?.runtime
    ? `${Math.floor(detail.value.runtime / 60)}h ${detail.value.runtime % 60}m`
    : "";
  return [String(it.year ?? "—"), runtime, ...it.genres].filter(Boolean);
});

const directors = computed(() => detail.value?.directors.map((d) => d.name).join("、") ?? "");
const similar = computed(() => detail.value?.similar.map(refToItem) ?? []);

function playMovie() {
  const movie = item.value;
  if (!movie) return;

  playback.open({
    type: "movie",
    id: String(movie.id),
    title: movie.title,
    subtitle: meta.value.join(" · "),
    poster: imageUrl(movie.poster_path, "w500"),
    backdrop: imageUrl(movie.backdrop_path, "w1280"),
    tags: versions.value[0]?.tags,
    duration: detail.value?.runtime ? detail.value.runtime * 60 : 108 * 60,
  });
}
</script>

<template>
  <main v-if="item" class="detail-view">
    <PageHeader :title="item.title" fallback="/library/movie" />

    <DetailHero
      :title="item.title"
      :poster="imageUrl(item.poster_path, 'w500')"
      :backdrop="imageUrl(item.backdrop_path, 'w1280')"
      :meta="meta"
      :tagline="detail?.tagline"
      :overview="item.overview"
      :rating="item.rating"
      :versions="versions"
      @play="playMovie"
    >
      <template #extra>
        <dl class="facts">
          <div v-if="directors" class="fact">
            <dt>导演</dt>
            <dd>{{ directors }}</dd>
          </div>
          <div v-if="detail && detail.original_title !== item.title" class="fact">
            <dt>原名</dt>
            <dd>{{ detail.original_title }}</dd>
          </div>
          <div v-if="detail?.collection_id" class="fact">
            <dt>所属系列</dt>
            <dd>
              <RouterLink :to="`/collection/${detail.collection_id}`" class="link">
                {{ detail.collection_name }}
              </RouterLink>
            </dd>
          </div>
        </dl>
      </template>
    </DetailHero>

    <CastRow v-if="detail" :cast="detail.cast" />
    <SimilarRow :items="similar" />
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

.detail-missing {
  min-height: 100vh;
  display: grid;
  place-content: center;
  gap: var(--fbz-space-3);
  text-align: center;
  color: var(--fbz-color-text-muted);
}
</style>
