//! DeviceProfile → PlayMethod 决策表骨架。
//!
//! Emby 语义近似：
//! - **DirectPlay**：容器 + 音视频编码均被客户端 DirectPlayProfiles 接受，可原样直放。
//! - **Remux**（协议侧常报 `DirectStream`）：视频可直放，但容器需换封装（或仅音频需转码时的简化占位）。
//! - **Transcode**：视频编码不被接受，或码率/分辨率超出限制，需转码（HLS 等）。
//!
//! 本模块只做可单测的纯函数骨架；完整字幕烧录、音轨挑选、bitrate ladder 后续再接。

use serde_json::Value;

/// 对外/上报用的 PlayMethod 名称（与 Emby 客户端字段对齐）。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PlayMethod {
    DirectPlay,
    /// 对应客户端常见的 DirectStream（remux / 换封装）。
    Remux,
    Transcode,
}

impl PlayMethod {
    pub const fn as_emby_str(self) -> &'static str {
        match self {
            PlayMethod::DirectPlay => "DirectPlay",
            PlayMethod::Remux => "DirectStream",
            PlayMethod::Transcode => "Transcode",
        }
    }
}

/// 媒体源能力摘要（来自 probe / MediaSource，非完整 DeviceProfile）。
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MediaCapabilities {
    pub container: Option<String>,
    pub video_codec: Option<String>,
    pub audio_codec: Option<String>,
    pub bitrate: Option<i64>,
    pub width: Option<i32>,
    pub height: Option<i32>,
}

