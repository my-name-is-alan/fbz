<script setup lang="ts">
/**
 * 媒体库管理：已挂载媒体库卡片网格 + 新建入口。
 * 编辑/新建走全局 LibrarySettingsModal（uiStore.openLibraryEditor）。
 */
import { useLibraryStore } from "@/stores/library.ts";
import { useUiStore } from "@/stores/ui.ts";

const libraryStore = useLibraryStore();
const uiStore = useUiStore();

const libraryTypeOptions = [
  { label: "电影 (Movie)", value: "movie" },
  { label: "电视剧 (TV Series)", value: "series" },
  { label: "动漫 (Anime)", value: "anime" },
  { label: "纪录片 (Documentary)", value: "documentary" },
  { label: "音乐 (Music)", value: "music" },
];

/** Library type → icon SVG path data and accent color */
const libTypeVisuals: Record<string, { icon: string; accent: string }> = {
  movie: {
    icon: "M2 2h20v20H2z M7 2v20 M17 2v20 M2 12h20 M2 7h5 M2 17h5 M17 17h5 M17 7h5",
    accent: "#0ea5e9",
  },
  series: {
    icon: "M2 7h20v15H2z M17 2l-5 5-5-5",
    accent: "#8b5cf6",
  },
  anime: {
    icon: "M12 2l3.09 6.26L22 9.27l-5 4.87L18.18 21 12 17.77 5.82 21 7 14.14l-5-4.87 6.91-1.01L12 2z",
    accent: "#f43f5e",
  },
  documentary: {
    icon: "M12 2a10 10 0 1 0 0 20 10 10 0 0 0 0-20z M2 12h20 M12 2a15.3 15.3 0 0 1 4 10 15.3 15.3 0 0 1-4 10 15.3 15.3 0 0 1-4-10 15.3 15.3 0 0 1 4-10z",
    accent: "#10b981",
  },
  music: {
    icon: "M9 18V5l12-2v13 M6 18a3 3 0 1 0 0-6 3 3 0 0 0 0 6z M18 16a3 3 0 1 0 0-6 3 3 0 0 0 0 6z",
    accent: "#f59e0b",
  },
};

function getLibTypeName(kind: string) {
  return libraryTypeOptions.find((o) => o.value === kind)?.label.split(" ")[0] ?? "未知";
}

function getLibVisuals(kind: string) {
  return libTypeVisuals[kind] ?? libTypeVisuals.movie;
}

function handleEditLibrary(lib: { id: string }) {
  uiStore.openLibraryEditor(lib.id);
}

function handleAddLibrary() {
  uiStore.openLibraryEditor(null);
}

onMounted(() => {
  if (!libraryStore.loaded) {
    void libraryStore.loadFromBackend();
  }
});
</script>

<template>
  <div class="lib-manager-view">
    <div class="section-label">
      <span class="label-text">已挂载影视媒体库</span>
      <span class="label-count">{{ libraryStore.libraries.length }}</span>
    </div>

    <div class="lib-cards-grid">
      <!-- Library cards -->
      <div
        v-for="lib in libraryStore.libraries"
        :key="lib.id"
        class="lib-preview-card"
        @click="handleEditLibrary(lib)"
        @contextmenu.prevent="
          uiStore.openLibraryContextMenu($event.clientX, $event.clientY, {
            id: lib.id,
            name: lib.name,
          })
        "
      >
        <div class="card-accent-bar" :style="{ background: getLibVisuals(lib.kind).accent }" />
        <div class="card-content">
          <div class="card-top">
            <span
              class="lib-icon-container"
              :style="{ '--icon-accent': getLibVisuals(lib.kind).accent }"
            >
              <svg
                viewBox="0 0 24 24"
                width="18"
                height="18"
                fill="none"
                stroke="currentColor"
                stroke-width="2"
                stroke-linecap="round"
                stroke-linejoin="round"
              >
                <path :d="getLibVisuals(lib.kind).icon" />
              </svg>
            </span>
            <div class="card-title-area">
              <span class="lib-name">{{ lib.name }}</span>
              <span class="lib-badge">{{ getLibTypeName(lib.kind) }}</span>
            </div>
            <div class="item-stat">
              <span class="num">{{ lib.count }}</span>
              <span class="lbl">条目</span>
            </div>
          </div>
          <div class="card-bottom">
            <svg
              viewBox="0 0 24 24"
              width="12"
              height="12"
              fill="none"
              stroke="currentColor"
              stroke-width="2"
              stroke-linecap="round"
              stroke-linejoin="round"
              class="path-icon"
            >
              <path
                d="M22 19a2 2 0 0 1-2 2H4a2 2 0 0 1-2-2V5a2 2 0 0 1 2-2h5l2 3h9a2 2 0 0 1 2 2z"
              />
            </svg>
            <span class="path-val">{{ lib.paths?.[0] || "未配置路径" }}</span>
            <svg
              viewBox="0 0 24 24"
              width="14"
              height="14"
              fill="none"
              stroke="currentColor"
              stroke-width="2"
              stroke-linecap="round"
              stroke-linejoin="round"
              class="edit-icon"
            >
              <path d="M11 4H4a2 2 0 0 0-2 2v14a2 2 0 0 0 2 2h14a2 2 0 0 0 2-2v-7" />
              <path d="M18.5 2.5a2.121 2.121 0 1 1 3 3L12 15l-4 1 1-4Z" />
            </svg>
          </div>
        </div>
      </div>

      <!-- Add Library placeholder card -->
      <button class="add-lib-card" type="button" @click="handleAddLibrary">
        <svg
          viewBox="0 0 24 24"
          width="24"
          height="24"
          fill="none"
          stroke="currentColor"
          stroke-width="1.5"
          stroke-linecap="round"
          stroke-linejoin="round"
        >
          <line x1="12" y1="5" x2="12" y2="19" />
          <line x1="5" y1="12" x2="19" y2="12" />
        </svg>
        <span>添加媒体库</span>
      </button>
    </div>
  </div>
