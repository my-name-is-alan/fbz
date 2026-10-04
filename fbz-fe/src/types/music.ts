/** 音乐浏览 BFF (`GET /api/music/*`) 的响应类型（camelCase，对齐后端 `music/dto.rs`）。 */

/** 一个艺术家（列表项）。 */
export interface MusicArtist {
  id: string;
  name: string;
}

/** `GET /api/music/artists?libraryId=` 的响应。 */
export interface MusicArtistList {
  items: MusicArtist[];
  total: number;
}

/** 一张专辑（poster 为服务器根路径，需经 mediaImageUrl 拼绝对地址）。 */
export interface MusicAlbum {
  id: string;
  title: string;
  year?: number;
  poster?: string;
}

/** `GET /api/music/artists/:id` 的响应：艺术家详情 + 名下专辑。 */
export interface MusicArtistDetail {
  id: string;
  name: string;
  albums: MusicAlbum[];
}

/** 一首曲目。 */
export interface MusicTrack {
  id: string;
  title: string;
  /** 时长（秒）；ffprobe 跑完后才有。 */
  duration?: number;
}

/** `GET /api/music/albums/:id` 的响应：专辑详情 + 内含曲目。 */
export interface MusicAlbumDetail {
  id: string;
  title: string;
  year?: number;
  poster?: string;
  tracks: MusicTrack[];
}
