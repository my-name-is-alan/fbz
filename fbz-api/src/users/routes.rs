//! 用户头像路由：`/api/users/{id}/avatar`（GET 取图 / POST 上传 / DELETE 清除）。
//!
//! 瘦控制器：鉴权 + 校验 + 存盘/读盘，元数据 SQL 在 [`crate::users::repository`]。
//! 头像二进制存磁盘（`artwork_cache_dir/avatars/<public_id>`），DB 只存 content-type 与
//! 更新时间。GET 无需鉴权（同实例部署、public_id 不可枚举，且 `<img>` 不便带令牌）；
//! POST/DELETE 要求登录用户，且只能改自己的头像或需服务器管理权限。

use axum::{
    Router,
    body::Bytes,
    extract::{Path, State},
    http::{
        HeaderMap, StatusCode, Uri,
        header::{CACHE_CONTROL, CONTENT_TYPE},
    },
    response::{IntoResponse, Response},
    routing::get,
};

use crate::{error::AppError, navigation::access::authenticate_user, state::AppState};

/// 头像最大字节数（2 MiB）——足够高清方图，又能挡住误传大文件。
const MAX_AVATAR_BYTES: usize = 2 * 1024 * 1024;

/// 允许的头像 content-type 白名单 → 落盘扩展名（这里只用作校验，文件名固定为 public_id）。
const ALLOWED_AVATAR_TYPES: &[&str] = &["image/jpeg", "image/png", "image/webp", "image/gif"];

/// GET 响应缓存策略：短缓存 + 必须重验证；URL 上的 `?v=` 才是真正的击穿手段。
const AVATAR_CACHE_CONTROL: &str = "private, max-age=60, must-revalidate";

pub fn router() -> Router<AppState> {
    Router::new().route(
        "/api/users/{id}/avatar",
        get(get_avatar).post(upload_avatar).delete(delete_avatar),
    )
}

/// 校验 UUID 形态的 public_id，挡住路径穿越（文件名直接取自它）。
fn validate_user_public_id(user_id: &str) -> Result<(), AppError> {
    let is_uuid = user_id.len() == 36
        && user_id.chars().all(|ch| ch.is_ascii_hexdigit() || ch == '-')
        && user_id.split('-').map(str::len).eq([8, 4, 4, 4, 12]);
    if is_uuid {
        Ok(())
    } else {
        Err(AppError::not_found("user not found"))
    }
}

fn avatar_path(state: &AppState, user_id: &str) -> std::path::PathBuf {
    state
        .config()
        .storage
        .artwork_cache_dir
        .join("avatars")
        .join(user_id)
}

/// `GET /api/users/{id}/avatar`：无鉴权返回头像二进制；未设置头像时 404，交前端回退首字母。
pub async fn get_avatar(
    State(state): State<AppState>,
    Path(user_id): Path<String>,
) -> Result<Response, AppError> {
    validate_user_public_id(&user_id)?;
    let Some(database) = state.database() else {
        return Err(AppError::internal("database is not configured"));
    };

    let meta = crate::users::repository::UsersRepository::new(database.clone())
        .find_avatar_meta(&user_id)
        .await
        .map_err(|err| AppError::internal(format!("failed to read avatar meta: {err}")))?;
    let Some(Some(meta)) = meta else {
        // 用户不存在或未设置头像：统一 404（不泄露用户是否存在）。
        return Err(AppError::not_found("avatar not set"));
    };

    let bytes = tokio::fs::read(avatar_path(&state, &user_id))
        .await
        .map_err(|_| AppError::not_found("avatar not set"))?;

    let mut response = (StatusCode::OK, bytes).into_response();
    let headers = response.headers_mut();
    if let Ok(value) = axum::http::HeaderValue::from_str(&meta.content_type) {
        headers.insert(CONTENT_TYPE, value);
    }
    headers.insert(
        CACHE_CONTROL,
        axum::http::HeaderValue::from_static(AVATAR_CACHE_CONTROL),
    );
    Ok(response)
}

/// 上传/删除前的授权：登录用户本人，或具备服务器管理权限。
async fn authorize_avatar_mutation(
    state: &AppState,
    headers: &HeaderMap,
    uri: &Uri,
    target_user_id: &str,
) -> Result<(), AppError> {
    let user = authenticate_user(state, headers, uri).await?;
    if user.public_id == target_user_id || user.can_manage_server() {
        Ok(())
    } else {
        Err(AppError::forbidden(
            "cannot modify another user's avatar without admin rights",
        ))
    }
}

