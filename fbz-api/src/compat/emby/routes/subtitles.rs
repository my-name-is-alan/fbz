use std::path::{Path, PathBuf};

use axum::{
    Json,
    body::Body,
    extract::{Path as AxumPath, Query, State},
    http::{
        HeaderMap, HeaderValue, StatusCode, Uri,
        header::{CACHE_CONTROL, CONTENT_LENGTH, CONTENT_TYPE},
    },
    response::Response,
};
use serde::Deserialize;
use serde_json::Value;
use tokio::fs::File;
use tokio_util::io::ReaderStream;

use crate::{
    compat::emby::dto::RemoteSubtitleInfoDto,
    error::AppError,
    library::repository::LibraryRepository,
    media::repository::{MediaRepository, SubtitleStreamRecord},
    state::AppState,
};

use super::access::authenticate_request_user;

#[derive(Clone, Debug, Default, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "PascalCase")]
pub struct RemoteSubtitleSearchQuery {
    #[serde(alias = "mediaSourceId", alias = "media_source_id")]
    pub media_source_id: Option<String>,
    #[serde(alias = "userId", alias = "user_id")]
    pub user_id: Option<String>,
    #[serde(alias = "isPerfectMatch", alias = "is_perfect_match")]
    pub is_perfect_match: Option<bool>,
    #[serde(alias = "isForced", alias = "is_forced")]
    pub is_forced: Option<bool>,
    #[serde(alias = "isHearingImpaired", alias = "is_hearing_impaired")]
    pub is_hearing_impaired: Option<bool>,
}

#[derive(Clone, Debug, Default, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "PascalCase")]
pub struct SubtitleStreamQuery {
    #[serde(alias = "startPositionTicks", alias = "start_position_ticks")]
    pub start_position_ticks: Option<i64>,
    #[serde(alias = "endPositionTicks", alias = "end_position_ticks")]
    pub end_position_ticks: Option<i64>,
    #[serde(alias = "copyTimestamps", alias = "copy_timestamps")]
    pub copy_timestamps: Option<bool>,
    #[serde(alias = "userId", alias = "user_id")]
    pub user_id: Option<String>,
}

#[derive(Clone, Debug, Default, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "PascalCase")]
pub struct HlsSubtitlePlaylistQuery {
    #[serde(alias = "subtitleSegmentLength", alias = "subtitle_segment_length")]
    pub subtitle_segment_length: Option<i64>,
    #[serde(alias = "manifestSubtitles", alias = "manifest_subtitles")]
    pub manifest_subtitles: Option<String>,
    #[serde(alias = "userId", alias = "user_id")]
    pub user_id: Option<String>,
}

#[derive(Clone, Debug, Default, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "PascalCase")]
pub struct RemoteSubtitleDownloadQuery {
    #[serde(alias = "mediaSourceId", alias = "media_source_id")]
    pub media_source_id: Option<String>,
}

#[derive(Clone, Debug, Default, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "PascalCase")]
pub struct DeleteSubtitleQuery {
    #[serde(alias = "mediaSourceId", alias = "media_source_id")]
    pub media_source_id: Option<String>,
}

const MAX_SUBTITLE_LANGUAGE_LEN: usize = 32;
const MAX_MEDIA_SOURCE_ID_LEN: usize = 256;
const MAX_REMOTE_SUBTITLE_ID_LEN: usize = 512;
const MAX_SUBTITLE_FORMAT_LEN: usize = 16;
const DEFAULT_HLS_SUBTITLE_SEGMENT_LENGTH_SECONDS: i64 = 4;
const MAX_HLS_SUBTITLE_SEGMENT_LENGTH_SECONDS: i64 = 3600;

pub async fn remote_subtitle_search(
    State(state): State<AppState>,
    AxumPath((item_id, language)): AxumPath<(String, String)>,
    Query(query): Query<RemoteSubtitleSearchQuery>,
    headers: HeaderMap,
    uri: Uri,
) -> Result<Json<Vec<RemoteSubtitleInfoDto>>, AppError> {
    let user = authenticate_request_user(&state, &headers, &uri).await?;
    if let Some(query_user_id) = query.user_id.as_deref()
        && query_user_id != user.public_id
    {
        return Err(AppError::forbidden(
            "authenticated user does not match query user",
        ));
    }

    let language = normalize_subtitle_language(&language)?;
    let _media_source_id = normalize_optional_media_source_id(query.media_source_id.as_deref())?;
    let _requested_match_flags = (
        query.is_perfect_match.unwrap_or(false),
        query.is_forced.unwrap_or(false),
        query.is_hearing_impaired.unwrap_or(false),
    );

    ensure_user_can_access_item(&state, user.id, &item_id).await?;

    Ok(Json(empty_remote_subtitle_results(language)))
}

pub async fn provider_subtitle_download(
    State(state): State<AppState>,
    AxumPath(subtitle_id): AxumPath<String>,
    headers: HeaderMap,
    uri: Uri,
) -> Result<StatusCode, AppError> {
    let _user = authenticate_request_user(&state, &headers, &uri).await?;
    let _subtitle_id = normalize_remote_subtitle_id(&subtitle_id)?;

    Err(AppError::conflict(
        "remote subtitle provider downloads are not configured",
    ))
}

