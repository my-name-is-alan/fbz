<script setup lang="ts">
import type { MediaItem, PersonDetail } from "@/types/media.ts";
import { loadPersonDetail } from "@/service/modules/detail.ts";

const route = useRoute();
const id = computed(() => String(route.params.id ?? ""));
const person = ref<PersonDetail>();

const isExpanded = ref(false);
const isBioLong = ref(false);

const activeFilter = ref<"all" | "movie" | "tv">("all");
const sortBy = ref<"default" | "rating" | "year">("default");

const filterTabs = [
  { label: "全部", value: "all" },
  { label: "电影", value: "movie" },
  { label: "电视剧", value: "tv" },
] as const;

const sortOptions = [
  { label: "默认顺序", value: "default" },
  { label: "评分最高", value: "rating" },
  { label: "上映年份", value: "year" },
] as const;

watch(
  id,
  async (v) => {
    person.value = await loadPersonDetail(v);
  },
  { immediate: true },
);

watch(
  person,
  (newVal) => {
    isExpanded.value = false;
    if (!newVal?.biography) {
      isBioLong.value = false;
      return;
    }
    // 当字数大于 240 时，开启折叠功能
    isBioLong.value = newVal.biography.length > 240;
  },
  { immediate: true },
);

// 格式化年龄展示，采用动态计算
function getAge(birthdayStr: string | null | undefined): string {
  if (!birthdayStr) return "";
  const birth = new Date(birthdayStr);
  if (isNaN(birth.getTime())) return birthdayStr;

  const today = new Date();
  let age = today.getFullYear() - birth.getFullYear();
  const m = today.getMonth() - birth.getMonth();
  if (m < 0 || (m === 0 && today.getDate() < birth.getDate())) {
    age--;
  }
  return `${birthdayStr} (${age} 岁)`;
}

const knownFor = computed<MediaItem[]>(
  () =>
    person.value?.known_for.map((c) => ({
      id: String(c.id),
      libraryId: c.libraryId,
      detailType: c.type,
      title: c.title,
      meta: c.character ? `饰 ${c.character}` : "",
      poster: c.poster_path ?? undefined,
      rating: c.rating ?? undefined,
      year: c.year ?? undefined,
    })) ?? [],
);

const filteredAndSortedWorks = computed(() => {
  let list = [...knownFor.value];

  // 1. 过滤类型
  if (activeFilter.value !== "all") {
    list = list.filter((item) => item.detailType === activeFilter.value);
  }

  // 2. 排序
  if (sortBy.value === "rating") {
    list.sort((a, b) => (b.rating ?? 0) - (a.rating ?? 0));
  } else if (sortBy.value === "year") {
    list.sort((a, b) => (b.year ?? 0) - (a.year ?? 0));
  }

  return list;
});

const creditSummary = computed(() => {
  const credits = person.value?.known_for ?? [];
  const ratedCredits = credits.filter((c) => c.rating != null && c.rating > 0);
  const avg = ratedCredits.length
    ? (ratedCredits.reduce((acc, c) => acc + (c.rating ?? 0), 0) / ratedCredits.length).toFixed(1)
    : "—";

  return [
    { filter: "all" as const, label: "代表作品", value: credits.length, clickable: true },
    {
      filter: "movie" as const,
      label: "电影作品",
      value: credits.filter((credit) => credit.type === "movie").length,
      clickable: true,
    },
    {
      filter: "tv" as const,
      label: "剧集作品",
      value: credits.filter((credit) => credit.type === "tv").length,
      clickable: true,
    },
    { filter: "all" as const, label: "作品均分", value: avg, clickable: false },
  ];
});
</script>

