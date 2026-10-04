//! 媒体识别核心（design §3-§5）：`scan` 与 `metadata` 之间的纯函数识别层。
//!
//! `recognize()` 把一条物理路径解析为结构化 [`RecognizedMedia`]：类型、标题、年份、
//! 季集、版本、制作组、技术标签。完全确定性、无 IO，便于穷举单测（design §9 质量主轴）。
//!
//! 管线分阶段（design §5），顺序固定，后阶段依赖前阶段产出：
//! - 阶段 A 自定义识别词预处理（阶段 2 接入，本阶段空跑）
//! - 阶段 B token 化与技术标签剥离
//! - 阶段 C 类型判定（库类型先验 + 定位证据）
//! - 阶段 D 字段提取（年份、季集）
//! - 阶段 E 目录/文件证据合并
//! - 阶段 F 置信度与退化
//!
//! 净室重写：依据公开命名约定独立实现，不参照 MoviePilot（GPL）源码（design §0.2）。

pub mod dict;
pub mod repository;
pub mod routes;
pub mod rules;
pub mod types;

use std::sync::LazyLock;

use regex::Regex;

use crate::media_types::LibraryType;

pub use types::{
    Confidence, ContentHint, QualityTags, RecognitionInput, RecognizedKind, RecognizedMedia,
};

/// 标准 SxxExx（含多集区间 S01E01-E03 / S01E01E02）。
static RE_SXXEXX: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?i)\bs(\d{1,2})\s*e(\d{1,3})(?:\s*-?\s*e?(\d{1,3}))?\b")
        .expect("SxxExx pattern must compile")
});

/// 1x05 风格（season x episode）。
static RE_XSTYLE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?i)\b(\d{1,2})x(\d{1,3})\b").expect("NxNN pattern must compile")
});

/// 4 位年份 1900-2099。
static RE_YEAR: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"\b(19\d{2}|20\d{2})\b").expect("year pattern must compile"));

/// 中文季：第X季（X 为中文数字或阿拉伯数字）。
static RE_CN_SEASON: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"第([0-9零〇一二两三四五六七八九十廿卅壹贰叁肆伍陆柒捌玖拾]+)季")
        .expect("cn season pattern must compile")
});

/// 中文集：第X集/话/期。
static RE_CN_EPISODE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"第([0-9零〇一二两三四五六七八九十廿卅壹贰叁肆伍陆柒捌玖拾]+)[集话話期]")
        .expect("cn episode pattern must compile")
});

/// 目录季：Season 02 / S01 / 第一季。
static RE_DIR_SEASON: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?i)^(?:season\s*|s)(\d{1,2})$").expect("dir season pattern must compile")
});

/// specials 目录/标记：S00 / Season 00 / Specials / SP。
static RE_SPECIALS: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?i)\b(s00|season\s*0|specials?|sp\d*)\b").expect("specials pattern must compile")
});

/// 独立集号 token：纯 1-3 位数字（可带 v2 版本号），用于动漫方括号命名 `[01]` /
/// `Title - 05v2` 在剥离技术标签后的标题 token 里定位集号。4 位以上不匹配（避开年份）。
static RE_EP_TOKEN: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"^(\d{1,3})(?:v\d+)?$").expect("episode token pattern must compile")
});

/// 裸 E 集号（无 S 前缀，如 `friends.e08`），季由目录补。仅剧集/混合库使用。
static RE_BARE_EP: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(?i)\be(\d{1,3})\b").expect("bare episode pattern must compile"));

/// 分卷：CD1 / part1 / pt2 / disc1。
static RE_PART: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?i)\b(?:cd|part|pt|disc)\s*(\d{1,2})\b").expect("part pattern must compile")
});

/// Scene 命名末尾的发行组：`-GROUP`，含 `-GROUP@SITE` 站点标记（如 `-DSNP@HiveWeb`）。
static RE_TRAILING_GROUP: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"-([A-Za-z0-9]{2,}(?:@[A-Za-z0-9]+)?)$")
        .expect("trailing group pattern must compile")
});

/// 音频声道布局（`2.0`/`5.1`/`7.1` 等），常紧跟编码名（`AAC 2.0`）。需在 token 化前剥离，
/// 否则 `2.0` 被分隔符拆成残留标题 token。
static RE_AUDIO_CHANNELS: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"\b\d\.\d\b").expect("audio channels pattern must compile"));

/// 显式外部 provider id（Emby/Jellyfin/Kodi 命名约定）：`{tmdb-123}` / `{tmdbid-123}` /
/// `{imdb-tt123}` / `{tvdb-123}`，大小写不敏感，连字符可为 `-` 或 `=`。从文件名/目录链解析。
static RE_EXTERNAL_ID: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?i)\{(tmdb|tmdbid|imdb|tvdb|tvdbid)[-=]([a-z0-9]+)\}")
        .expect("external id pattern must compile")
});

