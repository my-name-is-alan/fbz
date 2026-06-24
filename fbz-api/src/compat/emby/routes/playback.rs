use axum::{
    Json,
    body::{Body, Bytes},
    extract::{Path, Query, State},
    http::{
        HeaderMap, HeaderValue, StatusCode, Uri,
        header::{CACHE_CONTROL, CONTENT_LENGTH, CONTENT_TYPE},
    },
    response::Response,
};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use tokio::io::{self, AsyncReadExt};
use tokio_util::io::ReaderStream;
use tracing::warn;

use crate::{
    auth::{service::AuthenticatedUser, token::issue_access_token},
    compat::emby::dto::{
        MediaSourceDto, MediaStreamDto, PlaybackInfoRequestDto, PlaybackInfoResponseDto,
        PlaybackProgressDto,
    },
    compat::emby::payload::parse_emby_body,
    db::DbPool,
    error::AppError,
    media::repository::{
        MediaRepository, PlaybackMediaSourceRecord, PlaybackMediaStreamRecord, PlaybackReportInput,
    },
    plugins::hooks::{PluginHookDispatcher, PluginHookEvent},
    state::AppState,
    transcode::repository::{CreateTranscodeSessionInput, TranscodeRepository},
};

use super::access::{
    access_token_from_request, authenticate_request_user, authenticate_route_user,
    client_device_id_from_request,
};

const PLAYBACK_STARTED_EVENT: &str = "playback.started";
const PLAYBACK_STOPPED_EVENT: &str = "playback.stopped";
const MAX_BITRATE_TEST_SIZE: usize = 64 * 1024 * 1024;
const MAX_PLAY_SESSION_ID_LEN: usize = 256;

#[derive(Clone, Copy, Debug, Default, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "PascalCase")]
pub(super) struct BitrateTestQuery {
    pub size: Option<usize>,
}

#[derive(Clone, Debug, Default, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "PascalCase")]
pub(super) struct PlaybackPingQuery {
    pub play_session_id: Option<String>,
}

#[derive(Clone, Debug, Default, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "PascalCase")]
pub(super) struct LiveStreamQuery {
    pub live_stream_id: Option<String>,
}

#[derive(Clone, Debug, Default, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "PascalCase")]
pub(super) struct LiveStreamRequestDto {
    pub open_token: Option<String>,
    pub user_id: Option<String>,
    pub play_session_id: Option<String>,
}

