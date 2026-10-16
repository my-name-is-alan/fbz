/**
 * SyncPlay C 端 service：对接 Jellyfin 兼容 `/SyncPlay/*`
 * - REST：List / New / Join / Leave / SetNewQueue / Pause / Unpause / Seek / Stop / Ping
 * - websocket（/embywebsocket）消息：`SyncPlayGroupUpdate` / `SyncPlayCommand`
 *   由 `applySyncPlayMessage` 归约成本地状态，播放器按 `pendingCommand` 执行。
 *
 * 不改鉴权网关。
 */
import { embyRequest } from "@/service/request.ts";

export interface SyncPlayGroup {
  GroupId: string;
  GroupName: string;
  State: "Idle" | "Waiting" | "Paused" | "Playing" | string;
  Participants: string[];
  LastUpdatedAt: string;
}

export interface SyncPlayCommand {
  GroupId: string;
  PlaylistItemId: string;
  When: string;
  PositionTicks: number;
  Command: "Pause" | "Unpause" | "Seek" | "Stop" | string;
  EmittedAt: string;
}

export interface SyncPlayState {
  group: SyncPlayGroup | null;
  queue: { ItemId: string; PlaylistItemId: string }[];
  playingIndex: number;
  pendingCommand: SyncPlayCommand | null;
  lastEvent: string | null;
  shuffleMode: SyncPlayShuffleMode;
  repeatMode: SyncPlayRepeatMode;
}

export type SyncPlayShuffleMode = "Sorted" | "Shuffle";
export type SyncPlayRepeatMode = "RepeatNone" | "RepeatAll" | "RepeatOne";

/** 循环模式轮换：不循环 → 列表循环 → 单集循环。 */
export function nextRepeatMode(mode: SyncPlayRepeatMode): SyncPlayRepeatMode {
  return mode === "RepeatNone" ? "RepeatAll" : mode === "RepeatAll" ? "RepeatOne" : "RepeatNone";
}

export function emptySyncPlayState(): SyncPlayState {
  return {
    group: null,
    queue: [],
    playingIndex: -1,
    pendingCommand: null,
    lastEvent: null,
    shuffleMode: "Sorted",
    repeatMode: "RepeatNone",
  };
}

export async function listGroups(): Promise<SyncPlayGroup[]> {
  const { data } = await embyRequest.get<SyncPlayGroup[]>("/SyncPlay/List");
  return Array.isArray(data) ? data : [];
}

export async function createGroup(groupName: string): Promise<SyncPlayGroup> {
  const { data } = await embyRequest.post<SyncPlayGroup>("/SyncPlay/New", { GroupName: groupName });
  return data;
}

export async function joinGroup(groupId: string): Promise<void> {
  await embyRequest.post("/SyncPlay/Join", { GroupId: groupId });
}

export async function leaveGroup(): Promise<void> {
  await embyRequest.post("/SyncPlay/Leave");
}

export async function setNewQueue(itemIds: string[], index = 0, startTicks = 0): Promise<void> {
  await embyRequest.post("/SyncPlay/SetNewQueue", {
    PlayingQueue: itemIds,
    PlayingItemPosition: index,
    StartPositionTicks: startTicks,
  });
}

