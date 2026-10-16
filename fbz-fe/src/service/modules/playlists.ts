/**
 * 播放列表 C 端 service：对接 Emby 兼容面
 * - `GET /Playlists` 用户播放列表
 * - `POST /Playlists` 创建
 * - `POST /Playlists/{id}?Name=` 改名
 * - `GET /Playlists/{id}/Items` 列表成员
 * - `POST /Playlists/{id}/Items?Ids=` 加条目
 * - `DELETE /Playlists/{id}/Items?EntryIds=` 删条目
 *
 * 不改鉴权网关。
 */
import { embyRequest, mediaImageUrl } from "@/service/request.ts";
import type { ServerItem } from "@/service/modules/server.ts";
import type { ContinueItem } from "@/types/media.ts";

interface BaseItemDto {
  Id: string;
  Name: string;
  Type: string;
  ProductionYear?: number | null;
  CommunityRating?: number | null;
  RunTimeTicks?: number | null;
  SeriesName?: string | null;
  ParentIndexNumber?: number | null;
  IndexNumber?: number | null;
  ImageTags?: Record<string, string> | null;
  /** 播放列表成员 EntryId（删/移用）。 */
  PlaylistItemId?: string | null;
  UserData?: {
    PlaybackPositionTicks?: number;
    Played?: boolean;
    IsFavorite?: boolean;
  } | null;
}

interface QueryResultDto {
  Items?: BaseItemDto[];
  TotalRecordCount?: number;
  StartIndex?: number;
}

interface PlaylistWriteResultDto {
  Id: string;
  Name: string;
  ItemAddedCount?: number;
}

export interface PlaylistSummary {
  id: string;
  name: string;
}

export interface PlaylistItemsPage {
  items: ContinueItem[];
  /** 原始 Emby 条目，供 preparePlayback 使用。 */
  serverItems: ServerItem[];
  /** 与 items 对齐的 EntryId；无则空串。 */
  entryIds: string[];
  totalRecordCount: number;
}

function toServerItem(dto: BaseItemDto): ServerItem {
  return {
    Id: dto.Id,
    Name: dto.Name,
    Type: dto.Type,
    ProductionYear: dto.ProductionYear ?? undefined,
    CommunityRating: dto.CommunityRating ?? undefined,
    RunTimeTicks: dto.RunTimeTicks ?? undefined,
    SeriesName: dto.SeriesName ?? undefined,
    ParentIndexNumber: dto.ParentIndexNumber ?? undefined,
    IndexNumber: dto.IndexNumber ?? undefined,
    ImageTags: dto.ImageTags ?? undefined,
    UserData: dto.UserData
      ? {
          PlaybackPositionTicks: dto.UserData.PlaybackPositionTicks,
          Played: dto.UserData.Played,
          IsFavorite: dto.UserData.IsFavorite,
        }
      : undefined,
  };
}

function toContinueItem(dto: BaseItemDto): ContinueItem {
  const isEpisode = dto.Type === "Episode";
  const isSeries = dto.Type === "Series";
  const detailType = isSeries || isEpisode ? "tv" : "movie";
  const played = dto.UserData?.Played ?? false;
  const position = dto.UserData?.PlaybackPositionTicks ?? 0;
  const runtime = dto.RunTimeTicks ?? 0;
  return {
    id: dto.Id,
    libraryId: detailType === "tv" ? "series" : "movie",
    title: isEpisode && dto.SeriesName ? dto.SeriesName : dto.Name,
    meta: isEpisode
      ? `第 ${dto.ParentIndexNumber ?? 1} 季 · 第 ${dto.IndexNumber ?? "—"} 集`
      : [dto.ProductionYear, dto.Type].filter(Boolean).join(" · "),
    detailType,
    poster: dto.ImageTags?.Primary ? mediaImageUrl(`/Items/${dto.Id}/Images/Primary`) : undefined,
    year: dto.ProductionYear ?? undefined,
    rating: dto.CommunityRating ?? undefined,
    isPlayed: played,
    progress:
      !played && position > 0 && runtime > 0
        ? Math.min(100, Math.max(0, (position / runtime) * 100))
        : undefined,
    serverItem: toServerItem(dto),
    playlistItemId: dto.PlaylistItemId ?? undefined,
  };
}

/** 列出当前用户播放列表。 */
export async function listPlaylists(userId: string): Promise<PlaylistSummary[]> {
  const { data } = await embyRequest.get<QueryResultDto>("/Playlists", {
    params: {
      userId,
      startIndex: 0,
      limit: 100,
      enableImages: false,
    },
  });
  return (data.Items ?? [])
    .filter((item) => item.Type === "Playlist" || !item.Type)
    .map((item) => ({ id: item.Id, name: item.Name }));
}