pub async fn download_remote_subtitle(
    State(state): State<AppState>,
    AxumPath((item_id, subtitle_id)): AxumPath<(String, String)>,
    Query(query): Query<RemoteSubtitleDownloadQuery>,
    headers: HeaderMap,
    uri: Uri,
) -> Result<StatusCode, AppError> {
    let user = authenticate_request_user(&state, &headers, &uri).await?;
    let _subtitle_id = normalize_remote_subtitle_id(&subtitle_id)?;
    let _media_source_id = normalize_required_media_source_id(query.media_source_id.as_deref())?;
    ensure_user_can_access_item(&state, user.id, &item_id).await?;

    Err(AppError::conflict(
        "remote subtitle downloads are not configured",
    ))
}

pub async fn delete_item_subtitle(
    State(state): State<AppState>,
    AxumPath((item_id, index)): AxumPath<(String, String)>,
    Query(query): Query<DeleteSubtitleQuery>,
    headers: HeaderMap,
    uri: Uri,
) -> Result<StatusCode, AppError> {
    delete_subtitle_response(state, item_id, index, query, headers, uri).await
}

pub async fn delete_video_subtitle(
    State(state): State<AppState>,
    AxumPath((item_id, index)): AxumPath<(String, String)>,
    Query(query): Query<DeleteSubtitleQuery>,
    headers: HeaderMap,
    uri: Uri,
) -> Result<StatusCode, AppError> {
    delete_subtitle_response(state, item_id, index, query, headers, uri).await
}

pub async fn subtitle_stream(
    State(state): State<AppState>,
    AxumPath((item_id, media_source_id, index, format)): AxumPath<(String, String, String, String)>,
    Query(query): Query<SubtitleStreamQuery>,
    headers: HeaderMap,
    uri: Uri,
) -> Result<Response, AppError> {
    subtitle_stream_response(
        state,
        SubtitleStreamPath {
            item_id,
            media_source_id,
            index,
            start_position_ticks: None,
            format,
        },
        query,
        headers,
        uri,
    )
    .await
}

pub async fn subtitle_stream_with_start_position(
    State(state): State<AppState>,
    AxumPath((item_id, media_source_id, index, start_position_ticks, format)): AxumPath<(
        String,
        String,
        String,
        String,
        String,
    )>,
    Query(query): Query<SubtitleStreamQuery>,
    headers: HeaderMap,
    uri: Uri,
) -> Result<Response, AppError> {
    subtitle_stream_response(
        state,
        SubtitleStreamPath {
            item_id,
            media_source_id,
            index,
            start_position_ticks: Some(start_position_ticks),
            format,
        },
        query,
        headers,
        uri,
    )
    .await
}

pub async fn video_attachment_stream(
    State(state): State<AppState>,
    AxumPath((item_id, media_source_id, index)): AxumPath<(String, String, String)>,
    headers: HeaderMap,
    uri: Uri,
) -> Result<Response, AppError> {
    let user = authenticate_request_user(&state, &headers, &uri).await?;
    let media_file_id = parse_media_source_id(&media_source_id)?;
    let attachment_index = parse_subtitle_stream_index(&index)?;
    let Some(database) = state.database() else {
        return Err(AppError::internal("database is not configured"));
    };

    let attachment = MediaRepository::new(database.clone())
        .find_attachment_stream(user.id, &item_id, media_file_id, attachment_index)
        .await
        .map_err(|err| AppError::internal(format!("failed to get attachment stream: {err}")))?
        .ok_or_else(|| AppError::not_found("attachment stream not found"))?;

    let cache_path = subtitle_cache_path(
        &state,
        attachment.media_file_id,
        attachment.stream_index,
        "attachment",
    )?;
    if !cache_file_exists(&cache_path).await {
        extract_attachment_to_cache(&state, &attachment, &cache_path).await?;
    }

    attachment_file_response(&cache_path).await
}

pub async fn hls_subtitle_playlist(
    State(state): State<AppState>,
    AxumPath(item_id): AxumPath<String>,
    Query(query): Query<HlsSubtitlePlaylistQuery>,
    headers: HeaderMap,
    uri: Uri,
) -> Result<Response, AppError> {
    hls_subtitle_playlist_response(state, item_id, query, headers, uri).await
}

pub async fn hls_live_subtitle_playlist(
    State(state): State<AppState>,
    AxumPath(item_id): AxumPath<String>,
    Query(query): Query<HlsSubtitlePlaylistQuery>,
    headers: HeaderMap,
    uri: Uri,
) -> Result<Response, AppError> {
    hls_subtitle_playlist_response(state, item_id, query, headers, uri).await
}

async fn hls_subtitle_playlist_response(
    state: AppState,
    item_id: String,
    query: HlsSubtitlePlaylistQuery,
    headers: HeaderMap,
    uri: Uri,
) -> Result<Response, AppError> {
    let user = authenticate_request_user(&state, &headers, &uri).await?;
    if let Some(query_user_id) = query.user_id.as_deref()
        && query_user_id != user.public_id
    {
        return Err(AppError::forbidden(
            "authenticated user does not match query user",
        ));
    }
    let input = hls_subtitle_playlist_input(&query)?;
    ensure_user_can_access_item(&state, user.id, &item_id).await?;

    Ok(hls_subtitle_playlist_response_body(
        input.target_duration_seconds,
    ))
}

