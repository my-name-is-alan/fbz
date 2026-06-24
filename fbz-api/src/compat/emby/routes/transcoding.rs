use std::{
    io::ErrorKind,
    path::{Path, PathBuf},
};

use axum::{
    body::Body,
    extract::{Path as AxumPath, Query, State},
    http::{
        HeaderMap, HeaderValue, StatusCode, Uri,
        header::{CACHE_CONTROL, CONTENT_TYPE},
    },
    response::Response,
};
use serde::Deserialize;
use tokio::fs;

use crate::{
    auth::service::AuthenticatedUser,
    error::AppError,
    state::AppState,
    transcode::{
        cleanup::cleanup_session_output_dir_best_effort,
        repository::{HlsTranscodeSessionRecord, TranscodeRepository},
    },
};

use super::{
    access::{access_token_from_request, authenticate_request_user},
    streaming,
};

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct HlsFileQuery {
    pub transcode_session_id: String,
    pub media_source_id: Option<String>,
}

#[derive(Clone, Debug, Default, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "PascalCase")]
pub struct VideoFileQuery {
    pub transcode_session_id: Option<String>,
    pub media_source_id: Option<String>,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct ActiveEncodingQuery {
    pub device_id: Option<String>,
    pub play_session_id: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct ActiveEncodingInput {
    device_id: Option<String>,
    play_session_id: String,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum HlsRouteKind {
    Video,
    Audio,
}

impl HlsRouteKind {
    fn route_segment(self) -> &'static str {
        match self {
            Self::Video => "Videos",
            Self::Audio => "Audio",
        }
    }
}

pub async fn video_file(
    State(state): State<AppState>,
    AxumPath((item_id, file_name)): AxumPath<(String, String)>,
    Query(query): Query<VideoFileQuery>,
    headers: HeaderMap,
    uri: Uri,
) -> Result<Response, AppError> {
    if let Some(transcode_session_id) = query.transcode_session_id {
        let file_name = validate_hls_file_name(&file_name)?.to_owned();
        return serve_hls_file(
            state,
            item_id,
            HlsRouteKind::Video,
            file_name,
            HlsFileQuery {
                transcode_session_id,
                media_source_id: query.media_source_id,
            },
            headers,
            uri,
        )
        .await;
    }

    streaming::video_stream_file(
        State(state),
        AxumPath((item_id, file_name)),
        Query(streaming::DirectStreamQuery {
            media_source_id: query.media_source_id,
        }),
        headers,
        uri,
    )
    .await
}

pub async fn hls_master_manifest(
    State(state): State<AppState>,
    AxumPath(item_id): AxumPath<String>,
    Query(query): Query<HlsFileQuery>,
    headers: HeaderMap,
    uri: Uri,
) -> Result<Response, AppError> {
    hls_named_manifest(
        state,
        item_id,
        HlsRouteKind::Video,
        "master",
        query,
        headers,
        uri,
    )
    .await
}

pub async fn hls_main_manifest(
    State(state): State<AppState>,
    AxumPath(item_id): AxumPath<String>,
    Query(query): Query<HlsFileQuery>,
    headers: HeaderMap,
    uri: Uri,
) -> Result<Response, AppError> {
    hls_named_manifest(
        state,
        item_id,
        HlsRouteKind::Video,
        "main",
        query,
        headers,
        uri,
    )
    .await
}

pub async fn hls_live_manifest(
    State(state): State<AppState>,
    AxumPath(item_id): AxumPath<String>,
    Query(query): Query<HlsFileQuery>,
    headers: HeaderMap,
    uri: Uri,
) -> Result<Response, AppError> {
    hls_named_manifest(
        state,
        item_id,
        HlsRouteKind::Video,
        "live",
        query,
        headers,
        uri,
    )
    .await
}

pub async fn audio_hls_master_manifest(
    State(state): State<AppState>,
    AxumPath(item_id): AxumPath<String>,
    Query(query): Query<HlsFileQuery>,
    headers: HeaderMap,
    uri: Uri,
) -> Result<Response, AppError> {
    hls_named_manifest(
        state,
        item_id,
        HlsRouteKind::Audio,
        "master",
        query,
        headers,
        uri,
    )
    .await
}

pub async fn audio_hls_main_manifest(
    State(state): State<AppState>,
    AxumPath(item_id): AxumPath<String>,
    Query(query): Query<HlsFileQuery>,
    headers: HeaderMap,
    uri: Uri,
) -> Result<Response, AppError> {
    hls_named_manifest(
        state,
        item_id,
        HlsRouteKind::Audio,
        "main",
        query,
        headers,
        uri,
    )
    .await
}

pub async fn audio_hls_live_manifest(
    State(state): State<AppState>,
    AxumPath(item_id): AxumPath<String>,
    Query(query): Query<HlsFileQuery>,
    headers: HeaderMap,
    uri: Uri,
) -> Result<Response, AppError> {
    hls_named_manifest(
        state,
        item_id,
        HlsRouteKind::Audio,
        "live",
        query,
        headers,
        uri,
    )
    .await
}

pub async fn hls_segment(
    State(state): State<AppState>,
    AxumPath((item_id, playlist_id, segment_file_name)): AxumPath<(String, String, String)>,
    Query(query): Query<HlsFileQuery>,
    headers: HeaderMap,
    uri: Uri,
) -> Result<Response, AppError> {
    let file_name = dynamic_hls_segment_file_name_from_path(&playlist_id, &segment_file_name)?;
    serve_hls_file(
        state,
        item_id,
        HlsRouteKind::Video,
        file_name,
        query,
        headers,
        uri,
    )
    .await
}

pub async fn audio_hls_segment(
    State(state): State<AppState>,
    AxumPath((item_id, playlist_id, segment_file_name)): AxumPath<(String, String, String)>,
    Query(query): Query<HlsFileQuery>,
    headers: HeaderMap,
    uri: Uri,
) -> Result<Response, AppError> {
    let file_name = dynamic_hls_segment_file_name_from_path(&playlist_id, &segment_file_name)?;
    serve_hls_file(
        state,
        item_id,
        HlsRouteKind::Audio,
        file_name,
        query,
        headers,
        uri,
    )
    .await
}

pub async fn delete_active_encodings(
    State(state): State<AppState>,
    Query(query): Query<ActiveEncodingQuery>,
    headers: HeaderMap,
    uri: Uri,
) -> Result<StatusCode, AppError> {
    let input = active_encoding_input(&query)?;
    let user = authenticate_request_user(&state, &headers, &uri).await?;
    let Some(database) = state.database() else {
        return Err(AppError::internal("database is not configured"));
    };

    if let Some(session) = TranscodeRepository::new(database.clone())
        .cancel_active_encoding(user.id, &input.play_session_id, input.device_id.as_deref())
        .await
        .map_err(|err| AppError::internal(format!("failed to cancel active encoding: {err}")))?
    {
        cleanup_session_output_dir_best_effort(
            &state.config().storage.transcode_cache_dir,
            &session.id,
            session.output_path.as_deref(),
            "active_encoding_cancel",
        )
        .await;
    }

    Ok(StatusCode::OK)
}

async fn hls_named_manifest(
    state: AppState,
    item_id: String,
    route_kind: HlsRouteKind,
    name: &'static str,
    query: HlsFileQuery,
    headers: HeaderMap,
    uri: Uri,
) -> Result<Response, AppError> {
    let file_name = dynamic_hls_manifest_file_name(name)?.to_owned();
    serve_hls_file(state, item_id, route_kind, file_name, query, headers, uri).await
}

async fn serve_hls_file(
    state: AppState,
    item_id: String,
    route_kind: HlsRouteKind,
    file_name: String,
    query: HlsFileQuery,
    headers: HeaderMap,
    uri: Uri,
) -> Result<Response, AppError> {
    let access_token = access_token_from_request(&headers, uri.query())?;
    let user = authenticate_request_user(&state, &headers, &uri).await?;
    let file_name = validate_hls_file_name(&file_name)?;
    let session = find_session(&state, &user, &item_id, &query).await?;
    let content_type = hls_content_type(file_name);
    let bytes = read_hls_file(&session, file_name).await?;

    if is_manifest_file(file_name) {
        let manifest = String::from_utf8(bytes)
            .map_err(|_| AppError::internal("hls manifest is not valid utf-8"))?;
        return Ok(hls_response(
            rewrite_hls_manifest(
                &manifest,
                route_kind,
                &session.item_id,
                session.media_file_id,
                &session.id,
                &access_token,
            )
            .into_bytes(),
            content_type,
        ));
    }

    Ok(hls_response(bytes, content_type))
}

async fn find_session(
    state: &AppState,
    user: &AuthenticatedUser,
    item_id: &str,
    query: &HlsFileQuery,
) -> Result<HlsTranscodeSessionRecord, AppError> {
    let Some(database) = state.database() else {
        return Err(AppError::internal("database is not configured"));
    };

    let session = TranscodeRepository::new(database.clone())
        .find_hls_session(
            user.id,
            item_id,
            &query.transcode_session_id,
            media_source_id_as_i64(query.media_source_id.as_deref()),
        )
        .await
        .map_err(|err| AppError::internal(format!("failed to get hls transcode session: {err}")))?
        .ok_or_else(|| AppError::not_found("hls transcode session not found"))?;

    if matches!(session.status.as_str(), "cancelled" | "failed") {
        return Err(AppError::conflict("hls transcode session is not playable"));
    }

    Ok(session)
}

async fn read_hls_file(
    session: &HlsTranscodeSessionRecord,
    file_name: &str,
) -> Result<Vec<u8>, AppError> {
    let output_path = session
        .output_path
        .as_deref()
        .ok_or_else(|| AppError::conflict("hls output path is not ready"))?;
    let output_dir = PathBuf::from(output_path);
    let target = if is_manifest_file(file_name) {
        PathBuf::from(
            session
                .manifest_path
                .as_deref()
                .ok_or_else(|| AppError::conflict("hls manifest path is not ready"))?,
        )
    } else {
        output_dir.join(file_name)
    };

    read_confined_file(&output_dir, &target, &session.status).await
}

async fn read_confined_file(
    output_dir: &Path,
    target: &Path,
    session_status: &str,
) -> Result<Vec<u8>, AppError> {
    let canonical_output = canonicalize_hls_path(output_dir, session_status).await?;
    let canonical_target = canonicalize_hls_path(target, session_status).await?;
    if !canonical_target.starts_with(&canonical_output) {
        return Err(AppError::forbidden(
            "hls output path escapes session directory",
        ));
    }

    fs::read(&canonical_target)
        .await
        .map_err(|err| hls_io_error(err, session_status))
}

async fn canonicalize_hls_path(path: &Path, session_status: &str) -> Result<PathBuf, AppError> {
    fs::canonicalize(path)
        .await
        .map_err(|err| hls_io_error(err, session_status))
}

fn hls_io_error(error: std::io::Error, session_status: &str) -> AppError {
    if error.kind() == ErrorKind::NotFound {
        if matches!(session_status, "queued" | "running") {
            return AppError::conflict("hls output is not ready");
        }

        return AppError::not_found("hls output file not found");
    }

    AppError::internal(format!("failed to read hls output: {error}"))
}

fn hls_response(content: Vec<u8>, content_type: &'static str) -> Response {
    let mut response = Response::new(Body::from(content));
    *response.status_mut() = StatusCode::OK;
    response
        .headers_mut()
        .insert(CONTENT_TYPE, HeaderValue::from_static(content_type));
    response
        .headers_mut()
        .insert(CACHE_CONTROL, HeaderValue::from_static("no-store"));
    response
}

fn validate_hls_file_name(file_name: &str) -> Result<&str, AppError> {
    let trimmed = file_name.trim();
    if trimmed.is_empty() {
        return Err(AppError::unprocessable("hls file name is required"));
    }
    if trimmed.contains('/') || trimmed.contains('\\') || trimmed.contains("..") {
        return Err(AppError::unprocessable("invalid hls file name"));
    }

    let lower = trimmed.to_ascii_lowercase();
    if !matches!(
        Path::new(&lower)
            .extension()
            .and_then(|extension| extension.to_str()),
        Some("m3u8" | "ts" | "m4s" | "aac" | "vtt")
    ) {
        return Err(AppError::unprocessable("unsupported hls file extension"));
    }

    Ok(trimmed)
}

fn dynamic_hls_manifest_file_name(name: &'static str) -> Result<&'static str, AppError> {
    match name {
        "master" => Ok("master.m3u8"),
        "main" => Ok("main.m3u8"),
        "live" => Ok("live.m3u8"),
        _ => Err(AppError::not_found("hls manifest route not found")),
    }
}

fn dynamic_hls_segment_file_name(
    playlist_id: &str,
    segment_id: &str,
    segment_container: &str,
) -> Result<String, AppError> {
    let playlist_id = validate_hls_route_segment(playlist_id, "hls playlist id")?;
    let segment_id = validate_hls_route_segment(segment_id, "hls segment id")?;
    let segment_container = normalize_hls_segment_container(segment_container)?;
    let file_name = format!("{playlist_id}{segment_id}.{segment_container}");
    validate_hls_file_name(&file_name)?;
    Ok(file_name)
}

fn dynamic_hls_segment_file_name_from_path(
    playlist_id: &str,
    segment_file_name: &str,
) -> Result<String, AppError> {
    let (segment_id, segment_container) = segment_file_name
        .trim()
        .rsplit_once('.')
        .ok_or_else(|| AppError::unprocessable("hls segment file name is invalid"))?;

    dynamic_hls_segment_file_name(playlist_id, segment_id, segment_container)
}

fn validate_hls_route_segment<'a>(
    value: &'a str,
    field: &'static str,
) -> Result<&'a str, AppError> {
    let value = value.trim();
    if value.is_empty() {
        return Err(AppError::unprocessable(format!("{field} is required")));
    }
    if value.len() > 64
        || !value
            .chars()
            .all(|ch| ch.is_ascii_alphanumeric() || matches!(ch, '_' | '-'))
    {
        return Err(AppError::unprocessable(format!("{field} is invalid")));
    }

    Ok(value)
}

fn normalize_hls_segment_container(value: &str) -> Result<String, AppError> {
    let value = value.trim().trim_start_matches('.').to_ascii_lowercase();
    if !matches!(value.as_str(), "ts" | "m4s" | "aac" | "vtt") {
        return Err(AppError::unprocessable(
            "hls segment container is unsupported",
        ));
    }

    Ok(value)
}

fn hls_content_type(file_name: &str) -> &'static str {
    let lower = file_name.to_ascii_lowercase();
    match Path::new(&lower)
        .extension()
        .and_then(|extension| extension.to_str())
    {
        Some("m3u8") => "application/vnd.apple.mpegurl",
        Some("ts") => "video/mp2t",
        Some("m4s") => "video/iso.segment",
        Some("aac") => "audio/aac",
        Some("vtt") => "text/vtt; charset=utf-8",
        _ => "application/octet-stream",
    }
}

