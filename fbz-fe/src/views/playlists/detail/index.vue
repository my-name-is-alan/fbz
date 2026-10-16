<script setup lang="ts">
import {
  addPlaylistItems,
  fetchPlaylistItems,
  listPlaylists,
  removePlaylistItems,
  renamePlaylist,
  fetchPlaylistSharing,
  fetchShareableUsers,
  setPlaylistPublic,
  sharePlaylist,
  unsharePlaylist,
  type PlaylistShare,
  type PlaylistSummary,
  type ShareableUser,
} from "@/service/modules/playlists.ts";
import { preparePlayback } from "@/service/modules/server.ts";
import { useAuthStore } from "@/stores/auth.ts";
import { usePlaybackStore } from "@/stores/playback.ts";
import { useUiStore } from "@/stores/ui.ts";
import type { ContinueItem } from "@/types/media.ts";
import type { ServerItem } from "@/service/modules/server.ts";
import { itemTypeMeta } from "@/utils/itemType.ts";

const route = useRoute();
const authStore = useAuthStore();
const playback = usePlaybackStore();
const ui = useUiStore();

const playlistId = computed(() => String(route.params.id ?? ""));
const playlist = ref<PlaylistSummary | null>(null);
const items = ref<ContinueItem[]>([]);
const serverItems = ref<ServerItem[]>([]);
const entryIds = ref<string[]>([]);
const total = ref(0);
const loading = ref(true);
const error = ref<string | null>(null);
const playing = ref(false);
const busy = ref(false);

const renameOpen = ref(false);
const renameName = ref("");
const shareOpen = ref(false);
const shareError = ref("");
const openAccess = ref(false);
const shares = ref<PlaylistShare[]>([]);
const shareableUsers = ref<ShareableUser[]>([]);
const shareTarget = ref("");
const shareCanEdit = ref(false);

function userName(id: string): string {
  return shareableUsers.value.find((u) => u.id === id)?.name ?? id;
}
const unsharedUsers = computed(() =>
  shareableUsers.value.filter((u) => !shares.value.some((s) => s.UserId === u.id)),
);

async function openShare() {
  shareOpen.value = true;
  shareError.value = "";
  try {
    const [sharing, users] = await Promise.all([
      fetchPlaylistSharing(playlistId.value),
      fetchShareableUsers(authStore.userId),
    ]);
    openAccess.value = sharing.openAccess;
    shares.value = sharing.shares;
    shareableUsers.value = users;
    shareTarget.value = "";
  } catch {
    shareError.value = "无法读取共享设置";
  }
}

async function shareAction(task: () => Promise<void>) {
  busy.value = true;
  shareError.value = "";
  try {
    await task();
    const sharing = await fetchPlaylistSharing(playlistId.value);
    openAccess.value = sharing.openAccess;
    shares.value = sharing.shares;
  } catch (err) {
    const status = (err as { response?: { status?: number } })?.response?.status;
    shareError.value = status === 403 ? "只有创建者可以管理共享" : "操作失败，请重试";
  } finally {
    busy.value = false;
  }
}

const togglePublic = () =>
  shareAction(() => setPlaylistPublic(playlistId.value, !openAccess.value));
const addShare = () =>
  shareTarget.value
    ? shareAction(() =>
        sharePlaylist(playlistId.value, shareTarget.value, shareCanEdit.value),
      ).then(() => {
        shareTarget.value = "";
      })
    : Promise.resolve();
const toggleCanEdit = (share: PlaylistShare) =>
  shareAction(() => sharePlaylist(playlistId.value, share.UserId, !share.CanEdit));
const removeShare = (share: PlaylistShare) =>
  shareAction(() => unsharePlaylist(playlistId.value, share.UserId));

const addOpen = ref(false);

/** 头部封面拼图：取前四个有海报的条目。 */
const mosaic = computed(() => items.value.filter((i) => i.poster).slice(0, 4));
const shareUserOptions = computed(() => [
  { label: unsharedUsers.value.length ? "选择用户" : "没有可共享的用户", value: "" },
  ...unsharedUsers.value.map((u) => ({ label: u.name, value: u.id })),
]);
const totalRuntime = computed(() => {
  const ticks = serverItems.value.reduce((sum, it) => sum + (it.RunTimeTicks ?? 0), 0);
  if (!ticks) return "";
  const minutes = Math.round(ticks / 600_000_000);
  return minutes >= 60
    ? `${Math.floor(minutes / 60)} 小时 ${minutes % 60} 分钟`
    : `${minutes} 分钟`;
});
const addIdsText = ref("");

