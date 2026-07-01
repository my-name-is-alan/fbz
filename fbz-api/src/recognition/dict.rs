//! 内置技术标签词典（design §6.2）。
//!
//! 这些是「通用命名约定」知识——分辨率、来源、编码、HDR、版本词——作为进程内
//! 常量编译一次（`LazyLock`），与用户的 `recognition_words` 规则分离：内置词典管
//! 通用约定，用户规则管本地化异常修正。
//!
//! 全部为净室重写：依据公开的 PT/Scene 命名约定独立整理，不参照任何 GPL 源码。

use std::sync::LazyLock;

use regex::Regex;

/// 一个技术标签的匹配规则：正则 + 归一化后的标签值。
pub struct TagPattern {
    pub regex: Regex,
    pub value: &'static str,
}

fn compile(pattern: &str, value: &'static str) -> TagPattern {
    // 内置词典是固定常量，编译失败属编程错误，直接 panic（启动即暴露）。
    let regex = Regex::new(pattern).expect("built-in tag pattern must compile");
    TagPattern { regex, value }
}

/// 分辨率词典：匹配 token，归一到 `480p/720p/1080p/2160p`。
pub static RESOLUTION_PATTERNS: LazyLock<Vec<TagPattern>> = LazyLock::new(|| {
    vec![
        compile(r"(?i)^(2160p|4k|uhd)$", "2160p"),
        compile(r"(?i)^1080[pi]$", "1080p"),
        compile(r"(?i)^720p$", "720p"),
        compile(r"(?i)^576[pi]$", "576p"),
        compile(r"(?i)^480[pi]$", "480p"),
    ]
});

/// 来源词典：BluRay / WEB-DL / WEBRip / HDTV / Remux / DVDRip 等。
pub static SOURCE_PATTERNS: LazyLock<Vec<TagPattern>> = LazyLock::new(|| {
    vec![
        compile(r"(?i)^remux$", "Remux"),
        compile(r"(?i)^(blu-?ray|bdrip|brrip|bd)$", "BluRay"),
        compile(r"(?i)^(web-?dl)$", "WEB-DL"),
        compile(r"(?i)^(web-?rip|webrip)$", "WEBRip"),
        compile(r"(?i)^web$", "WEB"),
        compile(r"(?i)^hdtv$", "HDTV"),
        compile(r"(?i)^(dvdrip|dvd)$", "DVD"),
        compile(r"(?i)^(hdrip)$", "HDRip"),
    ]
});

/// 视频编码词典：x264/x265/H.264/H.265/HEVC/AVC/AV1。
pub static VIDEO_CODEC_PATTERNS: LazyLock<Vec<TagPattern>> = LazyLock::new(|| {
    vec![
        compile(r"(?i)^(x265|h\.?265|hevc)$", "x265"),
        compile(r"(?i)^(x264|h\.?264|avc)$", "x264"),
        compile(r"(?i)^av1$", "AV1"),
    ]
});

/// 音频编码词典：DTS/AC3/EAC3/AAC/FLAC/TrueHD/Atmos 等。
pub static AUDIO_CODEC_PATTERNS: LazyLock<Vec<TagPattern>> = LazyLock::new(|| {
    vec![
        compile(r"(?i)^(dts-?hd|dts)$", "DTS"),
        compile(r"(?i)^(e-?ac-?3|ddp|dd\+)$", "EAC3"),
        compile(r"(?i)^(ac-?3|dd)$", "AC3"),
        compile(r"(?i)^truehd$", "TrueHD"),
        compile(r"(?i)^atmos$", "Atmos"),
        compile(r"(?i)^flac$", "FLAC"),
        compile(r"(?i)^aac$", "AAC"),
    ]
});

/// HDR 词典：HDR/HDR10+/DV(Dolby Vision)。
pub static HDR_PATTERNS: LazyLock<Vec<TagPattern>> = LazyLock::new(|| {
    vec![
        compile(r"(?i)^(hdr10\+|hdr10plus)$", "HDR10+"),
        compile(r"(?i)^hdr10$", "HDR10"),
        compile(r"(?i)^hdr$", "HDR"),
        compile(r"(?i)^(dovi|dv|dolby-?vision)$", "DV"),
    ]
});

