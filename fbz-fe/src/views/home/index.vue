<script setup lang="ts">
import { fetchHomeData } from "@/service/modules/navigation.ts";
import { useLibraryStore } from "@/stores/library.ts";
import { useUiStore } from "@/stores/ui.ts";
import { useAuthStore } from "@/stores/auth.ts";
import type { FeaturedItem } from "@/types/media.ts";
import type { HomeRow } from "@/types/navigation.ts";
import { libraryRoute } from "@/utils/libraryRoute.ts";
import { libraryTypeMeta } from "@/utils/libraryType.ts";

const libraryStore = useLibraryStore();
const uiStore = useUiStore();
const authStore = useAuthStore();
const { libraries } = storeToRefs(libraryStore);

const featured = ref<FeaturedItem[]>([]);
const rows = ref<HomeRow[]>([]);
const loaded = ref(false);
const error = ref<string | null>(null);

const fmt = new Intl.NumberFormat("zh-CN");

const hasLibraries = computed(() => libraries.value.length > 0);
const hasAnyLibraryItems = computed(() => libraryStore.totalCount > 0);
const emptyStateTitle = computed(() =>
  hasLibraries.value ? "媒体库还没有可展示内容" : "还没有创建媒体库",
);
const emptyStateDescription = computed(() =>
  hasLibraries.value
    ? "媒体库已存在，但还没有扫描出的媒体条目。进入媒体库管理检查路径并重新触发扫描。"
    : "先创建一个媒体库，选择后端可访问的服务器目录，然后让后端扫描真实媒体文件。",
);

/** 分区 key → 图标（导航 BFF 行：续看 / 最新 / 收藏 / 按库）。 */
function rowIcon(row: HomeRow): string {
  if (row.key === "continue") return "clock";
  if (row.key === "latest") return "sparkles";
  if (row.key === "favorites") return "heart";
  const lib = libraries.value.find((l) => row.to?.endsWith(l.id) || row.key.endsWith(l.id));
  return lib ? libraryTypeMeta(lib.libraryType).icon : "library";
}

async function refreshHome() {
  try {
    const [data] = await Promise.all([
      fetchHomeData(),
      libraryStore.loaded ? Promise.resolve(true) : libraryStore.loadFromBackend(),
    ]);
    rows.value = data.rows;
    featured.value = data.featured;
    error.value = null;
  } catch {
    error.value = "首页数据加载失败，请检查网络、登录状态或服务器运行状态。";
  } finally {
    loaded.value = true;
  }
}

onMounted(() => {
  void refreshHome();
});

useEventListener(window, "fbz-playback-saved", () => {
  void refreshHome();
});

useEventListener(window, "fbz:item-deleted", () => {
  void refreshHome();
});
</script>

<template>
  <main class="c-page home-view">
    <nav v-if="libraries.length" class="lib-chips" aria-label="媒体库快捷入口">
      <RouterLink to="/library" class="chip">
        <BaseIcon name="grid" :size="14" />全部媒体
        <span class="n">{{ fmt.format(libraryStore.totalCount) }}</span>
      </RouterLink>
      <RouterLink v-for="lib in libraries" :key="lib.id" :to="libraryRoute(lib)" class="chip">
        <BaseIcon :name="libraryTypeMeta(lib.libraryType).icon" :size="14" />{{ lib.name }}
        <span class="n">{{ fmt.format(lib.count) }}</span>
      </RouterLink>
    </nav>

    <HomeHero v-if="featured.length" :items="featured" />

    <div v-if="!loaded" class="skeleton-rows" aria-busy="true" aria-label="正在加载首页">
      <div v-for="r in 2" :key="r" class="skeleton-row">
        <div class="sk-title" />
        <div class="sk-cards">
          <div v-for="c in 7" :key="c" class="sk-card" />
        </div>
      </div>
    </div>
    <p v-else-if="error" class="c-state is-error">
      <BaseIcon name="error" :size="16" />{{ error }}
    </p>
    <BaseEmptyState
      v-else-if="!rows.length || !hasAnyLibraryItems"
      :icon="hasLibraries ? 'scan' : 'library-big'"
      :title="emptyStateTitle"
      :description="emptyStateDescription"
    >
      <button
        v-if="authStore.isAdmin"
        class="du-btn du-btn-primary"
        type="button"
        @click="uiStore.openLibraryEditor(null)"
      >
        <BaseIcon name="plus" :size="16" />新建媒体库
      </button>
      <RouterLink v-if="authStore.isAdmin" class="du-btn du-btn-ghost" to="/admin/libraries">
        <BaseIcon name="settings" :size="16" />媒体库管理
      </RouterLink>
    </BaseEmptyState>

    <MediaRow
      v-for="row in rows"
      :key="row.key"
      :title="row.title"
      :icon="rowIcon(row)"
      :items="row.items"
      :layout="row.layout"
      :size="row.layout === 'wide' ? 'xlarge' : 'normal'"
      :to="row.to"
      :show-resolution="row.layout !== 'wide'"
      :show-rating="row.layout !== 'wide'"
    />
  </main>
</template>

<style scoped lang="scss">
.home-view {
  display: flex;
  flex-direction: column;
  gap: 28px;
}

.lib-chips {
  display: flex;
  gap: 8px;
  margin-bottom: -8px;
  overflow-x: auto;
  scrollbar-width: none;

  &::-webkit-scrollbar {
    display: none;
  }
}

.chip {
  flex: 0 0 auto;
  display: inline-flex;
  align-items: center;
  gap: 6px;
  height: 30px;
  padding: 0 10px;
  border: 1px solid var(--ui-border);
  border-radius: var(--ui-radius-sm);
  background: var(--ui-surface);
  color: var(--ui-text-muted);
  font-size: 12.5px;
  font-weight: 500;
  white-space: nowrap;
  text-decoration: none;
  transition:
    color var(--ui-duration-fast),
    border-color var(--ui-duration-fast),
    background var(--ui-duration-fast);

  .n {
    color: var(--ui-text-subtle);
    font-size: 11.5px;
    font-variant-numeric: tabular-nums;
  }

  &:hover {
    color: var(--ui-text);
    background: var(--ui-surface-hover);
  }

  &.router-link-exact-active {
    border-color: var(--ui-accent);
    color: var(--ui-accent);
    background: var(--ui-surface-hover);
  }
}

.skeleton-rows {
  display: flex;
  flex-direction: column;
  gap: 28px;
}

.sk-title {
  width: 140px;
  height: 16px;
  margin-bottom: 12px;
  border-radius: 4px;
  background: var(--ui-surface-raised);
}

.sk-cards {
  display: flex;
  gap: 16px;
  overflow: hidden;
}

.sk-card {
  flex: 0 0 156px;
  aspect-ratio: 2 / 3;
  border-radius: var(--ui-radius-md);
  background: linear-gradient(
    100deg,
    var(--ui-surface) 30%,
    var(--ui-surface-raised) 50%,
    var(--ui-surface) 70%
  );
  background-size: 200% 100%;
  animation: sk 1.4s ease infinite;
}

@keyframes sk {
  0% {
    background-position: 150% 0;
  }

  100% {
    background-position: -50% 0;
  }
}
</style>
