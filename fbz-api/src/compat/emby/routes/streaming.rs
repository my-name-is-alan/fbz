use std::{
    net::{IpAddr, Ipv4Addr, Ipv6Addr},
    path::Path,
    str::FromStr,
};

use axum::{
    body::Body,
    extract::{Path as AxumPath, Query, State},
    http::{
        HeaderMap, HeaderValue, StatusCode, Uri,
        header::{
            ACCEPT_RANGES, CACHE_CONTROL, CONTENT_DISPOSITION, CONTENT_LENGTH, CONTENT_RANGE,
            CONTENT_TYPE, LOCATION, RANGE,
        },
    },
    response::Response,
};
use serde::Deserialize;
use serde_json::{Value, json};
use tokio::{
    fs::File,
    io::{AsyncReadExt, AsyncSeekExt, SeekFrom},
};
use tokio_util::io::ReaderStream;
use tracing::warn;

use crate::{
    auth::{service::AuthenticatedUser, token::issue_access_token},
    config::MediaConfig,
    db::DbPool,
    error::AppError,
    media::repository::{MediaRepository, PlaybackMediaSourceRecord},
    plugins::hooks::{PluginHookDispatcher, PluginHookEvent},
    state::AppState,
    transcode::repository::{CreateTranscodeSessionInput, TranscodeRepository},
};

use super::access::{access_token_from_request, authenticate_request_user};

const MEDIA_DOWNLOAD_STARTED_EVENT: &str = "media.download.started";
const DEFAULT_UNIVERSAL_AUDIO_CODEC: &str = "aac";

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct DirectStreamQuery {
    pub media_source_id: Option<String>,
}

#[derive(Clone, Debug, Default, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct UniversalAudioQuery {
    pub user_id: Option<String>,
    pub device_id: Option<String>,
    pub media_source_id: Option<String>,
    pub max_streaming_bitrate: Option<i64>,
    pub audio_bit_rate: Option<i64>,
    pub container: Option<String>,
    pub max_sample_rate: Option<i32>,
    pub play_session_id: Option<String>,
    pub transcoding_protocol: Option<String>,
    pub transcoding_container: Option<String>,
    pub audio_codec: Option<String>,
    pub enable_redirection: Option<bool>,
    pub enable_remote_media: Option<bool>,
    pub start_time_ticks: Option<i64>,
}

pub async fn video_stream(
    State(state): State<AppState>,
    AxumPath(item_id): AxumPath<String>,
    Query(query): Query<DirectStreamQuery>,
    headers: HeaderMap,
    uri: Uri,
) -> Result<Response, AppError> {
    stream_source(state, item_id, query, headers, uri).await
}

pub async fn video_stream_container(
    State(state): State<AppState>,
    AxumPath((item_id, _container)): AxumPath<(String, String)>,
    Query(query): Query<DirectStreamQuery>,
    headers: HeaderMap,
    uri: Uri,
) -> Result<Response, AppError> {
    stream_source(state, item_id, query, headers, uri).await
}

pub async fn video_stream_file(
    State(state): State<AppState>,
    AxumPath((item_id, stream_file_name)): AxumPath<(String, String)>,
    Query(query): Query<DirectStreamQuery>,
    headers: HeaderMap,
    uri: Uri,
) -> Result<Response, AppError> {
    validate_stream_file_name(&stream_file_name)?;
    stream_source(state, item_id, query, headers, uri).await
}

pub async fn universal_audio_stream(
    State(state): State<AppState>,
    AxumPath(item_id): AxumPath<String>,
    Query(query): Query<UniversalAudioQuery>,
    headers: HeaderMap,
    uri: Uri,
) -> Result<Response, AppError> {
    let input = universal_audio_input(&query)?;
    if input.requests_hls_transcode() {
        return universal_audio_hls_stream(state, item_id, input, headers, uri).await;
    }

    stream_source(
        state,
        item_id,
        input.into_direct_stream_query(),
        headers,
        uri,
    )
    .await
}

pub async fn audio_stream(
    State(state): State<AppState>,
    AxumPath((item_id, stream_file_name)): AxumPath<(String, String)>,
    Query(query): Query<DirectStreamQuery>,
    headers: HeaderMap,
    uri: Uri,
) -> Result<Response, AppError> {
    validate_stream_file_name(&stream_file_name)?;
    stream_source(state, item_id, query, headers, uri).await
}