function isPlayable(item: ServerItem): boolean {
  return item.Type === "Movie" || item.Type === "Episode" || item.Type === "Video";
}

async function load() {
  loading.value = true;
  error.value = null;
  playlist.value = null;
  items.value = [];
  serverItems.value = [];
  entryIds.value = [];
  total.value = 0;
  try {
    const userId = authStore.userId;
    if (!userId) {
      error.value = "请先登录";
      return;
    }
    const id = playlistId.value;
    if (!id) {
      error.value = "无效的播放列表";
      return;
    }
    const [lists, page] = await Promise.all([
      listPlaylists(userId),
      fetchPlaylistItems(id, userId),
    ]);
    playlist.value = lists.find((p) => p.id === id) ?? { id, name: "播放列表" };
    items.value = page.items;
    serverItems.value = page.serverItems;
    entryIds.value = page.entryIds;
    total.value = page.totalRecordCount;
  } catch {
    error.value = "无法打开该播放列表";
  } finally {
    loading.value = false;
  }
}

async function playServerItem(item: ServerItem) {
  if (!isPlayable(item)) {
    ui.showToast("该条目类型暂不支持直接播放", "error");
    return;
  }
  playing.value = true;
  try {
    const prepared = await preparePlayback(item);
    playback.open(prepared);
  } catch {
    ui.showToast("无法播放该条目，请稍后重试", "error");
  } finally {
    playing.value = false;
  }
}

async function playAll() {
  const first = serverItems.value.find(isPlayable);
  if (!first) {
    ui.showToast("列表中暂无可播放条目", "error");
    return;
  }
  await playServerItem(first);
}

async function playAt(index: number) {
  const item = serverItems.value[index];
  if (!item) return;
  await playServerItem(item);
}

function openRename() {
  renameName.value = playlist.value?.name ?? "";
  renameOpen.value = true;
}

async function submitRename() {
  const name = renameName.value.trim();
  if (!name) {
    ui.showToast("名称不能为空", "warning");
    return;
  }
  busy.value = true;
  try {
    const updated = await renamePlaylist(playlistId.value, name);
    playlist.value = updated;
    renameOpen.value = false;
    ui.showToast("已改名", "success");
  } catch {
    ui.showToast("改名失败", "error");
  } finally {
    busy.value = false;
  }
}

function openAdd() {
  addIdsText.value = "";
  addOpen.value = true;
}

async function submitAdd() {
  const ids = addIdsText.value
    .split(/[\s,;]+/)
    .map((s) => s.trim())
    .filter(Boolean);
  if (!ids.length) {
    ui.showToast("请输入至少一个媒体条目 Id", "warning");
    return;
  }
  busy.value = true;
  try {
    const added = await addPlaylistItems(playlistId.value, ids, authStore.userId ?? undefined);
    addOpen.value = false;
    ui.showToast(
      added > 0 ? `已添加 ${added} 条` : "未添加新条目（可能已存在或 Id 无效）",
      "success",
    );
    await load();
  } catch {
    ui.showToast("添加失败", "error");
  } finally {
    busy.value = false;
  }
}

async function removeAt(index: number) {
  const entryId = entryIds.value[index] || items.value[index]?.playlistItemId;
  if (!entryId) {
    ui.showToast("缺少 EntryId，无法删除该行", "error");
    return;
  }
  if (!window.confirm("从播放列表移除此条目？")) return;
  busy.value = true;
  try {
    await removePlaylistItems(playlistId.value, [entryId]);
    ui.showToast("已移除", "success");
    await load();
  } catch {
    ui.showToast("移除失败", "error");
  } finally {
    busy.value = false;
  }
}

watch(
  playlistId,
  () => {
    void load();
  },
  { immediate: true },
);
</script>

