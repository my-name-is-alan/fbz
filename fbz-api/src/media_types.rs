//! 媒体类型分类法的单一事实源（single source of truth）。
//!
//! 库类型（[`LibraryType`]）与条目类型（[`ItemType`]）的全部取值、字符串表示、
//! 分类规则、Emby `CollectionType` 映射都集中在本模块。其他层一律从这里派生：
//!
//! - DB CHECK 约束的取值集合由 [`LibraryType::ALL`] / [`ItemType::ALL`] 守卫（见单测）。
//! - admin API 校验走 [`LibraryType::parse`]，不再手写 allowlist。
//! - 扫描分类走 [`ItemType::classify`]，不再把非音频一律当 `movie`。
//! - Emby 边界走 [`LibraryType::collection_type`]，消除 `library_type` 原样透传的歧义。
//!
//! 新增一种媒体类型 = 在本模块加一个枚举分支并补齐其映射，编译器会强制所有 `match`
//! 跟进，杜绝「散落各处写死字符串、漏改一处就出 bug」。

use std::path::Path;

use serde::{Deserialize, Serialize};

/// 媒体库类型，与 Emby `CollectionType` 词汇表一一对齐。
///
/// 存储为 `libraries.library_type` 文本列；序列化为 Emby 风格小写串
/// （`movies`/`tvshows`/`music`/`homevideos`/`mixed`/`livetv`）。
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum LibraryType {
    /// 电影库。
    Movies,
    /// 剧集库（对齐 Emby `tvshows`）。
    TvShows,
    /// 音乐库。
    Music,
    /// 家庭视频 + 照片（视频/图片混合）。
    HomeVideos,
    /// 电影 + 剧集混合库。
    Mixed,
    /// 直播电视入口（频道/EPG/录制的数据模型另立子系统，此处仅占类型位）。
    LiveTv,
}

impl LibraryType {
    /// 全部库类型，顺序即 UI/文档展示顺序；DB CHECK 取值集合以此为准。
    pub const ALL: [LibraryType; 6] = [
        LibraryType::Movies,
        LibraryType::TvShows,
        LibraryType::Music,
        LibraryType::HomeVideos,
        LibraryType::Mixed,
        LibraryType::LiveTv,
    ];

    /// 存储/传输用的规范字符串（与 Emby `CollectionType` 词汇一致）。
    pub const fn as_str(self) -> &'static str {
        match self {
            LibraryType::Movies => "movies",
            LibraryType::TvShows => "tvshows",
            LibraryType::Music => "music",
            LibraryType::HomeVideos => "homevideos",
            LibraryType::Mixed => "mixed",
            LibraryType::LiveTv => "livetv",
        }
    }

    /// 从存储字符串解析；未知值返回 `None`（调用方决定退化或报错）。
    pub fn parse(value: &str) -> Option<LibraryType> {
        LibraryType::ALL
            .into_iter()
            .find(|kind| kind.as_str() == value)
    }

    /// 该库映射到 Emby 客户端的 `CollectionType`。
    ///
    /// 当前与 [`as_str`](Self::as_str) 恒等（库类型已对齐 Emby 词汇），但保留独立方法，
    /// 以便将来内部存储与对外传输词汇分叉时只改这一处。
    pub const fn collection_type(self) -> &'static str {
        self.as_str()
    }

    /// 该库默认产出的「主」条目类型，用于识别失败时的退化分类。
    pub const fn default_item_type(self) -> ItemType {
        match self {
            LibraryType::Movies => ItemType::Movie,
            LibraryType::TvShows => ItemType::Episode,
            LibraryType::Music => ItemType::Track,
            LibraryType::HomeVideos => ItemType::Video,
            LibraryType::Mixed => ItemType::Movie,
            LibraryType::LiveTv => ItemType::TvChannel,
        }
    }
}

/// 媒体条目类型，存储为 `media_items.item_type` 文本列（内部小写词汇）。
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum ItemType {
    /// 普通目录容器。
    Folder,
    /// 电影。
    Movie,
    /// 剧集（容器）。
    Series,
    /// 季（容器）。
    Season,
    /// 单集。
    Episode,
    /// 音乐艺术家（容器）。
    Artist,
    /// 专辑（容器）。
    Album,
    /// 音轨。
    Track,
    /// 合集/系列（如电影系列）。
    Collection,
    /// 图片（家庭库）。
    Photo,
    /// 通用视频片段/家庭录像（无电影语义）。
    Video,
    /// 直播频道（livetv，预留）。
    TvChannel,
    /// EPG 节目（livetv，预留）。
    Program,
    /// 录制节目（livetv，预留）。
    Recording,
}

