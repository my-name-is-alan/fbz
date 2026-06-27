use axum::{
    Json,
    body::Bytes,
    extract::{Path, Query, State},
    http::{HeaderMap, StatusCode, Uri},
};
use serde::Deserialize;
use serde_json::Value;

use crate::{
    auth::{
        repository::{AuthRepository, SessionCapabilitiesInput, SessionInfoRecord},
        service::{AuthService, AuthenticatedUser},
    },
    compat::emby::auth::{EmbyCredential, parse_auth_context},
    compat::emby::dto::{BaseItemDto, NameIdPairDto, QueryResultDto, SessionInfoDto},
    compat::emby::payload::parse_emby_body,
    error::AppError,
    state::AppState,
};

use super::access::authenticate_request_user;

#[derive(Clone, Debug, Default, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "PascalCase")]
pub struct SessionsQuery {
    #[serde(alias = "userId", alias = "user_id")]
    pub user_id: Option<String>,
    #[serde(alias = "deviceId", alias = "device_id")]
    pub device_id: Option<String>,
}

#[derive(Clone, Debug, Default, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "PascalCase")]
pub struct PlayQueueQuery {
    #[serde(alias = "id")]
    pub id: Option<String>,
    #[serde(alias = "deviceId", alias = "device_id")]
    pub device_id: Option<String>,
}

#[derive(Clone, Debug, Default, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "PascalCase")]
pub struct AuthKeysQuery {
    #[serde(alias = "startIndex", alias = "start_index")]
    pub start_index: Option<u32>,
    #[serde(alias = "limit")]
    pub limit: Option<u32>,
}

#[derive(Clone, Debug, Default, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "PascalCase")]
pub struct CreateAuthKeyQuery {
    #[serde(alias = "app")]
    pub app: Option<String>,
}

#[derive(Clone, Debug, Default, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "PascalCase")]
pub struct SessionCapabilitiesQuery {
    #[serde(alias = "id")]
    pub id: Option<String>,
    #[serde(alias = "playableMediaTypes", alias = "playable_media_types")]
    pub playable_media_types: Option<String>,
    #[serde(alias = "supportedCommands", alias = "supported_commands")]
    pub supported_commands: Option<String>,
    #[serde(alias = "supportsMediaControl", alias = "supports_media_control")]
    pub supports_media_control: Option<bool>,
    #[serde(alias = "supportsSync", alias = "supports_sync")]
    pub supports_sync: Option<bool>,
}

#[derive(Clone, Debug, Default, Deserialize, PartialEq)]
#[serde(rename_all = "PascalCase")]
pub struct ClientCapabilitiesDto {
    #[serde(default)]
    #[serde(alias = "playableMediaTypes", alias = "playable_media_types")]
    pub playable_media_types: Vec<String>,
    #[serde(default)]
    #[serde(alias = "supportedCommands", alias = "supported_commands")]
    pub supported_commands: Vec<String>,
    #[serde(default)]
    #[serde(alias = "supportsMediaControl", alias = "supports_media_control")]
    pub supports_media_control: bool,
    #[serde(alias = "pushToken", alias = "push_token")]
    pub push_token: Option<String>,
    #[serde(alias = "pushTokenType", alias = "push_token_type")]
    pub push_token_type: Option<String>,
    #[serde(default)]
    #[serde(alias = "supportsSync", alias = "supports_sync")]
    pub supports_sync: bool,
    #[serde(alias = "deviceProfile", alias = "device_profile")]
    pub device_profile: Option<Value>,
    #[serde(alias = "iconUrl", alias = "icon_url")]
    pub icon_url: Option<String>,
    #[serde(alias = "appId", alias = "app_id")]
    pub app_id: Option<String>,
}

#[derive(Clone, Debug, Default, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "PascalCase")]
pub struct RemotePlayQuery {
    #[serde(alias = "itemIds", alias = "item_ids")]
    pub item_ids: Option<String>,
    #[serde(alias = "playCommand", alias = "play_command")]
    pub play_command: Option<String>,
    #[serde(alias = "startPositionTicks", alias = "start_position_ticks")]
    pub start_position_ticks: Option<i64>,
    #[serde(alias = "mediaSourceId", alias = "media_source_id")]
    pub media_source_id: Option<String>,
    #[serde(alias = "audioStreamIndex", alias = "audio_stream_index")]
    pub audio_stream_index: Option<i32>,
    #[serde(alias = "subtitleStreamIndex", alias = "subtitle_stream_index")]
    pub subtitle_stream_index: Option<i32>,
    #[serde(alias = "startIndex", alias = "start_index")]
    pub start_index: Option<i32>,
}

#[derive(Clone, Debug, Default, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "PascalCase")]
pub struct RemotePlayRequestDto {
    #[serde(alias = "itemIds", alias = "item_ids")]
    pub item_ids: Option<RemoteItemIdsDto>,
    #[serde(alias = "playCommand", alias = "play_command")]
    pub play_command: Option<String>,
    #[serde(alias = "startPositionTicks", alias = "start_position_ticks")]
    pub start_position_ticks: Option<i64>,
    #[serde(alias = "mediaSourceId", alias = "media_source_id")]
    pub media_source_id: Option<String>,
    #[serde(alias = "audioStreamIndex", alias = "audio_stream_index")]
    pub audio_stream_index: Option<i32>,
    #[serde(alias = "subtitleStreamIndex", alias = "subtitle_stream_index")]
    pub subtitle_stream_index: Option<i32>,
    #[serde(alias = "startIndex", alias = "start_index")]
    pub start_index: Option<i32>,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Eq)]
#[serde(untagged)]
pub enum RemoteItemIdsDto {
    Csv(String),
    List(Vec<String>),
}

#[derive(Clone, Debug, Default, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "PascalCase")]
pub struct RemotePlaystateCommandQuery {
    #[serde(alias = "seekPositionTicks", alias = "seek_position_ticks")]
    pub seek_position_ticks: Option<i64>,
}

#[derive(Clone, Debug, Default, Deserialize, PartialEq)]
#[serde(rename_all = "PascalCase")]
pub struct RemoteGeneralCommandDto {
    #[serde(alias = "name")]
    pub name: Option<String>,
    #[serde(alias = "command")]
    pub command: Option<String>,
    #[serde(alias = "arguments")]
    pub arguments: Option<Value>,
}

#[derive(Clone, Debug, Default, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "PascalCase")]
pub struct RemoteViewingDto {
    #[serde(alias = "itemId", alias = "item_id")]
    pub item_id: Option<String>,
    #[serde(alias = "itemName", alias = "item_name")]
    pub item_name: Option<String>,
    #[serde(alias = "itemType", alias = "item_type")]
    pub item_type: Option<String>,
}

