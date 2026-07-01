/**
 * 详情页 service：把 Emby 兼容面的条目详情接到详情页。
 *
 * 后端真实可用端点（走 {@link embyRequest}，拦截器自动带 token）：
 * - `GET /Items/{id}`             单条 `BaseItemDto`
 * - `GET /Items/{id}/Images`      该条目实际拥有的图片清单（Primary/Backdrop/...）
 * - `GET /Items/{id}/Similar`     相似推荐 `{ Items: BaseItemDto[] }`
 * - `GET /Persons/{name}`         人物详情
 * - `GET /Collections/{id}`       合集详情
 * - `GET /Collections/{id}/Items` 合集成员
 *
 * 路由 `:id` 使用后端 `public_id`。后端不可达或条目不存在时返回 undefined，
 * 页面展示真实空态，不再回退到设计态 mock。
 */
import { embyRequest, mediaImageUrl } from "@/service/request.ts";
import type {
  CastMember,
  CollectionDetail,
  MediaItem,
  MediaVersion,
  PersonDetail,
  SeasonInfo,
} from "@/types/media.ts";

/** 详情页统一视图模型：只承载后端真实数据；请求失败时页面展示空态。 */
export interface DetailViewModel {
  /** 数据来源，便于调试 / 后续按来源差异化展示。 */
  source: "backend";
  /** 面向路由 / 播放的 id（后端为 public_id，TMDB 为数字字符串）。 */
  id: string;
  detailType: "movie" | "tv";
  title: string;
  /** 海报绝对地址（已带鉴权）；无图为 undefined，交 MediaPoster 渲染占位。 */
  poster?: string;
  /** 剧照绝对地址；仅在确认存在时给值，避免 DetailHero 背景图 404 破图。 */
  backdrop?: string;
  /** 标题下的信息片段，如 ["2024", "2h 6m", "科幻"]。 */
  meta: string[];
  tagline?: string;
  overview?: string;
  rating?: number | null;
  /** 时长（秒），用于播放覆盖层 duration。 */
  runtimeSeconds?: number;
  /** 导演（电影），多名以「、」连接；后端无此数据时为空串。 */
  directors?: string;
  /** 主创（剧集）。 */
  creators?: string;
  /** 原名，与标题不同才给值。 */
  originalTitle?: string;
  /** 所属系列 id / 名称（仅 TMDB 占位提供）。 */
  collectionId?: string | number | null;
  collectionName?: string | null;
  /** 演职员；后端无 People 数据时为空数组（CastRow 自动隐藏）。 */
  cast: CastMember[];
  /** 相似推荐卡片。 */
  similar: MediaItem[];
  /** 播放版本/规格；后端无清晰度/编码明细时为空数组（DetailHero 优雅降级）。 */
  versions: MediaVersion[];
  /** 季列表（仅 TMDB 占位提供，后端缺季集字段时为空）。 */
  seasons: SeasonInfo[];
  seasonsCount?: number;
  episodesCount?: number;
}

/* ---------- 后端 DTO 形状（PascalCase，仅取详情页用得到的字段） ---------- */

interface BaseItemDto {
  Id: string;
  Name: string;
  Type: string;
  ProductionYear?: number | null;
  RunTimeTicks?: number | null;
  Overview?: string | null;
  PremiereDate?: string | null;
  EndDate?: string | null;
  CommunityRating?: number | null;
  CollectionType?: string | null;
  ImageTags?: Record<string, string> | null;
  BackdropImageTags?: string[] | null;
  People?: BaseItemPersonDto[] | null;
  UserData?: {
    IsFavorite?: boolean;
  } | null;
}

/** 后端关联人物（Emby BaseItemPerson 形状）。 */
interface BaseItemPersonDto {
  Id: string;
  Name: string;
  Role?: string | null;
  Type: string; // Actor / Director / Writer / ...
  PrimaryImageTag?: string | null;
  SortOrder?: number | null;
}

interface ItemImageInfoDto {
  ImageType: string;
}

interface SimilarResultDto {
  Items?: BaseItemDto[];
}

/** PlaybackInfo 里的单条媒体流（只取详情页展示用得到的字段）。 */
interface MediaStreamDto {
  Type: string; // Video / Audio / Subtitle
  Codec?: string | null;
  Language?: string | null;
  DisplayTitle?: string | null;
  Width?: number | null;
  Height?: number | null;
  Channels?: number | null;
}

interface PlaybackMediaSourceDto {
  Id?: string;
  Container?: string | null;
  MediaStreams?: MediaStreamDto[] | null;
}

interface PlaybackInfoResponseDto {
  MediaSources?: PlaybackMediaSourceDto[] | null;
}

interface QueryResultDto<T> {
  Items?: T[];
  TotalRecordCount?: number;
  StartIndex?: number;
}

/** Emby RunTimeTicks 为 100ns 单位 → 秒。 */
function ticksToSeconds(ticks: number | null | undefined): number | undefined {
  if (!ticks || ticks <= 0) return undefined;
  return Math.round(ticks / 10_000_000);
}

