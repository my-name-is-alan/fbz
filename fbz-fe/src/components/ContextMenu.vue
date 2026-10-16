<script setup lang="ts">
import { useUiStore } from "@/stores/ui.ts";
import { useAuthStore } from "@/stores/auth.ts";
import { usePlaybackStore } from "@/stores/playback.ts";
import { queueItemMetadataRefresh } from "@/service/modules/admin.ts";
import { setFavorite, setPlayed } from "@/service/modules/userData.ts";
import { deleteMediaItem } from "@/service/modules/detail.ts";
import {
  addPlaylistItems,
  createPlaylist,
  listPlaylists,
  type PlaylistSummary,
} from "@/service/modules/playlists.ts";
import {
  addCollectionItems,
  createCollection,
  listCollectionSummaries,
  type CollectionSummary,
} from "@/service/modules/collections.ts";

const router = useRouter();
const uiStore = useUiStore();
const authStore = useAuthStore();
const playback = usePlaybackStore();
const { contextMenu } = storeToRefs(uiStore);
const { isAdmin } = storeToRefs(authStore);

const menuRef = ref<HTMLElement>();
const showDeleteConfirm = ref(false);
const showPlaylistPicker = ref(false);
const showCollectionPicker = ref(false);
const playlistChoices = ref<PlaylistSummary[]>([]);
const playlistLoading = ref(false);
const collectionChoices = ref<CollectionSummary[]>([]);
const collectionLoading = ref(false);
const busyAction = ref<
  "favorite" | "played" | "refresh" | "play" | "delete" | "playlist" | "collection" | null
>(null);

const canResume = computed(() => {
  const item = contextMenu.value.item;
  return Boolean(item && !item.isPlayed && item.progress != null && item.progress > 0);
});

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
      showPlaylistPicker.value = false;
      showCollectionPicker.value = false;
      playlistChoices.value = [];
      collectionChoices.value = [];
    }
  },
);

async function toggleFavorite() {
  if (!contextMenu.value.item || busyAction.value) return;
  const item = contextMenu.value.item;
  if (!authStore.userId) {
    uiStore.showToast("请先登录后再修改收藏状态。", "warning");
    return;
  }

  const nextFavorite = !item.isFavorite;
  busyAction.value = "favorite";
  try {
    await setFavorite(authStore.userId, item.id, nextFavorite);
    item.isFavorite = nextFavorite;
    uiStore.showToast(nextFavorite ? "已成功添加到收藏夹。" : "已从收藏夹中移除。", "success");
    uiStore.closeContextMenu();
  } catch {
    uiStore.showToast("收藏状态更新失败，请检查网络与权限。", "error");
  } finally {
    busyAction.value = null;
  }
}

async function togglePlayed() {
  if (!contextMenu.value.item || busyAction.value) return;
  const item = contextMenu.value.item;
  if (!authStore.userId) {
    uiStore.showToast("请先登录后再修改观看状态。", "warning");
    return;
  }

  const nextPlayed = !item.isPlayed;
  busyAction.value = "played";
  try {
    await setPlayed(authStore.userId, item.id, nextPlayed);
    item.isPlayed = nextPlayed;
    // 与 PlayedItems 联动：标已看清进度条；标未看保留原进度（若有）。
    if (nextPlayed) item.progress = undefined;
    uiStore.showToast(nextPlayed ? "已标记为已看。" : "已标记为未看。", "success");
    uiStore.closeContextMenu();
  } catch {
    uiStore.showToast("观看状态更新失败，请检查网络与权限。", "error");
  } finally {
    busyAction.value = null;
  }
}

/** 右键「播放 / 继续播放」：剧集进详情由续看逻辑接管；电影直接起播。 */
async function playFromMenu(resume: boolean) {
  const item = contextMenu.value.item;
  if (!item || busyAction.value) return;
  const detailType = item.detailType ?? (item.libraryId === "series" ? "tv" : "movie");
  uiStore.closeContextMenu();

  if (detailType === "tv") {
    void router.push(`/tv/${item.id}`);
    return;
  }

  busyAction.value = "play";
  try {
    const { fetchPlaybackSource } = await import("@/service/modules/detail.ts");
    const source = await fetchPlaybackSource(String(item.id));
    if (!source?.uri) {
      uiStore.showToast("该条目暂无可播放媒体源。", "warning");
      return;
    }
    playback.open({
      type: "movie",
      id: String(item.id),
      title: item.title,
      subtitle: item.meta,
      poster: item.poster,
      tags: item.resolution ? [item.resolution] : undefined,
      duration: source.duration,
      startPositionSeconds: resume ? undefined : 0,
      source,
      server: source.server,
      mediaSources: source.mediaSources,
      chapters: source.chapters,
    });
  } catch {
    uiStore.showToast("无法开始播放，请稍后重试。", "error");
  } finally {
    busyAction.value = null;
  }
}