async fn subtitle_stream_response(
    state: AppState,
    path: SubtitleStreamPath,
    query: SubtitleStreamQuery,
    headers: HeaderMap,
    uri: Uri,
) -> Result<Response, AppError> {
    let user = authenticate_request_user(&state, &headers, &uri).await?;
    if let Some(query_user_id) = query.user_id.as_deref()
        && query_user_id != user.public_id
    {
        return Err(AppError::forbidden(
            "authenticated user does not match query user",
        ));
    }

    let media_file_id = parse_media_source_id(&path.media_source_id)?;
    let stream_index = parse_subtitle_stream_index(&path.index)?;
    let format = normalize_subtitle_format(&path.format)?;
    validate_subtitle_ticks(
        path.start_position_ticks.as_deref(),
        query.start_position_ticks,
        query.end_position_ticks,
    )?;
    let _copy_timestamps = query.copy_timestamps.unwrap_or(false);
    let Some(database) = state.database() else {
        return Err(AppError::internal("database is not configured"));
    };
    let subtitle = MediaRepository::new(database.clone())
        .find_subtitle_stream(user.id, &path.item_id, media_file_id, stream_index)
        .await
        .map_err(|err| AppError::internal(format!("failed to get subtitle stream: {err}")))?
        .ok_or_else(|| AppError::not_found("subtitle stream not found"))?;

    if let Some(source_path) = external_subtitle_path(&subtitle)? {
        // 外挂字幕：同格式直接流式返回；格式不同（典型 srt→vtt）经 ffmpeg 转换缓存。
        if external_subtitle_matches_format(&subtitle, &source_path, format) {
            return local_subtitle_response(&source_path, format).await;
        }
        let cache_path =
            subtitle_cache_path(&state, subtitle.media_file_id, subtitle.stream_index, format)?;
        if !cache_file_exists(&cache_path).await {
            convert_subtitle_to_cache(&state, &source_path, format, &cache_path).await?;
        }
        return local_subtitle_response(&cache_path, format).await;
    }

    // 内嵌字幕：ffmpeg 按流序号抽取转换到请求格式（图形字幕如 PGS 无法转文本 → 422）。
    let cache_path =
        subtitle_cache_path(&state, subtitle.media_file_id, subtitle.stream_index, format)?;
    if !cache_file_exists(&cache_path).await {
        extract_embedded_subtitle_to_cache(&state, &subtitle, format, &cache_path).await?;
    }

    local_subtitle_response(&cache_path, format).await
}

async fn ensure_user_can_access_item(
    state: &AppState,
    user_id: i64,
    item_id: &str,
) -> Result<(), AppError> {
    let Some(database) = state.database() else {
        return Err(AppError::internal("database is not configured"));
    };

    let item = LibraryRepository::new(database.clone())
        .find_user_item_by_id(user_id, item_id)
        .await
        .map_err(|err| AppError::internal(format!("failed to get media item: {err}")))?;
    if item.is_none() {
        return Err(AppError::not_found("item not found"));
    }

    Ok(())
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct SubtitleStreamPath {
    item_id: String,
    media_source_id: String,
    index: String,
    start_position_ticks: Option<String>,
    format: String,
}

fn normalize_subtitle_language(value: &str) -> Result<String, AppError> {
    let value = value.trim();
    if value.is_empty() {
        return Err(AppError::unprocessable("subtitle language is required"));
    }

    if value.len() > MAX_SUBTITLE_LANGUAGE_LEN
        || !value
            .chars()
            .all(|ch| ch.is_ascii_alphanumeric() || matches!(ch, '-' | '_'))
    {
        return Err(AppError::unprocessable("subtitle language is invalid"));
    }

    Ok(value.to_ascii_lowercase())
}

fn normalize_optional_media_source_id(value: Option<&str>) -> Result<Option<String>, AppError> {
    let Some(value) = value.map(str::trim).filter(|value| !value.is_empty()) else {
        return Ok(None);
    };

    if value.len() > MAX_MEDIA_SOURCE_ID_LEN {
        return Err(AppError::unprocessable("MediaSourceId is too long"));
    }

    Ok(Some(value.to_owned()))
}

fn parse_media_source_id(value: &str) -> Result<i64, AppError> {
    let value = value.trim();
    if value.is_empty() {
        return Err(AppError::unprocessable("MediaSourceId is required"));
    }
    value
        .parse::<i64>()
        .ok()
        .filter(|id| *id > 0)
        .ok_or_else(|| AppError::unprocessable("MediaSourceId is invalid"))
}

fn parse_subtitle_stream_index(value: &str) -> Result<i32, AppError> {
    let value = value.trim();
    if value.is_empty() {
        return Err(AppError::unprocessable("subtitle stream index is required"));
    }
    value
        .parse::<i32>()
        .ok()
        .filter(|index| *index >= 0)
        .ok_or_else(|| AppError::unprocessable("subtitle stream index is invalid"))
}

fn normalize_subtitle_format(value: &str) -> Result<&'static str, AppError> {
    let value = value.trim().trim_start_matches('.').to_ascii_lowercase();
    if value.is_empty() {
        return Err(AppError::unprocessable("subtitle format is required"));
    }
    if value.len() > MAX_SUBTITLE_FORMAT_LEN
        || !value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'-'))
    {
        return Err(AppError::unprocessable("subtitle format is invalid"));
    }

    match value.as_str() {
        "srt" => Ok("srt"),
        "vtt" | "webvtt" => Ok("vtt"),
        "ass" => Ok("ass"),
        "ssa" => Ok("ssa"),
        "sub" => Ok("sub"),
        _ => Err(AppError::unprocessable("subtitle format is not supported")),
    }
}