impl ItemType {
    /// 全部条目类型；DB CHECK 取值集合以此为准。
    pub const ALL: [ItemType; 14] = [
        ItemType::Folder,
        ItemType::Movie,
        ItemType::Series,
        ItemType::Season,
        ItemType::Episode,
        ItemType::Artist,
        ItemType::Album,
        ItemType::Track,
        ItemType::Collection,
        ItemType::Photo,
        ItemType::Video,
        ItemType::TvChannel,
        ItemType::Program,
        ItemType::Recording,
    ];

    /// 存储用的规范字符串。
    pub const fn as_str(self) -> &'static str {
        match self {
            ItemType::Folder => "folder",
            ItemType::Movie => "movie",
            ItemType::Series => "series",
            ItemType::Season => "season",
            ItemType::Episode => "episode",
            ItemType::Artist => "artist",
            ItemType::Album => "album",
            ItemType::Track => "track",
            ItemType::Collection => "collection",
            ItemType::Photo => "photo",
            ItemType::Video => "video",
            ItemType::TvChannel => "tvchannel",
            ItemType::Program => "program",
            ItemType::Recording => "recording",
        }
    }

    /// 从存储字符串解析。
    pub fn parse(value: &str) -> Option<ItemType> {
        ItemType::ALL
            .into_iter()
            .find(|kind| kind.as_str() == value)
    }

    /// 按库类型 + 文件路径分类出条目类型。
    ///
    /// 取代旧的 `item_type_for_file`（把非音频一律当 `movie`）。规则：
    /// - 音乐库：一律 `track`。
    /// - 任意库中的音频文件：`track`。
    /// - 家庭库（`homevideos`）：图片→`photo`，其余视频→`video`（不强行当电影）。
    /// - 其余库的视频/未知文件：退化到该库的默认条目类型。
    pub fn classify(library_type: LibraryType, path: &Path) -> ItemType {
        let category = MediaCategory::from_path(path);
        match (library_type, category) {
            (LibraryType::Music, _) => ItemType::Track,
            (_, Some(MediaCategory::Audio)) => ItemType::Track,
            (LibraryType::HomeVideos, Some(MediaCategory::Photo)) => ItemType::Photo,
            (LibraryType::HomeVideos, _) => ItemType::Video,
            (_, Some(MediaCategory::Photo)) => ItemType::Photo,
            (library_type, _) => library_type.default_item_type(),
        }
    }
}

/// 文件按扩展名归入的媒体大类，是扫描分类与「是否纳入扫描」的统一判据。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MediaCategory {
    /// 音频。
    Audio,
    /// 视频。
    Video,
    /// 图片。
    Photo,
}

impl MediaCategory {
    /// 由文件扩展名判定大类；无法识别的扩展名返回 `None`。
    pub fn from_path(path: &Path) -> Option<MediaCategory> {
        let ext = path.extension()?.to_str()?.to_ascii_lowercase();
        Self::from_extension(&ext)
    }

    /// 由已归一化（小写、无点）的扩展名判定大类。
    pub fn from_extension(ext: &str) -> Option<MediaCategory> {
        if VIDEO_EXTENSIONS.contains(&ext) {
            Some(MediaCategory::Video)
        } else if AUDIO_EXTENSIONS.contains(&ext) {
            Some(MediaCategory::Audio)
        } else if PHOTO_EXTENSIONS.contains(&ext) {
            Some(MediaCategory::Photo)
        } else {
            None
        }
    }
}

/// 受支持的视频扩展名（含 `.strm` 间接流）。
pub const VIDEO_EXTENSIONS: &[&str] = &["mkv", "mp4", "avi", "mov", "m4v", "ts", "strm"];

/// 受支持的音频扩展名。
pub const AUDIO_EXTENSIONS: &[&str] = &["mp3", "flac", "m4a", "wav", "ogg"];

/// 受支持的图片扩展名（家庭库照片）。
pub const PHOTO_EXTENSIONS: &[&str] = &[
    "jpg", "jpeg", "png", "gif", "webp", "heic", "heif", "bmp", "tiff",
];

