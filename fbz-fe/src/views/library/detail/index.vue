<script setup lang="ts">
import type { MediaItem } from "@/types/media.ts";
import {
  fetchLibraryFilters,
  fetchLibraryItems,
  LIBRARY_PAGE_SIZE,
} from "@/service/modules/library.ts";
import { queueLibraryScan } from "@/service/modules/admin.ts";
import { useLibraryStore } from "@/stores/library.ts";
import { useAuthStore } from "@/stores/auth.ts";
import { useUiStore } from "@/stores/ui.ts";
import { libraryTypeMeta } from "@/utils/libraryType.ts";

/**
 * 单库浏览：Emby Items（ParentId + 按库类型的 IncludeItemTypes）+ Filters 选项。
 * 筛选 / 排序 / 分页全部透传服务端，并与 URL query 双向同步（可分享、可回退）。
 */
const route = useRoute();
const router = useRouter();
const libraryStore = useLibraryStore();
const authStore = useAuthStore();
const uiStore = useUiStore();

const libraryId = computed(() => String(route.params.id));
const library = computed(() => libraryStore.getById(libraryId.value));
const typeMeta = computed(() => libraryTypeMeta(library.value?.libraryType));
/** 库类型决定浏览的条目类型（movies→Movie、tvshows→Series、homevideos→Video,Photo…）。 */
const includeItemTypes = computed(() => typeMeta.value.itemTypes);

const items = ref<MediaItem[]>([]);
const total = ref(0);
const startIndex = ref(0);
const loading = ref(false);
const error = ref<string | null>(null);
const scanning = ref(false);

const serverGenres = ref<string[]>([]);
const serverTags = ref<string[]>([]);
const serverYears = ref<number[]>([]);

/* ---------- 筛选 / 排序状态（URL query 为事实源） ---------- */
type SortKey = "SortName" | "ProductionYear" | "CommunityRating" | "DateCreated";
type PlayedFilter = "" | "played" | "unplayed";

interface BrowseState {
  genre: string;
  year: string;
  tag: string;
  exclude: string;
  played: PlayedFilter;
  fav: boolean;
  sort: SortKey;
  desc: boolean;
  page: number;
}

const SORTS: { key: SortKey; label: string }[] = [
  { key: "SortName", label: "名称" },
  { key: "DateCreated", label: "入库" },
  { key: "ProductionYear", label: "年份" },
  { key: "CommunityRating", label: "评分" },
];

function q(name: string): string {
  const v = route.query[name];
  return (Array.isArray(v) ? v[0] : v) ?? "";
}

function stateFromRoute(): BrowseState {
  const sort = q("sort") as SortKey;
  const played = q("played") as PlayedFilter;
  const page = Number(q("page"));
  return {
    genre: q("genre"),
    year: q("year"),
    tag: q("tag"),
    exclude: q("exclude"),
    played: played === "played" || played === "unplayed" ? played : "",
    fav: q("fav") === "1",
    sort: SORTS.some((s) => s.key === sort) ? sort : "SortName",
    desc: q("desc") === "1",
    page: Number.isFinite(page) && page > 1 ? Math.floor(page) - 1 : 0,
  };
}

const state = reactive<BrowseState>(stateFromRoute());

function queryFromState(): Record<string, string> {
  const out: Record<string, string> = {};
  if (state.genre) out.genre = state.genre;
  if (state.year) out.year = state.year;
  if (state.tag) out.tag = state.tag;
  if (state.exclude) out.exclude = state.exclude;
  if (state.played) out.played = state.played;
  if (state.fav) out.fav = "1";
  if (state.sort !== "SortName") out.sort = state.sort;
  if (state.desc) out.desc = "1";
  if (state.page > 0) out.page = String(state.page + 1);
  return out;
}

function pushState() {
  void router.replace({ query: queryFromState() });
}

