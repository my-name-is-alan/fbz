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
  <main class="music-library">
    <header class="page-head">
      <h1>{{ library?.name ?? "音乐" }}</h1>
      <p class="sub">{{ artists.length }} 位艺术家</p>
    </header>

    <p v-if="loading" class="hint">加载中…</p>
    <p v-else-if="failed" class="hint">无法连接到服务器，请检查登录状态。</p>
    <p v-else-if="!artists.length" class="hint">该音乐库暂无艺术家。扫描音乐文件后将自动归类。</p>

    <div v-else class="grid">
      <RouterLink
        v-for="(artist, i) in artists"
        :key="artist.id"
        :to="`/artist/${artist.id}`"
        class="artist-card"
      >
        <div class="avatar" :class="{ alt: i % 2 === 1 }">
          <span>{{ initial(artist.name) }}</span>
        </div>
        <span class="name">{{ artist.name }}</span>
      </RouterLink>
    </div>
  </main>
</template>

<style scoped lang="scss">
.music-library {
  padding: calc(var(--header-h, 60px) + var(--fbz-space-8)) var(--fbz-space-8) 80px;
  min-height: 100vh;
}

.page-head {
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

.hint {
  color: var(--fbz-color-text-muted);
  font-size: var(--fbz-font-size-md);
}

.grid {
  display: grid;
  grid-template-columns: repeat(auto-fill, minmax(140px, 1fr));
  gap: var(--fbz-space-5);
}

.artist-card {
  display: flex;
  flex-direction: column;
  align-items: center;
  gap: var(--fbz-space-3);
  text-decoration: none;
  color: inherit;
}

.avatar {
  width: 100%;
  aspect-ratio: 1;
  border-radius: 50%;
  border: 1px solid var(--fbz-color-line-soft);
  background: var(--fbz-color-panel);
  display: grid;
  place-content: center;
  transition:
    border-color var(--fbz-motion-fast),
    transform var(--fbz-motion-fast);

  &.alt {
    background: var(--fbz-color-panel-strong);
  }

  span {
    font-family: var(--fbz-font-display);
    font-size: var(--fbz-font-size-xl);
    font-weight: 700;
    color: var(--fbz-color-text-muted);
  }

  .artist-card:hover & {
    border-color: var(--fbz-color-brand-500);
    transform: translateY(-3px);
  }
}

.name {
  font-size: var(--fbz-font-size-sm);
  font-weight: 600;
  text-align: center;
}

@media (max-width: 600px) {
  .music-library {
    padding: calc(var(--header-h, 56px) + var(--fbz-space-5)) var(--fbz-space-4) 60px;
  }

  .grid {
    grid-template-columns: repeat(auto-fill, minmax(100px, 1fr));
    gap: var(--fbz-space-4);
  }
}
</style>