<template>
  <main class="c-page playlist-detail">
    <PageHeader
      :title="playlist?.name"
      :crumbs="[{ label: '播放列表', to: '/playlists', icon: 'playlist' }]"
      fallback="/playlists"
    />

    <p v-if="loading" class="c-state">
      <BaseIcon name="loading" :size="16" class="c-spin" />正在打开播放列表…
    </p>
    <p v-else-if="error" class="c-state is-error">
      <BaseIcon name="error" :size="16" />{{ error }}
    </p>

    <template v-else>
      <header class="hero">
        <div class="cover" :class="{ mosaic: mosaic.length >= 4 }">
          <template v-if="mosaic.length >= 4">
            <img v-for="m in mosaic" :key="m.id" :src="m.poster" alt="" loading="lazy" />
          </template>
          <img v-else-if="mosaic.length" :src="mosaic[0]!.poster" alt="" />
          <BaseIcon v-else name="playlist" :size="44" :stroke-width="1.3" />
        </div>
        <div class="hero-body">
          <span class="type-chip"><BaseIcon name="playlist" :size="12" />播放列表</span>
          <h1>{{ playlist?.name ?? "播放列表" }}</h1>
          <p class="c-meta">
            {{ total }} 个条目<template v-if="totalRuntime"> · 共 {{ totalRuntime }}</template>
          </p>
          <div class="actions">
            <button
              class="du-btn du-btn-primary act-main"
              type="button"
              :disabled="playing || busy || !serverItems.some(isPlayable)"
              @click="playAll"
            >
              <BaseIcon
                :name="playing ? 'loading' : 'play'"
                :size="16"
                :fill="!playing"
                :class="{ 'c-spin': playing }"
              />
              {{ playing ? "准备中…" : "播放" }}
            </button>
            <button class="du-btn act" type="button" :disabled="busy" @click="openRename">
              <BaseIcon name="edit" :size="15" />改名
            </button>
            <button class="du-btn act" type="button" :disabled="busy" @click="openAdd">
              <BaseIcon name="playlist-add" :size="15" />添加条目
            </button>
            <button class="du-btn act" type="button" :disabled="busy" @click="openShare">
              <BaseIcon name="share" :size="15" />共享
            </button>
          </div>
        </div>
      </header>

      <section class="items">
        <h2 class="c-section-title"><BaseIcon name="list" :size="16" class="lead" />列表内容</h2>
        <BaseEmptyState
          v-if="!items.length"
          compact
          icon="playlist"
          title="列表为空"
          description="点击「添加条目」粘贴媒体 Id，或从媒体卡片右键「加入播放列表」。"
        />
        <ol v-else class="track-list">
          <li v-for="(item, i) in items" :key="`${entryIds[i] || item.id}-${i}`" class="track">
            <span class="idx">{{ i + 1 }}</span>
            <button
              class="track-play"
              type="button"
              :disabled="playing || busy || !serverItems[i] || !isPlayable(serverItems[i]!)"
              :aria-label="`播放 ${item.title}`"
              @click="playAt(i)"
            >
              <BaseIcon name="play" :size="14" fill />
            </button>
            <div class="track-poster">
              <MediaPoster
                :src="item.poster"
                :title="item.title"
                ratio="poster"
                :icon="itemTypeMeta(serverItems[i]?.Type).icon"
              />
            </div>
            <div class="track-info">
              <span class="track-title">{{ item.title }}</span>
              <span class="track-meta c-meta">
                <BaseIcon :name="itemTypeMeta(serverItems[i]?.Type).icon" :size="12" />
                {{
                  [itemTypeMeta(serverItems[i]?.Type).label, item.meta].filter(Boolean).join(" · ")
                }}
              </span>
            </div>
            <span v-if="item.isPlayed" class="played" title="已看">
              <BaseIcon name="check" :size="14" :stroke-width="2.6" />
            </span>
            <RouterLink
              v-if="item.detailType"
              class="du-btn du-btn-ghost du-btn-xs"
              :to="`/${item.detailType}/${item.id}`"
            >
              详情
            </RouterLink>
            <button
              class="track-remove"
              type="button"
              :disabled="busy"
              :aria-label="`移除 ${item.title}`"
              @click="removeAt(i)"
            >
              <BaseIcon name="trash" :size="15" />
            </button>
          </li>
        </ol>
      </section>
    </template>

    <BaseModal
      :open="renameOpen"
      title="改名"
      description="更新此播放列表的显示名称。"
      @close="renameOpen = false"
    >
      <label class="field">
        <span class="field-label">名称</span>
        <input
          v-model="renameName"
          class="du-input field-input"
          type="text"
          maxlength="256"
          @keydown.enter.prevent="submitRename"
        />
      </label>
      <template #actions>
        <button
          class="du-btn du-btn-ghost"
          type="button"
          :disabled="busy"
          @click="renameOpen = false"
        >
          取消
        </button>
        <button class="du-btn du-btn-primary" type="button" :disabled="busy" @click="submitRename">
          {{ busy ? "保存中…" : "保存" }}
        </button>
      </template>
    </BaseModal>

    <BaseModal
      :open="shareOpen"
      title="共享"
      description="公开后所有用户可见；也可单独共享给指定用户，并决定对方能否编辑。"
      @close="shareOpen = false"
    >
      <label class="check-row">
        <input
          class="du-toggle du-toggle-sm du-toggle-primary"
          type="checkbox"
          :checked="openAccess"
          :disabled="busy"
          @change="togglePublic"
        />
        <span>公开给所有用户（只读）</span>
      </label>
      <div class="field">
        <span class="field-label">已共享</span>
        <p v-if="!shares.length" class="c-meta">尚未共享给任何人。</p>
        <ul v-else class="share-list">
          <li v-for="share in shares" :key="share.UserId" class="share-row">
            <BaseIcon name="user" :size="15" />
            <span class="share-name">{{ userName(share.UserId) }}</span>
            <label class="check-row">
              <input
                class="du-checkbox du-checkbox-sm"
                type="checkbox"
                :checked="share.CanEdit"
                :disabled="busy"
                @change="toggleCanEdit(share)"
              />
              可编辑
            </label>
            <button
              class="du-btn du-btn-ghost du-btn-xs"
              type="button"
              :disabled="busy"
              @click="removeShare(share)"
            >
              <BaseIcon name="close" :size="13" />移除
            </button>
          </li>
        </ul>
      </div>
      <div class="field">
        <span class="field-label">共享给</span>
        <div class="share-add">
          <BaseSelect
            v-model="shareTarget"
            :options="shareUserOptions"
            size="sm"
            aria-label="选择共享用户"
          />
          <label class="check-row">
            <input
              v-model="shareCanEdit"
              class="du-checkbox du-checkbox-sm"
              type="checkbox"
            />可编辑
          </label>
        </div>
      </div>
      <p v-if="shareError" class="c-state is-error">{{ shareError }}</p>
      <template #actions>
        <button class="du-btn du-btn-ghost" type="button" @click="shareOpen = false">关闭</button>
        <button
          class="du-btn du-btn-primary"
          type="button"
          :disabled="busy || !shareTarget"
          @click="addShare"
        >
          <BaseIcon name="share" :size="15" />{{ busy ? "保存中…" : "共享" }}
        </button>
      </template>
    </BaseModal>

    <BaseModal
      :open="addOpen"
      title="添加条目"
      description="粘贴一个或多个媒体条目 public Id（逗号/空格分隔）。也可从媒体卡片右键加入。"
      @close="addOpen = false"
    >
      <label class="field">
        <span class="field-label">条目 Id</span>
        <textarea
          v-model="addIdsText"
          class="du-textarea field-input"
          rows="3"
          placeholder="uuid-1, uuid-2"
        />
      </label>
      <template #actions>
        <button class="du-btn du-btn-ghost" type="button" :disabled="busy" @click="addOpen = false">
          取消
        </button>
        <button class="du-btn du-btn-primary" type="button" :disabled="busy" @click="submitAdd">
          {{ busy ? "添加中…" : "添加" }}
        </button>
      </template>
    </BaseModal>
  </main>
