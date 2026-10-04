<script setup lang="ts">
import { useUiStore } from "@/stores/ui.ts";
import { useLibraryStore } from "@/stores/library.ts";
import {
  deleteLibrary,
  queueLibraryMetadataRefresh,
  queueLibraryScan,
} from "@/service/modules/admin.ts";

const uiStore = useUiStore();
const libraryStore = useLibraryStore();
const { libraryContextMenu } = storeToRefs(uiStore);

const menuRef = ref<HTMLElement>();
const showDeleteConfirm = ref(false);
const busyAction = ref<"scan" | "refresh" | "delete" | null>(null);

onClickOutside(menuRef, () => {
  uiStore.closeLibraryContextMenu();
});

watch(
  () => libraryContextMenu.value.open,
  (open) => {
    if (!open) {
      showDeleteConfirm.value = false;
      busyAction.value = null;
    }
  },
);

async function scanLibrary() {
  const library = libraryContextMenu.value.library;
  if (!library || busyAction.value) return;
  busyAction.value = "scan";
  try {
    await queueLibraryScan(library.id, "manual-context-menu");
    uiStore.showToast(`已将【${library.name}】加入扫描队列。`, "success");
    uiStore.closeLibraryContextMenu();
  } catch {
    uiStore.showToast("扫描入队失败，请确认当前账号具备管理权限。", "error");
  } finally {
    busyAction.value = null;
  }
}

async function refreshMetadata() {
  const library = libraryContextMenu.value.library;
  if (!library || busyAction.value) return;
  busyAction.value = "refresh";
  try {
    await queueLibraryMetadataRefresh(library.id, { reason: "manual-context-menu" });
    uiStore.showToast(`已将【${library.name}】的元数据刷新加入队列。`, "success");
    uiStore.closeLibraryContextMenu();
  } catch {
    uiStore.showToast("元数据刷新入队失败，请确认当前账号具备管理权限。", "error");
  } finally {
    busyAction.value = null;
  }
}

function editLibrary() {
  const library = libraryContextMenu.value.library;
  if (!library) return;
  uiStore.openLibraryEditor(library.id);
  uiStore.closeLibraryContextMenu();
}

async function removeLibrary() {
  const library = libraryContextMenu.value.library;
  if (!library || busyAction.value) return;
  busyAction.value = "delete";
  try {
    await deleteLibrary(library.id);
    uiStore.showToast(`已删除媒体库【${library.name}】。`, "success");
    uiStore.closeLibraryContextMenu();
    void libraryStore.loadFromBackend();
  } catch {
    uiStore.showToast("删除媒体库失败，请确认当前账号具备管理权限。", "error");
  } finally {
    busyAction.value = null;
  }
}

useEventListener(window, "keydown", (e) => {
  if (e.key === "Escape" && libraryContextMenu.value.open) {
    uiStore.closeLibraryContextMenu();
  }
});
</script>

<template>
  <div
    v-if="libraryContextMenu.open"
    ref="menuRef"
    class="context-menu"
    role="menu"
    aria-label="媒体库操作"
    :style="{
      top: `${libraryContextMenu.y}px`,
      left: `${libraryContextMenu.x}px`,
    }"
  >
    <ul v-if="!showDeleteConfirm" class="menu-list">
      <li
        class="menu-item"
        :class="{ disabled: busyAction === 'scan' }"
        tabindex="0"
        role="menuitem"
        @click="scanLibrary"
        @keydown.enter="scanLibrary"
        @keydown.space.prevent="scanLibrary"
      >
        <span class="icon" aria-hidden="true">🔍</span>
        <span>{{ busyAction === "scan" ? "正在入队…" : "扫描媒体库" }}</span>
      </li>
      <li
        class="menu-item"
        :class="{ disabled: busyAction === 'refresh' }"
        tabindex="0"
        role="menuitem"
        @click="refreshMetadata"
        @keydown.enter="refreshMetadata"
        @keydown.space.prevent="refreshMetadata"
      >
        <span class="icon" aria-hidden="true">🔄</span>
        <span>{{ busyAction === "refresh" ? "正在入队…" : "刷新元数据" }}</span>
      </li>
      <li
        class="menu-item"
        tabindex="0"
        role="menuitem"
        @click="editLibrary"
        @keydown.enter="editLibrary"
        @keydown.space.prevent="editLibrary"
      >
        <span class="icon" aria-hidden="true">⚙️</span>
        <span>编辑设置</span>
      </li>

      <li class="menu-separator" aria-hidden="true" />

      <li
        class="menu-item danger-item"
        tabindex="0"
        role="menuitem"
        @click="showDeleteConfirm = true"
        @keydown.enter="showDeleteConfirm = true"
        @keydown.space.prevent="showDeleteConfirm = true"
      >
        <span class="icon" aria-hidden="true">🗑️</span>
        <span>删除媒体库</span>
      </li>
    </ul>

    <div v-else class="confirm-box" role="dialog" aria-modal="true" aria-label="确认删除媒体库">
      <span class="confirm-title">
        <span aria-hidden="true">⚠️ </span>确认删除媒体库？仅移除库记录，磁盘文件保留。
      </span>
      <div class="confirm-actions">
        <button class="confirm-btn cancel" type="button" @click="showDeleteConfirm = false">
          取消
        </button>
        <button
          class="confirm-btn confirm"
          type="button"
          :disabled="busyAction === 'delete'"
          @click="removeLibrary"
        >
          {{ busyAction === "delete" ? "删除中…" : "确认删除" }}
        </button>
      </div>
    </div>
  </div>
