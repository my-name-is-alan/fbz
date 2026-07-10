use std::{
    error::Error,
    fmt::{Display, Formatter},
    path::{Path, PathBuf},
};

use crate::{
    config::{HardwareMode, TranscodeConfig},
    media::tools::MediaToolDiagnostics,
    transcode::repository::TranscodeClaimRecord,
};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FfmpegPlan {
    pub program: PathBuf,
    pub args: Vec<String>,
    pub manifest_path: PathBuf,
    pub output_dir: PathBuf,
    pub hardware_acceleration: Option<String>,
    pub software_fallback: bool,
}

/// 管理端「转码设置」（transcode_settings 单行表）落到 planner 的调优参数。
/// 全部可缺省：缺省时沿用 env TranscodeConfig 与内置默认，向后兼容。
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct TranscodeTuning {
    /// 硬件加速后端覆盖：`Some("none")` 强制软件编码；`Some(其它)` 覆盖 env 优先级。
    pub hardware_acceleration: Option<String>,
    /// 显式视频编码器（如 `hevc_nvenc`）；`None`/`auto` 时按硬件推导。
    pub preferred_encoder: Option<String>,
    /// 转码输出最大高度（480/720/1080/2160）；None = 不限制（original）。
    pub max_height: Option<i32>,
    /// HLS 分片时长（秒）。
    pub segment_duration: Option<i32>,
    /// 节流：按输入原生速率读取（`-re`），避免转码吃满磁盘/CPU。
    pub throttle: Option<bool>,
}

impl TranscodeTuning {
    /// 从管理端设置行构造。`none`/`auto`/`original` 等占位值归一化为 None 语义。
    pub fn from_settings(
        hardware_acceleration: &str,
        preferred_encoder: &str,
        max_resolution: &str,
        segment_duration: i32,
        throttle: bool,
    ) -> Self {
        let hardware = hardware_acceleration.trim().to_ascii_lowercase();
        let encoder = preferred_encoder.trim().to_ascii_lowercase();
        let max_height = match max_resolution.trim().to_ascii_lowercase().as_str() {
            "480" | "480p" => Some(480),
            "720" | "720p" => Some(720),
            "1080" | "1080p" => Some(1080),
            "2160" | "2160p" | "4k" => Some(2160),
            _ => None,
        };

        Self {
            hardware_acceleration: (!hardware.is_empty()).then_some(hardware),
            preferred_encoder: (!encoder.is_empty() && encoder != "auto").then_some(encoder),
            max_height,
            segment_duration: (segment_duration > 0).then_some(segment_duration.min(60)),
            throttle: Some(throttle),
        }
    }
}

#[derive(Debug)]
pub enum TranscodePlanError {
    MissingInputPath(String),
    MissingManifestPath(String),
    HardwareRequiredButUnavailable,
}

impl Display for TranscodePlanError {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::MissingInputPath(session_id) => {
                write!(f, "transcode session `{session_id}` is missing input path")
            }
            Self::MissingManifestPath(session_id) => {
                write!(
                    f,
                    "transcode session `{session_id}` is missing manifest path"
                )
            }
            Self::HardwareRequiredButUnavailable => f.write_str(
                "hardware transcoding is required but no hardware priority is configured",
            ),
        }
    }
}

impl Error for TranscodePlanError {}

pub fn build_ffmpeg_plan(
    config: &TranscodeConfig,
    tools: &MediaToolDiagnostics,
    session: &TranscodeClaimRecord,
) -> Result<FfmpegPlan, TranscodePlanError> {
    build_ffmpeg_plan_with_tuning(config, tools, session, &TranscodeTuning::default())
}

