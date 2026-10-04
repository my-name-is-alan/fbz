<script setup lang="ts">
import {
  enrich,
  listItems,
  imageUrl,
  readSession,
  mediaCard,
  serverRequest,
  errorMessage,
  preparePlayback,
  seriesEpisodes,
} from "@/service/modules/server.ts";
import type { ServerItem } from "@/service/modules/server.ts";
import { useLibraryStore } from "@/stores/library.ts";
import { usePlaybackStore } from "@/stores/playback.ts";
const props = withDefaults(
  defineProps<{ libraryId?: string; title?: string; mode?: "home" | "library" }>(),
  { mode: "library" },
);
const route = useRoute();
const router = useRouter();
const library = useLibraryStore();
const playback = usePlaybackStore();
const items = ref<ServerItem[]>([]);
const movies = ref<ServerItem[]>([]);
const series = ref<ServerItem[]>([]);
const resume = ref<ServerItem[]>([]);
const total = ref(0);
const hasMore = computed(() => total.value > page.value * 36 + items.value.length);
const loading = ref(true);
const error = ref("");
const preparing = ref(false);
const search = ref(String(route.query.search || ""));
const filters = reactive({
  library: "",
  type: String(route.query.type || ""),
  genre: "",
  year: "",
  played: "",
  sort: "DateCreated",
});
const filterOptions = ref<{ Genres: string[]; Years: number[] }>({ Genres: [], Years: [] });
const page = ref(0);
const showFilters = ref(false);
let generation = 0;
const hero = computed(
  () => movies.value.find((i) => imageUrl(i, "Backdrop")) || movies.value[0] || series.value[0],
);
const activeCount = computed(
  () => [filters.genre, filters.year, filters.played].filter(Boolean).length,
);
const filtering = computed(() => !!search.value || !!filters.type || activeCount.value > 0);
const activeLibrary = computed(() => props.libraryId || filters.library);
const title = computed(() => props.title || library.getById(activeLibrary.value)?.name || "媒体库");
const heroStyle = computed(() =>
  hero.value
    ? { "--hero-image": `url("${imageUrl(hero.value, "Backdrop") || imageUrl(hero.value) || ""}")` }
    : {},
);
async function refresh() {
  const current = ++generation;
  loading.value = true;
  error.value = "";
  if (!readSession()) {
    items.value = [];
    loading.value = false;
    return;
  }
  try {
    if (props.mode === "home") {
      const [m, s, r] = await Promise.all([
        listItems("", 0, "", { types: "Movie", limit: 14 }),
        listItems("", 0, "", { types: "Series", limit: 14 }),
        serverRequest<{ Items: ServerItem[] }>(
          `/emby/Users/${readSession()?.userId}/Items/Resume?Limit=4&EnableImages=true&EnableImageTypes=Primary,Backdrop`,
        ),
      ]);
      if (current !== generation) return;
      movies.value = m.Items;
      series.value = s.Items;
      resume.value = await enrich(r.Items);
    } else {
      const result = await listItems(activeLibrary.value, page.value * 36, search.value, {
        types: filters.type || "Movie,Series",
        genre: filters.genre,
        year: filters.year,
        played: filters.played,
        sort: filters.sort,
        descending: filters.sort !== "SortName",
      });
      if (current !== generation) return;
      items.value = result.Items;
      total.value = result.TotalRecordCount;
    }
  } catch (err) {
    if (current === generation) error.value = errorMessage(err);
  } finally {
    if (current === generation) loading.value = false;
  }
}
async function loadFilters() {
  if (!readSession()) return;
  try {
    const q = new URLSearchParams({ UserId: readSession()?.userId ?? "" });
    if (activeLibrary.value) q.set("ParentId", activeLibrary.value);
    filterOptions.value = await serverRequest(`/emby/Items/Filters?${q}`);
  } catch {
    filterOptions.value = { Genres: [], Years: [] };
  }
}
function find() {
  page.value = 0;
  void refresh();
}
function reset() {
  filters.genre = "";
  filters.year = "";
  filters.played = "";
  filters.type = "";
  search.value = "";
  find();
}
function paginate(delta: number) {
  page.value = Math.max(0, page.value + delta);
  void refresh();
  window.scrollTo({ top: 0, behavior: "smooth" });
}
async function heroPlay() {
  if (!hero.value || preparing.value) return;
  preparing.value = true;
  try {
    let item = hero.value;
    if (item.Type === "Series") {
      const result = await seriesEpisodes(item.Id);
      item = result.Items[0]!;
    }
    if (item) playback.open(await preparePlayback(item));
  } catch (err) {
    error.value = errorMessage(err);
  } finally {
    preparing.value = false;
  }
}
watch(
  () => [props.libraryId, props.mode],
  () => {
    page.value = 0;
    void refresh();
    void loadFilters();
  },
  { immediate: true },
);
watch(() => [filters.type, filters.genre, filters.year, filters.played, filters.sort], find);
watch(
  () => filters.library,
  () => {
    filters.genre = "";
    filters.year = "";
    find();
    void loadFilters();
  },
);
watch(
  () => route.query,
  () => {
    search.value = String(route.query.search || "");
    filters.type = String(route.query.type || "");
    find();
  },
);
useEventListener(window, "fbz-playback-saved", () => {
  void refresh();
});
</script>
<template>
  <main class="media-workspace" :class="{ 'is-home': mode === 'home' }">
    <template v-if="!readSession()"
      ><BaseEmptyState
        title="你的私人影院，从这里开始"
        description="连接媒体服务器，浏览自己的电影、剧集与观看记录。"
        icon="movie"
        ><RouterLink to="/user/login" class="du-btn du-btn-primary"
          >连接服务器<BaseIcon name="right" :size="16" /></RouterLink></BaseEmptyState
    ></template>
    <BaseEmptyState v-else-if="error" title="暂时无法读取媒体库" :description="error" error
      ><button class="du-btn" @click="refresh"><BaseIcon name="refresh" :size="16" />重试</button
      ><RouterLink class="du-btn du-btn-ghost" to="/user/login"
        >检查连接</RouterLink
      ></BaseEmptyState
    >
    <template v-else-if="mode === 'home'">
      <div v-if="loading" class="du-skeleton hero-skeleton" />
      <div v-else-if="hero" class="home-lead">
        <section class="feature-hero" :style="heroStyle">
          <div class="hero-shade" />
          <div class="hero-copy">
            <span class="eyebrow">私人收藏 · 随时开场</span>
            <h1>{{ hero.Name }}</h1>
            <div class="hero-meta">
              <span v-if="hero.ProductionYear">{{ hero.ProductionYear }}</span
              ><span v-if="hero.Genres?.length">{{ hero.Genres.slice(0, 3).join(" / ") }}</span
              ><span v-if="hero.CommunityRating" class="hero-rating"
                ><BaseIcon name="star" :size="14" />{{ hero.CommunityRating.toFixed(1) }}</span
              >
            </div>
            <p v-if="hero.Overview">{{ hero.Overview }}</p>
            <div class="hero-actions">
              <button class="du-btn hero-play" :disabled="preparing" @click="heroPlay">
                <BaseIcon :name="preparing ? 'loading' : 'play'" />立即播放</button
              ><RouterLink
                class="du-btn hero-more"
                :to="`/${hero.Type === 'Series' ? 'tv' : 'movie'}/${hero.Id}`"
                ><BaseIcon name="info" />查看详情</RouterLink
              >
            </div>
          </div>
        </section>
        <aside class="watching-panel">
          <header>
            <h2>继续观看</h2>
            <BaseIcon name="clock" :size="17" />
          </header>
          <div v-if="resume.length" class="continue-stack">
            <MediaCard
              v-for="item in resume.slice(0, 3)"
              :key="item.Id"
              :item="{ ...mediaCard(item), poster: imageUrl(item, 'Backdrop') || imageUrl(item) }"
              layout="wide"
              compact
              :show-rating="false"
            />
          </div>
          <div v-else class="continue-empty">
            <BaseIcon name="play" :size="28" />
            <h3>好故事，随时继续</h3>
            <p>开始播放后，你的观看记录会出现在这里。</p>
          </div>
        </aside>
      </div>
      <BaseEmptyState
        v-else
        title="为你的影院添加第一部作品"
        description="添加本地媒体库或连接光鸭目录，扫描后即可在这里观看。"
        ><RouterLink class="du-btn du-btn-primary" to="/admin/libraries">添加媒体库</RouterLink
        ><RouterLink class="du-btn du-btn-ghost" to="/admin/storage"
          >连接光鸭</RouterLink
        ></BaseEmptyState
      >
      <BaseScroller
        v-if="library.libraries.length"
        class="library-shortcuts"
        col-width="max-content"
        gap="8px"
        role="navigation"
        aria-label="快速切换媒体库"
      >
        <RouterLink
          v-for="lib in library.libraries"
          :key="lib.id"
          :to="`/library/${lib.id}`"
          class="du-btn du-btn-ghost"
          ><BaseIcon :name="lib.kind === 'series' ? 'tv' : 'movie'" :size="18" />{{ lib.name
          }}<span>{{ lib.count }}</span></RouterLink
        >
      </BaseScroller>
      <section
        v-for="row in [
          { title: '电影，值得留一点时间', items: movies, type: 'Movie' },
          { title: '下一集，接着看', items: series, type: 'Series' },
        ].filter((r) => r.items.length)"
        :key="row.type"
        class="media-section"
      >
        <header class="section-heading">
          <div>
            <span class="eyebrow">{{ row.type === "Movie" ? "电影收藏" : "剧集收藏" }}</span>
            <h2>{{ row.title }}</h2>
          </div>
          <RouterLink :to="`/library?type=${row.type}`" class="du-btn du-btn-ghost du-btn-sm"
            >查看全部<BaseIcon name="right" :size="16"
          /></RouterLink>
        </header>
        <div class="editorial-grid">
          <MediaCard v-for="item in row.items" :key="item.Id" :item="mediaCard(item)" />
        </div>
      </section>
    </template>
    <template v-else>
      <header class="browse-heading">
        <div>
          <span class="eyebrow">你的私人收藏</span>
          <h1>{{ title }}</h1>
          <p>电影、剧集，还有那些想再看一次的作品。</p>
        </div>
        <RouterLink class="du-btn du-btn-ghost du-btn-sm" to="/admin/libraries"
          ><BaseIcon name="settings" :size="17" />管理媒体库</RouterLink
        >
      </header>
      <div class="browse-tools">
        <div class="du-tabs type-tabs" role="tablist" aria-label="内容类型">
          <button
            v-for="tab in [
              { name: '全部', value: '' },
              { name: '电影', value: 'Movie' },
              { name: '剧集', value: 'Series' },
            ]"
            :key="tab.value"
            role="tab"
            class="du-tab"
            :class="{ 'du-tab-active': filters.type === tab.value }"
            :aria-selected="filters.type === tab.value"
            @click="filters.type = tab.value"
          >
            {{ tab.name }}
          </button>
        </div>
        <form class="search-field" @submit.prevent="find">
          <BaseIcon name="search" :size="17" /><input
            v-model="search"
            class="du-input"
            aria-label="搜索媒体"
            placeholder="搜索标题…"
          /><button class="du-btn du-btn-ghost du-btn-sm" aria-label="搜索" type="submit">
            <BaseIcon name="right" :size="16" />
          </button>
        </form>
        <button
          class="du-btn filter-toggle"
          :class="{ 'du-btn-active': showFilters }"
          :aria-expanded="showFilters"
          @click="showFilters = !showFilters"
        >
          <BaseIcon name="filter" :size="17" />筛选<span
            v-if="activeCount"
            class="du-badge du-badge-primary"
            >{{ activeCount }}</span
          >
        </button>
      </div>
      <div v-if="showFilters" class="filter-bar">
        <label v-if="!libraryId"
          >媒体库<BaseSelect
            v-model="filters.library"
            aria-label="媒体库"
            :options="[
              { label: '全部媒体库', value: '' },
              ...library.libraries.map((l) => ({ label: l.name, value: l.id })),
            ]" /></label
        ><label
          >题材<BaseSelect
            v-model="filters.genre"
            aria-label="题材"
            :options="[
              { label: '全部题材', value: '' },
              ...filterOptions.Genres.map((g) => ({ label: g, value: g })),
            ]" /></label
        ><label
          >年份<BaseSelect
            v-model="filters.year"
            aria-label="年份"
            :options="[
              { label: '全部年份', value: '' },
              ...filterOptions.Years.map((y) => ({ label: String(y), value: String(y) })),
            ]" /></label
        ><label
          >观看状态<BaseSelect
            v-model="filters.played"
            aria-label="观看状态"
            :options="[
              { label: '全部状态', value: '' },
              { label: '未看过', value: 'false' },
              { label: '已看完', value: 'true' },
            ]" /></label
        ><button class="du-btn du-btn-ghost" @click="reset">重置</button>
      </div>
      <div class="results-heading">
        <span>{{ loading ? "正在加载…" : `${total} 个作品` }}</span>
        <div>
          <BaseIcon name="list" :size="16" /><BaseSelect
            v-model="filters.sort"
            size="sm"
            aria-label="排序方式"
            :options="[
              { label: '最近添加', value: 'DateCreated' },
              { label: '标题 A–Z', value: 'SortName' },
              { label: '发行年份', value: 'ProductionYear' },
              { label: '评分优先', value: 'CommunityRating' },
            ]"
          />
        </div>
      </div>
      <div v-if="loading" class="editorial-grid">
        <div v-for="n in 12" :key="n" class="skeleton-card">
          <div class="du-skeleton" />
          <span class="du-skeleton" />
        </div>
      </div>
      <BaseEmptyState
        v-else-if="!items.length"
        :title="filtering ? '没有找到符合条件的作品' : '媒体库还在等待第一部作品'"
        :description="
          filtering
            ? '试试其他关键词，或清除筛选条件。'
            : '添加媒体目录并扫描后，电影与剧集会显示在这里。'
        "
        :icon="filtering ? 'search' : 'library'"
        ><button v-if="filtering" class="du-btn" @click="reset">清除筛选</button
        ><RouterLink v-else class="du-btn du-btn-primary" to="/admin/libraries"
          >添加媒体库</RouterLink
        ></BaseEmptyState
      >
      <div v-else class="editorial-grid">
        <MediaCard v-for="item in items" :key="item.Id" :item="mediaCard(item)" />
      </div>
      <nav v-if="page > 0 || hasMore" class="pagination" aria-label="分页">
        <button
          class="du-btn du-btn-circle"
          :disabled="page === 0 || loading"
          aria-label="上一页"
          @click="paginate(-1)"
        >
          <BaseIcon name="left" /></button
        ><span>第 {{ page + 1 }} 页</span
        ><button
          class="du-btn du-btn-circle"
          :disabled="!hasMore || loading"
          aria-label="下一页"
          @click="paginate(1)"
        >
          <BaseIcon name="right" />
        </button>
      </nav>
    </template>
  </main>