</template>

<style scoped lang="scss">
.lib-manager-view {
  display: flex;
  flex-direction: column;
  gap: var(--fbz-space-5);
}

.section-label {
  display: flex;
  align-items: center;
  gap: 8px;

  .label-text {
    font-size: 13px;
    font-weight: 700;
    color: var(--fbz-color-text-soft);
    text-transform: uppercase;
    letter-spacing: 0.5px;
  }

  .label-count {
    font-family: var(--fbz-font-display);
    font-size: 11px;
    font-weight: 800;
    color: var(--fbz-color-text-muted);
    background: var(--fbz-color-panel-strong);
    border: 1px solid var(--fbz-color-line-soft);
    padding: 1px 8px;
    border-radius: var(--fbz-radius-round);
  }
}

.lib-cards-grid {
  display: grid;
  grid-template-columns: repeat(auto-fill, minmax(300px, 1fr));
  gap: var(--fbz-space-3);
}

.lib-preview-card {
  border: 1px solid var(--fbz-color-line-soft);
  background: var(--fbz-color-panel-strong);
  border-radius: var(--fbz-radius-card);
  cursor: pointer;
  transition: all var(--fbz-motion-base);
  overflow: hidden;
  position: relative;
  height: 160px;

  .card-accent-bar {
    height: 3px;
    width: 100%;
    opacity: 0.6;
    transition: opacity var(--fbz-motion-fast);
  }

  &:hover {
    border-color: var(--fbz-color-brand-500);
    box-shadow: 0 8px 24px rgba(0, 0, 0, 0.25);
    transform: translateY(-1px);

    .card-accent-bar {
      opacity: 1;
    }

    .edit-icon {
      opacity: 1;
      color: var(--fbz-color-brand-500);
    }
  }

  .card-content {
    padding: 16px 18px;
    display: flex;
    flex-direction: column;
    justify-content: space-between;
    position: relative;
    z-index: 2;
    height: 100%;
  }

  .card-top {
    display: flex;
    align-items: center;
    gap: 12px;
  }

  .lib-icon-container {
    width: 38px;
    height: 38px;
    background: var(--fbz-color-panel);
    border: 1px solid var(--fbz-color-line-soft);
    border-radius: var(--fbz-radius-control);
    display: grid;
    place-content: center;
    color: var(--icon-accent, var(--fbz-color-text-soft));
    flex-shrink: 0;
    transition: all var(--fbz-motion-fast);
  }

  .card-title-area {
    flex: 1;
    min-width: 0;
    display: flex;
    flex-direction: column;
    gap: 3px;

    .lib-name {
      font-size: 14px;
      font-weight: 700;
      color: var(--fbz-color-text);
      white-space: nowrap;
      overflow: hidden;
      text-overflow: ellipsis;
    }

    .lib-badge {
      font-size: 10px;
      font-weight: 700;
      color: var(--fbz-color-text-muted);
      text-transform: uppercase;
      letter-spacing: 0.5px;
    }
  }

  .item-stat {
    display: flex;
    flex-direction: column;
    align-items: flex-end;
    flex-shrink: 0;

    .num {
      font-family: var(--fbz-font-display);
      font-size: 16px;
      font-weight: 800;
      color: var(--fbz-color-text);
      line-height: 1;
    }

    .lbl {
      font-size: 9px;
      color: var(--fbz-color-text-muted);
      font-weight: 700;
      margin-top: 2px;
    }
  }

  .card-bottom {
    display: flex;
    align-items: center;
    gap: 6px;
    padding-top: 10px;
    border-top: 1px solid var(--fbz-color-line-soft);

    .path-icon {
      color: var(--fbz-color-text-muted);
      flex-shrink: 0;
      opacity: 0.6;
    }

    .path-val {
      flex: 1;
      font-size: 11px;
      color: var(--fbz-color-text-muted);
      white-space: nowrap;
      overflow: hidden;
      text-overflow: ellipsis;
    }

    .edit-icon {
      flex-shrink: 0;
      color: var(--fbz-color-text-muted);
      opacity: 0;
      transition: all var(--fbz-motion-fast);
    }
  }
}

/* Add Library placeholder card */
.add-lib-card {
  border: 1px dashed var(--fbz-color-line-bright);
  background: transparent;
  border-radius: var(--fbz-radius-card);
  display: flex;
  flex-direction: column;
  align-items: center;
  justify-content: center;
  gap: 10px;
  height: 160px;
  color: var(--fbz-color-text-muted);
  cursor: pointer;
  transition: all var(--fbz-motion-base);

  svg {
    opacity: 0.5;
    transition: all var(--fbz-motion-fast);
  }

  span {
    font-size: 12px;
    font-weight: 600;
  }

  &:hover {
    border-color: var(--fbz-color-brand-500);
    color: var(--fbz-color-brand-500);
    background: color-mix(in srgb, var(--fbz-color-brand-500) 3%, transparent);

    svg {
      opacity: 1;
    }
  }
}

@media (max-width: 768px) {
  .lib-cards-grid {
    grid-template-columns: 1fr;
  }
}
</style>
