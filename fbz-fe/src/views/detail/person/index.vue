<script setup lang="ts">
import type { MediaItem, PersonDetail } from "@/types/media.ts";
import { loadPersonDetail } from "@/service/modules/detail.ts";

const route = useRoute();
const id = computed(() => String(route.params.id ?? ""));
const person = ref<PersonDetail>();
const loading = ref(true);

const isExpanded = ref(false);
const isBioLong = ref(false);

type WorkFilter = "all" | "movie" | "tv";
type WorkSort = "default" | "rating" | "year";

const activeFilter = ref<WorkFilter>("all");
const sortBy = ref<WorkSort>("default");

const filterTabs: { label: string; value: WorkFilter }[] = [
  { label: "全部", value: "all" },
  { label: "电影", value: "movie" },
  { label: "剧集", value: "tv" },
];

const sortOptions: { label: string; value: WorkSort }[] = [
  { label: "默认顺序", value: "default" },
  { label: "评分最高", value: "rating" },
  { label: "上映年份", value: "year" },
];

watch(
  id,
  async (v) => {
    loading.value = true;
    const loaded = await loadPersonDetail(v);
    if (id.value !== v) return;
    person.value = loaded;
    loading.value = false;
  },
  { immediate: true },
);

watch(
  person,
  (newVal) => {
    isExpanded.value = false;
    // 当字数大于 240 时，开启折叠功能
    isBioLong.value = (newVal?.biography?.length ?? 0) > 240;
  },
  { immediate: true },
);

/** 生日 + 动态年龄。 */
function getAge(birthdayStr: string | null | undefined): string {
  if (!birthdayStr) return "";
  const birth = new Date(birthdayStr);
  if (isNaN(birth.getTime())) return birthdayStr;
  const today = new Date();
  let age = today.getFullYear() - birth.getFullYear();
  const m = today.getMonth() - birth.getMonth();
  if (m < 0 || (m === 0 && today.getDate() < birth.getDate())) age--;
  return `${birthdayStr.slice(0, 10)}（${age} 岁）`;
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
  if (activeFilter.value !== "all")
    list = list.filter((item) => item.detailType === activeFilter.value);
  if (sortBy.value === "rating") list.sort((a, b) => (b.rating ?? 0) - (a.rating ?? 0));
  else if (sortBy.value === "year") list.sort((a, b) => (b.year ?? 0) - (a.year ?? 0));
  return list;
});

const creditSummary = computed(() => {
  const credits = person.value?.known_for ?? [];
  const rated = credits.filter((c) => c.rating != null && c.rating > 0);
  const avg = rated.length
    ? (rated.reduce((acc, c) => acc + (c.rating ?? 0), 0) / rated.length).toFixed(1)
    : "—";
  return [
    { filter: "all" as const, label: "参演作品", value: credits.length, clickable: true },
    {
      filter: "movie" as const,
      label: "电影",
      value: credits.filter((credit) => credit.type === "movie").length,
      clickable: true,
    },
    {
      filter: "tv" as const,
      label: "剧集",
      value: credits.filter((credit) => credit.type === "tv").length,
      clickable: true,
    },
    { filter: "all" as const, label: "作品均分", value: avg, clickable: false },
  ];
});
</script>

