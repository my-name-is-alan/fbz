//! setup 路由：`GET /api/setup/status` 与 `POST /api/setup`，均无认证。
//!
//! 瘦控制器——只做请求体解析、调用 [`super::service`]、映射错误。

use axum::{
    Json, Router,
    extract::State,
    routing::{get, post},
};
use serde::{Deserialize, Serialize};

use crate::{error::AppError, state::AppState};

use super::service::{SetupError, complete_setup, has_any_user};

/// 挂载 setup 相关路由。
pub fn router() -> Router<AppState> {
    Router::new()
        .route("/api/setup/status", get(setup_status))
        .route("/api/setup", post(submit_setup))
}

/// `GET /api/setup/status` 的响应。
#[derive(Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct SetupStatusDto {
    /// 是否已初始化（已存在任意用户）。
    pub initialized: bool,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SetupRequestDto {
    pub username: String,
    pub password: String,
}

/// `GET /api/setup/status`：开机即可问，返回是否已初始化。
pub async fn setup_status(State(state): State<AppState>) -> Result<Json<SetupStatusDto>, AppError> {
    let Some(database) = state.database() else {
        return Err(AppError::internal("database is not configured"));
    };
    let initialized = has_any_user(database)
        .await
        .map_err(|err| AppError::internal(format!("failed to read setup status: {err}")))?;
    Ok(Json(SetupStatusDto { initialized }))
}

/// `POST /api/setup`：锁定式建首个管理员；已初始化返回 409。
pub async fn submit_setup(
    State(state): State<AppState>,
    Json(payload): Json<SetupRequestDto>,
) -> Result<Json<SetupStatusDto>, AppError> {
    let Some(database) = state.database() else {
        return Err(AppError::internal("database is not configured"));
    };

    complete_setup(database, &payload.username, &payload.password)
        .await
        .map_err(map_setup_error)?;

    Ok(Json(SetupStatusDto { initialized: true }))
}

fn map_setup_error(err: SetupError) -> AppError {
    match err {
        SetupError::AlreadyInitialized => AppError::conflict("setup already completed"),
        SetupError::InvalidUsername => AppError::unprocessable("username is required"),
        SetupError::WeakPassword => {
            AppError::unprocessable("password must be at least 12 characters")
        }
        SetupError::Database(err) => AppError::internal(format!("failed to complete setup: {err}")),
    }
}