<template>
  <div v-if="person" class="person-view-container">
    <!-- 动态氛围感背景：高斯模糊的人物照，自适应主题亮度 -->
    <div class="person-backdrop">
      <img
        v-if="person.profile_path"
        :src="person.profile_path"
        :alt="person.name"
        loading="lazy"
      />
      <div class="backdrop-scrim" />
    </div>

    <main class="person-view">
      <PageHeader :title="person.name" />

      <div class="head">
        <div class="photo">
          <MediaPoster
            :src="person.profile_path ?? undefined"
            :title="person.name"
            ratio="poster"
          />
        </div>

        <div class="info">
          <h1 class="name">{{ person.name }}</h1>
          <div class="meta">
            <span v-if="person.known_for_department" class="dept-badge">
              {{ person.known_for_department }}
            </span>
            <template v-if="person.birthday">
              <span class="dot" />
              <span>🎂 {{ getAge(person.birthday) }}</span>
            </template>
            <template v-if="person.place_of_birth">
              <span class="dot" />
              <span>📍 {{ person.place_of_birth }}</span>
            </template>
          </div>

          <!-- 简介折叠面板：渐变蒙版，高度平滑过渡 -->
          <div class="bio-wrapper">
            <div class="bio-container" :class="{ 'is-collapsed': isBioLong && !isExpanded }">
              <p v-if="person.biography" class="bio">{{ person.biography }}</p>
              <p v-else class="bio muted">暂无简介</p>
              <div v-if="isBioLong && !isExpanded" class="bio-fade-mask" />
            </div>
            <button
              v-if="isBioLong"
              class="bio-toggle-btn"
              type="button"
              @click="isExpanded = !isExpanded"
            >
              <span class="toggle-icon">{{ isExpanded ? "▲" : "▼" }}</span>
              <span>{{ isExpanded ? "收起简介" : "展开全部简介" }}</span>
            </button>
          </div>

          <!-- 交互指标卡片 -->
          <div class="credit-summary" aria-label="作品概览">
            <component
              :is="item.clickable ? 'button' : 'div'"
              v-for="item in creditSummary"
              :key="item.label"
              class="summary-item"
              :class="{
                'is-clickable': item.clickable,
                'is-active': item.clickable && activeFilter === item.filter,
              }"
              :type="item.clickable ? 'button' : undefined"
              @click="item.clickable ? (activeFilter = item.filter) : undefined"
            >
              <span class="summary-value">{{ item.value }}</span>
              <span class="summary-label">{{ item.label }}</span>
            </component>
          </div>
        </div>
      </div>

      <section v-if="knownFor.length" class="known">
        <!-- 头部控制栏 -->
        <div class="known-header-row">
          <h2 class="section-title">参演作品</h2>
          <div class="works-controls">
            <!-- 筛选 Tab 页签 -->
            <div class="filter-tabs">
              <button
                v-for="tab in filterTabs"
                :key="tab.value"
                class="tab-btn"
                :class="{ active: activeFilter === tab.value }"
                type="button"
                @click="activeFilter = tab.value"
              >
                {{ tab.label }}
              </button>
            </div>
            <!-- 排序下拉选择 -->
            <div class="sort-selector">
              <BaseSelect
                v-slot="{ value }"
                v-model="sortBy"
                :options="sortOptions"
                aria-label="排序作品"
                size="sm"
              />
            </div>
          </div>
        </div>

        <!-- 作品网格 -->
        <div v-if="filteredAndSortedWorks.length" class="grid">
          <MediaCard
            v-for="(item, i) in filteredAndSortedWorks"
            :key="item.id"
            :item="item"
            :subtitle="
              item.year
                ? item.meta
                  ? `${item.year} · ${item.meta}`
                  : String(item.year)
                : item.meta
            "
            layout="poster"
            :variant="(i % 2) as 0 | 1"
          />
        </div>

        <!-- 空状态占位 -->
        <div v-else class="empty-works">
          <svg
            class="empty-icon"
            viewBox="0 0 24 24"
            width="48"
            height="48"
            fill="none"
            stroke="currentColor"
            stroke-width="1.5"
          >
            <circle cx="12" cy="12" r="10" />
            <path d="M8 15h8" />
            <circle cx="9" cy="9.5" r="1.5" fill="currentColor" />
            <circle cx="15" cy="9.5" r="1.5" fill="currentColor" />
          </svg>
          <p>该分类下暂无参演作品</p>
        </div>
      </section>
    </main>
  </div>

  <main v-else class="detail-missing">
    <p>未找到该人物，或后端尚未提供该人物详情。</p>
    <RouterLink to="/" class="link">返回首页</RouterLink>
  </main>
</template>

<style scoped lang="scss">
.person-view-container {
  position: relative;
  min-height: 100vh;
}

// 氛围感背景样式
.person-backdrop {
  position: fixed;
  inset: 0;
  z-index: 0;
  pointer-events: none;
  overflow: hidden;

  img {
    width: 100%;
    height: 100%;
    object-fit: cover;
    object-position: center 25%;
    filter: blur(80px) brightness(0.35) saturate(130%);
    opacity: 0.22;
    transition:
      filter var(--fbz-motion-slow),
      opacity var(--fbz-motion-slow);
  }

  .backdrop-scrim {
    position: absolute;
    inset: 0;
    background: radial-gradient(circle at center, transparent 20%, var(--fbz-color-bg) 100%);
  }
}

