//! Jellyfin Trickplay 兼容路由：
//! - `GET /Videos/{id}/Trickplay/{width}/tiles.m3u8`
//! - `GET /Videos/{id}/Trickplay/{width}/{index}.jpg`
//! - `GET /Videos/{id}/Trickplay`（fbz 扩展：已生成宽度列表，C 端进度条用）
//! - `POST /Videos/{id}/Trickplay/Generate?Width=`（管理员；后台 ffmpeg 任务，202）

use axum::{
    Json,
    body::Body,
    extract::{Path, Query, State},
    http::{HeaderMap, HeaderValue, StatusCode, Uri, header::{CACHE_CONTROL, CONTENT_TYPE}},
    response::Response,
};
use serde::Deserialize;

use crate::{
    error::AppError,
    library::repository::LibraryRepository,
    media::{repository::MediaRepository, trickplay::{self, TrickplayInfo}},
    state::AppState,
};

use super::access::authenticate_request_user;

#[derive(Clone, Debug, Default, Deserialize)]
pub struct GenerateQuery {
    #[serde(alias = "width", alias = "Width")]
    pub width: Option<u32>,
}

pub(super) fn cache_root(state: &AppState) -> std::path::PathBuf {
    state.config().storage.artwork_cache_dir.clone()
}

fn parse_width(raw: &str) -> Result<u32, AppError> {
    raw.parse::<u32>()
        .ok()
        .filter(|w| trickplay::ALLOWED_WIDTHS.contains(w))
        .ok_or_else(|| AppError::unprocessable("unsupported trickplay width"))
}

fn parse_index(raw: &str) -> Result<u32, AppError> {
    raw.strip_suffix(".jpg")
        .and_then(|v| v.parse::<u32>().ok())
        .ok_or_else(|| AppError::not_found("trickplay tile not found"))
}

async fn ensure_access(state: &AppState, headers: &HeaderMap, uri: &Uri, item_id: &str) -> Result<i64, AppError> {
    let user = authenticate_request_user(state, headers, uri).await?;
    let Some(database) = state.database() else {
        return Err(AppError::internal("database is not configured"));
    };
    LibraryRepository::new(database.clone())
        .find_user_item_by_id(user.id, item_id)
        .await
        .map_err(|err| AppError::internal(format!("failed to get media item: {err}")))?
        .ok_or_else(|| AppError::not_found("item not found"))?;
    Ok(user.id)
}

async fn manifest(state: &AppState, item_id: &str, width: u32) -> Result<TrickplayInfo, AppError> {
    trickplay::read_manifest(&trickplay::width_dir(&cache_root(state), item_id, width))
        .await
        .ok_or_else(|| AppError::not_found("trickplay not generated"))
}

/// 去掉 query 中非 api_key 的参数，供 m3u8 子资源复用鉴权。
fn auth_query(uri: &Uri) -> String {
    uri.query()
        .unwrap_or_default()
        .split('&')
        .filter(|kv| kv.to_ascii_lowercase().starts_with("api_key="))
        .collect::<Vec<_>>()
        .join("&")
}

pub async fn list(
    State(state): State<AppState>,
    Path(item_id): Path<String>,
    headers: HeaderMap,
    uri: Uri,
) -> Result<Json<Vec<TrickplayInfo>>, AppError> {
    ensure_access(&state, &headers, &uri, &item_id).await?;
    Ok(Json(trickplay::available(&cache_root(&state), &item_id).await))
}

pub async fn playlist(
    State(state): State<AppState>,
    Path((item_id, width)): Path<(String, String)>,
    headers: HeaderMap,
    uri: Uri,
) -> Result<Response, AppError> {
    let width = parse_width(&width)?;
    ensure_access(&state, &headers, &uri, &item_id).await?;
    let info = manifest(&state, &item_id, width).await?;
    let mut response = Response::new(Body::from(trickplay::m3u8(&info, &auth_query(&uri))));
    response.headers_mut().insert(CONTENT_TYPE, HeaderValue::from_static("application/x-mpegURL"));
    Ok(response)
}

pub async fn tile(
    State(state): State<AppState>,
    Path((item_id, width, index)): Path<(String, String, String)>,
    headers: HeaderMap,
    uri: Uri,
) -> Result<Response, AppError> {
    let width = parse_width(&width)?;
    let index = parse_index(&index)?;
    ensure_access(&state, &headers, &uri, &item_id).await?;
    let path = trickplay::width_dir(&cache_root(&state), &item_id, width).join(format!("{index}.jpg"));
    let bytes = tokio::fs::read(&path).await.map_err(|_| AppError::not_found("trickplay tile not found"))?;
    let mut response = Response::new(Body::from(bytes));
    response.headers_mut().insert(CONTENT_TYPE, HeaderValue::from_static("image/jpeg"));
    response.headers_mut().insert(CACHE_CONTROL, HeaderValue::from_static("public, max-age=86400"));
    Ok(response)
}

