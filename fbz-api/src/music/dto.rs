//! 音乐浏览 BFF 的对外 DTO（`camelCase`，对齐前端 `fbz-fe/src/types/media.ts`）。
//!
//! 与 navigation DTO 同风格：图片走服务器根路径 `/Items/{id}/Images/Primary`（前端按需补
//! `api_key`），无图时省略字段交前端渲染占位块。

use serde::Serialize;

/// `GET /api/music/artists?libraryId=` 的响应：某音乐库下的艺术家列表。
#[derive(Clone, Debug, Default, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ArtistListDto {
    pub items: Vec<ArtistDto>,
    /// 命中总数（分页用），近似下界见 repository。
    pub total: u32,
}

/// 一个艺术家。
#[derive(Clone, Debug, Default, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ArtistDto {
    /// 艺术家对外标识（`public_id`），用于 `GET /api/music/artists/:id` 下钻专辑。
    pub id: String,
    pub name: String,
}

/// `GET /api/music/artists/:id` 的响应：艺术家详情 + 其专辑列表。
#[derive(Clone, Debug, Default, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ArtistDetailDto {
    pub id: String,
    pub name: String,
    /// 该艺术家名下专辑（按发行年/名排序）。
    pub albums: Vec<AlbumDto>,
}

/// 一张专辑。
#[derive(Clone, Debug, Default, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AlbumDto {
    /// 专辑对外标识，用于 `GET /api/music/albums/:id` 下钻曲目。
    pub id: String,
    pub title: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub year: Option<i32>,
    /// 封面地址（服务器根路径）；为空时前端渲染占位块。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub poster: Option<String>,
}

/// `GET /api/music/albums/:id` 的响应：专辑详情 + 其曲目列表。
#[derive(Clone, Debug, Default, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AlbumDetailDto {
    pub id: String,
    pub title: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub year: Option<i32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub poster: Option<String>,
    /// 专辑内曲目（按音轨号排序）。
    pub tracks: Vec<TrackDto>,
}

/// 一首曲目。
#[derive(Clone, Debug, Default, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TrackDto {
    /// 曲目对外标识，用于播放/取流。
    pub id: String,
    pub title: String,
    /// 时长（秒）；ffprobe 跑完后才有，缺省省略。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub duration: Option<i64>,
}