pub async fn item_download(
    State(state): State<AppState>,
    AxumPath(item_id): AxumPath<String>,
    Query(query): Query<DirectStreamQuery>,
    headers: HeaderMap,
    uri: Uri,
) -> Result<Response, AppError> {
    let user = authenticate_request_user(&state, &headers, &uri).await?;
    let Some(database) = state.database() else {
        return Err(AppError::internal("database is not configured"));
    };

    let source = MediaRepository::new(database.clone())
        .find_download_media_source(
            user.id,
            &item_id,
            media_source_id_as_i64(query.media_source_id.as_deref()),
        )
        .await
        .map_err(|err| AppError::internal(format!("failed to get download source: {err}")))?
        .ok_or_else(|| AppError::not_found("download source not found"))?;

    if source.is_strm {
        let response = strm_redirect_response(&state.config().media, &source)?;
        dispatch_download_hook(database, &user, &source, "strm_redirect").await;
        return Ok(response);
    }

    let mut response = local_file_response(&source, headers.get(RANGE)).await?;
    if let Ok(value) = HeaderValue::from_str(&download_content_disposition(&source)) {
        response.headers_mut().insert(CONTENT_DISPOSITION, value);
    }
    dispatch_download_hook(database, &user, &source, "file").await;
    Ok(response)
}

async fn stream_source(
    state: AppState,
    item_id: String,
    query: DirectStreamQuery,
    headers: HeaderMap,
    uri: Uri,
) -> Result<Response, AppError> {
    let user = authenticate_request_user(&state, &headers, &uri).await?;
    let Some(database) = state.database() else {
        return Err(AppError::internal("database is not configured"));
    };

    let source = MediaRepository::new(database.clone())
        .find_playback_media_source(
            user.id,
            &item_id,
            media_source_id_as_i64(query.media_source_id.as_deref()),
        )
        .await
        .map_err(|err| AppError::internal(format!("failed to get stream source: {err}")))?
        .ok_or_else(|| AppError::not_found("stream source not found"))?;

    stream_source_response(&state, &source, headers.get(RANGE)).await
}

async fn stream_source_response(
    state: &AppState,
    source: &PlaybackMediaSourceRecord,
    range_header: Option<&HeaderValue>,
) -> Result<Response, AppError> {
    if source.is_strm {
        return strm_redirect_response(&state.config().media, &source);
    }

    local_file_response(source, range_header).await
}

async fn universal_audio_hls_stream(
    state: AppState,
    item_id: String,
    input: UniversalAudioInput,
    headers: HeaderMap,
    uri: Uri,
) -> Result<Response, AppError> {
    let access_token = access_token_from_request(&headers, uri.query())?;
    let user = authenticate_request_user(&state, &headers, &uri).await?;
    assert_universal_audio_user(&user, input.user_id.as_deref())?;
    let Some(database) = state.database() else {
        return Err(AppError::internal("database is not configured"));
    };

    let source = MediaRepository::new(database.clone())
        .find_playback_media_source(
            user.id,
            &item_id,
            media_source_id_as_i64(input.media_source_id.as_deref()),
        )
        .await
        .map_err(|err| AppError::internal(format!("failed to get universal audio source: {err}")))?
        .ok_or_else(|| AppError::not_found("universal audio source not found"))?;

    let output_base_path = state
        .config()
        .storage
        .transcode_cache_dir
        .to_string_lossy()
        .into_owned();
    let play_session_id = input
        .play_session_id
        .clone()
        .unwrap_or_else(|| issue_access_token().token);
    let Some(session_input) = universal_audio_transcode_session_input(
        user.id,
        &source,
        &input,
        &output_base_path,
        &play_session_id,
    ) else {
        return stream_source_response(&state, &source, headers.get(RANGE)).await;
    };
    let session = TranscodeRepository::new(database.clone())
        .create_session(session_input)
        .await
        .map_err(|err| AppError::internal(format!("failed to queue universal audio hls: {err}")))?;

    redirect_response(&universal_audio_hls_manifest_location(
        &source,
        &session.id,
        &play_session_id,
        &access_token,
    ))
}

fn download_content_disposition(source: &PlaybackMediaSourceRecord) -> String {
    format!("attachment; filename=\"{}\"", download_file_name(source))
}

fn download_file_name(source: &PlaybackMediaSourceRecord) -> String {
    Path::new(source.path.trim())
        .file_name()
        .and_then(|name| name.to_str())
        .map(sanitize_download_file_name)
        .filter(|name| !name.is_empty())
        .unwrap_or_else(|| format!("{}.bin", source.item_id))
}

fn sanitize_download_file_name(value: &str) -> String {
    value
        .chars()
        .map(|ch| match ch {
            '"' | '\\' | '/' | '\r' | '\n' | '\t' => '_',
            ch if ch.is_control() => '_',
            ch if ch.is_ascii() => ch,
            _ => '_',
        })
        .collect()
}

