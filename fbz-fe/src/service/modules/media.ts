import type { MediaItem, MediaLibrary } from "@/types/media.ts";

/**
 * 媒体库元信息 + 无 TMDB 数据时的占位条目（如音乐库）。
 * 影视类库的真实数据来自 service/modules/tmdb.ts；这里只保留库定义与兜底占位。
 * 真实接入后端后，由 service 层返回即可。
 */

export const libraries: MediaLibrary[] = [
  { id: "movie", name: "电影", kind: "movie", count: 0 },
  { id: "series", name: "剧集", kind: "series", count: 0 },
  { id: "anime", name: "动漫", kind: "anime", count: 0 },
  { id: "documentary", name: "纪录片", kind: "documentary", count: 0 },
  { id: "music", name: "音乐", kind: "music", count: 36 },
];

/** 题材池：仅用于无真实数据的库（占位筛选项） */
export const genrePool = ["流行", "摇滚", "电子", "古典", "嘻哈", "民谣"];

/** 占位条目：给没有 TMDB 数据的库（音乐等）生成网格演示数据 */
export function libraryItems(libraryId: string, count = 36): MediaItem[] {
  const lib = libraries.find((l) => l.id === libraryId);
  const name = lib?.name ?? "条目";
  return Array.from({ length: count }, (_, i) => {
    const year = 2015 + (i % 11);
    const genre = genrePool[i % genrePool.length];
    const rating = Number((9.6 - ((i * 7) % 40) / 10).toFixed(1));
    return {
      id: `${libraryId}-${i + 1}`,
      libraryId,
      title: `${name} ${i + 1}`,
      meta: `${year} · ${genre}`,
      year,
      genre,
      rating,
    };
  });
}