export async function pause(): Promise<void> {
  await embyRequest.post("/SyncPlay/Pause");
}
export async function unpause(): Promise<void> {
  await embyRequest.post("/SyncPlay/Unpause");
}
export async function stop(): Promise<void> {
  await embyRequest.post("/SyncPlay/Stop");
}
export async function seek(positionTicks: number): Promise<void> {
  await embyRequest.post("/SyncPlay/Seek", {
    PositionTicks: Math.max(0, Math.round(positionTicks)),
  });
}
export async function buffering(): Promise<void> {
  await embyRequest.post("/SyncPlay/Buffering", {});
}
export async function ready(): Promise<void> {
  await embyRequest.post("/SyncPlay/Ready", {});
}
export async function setPlaylistItem(playlistItemId: string): Promise<void> {
  await embyRequest.post("/SyncPlay/SetPlaylistItem", { PlaylistItemId: playlistItemId });
}
export async function setShuffleMode(mode: SyncPlayShuffleMode): Promise<void> {
  await embyRequest.post("/SyncPlay/SetShuffleMode", { Mode: mode });
}
export async function setRepeatMode(mode: SyncPlayRepeatMode): Promise<void> {
  await embyRequest.post("/SyncPlay/SetRepeatMode", { Mode: mode });
}
/** 带上当前 PlaylistItemId：服务端据此忽略重复（多端同时播完）的推进请求。 */
export async function nextItem(playlistItemId = ""): Promise<void> {
  await embyRequest.post("/SyncPlay/NextItem", { PlaylistItemId: playlistItemId });
}
/** 入队：`Queue` 追加队尾，`QueueNext` 插到当前项之后。 */
export async function queueItems(
  itemIds: string[],
  mode: "Queue" | "QueueNext" = "Queue",
): Promise<void> {
  await embyRequest.post("/SyncPlay/Queue", { ItemIds: itemIds, Mode: mode });
}
/** 从组队列移除（按 PlaylistItemId）；`clearPlaylist` 清空除当前项外全部。 */
export async function removeFromPlaylist(
  playlistItemIds: string[],
  clearPlaylist = false,
  clearPlayingItem = false,
): Promise<void> {
  await embyRequest.post("/SyncPlay/RemoveFromPlaylist", {
    PlaylistItemIds: playlistItemIds,
    ClearPlaylist: clearPlaylist,
    ClearPlayingItem: clearPlayingItem,
  });
}
export async function movePlaylistItem(playlistItemId: string, newIndex: number): Promise<void> {
  await embyRequest.post("/SyncPlay/MovePlaylistItem", {
    PlaylistItemId: playlistItemId,
    NewIndex: newIndex,
  });
}
export async function previousItem(): Promise<void> {
  await embyRequest.post("/SyncPlay/PreviousItem", {});
}
export async function ping(ms: number): Promise<void> {
  await embyRequest.post("/SyncPlay/Ping", { Ping: Math.round(ms) });
}

/** 把 websocket 消息归约到 SyncPlay 状态；非 SyncPlay 消息原样返回。 */
export function applySyncPlayMessage(state: SyncPlayState, raw: unknown): SyncPlayState {
  const msg = typeof raw === "string" ? safeParse(raw) : raw;
  if (!msg || typeof msg !== "object") return state;
  const { MessageType, Data } = msg as { MessageType?: string; Data?: any };
  if (MessageType === "SyncPlayCommand" && Data) {
    return {
      ...state,
      pendingCommand: Data as SyncPlayCommand,
      lastEvent: `Command:${Data.Command}`,
    };
  }
  if (MessageType !== "SyncPlayGroupUpdate" || !Data) return state;
  const type: string = Data.Type;
  const payload = Data.Data;
  switch (type) {
    case "GroupJoined":
      return { ...state, group: payload as SyncPlayGroup, lastEvent: type };
    case "GroupLeft":
      return { ...emptySyncPlayState(), lastEvent: type };
    case "UserJoined":
      if (!state.group) return state;
      return {
        ...state,
        group: { ...state.group, Participants: [...state.group.Participants, String(payload)] },
        lastEvent: type,
      };
    case "UserLeft": {
      if (!state.group) return state;
      const parts = [...state.group.Participants];
      const i = parts.indexOf(String(payload));
      if (i >= 0) parts.splice(i, 1);
      return { ...state, group: { ...state.group, Participants: parts }, lastEvent: type };
    }
    case "StateUpdate":
      if (!state.group) return state;
      return {
        ...state,
        group: { ...state.group, State: payload?.State ?? state.group.State },
        lastEvent: type,
      };
    case "PlayQueue":
      return {
        ...state,
        queue: Array.isArray(payload?.Playlist) ? payload.Playlist : [],
        playingIndex: typeof payload?.PlayingItemIndex === "number" ? payload.PlayingItemIndex : -1,
        shuffleMode: payload?.ShuffleMode === "Shuffle" ? "Shuffle" : "Sorted",
        repeatMode: ["RepeatAll", "RepeatOne"].includes(payload?.RepeatMode)
          ? payload.RepeatMode
          : "RepeatNone",
        lastEvent: type,
      };
    default:
      return state;
  }
}

/** 距命令计划执行时刻的毫秒数（负数=已过期，立即执行）。 */
export function msUntil(command: Pick<SyncPlayCommand, "When">, now = Date.now()): number {
  const t = Date.parse(command.When);
  return Number.isNaN(t) ? 0 : t - now;
}

function safeParse(text: string): unknown {
  try {
    return JSON.parse(text);
  } catch {
    return null;
  }
}