/** 修改筛选/排序：回到第一页并同步 URL。 */
function setFilter<K extends keyof BrowseState>(key: K, value: BrowseState[K]) {
  state[key] = value;
  if (key !== "page") state.page = 0;
  pushState();
}

const activeFilters = computed(() => {
  const chips: { key: keyof BrowseState; icon: string; label: string }[] = [];
  if (state.genre) chips.push({ key: "genre", icon: "tag", label: state.genre });
  if (state.year) chips.push({ key: "year", icon: "calendar", label: state.year });
  if (state.tag) chips.push({ key: "tag", icon: "tags", label: `标签：${state.tag}` });
  if (state.exclude)
    chips.push({ key: "exclude", icon: "eye-off", label: `排除：${state.exclude}` });
  if (state.played)
    chips.push({
      key: "played",
      icon: state.played === "played" ? "check" : "eye",
      label: state.played === "played" ? "已看" : "未看",
    });
  if (state.fav) chips.push({ key: "fav", icon: "heart", label: "收藏" });
  return chips;
});

function clearFilter(key: keyof BrowseState) {
  if (key === "fav") setFilter("fav", false);
  else if (key === "played") setFilter("played", "");
  else setFilter(key, "" as never);
}

function clearFilters() {
  Object.assign(state, {
    genre: "",
    year: "",
    tag: "",
    exclude: "",
    played: "",
    fav: false,
    page: 0,
  });
  pushState();
}

const genreOptions = computed(() => [
  { label: "全部题材", value: "" },
  ...[...new Set([...serverGenres.value, ...items.value.map((i) => i.genre).filter(Boolean)])]
    .sort((a, b) => String(a).localeCompare(String(b), "zh"))
    .map((g) => ({ label: String(g), value: String(g) })),
]);

const yearOptions = computed(() => [
  { label: "全部年份", value: "" },
  ...[
    ...new Set([...serverYears.value, ...items.value.map((i) => i.year).filter((y) => y != null)]),
  ]
    .sort((a, b) => Number(b) - Number(a))
    .map((y) => ({ label: String(y), value: String(y) })),
]);

const tagOptions = computed(() => [
  { label: "全部标签", value: "" },
  ...serverTags.value.map((t) => ({ label: t, value: t })),
]);

const excludeOptions = computed(() => [
  { label: "不排除标签", value: "" },
  ...serverTags.value.map((t) => ({ label: `排除：${t}`, value: t })),
]);

const totalPages = computed(() => Math.max(1, Math.ceil(total.value / LIBRARY_PAGE_SIZE)));
const rangeLabel = computed(() => {
  if (!items.value.length) return "0";
  return `${startIndex.value + 1}–${startIndex.value + items.value.length}`;
});
const pageNumbers = computed(() => {
  const count = totalPages.value;
  const cur = state.page;
  const set = new Set([0, count - 1, cur - 1, cur, cur + 1].filter((p) => p >= 0 && p < count));
  const sorted = [...set].sort((a, b) => a - b);
  const out: (number | "gap")[] = [];
  sorted.forEach((p, i) => {
    if (i > 0 && p - sorted[i - 1]! > 1) out.push("gap");
    out.push(p);
  });
  return out;
});

function filtersParam(): string | undefined {
  const parts: string[] = [];
  if (state.played === "played") parts.push("IsPlayed");
  if (state.played === "unplayed") parts.push("IsUnplayed");
  if (state.fav) parts.push("IsFavorite");
  return parts.length ? parts.join(",") : undefined;
}

let loadSerial = 0;

