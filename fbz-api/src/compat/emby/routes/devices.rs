use axum::{
    Json,
    body::Bytes,
    extract::{Query, State},
    http::{HeaderMap, StatusCode, Uri},
};
use serde::Deserialize;

use crate::{
    auth::repository::{AuthRepository, DeviceInfoRecord, InsertCameraUploadInput},
    compat::emby::{
        auth::parse_auth_context,
        dto::{
            ContentUploadHistoryDto, DeviceInfoDto, DeviceOptionsDto, LocalFileInfoDto,
            QueryResultDto,
        },
        payload::parse_emby_body,
    },
    error::AppError,
    state::AppState,
};

use super::access::authenticate_request_user;

#[derive(Clone, Debug, Default, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "PascalCase")]
pub struct DevicesQuery {
    #[serde(alias = "sortOrder", alias = "sort_order")]
    pub sort_order: Option<String>,
}

#[derive(Clone, Debug, Default, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "PascalCase")]
pub struct DeviceInfoQuery {
    #[serde(alias = "id")]
    pub id: String,
}

#[derive(Clone, Debug, Default, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "PascalCase")]
pub struct CameraUploadQuery {
    #[serde(alias = "deviceId", alias = "device_id")]
    pub device_id: Option<String>,
    #[serde(alias = "album")]
    pub album: Option<String>,
    #[serde(alias = "name")]
    pub name: Option<String>,
    #[serde(alias = "id")]
    pub id: Option<String>,
}

const MAX_DEVICE_CUSTOM_NAME_LEN: usize = 128;
const MAX_CAMERA_UPLOAD_HISTORY: i64 = 10_000;
const MAX_CAMERA_UPLOAD_BYTES: usize = 256 * 1024 * 1024;
const MAX_CAMERA_UPLOAD_TEXT_LEN: usize = 256;

pub async fn list_devices(
    State(state): State<AppState>,
    Query(query): Query<DevicesQuery>,
    headers: HeaderMap,
    uri: Uri,
) -> Result<Json<QueryResultDto<DeviceInfoDto>>, AppError> {
    let user = authenticate_request_user(&state, &headers, &uri).await?;
    ensure_device_admin(&user)?;
    let sort_descending = normalize_sort_descending(query.sort_order.as_deref());

    let Some(database) = state.database() else {
        return Err(AppError::internal("database is not configured"));
    };

    let devices = AuthRepository::new(database.clone())
        .list_devices(sort_descending)
        .await
        .map_err(|err| AppError::internal(format!("failed to list devices: {err}")))?
        .into_iter()
        .map(device_record_to_dto)
        .collect::<Vec<_>>();
    let total_record_count = devices.len() as u32;

    Ok(Json(QueryResultDto::new(devices, total_record_count, 0)))
}

pub async fn device_info(
    State(state): State<AppState>,
    Query(query): Query<DeviceInfoQuery>,
    headers: HeaderMap,
    uri: Uri,
) -> Result<Json<DeviceInfoDto>, AppError> {
    let user = authenticate_request_user(&state, &headers, &uri).await?;
    ensure_device_admin(&user)?;
    let device_id = normalize_device_query_id(&query.id)?;

    let Some(database) = state.database() else {
        return Err(AppError::internal("database is not configured"));
    };

    let Some(device) = AuthRepository::new(database.clone())
        .find_device_info(device_id)
        .await
        .map_err(|err| AppError::internal(format!("failed to get device info: {err}")))?
    else {
        return Err(AppError::not_found("device not found"));
    };

    Ok(Json(device_record_to_dto(device)))
}

pub async fn device_options(
    State(state): State<AppState>,
    Query(query): Query<DeviceInfoQuery>,
    headers: HeaderMap,
    uri: Uri,
) -> Result<Json<DeviceOptionsDto>, AppError> {
    let user = authenticate_request_user(&state, &headers, &uri).await?;
    ensure_device_admin(&user)?;
    let device_id = normalize_device_query_id(&query.id)?;

    let Some(database) = state.database() else {
        return Err(AppError::internal("database is not configured"));
    };

    let Some(device) = AuthRepository::new(database.clone())
        .find_device_info(device_id)
        .await
        .map_err(|err| AppError::internal(format!("failed to get device options: {err}")))?
    else {
        return Err(AppError::not_found("device not found"));
    };

    Ok(Json(device_options_from_record(&device)))
}