// 亮色主题下弱化背景亮度和透明度，保证对比度
:root[data-theme="light"] {
  .person-backdrop img {
    filter: blur(80px) brightness(0.9) saturate(110%);
    opacity: 0.11;
  }
}

.person-view {
  position: relative;
  z-index: 1;
  max-width: 1280px;
  margin: 0 auto;
  padding: calc(var(--header-h, 60px) + var(--fbz-space-8)) var(--fbz-space-8) 80px;
  min-height: 100vh;
}

.head {
  display: flex;
  gap: var(--fbz-space-8);
  margin-bottom: var(--fbz-space-8);
}

.photo {
  flex: 0 0 220px;
  width: 220px;
  border-radius: var(--fbz-radius-hero);
  overflow: hidden;
  border: 1px solid var(--fbz-color-line);
  box-shadow: var(--fbz-shadow-panel);
}

.info {
  flex: 1;
  min-width: 0;
}

.name {
  margin: 0 0 var(--fbz-space-3);
  font-size: 40px;
  font-weight: 800;
  letter-spacing: -0.5px;
}

.meta {
  display: flex;
  align-items: center;
  flex-wrap: wrap;
  gap: var(--fbz-space-3);
  margin-bottom: var(--fbz-space-5);
  font-size: var(--fbz-font-size-md);
  color: var(--fbz-color-text-soft);

  .dept-badge {
    padding: 2px 8px;
    background: color-mix(in srgb, var(--fbz-color-brand-500) 10%, var(--fbz-color-panel-strong));
    border: 1px solid color-mix(in srgb, var(--fbz-color-brand-500) 25%, var(--fbz-color-line));
    color: var(--fbz-color-brand-500);
    border-radius: 4px;
    font-size: var(--fbz-font-size-xs);
    font-weight: 700;
  }

  .dot {
    width: 3px;
    height: 3px;
    border-radius: 50%;
    background: var(--fbz-color-text-muted);
  }
}

// 简介折叠逻辑
.bio-wrapper {
  margin-bottom: var(--fbz-space-5);
}

.bio-container {
  position: relative;
  max-width: 760px;
  overflow: hidden;
  max-height: 1000px;
  transition: max-height 0.4s cubic-bezier(0.4, 0, 0.2, 1);

  &.is-collapsed {
    max-height: 108px; // 约4行高度
  }
}

.bio {
  margin: 0;
  font-size: var(--fbz-font-size-md);
  line-height: 1.75;
  color: var(--fbz-color-text-soft);
  white-space: pre-line;

  &.muted {
    color: var(--fbz-color-text-muted);
  }
}

.bio-fade-mask {
  position: absolute;
  inset: auto 0 0 0;
  height: 48px;
  background: linear-gradient(to bottom, transparent 0%, var(--fbz-color-bg) 100%);
  pointer-events: none;
}

.bio-toggle-btn {
  background: none;
  border: none;
  padding: 0;
  margin-top: var(--fbz-space-2);
  color: var(--fbz-color-brand-500);
  font-size: var(--fbz-font-size-sm);
  font-weight: 700;
  cursor: pointer;
  display: inline-flex;
  align-items: center;
  gap: 5px;
  transition:
    color var(--fbz-motion-fast) ease,
    transform var(--fbz-motion-fast) ease;

  &:hover {
    color: var(--fbz-color-brand-600);
    transform: translateY(1px);
  }

  .toggle-icon {
    font-size: 8px;
  }
}

// 交互指标卡片
.credit-summary {
  display: flex;
  flex-wrap: wrap;
  gap: var(--fbz-space-3);
  margin-top: var(--fbz-space-5);
}

.summary-item {
  min-width: 104px;
  padding: var(--fbz-space-3) var(--fbz-space-4);
  border: 1px solid var(--fbz-color-line-soft);
  border-radius: var(--fbz-radius-card);
  background: var(--fbz-color-panel);
  box-shadow: 0 4px 12px rgba(0, 0, 0, 0.04);
  transition:
    transform var(--fbz-motion-fast) ease,
    border-color var(--fbz-motion-fast) ease,
    background var(--fbz-motion-fast) ease,
    box-shadow var(--fbz-motion-fast) ease;

  &.is-clickable {
    cursor: pointer;
    text-align: left;

    &:hover {
      transform: translateY(-3px);
      border-color: var(--fbz-color-brand-500);
      box-shadow: 0 8px 20px color-mix(in srgb, var(--fbz-color-brand-500) 10%, transparent);
    }

    &:active {
      transform: translateY(-1px) scale(0.98);
    }
  }

  &.is-active {
    border-color: var(--fbz-color-brand-500);
    background: color-mix(in srgb, var(--fbz-color-brand-500) 8%, var(--fbz-color-panel));
    box-shadow: 0 6px 16px color-mix(in srgb, var(--fbz-color-brand-500) 12%, transparent);
    transform: translateY(-2px);
  }
}

