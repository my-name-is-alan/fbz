/**
 * SyncPlay 播放器接线：连 `/embywebsocket` 收 `SyncPlay*` 消息，归约状态，
 * 并在命令计划时刻（When）回调播放器执行 pause/unpause/seek/stop。
 * 本地操作在组内时改为发 REST，由服务端广播给全员（含自己）后统一执行。
 */
import { computed, ref } from "vue";
import { getAccessToken } from "@/service/request.ts";
import * as api from "@/service/modules/syncplay.ts";
import type { SyncPlayCommand, SyncPlayGroup } from "@/service/modules/syncplay.ts";

export interface SyncPlayPlayerHooks {
  apply: (command: SyncPlayCommand) => void;
}

export function websocketUrl(origin: string, token: string): string {
  const base = origin.replace(/^http/, "ws");
  return `${base}/embywebsocket?api_key=${encodeURIComponent(token)}`;
}

export function useSyncPlay(hooks: SyncPlayPlayerHooks) {
  const state = ref(api.emptySyncPlayState());
  const groups = ref<SyncPlayGroup[]>([]);
  const busy = ref(false);
  const error = ref("");
  let socket: WebSocket | null = null;
  let keepAlive: ReturnType<typeof setInterval> | undefined;
  const timers = new Set<ReturnType<typeof setTimeout>>();
  const active = computed(() => state.value.group !== null);

  function handleMessage(raw: unknown) {
    const before = state.value.pendingCommand;
    state.value = api.applySyncPlayMessage(state.value, raw);
    const command = state.value.pendingCommand;
    if (command && command !== before) {
      const timer = setTimeout(
        () => {
          timers.delete(timer);
          hooks.apply(command);
        },
        Math.max(0, api.msUntil(command)),
      );
      timers.add(timer);
    }
  }

  function connect() {
    if (socket || typeof WebSocket === "undefined") return;
    const token = getAccessToken();
    if (!token) return;
    socket = new WebSocket(websocketUrl(window.location.origin, token));
    socket.onmessage = (event) => {
      const text = String(event.data);
      if (text.includes('"ForceKeepAlive"') && !keepAlive) {
        keepAlive = setInterval(
          () => socket?.send(JSON.stringify({ MessageType: "KeepAlive" })),
          30_000,
        );
      }
      handleMessage(text);
    };
    socket.onclose = () => {
      socket = null;
      if (keepAlive) clearInterval(keepAlive);
      keepAlive = undefined;
    };
  }

  async function run(task: () => Promise<void>) {
    busy.value = true;
    error.value = "";
    try {
      await task();
    } catch {
      error.value = "同步观看操作失败，请重试。";
    } finally {
      busy.value = false;
    }
  }

  const refresh = () => run(async () => void (groups.value = await api.listGroups()));
  /** 建组并把当前视频（或整季选集）设为组队列。 */
  const create = (name: string, itemId: string, startSeconds: number, queueIds: string[] = []) =>
    run(async () => {
      connect();
      const group = await api.createGroup(name);
      state.value = { ...state.value, group };
      const queue = queueIds.includes(itemId) ? queueIds : [itemId];
      await api.setNewQueue(queue, queue.indexOf(itemId), Math.round(startSeconds * 1e7));
    });
  const join = (groupId: string) =>
    run(async () => {
      connect();
      await api.joinGroup(groupId);
    });
  const leave = () =>
    run(async () => {
      await api.leaveGroup();
      state.value = api.emptySyncPlayState();
    });

  /** 组内本地操作 → 服务端广播；返回 true 表示已接管（播放器勿本地执行）。 */
  function requestPlayPause(paused: boolean): boolean {
    if (!active.value) return false;
    void run(() => (paused ? api.unpause() : api.pause()));
    return true;
  }
  function requestSeek(seconds: number): boolean {
    if (!active.value) return false;
    void run(() => api.seek(seconds * 1e7));
    return true;
  }

  /** 组内切集：按 ItemId 找队列项，找不到返回 false（交给本地处理）。 */
  function requestItem(itemId: string): boolean {
    if (!active.value) return false;
    const entry = state.value.queue.find((e) => e.ItemId === itemId);
    if (!entry) return false;
    void run(() => api.setPlaylistItem(entry.PlaylistItemId));
    return true;
  }
  /** 当前组队列指向的 ItemId（播放器据此切集）。 */
  const queueItemId = computed(() =>
    active.value ? (state.value.queue[state.value.playingIndex]?.ItemId ?? null) : null,
  );
  /** 切换随机播放（组内广播 PlayQueue）。 */
  function toggleShuffle(): boolean {
    if (!active.value) return false;
    const mode = state.value.shuffleMode === "Shuffle" ? "Sorted" : "Shuffle";
    void run(() => api.setShuffleMode(mode));
    return true;
  }
  /** 轮换循环模式。 */
  function cycleRepeat(): boolean {
    if (!active.value) return false;
    void run(() => api.setRepeatMode(api.nextRepeatMode(state.value.repeatMode)));
    return true;
  }
  /** 当前项播放结束：交给组队列推进（遵循 Repeat 模式）。 */
  function requestNext(): boolean {
    if (!active.value || state.value.queue.length === 0) return false;
    const current = state.value.queue[state.value.playingIndex]?.PlaylistItemId ?? "";
    void api.nextItem(current).catch(() => undefined);
    return true;
  }
  /** 组队列增删移（广播 PlayQueue，各端按新队列归约）。 */
  function queueNext(itemId: string): boolean {
    if (!active.value) return false;
    void run(() => api.queueItems([itemId], "QueueNext"));
    return true;
  }
  function removeQueueItem(playlistItemId: string): boolean {
    if (!active.value) return false;
    void run(() => api.removeFromPlaylist([playlistItemId]));
    return true;
  }
  function moveQueueItem(playlistItemId: string, newIndex: number): boolean {
    if (!active.value || newIndex < 0) return false;
    void run(() => api.movePlaylistItem(playlistItemId, newIndex));
    return true;
  }
  let localBuffering = false;
  function reportBuffering() {
    if (!active.value || localBuffering) return;
    localBuffering = true;
    void api.buffering().catch(() => undefined);
  }
  function reportReady() {
    if (!active.value) return;
    localBuffering = false;
    void api.ready().catch(() => undefined);
  }

  function dispose() {
    timers.forEach(clearTimeout);
    timers.clear();
    if (active.value) void api.leaveGroup().catch(() => undefined);
    socket?.close();
  }

  return {
    state,
    groups,
    busy,
    error,
    active,
    refresh,
    create,
    join,
    leave,
    requestPlayPause,
    requestSeek,
    requestItem,
    toggleShuffle,
    cycleRepeat,
    requestNext,
    queueNext,
    removeQueueItem,
    moveQueueItem,
    queueItemId,
    reportBuffering,
    reportReady,
    handleMessage,
    dispose,
  };
}