pub async fn update_device_options(
    State(state): State<AppState>,
    Query(query): Query<DeviceInfoQuery>,
    headers: HeaderMap,
    uri: Uri,
    body: Bytes,
) -> Result<StatusCode, AppError> {
    let user = authenticate_request_user(&state, &headers, &uri).await?;
    ensure_device_admin(&user)?;
    let device_id = normalize_device_query_id(&query.id)?;
    let request: DeviceOptionsDto = parse_emby_body(&headers, &body)?;
    let custom_name = normalize_custom_name(request.custom_name.as_deref())?;

    let Some(database) = state.database() else {
        return Err(AppError::internal("database is not configured"));
    };

    let updated = AuthRepository::new(database.clone())
        .update_device_custom_name(device_id, custom_name.as_deref())
        .await
        .map_err(|err| AppError::internal(format!("failed to update device options: {err}")))?;

    if !updated {
        return Err(AppError::not_found("device not found"));
    }

    Ok(StatusCode::OK)
}

pub async fn delete_device(
    State(state): State<AppState>,
    Query(query): Query<DeviceInfoQuery>,
    headers: HeaderMap,
    uri: Uri,
) -> Result<StatusCode, AppError> {
    let user = authenticate_request_user(&state, &headers, &uri).await?;
    ensure_device_admin(&user)?;
    let device_id = normalize_device_query_id(&query.id)?;

    let Some(database) = state.database() else {
        return Err(AppError::internal("database is not configured"));
    };

    let revoked = AuthRepository::new(database.clone())
        .revoke_device(device_id)
        .await
        .map_err(|err| AppError::internal(format!("failed to delete device: {err}")))?;

    if !revoked {
        return Err(AppError::not_found("device not found"));
    }

    Ok(StatusCode::OK)
}

pub async fn camera_upload_history(
    State(state): State<AppState>,
    Query(query): Query<CameraUploadQuery>,
    headers: HeaderMap,
    uri: Uri,
) -> Result<Json<ContentUploadHistoryDto>, AppError> {
    let user = authenticate_request_user(&state, &headers, &uri).await?;
    let auth_context = parse_auth_context(&headers, uri.query())?;
    let device_id = resolve_camera_device_id(&query, auth_context.client.device_id.as_deref())?;

    let Some(database) = state.database() else {
        return Err(AppError::internal("database is not configured"));
    };
    let repository = AuthRepository::new(database.clone());

    // 设备不存在时返回空历史（客户端首次上传前会先查询历史）。
    let Some(device) = repository
        .find_device_info(&device_id)
        .await
        .map_err(|err| AppError::internal(format!("failed to get device: {err}")))?
    else {
        return Ok(Json(ContentUploadHistoryDto {
            device_id,
            files_uploaded: Vec::new(),
        }));
    };
    ensure_camera_device_owner(&user, &device)?;

    let files_uploaded = repository
        .list_camera_uploads(device.internal_id, MAX_CAMERA_UPLOAD_HISTORY)
        .await
        .map_err(|err| AppError::internal(format!("failed to list camera uploads: {err}")))?
        .into_iter()
        .map(|record| LocalFileInfoDto {
            name: record.name,
            id: record.upload_id,
            album: record.album,
            mime_type: record.mime_type,
            date_created: record.created_at,
        })
        .collect();

    Ok(Json(ContentUploadHistoryDto {
        device_id,
        files_uploaded,
    }))
}

