<script setup lang="ts">
import {
  fetchChapter,
  fetchManifest,
  fileUrl,
  formatBadge,
  pageUrl,
  rewriteChapterHtml,
  type BookManifest,
} from "@/service/modules/books.ts";

const route = useRoute();
const router = useRouter();
const itemId = computed(() => String(route.params.id));

const manifest = ref<BookManifest | null>(null);
const index = ref(0);
const html = ref("");
const loading = ref(false);
const error = ref("");

const storageKey = computed(() => `fbz_book_pos_${itemId.value}`);
const total = computed(() => {
  const m = manifest.value;
  if (!m) return 0;
  if (m.Format === "epub") return m.Chapters?.length ?? 0;
  if (m.Format === "cbz") return m.PageCount ?? 0;
  return 1;
});

async function loadChapter() {
  const m = manifest.value;
  if (!m || m.Format !== "epub") return;
  loading.value = true;
  try {
    const ch = await fetchChapter(itemId.value, index.value);
    html.value = rewriteChapterHtml(itemId.value, ch.Path, ch.Html);
  } catch {
    error.value = "章节加载失败";
  } finally {
    loading.value = false;
  }
}

async function load() {
  error.value = "";
  manifest.value = null;
  try {
    manifest.value = await fetchManifest(itemId.value);
    const saved = Number(localStorage.getItem(storageKey.value) ?? 0);
    index.value = Number.isFinite(saved) && saved > 0 && saved < total.value ? saved : 0;
    await loadChapter();
  } catch {
    error.value = "无法打开此书";
  }
}

function go(delta: number) {
  const next = index.value + delta;
  if (next < 0 || next >= total.value) return;
  index.value = next;
  localStorage.setItem(storageKey.value, String(next));
  void loadChapter();
  window.scrollTo({ top: 0 });
}

function onKey(e: KeyboardEvent) {
  if (e.key === "ArrowRight") go(1);
  else if (e.key === "ArrowLeft") go(-1);
}

onMounted(() => {
  window.addEventListener("keydown", onKey);
  void load();
});
onBeforeUnmount(() => window.removeEventListener("keydown", onKey));
watch(itemId, () => void load());
</script>

<template>
  <main class="book-reader">
    <header class="reader-bar">
      <button type="button" class="ghost" @click="router.back()">‹ 返回</button>
      <span class="title">{{ manifest?.Title ?? "阅读" }}</span>
      <span
        v-if="manifest"
        class="format"
        :title="
          manifest.Converted ? '服务端已转换为可在线阅读的格式；下载得到原始文件' : '原始格式'
        "
        >{{ formatBadge(manifest) }}</span
      >
      <a v-if="manifest?.Converted" class="ghost original" :href="fileUrl(itemId)" download
        >下载原文件</a
      >
      <span v-if="total > 1" class="pos">{{ index + 1 }} / {{ total }}</span>
    </header>

    <p v-if="error" class="hint">{{ error }}</p>
    <p v-else-if="!manifest" class="hint">加载中…</p>
    <p v-else-if="!manifest.Readable" class="hint">
      暂不支持在线阅读 {{ manifest.Format }} 格式，可在详情中下载。
    </p>

    <!-- eslint-disable-next-line vue/no-v-html -- sanitized by rewriteChapterHtml -->
    <article
      v-else-if="manifest.Format === 'epub'"
      class="epub"
      :class="{ dim: loading }"
      v-html="html"
    />

    <div v-else-if="manifest.Format === 'cbz'" class="comic">
      <img :src="pageUrl(itemId, index)" alt="" @click="go(1)" />
    </div>

    <iframe v-else-if="manifest.Format === 'pdf'" class="pdf" :src="fileUrl(itemId)" title="PDF" />

    <nav v-if="manifest && manifest.Readable && total > 1" class="pager">
      <button type="button" :disabled="index === 0" @click="go(-1)">
        上一{{ manifest.Format === "cbz" ? "页" : "章" }}
      </button>
      <button type="button" :disabled="index >= total - 1" @click="go(1)">
        下一{{ manifest.Format === "cbz" ? "页" : "章" }}
      </button>
    </nav>
  </main>
</template>

<style scoped lang="scss">
.book-reader {
  padding: calc(var(--header-h, 60px) + var(--fbz-space-6)) var(--fbz-space-6) 80px;
  min-height: 100vh;
  max-width: 920px;
  margin: 0 auto;
}

.reader-bar {
  display: flex;
  align-items: center;
  gap: var(--fbz-space-4);
  margin-bottom: var(--fbz-space-5);

  .title {
    flex: 1;
    font-weight: 700;
    font-size: var(--fbz-font-size-lg);
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .format {
    padding: 2px var(--fbz-space-2);
    border: 1px solid var(--fbz-color-border, currentColor);
    border-radius: 999px;
    color: var(--fbz-color-text-muted);
    font-size: var(--fbz-font-size-xs, 12px);
    white-space: nowrap;
  }

  .original {
    color: var(--fbz-color-text-muted);
    font-size: var(--fbz-font-size-sm);
    white-space: nowrap;
  }

  .pos {
    color: var(--fbz-color-text-muted);
    font-size: var(--fbz-font-size-sm);
  }
}

button {
  border: 1px solid var(--fbz-color-line-soft);
  background: var(--fbz-color-panel);
  color: inherit;
  border-radius: 999px;
  padding: 6px 16px;
  cursor: pointer;

  &:disabled {
    opacity: 0.4;
    cursor: default;
  }

  &.ghost {
    background: none;
  }
}

.hint {
  color: var(--fbz-color-text-muted);
}

.epub {
  line-height: 1.8;
  font-size: 17px;
  transition: opacity var(--fbz-motion-fast);

  &.dim {
    opacity: 0.5;
  }

  :deep(img) {
    max-width: 100%;
    height: auto;
  }
}

.comic img {
  display: block;
  max-width: 100%;
  margin: 0 auto;
  cursor: pointer;
}

.pdf {
  width: 100%;
  height: calc(100vh - 180px);
  border: 1px solid var(--fbz-color-line-soft);
  border-radius: 10px;
  background: #fff;
}

.pager {
  display: flex;
  justify-content: space-between;
  margin-top: var(--fbz-space-6);
}
</style>