<template>
  <main v-if="person" class="c-page detail-page person-page">
    <header class="person-hero">
      <div class="person-backdrop" aria-hidden="true">
        <img v-if="person.profile_path" :src="person.profile_path" alt="" loading="lazy" />
        <div class="backdrop-scrim" />
      </div>

      <PageHeader :title="person.name" />

      <div class="head">
        <div class="photo">
          <MediaPoster
            :src="person.profile_path ?? undefined"
            :title="person.name"
            ratio="poster"
            icon="user-round"
          />
        </div>

        <div class="info">
          <span class="type-chip"><BaseIcon name="user-round" :size="12" />人物</span>
          <h1 class="name">{{ person.name }}</h1>
          <div class="meta c-meta">
            <span v-if="person.known_for_department" class="dept-badge">
              {{ person.known_for_department }}
            </span>
            <span v-if="person.birthday" class="meta-item">
              <BaseIcon name="cake" :size="13" />{{ getAge(person.birthday) }}
            </span>
            <span v-if="person.place_of_birth" class="meta-item">
              <BaseIcon name="location" :size="13" />{{ person.place_of_birth }}
            </span>
          </div>

          <div class="bio-wrapper">
            <div class="bio-container" :class="{ 'is-collapsed': isBioLong && !isExpanded }">
              <p v-if="person.biography" class="bio">{{ person.biography }}</p>
              <p v-else class="bio muted">暂无简介</p>
            </div>
            <button
              v-if="isBioLong"
              class="bio-toggle-btn"
              type="button"
              @click="isExpanded = !isExpanded"
            >
              <BaseIcon :name="isExpanded ? 'up' : 'down'" :size="14" />
              {{ isExpanded ? "收起简介" : "展开全部简介" }}
            </button>
          </div>

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
    </header>

    <section v-if="knownFor.length" class="known">
      <div class="known-header-row">
        <h2 class="c-section-title">
          <BaseIcon name="clapperboard" :size="16" class="lead" />参演作品
        </h2>
        <div class="works-controls">
          <div class="seg" role="group" aria-label="作品类型">
            <button
              v-for="tab in filterTabs"
              :key="tab.value"
              type="button"
              :class="{ on: activeFilter === tab.value }"
              @click="activeFilter = tab.value"
            >
              {{ tab.label }}
            </button>
          </div>
          <BaseSelect v-model="sortBy" :options="sortOptions" aria-label="排序作品" size="sm" />
        </div>
      </div>

      <div v-if="filteredAndSortedWorks.length" class="grid">
        <MediaCard
          v-for="(item, i) in filteredAndSortedWorks"
          :key="item.id"
          :item="item"
          :subtitle="
            item.year ? (item.meta ? `${item.year} · ${item.meta}` : String(item.year)) : item.meta
          "
          layout="poster"
          :variant="(i % 2) as 0 | 1"
        />
      </div>
      <BaseEmptyState v-else compact icon="search-x" title="该分类下暂无参演作品" />
    </section>
  </main>

  <main v-else-if="loading" class="c-page">
    <p class="c-state"><BaseIcon name="loading" :size="16" class="c-spin" />正在加载人物…</p>
  </main>

  <main v-else class="c-page">
    <BaseEmptyState
      icon="user-round"
      title="未找到该人物"
      description="人物可能不存在，或后端尚未提供该人物详情。"
    >
      <RouterLink to="/" class="du-btn du-btn-ghost">
        <BaseIcon name="home" :size="15" />返回首页
      </RouterLink>
    </BaseEmptyState>
  </main>
</template>

<style scoped lang="scss">
.person-hero {
  position: relative;
  isolation: isolate;
  margin: 0 calc(var(--ui-shell-gutter) * -1);
  padding: 16px var(--ui-shell-gutter) 8px;
}

.person-backdrop {
  position: absolute;
  inset: 0;
  z-index: -1;
  overflow: hidden;

  img {
    width: 100%;
    height: 100%;
    object-fit: cover;
    filter: blur(40px) saturate(120%);
    opacity: 0.28;
    transform: scale(1.2);
  }
}

.backdrop-scrim {
  position: absolute;
  inset: 0;
  background: linear-gradient(180deg, rgb(var(--ui-scrim) / 40%) 0%, var(--ui-background) 100%);
}

.head {
  display: flex;
  gap: 32px;
  margin-top: 28px;
}

.photo {
  flex: 0 0 auto;
  align-self: flex-start;
  width: 220px;
  overflow: hidden;
  border: 1px solid var(--ui-glass-border);
  border-radius: var(--ui-radius-md);
  box-shadow: var(--ui-shadow-float);
}

.info {
  flex: 1;
  min-width: 0;
}

.type-chip {
  display: inline-flex;
  align-items: center;
  gap: 4px;
  height: 22px;
  margin-bottom: 10px;
  padding: 0 8px;
  border: 1px solid var(--ui-glass-border);
  border-radius: 5px;
  background: var(--ui-glass);
  color: var(--ui-text-muted);
  font-size: 11.5px;
  font-weight: 600;
}