pub async fn camera_upload(
    State(state): State<AppState>,
    Query(query): Query<CameraUploadQuery>,
    headers: HeaderMap,
    uri: Uri,
    body: Bytes,
) -> Result<StatusCode, AppError> {
    let user = authenticate_request_user(&state, &headers, &uri).await?;
    let auth_context = parse_auth_context(&headers, uri.query())?;
    let device_id = resolve_camera_device_id(&query, auth_context.client.device_id.as_deref())?;
    let input = camera_upload_input(&query, &headers, body.len())?;
    if body.is_empty() {
        return Err(AppError::unprocessable("upload body is empty"));
    }

    let Some(database) = state.database() else {
        return Err(AppError::internal("database is not configured"));
    };
    let repository = AuthRepository::new(database.clone());
    let device = repository
        .find_device_info(&device_id)
        .await
        .map_err(|err| AppError::internal(format!("failed to get device: {err}")))?
        .ok_or_else(|| AppError::not_found("device not found"))?;
    ensure_camera_device_owner(&user, &device)?;

    // 落盘：{camera_upload_dir}/{device 安全名}/{album 安全名}/{name 安全名}
    let mut target_dir = state.config().storage.camera_upload_dir.clone();
    target_dir.push(sanitize_camera_path_segment(&device.reported_device_id)?);
    if !input.album.is_empty() {
        target_dir.push(sanitize_camera_path_segment(&input.album)?);
    }
    tokio::fs::create_dir_all(&target_dir)
        .await
        .map_err(|err| AppError::internal(format!("failed to create upload directory: {err}")))?;
    let file_name = sanitize_camera_path_segment(&input.name)?;
    let target_path = target_dir.join(&file_name);
    tokio::fs::write(&target_path, &body)
        .await
        .map_err(|err| AppError::internal(format!("failed to store upload: {err}")))?;

    repository
        .upsert_camera_upload(InsertCameraUploadInput {
            device_internal_id: device.internal_id,
            album: input.album,
            name: input.name,
            upload_id: input.upload_id,
            mime_type: input.mime_type,
            file_path: target_path.to_string_lossy().into_owned(),
            size_bytes: body.len() as i64,
        })
        .await
        .map_err(|err| AppError::internal(format!("failed to record upload: {err}")))?;

    Ok(StatusCode::OK)
}

fn ensure_device_admin(user: &crate::auth::service::AuthenticatedUser) -> Result<(), AppError> {
    if user.can_manage_server() {
        return Ok(());
    }

    Err(AppError::forbidden(
        "authenticated user cannot manage devices",
    ))
}

fn normalize_sort_descending(value: Option<&str>) -> bool {
    !matches!(
        value
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(str::to_ascii_lowercase)
            .as_deref(),
        Some("ascending")
    )
}

fn normalize_device_query_id(value: &str) -> Result<&str, AppError> {
    let value = value.trim();
    if value.is_empty() {
        return Err(AppError::unprocessable("device Id is required"));
    }

    if value.len() > 256 {
        return Err(AppError::unprocessable("device Id is too long"));
    }

    Ok(value)
}

fn normalize_custom_name(value: Option<&str>) -> Result<Option<String>, AppError> {
    let Some(value) = value.map(str::trim).filter(|value| !value.is_empty()) else {
        return Ok(None);
    };

    if value.chars().count() > MAX_DEVICE_CUSTOM_NAME_LEN {
        return Err(AppError::unprocessable(format!(
            "device CustomName must be at most {MAX_DEVICE_CUSTOM_NAME_LEN} characters"
        )));
    }

    Ok(Some(value.to_owned()))
}

fn device_record_to_dto(record: DeviceInfoRecord) -> DeviceInfoDto {
    let fallback_name = record.reported_device_id.clone();

    DeviceInfoDto {
        name: record.name.unwrap_or(fallback_name),
        id: record.public_id,
        internal_id: record.internal_id,
        reported_device_id: record.reported_device_id,
        last_user_name: record.last_user_name,
        app_name: record.app_name.unwrap_or_default(),
        app_version: record.app_version.unwrap_or_default(),
        last_user_id: record.last_user_id,
        date_last_activity: record.date_last_activity,
        icon_url: record.icon_url,
        ip_address: None,
    }
}