async function loadItems() {
  const serial = ++loadSerial;
  if (!authStore.userId) {
    items.value = [];
    total.value = 0;
    error.value = "请先登录后再浏览媒体库。";
    return;
  }
  loading.value = true;
  error.value = null;
  try {
    const result = await fetchLibraryItems({
      userId: authStore.userId,
      libraryId: libraryId.value,
      startIndex: state.page * LIBRARY_PAGE_SIZE,
      limit: LIBRARY_PAGE_SIZE,
      sortBy: state.sort,
      sortOrder: state.desc ? "Descending" : "Ascending",
      includeItemTypes: includeItemTypes.value,
      genres: state.genre || undefined,
      years: state.year || undefined,
      tags: state.tag || undefined,
      excludeTags: state.exclude || undefined,
      filters: filtersParam(),
      isPlayed: state.played === "played" ? true : state.played === "unplayed" ? false : undefined,
      isFavorite: state.fav ? true : undefined,
    });
    if (serial !== loadSerial) return;
    items.value = result.items;
    total.value = result.total;
    startIndex.value = result.startIndex;
  } catch {
    if (serial !== loadSerial) return;
    items.value = [];
    total.value = 0;
    startIndex.value = 0;
    error.value = "媒体库条目加载失败，请检查网络、登录状态或库权限。";
  } finally {
    if (serial === loadSerial) loading.value = false;
  }
}

async function loadFilterOptions() {
  if (!authStore.userId) return;
  try {
    const filters = await fetchLibraryFilters(
      authStore.userId,
      libraryId.value,
      includeItemTypes.value,
    );
    serverGenres.value = filters.genres;
    serverTags.value = filters.tags;
    serverYears.value = filters.years;
  } catch {
    serverGenres.value = [];
    serverTags.value = [];
    serverYears.value = [];
  }
}

function goPage(next: number) {
  setFilter("page", Math.max(0, Math.min(next, totalPages.value - 1)));
  window.scrollTo({ top: 0, behavior: "smooth" });
}

async function triggerScan() {
  if (scanning.value) return;
  scanning.value = true;
  try {
    await queueLibraryScan(libraryId.value, "library-detail-empty");
    uiStore.showToast("已将该媒体库加入扫描队列，稍后刷新查看。", "success");
  } catch {
    uiStore.showToast("扫描入队失败，请确认当前账号具备管理权限。", "error");
  } finally {
    scanning.value = false;
  }
}

function openLibraryMenu(e: MouseEvent) {
  if (!authStore.isAdmin || !library.value) return;
  const rect = (e.currentTarget as HTMLElement).getBoundingClientRect();
  uiStore.openLibraryContextMenu(rect.left, rect.bottom + 6, {
    id: library.value.id,
    name: library.value.name,
  });
}

// 库切换：重拉筛选项（库类型可能不同）。
watch(
  [libraryId, includeItemTypes],
  () => {
    if (route.name !== "library-detail") return;
    void loadFilterOptions();
  },
  { immediate: true },
);

// URL query 变化（含前进/后退）→ 同步状态并重拉。
watch(
  () => [libraryId.value, includeItemTypes.value, route.query] as const,
  () => {
    // 离开本页时 route 先变：不要拿新路由的参数去拉库条目。
    if (route.name !== "library-detail") return;
    Object.assign(state, stateFromRoute());
    void loadItems();
  },
  { immediate: true, deep: true },
);

onMounted(() => {
  if (!libraryStore.loaded) void libraryStore.loadFromBackend();
});

/** 播放器进度回传后刷新进度条 / 已看状态（与 PlayedItems / Sessions 联动）。 */
useEventListener(window, "fbz-playback-saved", () => void loadItems());

useEventListener(window, "fbz:item-deleted", ((event: CustomEvent<{ id: string }>) => {
  const id = event.detail?.id;
  if (!id) return;
  const before = items.value.length;
  items.value = items.value.filter((item) => item.id !== id);
  if (items.value.length < before) total.value = Math.max(0, total.value - 1);
}) as EventListener);

/* ---------- 分组（电梯导航按当前排序维度分段） ---------- */
function sectionLabel(item: MediaItem): string {
  switch (state.sort) {
    case "ProductionYear":
      return String(item.year ?? "未知");
    case "CommunityRating":
      return item.rating ? `${Math.floor(item.rating)} 分` : "未评分";
    case "DateCreated":
      return "";
    default:
      return item.title.charAt(0).toUpperCase();
  }
}

