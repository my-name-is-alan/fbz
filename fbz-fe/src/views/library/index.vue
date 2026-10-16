<script setup lang="ts">
import { useLibraryStore } from "@/stores/library.ts";
import { useUiStore } from "@/stores/ui.ts";
import { useAuthStore } from "@/stores/auth.ts";
import { fetchLibraryCover } from "@/service/modules/library.ts";
import { libraryRoute } from "@/utils/libraryRoute.ts";
import { LIBRARY_TYPES, libraryTypeMeta, normalizeLibraryType } from "@/utils/libraryType.ts";
import type { MediaLibrary } from "@/types/media.ts";

const libraryStore = useLibraryStore();
const uiStore = useUiStore();
const authStore = useAuthStore();
const { libraries, totalCount, loading, error } = storeToRefs(libraryStore);

const fmt = new Intl.NumberFormat("zh-CN");
/** 库 id → 封面 URL（有 ImageTags 才写入；失败保持空，继续图标占位）。 */
const covers = ref<Record<string, string>>({});
let coverGeneration = 0;

async function enrichCovers(libs: MediaLibrary[]) {
  const userId = authStore.userId;
  if (!userId || !libs.length) return;
  const gen = ++coverGeneration;
  const next: Record<string, string> = { ...covers.value };
  await Promise.all(
    libs.map(async (lib) => {
      if (next[lib.id]) return;
      try {
        const cover = await fetchLibraryCover(
          userId,
          lib.id,
          normalizeLibraryType(lib.libraryType)
            ? libraryTypeMeta(lib.libraryType).itemTypes
            : undefined,
        );
        if (cover) next[lib.id] = cover;
      } catch {
        // 单库封面失败不影响总览。
      }
    }),
  );
  if (gen !== coverGeneration) return;
  covers.value = next;
}

onMounted(() => {
  if (!libraryStore.loaded) void libraryStore.loadFromBackend();
});

watch(libraries, (libs) => void enrichCovers(libs), { immediate: true });

/** 库类型分布（顶部摘要；库本身保持服务端 / 后台「库排序」给出的顺序，不按类型重排）。 */
const typeSummary = computed(() => {
  const counts = new Map<string, number>();
  for (const lib of libraries.value) {
    const key = normalizeLibraryType(lib.libraryType) ?? "other";
    counts.set(key, (counts.get(key) ?? 0) + 1);
  }
  return [...LIBRARY_TYPES, "other" as const]
    .filter((key) => counts.has(key))
    .map((key) => ({
      key,
      meta: key === "other" ? libraryTypeMeta(undefined) : libraryTypeMeta(key),
      count: counts.get(key)!,
    }));
});

function onLibraryContextMenu(e: MouseEvent, lib: MediaLibrary) {
  if (!authStore.isAdmin) return;
  uiStore.openLibraryContextMenu(e.clientX, e.clientY, { id: lib.id, name: lib.name });
}

function coverOf(lib: MediaLibrary): string | undefined {
  return covers.value[lib.id] || lib.cover;
}
</script>

<template>
  <main class="c-page library-overview">
    <header class="page-head">
      <div>
        <h1 class="c-page-title">媒体库</h1>
        <p class="c-meta">{{ libraries.length }} 个库 · 共 {{ fmt.format(totalCount) }} 个条目</p>
      </div>
      <div v-if="authStore.isAdmin" class="head-actions">
        <button
          class="du-btn du-btn-sm du-btn-ghost"
          type="button"
          @click="uiStore.openLibraryEditor(null)"
        >
          <BaseIcon name="plus" :size="15" />新建媒体库
        </button>
        <RouterLink class="du-btn du-btn-sm du-btn-ghost" to="/admin/libraries">
          <BaseIcon name="settings" :size="15" />管理
        </RouterLink>
      </div>
    </header>

    <p v-if="loading" class="c-state">
      <BaseIcon name="loading" :size="16" class="c-spin" />正在加载媒体库…
    </p>
    <p v-else-if="error" class="c-state is-error">
      <BaseIcon name="error" :size="16" />{{ error }}
    </p>
    <BaseEmptyState
      v-else-if="!libraries.length"
      icon="library-big"
      title="还没有创建媒体库"
      description="先创建一个媒体库，选择服务器上后端可访问的目录，保存后系统会自动扫描并预提取元数据。"
    >
      <button
        v-if="authStore.isAdmin"
        class="du-btn du-btn-primary"
        type="button"
        @click="uiStore.openLibraryEditor(null)"
      >
        <BaseIcon name="plus" :size="16" />新建媒体库
      </button>
    </BaseEmptyState>

    <template v-else>
      <div class="type-summary">
        <span v-for="t in typeSummary" :key="t.key" class="ts-chip">
          <BaseIcon :name="t.meta.icon" :size="13" />{{ t.meta.label }}<b>{{ t.count }}</b>
        </span>
      </div>
      <div class="grid">
        <RouterLink
          v-for="(lib, i) in libraries"
          :key="lib.id"
          :to="libraryRoute(lib)"
          class="lib-card"
          @contextmenu.prevent="onLibraryContextMenu($event, lib)"
        >
          <div class="cover">
            <MediaPoster
              v-if="coverOf(lib)"
              :src="coverOf(lib)"
              :title="lib.name"
              ratio="wide"
              :variant="(i % 2) as 0 | 1"
            />
            <div v-else class="cover-ph">
              <BaseIcon
                :name="libraryTypeMeta(lib.libraryType).icon"
                :size="34"
                :stroke-width="1.3"
              />
            </div>
            <span class="type-badge">
              <BaseIcon
                :name="libraryTypeMeta(lib.libraryType).icon"
                :size="12"
                :stroke-width="2"
              />{{ libraryTypeMeta(lib.libraryType).label }}
            </span>
            <span v-if="lib.isHidden" class="hidden-badge" title="已隐藏">
              <BaseIcon name="eye-off" :size="12" />
            </span>
          </div>
          <div class="info">
            <span class="name">{{ lib.name }}</span>
            <span class="count">{{ fmt.format(lib.count) }}</span>
          </div>
        </RouterLink>
      </div>
    </template>
  </main>