fn device_options_from_record(record: &DeviceInfoRecord) -> DeviceOptionsDto {
    DeviceOptionsDto {
        custom_name: record.name.clone(),
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct CameraUploadInput {
    album: String,
    name: String,
    upload_id: String,
    mime_type: String,
}

/// 上传目标设备：query `DeviceId` 优先，缺省回退认证上下文携带的设备 id。
fn resolve_camera_device_id(
    query: &CameraUploadQuery,
    context_device_id: Option<&str>,
) -> Result<String, AppError> {
    let device_id = query
        .device_id
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .or_else(|| {
            context_device_id
                .map(str::trim)
                .filter(|value| !value.is_empty())
        })
        .ok_or_else(|| AppError::unprocessable("DeviceId is required"))?;
    if device_id.len() > 256 {
        return Err(AppError::unprocessable("DeviceId is too long"));
    }

    Ok(device_id.to_owned())
}

/// 相机上传只允许设备属主本人或服务器管理员，防止跨设备写入他人相册。
fn ensure_camera_device_owner(
    user: &crate::auth::service::AuthenticatedUser,
    device: &DeviceInfoRecord,
) -> Result<(), AppError> {
    if user.can_manage_server() || device.last_user_id == user.public_id {
        return Ok(());
    }

    Err(AppError::forbidden(
        "authenticated user does not own this device",
    ))
}

fn camera_upload_input(
    query: &CameraUploadQuery,
    headers: &HeaderMap,
    body_len: usize,
) -> Result<CameraUploadInput, AppError> {
    if body_len > MAX_CAMERA_UPLOAD_BYTES {
        return Err(AppError::unprocessable("upload body is too large"));
    }
    let name = query
        .name
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| AppError::unprocessable("Name is required"))?;
    if name.chars().count() > MAX_CAMERA_UPLOAD_TEXT_LEN {
        return Err(AppError::unprocessable("Name is too long"));
    }
    let album = query.album.as_deref().map(str::trim).unwrap_or_default();
    if album.chars().count() > MAX_CAMERA_UPLOAD_TEXT_LEN {
        return Err(AppError::unprocessable("Album is too long"));
    }
    let upload_id = query.id.as_deref().map(str::trim).unwrap_or_default();
    if upload_id.chars().count() > MAX_CAMERA_UPLOAD_TEXT_LEN {
        return Err(AppError::unprocessable("Id is too long"));
    }
    let mime_type = headers
        .get(axum::http::header::CONTENT_TYPE)
        .and_then(|value| value.to_str().ok())
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .unwrap_or("application/octet-stream")
        .to_owned();

    Ok(CameraUploadInput {
        album: album.to_owned(),
        name: name.to_owned(),
        upload_id: upload_id.to_owned(),
        mime_type,
    })
}

/// 把客户端提供的名字规约成安全的单层文件/目录名：拒绝路径分隔符、`..`、
/// 控制字符和 Windows 保留字符，防止逃出上传根目录。
fn sanitize_camera_path_segment(value: &str) -> Result<String, AppError> {
    let value = value.trim();
    if value.is_empty() || value == "." || value == ".." {
        return Err(AppError::unprocessable("path segment is invalid"));
    }
    if value.chars().any(|ch| {
        ch.is_control() || matches!(ch, '/' | '\\' | ':' | '*' | '?' | '"' | '<' | '>' | '|')
    }) {
        return Err(AppError::unprocessable("path segment contains invalid characters"));
    }

    Ok(value.to_owned())
}

#[cfg(test)]
mod tests {
    use axum::extract::Query;
    use http::Uri;

    use super::*;
    use crate::auth::service::AuthenticatedUser;

    #[test]
    fn sort_order_defaults_to_descending_and_accepts_ascending() {
        assert!(normalize_sort_descending(None));
        assert!(normalize_sort_descending(Some("Descending")));
        assert!(normalize_sort_descending(Some("unknown")));
        assert!(!normalize_sort_descending(Some(" Ascending ")));
    }

    #[test]
    fn device_queries_accept_lower_camel_client_fields() {
        let uri = "/Devices?sortOrder=Ascending".parse::<Uri>().unwrap();
        let Query(query) = Query::<DevicesQuery>::try_from_uri(&uri).unwrap();
        assert_eq!(query.sort_order.as_deref(), Some("Ascending"));
        assert!(!normalize_sort_descending(query.sort_order.as_deref()));

        let uri = "/Devices/Info?id=device-1".parse::<Uri>().unwrap();
        let Query(query) = Query::<DeviceInfoQuery>::try_from_uri(&uri).unwrap();
        assert_eq!(normalize_device_query_id(&query.id).unwrap(), "device-1");
    }

    #[test]
    fn device_admin_boundary_requires_server_manager() {
        let admin = test_user("administrator");
        let normal_user = test_user("user");

        assert!(ensure_device_admin(&admin).is_ok());
        assert!(ensure_device_admin(&normal_user).is_err());
    }

    #[test]
    fn device_query_id_is_trimmed_and_bounded() {
        assert_eq!(normalize_device_query_id(" device-1 ").unwrap(), "device-1");
        assert!(normalize_device_query_id("").is_err());
        assert!(normalize_device_query_id(&"x".repeat(257)).is_err());
    }

    #[test]
    fn custom_name_is_trimmed_bounded_and_nullable() {
        assert_eq!(normalize_custom_name(None).unwrap(), None);
        assert_eq!(normalize_custom_name(Some("   ")).unwrap(), None);
        assert_eq!(
            normalize_custom_name(Some(" Living Room "))
                .unwrap()
                .as_deref(),
            Some("Living Room")
        );
        assert!(normalize_custom_name(Some(&"x".repeat(129))).is_err());
    }

    #[test]
    fn device_record_maps_to_emby_shape_with_safe_defaults() {
        let dto = device_record_to_dto(DeviceInfoRecord {
            internal_id: 42,
            public_id: "device-public-id".to_owned(),
            reported_device_id: "reported-device".to_owned(),
            name: None,
            last_user_name: "admin".to_owned(),
            app_name: Some("Infuse".to_owned()),
            app_version: None,
            last_user_id: "user-1".to_owned(),
            date_last_activity: Some("2026-06-22 12:00:00+00".to_owned()),
            icon_url: Some("https://example.test/icon.png".to_owned()),
        });

        assert_eq!(dto.id, "device-public-id");
        assert_eq!(dto.name, "reported-device");
        assert_eq!(dto.reported_device_id, "reported-device");
        assert_eq!(dto.app_name, "Infuse");
        assert_eq!(dto.app_version, "");
        assert_eq!(dto.internal_id, 42);
    }

    #[test]
    fn device_options_use_current_custom_name() {
        let options = device_options_from_record(&DeviceInfoRecord {
            internal_id: 42,
            public_id: "device-public-id".to_owned(),
            reported_device_id: "reported-device".to_owned(),
            name: Some("Living Room".to_owned()),
            last_user_name: "admin".to_owned(),
            app_name: None,
            app_version: None,
            last_user_id: "user-1".to_owned(),
            date_last_activity: None,
            icon_url: None,
        });

        assert_eq!(options.custom_name.as_deref(), Some("Living Room"));
    }

    #[test]
    fn camera_device_id_prefers_query_over_context() {
        let device_id = resolve_camera_device_id(
            &CameraUploadQuery {
                device_id: Some(" device-1 ".to_owned()),
                ..CameraUploadQuery::default()
            },
            Some("context-device"),
        )
        .unwrap();
        assert_eq!(device_id, "device-1");

        let fallback =
            resolve_camera_device_id(&CameraUploadQuery::default(), Some(" context-device "))
                .unwrap();
        assert_eq!(fallback, "context-device");

        assert!(resolve_camera_device_id(&CameraUploadQuery::default(), None).is_err());
    }

    #[test]
    fn camera_path_segments_reject_traversal_and_separators() {
        assert!(sanitize_camera_path_segment("..").is_err());
        assert!(sanitize_camera_path_segment("a/b").is_err());
        assert!(sanitize_camera_path_segment("a\\b").is_err());
        assert!(sanitize_camera_path_segment("photo:1.jpg").is_err());
        assert_eq!(
            sanitize_camera_path_segment(" photo.jpg ").unwrap(),
            "photo.jpg"
        );
    }

    #[test]
    fn camera_upload_input_requires_name_and_bounds_size() {
        let headers = HeaderMap::new();
        let query = CameraUploadQuery {
            device_id: Some("device-1".to_owned()),
            album: Some(" Camera ".to_owned()),
            name: Some(" photo.jpg ".to_owned()),
            id: Some(" file-1 ".to_owned()),
        };

        let input = camera_upload_input(&query, &headers, 1024).unwrap();
        assert_eq!(input.album, "Camera");
        assert_eq!(input.name, "photo.jpg");
        assert_eq!(input.upload_id, "file-1");
        assert_eq!(input.mime_type, "application/octet-stream");

        let missing_name = CameraUploadQuery {
            name: None,
            ..query.clone()
        };
        assert!(camera_upload_input(&missing_name, &headers, 1024).is_err());
        assert!(camera_upload_input(&query, &headers, MAX_CAMERA_UPLOAD_BYTES + 1).is_err());
    }

    fn test_user(role_name_normalized: &str) -> AuthenticatedUser {
        AuthenticatedUser {
            id: 1,
            public_id: "user-1".to_owned(),
            username: "tester".to_owned(),
            role_name: role_name_normalized.to_owned(),
            role_name_normalized: role_name_normalized.to_owned(),
        }
    }
}
