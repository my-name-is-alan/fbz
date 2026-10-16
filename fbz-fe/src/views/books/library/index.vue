<script setup lang="ts">
import {
  audioBookResume,
  fetchBooks,
  groupAudioBooks,
  type BookItem,
} from "@/service/modules/books.ts";
import { useLibraryStore } from "@/stores/library.ts";
import { useAuthStore } from "@/stores/auth.ts";
import { useMusicPlayerStore } from "@/stores/musicPlayer.ts";

const route = useRoute();
const router = useRouter();
const libraryStore = useLibraryStore();
const authStore = useAuthStore();
const player = useMusicPlayerStore();

const libraryId = computed(() => String(route.params.id));
const library = computed(() => libraryStore.getById(libraryId.value));

const items = ref<BookItem[]>([]);
const loading = ref(false);
const failed = ref(false);

const books = computed(() => items.value.filter((it) => it.type === "Book"));
const audioBooks = computed(() => groupAudioBooks(items.value));

async function load(id: string) {
  if (!authStore.userId) {
    failed.value = true;
    return;
  }
  loading.value = true;
  failed.value = false;
  try {
    items.value = await fetchBooks(authStore.userId, id);
  } catch {
    items.value = [];
    failed.value = true;
  } finally {
    loading.value = false;
  }
}

onMounted(() => {
  if (!libraryStore.loaded) void libraryStore.loadFromBackend();
  void load(libraryId.value);
});
watch(libraryId, (id) => void load(id));

function initial(name: string): string {
  return name.trim().charAt(0).toUpperCase() || "?";
}

/** 点有声书：从服务端记录的续听点继续（最近未播完的段落 + 位置）。 */
function playAudioBook(group: { title: string; tracks: BookItem[] }) {
  const resume = audioBookResume(group.tracks);
  player.playQueue(
    group.tracks.map((t, i) => ({
      id: t.id,
      title: t.name,
      subtitle: group.title,
      poster: t.poster ?? group.tracks.find((x) => x.poster)?.poster,
      startSeconds: i === resume.index ? resume.seconds : 0,
    })),
    resume.index,
  );
}

function resumeLabel(tracks: BookItem[]): string {
  const resume = audioBookResume(tracks);
  if (resume.index === 0 && resume.seconds === 0) return `${tracks.length} 段`;
  const minutes = Math.floor(resume.seconds / 60);
  return `继续 · 第 ${resume.index + 1}/${tracks.length} 段${minutes ? ` ${minutes} 分` : ""}`;
}

function bookMeta(book: BookItem): string {
  return [book.authors?.slice(0, 2).join("、"), book.year, book.folder]
    .filter((x) => x !== undefined && x !== "")
    .join(" · ");
}
</script>

<template>
  <main class="c-page books-library">
    <header class="page-head">
      <nav class="crumbs" aria-label="面包屑">
        <RouterLink to="/library"><BaseIcon name="library" :size="14" />媒体库</RouterLink>
        <BaseIcon name="right" :size="13" class="sep" />
        <span>{{ library?.name ?? "书籍" }}</span>
      </nav>
      <div class="title-row">
        <span class="type-ic"><BaseIcon name="book" :size="20" /></span>
        <h1 class="c-page-title">{{ library?.name ?? "书籍" }}</h1>
        <span class="c-meta">{{ books.length }} 本电子书 · {{ audioBooks.length }} 部有声书</span>
      </div>
    </header>

    <p v-if="loading" class="c-state">
      <BaseIcon name="loading" :size="16" class="c-spin" />加载中…
    </p>
    <p v-else-if="failed" class="c-state is-error">
      <BaseIcon name="error" :size="16" />无法连接到服务器，请检查登录状态。
    </p>
    <BaseEmptyState
      v-else-if="!items.length"
      icon="book"
      title="该书库暂无内容"
      description="支持 epub / pdf / cbz / mobi / azw3 / cbr（在线阅读）与 m4b / mp3 等有声书目录。"
    />

    <template v-else>
      <section v-if="books.length" class="section">
        <h2 class="c-section-title">
          <BaseIcon name="book" :size="16" class="lead" />电子书<span class="n">{{
            books.length
          }}</span>
        </h2>
        <div class="grid">
          <button
            v-for="(book, i) in books"
            :key="book.id"
            type="button"
            class="book-card"
            @click="router.push(`/book/${book.id}`)"
          >
            <div class="cover" :class="{ alt: i % 2 === 1 }">
              <img v-if="book.poster" :src="book.poster" alt="" loading="lazy" />
              <div v-else class="cover-ph">
                <BaseIcon name="book" :size="28" :stroke-width="1.3" />
                <span>{{ initial(book.name) }}</span>
              </div>
              <span class="open-hint"><BaseIcon name="book" :size="14" />阅读</span>
            </div>
            <span class="name" :title="book.name">{{ book.name }}</span>
            <span v-if="bookMeta(book)" class="c-meta">{{ bookMeta(book) }}</span>
          </button>
        </div>
      </section>

      <section v-if="audioBooks.length" class="section">
        <h2 class="c-section-title">
          <BaseIcon name="audiobook" :size="16" class="lead" />有声书<span class="n">{{
            audioBooks.length
          }}</span>
        </h2>
        <div class="grid">
          <button
            v-for="(group, i) in audioBooks"
            :key="group.title"
            type="button"
            class="book-card"
            @click="playAudioBook(group)"
          >
            <div class="cover square" :class="{ alt: i % 2 === 1 }">
              <img
                v-if="group.tracks[0]?.poster"
                :src="group.tracks[0].poster"
                alt=""
                loading="lazy"
              />
              <div v-else class="cover-ph">
                <BaseIcon name="audiobook" :size="28" :stroke-width="1.3" />
                <span>{{ initial(group.title) }}</span>
              </div>
              <span class="open-hint"><BaseIcon name="play" :size="13" fill />收听</span>
            </div>
            <span class="name" :title="group.title">{{ group.title }}</span>
            <span class="c-meta">{{ resumeLabel(group.tracks) }}</span>
          </button>
        </div>
      </section>
    </template>
  </main>