fn sniff_content_type(body: &Bytes) -> Option<&'static str> {
    // 只用魔数嗅探，忽略客户端声明的 content-type（更可信）。
    if body.len() >= 3 && &body[0..3] == b"\xFF\xD8\xFF" {
        Some("image/jpeg")
    } else if body.len() >= 8 && &body[0..8] == b"\x89PNG\r\n\x1a\n" {
        Some("image/png")
    } else if body.len() >= 12 && &body[0..4] == b"RIFF" && &body[8..12] == b"WEBP" {
        Some("image/webp")
    } else if body.len() >= 6 && (&body[0..6] == b"GIF87a" || &body[0..6] == b"GIF89a") {
        Some("image/gif")
    } else {
        None
    }
}

/// `POST /api/users/{id}/avatar`：请求体为原始图片字节，按魔数识别类型后落盘。
pub async fn upload_avatar(
    State(state): State<AppState>,
    Path(user_id): Path<String>,
    headers: HeaderMap,
    uri: Uri,
    body: Bytes,
) -> Result<StatusCode, AppError> {
    validate_user_public_id(&user_id)?;
    authorize_avatar_mutation(&state, &headers, &uri, &user_id).await?;

    if body.is_empty() {
        return Err(AppError::unprocessable("avatar body is empty"));
    }
    if body.len() > MAX_AVATAR_BYTES {
        return Err(AppError::unprocessable("avatar exceeds 2 MiB limit"));
    }
    let Some(content_type) = sniff_content_type(&body) else {
        return Err(AppError::unprocessable(
            "avatar must be a JPEG, PNG, WebP, or GIF image",
        ));
    };
    debug_assert!(ALLOWED_AVATAR_TYPES.contains(&content_type));

    let Some(database) = state.database() else {
        return Err(AppError::internal("database is not configured"));
    };

    let dir = state.config().storage.artwork_cache_dir.join("avatars");
    tokio::fs::create_dir_all(&dir)
        .await
        .map_err(|err| AppError::internal(format!("failed to create avatar dir: {err}")))?;
    tokio::fs::write(dir.join(&user_id), &body)
        .await
        .map_err(|err| AppError::internal(format!("failed to store avatar: {err}")))?;

    let affected = crate::users::repository::UsersRepository::new(database.clone())
        .set_avatar_meta(&user_id, content_type)
        .await
        .map_err(|err| AppError::internal(format!("failed to record avatar meta: {err}")))?;
    if affected == 0 {
        // 用户不存在：回滚刚写入的文件，返回 404。
        let _ = tokio::fs::remove_file(dir.join(&user_id)).await;
        return Err(AppError::not_found("user not found"));
    }

    Ok(StatusCode::NO_CONTENT)
}

/// `DELETE /api/users/{id}/avatar`：删除文件并清空元数据，恢复首字母头像。
pub async fn delete_avatar(
    State(state): State<AppState>,
    Path(user_id): Path<String>,
    headers: HeaderMap,
    uri: Uri,
) -> Result<StatusCode, AppError> {
    validate_user_public_id(&user_id)?;
    authorize_avatar_mutation(&state, &headers, &uri, &user_id).await?;

    let Some(database) = state.database() else {
        return Err(AppError::internal("database is not configured"));
    };

    let _ = tokio::fs::remove_file(avatar_path(&state, &user_id)).await;
    crate::users::repository::UsersRepository::new(database.clone())
        .clear_avatar_meta(&user_id)
        .await
        .map_err(|err| AppError::internal(format!("failed to clear avatar meta: {err}")))?;

    Ok(StatusCode::NO_CONTENT)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn validate_user_public_id_accepts_uuid_rejects_traversal() {
        assert!(validate_user_public_id("00000000-0000-0000-0000-000000000001").is_ok());
        assert!(validate_user_public_id("../../etc/passwd").is_err());
        assert!(validate_user_public_id("not-a-uuid").is_err());
    }

    #[test]
    fn sniff_content_type_recognizes_common_images() {
        assert_eq!(sniff_content_type(&Bytes::from_static(b"\xFF\xD8\xFFxx")), Some("image/jpeg"));
        assert_eq!(
            sniff_content_type(&Bytes::from_static(b"\x89PNG\r\n\x1a\nxx")),
            Some("image/png")
        );
        assert_eq!(sniff_content_type(&Bytes::from_static(b"GIF89a...")), Some("image/gif"));
        assert_eq!(sniff_content_type(&Bytes::from_static(b"plain text")), None);
    }
}
