use axum::{
    Json,
    body::Bytes,
    extract::{Path, State},
    http::{HeaderMap, StatusCode, Uri},
    response::{IntoResponse, Response},
};
use serde::Serialize;
use serde_json::Value;

use crate::{
    error::AppError,
    plugins::repository::{PluginRepository, PluginSummaryRecord},
    state::AppState,
};

use super::access::authenticate_request_user;

const MAX_EMBY_PLUGIN_LIST_LIMIT: i64 = 200;
const MAX_EMBY_PLUGIN_CONFIG_BODY_BYTES: usize = 64 * 1024;

#[derive(Clone, Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "PascalCase")]
pub struct PluginInfoDto {
    pub name: String,
    pub version: String,
    pub configuration_file_name: String,
    pub description: String,
    pub id: String,
    pub image_tag: Option<String>,
}

pub async fn plugins(
    State(state): State<AppState>,
    headers: HeaderMap,
    uri: Uri,
) -> Result<Json<Vec<PluginInfoDto>>, AppError> {
    authenticate_admin_compatible(&state, &headers, &uri).await?;
    let Some(database) = state.database() else {
        return Err(AppError::internal("database is not configured"));
    };

    let records = PluginRepository::new(database.clone())
        .list_plugins(MAX_EMBY_PLUGIN_LIST_LIMIT)
        .await
        .map_err(|err| AppError::internal(format!("failed to list plugins: {err}")))?;

    Ok(Json(
        records.into_iter().map(plugin_info_from_record).collect(),
    ))
}

pub async fn plugin_configuration(
    State(state): State<AppState>,
    Path(plugin_id): Path<String>,
    headers: HeaderMap,
    uri: Uri,
) -> Result<Json<Value>, AppError> {
    authenticate_admin_compatible(&state, &headers, &uri).await?;
    let plugin_id = validate_plugin_path_id(&plugin_id)?;
    let Some(database) = state.database() else {
        return Err(AppError::internal("database is not configured"));
    };

    let Some(config) = PluginRepository::new(database.clone())
        .get_plugin_config(plugin_id)
        .await
        .map_err(|err| AppError::internal(format!("failed to get plugin config: {err}")))?
    else {
        return Err(AppError::not_found("plugin config not found"));
    };

    Ok(Json(config.values))
}

pub async fn update_plugin_configuration(
    State(state): State<AppState>,
    Path(plugin_id): Path<String>,
    headers: HeaderMap,
    uri: Uri,
    body: Bytes,
) -> Result<Response, AppError> {
    authenticate_admin_compatible(&state, &headers, &uri).await?;
    validate_plugin_path_id(&plugin_id)?;
    validate_plugin_config_body(&body)?;

    Err(AppError::conflict(
        "plugin configuration updates must use the FBZ admin plugin API",
    ))
}

pub async fn plugin_thumb(
    State(state): State<AppState>,
    Path(plugin_id): Path<String>,
    headers: HeaderMap,
    uri: Uri,
) -> Result<Response, AppError> {
    authenticate_request_user(&state, &headers, &uri).await?;
    validate_plugin_path_id(&plugin_id)?;

    Ok((StatusCode::NO_CONTENT, "").into_response())
}

pub async fn delete_plugin(
    State(state): State<AppState>,
    Path(plugin_id): Path<String>,
    headers: HeaderMap,
    uri: Uri,
) -> Result<Response, AppError> {
    authenticate_admin_compatible(&state, &headers, &uri).await?;
    validate_plugin_path_id(&plugin_id)?;

    Err(AppError::conflict(
        "plugin uninstall must use the FBZ admin plugin lifecycle",
    ))
}

async fn authenticate_admin_compatible(
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

fn plugin_info_from_record(record: PluginSummaryRecord) -> PluginInfoDto {
    let version = record
        .package_version
        .clone()
        .unwrap_or_else(|| "0.0.0".to_owned());
    let plugin_name = record
        .name
        .clone()
        .unwrap_or_else(|| record.plugin_id.clone());
    let runtime = record.runtime.as_deref().unwrap_or("unknown");
    let package_status = record.package_status.as_deref().unwrap_or("not_installed");
    let enabled = if record.enabled {
        "enabled"
    } else {
        "disabled"
    };

    PluginInfoDto {
        name: plugin_name,
        version,
        configuration_file_name: format!("{}.json", record.plugin_id),
        description: format!("FBZ plugin ({runtime}, {package_status}, {enabled})"),
        id: record.plugin_id,
        image_tag: record.package_id,
    }
}

fn validate_plugin_path_id(value: &str) -> Result<&str, AppError> {
    let value = value.trim();
    if value.is_empty()
        || value.len() > 200
        || value
            .chars()
            .any(|ch| ch.is_whitespace() || matches!(ch, '/' | '\\'))
    {
        return Err(AppError::unprocessable("invalid plugin id"));
    }

    Ok(value)
}

fn validate_plugin_config_body(body: &Bytes) -> Result<(), AppError> {
    if body.len() > MAX_EMBY_PLUGIN_CONFIG_BODY_BYTES {
        return Err(AppError::unprocessable(
            "plugin configuration payload is too large",
        ));
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn plugin_record() -> PluginSummaryRecord {
        PluginSummaryRecord {
            installation_id: "install-1".to_owned(),
            plugin_id: "dev.fbz.notify".to_owned(),
            package_id: Some("package-1".to_owned()),
            package_version: Some("1.2.3".to_owned()),
            package_status: Some("approved".to_owned()),
            approval_status: "approved".to_owned(),
            enabled: true,
            name: Some("Notify".to_owned()),
            runtime: Some("http".to_owned()),
        }
    }

    #[test]
    fn plugin_info_mapping_preserves_emby_shape() {
        let dto = plugin_info_from_record(plugin_record());

        assert_eq!(dto.id, "dev.fbz.notify");
        assert_eq!(dto.name, "Notify");
        assert_eq!(dto.version, "1.2.3");
        assert_eq!(dto.configuration_file_name, "dev.fbz.notify.json");
        assert_eq!(dto.image_tag.as_deref(), Some("package-1"));
        assert!(dto.description.contains("http"));
    }

    #[test]
    fn plugin_info_serializes_pascal_case_keys() {
        let value = serde_json::to_value(plugin_info_from_record(plugin_record())).unwrap();

        assert_eq!(value["Name"], "Notify");
        assert_eq!(value["Version"], "1.2.3");
        assert_eq!(value["ConfigurationFileName"], "dev.fbz.notify.json");
        assert_eq!(value["Id"], "dev.fbz.notify");
        assert_eq!(value["ImageTag"], "package-1");
    }

    #[test]
    fn plugin_path_id_rejects_empty_or_path_like_values() {
        assert_eq!(
            validate_plugin_path_id(" dev.fbz.notify ").unwrap(),
            "dev.fbz.notify"
        );
        assert!(validate_plugin_path_id("").is_err());
        assert!(validate_plugin_path_id("dev fbz").is_err());
        assert!(validate_plugin_path_id("dev/fbz").is_err());
        assert!(validate_plugin_path_id("dev\\fbz").is_err());
    }

    #[test]
    fn plugin_config_body_is_bounded() {
        assert!(validate_plugin_config_body(&Bytes::from_static(b"{}")).is_ok());
        assert!(
            validate_plugin_config_body(&Bytes::from(vec![
                b'a';
                MAX_EMBY_PLUGIN_CONFIG_BODY_BYTES + 1
            ]))
            .is_err()
        );
    }
}