/// 把路径主名按分隔符切成 token（`.` 空格 `_`）。方括号/圆括号先转成空格。
fn tokenize(input: &str) -> Vec<String> {
    input
        .replace(['[', ']', '(', ')'], " ")
        .split(['.', ' ', '_'])
        .map(str::trim)
        .filter(|t| !t.is_empty())
        .map(str::to_owned)
        .collect()
}

/// 在一组词典模式里查 token，命中返回归一值。
fn match_tag(token: &str, patterns: &[dict::TagPattern]) -> Option<&'static str> {
    patterns
        .iter()
        .find(|p| p.regex.is_match(token))
        .map(|p| p.value)
}

/// 阶段 B 产出：剥离技术标签后，剩余「标题候选 + 定位 token」+ 已提取的 QualityTags + 制作组。
struct StrippedTokens {
    title_tokens: Vec<String>,
    quality: QualityTags,
    release_group: Option<String>,
}

/// 阶段 B：token 化 + 剥离归类技术标签。release group 走 Scene 末尾 `-GROUP` 启发式。
fn strip_technical_tokens(stem: &str) -> StrippedTokens {
    // 先抓 Scene 末尾制作组（在 token 化前，避免被分隔符拆散）。
    let release_group = RE_TRAILING_GROUP
        .captures(stem)
        .and_then(|c| c.get(1))
        .map(|m| m.as_str().to_owned())
        // 末尾 group 不能是纯数字或技术标签（如 `-1080p`）。
        .filter(|g| {
            !g.chars().all(|c| c.is_ascii_digit())
                && match_tag(g, &dict::RESOLUTION_PATTERNS).is_none()
                && match_tag(g, &dict::VIDEO_CODEC_PATTERNS).is_none()
        });

    let stem_without_group = if release_group.is_some() {
        RE_TRAILING_GROUP.replace(stem, "").into_owned()
    } else {
        stem.to_owned()
    };
    // 声道数（`2.0`/`5.1`）在 token 化前整串剥离，避免被分隔符拆成残留标题 token。
    let stem_without_group = RE_AUDIO_CHANNELS
        .replace_all(&stem_without_group, " ")
        .into_owned();

    let mut quality = QualityTags::default();
    let mut title_tokens = Vec::new();

    for token in tokenize(&stem_without_group) {
        if let Some(v) = match_tag(&token, &dict::RESOLUTION_PATTERNS) {
            quality.resolution.get_or_insert_with(|| v.to_owned());
        } else if let Some(v) = match_tag(&token, &dict::SOURCE_PATTERNS) {
            quality.source.get_or_insert_with(|| v.to_owned());
        } else if let Some(v) = match_tag(&token, &dict::VIDEO_CODEC_PATTERNS) {
            quality.video_codec.get_or_insert_with(|| v.to_owned());
        } else if let Some(v) = match_tag(&token, &dict::AUDIO_CODEC_PATTERNS) {
            quality.audio_codec.get_or_insert_with(|| v.to_owned());
        } else if let Some(v) = match_tag(&token, &dict::HDR_PATTERNS) {
            quality.hdr.get_or_insert_with(|| v.to_owned());
        } else if dict::is_noise_token(&token) {
            // 噪音：丢弃。
        } else {
            title_tokens.push(token);
        }
    }

    StrippedTokens {
        title_tokens,
        quality,
        release_group,
    }
}

/// 季集证据。
struct SeasonEpisode {
    season: Option<i32>,
    episodes: Vec<i32>,
}

/// 阶段 D（季集）：在原始 stem 上匹配 SxxExx / 1x05。返回季 + 集（含多集区间展开）。
fn extract_season_episode(stem: &str) -> Option<SeasonEpisode> {
    if let Some(caps) = RE_SXXEXX.captures(stem) {
        let season = caps.get(1)?.as_str().parse::<i32>().ok();
        let first = caps.get(2)?.as_str().parse::<i32>().ok()?;
        let episodes = match caps.get(3).and_then(|m| m.as_str().parse::<i32>().ok()) {
            // 区间 E01-E03 → 1,2,3（上界 < 下界时只取首集，防御）。
            Some(last) if last >= first && (last - first) <= 100 => (first..=last).collect(),
            _ => vec![first],
        };
        return Some(SeasonEpisode { season, episodes });
    }
    if let Some(caps) = RE_XSTYLE.captures(stem) {
        let season = caps.get(1)?.as_str().parse::<i32>().ok();
        let episode = caps.get(2)?.as_str().parse::<i32>().ok()?;
        return Some(SeasonEpisode {
            season,
            episodes: vec![episode],
        });
    }
    // 中文季集：第X季 / 第X集（季可缺省，集可缺省）。
    let cn_season = RE_CN_SEASON
        .captures(stem)
        .and_then(|c| c.get(1))
        .and_then(|m| dict::chinese_numeral_to_int(m.as_str()));
    let cn_episode = RE_CN_EPISODE
        .captures(stem)
        .and_then(|c| c.get(1))
        .and_then(|m| dict::chinese_numeral_to_int(m.as_str()));
    if cn_season.is_some() || cn_episode.is_some() {
        return Some(SeasonEpisode {
            season: cn_season,
            episodes: cn_episode.map(|e| vec![e]).unwrap_or_default(),
        });
    }
    None
}