/** 秒 → "2h 6m" / "48m"，用于 meta 展示。 */
function formatRuntime(seconds: number | undefined): string {
  if (!seconds) return "";
  const totalMinutes = Math.round(seconds / 60);
  const h = Math.floor(totalMinutes / 60);
  const m = totalMinutes % 60;
  return h ? `${h}h ${m}m` : `${m}m`;
}

/** 后端条目类型 → 详情/路由类型。 */
function detailTypeOf(itemType: string): "movie" | "tv" {
  return itemType === "Series" || itemType === "Season" || itemType === "Episode" ? "tv" : "movie";
}

/** 后端条目 → 相似行卡片 MediaItem（海报指向 Primary 端点，无图时 MediaPoster 兜底）。 */
function similarToMediaItem(dto: BaseItemDto): MediaItem {
  const detailType = detailTypeOf(dto.Type);
  return {
    id: dto.Id,
    libraryId: detailType === "tv" ? "series" : "movie",
    detailType,
    title: dto.Name,
    meta: `${dto.ProductionYear ?? "—"} · ${detailType === "tv" ? "剧集" : "电影"}`,
    poster: mediaImageUrl(`/Items/${dto.Id}/Images/Primary`),
    year: dto.ProductionYear ?? undefined,
    rating: dto.CommunityRating ?? undefined,
    isFavorite: dto.UserData?.IsFavorite ?? false,
  };
}

/** 拉取相似推荐（失败 / 空结果时返回空数组，不阻断详情渲染）。 */
async function fetchSimilar(id: string): Promise<MediaItem[]> {
  try {
    const { data } = await embyRequest.get<SimilarResultDto>(`/Items/${id}/Similar`, {
      params: { Limit: 12 },
    });
    return (data.Items ?? []).map(similarToMediaItem);
  } catch {
    return [];
  }
}

/** 拉取条目实际拥有的图片类型集合（失败时为空，海报/剧照随之降级为占位）。 */
async function fetchImageTypes(id: string): Promise<Set<string>> {
  try {
    const { data } = await embyRequest.get<ItemImageInfoDto[]>(`/Items/${id}/Images`);
    return new Set(data.map((image) => image.ImageType));
  } catch {
    return new Set();
  }
}

/** 分辨率高度 → 清晰度标签（与卡片徽章口径一致）。 */
function resolutionTag(height: number | null | undefined): string | undefined {
  if (!height) return undefined;
  if (height >= 2000) return "4K";
  if (height >= 1400) return "2K";
  if (height >= 1000) return "1080P";
  if (height >= 700) return "720P";
  return `${height}P`;
}

/**
 * 从 PlaybackInfo 组装播放版本（视频元信息：清晰度/编码/音轨/字幕）。
 * 后端 probe worker 把 ffprobe 结果存进 media_streams，PlaybackInfo 会带出完整 MediaStreams。
 * 失败或无流时返回空数组，DetailHero 优雅降级（不显示规格区）。
 */
async function fetchPlaybackVersions(id: string): Promise<MediaVersion[]> {
  try {
    const { data } = await embyRequest.get<PlaybackInfoResponseDto>(`/Items/${id}/PlaybackInfo`);
    const source = data.MediaSources?.[0];
    if (!source) return [];
    const streams = source.MediaStreams ?? [];

    const tags: string[] = [];
    const video = streams.find((s) => s.Type === "Video");
    if (video) {
      const res = resolutionTag(video.Height);
      if (res) tags.push(res);
      if (video.Codec) tags.push(video.Codec.toUpperCase());
    }
    if (source.Container) tags.push(source.Container.toUpperCase());
    const audio = streams.find((s) => s.Type === "Audio");
    if (audio?.Codec) {
      const ch = audio.Channels ? `${audio.Channels}ch` : "";
      tags.push(`${audio.Codec.toUpperCase()}${ch ? ` ${ch}` : ""}`);
    }

    const subtitles = streams
      .filter((s) => s.Type === "Subtitle")
      .map((s) => s.Language || s.DisplayTitle || "未知")
      .filter((v, i, arr) => arr.indexOf(v) === i);

    if (!tags.length && !subtitles.length) return [];
    return [{ id: source.Id ?? "default", label: "默认版本", tags, subtitles }];
  } catch {
    return [];
  }
}

/** 后端 People → 演职员卡片（仅取 Actor），linkId 用人物名（后端 /person/{name} 解析）。 */
function peopleToCast(people: BaseItemPersonDto[]): CastMember[] {
  return people
    .filter((person) => person.Type === "Actor")
    .map((person, index) => ({
      id: index,
      name: person.Name,
      character: person.Role ?? "",
      profile_path: person.PrimaryImageTag
        ? (mediaImageUrl(`/Persons/${encodeURIComponent(person.Name)}/Images/Primary`) ?? null)
        : null,
      order: person.SortOrder ?? index,
      linkId: person.Name,
    }));
}

/** 后端 People → 指定职务的姓名串（如导演），以「、」连接；无则空串。 */
function peopleRoleNames(people: BaseItemPersonDto[], type: string): string {
  return people
    .filter((person) => person.Type === type)
    .map((person) => person.Name)
    .join("、");
}