async function refreshMetadata() {
  if (!contextMenu.value.item || busyAction.value || !isAdmin.value) return;
  const item = contextMenu.value.item;
  busyAction.value = "refresh";
  try {
    await queueItemMetadataRefresh(item.id, "manual-context-menu");
    uiStore.showToast(`已将【${item.title}】加入元数据刷新队列。`, "success");
    uiStore.closeContextMenu();
  } catch {
    uiStore.showToast("刷新元数据入队失败，请确认当前账号具备管理权限。", "error");
  } finally {
    busyAction.value = null;
  }
}

function openEditMetadata() {
  if (!contextMenu.value.item || !isAdmin.value) return;
  uiStore.openMetadataManager(contextMenu.value.item);
}

function openIdentify() {
  if (!contextMenu.value.item || !isAdmin.value) return;
  uiStore.openIdentifyManager(contextMenu.value.item);
}

async function deleteItem() {
  if (!contextMenu.value.item || !isAdmin.value || busyAction.value) return;
  const item = contextMenu.value.item;
  busyAction.value = "delete";
  try {
    await deleteMediaItem(item.id, false);
    uiStore.showToast(`已删除「${item.title}」`, "success");
    uiStore.closeContextMenu();
    // 通知列表页刷新：抛出自定义事件；库页/首页可监听。
    window.dispatchEvent(new CustomEvent("fbz:item-deleted", { detail: { id: item.id } }));
  } catch {
    uiStore.showToast("删除失败，请稍后重试", "error");
  } finally {
    busyAction.value = null;
    showDeleteConfirm.value = false;
  }
}

async function openPlaylistPicker() {
  if (!contextMenu.value.item || busyAction.value) return;
  if (!authStore.userId) {
    uiStore.showToast("请先登录后再加入播放列表。", "warning");
    return;
  }
  showPlaylistPicker.value = true;
  playlistLoading.value = true;
  try {
    playlistChoices.value = await listPlaylists(authStore.userId);
  } catch {
    playlistChoices.value = [];
    uiStore.showToast("无法加载播放列表。", "error");
  } finally {
    playlistLoading.value = false;
  }
}

async function addToPlaylist(playlist: PlaylistSummary) {
  const item = contextMenu.value.item;
  if (!item || busyAction.value) return;
  busyAction.value = "playlist";
  try {
    await addPlaylistItems(playlist.id, [String(item.id)], authStore.userId ?? undefined);
    uiStore.showToast(`已加入「${playlist.name}」`, "success");
    uiStore.closeContextMenu();
  } catch {
    uiStore.showToast("加入播放列表失败。", "error");
  } finally {
    busyAction.value = null;
  }
}

async function createAndAddPlaylist() {
  const item = contextMenu.value.item;
  if (!item || busyAction.value) return;
  const name = window.prompt("新建播放列表名称", item.title);
  if (!name?.trim()) return;
  busyAction.value = "playlist";
  try {
    const created = await createPlaylist({ name: name.trim(), ids: [String(item.id)] });
    uiStore.showToast(`已创建并加入「${created.name}」`, "success");
    uiStore.closeContextMenu();
  } catch {
    uiStore.showToast("创建播放列表失败。", "error");
  } finally {
    busyAction.value = null;
  }
}

/** 合集写操作后端限管理员；与播放列表 picker 同交互。 */
async function openCollectionPicker() {
  if (!contextMenu.value.item || busyAction.value) return;
  if (!isAdmin.value) {
    uiStore.showToast("加入合集需要管理员权限。", "warning");
    return;
  }
  if (!authStore.userId) {
    uiStore.showToast("请先登录后再加入合集。", "warning");
    return;
  }
  showCollectionPicker.value = true;
  collectionLoading.value = true;
  try {
    collectionChoices.value = await listCollectionSummaries(authStore.userId);
  } catch {
    collectionChoices.value = [];
    uiStore.showToast("无法加载合集列表。", "error");
  } finally {
    collectionLoading.value = false;
  }
}

