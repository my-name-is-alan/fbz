<script setup lang="ts">
import { SEARCH_TYPE_OPTIONS, searchHints } from "@/service/modules/search.ts";
import { useAuthStore } from "@/stores/auth.ts";
import type { SearchKind, SearchResultItem } from "@/service/modules/search.ts";

/**
 * 搜索结果页：从路由 query `?q=` 取关键词，受控输入框（回车立即跳转 / 输入 debounce 更新 URL），
 * 结果按类型分区展示（电影 / 剧集 / 人物 / 专辑 / 艺术家）。
 * 电影/剧集用 MediaCard（自带播放覆盖层与自路由）；人物/专辑/艺术家用 RouterLink 卡片按各自详情路由跳转。
 */

const route = useRoute();
const router = useRouter();
const authStore = useAuthStore();

/** 关键词的唯一事实源是路由 query；输入框只是它的受控镜像。 */
function queryFromRoute(): string {
  const q = route.query.q;
  return Array.isArray(q) ? (q[0] ?? "") : (q ?? "");
}

function typeFromRoute(): string {
  const t = route.query.type;
  const raw = Array.isArray(t) ? (t[0] ?? "") : (t ?? "");
  const known = SEARCH_TYPE_OPTIONS.some((o) => o.value === raw);
  return known ? raw : "";
}

function excludeFromRoute(): string {
  const t = route.query.excludeTags;
  return Array.isArray(t) ? (t[0] ?? "") : (t ?? "");
}

function normalizeExcludeTags(raw: string): string | undefined {
  const parts = raw
    .split(/[|,，]/)
    .map((s) => s.trim())
    .filter(Boolean);
  return parts.length ? parts.join("|") : undefined;
}

const keyword = ref(queryFromRoute());
const results = ref<SearchResultItem[]>([]);
const loading = ref(false);
const failed = ref(false);
/** 已发起过至少一次有效搜索（用于区分「未搜索引导态」与「无结果空态」）。 */
const searched = ref(false);
const inputEl = ref<HTMLInputElement | null>(null);

/** 类型筛选（透传 includeItemTypes；空 = 默认全类型）。 */
const typeFilter = ref(typeFromRoute());
/** 排除标签原始输入（`|` 或逗号分隔）；透传 excludeTags。 */
const excludeTagsInput = ref(excludeFromRoute());
const typeOptions = SEARCH_TYPE_OPTIONS;

function currentQuery(): Record<string, string> {
  const q: Record<string, string> = {};
  const term = keyword.value.trim();
  if (term) q.q = term;
  if (typeFilter.value) q.type = typeFilter.value;
  const ex = normalizeExcludeTags(excludeTagsInput.value);
  if (ex) q.excludeTags = ex;
  return q;
}

/** 当前进行中的请求控制器，切换关键词时中止上一请求。 */
let controller: AbortController | undefined;

/** 类型筛选 / 分区 → Lucide 图标。 */
const TYPE_ICONS: Record<string, string> = {
  Movie: "movie",
  Series: "tv",
  Person: "user-round",
  MusicAlbum: "album",
  MusicArtist: "artist",
};
const SECTION_ICONS: Record<string, string> = {
  movie: "movie",
  tv: "tv",
  person: "user-round",
  album: "album",
  artist: "artist",
};

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
    results.value = await searchHints(trimmed, {
      userId: authStore.userId,
      includeItemTypes: typeFilter.value || undefined,
      excludeTags: normalizeExcludeTags(excludeTagsInput.value),
      signal: controller.signal,
    });
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

/** 输入防抖：回写 URL（q/type/excludeTags），真正的搜索由 route.query watch 驱动。 */
const pushQuery = debounce(() => {
  const next = currentQuery();
  const cur = {
    q: queryFromRoute() || undefined,
    type: typeFromRoute() || undefined,
    excludeTags: excludeFromRoute() || undefined,
  };
  const same =
    (next.q || undefined) === cur.q &&
    (next.type || undefined) === cur.type &&
    (next.excludeTags || undefined) === cur.excludeTags;
  if (same) return;
  void router.replace({ name: "search", query: next });
}, 320);

function onInput() {
  pushQuery();
}

function onFiltersChange() {
  pushQuery.cancel();
  void router.replace({ name: "search", query: currentQuery() });
}

