use axum::{
    Json,
    body::Bytes,
    extract::{Query, State},
    http::{HeaderMap, StatusCode, Uri},
};
use serde::Deserialize;
use serde::Serialize;
use serde_json::Value;

use crate::{
    auth::{
        repository::{AuthRepository, SessionCapabilitiesInput, SessionInfoRecord},
        service::{AuthService, AuthenticatedUser},
    },
    compat::emby::auth::{EmbyCredential, parse_auth_context},
    compat::emby::dto::{NameIdPairDto, SessionInfoDto},
    compat::emby::payload::parse_emby_body,
    error::AppError,
    state::AppState,
};

use super::access::authenticate_request_user;

#[derive(Serialize)]
#[serde(rename_all = "PascalCase")]
pub struct LogoutResponseDto {
    pub success: bool,
}

#[derive(Clone, Debug, Default, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "PascalCase")]
pub struct SessionsQuery {
    pub user_id: Option<String>,
}

#[derive(Clone, Debug, Default, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "PascalCase")]
pub struct SessionCapabilitiesQuery {
    pub id: Option<String>,
    pub playable_media_types: Option<String>,
    pub supported_commands: Option<String>,
    pub supports_media_control: Option<bool>,
    pub supports_sync: Option<bool>,
}

#[derive(Clone, Debug, Default, Deserialize, PartialEq)]
#[serde(rename_all = "PascalCase")]
pub struct ClientCapabilitiesDto {
    #[serde(default)]
    pub playable_media_types: Vec<String>,
    #[serde(default)]
    pub supported_commands: Vec<String>,
    #[serde(default)]
    pub supports_media_control: bool,
    pub push_token: Option<String>,
    pub push_token_type: Option<String>,
    #[serde(default)]
    pub supports_sync: bool,
    pub device_profile: Option<Value>,
    pub icon_url: Option<String>,
    pub app_id: Option<String>,
}

const MAX_SESSION_LIST_LIMIT: i64 = 100;
const MAX_CAPABILITY_VALUES: usize = 128;
const MAX_CAPABILITY_VALUE_LEN: usize = 128;
const MAX_CAPABILITY_TEXT_LEN: usize = 512;

pub async fn auth_providers(
    State(state): State<AppState>,
    headers: HeaderMap,
    uri: Uri,
) -> Result<Json<Vec<NameIdPairDto>>, AppError> {
    let user = authenticate_request_user(&state, &headers, &uri).await?;
    if !user.can_manage_server() {
        return Err(AppError::forbidden(
            "authenticated user cannot manage authentication providers",
        ));
    }

    Ok(Json(auth_provider_items()))
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

    let Some(database) = state.database() else {
        return Err(AppError::internal("database is not configured"));
    };

    let sessions = AuthRepository::new(database.clone())
        .list_active_sessions_for_user(authenticated.id, MAX_SESSION_LIST_LIMIT)
        .await
        .map_err(|err| AppError::internal(format!("failed to list sessions: {err}")))?
        .into_iter()
        .map(session_record_to_dto)
        .collect();

    Ok(Json(sessions))
}

pub async fn logout(
    State(state): State<AppState>,
    headers: HeaderMap,
    uri: Uri,
) -> Result<Json<LogoutResponseDto>, AppError> {
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

    Ok(Json(LogoutResponseDto { success: revoked }))
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

fn normalize_session_id(value: Option<&str>) -> Result<String, AppError> {
    let Some(value) = value.map(str::trim).filter(|value| !value.is_empty()) else {
        return Err(AppError::unprocessable("session Id is required"));
    };

    if value.len() > 64 || !value.chars().all(|ch| ch.is_ascii_hexdigit() || ch == '-') {
        return Err(AppError::unprocessable("session Id is invalid"));
    }

    Ok(value.to_owned())
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
    fn capabilities_reject_unbounded_values() {
        let long_value = "a".repeat(MAX_CAPABILITY_VALUE_LEN + 1);
        let err = normalize_capability_values(vec![long_value]).unwrap_err();

        assert_eq!(err.status_code(), http::StatusCode::UNPROCESSABLE_ENTITY);

        let err = normalize_session_id(Some("not a uuid")).unwrap_err();

        assert_eq!(err.status_code(), http::StatusCode::UNPROCESSABLE_ENTITY);
    }
}
