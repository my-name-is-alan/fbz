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
  <main class="playlist-detail">
    <PageHeader :title="playlist?.name" fallback="/playlists" />

    <p v-if="loading" class="state-text">正在打开播放列表…</p>
    <p v-else-if="error" class="state-text is-error">{{ error }}</p>

    <template v-else>
      <header class="hero">
        <div class="hero-ic" aria-hidden="true">♫</div>
        <div class="hero-body">
          <h1>{{ playlist?.name ?? "播放列表" }}</h1>
          <p class="meta">{{ total }} 个条目</p>
          <div class="actions">
            <button
              class="btn-play"
              type="button"
              :disabled="playing || busy || !serverItems.some(isPlayable)"
              @click="playAll"
            >
              {{ playing ? "准备中…" : "播放" }}
            </button>
            <button class="btn-secondary" type="button" :disabled="busy" @click="openRename">
              改名
            </button>
            <button class="btn-secondary" type="button" :disabled="busy" @click="openAdd">
              添加条目
            </button>
            <button class="btn-secondary" type="button" :disabled="busy" @click="openShare">
              共享
            </button>
          </div>
        </div>
      </header>

      <section class="items">
        <h2 class="section-title">列表内容</h2>
        <BaseEmptyState
          v-if="!items.length"
          icon="♫"
          title="列表为空"
          description="点击「添加条目」粘贴媒体 Id，或从媒体卡片右键「加入播放列表」。"
        />
        <ol v-else class="track-list">
          <li v-for="(item, i) in items" :key="`${entryIds[i] || item.id}-${i}`" class="track">
            <button class="track-play" type="button" :disabled="playing || busy" @click="playAt(i)">
              ▶
            </button>
            <MediaPoster
              class="track-poster"
              :src="item.poster"
              :title="item.title"
              ratio="poster"
              :variant="(i % 2) as 0 | 1"
            />
            <div class="track-info">
              <span class="track-title">{{ item.title }}</span>
              <span class="track-meta">{{ item.meta }}</span>
            </div>
            <RouterLink
              v-if="item.detailType"
              class="track-open"
              :to="`/${item.detailType}/${item.id}`"
            >
              详情
            </RouterLink>
            <button class="track-remove" type="button" :disabled="busy" @click="removeAt(i)">
              移除
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
          class="field-input"
          type="text"
          maxlength="256"
          @keydown.enter.prevent="submitRename"
        />
      </label>
      <template #actions>
        <button class="btn-ghost" type="button" :disabled="busy" @click="renameOpen = false">
          取消
        </button>
        <button class="btn-play" type="button" :disabled="busy" @click="submitRename">
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
      <label class="field share-row">
        <input type="checkbox" :checked="openAccess" :disabled="busy" @change="togglePublic" />
        <span class="field-label">公开给所有用户（只读）</span>
      </label>
      <div class="field">
        <span class="field-label">已共享</span>
        <p v-if="!shares.length" class="share-note">尚未共享给任何人。</p>
        <ul v-else class="share-list">
          <li v-for="share in shares" :key="share.UserId" class="share-row">
            <span class="share-name">{{ userName(share.UserId) }}</span>
            <label class="share-row">
              <input
                type="checkbox"
                :checked="share.CanEdit"
                :disabled="busy"
                @change="toggleCanEdit(share)"
              />
              可编辑
            </label>
            <button class="btn-ghost" type="button" :disabled="busy" @click="removeShare(share)">
              移除
            </button>
          </li>
        </ul>
      </div>
      <div class="field">
        <span class="field-label">共享给</span>
        <div class="share-row">
          <select
            v-model="shareTarget"
            class="field-input"
            :disabled="busy || !unsharedUsers.length"
          >
            <option value="">{{ unsharedUsers.length ? "选择用户" : "没有可共享的用户" }}</option>
            <option v-for="u in unsharedUsers" :key="u.id" :value="u.id">{{ u.name }}</option>
          </select>
          <label class="share-row"><input v-model="shareCanEdit" type="checkbox" />可编辑</label>
        </div>
      </div>
      <p v-if="shareError" class="share-note is-error">{{ shareError }}</p>
      <template #actions>
        <button class="btn-ghost" type="button" @click="shareOpen = false">关闭</button>
        <button class="btn-play" type="button" :disabled="busy || !shareTarget" @click="addShare">
          {{ busy ? "保存中…" : "共享" }}
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
          class="field-input field-area"
          rows="3"
          placeholder="uuid-1, uuid-2"
        />
      </label>
      <template #actions>
        <button class="btn-ghost" type="button" :disabled="busy" @click="addOpen = false">
          取消
        </button>
        <button class="btn-play" type="button" :disabled="busy" @click="submitAdd">
          {{ busy ? "添加中…" : "添加" }}
        </button>
      </template>
    </BaseModal>
  </main>
</template>

<style scoped lang="scss">
.playlist-detail {
  /* 顶栏 + 固定返回条（PageHeader：顶栏下 12px、高 36px）之下再留白；原用未定义的 --fbz-space-10 致 calc 失效、内容顶到导航栏。 */
  padding: calc(var(--header-h, 60px) + var(--fbz-space-8) * 2 + var(--fbz-space-4))
    var(--fbz-space-8) 80px;
  min-height: 100vh;
  max-width: 960px;
  margin: 0 auto;
}

