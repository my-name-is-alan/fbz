use axum::{
    Json,
    body::Bytes,
    extract::{Query, State},
    http::{HeaderMap, StatusCode, Uri},
};
use serde::{Deserialize, Serialize, de::DeserializeOwned};
use serde_json::{Map, Value};

use crate::{compat::emby::payload::parse_emby_body, error::AppError, state::AppState};

use super::access::authenticate_request_user;

const MAX_NOTIFICATION_TEXT_LEN: usize = 512;
const MAX_NOTIFICATION_URL_LEN: usize = 2048;
const MAX_NOTIFICATION_LEVEL_LEN: usize = 64;
const MAX_NOTIFIER_KEY_LEN: usize = 128;

#[derive(Clone, Debug, Default, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "PascalCase")]
pub struct AdminNotificationQuery {
    pub name: Option<String>,
    pub description: Option<String>,
    pub image_url: Option<String>,
    pub url: Option<String>,
    pub level: Option<String>,
}

#[derive(Clone, Debug, Default, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "PascalCase")]
pub struct AddAdminNotificationDto {
    pub display_date_time: Option<bool>,
}

#[derive(Clone, Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "PascalCase")]
pub struct NotificationCategoryInfoDto {
    pub name: String,
    pub id: String,
    pub events: Vec<NotificationTypeInfoDto>,
}

