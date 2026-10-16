/**
 * 搜索 service：对接 Emby 兼容面 `GET /Search/Hints`（走 {@link embyRequest}，拦截器自动带 token）。
 *
 * 后端返回 `SearchHintsResultDto`：`{ SearchHints: SearchHintDto[], TotalRecordCount }`，
 * 单条 `SearchHintDto` 字段（PascalCase）为 `ItemId / Id / Name / Type / MediaType? /
 * ParentId? / IsFolder / RunTimeTicks? / ProductionYear?`（无 `PrimaryImageTag` / `ImageTags`）。
 *
 * 这里把 hint 映射为可直接喂给 `MediaCard` 的 {@link SearchResultItem}（继承 `MediaItem`）：
 * - 按 `Type` 区分 movie/tv/person/album/artist，给出对应详情路由 `to` 与展示用 `detailType`。
 * - **无图标签时不盲拼海报 URL**（避免 404 烂图）；有 `PrimaryImageTag` 才拼绝对地址。
 *   若需要带海报的 Items 搜索，可改走 `/Users/{id}/Items?SearchTerm=&EnableImages=true`。
 * - debounce 由调用方处理（视图层用自动导入的 lodash-es `debounce`）。
 */
import { embyRequest, mediaImageUrl } from "@/service/request.ts";
import type { MediaItem } from "@/types/media.ts";

/** 搜索结果实体类型（对应五类详情路由）。 */
export type SearchKind = "movie" | "tv" | "person" | "album" | "artist";

/** 结果条目：继承 MediaItem（供 MediaCard 直接消费），额外带类型与详情路由。 */
export interface SearchResultItem extends MediaItem {
  /** 实体类型，用于结果分区与选择卡片渲染方式。 */
  kind: SearchKind;
  /** 目标详情路由（MediaCard 只能路由 movie/tv，person/album/artist 用它跳转）。 */
  to: string;
}

/** 搜索可选参数。 */
export interface SearchOptions {
  /** 当前用户 public_id（Items 搜索路径预留；Hints 路径不强制）。 */
  userId?: string;
  /** 返回条数上限，默认 40。 */
  limit?: number;
  /** 参与搜索的 Emby 条目类型，默认电影/剧集/人物/专辑/艺术家。 */
  includeItemTypes?: string;
  /**
   * 排除标签（`|` 分隔），透传 Emby `ExcludeTags`（Items 路径）。
   * Hints 回退路径不支持该参数，有值时仍优先走 Items。
   */
  excludeTags?: string;
  /** 取消信号：调用方切换关键词时中止上一请求。 */
  signal?: AbortSignal;
}

/** 搜索页类型筛选 UI 选项（value 即 includeItemTypes；空 = 默认全类型）。 */
export const SEARCH_TYPE_OPTIONS: { label: string; value: string }[] = [
  { label: "全部类型", value: "" },
  { label: "电影", value: "Movie" },
  { label: "剧集", value: "Series" },
  { label: "人物", value: "Person" },
  { label: "专辑", value: "MusicAlbum" },
  { label: "艺术家", value: "MusicArtist" },
];

/** 后端 SearchHint DTO 形状（仅取用得到的字段）。 */
interface SearchHintDto {
  ItemId: string;
  Id: string;
  Name: string;
  Type: string;
  MediaType?: string | null;
  ParentId?: string | null;
  IsFolder?: boolean;
  RunTimeTicks?: number | null;
  ProductionYear?: number | null;
  /** 部分兼容实现可能下发；有值才拼海报。 */
  PrimaryImageTag?: string | null;
  ImageTags?: Record<string, string> | null;
}

interface SearchHintsResultDto {
  SearchHints?: SearchHintDto[];
  TotalRecordCount?: number;
}

/** Items 列表 DTO 子集（带图搜索路径）。 */
interface SearchItemDto {
  Id: string;
  Name: string;
  Type: string;
  ProductionYear?: number | null;
  ImageTags?: Record<string, string> | null;
  UserData?: {
    Played?: boolean;
    IsFavorite?: boolean;
  } | null;
}

interface ItemsResultDto {
  Items?: SearchItemDto[];
}

const DEFAULT_INCLUDE_ITEM_TYPES = "Movie,Series,Person,MusicAlbum,MusicArtist";

/** hint 类型描述：展示归属库、详情类型（仅 movie/tv 供 MediaCard）与分区标签。 */
interface KindDescriptor {
  kind: SearchKind;
  /** MediaCard 路由用；person/album/artist 无（改用 `to`）。 */
  detailType?: "movie" | "tv";
  /** 兜底归属库 id（MediaItem 必填）。 */
  libraryId: string;
  /** 分区标签，拼进 meta 副标题。 */
  label: string;
}

/** Emby `Type` → 前端类型描述；未知/不支持的类型返回 undefined（过滤掉）。 */
function describe(itemType: string): KindDescriptor | undefined {
  switch (itemType) {
    case "Movie":
      return { kind: "movie", detailType: "movie", libraryId: "movie", label: "电影" };
    case "Series":
      return { kind: "tv", detailType: "tv", libraryId: "series", label: "剧集" };
    case "Person":
      return { kind: "person", libraryId: "person", label: "人物" };
    case "MusicAlbum":
      return { kind: "album", libraryId: "music", label: "专辑" };
    case "MusicArtist":
      return { kind: "artist", libraryId: "music", label: "艺术家" };
    default:
      return undefined;
  }
}

