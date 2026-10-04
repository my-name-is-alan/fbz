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
  <main class="artist-detail">
    <PageHeader :title="detail?.name ?? '艺术家'" fallback="/library" />

    <p v-if="loading" class="hint">加载中…</p>
    <p v-else-if="failed" class="hint">无法加载该艺术家。</p>

    <template v-else-if="detail">
      <header class="head">
        <h1>{{ detail.name }}</h1>
        <p class="sub">
          {{ detail.albums.length }} 张专辑<span v-if="yearRange"> · {{ yearRange }}</span>
        </p>
      </header>

      <p v-if="!detail.albums.length" class="hint">暂无专辑。</p>
      <div v-else class="grid">
        <RouterLink
          v-for="(album, i) in detail.albums"
          :key="album.id"
          :to="`/album/${album.id}`"
          class="album-card"
        >
          <MediaPoster :src="album.poster" :title="album.title" :variant="(i % 2) as 0 | 1" />
          <span class="title">{{ album.title }}</span>
          <span v-if="album.year" class="year">{{ album.year }}</span>
        </RouterLink>
      </div>
    </template>
  </main>
</template>

<style scoped lang="scss">
.artist-detail {
  padding: calc(var(--header-h, 60px) + var(--fbz-space-6)) var(--fbz-space-8) 80px;
  min-height: 100vh;
}

.hint {
  color: var(--fbz-color-text-muted);
  font-size: var(--fbz-font-size-md);
}

.head {
  margin-bottom: var(--fbz-space-6);

  h1 {
    margin: 0 0 var(--fbz-space-2);
    font-size: var(--fbz-font-size-xl);
    font-weight: 800;
  }

  .sub {
    margin: 0;
    color: var(--fbz-color-text-muted);
    font-size: var(--fbz-font-size-md);
  }
}

.grid {
  display: grid;
  grid-template-columns: repeat(auto-fill, minmax(160px, 1fr));
  gap: var(--fbz-space-5);
}

.album-card {
  display: flex;
  flex-direction: column;
  gap: var(--fbz-space-2);
  text-decoration: none;
  color: inherit;

  .title {
    font-size: var(--fbz-font-size-sm);
    font-weight: 600;
  }

  .year {
    font-size: var(--fbz-font-size-xs);
    color: var(--fbz-color-text-muted);
  }
}

@media (max-width: 600px) {
  .artist-detail {
    padding: calc(var(--header-h, 56px) + var(--fbz-space-4)) var(--fbz-space-4) 60px;
  }

  .grid {
    grid-template-columns: repeat(auto-fill, minmax(110px, 1fr));
    gap: var(--fbz-space-4);
  }
}
</style>