fn is_manifest_file(file_name: &str) -> bool {
    file_name.to_ascii_lowercase().ends_with(".m3u8")
}

fn rewrite_hls_manifest(
    manifest: &str,
    route_kind: HlsRouteKind,
    item_id: &str,
    media_file_id: Option<i64>,
    session_id: &str,
    access_token: &str,
) -> String {
    let mut rewritten = String::with_capacity(manifest.len() + 128);

    for line in manifest.lines() {
        let trimmed = line.trim();
        if should_rewrite_manifest_uri(trimmed) {
            rewritten.push_str(&hls_file_url(
                route_kind,
                item_id,
                trimmed,
                media_file_id,
                session_id,
                access_token,
            ));
        } else {
            rewritten.push_str(line);
        }
        rewritten.push('\n');
    }

    rewritten
}

fn should_rewrite_manifest_uri(value: &str) -> bool {
    if value.is_empty()
        || value.starts_with('#')
        || value.starts_with('/')
        || value.contains("://")
        || value.contains('?')
    {
        return false;
    }

    validate_hls_file_name(value).is_ok()
}

fn hls_file_url(
    route_kind: HlsRouteKind,
    item_id: &str,
    file_name: &str,
    media_file_id: Option<i64>,
    session_id: &str,
    access_token: &str,
) -> String {
    if let Some(parts) = hls_segment_route_parts(file_name) {
        return hls_segment_url(
            route_kind,
            item_id,
            &parts,
            media_file_id,
            session_id,
            access_token,
        );
    }

    let media_source = media_file_id
        .map(|id| format!("&MediaSourceId={id}"))
        .unwrap_or_default();

    format!(
        "/emby/Videos/{item_id}/{file_name}?TranscodeSessionId={session_id}{media_source}&api_key={access_token}"
    )
}

