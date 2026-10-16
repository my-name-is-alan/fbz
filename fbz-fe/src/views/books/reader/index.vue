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
      <button type="button" class="bar-btn" aria-label="返回" @click="router.back()">
        <BaseIcon name="back" :size="16" /><span>返回</span>
      </button>
      <BaseIcon name="book" :size="16" class="bar-ic" />
      <span class="title">{{ manifest?.Title ?? "阅读" }}</span>
      <span
        v-if="manifest"
        class="format"
        :title="
          manifest.Converted ? '服务端已转换为可在线阅读的格式；下载得到原始文件' : '原始格式'
        "
        >{{ formatBadge(manifest) }}</span
      >
      <a v-if="manifest?.Converted" class="bar-btn" :href="fileUrl(itemId)" download>
        <BaseIcon name="download" :size="15" /><span>下载原文件</span>
      </a>
      <span v-if="total > 1" class="pos">{{ index + 1 }} / {{ total }}</span>
    </header>

    <div v-if="total > 1" class="reader-progress" aria-hidden="true">
      <span :style="{ width: `${((index + 1) / total) * 100}%` }" />
    </div>

    <p v-if="error" class="c-state is-error"><BaseIcon name="error" :size="16" />{{ error }}</p>
    <p v-else-if="!manifest" class="c-state">
      <BaseIcon name="loading" :size="16" class="c-spin" />加载中…
    </p>
    <BaseEmptyState
      v-else-if="!manifest.Readable"
      icon="book"
      :title="`暂不支持在线阅读 ${manifest.Format} 格式`"
      description="可下载原文件后使用本地阅读器打开。"
    >
      <a class="du-btn du-btn-ghost" :href="fileUrl(itemId)" download>
        <BaseIcon name="download" :size="15" />下载原文件
      </a>
    </BaseEmptyState>

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
      <button type="button" class="du-btn du-btn-ghost" :disabled="index === 0" @click="go(-1)">
        <BaseIcon name="left" :size="16" />上一{{ manifest.Format === "cbz" ? "页" : "章" }}
      </button>
      <span class="c-meta">{{ index + 1 }} / {{ total }} · 方向键翻页</span>
      <button
        type="button"
        class="du-btn du-btn-ghost"
        :disabled="index >= total - 1"
        @click="go(1)"
      >
        下一{{ manifest.Format === "cbz" ? "页" : "章" }}<BaseIcon name="right" :size="16" />
      </button>
    </nav>
  </main>
</template>

<style scoped lang="scss">
.book-reader {
  min-height: calc(100vh - var(--ui-header-height));
  display: flex;
  flex-direction: column;
}

.reader-bar {
  position: sticky;
  top: var(--ui-header-height);
  z-index: 10;
  display: flex;
  align-items: center;
  gap: 10px;
  padding: 10px var(--ui-shell-gutter);
  border-bottom: 1px solid var(--ui-border);
  background: var(--ui-glass-strong);
  backdrop-filter: blur(14px);
}

.bar-btn {
  display: inline-flex;
  align-items: center;
  gap: 4px;
  height: 30px;
  padding: 0 10px;
  border: 1px solid var(--ui-border);
  border-radius: var(--ui-radius-sm);
  background: var(--ui-surface);
  color: var(--ui-text);
  font-size: 12.5px;
  text-decoration: none;
  cursor: pointer;

  &:hover {
    border-color: var(--ui-border-strong);
    background: var(--ui-surface-hover);
  }
}

.bar-ic {
  color: var(--ui-text-subtle);
}

.title {
  flex: 1;
  min-width: 0;
  overflow: hidden;
  color: var(--ui-text);
  font-size: 14px;
  font-weight: 600;
  white-space: nowrap;
  text-overflow: ellipsis;
}

.format {
  height: 20px;
  padding: 0 6px;
  display: inline-flex;
  align-items: center;
  border: 1px solid var(--ui-border-strong);
  border-radius: 4px;
  color: var(--ui-text-muted);
  font-size: 10.5px;
  font-weight: 700;
}

.pos {
  color: var(--ui-text-subtle);
  font-family: var(--ui-font-mono);
  font-size: 12px;
}

.reader-progress {
  height: 2px;
  background: var(--ui-border);

  span {
    display: block;
    height: 100%;
    background: var(--ui-accent);
    transition: width var(--ui-duration-base);
  }
}

.c-state {
  padding: 40px var(--ui-shell-gutter);
}

.epub {
  width: 100%;
  max-width: 760px;
  margin: 0 auto;
  padding: 32px var(--ui-shell-gutter) 24px;
  color: var(--ui-text);
  font-size: 17px;
  line-height: 1.9;
  transition: opacity var(--ui-duration-fast);

  &.dim {
    opacity: 0.5;
  }

  :deep(img) {
    max-width: 100%;
    height: auto;
  }

  :deep(a) {
    color: var(--ui-accent);
  }
}

.comic {
  display: flex;
  justify-content: center;
  padding: 16px;

  img {
    max-width: 100%;
    max-height: calc(100vh - var(--ui-header-height) - 140px);
    border-radius: var(--ui-radius-sm);
    cursor: pointer;
  }
}

.pdf {
  flex: 1;
  width: 100%;
  min-height: calc(100vh - var(--ui-header-height) - 60px);
  border: 0;
  background: #fff;
}

.pager {
  display: flex;
  align-items: center;
  justify-content: center;
  gap: 16px;
  padding: 16px var(--ui-shell-gutter) 40px;
}

@media (max-width: 600px) {
  .bar-btn span {
    display: none;
  }

  .epub {
    font-size: 16px;
  }
}
</style>
