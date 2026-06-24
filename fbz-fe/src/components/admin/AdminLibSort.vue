<script setup lang="ts">
import { useLibraryStore } from "@/stores/library.ts";
import { useUiStore } from "@/stores/ui.ts";

const libraryStore = useLibraryStore();
const uiStore = useUiStore();

function moveUp(index: number) {
  if (index === 0) return;
  const libs = libraryStore.libraries;
  const temp = libs[index];
  libs[index] = libs[index - 1];
  libs[index - 1] = temp;
  uiStore.showToast("已调整媒体库顺序，前台排版已即时同步！", "success");
}

function moveDown(index: number) {
  const libs = libraryStore.libraries;
  if (index === libs.length - 1) return;
  const temp = libs[index];
  libs[index] = libs[index + 1];
  libs[index + 1] = temp;
  uiStore.showToast("已调整媒体库顺序，前台排版已即时同步！", "success");
}
</script>

<template>
  <div class="admin-lib-sort-view">
    <section class="settings-card">
      <div class="card-header">
        <span class="indicator" />
        <h3>媒体库显示顺序调整</h3>
      </div>
      <div class="card-body">
        <p class="settings-hint">调整媒体库在主系统首页海报墙分类和侧栏菜单中的出现顺序。</p>

        <div class="sort-list" role="list" aria-label="媒体库列表">
          <div
            v-for="(lib, idx) in libraryStore.libraries"
            :key="lib.id"
            class="sort-item"
            role="listitem"
          >
            <div class="item-left">
              <span class="drag-handle" aria-hidden="true">☰</span>
              <span class="lib-icon">📁</span>
              <div class="lib-info">
                <span class="name">{{ lib.name }}</span>
                <span class="kind">
                  {{
                    lib.kind === "series"
                      ? "电视剧"
                      : lib.kind === "movie"
                        ? "电影"
                        : lib.kind === "anime"
                          ? "动漫"
                          : lib.kind === "documentary"
                            ? "纪录片"
                            : "媒体库"
                  }}
                  · {{ lib.count }} 个条目
                </span>
              </div>
            </div>
            <div class="item-actions">
              <button
                class="sort-btn"
                type="button"
                :disabled="idx === 0"
                :aria-label="`向上移动 ${lib.name}`"
                @click="moveUp(idx)"
              >
                ▲ 上移
              </button>
              <button
                class="sort-btn"
                type="button"
                :disabled="idx === libraryStore.libraries.length - 1"
                :aria-label="`向下移动 ${lib.name}`"
                @click="moveDown(idx)"
              >
                ▼ 下移
              </button>
            </div>
          </div>
        </div>
      </div>
    </section>
  </div>
</template>

<style scoped lang="scss">
.admin-lib-sort-view {
  max-width: 800px;
}

.sort-list {
  display: flex;
  flex-direction: column;
  gap: var(--fbz-space-2);
}

.sort-item {
  display: flex;
  justify-content: space-between;
  align-items: center;
  padding: 12px var(--fbz-space-4);
  background: var(--fbz-color-panel);
  border: 1px solid var(--fbz-color-line);
  border-radius: 6px;
  transition: all var(--fbz-motion-fast);

  &:hover {
    border-color: var(--fbz-color-line-bright);
    background: var(--fbz-color-panel-strong);
  }

  .item-left {
    display: flex;
    align-items: center;
    gap: 12px;

    .drag-handle {
      color: var(--fbz-color-text-disabled);
      cursor: grab;
      font-size: 16px;
    }

    .lib-icon {
      font-size: 18px;
      color: var(--fbz-color-brand-500);
    }

    .lib-info {
      display: flex;
      flex-direction: column;
      gap: 2px;

      .name {
        font-size: 13px;
        font-weight: 700;
        color: var(--fbz-color-text);
      }

      .kind {
        font-size: 11px;
        color: var(--fbz-color-text-muted);
      }
    }
  }

  .item-actions {
    display: flex;
    gap: 8px;

    .sort-btn {
      height: 28px;
      padding: 0 10px;
      background: var(--fbz-color-panel-strong);
      border: 1px solid var(--fbz-color-line);
      color: var(--fbz-color-text-soft);
      font-size: var(--fbz-font-size-xs);
      font-weight: 700;
      border-radius: 4px;
      cursor: pointer;
      transition: all var(--fbz-motion-fast);

      &:hover:not(:disabled) {
        background: var(--fbz-color-panel-elevated);
        color: var(--fbz-color-text);
      }

      &:disabled {
        opacity: 0.3;
        cursor: not-allowed;
      }
    }
  }
}
</style>
