/**
 * 媒体库内条目 service：对接 Emby 兼容面 `GET /Users/{userId}/Items`。
 *
 * 后端没有裸 `/Items` 浏览端点；列表统一走 `/Users/{userId}/Items`（见
 * `fbz-api/src/compat/emby/routes/items.rs` 的 `user_items` / `ItemsQuery`），
 * 路径里的 userId 必须与令牌用户一致，由 {@link embyRequest} 拦截器自动带上 `x-emby-token`。
 * 这里把后端 `BaseItemDto`（PascalCase）映射为页面通用的 `MediaItem`，
 * poster 经 `mediaImageUrl()` 拼成带鉴权的绝对地址，消费方与 TMDB 占位时完全一致。
 */
import { embyRequest, mediaImageUrl } from "@/service/request.ts";
import type { MediaItem } from "@/types/media.ts";

/**
 * 后端 `BaseItemDto` 的关键子集（仅取列表网格需要的字段）。
 * `CommunityRating` 当前 DTO 未下发，作可选保留：后端补齐后无需改动消费方。
 */
interface BaseItemDto {
  Id: string;
  Name: string;
  /** Movie / Series / Episode / MusicAlbum … */
  Type: string;
  ProductionYear?: number | null;
  CommunityRating?: number | null;
  RunTimeTicks?: number | null;
  /** 图片标签表，含 Primary 时表示该条目有主海报。 */
  ImageTags?: Record<string, string> | null;
  UserData?: {
    PlaybackPositionTicks?: number;
    Played?: boolean;
    IsFavorite?: boolean;
  } | null;
}

/** `GET /Users/{userId}/Items` 的响应（PascalCase，对齐后端 `QueryResultDto`）。 */
interface ItemsResultDto {
  Items: BaseItemDto[];
  TotalRecordCount: number;
  StartIndex: number;
}

/** 单库条目查询入参。 */
export interface LibraryItemsQuery {
  /** 当前登录用户 public_id（来自 auth store）。 */
  userId: string;
  /** 库 public_id（作为 parentId）。 */
  libraryId: string;
  startIndex?: number;
  limit?: number;
  /** 后端排序字段，如 SortName / ProductionYear / CommunityRating / DateCreated。 */
  sortBy?: string;
  sortOrder?: "Ascending" | "Descending";
  /** 限定条目类型，如 "Movie,Series"；缺省时由 parentId 圈定整库。 */
  includeItemTypes?: string;
  /** 题材名（逗号分隔），交后端按题材过滤。 */
  genres?: string;
}

/** 映射后的单库条目结果。 */
export interface LibraryItemsResult {
  items: MediaItem[];
  total: number;
  startIndex: number;
}

/** 后端 Type → 详情路由类型（剧集系/集归 tv，其余归 movie）。 */
function detailTypeFor(type: string): "movie" | "tv" {
  return type === "Series" || type === "Episode" || type === "Season" ? "tv" : "movie";
}

/** 单条 `BaseItemDto` → 前端 `MediaItem`（拼绝对海报地址）。 */
function toMediaItem(dto: BaseItemDto, libraryId: string): MediaItem {
  const detailType = detailTypeFor(dto.Type);
  const year = dto.ProductionYear ?? undefined;
  const hasPrimary = Boolean(dto.ImageTags?.Primary);
  return {
    id: dto.Id,
    libraryId,
    title: dto.Name,
    meta: `${year ?? "—"} · ${detailType === "tv" ? "剧集" : "电影"}`,
    detailType,
    poster: hasPrimary ? mediaImageUrl(`/Items/${dto.Id}/Images/Primary`) : undefined,
    year,
    rating: dto.CommunityRating ?? undefined,
    isFavorite: dto.UserData?.IsFavorite ?? false,
  };
}

/** 单库默认抓取上限：对齐后端 `MAX_ITEMS_LIMIT`，一次性取满供前端分组/排序。 */
const DEFAULT_LIBRARY_ITEMS_LIMIT = 200;

/**
 * 拉取某库内条目并映射为 `MediaItem[]`。
 * 默认递归整库、按 SortName 升序、取满 200 条（后端硬上限），让页面沿用现有的
 * 客户端排序/分组逻辑而无需感知后端形状。
 */
export async function fetchLibraryItems(query: LibraryItemsQuery): Promise<LibraryItemsResult> {
  const { data } = await embyRequest.get<ItemsResultDto>(
    `/Users/${encodeURIComponent(query.userId)}/Items`,
    {
      params: {
        parentId: query.libraryId,
        recursive: true,
        startIndex: query.startIndex ?? 0,
        limit: query.limit ?? DEFAULT_LIBRARY_ITEMS_LIMIT,
        sortBy: query.sortBy ?? "SortName",
        sortOrder: query.sortOrder ?? "Ascending",
        includeItemTypes: query.includeItemTypes,
        genres: query.genres,
      },
    },
  );

  return {
    items: data.Items.map((dto) => toMediaItem(dto, query.libraryId)),
    total: data.TotalRecordCount,
    startIndex: data.StartIndex,
  };
}
