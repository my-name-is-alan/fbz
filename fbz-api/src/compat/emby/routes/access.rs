use axum::http::{HeaderMap, Uri};

use crate::{
    auth::{repository::AuthRepository, service::AuthService, service::AuthenticatedUser},
    compat::emby::auth::{EmbyCredential, parse_auth_context},
    error::AppError,
    state::AppState,
};

pub async fn authenticate_route_user(
    state: &AppState,
    route_user_id: &str,
    headers: &HeaderMap,
    uri: &Uri,
) -> Result<AuthenticatedUser, AppError> {
    let user = authenticate_request_user(state, headers, uri).await?;

    if user.public_id != route_user_id {
        return Err(AppError::forbidden(
            "authenticated user does not match route user",
        ));
    }

    Ok(user)
}

pub async fn authenticate_request_user(
    state: &AppState,
    headers: &HeaderMap,
    uri: &Uri,
) -> Result<AuthenticatedUser, AppError> {
    let Some(database) = state.database() else {
        return Err(AppError::internal("database is not configured"));
    };

    let token = access_token_from_request(headers, uri.query())?;
    AuthService::new(AuthRepository::new(database.clone()))
        .authenticate_access_token(&token)
        .await
        .map_err(|err| AppError::unauthorized(err.to_string()))
}

pub async fn authenticate_query_user(
    state: &AppState,
    query_user_id: Option<&str>,
    headers: &HeaderMap,
    uri: &Uri,
) -> Result<AuthenticatedUser, AppError> {
    let user = authenticate_request_user(state, headers, uri).await?;
    if let Some(query_user_id) = query_user_id
        && query_user_id != user.public_id
    {
        return Err(AppError::forbidden(
            "authenticated user does not match query user",
        ));
    }

    Ok(user)
}

pub(crate) fn access_token_from_request(
    headers: &HeaderMap,
    query: Option<&str>,
) -> Result<String, AppError> {
    let context = parse_auth_context(headers, query)?;
    match context.require_credential()? {
        EmbyCredential::AccessToken(token) | EmbyCredential::ApiKey(token) => Ok(token.to_owned()),
    }
}

#[cfg(test)]
mod tests {
    use axum::http::HeaderMap;

    use super::*;

    #[test]
    fn query_api_key_is_accepted_as_emby_access_token() {
        let token = access_token_from_request(&HeaderMap::new(), Some("api_key=abc123")).unwrap();

        assert_eq!(token, "abc123");
    }
}