fn validate_subtitle_ticks(
    path_start_position_ticks: Option<&str>,
    query_start_position_ticks: Option<i64>,
    query_end_position_ticks: Option<i64>,
) -> Result<(), AppError> {
    let path_start_position_ticks = path_start_position_ticks
        .map(parse_subtitle_tick)
        .transpose()?;
    let start = path_start_position_ticks.or(query_start_position_ticks);
    if start.is_some_and(|value| value < 0) {
        return Err(AppError::unprocessable(
            "StartPositionTicks must be non-negative",
        ));
    }
    if query_end_position_ticks.is_some_and(|value| value < 0) {
        return Err(AppError::unprocessable(
            "EndPositionTicks must be non-negative",
        ));
    }
    if let (Some(start), Some(end)) = (start, query_end_position_ticks)
        && end < start
    {
        return Err(AppError::unprocessable(
            "EndPositionTicks must be greater than or equal to StartPositionTicks",
        ));
    }

    Ok(())
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct HlsSubtitlePlaylistInput {
    target_duration_seconds: i64,
    manifest_subtitle_format: Option<String>,
}

fn hls_subtitle_playlist_input(
    query: &HlsSubtitlePlaylistQuery,
) -> Result<HlsSubtitlePlaylistInput, AppError> {
    let target_duration_seconds = match query.subtitle_segment_length {
        Some(value) if (1..=MAX_HLS_SUBTITLE_SEGMENT_LENGTH_SECONDS).contains(&value) => value,
        Some(_) => {
            return Err(AppError::unprocessable(
                "SubtitleSegmentLength must be between 1 and 3600 seconds",
            ));
        }
        None => DEFAULT_HLS_SUBTITLE_SEGMENT_LENGTH_SECONDS,
    };
    let manifest_subtitle_format = query
        .manifest_subtitles
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(normalize_subtitle_format)
        .transpose()?
        .map(str::to_owned);

    Ok(HlsSubtitlePlaylistInput {
        target_duration_seconds,
        manifest_subtitle_format,
    })
}

fn hls_subtitle_playlist_response_body(target_duration_seconds: i64) -> Response {
    let mut response = Response::new(Body::from(empty_hls_subtitle_playlist(
        target_duration_seconds,
    )));
    *response.status_mut() = StatusCode::OK;
    response.headers_mut().insert(
        CONTENT_TYPE,
        HeaderValue::from_static("application/vnd.apple.mpegurl"),
    );
    response
        .headers_mut()
        .insert(CACHE_CONTROL, HeaderValue::from_static("no-store"));
    response
}

fn empty_hls_subtitle_playlist(target_duration_seconds: i64) -> String {
    format!(
        "#EXTM3U\n#EXT-X-VERSION:3\n#EXT-X-TARGETDURATION:{target_duration_seconds}\n#EXT-X-MEDIA-SEQUENCE:0\n#EXT-X-PLAYLIST-TYPE:VOD\n#EXT-X-ENDLIST\n"
    )
}

fn parse_subtitle_tick(value: &str) -> Result<i64, AppError> {
    value
        .trim()
        .parse::<i64>()
        .map_err(|_| AppError::unprocessable("StartPositionTicks is invalid"))
}

fn external_subtitle_path(subtitle: &SubtitleStreamRecord) -> Result<Option<PathBuf>, AppError> {
    if !subtitle.is_external {
        return Ok(None);
    }
    let Some(raw_path) = external_subtitle_path_value(&subtitle.extra) else {
        return Ok(None);
    };

    resolve_external_subtitle_path(&subtitle.media_path, raw_path).map(Some)
}

fn external_subtitle_path_value(extra: &Value) -> Option<&str> {
    [
        "path",
        "filePath",
        "externalPath",
        "subtitlePath",
        "storagePath",
    ]
    .into_iter()
    .find_map(|key| extra.get(key).and_then(Value::as_str))
    .map(str::trim)
    .filter(|value| !value.is_empty())
}

fn resolve_external_subtitle_path(
    media_path: &str,
    raw_subtitle_path: &str,
) -> Result<PathBuf, AppError> {
    let media_path = Path::new(media_path.trim());
    let media_parent = media_path
        .parent()
        .ok_or_else(|| AppError::not_found("media source directory not found"))?;
    let subtitle_path = Path::new(raw_subtitle_path.trim());
    let candidate = if subtitle_path.is_absolute() {
        subtitle_path.to_path_buf()
    } else {
        media_parent.join(subtitle_path)
    };

    let media_parent = std::fs::canonicalize(media_parent)
        .map_err(|err| subtitle_path_io_error(err, "media source directory not found"))?;
    let candidate = std::fs::canonicalize(&candidate)
        .map_err(|err| subtitle_path_io_error(err, "subtitle file not found"))?;
    if !candidate.starts_with(&media_parent) {
        return Err(AppError::forbidden(
            "subtitle file must stay under the media source directory",
        ));
    }

    Ok(candidate)
}

/// 外挂字幕源格式与请求格式一致时可直出（无需转换）。
fn external_subtitle_matches_format(
    subtitle: &SubtitleStreamRecord,
    path: &Path,
    requested_format: &str,
) -> bool {
    let source_format = path
        .extension()
        .and_then(|value| value.to_str())
        .or(subtitle.codec.as_deref())
        .unwrap_or_default()
        .trim()
        .trim_start_matches('.')
        .to_ascii_lowercase();

    matches!(normalize_subtitle_format(&source_format), Ok(format) if format == requested_format)
}

/// 抽取/转换产物缓存路径：`{transcode_cache_dir}/subtitles/{fileId}-{streamIndex}.{ext}`。
/// 落在转码缓存目录下，可被管理端"清理缓存"回收并按需重建。
fn subtitle_cache_path(
    state: &AppState,
    media_file_id: i64,
    stream_index: i32,
    extension: &str,
) -> Result<PathBuf, AppError> {
    if !extension
        .bytes()
        .all(|byte| byte.is_ascii_alphanumeric())
    {
        return Err(AppError::unprocessable("subtitle format is invalid"));
    }

    Ok(state
        .config()
        .storage
        .transcode_cache_dir
        .join("subtitles")
        .join(format!("{media_file_id}-{stream_index}.{extension}")))
}

async fn cache_file_exists(path: &Path) -> bool {
    tokio::fs::metadata(path)
        .await
        .map(|metadata| metadata.is_file() && metadata.len() > 0)
        .unwrap_or(false)
}

/// 目标格式 → (ffmpeg 字幕编码器, muxer)。`sub`（MicroDVD/VobSub）不支持作为转换目标。
fn subtitle_codec_and_muxer(format: &str) -> Result<(&'static str, &'static str), AppError> {
    match format {
        "srt" => Ok(("srt", "srt")),
        "vtt" => Ok(("webvtt", "webvtt")),
        "ass" | "ssa" => Ok(("ass", "ass")),
        _ => Err(AppError::unprocessable(
            "subtitle format conversion target is not supported",
        )),
    }
}

fn resolved_ffmpeg_path(state: &AppState) -> Result<PathBuf, AppError> {
    crate::media::tools::resolve_media_tools(&state.config().media_tools)
        .map(|tools| tools.ffmpeg.path)
        .map_err(|err| AppError::internal(format!("ffmpeg is not available: {err}")))
}

/// 运行 ffmpeg（60 秒超时），失败/超时返回 422/500。stdout/stderr 丢弃。
async fn run_ffmpeg_for_subtitles(
    ffmpeg: &Path,
    args: &[std::ffi::OsString],
    failure_message: &'static str,
    allow_nonzero_exit: bool,
) -> Result<(), AppError> {
    let mut command = tokio::process::Command::new(ffmpeg);
    command
        .args(args)
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null());
    #[cfg(windows)]
    {
        // CREATE_NO_WINDOW：避免 Windows 服务/桌面环境下弹出控制台窗口。
        command.creation_flags(0x0800_0000);
    }

    let mut child = command
        .spawn()
        .map_err(|err| AppError::internal(format!("failed to start ffmpeg: {err}")))?;
    let status = match tokio::time::timeout(
        std::time::Duration::from_secs(60),
        child.wait(),
    )
    .await
    {
        Ok(result) => {
            result.map_err(|err| AppError::internal(format!("ffmpeg wait failed: {err}")))?
        }
        Err(_) => {
            let _ = child.kill().await;
            return Err(AppError::internal("ffmpeg timed out"));
        }
    };
    if !status.success() && !allow_nonzero_exit {
        return Err(AppError::unprocessable(failure_message));
    }

    Ok(())
}

