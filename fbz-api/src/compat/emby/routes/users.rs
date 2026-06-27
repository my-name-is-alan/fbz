use axum::{
    Json,
    body::Bytes,
    extract::{Path, Query, State},
    http::{HeaderMap, Uri},
};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::collections::BTreeSet;
use tracing::warn;

use crate::{
    auth::{
        repository::AuthRepository,
        service::{AuthService, AuthServiceError, LoginByUserIdInput, LoginInput, LoginOutput},
    },
    compat::emby::{
        auth::parse_auth_context,
        dto::{
            AuthenticateByNameRequestDto, AuthenticateUserRequestDto, AuthenticationResultDto,
            NameIdPairDto, PublicUserDto, QueryResultDto, SessionInfoDto, UserDetailSource,
            UserDto, UserSource,
        },
        payload::parse_emby_body,
    },
    db::DbPool,
    error::AppError,
    plugins::hooks::{PluginHookDispatcher, PluginHookEvent},
    state::AppState,
    users::repository::{PublicUserRecord, UserDetailRecord, UsersQueryFilter, UsersRepository},
};

use super::access::authenticate_request_user;

const USER_LOGIN_EVENT: &str = "user.login";
const DEFAULT_USERS_QUERY_LIMIT: i64 = 100;
const MAX_USERS_QUERY_LIMIT: i64 = 100;
const MAX_USERS_QUERY_START_INDEX: i64 = 10_000;
const MAX_USERS_QUERY_TEXT_LEN: usize = 128;
const MAX_EMBY_USER_WRITE_BODY_BYTES: usize = 64 * 1024;

#[derive(Clone, Debug, Default, serde::Deserialize, PartialEq, Eq)]
#[serde(rename_all = "PascalCase")]
pub struct UsersQuery {
    #[serde(alias = "isHidden", alias = "is_hidden")]
    pub is_hidden: Option<bool>,
    #[serde(alias = "isDisabled", alias = "is_disabled")]
    pub is_disabled: Option<bool>,
    #[serde(alias = "startIndex", alias = "start_index")]
    pub start_index: Option<i64>,
    #[serde(alias = "limit")]
    pub limit: Option<i64>,
    #[serde(
        alias = "nameStartsWithOrGreater",
        alias = "name_starts_with_or_greater"
    )]
    pub name_starts_with_or_greater: Option<String>,
    #[serde(alias = "sortOrder", alias = "sort_order")]
    pub sort_order: Option<String>,
}

#[derive(Clone, Debug, Default, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "PascalCase")]
pub struct CreateUserByNameDto {
    #[serde(alias = "name")]
    pub name: Option<String>,
    #[serde(alias = "copyFromUserId", alias = "copy_from_user_id")]
    pub copy_from_user_id: Option<String>,
    #[serde(alias = "userCopyOptions", alias = "user_copy_options")]
    pub user_copy_options: Option<Vec<String>>,
}

#[derive(Clone, Debug, Default, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "PascalCase")]
pub struct UpdateUserPasswordDto {
    #[serde(alias = "id")]
    pub id: Option<String>,
    #[serde(alias = "newPw", alias = "new_pw")]
    pub new_pw: Option<String>,
    #[serde(alias = "resetPassword", alias = "reset_password")]
    pub reset_password: Option<bool>,
}

#[derive(Clone, Debug, Default, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "PascalCase")]
pub struct ForgotPasswordRequestDto {
    #[serde(alias = "enteredUsername", alias = "entered_username")]
    pub entered_username: Option<String>,
}

#[derive(Clone, Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "PascalCase")]
pub struct ForgotPasswordResultDto {
    pub action: String,
    pub pin_file: Option<String>,
    pub pin_expiration_date: Option<String>,
}

#[derive(Clone, Debug, Default, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "PascalCase")]
pub struct ForgotPasswordPinDto {
    #[serde(alias = "pin")]
    pub pin: Option<String>,
}