/**
 * 单条 → 详情路由。
 * 人物详情页按「姓名」解析（`GET /Persons/{name}`），故 person 用 Name 而非 item id；
 * 其余（电影/剧集/专辑/艺术家）详情页按 id 解析。
 */
function routeFor(kind: SearchKind, id: string, name: string): string {
  if (kind === "person") return `/person/${encodeURIComponent(name)}`;
  const prefix = kind === "tv" ? "tv" : kind; // movie/album/artist 同名，tv 亦为 tv
  return `/${prefix}/${id}`;
}

/**
 * 有 Primary 图标签才拼海报；否则返回 undefined，交给 MediaPoster 占位。
 * 人物走 `/Persons/{name}`，其余走 `/Items/{id}`。
 */
function posterFor(
  kind: SearchKind,
  id: string,
  name: string,
  hasPrimary: boolean,
): string | undefined {
  if (!hasPrimary) return undefined;
  const path =
    kind === "person"
      ? `/Persons/${encodeURIComponent(name)}/Images/Primary`
      : `/Items/${id}/Images/Primary`;
  return mediaImageUrl(path);
}

function hasPrimaryImage(hint: SearchHintDto): boolean {
  return Boolean(hint.PrimaryImageTag || hint.ImageTags?.Primary);
}

/** 单条 hint → SearchResultItem；无法归类或缺 id 时返回 undefined。 */
function toResultItem(hint: SearchHintDto): SearchResultItem | undefined {
  const desc = describe(hint.Type);
  if (!desc) return undefined;
  const id = hint.Id || hint.ItemId;
  if (!id) return undefined;

  const meta = [hint.ProductionYear ? String(hint.ProductionYear) : "", desc.label]
    .filter(Boolean)
    .join(" · ");

  return {
    id,
    kind: desc.kind,
    to: routeFor(desc.kind, id, hint.Name),
    libraryId: desc.libraryId,
    detailType: desc.detailType,
    title: hint.Name,
    meta,
    poster: posterFor(desc.kind, id, hint.Name, hasPrimaryImage(hint)),
    year: hint.ProductionYear ?? undefined,
  };
}

function toResultItemFromDto(dto: SearchItemDto): SearchResultItem | undefined {
  const desc = describe(dto.Type);
  if (!desc) return undefined;
  if (!dto.Id) return undefined;
  const meta = [dto.ProductionYear ? String(dto.ProductionYear) : "", desc.label]
    .filter(Boolean)
    .join(" · ");
  return {
    id: dto.Id,
    kind: desc.kind,
    to: routeFor(desc.kind, dto.Id, dto.Name),
    libraryId: desc.libraryId,
    detailType: desc.detailType,
    title: dto.Name,
    meta,
    poster: posterFor(desc.kind, dto.Id, dto.Name, Boolean(dto.ImageTags?.Primary)),
    year: dto.ProductionYear ?? undefined,
    isFavorite: dto.UserData?.IsFavorite ?? false,
    isPlayed: dto.UserData?.Played ?? false,
  };
}

/**
 * 按关键词搜索。优先走 Items（`SearchTerm` + `EnableImages`）以拿到真实海报；
 * 无 userId 或 Items 失败时回退 Search/Hints，且**绝不盲拼无标签海报**。
 */
export async function searchHints(
  query: string,
  opts: SearchOptions = {},
): Promise<SearchResultItem[]> {
  const term = query.trim();
  if (!term) return [];

  const userId =
    opts.userId?.trim() ||
    (typeof localStorage !== "undefined" ? (localStorage.getItem("fbz_auth_user_id") ?? "") : "");

  if (userId) {
    try {
      const { data } = await embyRequest.get<ItemsResultDto>(
        `/Users/${encodeURIComponent(userId)}/Items`,
        {
          params: {
            searchTerm: term,
            recursive: true,
            limit: opts.limit ?? 40,
            includeItemTypes: opts.includeItemTypes ?? DEFAULT_INCLUDE_ITEM_TYPES,
            excludeTags: opts.excludeTags || undefined,
            enableImages: true,
            enableImageTypes: "Primary",
          },
          signal: opts.signal,
        },
      );
      const results: SearchResultItem[] = [];
      for (const item of data.Items ?? []) {
        const mapped = toResultItemFromDto(item);
        if (mapped) results.push(mapped);
      }
      // Items 有结果就用之；空结果仍回退 Hints（人物等可能只在 Hints 出现）。
      // 已请求 excludeTags 时不回退：Hints 无法排阻，避免结果“漏网”。
      if (results.length || opts.excludeTags) return results;
    } catch (error) {
      if ((error as { code?: string })?.code === "ERR_CANCELED") throw error;
      // 回退 Hints（excludeTags 场景下 Items 失败则返回空，不强行用无排阻结果）
      if (opts.excludeTags) return [];
    }
  }

  const { data } = await embyRequest.get<SearchHintsResultDto>("/Search/Hints", {
    params: {
      SearchTerm: term,
      Limit: opts.limit ?? 40,
      IncludeItemTypes: opts.includeItemTypes ?? DEFAULT_INCLUDE_ITEM_TYPES,
    },
    signal: opts.signal,
  });

  const results: SearchResultItem[] = [];
  for (const hint of data.SearchHints ?? []) {
    const item = toResultItem(hint);
    if (item) results.push(item);
  }
  return results;
}