async fn ensure_cache_parent(path: &Path) -> Result<(), AppError> {
    if let Some(parent) = path.parent() {
        tokio::fs::create_dir_all(parent).await.map_err(|err| {
            AppError::internal(format!("failed to create subtitle cache directory: {err}"))
        })?;
    }

    Ok(())
}

/// 外挂字幕格式转换（如 srt→vtt）到缓存文件。
async fn convert_subtitle_to_cache(
    state: &AppState,
    source: &Path,
    format: &str,
    cache_path: &Path,
) -> Result<(), AppError> {
    let (codec, muxer) = subtitle_codec_and_muxer(format)?;
    let ffmpeg = resolved_ffmpeg_path(state)?;
    ensure_cache_parent(cache_path).await?;

    let args: Vec<std::ffi::OsString> = vec![
        "-y".into(),
        "-i".into(),
        source.as_os_str().to_owned(),
        "-map".into(),
        "0:s:0".into(),
        "-c:s".into(),
        codec.into(),
        "-f".into(),
        muxer.into(),
        cache_path.as_os_str().to_owned(),
    ];
    run_ffmpeg_for_subtitles(
        &ffmpeg,
        &args,
        "subtitle format conversion failed",
        false,
    )
    .await?;

    if !cache_file_exists(cache_path).await {
        return Err(AppError::unprocessable("subtitle format conversion failed"));
    }

    Ok(())
}

/// 内嵌字幕抽取转换到缓存文件（按容器内流序号 map）。
async fn extract_embedded_subtitle_to_cache(
    state: &AppState,
    subtitle: &SubtitleStreamRecord,
    format: &str,
    cache_path: &Path,
) -> Result<(), AppError> {
    let (codec, muxer) = subtitle_codec_and_muxer(format)?;
    let ffmpeg = resolved_ffmpeg_path(state)?;
    ensure_cache_parent(cache_path).await?;

    let args: Vec<std::ffi::OsString> = vec![
        "-y".into(),
        "-i".into(),
        Path::new(&subtitle.media_path).as_os_str().to_owned(),
        "-map".into(),
        format!("0:{}", subtitle.stream_index).into(),
        "-c:s".into(),
        codec.into(),
        "-f".into(),
        muxer.into(),
        cache_path.as_os_str().to_owned(),
    ];
    run_ffmpeg_for_subtitles(
        &ffmpeg,
        &args,
        "embedded subtitle extraction failed (graphic subtitles cannot be converted to text)",
        false,
    )
    .await?;

    if !cache_file_exists(cache_path).await {
        return Err(AppError::unprocessable(
            "embedded subtitle extraction failed (graphic subtitles cannot be converted to text)",
        ));
    }

    Ok(())
}

