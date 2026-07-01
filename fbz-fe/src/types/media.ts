/** 媒体库类型 —— 用户可创建任意多个、任意类型的库 */
export type MediaKind = "movie" | "series" | "anime" | "documentary" | "music";

export interface MediaLibrary {
  id: string;
  name: string;
  kind: MediaKind;
  /** 该库的条目数 */
  count: number;
  /** 后端规范库类型（movies / tvshows / music / mixed 等）。 */
  libraryType?: string;
  /** 后端物理路径配置；管理弹窗按需填充。 */
  paths?: string[];
  metadataLanguage?: string;
  metadataCountry?: string;
  imageLanguage?: string;
  preferOriginalPoster?: boolean;
  imageFallbackLanguages?: string[];
  isHidden?: boolean;
}

/** 媒体条目（电影 / 剧集等的统一展示模型） */
export interface MediaItem {
  id: string;
  libraryId: string;
  title: string;
  /** 年份 + 类型等副标题信息 */
  meta: string;
  /**
   * 详情页类型路由：movie → /movie/:id，tv → /tv/:id。
   * 与 libraryId 解耦（动漫库条目多为 tv，纪录片库多为 movie）。缺省按 movie。
   */
  detailType?: "movie" | "tv";
  /** 海报图地址；为空时前端渲染占位块 */
  poster?: string;
  /** 发行年份，用于排序/筛选 */
  year?: number;
  /** 题材，用于筛选，如「科幻」「剧情」 */
  genre?: string;
  /** 画质，用于筛选，如「4K」「1080p」 */
  resolution?: string;
  /** 评分 0–10，用于排序 */
  rating?: number;
  /** 入库时间戳（ms），用于按添加时间排序 */
  addedAt?: number;
  /** 用户收藏状态（来自后端 UserData / 右键菜单更新）。 */
  isFavorite?: boolean;
}

/** 单库列表的排序方式 */
export type SortKey = "title" | "year" | "rating";

/** 排序选项 */
export interface SortOption {
  key: SortKey;
  label: string;
}

/** 首页 hero 轮播项（最新入库主打） */
export interface FeaturedItem {
  id: string;
  title: string;
  /** 标题下的元信息片段，如 ["电影", "2025", "2h 46m"] */
  meta: string[];
  /** 规格标签，如 ["4K", "HDR10", "Atmos"] */
  tags: string[];
  overview: string;
  /** 背景剧照地址；为空时渲染占位块 */
  backdrop?: string;
  /** 缩略图地址；为空时渲染占位块 */
  thumb?: string;
}

/** 首页内容行 */
export interface MediaRow {
  id: string;
  title: string;
  layout: "poster" | "wide";
  items: ContinueItem[];
}

/** 继续观看条目（带进度） */
export interface ContinueItem extends MediaItem {
  /** 观看进度百分比 0–100 */
  progress?: number;
}

/* =========================================================
   TMDB 真实数据模型（与 tmdb-catalog.json / tmdb-details.json 对齐）
   图片字段都是 TMDB 相对路径（如 /abc.jpg），用 imageUrl() 拼完整地址
   catalog = 轻量目录（随包加载，用于首页/媒体库网格）
   details = 完整详情（懒加载，用于详情页），按 "type:id" 索引
   ========================================================= */

/** 详情页实体类型 → 对应路由 /movie /tv /person /collection */
export type DetailType = "movie" | "tv" | "person" | "collection";

/** 轻量目录条目 */
export interface CatalogItem {
  id: number;
  type: "movie" | "tv";
  /** 归属媒体库：movie/series/anime/documentary */
  libraryId: string;
  title: string;
  overview: string;
  poster_path: string | null;
  backdrop_path: string | null;
  year: number | null;
  rating: number | null;
  genres: string[];
}

export interface TmdbCatalog {
  generated_at: string;
  image_base: string;
  items: CatalogItem[];
}

/** 演员条目（影片演职表） */
export interface CastMember {
  id: number;
  name: string;
  character: string;
  profile_path: string | null;
  order: number;
  /** 人物详情路由 key：后端按人物名解析（/person/{name}），缺省回退到数字 id。 */
  linkId?: string;
}

/** 导演 / 主创 */
export interface CrewMember {
  id: number;
  name: string;
  job?: string;
}

/** 相似推荐 / 系列作品等的精简卡片数据 */
export interface RefItem {
  id: number;
  type: "movie" | "tv";
  libraryId: string;
  title: string;
  poster_path: string | null;
  year: number | null;
  rating: number | null;
}

/** 电影详情（details 文件，key = "movie:id"） */
export interface MovieDetail {
  key: string;
  runtime: number | null;
  tagline: string;
  original_title: string;
  collection_id: number | null;
  collection_name: string | null;
  cast: CastMember[];
  directors: CrewMember[];
  similar: RefItem[];
}

/** 季概要 */
export interface SeasonInfo {
  season_number: number;
  name: string;
  episode_count: number;
  air_date: string | null;
  poster_path: string | null;
  overview: string;
}

/** 剧集详情（details 文件，key = "tv:id"） */
export interface TvDetail {
  key: string;
  tagline: string;
  original_title: string;
  seasons_count: number;
  episodes_count: number;
  creators: CrewMember[];
  seasons: SeasonInfo[];
  cast: CastMember[];
  similar: RefItem[];
}

/** 系列 / 合集中的一部作品 */
export interface CollectionPart {
  id: number;
  title: string;
  poster_path: string | null;
  year: number | null;
  rating: number | null;
}

/** 系列 / 合集详情（details 文件，key = "collection:id"） */
export interface CollectionDetail {
  key: string;
  id: number;
  type: "collection";
  title: string;
  overview: string;
  poster_path: string | null;
  backdrop_path: string | null;
  parts: CollectionPart[];
}

/** 演员代表作 */
export interface PersonCredit {
  id: number;
  type: "movie" | "tv";
  libraryId: string;
  title: string;
  character: string;
  poster_path: string | null;
  year: number | null;
  rating: number | null;
}

/** 演员 / 人物详情（details 文件，key = "person:id"） */
export interface PersonDetail {
  key: string;
  id: number;
  type: "person";
  name: string;
  biography: string;
  profile_path: string | null;
  birthday: string | null;
  place_of_birth: string | null;
  known_for_department: string;
  known_for: PersonCredit[];
}

/** details 文件：key("type:id") → 详情对象 */
export type DetailRecord = MovieDetail | TvDetail | CollectionDetail | PersonDetail;

/* ---------- 前端合成的播放版本 / 规格（TMDB 不提供） ---------- */

/** 一个可播放版本（同一资源的不同来源/规格） */
export interface MediaVersion {
  id: string;
  label: string;
  /** 规格标签：4K / DTS-HD / Dolby Atmos 等 */
  tags: string[];
  /** 字幕语言 */
  subtitles: string[];
}