pub fn build_ffmpeg_plan_with_tuning(
    config: &TranscodeConfig,
    tools: &MediaToolDiagnostics,
    session: &TranscodeClaimRecord,
    tuning: &TranscodeTuning,
) -> Result<FfmpegPlan, TranscodePlanError> {
    let input_path = session
        .input_path
        .as_deref()
        .filter(|path| !path.trim().is_empty())
        .ok_or_else(|| TranscodePlanError::MissingInputPath(session.id.clone()))?;
    let manifest_path = session
        .manifest_path
        .as_deref()
        .filter(|path| !path.trim().is_empty())
        .ok_or_else(|| TranscodePlanError::MissingManifestPath(session.id.clone()))?;
    let manifest_path = PathBuf::from(manifest_path);
    let output_dir = session
        .output_path
        .as_deref()
        .filter(|path| !path.trim().is_empty())
        .map(PathBuf::from)
        .or_else(|| manifest_path.parent().map(Path::to_path_buf))
        .unwrap_or_else(|| PathBuf::from("."));
    let has_video = session
        .video_codec
        .as_deref()
        .map(str::trim)
        .is_some_and(|codec| !codec.is_empty());
    let hardware = if has_video {
        // 管理端设置优先：`none` 强制软件路径；其余值直接作为硬件后端。
        match tuning.hardware_acceleration.as_deref() {
            Some("none") => None,
            Some(acceleration) => Some(acceleration.to_owned()),
            None => choose_hardware(config)?,
        }
    } else {
        None
    };

    let mut args = vec!["-hide_banner".to_owned(), "-y".to_owned()];
    if tuning.throttle.unwrap_or(false) {
        // 节流：按输入原生速率读取，避免一次转码吃满磁盘带宽（多用户并发时更平滑）。
        args.push("-re".to_owned());
    }
    if let Some(acceleration) = &hardware {
        args.extend([
            "-hwaccel".to_owned(),
            hardware_accel_arg(acceleration).to_owned(),
        ]);
    }
    args.extend(["-i".to_owned(), input_path.to_owned()]);
    if has_video {
        let encoder = tuning
            .preferred_encoder
            .clone()
            .unwrap_or_else(|| video_encoder(hardware.as_deref()).to_owned());
        args.extend(["-map".to_owned(), "0:v:0?".to_owned()]);
        if let Some(max_height) = tuning.max_height {
            // 只降不升：输出高度钳到 min(上限, 源高)，宽度按比例（-2 保证偶数）。
            args.extend([
                "-vf".to_owned(),
                format!("scale=-2:min({max_height}\\,ih)"),
            ]);
        }
        args.extend(["-c:v".to_owned(), encoder]);
    }
    let segment_seconds = tuning.segment_duration.unwrap_or(4).clamp(1, 60);
    args.extend([
        "-map".to_owned(),
        "0:a:0?".to_owned(),
        "-c:a".to_owned(),
        audio_encoder(session.audio_codec.as_deref()).to_owned(),
        "-f".to_owned(),
        "hls".to_owned(),
        "-hls_time".to_owned(),
        segment_seconds.to_string(),
        "-hls_playlist_type".to_owned(),
        "vod".to_owned(),
        manifest_path.to_string_lossy().into_owned(),
    ]);

    Ok(FfmpegPlan {
        program: tools.ffmpeg.path.clone(),
        args,
        manifest_path,
        output_dir,
        hardware_acceleration: hardware,
        software_fallback: config.software_fallback,
    })
}

fn choose_hardware(config: &TranscodeConfig) -> Result<Option<String>, TranscodePlanError> {
    match config.hardware_mode {
        HardwareMode::Disabled | HardwareMode::SoftwareOnly => Ok(None),
        HardwareMode::Auto => Ok(config.hardware_priority.first().cloned()),
        HardwareMode::HardwareOnly => config
            .hardware_priority
            .first()
            .cloned()
            .map(Some)
            .ok_or(TranscodePlanError::HardwareRequiredButUnavailable),
    }
}

fn hardware_accel_arg(value: &str) -> &'static str {
    match value.to_ascii_lowercase().as_str() {
        "intel" | "qsv" => "qsv",
        "nvidia" | "cuda" | "nvenc" => "cuda",
        "amd" | "amf" | "d3d11va" => "d3d11va",
        "vaapi" => "vaapi",
        "videotoolbox" => "videotoolbox",
        _ => "auto",
    }
}

fn video_encoder(hardware: Option<&str>) -> &'static str {
    match hardware.map(str::to_ascii_lowercase).as_deref() {
        Some("intel" | "qsv") => "h264_qsv",
        Some("nvidia" | "cuda" | "nvenc") => "h264_nvenc",
        Some("amd" | "amf" | "d3d11va") => "h264_amf",
        Some("vaapi") => "h264_vaapi",
        Some("videotoolbox") => "h264_videotoolbox",
        _ => "libx264",
    }
}

