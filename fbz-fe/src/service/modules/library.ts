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
import { itemTypeMeta } from "@/utils/itemType.ts";

/**
 * 后端 `BaseItemDto` 的关键子集（仅取列表网格需要的字段）。
 * `CommunityRating` / `Genres` 由 Items 列表回填；缺省时省略。
 * 题材筛选项仍优先走 {@link fetchLibraryFilters}。
 */
interface BaseItemDto {
  Id: string;
  Name: string;
  /** Movie / Series / Episode / MusicAlbum … */
  Type: string;
  ProductionYear?: number | null;
  CommunityRating?: number | null;
  RunTimeTicks?: number | null;
  /** 题材列表；取首个供客户端筛选展示。 */
  Genres?: string[] | null;
  /** 集号 / 季号（Episode / Season）。 */
  IndexNumber?: number | null;
  ParentIndexNumber?: number | null;
  ParentId?: string | null;
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

/** `GET /Items/Filters` 响应（PascalCase）。 */
interface ItemsFiltersDto {
  Genres?: string[];
  Tags?: string[];
  OfficialRatings?: string[];
  Years?: number[];
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
  /** 题材名（`|` 或单值），交后端按题材过滤。 */
  genres?: string;
  /** 发行年份（逗号分隔），透传 Emby `Years`。 */
  years?: string;
  /** 包含标签（`|` 分隔），透传 Emby `Tags`。 */
  tags?: string;
  /**
   * 排除标签（`|` 分隔），透传 Emby `ExcludeTags`。
   * 后端已支持；无标签数据时 UI 仍保留参数位便于扩展。
   */
  excludeTags?: string;
  /**
   * Emby `Filters` 复合串，如 `IsPlayed` / `IsUnplayed` / `IsFavorite`。
   * 与 {@link isPlayed}/{@link isFavorite} 布尔参可并存；布尔参更精确时优先用它们。
   */
  filters?: string;
  /** 按已看/未看过滤（映射 `IsPlayed`）。 */
  isPlayed?: boolean;
  /** 按收藏过滤（映射 `IsFavorite`）。 */
  isFavorite?: boolean;
}

/** 映射后的单库条目结果。 */
export interface LibraryItemsResult {
  items: MediaItem[];
  total: number;
  startIndex: number;
}

/** 库内可用筛选项（题材等）。 */
export interface LibraryFiltersResult {
  genres: string[];
  years: number[];
  tags: string[];
  officialRatings: string[];
}

/** 后端 Type → 详情路由类型（剧集系/集归 tv，BoxSet 归 collection，其余归 movie）。 */
function detailTypeFor(type: string): "movie" | "tv" | "collection" {
  if (type === "Series" || type === "Episode" || type === "Season") return "tv";
  if (type === "BoxSet" || type === "Collection") return "collection";
  return "movie";
}

/** 单条 `BaseItemDto` → 前端 `MediaItem`（拼绝对海报地址；无 Primary 标签不盲拼）。 */
export function toMediaItem(dto: BaseItemDto, libraryId: string): MediaItem {
  const detailType = detailTypeFor(dto.Type);
  const year = dto.ProductionYear ?? undefined;
  const hasPrimary = Boolean(dto.ImageTags?.Primary);
  const genres = (dto.Genres ?? []).filter((g) => Boolean(g?.trim()));
  const played = dto.UserData?.Played ?? false;
  const position = dto.UserData?.PlaybackPositionTicks ?? 0;
  const runtime = dto.RunTimeTicks ?? 0;
  const progress =
    !played && position > 0 && runtime > 0
      ? Math.min(100, Math.max(0, (position / runtime) * 100))
      : undefined;
  const typeLabel = itemTypeMeta(dto.Type).label;
  return {
    id: dto.Id,
    libraryId: detailType === "collection" ? "collection" : libraryId,
    title: dto.Name,
    meta: `${year ?? "—"} · ${typeLabel}`,
    detailType,
    poster: hasPrimary ? mediaImageUrl(`/Items/${dto.Id}/Images/Primary`) : undefined,
    year,
    genre: genres[0],
    rating: dto.CommunityRating ?? undefined,
    isFavorite: dto.UserData?.IsFavorite ?? false,
    isPlayed: played,
    progress,
    itemType: dto.Type,
    seasonNumber: dto.ParentIndexNumber ?? undefined,
    episodeNumber: dto.IndexNumber ?? undefined,
  };
}

/**
 * 库内条目分页默认页大小。
 * 不再一次取满后端 `MAX_ITEMS_LIMIT`(200) 假全量；页面用 StartIndex/Limit 翻页。
 */
export const LIBRARY_PAGE_SIZE = 36;

/**
 * 拉取某库内条目并映射为 `MediaItem[]`。
 * 默认递归、按 SortName 升序、一页 {@link LIBRARY_PAGE_SIZE} 条；
 * 排序 / 题材 / 标签排阻 / 已看 / 收藏尽量透传服务端，避免前端假筛。
 * 显式 `EnableImages`，否则后端不下发 ImageTags，海报会全空。
 */
export async function fetchLibraryItems(query: LibraryItemsQuery): Promise<LibraryItemsResult> {
  const { data } = await embyRequest.get<ItemsResultDto>(
    `/Users/${encodeURIComponent(query.userId)}/Items`,
    {
      params: {
        parentId: query.libraryId,
        recursive: true,
        startIndex: query.startIndex ?? 0,
        limit: query.limit ?? LIBRARY_PAGE_SIZE,
        sortBy: query.sortBy ?? "SortName",
        sortOrder: query.sortOrder ?? "Ascending",
        includeItemTypes: query.includeItemTypes,
        genres: query.genres || undefined,
        years: query.years || undefined,
        tags: query.tags || undefined,
        excludeTags: query.excludeTags || undefined,
        filters: query.filters || undefined,
        isPlayed: query.isPlayed,
        isFavorite: query.isFavorite,
        fields: "UserData,RunTimeTicks,PrimaryImageAspectRatio,Genres,CommunityRating",
        enableImages: true,
        enableImageTypes: "Primary",
      },
    },
  );

  return {
    items: data.Items.map((dto) => toMediaItem(dto, query.libraryId)),
    total: data.TotalRecordCount,
    startIndex: data.StartIndex,
  };
}

/**
 * 为库总览取一张真实封面：抓最新 1 条带 Primary ImageTags 的条目海报。
 * 无图标签时返回 undefined，交视图层继续用占位块。
 */
export async function fetchLibraryCover(
  userId: string,
  libraryId: string,
  itemTypes?: string,
): Promise<string | undefined> {
  const result = await fetchLibraryItems({
    userId,
    libraryId,
    startIndex: 0,
    limit: 1,
    sortBy: "DateCreated",
    sortOrder: "Descending",
    includeItemTypes: itemTypes ?? "Movie,Series,MusicAlbum,Video,Book",
  });
  return result.items[0]?.poster;
}

/**
 * 拉取库内可用筛选项（题材 / 年份等），对接 `GET /Items/Filters`。
 * 比从列表条目聚合更完整（筛选项覆盖整库，不限当前页）。
 */
export async function fetchLibraryFilters(
  userId: string,
  libraryId?: string,
  includeItemTypes = "Movie,Series",
): Promise<LibraryFiltersResult> {
  const { data } = await embyRequest.get<ItemsFiltersDto>("/Items/Filters", {
    params: {
      userId,
      parentId: libraryId || undefined,
      includeItemTypes,
    },
  });
  return {
    genres: data.Genres ?? [],
    years: data.Years ?? [],
    tags: data.Tags ?? [],
    officialRatings: data.OfficialRatings ?? [],
  };
}
