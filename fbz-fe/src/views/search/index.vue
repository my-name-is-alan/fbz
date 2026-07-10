<script setup lang="ts">
import { searchHints } from "@/service/modules/search.ts";
import type { SearchKind, SearchResultItem } from "@/service/modules/search.ts";

/**
 * 搜索结果页：从路由 query `?q=` 取关键词，受控输入框（回车立即跳转 / 输入 debounce 更新 URL），
 * 结果按类型分区展示（电影 / 剧集 / 人物 / 专辑 / 艺术家）。
 * 电影/剧集用 MediaCard（自带播放覆盖层与自路由）；人物/专辑/艺术家用 RouterLink 卡片按各自详情路由跳转。
 */

const route = useRoute();
const router = useRouter();

/** 关键词的唯一事实源是路由 query；输入框只是它的受控镜像。 */
function queryFromRoute(): string {
  const q = route.query.q;
  return Array.isArray(q) ? (q[0] ?? "") : (q ?? "");
}

const keyword = ref(queryFromRoute());
const results = ref<SearchResultItem[]>([]);
const loading = ref(false);
const failed = ref(false);
/** 已发起过至少一次有效搜索（用于区分「未搜索引导态」与「无结果空态」）。 */
const searched = ref(false);
const inputEl = ref<HTMLInputElement | null>(null);

/** 当前进行中的请求控制器，切换关键词时中止上一请求。 */
let controller: AbortController | undefined;

/** 分区顺序与标题。 */
const SECTIONS: { kind: SearchKind; title: string }[] = [
  { kind: "movie", title: "电影" },
  { kind: "tv", title: "剧集" },
  { kind: "person", title: "人物" },
  { kind: "album", title: "专辑" },
  { kind: "artist", title: "艺术家" },
];

/** 按类型分组的非空分区（保持 SECTIONS 顺序）。 */
const groups = computed(() =>
  SECTIONS.map((section) => ({
    ...section,
    items: results.value.filter((item) => item.kind === section.kind),
  })).filter((section) => section.items.length > 0),
);

/** 执行搜索：空关键词清空结果并复位；否则拉取并映射，中止的请求忽略。 */
async function runSearch(term: string) {
  controller?.abort();
  const trimmed = term.trim();
  if (!trimmed) {
    results.value = [];
    searched.value = false;
    failed.value = false;
    loading.value = false;
    return;
  }
  controller = new AbortController();
  loading.value = true;
  failed.value = false;
  try {
    results.value = await searchHints(trimmed, { signal: controller.signal });
    searched.value = true;
  } catch (error) {
    // 请求被新的关键词取消：忽略，保留加载态交给后续请求收敛。
    if ((error as { code?: string })?.code === "ERR_CANCELED") return;
    results.value = [];
    searched.value = true;
    failed.value = true;
  } finally {
    if (!controller?.signal.aborted) loading.value = false;
  }
}

/** 输入防抖：仅更新 URL（?q=），真正的搜索由对 route.query 的 watch 驱动，保证可分享/前进后退。 */
const pushQuery = debounce((value: string) => {
  const next = value.trim();
  if (next === queryFromRoute()) return;
  void router.replace({ name: "search", query: next ? { q: next } : {} });
}, 320);

function onInput() {
  pushQuery(keyword.value);
}

/** 回车立即提交：取消防抖并同步 URL（若关键词未变则直接重搜一次）。 */
function onSubmit() {
  pushQuery.cancel();
  const next = keyword.value.trim();
  if (next === queryFromRoute()) {
    void runSearch(next);
    return;
  }
  void router.replace({ name: "search", query: next ? { q: next } : {} });
}

// 路由 query 是关键词的事实源：直接／通过输入回写后统一在此触发搜索。
watch(
  () => route.query.q,
  () => {
    const q = queryFromRoute();
    keyword.value = q;
    void runSearch(q);
  },
  { immediate: true },
);

onMounted(() => inputEl.value?.focus());
onBeforeUnmount(() => {
  pushQuery.cancel();
  controller?.abort();
});
</script>

