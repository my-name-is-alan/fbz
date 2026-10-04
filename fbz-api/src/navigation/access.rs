//! 导航 BFF 的请求鉴权。
//!
//! 跟随既定样板（`admin/access.rs`、`compat/emby/routes/access.rs`）：从 `x-emby-token`
//! 头或 `api_key` 查询串取访问令牌，交 [`AuthService`] 校验。与 admin 不同，导航面向所有
//! 已登录用户，不要求服务器管理权限。

use axum::http::{HeaderMap, Uri};

use crate::{
    auth::{repository::AuthRepository, service::AuthService, service::AuthenticatedUser},
    compat::emby::auth::{EmbyCredential, parse_auth_context},
    error::AppError,
    state::AppState,
};

/// 校验请求携带的访问令牌，返回已登录用户（任意角色）。
pub async fn authenticate_user(
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

fn access_token_from_request(headers: &HeaderMap, query: Option<&str>) -> Result<String, AppError> {
    let context = parse_auth_context(headers, query)?;
    match context.require_credential()? {
        EmbyCredential::AccessToken(token) | EmbyCredential::ApiKey(token) => Ok(token.to_owned()),
    }
}
