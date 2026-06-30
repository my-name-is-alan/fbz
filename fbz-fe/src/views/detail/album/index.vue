<script setup lang="ts">
import { fetchAlbumDetail, formatDuration } from "@/service/modules/music.ts";
import type { MusicAlbumDetail } from "@/types/music.ts";
import { useMusicPlayerStore } from "@/stores/musicPlayer.ts";

const route = useRoute();
const albumId = computed(() => String(route.params.id));
const player = useMusicPlayerStore();

const detail = ref<MusicAlbumDetail | null>(null);
const loading = ref(false);
const failed = ref(false);

async function loadAlbum(id: string) {
  loading.value = true;
  failed.value = false;
  try {
    detail.value = await fetchAlbumDetail(id);
  } catch {
    detail.value = null;
    failed.value = true;
  } finally {
    loading.value = false;
  }
}

watch(albumId, (id) => void loadAlbum(id), { immediate: true });

/** 点击某曲目：把整张专辑作为队列、从该曲开始播放（专辑名/年份作副标题）。 */
function playFrom(index: number) {
  const album = detail.value;
  if (!album) return;
  const subtitle = album.year ? `${album.title} · ${album.year}` : album.title;
  player.playQueue(
    album.tracks.map((track) => ({
      id: track.id,
      title: track.title,
      subtitle,
      poster: album.poster,
    })),
    index,
  );
}

/** 当前播放中的曲目（高亮用）。 */
const playingId = computed(() => player.current?.id);
</script>

<template>
  <main class="album-detail">
    <PageHeader :title="detail?.title ?? '专辑'" fallback="/library" />

    <p v-if="loading" class="hint">加载中…</p>
    <p v-else-if="failed" class="hint">无法加载该专辑。</p>

    <template v-else-if="detail">
      <header class="head">
        <MediaPoster class="cover" :src="detail.poster" :title="detail.title" />
        <div class="meta">
          <h1>{{ detail.title }}</h1>
          <p class="sub">
            {{ detail.tracks.length }} 首<span v-if="detail.year"> · {{ detail.year }}</span>
          </p>
        </div>
      </header>

      <p v-if="!detail.tracks.length" class="hint">暂无曲目。</p>
      <ol v-else class="tracks">
        <li
          v-for="(track, i) in detail.tracks"
          :key="track.id"
          class="track"
          :class="{ playing: track.id === playingId }"
          @click="playFrom(i)"
        >
          <span class="idx">{{ track.id === playingId ? "♪" : i + 1 }}</span>
          <span class="title">{{ track.title }}</span>
          <span class="dur">{{ formatDuration(track.duration) }}</span>
        </li>
      </ol>
    </template>
  </main>
</template>

<style scoped lang="scss">
.album-detail {
  padding: calc(var(--header-h, 60px) + var(--fbz-space-6)) var(--fbz-space-8) 80px;
  min-height: 100vh;
}

.hint {
  color: var(--fbz-color-text-muted);
  font-size: var(--fbz-font-size-md);
}

.head {
  display: flex;
  gap: var(--fbz-space-5);
  align-items: flex-end;
  margin-bottom: var(--fbz-space-6);

  .cover {
    width: 200px;
    flex: none;
  }

  .meta {
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
}

.tracks {
  list-style: none;
  margin: 0;
  padding: 0;
}

.track {
  display: flex;
  align-items: center;
  gap: var(--fbz-space-4);
  padding: var(--fbz-space-3) var(--fbz-space-3);
  border-bottom: 1px solid var(--fbz-color-line-soft);
  cursor: pointer;
  transition: background var(--fbz-motion-fast);

  &:hover {
    background: var(--fbz-color-panel);
  }

  &.playing {
    .idx,
    .title {
      color: var(--fbz-color-brand-500);
    }
  }

  .idx {
    width: 2ch;
    text-align: right;
    color: var(--fbz-color-text-muted);
    font-variant-numeric: tabular-nums;
  }

  .title {
    flex: 1;
    font-size: var(--fbz-font-size-md);
  }

  .dur {
    color: var(--fbz-color-text-muted);
    font-size: var(--fbz-font-size-sm);
    font-variant-numeric: tabular-nums;
  }
}

@media (max-width: 600px) {
  .album-detail {
    padding: calc(var(--header-h, 56px) + var(--fbz-space-4)) var(--fbz-space-4) 60px;
  }

  .head .cover {
    width: 120px;
  }
}
</style>