fn hls_segment_url(
    route_kind: HlsRouteKind,
    item_id: &str,
    parts: &HlsSegmentRouteParts,
    media_file_id: Option<i64>,
    session_id: &str,
    access_token: &str,
) -> String {
    let media_source = media_file_id
        .map(|id| format!("&MediaSourceId={id}"))
        .unwrap_or_default();

    format!(
        "/emby/{}/{item_id}/hls1/{}/{}.{}?TranscodeSessionId={session_id}{media_source}&api_key={access_token}",
        route_kind.route_segment(),
        parts.playlist_id,
        parts.segment_id,
        parts.segment_container
    )
}

fn hls_segment_route_parts(file_name: &str) -> Option<HlsSegmentRouteParts> {
    let file_name = validate_hls_file_name(file_name).ok()?;
    let path = Path::new(file_name);
    let segment_container = path
        .extension()
        .and_then(|value| value.to_str())
        .map(str::to_ascii_lowercase)
        .filter(|value| matches!(value.as_str(), "ts" | "m4s" | "aac" | "vtt"))?;
    let stem = path.file_stem()?.to_str()?.trim();
    if !stem.is_ascii() {
        return None;
    }

    let digit_count = stem
        .chars()
        .rev()
        .take_while(|ch| ch.is_ascii_digit())
        .count();
    if digit_count == 0 || digit_count == stem.len() {
        return None;
    }
    let split_at = stem.len() - digit_count;
    let (playlist_id, segment_id) = stem.split_at(split_at);
    validate_hls_route_segment(playlist_id, "hls playlist id").ok()?;
    validate_hls_route_segment(segment_id, "hls segment id").ok()?;

    Some(HlsSegmentRouteParts {
        playlist_id: playlist_id.to_owned(),
        segment_id: segment_id.to_owned(),
        segment_container,
    })
}

