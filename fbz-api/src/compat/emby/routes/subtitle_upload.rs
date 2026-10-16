//! Jellyfin `POST /Videos/{itemId}/Subtitles`（UploadSubtitleDto）。
//!
//! 管理员上传字幕（Base64 `Data`），写为媒体同目录 sidecar
//! `{stem}.{lang}[.sdh][.forced].{fmt}`，并入队 `media.probe` 让 probe 的
//! sidecar 扫描写入 media_streams（随后 PlaybackInfo 可见）。

use std::path::{Path, PathBuf};

use axum::{
    Json,
    extract::{Path as AxumPath, State},
    http::{HeaderMap, StatusCode, Uri},
};
use serde::Deserialize;
use serde_json::json;

use crate::{
    error::AppError,
    media::{probe::MEDIA_PROBE_JOB_TYPE, repository::MediaRepository},
    state::AppState,
};

use super::access::authenticate_request_user;

const MAX_SUBTITLE_BYTES: usize = 10 * 1024 * 1024;
const ALLOWED_FORMATS: [&str; 4] = ["srt", "ass", "ssa", "vtt"];

#[derive(Clone, Debug, Default, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "PascalCase")]
pub struct UploadSubtitleDto {
    #[serde(default, alias = "language")]
    pub language: String,
    #[serde(default, alias = "format")]
    pub format: String,
    #[serde(default, alias = "isForced")]
    pub is_forced: bool,
    #[serde(default, alias = "isHearingImpaired")]
    pub is_hearing_impaired: bool,
    #[serde(default, alias = "data")]
    pub data: String,
}

/// 标准 Base64 解码（容忍空白、缺省 padding；也接受 URL-safe 字母表）。
pub fn decode_base64(input: &str) -> Option<Vec<u8>> {
    fn val(c: u8) -> Option<u32> {
        Some(match c {
            b'A'..=b'Z' => c - b'A',
            b'a'..=b'z' => c - b'a' + 26,
            b'0'..=b'9' => c - b'0' + 52,
            b'+' | b'-' => 62,
            b'/' | b'_' => 63,
            _ => return None,
        } as u32)
    }
    // 兼容 data URL 前缀。
    let input = input.split_once("base64,").map(|(_, rest)| rest).unwrap_or(input);
    let clean: Vec<u8> = input.bytes().filter(|b| !b.is_ascii_whitespace() && *b != b'=').collect();
    if clean.len() % 4 == 1 {
        return None;
    }
    let mut out = Vec::with_capacity(clean.len() * 3 / 4);
    for chunk in clean.chunks(4) {
        let mut acc = 0u32;
        for (i, &c) in chunk.iter().enumerate() {
            acc |= val(c)? << (18 - 6 * i);
        }
        let bytes = acc.to_be_bytes();
        out.extend_from_slice(&bytes[1..chunk.len()]);
    }
    Some(out)
}

fn normalize_language(raw: &str) -> Result<String, AppError> {
    let lang = raw.trim().to_ascii_lowercase();
    if lang.is_empty() {
        return Ok("und".to_owned());
    }
    if lang.len() > 16 || !lang.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-') {
        return Err(AppError::unprocessable("Language is invalid"));
    }
    Ok(lang)
}

fn normalize_format(raw: &str) -> Result<&'static str, AppError> {
    let fmt = raw.trim().trim_start_matches('.').to_ascii_lowercase();
    ALLOWED_FORMATS
        .into_iter()
        .find(|f| *f == fmt)
        .ok_or_else(|| AppError::unprocessable("Format must be one of srt/ass/ssa/vtt"))
}

/// sidecar 目标路径（probe 的 `sidecar_subtitle_language/is_forced` 能解析回语言与 forced）。
pub fn sidecar_path(media_path: &Path, language: &str, format: &str, forced: bool, sdh: bool) -> Option<PathBuf> {
    let stem = media_path.file_stem()?.to_str()?;
    let parent = media_path.parent()?;
    let mut name = format!("{stem}.{language}");
    if sdh {
        name.push_str(".sdh");
    }
    if forced {
        name.push_str(".forced");
    }
    name.push('.');
    name.push_str(format);
    Some(parent.join(name))
}

