<script setup lang="ts">
import { useUiStore } from "@/stores/ui.ts";

const uiStore = useUiStore();
const { contextMenu } = storeToRefs(uiStore);

const menuRef = ref<HTMLElement>();
const showDeleteConfirm = ref(false);

// Close menu if user clicks outside
onClickOutside(menuRef, () => {
  uiStore.closeContextMenu();
});

// Watch menu open to reset confirm state
watch(
  () => contextMenu.value.open,
  (open) => {
    if (!open) {
      showDeleteConfirm.value = false;
    }
  },
);

function toggleFavorite() {
  if (!contextMenu.value.item) return;
  const item = contextMenu.value.item;
  item.isFavorite = !item.isFavorite;
  uiStore.showToast(item.isFavorite ? "已成功添加到收藏夹！" : "已从收藏夹中移除。", "success");
  uiStore.closeContextMenu();
}

function refreshMetadata() {
  if (!contextMenu.value.item) return;
  uiStore.showToast(
    `正在重新连接搜刮源并刷新【${contextMenu.value.item.title}】的元数据...`,
    "info",
  );
  uiStore.closeContextMenu();
}

function openEditMetadata() {
  if (!contextMenu.value.item) return;
  uiStore.openMetadataManager(contextMenu.value.item);
}

function deleteItem() {
  if (!contextMenu.value.item) return;
  // Simulating removal
  uiStore.showToast(
    `条目【${contextMenu.value.item.title}】已成功从本地物理磁盘及数据库中删除！`,
    "success",
  );
  uiStore.closeContextMenu();
}

useEventListener(window, "keydown", (e) => {
  if (e.key === "Escape" && contextMenu.value.open) {
    uiStore.closeContextMenu();
  }
});
</script>

<template>
  <div
    v-if="contextMenu.open"
    ref="menuRef"
    class="context-menu"
    role="menu"
    aria-label="快捷操作"
    :style="{
      top: `${contextMenu.y}px`,
      left: `${contextMenu.x}px`,
    }"
  >
    <!-- Regular Menu List -->
    <ul v-if="!showDeleteConfirm" class="menu-list">
      <li
        class="menu-item"
        tabindex="0"
        role="menuitem"
        @click="toggleFavorite"
        @keydown.enter="toggleFavorite"
        @keydown.space.prevent="toggleFavorite"
      >
        <span class="icon" aria-hidden="true">⭐</span>
        <span>{{ contextMenu.item?.isFavorite ? "取消收藏" : "加入收藏" }}</span>
      </li>
      <li
        class="menu-item"
        tabindex="0"
        role="menuitem"
        @click="refreshMetadata"
        @keydown.enter="refreshMetadata"
        @keydown.space.prevent="refreshMetadata"
      >
        <span class="icon" aria-hidden="true">🔄</span>
        <span>刷新元数据</span>
      </li>
      <li
        class="menu-item"
        tabindex="0"
        role="menuitem"
        @click="openEditMetadata"
        @keydown.enter="openEditMetadata"
        @keydown.space.prevent="openEditMetadata"
      >
        <span class="icon" aria-hidden="true">✏️</span>
        <span>修改元数据</span>
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
        <span>删除条目</span>
      </li>
    </ul>

    <!-- Inline Secondary Confirmation -->
    <div v-else class="confirm-box" role="dialog" aria-modal="true" aria-label="确认删除">
      <span class="confirm-title"
        ><span aria-hidden="true">⚠️ </span>确认永久删除视频及 NFO 元数据文件吗？</span
      >
      <div class="confirm-actions">
        <button class="confirm-btn cancel" type="button" @click="showDeleteConfirm = false">
          取消
        </button>
        <button class="confirm-btn confirm" type="button" @click="deleteItem">确认删除</button>
      </div>
    </div>
  </div>
</template>

<style scoped lang="scss">
.context-menu {
  position: fixed;
  z-index: 160;
  min-width: 170px;
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
}

.menu-separator {
  height: 1px;
  background: var(--fbz-color-line-soft);
  margin: 4px 0;
}

.confirm-box {
  padding: var(--fbz-space-3);
  width: 200px;
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
    }
  }
}
</style>
