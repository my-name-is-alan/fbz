<script setup lang="ts">
import type { MediaItem, PersonDetail } from "@/types/media.ts";
import { getPersonDetail, imageUrl } from "@/service/modules/tmdb.ts";

const route = useRoute();
const id = computed(() => Number(route.params.id));
const person = ref<PersonDetail>();

watch(
  id,
  async (v) => {
    person.value = await getPersonDetail(v);
  },
  { immediate: true },
);

const knownFor = computed<MediaItem[]>(
  () =>
    person.value?.known_for.map((c) => ({
      id: String(c.id),
      libraryId: c.libraryId,
      detailType: c.type,
      title: c.title,
      meta: c.character ? `饰 ${c.character}` : String(c.year ?? ""),
      poster: imageUrl(c.poster_path, "w500"),
      rating: c.rating ?? undefined,
    })) ?? [],
);

const creditSummary = computed(() => {
  const credits = person.value?.known_for ?? [];
  return [
    { label: "代表作品", value: credits.length },
    { label: "电影", value: credits.filter((credit) => credit.type === "movie").length },
    { label: "剧集", value: credits.filter((credit) => credit.type === "tv").length },
  ];
});
</script>

<template>
  <main v-if="person" class="person-view">
    <PageHeader :title="person.name" />

    <div class="head">
      <div class="photo">
        <MediaPoster
          :src="imageUrl(person.profile_path, 'w500')"
          :title="person.name"
          ratio="poster"
        />
      </div>

      <div class="info">
        <h1 class="name">{{ person.name }}</h1>
        <div class="meta">
          <span v-if="person.known_for_department">{{ person.known_for_department }}</span>
          <template v-if="person.birthday">
            <span class="dot" />
            <span>{{ person.birthday }}</span>
          </template>
          <template v-if="person.place_of_birth">
            <span class="dot" />
            <span>{{ person.place_of_birth }}</span>
          </template>
        </div>
        <p v-if="person.biography" class="bio">{{ person.biography }}</p>
        <p v-else class="bio muted">暂无简介</p>

        <div class="credit-summary" aria-label="作品概览">
          <div v-for="item in creditSummary" :key="item.label" class="summary-item">
            <span class="summary-value">{{ item.value }}</span>
            <span class="summary-label">{{ item.label }}</span>
          </div>
        </div>
      </div>
    </div>

    <section v-if="knownFor.length" class="known">
      <h2 class="section-title">代表作品</h2>
      <div class="grid">
        <MediaCard
          v-for="(item, i) in knownFor"
          :key="item.id"
          :item="item"
          layout="poster"
          :variant="(i % 2) as 0 | 1"
        />
      </div>
    </section>
  </main>

  <main v-else class="detail-missing">
    <p>未找到该人物</p>
    <RouterLink to="/" class="link">返回首页</RouterLink>
  </main>
</template>

<style scoped lang="scss">
.person-view {
  max-width: 1280px;
  margin: 0 auto;
  padding: calc(var(--header-h, 60px) + var(--fbz-space-8)) var(--fbz-space-8) 80px;
  min-height: 100vh;
}

.head {
  display: flex;
  gap: var(--fbz-space-8);
  margin-bottom: var(--fbz-space-8);
}

.photo {
  flex: 0 0 220px;
  width: 220px;
  border-radius: var(--fbz-radius-hero);
  overflow: hidden;
  border: 1px solid var(--fbz-color-line);
  box-shadow: var(--fbz-shadow-panel);
}

.info {
  flex: 1;
  min-width: 0;
}

.name {
  margin: 0 0 var(--fbz-space-3);
  font-size: 36px;
  font-weight: 800;
}

.meta {
  display: flex;
  align-items: center;
  flex-wrap: wrap;
  gap: var(--fbz-space-3);
  margin-bottom: var(--fbz-space-4);
  font-size: var(--fbz-font-size-md);
  color: var(--fbz-color-text-soft);

  .dot {
    width: 3px;
    height: 3px;
    border-radius: 50%;
    background: var(--fbz-color-text-muted);
  }
}

.bio {
  max-width: 760px;
  margin: 0;
  font-size: var(--fbz-font-size-md);
  line-height: 1.8;
  color: var(--fbz-color-text-soft);
  white-space: pre-line;

  &.muted {
    color: var(--fbz-color-text-muted);
  }
}

.credit-summary {
  display: flex;
  flex-wrap: wrap;
  gap: var(--fbz-space-3);
  margin-top: var(--fbz-space-5);
}

.summary-item {
  min-width: 92px;
  padding: var(--fbz-space-3);
  border: 1px solid var(--fbz-color-line-soft);
  border-radius: var(--fbz-radius-card);
  background: rgba(255, 255, 255, 0.035);
}

.summary-value {
  display: block;
  color: var(--fbz-color-brand-500);
  font-size: var(--fbz-font-size-lg);
  font-weight: 900;
}

.summary-label {
  display: block;
  margin-top: 2px;
  color: var(--fbz-color-text-muted);
  font-size: var(--fbz-font-size-sm);
}

.section-title {
  margin: 0 0 var(--fbz-space-4);
  font-size: var(--fbz-font-size-lg);
  font-weight: 700;
}

.grid {
  display: grid;
  grid-template-columns: repeat(auto-fill, minmax(132px, 1fr));
  gap: var(--fbz-space-5) var(--fbz-space-4);
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
  .person-view {
    padding: calc(var(--header-h, 56px) + var(--fbz-space-5)) var(--fbz-space-4) 60px;
  }

  .head {
    flex-direction: column;
    gap: var(--fbz-space-4);
  }

  .photo {
    flex-basis: auto;
    width: 132px;
  }

  .name {
    font-size: 28px;
  }

  .grid {
    grid-template-columns: repeat(auto-fill, minmax(104px, 1fr));
    gap: var(--fbz-space-4) var(--fbz-space-3);
  }
}
</style>
