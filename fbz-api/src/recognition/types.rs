//! 媒体识别核心类型（design §4.1）。
//!
//! 这些是 `recognize()` 纯函数的输入/输出契约：输入一条路径链 + 库类型先验 +
//! 已编译识别词规则，输出结构化 [`RecognizedMedia`]。完全确定性、无 IO，便于穷举单测。

use crate::media_types::ItemType;

/// 识别核心的输入：一条文件路径拆为「文件名」与「祖先目录名链」+ 库类型先验。
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RecognitionInput<'a> {
    /// 不含扩展名的文件主名。
    pub file_stem: &'a str,
    /// 扩展名（小写、不含点），如 `mkv`。
    pub extension: Option<&'a str>,
    /// 从库根到文件的目录名链（不含库根、不含文件名），**近→远**顺序。
    /// 例：库根=/media/tv，文件=/media/tv/Friends/Season 02/x.mkv → ["Season 02", "Friends"]。
    pub ancestors: &'a [&'a str],
}

/// 识别出的媒体种类（design §4.1 的 `RecognizedKind`）。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RecognizedKind {
    Movie,
    Series,
    Season,
    Episode,
    Track,
    Photo,
    Video,
    Unknown,
}

impl RecognizedKind {
    /// 映射到存储用的 [`ItemType`]。`Unknown` 由上层按库默认决定，这里给 `Movie` 兜底。
    pub fn to_item_type(self) -> ItemType {
        match self {
            RecognizedKind::Movie => ItemType::Movie,
            RecognizedKind::Series => ItemType::Series,
            RecognizedKind::Season => ItemType::Season,
            RecognizedKind::Episode => ItemType::Episode,
            RecognizedKind::Track => ItemType::Track,
            RecognizedKind::Photo => ItemType::Photo,
            RecognizedKind::Video => ItemType::Video,
            RecognizedKind::Unknown => ItemType::Movie,
        }
    }
}

/// 题材/内容画像提示——与库类型解耦的独立信号（design §1.3 / 修订）。
/// 用于偏置解析与 provider 默认，**不参与类型判定**。
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum ContentHint {
    Anime,
    Documentary,
    #[default]
    None,
}

/// 识别置信度，决定上层是否退化为现状 `title_from_path` 行为（design §5 阶段 F）。
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Confidence {
    /// 仅剥离出标题、无任何定位证据：上层应退化。
    Low,
    /// 有部分定位证据（年份或季集之一）。
    Medium,
    /// 类型 + 标题 + 定位证据齐备。
    High,
}

/// 技术标签（design §4.1 的 `QualityTags`）——不参与 provider 查询，供 UI/媒体文件展示。
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct QualityTags {
    /// 480p/720p/1080p/2160p。
    pub resolution: Option<String>,
    /// BluRay/WEB-DL/HDTV/Remux。
    pub source: Option<String>,
    /// x264/x265/HEVC/AVC。
    pub video_codec: Option<String>,
    /// DTS/AC3/FLAC/AAC。
    pub audio_codec: Option<String>,
    /// HDR/DV/HDR10+。
    pub hdr: Option<String>,
}

/// 显式外部 provider id（Emby/Jellyfin/Kodi 命名约定 `{tmdb-XXX}` / `{imdb-ttXXX}` /
/// `{tvdb-XXX}`，从文件名或目录链解析）。有显式 id 时 provider 直接按 id 拉详情，
/// 跳过模糊标题搜索——零歧义、最准、最快，正是媒体中心刮削的首选路径。
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ExternalIds {
    pub tmdb: Option<String>,
    pub imdb: Option<String>,
    pub tvdb: Option<String>,
}

impl ExternalIds {
    /// 是否含任一显式 id。
    pub fn is_empty(&self) -> bool {
        self.tmdb.is_none() && self.imdb.is_none() && self.tvdb.is_none()
    }
}

/// 识别结果（design §4.1 的 `RecognizedMedia`）。所有字段尽力而为；无法确定即 None。
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct RecognizedMedia {
    pub kind: RecognizedKind,
    /// 清洗后的主标题（查询用）。
    pub title: String,
    /// 双标题时的另一语言标题。
    pub original_title: Option<String>,
    pub year: Option<i32>,
    /// specials = 0。
    pub season: Option<i32>,
    /// 多集合并时 >1；单集 len==1；电影/无集为空。
    pub episodes: Vec<i32>,
    /// Director's Cut / Extended / Remastered。
    pub edition: Option<String>,
    /// 制作组 / 字幕组。
    pub release_group: Option<String>,
    pub quality: QualityTags,
    /// CD1/part1，用于同一 item 的多 media_file。
    pub part: Option<i32>,
    pub content_hint: ContentHint,
    /// 显式外部 provider id（`{tmdb-XXX}` 等），命中即按 id 直接刮削。
    pub external_ids: ExternalIds,
    pub confidence: Confidence,
    /// 命中的自定义识别词 id，便于 admin 调试。
    pub matched_rules: Vec<String>,
}

// `RecognizedMedia` 需要 Default（穷举单测里大量构造期望值），但 RecognizedKind /
// Confidence 没有自然默认值。给枚举各 impl 语义默认（Unknown / Low）。
impl Default for RecognizedKind {
    fn default() -> Self {
        RecognizedKind::Unknown
    }
}

impl Default for Confidence {
    fn default() -> Self {
        Confidence::Low
    }
}
