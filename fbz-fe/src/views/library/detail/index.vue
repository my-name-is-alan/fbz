<script setup lang="ts">
import type { MediaItem, SortKey, SortOption } from "@/types/media.ts";
import { genrePool, libraryItems } from "@/service/modules/media.ts";
import { itemsByLibrary } from "@/service/modules/tmdb.ts";
import { useLibraryStore } from "@/stores/library.ts";

const route = useRoute();
const libraryStore = useLibraryStore();

const libraryId = computed(() => String(route.params.id));
const library = computed(() => libraryStore.getById(libraryId.value));

// 影视类库用 TMDB 真实数据，音乐等无 TMDB 数据的库用占位生成数据
const allItems = computed<MediaItem[]>(() => {
  const real = itemsByLibrary(libraryId.value);
  return real.length ? real : libraryItems(libraryId.value);
});

/* ---------- 筛选 ---------- */
const genreFilter = ref<string>("");

function clearFilters() {
  genreFilter.value = "";
}

// 题材选项从当前库的真实条目动态汇总；无则回退到占位题材池。含「全部题材」项（值为空）
const genreOptions = computed(() => {
  const set = new Set<string>();
  for (const it of allItems.value) if (it.genre) set.add(it.genre);
  const genres = set.size ? [...set].sort((a, b) => a.localeCompare(b, "zh")) : genrePool;
  return [{ label: "全部题材", value: "" }, ...genres.map((g) => ({ label: g, value: g }))];
});

const filtered = computed(() =>
  allItems.value.filter((it) => !genreFilter.value || it.genre === genreFilter.value),
);

/* ---------- 排序 ---------- */
const sortOptions: SortOption[] = [
  { key: "rating", label: "评分" },
  { key: "title", label: "名称" },
  { key: "year", label: "年份" },
];
const sortKey = ref<SortKey>("rating");
const sortDesc = ref(true);

function toggleDir() {
  sortDesc.value = !sortDesc.value;
}

const sorted = computed(() => {
  const list = [...filtered.value];
  list.sort((a, b) => {
    let r = 0;
    switch (sortKey.value) {
      case "title":
        r = a.title.localeCompare(b.title, "zh");
        break;
      case "year":
        r = (a.year ?? 0) - (b.year ?? 0);
        break;
      case "rating":
        r = (a.rating ?? 0) - (b.rating ?? 0);
        break;
    }
    return sortDesc.value ? -r : r;
  });
  return list;
});

/* ---------- 分组（电梯导航按当前排序维度分段） ---------- */
function sectionLabel(item: MediaItem): string {
  switch (sortKey.value) {
    case "year":
      return String(item.year ?? "未知");
    case "rating":
      return `${Math.floor(item.rating ?? 0)} 分`;
    case "title":
    default:
      return item.title.charAt(0).toUpperCase();
  }
}

interface Section {
  label: string;
  id: string;
  items: MediaItem[];
}

const sections = computed<Section[]>(() => {
  const map = new Map<string, MediaItem[]>();
  for (const item of sorted.value) {
    const label = sectionLabel(item);
    if (!map.has(label)) map.set(label, []);
    map.get(label)!.push(item);
  }
  return [...map].map(([label, items], i) => ({
    label,
    id: `sec-${i}`,
    items,
  }));
});

/* ---------- 电梯导航：滚动定位 + 当前高亮 ---------- */
const activeSection = ref<string>("");

function scrollToSection(id: string) {
  document.getElementById(id)?.scrollIntoView({ behavior: "smooth", block: "start" });
}

let observer: IntersectionObserver | undefined;

function observeSections() {
  observer?.disconnect();
  observer = new IntersectionObserver(
    (entries) => {
      for (const entry of entries) {
        if (entry.isIntersecting) activeSection.value = entry.target.id;
      }
    },
    { rootMargin: "-30% 0px -60% 0px" },
  );
  for (const sec of sections.value) {
    const el = document.getElementById(sec.id);
    if (el) observer.observe(el);
  }
}

onMounted(() => nextTick(observeSections));
watch(sections, () => nextTick(observeSections));
onBeforeUnmount(() => observer?.disconnect());
</script>

<template>
  <main class="library-view">
    <header class="page-head">
      <RouterLink to="/library" class="back">← 媒体库</RouterLink>
      <div class="title-row">
        <h1>{{ library?.name ?? "未知媒体库" }}</h1>
        <span class="result-count">{{ sorted.length }} / {{ allItems.length }}</span>
      </div>
    </header>

    <!-- 工具条：筛选 + 排序 -->
    <div class="toolbar">
      <div class="filters">
        <BaseSelect v-model="genreFilter" :options="genreOptions" aria-label="按题材筛选" />
        <button v-if="genreFilter" class="clear" type="button" @click="clearFilters">
          清除筛选
        </button>
      </div>

      <div class="sort">
        <button
          v-for="opt in sortOptions"
          :key="opt.key"
          class="sort-btn"
          :class="{ active: sortKey === opt.key }"
          type="button"
          @click="sortKey = opt.key"
        >
          {{ opt.label }}
        </button>
        <button
          class="dir-btn"
          type="button"
          :title="sortDesc ? '降序' : '升序'"
          @click="toggleDir"
        >
          {{ sortDesc ? "↓" : "↑" }}
        </button>
      </div>
    </div>

    <div class="body">
      <div class="sections">
        <section v-for="sec in sections" :id="sec.id" :key="sec.id" class="group">
          <h2 class="group-label">{{ sec.label }}</h2>
          <div class="grid">
            <MediaCard
              v-for="(item, i) in sec.items"
              :key="item.id"
              :item="item"
              layout="poster"
              :variant="(i % 2) as 0 | 1"
            />
          </div>
        </section>

        <p v-if="!sorted.length" class="empty">没有符合条件的条目</p>
      </div>

      <!-- 电梯导航 -->
      <nav v-if="sections.length > 1" class="elevator" aria-label="快速定位">
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
  </main>