async fn dispatch_download_hook(
    pool: &DbPool,
    user: &AuthenticatedUser,
    source: &PlaybackMediaSourceRecord,
    delivery_method: &'static str,
) {
    let event = download_hook_event(user, source, delivery_method);
    if let Err(err) = PluginHookDispatcher::new(pool.clone())
        .dispatch(event)
        .await
    {
        warn!(
            error = %err,
            event_key = MEDIA_DOWNLOAD_STARTED_EVENT,
            item_id = %source.item_id,
            user_id = %user.public_id,
            "failed to dispatch plugin download hooks"
        );
    }
}

fn download_hook_event(
    user: &AuthenticatedUser,
    source: &PlaybackMediaSourceRecord,
    delivery_method: &'static str,
) -> PluginHookEvent {
    PluginHookEvent {
        event_key: MEDIA_DOWNLOAD_STARTED_EVENT.to_owned(),
        aggregate_type: "media_item".to_owned(),
        aggregate_id: source.item_id.clone(),
        payload: download_hook_payload(user, source, delivery_method),
    }
}

fn download_hook_payload(
    user: &AuthenticatedUser,
    source: &PlaybackMediaSourceRecord,
    delivery_method: &'static str,
) -> Value {
    json!({
        "userId": &user.public_id,
        "username": &user.username,
        "itemId": &source.item_id,
        "itemType": &source.item_type,
        "mediaSourceId": source.media_file_id.to_string(),
        "deliveryMethod": delivery_method,
        "isStrm": source.is_strm,
        "container": source.container.as_deref(),
        "bitrate": source.bitrate,
    })
}