/// 阶段 D（年份）：取「最靠右」的合理年份，并从标题 token 里移除它。
/// 「最靠右」启发式应对标题内含数字（如 *Blade Runner 2049* 2017）。
fn extract_year(title_tokens: &mut Vec<String>) -> Option<i32> {
    let mut year_idx = None;
    for (idx, token) in title_tokens.iter().enumerate() {
        if RE_YEAR.is_match(token) && token.len() == 4 {
            year_idx = Some(idx);
        }
    }
    let idx = year_idx?;
    // 标题只有一个 token 且它就是年份 → 不当年份剥离（避免标题清空）。
    if title_tokens.len() == 1 {
        return None;
    }
    let year = title_tokens[idx].parse::<i32>().ok()?;
    title_tokens.remove(idx);
    Some(year)
}

/// 把标题 token 拼回清洗后的标题串。
fn join_title(tokens: &[String]) -> String {
    tokens.join(" ").trim().to_owned()
}

/// 识别核心入口（design §3）。纯函数：路径链 + 库类型先验 + 已编译识别词规则 → 结构化结果。
pub fn recognize(
    input: &RecognitionInput,
    library_type: LibraryType,
    rules: &rules::RuleSet,
) -> RecognizedMedia {
    // 阶段 A 自定义识别词预处理：block 删除 + replace 替换，把异常命名「纠正」成解析器认识的形态。
    let (preprocessed, mut matched_rules) = rules.apply_preprocess(input.file_stem);
    let stem = preprocessed.as_str();

    // 阶段 B：token 化 + 剥离技术标签。
    let stripped = strip_technical_tokens(stem);
    let mut title_tokens = stripped.title_tokens;

    // 阶段 D（季集）：文件名优先找 SxxExx / 1x05 / 中文季集。
    let mut season_episode = extract_season_episode(stem);

    // 阶段 D（分卷）：CD1/part1，不计入集号。
    let part = RE_PART
        .captures(stem)
        .and_then(|c| c.get(1))
        .and_then(|m| m.as_str().parse::<i32>().ok());

    // 阶段 D（年份）：从标题 token 里取最靠右年份。
    let year = extract_year(&mut title_tokens);

    // 季集/分卷定位 token 混在标题里，剔除。
    title_tokens.retain(|t| {
        !RE_SXXEXX.is_match(t)
            && !RE_XSTYLE.is_match(t)
            && !RE_PART.is_match(t)
            && !dict::is_edition_token(t)
    });

    // 阶段 E（目录证据合并）：季号目录证据优先于文件缺省；标题缺失时取最近含标题的祖先。
    let dir_season = directory_season(input.ancestors);
    let specials =
        RE_SPECIALS.is_match(stem) || input.ancestors.iter().any(|a| RE_SPECIALS.is_match(a));

    // 先清掉孤立标点 token（`-` 等），避免 `- 05` 的标题残留为 "-"，也让集号定位更干净。
    title_tokens.retain(|t| t.chars().any(|c| c.is_alphanumeric()));

    // 裸 E 集号（`friends.e08`，无 S 前缀，季由目录补）：仅剧集/混合库，且文件名无 SxxExx。
    if season_episode.is_none()
        && matches!(library_type, LibraryType::TvShows | LibraryType::Mixed)
        && let Some(caps) = RE_BARE_EP.captures(stem)
        && let Some(ep) = caps.get(1).and_then(|m| m.as_str().parse::<i32>().ok())
    {
        season_episode = Some(SeasonEpisode {
            season: None,
            episodes: vec![ep],
        });
        // 剔除标题里的 e08 token（大小写不敏感）。
        title_tokens.retain(|t| !RE_BARE_EP.is_match(t));
    }

    // 动漫/裸集号：文件名无 SxxExx 但库为剧集/混合时，在标题 token 里找独立集号 token
    // （`[01]` 方括号已 tokenize 成 `01`、`05v2`）。从右往左取首个（动漫集号通常在标题后）。
    // 允许提取的条件：其左侧有非数字标题 token，或有目录季号证据（如 `Show/Season 1/- 05`）。
    if season_episode.is_none() && matches!(library_type, LibraryType::TvShows | LibraryType::Mixed)
    {
        let ep_idx = title_tokens.iter().rposition(|t| RE_EP_TOKEN.is_match(t));
        if let Some(idx) = ep_idx {
            let has_title_before = title_tokens[..idx].iter().any(|t| !RE_EP_TOKEN.is_match(t));
            if (has_title_before || dir_season.is_some())
                && let Some(caps) = RE_EP_TOKEN.captures(&title_tokens[idx])
                && let Some(ep) = caps.get(1).and_then(|m| m.as_str().parse::<i32>().ok())
            {
                season_episode = Some(SeasonEpisode {
                    season: None,
                    episodes: vec![ep],
                });
                title_tokens.remove(idx);
            }
        }
    }

    // 合并季号：specials → 0；否则文件名季号优先，缺省用目录季号。
    if let Some(se) = season_episode.as_mut() {
        if specials {
            se.season = Some(0);
        } else if se.season.is_none() {
            se.season = dir_season;
        }
    }

    // 阶段 D（集偏移）：对已提取集号施加 offset 规则（绝对集号 → 季内集号修正）。
    // 定位窗口用原始文件名匹配；越界丢弃保留原集号。命中规则记入 matched_rules。
    if let Some(se) = season_episode.as_mut() {
        for ep in se.episodes.iter_mut() {
            let (new_ep, mut off_matched) = rules.apply_offset(input.file_stem, *ep);
            *ep = new_ep;
            matched_rules.append(&mut off_matched);
        }
    }

    // 标题：文件名 token 拼出；若为空（如 `- 05.mkv`），取最近含标题的祖先目录。
    let mut title = join_title(&title_tokens);
    if title.is_empty() {
        title = nearest_title_from_ancestors(input.ancestors);
    }

    // content_hint：题材提示（动漫/纪录片），由命名/目录关键词推断，不参与类型判定。
    let content_hint = infer_content_hint(stem, input.ancestors);

    // 显式外部 id（`{tmdb-XXX}` 等）：从文件名 + 目录链解析。命中即 provider 直接按 id 刮削。
    let external_ids = extract_external_ids(input.file_stem, input.ancestors);

    // 阶段 C：类型判定（库类型先验 + 定位证据）。
    let has_episode = season_episode
        .as_ref()
        .is_some_and(|se| !se.episodes.is_empty() || se.season == Some(0));
    let kind = classify_kind(library_type, has_episode);

    // 阶段 F：置信度。显式外部 id = 零歧义，直接 High。
    let has_locator = year.is_some() || season_episode.is_some();
    let confidence = if title.is_empty() && external_ids.is_empty() {
        Confidence::Low
    } else if matches!(kind, RecognizedKind::Unknown) && external_ids.is_empty() {
        Confidence::Low
    } else if !external_ids.is_empty() || has_locator {
        Confidence::High
    } else {
        Confidence::Medium
    };

    // 命中规则去重（同一规则可能在 preprocess 和 offset 各记一次）。
    matched_rules.dedup();

    RecognizedMedia {
        kind,
        title,
        original_title: None,
        year,
        season: season_episode.as_ref().and_then(|se| se.season),
        episodes: season_episode.map(|se| se.episodes).unwrap_or_default(),
        edition: extract_edition(stem),
        release_group: stripped.release_group,
        quality: stripped.quality,
        part,
        content_hint,
        external_ids,
        confidence,
        matched_rules,
    }
}