</template>

<style scoped lang="scss">
.playlist-detail {
  display: flex;
  flex-direction: column;
  gap: 24px;
}

.hero {
  display: flex;
  align-items: flex-end;
  gap: 24px;
  padding: 20px;
  border: 1px solid var(--ui-border);
  border-radius: var(--ui-radius-lg);
  background:
    radial-gradient(circle at 0% 0%, rgb(var(--ui-accent-channel) / 10%), transparent 60%),
    var(--ui-surface);
}

.cover {
  flex: 0 0 auto;
  width: 180px;
  height: 180px;
  display: grid;
  place-items: center;
  overflow: hidden;
  border: 1px solid var(--ui-border);
  border-radius: var(--ui-radius-md);
  background: var(--ui-surface-raised);
  color: var(--ui-text-subtle);
  box-shadow: var(--ui-shadow-surface);

  img {
    width: 100%;
    height: 100%;
    object-fit: cover;
  }

  &.mosaic {
    grid-template-columns: 1fr 1fr;
    grid-template-rows: 1fr 1fr;
    place-items: stretch;
  }
}

.hero-body {
  flex: 1;
  min-width: 0;

  h1 {
    margin: 8px 0 4px;
    color: var(--ui-text);
    font-size: clamp(24px, 3vw, 34px);
    font-weight: 700;
    letter-spacing: -0.02em;
  }
}

