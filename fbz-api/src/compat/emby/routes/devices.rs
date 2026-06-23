use axum::{
    Json,
    body::Bytes,
    extract::{Query, State},
    http::{HeaderMap, StatusCode, Uri},
};
use serde::Deserialize;

use crate::{
    auth::repository::{AuthRepository, DeviceInfoRecord},
    compat::emby::{
        auth::parse_auth_context,
        dto::{ContentUploadHistoryDto, DeviceInfoDto, DeviceOptionsDto, QueryResultDto},
        payload::parse_emby_body,
    },
    error::AppError,
    state::AppState,
};

use super::access::authenticate_request_user;

#[derive(Clone, Debug, Default, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "PascalCase")]
pub struct DevicesQuery {
    pub sort_order: Option<String>,
}

#[derive(Clone, Debug, Default, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "PascalCase")]
pub struct DeviceInfoQuery {
    pub id: String,
}

const MAX_DEVICE_CUSTOM_NAME_LEN: usize = 128;

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
    headers: HeaderMap,
    uri: Uri,
) -> Result<Json<ContentUploadHistoryDto>, AppError> {
    let _user = authenticate_request_user(&state, &headers, &uri).await?;
    let auth_context = parse_auth_context(&headers, uri.query())?;

    Ok(Json(empty_camera_upload_history(
        auth_context.client.device_id.as_deref(),
    )))
}

pub async fn camera_upload_disabled(
    State(state): State<AppState>,
    headers: HeaderMap,
    uri: Uri,
) -> Result<StatusCode, AppError> {
    let _user = authenticate_request_user(&state, &headers, &uri).await?;

    Err(AppError::forbidden("camera uploads are not enabled"))
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

fn empty_camera_upload_history(device_id: Option<&str>) -> ContentUploadHistoryDto {
    ContentUploadHistoryDto {
        device_id: device_id
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .unwrap_or_default()
            .to_owned(),
        files_uploaded: Vec::new(),
    }
}

#[cfg(test)]
mod tests {
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
    fn empty_camera_upload_history_preserves_device_id() {
        let history = empty_camera_upload_history(Some(" device-1 "));

        assert_eq!(history.device_id, "device-1");
        assert!(history.files_uploaded.is_empty());
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