/// 从 PlaybackInfo 的 DeviceProfile JSON 抽出的决策用摘要。
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct DeviceProfileSummary {
    /// 小写容器列表，空 = 未声明（决策时视为「未知，倾向直放」仅当编解码也未声明限制）。
    pub direct_play_containers: Vec<String>,
    pub direct_play_video_codecs: Vec<String>,
    pub direct_play_audio_codecs: Vec<String>,
    pub max_streaming_bitrate: Option<i64>,
    pub max_width: Option<i32>,
    pub max_height: Option<i32>,
    /// 是否声明了任一 DirectPlay 视频相关约束（用于区分「无 profile」与「显式空」）。
    pub has_direct_play_hints: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PlayMethodDecision {
    pub method: PlayMethod,
    pub reason: &'static str,
}

/// 决策表：无 DeviceProfile 提示时默认 DirectPlay（与当前 PlaybackInfo 填 true 行为一致）。
pub fn decide_play_method(
    media: &MediaCapabilities,
    profile: &DeviceProfileSummary,
) -> PlayMethodDecision {
    if !profile.has_direct_play_hints {
        return PlayMethodDecision {
            method: PlayMethod::DirectPlay,
            reason: "no_device_profile_hints_default_direct_play",
        };
    }

    let container_ok = match_token(
        media.container.as_deref(),
        &profile.direct_play_containers,
        true,
    );
    let video_ok = match_token(
        media.video_codec.as_deref(),
        &profile.direct_play_video_codecs,
        true,
    );
    let audio_ok = match_token(
        media.audio_codec.as_deref(),
        &profile.direct_play_audio_codecs,
        true,
    );

    if exceeds_bitrate(media.bitrate, profile.max_streaming_bitrate)
        || exceeds_dim(media.width, profile.max_width)
        || exceeds_dim(media.height, profile.max_height)
    {
        return PlayMethodDecision {
            method: PlayMethod::Transcode,
            reason: "exceeds_bitrate_or_resolution_limit",
        };
    }

    if video_ok && audio_ok && container_ok {
        return PlayMethodDecision {
            method: PlayMethod::DirectPlay,
            reason: "container_and_codecs_direct_play",
        };
    }

    if video_ok && audio_ok && !container_ok {
        return PlayMethodDecision {
            method: PlayMethod::Remux,
            reason: "codecs_ok_container_needs_remux",
        };
    }

    if !video_ok {
        return PlayMethodDecision {
            method: PlayMethod::Transcode,
            reason: "video_codec_not_in_direct_play_profile",
        };
    }

    // 视频可直放、音频不行：骨架先标 Remux（真实环境可能要 audio transcode / remux）。
    PlayMethodDecision {
        method: PlayMethod::Remux,
        reason: "video_ok_audio_needs_reencode_or_remux",
    }
}

fn match_token(value: Option<&str>, allowed: &[String], empty_means_any: bool) -> bool {
    if allowed.is_empty() {
        return empty_means_any;
    }
    let Some(value) = value.map(str::trim).filter(|v| !v.is_empty()) else {
        return false;
    };
    let normalized = normalize_codec_or_container(value);
    allowed.iter().any(|item| {
        normalize_codec_or_container(item) == normalized
            || item
                .split(',')
                .map(str::trim)
                .any(|part| normalize_codec_or_container(part) == normalized)
    })
}

fn normalize_codec_or_container(value: &str) -> String {
    let v = value.trim().trim_start_matches('.').to_ascii_lowercase();
    match v.as_str() {
        "matroska" => "mkv".to_owned(),
        "avc" | "avc1" => "h264".to_owned(),
        "h265" | "hvc1" | "x265" => "hevc".to_owned(),
        "mpegts" => "ts".to_owned(),
        other => other.to_owned(),
    }
}

fn exceeds_bitrate(actual: Option<i64>, max: Option<i64>) -> bool {
    match (actual, max) {
        (Some(a), Some(m)) if m > 0 => a > m,
        _ => false,
    }
}

fn exceeds_dim(actual: Option<i32>, max: Option<i32>) -> bool {
    match (actual, max) {
        (Some(a), Some(m)) if m > 0 => a > m,
        _ => false,
    }
}

/// 从 PlaybackInfo 请求体中的 DeviceProfile JSON 抽取摘要（容错 PascalCase / camelCase）。
/// Query 里偶发把 DeviceProfile 编成 JSON 字符串，此处一并解开。
pub fn device_profile_summary_from_value(value: Option<&Value>) -> DeviceProfileSummary {
    let Some(raw) = value else {
        return DeviceProfileSummary::default();
    };

    let owned;
    let root = match coerce_json_object(raw) {
        Some(CoercedProfile::Borrowed(v)) => v,
        Some(CoercedProfile::Owned(v)) => {
            owned = v;
            &owned
        }
        None => return DeviceProfileSummary::default(),
    };

    let mut summary = DeviceProfileSummary::default();
    summary.max_streaming_bitrate = int_field(root, &["MaxStreamingBitrate", "maxStreamingBitrate"]);
    summary.max_width = int_field(root, &["MaxWidth", "maxWidth"]).map(|v| v as i32);
    summary.max_height = int_field(root, &["MaxHeight", "maxHeight"]).map(|v| v as i32);

    let profiles = root
        .get("DirectPlayProfiles")
        .or_else(|| root.get("directPlayProfiles"))
        .and_then(|v| v.as_array())
        .cloned()
        .unwrap_or_default();

    for profile in profiles {
        let type_name = profile
            .get("Type")
            .or_else(|| profile.get("type"))
            .and_then(|v| v.as_str())
            .unwrap_or("Video");
        if !type_name.eq_ignore_ascii_case("Video") && !type_name.eq_ignore_ascii_case("Audio") {
            continue;
        }
        summary.has_direct_play_hints = true;
        push_csv(
            &mut summary.direct_play_containers,
            string_field(&profile, &["Container", "container"]),
        );
        if type_name.eq_ignore_ascii_case("Video") {
            push_csv(
                &mut summary.direct_play_video_codecs,
                string_field(&profile, &["VideoCodec", "videoCodec"]),
            );
            push_csv(
                &mut summary.direct_play_audio_codecs,
                string_field(&profile, &["AudioCodec", "audioCodec"]),
            );
        } else {
            push_csv(
                &mut summary.direct_play_audio_codecs,
                string_field(&profile, &["AudioCodec", "audioCodec"]),
            );
        }
    }

    summary
}

/// PlaybackInfo：优先顶层 `DeviceProfile`，否则取 `ClientCapabilities.DeviceProfile`。
pub fn device_profile_summary_from_playback_hints(
    device_profile: Option<&Value>,
    client_capabilities: Option<&Value>,
) -> DeviceProfileSummary {
    if device_profile.is_some() {
        return device_profile_summary_from_value(device_profile);
    }

    let owned_caps;
    let caps = match client_capabilities.and_then(coerce_json_object) {
        Some(CoercedProfile::Borrowed(v)) => Some(v),
        Some(CoercedProfile::Owned(v)) => {
            owned_caps = v;
            Some(&owned_caps)
        }
        None => None,
    };

    let nested = caps.and_then(|root| {
        root.get("DeviceProfile")
            .or_else(|| root.get("deviceProfile"))
    });
    device_profile_summary_from_value(nested)
}

/// 把请求级 MaxStreamingBitrate 并入摘要（取更严上限）；无 DirectPlay 提示时仍不改变默认 DirectPlay。
pub fn merge_request_max_streaming_bitrate(
    mut summary: DeviceProfileSummary,
    max_streaming_bitrate: Option<i64>,
) -> DeviceProfileSummary {
    let Some(requested) = max_streaming_bitrate.filter(|bitrate| *bitrate > 0) else {
        return summary;
    };
    summary.max_streaming_bitrate = Some(match summary.max_streaming_bitrate {
        Some(existing) if existing > 0 => existing.min(requested),
        _ => requested,
    });
    summary
}

enum CoercedProfile<'a> {
    Borrowed(&'a Value),
    Owned(Value),
}

fn coerce_json_object(value: &Value) -> Option<CoercedProfile<'_>> {
    if value.is_object() {
        return Some(CoercedProfile::Borrowed(value));
    }
    let raw = value.as_str()?.trim();
    if raw.is_empty() {
        return None;
    }
    let parsed: Value = serde_json::from_str(raw).ok()?;
    parsed.is_object().then_some(CoercedProfile::Owned(parsed))
}