fn validate_stream_file_name(value: &str) -> Result<(), AppError> {
    let value = value.trim();
    if value.is_empty() {
        return Err(AppError::unprocessable("stream file name is required"));
    }
    if value.contains("..") || value.contains('/') || value.contains('\\') {
        return Err(AppError::unprocessable("stream file name is invalid"));
    }

    Ok(())
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct UniversalAudioInput {
    media_source_id: Option<String>,
    user_id: Option<String>,
    device_id: Option<String>,
    containers: Vec<String>,
    transcoding_protocol: Option<String>,
    transcoding_container: Option<String>,
    audio_codec: Option<String>,
    max_streaming_bitrate: Option<i64>,
    max_sample_rate: Option<i32>,
    play_session_id: Option<String>,
    enable_redirection: Option<bool>,
    enable_remote_media: Option<bool>,
    start_time_ticks: Option<i64>,
}

impl UniversalAudioInput {
    fn requests_hls_transcode(&self) -> bool {
        self.transcoding_protocol.as_deref() == Some("hls")
    }

    fn into_direct_stream_query(self) -> DirectStreamQuery {
        let Self {
            media_source_id,
            user_id: _,
            device_id: _,
            containers: _,
            transcoding_protocol: _,
            transcoding_container: _,
            audio_codec: _,
            max_streaming_bitrate: _,
            max_sample_rate: _,
            play_session_id: _,
            enable_redirection: _,
            enable_remote_media: _,
            start_time_ticks: _,
        } = self;

        DirectStreamQuery { media_source_id }
    }
}

fn assert_universal_audio_user(
    user: &AuthenticatedUser,
    requested_user_id: Option<&str>,
) -> Result<(), AppError> {
    if let Some(requested_user_id) = requested_user_id
        && requested_user_id != user.public_id
    {
        return Err(AppError::forbidden(
            "authenticated user does not match universal audio user",
        ));
    }

    Ok(())
}

fn universal_audio_transcode_session_input(
    user_id: i64,
    source: &PlaybackMediaSourceRecord,
    input: &UniversalAudioInput,
    output_base_path: &str,
    play_session_id: &str,
) -> Option<CreateTranscodeSessionInput> {
    if !source.supports_transcoding {
        return None;
    }
    let input_path = transcode_input_path(source)?;

    Some(CreateTranscodeSessionInput {
        user_id,
        media_item_id: source.media_item_id,
        media_file_id: Some(source.media_file_id),
        input_path,
        output_base_path: output_base_path.to_owned(),
        play_session_id: Some(play_session_id.to_owned()),
        device_id: input.device_id.clone(),
        video_codec: None,
        audio_codec: Some(
            input
                .audio_codec
                .clone()
                .unwrap_or_else(|| DEFAULT_UNIVERSAL_AUDIO_CODEC.to_owned()),
        ),
        container: Some("hls".to_owned()),
        bitrate: input
            .max_streaming_bitrate
            .map(|bitrate| bitrate.min(i64::from(i32::MAX)) as i32),
    })
}

fn transcode_input_path(source: &PlaybackMediaSourceRecord) -> Option<String> {
    let path = if source.is_strm {
        source.strm_target.as_deref().unwrap_or(&source.path)
    } else {
        &source.path
    };

    let trimmed = path.trim();
    (!trimmed.is_empty()).then(|| trimmed.to_owned())
}

fn universal_audio_hls_manifest_location(
    source: &PlaybackMediaSourceRecord,
    session_id: &str,
    play_session_id: &str,
    access_token: &str,
) -> String {
    format!(
        "/emby/Audio/{}/master.m3u8?MediaSourceId={}&TranscodeSessionId={}&PlaySessionId={}&api_key={}",
        source.item_id, source.media_file_id, session_id, play_session_id, access_token
    )
}

fn redirect_response(location: &str) -> Result<Response, AppError> {
    let location = HeaderValue::from_str(location)
        .map_err(|_| AppError::internal("universal audio hls location is invalid"))?;
    let mut response = Response::new(Body::empty());
    *response.status_mut() = StatusCode::FOUND;
    response.headers_mut().insert(LOCATION, location);
    response
        .headers_mut()
        .insert(CACHE_CONTROL, HeaderValue::from_static("no-store"));
    Ok(response)
}

fn universal_audio_input(query: &UniversalAudioQuery) -> Result<UniversalAudioInput, AppError> {
    Ok(UniversalAudioInput {
        media_source_id: normalize_optional_id(query.media_source_id.as_deref(), "MediaSourceId")?,
        user_id: normalize_optional_id(query.user_id.as_deref(), "UserId")?,
        device_id: normalize_optional_id(query.device_id.as_deref(), "DeviceId")?,
        containers: normalize_token_list(
            query.container.as_deref(),
            &[
                "aac", "ape", "flac", "m4a", "mp2", "mp3", "mpa", "oga", "ogg", "opus", "wav",
                "webm", "webma", "wma",
            ],
        ),
        transcoding_protocol: normalize_optional_token(
            query.transcoding_protocol.as_deref(),
            "TranscodingProtocol",
            &["hls"],
        )?,
        transcoding_container: normalize_optional_token(
            query.transcoding_container.as_deref(),
            "TranscodingContainer",
            &["aac", "mp3", "ts"],
        )?,
        audio_codec: normalize_optional_token(
            query.audio_codec.as_deref(),
            "AudioCodec",
            &["aac", "flac", "mp3", "opus", "vorbis", "wma"],
        )?,
        max_streaming_bitrate: universal_audio_bitrate(query)?,
        max_sample_rate: bounded_positive_i32(query.max_sample_rate, "MaxSampleRate", 768_000)?,
        play_session_id: normalize_optional_id(query.play_session_id.as_deref(), "PlaySessionId")?,
        enable_redirection: query.enable_redirection,
        enable_remote_media: query.enable_remote_media,
        start_time_ticks: bounded_non_negative_i64(
            query.start_time_ticks,
            "StartTimeTicks",
            3_153_600_000_000_000,
        )?,
    })
}

fn universal_audio_bitrate(query: &UniversalAudioQuery) -> Result<Option<i64>, AppError> {
    let max_streaming_bitrate = bounded_positive_i64(
        query.max_streaming_bitrate,
        "MaxStreamingBitrate",
        1_000_000_000,
    )?;
    let audio_bit_rate = bounded_positive_i64(query.audio_bit_rate, "AudioBitRate", 1_000_000_000)?;

    Ok(max_streaming_bitrate.or(audio_bit_rate))
}

fn normalize_optional_id(
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

fn normalize_token_list(value: Option<&str>, allowed: &[&str]) -> Vec<String> {
    let Some(value) = value else {
        return Vec::new();
    };

    let mut tokens = Vec::new();
    for raw in value.split(',') {
        let token = raw.trim().to_ascii_lowercase();
        if allowed.contains(&token.as_str()) && !tokens.contains(&token) {
            tokens.push(token);
        }
    }

    tokens
}

fn normalize_optional_token(
    value: Option<&str>,
    name: &'static str,
    allowed: &[&str],
) -> Result<Option<String>, AppError> {
    let Some(token) = value
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_ascii_lowercase)
    else {
        return Ok(None);
    };
    if !allowed.contains(&token.as_str()) {
        return Err(AppError::unprocessable(format!("{name} is invalid")));
    }

    Ok(Some(token))
}

fn bounded_positive_i64(
    value: Option<i64>,
    name: &'static str,
    max: i64,
) -> Result<Option<i64>, AppError> {
    let Some(value) = value else {
        return Ok(None);
    };
    if !(1..=max).contains(&value) {
        return Err(AppError::unprocessable(format!("{name} is out of range")));
    }

    Ok(Some(value))
}

fn bounded_positive_i32(
    value: Option<i32>,
    name: &'static str,
    max: i32,
) -> Result<Option<i32>, AppError> {
    let Some(value) = value else {
        return Ok(None);
    };
    if !(1..=max).contains(&value) {
        return Err(AppError::unprocessable(format!("{name} is out of range")));
    }

    Ok(Some(value))
}

fn bounded_non_negative_i64(
    value: Option<i64>,
    name: &'static str,
    max: i64,
) -> Result<Option<i64>, AppError> {
    let Some(value) = value else {
        return Ok(None);
    };
    if !(0..=max).contains(&value) {
        return Err(AppError::unprocessable(format!("{name} is out of range")));
    }

    Ok(Some(value))
}

fn strm_redirect_response(
    media_config: &MediaConfig,
    source: &PlaybackMediaSourceRecord,
) -> Result<Response, AppError> {
    let target = source.strm_target.as_deref().unwrap_or(&source.path).trim();

    if !is_allowed_strm_target(media_config, target) {
        return Err(AppError::forbidden("strm target is not allowed"));
    }

    let location = HeaderValue::from_str(target)
        .map_err(|_| AppError::unprocessable("strm target is not a valid redirect location"))?;
    let mut response = Response::new(Body::empty());
    *response.status_mut() = StatusCode::FOUND;
    response.headers_mut().insert(LOCATION, location);
    response
        .headers_mut()
        .insert(CACHE_CONTROL, HeaderValue::from_static("no-store"));
    Ok(response)
}

async fn local_file_response(
    source: &PlaybackMediaSourceRecord,
    range_header: Option<&HeaderValue>,
) -> Result<Response, AppError> {
    let path = Path::new(source.path.trim());
    let metadata = tokio::fs::metadata(path)
        .await
        .map_err(|err| stream_io_error(err, "stream source file not found"))?;
    if !metadata.is_file() {
        return Err(AppError::not_found("stream source is not a file"));
    }

    let total_len = metadata.len();
    let content_type = stream_content_type(source.container.as_deref(), path);
    let range = parse_range_header(range_header, total_len)?;
    let mut file = File::open(path)
        .await
        .map_err(|err| stream_io_error(err, "stream source file not found"))?;

    match range {
        Some(range) => {
            file.seek(SeekFrom::Start(range.start))
                .await
                .map_err(|err| AppError::internal(format!("failed to seek stream file: {err}")))?;
            let stream = ReaderStream::new(file.take(range.len()));
            Ok(stream_response(
                Body::from_stream(stream),
                StatusCode::PARTIAL_CONTENT,
                content_type,
                Some(range.len()),
                Some(format!("bytes {}-{}/{}", range.start, range.end, total_len)),
            ))
        }
        None => {
            let stream = ReaderStream::new(file);
            Ok(stream_response(
                Body::from_stream(stream),
                StatusCode::OK,
                content_type,
                Some(total_len),
                None,
            ))
        }
    }
}

fn stream_response(
    body: Body,
    status: StatusCode,
    content_type: &'static str,
    content_length: Option<u64>,
    content_range: Option<String>,
) -> Response {
    let mut response = Response::new(body);
    *response.status_mut() = status;
    response
        .headers_mut()
        .insert(CONTENT_TYPE, HeaderValue::from_static(content_type));
    response
        .headers_mut()
        .insert(ACCEPT_RANGES, HeaderValue::from_static("bytes"));
    response
        .headers_mut()
        .insert(CACHE_CONTROL, HeaderValue::from_static("no-store"));
    if let Some(content_length) = content_length
        && let Ok(value) = HeaderValue::from_str(&content_length.to_string())
    {
        response.headers_mut().insert(CONTENT_LENGTH, value);
    }
    if let Some(content_range) = content_range
        && let Ok(value) = HeaderValue::from_str(&content_range)
    {
        response.headers_mut().insert(CONTENT_RANGE, value);
    }

    response
}

fn parse_range_header(
    header: Option<&HeaderValue>,
    total_len: u64,
) -> Result<Option<ByteRange>, AppError> {
    let Some(header) = header else {
        return Ok(None);
    };
    if total_len == 0 {
        return Ok(None);
    }

    let value = header
        .to_str()
        .map_err(|_| AppError::unprocessable("invalid range header"))?
        .trim();
    let Some(range_value) = value.strip_prefix("bytes=") else {
        return Ok(None);
    };
    if range_value.contains(',') {
        return Err(AppError::unprocessable(
            "multiple byte ranges are not supported",
        ));
    }
    let (start, end) = range_value
        .split_once('-')
        .ok_or_else(|| AppError::unprocessable("invalid range header"))?;

    let start = start
        .trim()
        .parse::<u64>()
        .map_err(|_| AppError::unprocessable("invalid range start"))?;
    let end = if end.trim().is_empty() {
        total_len - 1
    } else {
        end.trim()
            .parse::<u64>()
            .map_err(|_| AppError::unprocessable("invalid range end"))?
    };

    if start > end || start >= total_len {
        return Err(AppError::unprocessable("range is outside stream length"));
    }

    Ok(Some(ByteRange {
        start,
        end: end.min(total_len - 1),
    }))
}

fn stream_io_error(error: std::io::Error, not_found_message: &'static str) -> AppError {
    if error.kind() == std::io::ErrorKind::NotFound {
        return AppError::not_found(not_found_message);
    }

    AppError::internal(format!("failed to read stream source: {error}"))
}

fn is_allowed_strm_target(media_config: &MediaConfig, target: &str) -> bool {
    let Ok(uri) = target.parse::<Uri>() else {
        return false;
    };
    if !matches!(uri.scheme_str(), Some("http" | "https")) {
        return false;
    }
    let Some(host) = uri
        .host()
        .map(|host| host.trim_matches(['[', ']']).to_ascii_lowercase())
    else {
        return false;
    };

    if host == "localhost" {
        return media_config.strm_allow_private_networks;
    }

    if let Ok(ip) = IpAddr::from_str(&host)
        && is_private_or_local_ip(ip)
    {
        return media_config.strm_allow_private_networks;
    }

    media_config
        .strm_allowed_domains
        .iter()
        .any(|domain| domain_matches(&host, domain))
}

fn domain_matches(host: &str, domain: &str) -> bool {
    let domain = domain.trim().trim_start_matches('.').to_ascii_lowercase();
    if domain.is_empty() {
        return false;
    }

    host == domain || host.ends_with(&format!(".{domain}"))
}

fn is_private_or_local_ip(ip: IpAddr) -> bool {
    match ip {
        IpAddr::V4(ip) => {
            ip.is_private() || ip.is_loopback() || ip.is_link_local() || ip == Ipv4Addr::UNSPECIFIED
        }
        IpAddr::V6(ip) => {
            ip.is_loopback()
                || ip.is_unspecified()
                || is_unique_local_ipv6(ip)
                || is_unicast_link_local_ipv6(ip)
        }
    }
}

fn is_unique_local_ipv6(ip: Ipv6Addr) -> bool {
    (ip.segments()[0] & 0xfe00) == 0xfc00
}

fn is_unicast_link_local_ipv6(ip: Ipv6Addr) -> bool {
    (ip.segments()[0] & 0xffc0) == 0xfe80
}

fn stream_content_type(container: Option<&str>, path: &Path) -> &'static str {
    let extension = container
        .filter(|value| !value.trim().is_empty())
        .or_else(|| path.extension().and_then(|value| value.to_str()))
        .unwrap_or_default()
        .trim()
        .trim_start_matches('.')
        .to_ascii_lowercase();

    match extension.as_str() {
        "mp4" | "m4v" => "video/mp4",
        "mkv" => "video/x-matroska",
        "webm" => "video/webm",
        "mov" => "video/quicktime",
        "avi" => "video/x-msvideo",
        "ts" => "video/mp2t",
        "mp3" => "audio/mpeg",
        "flac" => "audio/flac",
        "m4a" | "aac" => "audio/aac",
        _ => "application/octet-stream",
    }
}