</template>

<style scoped lang="scss">
.books-library {
  display: flex;
  flex-direction: column;
  gap: 24px;
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
  flex-wrap: wrap;
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

.section {
  display: flex;
  flex-direction: column;
  gap: 12px;

  .lead,
  .n {
    color: var(--ui-text-subtle);
  }

  .n {
    font-size: 12px;
    font-weight: 500;
  }
}

.grid {
  display: grid;
  grid-template-columns: repeat(auto-fill, minmax(140px, 1fr));
  gap: 20px 16px;
}

.book-card {
  display: flex;
  flex-direction: column;
  gap: 2px;
  min-width: 0;
  padding: 0;
  border: 0;
  background: transparent;
  color: inherit;
  text-align: left;
  cursor: pointer;

  &:hover {
    .cover {
      border-color: var(--ui-border-strong);
      transform: translateY(-2px);
    }

    .open-hint {
      opacity: 1;
    }

    .name {
      color: var(--ui-accent-hover);
    }
  }
}

.cover {
  position: relative;
  aspect-ratio: 2 / 3;
  margin-bottom: 6px;
  overflow: hidden;
  border: 1px solid var(--ui-border);
  border-radius: var(--ui-radius-md);
  background: var(--ui-surface-raised);
  transition:
    transform var(--ui-duration-base) var(--ui-ease-out),
    border-color var(--ui-duration-fast);

  &.square {
    aspect-ratio: 1;
  }

  &.alt {
    background: var(--ui-surface);
  }

  img {
    width: 100%;
    height: 100%;
    object-fit: cover;
  }
}

.cover-ph {
  position: absolute;
  inset: 0;
  display: flex;
  flex-direction: column;
  align-items: center;
  justify-content: center;
  gap: 6px;
  color: var(--ui-border-strong);

  span {
    color: var(--ui-text-subtle);
    font-size: 22px;
    font-weight: 600;
  }
}

.open-hint {
  position: absolute;
  left: 8px;
  bottom: 8px;
  display: inline-flex;
  align-items: center;
  gap: 4px;
  height: 24px;
  padding: 0 8px;
  border-radius: 6px;
  background: var(--ui-accent);
  color: var(--ui-accent-contrast);
  font-size: 11.5px;
  font-weight: 600;
  opacity: 0;
  transition: opacity var(--ui-duration-fast);
}

.name {
  overflow: hidden;
  color: var(--ui-text);
  font-size: 13.5px;
  font-weight: 600;
  white-space: nowrap;
  text-overflow: ellipsis;
  transition: color var(--ui-duration-fast);
}

@media (hover: none) {
  .open-hint {
    opacity: 1;
  }
}

@media (max-width: 600px) {
  .grid {
    grid-template-columns: repeat(auto-fill, minmax(104px, 1fr));
  }
}
</style>
