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
    transcode::repository::{HlsTranscodeSessionRecord, TranscodeRepository},
};

use super::access::{access_token_from_request, authenticate_request_user};

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct HlsFileQuery {
    pub transcode_session_id: String,
    pub media_source_id: Option<String>,
}

pub async fn hls_file(
    State(state): State<AppState>,
    AxumPath((item_id, file_name)): AxumPath<(String, String)>,
    Query(query): Query<HlsFileQuery>,
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
    item_id: &str,
    file_name: &str,
    media_file_id: Option<i64>,
    session_id: &str,
    access_token: &str,
) -> String {
    let media_source = media_file_id
        .map(|id| format!("&MediaSourceId={id}"))
        .unwrap_or_default();

    format!(
        "/emby/videos/{item_id}/{file_name}?TranscodeSessionId={session_id}{media_source}&api_key={access_token}"
    )
}

fn media_source_id_as_i64(value: Option<&str>) -> Option<i64> {
    value.and_then(|value| value.parse::<i64>().ok())
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
    fn manifest_rewrite_adds_session_and_token_to_relative_segments() {
        let manifest = "#EXTM3U\n#EXTINF:4.0,\nmaster0.ts\nhttps://cdn.example/a.ts\n";

        let rewritten = rewrite_hls_manifest(manifest, "item-1", Some(42), "session-1", "token-1");

        assert!(rewritten.contains(
            "/emby/videos/item-1/master0.ts?TranscodeSessionId=session-1&MediaSourceId=42&api_key=token-1"
        ));
        assert!(rewritten.contains("https://cdn.example/a.ts"));
    }

    #[test]
    fn content_type_matches_hls_extensions() {
        assert_eq!(
            hls_content_type("master.m3u8"),
            "application/vnd.apple.mpegurl"
        );
        assert_eq!(hls_content_type("master0.ts"), "video/mp2t");
    }
}