/// 阶段 E：从祖先目录链找季号（Season 02 / S01 / 第一季）。近→远，取第一个命中。
fn directory_season(ancestors: &[&str]) -> Option<i32> {
    for dir in ancestors {
        if let Some(caps) = RE_DIR_SEASON.captures(dir)
            && let Some(s) = caps.get(1).and_then(|m| m.as_str().parse::<i32>().ok())
        {
            return Some(s);
        }
        if let Some(caps) = RE_CN_SEASON.captures(dir)
            && let Some(s) = caps
                .get(1)
                .and_then(|m| dict::chinese_numeral_to_int(m.as_str()))
        {
            return Some(s);
        }
    }
    None
}

/// 阶段 E：取最近的「像标题」的祖先目录名（跳过纯季号目录如 `Season 02`）。
fn nearest_title_from_ancestors(ancestors: &[&str]) -> String {
    for dir in ancestors {
        if RE_DIR_SEASON.is_match(dir) || RE_CN_SEASON.is_match(dir) || RE_SPECIALS.is_match(dir) {
            continue;
        }
        let cleaned = strip_technical_tokens(dir);
        let title = join_title(&cleaned.title_tokens);
        if !title.is_empty() {
            return title;
        }
    }
    String::new()
}

/// 推断题材提示（design §1.3）：动漫/纪录片关键词。与库类型解耦，不定类型。
fn infer_content_hint(stem: &str, ancestors: &[&str]) -> ContentHint {
    let haystack = {
        let mut s = stem.to_ascii_lowercase();
        for a in ancestors {
            s.push(' ');
            s.push_str(&a.to_ascii_lowercase());
        }
        s
    };
    if haystack.contains("documentary") || haystack.contains("纪录") {
        ContentHint::Documentary
    } else if haystack.contains("anime") || haystack.contains("动漫") || haystack.contains("番")
    {
        ContentHint::Anime
    } else {
        ContentHint::None
    }
}

