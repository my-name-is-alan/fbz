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
  PersonCredit,
  PersonDetail,
} from "@/types/media.ts";

/** 详情页统一视图模型：只承载后端真实数据；请求失败时页面展示空态。 */
export interface DetailViewModel {
  /** 数据来源，便于调试 / 后续按来源差异化展示。 */
  source: "backend";
  /** 面向路由 / 播放的 id（后端为 public_id）。 */
  id: string;
  detailType: "movie" | "tv";
  title: string;
  /** 海报绝对地址（已带鉴权）；无图为 undefined，交 MediaPoster 渲染占位。 */
  poster?: string;
  /** 剧照绝对地址；仅在确认存在时给值，避免 DetailHero 背景图 404 破图。 */
  backdrop?: string;
  /** 标题下的信息片段（年份、时长）。题材单列在 genres。 */
  meta: string[];
  /** 题材名列表（详情接口回填）。 */
  genres: string[];
  /** 分级（如 PG-13），无则 undefined。 */
  officialRating?: string;
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
  /** 用户收藏状态（UserData.IsFavorite）。 */
  isFavorite: boolean;
  /** 已看完标记。 */
  played: boolean;
  /** 续播位置（秒）；无播放历史 / 已看完为 undefined。 */
  resumePositionSeconds?: number;
  /** 演职员；后端无 People 数据时为空数组（CastRow 自动隐藏）。 */
  cast: CastMember[];
  /** 相似推荐卡片。 */
  similar: MediaItem[];
  /** 播放版本（每个 MediaSource 一个版本，含规格 tag 与字幕语言）。 */
  versions: MediaVersion[];
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
  /** 题材名列表（详情接口回填）。 */
  Genres?: string[] | null;
  /** 原名（与标题不同时后端才下发）。 */
  OriginalTitle?: string | null;
  /** 分级（如 PG-13）。 */
  OfficialRating?: string | null;
  /** 集号（Episode）/季号（Season）；未刷元数据时可能缺省。 */
  IndexNumber?: number | null;
  /** 集所属季号（Episode）。 */
  ParentIndexNumber?: number | null;
  /** 子项数量（Season 的集数）；后端下发时用于季卡展示。 */
  ChildCount?: number | null;
  ImageTags?: Record<string, string> | null;
  BackdropImageTags?: string[] | null;
  People?: BaseItemPersonDto[] | null;
  UserData?: UserDataDto | null;
}