</template>

<style scoped lang="scss">
.context-menu {
  position: fixed;
  z-index: 160;
  min-width: 180px;
  background: color-mix(in srgb, var(--fbz-color-panel-elevated) 96%, transparent);
  border: 1px solid var(--fbz-color-line-bright);
  border-radius: var(--fbz-radius-control);
  box-shadow: var(--fbz-shadow-panel);
  backdrop-filter: blur(12px);
  -webkit-backdrop-filter: blur(12px);
  padding: 4px;
  animation: scale-in var(--fbz-motion-fast) cubic-bezier(0.16, 1, 0.3, 1);
  color: var(--fbz-color-text);
  font-family: var(--fbz-font-sans);
}

@keyframes scale-in {
  from {
    transform: scale(0.95);
    opacity: 0;
  }
  to {
    transform: scale(1);
    opacity: 1;
  }
}

.menu-list {
  list-style: none;
  margin: 0;
  padding: 0;
  display: flex;
  flex-direction: column;
  gap: 2px;
}

.menu-item {
  display: flex;
  align-items: center;
  gap: 10px;
  padding: 8px 12px;
  border-radius: 4px;
  font-size: var(--fbz-font-size-sm);
  font-weight: 600;
  color: var(--fbz-color-text-soft);
  cursor: pointer;
  user-select: none;
  transition: all var(--fbz-motion-fast);

  &:hover,
  &:focus-visible {
    background: var(--fbz-color-panel-strong);
    color: var(--fbz-color-text);
    outline: none;
  }

  &.danger-item {
    color: var(--fbz-color-danger-500);

    &:hover,
    &:focus-visible {
      background: color-mix(in srgb, var(--fbz-color-danger-500) 8%, transparent);
      color: var(--fbz-color-danger-500);
      outline: none;
    }
  }

  .icon {
    font-size: 14px;
  }

  &.disabled {
    pointer-events: none;
    opacity: 0.62;
  }
}

.menu-separator {
  height: 1px;
  background: var(--fbz-color-line-soft);
  margin: 4px 0;
}

.confirm-box {
  padding: var(--fbz-space-3);
  width: 220px;
  display: flex;
  flex-direction: column;
  gap: var(--fbz-space-2);
  text-align: center;

  .confirm-title {
    font-size: var(--fbz-font-size-xs);
    font-weight: 700;
    color: var(--fbz-color-danger-500);
    line-height: 1.4;
  }

  .confirm-actions {
    display: flex;
    gap: 6px;
  }

  .confirm-btn {
    flex: 1;
    height: 28px;
    border-radius: 4px;
    font-size: 11px;
    font-weight: 700;
    cursor: pointer;
    transition: all var(--fbz-motion-fast);

    &.cancel {
      background: var(--fbz-color-panel-strong);
      border: 1px solid var(--fbz-color-line);
      color: var(--fbz-color-text-soft);

      &:hover {
        background: var(--fbz-color-panel-elevated);
      }
    }

    &.confirm {
      background: var(--fbz-color-danger-500);
      border: 0;
      color: #ffffff;

      &:hover {
        background: color-mix(in srgb, var(--fbz-color-danger-500) 84%, black);
      }

      &:disabled {
        opacity: 0.6;
        cursor: default;
      }
    }
  }
}
</style>