/// 内嵌附件（字体等）抽取到缓存文件。`-dump_attachment` 在无输出文件时以非零
/// 退出码结束但附件已落盘，因此容忍非零退出、以产物存在与否判定成败。
async fn extract_attachment_to_cache(
    state: &AppState,
    attachment: &SubtitleStreamRecord,
    cache_path: &Path,
) -> Result<(), AppError> {
    let ffmpeg = resolved_ffmpeg_path(state)?;
    ensure_cache_parent(cache_path).await?;

    let args: Vec<std::ffi::OsString> = vec![
        "-y".into(),
        format!("-dump_attachment:{}", attachment.stream_index).into(),
        cache_path.as_os_str().to_owned(),
        "-i".into(),
        Path::new(&attachment.media_path).as_os_str().to_owned(),
    ];
    run_ffmpeg_for_subtitles(&ffmpeg, &args, "attachment extraction failed", true).await?;

    if !cache_file_exists(cache_path).await {
        return Err(AppError::not_found("attachment could not be extracted"));
    }

    Ok(())
}

async fn attachment_file_response(path: &Path) -> Result<Response, AppError> {
    let metadata = tokio::fs::metadata(path)
        .await
        .map_err(|err| subtitle_path_io_error(err, "attachment file not found"))?;
    if !metadata.is_file() {
        return Err(AppError::not_found("attachment file not found"));
    }

    let file = File::open(path)
        .await
        .map_err(|err| subtitle_path_io_error(err, "attachment file not found"))?;
    let stream = ReaderStream::new(file);
    let mut response = Response::new(Body::from_stream(stream));
    *response.status_mut() = StatusCode::OK;
    response.headers_mut().insert(
        CONTENT_TYPE,
        HeaderValue::from_static("application/octet-stream"),
    );
    response.headers_mut().insert(
        CACHE_CONTROL,
        HeaderValue::from_static("public, max-age=86400"),
    );
    if let Ok(value) = HeaderValue::from_str(&metadata.len().to_string()) {
        response.headers_mut().insert(CONTENT_LENGTH, value);
    }

    Ok(response)
}

async fn local_subtitle_response(path: &Path, format: &str) -> Result<Response, AppError> {
    let metadata = tokio::fs::metadata(path)
        .await
        .map_err(|err| subtitle_path_io_error(err, "subtitle file not found"))?;
    if !metadata.is_file() {
        return Err(AppError::not_found("subtitle path is not a file"));
    }

    let file = File::open(path)
        .await
        .map_err(|err| subtitle_path_io_error(err, "subtitle file not found"))?;
    let stream = ReaderStream::new(file);
    let mut response = Response::new(Body::from_stream(stream));
    *response.status_mut() = StatusCode::OK;
    response.headers_mut().insert(
        CONTENT_TYPE,
        HeaderValue::from_static(subtitle_content_type(format)),
    );
    response
        .headers_mut()
        .insert(CACHE_CONTROL, HeaderValue::from_static("no-store"));
    if let Ok(value) = HeaderValue::from_str(&metadata.len().to_string()) {
        response.headers_mut().insert(CONTENT_LENGTH, value);
    }

    Ok(response)
}

async fn delete_subtitle_response(
    state: AppState,
    item_id: String,
    index: String,
    query: DeleteSubtitleQuery,
    headers: HeaderMap,
    uri: Uri,
) -> Result<StatusCode, AppError> {
    let user = authenticate_request_user(&state, &headers, &uri).await?;
    let _media_source_id = normalize_required_media_source_id(query.media_source_id.as_deref())?;
    let _stream_index = parse_subtitle_stream_index(&index)?;
    ensure_user_can_access_item(&state, user.id, &item_id).await?;

    Err(AppError::conflict(
        "external subtitle deletion is managed by FBZ subtitle indexing",
    ))
}

fn subtitle_content_type(format: &str) -> &'static str {
    match format {
        "vtt" => "text/vtt; charset=utf-8",
        "ass" | "ssa" => "text/x-ssa; charset=utf-8",
        "srt" | "sub" => "application/x-subrip; charset=utf-8",
        _ => "text/plain; charset=utf-8",
    }
}

fn subtitle_path_io_error(error: std::io::Error, not_found_message: &'static str) -> AppError {
    if error.kind() == std::io::ErrorKind::NotFound {
        return AppError::not_found(not_found_message);
    }

    AppError::internal(format!("failed to read subtitle file: {error}"))
}

fn empty_remote_subtitle_results(_language: String) -> Vec<RemoteSubtitleInfoDto> {
    Vec::new()
}

fn normalize_remote_subtitle_id(value: &str) -> Result<String, AppError> {
    let value = value.trim();
    if value.is_empty() {
        return Err(AppError::unprocessable("SubtitleId is required"));
    }
    if value.len() > MAX_REMOTE_SUBTITLE_ID_LEN
        || value
            .chars()
            .any(|ch| ch.is_control() || matches!(ch, '/' | '\\'))
    {
        return Err(AppError::unprocessable("SubtitleId is invalid"));
    }

    Ok(value.to_owned())
}

fn normalize_required_media_source_id(value: Option<&str>) -> Result<String, AppError> {
    normalize_optional_media_source_id(value)?
        .ok_or_else(|| AppError::unprocessable("MediaSourceId is required"))
}

