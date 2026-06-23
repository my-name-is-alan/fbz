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
    pub media_source_id: Option<String>,
    pub user_id: Option<String>,
    pub is_perfect_match: Option<bool>,
    pub is_forced: Option<bool>,
    pub is_hearing_impaired: Option<bool>,
}

#[derive(Clone, Debug, Default, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "PascalCase")]
pub struct SubtitleStreamQuery {
    pub start_position_ticks: Option<i64>,
    pub end_position_ticks: Option<i64>,
    pub copy_timestamps: Option<bool>,
    pub user_id: Option<String>,
}

const MAX_SUBTITLE_LANGUAGE_LEN: usize = 32;
const MAX_MEDIA_SOURCE_ID_LEN: usize = 256;
const MAX_SUBTITLE_FORMAT_LEN: usize = 16;

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

    let Some(path) = external_subtitle_path(&subtitle)? else {
        return Err(AppError::not_found(
            "subtitle stream is not an external subtitle file",
        ));
    };
    ensure_subtitle_format_is_streamable(&subtitle, &path, format)?;

    local_subtitle_response(&path, format).await
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

fn ensure_subtitle_format_is_streamable(
    subtitle: &SubtitleStreamRecord,
    path: &Path,
    requested_format: &str,
) -> Result<(), AppError> {
    let source_format = path
        .extension()
        .and_then(|value| value.to_str())
        .or(subtitle.codec.as_deref())
        .unwrap_or_default()
        .trim()
        .trim_start_matches('.')
        .to_ascii_lowercase();
    let source_format = normalize_subtitle_format(&source_format)?;
    if source_format != requested_format {
        return Err(AppError::unprocessable(
            "subtitle format conversion is not supported",
        ));
    }

    Ok(())
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

#[cfg(test)]
mod tests {
    use std::{
        fs as std_fs, process,
        time::{SystemTime, UNIX_EPOCH},
    };

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
        assert!(ensure_subtitle_format_is_streamable(&subtitle, &resolved, "srt").is_ok());
        assert!(ensure_subtitle_format_is_streamable(&subtitle, &resolved, "vtt").is_err());

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