#[derive(Clone, Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "PascalCase")]
pub struct NotificationTypeInfoDto {
    pub name: String,
    pub id: String,
    pub category_name: String,
    pub category_id: String,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
#[serde(default, rename_all = "PascalCase")]
pub struct UserNotificationInfoDto {
    pub notifier_key: String,
    pub setup_module_url: String,
    pub service_name: String,
    pub plugin_id: String,
    pub friendly_name: String,
    pub id: String,
    pub enabled: bool,
    pub user_ids: Vec<String>,
    pub device_ids: Vec<String>,
    pub library_ids: Vec<String>,
    pub event_ids: Vec<String>,
    pub user_id: Option<String>,
    pub is_self_notification: bool,
    pub group_items: bool,
    pub options: Value,
}

impl Default for UserNotificationInfoDto {
    fn default() -> Self {
        Self {
            notifier_key: String::new(),
            setup_module_url: String::new(),
            service_name: String::new(),
            plugin_id: String::new(),
            friendly_name: String::new(),
            id: String::new(),
            enabled: false,
            user_ids: Vec::new(),
            device_ids: Vec::new(),
            library_ids: Vec::new(),
            event_ids: Vec::new(),
            user_id: None,
            is_self_notification: false,
            group_items: false,
            options: Value::Object(Map::new()),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct AdminNotificationInput {
    name: String,
    description: String,
    image_url: Option<String>,
    url: Option<String>,
    level: Option<String>,
    display_date_time: bool,
}

pub async fn notification_types(
    State(state): State<AppState>,
    headers: HeaderMap,
    uri: Uri,
) -> Result<Json<Vec<NotificationCategoryInfoDto>>, AppError> {
    authenticate_request_user(&state, &headers, &uri).await?;

    Ok(Json(notification_categories()))
}

pub async fn admin_notification(
    State(state): State<AppState>,
    Query(query): Query<AdminNotificationQuery>,
    headers: HeaderMap,
    uri: Uri,
    body: Bytes,
) -> Result<StatusCode, AppError> {
    authenticate_admin_user(&state, &headers, &uri).await?;
    let request: AddAdminNotificationDto = parse_optional_emby_body(&headers, &body)?;
    let _input = admin_notification_input(&query, &request)?;

    Err(AppError::conflict(
        "Emby admin notifications are managed by FBZ notification targets",
    ))
}

pub async fn service_defaults(
    State(state): State<AppState>,
    headers: HeaderMap,
    uri: Uri,
) -> Result<Json<UserNotificationInfoDto>, AppError> {
    authenticate_request_user(&state, &headers, &uri).await?;

    Ok(Json(default_notification_service()))
}

pub async fn service_test(
    State(state): State<AppState>,
    headers: HeaderMap,
    uri: Uri,
    body: Bytes,
) -> Result<StatusCode, AppError> {
    authenticate_admin_user(&state, &headers, &uri).await?;
    let request: UserNotificationInfoDto = parse_optional_emby_body(&headers, &body)?;
    let _notifier_key = normalize_optional_text(
        request.notifier_key.as_str(),
        "NotifierKey",
        MAX_NOTIFIER_KEY_LEN,
    )?;

    Err(AppError::conflict(
        "Emby notification service tests are managed by FBZ notification targets",
    ))
}

async fn authenticate_admin_user(
    state: &AppState,
    headers: &HeaderMap,
    uri: &Uri,
) -> Result<(), AppError> {
    let user = authenticate_request_user(state, headers, uri).await?;
    if !user.can_manage_server() {
        return Err(AppError::forbidden("server management permission required"));
    }

    Ok(())
}

fn notification_categories() -> Vec<NotificationCategoryInfoDto> {
    Vec::new()
}

fn default_notification_service() -> UserNotificationInfoDto {
    UserNotificationInfoDto {
        notifier_key: "fbz-host".to_owned(),
        setup_module_url: String::new(),
        service_name: "FBZ Host Notifications".to_owned(),
        plugin_id: "fbz-core".to_owned(),
        friendly_name: "FBZ Host Notifications".to_owned(),
        id: "fbz-host".to_owned(),
        enabled: false,
        group_items: true,
        options: Value::Object(Map::new()),
        ..UserNotificationInfoDto::default()
    }
}

fn admin_notification_input(
    query: &AdminNotificationQuery,
    request: &AddAdminNotificationDto,
) -> Result<AdminNotificationInput, AppError> {
    Ok(AdminNotificationInput {
        name: normalize_required_text(query.name.as_deref(), "Name", MAX_NOTIFICATION_TEXT_LEN)?,
        description: normalize_required_text(
            query.description.as_deref(),
            "Description",
            MAX_NOTIFICATION_TEXT_LEN,
        )?,
        image_url: normalize_optional_text(
            query.image_url.as_deref().unwrap_or_default(),
            "ImageUrl",
            MAX_NOTIFICATION_URL_LEN,
        )?,
        url: normalize_optional_text(
            query.url.as_deref().unwrap_or_default(),
            "Url",
            MAX_NOTIFICATION_URL_LEN,
        )?,
        level: normalize_optional_text(
            query.level.as_deref().unwrap_or_default(),
            "Level",
            MAX_NOTIFICATION_LEVEL_LEN,
        )?,
        display_date_time: request.display_date_time.unwrap_or(false),
    })
}

fn normalize_required_text(
    value: Option<&str>,
    field: &str,
    max_len: usize,
) -> Result<String, AppError> {
    normalize_optional_text(value.unwrap_or_default(), field, max_len)?
        .ok_or_else(|| AppError::unprocessable(format!("{field} is required")))
}

fn normalize_optional_text(
    value: &str,
    field: &str,
    max_len: usize,
) -> Result<Option<String>, AppError> {
    if value.chars().any(char::is_control) {
        return Err(AppError::unprocessable(format!(
            "{field} contains invalid characters"
        )));
    }

    let value = value.trim();
    if value.is_empty() {
        return Ok(None);
    }

    if value.chars().count() > max_len {
        return Err(AppError::unprocessable(format!(
            "{field} must be at most {max_len} characters"
        )));
    }

    Ok(Some(value.to_owned()))
}

fn parse_optional_emby_body<T>(headers: &HeaderMap, body: &Bytes) -> Result<T, AppError>
where
    T: DeserializeOwned + Default,
{
    if body.is_empty() {
        return Ok(T::default());
    }

    parse_emby_body(headers, body)
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    #[test]
    fn admin_notification_query_requires_name_and_description() {
        assert!(
            admin_notification_input(
                &AdminNotificationQuery {
                    description: Some("Scan finished".to_owned()),
                    ..AdminNotificationQuery::default()
                },
                &AddAdminNotificationDto::default(),
            )
            .is_err()
        );
        assert!(
            admin_notification_input(
                &AdminNotificationQuery {
                    name: Some("Library".to_owned()),
                    ..AdminNotificationQuery::default()
                },
                &AddAdminNotificationDto::default(),
            )
            .is_err()
        );
    }

    #[test]
    fn admin_notification_query_trims_and_bounds_safe_text() {
        let input = admin_notification_input(
            &AdminNotificationQuery {
                name: Some(" Library ".to_owned()),
                description: Some(" Scan finished ".to_owned()),
                image_url: Some(" https://example.test/image.png ".to_owned()),
                url: Some(" https://example.test/activity ".to_owned()),
                level: Some(" Info ".to_owned()),
            },
            &AddAdminNotificationDto {
                display_date_time: Some(true),
            },
        )
        .expect("safe admin notification query should normalize");

        assert_eq!(input.name, "Library");
        assert_eq!(input.description, "Scan finished");
        assert_eq!(
            input.image_url.as_deref(),
            Some("https://example.test/image.png")
        );
        assert_eq!(input.url.as_deref(), Some("https://example.test/activity"));
        assert_eq!(input.level.as_deref(), Some("Info"));
        assert!(input.display_date_time);

        assert!(
            admin_notification_input(
                &AdminNotificationQuery {
                    name: Some("x".repeat(MAX_NOTIFICATION_TEXT_LEN + 1)),
                    description: Some("Scan finished".to_owned()),
                    ..AdminNotificationQuery::default()
                },
                &AddAdminNotificationDto::default(),
            )
            .is_err()
        );
        assert!(
            admin_notification_input(
                &AdminNotificationQuery {
                    name: Some("Library\n".to_owned()),
                    description: Some("Scan finished".to_owned()),
                    ..AdminNotificationQuery::default()
                },
                &AddAdminNotificationDto::default(),
            )
            .is_err()
        );
    }

    #[test]
    fn default_notification_service_serializes_official_pascal_shape() {
        let value = serde_json::to_value(default_notification_service()).unwrap();

        assert_eq!(value["NotifierKey"], "fbz-host");
        assert_eq!(value["ServiceName"], "FBZ Host Notifications");
        assert_eq!(value["PluginId"], "fbz-core");
        assert_eq!(value["Enabled"], false);
        assert_eq!(value["GroupItems"], true);
        assert_eq!(value["UserIds"], json!([]));
        assert_eq!(value["DeviceIds"], json!([]));
        assert_eq!(value["LibraryIds"], json!([]));
        assert_eq!(value["EventIds"], json!([]));
        assert_eq!(value["Options"], json!({}));
    }

    #[test]
    fn notification_category_serializes_events_with_pascal_shape() {
        let category = NotificationCategoryInfoDto {
            name: "Server".to_owned(),
            id: "server".to_owned(),
            events: vec![NotificationTypeInfoDto {
                name: "Library scan completed".to_owned(),
                id: "library.scan.completed".to_owned(),
                category_name: "Server".to_owned(),
                category_id: "server".to_owned(),
            }],
        };

        let value = serde_json::to_value(category).unwrap();

        assert_eq!(
            value,
            json!({
                "Name": "Server",
                "Id": "server",
                "Events": [{
                    "Name": "Library scan completed",
                    "Id": "library.scan.completed",
                    "CategoryName": "Server",
                    "CategoryId": "server"
                }]
            })
        );
    }
}