/// 解析显式外部 provider id（`{tmdb-XXX}`/`{imdb-ttXXX}`/`{tvdb-XXX}`，Emby/Jellyfin/Kodi 约定）。
/// 扫描文件名 + 全部祖先目录（id 通常在目录名）。同类型多处出现取第一个命中。
fn extract_external_ids(file_stem: &str, ancestors: &[&str]) -> types::ExternalIds {
    let mut ids = types::ExternalIds::default();
    // 文件名优先，再目录链（近→远）；先命中的不被覆盖。
    let sources = std::iter::once(file_stem).chain(ancestors.iter().copied());
    for source in sources {
        for caps in RE_EXTERNAL_ID.captures_iter(source) {
            let provider = caps.get(1).map(|m| m.as_str().to_ascii_lowercase());
            let value = caps.get(2).map(|m| m.as_str().to_owned());
            let (Some(provider), Some(value)) = (provider, value) else {
                continue;
            };
            match provider.as_str() {
                "tmdb" | "tmdbid" => {
                    if ids.tmdb.is_none() {
                        ids.tmdb = Some(value);
                    }
                }
                "imdb" => {
                    if ids.imdb.is_none() {
                        ids.imdb = Some(value);
                    }
                }
                "tvdb" | "tvdbid" => {
                    if ids.tvdb.is_none() {
                        ids.tvdb = Some(value);
                    }
                }
                _ => {}
            }
        }
    }
    ids
}

/// 阶段 C：依库类型先验 + 季集证据决定种类（design §1.5 冲突表）。
fn classify_kind(library_type: LibraryType, has_episode: bool) -> RecognizedKind {
    match library_type {
        LibraryType::Music => RecognizedKind::Track,
        LibraryType::Movies => RecognizedKind::Movie,
        LibraryType::TvShows => {
            if has_episode {
                RecognizedKind::Episode
            } else {
                // 剧集库但无集号证据：可能是 series 容器或裸命名，交后续阶段/provider。
                RecognizedKind::Series
            }
        }
        // 混合库：有集号则剧集，否则电影。
        LibraryType::Mixed => {
            if has_episode {
                RecognizedKind::Episode
            } else {
                RecognizedKind::Movie
            }
        }
        LibraryType::HomeVideos => RecognizedKind::Video,
        LibraryType::LiveTv => RecognizedKind::Unknown,
    }
}