/** 回车立即提交：取消防抖并同步 URL（若查询未变则直接重搜一次）。 */
function onSubmit() {
  pushQuery.cancel();
  const next = currentQuery();
  const same =
    (next.q || "") === queryFromRoute() &&
    (next.type || "") === typeFromRoute() &&
    (next.excludeTags || "") === (normalizeExcludeTags(excludeFromRoute()) || "");
  if (same) {
    void runSearch(keyword.value);
    return;
  }
  void router.replace({ name: "search", query: next });
}

// 路由 query 是事实源：关键词 / 类型 / 排阻变化统一在此触发搜索。
watch(
  () => [route.query.q, route.query.type, route.query.excludeTags],
  () => {
    keyword.value = queryFromRoute();
    typeFilter.value = typeFromRoute();
    excludeTagsInput.value = excludeFromRoute();
    void runSearch(keyword.value);
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
  <main class="c-page search-view">
    <form class="search-bar" role="search" @submit.prevent="onSubmit">
      <BaseIcon name="search" :size="18" class="search-icon" />
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
      <BaseIcon v-if="loading" name="loading" :size="16" class="c-spin busy" />
      <button
        v-else-if="keyword"
        class="clear"
        type="button"
        aria-label="清空"
        @click="
          keyword = '';
          onSubmit();
        "
      >
        <BaseIcon name="close" :size="15" />
      </button>
    </form>

    <div class="search-filters">
      <div class="seg" role="group" aria-label="按类型筛选">
        <button
          v-for="opt in typeOptions"
          :key="opt.value"
          type="button"
          :class="{ on: typeFilter === opt.value }"
          @click="
            typeFilter = opt.value;
            onFiltersChange();
          "
        >
          <BaseIcon v-if="TYPE_ICONS[opt.value]" :name="TYPE_ICONS[opt.value]!" :size="13" />{{
            opt.label
          }}
        </button>
      </div>
      <label class="exclude-field">
        <BaseIcon name="eye-off" :size="14" />
        <span class="exclude-label">排除标签</span>
        <input
          v-model="excludeTagsInput"
          class="exclude-input"
          type="text"
          name="excludeTags"
          placeholder="可选，| 分隔"
          autocomplete="off"
          aria-label="排除标签"
          @change="onFiltersChange"
          @keydown.enter.prevent="onFiltersChange"
        />
      </label>
    </div>

    <p v-if="loading && !results.length" class="c-state">
      <BaseIcon name="loading" :size="16" class="c-spin" />搜索中…
    </p>
    <p v-else-if="failed" class="c-state is-error">
      <BaseIcon name="error" :size="16" />搜索失败，请检查网络或登录状态后重试。
    </p>
    <BaseEmptyState
      v-else-if="!keyword.trim()"
      compact
      icon="search"
      title="搜索媒体库"
      description="输入关键词以搜索影片、剧集、人物、专辑与艺术家；可按类型筛选、排除标签。"
    />
    <BaseEmptyState
      v-else-if="searched && !groups.length"
      compact
      icon="search-x"
      :title="`没有找到与「${keyword.trim()}」相关的内容`"
      description="换个关键词，或放宽类型 / 排除标签条件再试。"
    />

    <section v-for="group in groups" :key="group.kind" class="result-section">
      <h2 class="c-section-title">
        <BaseIcon :name="SECTION_ICONS[group.kind] ?? 'file'" :size="16" class="lead" />{{
          group.title
        }}
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
          <RouterLink v-else :to="item.to" class="entity-card" :class="`kind-${item.kind}`">
            <div class="entity-cover">
              <MediaPoster
                :src="item.poster"
                :title="item.title"
                :ratio="item.kind === 'album' ? 'square' : 'poster'"
                :icon="SECTION_ICONS[item.kind]"
                :variant="(i % 2) as 0 | 1"
              />
            </div>
            <div class="entity-footer">
              <span class="entity-title" :title="item.title">{{ item.title }}</span>
              <span v-if="item.meta" class="c-meta">{{ item.meta }}</span>
            </div>
          </RouterLink>
        </template>
      </div>
    </section>
  </main>
</template>

<style scoped lang="scss">
.search-view {
  display: flex;
  flex-direction: column;
  gap: 18px;
}

.search-bar {
  display: flex;
  align-items: center;
  gap: 10px;
  height: 52px;
  padding: 0 16px;
  border: 1px solid var(--ui-border);
  border-radius: var(--ui-radius-md);
  background: var(--ui-input);
  transition:
    border-color var(--ui-duration-fast),
    box-shadow var(--ui-duration-fast);

  &:focus-within {
    border-color: rgb(var(--ui-accent-channel) / 60%);
    box-shadow: 0 0 0 3px rgb(var(--ui-accent-channel) / 12%);
  }

  .search-icon,
  .busy {
    color: var(--ui-text-subtle);
  }
}

.search-input {
  flex: 1;
  min-width: 0;
  height: 100%;
  border: 0;
  outline: none;
  background: transparent;
  color: var(--ui-text);
  font-size: 16px;

  &::placeholder {
    color: var(--ui-text-subtle);
  }

  &::-webkit-search-cancel-button {
    appearance: none;
  }
}

.clear {
  width: 28px;
  height: 28px;
  display: grid;
  place-items: center;
  border: 0;
  border-radius: var(--ui-radius-sm);
  background: transparent;
  color: var(--ui-text-subtle);
  cursor: pointer;

  &:hover {
    background: var(--ui-surface-hover);
    color: var(--ui-text);
  }
}

.search-filters {
  display: flex;
  flex-wrap: wrap;
  align-items: center;
  gap: 10px;
}

.seg {
  display: inline-flex;
  flex-wrap: wrap;
  padding: 2px;
  gap: 2px;
  border: 1px solid var(--ui-border);
  border-radius: 9px;
  background: var(--ui-surface);

  button {
    display: inline-flex;
    align-items: center;
    gap: 4px;
    height: 28px;
    padding: 0 10px;
    border: 0;
    border-radius: 6px;
    background: transparent;
    color: var(--ui-text-muted);
    font-size: 12.5px;
    cursor: pointer;

    &:hover {
      color: var(--ui-text);
    }

    &.on {
      color: var(--ui-text);
      background: rgb(var(--ui-accent-channel) / 14%);
      box-shadow: inset 0 0 0 1px rgb(var(--ui-accent-channel) / 30%);
    }
  }
}

.exclude-field {
  display: inline-flex;
  align-items: center;
  gap: 6px;
  height: 34px;
  padding: 0 10px;
  border: 1px solid var(--ui-border);
  border-radius: var(--ui-radius-sm);
  background: var(--ui-input);
  color: var(--ui-text-subtle);
}

.exclude-label {
  font-size: 12px;
  white-space: nowrap;
}

.exclude-input {
  width: 160px;
  border: 0;
  outline: none;
  background: transparent;
  color: var(--ui-text);
  font-size: 12.5px;
}

.result-section {
  display: flex;
  flex-direction: column;
  gap: 12px;

  .lead {
    color: var(--ui-text-subtle);
  }
}

.section-count {
  color: var(--ui-text-subtle);
  font-size: 12px;
  font-weight: 500;
}

.grid {
  display: grid;
  grid-template-columns: repeat(auto-fill, minmax(150px, 1fr));
  gap: 20px 16px;
}

.entity-card {
  min-width: 0;
  color: inherit;
  text-decoration: none;

  &:hover {
    .entity-cover {
      border-color: var(--ui-border-strong);
      transform: translateY(-2px);
    }

    .entity-title {
      color: var(--ui-accent-hover);
    }
  }

  &.kind-person .entity-cover,
  &.kind-artist .entity-cover {
    border-radius: 50%;
    aspect-ratio: 1;

    :deep(.media-poster) {
      height: 100%;
    }
  }
}

.entity-cover {
  overflow: hidden;
  border: 1px solid var(--ui-border);
  border-radius: var(--ui-radius-md);
  background: var(--ui-surface-raised);
  transition:
    transform var(--ui-duration-base) var(--ui-ease-out),
    border-color var(--ui-duration-fast);
}

.entity-footer {
  display: flex;
  flex-direction: column;
  gap: 2px;
  padding: 8px 2px 0;
  text-align: center;
}

.entity-title {
  overflow: hidden;
  color: var(--ui-text);
  font-size: 13.5px;
  font-weight: 600;
  white-space: nowrap;
  text-overflow: ellipsis;
  transition: color var(--ui-duration-fast);
}

@media (max-width: 600px) {
  .grid {
    grid-template-columns: repeat(auto-fill, minmax(104px, 1fr));
  }

  .exclude-input {
    width: 110px;
  }
}
</style>
