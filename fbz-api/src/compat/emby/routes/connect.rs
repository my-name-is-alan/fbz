use axum::{
    Json,
    body::Bytes,
    extract::{Path, Query, State},
    http::{HeaderMap, StatusCode, Uri},
};
use serde::{Deserialize, Serialize};

use crate::{compat::emby::payload::parse_emby_body, error::AppError, state::AppState};

use super::access::authenticate_request_user;

const MAX_CONNECT_TEXT_LEN: usize = 256;
const MAX_CONNECT_LINK_BODY_BYTES: usize = 64 * 1024;

#[derive(Clone, Debug, Default, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "PascalCase")]
pub struct ConnectExchangeQuery {
    #[serde(alias = "connectUserName", alias = "connect_user_name")]
    pub connect_user_name: Option<String>,
    #[serde(alias = "pin")]
    pub pin: Option<String>,
}

#[derive(Clone, Debug, Default, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "PascalCase")]
pub struct ConnectPendingQuery {
    #[serde(alias = "connectUserName", alias = "connect_user_name")]
    pub connect_user_name: Option<String>,
}

#[derive(Clone, Debug, Default, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "PascalCase")]
pub struct ConnectLinkRequestDto {
    #[serde(alias = "connectUserName", alias = "connect_user_name")]
    pub connect_user_name: Option<String>,
}

#[derive(Clone, Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "PascalCase")]
pub struct ConnectExchangeResultDto {
    pub local_user_id: Option<String>,
    pub access_token: Option<String>,
}