pub async fn upload_subtitle(
    State(state): State<AppState>,
    AxumPath(item_id): AxumPath<String>,
    headers: HeaderMap,
    uri: Uri,
    Json(body): Json<UploadSubtitleDto>,
) -> Result<StatusCode, AppError> {
    let user = authenticate_request_user(&state, &headers, &uri).await?;
    if !user.can_manage_server() {
        return Err(AppError::forbidden("subtitle management permission required"));
    }
    let language = normalize_language(&body.language)?;
    let format = normalize_format(&body.format)?;
    let data = decode_base64(&body.data).ok_or_else(|| AppError::unprocessable("Data is not valid base64"))?;
    if data.is_empty() || data.len() > MAX_SUBTITLE_BYTES {
        return Err(AppError::unprocessable("Data is empty or too large"));
    }

    write_sidecar(&state, user.id, &item_id, &language, format, body.is_forced, body.is_hearing_impaired, &data, "subtitle.upload").await?;
    Ok(StatusCode::NO_CONTENT)
}

/// 写 sidecar 并入队 probe（上传与远程下载共用）。返回写入路径。
#[allow(clippy::too_many_arguments)]
pub async fn write_sidecar(
    state: &AppState,
    user_id: i64,
    item_id: &str,
    language: &str,
    format: &str,
    forced: bool,
    sdh: bool,
    data: &[u8],
    reason: &str,
) -> Result<PathBuf, AppError> {
    let language = normalize_language(language)?;
    let format = normalize_format(format)?;
    let Some(database) = state.database() else {
        return Err(AppError::internal("database is not configured"));
    };
    let source = MediaRepository::new(database.clone())
        .find_playback_media_source(user_id, item_id, None)
        .await
        .map_err(|err| AppError::internal(format!("failed to get media source: {err}")))?
        .ok_or_else(|| AppError::not_found("item not found"))?;
    if source.is_strm {
        return Err(AppError::unprocessable("subtitle upload is not supported for STRM items"));
    }
    let target = sidecar_path(Path::new(source.path.trim()), &language, format, forced, sdh)
        .ok_or_else(|| AppError::unprocessable("media path has no file name"))?;
    tokio::fs::write(&target, data)
        .await
        .map_err(|err| AppError::internal(format!("failed to write subtitle: {err}")))?;

    // 重新 probe 以把 sidecar 写进 media_streams；已有未完成 probe job 时跳过。
    sqlx::query(
        r#"
        insert into jobs (job_type, status, queue_name, priority, payload)
        select $1, 'queued', 'probe', -3, jsonb_build_object('mediaFileId', $2::bigint, 'reason', $3::jsonb)
        where not exists (
            select 1 from jobs j
            where j.job_type = $1
              and j.status in ('queued', 'running')
              and j.payload->>'mediaFileId' = $2::bigint::text
        )
        on conflict do nothing
        "#,
    )
    .bind(MEDIA_PROBE_JOB_TYPE)
    .bind(source.media_file_id)
    .bind(json!(reason))
    .execute(database)
    .await
    .map_err(|err| AppError::internal(format!("failed to queue probe: {err}")))?;

    Ok(target)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn base64_roundtrip_cases() {
        assert_eq!(decode_base64("aGVsbG8=").unwrap(), b"hello");
        assert_eq!(decode_base64("aGVsbG8").unwrap(), b"hello");
        assert_eq!(decode_base64("aGVs\nbG8h").unwrap(), b"hello!");
        assert_eq!(decode_base64("data:text/plain;base64,MQ==").unwrap(), b"1");
        assert!(decode_base64("a").is_none());
        assert!(decode_base64("@@@@").is_none());
    }

    #[test]
    fn language_and_format_validation() {
        assert_eq!(normalize_language(" ZH-cn ").unwrap(), "zh-cn");
        assert_eq!(normalize_language("").unwrap(), "und");
        assert!(normalize_language("../x").is_err());
        assert_eq!(normalize_format(".SRT").unwrap(), "srt");
        assert!(normalize_format("exe").is_err());
    }

    #[test]
    fn sidecar_path_is_next_to_media_with_flags() {
        let p = sidecar_path(Path::new("/m/Show/S01E01.mkv"), "chi", "srt", true, true).unwrap();
        assert_eq!(p, PathBuf::from("/m/Show/S01E01.chi.sdh.forced.srt"));
        let p = sidecar_path(Path::new("/m/a.b.mp4"), "en", "vtt", false, false).unwrap();
        assert_eq!(p, PathBuf::from("/m/a.b.en.vtt"));
    }

    #[test]
    fn dto_accepts_jellyfin_shape() {
        let d: UploadSubtitleDto = serde_json::from_str(
            r#"{"Language":"eng","Format":"srt","IsForced":false,"IsHearingImpaired":true,"Data":"MQ=="}"#,
        )
        .unwrap();
        assert!(d.is_hearing_impaired);
        assert_eq!(d.format, "srt");
    }
}
