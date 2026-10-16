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
  <main class="books-library">
    <header class="page-head">
      <h1>{{ library?.name ?? "书籍" }}</h1>
      <p class="sub">{{ books.length }} 本电子书 · {{ audioBooks.length }} 部有声书</p>
    </header>

    <p v-if="loading" class="hint">加载中…</p>
    <p v-else-if="failed" class="hint">无法连接到服务器，请检查登录状态。</p>
    <p v-else-if="!items.length" class="hint">
      该书库暂无内容。支持 epub / pdf / cbz（阅读）与 m4b / mp3 等有声书目录。
    </p>

    <template v-else>
      <section v-if="books.length" class="section">
        <h2>电子书</h2>
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
              <span v-else>{{ initial(book.name) }}</span>
            </div>
            <span class="name">{{ book.name }}</span>
            <span v-if="bookMeta(book)" class="meta">{{ bookMeta(book) }}</span>
          </button>
        </div>
      </section>

      <section v-if="audioBooks.length" class="section">
        <h2>有声书</h2>
        <div class="grid">
          <button
            v-for="(group, i) in audioBooks"
            :key="group.title"
            type="button"
            class="book-card"
            @click="playAudioBook(group)"
          >
            <div class="cover audio" :class="{ alt: i % 2 === 1 }">
              <img
                v-if="group.tracks[0]?.poster"
                :src="group.tracks[0].poster"
                alt=""
                loading="lazy"
              />
              <span v-else>{{ initial(group.title) }}</span>
            </div>
            <span class="name">{{ group.title }}</span>
            <span class="meta">{{ resumeLabel(group.tracks) }}</span>
          </button>
        </div>
      </section>
    </template>
  </main>
</template>

<style scoped lang="scss">
.books-library {
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

.section + .section {
  margin-top: var(--fbz-space-8);
}

.section h2 {
  margin: 0 0 var(--fbz-space-4);
  font-size: var(--fbz-font-size-lg);
  font-weight: 700;
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

.book-card {
  display: flex;
  flex-direction: column;
  gap: var(--fbz-space-2);
  padding: 0;
  border: 0;
  background: none;
  color: inherit;
  text-align: left;
  cursor: pointer;
}

.cover {
  width: 100%;
  aspect-ratio: 2 / 3;
  border-radius: var(--fbz-radius-md, 10px);
  border: 1px solid var(--fbz-color-line-soft);
  background: var(--fbz-color-panel);
  display: grid;
  place-content: center;
  overflow: hidden;
  transition:
    border-color var(--fbz-motion-fast),
    transform var(--fbz-motion-fast);

  &.audio {
    aspect-ratio: 1;
  }

  &.alt {
    background: var(--fbz-color-panel-strong);
  }

  img {
    width: 100%;
    height: 100%;
    object-fit: cover;
  }

  span {
    font-family: var(--fbz-font-display);
    font-size: var(--fbz-font-size-xl);
    font-weight: 700;
    color: var(--fbz-color-text-muted);
  }

  .book-card:hover & {
    border-color: var(--fbz-color-brand-500);
    transform: translateY(-3px);
  }
}

.name {
  font-size: var(--fbz-font-size-sm);
  font-weight: 600;
}

.meta {
  font-size: var(--fbz-font-size-xs, 12px);
  color: var(--fbz-color-text-muted);
}

@media (max-width: 600px) {
  .books-library {
    padding: calc(var(--header-h, 56px) + var(--fbz-space-5)) var(--fbz-space-4) 60px;
  }

  .grid {
    grid-template-columns: repeat(auto-fill, minmax(100px, 1fr));
    gap: var(--fbz-space-4);
  }
}
</style>
