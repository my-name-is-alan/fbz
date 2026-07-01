//! 导航 BFF 路由：`/api/navigation`。
//!
//! 瘦控制器——只做鉴权、调用 service、映射错误。聚合编排与映射在 [`super::service`]，
//! SQL 在 [`crate::library::repository`]。

use axum::{Json, Router, extract::State, http::HeaderMap, http::Uri, routing::get};

use crate::{error::AppError, navigation::dto::NavigationDto, state::AppState};

use super::{access::authenticate_user, service::load_navigation};

/// 挂载导航相关路由。
pub fn router() -> Router<AppState> {
    Router::new().route("/api/navigation", get(navigation))
}

/// `GET /api/navigation`：返回当前用户首屏所需的聚合数据。
pub async fn navigation(
    State(state): State<AppState>,
    headers: HeaderMap,
    uri: Uri,
) -> Result<Json<NavigationDto>, AppError> {
    let Some(database) = state.database() else {
        return Err(AppError::internal("database is not configured"));
    };

    let user = authenticate_user(&state, &headers, &uri).await?;
    let navigation = load_navigation(database.clone(), &user)
        .await
        .map_err(|err| AppError::internal(format!("failed to load navigation: {err}")))?;

    Ok(Json(navigation))
}