</template>

<style scoped lang="scss">
.library-view {
  padding: calc(var(--header-h, 60px) + var(--fbz-space-8)) var(--fbz-space-8) 80px;
  min-height: 100vh;
}

.page-head {
  margin-bottom: var(--fbz-space-5);

  .back {
    display: inline-block;
    margin-bottom: var(--fbz-space-3);
    color: var(--fbz-color-text-muted);
    text-decoration: none;
    font-size: var(--fbz-font-size-md);

    &:hover {
      color: var(--fbz-color-text);
    }
  }

  .title-row {
    display: flex;
    align-items: baseline;
    gap: var(--fbz-space-3);
  }

  h1 {
    margin: 0;
    font-size: var(--fbz-font-size-xl);
    font-weight: 800;
  }

  .result-count {
    font-size: var(--fbz-font-size-sm);
    color: var(--fbz-color-text-muted);
  }
}

.toolbar {
  position: sticky;
  top: var(--header-h, 60px);
  z-index: var(--fbz-z-sticky);
  display: flex;
  flex-wrap: wrap;
  align-items: center;
  justify-content: space-between;
  gap: var(--fbz-space-3);
  padding: var(--fbz-space-3) 0;
  margin-bottom: var(--fbz-space-4);
  background: var(--fbz-color-bg);
  border-bottom: 1px solid var(--fbz-color-line-soft);
}

.filters,
.sort {
  display: flex;
  align-items: center;
  gap: var(--fbz-space-2);
  flex-wrap: wrap;
}

.clear {
  height: 34px;
  padding: 0 12px;
  border-radius: var(--fbz-radius-control);
  border: 1px solid transparent;
  background: none;
  color: var(--fbz-color-text-muted);
  font-size: var(--fbz-font-size-sm);

  &:hover {
    color: var(--fbz-color-text);
  }
}

.sort-btn {
  height: 34px;
  padding: 0 12px;
  border-radius: var(--fbz-radius-control);
  border: 1px solid transparent;
  background: none;
  color: var(--fbz-color-text-soft);
  font-size: var(--fbz-font-size-sm);
  transition:
    color var(--fbz-motion-fast),
    background var(--fbz-motion-fast);

  &:hover {
    color: #fff;
    background: rgba(255, 255, 255, 0.06);
  }

  &.active {
    color: #fff;
    background: rgba(30, 215, 96, 0.14);
    border-color: rgba(30, 215, 96, 0.4);
  }
}

.dir-btn {
  width: 34px;
  height: 34px;
  border-radius: var(--fbz-radius-control);
  border: 1px solid var(--fbz-color-line);
  background: var(--fbz-color-panel);
  color: var(--fbz-color-text);
  font-size: 14px;
}

.body {
  display: flex;
  gap: var(--fbz-space-5);
  align-items: flex-start;
}

.sections {
  flex: 1;
  min-width: 0;
}

.group {
  // 锚点定位时让出固定 header + 工具条高度
  scroll-margin-top: calc(var(--header-h, 60px) + 56px);
  margin-bottom: var(--fbz-space-6);
}

.group-label {
  margin: 0 0 var(--fbz-space-3);
  font-size: var(--fbz-font-size-md);
  font-weight: 700;
  color: var(--fbz-color-text-soft);
}

.grid {
  display: grid;
  grid-template-columns: repeat(auto-fill, minmax(132px, 1fr));
  gap: var(--fbz-space-5) var(--fbz-space-4);
}

.empty {
  padding: 60px 0;
  text-align: center;
  color: var(--fbz-color-text-muted);
}

.elevator {
  position: sticky;
  top: calc(var(--header-h, 60px) + 64px);
  flex: 0 0 auto;
  display: flex;
  flex-direction: column;
  gap: 2px;
  max-height: calc(100vh - var(--header-h, 60px) - 100px);
  overflow-y: auto;
  padding-left: var(--fbz-space-2);
  border-left: 1px solid var(--fbz-color-line-soft);
}

.ev-item {
  min-width: 40px;
  padding: 4px 8px;
  border: 0;
  background: none;
  border-radius: 4px;
  color: var(--fbz-color-text-muted);
  font-size: var(--fbz-font-size-xs);
  text-align: right;
  transition:
    color var(--fbz-motion-fast),
    background var(--fbz-motion-fast);

  &:hover {
    color: var(--fbz-color-text);
  }

  &.active {
    color: var(--fbz-color-brand-500);
    font-weight: 700;
  }
}

@media (max-width: 600px) {
  .library-view {
    padding: calc(var(--header-h, 56px) + var(--fbz-space-5)) var(--fbz-space-4) 60px;
  }

  .toolbar {
    flex-direction: column;
    align-items: stretch;
  }

  .grid {
    grid-template-columns: repeat(auto-fill, minmax(104px, 1fr));
    gap: var(--fbz-space-4) var(--fbz-space-3);
  }

  // 移动端隐藏电梯导航（屏幕窄，靠工具条排序足够）
  .elevator {
    display: none;
  }
}
</style>
