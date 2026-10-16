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
  <main class="c-page album-detail">
    <PageHeader :title="detail?.title ?? '专辑'" fallback="/library" />

    <p v-if="loading" class="c-state">
      <BaseIcon name="loading" :size="16" class="c-spin" />加载中…
    </p>
    <p v-else-if="failed" class="c-state is-error">
      <BaseIcon name="error" :size="16" />无法加载该专辑。
    </p>

    <template v-else-if="detail">
      <header class="head">
        <div class="cover">
          <MediaPoster :src="detail.poster" :title="detail.title" ratio="square" icon="album" />
        </div>
        <div class="meta">
          <span class="type-chip"><BaseIcon name="album" :size="12" />专辑</span>
          <h1>{{ detail.title }}</h1>
          <p class="c-meta">
            {{ detail.tracks.length }} 首<template v-if="detail.year">
              · {{ detail.year }}</template
            >
          </p>
          <button
            v-if="detail.tracks.length"
            class="du-btn du-btn-primary play-all"
            type="button"
            @click="playFrom(0)"
          >
            <BaseIcon name="play" :size="16" fill />播放全部
          </button>
        </div>
      </header>

      <BaseEmptyState v-if="!detail.tracks.length" compact icon="music" title="暂无曲目" />
      <ol v-else class="tracks">
        <li
          v-for="(track, i) in detail.tracks"
          :key="track.id"
          class="track"
          :class="{ playing: track.id === playingId }"
          role="button"
          tabindex="0"
          @click="playFrom(i)"
          @keydown.enter.prevent="playFrom(i)"
        >
          <span class="idx">
            <BaseIcon v-if="track.id === playingId" name="audio" :size="15" />
            <template v-else>{{ i + 1 }}</template>
          </span>
          <BaseIcon name="play" :size="13" fill class="hover-play" />
          <span class="title">{{ track.title }}</span>
          <span class="dur">{{ formatDuration(track.duration) }}</span>
        </li>
      </ol>
    </template>
  </main>
</template>

<style scoped lang="scss">
.album-detail {
  display: flex;
  flex-direction: column;
  gap: 24px;
}

.head {
  display: flex;
  align-items: flex-end;
  gap: 24px;
}

.cover {
  flex: 0 0 auto;
  width: 200px;
  overflow: hidden;
  border: 1px solid var(--ui-border);
  border-radius: var(--ui-radius-md);
  box-shadow: var(--ui-shadow-float);
}

.meta {
  min-width: 0;

  h1 {
    margin: 8px 0 4px;
    color: var(--ui-text);
    font-size: clamp(24px, 3vw, 36px);
    font-weight: 700;
    letter-spacing: -0.02em;
  }
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

.play-all {
  height: 38px;
  margin-top: 14px;
  border-radius: var(--ui-radius-md);
}

.tracks {
  margin: 0;
  padding: 0;
  overflow: hidden;
  border: 1px solid var(--ui-border);
  border-radius: var(--ui-radius-lg);
  background: var(--ui-surface);
  list-style: none;
}

.track {
  display: flex;
  align-items: center;
  gap: 12px;
  min-height: 44px;
  padding: 0 14px;
  border-bottom: 1px solid var(--ui-border);
  cursor: pointer;
  outline: none;

  &:last-child {
    border-bottom: 0;
  }

  &:hover,
  &:focus-visible {
    background: var(--ui-surface-hover);

    .idx {
      display: none;
    }

    .hover-play {
      display: inline-block;
    }
  }

  &.playing {
    background: rgb(var(--ui-accent-channel) / 8%);

    .idx,
    .title {
      color: var(--ui-accent-hover);
    }
  }
}

.idx {
  width: 22px;
  display: inline-flex;
  justify-content: flex-end;
  color: var(--ui-text-subtle);
  font-family: var(--ui-font-mono);
  font-size: 12px;
}

.hover-play {
  display: none;
  width: 22px;
  color: var(--ui-accent);
}

.title {
  flex: 1;
  min-width: 0;
  overflow: hidden;
  color: var(--ui-text);
  font-size: 13.5px;
  white-space: nowrap;
  text-overflow: ellipsis;
}

.dur {
  color: var(--ui-text-subtle);
  font-family: var(--ui-font-mono);
  font-size: 12px;
}

@media (max-width: 600px) {
  .head {
    flex-direction: column;
    align-items: flex-start;
  }

  .cover {
    width: 150px;
  }
}
</style>