fn string_field<'a>(value: &'a Value, keys: &[&str]) -> Option<&'a str> {
    keys.iter()
        .find_map(|key| value.get(*key).and_then(|v| v.as_str()))
}

fn int_field(value: &Value, keys: &[&str]) -> Option<i64> {
    keys.iter().find_map(|key| {
        value.get(*key).and_then(|v| {
            v.as_i64()
                .or_else(|| v.as_u64().map(|n| n as i64))
                .or_else(|| v.as_f64().map(|n| n as i64))
        })
    })
}

fn push_csv(out: &mut Vec<String>, raw: Option<&str>) {
    let Some(raw) = raw else {
        return;
    };
    for part in raw.split([',', '|']).map(str::trim).filter(|p| !p.is_empty()) {
        let normalized = normalize_codec_or_container(part);
        if !out.iter().any(|e| e == &normalized) {
            out.push(normalized);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn media_h264_aac_mkv() -> MediaCapabilities {
        MediaCapabilities {
            container: Some("mkv".to_owned()),
            video_codec: Some("h264".to_owned()),
            audio_codec: Some("aac".to_owned()),
            bitrate: Some(8_000_000),
            width: Some(1920),
            height: Some(1080),
        }
    }

    #[test]
    fn no_profile_defaults_to_direct_play() {
        let decision = decide_play_method(&media_h264_aac_mkv(), &DeviceProfileSummary::default());
        assert_eq!(decision.method, PlayMethod::DirectPlay);
        assert_eq!(decision.method.as_emby_str(), "DirectPlay");
    }

    #[test]
    fn matching_direct_play_profile_selects_direct_play() {
        let profile = DeviceProfileSummary {
            direct_play_containers: vec!["mkv".to_owned(), "mp4".to_owned()],
            direct_play_video_codecs: vec!["h264".to_owned()],
            direct_play_audio_codecs: vec!["aac".to_owned()],
            has_direct_play_hints: true,
            ..DeviceProfileSummary::default()
        };
        let decision = decide_play_method(&media_h264_aac_mkv(), &profile);
        assert_eq!(decision.method, PlayMethod::DirectPlay);
        assert_eq!(decision.reason, "container_and_codecs_direct_play");
    }

    #[test]
    fn container_mismatch_with_ok_codecs_selects_remux() {
        let profile = DeviceProfileSummary {
            direct_play_containers: vec!["mp4".to_owned()],
            direct_play_video_codecs: vec!["h264".to_owned()],
            direct_play_audio_codecs: vec!["aac".to_owned()],
            has_direct_play_hints: true,
            ..DeviceProfileSummary::default()
        };
        let decision = decide_play_method(&media_h264_aac_mkv(), &profile);
        assert_eq!(decision.method, PlayMethod::Remux);
        assert_eq!(decision.method.as_emby_str(), "DirectStream");
        assert_eq!(decision.reason, "codecs_ok_container_needs_remux");
    }

    #[test]
    fn unsupported_video_codec_selects_transcode() {
        let profile = DeviceProfileSummary {
            direct_play_containers: vec!["mkv".to_owned()],
            direct_play_video_codecs: vec!["h264".to_owned()],
            direct_play_audio_codecs: vec!["aac".to_owned()],
            has_direct_play_hints: true,
            ..DeviceProfileSummary::default()
        };
        let mut media = media_h264_aac_mkv();
        media.video_codec = Some("hevc".to_owned());
        let decision = decide_play_method(&media, &profile);
        assert_eq!(decision.method, PlayMethod::Transcode);
    }

    #[test]
    fn bitrate_over_limit_selects_transcode() {
        let profile = DeviceProfileSummary {
            direct_play_containers: vec!["mkv".to_owned()],
            direct_play_video_codecs: vec!["h264".to_owned()],
            direct_play_audio_codecs: vec!["aac".to_owned()],
            max_streaming_bitrate: Some(4_000_000),
            has_direct_play_hints: true,
            ..DeviceProfileSummary::default()
        };
        let decision = decide_play_method(&media_h264_aac_mkv(), &profile);
        assert_eq!(decision.method, PlayMethod::Transcode);
        assert_eq!(decision.reason, "exceeds_bitrate_or_resolution_limit");
    }

    #[test]
    fn matroska_alias_matches_mkv_direct_play_container() {
        let profile = DeviceProfileSummary {
            direct_play_containers: vec!["mkv".to_owned()],
            direct_play_video_codecs: vec!["h264".to_owned()],
            direct_play_audio_codecs: vec!["aac".to_owned()],
            has_direct_play_hints: true,
            ..DeviceProfileSummary::default()
        };
        let mut media = media_h264_aac_mkv();
        media.container = Some("matroska".to_owned());
        let decision = decide_play_method(&media, &profile);
        assert_eq!(decision.method, PlayMethod::DirectPlay);
    }

    #[test]
    fn parses_device_profile_json_fixture() {
        let value = json!({
            "MaxStreamingBitrate": 20_000_000,
            "DirectPlayProfiles": [{
                "Type": "Video",
                "Container": "mp4,mkv",
                "VideoCodec": "h264,hevc",
                "AudioCodec": "aac,ac3"
            }]
        });
        let summary = device_profile_summary_from_value(Some(&value));
        assert!(summary.has_direct_play_hints);
        assert_eq!(summary.max_streaming_bitrate, Some(20_000_000));
        assert!(summary.direct_play_containers.contains(&"mkv".to_owned()));
        assert!(summary.direct_play_video_codecs.contains(&"hevc".to_owned()));
        assert!(summary.direct_play_audio_codecs.contains(&"ac3".to_owned()));
    }

    #[test]
    fn parses_stringified_device_profile_json() {
        let value = Value::String(
            r#"{"DirectPlayProfiles":[{"Type":"Video","Container":"mp4","VideoCodec":"h264","AudioCodec":"aac"}]}"#
                .to_owned(),
        );
        let summary = device_profile_summary_from_value(Some(&value));
        assert!(summary.has_direct_play_hints);
        assert!(summary.direct_play_containers.contains(&"mp4".to_owned()));
    }

    #[test]
    fn playback_hints_prefer_top_level_device_profile() {
        let device = json!({
            "DirectPlayProfiles": [{
                "Type": "Video",
                "Container": "mkv",
                "VideoCodec": "h264",
                "AudioCodec": "aac"
            }]
        });
        let caps = json!({
            "DeviceProfile": {
                "DirectPlayProfiles": [{
                    "Type": "Video",
                    "Container": "mp4",
                    "VideoCodec": "hevc",
                    "AudioCodec": "aac"
                }]
            }
        });
        let summary = device_profile_summary_from_playback_hints(Some(&device), Some(&caps));
        assert!(summary.direct_play_containers.contains(&"mkv".to_owned()));
        assert!(!summary.direct_play_containers.contains(&"mp4".to_owned()));
    }

    #[test]
    fn playback_hints_fall_back_to_client_capabilities_device_profile() {
        let caps = json!({
            "DeviceProfile": {
                "DirectPlayProfiles": [{
                    "Type": "Video",
                    "Container": "mp4",
                    "VideoCodec": "h264",
                    "AudioCodec": "aac"
                }]
            }
        });
        let summary = device_profile_summary_from_playback_hints(None, Some(&caps));
        assert!(summary.has_direct_play_hints);
        assert!(summary.direct_play_containers.contains(&"mp4".to_owned()));
    }

    #[test]
    fn merge_request_bitrate_takes_stricter_limit() {
        let summary = DeviceProfileSummary {
            max_streaming_bitrate: Some(20_000_000),
            has_direct_play_hints: true,
            ..DeviceProfileSummary::default()
        };
        let merged = merge_request_max_streaming_bitrate(summary, Some(4_000_000));
        assert_eq!(merged.max_streaming_bitrate, Some(4_000_000));
    }
}