/** 后端态：拉取单条详情；条目不存在 / 请求失败时返回 undefined 以触发回退。 */
async function backendDetail(
  id: string,
  detailType: "movie" | "tv",
): Promise<DetailViewModel | undefined> {
  let item: BaseItemDto;
  try {
    const { data } = await embyRequest.get<BaseItemDto>(`/Items/${id}`);
    item = data;
  } catch {
    return undefined;
  }

  const [imageTypes, similar, versions] = await Promise.all([
    fetchImageTypes(id),
    fetchSimilar(id),
    fetchPlaybackVersions(id),
  ]);

  const runtimeSeconds = ticksToSeconds(item.RunTimeTicks);
  const meta = [
    item.ProductionYear ? String(item.ProductionYear) : "",
    formatRuntime(runtimeSeconds),
  ].filter(Boolean);

  const people = item.People ?? [];
  const directors = peopleRoleNames(people, "Director");

  return {
    source: "backend",
    id: item.Id,
    detailType,
    title: item.Name,
    poster: imageTypes.has("Primary") ? mediaImageUrl(`/Items/${id}/Images/Primary`) : undefined,
    backdrop: imageTypes.has("Backdrop")
      ? mediaImageUrl(`/Items/${id}/Images/Backdrop`)
      : undefined,
    meta,
    overview: item.Overview ?? undefined,
    rating: item.CommunityRating ?? null,
    runtimeSeconds,
    directors: detailType === "movie" ? directors || undefined : undefined,
    creators: detailType === "tv" ? directors || undefined : undefined,
    cast: peopleToCast(people),
    similar,
    versions,
    seasons: [],
  };
}

/** 加载电影详情：打后端 `public_id`。 */
export async function loadMovieDetail(routeId: string): Promise<DetailViewModel | undefined> {
  return backendDetail(routeId, "movie");
}

/** 加载剧集详情：打后端 `public_id`。 */
export async function loadTvDetail(routeId: string): Promise<DetailViewModel | undefined> {
  return backendDetail(routeId, "tv");
}

function personImage(name: string, hasPrimary: boolean): string | undefined {
  if (!hasPrimary) return undefined;
  return mediaImageUrl(`/Persons/${encodeURIComponent(name)}/Images/Primary`);
}

function collectionPartToMediaItem(dto: BaseItemDto): MediaItem {
  const detailType = detailTypeOf(dto.Type);
  return {
    id: dto.Id,
    libraryId: detailType === "tv" ? "series" : "movie",
    detailType,
    title: dto.Name,
    meta: `${dto.ProductionYear ?? "—"} · ${detailType === "tv" ? "剧集" : "电影"}`,
    poster: dto.ImageTags?.Primary ? mediaImageUrl(`/Items/${dto.Id}/Images/Primary`) : undefined,
    year: dto.ProductionYear ?? undefined,
    rating: dto.CommunityRating ?? undefined,
    isFavorite: dto.UserData?.IsFavorite ?? false,
  };
}

/** 加载人物详情：后端按姓名/人物 public id 暴露兼容接口，代表作品暂由后端扩展后补齐。 */
export async function loadPersonDetail(routeId: string): Promise<PersonDetail | undefined> {
  try {
    const name = decodeURIComponent(routeId);
    const { data } = await embyRequest.get<BaseItemDto>(`/Persons/${encodeURIComponent(name)}`);
    const profile = personImage(data.Name, Boolean(data.ImageTags?.Primary));
    return {
      key: `person:${data.Id}`,
      id: Number.isFinite(Number(data.Id)) ? Number(data.Id) : 0,
      type: "person",
      name: data.Name,
      biography: data.Overview ?? "",
      profile_path: profile ?? null,
      birthday: data.PremiereDate ?? null,
      place_of_birth: null,
      known_for_department: "",
      known_for: [],
    };
  } catch {
    return undefined;
  }
}

/** 加载合集详情与成员，全部来自 Rust 后端 `/Collections/*`。 */
export async function loadCollectionDetail(
  routeId: string,
): Promise<{ collection: CollectionDetail; parts: MediaItem[] } | undefined> {
  try {
    const id = encodeURIComponent(routeId);
    const [detailResponse, itemsResponse] = await Promise.all([
      embyRequest.get<BaseItemDto>(`/Collections/${id}`),
      embyRequest.get<QueryResultDto<BaseItemDto>>(`/Collections/${id}/Items`, {
        params: { limit: 100, enableImages: true },
      }),
    ]);

    const item = detailResponse.data;
    const parts = (itemsResponse.data.Items ?? []).map(collectionPartToMediaItem);
    const firstPoster = parts.find((part) => part.poster)?.poster ?? null;
    return {
      collection: {
        key: `collection:${item.Id}`,
        id: Number.isFinite(Number(item.Id)) ? Number(item.Id) : 0,
        type: "collection",
        title: item.Name,
        overview: item.Overview ?? "",
        poster_path: firstPoster,
        backdrop_path: null,
        parts: [],
      },
      parts,
    };
  } catch {
    return undefined;
  }
}