/// 阶段 D（edition）：在整串上匹配版本词典。
fn extract_edition(stem: &str) -> Option<String> {
    dict::EDITION_PATTERNS
        .iter()
        .find(|p| p.regex.is_match(stem))
        .map(|p| p.value.to_owned())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn input<'a>(stem: &'a str, ext: &'a str) -> RecognitionInput<'a> {
        RecognitionInput {
            file_stem: stem,
            extension: Some(ext),
            ancestors: &[],
        }
    }

    /// 测试 helper：用空 RuleSet 跑识别（阶段 0/1 不依赖自定义识别词）。
    fn rec(input: &RecognitionInput, library_type: LibraryType) -> RecognizedMedia {
        let (empty, _) = rules::RuleSet::compile(Vec::new());
        recognize(input, library_type, &empty)
    }

    // ---- §1.1 电影 ----

    #[test]
    fn movie_standard_scene_naming() {
        let r = rec(
            &input("Inception.2010.1080p.BluRay.x264-GROUP", "mkv"),
            LibraryType::Movies,
        );
        assert_eq!(r.kind, RecognizedKind::Movie);
        assert_eq!(r.title, "Inception");
        assert_eq!(r.year, Some(2010));
        assert_eq!(r.quality.resolution.as_deref(), Some("1080p"));
        assert_eq!(r.quality.source.as_deref(), Some("BluRay"));
        assert_eq!(r.quality.video_codec.as_deref(), Some("x264"));
        assert_eq!(r.release_group.as_deref(), Some("GROUP"));
        assert_eq!(r.confidence, Confidence::High);
    }

    #[test]
    fn movie_title_with_year_in_name_picks_rightmost_year() {
        // *Blade Runner 2049* (2017)：标题内含 2049，年份应取最靠右的 2017。
        let r = rec(
            &input("Blade.Runner.2049.2017.2160p.UHD.BluRay.x265.HDR", "mkv"),
            LibraryType::Movies,
        );
        assert_eq!(r.title, "Blade Runner 2049");
        assert_eq!(r.year, Some(2017));
        assert_eq!(r.quality.resolution.as_deref(), Some("2160p"));
        assert_eq!(r.quality.video_codec.as_deref(), Some("x265"));
        assert_eq!(r.quality.hdr.as_deref(), Some("HDR"));
    }

    #[test]
    fn movie_space_naming_non_scene() {
        let r = rec(&input("The Matrix (1999)", "mp4"), LibraryType::Movies);
        assert_eq!(r.title, "The Matrix");
        assert_eq!(r.year, Some(1999));
    }

    #[test]
    fn movie_edition_marker() {
        let r = rec(
            &input("Aliens.1986.Directors.Cut.1080p", "mkv"),
            LibraryType::Movies,
        );
        assert_eq!(r.title, "Aliens");
        assert_eq!(r.year, Some(1986));
        assert_eq!(r.edition.as_deref(), Some("Director's Cut"));
    }

    #[test]
    fn movie_without_year() {
        let r = rec(&input("Interstellar", "mkv"), LibraryType::Movies);
        assert_eq!(r.title, "Interstellar");
        assert_eq!(r.year, None);
        // 无定位证据但类型由库定型 → Medium。
        assert_eq!(r.confidence, Confidence::Medium);
    }

    // ---- §1.2 剧集标准 SxxExx ----

    #[test]
    fn episode_standard_sxxexx() {
        let r = rec(
            &input("Breaking.Bad.S01E05.1080p", "mkv"),
            LibraryType::TvShows,
        );
        assert_eq!(r.kind, RecognizedKind::Episode);
        assert_eq!(r.title, "Breaking Bad");
        assert_eq!(r.season, Some(1));
        assert_eq!(r.episodes, vec![5]);
        assert_eq!(r.quality.resolution.as_deref(), Some("1080p"));
        assert_eq!(r.confidence, Confidence::High);
    }

    #[test]
    fn episode_multi_range() {
        let r = rec(&input("Show.S01E01-E03", "mkv"), LibraryType::TvShows);
        assert_eq!(r.season, Some(1));
        assert_eq!(r.episodes, vec![1, 2, 3]);
    }

    #[test]
    fn episode_1x05_style() {
        let r = rec(&input("Show.1x05", "mkv"), LibraryType::TvShows);
        assert_eq!(r.kind, RecognizedKind::Episode);
        assert_eq!(r.season, Some(1));
        assert_eq!(r.episodes, vec![5]);
        assert_eq!(r.title, "Show");
    }

    // ---- §1.5 退化与库类型先验 ----

    #[test]
    fn library_kind_prior_changes_classification() {
        // 同一裸文件名：movie 库 → Movie，tvshows 库无集号 → Series。
        let movie = rec(&input("Some Title", "mkv"), LibraryType::Movies);
        assert_eq!(movie.kind, RecognizedKind::Movie);
        let series = rec(&input("Some Title", "mkv"), LibraryType::TvShows);
        assert_eq!(series.kind, RecognizedKind::Series);
    }

    #[test]
    fn music_library_yields_track() {
        let r = rec(&input("01 - Track Name", "flac"), LibraryType::Music);
        assert_eq!(r.kind, RecognizedKind::Track);
    }

    // ---- §1.2 目录证据合并 / specials / 中文季集 ----

    fn input_with<'a>(stem: &'a str, ancestors: &'a [&'a str]) -> RecognitionInput<'a> {
        RecognitionInput {
            file_stem: stem,
            extension: Some("mkv"),
            ancestors,
        }
    }

    #[test]
    fn season_in_directory_episode_in_file() {
        // Friends/Season 02/friends.e08.mkv — 季来自目录，集来自文件。
        let r = rec(
            &input_with("friends.S02E08", &["Season 02", "Friends"]),
            LibraryType::TvShows,
        );
        assert_eq!(r.season, Some(2));
        assert_eq!(r.episodes, vec![8]);
    }

    #[test]
    fn bare_episode_with_season_directory() {
        // Show/Season 1/Show - 05.mkv — 文件只有裸集号，季来自目录。
        let r = rec(
            &input_with("Show - 05", &["Season 1", "Show"]),
            LibraryType::TvShows,
        );
        assert_eq!(r.kind, RecognizedKind::Episode);
        assert_eq!(r.season, Some(1));
        assert_eq!(r.episodes, vec![5]);
    }

    #[test]
    fn title_falls_back_to_ancestor_directory() {
        // - 05.mkv：文件无标题，取最近含标题的祖先（跳过季目录）。
        let r = rec(
            &input_with("- 05", &["Season 02", "Breaking Bad"]),
            LibraryType::TvShows,
        );
        assert_eq!(r.title, "Breaking Bad");
        assert_eq!(r.season, Some(2));
        assert_eq!(r.episodes, vec![5]);
    }

    #[test]
    fn chinese_season_episode() {
        let r = rec(&input("庆余年 第二季 第5集", "mkv"), LibraryType::TvShows);
        assert_eq!(r.season, Some(2));
        assert_eq!(r.episodes, vec![5]);
        assert!(r.title.contains("庆余年"));
    }

    #[test]
    fn chinese_episode_only_with_arabic() {
        let r = rec(&input("庆余年.第01集", "mkv"), LibraryType::TvShows);
        assert_eq!(r.episodes, vec![1]);
    }

    #[test]
    fn specials_season_zero() {
        let r = rec(&input("Show.S00E01", "mkv"), LibraryType::TvShows);
        assert_eq!(r.season, Some(0));
        assert_eq!(r.episodes, vec![1]);
    }

    // ---- §1.3 动漫 ----

    #[test]
    fn anime_group_prefix_bracket_tags() {
        // [VCB-Studio] Fate [01][Ma10p_1080p][x265].mkv
        let r = rec(
            &input("[VCB-Studio] Fate [01][Ma10p_1080p][x265]", "mkv"),
            LibraryType::TvShows,
        );
        assert_eq!(r.episodes, vec![1]);
        assert_eq!(r.quality.resolution.as_deref(), Some("1080p"));
        assert_eq!(r.quality.video_codec.as_deref(), Some("x265"));
        assert!(r.title.contains("Fate"));
    }

    #[test]
    fn anime_version_number_not_counted_as_episode() {
        // [Group] Title - 05v2 [1080p] — v2 不计入集号，集号=5。
        let r = rec(
            &input("[Group] Title - 05v2 [1080p]", "mkv"),
            LibraryType::TvShows,
        );
        assert_eq!(r.episodes, vec![5]);
        assert_eq!(r.quality.resolution.as_deref(), Some("1080p"));
    }

    // ---- §1.1 多碟/分卷 ----

    #[test]
    fn movie_part_volume_extracted_not_in_title() {
        let r = rec(&input("Movie.2020.CD1", "avi"), LibraryType::Movies);
        assert_eq!(r.title, "Movie");
        assert_eq!(r.year, Some(2020));
        assert_eq!(r.part, Some(1));
    }

    // ---- §1.3 content_hint（题材，不参与类型判定） ----

    #[test]
    fn content_hint_anime_from_directory() {
        let r = rec(
            &input_with("Title - 03", &["Anime", "Title"]),
            LibraryType::TvShows,
        );
        assert_eq!(r.content_hint, ContentHint::Anime);
        // 题材是 Anime 但类型仍由库 + 集号定为 Episode（解耦验证）。
        assert_eq!(r.kind, RecognizedKind::Episode);
    }

    // ---- 中文/罗马数字解析单元 ----

    #[test]
    fn chinese_numeral_parsing_covers_boundaries() {
        use super::dict::chinese_numeral_to_int;
        assert_eq!(chinese_numeral_to_int("一"), Some(1));
        assert_eq!(chinese_numeral_to_int("十"), Some(10));
        assert_eq!(chinese_numeral_to_int("十二"), Some(12));
        assert_eq!(chinese_numeral_to_int("二十"), Some(20));
        assert_eq!(chinese_numeral_to_int("二十三"), Some(23));
        assert_eq!(chinese_numeral_to_int("廿"), Some(20));
        assert_eq!(chinese_numeral_to_int("廿一"), Some(21));
        assert_eq!(chinese_numeral_to_int("两"), Some(2));
        assert_eq!(chinese_numeral_to_int("零"), Some(0));
        assert_eq!(chinese_numeral_to_int("05"), Some(5));
        assert_eq!(chinese_numeral_to_int("abc"), None);
    }

    #[test]
    fn roman_numeral_parsing() {
        use super::dict::roman_numeral_to_int;
        assert_eq!(roman_numeral_to_int("II"), Some(2));
        assert_eq!(roman_numeral_to_int("IV"), Some(4));
        assert_eq!(roman_numeral_to_int("IX"), Some(9));
        assert_eq!(roman_numeral_to_int("XIV"), Some(14));
        assert_eq!(roman_numeral_to_int("foo"), None);
    }

    // ---- §1.5 退化保证 ----

    #[test]
    fn unrecognizable_degrades_to_low_confidence() {
        // 完全无定位证据 + 库无法定型（livetv）→ Low，上层应退化。
        let r = rec(&input("random_clip_xyz", "mkv"), LibraryType::LiveTv);
        assert_eq!(r.confidence, Confidence::Low);
        assert_eq!(r.kind, RecognizedKind::Unknown);
    }

    // ---- §7 自定义识别词引擎集成（规则真正影响识别结果） ----

    fn word(
        id: &str,
        kind: rules::RuleKind,
        pattern: &str,
        repl: Option<&str>,
        expr: Option<&str>,
    ) -> rules::RecognitionWord {
        rules::RecognitionWord {
            id: id.to_owned(),
            kind,
            pattern: pattern.to_owned(),
            replacement: repl.map(str::to_owned),
            anchor_after: None,
            offset_expr: expr.map(str::to_owned),
            is_regex: false,
            priority: 100,
        }
    }

    #[test]
    fn custom_block_word_removes_noise_before_parsing() {
        // 屏蔽词删掉广告组前缀，标题才干净。
        let (rs, _) = rules::RuleSet::compile(vec![word(
            "blk",
            rules::RuleKind::Block,
            "[广告组]",
            None,
            None,
        )]);
        let r = recognize(
            &input("[广告组]Inception.2010.1080p", "mkv"),
            LibraryType::Movies,
            &rs,
        );
        assert_eq!(r.title, "Inception");
        assert_eq!(r.year, Some(2010));
        assert!(r.matched_rules.contains(&"blk".to_owned()));
    }

    #[test]
    fn custom_replace_word_corrects_alias() {
        // 替换词把别名纠正为正名，再走标准解析。
        let (rs, _) = rules::RuleSet::compile(vec![word(
            "rep",
            rules::RuleKind::Replace,
            "斗破苍穹年番",
            Some("斗破苍穹"),
            None,
        )]);
        let r = recognize(
            &input("斗破苍穹年番 第38集", "mkv"),
            LibraryType::TvShows,
            &rs,
        );
        assert!(r.title.contains("斗破苍穹"));
        assert!(!r.title.contains("年番"));
        assert_eq!(r.episodes, vec![38]);
        assert!(r.matched_rules.contains(&"rep".to_owned()));
    }

    #[test]
    fn custom_offset_word_corrects_absolute_episode() {
        // 集偏移：绝对集号 38 → 季内集号 12（-26 偏移），design §1.2 绝对集号需偏移。
        let (rs, _) = rules::RuleSet::compile(vec![word(
            "off",
            rules::RuleKind::Offset,
            "", // 无窗口约束
            None,
            Some("-26"),
        )]);
        let r = recognize(&input("Show.S02E38", "mkv"), LibraryType::TvShows, &rs);
        assert_eq!(
            r.episodes,
            vec![12],
            "absolute episode must be offset into season"
        );
        assert!(r.matched_rules.contains(&"off".to_owned()));
    }

    #[test]
    fn custom_offset_out_of_bounds_preserves_episode() {
        // 偏移越界（算出 ≤ 0）：保留原集号，不写非法值（design §7.3）。
        let (rs, _) = rules::RuleSet::compile(vec![word(
            "off",
            rules::RuleKind::Offset,
            "",
            None,
            Some("-100"),
        )]);
        let r = recognize(&input("Show.S01E05", "mkv"), LibraryType::TvShows, &rs);
        assert_eq!(
            r.episodes,
            vec![5],
            "out-of-bounds offset must keep original episode"
        );
    }

    #[test]
    fn empty_ruleset_is_noop() {
        // 空规则集：识别结果与无规则路径一致（零回归保证）。
        let (rs, _) = rules::RuleSet::compile(Vec::new());
        let r = recognize(
            &input("Breaking.Bad.S01E05.1080p", "mkv"),
            LibraryType::TvShows,
            &rs,
        );
        assert_eq!(r.title, "Breaking Bad");
        assert_eq!(r.season, Some(1));
        assert_eq!(r.episodes, vec![5]);
        assert!(r.matched_rules.is_empty());
    }

    // ---- 显式外部 id 解析（Emby/Jellyfin/Kodi 命名约定） ----

    #[test]
    fn explicit_tmdb_id_from_directory_is_parsed_and_boosts_confidence() {
        // 真实目录：Tv/21世纪大君夫人 (2026) {tmdb-278573}/Season 1/Perfect Crown.S01E01...
        let r = rec(
            &input_with(
                "Perfect Crown.2026.S01E01.1080p.WEB-DL",
                &["Season 1", "21世纪大君夫人 (2026) {tmdb-278573}"],
            ),
            LibraryType::TvShows,
        );
        assert_eq!(r.external_ids.tmdb.as_deref(), Some("278573"));
        assert_eq!(r.confidence, Confidence::High);
    }

    #[test]
    fn explicit_ids_parse_all_provider_forms() {
        // tmdbid 别名 + imdb tt 前缀 + tvdb，各种约定形态。
        let r = rec(
            &input_with("Movie", &["Inception (2010) {tmdb-27205}"]),
            LibraryType::Movies,
        );
        assert_eq!(r.external_ids.tmdb.as_deref(), Some("27205"));

        let r2 = rec(
            &input_with("Show", &["The Wire {imdb-tt0306414} {tvdb-79126}"]),
            LibraryType::TvShows,
        );
        assert_eq!(r2.external_ids.imdb.as_deref(), Some("tt0306414"));
        assert_eq!(r2.external_ids.tvdb.as_deref(), Some("79126"));

        // 文件名里的 {tmdbid=NNN} 等号形态也认。
        let r3 = rec(&input("Film {tmdbid=550}", "mkv"), LibraryType::Movies);
        assert_eq!(r3.external_ids.tmdb.as_deref(), Some("550"));

        // 无显式 id：空。
        let r4 = rec(&input("Plain.Movie.2020", "mkv"), LibraryType::Movies);
        assert!(r4.external_ids.is_empty());
    }
}