const sections = computed(() => {
  const map = new Map<string, MediaItem[]>();
  for (const item of items.value) {
    const label = sectionLabel(item);
    if (!map.has(label)) map.set(label, []);
    map.get(label)!.push(item);
  }
  return [...map].map(([label, list], i) => ({ label, id: `sec-${i}`, items: list }));
});

/** 段首条目 id → 锚点 id（电梯导航滚动目标）。 */
const anchors = computed(() => new Map(sections.value.map((sec) => [sec.items[0]!.id, sec.id])));

const activeSection = ref("");
let observer: IntersectionObserver | undefined;

function scrollToSection(id: string) {
  document.getElementById(id)?.scrollIntoView({ behavior: "smooth", block: "start" });
}

function observeSections() {
  observer?.disconnect();
  if (typeof IntersectionObserver === "undefined") return;
  observer = new IntersectionObserver(
    (entries) => {
      for (const entry of entries) if (entry.isIntersecting) activeSection.value = entry.target.id;
    },
    { rootMargin: "-30% 0px -60% 0px" },
  );
  for (const sec of sections.value) {
    const el = document.getElementById(sec.id);
    if (el) observer.observe(el);
  }
}

watch(sections, () => nextTick(observeSections));
onBeforeUnmount(() => observer?.disconnect());
</script>