#[derive(Clone, Debug, Default, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "PascalCase")]
pub struct RemoteMessageDto {
    #[serde(alias = "header")]
    pub header: Option<String>,
    #[serde(alias = "text")]
    pub text: Option<String>,
    #[serde(alias = "timeoutMs", alias = "timeout_ms")]
    pub timeout_ms: Option<i64>,
}

const MAX_SESSION_LIST_LIMIT: i64 = 100;
const MAX_CAPABILITY_VALUES: usize = 128;
const MAX_CAPABILITY_VALUE_LEN: usize = 128;
const MAX_CAPABILITY_TEXT_LEN: usize = 512;
const MAX_AUTH_KEY_LIMIT: u32 = 200;
const MAX_AUTH_KEY_START_INDEX: u32 = 10_000;
const MAX_AUTH_KEY_TEXT_LEN: usize = 128;
const MAX_REMOTE_ITEM_IDS: usize = 256;
const MAX_REMOTE_ID_LEN: usize = 128;
const MAX_REMOTE_COMMAND_LEN: usize = 64;

pub async fn auth_providers(
    State(state): State<AppState>,
    headers: HeaderMap,
    uri: Uri,
) -> Result<Json<Vec<NameIdPairDto>>, AppError> {
    authenticate_admin_user(&state, &headers, &uri).await?;

    Ok(Json(auth_provider_items()))
}

pub async fn auth_keys(
    State(state): State<AppState>,
    Query(query): Query<AuthKeysQuery>,
    headers: HeaderMap,
    uri: Uri,
) -> Result<Json<QueryResultDto<Value>>, AppError> {
    authenticate_admin_user(&state, &headers, &uri).await?;
    let input = auth_keys_query_input(&query);
    let _limit = input.limit;

    Ok(Json(empty_auth_keys_result(input.start_index)))
}

pub async fn create_auth_key(
    State(state): State<AppState>,
    Query(query): Query<CreateAuthKeyQuery>,
    headers: HeaderMap,
    uri: Uri,
) -> Result<StatusCode, AppError> {
    authenticate_admin_user(&state, &headers, &uri).await?;
    let input = create_auth_key_input(&query)?;
    let _app = input.app;

    Err(AppError::forbidden("auth key creation is not enabled"))
}

pub async fn delete_auth_key(
    State(state): State<AppState>,
    Path(key): Path<String>,
    headers: HeaderMap,
    uri: Uri,
) -> Result<StatusCode, AppError> {
    authenticate_admin_user(&state, &headers, &uri).await?;
    let _key = auth_key_path_input(&key)?;

    Ok(StatusCode::OK)
}

pub async fn list_sessions(
    State(state): State<AppState>,
    Query(query): Query<SessionsQuery>,
    headers: HeaderMap,
    uri: Uri,
) -> Result<Json<Vec<SessionInfoDto>>, AppError> {
    let authenticated = authenticate_request_user(&state, &headers, &uri).await?;
    if let Some(query_user_id) = query.user_id
        && query_user_id != authenticated.public_id
    {
        return Err(AppError::forbidden(
            "authenticated user does not match query user",
        ));
    }
    let device_id = normalize_optional_remote_id(query.device_id.as_deref())?;

    let Some(database) = state.database() else {
        return Err(AppError::internal("database is not configured"));
    };

    let sessions = AuthRepository::new(database.clone())
        .list_active_sessions_for_user(
            authenticated.id,
            MAX_SESSION_LIST_LIMIT,
            device_id.as_deref(),
        )
        .await
        .map_err(|err| AppError::internal(format!("failed to list sessions: {err}")))?
        .into_iter()
        .map(session_record_to_dto)
        .collect();

    Ok(Json(sessions))
}

pub async fn session_by_id(
    State(state): State<AppState>,
    Path(session_id): Path<String>,
    headers: HeaderMap,
    uri: Uri,
) -> Result<Json<SessionInfoDto>, AppError> {
    let authenticated = authenticate_request_user(&state, &headers, &uri).await?;
    let session_id = normalize_session_id(Some(&session_id))?;

    let Some(database) = state.database() else {
        return Err(AppError::internal("database is not configured"));
    };

    let session = AuthRepository::new(database.clone())
        .find_active_session_for_user(authenticated.id, &session_id)
        .await
        .map_err(|err| AppError::internal(format!("failed to get session: {err}")))?
        .ok_or_else(|| AppError::not_found("session not found"))?;

    Ok(Json(session_record_to_dto(session)))
}

pub async fn logout(
    State(state): State<AppState>,
    headers: HeaderMap,
    uri: Uri,
) -> Result<StatusCode, AppError> {
    let Some(database) = state.database() else {
        return Err(AppError::internal("database is not configured"));
    };

    let context = parse_auth_context(&headers, uri.query())?;
    let credential = context.require_credential()?;
    let token = match credential {
        EmbyCredential::AccessToken(token) => token,
        EmbyCredential::ApiKey(_) => {
            return Err(AppError::unauthorized(
                "session logout requires an access token",
            ));
        }
    };

    let revoked = AuthService::new(AuthRepository::new(database.clone()))
        .logout(token)
        .await
        .map_err(|err| AppError::internal(err.to_string()))?;

    Ok(logout_response(revoked))
}

pub async fn play_queue(
    State(state): State<AppState>,
    Query(query): Query<PlayQueueQuery>,
    headers: HeaderMap,
    uri: Uri,
) -> Result<Json<QueryResultDto<BaseItemDto>>, AppError> {
    authenticate_request_user(&state, &headers, &uri).await?;
    let _scope = play_queue_scope_from_query(&query)?;

    Ok(Json(empty_play_queue_result()))
}

pub async fn update_capabilities(
    State(state): State<AppState>,
    Query(query): Query<SessionCapabilitiesQuery>,
    headers: HeaderMap,
    uri: Uri,
) -> Result<StatusCode, AppError> {
    let user = authenticate_request_user(&state, &headers, &uri).await?;
    let input = capabilities_input_from_query(&user, query)?;
    persist_capabilities(&state, input).await?;

    Ok(StatusCode::OK)
}

pub async fn update_capabilities_full(
    State(state): State<AppState>,
    Query(query): Query<SessionCapabilitiesQuery>,
    headers: HeaderMap,
    uri: Uri,
    body: Bytes,
) -> Result<StatusCode, AppError> {
    let user = authenticate_request_user(&state, &headers, &uri).await?;
    let request: ClientCapabilitiesDto = parse_emby_body(&headers, &body)?;
    let input = capabilities_input_from_full(&user, query, request)?;
    persist_capabilities(&state, input).await?;

    Ok(StatusCode::OK)
}

pub async fn remote_play(
    State(state): State<AppState>,
    Path(session_id): Path<String>,
    Query(query): Query<RemotePlayQuery>,
    headers: HeaderMap,
    uri: Uri,
    body: Bytes,
) -> Result<StatusCode, AppError> {
    authenticate_request_user(&state, &headers, &uri).await?;
    let request = parse_optional_emby_body::<RemotePlayRequestDto>(&headers, &body)?;
    let _input = remote_play_input(&session_id, query, request)?;

    Ok(StatusCode::NO_CONTENT)
}