.type-chip {
  display: inline-flex;
  align-items: center;
  gap: 4px;
  height: 22px;
  padding: 0 8px;
  border: 1px solid var(--ui-border-strong);
  border-radius: 5px;
  color: var(--ui-text-muted);
  font-size: 11.5px;
  font-weight: 600;
}

.actions {
  display: flex;
  flex-wrap: wrap;
  gap: 8px;
  margin-top: 16px;
}

.act-main,
.act {
  height: 38px;
  min-height: 38px;
  border-radius: var(--ui-radius-md);
}

.act {
  border: 1px solid var(--ui-border-strong);
  background: transparent;
  color: var(--ui-text);

  &:hover {
    background: var(--ui-surface-hover);
  }
}

.items {
  display: flex;
  flex-direction: column;
  gap: 12px;

  .lead {
    color: var(--ui-text-subtle);
  }
}

.track-list {
  margin: 0;
  padding: 0;
  overflow: hidden;
  border: 1px solid var(--ui-border);
  border-radius: var(--ui-radius-lg);
  background: var(--ui-surface);
  list-style: none;
}

.track {
  display: flex;
  align-items: center;
  gap: 12px;
  padding: 8px 12px;
  border-bottom: 1px solid var(--ui-border);

  &:last-child {
    border-bottom: 0;
  }

  &:hover {
    background: var(--ui-surface-hover);
  }
}

.idx {
  width: 24px;
  flex: 0 0 auto;
  color: var(--ui-text-subtle);
  font-family: var(--ui-font-mono);
  font-size: 12px;
  text-align: right;
}

.track-play {
  width: 30px;
  height: 30px;
  flex: 0 0 auto;
  display: grid;
  place-items: center;
  padding-left: 2px;
  border: 0;
  border-radius: 50%;
  background: var(--ui-accent);
  color: var(--ui-accent-contrast);
  cursor: pointer;

  &:hover:not(:disabled) {
    background: var(--ui-accent-hover);
  }

  &:disabled {
    opacity: 0.3;
    cursor: default;
  }
}

.track-poster {
  width: 40px;
  flex: 0 0 auto;
  overflow: hidden;
  border: 1px solid var(--ui-border);
  border-radius: 5px;
}

.track-info {
  flex: 1;
  min-width: 0;
  display: flex;
  flex-direction: column;
  gap: 2px;
}

.track-title {
  overflow: hidden;
  color: var(--ui-text);
  font-size: 13.5px;
  font-weight: 600;
  white-space: nowrap;
  text-overflow: ellipsis;
}

.track-meta {
  display: inline-flex;
  align-items: center;
  gap: 4px;
}

.played {
  width: 22px;
  height: 22px;
  flex: 0 0 auto;
  display: grid;
  place-items: center;
  border-radius: 50%;
  background: var(--ui-accent);
  color: var(--ui-accent-contrast);
}

.track-remove {
  width: 30px;
  height: 30px;
  flex: 0 0 auto;
  display: grid;
  place-items: center;
  border: 0;
  border-radius: var(--ui-radius-sm);
  background: transparent;
  color: var(--ui-text-subtle);
  cursor: pointer;

  &:hover:not(:disabled) {
    background: rgb(251 113 133 / 10%);
    color: var(--ui-danger);
  }
}

.field {
  display: flex;
  flex-direction: column;
  gap: 6px;
  margin-top: 12px;
}

.field-label {
  color: var(--ui-text-muted);
  font-size: 12px;
  font-weight: 600;
}

.field-input {
  width: 100%;
}

.check-row {
  display: inline-flex;
  align-items: center;
  gap: 8px;
  font-size: 13px;
}

.share-list {
  display: flex;
  flex-direction: column;
  gap: 6px;
  margin: 0;
  padding: 0;
  list-style: none;
}

.share-row {
  display: flex;
  align-items: center;
  gap: 10px;
  padding: 6px 8px;
  border: 1px solid var(--ui-border);
  border-radius: var(--ui-radius-sm);
}

.share-name {
  flex: 1;
  min-width: 0;
  overflow: hidden;
  white-space: nowrap;
  text-overflow: ellipsis;
}

.share-add {
  display: flex;
  align-items: center;
  gap: 10px;

  :deep(.base-select) {
    flex: 1;
  }
}

@media (max-width: 600px) {
  .hero {
    flex-direction: column;
    align-items: flex-start;
  }

  .cover {
    width: 140px;
    height: 140px;
  }

  .idx {
    display: none;
  }
}
</style>