fn audio_encoder(codec: Option<&str>) -> &str {
    codec
        .map(str::trim)
        .filter(|codec| !codec.is_empty())
        .unwrap_or("aac")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::media::tools::{MediaToolKind, MediaToolSource, ResolvedMediaTool};

    #[test]
    fn ffmpeg_plan_prefers_first_hardware_candidate_in_auto_mode() {
        let plan = build_ffmpeg_plan(
            &TranscodeConfig {
                max_concurrent: 3,
                lease_seconds: 900,
                hardware_mode: HardwareMode::Auto,
                hardware_priority: vec!["nvidia".to_owned(), "intel".to_owned()],
                software_fallback: true,
            },
            &tools(),
            &claim(),
        )
        .unwrap();

        assert_eq!(plan.program, PathBuf::from("ffmpeg"));
        assert_eq!(plan.hardware_acceleration.as_deref(), Some("nvidia"));
        assert!(plan.args.contains(&"h264_nvenc".to_owned()));
        assert!(plan.software_fallback);
    }

    #[test]
    fn ffmpeg_plan_uses_software_when_hardware_is_disabled() {
        let plan = build_ffmpeg_plan(
            &TranscodeConfig {
                max_concurrent: 3,
                lease_seconds: 900,
                hardware_mode: HardwareMode::Disabled,
                hardware_priority: vec!["nvidia".to_owned()],
                software_fallback: true,
            },
            &tools(),
            &claim(),
        )
        .unwrap();

        assert_eq!(plan.hardware_acceleration, None);
        assert!(plan.args.contains(&"libx264".to_owned()));
    }

    #[test]
    fn ffmpeg_plan_omits_video_encoder_for_audio_only_hls_sessions() {
        let session = TranscodeClaimRecord {
            video_codec: None,
            audio_codec: Some("mp3".to_owned()),
            ..claim()
        };

        let plan = build_ffmpeg_plan(
            &TranscodeConfig {
                max_concurrent: 3,
                lease_seconds: 900,
                hardware_mode: HardwareMode::Auto,
                hardware_priority: vec!["nvidia".to_owned()],
                software_fallback: true,
            },
            &tools(),
            &session,
        )
        .unwrap();

        assert_eq!(plan.hardware_acceleration, None);
        assert!(!plan.args.contains(&"0:v:0?".to_owned()));
        assert!(!plan.args.contains(&"-c:v".to_owned()));
        assert!(!plan.args.contains(&"libx264".to_owned()));
        assert!(plan.args.contains(&"0:a:0?".to_owned()));
        assert!(plan.args.windows(2).any(|args| args == ["-c:a", "mp3"]));
    }

    fn tools() -> MediaToolDiagnostics {
        MediaToolDiagnostics {
            ffmpeg: ResolvedMediaTool {
                kind: MediaToolKind::Ffmpeg,
                source: MediaToolSource::External,
                path: PathBuf::from("ffmpeg"),
                version_line: "ffmpeg test".to_owned(),
            },
            ffprobe: ResolvedMediaTool {
                kind: MediaToolKind::Ffprobe,
                source: MediaToolSource::External,
                path: PathBuf::from("ffprobe"),
                version_line: "ffprobe test".to_owned(),
            },
        }
    }

    fn claim() -> TranscodeClaimRecord {
        TranscodeClaimRecord {
            id: "session-1".to_owned(),
            status: "running".to_owned(),
            user_id: "user-1".to_owned(),
            item_id: "item-1".to_owned(),
            media_file_id: Some(2),
            hardware_acceleration: None,
            input_path: Some("input.mkv".to_owned()),
            output_path: Some("out".to_owned()),
            manifest_path: Some("out/master.m3u8".to_owned()),
            video_codec: Some("h264".to_owned()),
            audio_codec: Some("aac".to_owned()),
            container: Some("mkv".to_owned()),
            bitrate: Some(10_000_000),
            worker_id: "worker-1".to_owned(),
            lease_expires_at: "now".to_owned(),
            attempts: 1,
            max_attempts: 3,
        }
    }
}