<template>
  <main class="search-view">
    <form class="search-bar" role="search" @submit.prevent="onSubmit">
      <span class="search-icon" aria-hidden="true" />
      <input
        ref="inputEl"
        v-model="keyword"
        class="search-input"
        type="search"
        name="q"
        placeholder="搜索影片、剧集、人物、专辑、艺术家…"
        autocomplete="off"
        aria-label="搜索关键词"
        @input="onInput"
      />
    </form>

    <p v-if="loading" class="hint">搜索中…</p>
    <p v-else-if="failed" class="hint">搜索失败，请检查网络或登录状态后重试。</p>
    <p v-else-if="!keyword.trim()" class="hint">输入关键词以搜索影片、剧集、人物、专辑与艺术家。</p>
    <p v-else-if="searched && !groups.length" class="hint">
      没有找到与「{{ keyword.trim() }}」相关的内容。
    </p>

    <section v-for="group in groups" :key="group.kind" class="result-section">
      <h2 class="section-title">
        {{ group.title }}
        <span class="section-count">{{ group.items.length }}</span>
      </h2>

      <div class="grid">
        <template v-for="(item, i) in group.items" :key="item.kind + item.id">
          <MediaCard
            v-if="item.kind === 'movie' || item.kind === 'tv'"
            :item="item"
            :variant="(i % 2) as 0 | 1"
            :show-resolution="false"
          />
          <RouterLink v-else :to="item.to" class="entity-card">
            <MediaPoster :src="item.poster" :title="item.title" :variant="(i % 2) as 0 | 1" />
            <div class="entity-footer">
              <span class="entity-title" :title="item.title">{{ item.title }}</span>
              <span v-if="item.meta" class="entity-meta">{{ item.meta }}</span>
            </div>
          </RouterLink>
        </template>
      </div>
    </section>
  </main>
</template>

<style scoped lang="scss">
.search-view {
  padding: calc(var(--header-h, 60px) + var(--fbz-space-8)) var(--fbz-space-8) 80px;
  min-height: 100vh;
}

.search-bar {
  display: flex;
  align-items: center;
  gap: var(--fbz-space-3);
  max-width: 640px;
  height: 48px;
  margin-bottom: var(--fbz-space-8);
  padding: 0 var(--fbz-space-4);
  border-radius: var(--fbz-radius-control);
  border: 1px solid var(--fbz-color-line);
  background: var(--fbz-color-panel-strong);
  transition: border-color var(--fbz-motion-fast);

  &:focus-within {
    border-color: var(--fbz-color-brand-500);
  }
}

.search-icon {
  flex: 0 0 auto;
  width: 15px;
  height: 15px;
  border-radius: 50%;
  border: 1.5px solid var(--fbz-color-text-muted);
  opacity: 0.8;
}

.search-input {
  flex: 1 1 auto;
  min-width: 0;
  height: 100%;
  border: 0;
  background: none;
  outline: none;
  color: var(--fbz-color-text);
  font-size: var(--fbz-font-size-lg);

  &::placeholder {
    color: var(--fbz-color-text-muted);
  }

  // 去掉浏览器 type=search 自带的清除按钮，保持视觉统一
  &::-webkit-search-cancel-button {
    appearance: none;
  }
}

.hint {
  color: var(--fbz-color-text-muted);
  font-size: var(--fbz-font-size-md);
}

.result-section {
  margin-bottom: var(--fbz-space-8);
}

.section-title {
  display: flex;
  align-items: center;
  gap: var(--fbz-space-2);
  margin: 0 0 var(--fbz-space-4);
  font-size: var(--fbz-font-size-lg);
  font-weight: 800;
}

.section-count {
  font-family: var(--fbz-font-display);
  font-size: var(--fbz-font-size-sm);
  font-weight: 700;
  color: var(--fbz-color-text-muted);
}

.grid {
  display: grid;
  grid-template-columns: repeat(auto-fill, minmax(150px, 1fr));
  gap: var(--fbz-space-5);
}

.entity-card {
  display: block;
  text-decoration: none;
  color: inherit;
  overflow: hidden;
  border: 1px solid var(--fbz-color-line-soft);
  border-radius: var(--fbz-radius-card);
  background: var(--fbz-color-panel);
  transition:
    border-color var(--fbz-motion-fast),
    transform var(--fbz-motion-fast);

  &:hover,
  &:focus-visible {
    border-color: var(--fbz-color-brand-500);
    transform: translateY(-4px);
  }

  :deep(.media-poster) {
    border-radius: 0;
  }
}

.entity-footer {
  padding: var(--fbz-space-3) var(--fbz-space-3) var(--fbz-space-4);
}

.entity-title {
  display: block;
  font-family: var(--fbz-font-display);
  font-size: 14px;
  font-weight: 700;
  line-height: 1.3;
  white-space: nowrap;
  overflow: hidden;
  text-overflow: ellipsis;
}

.entity-meta {
  display: block;
  margin-top: 3px;
  font-size: var(--fbz-font-size-sm);
  color: var(--fbz-color-text-muted);
}

@media (max-width: 600px) {
  .search-view {
    padding: calc(var(--header-h, 56px) + var(--fbz-space-5)) var(--fbz-space-4) 60px;
  }

  .search-bar {
    height: 44px;
    margin-bottom: var(--fbz-space-6);
  }

  .grid {
    grid-template-columns: repeat(auto-fill, minmax(108px, 1fr));
    gap: var(--fbz-space-4);
  }
}
</style>