</template>

<style scoped lang="scss">
.library-overview {
  display: flex;
  flex-direction: column;
  gap: 28px;
}

.page-head {
  display: flex;
  align-items: flex-end;
  justify-content: space-between;
  gap: 16px;

  .c-meta {
    margin: 4px 0 0;
  }
}

.head-actions {
  display: flex;
  gap: 6px;
}

.type-summary {
  display: flex;
  flex-wrap: wrap;
  gap: 6px;
  margin-bottom: -12px;
}

.ts-chip {
  display: inline-flex;
  align-items: center;
  gap: 5px;
  height: 26px;
  padding: 0 9px;
  border: 1px solid var(--ui-border);
  border-radius: var(--ui-radius-sm);
  background: var(--ui-surface);
  color: var(--ui-text-muted);
  font-size: 12px;

  b {
    color: var(--ui-text);
    font-weight: 600;
  }
}

.grid {
  display: grid;
  grid-template-columns: repeat(auto-fill, minmax(240px, 1fr));
  gap: 16px;
}

.lib-card {
  color: inherit;
  text-decoration: none;

  &:hover .cover {
    border-color: rgb(var(--ui-accent-channel) / 60%);
    transform: translateY(-2px);
  }

  &:hover .name {
    color: var(--ui-accent-hover);
  }
}

.cover {
  position: relative;
  aspect-ratio: 16 / 9;
  overflow: hidden;
  border: 1px solid var(--ui-border);
  border-radius: var(--ui-radius-md);
  background: var(--ui-surface-raised);
  transition:
    border-color var(--ui-duration-fast),
    transform var(--ui-duration-base) var(--ui-ease-out);

  :deep(.media-poster) {
    height: 100%;
  }
}

.cover-ph {
  position: absolute;
  inset: 0;
  display: grid;
  place-items: center;
  color: var(--ui-border-strong);
  background:
    radial-gradient(circle at 30% 20%, rgb(var(--ui-accent-channel) / 6%), transparent 60%),
    var(--ui-surface-raised);
}

.type-badge,
.hidden-badge {
  position: absolute;
  top: 8px;
  display: inline-flex;
  align-items: center;
  gap: 4px;
  height: 20px;
  padding: 0 6px;
  border-radius: 4px;
  background: rgb(0 0 0 / 62%);
  color: #fff;
  font-size: 10.5px;
  font-weight: 600;
  backdrop-filter: blur(6px);
}

.type-badge {
  left: 8px;
}

.hidden-badge {
  right: 8px;
}

.info {
  display: flex;
  align-items: baseline;
  justify-content: space-between;
  gap: 8px;
  padding: 8px 2px 0;

  .name {
    overflow: hidden;
    color: var(--ui-text);
    font-size: 13.5px;
    font-weight: 600;
    white-space: nowrap;
    text-overflow: ellipsis;
    transition: color var(--ui-duration-fast);
  }

  .count {
    flex: 0 0 auto;
    color: var(--ui-text-subtle);
    font-size: 12px;
    font-variant-numeric: tabular-nums;
  }
}

@media (max-width: 600px) {
  .grid {
    grid-template-columns: repeat(auto-fill, minmax(160px, 1fr));
    gap: 12px;
  }
}
</style>
