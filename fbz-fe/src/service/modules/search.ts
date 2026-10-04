/**
 * 搜索 service：对接 Emby 兼容面 `GET /Search/Hints`（走 {@link embyRequest}，拦截器自动带 token）。
 *
 * 后端返回 `SearchHintsResultDto`：`{ SearchHints: SearchHintDto[], TotalRecordCount }`，
 * 单条 `SearchHintDto` 字段（PascalCase）为 `ItemId / Id / Name / Type / MediaType? /
 * ParentId? / IsFolder / RunTimeTicks? / ProductionYear?`（无 `PrimaryImageTag`）。
 *
 * 这里把 hint 映射为可直接喂给 `MediaCard` 的 {@link SearchResultItem}（继承 `MediaItem`）：
 * - 按 `Type` 区分 movie/tv/person/album/artist，给出对应详情路由 `to` 与展示用 `detailType`。
 * - 海报统一走图片端点并用 {@link mediaImageUrl} 拼带鉴权的绝对地址（人物走 `/Persons/{name}`，
 *   其余走 `/Items/{id}`，与 detail.ts 约定一致）。
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
  /** 返回条数上限，默认 40。 */
  limit?: number;
  /** 参与搜索的 Emby 条目类型，默认电影/剧集/人物/专辑/艺术家。 */
  includeItemTypes?: string;
  /** 取消信号：调用方切换关键词时中止上一请求。 */
  signal?: AbortSignal;
}

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
}

interface SearchHintsResultDto {
  SearchHints?: SearchHintDto[];
  TotalRecordCount?: number;
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
 * 单条 hint → 详情路由。
 * 人物详情页按「姓名」解析（`GET /Persons/{name}`），故 person 用 Name 而非 item id；
 * 其余（电影/剧集/专辑/艺术家）详情页按 id 解析。
 */
function routeFor(kind: SearchKind, id: string, name: string): string {
  if (kind === "person") return `/person/${encodeURIComponent(name)}`;
  const prefix = kind === "tv" ? "tv" : kind; // movie/album/artist 同名，tv 亦为 tv
  return `/${prefix}/${id}`;
}

/** 单条 hint → 海报地址：人物走 `/Persons/{name}`，其余走 `/Items/{id}`（无图时视图渲染占位）。 */
function posterFor(kind: SearchKind, id: string, name: string): string | undefined {
  const path =
    kind === "person"
      ? `/Persons/${encodeURIComponent(name)}/Images/Primary`
      : `/Items/${id}/Images/Primary`;
  return mediaImageUrl(path);
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
    poster: posterFor(desc.kind, id, hint.Name),
    year: hint.ProductionYear ?? undefined,
  };
}

/**
 * 按关键词搜索，返回映射后的结果列表（空关键词直接返回空数组，不发请求）。
 * 请求被取消（调用方 abort）时向上抛出，调用方据 `code === "ERR_CANCELED"` 忽略。
 */
export async function searchHints(
  query: string,
  opts: SearchOptions = {},
): Promise<SearchResultItem[]> {
  const term = query.trim();
  if (!term) return [];

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