<template>
  <main class="c-page library-view">
    <header class="page-head">
      <nav class="crumbs" aria-label="面包屑">
        <RouterLink to="/library"><BaseIcon name="library" :size="14" />媒体库</RouterLink>
        <BaseIcon name="right" :size="13" class="sep" />
        <span>{{ library?.name ?? "媒体库" }}</span>
      </nav>
      <div class="title-row">
        <span class="type-ic"><BaseIcon :name="typeMeta.icon" :size="20" /></span>
        <h1 class="c-page-title">{{ library?.name ?? "未知媒体库" }}</h1>
        <span class="type-label">{{ typeMeta.label }}</span>
        <span class="c-meta count">{{ rangeLabel }} / {{ total }}</span>
        <button
          v-if="authStore.isAdmin && library"
          class="du-btn du-btn-ghost du-btn-sm du-btn-square more"
          type="button"
          aria-label="媒体库操作"
          @click.stop="openLibraryMenu"
        >
          <BaseIcon name="more" :size="18" />
        </button>
      </div>
    </header>

    <div class="toolbar">
      <div class="filters">
        <BaseIcon name="funnel" :size="15" class="tb-ic" :class="{ on: activeFilters.length }" />
        <BaseSelect
          :model-value="state.genre"
          :options="genreOptions"
          size="sm"
          aria-label="按题材筛选"
          @update:model-value="(v) => setFilter('genre', String(v))"
        />
        <BaseSelect
          :model-value="state.year"
          :options="yearOptions"
          size="sm"
          aria-label="按年份筛选"
          @update:model-value="(v) => setFilter('year', String(v))"
        />
        <BaseSelect
          v-if="tagOptions.length > 1"
          :model-value="state.tag"
          :options="tagOptions"
          size="sm"
          aria-label="按标签筛选"
          @update:model-value="(v) => setFilter('tag', String(v))"
        />
        <BaseSelect
          v-if="excludeOptions.length > 1"
          :model-value="state.exclude"
          :options="excludeOptions"
          size="sm"
          aria-label="排除标签"
          @update:model-value="(v) => setFilter('exclude', String(v))"
        />
        <div class="seg" role="group" aria-label="观看状态">
          <button type="button" :class="{ on: !state.played }" @click="setFilter('played', '')">
            全部
          </button>
          <button
            type="button"
            :class="{ on: state.played === 'unplayed' }"
            @click="setFilter('played', 'unplayed')"
          >
            <BaseIcon name="eye" :size="13" />未看
          </button>
          <button
            type="button"
            :class="{ on: state.played === 'played' }"
            @click="setFilter('played', 'played')"
          >
            <BaseIcon name="check" :size="13" />已看
          </button>
        </div>
        <button
          type="button"
          class="toggle"
          :class="{ on: state.fav }"
          :aria-pressed="state.fav"
          @click="setFilter('fav', !state.fav)"
        >
          <BaseIcon name="heart" :size="13" :fill="state.fav" />收藏
        </button>
      </div>

      <div class="sort">
        <div class="seg" role="group" aria-label="排序">
          <button
            v-for="opt in SORTS"
            :key="opt.key"
            type="button"
            :class="{ on: state.sort === opt.key }"
            @click="setFilter('sort', opt.key)"
          >
            {{ opt.label }}
          </button>
        </div>
        <button
          class="toggle icon"
          type="button"
          :title="state.desc ? '降序' : '升序'"
          :aria-label="state.desc ? '当前降序，切换为升序' : '当前升序，切换为降序'"
          @click="setFilter('desc', !state.desc)"
        >
          <BaseIcon :name="state.desc ? 'sort-desc' : 'sort-asc'" :size="15" />
        </button>
      </div>
    </div>

    <div v-if="activeFilters.length" class="active-filters">
      <span v-for="chip in activeFilters" :key="chip.key" class="af-chip">
        <BaseIcon :name="chip.icon" :size="12" />{{ chip.label }}
        <button type="button" :aria-label="`移除筛选 ${chip.label}`" @click="clearFilter(chip.key)">
          <BaseIcon name="close" :size="12" />
        </button>
      </span>
      <button class="af-clear" type="button" @click="clearFilters">
        <BaseIcon name="funnel-x" :size="13" />清除全部
      </button>
    </div>

    <p v-if="loading && !items.length" class="c-state">
      <BaseIcon name="loading" :size="16" class="c-spin" />正在加载媒体库条目…
    </p>
    <p v-else-if="error" class="c-state is-error">
      <BaseIcon name="error" :size="16" />{{ error }}
    </p>

    <template v-else>
      <BaseEmptyState
        v-if="!items.length && activeFilters.length"
        compact
        icon="search-x"
        title="没有符合条件的条目"
        description="当前筛选条件下没有匹配的内容，换个条件或清除筛选再试。"
      >
        <button class="du-btn du-btn-sm du-btn-ghost" type="button" @click="clearFilters">
          <BaseIcon name="funnel-x" :size="15" />清除筛选
        </button>
      </BaseEmptyState>
      <BaseEmptyState
        v-else-if="!items.length"
        :icon="typeMeta.icon"
        title="该媒体库暂无条目"
        description="库里还没有扫描出媒体文件。确认库路径下有文件后，触发一次扫描即可。"
      >
        <button
          v-if="authStore.isAdmin"
          class="du-btn du-btn-primary"
          type="button"
          :disabled="scanning"
          @click="triggerScan"
        >
          <BaseIcon
            :name="scanning ? 'loading' : 'scan'"
            :size="16"
            :class="{ 'c-spin': scanning }"
          />
          {{ scanning ? "正在入队…" : "扫描媒体库" }}
        </button>
      </BaseEmptyState>

      <div v-else class="body" :class="{ dim: loading }">
        <!-- 平铺网格；电梯导航锚定到每段第一张卡（不再每段单独起一行） -->
        <div class="grid">
          <div v-for="(item, i) in items" :id="anchors.get(item.id)" :key="item.id" class="cell">
            <MediaCard :item="item" layout="poster" :variant="(i % 2) as 0 | 1" />
          </div>
        </div>

        <nav
          v-if="sections.length > 1 && sections[0]?.label"
          class="elevator"
          aria-label="快速定位"
        >
          <button
            v-for="sec in sections"
            :key="sec.id"
            class="ev-item"
            :class="{ active: activeSection === sec.id }"
            type="button"
            @click="scrollToSection(sec.id)"
          >
            {{ sec.label }}
          </button>
        </nav>
      </div>

      <nav v-if="totalPages > 1" class="pagination" aria-label="分页">
        <button
          class="pg"
          type="button"
          :disabled="state.page === 0 || loading"
          aria-label="上一页"
          @click="goPage(state.page - 1)"
        >
          <BaseIcon name="left" :size="15" />
        </button>
        <template v-for="(p, i) in pageNumbers" :key="`${p}-${i}`">
          <span v-if="p === 'gap'" class="gap">…</span>
          <button
            v-else
            class="pg"
            type="button"
            :class="{ on: p === state.page }"
            :aria-current="p === state.page ? 'page' : undefined"
            @click="goPage(p)"
          >
            {{ p + 1 }}
          </button>
        </template>
        <button
          class="pg"
          type="button"
          :disabled="state.page >= totalPages - 1 || loading"
          aria-label="下一页"
          @click="goPage(state.page + 1)"
        >
          <BaseIcon name="right" :size="15" />
        </button>
        <span class="c-meta">每页 {{ LIBRARY_PAGE_SIZE }}</span>
      </nav>
    </template>
  </main>