/** 创建空播放列表（可选初始 Ids）。 */
export async function createPlaylist(opts: {
  name: string;
  ids?: string[];
  mediaType?: string;
}): Promise<PlaylistSummary> {
  const { data } = await embyRequest.post<PlaylistWriteResultDto>("/Playlists", null, {
    params: {
      Name: opts.name.trim(),
      Ids: opts.ids?.length ? opts.ids.join(",") : undefined,
      MediaType: opts.mediaType,
    },
  });
  return { id: data.Id, name: data.Name };
}

/** 改名。 */
export async function renamePlaylist(playlistId: string, name: string): Promise<PlaylistSummary> {
  const { data } = await embyRequest.post<PlaylistWriteResultDto>(
    `/Playlists/${encodeURIComponent(playlistId)}`,
    null,
    { params: { Name: name.trim() } },
  );
  return { id: data.Id, name: data.Name };
}

/** 打开播放列表成员（分页）。 */
export async function fetchPlaylistItems(
  playlistId: string,
  userId: string,
  opts?: { startIndex?: number; limit?: number },
): Promise<PlaylistItemsPage> {
  const { data } = await embyRequest.get<QueryResultDto>(
    `/Playlists/${encodeURIComponent(playlistId)}/Items`,
    {
      params: {
        userId,
        startIndex: opts?.startIndex ?? 0,
        limit: opts?.limit ?? 100,
        enableImages: true,
        fields: "PrimaryImageAspectRatio,Overview",
      },
    },
  );
  const dtos = data.Items ?? [];
  return {
    items: dtos.map(toContinueItem),
    serverItems: dtos.map(toServerItem),
    entryIds: dtos.map((d) => d.PlaylistItemId ?? ""),
    totalRecordCount: data.TotalRecordCount ?? dtos.length,
  };
}

/** 向播放列表追加媒体条目（媒体 public_id）。 */
export async function addPlaylistItems(
  playlistId: string,
  itemIds: string[],
  userId?: string,
): Promise<number> {
  const ids = itemIds.map((id) => id.trim()).filter(Boolean);
  if (!ids.length) return 0;
  const { data } = await embyRequest.post<PlaylistWriteResultDto>(
    `/Playlists/${encodeURIComponent(playlistId)}/Items`,
    null,
    {
      params: {
        Ids: ids.join(","),
        userId,
      },
    },
  );
  return data.ItemAddedCount ?? ids.length;
}

/** 按 EntryId（PlaylistItemId）删除成员。 */
export async function removePlaylistItems(playlistId: string, entryIds: string[]): Promise<void> {
  const ids = entryIds.map((id) => id.trim()).filter(Boolean);
  if (!ids.length) return;
  await embyRequest.delete(`/Playlists/${encodeURIComponent(playlistId)}/Items`, {
    params: { EntryIds: ids.join(",") },
  });
}

/** JF `PlaylistUserPermissions`。 */
export interface PlaylistShare {
  UserId: string;
  CanEdit: boolean;
}

export interface PlaylistSharing {
  openAccess: boolean;
  shares: PlaylistShare[];
}

export interface ShareableUser {
  id: string;
  name: string;
}

/** `GET /Playlists/{id}`（JF PlaylistDto）→ 公开状态 + 共享用户。 */
export async function fetchPlaylistSharing(playlistId: string): Promise<PlaylistSharing> {
  const { data } = await embyRequest.get<{ OpenAccess?: boolean; Shares?: PlaylistShare[] }>(
    `/Playlists/${encodeURIComponent(playlistId)}`,
  );
  return { openAccess: !!data.OpenAccess, shares: Array.isArray(data.Shares) ? data.Shares : [] };
}

/** 公开/取消公开（JF `IsPublic` → OpenAccess）。 */
export async function setPlaylistPublic(playlistId: string, isPublic: boolean): Promise<void> {
  await embyRequest.post(`/Playlists/${encodeURIComponent(playlistId)}`, null, {
    params: { IsPublic: isPublic },
  });
}

/** 共享给用户 / 更新 CanEdit（`POST /Playlists/{id}/Users/{userId}`）。 */
export async function sharePlaylist(
  playlistId: string,
  userId: string,
  canEdit: boolean,
): Promise<void> {
  await embyRequest.post(
    `/Playlists/${encodeURIComponent(playlistId)}/Users/${encodeURIComponent(userId)}`,
    { CanEdit: canEdit },
  );
}

/** 取消共享（`DELETE /Playlists/{id}/Users/{userId}`）。 */
export async function unsharePlaylist(playlistId: string, userId: string): Promise<void> {
  await embyRequest.delete(
    `/Playlists/${encodeURIComponent(playlistId)}/Users/${encodeURIComponent(userId)}`,
  );
}

/** 可共享对象：`GET /Users/Public`（排除自己）。 */
export async function fetchShareableUsers(selfId: string): Promise<ShareableUser[]> {
  const { data } = await embyRequest.get<{ Id: string; Name: string }[]>("/Users/Public");
  return (Array.isArray(data) ? data : [])
    .filter((u) => u.Id !== selfId)
    .map((u) => ({ id: u.Id, name: u.Name }));
}