#[derive(Clone, Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "PascalCase")]
pub struct PinRedeemResultDto {
    pub success: bool,
    pub users_reset: Vec<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct ForgotPasswordInput {
    entered_username: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct ForgotPasswordPinInput {
    pin: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct CreateUserInput {
    name: String,
    copy_from_user_id: Option<String>,
    user_copy_options: Vec<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct PasswordUpdateInput {
    user_id: String,
    new_password: Option<String>,
    reset_password: bool,
}

pub async fn public_users(
    State(state): State<AppState>,
) -> Result<Json<Vec<PublicUserDto>>, AppError> {
    let Some(database) = state.database() else {
        return Ok(Json(Vec::new()));
    };

    let users = UsersRepository::new(database.clone())
        .list_public_users()
        .await
        .map_err(|err| AppError::internal(format!("failed to list public users: {err}")))?
        .into_iter()
        .map(public_user_record_to_dto)
        .collect();

    Ok(Json(users))
}

pub async fn current_user(
    State(state): State<AppState>,
    headers: HeaderMap,
    uri: Uri,
) -> Result<Json<UserDto>, AppError> {
    let authenticated = authenticate_request_user(&state, &headers, &uri).await?;
    user_by_id_for_authenticated(
        &state,
        &authenticated.public_id,
        authenticated.can_manage_server(),
        &authenticated.public_id,
    )
    .await
}

pub async fn users_query(
    State(state): State<AppState>,
    Query(query): Query<UsersQuery>,
    headers: HeaderMap,
    uri: Uri,
) -> Result<Json<QueryResultDto<UserDto>>, AppError> {
    let authenticated = authenticate_request_user(&state, &headers, &uri).await?;
    if !authenticated.can_manage_server() {
        return Err(AppError::forbidden(
            "authenticated user cannot query server users",
        ));
    }

    let input = users_query_input(query);
    let Some(database) = state.database() else {
        return Err(AppError::internal("database is not configured"));
    };

    let page = UsersRepository::new(database.clone())
        .list_users_query(input.clone())
        .await
        .map_err(|err| AppError::internal(format!("failed to query users: {err}")))?;
    let items = page
        .records
        .into_iter()
        .map(user_detail_record_to_dto)
        .collect();

    Ok(Json(QueryResultDto::new(
        items,
        u32::try_from(page.total_record_count).unwrap_or(u32::MAX),
        u32::try_from(input.start_index).unwrap_or(u32::MAX),
    )))
}

pub async fn create_user(
    State(state): State<AppState>,
    headers: HeaderMap,
    uri: Uri,
    body: Bytes,
) -> Result<(), AppError> {
    authenticate_admin_user(&state, &headers, &uri).await?;
    ensure_user_write_body_size(&body)?;
    let request: CreateUserByNameDto = parse_emby_body(&headers, &body)?;
    let _input = create_user_input(request)?;

    Err(user_mutation_disabled_error())
}

pub async fn user_prefixes(
    State(state): State<AppState>,
    Query(query): Query<UsersQuery>,
    headers: HeaderMap,
    uri: Uri,
) -> Result<Json<Vec<NameIdPairDto>>, AppError> {
    let authenticated = authenticate_request_user(&state, &headers, &uri).await?;
    if !authenticated.can_manage_server() {
        return Err(AppError::forbidden(
            "authenticated user cannot query server user prefixes",
        ));
    }

    let input = users_query_input(query);
    let Some(database) = state.database() else {
        return Err(AppError::internal("database is not configured"));
    };

    let page = UsersRepository::new(database.clone())
        .list_users_query(input)
        .await
        .map_err(|err| AppError::internal(format!("failed to query user prefixes: {err}")))?;

    Ok(Json(user_prefix_items(page.records)))
}

pub async fn user_by_id(
    State(state): State<AppState>,
    Path(user_id): Path<String>,
    headers: HeaderMap,
    uri: Uri,
) -> Result<Json<UserDto>, AppError> {
    let authenticated = authenticate_request_user(&state, &headers, &uri).await?;
    user_by_id_for_authenticated(
        &state,
        &user_id,
        authenticated.can_manage_server(),
        &authenticated.public_id,
    )
    .await
}

pub async fn update_user(
    State(state): State<AppState>,
    Path(user_id): Path<String>,
    headers: HeaderMap,
    uri: Uri,
    body: Bytes,
) -> Result<(), AppError> {
    authenticate_user_write_target(&state, &user_id, &headers, &uri).await?;
    ensure_user_write_body_size(&body)?;
    let _user_id = normalized_required_user_text("Id", Some(user_id))?;
    let _payload = parse_user_management_generic_body(&headers, &body)?;

    Err(user_mutation_disabled_error())
}

pub async fn update_user_configuration(
    State(state): State<AppState>,
    Path(user_id): Path<String>,
    headers: HeaderMap,
    uri: Uri,
    body: Bytes,
) -> Result<(), AppError> {
    authenticate_user_write_target(&state, &user_id, &headers, &uri).await?;
    ensure_user_write_body_size(&body)?;
    let _user_id = normalized_required_user_text("Id", Some(user_id))?;
    let _payload = parse_user_management_generic_body(&headers, &body)?;

    Err(user_mutation_disabled_error())
}

pub async fn update_user_configuration_partial(
    State(state): State<AppState>,
    Path(user_id): Path<String>,
    headers: HeaderMap,
    uri: Uri,
    body: Bytes,
) -> Result<(), AppError> {
    authenticate_user_write_target(&state, &user_id, &headers, &uri).await?;
    ensure_user_write_body_size(&body)?;
    let _user_id = normalized_required_user_text("Id", Some(user_id))?;
    let _payload = parse_user_management_generic_body(&headers, &body)?;

    Err(user_mutation_disabled_error())
}

pub async fn update_user_policy(
    State(state): State<AppState>,
    Path(user_id): Path<String>,
    headers: HeaderMap,
    uri: Uri,
    body: Bytes,
) -> Result<(), AppError> {
    authenticate_admin_user(&state, &headers, &uri).await?;
    ensure_user_write_body_size(&body)?;
    let _user_id = normalized_required_user_text("Id", Some(user_id))?;
    let _payload = parse_user_management_generic_body(&headers, &body)?;

    Err(user_mutation_disabled_error())
}

pub async fn update_user_password(
    State(state): State<AppState>,
    Path(user_id): Path<String>,
    headers: HeaderMap,
    uri: Uri,
    body: Bytes,
) -> Result<(), AppError> {
    authenticate_user_write_target(&state, &user_id, &headers, &uri).await?;
    ensure_user_write_body_size(&body)?;
    let request: UpdateUserPasswordDto = parse_emby_body(&headers, &body)?;
    let _input = password_update_input(&user_id, request)?;

    Err(user_mutation_disabled_error())
}

pub async fn update_easy_password(
    State(state): State<AppState>,
    Path(user_id): Path<String>,
    headers: HeaderMap,
    uri: Uri,
    body: Bytes,
) -> Result<(), AppError> {
    authenticate_user_write_target(&state, &user_id, &headers, &uri).await?;
    ensure_user_write_body_size(&body)?;
    let request: UpdateUserPasswordDto = parse_emby_body(&headers, &body)?;
    let _input = easy_password_update_input(&user_id, request)?;

    Err(user_mutation_disabled_error())
}

pub async fn delete_user(
    State(state): State<AppState>,
    Path(user_id): Path<String>,
    headers: HeaderMap,
    uri: Uri,
) -> Result<(), AppError> {
    authenticate_admin_user(&state, &headers, &uri).await?;
    let _user_id = normalized_required_user_text("Id", Some(user_id))?;

    Err(user_mutation_disabled_error())
}

pub async fn authenticate_by_name(
    State(state): State<AppState>,
    headers: HeaderMap,
    uri: Uri,
    body: Bytes,
) -> Result<Json<AuthenticationResultDto>, AppError> {
    let request: AuthenticateByNameRequestDto = parse_emby_body(&headers, &body)?;
    let Some(database) = state.database() else {
        return Err(AppError::internal("database is not configured"));
    };

    let password = request
        .password()
        .ok_or_else(|| AppError::unprocessable("password is required"))?
        .to_owned();
    let client = parse_auth_context(&headers, uri.query())?.client;
    let service = AuthService::new(AuthRepository::new(database.clone()));
    let output = service
        .authenticate_by_name(LoginInput {
            username: request.username,
            password,
            client: client.client,
            device: client.device,
            device_id: client.device_id,
            version: client.version,
        })
        .await
        .map_err(auth_service_error_to_app_error)?;

    dispatch_login_hook(database, &output).await;

    Ok(Json(authentication_result_to_dto(output)))
}

pub async fn authenticate_by_user_id(
    State(state): State<AppState>,
    Path(user_id): Path<String>,
    headers: HeaderMap,
    uri: Uri,
    body: Bytes,
) -> Result<Json<AuthenticationResultDto>, AppError> {
    let request: AuthenticateUserRequestDto = parse_emby_body(&headers, &body)?;
    let Some(database) = state.database() else {
        return Err(AppError::internal("database is not configured"));
    };

    let password = request
        .password()
        .ok_or_else(|| AppError::unprocessable("password is required"))?
        .to_owned();
    let client = parse_auth_context(&headers, uri.query())?.client;
    let service = AuthService::new(AuthRepository::new(database.clone()));
    let output = service
        .authenticate_by_user_id(LoginByUserIdInput {
            user_id,
            password,
            client: client.client,
            device: client.device,
            device_id: client.device_id,
            version: client.version,
        })
        .await
        .map_err(auth_service_error_to_app_error)?;

    dispatch_login_hook(database, &output).await;

    Ok(Json(authentication_result_to_dto(output)))
}

pub async fn forgot_password(
    headers: HeaderMap,
    body: Bytes,
) -> Result<Json<ForgotPasswordResultDto>, AppError> {
    let request: ForgotPasswordRequestDto = parse_emby_body(&headers, &body)?;
    let _input = forgot_password_request_input(request)?;

    Ok(Json(forgot_password_result()))
}

pub async fn forgot_password_pin(
    headers: HeaderMap,
    body: Bytes,
) -> Result<Json<PinRedeemResultDto>, AppError> {
    let request: ForgotPasswordPinDto = parse_emby_body(&headers, &body)?;
    let _input = forgot_password_pin_input(request)?;

    Ok(Json(forgot_password_pin_redeem_result()))
}

async fn user_by_id_for_authenticated(
    state: &AppState,
    requested_user_id: &str,
    can_manage_server: bool,
    authenticated_user_id: &str,
) -> Result<Json<UserDto>, AppError> {
    if requested_user_id != authenticated_user_id && !can_manage_server {
        return Err(AppError::forbidden(
            "authenticated user does not match requested user",
        ));
    }

    let Some(database) = state.database() else {
        return Err(AppError::internal("database is not configured"));
    };

    let Some(record) = UsersRepository::new(database.clone())
        .find_user_by_public_id(requested_user_id)
        .await
        .map_err(|err| AppError::internal(format!("failed to get user: {err}")))?
    else {
        return Err(AppError::not_found("user not found"));
    };

    Ok(Json(user_detail_record_to_dto(record)))
}

fn public_user_record_to_dto(record: PublicUserRecord) -> PublicUserDto {
    PublicUserDto::from(UserSource {
        id: record.id,
        name: record.name,
        has_password: record.has_password,
    })
}

fn user_detail_record_to_dto(record: UserDetailRecord) -> UserDto {
    UserDto::from(UserDetailSource {
        id: record.id,
        name: record.name,
        has_password: record.has_password,
        is_administrator: record.is_administrator,
        is_disabled: record.is_disabled,
        allow_download: record.allow_download,
        allow_transcode: record.allow_transcode,
        allow_new_device_login: record.allow_new_device_login,
        enable_content_downloading: record.enable_content_downloading,
        enable_playback_transcoding: record.enable_playback_transcoding,
        enable_all_folders: record.enable_all_folders,
        enabled_folders: record.enabled_folders,
    })
}

fn authentication_result_to_dto(output: LoginOutput) -> AuthenticationResultDto {
    let user = PublicUserDto::from(UserSource {
        id: output.user_id.clone(),
        name: output.username.clone(),
        has_password: true,
    });

    AuthenticationResultDto {
        user,
        session_info: SessionInfoDto {
            id: output.session_id,
            user_id: output.user_id,
            user_name: output.username,
            client: output.client,
            device_id: output.device_id,
            device_name: output.device_name,
            application_version: output.version,
            is_active: true,
        },
        access_token: output.access_token,
        server_id: "fbz-api".to_owned(),
    }
}

async fn authenticate_admin_user(
    state: &AppState,
    headers: &HeaderMap,
    uri: &Uri,
) -> Result<(), AppError> {
    let user = authenticate_request_user(state, headers, uri).await?;
    if !user.can_manage_server() {
        return Err(AppError::forbidden("server management permission required"));
    }

    Ok(())
}

async fn authenticate_user_write_target(
    state: &AppState,
    requested_user_id: &str,
    headers: &HeaderMap,
    uri: &Uri,
) -> Result<(), AppError> {
    let authenticated = authenticate_request_user(state, headers, uri).await?;
    let requested_user_id =
        normalized_required_user_text("Id", Some(requested_user_id.to_owned()))?;
    if authenticated.public_id != requested_user_id && !authenticated.can_manage_server() {
        return Err(AppError::forbidden(
            "authenticated user does not match requested user",
        ));
    }

    Ok(())
}

fn create_user_input(request: CreateUserByNameDto) -> Result<CreateUserInput, AppError> {
    let name = normalized_required_user_text("Name", request.name)?;
    let copy_from_user_id = normalized_user_query_text(request.copy_from_user_id);
    let user_copy_options = request
        .user_copy_options
        .unwrap_or_default()
        .into_iter()
        .filter_map(|option| normalized_user_query_text(Some(option)))
        .collect();

    Ok(CreateUserInput {
        name,
        copy_from_user_id,
        user_copy_options,
    })
}

fn password_update_input(
    path_user_id: &str,
    request: UpdateUserPasswordDto,
) -> Result<PasswordUpdateInput, AppError> {
    let user_id = normalized_required_user_text("Id", Some(path_user_id.to_owned()))?;
    if let Some(body_user_id) = normalized_user_query_text(request.id)
        && body_user_id != user_id
    {
        return Err(AppError::forbidden("body Id does not match requested user"));
    }

    let reset_password = request.reset_password.unwrap_or(false);
    let new_password = normalized_user_query_text(request.new_pw);
    if !reset_password && new_password.is_none() {
        return Err(AppError::unprocessable(
            "NewPw is required unless ResetPassword is true",
        ));
    }

    Ok(PasswordUpdateInput {
        user_id,
        new_password,
        reset_password,
    })
}

fn easy_password_update_input(
    path_user_id: &str,
    request: UpdateUserPasswordDto,
) -> Result<PasswordUpdateInput, AppError> {
    password_update_input(path_user_id, request)
}

fn ensure_user_write_body_size(body: &Bytes) -> Result<(), AppError> {
    if body.len() > MAX_EMBY_USER_WRITE_BODY_BYTES {
        return Err(AppError::unprocessable(format!(
            "user management payload must be at most {MAX_EMBY_USER_WRITE_BODY_BYTES} bytes"
        )));
    }

    Ok(())
}

fn parse_user_management_generic_body(
    headers: &HeaderMap,
    body: &Bytes,
) -> Result<Value, AppError> {
    parse_emby_body(headers, body)
}

fn user_mutation_disabled_error() -> AppError {
    AppError::conflict("Emby user management writes are disabled; use FBZ user management APIs")
}

fn forgot_password_request_input(
    request: ForgotPasswordRequestDto,
) -> Result<ForgotPasswordInput, AppError> {
    let entered_username =
        normalized_required_user_text("EnteredUsername", request.entered_username)?;

    Ok(ForgotPasswordInput { entered_username })
}

fn forgot_password_pin_input(
    request: ForgotPasswordPinDto,
) -> Result<ForgotPasswordPinInput, AppError> {
    let pin = normalized_required_user_text("Pin", request.pin)?;

    Ok(ForgotPasswordPinInput { pin })
}

fn forgot_password_result() -> ForgotPasswordResultDto {
    ForgotPasswordResultDto {
        action: "ContactAdmin".to_owned(),
        pin_file: None,
        pin_expiration_date: None,
    }
}

fn forgot_password_pin_redeem_result() -> PinRedeemResultDto {
    PinRedeemResultDto {
        success: false,
        users_reset: Vec::new(),
    }
}

fn user_prefix_items(records: Vec<UserDetailRecord>) -> Vec<NameIdPairDto> {
    records
        .into_iter()
        .filter_map(|record| {
            let prefix = record
                .name
                .trim()
                .chars()
                .next()?
                .to_uppercase()
                .to_string();
            (!prefix.is_empty()).then_some(prefix)
        })
        .collect::<BTreeSet<_>>()
        .into_iter()
        .map(|prefix| NameIdPairDto {
            id: prefix.clone(),
            name: prefix,
        })
        .collect()
}

async fn dispatch_login_hook(pool: &DbPool, output: &LoginOutput) {
    let event = login_hook_event(output);
    if let Err(err) = PluginHookDispatcher::new(pool.clone())
        .dispatch(event)
        .await
    {
        warn!(
            error = %err,
            event_key = USER_LOGIN_EVENT,
            user_id = %output.user_id,
            "failed to dispatch plugin login hooks"
        );
    }
}

fn login_hook_event(output: &LoginOutput) -> PluginHookEvent {
    PluginHookEvent {
        event_key: USER_LOGIN_EVENT.to_owned(),
        aggregate_type: "user".to_owned(),
        aggregate_id: output.user_id.clone(),
        payload: login_hook_payload(output),
    }
}

fn login_hook_payload(output: &LoginOutput) -> Value {
    json!({
        "userId": &output.user_id,
        "username": &output.username,
        "sessionId": &output.session_id,
        "client": output.client.as_deref(),
        "deviceId": output.device_id.as_deref(),
        "deviceName": output.device_name.as_deref(),
        "version": output.version.as_deref(),
    })
}

fn auth_service_error_to_app_error(error: AuthServiceError) -> AppError {
    match error {
        AuthServiceError::InvalidCredentials
        | AuthServiceError::DisabledUser
        | AuthServiceError::MissingPassword => AppError::unauthorized(error.to_string()),
        AuthServiceError::MissingDeviceId => AppError::unprocessable(error.to_string()),
        AuthServiceError::NewDeviceLoginDisabled | AuthServiceError::DeviceRevoked => {
            AppError::forbidden(error.to_string())
        }
        AuthServiceError::Repository(_) => AppError::internal(error.to_string()),
    }
}

#[cfg(test)]
mod tests {
    use axum::http::{StatusCode, header::CONTENT_TYPE};

    use super::*;

    fn json_headers() -> HeaderMap {
        let mut headers = HeaderMap::new();
        headers.insert(CONTENT_TYPE, "application/json".parse().unwrap());
        headers
    }

    #[test]
    fn device_policy_errors_are_forbidden() {
        assert_eq!(
            auth_service_error_to_app_error(AuthServiceError::NewDeviceLoginDisabled).status_code(),
            StatusCode::FORBIDDEN
        );
        assert_eq!(
            auth_service_error_to_app_error(AuthServiceError::DeviceRevoked).status_code(),
            StatusCode::FORBIDDEN
        );
    }

    #[test]
    fn login_hook_payload_exposes_safe_session_context() {
        let output = LoginOutput {
            user_id: "user-1".to_owned(),
            username: "alice".to_owned(),
            session_id: "session-1".to_owned(),
            access_token: "fbz_secret_token".to_owned(),
            client: Some("Infuse".to_owned()),
            device_id: Some("device-1".to_owned()),
            device_name: Some("Apple TV".to_owned()),
            version: Some("8.0".to_owned()),
        };

        let event = login_hook_event(&output);

        assert_eq!(event.event_key, USER_LOGIN_EVENT);
        assert_eq!(event.aggregate_type, "user");
        assert_eq!(event.aggregate_id, "user-1");
        assert_eq!(event.payload["userId"], "user-1");
        assert_eq!(event.payload["username"], "alice");
        assert_eq!(event.payload["sessionId"], "session-1");
        assert_eq!(event.payload["client"], "Infuse");
        assert_eq!(event.payload["deviceId"], "device-1");
        assert_eq!(event.payload["deviceName"], "Apple TV");
        assert_eq!(event.payload["version"], "8.0");
        assert!(event.payload.get("accessToken").is_none());
        assert!(event.payload.get("password").is_none());
    }

    #[test]
    fn user_detail_mapping_exposes_policy_boundary() {
        let dto = user_detail_record_to_dto(UserDetailRecord {
            id: "user-1".to_owned(),
            name: "alice".to_owned(),
            has_password: true,
            is_administrator: true,
            is_disabled: false,
            allow_download: true,
            allow_transcode: false,
            allow_new_device_login: false,
            enable_content_downloading: true,
            enable_playback_transcoding: false,
            enable_all_folders: false,
            enabled_folders: vec!["library-1".to_owned()],
        });

        assert_eq!(dto.id, "user-1");
        assert!(dto.policy.is_administrator);
        assert!(dto.policy.enable_content_downloading);
        assert!(!dto.policy.enable_video_playback_transcoding);
        assert!(!dto.policy.enable_media_conversion);
        assert!(!dto.policy.enable_all_devices);
        assert!(!dto.policy.enable_all_folders);
        assert_eq!(dto.policy.enabled_folders, ["library-1"]);
        assert!(dto.configuration.remember_audio_selections);
    }

    #[test]
    fn users_query_maps_official_filters_to_repository_input() {
        let query = serde_json::from_value::<UsersQuery>(json!({
            "IsHidden": false,
            "IsDisabled": true,
            "StartIndex": 3,
            "Limit": 500,
            "NameStartsWithOrGreater": "  Bob  ",
            "SortOrder": "Descending"
        }))
        .expect("users query should deserialize");

        let input = users_query_input(query);

        assert_eq!(input.is_hidden, Some(false));
        assert_eq!(input.is_disabled, Some(true));
        assert_eq!(input.start_index, 3);
        assert_eq!(input.limit, 100);
        assert_eq!(input.name_starts_with_or_greater.as_deref(), Some("Bob"));
        assert!(input.sort_descending);
    }

    #[test]
    fn users_query_accepts_lower_camel_and_snake_case_filters() {
        let lower_camel_uri: Uri = "/emby/Users/Query?isHidden=false&isDisabled=true&startIndex=3&limit=500&nameStartsWithOrGreater=Bob&sortOrder=Descending"
            .parse()
            .unwrap();
        let Query(lower_camel) = Query::<UsersQuery>::try_from_uri(&lower_camel_uri).unwrap();
        let lower_input = users_query_input(lower_camel);

        assert_eq!(lower_input.is_hidden, Some(false));
        assert_eq!(lower_input.is_disabled, Some(true));
        assert_eq!(lower_input.start_index, 3);
        assert_eq!(lower_input.limit, MAX_USERS_QUERY_LIMIT);
        assert_eq!(
            lower_input.name_starts_with_or_greater.as_deref(),
            Some("Bob")
        );
        assert!(lower_input.sort_descending);

        let snake_case_uri: Uri = "/Users/Prefixes?is_hidden=true&is_disabled=false&start_index=2&limit=20&name_starts_with_or_greater=Alice&sort_order=Ascending"
            .parse()
            .unwrap();
        let Query(snake_case) = Query::<UsersQuery>::try_from_uri(&snake_case_uri).unwrap();
        let snake_input = users_query_input(snake_case);

        assert_eq!(snake_input.is_hidden, Some(true));
        assert_eq!(snake_input.is_disabled, Some(false));
        assert_eq!(snake_input.start_index, 2);
        assert_eq!(snake_input.limit, 20);
        assert_eq!(
            snake_input.name_starts_with_or_greater.as_deref(),
            Some("Alice")
        );
        assert!(!snake_input.sort_descending);
    }

    #[test]
    fn users_query_clamps_pathologically_large_start_index() {
        let input = users_query_input(UsersQuery {
            start_index: Some(500_000),
            limit: Some(50),
            ..UsersQuery::default()
        });

        assert_eq!(input.start_index, 10_000);
        assert_eq!(input.limit, 50);
    }

    #[test]
    fn forgot_password_request_normalizes_username() {
        let request = serde_json::from_value::<ForgotPasswordRequestDto>(json!({
            "EnteredUsername": "  Alice  "
        }))
        .expect("forgot password request should deserialize");

        assert_eq!(
            forgot_password_request_input(request)
                .expect("forgot password username should normalize")
                .entered_username,
            "Alice"
        );
        assert!(
            forgot_password_request_input(ForgotPasswordRequestDto {
                entered_username: Some("  ".to_owned())
            })
            .is_err()
        );
    }

    #[test]
    fn user_management_bodies_accept_lower_camel_and_snake_case_fields() {
        let forgot = serde_json::from_value::<ForgotPasswordRequestDto>(json!({
            "enteredUsername": "  Alice  "
        }))
        .expect("lower-camel forgot password request should deserialize");
        assert_eq!(
            forgot_password_request_input(forgot)
                .expect("forgot password username should normalize")
                .entered_username,
            "Alice"
        );

        let pin = serde_json::from_value::<ForgotPasswordPinDto>(json!({
            "pin": "  123456  "
        }))
        .expect("lowercase forgot password pin should deserialize");
        assert_eq!(
            forgot_password_pin_input(pin)
                .expect("forgot password pin should normalize")
                .pin,
            "123456"
        );

        let create = serde_json::from_value::<CreateUserByNameDto>(json!({
            "name": "  Bob  ",
            "copyFromUserId": " template-user ",
            "userCopyOptions": ["UserPolicy"]
        }))
        .expect("lower-camel create user request should deserialize");
        let create_input = create_user_input(create).expect("create user input should normalize");
        assert_eq!(create_input.name, "Bob");
        assert_eq!(
            create_input.copy_from_user_id.as_deref(),
            Some("template-user")
        );
        assert_eq!(create_input.user_copy_options, ["UserPolicy"]);

        let snake_create = serde_json::from_value::<CreateUserByNameDto>(json!({
            "copy_from_user_id": " template-user ",
            "user_copy_options": ["UserConfiguration"],
            "name": "  Carol  "
        }))
        .expect("snake-case create user request should deserialize");
        let snake_create_input =
            create_user_input(snake_create).expect("create user input should normalize");
        assert_eq!(snake_create_input.name, "Carol");
        assert_eq!(
            snake_create_input.copy_from_user_id.as_deref(),
            Some("template-user")
        );
        assert_eq!(snake_create_input.user_copy_options, ["UserConfiguration"]);

        let password = serde_json::from_value::<UpdateUserPasswordDto>(json!({
            "id": " user-1 ",
            "newPw": "  secret  ",
            "resetPassword": false
        }))
        .expect("lower-camel password request should deserialize");
        let password_input = password_update_input("user-1", password)
            .expect("password update input should normalize");
        assert_eq!(password_input.new_password.as_deref(), Some("secret"));
        assert!(!password_input.reset_password);

        let snake_password = serde_json::from_value::<UpdateUserPasswordDto>(json!({
            "id": " user-1 ",
            "new_pw": "  secret2  ",
            "reset_password": false
        }))
        .expect("snake-case password request should deserialize");
        let snake_password_input = password_update_input("user-1", snake_password)
            .expect("password update input should normalize");
        assert_eq!(
            snake_password_input.new_password.as_deref(),
            Some("secret2")
        );
        assert!(!snake_password_input.reset_password);

        let easy_password = serde_json::from_value::<UpdateUserPasswordDto>(json!({
            "id": " user-1 ",
            "newPw": "  1234  ",
            "resetPassword": false
        }))
        .expect("lower-camel easy password request should deserialize");
        let easy_password_input = easy_password_update_input("user-1", easy_password)
            .expect("easy password update input should normalize");
        assert_eq!(easy_password_input.new_password.as_deref(), Some("1234"));
        assert!(!easy_password_input.reset_password);
    }

    #[test]
    fn user_management_generic_write_body_rejects_malformed_json() {
        let headers = json_headers();
        let body = Bytes::from_static(br#"{"Id":"user-1""#);

        let err = parse_user_management_generic_body(&headers, &body).unwrap_err();

        assert_eq!(err.status_code(), http::StatusCode::UNPROCESSABLE_ENTITY);
        assert!(err.message().contains("invalid JSON request body"));
    }

    #[test]
    fn forgot_password_result_contacts_admin_without_pin_state() {
        let result = forgot_password_result();

        assert_eq!(result.action, "ContactAdmin");
        assert!(result.pin_file.is_none());
        assert!(result.pin_expiration_date.is_none());
    }

    #[test]
    fn forgot_password_pin_redeem_normalizes_and_fails_closed() {
        let request = serde_json::from_value::<ForgotPasswordPinDto>(json!({
            "Pin": "  123456  "
        }))
        .expect("forgot password pin should deserialize");

        assert_eq!(
            forgot_password_pin_input(request)
                .expect("forgot password pin should normalize")
                .pin,
            "123456"
        );

        let result = forgot_password_pin_redeem_result();
        assert!(!result.success);
        assert!(result.users_reset.is_empty());
    }

    #[test]
    fn user_prefix_items_deduplicate_normalized_initials() {
        let prefixes = user_prefix_items(vec![
            user_detail_record("user-1", " alice "),
            user_detail_record("user-2", "Alice B"),
            user_detail_record("user-3", "bob"),
            user_detail_record("user-4", "  "),
        ]);

        assert_eq!(prefixes.len(), 2);
        assert_eq!(prefixes[0].name, "A");
        assert_eq!(prefixes[0].id, "A");
        assert_eq!(prefixes[1].name, "B");
        assert_eq!(prefixes[1].id, "B");
    }

    #[test]
    fn create_user_request_requires_normalized_name() {
        let request = serde_json::from_value::<CreateUserByNameDto>(json!({
            "Name": "  Alice  ",
            "CopyFromUserId": " template-user ",
            "UserCopyOptions": ["UserPolicy", "UserConfiguration"]
        }))
        .expect("create user request should deserialize");

        let input = create_user_input(request).expect("create user input should normalize");

        assert_eq!(input.name, "Alice");
        assert_eq!(input.copy_from_user_id.as_deref(), Some("template-user"));
        assert_eq!(input.user_copy_options, ["UserPolicy", "UserConfiguration"]);
        assert!(
            create_user_input(CreateUserByNameDto {
                name: Some("  ".to_owned()),
                copy_from_user_id: None,
                user_copy_options: None,
            })
            .is_err()
        );
    }

    #[test]
    fn password_update_matches_path_and_accepts_reset_mode() {
        let request = serde_json::from_value::<UpdateUserPasswordDto>(json!({
            "Id": " user-1 ",
            "NewPw": "  secret  ",
            "ResetPassword": false
        }))
        .expect("password request should deserialize");

        let input = password_update_input(" user-1 ", request)
            .expect("password update input should normalize");

        assert_eq!(input.user_id, "user-1");
        assert_eq!(input.new_password.as_deref(), Some("secret"));
        assert!(!input.reset_password);
        assert!(
            password_update_input(
                "user-1",
                UpdateUserPasswordDto {
                    id: Some("other-user".to_owned()),
                    new_pw: Some("secret".to_owned()),
                    reset_password: Some(false),
                },
            )
            .is_err()
        );
        assert!(
            password_update_input(
                "user-1",
                UpdateUserPasswordDto {
                    id: Some("user-1".to_owned()),
                    new_pw: None,
                    reset_password: Some(false),
                },
            )
            .is_err()
        );
        assert!(
            password_update_input(
                "user-1",
                UpdateUserPasswordDto {
                    id: Some("user-1".to_owned()),
                    new_pw: None,
                    reset_password: Some(true),
                },
            )
            .is_ok()
        );
    }

    #[test]
    fn user_write_body_rejects_oversized_payload() {
        ensure_user_write_body_size(&Bytes::from(vec![0; MAX_EMBY_USER_WRITE_BODY_BYTES]))
            .expect("max sized user write payload should pass");
        assert!(
            ensure_user_write_body_size(&Bytes::from(vec![0; MAX_EMBY_USER_WRITE_BODY_BYTES + 1]))
                .is_err()
        );
    }

    #[test]
    fn user_mutation_disabled_error_is_conflict() {
        assert_eq!(
            user_mutation_disabled_error().status_code(),
            StatusCode::CONFLICT
        );
    }

    fn user_detail_record(id: &str, name: &str) -> UserDetailRecord {
        UserDetailRecord {
            id: id.to_owned(),
            name: name.to_owned(),
            has_password: true,
            is_administrator: false,
            is_disabled: false,
            allow_download: true,
            allow_transcode: true,
            allow_new_device_login: true,
            enable_content_downloading: true,
            enable_playback_transcoding: true,
            enable_all_folders: true,
            enabled_folders: Vec::new(),
        }
    }
}

fn users_query_input(query: UsersQuery) -> UsersQueryFilter {
    UsersQueryFilter {
        is_hidden: query.is_hidden,
        is_disabled: query.is_disabled,
        start_index: query
            .start_index
            .unwrap_or_default()
            .clamp(0, MAX_USERS_QUERY_START_INDEX),
        limit: query
            .limit
            .unwrap_or(DEFAULT_USERS_QUERY_LIMIT)
            .clamp(1, MAX_USERS_QUERY_LIMIT),
        name_starts_with_or_greater: normalized_user_query_text(query.name_starts_with_or_greater),
        sort_descending: query
            .sort_order
            .as_deref()
            .is_some_and(|value| value.eq_ignore_ascii_case("Descending")),
    }
}

fn normalized_user_query_text(value: Option<String>) -> Option<String> {
    value.and_then(|value| {
        let trimmed = value.trim();
        (!trimmed.is_empty()).then(|| trimmed.chars().take(MAX_USERS_QUERY_TEXT_LEN).collect())
    })
}

fn normalized_required_user_text(name: &str, value: Option<String>) -> Result<String, AppError> {
    normalized_user_query_text(value)
        .ok_or_else(|| AppError::unprocessable(format!("{name} is required")))
}