.summary-value {
  display: block;
  color: var(--fbz-color-brand-500);
  font-size: 20px;
  font-weight: 900;
  font-family: var(--fbz-font-display);
}

.summary-label {
  display: block;
  margin-top: 2px;
  color: var(--fbz-color-text-muted);
  font-size: var(--fbz-font-size-sm);
  font-weight: 500;
}

// 作品控制栏
.known-header-row {
  display: flex;
  align-items: center;
  justify-content: space-between;
  flex-wrap: wrap;
  gap: var(--fbz-space-4);
  margin: var(--fbz-space-6) 0 var(--fbz-space-4);
  border-bottom: 1px solid var(--fbz-color-line-soft);
  padding-bottom: var(--fbz-space-3);
}

.section-title {
  margin: 0;
  font-size: var(--fbz-font-size-lg);
  font-weight: 800;
  border-left: 3px solid var(--fbz-color-brand-500);
  padding-left: var(--fbz-space-2);
  line-height: 1.2;
}

.works-controls {
  display: flex;
  align-items: center;
  gap: var(--fbz-space-4);
}

.filter-tabs {
  display: inline-flex;
  background: var(--fbz-color-panel-strong);
  border: 1px solid var(--fbz-color-line);
  padding: 3px;
  border-radius: var(--fbz-radius-control);
}

.tab-btn {
  background: none;
  border: none;
  padding: 6px 14px;
  font-size: var(--fbz-font-size-sm);
  font-weight: 600;
  color: var(--fbz-color-text-muted);
  border-radius: calc(var(--fbz-radius-control) - 2px);
  cursor: pointer;
  transition:
    color var(--fbz-motion-fast),
    background var(--fbz-motion-fast);

  &:hover {
    color: var(--fbz-color-text);
  }

  &.active {
    background: var(--fbz-color-panel);
    color: var(--fbz-color-brand-500);
    box-shadow: 0 2px 8px rgba(0, 0, 0, 0.1);
  }
}

.sort-selector {
  display: flex;
  align-items: center;
}

.grid {
  display: grid;
  grid-template-columns: repeat(auto-fill, minmax(132px, 1fr));
  gap: var(--fbz-space-5) var(--fbz-space-4);
}

.empty-works {
  display: flex;
  flex-direction: column;
  align-items: center;
  justify-content: center;
  padding: var(--fbz-space-8) 0;
  color: var(--fbz-color-text-muted);
  text-align: center;

  .empty-icon {
    margin-bottom: var(--fbz-space-3);
    opacity: 0.6;
    color: var(--fbz-color-text-muted);
  }

  p {
    margin: 0;
    font-size: var(--fbz-font-size-md);
  }
}

.link {
  color: var(--fbz-color-brand-500);
  text-decoration: none;
  font-weight: 700;

  &:hover {
    text-decoration: underline;
  }
}

.detail-missing {
  min-height: 100vh;
  display: grid;
  place-content: center;
  gap: var(--fbz-space-3);
  text-align: center;
  color: var(--fbz-color-text-muted);
}

@media (max-width: 768px) {
  .head {
    flex-direction: column;
    align-items: center;
    text-align: center;
    gap: var(--fbz-space-5);
  }

  .meta {
    justify-content: center;
  }

  .bio-container {
    margin: 0 auto;
  }

  .bio-toggle-btn {
    justify-content: center;
    width: 100%;
  }

  .credit-summary {
    justify-content: center;
  }
}

@media (max-width: 600px) {
  .person-view {
    padding: calc(var(--header-h, 56px) + var(--fbz-space-5)) var(--fbz-space-4) 60px;
  }

  .photo {
    width: 140px;
  }

  .name {
    font-size: 30px;
  }

  .known-header-row {
    flex-direction: column;
    align-items: flex-start;
    gap: var(--fbz-space-3);
  }

  .works-controls {
    width: 100%;
    justify-content: space-between;
    gap: var(--fbz-space-2);
  }

  .filter-tabs {
    flex: 1;
  }

  .tab-btn {
    flex: 1;
    text-align: center;
    padding: 6px 8px;
    font-size: var(--fbz-font-size-xs);
  }

  .grid {
    grid-template-columns: repeat(auto-fill, minmax(104px, 1fr));
    gap: var(--fbz-space-4) var(--fbz-space-3);
  }
}
</style>