struct HlsSegmentRouteParts {
    playlist_id: String,
    segment_id: String,
    segment_container: String,
}

fn media_source_id_as_i64(value: Option<&str>) -> Option<i64> {
    value.and_then(|value| value.parse::<i64>().ok())
}

fn active_encoding_input(query: &ActiveEncodingQuery) -> Result<ActiveEncodingInput, AppError> {
    let play_session_id =
        normalize_required_active_encoding_id(query.play_session_id.as_deref(), "PlaySessionId")?;
    let device_id = normalize_optional_active_encoding_id(query.device_id.as_deref(), "DeviceId")?;

    Ok(ActiveEncodingInput {
        device_id,
        play_session_id,
    })
}

fn normalize_required_active_encoding_id(
    value: Option<&str>,
    name: &'static str,
) -> Result<String, AppError> {
    normalize_optional_active_encoding_id(value, name)?
        .ok_or_else(|| AppError::unprocessable(format!("{name} is required")))
}

fn normalize_optional_active_encoding_id(
    value: Option<&str>,
    name: &'static str,
) -> Result<Option<String>, AppError> {
    let Some(value) = value.map(str::trim).filter(|value| !value.is_empty()) else {
        return Ok(None);
    };
    if value.len() > 128
        || !value
            .chars()
            .all(|ch| ch.is_ascii_alphanumeric() || matches!(ch, '-' | '_' | '.' | ':'))
    {
        return Err(AppError::unprocessable(format!("{name} is invalid")));
    }

    Ok(Some(value.to_owned()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hls_file_name_validation_blocks_traversal() {
        assert!(validate_hls_file_name("master.m3u8").is_ok());
        assert!(validate_hls_file_name("master0.ts").is_ok());
        assert!(validate_hls_file_name("../secret.ts").is_err());
        assert!(validate_hls_file_name("nested/segment.ts").is_err());
        assert!(validate_hls_file_name("segment.exe").is_err());
    }

    #[test]
    fn video_file_query_accepts_hls_and_stream_media_source_parameters() {
        let hls_query = serde_json::from_value::<VideoFileQuery>(serde_json::json!({
            "TranscodeSessionId": "session-1",
            "MediaSourceId": "42"
        }))
        .unwrap();

        assert_eq!(hls_query.transcode_session_id.as_deref(), Some("session-1"));
        assert_eq!(hls_query.media_source_id.as_deref(), Some("42"));

        let stream_query =
            serde_json::from_value::<VideoFileQuery>(serde_json::json!({ "MediaSourceId": "42" }))
                .unwrap();

        assert_eq!(stream_query.transcode_session_id, None);
        assert_eq!(stream_query.media_source_id.as_deref(), Some("42"));
    }

    #[test]
    fn dynamic_hls_named_manifest_routes_are_limited_to_known_names() {
        assert_eq!(
            dynamic_hls_manifest_file_name("master").unwrap(),
            "master.m3u8"
        );
        assert_eq!(dynamic_hls_manifest_file_name("main").unwrap(), "main.m3u8");
        assert_eq!(dynamic_hls_manifest_file_name("live").unwrap(), "live.m3u8");
        assert!(dynamic_hls_manifest_file_name("segment").is_err());
    }

    #[test]
    fn dynamic_hls_segment_path_maps_to_safe_local_segment_file() {
        assert_eq!(
            dynamic_hls_segment_file_name("master", "0", "ts").unwrap(),
            "master0.ts"
        );
        assert_eq!(
            dynamic_hls_segment_file_name("audio-1", "42", "aac").unwrap(),
            "audio-142.aac"
        );
        assert!(dynamic_hls_segment_file_name("../master", "0", "ts").is_err());
        assert!(dynamic_hls_segment_file_name("master", "../0", "ts").is_err());
        assert!(dynamic_hls_segment_file_name("master", "0", "exe").is_err());
    }

    #[test]
    fn manifest_rewrite_prefers_official_hls1_segment_routes() {
        let manifest = "#EXTM3U\n#EXTINF:4.0,\nmaster0.ts\nsegment-custom.ts\n";

        let rewritten = rewrite_hls_manifest(
            manifest,
            HlsRouteKind::Video,
            "item-1",
            Some(42),
            "session-1",
            "token-1",
        );

        assert!(rewritten.contains(
            "/emby/Videos/item-1/hls1/master/0.ts?TranscodeSessionId=session-1&MediaSourceId=42&api_key=token-1"
        ));
        assert!(rewritten.contains(
            "/emby/Videos/item-1/segment-custom.ts?TranscodeSessionId=session-1&MediaSourceId=42&api_key=token-1"
        ));
    }

    #[test]
    fn manifest_rewrite_adds_session_and_token_to_relative_segments() {
        let manifest = "#EXTM3U\n#EXTINF:4.0,\nmaster0.ts\nhttps://cdn.example/a.ts\n";

        let rewritten = rewrite_hls_manifest(
            manifest,
            HlsRouteKind::Video,
            "item-1",
            Some(42),
            "session-1",
            "token-1",
        );

        assert!(rewritten.contains(
            "/emby/Videos/item-1/hls1/master/0.ts?TranscodeSessionId=session-1&MediaSourceId=42&api_key=token-1"
        ));
        assert!(rewritten.contains("https://cdn.example/a.ts"));
    }

    #[test]
    fn audio_manifest_rewrite_uses_official_audio_hls1_segment_routes() {
        let manifest = "#EXTM3U\n#EXTINF:4.0,\nmaster0.ts\nmain0.aac\n";

        let rewritten = rewrite_hls_manifest(
            manifest,
            HlsRouteKind::Audio,
            "track-1",
            Some(42),
            "session-1",
            "token-1",
        );

        assert!(rewritten.contains(
            "/emby/Audio/track-1/hls1/master/0.ts?TranscodeSessionId=session-1&MediaSourceId=42&api_key=token-1"
        ));
        assert!(rewritten.contains(
            "/emby/Audio/track-1/hls1/main/0.aac?TranscodeSessionId=session-1&MediaSourceId=42&api_key=token-1"
        ));
        assert!(!rewritten.contains("/emby/Videos/track-1/hls1/"));
    }

    #[test]
    fn content_type_matches_hls_extensions() {
        assert_eq!(
            hls_content_type("master.m3u8"),
            "application/vnd.apple.mpegurl"
        );
        assert_eq!(hls_content_type("master0.ts"), "video/mp2t");
    }

    #[test]
    fn active_encoding_query_normalizes_client_ids() {
        let input = active_encoding_input(&ActiveEncodingQuery {
            device_id: Some(" device-1 ".to_owned()),
            play_session_id: Some(" play:session.1 ".to_owned()),
        })
        .expect("safe active encoding query should normalize");

        assert_eq!(input.device_id.as_deref(), Some("device-1"));
        assert_eq!(input.play_session_id, "play:session.1");

        assert!(
            active_encoding_input(&ActiveEncodingQuery {
                device_id: Some("../device".to_owned()),
                play_session_id: Some("play-1".to_owned()),
            })
            .is_err()
        );
        assert!(
            active_encoding_input(&ActiveEncodingQuery {
                device_id: Some("device-1".to_owned()),
                play_session_id: None,
            })
            .is_err()
        );
    }

    #[test]
    fn active_encoding_cancel_attempts_output_cleanup() {
        let source = include_str!("transcoding.rs");

        assert!(source.contains("cleanup_session_output_dir_best_effort"));
        assert!(source.contains("state.config().storage.transcode_cache_dir"));
        assert!(source.contains("\"active_encoding_cancel\""));
    }
}
