<script setup lang="ts">
import { albumYearRange, fetchArtistDetail } from "@/service/modules/music.ts";
import type { MusicArtistDetail } from "@/types/music.ts";

const route = useRoute();
const artistId = computed(() => String(route.params.id));

const detail = ref<MusicArtistDetail | null>(null);
const loading = ref(false);
const failed = ref(false);

async function loadArtist(id: string) {
  loading.value = true;
  failed.value = false;
  try {
    detail.value = await fetchArtistDetail(id);
  } catch {
    detail.value = null;
    failed.value = true;
  } finally {
    loading.value = false;
  }
}

watch(artistId, (id) => void loadArtist(id), { immediate: true });

const yearRange = computed(() => (detail.value ? albumYearRange(detail.value.albums) : ""));
</script>

<template>
  <main class="c-page artist-detail">
    <PageHeader :title="detail?.name ?? '艺术家'" fallback="/library" />

    <p v-if="loading" class="c-state">
      <BaseIcon name="loading" :size="16" class="c-spin" />加载中…
    </p>
    <p v-else-if="failed" class="c-state is-error">
      <BaseIcon name="error" :size="16" />无法加载该艺术家。
    </p>

    <template v-else-if="detail">
      <header class="head">
        <div class="avatar"><BaseIcon name="artist" :size="40" :stroke-width="1.3" /></div>
        <div>
          <span class="type-chip"><BaseIcon name="artist" :size="12" />艺术家</span>
          <h1>{{ detail.name }}</h1>
          <p class="c-meta">
            {{ detail.albums.length }} 张专辑<template v-if="yearRange">
              · {{ yearRange }}</template
            >
          </p>
        </div>
      </header>

      <BaseEmptyState v-if="!detail.albums.length" compact icon="album" title="暂无专辑" />
      <section v-else class="albums">
        <h2 class="c-section-title"><BaseIcon name="album" :size="16" class="lead" />专辑</h2>
        <div class="grid">
          <RouterLink
            v-for="(album, i) in detail.albums"
            :key="album.id"
            :to="`/album/${album.id}`"
            class="album-card"
          >
            <div class="cover">
              <MediaPoster
                :src="album.poster"
                :title="album.title"
                ratio="square"
                icon="album"
                :variant="(i % 2) as 0 | 1"
              />
            </div>
            <span class="title" :title="album.title">{{ album.title }}</span>
            <span v-if="album.year" class="c-meta">{{ album.year }}</span>
          </RouterLink>
        </div>
      </section>
    </template>
  </main>
</template>

<style scoped lang="scss">
.artist-detail {
  display: flex;
  flex-direction: column;
  gap: 24px;
}

.head {
  display: flex;
  align-items: center;
  gap: 20px;

  h1 {
    margin: 8px 0 4px;
    color: var(--ui-text);
    font-size: clamp(24px, 3vw, 36px);
    font-weight: 700;
    letter-spacing: -0.02em;
  }
}

.avatar {
  width: 120px;
  height: 120px;
  display: grid;
  place-items: center;
  border: 1px solid var(--ui-border);
  border-radius: 50%;
  background:
    radial-gradient(circle at 30% 25%, rgb(var(--ui-accent-channel) / 14%), transparent 60%),
    var(--ui-surface-raised);
  color: var(--ui-text-subtle);
}

.type-chip {
  display: inline-flex;
  align-items: center;
  gap: 4px;
  height: 22px;
  padding: 0 8px;
  border: 1px solid var(--ui-border-strong);
  border-radius: 5px;
  color: var(--ui-text-muted);
  font-size: 11.5px;
  font-weight: 600;
}

.albums {
  display: flex;
  flex-direction: column;
  gap: 12px;

  .lead {
    color: var(--ui-text-subtle);
  }
}

.grid {
  display: grid;
  grid-template-columns: repeat(auto-fill, minmax(160px, 1fr));
  gap: 20px 16px;
}

.album-card {
  display: flex;
  flex-direction: column;
  gap: 2px;
  min-width: 0;
  color: inherit;
  text-decoration: none;

  &:hover {
    .cover {
      border-color: var(--ui-border-strong);
      transform: translateY(-2px);
    }

    .title {
      color: var(--ui-accent-hover);
    }
  }
}

.cover {
  margin-bottom: 6px;
  overflow: hidden;
  border: 1px solid var(--ui-border);
  border-radius: var(--ui-radius-md);
  transition:
    transform var(--ui-duration-base) var(--ui-ease-out),
    border-color var(--ui-duration-fast);
}

.title {
  overflow: hidden;
  color: var(--ui-text);
  font-size: 13.5px;
  font-weight: 600;
  white-space: nowrap;
  text-overflow: ellipsis;
  transition: color var(--ui-duration-fast);
}
</style>