pub async fn remote_playstate_command(
    State(state): State<AppState>,
    Path((session_id, command)): Path<(String, String)>,
    Query(query): Query<RemotePlaystateCommandQuery>,
    headers: HeaderMap,
    uri: Uri,
) -> Result<StatusCode, AppError> {
    authenticate_request_user(&state, &headers, &uri).await?;
    let _input = remote_playstate_command_input(&session_id, &command, query)?;

    Ok(StatusCode::NO_CONTENT)
}

pub async fn remote_general_command(
    State(state): State<AppState>,
    Path(session_id): Path<String>,
    headers: HeaderMap,
    uri: Uri,
    body: Bytes,
) -> Result<StatusCode, AppError> {
    authenticate_request_user(&state, &headers, &uri).await?;
    let request = parse_optional_emby_body::<RemoteGeneralCommandDto>(&headers, &body)?;
    let _input = remote_general_command_input(Some(&session_id), None, request)?;

    Ok(StatusCode::NO_CONTENT)
}

pub async fn remote_general_command_without_session(
    State(state): State<AppState>,
    headers: HeaderMap,
    uri: Uri,
    body: Bytes,
) -> Result<StatusCode, AppError> {
    authenticate_request_user(&state, &headers, &uri).await?;
    let request = parse_optional_emby_body::<RemoteGeneralCommandDto>(&headers, &body)?;
    let _input = remote_general_command_input(None, None, request)?;

    Ok(StatusCode::NO_CONTENT)
}

pub async fn remote_general_command_by_name(
    State(state): State<AppState>,
    Path((session_id, command)): Path<(String, String)>,
    headers: HeaderMap,
    uri: Uri,
    body: Bytes,
) -> Result<StatusCode, AppError> {
    authenticate_request_user(&state, &headers, &uri).await?;
    let request = parse_optional_emby_body::<RemoteGeneralCommandDto>(&headers, &body)?;
    let _input = remote_general_command_input(Some(&session_id), Some(&command), request)?;

    Ok(StatusCode::NO_CONTENT)
}

pub async fn remote_general_command_by_name_without_session(
    State(state): State<AppState>,
    Path(command): Path<String>,
    headers: HeaderMap,
    uri: Uri,
    body: Bytes,
) -> Result<StatusCode, AppError> {
    authenticate_request_user(&state, &headers, &uri).await?;
    let request = parse_optional_emby_body::<RemoteGeneralCommandDto>(&headers, &body)?;
    let _input = remote_general_command_input(None, Some(&command), request)?;

    Ok(StatusCode::NO_CONTENT)
}

pub async fn remote_system_command(
    State(state): State<AppState>,
    Path((session_id, command)): Path<(String, String)>,
    headers: HeaderMap,
    uri: Uri,
) -> Result<StatusCode, AppError> {
    authenticate_request_user(&state, &headers, &uri).await?;
    let _input = remote_named_session_input(&session_id, &command)?;

    Ok(StatusCode::NO_CONTENT)
}

pub async fn remote_message(
    State(state): State<AppState>,
    Path(session_id): Path<String>,
    Query(query): Query<RemoteMessageDto>,
    headers: HeaderMap,
    uri: Uri,
    body: Bytes,
) -> Result<StatusCode, AppError> {
    authenticate_request_user(&state, &headers, &uri).await?;
    let request = parse_optional_emby_body::<RemoteMessageDto>(&headers, &body)?;
    let _message = remote_message_input(request, query);
    normalize_session_id(Some(&session_id))?;

    Ok(StatusCode::NO_CONTENT)
}

pub async fn remote_viewing(
    State(state): State<AppState>,
    Path(session_id): Path<String>,
    Query(query): Query<RemoteViewingDto>,
    headers: HeaderMap,
    uri: Uri,
    body: Bytes,
) -> Result<StatusCode, AppError> {
    authenticate_request_user(&state, &headers, &uri).await?;
    let request = parse_optional_emby_body::<RemoteViewingDto>(&headers, &body)?;
    let _viewing = remote_viewing_input(request, query);
    normalize_session_id(Some(&session_id))?;

    Ok(StatusCode::NO_CONTENT)
}

pub async fn remote_add_session_user(
    State(state): State<AppState>,
    Path((session_id, user_id)): Path<(String, String)>,
    headers: HeaderMap,
    uri: Uri,
) -> Result<StatusCode, AppError> {
    authenticate_request_user(&state, &headers, &uri).await?;
    let _input = remote_session_user_input(&session_id, &user_id)?;

    Ok(StatusCode::OK)
}

pub async fn remote_remove_session_user(
    State(state): State<AppState>,
    Path((session_id, user_id)): Path<(String, String)>,
    headers: HeaderMap,
    uri: Uri,
) -> Result<StatusCode, AppError> {
    authenticate_request_user(&state, &headers, &uri).await?;
    let _input = remote_session_user_input(&session_id, &user_id)?;

    Ok(StatusCode::OK)
}

fn session_record_to_dto(record: SessionInfoRecord) -> SessionInfoDto {
    SessionInfoDto {
        id: record.id,
        user_id: record.user_id,
        user_name: record.user_name,
        client: record.client,
        device_id: record.device_id,
        device_name: record.device_name,
        application_version: record.application_version,
        is_active: record.is_active,
    }
}

async fn persist_capabilities(
    state: &AppState,
    input: SessionCapabilitiesInput,
) -> Result<(), AppError> {
    let Some(database) = state.database() else {
        return Err(AppError::internal("database is not configured"));
    };

    let updated = AuthRepository::new(database.clone())
        .update_session_capabilities(input)
        .await
        .map_err(|err| {
            AppError::internal(format!("failed to update session capabilities: {err}"))
        })?;

    if !updated {
        return Err(AppError::not_found("session not found"));
    }

    Ok(())
}

fn logout_response(_revoked: bool) -> StatusCode {
    StatusCode::OK
}

async fn authenticate_admin_user(
    state: &AppState,
    headers: &HeaderMap,
    uri: &Uri,
) -> Result<AuthenticatedUser, AppError> {
    let user = authenticate_request_user(state, headers, uri).await?;
    if !user.can_manage_server() {
        return Err(AppError::forbidden("server management permission required"));
    }

    Ok(user)
}

fn capabilities_input_from_query(
    user: &AuthenticatedUser,
    query: SessionCapabilitiesQuery,
) -> Result<SessionCapabilitiesInput, AppError> {
    Ok(SessionCapabilitiesInput {
        user_id: user.id,
        session_id: normalize_session_id(query.id.as_deref())?,
        playable_media_types: capability_values_from_csv(query.playable_media_types.as_deref())?,
        supported_commands: capability_values_from_csv(query.supported_commands.as_deref())?,
        supports_media_control: query.supports_media_control.unwrap_or(false),
        supports_sync: query.supports_sync.unwrap_or(false),
        push_token: None,
        push_token_type: None,
        icon_url: None,
        app_id: None,
        device_profile: None,
    })
}

