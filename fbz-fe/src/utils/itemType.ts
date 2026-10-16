/**
 * Emby 条目 `Type` → C 端路由 / 图标 / 文案。尊重真实类型（Movie / Series / Season / Episode /
 * BoxSet / MusicAlbum / MusicArtist / Audio / Book / AudioBook / Person / Video / Photo …），
 * 不再把「非剧集」一律当 movie。
 */

export interface ItemTypeMeta {
  label: string;
  /** BaseIcon 名称（Lucide）。 */
  icon: string;
}

const META: Record<string, ItemTypeMeta> = {
  Movie: { label: "电影", icon: "movie" },
  Series: { label: "剧集", icon: "tv" },
  Season: { label: "季", icon: "tv" },
  Episode: { label: "单集", icon: "tv-minimal" },
  BoxSet: { label: "合集", icon: "collection" },
  Playlist: { label: "播放列表", icon: "playlist" },
  MusicAlbum: { label: "专辑", icon: "album" },
  MusicArtist: { label: "艺术家", icon: "artist" },
  Audio: { label: "曲目", icon: "music" },
  Book: { label: "电子书", icon: "book" },
  AudioBook: { label: "有声书", icon: "audiobook" },
  Person: { label: "人物", icon: "user-round" },
  Video: { label: "视频", icon: "video" },
  MusicVideo: { label: "音乐视频", icon: "video" },
  Trailer: { label: "预告片", icon: "clapperboard" },
  Photo: { label: "照片", icon: "photo" },
  TvChannel: { label: "频道", icon: "livetv" },
  Program: { label: "节目", icon: "livetv" },
  Recording: { label: "录像", icon: "livetv" },
  Folder: { label: "文件夹", icon: "folder" },
};

export function itemTypeMeta(type: string | null | undefined): ItemTypeMeta {
  return (type && META[type]) || { label: "条目", icon: "file" };
}

/** 详情页壳类型。`movie` 壳也承载通用视频（Video / MusicVideo / Trailer / Photo）。 */
export type DetailShell =
  | "movie"
  | "tv"
  | "collection"
  | "album"
  | "artist"
  | "book"
  | "person"
  | "playlist";

export function detailShellFor(type: string | null | undefined): DetailShell {
  switch (type) {
    case "Series":
    case "Season":
    case "Episode":
      return "tv";
    case "BoxSet":
    case "Collection":
      return "collection";
    case "MusicAlbum":
    case "Audio":
      return "album";
    case "MusicArtist":
      return "artist";
    case "Book":
    case "AudioBook":
      return "book";
    case "Person":
      return "person";
    case "Playlist":
      return "playlist";
    default:
      return "movie";
  }
}

/** 条目详情路由。Audio 没有独立详情页，跳所属专辑（需 parentId）；缺省回到专辑 id 本身。 */
export function itemRoute(type: string | null | undefined, id: string, parentId?: string): string {
  const shell = detailShellFor(type);
  switch (shell) {
    case "album":
      return `/album/${type === "Audio" && parentId ? parentId : id}`;
    case "book":
      return `/book/${id}`;
    case "person":
      return `/person/${encodeURIComponent(id)}`;
    case "playlist":
      return `/playlists/${id}`;
    default:
      return `/${shell}/${id}`;
  }
}

/** 季/集号 → `S01E02`；缺省部分省略。 */
export function episodeCode(season?: number | null, episode?: number | null): string {
  const s = season != null ? `S${String(season).padStart(2, "0")}` : "";
  const e = episode != null ? `E${String(episode).padStart(2, "0")}` : "";
  return `${s}${e}`;
}
