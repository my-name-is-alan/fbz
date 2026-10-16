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
import {
  sourceBadges,
  sourceLabel,
  toMediaSourceInfo,
  type MediaSourceDto,
  type MediaSourceInfo,
} from "@/service/modules/mediaInfo.ts";
import { itemTypeMeta } from "@/utils/itemType.ts";
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
  /** 后端真实条目类型（Movie / Video / Series / Season / Episode …）。 */
  itemType: string;
  /** 父级 id（Episode → Season，Season → Series）。 */
  parentId?: string;
  /** 单集 / 季的季号、集号。 */
  seasonNumber?: number;
  episodeNumber?: number;
  /** 首播日期（YYYY-MM-DD）。 */
  premiereDate?: string;
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
  /** 编剧。 */
  writers?: string;
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
  /** 完整技术信息（每个 MediaSource 一份），驱动「媒体信息」面板。 */
  mediaSources: MediaSourceInfo[];
  /** 章节 / 片头片尾标记数量（Chapters）。 */
  chapterCount: number;
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
  ParentId?: string | null;
  Chapters?: unknown[] | null;
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

/** PlaybackInfo 的 MediaSource（技术字段见 mediaInfo.ts；此处补播放地址字段）。 */
interface PlaybackMediaSourceDto extends MediaSourceDto {
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
export function formatRuntime(seconds: number | undefined): string {
  if (!seconds) return "";
  // 不足 1 分钟的短片（预告/样片）按秒显示，避免出现「0m」。
  if (seconds < 60) return `${Math.max(1, Math.round(seconds))}s`;
  const totalMinutes = Math.round(seconds / 60);
  const h = Math.floor(totalMinutes / 60);
  const m = totalMinutes % 60;
  return h ? `${h}h ${m}m` : `${m}m`;
}

/** 后端条目类型 → 详情/路由类型。 */
function detailTypeOf(itemType: string): "movie" | "tv" {
  return itemType === "Series" || itemType === "Season" || itemType === "Episode" ? "tv" : "movie";
}

/** 卡片副标题：年份 · 真实类型文案（不再一律写「电影」）。 */
function cardMeta(dto: BaseItemDto): string {
  return `${dto.ProductionYear ?? "—"} · ${itemTypeMeta(dto.Type).label}`;
}

/** 后端条目 → 相似行卡片 MediaItem（海报指向 Primary 端点，无图时 MediaPoster 兜底）。 */
function similarToMediaItem(dto: BaseItemDto): MediaItem {
  const detailType = detailTypeOf(dto.Type);
  return {
    id: dto.Id,
    libraryId: detailType === "tv" ? "series" : "movie",
    detailType,
    title: dto.Name,
    meta: cardMeta(dto),
    poster: mediaImageUrl(`/Items/${dto.Id}/Images/Primary`),
    year: dto.ProductionYear ?? undefined,
    rating: dto.CommunityRating ?? undefined,
    isFavorite: dto.UserData?.IsFavorite ?? false,
    isPlayed: dto.UserData?.Played ?? false,
    itemType: dto.Type,
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

/** 单个 MediaSource 视图模型 → 播放版本（规格 tag + 字幕语言 + 版本标签）。 */
function sourceToVersion(info: MediaSourceInfo, index: number): MediaVersion {
  const subtitles = info.subtitles
    .map((sub) => sub.language || sub.title || "未知")
    .filter((v, i, arr) => arr.indexOf(v) === i);
  return {
    id: info.id,
    label: sourceLabel(info, index),
    tags: sourceBadges(info),
    subtitles,
    info,
  };
}

/**
 * 从 PlaybackInfo 取完整 MediaSources（多版本 = 多条）。失败或无流时返回空数组，
 * 详情页的版本选择与媒体信息面板随之隐藏，不伪造。
 */
export async function fetchMediaSources(id: string): Promise<MediaSourceInfo[]> {
  try {
    const { data } = await embyRequest.get<PlaybackInfoResponseDto>(`/Items/${id}/PlaybackInfo`);
    return (data.MediaSources ?? []).map((source, index) => toMediaSourceInfo(source, index));
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

  // 剧集 / 季没有自己的媒体源，PlaybackInfo 必然 404（版本属于具体分集），
  // 跳过请求避免控制台 404 噪音；单集与电影 / 视频类才取媒体源。
  const hasOwnMedia = item.Type !== "Series" && item.Type !== "Season";
  const [imageTypes, similar, mediaSources] = await Promise.all([
    fetchImageTypes(id),
    item.Type === "Episode" ? Promise.resolve([]) : fetchSimilar(id),
    hasOwnMedia ? fetchMediaSources(id) : Promise.resolve([]),
  ]);

  const runtimeSeconds = ticksToSeconds(item.RunTimeTicks);
  const meta = [
    item.ProductionYear ? String(item.ProductionYear) : "",
    formatRuntime(runtimeSeconds),
  ].filter(Boolean);

  const people = item.People ?? [];
  const directors = peopleRoleNames(people, "Director");
  const writers = peopleRoleNames(people, "Writer");

  // 续播位置：有进度且未看完才给（已看完显示"重新播放"而不是续播）。
  const positionSeconds = ticksToSeconds(item.UserData?.PlaybackPositionTicks);
  const played = item.UserData?.Played ?? false;
  const resumePositionSeconds =
    !played && positionSeconds && positionSeconds > 30 ? positionSeconds : undefined;

  return {
    source: "backend",
    id: item.Id,
    detailType,
    itemType: item.Type,
    parentId: item.ParentId ?? undefined,
    seasonNumber:
      item.Type === "Season"
        ? (item.IndexNumber ?? undefined)
        : (item.ParentIndexNumber ?? undefined),
    episodeNumber: item.Type === "Episode" ? (item.IndexNumber ?? undefined) : undefined,
    premiereDate: item.PremiereDate?.slice(0, 10) ?? undefined,
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
    writers: writers || undefined,
    originalTitle: item.OriginalTitle ?? undefined,
    isFavorite: item.UserData?.IsFavorite ?? false,
    played,
    resumePositionSeconds,
    cast: peopleToCast(people),
    similar,
    versions: mediaSources.map(sourceToVersion),
    mediaSources,
    chapterCount: item.Chapters?.length ?? 0,
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
          includeItemTypes: "Movie,Series,Video,Episode",
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
    meta: cardMeta(dto),
    poster: dto.ImageTags?.Primary ? mediaImageUrl(`/Items/${dto.Id}/Images/Primary`) : undefined,
    year: dto.ProductionYear ?? undefined,
    rating: dto.CommunityRating ?? undefined,
    isFavorite: dto.UserData?.IsFavorite ?? false,
    isPlayed: dto.UserData?.Played ?? false,
    itemType: dto.Type,
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

/** 加载合集详情与成员，全部来自 Rust 后端 `/Collections/*`。
 * 详情与成员分请求：成员成功即可浏览/播放，不因详情 404 整页回退到列表。
 */
export async function loadCollectionDetail(
  routeId: string,
): Promise<{ collection: CollectionDetail; parts: MediaItem[] } | undefined> {
  const id = encodeURIComponent(routeId);
  let item: BaseItemDto | undefined;
  let parts: MediaItem[] = [];

  try {
    const { data } = await embyRequest.get<BaseItemDto>(`/Collections/${id}`);
    item = data;
  } catch {
    item = undefined;
  }

  try {
    const { data } = await embyRequest.get<QueryResultDto<BaseItemDto>>(
      `/Collections/${id}/Items`,
      { params: { Limit: 100, EnableImages: true } },
    );
    parts = (data.Items ?? []).map(collectionPartToMediaItem);
  } catch {
    parts = [];
  }

  // 详情与成员皆失败 → 真缺失；否则仍展示合集壳，便于打开子项并播放。
  if (!item && !parts.length) return undefined;

  const title = item?.Name ?? "合集";
  const publicId = item?.Id ?? routeId;
  const firstPoster = parts.find((part) => part.poster)?.poster ?? null;
  return {
    collection: {
      key: `collection:${publicId}`,
      id: Number.isFinite(Number(publicId)) ? Number(publicId) : 0,
      type: "collection",
      title,
      overview: item?.Overview ?? "",
      poster_path: firstPoster,
      backdrop_path: null,
      parts: [],
    },
    parts,
  };
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

/** 播放源：交给 webplayer 的播放地址、Range 中继与进度会话。 */
export interface PlaybackSourceResult {
  uri: string;
  mimeType?: string;
  size?: number;
  proxyUri?: string;
  server?: { playSessionId: string; mediaSourceId: string; startTicks: number };
  mediaSources?: { id: string; label: string }[];
  chapters?: import("@/stores/playback.ts").PlaybackChapter[];
  duration?: number;
  /** PlaybackInfo 外挂字幕轨（DeliveryUrl）；播放器据此挂轨。 */
  subtitleTracks?: import("@/stores/playback.ts").PlaybackTrack[];
  /** 当前所选 MediaSource 的技术信息（播放器「媒体信息」面板）。 */
  mediaInfo?: MediaSourceInfo;
}

export async function fetchPlaybackSource(
  itemId: string,
  mediaSourceId?: string,
): Promise<PlaybackSourceResult | null> {
  try {
    const { itemDetail, preparePlayback } = await import("./server.ts");
    const item = await itemDetail(itemId);
    const prepared = await preparePlayback(item, mediaSourceId);
    return {
      ...prepared.source,
      server: prepared.server,
      mediaSources: prepared.mediaSources,
      duration: prepared.duration,
      chapters: prepared.chapters,
      subtitleTracks: prepared.subtitleTracks,
      mediaInfo: prepared.mediaInfo,
    };
  } catch {
    return null;
  }
}

/**
 * 管理员删除媒体条目。对接 Emby `DELETE /Items/{id}`（库内软删；默认不删磁盘文件）。
 * 右键「删除」走此接口；`DeleteFiles=true` 留给后续确认流。
 */
export async function deleteMediaItem(itemId: string, deleteFiles = false): Promise<void> {
  await embyRequest.delete(`/Items/${encodeURIComponent(itemId)}`, {
    params: { DeleteFiles: deleteFiles },
  });
}

/**
 * 单集 / 季的剧集上下文（SeriesId / SeriesName）。Emby BaseItemDto 未下发 SeriesId，
 * 走受权限保护的 presentation 补齐接口；失败返回空对象（页面退化为无面包屑）。
 */
export async function fetchSeriesContext(
  itemId: string,
): Promise<{ seriesId?: string; seriesName?: string }> {
  try {
    const { enrich } = await import("./server.ts");
    const [item] = await enrich([{ Id: itemId, Name: "", Type: "Episode" }]);
    return { seriesId: item?.SeriesId, seriesName: item?.SeriesName };
  } catch {
    return {};
  }
}