</template>
<style scoped lang="scss">
.media-workspace {
  max-width: 1560px;
  margin: auto;
  padding: calc(var(--header-h) + 44px) clamp(20px, 4.5vw, 72px) 90px;
}
.home-lead {
  display: grid;
  grid-template-columns: minmax(0, 1fr) 260px;
  gap: 30px;
}
.feature-hero {
  isolation: isolate;
  position: relative;
  min-height: 480px;
  border-radius: 20px;
  overflow: hidden;
  background:
    var(--hero-image) center 35% / cover,
    var(--fbz-color-panel);
}
.hero-shade {
  position: absolute;
  inset: 0;
  z-index: -1;
  background:
    linear-gradient(90deg, #080c11d9 0%, #090c1180 55%, #080c1115),
    linear-gradient(0deg, #080c11c9, transparent 80%);
}
.hero-copy {
  color: #fff;
  display: flex;
  flex-direction: column;
  justify-content: flex-end;
  padding: 42px;
  height: 100%;
  max-width: 660px;
  position: relative;
}
.eyebrow {
  font-size: 10px;
  letter-spacing: 1.4px;
  font-weight: 550;
  color: var(--fbz-color-text-muted);
}
.hero-copy .eyebrow {
  color: #e6e9eacc;
}
.hero-copy h1 {
  font-size: clamp(28px, 3.1vw, 47px);
  line-height: 1.16;
  letter-spacing: -1.4px;
  font-weight: 620;
  text-wrap: balance;
  margin: 17px 0;
}
.hero-meta {
  display: flex;
  gap: 15px;
  align-items: center;
  color: #e6e7e8;
  font-size: 12px;
}
.hero-rating {
  display: inline-flex;
  gap: 5px;
  align-items: center;
  color: #e6cc8b;
}
.hero-copy p {
  font-size: 13px;
  line-height: 1.9;
  color: #d1d5da;
  margin: 18px 0 0;
  display: -webkit-box;
  -webkit-line-clamp: 3;
  -webkit-box-orient: vertical;
  overflow: hidden;
  max-width: 46ch;
}
.hero-actions {
  display: flex;
  gap: 12px;
  margin-top: 26px;
}
.hero-play {
  background: #f4f5f5;
  color: #151a18;
  border: 0;
  padding-inline: 23px;
}
.hero-more {
  background: #ffffff18;
  color: white;
  border: 1px solid #ffffff35;
  backdrop-filter: blur(10px);
}
.watching-panel > header {
  display: flex;
  justify-content: space-between;
  align-items: center;
  margin: 3px 0 22px;
  color: var(--fbz-color-text-muted);
}
.watching-panel h2 {
  color: var(--fbz-color-text);
  font-size: 19px;
  font-weight: 550;
  margin: 0;
}
.continue-stack {
  display: grid;
  gap: 18px;
}
.continue-stack :deep(.visual) {
  max-height: 112px;
}
.continue-stack :deep(.card-caption) {
  padding-top: 8px;
}
.continue-empty {
  height: calc(100% - 50px);
  min-height: 240px;
  border: 1px dashed var(--fbz-color-line);
  border-radius: 14px;
  display: flex;
  flex-direction: column;
  align-items: center;
  justify-content: center;
  text-align: center;
  padding: 26px;
  color: var(--fbz-color-text-muted);
}
.continue-empty h3 {
  color: var(--fbz-color-text-soft);
  font-size: 14px;
  margin: 20px 0 12px;
}
.continue-empty p {
  font-size: 12px;
  line-height: 1.9;
  margin: 0;
}
.library-shortcuts {
  display: flex;
  gap: 8px;
  flex-wrap: wrap;
  margin: 25px 0 46px;
  padding-bottom: 24px;
  border-bottom: 1px solid var(--fbz-color-line-soft);
}
.library-shortcuts .du-btn {
  font-size: 12px;
  color: var(--fbz-color-text-soft);
}
.library-shortcuts span {
  color: var(--fbz-color-text-muted);
  font-size: 10px;
  margin-left: 8px;
}
.media-section {
  margin-top: 42px;
}
.section-heading {
  display: flex;
  align-items: end;
  justify-content: space-between;
  gap: 20px;
  margin-bottom: 25px;
}
.section-heading h2 {
  font-size: 23px;
  letter-spacing: -0.6px;
  font-weight: 550;
  margin: 8px 0 0;
}
.section-heading .du-btn {
  color: var(--fbz-color-text-muted);
}
.editorial-grid {
  display: grid;
  grid-template-columns: repeat(6, minmax(0, 1fr));
  gap: 30px 22px;
}
.hero-skeleton {
  height: 480px;
  border-radius: 20px;
}
.browse-heading {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 20px;
  margin-bottom: 35px;
}
.browse-heading h1 {
  font-size: 36px;
  letter-spacing: -1px;
  font-weight: 550;
  margin: 10px 0;
}
.browse-heading p {
  color: var(--fbz-color-text-muted);
  font-size: 13px;
}
.browse-tools {
  display: flex;
  align-items: center;
  gap: 14px;
  border-bottom: 1px solid var(--fbz-color-line-soft);
  padding-bottom: 18px;
}
.type-tabs {
  gap: 10px;
}
.du-tab {
  font-size: 14px;
  border-radius: 8px;
  padding-inline: 20px;
  color: var(--fbz-color-text-muted);
}
.du-tab-active {
  background: var(--fbz-color-panel-strong);
  color: var(--fbz-color-text);
}
.search-field {
  position: relative;
  margin-left: auto;
  display: flex;
  align-items: center;
  color: var(--fbz-color-text-muted);
}
.search-field > svg {
  position: absolute;
  left: 12px;
  pointer-events: none;
}
.search-field .du-input {
  padding-left: 38px;
  width: 240px;
  font-size: 13px;
  padding-right: 35px;
}
.search-field .du-btn {
  position: absolute;
  right: 3px;
  width: 32px;
}
.filter-toggle {
  font-size: 13px;
  font-weight: 500;
}
.filter-bar {
  display: grid;
  grid-template-columns: repeat(4, minmax(0, 1fr)) auto;
  gap: 15px;
  align-items: end;
  padding: 22px 0;
  border-bottom: 1px solid var(--fbz-color-line-soft);
}
label {
  display: grid;
  gap: 9px;
  color: var(--fbz-color-text-muted);
  font-size: 11px;
}
.results-heading {
  display: flex;
  justify-content: space-between;
  align-items: center;
  margin: 23px 0;
  color: var(--fbz-color-text-muted);
  font-size: 12px;
}
.results-heading > div {
  display: flex;
  align-items: center;
  gap: 9px;
}
.results-heading :deep(select) {
  width: 135px;
  background: transparent;
  border-color: transparent;
  font-size: 12px;
}
.skeleton-card > div {
  aspect-ratio: 2/3;
  border-radius: 12px;
}
.skeleton-card > span {
  display: block;
  height: 13px;
  width: 65%;
  margin-top: 14px;
}
.pagination {
  display: flex;
  align-items: center;
  justify-content: center;
  gap: 22px;
  margin-top: 42px;
}
.pagination span {
  color: var(--fbz-color-text-muted);
  font-size: 12px;
}
@media (min-width: 1600px) {
  .editorial-grid {
    grid-template-columns: repeat(7, minmax(0, 1fr));
  }
}
@media (max-width: 1100px) {
  .home-lead {
    grid-template-columns: minmax(0, 1fr) 210px;
    gap: 20px;
  }
  .feature-hero {
    min-height: 430px;
  }
  .hero-copy {
    padding: 30px;
  }
  .editorial-grid {
    grid-template-columns: repeat(5, minmax(0, 1fr));
    gap: 26px 18px;
  }
}
@media (max-width: 800px) {
  .home-lead {
    grid-template-columns: 1fr;
  }
  .watching-panel {
    display: none;
  }
  .editorial-grid {
    grid-template-columns: repeat(4, minmax(0, 1fr));
  }
  .filter-bar {
    grid-template-columns: repeat(2, minmax(0, 1fr));
  }
  .browse-tools {
    flex-wrap: wrap;
  }
  .search-field {
    flex: 1;
    min-width: 150px;
  }
  .search-field .du-input {
    width: 100%;
  }
  .type-tabs {
    width: 100%;
  }
  .browse-heading p {
    display: none;
  }
}
@media (max-width: 600px) {
  .media-workspace {
    padding: calc(var(--header-h) + 22px) 18px 100px;
  }
  .feature-hero {
    min-height: 510px;
    border-radius: 15px;
    background-position: 62% center;
  }
  .hero-copy {
    justify-content: flex-end;
    padding: 26px;
    max-width: none;
  }
  .hero-shade {
    background: linear-gradient(0deg, #080c11f2 0%, #080c1180 58%, #080c1120);
  }
  .hero-copy h1 {
    font-size: 31px;
    line-height: 1.2;
    max-width: 14ch;
  }
  .hero-copy p {
    -webkit-line-clamp: 3;
    font-size: 12px;
  }
  .hero-actions {
    gap: 8px;
  }
  .hero-actions .du-btn {
    flex: 1;
    font-size: 12px;
    padding-inline: 12px;
  }
  .library-shortcuts {
    flex-wrap: nowrap;
    overflow: auto;
    margin-bottom: 28px;
    padding-bottom: 16px;
  }
  .library-shortcuts .du-btn {
    flex-shrink: 0;
  }
  .editorial-grid {
    grid-template-columns: repeat(3, minmax(0, 1fr));
    gap: 24px 12px;
  }
  .section-heading h2 {
    font-size: 20px;
  }
  .section-heading .du-btn {
    font-size: 11px;
    padding-inline: 5px;
  }
  .browse-heading h1 {
    font-size: 29px;
  }
  .browse-heading > a {
    font-size: 0;
    min-width: 36px;
    padding: 8px;
  }
  .browse-heading > a svg {
    width: 19px;
  }
  .du-tab {
    padding-inline: 18px;
  }
  .browse-tools {
    gap: 10px;
  }
  .filter-toggle {
    font-size: 12px;
  }
  .filter-bar {
    gap: 12px;
  }
  .hero-skeleton {
    height: 510px;
  }
}
@media (max-width: 360px) {
  .editorial-grid {
    grid-template-columns: repeat(2, minmax(0, 1fr));
  }
}
@media (max-width: 800px) {
  .watching-panel {
    display: block;
    min-width: 0;
  }
  .continue-stack {
    grid-template-columns: repeat(3, minmax(0, 1fr));
    gap: 16px;
  }
  .continue-empty {
    min-height: 140px;
    height: auto;
    align-items: flex-start;
    text-align: left;
    padding: 24px;
  }
  .continue-empty h3 {
    margin: 10px 0 5px;
  }
}
@media (max-width: 600px) {
  .continue-stack {
    grid-template-columns: repeat(2, minmax(0, 1fr));
    gap: 14px;
  }
  .watching-panel > header {
    margin: 6px 0 16px;
  }
  .watching-panel h2 {
    font-size: 18px;
  }
}
.library-shortcuts {
  display: block;
  overflow: visible;
}
.library-shortcuts :deep(.track) {
  align-items: center;
  grid-auto-columns: max-content;
}
.hero-copy h1 {
  display: -webkit-box;
  -webkit-line-clamp: 3;
  -webkit-box-orient: vertical;
  overflow: hidden;
}
@media (min-width: 1101px) {
  .feature-hero {
    min-height: 400px;
  }
  .hero-skeleton {
    height: 400px;
  }
  .hero-copy {
    padding: 28px 36px;
  }
  .hero-copy h1 {
    font-size: clamp(28px, 2.8vw, 42px);
    margin: 14px 0;
  }
  .hero-copy p {
    font-size: 12.5px;
    line-height: 1.8;
  }
  .hero-actions {
    margin-top: 20px;
  }
  .library-shortcuts {
    margin: 20px 0 28px;
    padding-bottom: 18px;
  }
  .media-section {
    margin-top: 30px;
  }
  .section-heading {
    margin-bottom: 20px;
  }
}
@media (min-width: 1200px) {
  .editorial-grid {
    grid-template-columns: repeat(7, minmax(0, 1fr));
  }
}
.header-search > svg,
.search-field > svg {
  z-index: 1;
}
</style>
