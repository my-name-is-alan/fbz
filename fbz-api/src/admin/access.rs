use axum::http::{HeaderMap, Uri};

use crate::{
    auth::{repository::AuthRepository, service::AuthService, service::AuthenticatedUser},
    compat::emby::auth::{EmbyCredential, parse_auth_context},
    error::AppError,
    state::AppState,
};

pub async fn authenticate_admin(
    state: &AppState,
    headers: &HeaderMap,
    uri: &Uri,
) -> Result<AuthenticatedUser, AppError> {
    let Some(database) = state.database() else {
        return Err(AppError::internal("database is not configured"));
    };

    let token = access_token_from_request(headers, uri.query())?;
    let user = AuthService::new(AuthRepository::new(database.clone()))
        .authenticate_access_token(&token)
        .await
        .map_err(|err| AppError::unauthorized(err.to_string()))?;

    if !user.can_manage_server() {
        return Err(AppError::forbidden("server management permission required"));
    }

    Ok(user)
}

fn access_token_from_request(headers: &HeaderMap, query: Option<&str>) -> Result<String, AppError> {
    let context = parse_auth_context(headers, query)?;
    match context.require_credential()? {
        EmbyCredential::AccessToken(token) => Ok(token.to_owned()),
        EmbyCredential::ApiKey(_) => Err(AppError::unauthorized(
            "admin routes require an access token",
        )),
    }
}