/** Emby 用户播放数据（进度 / 已看标记）。 */
interface UserDataDto {
  IsFavorite?: boolean;
  /** 播放位置（100ns 单位）。 */
  PlaybackPositionTicks?: number | null;
  Played?: boolean;
  PlayCount?: number | null;
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
  /** 直出流地址（后端已把 api_key 拼进 URL，可直接播放）。 */
  DirectStreamUrl?: string | null;
  /** HLS 转码地址（`/emby/Videos/{id}/master.m3u8?...&api_key=...`）。 */
  TranscodingUrl?: string | null;
  /** 转码流 mime（通常 application/x-mpegURL）。 */
  TranscodingSubProtocol?: string | null;
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

/** 单个 MediaSource → 播放版本（规格 tag + 字幕语言 + 版本标签）。 */
function mediaSourceToVersion(source: PlaybackMediaSourceDto, index: number): MediaVersion {
  const streams = source.MediaStreams ?? [];

  const tags: string[] = [];
  const video = streams.find((s) => s.Type === "Video");
  const res = video ? resolutionTag(video.Height) : undefined;
  if (res) tags.push(res);
  if (video?.Codec) tags.push(video.Codec.toUpperCase());
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

  // 版本标签：清晰度 + 容器（如 "1080P · MKV"），信息不足时退化为「版本 N」。
  const labelParts = [res, source.Container?.toUpperCase()].filter(Boolean);
  const label = labelParts.length ? labelParts.join(" · ") : `版本 ${index + 1}`;

  return { id: source.Id ?? `source-${index}`, label, tags, subtitles };
}

/**
 * 从 PlaybackInfo 组装播放版本。后端对多文件条目会返回多个 MediaSource
 * （多版本），每个版本一条记录；失败或无流时返回空数组（DetailHero 优雅降级）。
 */
async function fetchPlaybackVersions(id: string): Promise<MediaVersion[]> {
  try {
    const { data } = await embyRequest.get<PlaybackInfoResponseDto>(`/Items/${id}/PlaybackInfo`);
    return (data.MediaSources ?? []).map(mediaSourceToVersion);
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

  // 续播位置：有进度且未看完才给（已看完显示"重新播放"而不是续播）。
  const positionSeconds = ticksToSeconds(item.UserData?.PlaybackPositionTicks);
  const played = item.UserData?.Played ?? false;
  const resumePositionSeconds =
    !played && positionSeconds && positionSeconds > 30 ? positionSeconds : undefined;

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
    genres: item.Genres ?? [],
    officialRating: item.OfficialRating ?? undefined,
    overview: item.Overview ?? undefined,
    rating: item.CommunityRating ?? null,
    runtimeSeconds,
    directors: detailType === "movie" ? directors || undefined : undefined,
    creators: detailType === "tv" ? directors || undefined : undefined,
    originalTitle: item.OriginalTitle ?? undefined,
    isFavorite: item.UserData?.IsFavorite ?? false,
    played,
    resumePositionSeconds,
    cast: peopleToCast(people),
    similar,
    versions,
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

/**
 * 拉取某人物参演的作品（代表作品）。走 `/Users/{uid}/Items?PersonIds={人物public_id}`，
 * 后端 person_ids 过滤内建（media_item_people 关联）。未登录 / 无关联作品时返回空数组，
 * 页面渲染空态，不合成假数据。
 */
async function fetchPersonCredits(personId: string): Promise<PersonCredit[]> {
  const { useAuthStore } = await import("@/stores/auth.ts");
  const userId = useAuthStore().userId;
  if (!userId || !personId) return [];
  try {
    const { data } = await embyRequest.get<QueryResultDto<BaseItemDto>>(
      `/Users/${encodeURIComponent(userId)}/Items`,
      {
        params: {
          personIds: personId,
          recursive: true,
          includeItemTypes: "Movie,Series",
          enableImages: true,
          limit: 100,
        },
      },
    );
    return (data.Items ?? []).map((dto) => {
      const detailType = detailTypeOf(dto.Type);
      return {
        id: dto.Id,
        type: detailType,
        libraryId: detailType === "tv" ? "series" : "movie",
        title: dto.Name,
        character: "",
        poster_path: dto.ImageTags?.Primary
          ? (mediaImageUrl(`/Items/${dto.Id}/Images/Primary`) ?? null)
          : null,
        year: dto.ProductionYear ?? null,
        rating: dto.CommunityRating ?? null,
      };
    });
  } catch {
    return [];
  }
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
    const known_for = await fetchPersonCredits(data.Id);
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
      known_for,
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

/* ---------- 剧集季/集（真实后端数据） ---------- */

/** 季视图模型：只承载后端真实字段，缺失即为空/占位。 */
export interface SeasonSummary {
  /** 季 public_id（拉分集时作 seasonId）。 */
  id: string;
  /** 季号（IndexNumber）；后端可能缺省。 */
  seasonNumber: number | null;
  name: string;
  /** 首播年份（由 PremiereDate 解析），缺失为 undefined。 */
  year?: number;
  /** 集数（ChildCount），缺失为 undefined，交页面渲空态。 */
  episodeCount?: number;
  overview?: string;
  /** 海报绝对地址（已带鉴权）；无 Primary 图为 undefined。 */
  poster?: string;
}

/** 分集视图模型：只承载后端真实字段，缺失即为空/占位。 */
export interface EpisodeSummary {
  /** 集 public_id（用于 fetchPlaybackSource / 播放列表定位）。 */
  id: string;
  /** 集号（IndexNumber）；后端可能缺省，页面用返回顺序兜底。 */
  episodeNumber: number | null;
  /** 所属季号（ParentIndexNumber）；可能缺省。 */
  seasonNumber: number | null;
  name: string;
  /** 时长（秒），无则 undefined（不展示时长）。 */
  runtimeSeconds?: number;
  /** 首播日期字符串（YYYY-MM-DD），无则 undefined。 */
  premiereDate?: string;
  /** 真实简介，无则 undefined。 */
  overview?: string;
  /** 集缩略图绝对地址（Primary），无则 undefined。 */
  poster?: string;
  /** 已完整观看。 */
  played: boolean;
  /** 播放进度百分比（0-100）；无历史/无时长为 undefined（不显示进度条）。 */
  progressPercent?: number;
}

/** PremiereDate → 年份（解析失败为 undefined）。 */
function yearOf(date: string | null | undefined): number | undefined {
  if (!date) return undefined;
  const year = new Date(date).getFullYear();
  return Number.isFinite(year) ? year : undefined;
}

/** 由 UserData 计算播放进度百分比；无进度或无时长返回 undefined。 */
function progressPercentOf(
  userData: UserDataDto | null | undefined,
  runtimeTicks: number | null | undefined,
): number | undefined {
  const position = userData?.PlaybackPositionTicks ?? 0;
  if (!position || position <= 0 || !runtimeTicks || runtimeTicks <= 0) return undefined;
  return Math.min(100, Math.max(0, (position / runtimeTicks) * 100));
}

/** 后端 Season DTO → 季视图模型。 */
function toSeasonSummary(dto: BaseItemDto): SeasonSummary {
  return {
    id: dto.Id,
    seasonNumber: dto.IndexNumber ?? null,
    name: dto.Name,
    year: yearOf(dto.PremiereDate),
    episodeCount: dto.ChildCount ?? undefined,
    overview: dto.Overview ?? undefined,
    poster: dto.ImageTags?.Primary ? mediaImageUrl(`/Items/${dto.Id}/Images/Primary`) : undefined,
  };
}

/** 后端 Episode DTO → 分集视图模型。 */
function toEpisodeSummary(dto: BaseItemDto): EpisodeSummary {
  return {
    id: dto.Id,
    episodeNumber: dto.IndexNumber ?? null,
    seasonNumber: dto.ParentIndexNumber ?? null,
    name: dto.Name,
    runtimeSeconds: ticksToSeconds(dto.RunTimeTicks),
    premiereDate: dto.PremiereDate ?? undefined,
    overview: dto.Overview ?? undefined,
    poster: dto.ImageTags?.Primary ? mediaImageUrl(`/Items/${dto.Id}/Images/Primary`) : undefined,
    played: dto.UserData?.Played ?? false,
    progressPercent: progressPercentOf(dto.UserData, dto.RunTimeTicks),
  };
}

/**
 * 拉取剧集的季列表（`GET /Shows/{seriesId}/Seasons`）。
 * 后端 `user_id` 可缺省（回落到令牌用户），失败 / 空结果返回空数组。
 */
export async function fetchSeasons(seriesId: string): Promise<SeasonSummary[]> {
  try {
    const { data } = await embyRequest.get<QueryResultDto<BaseItemDto>>(
      `/Shows/${encodeURIComponent(seriesId)}/Seasons`,
    );
    return (data.Items ?? []).map(toSeasonSummary);
  } catch {
    return [];
  }
}

/**
 * 拉取剧集分集（`GET /Shows/{seriesId}/Episodes`，可选 `seasonId` 限定单季）。
 * 失败 / 空结果返回空数组。
 */
export async function fetchEpisodes(
  seriesId: string,
  seasonId?: string,
): Promise<EpisodeSummary[]> {
  try {
    const { data } = await embyRequest.get<QueryResultDto<BaseItemDto>>(
      `/Shows/${encodeURIComponent(seriesId)}/Episodes`,
      { params: seasonId ? { seasonId } : undefined },
    );
    return (data.Items ?? []).map(toEpisodeSummary);
  } catch {
    return [];
  }
}

/* ---------- 视频播放流 ---------- */

/** 播放源：交给 shaka `load()` 的同源相对地址 + mime。 */
export interface PlaybackSourceResult {
  uri: string;
  mimeType: string;
}

/** Container → mime 猜测（DirectStream 无显式协议时用）。 */
function containerMime(container: string | null | undefined): string {
  const value = (container ?? "").toLowerCase();
  if (value.includes("webm")) return "video/webm";
  if (value.includes("ogg") || value.includes("ogv")) return "video/ogg";
  if (value.includes("m3u8") || value.includes("hls")) return "application/x-mpegURL";
  return "video/mp4";
}

/**
 * 归一为同源相对路径（以 `/` 开头）交给 shaka。
 * 后端可能给出绝对 URL 或相对路径；绝对 URL 仅取 path+search，保持同源。
 */
function toSameOriginPath(url: string): string {
  if (url.startsWith("/")) return url;
  try {
    const parsed = new URL(url, window.location.origin);
    return `${parsed.pathname}${parsed.search}`;
  } catch {
    return url;
  }
}

/**
 * 取某条目的可播放流地址（`GET /Items/{itemId}/PlaybackInfo`）。
 *
 * `mediaSourceId` 指定版本（多版本条目切换用）；缺省取首选版本。
 * 选择策略：有 `DirectStreamUrl` 优先直出（最省资源，mime 按 Container 推断）；
 * 否则回落 `TranscodingUrl`（HLS，mime `application/x-mpegURL`）。两者都无返回 `null`。
 * 返回的 `uri` 归一为同源相对路径。
 */
export async function fetchPlaybackSource(
  itemId: string,
  mediaSourceId?: string,
): Promise<PlaybackSourceResult | null> {
  try {
    const { data } = await embyRequest.get<PlaybackInfoResponseDto>(
      `/Items/${encodeURIComponent(itemId)}/PlaybackInfo`,
      { params: mediaSourceId ? { MediaSourceId: mediaSourceId } : undefined },
    );
    const source = mediaSourceId
      ? (data.MediaSources ?? []).find((entry) => entry.Id === mediaSourceId)
      : data.MediaSources?.[0];
    if (!source) return null;

    if (source.DirectStreamUrl) {
      return {
        uri: toSameOriginPath(source.DirectStreamUrl),
        mimeType: containerMime(source.Container),
      };
    }
    if (source.TranscodingUrl) {
      return {
        uri: toSameOriginPath(source.TranscodingUrl),
        mimeType: "application/x-mpegURL",
      };
    }
    return null;
  } catch {
    return null;
  }
}