#[derive(Clone, Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "PascalCase")]
pub struct LiveStreamResponseDto {
    pub media_source: Option<MediaSourceDto>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
struct PlaybackPingInput {
    play_session_id: Option<String>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
struct LiveStreamOpenInput {
    open_token: Option<String>,
    user_id: Option<String>,
    play_session_id: Option<String>,
}

#[derive(Clone, Debug, Default, Deserialize, PartialEq)]
#[serde(rename_all = "PascalCase")]
pub(super) struct PlaybackProgressPathDto {
    pub item_id: Option<String>,
    pub user_id: Option<String>,
    pub session_id: Option<String>,
    pub play_session_id: Option<String>,
    pub media_source_id: Option<String>,
    pub play_method: Option<String>,
    #[serde(default)]
    pub queueable_media_types: Vec<String>,
    pub can_seek: Option<bool>,
    pub event_name: Option<String>,
    pub audio_stream_index: Option<i32>,
    pub subtitle_stream_index: Option<i32>,
    pub position_ticks: Option<i64>,
    pub is_paused: Option<bool>,
    pub is_muted: Option<bool>,
    pub volume_level: Option<i32>,
    pub live_stream_id: Option<String>,
    pub playlist_index: Option<i32>,
    pub playlist_length: Option<i32>,
    pub subtitle_offset: Option<f64>,
    pub playback_rate: Option<f64>,
    #[serde(default)]
    pub now_playing_queue: Vec<Value>,
    pub playlist_item_id: Option<String>,
    #[serde(default)]
    pub playlist_item_ids: Vec<String>,
    #[serde(rename = "RunTimeTicks")]
    pub runtime_ticks: Option<i64>,
    pub playback_start_time_ticks: Option<i64>,
    pub brightness: Option<i32>,
    pub aspect_ratio: Option<String>,
    pub repeat_mode: Option<String>,
    pub sleep_timer_mode: Option<String>,
    pub sleep_timer_end_time: Option<String>,
    pub shuffle: Option<bool>,
}

pub async fn playback_info(
    State(state): State<AppState>,
    Path(item_id): Path<String>,
    Query(query): Query<PlaybackInfoRequestDto>,
    headers: HeaderMap,
    uri: Uri,
) -> Result<Json<PlaybackInfoResponseDto>, AppError> {
    let access_token = access_token_from_request(&headers, uri.query())?;
    let device_id = client_device_id_from_request(&headers, uri.query())?;
    let user = authenticate_request_user(&state, &headers, &uri).await?;
    assert_request_user(&user, query.user_id.as_deref())?;
    playback_info_for_user(
        &state,
        user,
        item_id,
        &query,
        &access_token,
        device_id.as_deref(),
    )
    .await
}

pub async fn post_playback_info(
    State(state): State<AppState>,
    Path(item_id): Path<String>,
    headers: HeaderMap,
    uri: Uri,
    body: Bytes,
) -> Result<Json<PlaybackInfoResponseDto>, AppError> {
    let payload: PlaybackInfoRequestDto = parse_emby_body(&headers, &body)?;
    let access_token = access_token_from_request(&headers, uri.query())?;
    let device_id = client_device_id_from_request(&headers, uri.query())?;
    let user = authenticate_request_user(&state, &headers, &uri).await?;
    assert_request_user(&user, payload.user_id.as_deref())?;
    playback_info_for_user(
        &state,
        user,
        item_id,
        &payload,
        &access_token,
        device_id.as_deref(),
    )
    .await
}

pub async fn user_playback_info(
    State(state): State<AppState>,
    Path((user_id, item_id)): Path<(String, String)>,
    Query(query): Query<PlaybackInfoRequestDto>,
    headers: HeaderMap,
    uri: Uri,
) -> Result<Json<PlaybackInfoResponseDto>, AppError> {
    let access_token = access_token_from_request(&headers, uri.query())?;
    let device_id = client_device_id_from_request(&headers, uri.query())?;
    let user = authenticate_route_user(&state, &user_id, &headers, &uri).await?;
    assert_request_user(&user, query.user_id.as_deref())?;
    playback_info_for_user(
        &state,
        user,
        item_id,
        &query,
        &access_token,
        device_id.as_deref(),
    )
    .await
}

pub async fn post_user_playback_info(
    State(state): State<AppState>,
    Path((user_id, item_id)): Path<(String, String)>,
    headers: HeaderMap,
    uri: Uri,
    body: Bytes,
) -> Result<Json<PlaybackInfoResponseDto>, AppError> {
    let payload: PlaybackInfoRequestDto = parse_emby_body(&headers, &body)?;
    let access_token = access_token_from_request(&headers, uri.query())?;
    let device_id = client_device_id_from_request(&headers, uri.query())?;
    let user = authenticate_route_user(&state, &user_id, &headers, &uri).await?;
    assert_request_user(&user, payload.user_id.as_deref())?;
    playback_info_for_user(
        &state,
        user,
        item_id,
        &payload,
        &access_token,
        device_id.as_deref(),
    )
    .await
}

pub async fn bitrate_test(
    State(state): State<AppState>,
    Query(query): Query<BitrateTestQuery>,
    headers: HeaderMap,
    uri: Uri,
) -> Result<Response, AppError> {
    let _user = authenticate_request_user(&state, &headers, &uri).await?;
    let size = validated_bitrate_test_size(query)?;

    Ok(bitrate_test_response(size))
}

pub async fn live_stream_open(
    State(state): State<AppState>,
    headers: HeaderMap,
    uri: Uri,
    body: Bytes,
) -> Result<Json<LiveStreamResponseDto>, AppError> {
    let request: LiveStreamRequestDto = parse_emby_body(&headers, &body)?;
    let user = authenticate_request_user(&state, &headers, &uri).await?;
    assert_request_user(&user, request.user_id.as_deref())?;
    let _input = live_stream_open_input(request)?;

    Ok(Json(empty_live_stream_response()))
}

pub async fn live_stream_media_info(
    State(state): State<AppState>,
    Query(query): Query<LiveStreamQuery>,
    headers: HeaderMap,
    uri: Uri,
) -> Result<Json<PlaybackInfoResponseDto>, AppError> {
    authenticate_request_user(&state, &headers, &uri).await?;
    let _live_stream_id = required_live_stream_id(query.live_stream_id.as_deref())?;

    Ok(Json(empty_live_stream_media_info()))
}

pub async fn live_stream_close(
    State(state): State<AppState>,
    Query(query): Query<LiveStreamQuery>,
    headers: HeaderMap,
    uri: Uri,
) -> Result<StatusCode, AppError> {
    authenticate_request_user(&state, &headers, &uri).await?;
    let _live_stream_id = required_live_stream_id(query.live_stream_id.as_deref())?;

    Ok(StatusCode::OK)
}

pub async fn playing(
    State(state): State<AppState>,
    headers: HeaderMap,
    uri: Uri,
    body: Bytes,
) -> Result<StatusCode, AppError> {
    let payload: PlaybackProgressDto = parse_emby_body(&headers, &body)?;
    let user = authenticate_request_user(&state, &headers, &uri).await?;
    assert_request_user(&user, payload.user_id.as_deref())?;
    start_playback_for_user(&state, user, payload).await
}

pub async fn playing_ping(
    State(state): State<AppState>,
    Query(query): Query<PlaybackPingQuery>,
    headers: HeaderMap,
    uri: Uri,
) -> Result<StatusCode, AppError> {
    let _user = authenticate_request_user(&state, &headers, &uri).await?;
    let _input = playback_ping_input(query)?;

    Ok(StatusCode::NO_CONTENT)
}

pub async fn user_playing_item(
    State(state): State<AppState>,
    Path((user_id, item_id)): Path<(String, String)>,
    Query(query): Query<PlaybackProgressPathDto>,
    headers: HeaderMap,
    uri: Uri,
    body: Bytes,
) -> Result<StatusCode, AppError> {
    let payload = playback_payload_from_user_item_path(&headers, &body, query, &user_id, &item_id)?;
    let user = authenticate_route_user(&state, &user_id, &headers, &uri).await?;
    assert_request_user(&user, payload.user_id.as_deref())?;
    start_playback_for_user(&state, user, payload).await
}

async fn start_playback_for_user(
    state: &AppState,
    user: AuthenticatedUser,
    payload: PlaybackProgressDto,
) -> Result<StatusCode, AppError> {
    let Some(database) = state.database() else {
        return Err(AppError::internal("database is not configured"));
    };

    let report = report_input(user.id, &payload);
    let started = MediaRepository::new(database.clone())
        .start_playback(report.clone())
        .await
        .map_err(|err| AppError::internal(format!("failed to start playback: {err}")))?;

    let Some(playback_session_id) = started.as_deref() else {
        return Err(AppError::not_found("playback item not found"));
    };

    dispatch_playback_hook(
        database,
        PLAYBACK_STARTED_EVENT,
        &user,
        &report,
        Some(playback_session_id),
    )
    .await;

    Ok(StatusCode::NO_CONTENT)
}

pub async fn playing_progress(
    State(state): State<AppState>,
    headers: HeaderMap,
    uri: Uri,
    body: Bytes,
) -> Result<StatusCode, AppError> {
    let payload: PlaybackProgressDto = parse_emby_body(&headers, &body)?;
    let user = authenticate_request_user(&state, &headers, &uri).await?;
    assert_request_user(&user, payload.user_id.as_deref())?;
    update_playback_progress_for_user(&state, user, payload).await
}

pub async fn user_playing_item_progress(
    State(state): State<AppState>,
    Path((user_id, item_id)): Path<(String, String)>,
    Query(query): Query<PlaybackProgressPathDto>,
    headers: HeaderMap,
    uri: Uri,
    body: Bytes,
) -> Result<StatusCode, AppError> {
    let payload = playback_payload_from_user_item_path(&headers, &body, query, &user_id, &item_id)?;
    let user = authenticate_route_user(&state, &user_id, &headers, &uri).await?;
    assert_request_user(&user, payload.user_id.as_deref())?;
    update_playback_progress_for_user(&state, user, payload).await
}

async fn update_playback_progress_for_user(
    state: &AppState,
    user: AuthenticatedUser,
    payload: PlaybackProgressDto,
) -> Result<StatusCode, AppError> {
    let Some(database) = state.database() else {
        return Err(AppError::internal("database is not configured"));
    };

    let updated = MediaRepository::new(database.clone())
        .update_playback_progress(report_input(user.id, &payload))
        .await
        .map_err(|err| AppError::internal(format!("failed to update playback progress: {err}")))?;

    if !updated {
        return Err(AppError::not_found("playback item not found"));
    }

    Ok(StatusCode::NO_CONTENT)
}

pub async fn playing_stopped(
    State(state): State<AppState>,
    headers: HeaderMap,
    uri: Uri,
    body: Bytes,
) -> Result<StatusCode, AppError> {
    let payload: PlaybackProgressDto = parse_emby_body(&headers, &body)?;
    let user = authenticate_request_user(&state, &headers, &uri).await?;
    assert_request_user(&user, payload.user_id.as_deref())?;
    stop_playback_for_user(&state, user, payload).await
}

pub async fn user_playing_item_stopped(
    State(state): State<AppState>,
    Path((user_id, item_id)): Path<(String, String)>,
    Query(query): Query<PlaybackProgressPathDto>,
    headers: HeaderMap,
    uri: Uri,
    body: Bytes,
) -> Result<StatusCode, AppError> {
    let payload = playback_payload_from_user_item_path(&headers, &body, query, &user_id, &item_id)?;
    let user = authenticate_route_user(&state, &user_id, &headers, &uri).await?;
    assert_request_user(&user, payload.user_id.as_deref())?;
    stop_playback_for_user(&state, user, payload).await
}

async fn stop_playback_for_user(
    state: &AppState,
    user: AuthenticatedUser,
    payload: PlaybackProgressDto,
) -> Result<StatusCode, AppError> {
    let Some(database) = state.database() else {
        return Err(AppError::internal("database is not configured"));
    };

    let report = report_input(user.id, &payload);
    let stopped = MediaRepository::new(database.clone())
        .stop_playback(report.clone())
        .await
        .map_err(|err| AppError::internal(format!("failed to stop playback: {err}")))?;

    if !stopped {
        return Err(AppError::not_found("playback item not found"));
    }

    dispatch_playback_hook(database, PLAYBACK_STOPPED_EVENT, &user, &report, None).await;

    Ok(StatusCode::NO_CONTENT)
}

async fn playback_info_for_user(
    state: &AppState,
    user: AuthenticatedUser,
    item_id: String,
    request: &PlaybackInfoRequestDto,
    access_token: &str,
    device_id: Option<&str>,
) -> Result<Json<PlaybackInfoResponseDto>, AppError> {
    let Some(database) = state.database() else {
        return Err(AppError::internal("database is not configured"));
    };

    let Some(source) = MediaRepository::new(database.clone())
        .find_playback_media_source(
            user.id,
            &item_id,
            media_source_id_as_i64(request.media_source_id.as_deref()),
        )
        .await
        .map_err(|err| AppError::internal(format!("failed to get playback source: {err}")))?
    else {
        return Err(AppError::not_found("playback source not found"));
    };

    let play_session_id = issue_access_token().token;
    let transcode = queue_transcode_if_needed(
        state,
        user.id,
        &source,
        request,
        &play_session_id,
        access_token,
        device_id,
    )
    .await?;

    Ok(Json(PlaybackInfoResponseDto {
        media_sources: vec![media_source_to_dto(
            &source,
            transcode.as_ref(),
            access_token,
        )],
        play_session_id,
        error_code: None,
    }))
}

fn assert_request_user(
    user: &AuthenticatedUser,
    requested_user_id: Option<&str>,
) -> Result<(), AppError> {
    if let Some(requested_user_id) = requested_user_id
        && requested_user_id != user.public_id
    {
        return Err(AppError::forbidden(
            "authenticated user does not match playback user",
        ));
    }

    Ok(())
}

fn validated_bitrate_test_size(query: BitrateTestQuery) -> Result<usize, AppError> {
    let Some(size) = query.size else {
        return Err(AppError::unprocessable("bitrate test size is required"));
    };
    if size == 0 {
        return Err(AppError::unprocessable(
            "bitrate test size must be greater than zero",
        ));
    }
    if size > MAX_BITRATE_TEST_SIZE {
        return Err(AppError::unprocessable(format!(
            "bitrate test size must be less than or equal to {MAX_BITRATE_TEST_SIZE}",
        )));
    }

    Ok(size)
}

fn bitrate_test_response(size: usize) -> Response {
    let stream = ReaderStream::new(io::repeat(0).take(size as u64));
    let mut response = Response::new(Body::from_stream(stream));
    response.headers_mut().insert(
        CONTENT_TYPE,
        HeaderValue::from_static("application/octet-stream"),
    );
    response
        .headers_mut()
        .insert(CACHE_CONTROL, HeaderValue::from_static("no-store"));
    if let Ok(value) = HeaderValue::from_str(&size.to_string()) {
        response.headers_mut().insert(CONTENT_LENGTH, value);
    }

    response
}

fn playback_ping_input(query: PlaybackPingQuery) -> Result<PlaybackPingInput, AppError> {
    Ok(PlaybackPingInput {
        play_session_id: normalize_play_session_id(query.play_session_id.as_deref())?,
    })
}

fn live_stream_open_input(request: LiveStreamRequestDto) -> Result<LiveStreamOpenInput, AppError> {
    Ok(LiveStreamOpenInput {
        open_token: normalize_live_stream_text(request.open_token.as_deref(), "OpenToken")?,
        user_id: normalize_live_stream_text(request.user_id.as_deref(), "UserId")?,
        play_session_id: normalize_live_stream_text(
            request.play_session_id.as_deref(),
            "PlaySessionId",
        )?,
    })
}

fn empty_live_stream_response() -> LiveStreamResponseDto {
    LiveStreamResponseDto { media_source: None }
}

fn empty_live_stream_media_info() -> PlaybackInfoResponseDto {
    PlaybackInfoResponseDto {
        media_sources: Vec::new(),
        play_session_id: String::new(),
        error_code: None,
    }
}

fn normalize_play_session_id(value: Option<&str>) -> Result<Option<String>, AppError> {
    let Some(value) = value.map(str::trim).filter(|value| !value.is_empty()) else {
        return Ok(None);
    };
    if value.len() > MAX_PLAY_SESSION_ID_LEN
        || !value
            .chars()
            .all(|ch| ch.is_ascii_alphanumeric() || matches!(ch, '-' | '_'))
    {
        return Err(AppError::unprocessable("play session id is invalid"));
    }

    Ok(Some(value.to_owned()))
}

fn required_live_stream_id(value: Option<&str>) -> Result<String, AppError> {
    normalize_live_stream_text(value, "LiveStreamId")?
        .ok_or_else(|| AppError::unprocessable("live stream id is required"))
}

fn normalize_live_stream_text(
    value: Option<&str>,
    name: &'static str,
) -> Result<Option<String>, AppError> {
    let Some(value) = value.map(str::trim).filter(|value| !value.is_empty()) else {
        return Ok(None);
    };
    if value.len() > MAX_PLAY_SESSION_ID_LEN
        || !value
            .chars()
            .all(|ch| ch.is_ascii_alphanumeric() || matches!(ch, '-' | '_' | '.' | ':'))
    {
        return Err(AppError::unprocessable(format!("{name} is invalid")));
    }

    Ok(Some(value.to_owned()))
}

fn playback_payload_from_user_item_path(
    headers: &HeaderMap,
    body: &Bytes,
    query: PlaybackProgressPathDto,
    route_user_id: &str,
    route_item_id: &str,
) -> Result<PlaybackProgressDto, AppError> {
    let body_payload = if body.is_empty() {
        PlaybackProgressPathDto::default()
    } else {
        parse_emby_body(headers, body)?
    };
    let payload = PlaybackProgressPathDto {
        item_id: body_payload.item_id.or(query.item_id),
        user_id: body_payload.user_id.or(query.user_id),
        session_id: body_payload.session_id.or(query.session_id),
        play_session_id: body_payload.play_session_id.or(query.play_session_id),
        media_source_id: body_payload.media_source_id.or(query.media_source_id),
        play_method: body_payload.play_method.or(query.play_method),
        queueable_media_types: if body_payload.queueable_media_types.is_empty() {
            query.queueable_media_types
        } else {
            body_payload.queueable_media_types
        },
        can_seek: body_payload.can_seek.or(query.can_seek),
        event_name: body_payload.event_name.or(query.event_name),
        audio_stream_index: body_payload.audio_stream_index.or(query.audio_stream_index),
        subtitle_stream_index: body_payload
            .subtitle_stream_index
            .or(query.subtitle_stream_index),
        position_ticks: body_payload.position_ticks.or(query.position_ticks),
        is_paused: body_payload.is_paused.or(query.is_paused),
        is_muted: body_payload.is_muted.or(query.is_muted),
        volume_level: body_payload.volume_level.or(query.volume_level),
        live_stream_id: body_payload.live_stream_id.or(query.live_stream_id),
        playlist_index: body_payload.playlist_index.or(query.playlist_index),
        playlist_length: body_payload.playlist_length.or(query.playlist_length),
        subtitle_offset: body_payload.subtitle_offset.or(query.subtitle_offset),
        playback_rate: body_payload.playback_rate.or(query.playback_rate),
        now_playing_queue: if body_payload.now_playing_queue.is_empty() {
            query.now_playing_queue
        } else {
            body_payload.now_playing_queue
        },
        playlist_item_id: body_payload.playlist_item_id.or(query.playlist_item_id),
        playlist_item_ids: if body_payload.playlist_item_ids.is_empty() {
            query.playlist_item_ids
        } else {
            body_payload.playlist_item_ids
        },
        runtime_ticks: body_payload.runtime_ticks.or(query.runtime_ticks),
        playback_start_time_ticks: body_payload
            .playback_start_time_ticks
            .or(query.playback_start_time_ticks),
        brightness: body_payload.brightness.or(query.brightness),
        aspect_ratio: body_payload.aspect_ratio.or(query.aspect_ratio),
        repeat_mode: body_payload.repeat_mode.or(query.repeat_mode),
        sleep_timer_mode: body_payload.sleep_timer_mode.or(query.sleep_timer_mode),
        sleep_timer_end_time: body_payload
            .sleep_timer_end_time
            .or(query.sleep_timer_end_time),
        shuffle: body_payload.shuffle.or(query.shuffle),
    };
    let item_id = match payload.item_id {
        Some(body_item_id) if body_item_id != route_item_id => {
            return Err(AppError::unprocessable(
                "playback item id does not match route item",
            ));
        }
        Some(body_item_id) => body_item_id,
        None => route_item_id.to_owned(),
    };

    Ok(PlaybackProgressDto {
        item_id,
        user_id: payload.user_id.or_else(|| Some(route_user_id.to_owned())),
        session_id: payload.session_id,
        play_session_id: payload.play_session_id,
        media_source_id: payload.media_source_id,
        play_method: payload.play_method,
        queueable_media_types: payload.queueable_media_types,
        can_seek: payload.can_seek,
        event_name: payload.event_name,
        audio_stream_index: payload.audio_stream_index,
        subtitle_stream_index: payload.subtitle_stream_index,
        position_ticks: payload.position_ticks,
        is_paused: payload.is_paused,
        is_muted: payload.is_muted,
        volume_level: payload.volume_level,
        live_stream_id: payload.live_stream_id,
        playlist_index: payload.playlist_index,
        playlist_length: payload.playlist_length,
        subtitle_offset: payload.subtitle_offset,
        playback_rate: payload.playback_rate,
        now_playing_queue: payload.now_playing_queue,
        playlist_item_id: payload.playlist_item_id,
        playlist_item_ids: payload.playlist_item_ids,
        runtime_ticks: payload.runtime_ticks,
        playback_start_time_ticks: payload.playback_start_time_ticks,
        brightness: payload.brightness,
        aspect_ratio: payload.aspect_ratio,
        repeat_mode: payload.repeat_mode,
        sleep_timer_mode: payload.sleep_timer_mode,
        sleep_timer_end_time: payload.sleep_timer_end_time,
        shuffle: payload.shuffle,
    })
}

fn report_input(user_id: i64, payload: &PlaybackProgressDto) -> PlaybackReportInput {
    PlaybackReportInput {
        user_id,
        item_id: payload.item_id.clone(),
        media_file_id: media_source_id_as_i64(payload.media_source_id.as_deref()),
        client_session_id: payload
            .play_session_id
            .clone()
            .or_else(|| payload.session_id.clone()),
        position_ticks: payload.position_ticks.unwrap_or(0).max(0),
        is_paused: payload.is_paused.unwrap_or(false),
        play_method: normalize_play_method(payload.play_method.as_deref()).to_owned(),
    }
}

async fn dispatch_playback_hook(
    pool: &DbPool,
    event_key: &'static str,
    user: &AuthenticatedUser,
    report: &PlaybackReportInput,
    playback_session_id: Option<&str>,
) {
    let event = playback_hook_event(event_key, user, report, playback_session_id);
    if let Err(err) = PluginHookDispatcher::new(pool.clone())
        .dispatch(event)
        .await
    {
        warn!(
            error = %err,
            event_key,
            item_id = %report.item_id,
            user_id = %user.public_id,
            "failed to dispatch plugin playback hooks"
        );
    }
}

fn playback_hook_event(
    event_key: &'static str,
    user: &AuthenticatedUser,
    report: &PlaybackReportInput,
    playback_session_id: Option<&str>,
) -> PluginHookEvent {
    PluginHookEvent {
        event_key: event_key.to_owned(),
        aggregate_type: "media_item".to_owned(),
        aggregate_id: report.item_id.clone(),
        payload: playback_hook_payload(user, report, playback_session_id),
    }
}

fn playback_hook_payload(
    user: &AuthenticatedUser,
    report: &PlaybackReportInput,
    playback_session_id: Option<&str>,
) -> Value {
    json!({
        "userId": &user.public_id,
        "username": &user.username,
        "itemId": &report.item_id,
        "mediaSourceId": report.media_file_id.map(|id| id.to_string()),
        "clientSessionId": report.client_session_id.as_deref(),
        "playbackSessionId": playback_session_id,
        "positionTicks": report.position_ticks,
        "isPaused": report.is_paused,
        "playMethod": &report.play_method,
    })
}

fn media_source_id_as_i64(value: Option<&str>) -> Option<i64> {
    value.and_then(|value| value.parse::<i64>().ok())
}

fn normalize_play_method(value: Option<&str>) -> &'static str {
    match value
        .unwrap_or("DirectPlay")
        .chars()
        .filter(|ch| *ch != '_' && *ch != '-')
        .collect::<String>()
        .to_ascii_lowercase()
        .as_str()
    {
        "directstream" => "direct_stream",
        "transcode" | "transcoding" => "transcode",
        "strmredirect" => "strm_redirect",
        _ => "direct_play",
    }
}

async fn queue_transcode_if_needed(
    state: &AppState,
    user_id: i64,
    source: &PlaybackMediaSourceRecord,
    request: &PlaybackInfoRequestDto,
    play_session_id: &str,
    access_token: &str,
    device_id: Option<&str>,
) -> Result<Option<PlaybackTranscodeInfo>, AppError> {
    if !should_queue_transcode(source, request) {
        return Ok(None);
    }

    let Some(database) = state.database() else {
        return Err(AppError::internal("database is not configured"));
    };
    let Some(input_path) = transcode_input_path(source) else {
        return Ok(None);
    };

    let bitrate = requested_streaming_bitrate(request);
    let output_base_path = state
        .config()
        .storage
        .transcode_cache_dir
        .to_string_lossy()
        .into_owned();
    let session = TranscodeRepository::new(database.clone())
        .create_session(CreateTranscodeSessionInput {
            user_id,
            media_item_id: source.media_item_id,
            media_file_id: Some(source.media_file_id),
            input_path,
            output_base_path,
            play_session_id: Some(play_session_id.to_owned()),
            device_id: device_id.map(str::to_owned),
            video_codec: Some("h264".to_owned()),
            audio_codec: Some("aac".to_owned()),
            container: Some("hls".to_owned()),
            bitrate,
        })
        .await
        .map_err(|err| AppError::internal(format!("failed to queue transcode session: {err}")))?;

    Ok(Some(PlaybackTranscodeInfo {
        url: transcode_url(source, &session.id, play_session_id, access_token),
        bitrate,
    }))
}

fn should_queue_transcode(
    source: &PlaybackMediaSourceRecord,
    request: &PlaybackInfoRequestDto,
) -> bool {
    if !source.supports_transcoding {
        return false;
    }

    let Some(requested_bitrate) = requested_streaming_bitrate(request) else {
        return false;
    };
    let Some(source_bitrate) = source.bitrate else {
        return false;
    };

    requested_bitrate < source_bitrate
}

fn requested_streaming_bitrate(request: &PlaybackInfoRequestDto) -> Option<i32> {
    request
        .max_streaming_bitrate
        .filter(|bitrate| *bitrate > 0)
        .map(|bitrate| bitrate.min(i64::from(i32::MAX)) as i32)
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

fn transcode_url(
    source: &PlaybackMediaSourceRecord,
    session_id: &str,
    play_session_id: &str,
    access_token: &str,
) -> String {
    format!(
        "/emby/Videos/{}/master.m3u8?MediaSourceId={}&TranscodeSessionId={}&PlaySessionId={}&api_key={}",
        source.item_id, source.media_file_id, session_id, play_session_id, access_token
    )
}

fn media_source_to_dto(
    source: &PlaybackMediaSourceRecord,
    transcode: Option<&PlaybackTranscodeInfo>,
    access_token: &str,
) -> MediaSourceDto {
    let protocol = media_source_protocol(&source);
    MediaSourceDto {
        id: source.media_file_id.to_string(),
        source_type: "Default".to_owned(),
        name: source.media_file_id.to_string(),
        item_id: Some(source.item_id.clone()),
        path: Some(media_source_path(&source)),
        protocol: protocol.to_owned(),
        is_remote: protocol == "Http",
        requires_opening: false,
        requires_closing: false,
        supports_probing: false,
        read_at_native_framerate: false,
        container: source.container.clone(),
        run_time_ticks: source.runtime_ticks,
        size: source.file_size,
        bitrate: transcode
            .and_then(|session| session.bitrate)
            .or(source.bitrate),
        default_audio_stream_index: default_stream_index(&source.streams, "audio"),
        default_subtitle_stream_index: default_stream_index(&source.streams, "subtitle"),
        media_streams: source
            .streams
            .iter()
            .cloned()
            .map(media_stream_to_dto)
            .collect(),
        supports_direct_play: true,
        supports_direct_stream: true,
        supports_transcoding: source.supports_transcoding,
        direct_stream_url: Some(direct_stream_url(source, access_token)),
        add_api_key_to_direct_stream_url: false,
        transcoding_url: transcode.map(|session| session.url.clone()),
        transcoding_sub_protocol: transcode.map(|_| "hls".to_owned()),
        transcoding_container: transcode.map(|_| "ts".to_owned()),
        chapters: Vec::new(),
    }
}

fn default_stream_index(streams: &[PlaybackMediaStreamRecord], stream_type: &str) -> Option<i32> {
    streams
        .iter()
        .find(|stream| stream.stream_type.eq_ignore_ascii_case(stream_type) && stream.is_default)
        .or_else(|| {
            streams
                .iter()
                .find(|stream| stream.stream_type.eq_ignore_ascii_case(stream_type))
        })
        .map(|stream| stream.stream_index)
}

fn direct_stream_url(source: &PlaybackMediaSourceRecord, access_token: &str) -> String {
    if is_audio_item(source) {
        return format!(
            "/emby/Audio/{}/{}?MediaSourceId={}&Static=true&api_key={}",
            source.item_id,
            audio_stream_file_name(source.container.as_deref()),
            source.media_file_id,
            access_token
        );
    }

    format!(
        "/emby/Videos/{}/stream?MediaSourceId={}&Static=true&api_key={}",
        source.item_id, source.media_file_id, access_token
    )
}

fn is_audio_item(source: &PlaybackMediaSourceRecord) -> bool {
    matches!(source.item_type.as_str(), "track" | "audio")
}

fn audio_stream_file_name(container: Option<&str>) -> String {
    let Some(container) = container
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .filter(|value| value.len() <= 16)
        .filter(|value| {
            value
                .chars()
                .all(|ch| ch.is_ascii_alphanumeric() || matches!(ch, '_' | '-'))
        })
    else {
        return "stream".to_owned();
    };

    format!("stream.{container}")
}

fn media_source_path(source: &PlaybackMediaSourceRecord) -> String {
    if source.is_strm {
        return source
            .strm_target
            .clone()
            .unwrap_or_else(|| source.path.clone());
    }

    source.path.clone()
}

fn media_source_protocol(source: &PlaybackMediaSourceRecord) -> &'static str {
    if source.is_strm
        && source
            .strm_target
            .as_deref()
            .is_some_and(|target| target.starts_with("http://") || target.starts_with("https://"))
    {
        return "Http";
    }

    "File"
}

fn media_stream_to_dto(stream: PlaybackMediaStreamRecord) -> MediaStreamDto {
    let display_title = media_stream_display_title(&stream);
    MediaStreamDto {
        index: stream.stream_index,
        stream_type: emby_stream_type(&stream.stream_type).to_owned(),
        codec: stream.codec.clone(),
        codec_tag: stream.codec_tag.clone(),
        language: stream.language.clone(),
        title: stream.title.clone(),
        display_title,
        profile: stream.profile.clone(),
        level: stream.level,
        width: stream.width,
        height: stream.height,
        channels: stream.channels,
        sample_rate: stream.sample_rate,
        bit_depth: stream.bit_depth,
        bit_rate: stream.bitrate,
        is_default: stream.is_default,
        is_forced: stream.is_forced,
    }
}

fn media_stream_display_title(stream: &PlaybackMediaStreamRecord) -> Option<String> {
    let mut parts = Vec::new();
    if let Some(title) = stream
        .title
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        parts.push(title.to_owned());
    }
    match stream.stream_type.as_str() {
        "video" => {
            if let Some(height) = stream.height.filter(|value| *value > 0) {
                parts.push(format!("{height}p"));
            }
            if let Some(codec) = stream
                .codec
                .as_deref()
                .map(str::trim)
                .filter(|value| !value.is_empty())
            {
                parts.push(codec.to_ascii_uppercase());
            }
        }
        "audio" => {
            if let Some(language) = stream
                .language
                .as_deref()
                .map(str::trim)
                .filter(|value| !value.is_empty())
            {
                parts.push(language.to_owned());
            }
            if let Some(codec) = stream
                .codec
                .as_deref()
                .map(str::trim)
                .filter(|value| !value.is_empty())
            {
                parts.push(codec.to_ascii_uppercase());
            }
            if let Some(channels) = stream.channels.filter(|value| *value > 0) {
                parts.push(format!("{channels} ch"));
            }
        }
        "subtitle" => {
            if let Some(language) = stream
                .language
                .as_deref()
                .map(str::trim)
                .filter(|value| !value.is_empty())
            {
                parts.push(language.to_owned());
            }
            if stream.is_forced {
                parts.push("Forced".to_owned());
            }
        }
        _ => {}
    }