fn capabilities_input_from_full(
    user: &AuthenticatedUser,
    query: SessionCapabilitiesQuery,
    request: ClientCapabilitiesDto,
) -> Result<SessionCapabilitiesInput, AppError> {
    Ok(SessionCapabilitiesInput {
        user_id: user.id,
        session_id: normalize_session_id(query.id.as_deref())?,
        playable_media_types: normalize_capability_values(request.playable_media_types)?,
        supported_commands: normalize_capability_values(request.supported_commands)?,
        supports_media_control: request.supports_media_control,
        supports_sync: request.supports_sync,
        push_token: normalize_optional_text(request.push_token)?,
        push_token_type: normalize_optional_text(request.push_token_type)?,
        icon_url: normalize_optional_text(request.icon_url)?,
        app_id: normalize_optional_text(request.app_id)?,
        device_profile: request.device_profile,
    })
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct RemotePlayInput {
    session_id: String,
    item_ids: Vec<String>,
    play_command: Option<String>,
    start_position_ticks: Option<i64>,
    media_source_id: Option<String>,
    audio_stream_index: Option<i32>,
    subtitle_stream_index: Option<i32>,
    start_index: Option<i32>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct RemotePlaystateCommandInput {
    session_id: String,
    command: String,
    seek_position_ticks: Option<i64>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct RemoteSessionUserInput {
    session_id: String,
    user_id: String,
}

#[derive(Clone, Debug, PartialEq)]
struct RemoteGeneralCommandInput {
    session_id: Option<String>,
    command: Option<String>,
    arguments: Option<Value>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct PlayQueueScope {
    session_id: Option<String>,
    device_id: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct AuthKeysInput {
    start_index: u32,
    limit: u32,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct CreateAuthKeyInput {
    app: String,
}

fn auth_keys_query_input(query: &AuthKeysQuery) -> AuthKeysInput {
    AuthKeysInput {
        start_index: query.start_index.unwrap_or(0).min(MAX_AUTH_KEY_START_INDEX),
        limit: query
            .limit
            .unwrap_or(MAX_AUTH_KEY_LIMIT)
            .min(MAX_AUTH_KEY_LIMIT),
    }
}

fn empty_auth_keys_result(start_index: u32) -> QueryResultDto<Value> {
    QueryResultDto::new(Vec::new(), 0, start_index)
}

fn create_auth_key_input(query: &CreateAuthKeyQuery) -> Result<CreateAuthKeyInput, AppError> {
    Ok(CreateAuthKeyInput {
        app: normalize_required_auth_key_text(query.app.as_deref(), "App")?,
    })
}

fn auth_key_path_input(key: &str) -> Result<String, AppError> {
    normalize_required_auth_key_text(Some(key), "Key")
}

fn play_queue_scope_from_query(query: &PlayQueueQuery) -> Result<PlayQueueScope, AppError> {
    Ok(PlayQueueScope {
        session_id: query
            .id
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(|value| normalize_session_id(Some(value)))
            .transpose()?,
        device_id: normalize_optional_remote_id(query.device_id.as_deref())?,
    })
}

fn empty_play_queue_result() -> QueryResultDto<BaseItemDto> {
    QueryResultDto::new(Vec::new(), 0, 0)
}

fn remote_play_input(
    session_id: &str,
    query: RemotePlayQuery,
    request: RemotePlayRequestDto,
) -> Result<RemotePlayInput, AppError> {
    Ok(RemotePlayInput {
        session_id: normalize_session_id(Some(session_id))?,
        item_ids: normalize_remote_item_ids(
            request
                .item_ids
                .as_ref()
                .map(remote_item_ids_to_values)
                .or_else(|| query.item_ids.as_deref().map(csv_values))
                .unwrap_or_default(),
        )?,
        play_command: normalize_optional_remote_name(
            request
                .play_command
                .as_deref()
                .or(query.play_command.as_deref()),
        )?,
        start_position_ticks: non_negative_ticks(
            request.start_position_ticks.or(query.start_position_ticks),
        ),
        media_source_id: normalize_optional_remote_id(
            request
                .media_source_id
                .as_deref()
                .or(query.media_source_id.as_deref()),
        )?,
        audio_stream_index: request.audio_stream_index.or(query.audio_stream_index),
        subtitle_stream_index: request
            .subtitle_stream_index
            .or(query.subtitle_stream_index),
        start_index: request.start_index.or(query.start_index),
    })
}

fn remote_playstate_command_input(
    session_id: &str,
    command: &str,
    query: RemotePlaystateCommandQuery,
) -> Result<RemotePlaystateCommandInput, AppError> {
    let (session_id, command) = remote_named_session_input(session_id, command)?;
    Ok(RemotePlaystateCommandInput {
        session_id,
        command,
        seek_position_ticks: non_negative_ticks(query.seek_position_ticks),
    })
}

fn remote_general_command_input(
    session_id: Option<&str>,
    path_command: Option<&str>,
    request: RemoteGeneralCommandDto,
) -> Result<RemoteGeneralCommandInput, AppError> {
    Ok(RemoteGeneralCommandInput {
        session_id: session_id
            .map(|value| normalize_session_id(Some(value)))
            .transpose()?,
        command: normalize_optional_remote_name(
            path_command
                .or(request.command.as_deref())
                .or(request.name.as_deref()),
        )?,
        arguments: request.arguments,
    })
}

fn remote_message_input(body: RemoteMessageDto, query: RemoteMessageDto) -> RemoteMessageDto {
    RemoteMessageDto {
        header: body.header.or(query.header),
        text: body.text.or(query.text),
        timeout_ms: body.timeout_ms.or(query.timeout_ms),
    }
}

fn remote_viewing_input(body: RemoteViewingDto, query: RemoteViewingDto) -> RemoteViewingDto {
    RemoteViewingDto {
        item_id: body.item_id.or(query.item_id),
        item_name: body.item_name.or(query.item_name),
        item_type: body.item_type.or(query.item_type),
    }
}

fn remote_named_session_input(
    session_id: &str,
    command: &str,
) -> Result<(String, String), AppError> {
    Ok((
        normalize_session_id(Some(session_id))?,
        normalize_remote_name(command)?,
    ))
}

fn remote_session_user_input(
    session_id: &str,
    user_id: &str,
) -> Result<RemoteSessionUserInput, AppError> {
    Ok(RemoteSessionUserInput {
        session_id: normalize_session_id(Some(session_id))?,
        user_id: normalize_remote_id(user_id)?,
    })
}

fn normalize_session_id(value: Option<&str>) -> Result<String, AppError> {
    let Some(value) = value.map(str::trim).filter(|value| !value.is_empty()) else {
        return Err(AppError::unprocessable("session Id is required"));
    };

    if value.len() > 64 || !value.chars().all(|ch| ch.is_ascii_hexdigit() || ch == '-') {
        return Err(AppError::unprocessable("session Id is invalid"));
    }

    Ok(value.to_owned())
}

fn parse_optional_emby_body<T>(headers: &HeaderMap, body: &Bytes) -> Result<T, AppError>
where
    T: serde::de::DeserializeOwned + Default,
{
    if body.is_empty() {
        return Ok(T::default());
    }

    parse_emby_body(headers, body)
}

fn remote_item_ids_to_values(value: &RemoteItemIdsDto) -> Vec<String> {
    match value {
        RemoteItemIdsDto::Csv(value) => csv_values(value),
        RemoteItemIdsDto::List(values) => values.clone(),
    }
}

fn csv_values(value: &str) -> Vec<String> {
    value.split(',').map(str::to_owned).collect()
}

fn normalize_remote_item_ids(values: Vec<String>) -> Result<Vec<String>, AppError> {
    let mut normalized = Vec::new();

    for value in values {
        if value.trim().is_empty() {
            continue;
        }
        let value = normalize_remote_id(&value)?;
        if normalized.iter().all(|existing| existing != &value) {
            normalized.push(value);
        }
        if normalized.len() > MAX_REMOTE_ITEM_IDS {
            return Err(AppError::unprocessable("too many remote item ids"));
        }
    }

    Ok(normalized)
}

fn normalize_optional_remote_id(value: Option<&str>) -> Result<Option<String>, AppError> {
    let Some(value) = value.map(str::trim).filter(|value| !value.is_empty()) else {
        return Ok(None);
    };

    Ok(Some(normalize_remote_id(value)?))
}

fn normalize_remote_id(value: &str) -> Result<String, AppError> {
    let value = value.trim();
    if value.is_empty() {
        return Err(AppError::unprocessable("remote id is required"));
    }
    if value.len() > MAX_REMOTE_ID_LEN
        || !value
            .chars()
            .all(|ch| ch.is_ascii_alphanumeric() || matches!(ch, '-' | '_'))
    {
        return Err(AppError::unprocessable("remote id is invalid"));
    }

    Ok(value.to_owned())
}

fn normalize_optional_remote_name(value: Option<&str>) -> Result<Option<String>, AppError> {
    let Some(value) = value.map(str::trim).filter(|value| !value.is_empty()) else {
        return Ok(None);
    };

    Ok(Some(normalize_remote_name(value)?))
}

fn normalize_remote_name(value: &str) -> Result<String, AppError> {
    let value = value.trim();
    if value.is_empty() {
        return Err(AppError::unprocessable("remote command is required"));
    }
    if value.len() > MAX_REMOTE_COMMAND_LEN
        || !value
            .chars()
            .all(|ch| ch.is_ascii_alphanumeric() || matches!(ch, '-' | '_'))
    {
        return Err(AppError::unprocessable("remote command is invalid"));
    }

    Ok(value.to_owned())
}

fn non_negative_ticks(value: Option<i64>) -> Option<i64> {
    value.map(|ticks| ticks.max(0))
}

fn capability_values_from_csv(value: Option<&str>) -> Result<Vec<String>, AppError> {
    let values = value
        .unwrap_or_default()
        .split(',')
        .map(str::to_owned)
        .collect::<Vec<_>>();

    normalize_capability_values(values)
}

fn normalize_capability_values(values: Vec<String>) -> Result<Vec<String>, AppError> {
    let mut normalized = Vec::new();

    for value in values {
        let value = value.trim();
        if value.is_empty() {
            continue;
        }
        if value.len() > MAX_CAPABILITY_VALUE_LEN {
            return Err(AppError::unprocessable("capability value is too long"));
        }
        if normalized.iter().all(|existing| existing != value) {
            normalized.push(value.to_owned());
        }
        if normalized.len() > MAX_CAPABILITY_VALUES {
            return Err(AppError::unprocessable("too many capability values"));
        }
    }

    Ok(normalized)
}

fn normalize_optional_text(value: Option<String>) -> Result<Option<String>, AppError> {
    let Some(value) = value.map(|value| value.trim().to_owned()) else {
        return Ok(None);
    };
    if value.is_empty() {
        return Ok(None);
    }
    if value.len() > MAX_CAPABILITY_TEXT_LEN {
        return Err(AppError::unprocessable("capability text value is too long"));
    }

    Ok(Some(value))
}

fn normalize_required_auth_key_text(value: Option<&str>, field: &str) -> Result<String, AppError> {
    let Some(value) = value.map(str::trim).filter(|value| !value.is_empty()) else {
        return Err(AppError::unprocessable(format!("{field} is required")));
    };

    if value.len() > MAX_AUTH_KEY_TEXT_LEN
        || !value
            .chars()
            .all(|ch| ch.is_ascii_alphanumeric() || matches!(ch, '-' | '_' | '.'))
    {
        return Err(AppError::unprocessable(format!("{field} is invalid")));
    }

    Ok(value.to_owned())
}

fn auth_provider_items() -> Vec<NameIdPairDto> {
    vec![NameIdPairDto {
        name: "Local".to_owned(),
        id: "local".to_owned(),
    }]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn auth_providers_expose_local_name_id_pair() {
        let providers = auth_provider_items();

        assert_eq!(providers.len(), 1);
        assert_eq!(providers[0].name, "Local");
        assert_eq!(providers[0].id, "local");
    }

    #[test]
    fn auth_keys_query_normalizes_paging_and_caps_limit() {
        let input = auth_keys_query_input(&AuthKeysQuery {
            start_index: Some(25),
            limit: Some(500),
        });

        assert_eq!(input.start_index, 25);
        assert_eq!(input.limit, MAX_AUTH_KEY_LIMIT);

        let input = auth_keys_query_input(&AuthKeysQuery::default());

        assert_eq!(input.start_index, 0);
        assert_eq!(input.limit, MAX_AUTH_KEY_LIMIT);
    }

    #[test]
    fn auth_keys_query_clamps_pathologically_large_start_index() {
        let input = auth_keys_query_input(&AuthKeysQuery {
            start_index: Some(500_000),
            limit: Some(50),
        });

        assert_eq!(input.start_index, 10_000);
        assert_eq!(input.limit, 50);
    }

    #[test]
    fn auth_key_inputs_require_bounded_safe_text() {
        let input = create_auth_key_input(&CreateAuthKeyQuery {
            app: Some(" Test.Client-1 ".to_owned()),
        })
        .unwrap();

        assert_eq!(input.app, "Test.Client-1");
        assert_eq!(auth_key_path_input(" key_1 ").unwrap(), "key_1");

        let err = create_auth_key_input(&CreateAuthKeyQuery { app: None }).unwrap_err();
        assert_eq!(err.status_code(), http::StatusCode::UNPROCESSABLE_ENTITY);

        let err = auth_key_path_input("bad key!").unwrap_err();
        assert_eq!(err.status_code(), http::StatusCode::UNPROCESSABLE_ENTITY);
    }

    #[test]
    fn session_record_mapping_preserves_client_context() {
        let dto = session_record_to_dto(SessionInfoRecord {
            id: "session-1".to_owned(),
            user_id: "user-1".to_owned(),
            user_name: "alice".to_owned(),
            client: Some("Infuse".to_owned()),
            device_id: Some("device-1".to_owned()),
            device_name: Some("Apple TV".to_owned()),
            application_version: Some("8.0".to_owned()),
            is_active: true,
        });

        assert_eq!(dto.id, "session-1");
        assert_eq!(dto.user_id, "user-1");
        assert_eq!(dto.user_name, "alice");
        assert_eq!(dto.client.as_deref(), Some("Infuse"));
        assert_eq!(dto.device_id.as_deref(), Some("device-1"));
        assert_eq!(dto.device_name.as_deref(), Some("Apple TV"));
        assert_eq!(dto.application_version.as_deref(), Some("8.0"));
        assert!(dto.is_active);
    }

    #[test]
    fn sessions_query_accepts_lower_camel_and_snake_case_user_id() {
        let uri: http::Uri = "/emby/Sessions?userId=user-1".parse().unwrap();
        let Query(query) = Query::<SessionsQuery>::try_from_uri(&uri).unwrap();

        assert_eq!(query.user_id.as_deref(), Some("user-1"));

        let uri: http::Uri = "/emby/Sessions?user_id=user-2".parse().unwrap();
        let Query(query) = Query::<SessionsQuery>::try_from_uri(&uri).unwrap();

        assert_eq!(query.user_id.as_deref(), Some("user-2"));
    }

    #[test]
    fn sessions_query_accepts_lower_camel_and_snake_case_device_id() {
        let uri: http::Uri = "/emby/Sessions?deviceId=device-1".parse().unwrap();
        let Query(query) = Query::<SessionsQuery>::try_from_uri(&uri).unwrap();

        assert_eq!(query.device_id.as_deref(), Some("device-1"));

        let uri: http::Uri = "/emby/Sessions?device_id=device-2".parse().unwrap();
        let Query(query) = Query::<SessionsQuery>::try_from_uri(&uri).unwrap();

        assert_eq!(query.device_id.as_deref(), Some("device-2"));
    }

    #[test]
    fn capabilities_query_normalizes_lists_and_booleans() {
        let user = AuthenticatedUser {
            id: 42,
            public_id: "user-1".to_owned(),
            username: "alice".to_owned(),
            role_name: "user".to_owned(),
            role_name_normalized: "user".to_owned(),
        };

        let input = capabilities_input_from_query(
            &user,
            SessionCapabilitiesQuery {
                id: Some(" 00000000-0000-0000-0000-000000000001 ".to_owned()),
                playable_media_types: Some(" Audio,Video,Audio,, ".to_owned()),
                supported_commands: Some("Play,Pause".to_owned()),
                supports_media_control: Some(true),
                supports_sync: None,
            },
        )
        .unwrap();

        assert_eq!(input.user_id, 42);
        assert_eq!(input.session_id, "00000000-0000-0000-0000-000000000001");
        assert_eq!(input.playable_media_types, ["Audio", "Video"]);
        assert_eq!(input.supported_commands, ["Play", "Pause"]);
        assert!(input.supports_media_control);
        assert!(!input.supports_sync);
    }

    #[test]
    fn capabilities_query_accepts_lower_camel_client_fields() {
        let user = AuthenticatedUser {
            id: 42,
            public_id: "user-1".to_owned(),
            username: "alice".to_owned(),
            role_name: "user".to_owned(),
            role_name_normalized: "user".to_owned(),
        };
        let query = serde_json::from_value::<SessionCapabilitiesQuery>(serde_json::json!({
            "id": "00000000-0000-0000-0000-000000000001",
            "playableMediaTypes": "Audio,Video",
            "supportedCommands": "Play,Pause",
            "supportsMediaControl": true,
            "supportsSync": true
        }))
        .unwrap();

        let input = capabilities_input_from_query(&user, query).unwrap();

        assert_eq!(input.session_id, "00000000-0000-0000-0000-000000000001");
        assert_eq!(input.playable_media_types, ["Audio", "Video"]);
        assert_eq!(input.supported_commands, ["Play", "Pause"]);
        assert!(input.supports_media_control);
        assert!(input.supports_sync);
    }

    #[test]
    fn capabilities_full_normalizes_body_values() {
        let user = AuthenticatedUser {
            id: 42,
            public_id: "user-1".to_owned(),
            username: "alice".to_owned(),
            role_name: "user".to_owned(),
            role_name_normalized: "user".to_owned(),
        };

        let input = capabilities_input_from_full(
            &user,
            SessionCapabilitiesQuery {
                id: Some("00000000-0000-0000-0000-000000000001".to_owned()),
                ..SessionCapabilitiesQuery::default()
            },
            ClientCapabilitiesDto {
                playable_media_types: vec!["Video".to_owned(), " Video ".to_owned()],
                supported_commands: vec!["DisplayMessage".to_owned()],
                supports_media_control: true,
                push_token: Some(" push-token ".to_owned()),
                push_token_type: Some(" fcm ".to_owned()),
                supports_sync: true,
                device_profile: Some(serde_json::json!({"Name": "Client"})),
                icon_url: Some(" https://example.test/icon.png ".to_owned()),
                app_id: Some(" app.id ".to_owned()),
            },
        )
        .unwrap();

        assert_eq!(input.playable_media_types, ["Video"]);
        assert_eq!(input.supported_commands, ["DisplayMessage"]);
        assert_eq!(input.push_token.as_deref(), Some("push-token"));
        assert_eq!(input.push_token_type.as_deref(), Some("fcm"));
        assert_eq!(
            input.device_profile.as_ref().unwrap()["Name"],
            serde_json::json!("Client")
        );
        assert!(input.supports_sync);
    }

    #[test]
    fn capabilities_full_accepts_lower_camel_client_fields() {
        let user = AuthenticatedUser {
            id: 42,
            public_id: "user-1".to_owned(),
            username: "alice".to_owned(),
            role_name: "user".to_owned(),
            role_name_normalized: "user".to_owned(),
        };
        let request = serde_json::from_value::<ClientCapabilitiesDto>(serde_json::json!({
            "playableMediaTypes": ["Audio", "Video"],
            "supportedCommands": ["Play", "Pause"],
            "supportsMediaControl": true,
            "pushToken": " push-token ",
            "pushTokenType": " fcm ",
            "supportsSync": true,
            "deviceProfile": {"Name": "Client"},
            "iconUrl": " https://example.test/icon.png ",
            "appId": " app.id "
        }))
        .unwrap();

        let input = capabilities_input_from_full(
            &user,
            SessionCapabilitiesQuery {
                id: Some("00000000-0000-0000-0000-000000000001".to_owned()),
                ..SessionCapabilitiesQuery::default()
            },
            request,
        )
        .unwrap();

        assert_eq!(input.playable_media_types, ["Audio", "Video"]);
        assert_eq!(input.supported_commands, ["Play", "Pause"]);
        assert!(input.supports_media_control);
        assert!(input.supports_sync);
        assert_eq!(input.push_token.as_deref(), Some("push-token"));
        assert_eq!(input.push_token_type.as_deref(), Some("fcm"));
        assert_eq!(
            input.icon_url.as_deref(),
            Some("https://example.test/icon.png")
        );
        assert_eq!(input.app_id.as_deref(), Some("app.id"));
        assert_eq!(
            input.device_profile.as_ref().unwrap()["Name"],
            serde_json::json!("Client")
        );
    }

    #[test]
    fn capabilities_reject_unbounded_values() {
        let long_value = "a".repeat(MAX_CAPABILITY_VALUE_LEN + 1);
        let err = normalize_capability_values(vec![long_value]).unwrap_err();

        assert_eq!(err.status_code(), http::StatusCode::UNPROCESSABLE_ENTITY);

        let err = normalize_session_id(Some("not a uuid")).unwrap_err();

        assert_eq!(err.status_code(), http::StatusCode::UNPROCESSABLE_ENTITY);
    }

    #[test]
    fn session_id_path_normalization_trims_uuid() {
        let session_id =
            normalize_session_id(Some(" 00000000-0000-0000-0000-000000000001 ")).unwrap();

        assert_eq!(session_id, "00000000-0000-0000-0000-000000000001");
    }

    #[test]
    fn logout_response_is_empty_success_status() {
        assert_eq!(logout_response(true), StatusCode::OK);
        assert_eq!(logout_response(false), StatusCode::OK);
    }

    #[test]
    fn remote_session_user_input_normalizes_session_and_user_ids() {
        let input = remote_session_user_input(" 00000000-0000-0000-0000-000000000001 ", " user-2 ")
            .unwrap();

        assert_eq!(input.session_id, "00000000-0000-0000-0000-000000000001");
        assert_eq!(input.user_id, "user-2");

        let err = remote_session_user_input("00000000-0000-0000-0000-000000000001", "bad user!")
            .unwrap_err();

        assert_eq!(err.status_code(), http::StatusCode::UNPROCESSABLE_ENTITY);
    }

    #[test]
    fn remote_play_input_merges_query_and_body_values() {
        let input = remote_play_input(
            " 00000000-0000-0000-0000-000000000001 ",
            RemotePlayQuery {
                item_ids: Some(" song-1,song-2,,song-1 ".to_owned()),
                play_command: Some("PlayNext".to_owned()),
                start_position_ticks: Some(-50),
                media_source_id: Some(" 42 ".to_owned()),
                audio_stream_index: Some(1),
                subtitle_stream_index: Some(-1),
                start_index: Some(2),
            },
            RemotePlayRequestDto::default(),
        )
        .unwrap();

        assert_eq!(input.session_id, "00000000-0000-0000-0000-000000000001");
        assert_eq!(input.item_ids, ["song-1", "song-2"]);
        assert_eq!(input.play_command.as_deref(), Some("PlayNext"));
        assert_eq!(input.start_position_ticks, Some(0));
        assert_eq!(input.media_source_id.as_deref(), Some("42"));
        assert_eq!(input.audio_stream_index, Some(1));
        assert_eq!(input.subtitle_stream_index, Some(-1));
        assert_eq!(input.start_index, Some(2));
    }

    #[test]
    fn remote_play_query_accepts_lower_camel_client_fields() {
        let query = serde_json::from_value::<RemotePlayQuery>(serde_json::json!({
            "itemIds": "song-1,song-2",
            "playCommand": "PlayNow",
            "startPositionTicks": 42,
            "mediaSourceId": "source-1",
            "audioStreamIndex": 1,
            "subtitleStreamIndex": -1,
            "startIndex": 2
        }))
        .unwrap();

        let input = remote_play_input(
            "00000000-0000-0000-0000-000000000001",
            query,
            RemotePlayRequestDto::default(),
        )
        .unwrap();

        assert_eq!(input.item_ids, ["song-1", "song-2"]);
        assert_eq!(input.play_command.as_deref(), Some("PlayNow"));
        assert_eq!(input.start_position_ticks, Some(42));
        assert_eq!(input.media_source_id.as_deref(), Some("source-1"));
        assert_eq!(input.audio_stream_index, Some(1));
        assert_eq!(input.subtitle_stream_index, Some(-1));
        assert_eq!(input.start_index, Some(2));
    }

    #[test]
    fn remote_play_body_accepts_lower_camel_client_fields() {
        let request = serde_json::from_value::<RemotePlayRequestDto>(serde_json::json!({
            "itemIds": ["song-1", "song-2"],
            "playCommand": "PlayNext",
            "startPositionTicks": 42,
            "mediaSourceId": "source-1",
            "audioStreamIndex": 1,
            "subtitleStreamIndex": -1,
            "startIndex": 2
        }))
        .unwrap();

        let input = remote_play_input(
            "00000000-0000-0000-0000-000000000001",
            RemotePlayQuery::default(),
            request,
        )
        .unwrap();

        assert_eq!(input.item_ids, ["song-1", "song-2"]);
        assert_eq!(input.play_command.as_deref(), Some("PlayNext"));
        assert_eq!(input.start_position_ticks, Some(42));
        assert_eq!(input.media_source_id.as_deref(), Some("source-1"));
        assert_eq!(input.audio_stream_index, Some(1));
        assert_eq!(input.subtitle_stream_index, Some(-1));
        assert_eq!(input.start_index, Some(2));
    }

    #[test]
    fn remote_playstate_command_input_normalizes_seek_ticks() {
        let input = remote_playstate_command_input(
            "00000000-0000-0000-0000-000000000001",
            " Seek ",
            RemotePlaystateCommandQuery {
                seek_position_ticks: Some(-10),
            },
        )
        .unwrap();

        assert_eq!(input.session_id, "00000000-0000-0000-0000-000000000001");
        assert_eq!(input.command, "Seek");
        assert_eq!(input.seek_position_ticks, Some(0));
    }

    #[test]
    fn remote_playstate_query_accepts_lower_camel_seek_position_ticks() {
        let query = serde_json::from_value::<RemotePlaystateCommandQuery>(serde_json::json!({
            "seekPositionTicks": 42
        }))
        .unwrap();

        let input =
            remote_playstate_command_input("00000000-0000-0000-0000-000000000001", "Seek", query)
                .unwrap();

        assert_eq!(input.seek_position_ticks, Some(42));
    }

    #[test]
    fn remote_general_command_input_accepts_no_session_alias() {
        let input = remote_general_command_input(
            None,
            Some(" DisplayMessage "),
            RemoteGeneralCommandDto {
                arguments: Some(serde_json::json!({"Header": "Now playing"})),
                ..RemoteGeneralCommandDto::default()
            },
        )
        .unwrap();

        assert_eq!(input.session_id, None);
        assert_eq!(input.command.as_deref(), Some("DisplayMessage"));
        assert_eq!(
            input.arguments.as_ref().unwrap()["Header"],
            serde_json::json!("Now playing")
        );
    }

    #[test]
    fn remote_general_command_body_accepts_lower_camel_client_fields() {
        let request = serde_json::from_value::<RemoteGeneralCommandDto>(serde_json::json!({
            "command": "DisplayMessage",
            "arguments": {"Header": "Now playing"}
        }))
        .unwrap();

        let input = remote_general_command_input(None, None, request).unwrap();

        assert_eq!(input.command.as_deref(), Some("DisplayMessage"));
        assert_eq!(
            input.arguments.as_ref().unwrap()["Header"],
            serde_json::json!("Now playing")
        );
    }

    #[test]
    fn remote_message_body_accepts_lower_camel_client_fields() {
        let message = serde_json::from_value::<RemoteMessageDto>(serde_json::json!({
            "header": "Playback",
            "text": "Paused",
            "timeoutMs": 1500
        }))
        .unwrap();

        assert_eq!(message.header.as_deref(), Some("Playback"));
        assert_eq!(message.text.as_deref(), Some("Paused"));
        assert_eq!(message.timeout_ms, Some(1500));
    }

    #[test]
    fn remote_message_input_accepts_query_only_fields() {
        let query = serde_json::from_value::<RemoteMessageDto>(serde_json::json!({
            "header": "Playback",
            "text": "Paused",
            "timeoutMs": 1500
        }))
        .unwrap();

        let input = remote_message_input(RemoteMessageDto::default(), query);

        assert_eq!(input.header.as_deref(), Some("Playback"));
        assert_eq!(input.text.as_deref(), Some("Paused"));
        assert_eq!(input.timeout_ms, Some(1500));
    }

    #[test]
    fn remote_message_input_preserves_body_fields_over_query() {
        let body = serde_json::from_value::<RemoteMessageDto>(serde_json::json!({
            "header": "Playback",
            "text": "Paused"
        }))
        .unwrap();
        let query = serde_json::from_value::<RemoteMessageDto>(serde_json::json!({
            "header": "Other",
            "text": "Other",
            "timeoutMs": 1500
        }))
        .unwrap();

        let input = remote_message_input(body, query);

        assert_eq!(input.header.as_deref(), Some("Playback"));
        assert_eq!(input.text.as_deref(), Some("Paused"));
        assert_eq!(input.timeout_ms, Some(1500));
    }

    #[test]
    fn remote_viewing_body_accepts_lower_camel_client_fields() {
        let viewing = serde_json::from_value::<RemoteViewingDto>(serde_json::json!({
            "itemId": "item-1",
            "itemName": "Movie",
            "itemType": "Movie"
        }))
        .unwrap();

        assert_eq!(viewing.item_id.as_deref(), Some("item-1"));
        assert_eq!(viewing.item_name.as_deref(), Some("Movie"));
        assert_eq!(viewing.item_type.as_deref(), Some("Movie"));
    }

    #[test]
    fn remote_viewing_input_accepts_query_only_fields() {
        let query = serde_json::from_value::<RemoteViewingDto>(serde_json::json!({
            "itemId": "item-1",
            "itemName": "Movie",
            "itemType": "Movie"
        }))
        .unwrap();

        let input = remote_viewing_input(RemoteViewingDto::default(), query);

        assert_eq!(input.item_id.as_deref(), Some("item-1"));
        assert_eq!(input.item_name.as_deref(), Some("Movie"));
        assert_eq!(input.item_type.as_deref(), Some("Movie"));
    }

    #[test]
    fn remote_viewing_input_preserves_body_fields_over_query() {
        let body = serde_json::from_value::<RemoteViewingDto>(serde_json::json!({
            "itemId": "item-1",
            "itemName": "Movie"
        }))
        .unwrap();
        let query = serde_json::from_value::<RemoteViewingDto>(serde_json::json!({
            "itemId": "other-item",
            "itemName": "Other",
            "itemType": "Movie"
        }))
        .unwrap();

        let input = remote_viewing_input(body, query);

        assert_eq!(input.item_id.as_deref(), Some("item-1"));
        assert_eq!(input.item_name.as_deref(), Some("Movie"));
        assert_eq!(input.item_type.as_deref(), Some("Movie"));
    }

    #[test]
    fn play_queue_query_normalizes_optional_session_and_device_scope() {
        let scope = play_queue_scope_from_query(&PlayQueueQuery {
            id: Some(" 00000000-0000-0000-0000-000000000001 ".to_owned()),
            device_id: Some(" device-1 ".to_owned()),
        })
        .unwrap();

        assert_eq!(
            scope.session_id.as_deref(),
            Some("00000000-0000-0000-0000-000000000001")
        );
        assert_eq!(scope.device_id.as_deref(), Some("device-1"));

        let scope = play_queue_scope_from_query(&PlayQueueQuery {
            id: None,
            device_id: Some(" ".to_owned()),
        })
        .unwrap();

        assert_eq!(scope.session_id, None);
        assert_eq!(scope.device_id, None);
    }

    #[test]
    fn play_queue_query_accepts_lower_camel_and_snake_case_client_fields() {
        let uri: http::Uri =
            "/emby/Sessions/PlayQueue?id=00000000-0000-0000-0000-000000000001&deviceId=device-1"
                .parse()
                .unwrap();
        let Query(query) = Query::<PlayQueueQuery>::try_from_uri(&uri).unwrap();
        let scope = play_queue_scope_from_query(&query).unwrap();

        assert_eq!(
            scope.session_id.as_deref(),
            Some("00000000-0000-0000-0000-000000000001")
        );
        assert_eq!(scope.device_id.as_deref(), Some("device-1"));

        let uri: http::Uri =
            "/emby/Sessions/PlayQueue?id=00000000-0000-0000-0000-000000000002&device_id=device-2"
                .parse()
                .unwrap();
        let Query(query) = Query::<PlayQueueQuery>::try_from_uri(&uri).unwrap();
        let scope = play_queue_scope_from_query(&query).unwrap();

        assert_eq!(
            scope.session_id.as_deref(),
            Some("00000000-0000-0000-0000-000000000002")
        );
        assert_eq!(scope.device_id.as_deref(), Some("device-2"));
    }
}
