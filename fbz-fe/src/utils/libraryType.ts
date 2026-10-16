/**
 * 库类型 —— 以后端 `LibraryType::ALL` / Emby `CollectionType` 为唯一事实源
 * （`fbz-api/src/media_types.rs`）。前端只做展示映射（图标 / 文案 / 浏览条目类型 / 路由），
 * 不折叠、不改写类型语义：homevideos / mixed / livetv 各自保留。
 */
import type { MediaKind } from "@/types/media.ts";

export const LIBRARY_TYPES = [
  "movies",
  "tvshows",
  "music",
  "homevideos",
  "mixed",
  "livetv",
  "books",
] as const;

export type LibraryType = (typeof LIBRARY_TYPES)[number];

export interface LibraryTypeMeta {
  label: string;
  /** BaseIcon 名称（Lucide）。 */
  icon: string;
  /** 浏览该库时透传的 Emby `IncludeItemTypes`。 */
  itemTypes: string;
  /** 前端展示分组（MediaKind）。 */
  kind: MediaKind;
}

export const LIBRARY_TYPE_META: Record<LibraryType, LibraryTypeMeta> = {
  movies: { label: "电影", icon: "movie", itemTypes: "Movie", kind: "movie" },
  tvshows: { label: "剧集", icon: "tv", itemTypes: "Series", kind: "series" },
  music: { label: "音乐", icon: "music", itemTypes: "MusicAlbum", kind: "music" },
  homevideos: {
    label: "家庭视频",
    icon: "video",
    itemTypes: "Video,Photo,Movie",
    kind: "homevideo",
  },
  mixed: {
    label: "混合内容",
    icon: "mixed",
    itemTypes: "Movie,Series,Video,Photo",
    kind: "mixed",
  },
  livetv: {
    label: "直播",
    icon: "livetv",
    itemTypes: "TvChannel,Program,Recording",
    kind: "livetv",
  },
  books: { label: "图书", icon: "book", itemTypes: "Book,AudioBook", kind: "book" },
};

/** 未知 / 缺省类型的兜底：按 Emby 传统 Movie,Series 浏览，不臆造类型。 */
const UNKNOWN_META: LibraryTypeMeta = {
  label: "媒体库",
  icon: "library",
  itemTypes: "Movie,Series",
  kind: "movie",
};

/** 规范化后端返回的库类型字符串（大小写 / 旧别名）；未知值返回 undefined。 */
export function normalizeLibraryType(raw: string | null | undefined): LibraryType | undefined {
  if (!raw) return undefined;
  const value = raw.trim().toLowerCase();
  if ((LIBRARY_TYPES as readonly string[]).includes(value)) return value as LibraryType;
  switch (value) {
    case "movie":
      return "movies";
    case "series":
    case "tv":
    case "shows":
      return "tvshows";
    case "book":
      return "books";
    case "homevideo":
    case "photos":
      return "homevideos";
    default:
      return undefined;
  }
}

export function libraryTypeMeta(raw: string | null | undefined): LibraryTypeMeta {
  const type = normalizeLibraryType(raw);
  return type ? LIBRARY_TYPE_META[type] : UNKNOWN_META;
}

/** 库类型 → 展示 kind（不再把 homevideos/mixed/livetv 并进 movie/series）。 */
export function kindForLibraryType(raw: string | null | undefined): MediaKind {
  return libraryTypeMeta(raw).kind;
}