async function addToCollection(collection: CollectionSummary) {
  const item = contextMenu.value.item;
  if (!item || busyAction.value) return;
  busyAction.value = "collection";
  try {
    await addCollectionItems(collection.id, [String(item.id)]);
    uiStore.showToast(`已加入合集「${collection.name}」`, "success");
    uiStore.closeContextMenu();
  } catch {
    uiStore.showToast("加入合集失败，请确认管理员权限。", "error");
  } finally {
    busyAction.value = null;
  }
}

async function createAndAddCollection() {
  const item = contextMenu.value.item;
  if (!item || busyAction.value) return;
  const name = window.prompt("新建合集名称", item.title);
  if (!name?.trim()) return;
  busyAction.value = "collection";
  try {
    const created = await createCollection({ name: name.trim(), ids: [String(item.id)] });
    uiStore.showToast(`已创建并加入合集「${created.name}」`, "success");
    uiStore.closeContextMenu();
  } catch {
    uiStore.showToast("创建合集失败，请确认管理员权限。", "error");
  } finally {
    busyAction.value = null;
  }
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
    <!-- Collection picker（管理员） -->
    <div
      v-if="showCollectionPicker"
      class="confirm-box"
      role="dialog"
      aria-modal="true"
      aria-label="加入合集"
    >
      <span class="confirm-title">加入合集</span>
      <p v-if="collectionLoading" class="picker-hint">加载中…</p>
      <ul v-else class="menu-list picker-list">
        <li
          v-for="col in collectionChoices"
          :key="col.id"
          class="menu-item"
          :class="{ disabled: busyAction === 'collection' }"
          tabindex="0"
          role="menuitem"
          @click="addToCollection(col)"
        >
          <span class="icon"><BaseIcon name="collection" :size="15" /></span>
          <span>{{ col.name }}</span>
        </li>
        <li class="menu-separator" aria-hidden="true" />
        <li
          class="menu-item"
          :class="{ disabled: busyAction === 'collection' }"
          tabindex="0"
          role="menuitem"
          @click="createAndAddCollection"
        >
          <span class="icon"><BaseIcon name="plus" :size="15" /></span>
          <span>新建并加入</span>
        </li>
        <li class="menu-item" tabindex="0" role="menuitem" @click="showCollectionPicker = false">
          <span class="icon"><BaseIcon name="back" :size="15" /></span>
          <span>返回</span>
        </li>
      </ul>
    </div>

    <!-- Playlist picker -->
    <div
      v-else-if="showPlaylistPicker"
      class="confirm-box"
      role="dialog"
      aria-modal="true"
      aria-label="加入播放列表"
    >
      <span class="confirm-title">加入播放列表</span>
      <p v-if="playlistLoading" class="picker-hint">加载中…</p>
      <ul v-else class="menu-list picker-list">
        <li
          v-for="pl in playlistChoices"
          :key="pl.id"
          class="menu-item"
          :class="{ disabled: busyAction === 'playlist' }"
          tabindex="0"
          role="menuitem"
          @click="addToPlaylist(pl)"
        >
          <span class="icon"><BaseIcon name="playlist" :size="15" /></span>
          <span>{{ pl.name }}</span>
        </li>
        <li class="menu-separator" aria-hidden="true" />
        <li
          class="menu-item"
          :class="{ disabled: busyAction === 'playlist' }"
          tabindex="0"
          role="menuitem"
          @click="createAndAddPlaylist"
        >
          <span class="icon"><BaseIcon name="plus" :size="15" /></span>
          <span>新建并加入</span>
        </li>
        <li class="menu-item" tabindex="0" role="menuitem" @click="showPlaylistPicker = false">
          <span class="icon"><BaseIcon name="back" :size="15" /></span>
          <span>返回</span>
        </li>
      </ul>
    </div>

    <!-- Regular Menu List -->
    <ul v-else-if="!showDeleteConfirm" class="menu-list">
      <li
        v-if="canResume"
        class="menu-item"
        :class="{ disabled: busyAction === 'play' }"
        tabindex="0"
        role="menuitem"
        @click="playFromMenu(true)"
        @keydown.enter="playFromMenu(true)"
        @keydown.space.prevent="playFromMenu(true)"
      >
        <span class="icon"><BaseIcon name="play" :size="15" /></span>
        <span>继续播放</span>
      </li>
      <li
        class="menu-item"
        :class="{ disabled: busyAction === 'play' }"
        tabindex="0"
        role="menuitem"
        @click="playFromMenu(false)"
        @keydown.enter="playFromMenu(false)"
        @keydown.space.prevent="playFromMenu(false)"
      >
        <span class="icon"><BaseIcon :name="canResume ? 'rewind' : 'play'" :size="15" /></span>
        <span>{{ canResume ? "从头播放" : "播放" }}</span>
      </li>
      <li class="menu-separator" aria-hidden="true" />
      <li
        class="menu-item"
        :class="{ disabled: busyAction === 'favorite' }"
        tabindex="0"
        role="menuitem"
        @click="toggleFavorite"
        @keydown.enter="toggleFavorite"
        @keydown.space.prevent="toggleFavorite"
      >
        <span class="icon"
          ><BaseIcon name="heart" :size="15" :fill="Boolean(contextMenu.item?.isFavorite)"
        /></span>
        <span>{{ contextMenu.item?.isFavorite ? "取消收藏" : "加入收藏" }}</span>
      </li>
      <li
        class="menu-item"
        :class="{ disabled: busyAction === 'playlist' }"
        tabindex="0"
        role="menuitem"
        @click="openPlaylistPicker"
        @keydown.enter="openPlaylistPicker"
        @keydown.space.prevent="openPlaylistPicker"
      >
        <span class="icon"><BaseIcon name="playlist" :size="15" /></span>
        <span>加入播放列表</span>
      </li>
      <li
        v-if="isAdmin"
        class="menu-item"
        :class="{ disabled: busyAction === 'collection' }"
        tabindex="0"
        role="menuitem"
        @click="openCollectionPicker"
        @keydown.enter="openCollectionPicker"
        @keydown.space.prevent="openCollectionPicker"
      >
        <span class="icon"><BaseIcon name="collection" :size="15" /></span>
        <span>加入合集</span>
      </li>
      <li
        class="menu-item"
        :class="{ disabled: busyAction === 'played' }"
        tabindex="0"
        role="menuitem"
        @click="togglePlayed"
        @keydown.enter="togglePlayed"
        @keydown.space.prevent="togglePlayed"
      >
        <span class="icon"
          ><BaseIcon :name="contextMenu.item?.isPlayed ? 'eye' : 'check'" :size="15"
        /></span>
        <span>{{ contextMenu.item?.isPlayed ? "标记未看" : "标记已看" }}</span>
      </li>

      <template v-if="isAdmin">
        <li
          class="menu-item"
          :class="{ disabled: busyAction === 'refresh' }"
          tabindex="0"
          role="menuitem"
          @click="refreshMetadata"
          @keydown.enter="refreshMetadata"
          @keydown.space.prevent="refreshMetadata"
        >
          <span class="icon"><BaseIcon name="refresh" :size="15" /></span>
          <span>{{ busyAction === "refresh" ? "正在入队…" : "刷新元数据" }}</span>
        </li>
        <li
          class="menu-item"
          tabindex="0"
          role="menuitem"
          @click="openIdentify"
          @keydown.enter="openIdentify"
          @keydown.space.prevent="openIdentify"
        >
          <span class="icon"><BaseIcon name="identify" :size="15" /></span>
          <span>识别</span>
        </li>
        <li
          class="menu-item"
          tabindex="0"
          role="menuitem"
          @click="openEditMetadata"
          @keydown.enter="openEditMetadata"
          @keydown.space.prevent="openEditMetadata"
        >
          <span class="icon"><BaseIcon name="edit" :size="15" /></span>
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
          <span class="icon"><BaseIcon name="trash" :size="15" /></span>
          <span>删除条目</span>
        </li>
      </template>
    </ul>

    <!-- Inline Secondary Confirmation -->
    <div v-else class="confirm-box" role="dialog" aria-modal="true" aria-label="确认删除">
      <span class="confirm-title"
        ><BaseIcon name="warning" :size="16" />确认永久删除视频及 NFO 元数据文件吗？</span
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

.picker-list {
  max-height: 240px;
  overflow: auto;
}

.picker-hint {
  margin: 8px 4px;
  font-size: 12px;
  color: var(--fbz-color-text-muted);
}

.menu-item {
  display: flex;
  align-items: center;
  gap: 10px;
  padding: 8px 12px;
  border-radius: 4px;
  font-size: var(--fbz-font-size-sm);
  font-weight: 500;
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
    display: inline-flex;
    width: 16px;
    color: var(--fbz-color-text-muted);
  }

  &:hover .icon,
  &:focus-visible .icon {
    color: inherit;
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
  width: 200px;
  display: flex;
  flex-direction: column;
  gap: var(--fbz-space-2);
  text-align: center;

  .confirm-title {
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: 6px;
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