</template>

<style scoped lang="scss">
.library-view {
  display: flex;
  flex-direction: column;
  gap: 14px;
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

  .sep {
    opacity: 0.6;
  }
}

.title-row {
  display: flex;
  align-items: center;
  gap: 10px;
  flex-wrap: wrap;
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

.type-label {
  height: 22px;
  padding: 0 8px;
  display: inline-flex;
  align-items: center;
  border: 1px solid var(--ui-border-strong);
  border-radius: 5px;
  color: var(--ui-text-muted);
  font-size: 11.5px;
  font-weight: 600;
}

.count {
  margin-left: 4px;
}

.more {
  margin-left: auto;
}

.toolbar {
  position: sticky;
  top: var(--ui-header-height);
  z-index: 20;
  display: flex;
  flex-wrap: wrap;
  align-items: center;
  justify-content: space-between;
  gap: 10px;
  margin: 0 calc(var(--ui-shell-gutter) * -1);
  padding: 10px var(--ui-shell-gutter);
  border-bottom: 1px solid var(--ui-border);
  background: var(--ui-glass-strong);
  backdrop-filter: blur(14px);
}

.filters,
.sort {
  display: flex;
  flex-wrap: wrap;
  align-items: center;
  gap: 6px;

  :deep(.base-select) {
    width: auto;
    min-width: 110px;
    max-width: 180px;
  }
}

.tb-ic {
  color: var(--ui-text-subtle);

  &.on {
    color: var(--ui-accent);
  }
}

.seg {
  display: inline-flex;
  padding: 2px;
  gap: 2px;
  border: 1px solid var(--ui-border);
  border-radius: 9px;
  background: var(--ui-surface);

  button {
    display: inline-flex;
    align-items: center;
    gap: 4px;
    height: 26px;
    padding: 0 9px;
    border: 0;
    border-radius: 6px;
    background: transparent;
    color: var(--ui-text-muted);
    font-size: 12.5px;
    font-weight: 500;
    cursor: pointer;

    &:hover {
      color: var(--ui-text);
      background: var(--ui-surface-hover);
    }

    &.on {
      color: var(--ui-text);
      background: rgb(var(--ui-accent-channel) / 14%);
      box-shadow: inset 0 0 0 1px rgb(var(--ui-accent-channel) / 30%);
    }
  }
}

.toggle {
  display: inline-flex;
  align-items: center;
  gap: 4px;
  height: 32px;
  padding: 0 10px;
  border: 1px solid var(--ui-border);
  border-radius: var(--ui-radius-sm);
  background: var(--ui-surface);
  color: var(--ui-text-muted);
  font-size: 12.5px;
  cursor: pointer;

  &.icon {
    width: 32px;
    padding: 0;
    justify-content: center;
  }

  &:hover {
    color: var(--ui-text);
    border-color: var(--ui-border-strong);
  }

  &.on {
    color: #fb7185;
    border-color: rgb(251 113 133 / 45%);
    background: rgb(251 113 133 / 8%);
  }
}

.active-filters {
  display: flex;
  flex-wrap: wrap;
  align-items: center;
  gap: 6px;
}

.af-chip {
  display: inline-flex;
  align-items: center;
  gap: 5px;
  height: 26px;
  padding: 0 4px 0 9px;
  border: 1px solid rgb(var(--ui-accent-channel) / 45%);
  border-radius: 6px;
  background: rgb(var(--ui-accent-channel) / 10%);
  color: var(--ui-accent-hover);
  font-size: 12px;

  button {
    width: 18px;
    height: 18px;
    display: grid;
    place-items: center;
    border: 0;
    border-radius: 4px;
    background: transparent;
    color: inherit;
    cursor: pointer;

    &:hover {
      background: rgb(var(--ui-accent-channel) / 20%);
    }
  }
}

.af-clear {
  display: inline-flex;
  align-items: center;
  gap: 4px;
  height: 26px;
  padding: 0 8px;
  border: 0;
  border-radius: 6px;
  background: transparent;
  color: var(--ui-text-muted);
  font-size: 12px;
  cursor: pointer;

  &:hover {
    color: var(--ui-text);
    background: var(--ui-surface-hover);
  }
}

.body {
  display: flex;
  gap: 16px;
  align-items: flex-start;
  transition: opacity var(--ui-duration-fast);

  &.dim {
    opacity: 0.55;
  }
}

.cell {
  min-width: 0;
  scroll-margin-top: calc(var(--ui-header-height) + 72px);
}

.grid {
  flex: 1;
  min-width: 0;
  display: grid;
  grid-template-columns: repeat(auto-fill, minmax(150px, 1fr));
  gap: 20px 16px;
}

.elevator {
  position: sticky;
  top: calc(var(--ui-header-height) + 72px);
  display: flex;
  flex-direction: column;
  gap: 1px;
  max-height: calc(100vh - var(--ui-header-height) - 110px);
  overflow-y: auto;
  padding-left: 8px;
  border-left: 1px solid var(--ui-border);
}

.ev-item {
  min-width: 36px;
  padding: 3px 6px;
  border: 0;
  border-radius: 4px;
  background: transparent;
  color: var(--ui-text-subtle);
  font-size: 11px;
  text-align: right;
  cursor: pointer;

  &:hover {
    color: var(--ui-text);
  }

  &.active {
    color: var(--ui-accent);
    font-weight: 700;
  }
}

.pagination {
  display: flex;
  flex-wrap: wrap;
  align-items: center;
  justify-content: center;
  gap: 6px;
  padding-top: 16px;
  border-top: 1px solid var(--ui-border);

  .gap {
    color: var(--ui-text-subtle);
  }

  .c-meta {
    margin-left: 8px;
  }
}

.pg {
  min-width: 32px;
  height: 32px;
  display: inline-grid;
  place-items: center;
  padding: 0 8px;
  border: 1px solid var(--ui-border);
  border-radius: var(--ui-radius-sm);
  background: var(--ui-surface);
  color: var(--ui-text-muted);
  font-size: 12.5px;
  font-variant-numeric: tabular-nums;
  cursor: pointer;

  &:hover:not(:disabled) {
    color: var(--ui-text);
    border-color: var(--ui-border-strong);
  }

  &.on {
    border-color: var(--ui-accent);
    background: var(--ui-accent);
    color: var(--ui-accent-contrast);
    font-weight: 700;
  }

  &:disabled {
    opacity: 0.4;
    cursor: default;
  }
}

@media (max-width: 600px) {
  .grid {
    grid-template-columns: repeat(auto-fill, minmax(104px, 1fr));
    gap: 16px 10px;
  }

  .elevator {
    display: none;
  }

  .filters :deep(.base-select) {
    min-width: 96px;
  }
}
</style>