#[derive(Clone, Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "PascalCase")]
pub struct ConnectPendingResultDto {
    pub is_pending: bool,
    pub is_enabled: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct ConnectExchangeInput {
    connect_user_name: String,
    pin: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct ConnectPendingInput {
    connect_user_name: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct ConnectLinkInput {
    user_id: String,
    connect_user_name: Option<String>,
}

pub async fn connect_exchange(
    State(state): State<AppState>,
    Query(query): Query<ConnectExchangeQuery>,
    headers: HeaderMap,
    uri: Uri,
) -> Result<Json<ConnectExchangeResultDto>, AppError> {
    authenticate_request_user(&state, &headers, &uri).await?;
    let _input = connect_exchange_input(query)?;

    Ok(Json(disabled_exchange_result()))
}

pub async fn connect_pending(
    State(state): State<AppState>,
    Query(query): Query<ConnectPendingQuery>,
    headers: HeaderMap,
    uri: Uri,
) -> Result<Json<ConnectPendingResultDto>, AppError> {
    authenticate_request_user(&state, &headers, &uri).await?;
    let _input = connect_pending_input(query)?;

    Ok(Json(disabled_pending_result()))
}

pub async fn connect_link_user(
    State(state): State<AppState>,
    Path(user_id): Path<String>,
    headers: HeaderMap,
    uri: Uri,
    body: Bytes,
) -> Result<(), AppError> {
    authenticate_connect_user_target(&state, &user_id, &headers, &uri).await?;
    ensure_connect_link_body_size(&body)?;
    let request: ConnectLinkRequestDto = parse_emby_body(&headers, &body)?;
    let _input = connect_link_input(&user_id, request)?;

    Err(connect_mutation_disabled_error())
}

pub async fn connect_unlink_user(
    State(state): State<AppState>,
    Path(user_id): Path<String>,
    headers: HeaderMap,
    uri: Uri,
) -> Result<StatusCode, AppError> {
    authenticate_connect_user_target(&state, &user_id, &headers, &uri).await?;
    let _user_id = normalized_required_connect_text("Id", Some(user_id))?;

    Err(connect_mutation_disabled_error())
}

async fn authenticate_connect_user_target(
    state: &AppState,
    requested_user_id: &str,
    headers: &HeaderMap,
    uri: &Uri,
) -> Result<(), AppError> {
    let authenticated = authenticate_request_user(state, headers, uri).await?;
    let requested_user_id =
        normalized_required_connect_text("Id", Some(requested_user_id.to_owned()))?;
    if authenticated.public_id != requested_user_id && !authenticated.can_manage_server() {
        return Err(AppError::forbidden(
            "authenticated user does not match requested user",
        ));
    }

    Ok(())
}

fn connect_exchange_input(query: ConnectExchangeQuery) -> Result<ConnectExchangeInput, AppError> {
    Ok(ConnectExchangeInput {
        connect_user_name: normalized_required_connect_text(
            "ConnectUserName",
            query.connect_user_name,
        )?,
        pin: normalized_required_connect_text("Pin", query.pin)?,
    })
}

fn connect_pending_input(query: ConnectPendingQuery) -> Result<ConnectPendingInput, AppError> {
    Ok(ConnectPendingInput {
        connect_user_name: normalized_required_connect_text(
            "ConnectUserName",
            query.connect_user_name,
        )?,
    })
}

fn connect_link_input(
    path_user_id: &str,
    request: ConnectLinkRequestDto,
) -> Result<ConnectLinkInput, AppError> {
    Ok(ConnectLinkInput {
        user_id: normalized_required_connect_text("Id", Some(path_user_id.to_owned()))?,
        connect_user_name: normalized_connect_text(request.connect_user_name),
    })
}

fn ensure_connect_link_body_size(body: &Bytes) -> Result<(), AppError> {
    if body.len() > MAX_CONNECT_LINK_BODY_BYTES {
        return Err(AppError::unprocessable(format!(
            "connect link payload must be at most {MAX_CONNECT_LINK_BODY_BYTES} bytes"
        )));
    }

    Ok(())
}

fn disabled_exchange_result() -> ConnectExchangeResultDto {
    ConnectExchangeResultDto {
        local_user_id: None,
        access_token: None,
    }
}

fn disabled_pending_result() -> ConnectPendingResultDto {
    ConnectPendingResultDto {
        is_pending: false,
        is_enabled: false,
    }
}

fn connect_mutation_disabled_error() -> AppError {
    AppError::conflict("Emby Connect account linking is not enabled")
}

fn normalized_connect_text(value: Option<String>) -> Option<String> {
    value.and_then(|value| {
        let trimmed = value.trim();
        (!trimmed.is_empty()).then(|| trimmed.chars().take(MAX_CONNECT_TEXT_LEN).collect())
    })
}

fn normalized_required_connect_text(name: &str, value: Option<String>) -> Result<String, AppError> {
    normalized_connect_text(value)
        .ok_or_else(|| AppError::unprocessable(format!("{name} is required")))
}

#[cfg(test)]
mod tests {
    use axum::http::StatusCode;

    use super::*;

    #[test]
    fn connect_exchange_query_accepts_lower_camel_and_snake_case_fields() {
        let lower = serde_json::from_value::<ConnectExchangeQuery>(serde_json::json!({
            "connectUserName": " alice ",
            "pin": " 123456 "
        }))
        .expect("lower-camel connect exchange query");
        let lower_input = connect_exchange_input(lower).expect("lower-camel exchange input");
        assert_eq!(lower_input.connect_user_name, "alice");
        assert_eq!(lower_input.pin, "123456");

        let snake = serde_json::from_value::<ConnectPendingQuery>(serde_json::json!({
            "connect_user_name": " bob "
        }))
        .expect("snake_case pending query");
        let snake_input = connect_pending_input(snake).expect("snake pending input");
        assert_eq!(snake_input.connect_user_name, "bob");
    }

    #[test]
    fn connect_link_body_accepts_lower_camel_and_snake_case_fields() {
        let lower = serde_json::from_value::<ConnectLinkRequestDto>(serde_json::json!({
            "connectUserName": " alice "
        }))
        .expect("lower-camel connect link body");
        let lower_input = connect_link_input(" user-1 ", lower).expect("lower-camel link input");
        assert_eq!(lower_input.user_id, "user-1");
        assert_eq!(lower_input.connect_user_name.as_deref(), Some("alice"));

        let snake = serde_json::from_value::<ConnectLinkRequestDto>(serde_json::json!({
            "connect_user_name": " bob "
        }))
        .expect("snake_case connect link body");
        let snake_input = connect_link_input("user-2", snake).expect("snake link input");
        assert_eq!(snake_input.connect_user_name.as_deref(), Some("bob"));
    }

    #[test]
    fn connect_link_body_size_is_bounded() {
        assert!(
            ensure_connect_link_body_size(&Bytes::from(vec![0; MAX_CONNECT_LINK_BODY_BYTES]))
                .is_ok()
        );
        let err =
            ensure_connect_link_body_size(&Bytes::from(vec![0; MAX_CONNECT_LINK_BODY_BYTES + 1]))
                .unwrap_err();
        assert_eq!(err.status_code(), StatusCode::UNPROCESSABLE_ENTITY);
    }

    #[test]
    fn connect_mutation_disabled_error_is_conflict() {
        assert_eq!(
            connect_mutation_disabled_error().status_code(),
            StatusCode::CONFLICT
        );
    }
}