/// 由 ffprobe 实测视频高度归一化为标准分辨率标签（probe 校正用，与文件名识别词典互补）。
/// 取与相邻标准档的中点为界，容忍编码 padding（如 1080p 常编码为 1088）：
/// ≥1601→2160p、≥1261→1440p、≥721→1080p（含 1080/1088）、≥577→720p、≥1→480p。
/// 返回与 recognition `QualityTags.resolution` 同词汇，便于冲突校正。
pub fn resolution_from_height(height: i32) -> Option<&'static str> {
    match height {
        h if h >= 1601 => Some("2160p"),
        h if h >= 1261 => Some("1440p"),
        h if h >= 721 => Some("1080p"),
        h if h >= 577 => Some("720p"),
        h if h >= 1 => Some("480p"),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 守卫 DB CHECK 漂移：这两个清单必须与 migrations/0002 的 CHECK 取值逐字一致。
    /// 改动任一侧都要同步另一侧，否则插入会被约束拒绝或校验放过非法值。
    #[test]
    fn library_type_check_values_match_schema() {
        let values: Vec<&str> = LibraryType::ALL.iter().map(|k| k.as_str()).collect();
        assert_eq!(
            values,
            [
                "movies",
                "tvshows",
                "music",
                "homevideos",
                "mixed",
                "livetv"
            ]
        );
    }

    #[test]
    fn item_type_check_values_match_schema() {
        let values: Vec<&str> = ItemType::ALL.iter().map(|k| k.as_str()).collect();
        assert_eq!(
            values,
            [
                "folder",
                "movie",
                "series",
                "season",
                "episode",
                "artist",
                "album",
                "track",
                "collection",
                "photo",
                "video",
                "tvchannel",
                "program",
                "recording"
            ]
        );
    }

    #[test]
    fn library_type_round_trips_through_string() {
        for kind in LibraryType::ALL {
            assert_eq!(LibraryType::parse(kind.as_str()), Some(kind));
        }
        assert_eq!(LibraryType::parse("books"), None);
        assert_eq!(
            LibraryType::parse("tv"),
            None,
            "legacy 'tv' must be rejected"
        );
    }

    #[test]
    fn item_type_round_trips_through_string() {
        for kind in ItemType::ALL {
            assert_eq!(ItemType::parse(kind.as_str()), Some(kind));
        }
        assert_eq!(ItemType::parse("unknown"), None);
    }

    #[test]
    fn collection_type_is_emby_aligned() {
        assert_eq!(LibraryType::TvShows.collection_type(), "tvshows");
        assert_eq!(LibraryType::HomeVideos.collection_type(), "homevideos");
        assert_eq!(LibraryType::LiveTv.collection_type(), "livetv");
    }

    #[test]
    fn classify_routes_files_by_library_and_extension() {
        // 音乐库一律 track。
        assert_eq!(
            ItemType::classify(LibraryType::Music, Path::new("a.flac")),
            ItemType::Track
        );
        // 任意库的音频 → track。
        assert_eq!(
            ItemType::classify(LibraryType::Movies, Path::new("ost.mp3")),
            ItemType::Track
        );
        // 家庭库图片 → photo，视频 → video（不当电影）。
        assert_eq!(
            ItemType::classify(LibraryType::HomeVideos, Path::new("IMG_0001.jpg")),
            ItemType::Photo
        );
        assert_eq!(
            ItemType::classify(LibraryType::HomeVideos, Path::new("clip.mp4")),
            ItemType::Video
        );
        // 电影库视频 → movie（退化到默认）。
        assert_eq!(
            ItemType::classify(LibraryType::Movies, Path::new("Inception.2010.mkv")),
            ItemType::Movie
        );
        // 剧集库视频 → episode（默认）。
        assert_eq!(
            ItemType::classify(LibraryType::TvShows, Path::new("show.s01e01.mkv")),
            ItemType::Episode
        );
    }

    #[test]
    fn category_classifies_known_extensions() {
        assert_eq!(
            MediaCategory::from_extension("mkv"),
            Some(MediaCategory::Video)
        );
        assert_eq!(
            MediaCategory::from_extension("flac"),
            Some(MediaCategory::Audio)
        );
        assert_eq!(
            MediaCategory::from_extension("heic"),
            Some(MediaCategory::Photo)
        );
        assert_eq!(MediaCategory::from_extension("txt"), None);
    }

    #[test]
    fn resolution_from_height_maps_standard_tiers() {
        assert_eq!(resolution_from_height(2160), Some("2160p"));
        assert_eq!(resolution_from_height(1080), Some("1080p"));
        assert_eq!(resolution_from_height(1088), Some("1080p")); // 略高于 1080 仍算 1080p
        assert_eq!(resolution_from_height(720), Some("720p"));
        assert_eq!(resolution_from_height(480), Some("480p"));
        assert_eq!(resolution_from_height(0), None);
        assert_eq!(resolution_from_height(-1), None);
    }
}