.state-text {
  margin: var(--fbz-space-8) 0;
  color: var(--fbz-color-text-muted);
  text-align: center;

  &.is-error {
    color: var(--fbz-color-danger, #e57373);
  }
}

.hero {
  display: flex;
  gap: var(--fbz-space-5);
  align-items: center;
  margin-bottom: var(--fbz-space-8);
}

.hero-ic {
  width: 96px;
  height: 96px;
  border-radius: 12px;
  display: grid;
  place-content: center;
  font-size: 36px;
  background: var(--fbz-color-panel, rgba(255, 255, 255, 0.04));
  border: 1px solid var(--fbz-color-line);
  color: var(--fbz-color-brand-500);
  flex-shrink: 0;
}

.hero-body {
  h1 {
    margin: 0 0 var(--fbz-space-2);
    font-size: var(--fbz-font-size-xl);
    font-weight: 700;
  }

  .meta {
    margin: 0 0 var(--fbz-space-4);
    color: var(--fbz-color-text-muted);
    font-size: var(--fbz-font-size-sm);
  }
}

.actions {
  display: flex;
  flex-wrap: wrap;
  gap: var(--fbz-space-3);
}

.btn-play {
  appearance: none;
  border: none;
  border-radius: 999px;
  padding: 10px 22px;
  font-weight: 700;
  cursor: pointer;
  background: var(--fbz-color-brand-500);
  color: #fff;

  &:disabled {
    opacity: 0.5;
    cursor: not-allowed;
  }
}

.btn-secondary,
.btn-ghost {
  appearance: none;
  border: 1px solid var(--fbz-color-line);
  border-radius: 999px;
  padding: 10px 16px;
  font-weight: 600;
  cursor: pointer;
  background: transparent;
  color: inherit;

  &:disabled {
    opacity: 0.5;
    cursor: not-allowed;
  }
}

.section-title {
  margin: 0 0 var(--fbz-space-4);
  font-size: var(--fbz-font-size-lg);
  font-weight: 700;
}

.track-list {
  list-style: none;
  margin: 0;
  padding: 0;
  display: grid;
  gap: var(--fbz-space-2);
}

.track {
  display: grid;
  grid-template-columns: 36px 48px 1fr auto auto;
  gap: var(--fbz-space-3);
  align-items: center;
  padding: var(--fbz-space-3);
  border-radius: 8px;
  border: 1px solid transparent;

  &:hover {
    background: var(--fbz-color-panel, rgba(255, 255, 255, 0.04));
    border-color: var(--fbz-color-line);
  }
}

.track-play {
  appearance: none;
  width: 36px;
  height: 36px;
  border-radius: 50%;
  border: 1px solid var(--fbz-color-line);
  background: transparent;
  color: var(--fbz-color-text-soft);
  cursor: pointer;

  &:hover:not(:disabled) {
    color: var(--fbz-color-brand-500);
    border-color: var(--fbz-color-brand-500);
  }

  &:disabled {
    opacity: 0.5;
  }
}

/* 48px 缩略图放不下占位文字（「图片加载失败」会溢出），只留图标。 */
.track-poster :deep(.fallback-title),
.track-poster :deep(.fallback-tips) {
  display: none;
}

.track-poster {
  width: 48px;
  border-radius: 4px;
  overflow: hidden;
}

.track-info {
  display: flex;
  flex-direction: column;
  gap: 2px;
  min-width: 0;
}

.track-title {
  font-weight: 600;
  white-space: nowrap;
  overflow: hidden;
  text-overflow: ellipsis;
}

.track-meta {
  font-size: var(--fbz-font-size-xs);
  color: var(--fbz-color-text-muted);
}

.track-open {
  color: var(--fbz-color-brand-500);
  text-decoration: none;
  font-size: var(--fbz-font-size-sm);
  padding: 6px 8px;

  &:hover {
    text-decoration: underline;
  }
}

.track-remove {
  appearance: none;
  border: none;
  background: transparent;
  color: var(--fbz-color-text-muted);
  font-size: var(--fbz-font-size-sm);
  cursor: pointer;
  padding: 6px 8px;

  &:hover:not(:disabled) {
    color: var(--fbz-color-danger, #e57373);
  }

  &:disabled {
    opacity: 0.5;
  }
}

.field {
  display: grid;
  gap: var(--fbz-space-2);
}

.field-label {
  font-size: var(--fbz-font-size-sm);
  color: var(--fbz-color-text-muted);
}

.field-input {
  appearance: none;
  border: 1px solid var(--fbz-color-line);
  border-radius: 10px;
  padding: 10px 12px;
  background: var(--fbz-color-bg-strong, rgba(0, 0, 0, 0.2));
  color: inherit;
  font-size: var(--fbz-font-size-md, 15px);
  font-family: inherit;
}

.field-area {
  resize: vertical;
  min-height: 72px;
}

@media (max-width: 600px) {
  .playlist-detail {
    padding: calc(var(--header-h, 60px) + var(--fbz-space-8)) var(--fbz-space-4) 60px;
  }

  .hero-ic {
    width: 72px;
    height: 72px;
    font-size: 28px;
  }

  .track {
    grid-template-columns: 32px 40px 1fr auto;
    grid-template-rows: auto auto;

    .track-open {
      grid-column: 3;
    }

    .track-remove {
      grid-column: 4;
    }
  }
}

.share-row {
  display: flex;
  align-items: center;
  gap: 10px;
}

.share-list {
  list-style: none;
  margin: 0;
  padding: 0;
  display: grid;
  gap: 8px;
}

.share-name {
  flex: 1;
  font-weight: 600;
}

.share-note {
  margin: 0;
  color: var(--fbz-color-text-muted);
  font-size: var(--fbz-font-size-sm);

  &.is-error {
    color: var(--fbz-color-danger, #ef4444);
  }
}
</style>
