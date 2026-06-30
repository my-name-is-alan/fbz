/**
 * 音乐浏览 service：对接 BFF `GET /api/music/*`（artist → album → track 三级下钻）。
 *
 * 与 navigation.ts 同风格：走 `/api` 面的 {@link request} 单例（自动带 `x-emby-token`），
 * 把后端原始 DTO 的 poster 路径经 {@link mediaImageUrl} 拼成带鉴权的绝对地址，
 * 让视图消费方无需感知后端形状与鉴权细节。
 */
import { mediaImageUrl, request } from "@/service/request.ts";
import type {
  MusicAlbum,
  MusicAlbumDetail,
  MusicArtistDetail,
  MusicArtistList,
} from "@/types/music.ts";

/** 拼好专辑/封面绝对地址（后端给的是服务器根路径）。 */
function withPoster<T extends { poster?: string }>(item: T): T {
  return { ...item, poster: mediaImageUrl(item.poster) };
}

/** 列出某音乐库下的艺术家。 */
export async function fetchArtists(libraryId: string): Promise<MusicArtistList> {
  const { data } = await request.get<MusicArtistList>("/music/artists", {
    params: { libraryId },
  });
  return data;
}

/** 艺术家详情 + 名下专辑（封面拼绝对地址）。 */
export async function fetchArtistDetail(artistId: string): Promise<MusicArtistDetail> {
  const { data } = await request.get<MusicArtistDetail>(
    `/music/artists/${encodeURIComponent(artistId)}`,
  );
  return { ...data, albums: data.albums.map(withPoster) };
}

/** 专辑详情 + 内含曲目（封面拼绝对地址）。 */
export async function fetchAlbumDetail(albumId: string): Promise<MusicAlbumDetail> {
  const { data } = await request.get<MusicAlbumDetail>(
    `/music/albums/${encodeURIComponent(albumId)}`,
  );
  return withPoster(data) as MusicAlbumDetail;
}

/** 把曲目时长（秒）格式化为 `m:ss`；缺省返回空串。 */
export function formatDuration(seconds: number | undefined): string {
  if (seconds == null || seconds <= 0) return "";
  const m = Math.floor(seconds / 60);
  const s = seconds % 60;
  return `${m}:${String(s).padStart(2, "0")}`;
}

/** 收集专辑列表里的年份范围标签（如 "2010 – 2020"），无年份返回空串。 */
export function albumYearRange(albums: MusicAlbum[]): string {
  const years = albums.map((a) => a.year).filter((y): y is number => y != null);
  if (!years.length) return "";
  const min = Math.min(...years);
  const max = Math.max(...years);
  return min === max ? String(min) : `${min} – ${max}`;
}