pub async fn generate(
    State(state): State<AppState>,
    Path(item_id): Path<String>,
    Query(query): Query<GenerateQuery>,
    headers: HeaderMap,
    uri: Uri,
) -> Result<StatusCode, AppError> {
    let user = authenticate_request_user(&state, &headers, &uri).await?;
    if !user.can_manage_server() {
        return Err(AppError::forbidden("server management permission required"));
    }
    let width = query.width.unwrap_or(trickplay::DEFAULT_WIDTH);
    if !trickplay::ALLOWED_WIDTHS.contains(&width) {
        return Err(AppError::unprocessable("unsupported trickplay width"));
    }
    let Some(database) = state.database() else {
        return Err(AppError::internal("database is not configured"));
    };
    let source = MediaRepository::new(database.clone())
        .find_playback_media_source(user.id, &item_id, None)
        .await
        .map_err(|err| AppError::internal(format!("failed to get media source: {err}")))?
        .ok_or_else(|| AppError::not_found("item not found"))?;
    if source.is_strm {
        return Err(AppError::unprocessable("trickplay is not supported for STRM sources"));
    }
    let video = source.streams.iter().find(|s| s.stream_type.eq_ignore_ascii_case("video"));
    let info = trickplay::plan(width, video.and_then(|v| v.width), video.and_then(|v| v.height), source.runtime_ticks)
        .ok_or_else(|| AppError::unprocessable("media runtime unknown; probe the item first"))?;
    let ffmpeg = crate::media::tools::resolve_media_tools(&state.config().media_tools)
        .map(|tools| tools.ffmpeg.path)
        .map_err(|err| AppError::internal(format!("ffmpeg is not available: {err}")))?;
    let root = cache_root(&state);
    let media_path = std::path::PathBuf::from(source.path.trim());
    trickplay::set_status(&item_id, width, "Queued", None);
    tokio::spawn(async move {
        match trickplay::generate_tracked(&ffmpeg, &media_path, &info, &root, &item_id).await {
            Ok(()) => tracing::info!(item_id = %item_id, width, "trickplay generated"),
            Err(err) => tracing::warn!(item_id = %item_id, width, error = %err, "trickplay generation failed"),
        }
    });
    Ok(StatusCode::ACCEPTED)
}

/// `GET /Videos/{id}/Trickplay/Status`（fbz 扩展）：生成进度。无记录且已生成 → Completed；都无 → NotStarted。
pub async fn status(
    State(state): State<AppState>,
    Path(item_id): Path<String>,
    headers: HeaderMap,
    uri: Uri,
) -> Result<Json<serde_json::Value>, AppError> {
    ensure_access(&state, &headers, &uri, &item_id).await?;
    if let Some(status) = trickplay::status(&item_id) {
        return Ok(Json(serde_json::to_value(status).unwrap_or_default()));
    }
    let available = trickplay::available(&cache_root(&state), &item_id).await;
    Ok(Json(serde_json::json!({
        "State": if available.is_empty() { "NotStarted" } else { "Completed" },
        "Widths": available.iter().map(|i| i.width).collect::<Vec<_>>(),
    })))
}

/// item 详情附加 JF `Trickplay` 字段（有已生成产物时）。
pub async fn attach_item_field(state: &AppState, dto: crate::compat::emby::dto::BaseItemDto) -> Json<serde_json::Value> {
    let infos = trickplay::available(&cache_root(state), &dto.id).await;
    let mut ids: Vec<String> = dto.media_sources.iter().map(|m| m.id.clone()).collect();
    if ids.is_empty() {
        ids.push(dto.id.replace('-', ""));
    }
    let mut value = serde_json::to_value(&dto).unwrap_or_default();
    if let (Some(field), Some(obj)) = (trickplay::item_field(&ids, &infos), value.as_object_mut()) {
        obj.insert("Trickplay".to_owned(), field);
    }
    Json(value)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn width_and_index_parsing() {
        assert_eq!(parse_width("320").unwrap(), 320);
        assert!(parse_width("321").is_err());
        assert_eq!(parse_index("12.jpg").unwrap(), 12);
        assert!(parse_index("x.jpg").is_err());
        assert!(parse_index("../1.jpg").is_err());
    }

    #[test]
    fn auth_query_keeps_only_api_key() {
        let uri: Uri = "/x?foo=1&api_key=abc&MediaSourceId=2".parse().unwrap();
        assert_eq!(auth_query(&uri), "api_key=abc");
        assert_eq!(auth_query(&"/x".parse().unwrap()), "");
    }
}