#[cfg(test)]
mod tests {
    use std::{
        fs as std_fs, process,
        time::{SystemTime, UNIX_EPOCH},
    };

    use axum::{extract::Query, http::Uri};
    use serde_json::json;

    use super::*;

    #[test]
    fn subtitle_language_is_normalized_and_bounded() {
        assert_eq!(normalize_subtitle_language(" ZH-cn ").unwrap(), "zh-cn");
        assert_eq!(normalize_subtitle_language("pt_BR").unwrap(), "pt_br");
        assert!(normalize_subtitle_language("").is_err());
        assert!(normalize_subtitle_language("zh cn").is_err());
        assert!(normalize_subtitle_language(&"x".repeat(33)).is_err());
    }

    #[test]
    fn media_source_id_is_optional_trimmed_and_bounded() {
        assert_eq!(normalize_optional_media_source_id(None).unwrap(), None);
        assert_eq!(
            normalize_optional_media_source_id(Some("  ")).unwrap(),
            None
        );
        assert_eq!(
            normalize_optional_media_source_id(Some(" source-1 "))
                .unwrap()
                .as_deref(),
            Some("source-1")
        );
        assert!(normalize_optional_media_source_id(Some(&"x".repeat(257))).is_err());
    }

    #[test]
    fn remote_subtitle_download_values_are_bounded() {
        assert_eq!(
            normalize_remote_subtitle_id(" provider:sub-1 ").unwrap(),
            "provider:sub-1"
        );
        assert_eq!(
            normalize_required_media_source_id(Some(" source-1 ")).unwrap(),
            "source-1"
        );
        assert!(normalize_remote_subtitle_id("").is_err());
        assert!(normalize_remote_subtitle_id("provider/sub-1").is_err());
        assert!(normalize_remote_subtitle_id(&"x".repeat(513)).is_err());
        assert!(normalize_required_media_source_id(None).is_err());
    }

    #[test]
    fn remote_subtitle_queries_accept_lower_camel_client_fields() {
        let uri: Uri = concat!(
            "/emby/Items/item-1/RemoteSearch/Subtitles/eng?",
            "mediaSourceId=source-1&userId=user-1",
            "&isPerfectMatch=true&isForced=false&isHearingImpaired=true"
        )
        .parse()
        .unwrap();
        let Query(search) = Query::<RemoteSubtitleSearchQuery>::try_from_uri(&uri).unwrap();

        assert_eq!(search.media_source_id.as_deref(), Some("source-1"));
        assert_eq!(search.user_id.as_deref(), Some("user-1"));
        assert_eq!(search.is_perfect_match, Some(true));
        assert_eq!(search.is_forced, Some(false));
        assert_eq!(search.is_hearing_impaired, Some(true));

        let uri: Uri = "/emby/Items/item-1/RemoteSearch/Subtitles/sub-1?mediaSourceId=42"
            .parse()
            .unwrap();
        let Query(download) = Query::<RemoteSubtitleDownloadQuery>::try_from_uri(&uri).unwrap();

        assert_eq!(download.media_source_id.as_deref(), Some("42"));

        let uri: Uri = "/emby/Items/item-1/Subtitles/3?mediaSourceId=42"
            .parse()
            .unwrap();
        let Query(delete) = Query::<DeleteSubtitleQuery>::try_from_uri(&uri).unwrap();

        assert_eq!(delete.media_source_id.as_deref(), Some("42"));
    }

    #[test]
    fn subtitle_stream_query_accepts_lower_camel_client_fields() {
        let uri: Uri = concat!(
            "/emby/Items/item-1/42/Subtitles/3/Stream.srt?",
            "startPositionTicks=10&endPositionTicks=20",
            "&copyTimestamps=true&userId=user-1"
        )
        .parse()
        .unwrap();
        let Query(query) = Query::<SubtitleStreamQuery>::try_from_uri(&uri).unwrap();

        assert_eq!(query.start_position_ticks, Some(10));
        assert_eq!(query.end_position_ticks, Some(20));
        assert_eq!(query.copy_timestamps, Some(true));
        assert_eq!(query.user_id.as_deref(), Some("user-1"));
    }

    #[test]
    fn hls_subtitle_playlist_query_accepts_lower_camel_client_fields() {
        let uri: Uri = concat!(
            "/emby/Videos/item-1/subtitles.m3u8?",
            "subtitleSegmentLength=6&manifestSubtitles=webvtt&userId=user-1"
        )
        .parse()
        .unwrap();
        let Query(query) = Query::<HlsSubtitlePlaylistQuery>::try_from_uri(&uri).unwrap();

        assert_eq!(query.user_id.as_deref(), Some("user-1"));

        let input = hls_subtitle_playlist_input(&query).unwrap();

        assert_eq!(input.target_duration_seconds, 6);
        assert_eq!(input.manifest_subtitle_format.as_deref(), Some("vtt"));
    }

    #[test]
    fn subtitle_stream_path_values_are_bounded() {
        assert_eq!(parse_media_source_id("42").unwrap(), 42);
        assert!(parse_media_source_id("").is_err());
        assert!(parse_media_source_id("source-1").is_err());
        assert!(parse_media_source_id("-1").is_err());

        assert_eq!(parse_subtitle_stream_index("0").unwrap(), 0);
        assert!(parse_subtitle_stream_index("-1").is_err());
        assert!(parse_subtitle_stream_index("bad").is_err());
    }

