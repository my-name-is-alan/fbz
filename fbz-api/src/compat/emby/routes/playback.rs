use axum::{
    Json,
    body::Bytes,
    extract::{Path, Query, State},
    http::{HeaderMap, StatusCode, Uri},
};
use serde_json::{Value, json};
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
};

const PLAYBACK_STARTED_EVENT: &str = "playback.started";
const PLAYBACK_STOPPED_EVENT: &str = "playback.stopped";

pub async fn playback_info(
    State(state): State<AppState>,
    Path(item_id): Path<String>,
    Query(query): Query<PlaybackInfoRequestDto>,
    headers: HeaderMap,
    uri: Uri,
) -> Result<Json<PlaybackInfoResponseDto>, AppError> {
    let access_token = access_token_from_request(&headers, uri.query())?;
    let user = authenticate_request_user(&state, &headers, &uri).await?;
    assert_request_user(&user, query.user_id.as_deref())?;
    playback_info_for_user(&state, user, item_id, &query, &access_token).await
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
    let user = authenticate_request_user(&state, &headers, &uri).await?;
    assert_request_user(&user, payload.user_id.as_deref())?;
    playback_info_for_user(&state, user, item_id, &payload, &access_token).await
}

pub async fn user_playback_info(
    State(state): State<AppState>,
    Path((user_id, item_id)): Path<(String, String)>,
    Query(query): Query<PlaybackInfoRequestDto>,
    headers: HeaderMap,
    uri: Uri,
) -> Result<Json<PlaybackInfoResponseDto>, AppError> {
    let access_token = access_token_from_request(&headers, uri.query())?;
    let user = authenticate_route_user(&state, &user_id, &headers, &uri).await?;
    assert_request_user(&user, query.user_id.as_deref())?;
    playback_info_for_user(&state, user, item_id, &query, &access_token).await
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
    let user = authenticate_route_user(&state, &user_id, &headers, &uri).await?;
    assert_request_user(&user, payload.user_id.as_deref())?;
    playback_info_for_user(&state, user, item_id, &payload, &access_token).await
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
        "/emby/videos/{}/master.m3u8?MediaSourceId={}&TranscodeSessionId={}&PlaySessionId={}&api_key={}",
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
        path: Some(media_source_path(&source)),
        protocol: protocol.to_owned(),
        container: source.container.clone(),
        run_time_ticks: source.runtime_ticks,
        size: source.file_size,
        bitrate: transcode
            .and_then(|session| session.bitrate)
            .or(source.bitrate),
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
    }
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
        assert_eq!(dto.protocol, "Http");
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
                position_ticks: Some(-10),
                is_paused: Some(true),
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
                "/emby/videos/item-1/master.m3u8?MediaSourceId=42&TranscodeSessionId=session-1&PlaySessionId=play-1&api_key=token-1"
            )
        );
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
