import type {
  CatalogItem,
  CollectionDetail,
  ContinueItem,
  DetailRecord,
  FeaturedItem,
  MediaItem,
  MediaVersion,
  MovieDetail,
  PersonDetail,
  RefItem,
  TmdbCatalog,
  TvDetail,
} from "@/types/media.ts";
import catalogJson from "@/service/modules/tmdb-catalog.json";

/**
 * TMDB 数据访问层（设计阶段从烤好的 JSON 读取）。
 * - 目录 catalog 随包加载（首页/媒体库网格用）。
 * - 详情 details 体积大，按需动态 import（详情页用），只下载一次。
 * 接入后端后，把这些函数换成对 fbz-api 的请求即可，页面消费方不变。
 */

const catalog = catalogJson as TmdbCatalog;
const IMAGE_BASE = catalog.image_base;

export const catalogItems = catalog.items;

/** TMDB 图片地址拼接。size 例：w200 / w500 / w780 / w1280 / original */
export function imageUrl(path: string | null | undefined, size = "w500"): string | undefined {
  if (!path) return undefined;
  return `${IMAGE_BASE}/${size}${path}`;
}

/* ---------- 详情懒加载 ---------- */
let detailsCache: Record<string, DetailRecord> | undefined;

async function loadDetails(): Promise<Record<string, DetailRecord>> {
  if (!detailsCache) {
    const mod = await import("@/service/modules/tmdb-details.json");
    detailsCache = mod.default as Record<string, DetailRecord>;
  }
  return detailsCache;
}

export async function getMovieDetail(id: number): Promise<MovieDetail | undefined> {
  const d = await loadDetails();
  return d[`movie:${id}`] as MovieDetail | undefined;
}
export async function getTvDetail(id: number): Promise<TvDetail | undefined> {
  const d = await loadDetails();
  return d[`tv:${id}`] as TvDetail | undefined;
}
export async function getCollectionDetail(id: number): Promise<CollectionDetail | undefined> {
  const d = await loadDetails();
  return d[`collection:${id}`] as CollectionDetail | undefined;
}
export async function getPersonDetail(id: number): Promise<PersonDetail | undefined> {
  const d = await loadDetails();
  return d[`person:${id}`] as PersonDetail | undefined;
}

/* ---------- 目录查找 ---------- */
export function findCatalogItem(type: "movie" | "tv", id: number): CatalogItem | undefined {
  return catalogItems.find((it) => it.type === type && it.id === id);
}

/* ---------- 适配为页面通用的 MediaItem（卡片用） ---------- */
export function catalogToItem(it: CatalogItem): MediaItem {
  return {
    id: String(it.id),
    libraryId: it.libraryId,
    detailType: it.type,
    title: it.title,
    meta: `${it.year ?? "—"} · ${it.genres[0] ?? (it.type === "tv" ? "剧集" : "电影")}`,
    poster: imageUrl(it.poster_path),
    year: it.year ?? undefined,
    genre: it.genres[0],
    rating: it.rating ?? undefined,
    resolution: resolutionFor(it.id),
  };
}

export function refToItem(r: RefItem): MediaItem {
  return {
    id: String(r.id),
    libraryId: r.libraryId,
    detailType: r.type,
    title: r.title,
    meta: `${r.year ?? "—"} · ${r.type === "tv" ? "剧集" : "电影"}`,
    poster: imageUrl(r.poster_path),
    year: r.year ?? undefined,
    rating: r.rating ?? undefined,
  };
}

/* ---------- 按库取条目 ---------- */
export function itemsByLibrary(libraryId: string): MediaItem[] {
  return catalogItems.filter((it) => it.libraryId === libraryId).map(catalogToItem);
}

/** 各库真实条目数（用于 store / 总览页） */
export function libraryCounts(): Record<string, number> {
  const counts: Record<string, number> = {};
  for (const it of catalogItems) counts[it.libraryId] = (counts[it.libraryId] ?? 0) + 1;
  return counts;
}

/* ---------- 合成清晰度（TMDB 不提供，按 id 稳定分配，演示用） ---------- */
const RESOLUTIONS = ["4K", "2K", "1080P", "720P"] as const;

/** 清晰度配色（借鉴 HDHive） */
export const resolutionColors: Record<string, string> = {
  "4K": "#4ade80",
  "2K": "#a3e635",
  "1080P": "#3b82f6",
  "720P": "#fb923c",
};

/** 按 id 稳定地给一个作品分配最高清晰度 */
export function resolutionFor(id: number | string): string {
  const n = typeof id === "number" ? id : Number(id) || 0;
  // 偏向高清：4K/2K/1080P 概率更高
  return RESOLUTIONS[n % 4 === 3 ? 2 : n % 4];
}

/* ---------- 合成播放版本 / 规格 / 字幕（TMDB 不提供，演示用） ---------- */
const VERSION_PRESETS: MediaVersion[] = [
  {
    id: "uhd",
    label: "4K 原盘 · REMUX",
    tags: ["4K", "HDR10", "DTS-HD MA 7.1", "Dolby Atmos"],
    subtitles: ["简体中文", "繁体中文", "English"],
  },
  {
    id: "1080p",
    label: "1080p · BluRay",
    tags: ["1080p", "DTS-HD MA 5.1"],
    subtitles: ["简体中文", "English"],
  },
  {
    id: "web",
    label: "1080p · WEB-DL",
    tags: ["1080p", "AAC 2.0"],
    subtitles: ["简体中文"],
  },
];

/** 按 id 稳定地给一个作品分配 1–3 个版本 */
export function versionsFor(id: number): MediaVersion[] {
  const n = (id % 3) + 1;
  return VERSION_PRESETS.slice(0, n);
}

/* ---------- 首页数据 ---------- */
const movieCatalog = catalogItems.filter((it) => it.libraryId === "movie");
const seriesCatalog = catalogItems.filter((it) => it.libraryId === "series");

/** hero 轮播：评分高 + 有 backdrop 的电影 */
export const homeFeatured: FeaturedItem[] = movieCatalog
  .filter((m) => m.backdrop_path)
  .sort((a, b) => (b.rating ?? 0) - (a.rating ?? 0))
  .slice(0, 6)
  .map((m) => ({
    id: String(m.id),
    title: m.title,
    meta: ["电影", String(m.year ?? "—"), ...m.genres.slice(0, 1)].filter(Boolean),
    tags: m.genres.slice(0, 3),
    overview: m.overview,
    backdrop: imageUrl(m.backdrop_path, "w1280"),
    thumb: imageUrl(m.poster_path, "w200"),
  }));

export const homeMovies: MediaItem[] = movieCatalog.slice(0, 20).map(catalogToItem);
export const homeSeries: MediaItem[] = seriesCatalog.slice(0, 20).map(catalogToItem);

/**
 * 继续观看：附演示进度。
 */
export const homeContinue: ContinueItem[] = [
  ...seriesCatalog.slice(20, 22),
  ...movieCatalog.slice(20, 22),
].map((it, i) => ({ ...catalogToItem(it), progress: [68, 31, 44, 12][i] }));
