<script setup lang="ts">
import { fetchArtists } from "@/service/modules/music.ts";
import type { MusicArtist } from "@/types/music.ts";
import { useLibraryStore } from "@/stores/library.ts";
import { useAuthStore } from "@/stores/auth.ts";

const route = useRoute();
const libraryStore = useLibraryStore();
const authStore = useAuthStore();

const libraryId = computed(() => String(route.params.id));
const library = computed(() => libraryStore.getById(libraryId.value));

const artists = ref<MusicArtist[]>([]);
const loading = ref(false);
const failed = ref(false);

/** 拉取当前音乐库的艺术家列表；未登录或后端不可达时置空并标记失败态。 */
async function loadArtists(id: string) {
  if (!authStore.userId) {
    failed.value = true;
    return;
  }
  loading.value = true;
  failed.value = false;
  try {
    const result = await fetchArtists(id);
    artists.value = result.items;
  } catch {
    artists.value = [];
    failed.value = true;
  } finally {
    loading.value = false;
  }
}

onMounted(() => {
  if (!libraryStore.loaded) void libraryStore.loadFromBackend();
  void loadArtists(libraryId.value);
});
watch(libraryId, (id) => {
  artists.value = [];
  void loadArtists(id);
});

/** 艺术家名首字母（大写）作占位封面字符；非字母取首字。 */
function initial(name: string): string {
  return name.trim().charAt(0).toUpperCase() || "?";
}
</script>

<template>
  <main class="c-page music-library">
    <header class="page-head">
      <nav class="crumbs" aria-label="面包屑">
        <RouterLink to="/library"><BaseIcon name="library" :size="14" />媒体库</RouterLink>
        <BaseIcon name="right" :size="13" class="sep" />
        <span>{{ library?.name ?? "音乐" }}</span>
      </nav>
      <div class="title-row">
        <span class="type-ic"><BaseIcon name="music" :size="20" /></span>
        <h1 class="c-page-title">{{ library?.name ?? "音乐" }}</h1>
        <span class="c-meta">{{ artists.length }} 位艺术家</span>
      </div>
    </header>

    <p v-if="loading" class="c-state">
      <BaseIcon name="loading" :size="16" class="c-spin" />加载中…
    </p>
    <p v-else-if="failed" class="c-state is-error">
      <BaseIcon name="error" :size="16" />无法连接到服务器，请检查登录状态。
    </p>
    <BaseEmptyState
      v-else-if="!artists.length"
      icon="artist"
      title="该音乐库暂无艺术家"
      description="扫描音乐文件后将按艺术家 → 专辑 → 曲目自动归类。"
    />

    <div v-else class="grid">
      <RouterLink
        v-for="artist in artists"
        :key="artist.id"
        :to="`/artist/${artist.id}`"
        class="artist-card"
      >
        <div class="avatar">
          <span class="initial">{{ initial(artist.name) }}</span>
          <BaseIcon name="artist" :size="14" class="badge" />
        </div>
        <span class="name" :title="artist.name">{{ artist.name }}</span>
      </RouterLink>
    </div>
  </main>
</template>

<style scoped lang="scss">
.music-library {
  display: flex;
  flex-direction: column;
  gap: 20px;
}

.crumbs {
  display: flex;
  align-items: center;
  gap: 6px;
  margin-bottom: 8px;
  color: var(--ui-text-subtle);
  font-size: 12.5px;

  a {
    display: inline-flex;
    align-items: center;
    gap: 4px;
    color: var(--ui-text-muted);
    text-decoration: none;

    &:hover {
      color: var(--ui-accent);
    }
  }
}

.title-row {
  display: flex;
  align-items: center;
  gap: 10px;
}

.type-ic {
  width: 36px;
  height: 36px;
  display: grid;
  place-items: center;
  border: 1px solid var(--ui-border);
  border-radius: var(--ui-radius-sm);
  background: var(--ui-surface);
  color: var(--ui-accent);
}

.grid {
  display: grid;
  grid-template-columns: repeat(auto-fill, minmax(132px, 1fr));
  gap: 22px 16px;
}

.artist-card {
  display: flex;
  flex-direction: column;
  align-items: center;
  gap: 8px;
  color: inherit;
  text-decoration: none;

  &:hover {
    .avatar {
      border-color: var(--ui-accent);
      transform: translateY(-2px);
    }

    .name {
      color: var(--ui-accent-hover);
    }
  }
}

.avatar {
  position: relative;
  width: 100%;
  aspect-ratio: 1;
  display: grid;
  place-items: center;
  border: 1px solid var(--ui-border);
  border-radius: 50%;
  background:
    radial-gradient(circle at 30% 25%, rgb(var(--ui-accent-channel) / 12%), transparent 60%),
    var(--ui-surface-raised);
  transition:
    border-color var(--ui-duration-fast),
    transform var(--ui-duration-base) var(--ui-ease-out);

  .initial {
    color: var(--ui-text-muted);
    font-size: 34px;
    font-weight: 600;
  }

  .badge {
    position: absolute;
    right: 10%;
    bottom: 10%;
    padding: 4px;
    box-sizing: content-box;
    border-radius: 50%;
    background: var(--ui-surface);
    color: var(--ui-text-subtle);
  }
}

.name {
  max-width: 100%;
  overflow: hidden;
  color: var(--ui-text);
  font-size: 13px;
  font-weight: 500;
  white-space: nowrap;
  text-overflow: ellipsis;
  transition: color var(--ui-duration-fast);
}

@media (max-width: 600px) {
  .grid {
    grid-template-columns: repeat(auto-fill, minmax(96px, 1fr));
  }
}
</style>