    (!parts.is_empty()).then(|| parts.join(" - "))
}

fn emby_stream_type(stream_type: &str) -> &'static str {
    match stream_type {
        "video" => "Video",
        "audio" => "Audio",
        "subtitle" => "Subtitle",
        "attachment" => "Attachment",
        _ => "Data",
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct PlaybackTranscodeInfo {
    url: String,
    bitrate: Option<i32>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn strm_http_source_uses_http_protocol_and_target_path() {
        let source = PlaybackMediaSourceRecord {
            path: "movie.strm".to_owned(),
            is_strm: true,
            strm_target: Some("http://192.168.1.10/movie.mkv".to_owned()),
            container: None,
            bitrate: None,
            supports_transcoding: false,
            ..test_source()
        };
        let dto = media_source_to_dto(&source, None, "token-1");

        assert_eq!(dto.id, "42");
        assert_eq!(dto.source_type, "Default");
        assert_eq!(dto.name, "42");
        assert_eq!(dto.item_id.as_deref(), Some("item-1"));
        assert_eq!(dto.protocol, "Http");
        assert!(dto.is_remote);
        assert!(!dto.requires_opening);
        assert!(!dto.requires_closing);
        assert!(!dto.supports_probing);
        assert!(!dto.read_at_native_framerate);
        assert_eq!(dto.path.as_deref(), Some("http://192.168.1.10/movie.mkv"));
        assert_eq!(
            dto.direct_stream_url.as_deref(),
            Some("/emby/Videos/item-1/stream?MediaSourceId=42&Static=true&api_key=token-1")
        );
    }

    #[test]
    fn stream_type_mapping_uses_emby_pascal_case() {
        let dto = media_stream_to_dto(PlaybackMediaStreamRecord {
            stream_index: 0,
            stream_type: "video".to_owned(),
            codec: Some("hevc".to_owned()),
            width: Some(3840),
            height: Some(2160),
            bit_depth: Some(10),
            bitrate: Some(12_000_000),
            ..test_stream()
        });

        assert_eq!(dto.stream_type, "Video");
        assert_eq!(dto.codec.as_deref(), Some("hevc"));
        assert_eq!(dto.bit_depth, Some(10));
        assert_eq!(dto.bit_rate, Some(12_000_000));
        assert_eq!(dto.display_title.as_deref(), Some("2160p - HEVC"));
    }

    #[test]
    fn report_input_normalizes_client_payload() {
        let input = report_input(
            7,
            &PlaybackProgressDto {
                item_id: "item-1".to_owned(),
                user_id: Some("user-1".to_owned()),
                session_id: None,
                play_session_id: Some("play-1".to_owned()),
                media_source_id: Some("42".to_owned()),
                play_method: Some("DirectStream".to_owned()),
                queueable_media_types: vec!["Audio".to_owned(), "Video".to_owned()],
                can_seek: Some(true),
                event_name: Some("TimeUpdate".to_owned()),
                audio_stream_index: Some(1),
                subtitle_stream_index: Some(-1),
                position_ticks: Some(-10),
                is_paused: Some(true),
                is_muted: Some(false),
                volume_level: Some(85),
                live_stream_id: Some("live-1".to_owned()),
                playlist_index: Some(2),
                playlist_length: Some(4),
                subtitle_offset: Some(0.0),
                playback_rate: Some(1.0),
                now_playing_queue: Vec::new(),
                playlist_item_id: None,
                playlist_item_ids: Vec::new(),
                runtime_ticks: None,
                playback_start_time_ticks: None,
                brightness: None,
                aspect_ratio: None,
                repeat_mode: None,
                sleep_timer_mode: None,
                sleep_timer_end_time: None,
                shuffle: None,
            },
        );

        assert_eq!(input.user_id, 7);
        assert_eq!(input.media_file_id, Some(42));
        assert_eq!(input.client_session_id.as_deref(), Some("play-1"));
        assert_eq!(input.position_ticks, 0);
        assert!(input.is_paused);
        assert_eq!(input.play_method, "direct_stream");
    }

    #[test]
    fn user_item_playback_payload_uses_path_ids_when_body_omits_them() {
        let headers = HeaderMap::new();
        let query = PlaybackProgressPathDto::default();
        let body = Bytes::from_static(
            br#"{"PlaySessionId":"play-1","MediaSourceId":"42","PositionTicks":42,"QueueableMediaTypes":["Audio","Video"],"CanSeek":true,"EventName":"Pause","AudioStreamIndex":1,"SubtitleStreamIndex":-1,"IsMuted":false,"VolumeLevel":75,"LiveStreamId":"live-1","PlaylistIndex":2,"PlaylistLength":4,"SubtitleOffset":0,"PlaybackRate":1.5,"NowPlayingQueue":[{"Id":"queue-1"}],"PlaylistItemId":"playlist-item-1","PlaylistItemIds":["playlist-item-1","playlist-item-2"],"RunTimeTicks":9000,"PlaybackStartTimeTicks":100,"Brightness":40,"AspectRatio":"16:9","RepeatMode":"RepeatAll","SleepTimerMode":"EndOfEpisode","SleepTimerEndTime":"2026-06-24T12:00:00Z","Shuffle":true}"#,
        );

        let payload =
            playback_payload_from_user_item_path(&headers, &body, query, "user-1", "item-1")
                .unwrap();

        assert_eq!(payload.item_id, "item-1");
        assert_eq!(payload.user_id.as_deref(), Some("user-1"));
        assert_eq!(payload.play_session_id.as_deref(), Some("play-1"));
        assert_eq!(payload.media_source_id.as_deref(), Some("42"));
        assert_eq!(payload.position_ticks, Some(42));
        assert_eq!(
            payload.queueable_media_types,
            vec!["Audio".to_owned(), "Video".to_owned()]
        );
        assert_eq!(payload.can_seek, Some(true));
        assert_eq!(payload.event_name.as_deref(), Some("Pause"));
        assert_eq!(payload.audio_stream_index, Some(1));
        assert_eq!(payload.subtitle_stream_index, Some(-1));
        assert_eq!(payload.is_muted, Some(false));
        assert_eq!(payload.volume_level, Some(75));
        assert_eq!(payload.live_stream_id.as_deref(), Some("live-1"));
        assert_eq!(payload.playlist_index, Some(2));
        assert_eq!(payload.playlist_length, Some(4));
        assert_eq!(payload.subtitle_offset, Some(0.0));
        assert_eq!(payload.playback_rate, Some(1.5));
        assert_eq!(payload.now_playing_queue.len(), 1);
        assert_eq!(payload.playlist_item_id.as_deref(), Some("playlist-item-1"));
        assert_eq!(
            payload.playlist_item_ids,
            vec!["playlist-item-1".to_owned(), "playlist-item-2".to_owned()]
        );
        assert_eq!(payload.runtime_ticks, Some(9000));
        assert_eq!(payload.playback_start_time_ticks, Some(100));
        assert_eq!(payload.brightness, Some(40));
        assert_eq!(payload.aspect_ratio.as_deref(), Some("16:9"));
        assert_eq!(payload.repeat_mode.as_deref(), Some("RepeatAll"));
        assert_eq!(payload.sleep_timer_mode.as_deref(), Some("EndOfEpisode"));
        assert_eq!(
            payload.sleep_timer_end_time.as_deref(),
            Some("2026-06-24T12:00:00Z")
        );
        assert_eq!(payload.shuffle, Some(true));
    }

    #[test]
    fn user_item_playback_payload_rejects_conflicting_body_item_id() {
        let headers = HeaderMap::new();
        let query = PlaybackProgressPathDto::default();
        let body = Bytes::from_static(br#"{"ItemId":"other-item","PositionTicks":42}"#);

        let err = playback_payload_from_user_item_path(&headers, &body, query, "user-1", "item-1")
            .unwrap_err();

        assert_eq!(err.status_code(), StatusCode::UNPROCESSABLE_ENTITY);
    }

    #[test]
    fn user_item_playback_payload_accepts_query_only_reports() {
        let headers = HeaderMap::new();
        let body = Bytes::new();
        let query = PlaybackProgressPathDto {
            item_id: None,
            user_id: None,
            session_id: None,
            play_session_id: Some("play-1".to_owned()),
            media_source_id: Some("42".to_owned()),
            play_method: Some("DirectStream".to_owned()),
            queueable_media_types: vec!["Video".to_owned()],
            can_seek: Some(false),
            event_name: Some("TimeUpdate".to_owned()),
            audio_stream_index: Some(2),
            subtitle_stream_index: Some(-1),
            position_ticks: Some(42),
            is_paused: Some(false),
            is_muted: Some(true),
            volume_level: Some(0),
            live_stream_id: Some("live-2".to_owned()),
            playlist_index: Some(3),
            playlist_length: Some(8),
            subtitle_offset: Some(250.0),
            playback_rate: Some(0.75),
            ..PlaybackProgressPathDto::default()
        };

        let payload =
            playback_payload_from_user_item_path(&headers, &body, query, "user-1", "item-1")
                .unwrap();

        assert_eq!(payload.item_id, "item-1");
        assert_eq!(payload.user_id.as_deref(), Some("user-1"));
        assert_eq!(payload.play_session_id.as_deref(), Some("play-1"));
        assert_eq!(payload.media_source_id.as_deref(), Some("42"));
        assert_eq!(payload.position_ticks, Some(42));
        assert_eq!(payload.is_paused, Some(false));
        assert_eq!(payload.queueable_media_types, vec!["Video".to_owned()]);
        assert_eq!(payload.can_seek, Some(false));
        assert_eq!(payload.event_name.as_deref(), Some("TimeUpdate"));
        assert_eq!(payload.audio_stream_index, Some(2));
        assert_eq!(payload.subtitle_stream_index, Some(-1));
        assert_eq!(payload.is_muted, Some(true));
        assert_eq!(payload.volume_level, Some(0));
        assert_eq!(payload.live_stream_id.as_deref(), Some("live-2"));
        assert_eq!(payload.playlist_index, Some(3));
        assert_eq!(payload.playlist_length, Some(8));
        assert_eq!(payload.subtitle_offset, Some(250.0));
        assert_eq!(payload.playback_rate, Some(0.75));
    }

    #[test]
    fn live_stream_compat_responses_use_stable_empty_shapes() {
        assert_eq!(
            serde_json::to_value(empty_live_stream_response()).unwrap(),
            json!({ "MediaSource": null })
        );
        assert_eq!(
            serde_json::to_value(empty_live_stream_media_info()).unwrap(),
            json!({
                "MediaSources": [],
                "PlaySessionId": "",
                "ErrorCode": null
            })
        );
    }

    #[test]
    fn bitrate_test_size_requires_positive_bounded_size() {
        assert_eq!(
            validated_bitrate_test_size(BitrateTestQuery { size: Some(1024) }).unwrap(),
            1024
        );
        assert_eq!(
            validated_bitrate_test_size(BitrateTestQuery { size: None })
                .unwrap_err()
                .status_code(),
            StatusCode::UNPROCESSABLE_ENTITY
        );
        assert_eq!(
            validated_bitrate_test_size(BitrateTestQuery { size: Some(0) })
                .unwrap_err()
                .status_code(),
            StatusCode::UNPROCESSABLE_ENTITY
        );
        assert_eq!(
            validated_bitrate_test_size(BitrateTestQuery {
                size: Some(MAX_BITRATE_TEST_SIZE + 1),
            })
            .unwrap_err()
            .status_code(),
            StatusCode::UNPROCESSABLE_ENTITY
        );
    }

    #[test]
    fn playback_ping_query_trims_and_bounds_play_session_id() {
        assert_eq!(
            playback_ping_input(PlaybackPingQuery {
                play_session_id: Some(" play-1 ".to_owned()),
            })
            .unwrap()
            .play_session_id
            .as_deref(),
            Some("play-1")
        );
        assert_eq!(
            playback_ping_input(PlaybackPingQuery {
                play_session_id: Some(" ".to_owned()),
            })
            .unwrap()
            .play_session_id,
            None
        );
        assert_eq!(
            playback_ping_input(PlaybackPingQuery {
                play_session_id: Some("x".repeat(MAX_PLAY_SESSION_ID_LEN + 1)),
            })
            .unwrap_err()
            .status_code(),
            StatusCode::UNPROCESSABLE_ENTITY
        );
    }

    #[test]
    fn playback_hook_event_preserves_client_context() {
        let user = AuthenticatedUser {
            id: 7,
            public_id: "user-1".to_owned(),
            username: "alice".to_owned(),
            role_name: "User".to_owned(),
            role_name_normalized: "user".to_owned(),
        };
        let report = PlaybackReportInput {
            user_id: 7,
            item_id: "item-1".to_owned(),
            media_file_id: Some(42),
            client_session_id: Some("play-1".to_owned()),
            position_ticks: 120_000,
            is_paused: false,
            play_method: "direct_stream".to_owned(),
        };

        let event = playback_hook_event(PLAYBACK_STARTED_EVENT, &user, &report, Some("session-1"));

        assert_eq!(event.event_key, "playback.started");
        assert_eq!(event.aggregate_type, "media_item");
        assert_eq!(event.aggregate_id, "item-1");
        assert_eq!(event.payload["userId"], "user-1");
        assert_eq!(event.payload["username"], "alice");
        assert_eq!(event.payload["mediaSourceId"], "42");
        assert_eq!(event.payload["clientSessionId"], "play-1");
        assert_eq!(event.payload["playbackSessionId"], "session-1");
        assert_eq!(event.payload["positionTicks"], 120_000);
        assert_eq!(event.payload["playMethod"], "direct_stream");
    }

    #[test]
    fn low_requested_bitrate_queues_transcode_only_when_source_is_higher() {
        let source = PlaybackMediaSourceRecord {
            bitrate: Some(12_000_000),
            supports_transcoding: true,
            ..test_source()
        };

        assert!(should_queue_transcode(
            &source,
            &PlaybackInfoRequestDto {
                user_id: None,
                max_streaming_bitrate: Some(8_000_000),
                start_time_ticks: None,
                media_source_id: None,
                device_profile: None,
            }
        ));
        assert!(!should_queue_transcode(
            &source,
            &PlaybackInfoRequestDto {
                user_id: None,
                max_streaming_bitrate: Some(20_000_000),
                start_time_ticks: None,
                media_source_id: None,
                device_profile: None,
            }
        ));
    }

    #[test]
    fn transcode_media_source_exposes_emby_hls_url_fields() {
        let source = PlaybackMediaSourceRecord {
            bitrate: Some(12_000_000),
            supports_transcoding: true,
            ..test_source()
        };
        let transcode = PlaybackTranscodeInfo {
            url: transcode_url(&source, "session-1", "play-1", "token-1"),
            bitrate: Some(8_000_000),
        };
        let dto = media_source_to_dto(&source, Some(&transcode), "token-1");

        assert_eq!(dto.bitrate, Some(8_000_000));
        assert_eq!(dto.run_time_ticks, Some(7_200_000_000));
        assert_eq!(dto.size, Some(42_000_000));
        assert_eq!(dto.source_type, "Default");
        assert_eq!(dto.name, "42");
        assert_eq!(dto.item_id.as_deref(), Some("item-1"));
        assert_eq!(
            dto.direct_stream_url.as_deref(),
            Some("/emby/Videos/item-1/stream?MediaSourceId=42&Static=true&api_key=token-1")
        );
        assert!(!dto.add_api_key_to_direct_stream_url);
        assert_eq!(dto.transcoding_sub_protocol.as_deref(), Some("hls"));
        assert_eq!(dto.transcoding_container.as_deref(), Some("ts"));
        assert_eq!(
            dto.transcoding_url.as_deref(),
            Some(
                "/emby/Videos/item-1/master.m3u8?MediaSourceId=42&TranscodeSessionId=session-1&PlaySessionId=play-1&api_key=token-1"
            )
        );
    }

    #[test]
    fn media_source_exposes_default_stream_indices() {
        let source = PlaybackMediaSourceRecord {
            streams: vec![
                PlaybackMediaStreamRecord {
                    stream_index: 0,
                    stream_type: "audio".to_owned(),
                    is_default: false,
                    ..test_stream()
                },
                PlaybackMediaStreamRecord {
                    stream_index: 1,
                    stream_type: "audio".to_owned(),
                    is_default: true,
                    ..test_stream()
                },
                PlaybackMediaStreamRecord {
                    stream_index: 2,
                    stream_type: "subtitle".to_owned(),
                    is_default: false,
                    ..test_stream()
                },
            ],
            ..test_source()
        };
        let dto = media_source_to_dto(&source, None, "token-1");

        assert_eq!(dto.default_audio_stream_index, Some(1));
        assert_eq!(dto.default_subtitle_stream_index, Some(2));
    }

    #[test]
    fn audio_track_direct_stream_uses_audio_endpoint() {
        let source = PlaybackMediaSourceRecord {
            item_id: "track-1".to_owned(),
            item_type: "track".to_owned(),
            path: "song.mp3".to_owned(),
            container: Some("mp3".to_owned()),
            bitrate: Some(320_000),
            supports_transcoding: true,
            ..test_source()
        };
        let dto = media_source_to_dto(&source, None, "token-1");

        assert_eq!(
            dto.direct_stream_url.as_deref(),
            Some("/emby/Audio/track-1/stream.mp3?MediaSourceId=42&Static=true&api_key=token-1")
        );
    }

    #[test]
    fn audio_stream_file_name_ignores_unsafe_container() {
        assert_eq!(audio_stream_file_name(Some("../mp3")), "stream");
        assert_eq!(audio_stream_file_name(Some("flac")), "stream.flac");
    }

    fn test_source() -> PlaybackMediaSourceRecord {
        PlaybackMediaSourceRecord {
            media_item_id: 7,
            item_id: "item-1".to_owned(),
            item_type: "movie".to_owned(),
            media_file_id: 42,
            path: "movie.mkv".to_owned(),
            file_size: Some(42_000_000),
            is_strm: false,
            strm_target: None,
            container: Some("mkv".to_owned()),
            runtime_ticks: Some(7_200_000_000),
            bitrate: Some(12_000_000),
            supports_transcoding: true,
            streams: vec![],
        }
    }

    fn test_stream() -> PlaybackMediaStreamRecord {
        PlaybackMediaStreamRecord {
            stream_index: 0,
            stream_type: "video".to_owned(),
            codec: None,
            codec_tag: None,
            language: None,
            title: None,
            profile: None,
            level: None,
            width: None,
            height: None,
            channels: None,
            sample_rate: None,
            bit_depth: None,
            bitrate: None,
            is_default: true,
            is_forced: false,
        }
    }
}