.name {
  margin: 0;
  color: var(--ui-text);
  font-size: clamp(26px, 3.2vw, 40px);
  font-weight: 700;
  letter-spacing: -0.02em;
}

.meta {
  display: flex;
  flex-wrap: wrap;
  align-items: center;
  gap: 12px;
  margin-top: 10px;
}

.meta-item {
  display: inline-flex;
  align-items: center;
  gap: 5px;
}

.dept-badge {
  height: 20px;
  padding: 0 7px;
  display: inline-flex;
  align-items: center;
  border: 1px solid rgb(var(--ui-accent-channel) / 50%);
  border-radius: 4px;
  color: var(--ui-accent-hover);
  font-size: 11px;
  font-weight: 600;
}

.bio-wrapper {
  max-width: 880px;
  margin-top: 16px;
}

.bio-container.is-collapsed {
  max-height: 7.2em;
  overflow: hidden;
  mask-image: linear-gradient(180deg, #000 60%, transparent);
}

.bio {
  margin: 0;
  color: var(--ui-text-muted);
  font-size: 13.5px;
  line-height: 1.8;
  white-space: pre-line;

  &.muted {
    color: var(--ui-text-subtle);
  }
}

.bio-toggle-btn {
  display: inline-flex;
  align-items: center;
  gap: 4px;
  margin-top: 6px;
  padding: 0;
  border: 0;
  background: transparent;
  color: var(--ui-accent);
  font-size: 12.5px;
  cursor: pointer;

  &:hover {
    color: var(--ui-accent-hover);
  }
}

.credit-summary {
  display: flex;
  flex-wrap: wrap;
  gap: 8px;
  margin-top: 18px;
}

.summary-item {
  min-width: 96px;
  display: flex;
  flex-direction: column;
  gap: 2px;
  padding: 10px 14px;
  border: 1px solid var(--ui-border);
  border-radius: var(--ui-radius-md);
  background: var(--ui-surface);
  color: inherit;
  text-align: left;

  &.is-clickable {
    cursor: pointer;

    &:hover {
      border-color: var(--ui-border-strong);
      background: var(--ui-surface-hover);
    }
  }

  &.is-active {
    border-color: rgb(var(--ui-accent-channel) / 60%);
    background: rgb(var(--ui-accent-channel) / 10%);

    .summary-value {
      color: var(--ui-accent-hover);
    }
  }
}

.summary-value {
  color: var(--ui-text);
  font-size: 18px;
  font-weight: 700;
  font-variant-numeric: tabular-nums;
}

.summary-label {
  color: var(--ui-text-subtle);
  font-size: 11.5px;
}

.known {
  display: flex;
  flex-direction: column;
  gap: 12px;
}

.known-header-row {
  display: flex;
  flex-wrap: wrap;
  align-items: center;
  justify-content: space-between;
  gap: 10px;

  .lead {
    color: var(--ui-text-subtle);
  }
}

.works-controls {
  display: flex;
  align-items: center;
  gap: 8px;
}

.seg {
  display: inline-flex;
  padding: 2px;
  gap: 2px;
  border: 1px solid var(--ui-border);
  border-radius: 9px;
  background: var(--ui-surface);

  button {
    height: 26px;
    padding: 0 10px;
    border: 0;
    border-radius: 6px;
    background: transparent;
    color: var(--ui-text-muted);
    font-size: 12.5px;
    cursor: pointer;

    &.on {
      color: var(--ui-text);
      background: rgb(var(--ui-accent-channel) / 14%);
      box-shadow: inset 0 0 0 1px rgb(var(--ui-accent-channel) / 30%);
    }
  }
}

.grid {
  display: grid;
  grid-template-columns: repeat(auto-fill, minmax(150px, 1fr));
  gap: 20px 16px;
}

@media (max-width: 760px) {
  .head {
    flex-direction: column;
    gap: 18px;
  }

  .photo {
    width: 160px;
  }

  .grid {
    grid-template-columns: repeat(auto-fill, minmax(104px, 1fr));
  }
}
</style>