/// 版本/剪辑词典（多 token 短语，匹配整段而非单 token）：Director's Cut 等。
/// 这些在 token 化前于整串上匹配，归一为展示用 edition 文本。
pub static EDITION_PATTERNS: LazyLock<Vec<TagPattern>> = LazyLock::new(|| {
    vec![
        compile(r"(?i)\bdirector'?s?\.?\s*cut\b", "Director's Cut"),
        compile(r"(?i)\bextended(\.?\s*(cut|edition))?\b", "Extended"),
        compile(r"(?i)\bunrated\b", "Unrated"),
        compile(r"(?i)\bremaster(ed)?\b", "Remastered"),
        compile(r"(?i)\btheatrical(\.?\s*cut)?\b", "Theatrical"),
        compile(r"(?i)\b(imax)\b", "IMAX"),
    ]
});

/// 其他需剥离但不归类的噪音 token（容器名残留、固定标记）。
pub static NOISE_TOKENS: &[&str] = &[
    "proper", "repack", "internal", "limited", "complete", "multi", "dual", "subbed", "dubbed",
];

/// 判断一个 token 是否纯噪音（应从标题候选剔除）。
pub fn is_noise_token(token: &str) -> bool {
    let lower = token.to_ascii_lowercase();
    NOISE_TOKENS.contains(&lower.as_str())
}

/// 构成 edition 短语的单 token（用于从标题剔除）。EDITION_PATTERNS 匹配多 token 整段，
/// 但 token 化后这些词散落在标题候选里，需逐个剔除。净室整理的通用版本词。
pub const EDITION_TOKENS: &[&str] = &[
    "director",
    "directors",
    "director's",
    "cut",
    "extended",
    "edition",
    "unrated",
    "remaster",
    "remastered",
    "theatrical",
    "imax",
];

/// 判断一个 token 是否属于 edition 短语（应从标题候选剔除）。
pub fn is_edition_token(token: &str) -> bool {
    let lower = token.to_ascii_lowercase();
    EDITION_TOKENS.contains(&lower.as_str())
}

/// 中文数字 → 整数（design §11 待确认项：覆盖 廿/两/零 边界）。
/// 支持 0-99：个位、十位、「十N」「N十」「N十M」「廿/卅」「两」。无法解析返回 None。
pub fn chinese_numeral_to_int(text: &str) -> Option<i32> {
    let trimmed = text.trim();
    if trimmed.is_empty() {
        return None;
    }
    // 纯阿拉伯数字直接解析（混合写法如「第01季」）。
    if let Ok(value) = trimmed.parse::<i32>() {
        return Some(value);
    }

    fn digit(c: char) -> Option<i32> {
        match c {
            '零' | '〇' => Some(0),
            '一' | '壹' => Some(1),
            '二' | '两' | '贰' => Some(2),
            '三' | '叁' => Some(3),
            '四' | '肆' => Some(4),
            '五' | '伍' => Some(5),
            '六' | '陆' => Some(6),
            '七' | '柒' => Some(7),
            '八' | '捌' => Some(8),
            '九' | '玖' => Some(9),
            _ => None,
        }
    }

    let chars: Vec<char> = trimmed.chars().collect();
    // 特例：廿=20、卅=30（可带个位，如 廿一=21）。
    if let Some(&first) = chars.first()
        && matches!(first, '廿' | '卅')
    {
        let base = if first == '廿' { 20 } else { 30 };
        let rest = &chars[1..];
        if rest.is_empty() {
            return Some(base);
        }
        return digit(rest[0]).map(|d| base + d);
    }

    let ten_pos = chars.iter().position(|&c| c == '十' || c == '拾');
    match ten_pos {
        None => {
            // 无「十」：单个数字。
            if chars.len() == 1 {
                digit(chars[0])
            } else {
                None
            }
        }
        Some(pos) => {
            // 有「十」：[tens]十[units]。
            let tens = if pos == 0 { 1 } else { digit(chars[pos - 1])? };
            let units = if pos + 1 < chars.len() {
                digit(chars[pos + 1])?
            } else {
                0
            };
            Some(tens * 10 + units)
        }
    }
}

/// 罗马数字 I-XX → 整数（动漫季常用，如 `Title II`）。仅大写，无法解析返回 None。
pub fn roman_numeral_to_int(text: &str) -> Option<i32> {
    let trimmed = text.trim();
    if trimmed.is_empty() || !trimmed.chars().all(|c| matches!(c, 'I' | 'V' | 'X')) {
        return None;
    }
    let value = |c: char| match c {
        'I' => 1,
        'V' => 5,
        'X' => 10,
        _ => 0,
    };
    let chars: Vec<char> = trimmed.chars().collect();
    let mut total = 0;
    for i in 0..chars.len() {
        let cur = value(chars[i]);
        let next = chars.get(i + 1).map(|&c| value(c)).unwrap_or(0);
        if cur < next {
            total -= cur;
        } else {
            total += cur;
        }
    }
    (1..=30).contains(&total).then_some(total)
}