fn media_source_id_as_i64(value: Option<&str>) -> Option<i64> {
    value.and_then(|value| value.parse::<i64>().ok())
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct ByteRange {
    start: u64,
    end: u64,
}

impl ByteRange {
    fn len(self) -> u64 {
        self.end - self.start + 1
    }
}

#[cfg(test)]
mod tests {
    use axum::http::HeaderValue;

    use crate::config::MediaConfig;

    use super::*;

    #[test]
    fn strm_allows_private_ips_only_when_enabled() {
        let mut config = media_config();
        config.strm_allow_private_networks = true;
        assert!(is_allowed_strm_target(
            &config,
            "http://192.168.1.20/movie.mkv"
        ));

        config.strm_allow_private_networks = false;
        assert!(!is_allowed_strm_target(
            &config,
            "http://192.168.1.20/movie.mkv"
        ));
    }

    #[test]
    fn strm_allows_configured_safe_domains_and_subdomains() {
        let mut config = media_config();
        config.strm_allow_private_networks = false;
        config.strm_allowed_domains = vec!["media.example.test".to_owned()];

        assert!(is_allowed_strm_target(
            &config,
            "https://media.example.test/a.mkv"
        ));
        assert!(is_allowed_strm_target(
            &config,
            "https://cdn.media.example.test/a.mkv"
        ));
        assert!(!is_allowed_strm_target(
            &config,
            "https://evil.example.test/a.mkv"
        ));
    }

    #[test]
    fn range_parser_supports_open_ended_single_range() {
        let range = parse_range_header(Some(&HeaderValue::from_static("bytes=10-")), 100)
            .unwrap()
            .unwrap();

        assert_eq!(range.start, 10);
        assert_eq!(range.end, 99);
        assert_eq!(range.len(), 90);
    }

    #[test]
    fn range_parser_rejects_multiple_ranges() {
        assert!(parse_range_header(Some(&HeaderValue::from_static("bytes=0-1,2-3")), 100).is_err());
    }

    #[test]
    fn stream_content_type_prefers_container() {
        assert_eq!(
            stream_content_type(Some("mkv"), Path::new("movie.bin")),
            "video/x-matroska"
        );
        assert_eq!(
            stream_content_type(None, Path::new("song.mp3")),
            "audio/mpeg"
        );
    }

    #[test]
    fn universal_audio_query_normalizes_safe_values() {
        let input = universal_audio_input(&UniversalAudioQuery {
            media_source_id: Some("42".to_owned()),
            container: Some(" mp3, AAC,../bad,flac,mp3, ".to_owned()),
            transcoding_protocol: Some(" HLS ".to_owned()),
            transcoding_container: Some(" TS ".to_owned()),
            audio_codec: Some(" AAC ".to_owned()),
            max_streaming_bitrate: Some(140_000_000),
            start_time_ticks: Some(123_000),
            ..UniversalAudioQuery::default()
        })
        .expect("safe universal audio query should normalize");

        assert_eq!(input.media_source_id.as_deref(), Some("42"));
        assert_eq!(input.containers, ["mp3", "aac", "flac"]);
        assert_eq!(input.transcoding_protocol.as_deref(), Some("hls"));
        assert_eq!(input.transcoding_container.as_deref(), Some("ts"));
        assert_eq!(input.audio_codec.as_deref(), Some("aac"));
        assert_eq!(input.max_streaming_bitrate, Some(140_000_000));
        assert_eq!(input.start_time_ticks, Some(123_000));
    }

    #[test]
    fn universal_audio_query_uses_audio_bit_rate_as_bitrate_fallback() {
        let audio_bit_rate_only = universal_audio_input(&UniversalAudioQuery {
            audio_bit_rate: Some(320_000),
            ..UniversalAudioQuery::default()
        })
        .expect("safe audio bit rate should normalize");

        assert_eq!(audio_bit_rate_only.max_streaming_bitrate, Some(320_000));

        let max_streaming_bitrate_wins = universal_audio_input(&UniversalAudioQuery {
            max_streaming_bitrate: Some(192_000),
            audio_bit_rate: Some(320_000),
            ..UniversalAudioQuery::default()
        })
        .expect("explicit max streaming bitrate should normalize");

        assert_eq!(
            max_streaming_bitrate_wins.max_streaming_bitrate,
            Some(192_000)
        );
    }

    #[test]
    fn universal_audio_query_rejects_unsafe_or_unbounded_values() {
        assert!(
            universal_audio_input(&UniversalAudioQuery {
                max_streaming_bitrate: Some(0),
                ..UniversalAudioQuery::default()
            })
            .is_err()
        );
        assert!(
            universal_audio_input(&UniversalAudioQuery {
                audio_bit_rate: Some(0),
                ..UniversalAudioQuery::default()
            })
            .is_err()
        );
        assert!(
            universal_audio_input(&UniversalAudioQuery {
                start_time_ticks: Some(-1),
                ..UniversalAudioQuery::default()
            })
            .is_err()
        );
        assert!(
            universal_audio_input(&UniversalAudioQuery {
                transcoding_protocol: Some("../hls".to_owned()),
                ..UniversalAudioQuery::default()
            })
            .is_err()
        );
    }

    #[test]
    fn universal_audio_hls_transcode_uses_audio_manifest_and_audio_only_session() {
        let source = PlaybackMediaSourceRecord {
            item_id: "track-1".to_owned(),
            item_type: "track".to_owned(),
            path: "song.flac".to_owned(),
            container: Some("flac".to_owned()),
            bitrate: Some(1_200_000),
            supports_transcoding: true,
            ..test_source()
        };
        let input = universal_audio_input(&UniversalAudioQuery {
            media_source_id: Some("2".to_owned()),
            transcoding_protocol: Some("hls".to_owned()),
            transcoding_container: Some("ts".to_owned()),
            audio_codec: Some("mp3".to_owned()),
            max_streaming_bitrate: Some(320_000),
            play_session_id: Some("play-1".to_owned()),
            ..UniversalAudioQuery::default()
        })
        .expect("hls universal audio query should normalize");

        assert!(input.requests_hls_transcode());

        let session = universal_audio_transcode_session_input(
            7,
            &source,
            &input,
            "./var/transcode",
            "play-1",
        )
        .expect("source should be transcodable");

        assert_eq!(session.user_id, 7);
        assert_eq!(session.media_item_id, 1);
        assert_eq!(session.media_file_id, Some(2));
        assert_eq!(session.input_path, "song.flac");
        assert_eq!(session.output_base_path, "./var/transcode");
        assert_eq!(session.play_session_id.as_deref(), Some("play-1"));
        assert_eq!(session.device_id.as_deref(), None);
        assert_eq!(session.video_codec, None);
        assert_eq!(session.audio_codec.as_deref(), Some("mp3"));
        assert_eq!(session.container.as_deref(), Some("hls"));
        assert_eq!(session.bitrate, Some(320_000));
        assert_eq!(
            universal_audio_hls_manifest_location(&source, "session-1", "play-1", "token-1"),
            "/emby/Audio/track-1/master.m3u8?MediaSourceId=2&TranscodeSessionId=session-1&PlaySessionId=play-1&api_key=token-1"
        );
    }

    #[test]
    fn download_file_name_rejects_header_unsafe_characters() {
        let source = PlaybackMediaSourceRecord {
            path: "D:/Media/Movie\r\nbad\"name.mkv".to_owned(),
            ..test_source()
        };

        assert_eq!(download_file_name(&source), "Movie__bad_name.mkv");
        assert_eq!(
            download_content_disposition(&source),
            "attachment; filename=\"Movie__bad_name.mkv\""
        );
    }

    #[test]
    fn download_hook_payload_exposes_safe_public_boundary() {
        let user = AuthenticatedUser {
            id: 10,
            public_id: "user-1".to_owned(),
            username: "alice".to_owned(),
            role_name: "User".to_owned(),
            role_name_normalized: "user".to_owned(),
        };
        let source = PlaybackMediaSourceRecord {
            path: "D:/Media/Private/Movie.mkv".to_owned(),
            is_strm: true,
            strm_target: Some("http://192.168.1.20/Movie.mkv".to_owned()),
            ..test_source()
        };

        let event = download_hook_event(&user, &source, "strm_redirect");

        assert_eq!(event.event_key, MEDIA_DOWNLOAD_STARTED_EVENT);
        assert_eq!(event.aggregate_type, "media_item");
        assert_eq!(event.aggregate_id, "item-1");
        assert_eq!(event.payload["userId"], "user-1");
        assert_eq!(event.payload["username"], "alice");
        assert_eq!(event.payload["mediaSourceId"], "2");
        assert_eq!(event.payload["deliveryMethod"], "strm_redirect");
        assert_eq!(event.payload["isStrm"], true);
        assert!(event.payload.get("path").is_none());
        assert!(event.payload.get("strmTarget").is_none());
    }

    fn media_config() -> MediaConfig {
        MediaConfig {
            roots: vec![],
            strm_allow_private_networks: true,
            strm_allowed_domains: vec![],
        }
    }

    fn test_source() -> PlaybackMediaSourceRecord {
        PlaybackMediaSourceRecord {
            media_item_id: 1,
            item_id: "item-1".to_owned(),
            item_type: "movie".to_owned(),
            media_file_id: 2,
            path: "D:/Media/Movie.mkv".to_owned(),
            file_size: Some(42_000_000),
            is_strm: false,
            strm_target: None,
            container: Some("mkv".to_owned()),
            runtime_ticks: Some(7_200_000_000),
            bitrate: Some(10_000_000),
            supports_transcoding: true,
            streams: Vec::new(),
        }
    }
}
