use axum::{
    Json,
    body::Bytes,
    extract::{Path, State},
    http::{HeaderMap, Uri},
};
use serde_json::{Value, json};
use tracing::warn;

use crate::{
    auth::{
        repository::AuthRepository,
        service::{AuthService, AuthServiceError, LoginInput, LoginOutput},
    },
    compat::emby::{
        auth::parse_auth_context,
        dto::{
            AuthenticateByNameRequestDto, AuthenticationResultDto, PublicUserDto, SessionInfoDto,
            UserDetailSource, UserDto, UserSource,
        },
        payload::parse_emby_body,
    },
    db::DbPool,
    error::AppError,
    plugins::hooks::{PluginHookDispatcher, PluginHookEvent},
    state::AppState,
    users::repository::{PublicUserRecord, UserDetailRecord, UsersRepository},
};

use super::access::authenticate_request_user;

const USER_LOGIN_EVENT: &str = "user.login";

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
    use axum::http::StatusCode;

    use super::*;

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
}