    #[test]
    fn subtitle_format_is_normalized_and_allowlisted() {
        assert_eq!(normalize_subtitle_format(".SRT").unwrap(), "srt");
        assert_eq!(normalize_subtitle_format("webvtt").unwrap(), "vtt");
        assert_eq!(subtitle_content_type("vtt"), "text/vtt; charset=utf-8");
        assert_eq!(
            subtitle_content_type("srt"),
            "application/x-subrip; charset=utf-8"
        );
        assert!(normalize_subtitle_format("../srt").is_err());
        assert!(normalize_subtitle_format("ttml").is_err());
    }

    #[test]
    fn subtitle_tick_window_rejects_invalid_ranges() {
        assert!(validate_subtitle_ticks(Some("0"), None, Some(10)).is_ok());
        assert!(validate_subtitle_ticks(None, Some(10), Some(10)).is_ok());
        assert!(validate_subtitle_ticks(Some("-1"), None, None).is_err());
        assert!(validate_subtitle_ticks(None, Some(20), Some(10)).is_err());
    }

    #[test]
    fn hls_subtitle_playlist_query_is_normalized_and_bounded() {
        let input = hls_subtitle_playlist_input(&HlsSubtitlePlaylistQuery {
            subtitle_segment_length: Some(6),
            manifest_subtitles: Some(" webvtt ".to_owned()),
            user_id: Some("user-1".to_owned()),
        })
        .unwrap();

        assert_eq!(input.target_duration_seconds, 6);
        assert_eq!(input.manifest_subtitle_format.as_deref(), Some("vtt"));
        assert_eq!(
            empty_hls_subtitle_playlist(input.target_duration_seconds),
            "#EXTM3U\n#EXT-X-VERSION:3\n#EXT-X-TARGETDURATION:6\n#EXT-X-MEDIA-SEQUENCE:0\n#EXT-X-PLAYLIST-TYPE:VOD\n#EXT-X-ENDLIST\n"
        );

        assert!(
            hls_subtitle_playlist_input(&HlsSubtitlePlaylistQuery {
                subtitle_segment_length: Some(0),
                manifest_subtitles: Some("vtt".to_owned()),
                user_id: None,
            })
            .is_err()
        );
        assert!(
            hls_subtitle_playlist_input(&HlsSubtitlePlaylistQuery {
                subtitle_segment_length: Some(4),
                manifest_subtitles: Some("../vtt".to_owned()),
                user_id: None,
            })
            .is_err()
        );
    }

    #[test]
    fn external_subtitle_path_must_stay_under_media_directory() {
        let base_dir = unique_test_dir("fbz-subtitle-path-test");
        let media_dir = base_dir.join("media");
        let outside_dir = base_dir.join("outside");
        std_fs::create_dir_all(media_dir.join("subs")).unwrap();
        std_fs::create_dir_all(&outside_dir).unwrap();
        std_fs::write(media_dir.join("Movie.mkv"), b"movie").unwrap();
        std_fs::write(media_dir.join("subs").join("zh.srt"), b"1\n").unwrap();
        std_fs::write(outside_dir.join("evil.srt"), b"evil").unwrap();

        let subtitle = SubtitleStreamRecord {
            media_item_id: 1,
            item_id: "item-1".to_owned(),
            media_file_id: 2,
            media_path: media_dir.join("Movie.mkv").to_string_lossy().into_owned(),
            stream_index: 3,
            codec: Some("srt".to_owned()),
            language: Some("zh".to_owned()),
            is_external: true,
            extra: json!({"path": "subs/zh.srt"}),
        };
        let resolved = external_subtitle_path(&subtitle).unwrap().unwrap();

        assert_eq!(
            resolved,
            std_fs::canonicalize(media_dir.join("subs").join("zh.srt")).unwrap()
        );
        assert!(external_subtitle_matches_format(&subtitle, &resolved, "srt"));
        // 格式不一致 → 走 ffmpeg 转换缓存路径（不再直接拒绝）。
        assert!(!external_subtitle_matches_format(&subtitle, &resolved, "vtt"));

        let escaping = SubtitleStreamRecord {
            extra: json!({"path": "../outside/evil.srt"}),
            ..subtitle
        };

        assert!(external_subtitle_path(&escaping).is_err());

        std_fs::remove_dir_all(base_dir).unwrap();
    }

    #[test]
    fn empty_remote_subtitle_results_serializes_as_array() {
        let value = serde_json::to_value(empty_remote_subtitle_results("eng".to_owned())).unwrap();

        assert_eq!(value, json!([]));
    }

    #[test]
    fn remote_subtitle_info_uses_emby_acronym_field() {
        let value = serde_json::to_value(RemoteSubtitleInfoDto {
            three_letter_iso_language_name: "eng".to_owned(),
            id: "sub-1".to_owned(),
            provider_name: "provider".to_owned(),
            name: "English".to_owned(),
            format: "srt".to_owned(),
            author: String::new(),
            comment: String::new(),
            date_created: None,
            community_rating: None,
            download_count: 0,
            is_hash_match: false,
            is_forced: false,
            is_hearing_impaired: false,
            language: "en".to_owned(),
        })
        .unwrap();

        assert_eq!(value["ThreeLetterISOLanguageName"], "eng");
        assert_eq!(value["ProviderName"], "provider");
    }

    fn unique_test_dir(prefix: &str) -> PathBuf {
        std::env::temp_dir().join(format!(
            "{}-{}-{}",
            prefix,
            process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ))
    }
}
