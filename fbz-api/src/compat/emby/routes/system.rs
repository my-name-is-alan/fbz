use axum::{
    Json,
    body::Bytes,
    extract::{Path, Query, State},
    http::{HeaderMap, StatusCode, Uri},
    response::{IntoResponse, Response},
};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

use crate::{
    auth::service::AuthenticatedUser,
    compat::emby::dto::{
        EndpointInfoDto, PublicSystemInfoDto, ServerConfigurationDto, ServerConfigurationSource,
        ServerInfoSource, SystemInfoDto, WakeOnLanInfoDto,
    },
    error::AppError,
    state::AppState,
};

use super::access::authenticate_request_user;

const MAX_SYSTEM_CONFIGURATION_KEY_LEN: usize = 128;
const MAX_SYSTEM_CONFIGURATION_BODY_BYTES: usize = 128 * 1024;
const MAX_SYSTEM_LOG_NAME_LEN: usize = 256;

/// Official Emby `LogFile` descriptor returned by `System/Logs`.
///
/// FBZ emits structured logs to stdout/stderr (captured by the container or
/// service manager) rather than to rotating on-disk log files, so the server
/// log list is intentionally empty. The DTO is still shaped exactly like the
/// official `LogFile` so the admin dashboard's log viewer renders an empty
/// list instead of choking on a missing field.
#[derive(Clone, Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "PascalCase")]
pub(crate) struct LogFileDto {
    name: String,
    size: i64,
    date_created: String,
    date_modified: String,
}

#[derive(Clone, Debug, Default, Deserialize)]
pub struct SystemLogQuery {
    #[serde(default, alias = "name")]
    pub name: Option<String>,
}

#[derive(Clone, Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub(crate) struct SystemPackageVersionInfoDto {
    name: String,
    guid: String,
    version_str: String,
    classification: SystemPackageVersionClassDto,
    description: String,
    required_version_str: String,
    source_url: String,
    checksum: String,
    target_filename: String,
    info_url: String,
    runtimes: String,
    timestamp: String,
}

#[derive(Clone, Copy, Debug, Serialize, PartialEq, Eq)]
pub(crate) enum SystemPackageVersionClassDto {
    Release,
}

pub async fn system_info(State(state): State<AppState>) -> Json<SystemInfoDto> {
    Json(SystemInfoDto::from(server_info_source(&state)))
}

pub async fn public_system_info(State(state): State<AppState>) -> Json<PublicSystemInfoDto> {
    Json(PublicSystemInfoDto::from(server_info_source(&state)))
}

pub async fn system_endpoint(
    State(state): State<AppState>,
    headers: HeaderMap,
    uri: Uri,
) -> Result<Json<EndpointInfoDto>, AppError> {
    authenticate_request_user(&state, &headers, &uri).await?;

    Ok(Json(EndpointInfoDto::conservative_default()))
}

pub async fn system_configuration(
    State(state): State<AppState>,
    headers: HeaderMap,
    uri: Uri,
) -> Result<Json<ServerConfigurationDto>, AppError> {
    authenticate_request_user(&state, &headers, &uri).await?;

    Ok(Json(ServerConfigurationDto::from(
        server_configuration_source(&state).await,
    )))
}

pub async fn system_configuration_by_key(
    State(state): State<AppState>,
    Path(config_key): Path<String>,
    headers: HeaderMap,
    uri: Uri,
) -> Result<Json<Value>, AppError> {
    authenticate_request_user(&state, &headers, &uri).await?;
    let key = normalized_configuration_key(&config_key)?;

    Ok(Json(named_configuration_value(
        &key,
        server_configuration_source(&state).await,
    )))
}

pub async fn update_system_configuration(
    State(state): State<AppState>,
    headers: HeaderMap,
    uri: Uri,
    body: Bytes,
) -> Result<StatusCode, AppError> {
    authenticate_admin_user(&state, &headers, &uri).await?;
    ensure_configuration_body_within_limit(&body)?;

    Err(AppError::conflict(
        "system configuration writes are managed by FBZ admin settings",
    ))
}

pub async fn update_system_configuration_by_key(
    State(state): State<AppState>,
    Path(config_key): Path<String>,
    headers: HeaderMap,
    uri: Uri,
    body: Bytes,
) -> Result<StatusCode, AppError> {
    authenticate_admin_user(&state, &headers, &uri).await?;
    let _key = normalized_configuration_key(&config_key)?;
    ensure_configuration_body_within_limit(&body)?;

    Err(AppError::conflict(
        "named system configuration writes are managed by FBZ admin settings",
    ))
}

pub async fn update_system_configuration_partial(
    State(state): State<AppState>,
    headers: HeaderMap,
    uri: Uri,
    body: Bytes,
) -> Result<StatusCode, AppError> {
    authenticate_admin_user(&state, &headers, &uri).await?;
    ensure_configuration_body_within_limit(&body)?;

    Err(AppError::conflict(
        "partial system configuration writes are managed by FBZ admin settings",
    ))
}

pub async fn wake_on_lan_info(
    State(state): State<AppState>,
    headers: HeaderMap,
    uri: Uri,
) -> Result<Json<Vec<WakeOnLanInfoDto>>, AppError> {
    authenticate_request_user(&state, &headers, &uri).await?;

    Ok(Json(Vec::new()))
}

pub async fn release_notes(
    State(state): State<AppState>,
    headers: HeaderMap,
    uri: Uri,
) -> Result<Json<SystemPackageVersionInfoDto>, AppError> {
    authenticate_request_user(&state, &headers, &uri).await?;

    Ok(Json(current_release_notes()))
}

pub async fn release_note_versions(
    State(state): State<AppState>,
    headers: HeaderMap,
    uri: Uri,
) -> Result<Json<Vec<SystemPackageVersionInfoDto>>, AppError> {
    authenticate_request_user(&state, &headers, &uri).await?;

    Ok(Json(vec![current_release_notes()]))
}

pub async fn system_ping() -> Response {
    (StatusCode::OK, "").into_response()
}

/// `GET System/Logs` — admin-only server log file listing.
///
/// FBZ logs to structured stdout/stderr, not rotating on-disk files, so this
/// returns an empty `LogFile[]` under the official shape. Keeping the route
/// present (instead of letting it 404) lets the Emby admin dashboard's log
/// viewer load cleanly and show "no server logs" rather than erroring.
pub async fn system_logs(
    State(state): State<AppState>,
    headers: HeaderMap,
    uri: Uri,
) -> Result<Json<Vec<LogFileDto>>, AppError> {
    authenticate_admin_user(&state, &headers, &uri).await?;

    Ok(Json(Vec::new()))
}

/// `GET System/Logs/Log?name=...` — admin-only fetch of a single named log.
///
/// The `name` is validated as a bounded, path-traversal-safe file name so a
/// malicious client cannot probe the host filesystem. Because FBZ exposes no
/// on-disk server log files, every valid request resolves to a controlled
/// not-found rather than a generic 404 or any filesystem path disclosure.
pub async fn system_log(
    State(state): State<AppState>,
    Query(query): Query<SystemLogQuery>,
    headers: HeaderMap,
    uri: Uri,
) -> Result<Response, AppError> {
    authenticate_admin_user(&state, &headers, &uri).await?;
    let name = normalized_log_name(query.name.as_deref())?;

    Err(AppError::not_found(format!(
        "server log file '{name}' is not available; FBZ logs to structured stdout"
    )))
}

/// `POST System/Restart` — admin-only server restart command.
///
/// The Emby admin dashboard exposes a "Restart" power button that POSTs here.
/// FBZ's process lifecycle is owned by the container/service manager (a node
/// restart is a supervised operation with its own graceful shutdown on signal),
/// so an Emby client must not be able to bounce the process out of band. The
/// route is admin-protected and returns a controlled conflict instead of a
/// generic 404, so the dashboard renders a clear "managed externally" message
/// rather than a missing-endpoint error or a fake success.
pub async fn system_restart(
    State(state): State<AppState>,
    headers: HeaderMap,
    uri: Uri,
) -> Result<StatusCode, AppError> {
    authenticate_admin_user(&state, &headers, &uri).await?;

    Err(AppError::conflict(
        "server restart is managed by the FBZ deployment (container/service manager), not the Emby client",
    ))
}

/// `POST System/Shutdown` — admin-only server shutdown command.
///
/// Mirrors [`system_restart`]: the dashboard's "Shutdown" power button POSTs
/// here. FBZ shuts down gracefully on process signal under its supervisor, so an
/// Emby client must not be able to halt the process out of band. Admin-protected
/// controlled conflict, never a fake success.
pub async fn system_shutdown(
    State(state): State<AppState>,
    headers: HeaderMap,
    uri: Uri,
) -> Result<StatusCode, AppError> {
    authenticate_admin_user(&state, &headers, &uri).await?;

    Err(AppError::conflict(
        "server shutdown is managed by the FBZ deployment (container/service manager), not the Emby client",
    ))
}

async fn authenticate_admin_user(
    state: &AppState,
    headers: &HeaderMap,
    uri: &Uri,
) -> Result<AuthenticatedUser, AppError> {
    let user = authenticate_request_user(state, headers, uri).await?;
    if !user.can_manage_server() {
        return Err(AppError::forbidden("server management permission required"));
    }

    Ok(user)
}

fn current_release_notes() -> SystemPackageVersionInfoDto {
    SystemPackageVersionInfoDto {
        name: "FBZ API".to_owned(),
        guid: "fbz-api".to_owned(),
        version_str: env!("CARGO_PKG_VERSION").to_owned(),
        classification: SystemPackageVersionClassDto::Release,
        description: "Current FBZ API server build.".to_owned(),
        required_version_str: "0.0.0".to_owned(),
        source_url: String::new(),
        checksum: String::new(),
        target_filename: "fbz-api".to_owned(),
        info_url: String::new(),
        runtimes: "server".to_owned(),
        timestamp: "1970-01-01T00:00:00Z".to_owned(),
    }
}

fn server_info_source(state: &AppState) -> ServerInfoSource {
    ServerInfoSource {
        id: "fbz-api".to_owned(),
        server_name: "FBZ".to_owned(),
        version: env!("CARGO_PKG_VERSION").to_owned(),
        local_address: state.config().server.public_base_url.clone(),
        operating_system: std::env::consts::OS.to_owned(),
    }
}

fn named_configuration_value(key: &str, source: ServerConfigurationSource) -> Value {
    let normalized = key.to_ascii_lowercase();
    let full_config = ServerConfigurationDto::from(source);

    match normalized.as_str() {
        "system" | "server" | "serverconfiguration" | "server-configuration" => {
            serde_json::to_value(full_config).unwrap_or_else(|_| json!({}))
        }
        "metadata" | "metadataoptions" | "metadata-options" => json!({
            "MetadataPath": full_config.metadata_path,
            "PreferredMetadataLanguage": full_config.preferred_metadata_language,
            "MetadataCountryCode": full_config.metadata_country_code,
            "EnableSavedMetadataForPeople": full_config.enable_saved_metadata_for_people
        }),
        "encoding" | "transcoding" | "transcodingoptions" | "transcoding-options" => json!({
            "SimultaneousStreamLimit": full_config.simultaneous_stream_limit,
            "RemoteClientBitrateLimit": full_config.remote_client_bitrate_limit,
            "EnableDebugLevelLogging": full_config.enable_debug_level_logging
        }),
        "branding" | "brandingoptions" | "branding-options" => json!({
            "LoginDisclaimer": "",
            "CustomCss": ""
        }),
        _ => json!({}),
    }
}

fn normalized_configuration_key(value: &str) -> Result<String, AppError> {
    let value = value.trim();
    if value.is_empty() {
        return Err(AppError::unprocessable("configuration key is required"));
    }

    if value.len() > MAX_SYSTEM_CONFIGURATION_KEY_LEN
        || !value
            .chars()
            .all(|ch| ch.is_ascii_alphanumeric() || matches!(ch, '-' | '_' | '.'))
    {
        return Err(AppError::unprocessable("configuration key is invalid"));
    }

    Ok(value.to_owned())
}

/// Validate the `name` query of `System/Logs/Log` as a bounded, path-traversal
/// safe file name. Rejects empty values, anything over the length cap, path
/// separators, parent-directory escapes and control characters so a client
/// cannot probe the host filesystem through the log-fetch route.
fn normalized_log_name(value: Option<&str>) -> Result<String, AppError> {
    let value = value.map(str::trim).unwrap_or_default();
    if value.is_empty() {
        return Err(AppError::unprocessable("log file name is required"));
    }

    if value.len() > MAX_SYSTEM_LOG_NAME_LEN
        || value.contains("..")
        || value
            .chars()
            .any(|ch| matches!(ch, '/' | '\\') || ch.is_control())
    {
        return Err(AppError::unprocessable("log file name is invalid"));
    }

    Ok(value.to_owned())
}

fn ensure_configuration_body_within_limit(body: &Bytes) -> Result<(), AppError> {
    if body.len() > MAX_SYSTEM_CONFIGURATION_BODY_BYTES {
        return Err(AppError::unprocessable(format!(
            "system configuration payload must be at most {MAX_SYSTEM_CONFIGURATION_BODY_BYTES} bytes"
        )));
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn release_notes_version_info_serializes_official_camel_case() {
        let value = serde_json::to_value(current_release_notes()).unwrap();

        assert_eq!(value["name"], "FBZ API");
        assert_eq!(value["guid"], "fbz-api");
        assert_eq!(value["versionStr"], env!("CARGO_PKG_VERSION"));
        assert_eq!(value["classification"], "Release");
        assert_eq!(value["targetFilename"], "fbz-api");
    }

    fn test_source() -> ServerConfigurationSource {
        ServerConfigurationSource {
            server_name: "FBZ".to_owned(),
            public_base_url: "https://media.example.test".to_owned(),
            http_server_port_number: 8096,
            cache_path: "./var/artwork".to_owned(),
            metadata_path: "./var/metadata".to_owned(),
            simultaneous_stream_limit: 3,
            has_users: true,
        }
    }

    #[test]
    fn named_configuration_system_key_returns_server_configuration_shape() {
        let value = named_configuration_value("system", test_source());

        assert_eq!(value["ServerName"], "FBZ");
        assert_eq!(value["HttpServerPortNumber"], 8096);
        assert_eq!(value["IsStartupWizardCompleted"], true);
    }

    #[test]
    fn named_configuration_known_slices_return_bounded_json_objects() {
        let metadata = named_configuration_value("metadata-options", test_source());
        assert_eq!(metadata["MetadataPath"], "./var/metadata");
        assert_eq!(metadata["PreferredMetadataLanguage"], "zh-CN");

        let transcoding = named_configuration_value("TranscodingOptions", test_source());
        assert_eq!(transcoding["SimultaneousStreamLimit"], 3);

        let unknown = named_configuration_value("unknown", test_source());
        assert_eq!(unknown, json!({}));
    }

    #[test]
    fn configuration_key_accepts_bounded_path_safe_values() {
        assert_eq!(
            normalized_configuration_key(" metadata-options ").unwrap(),
            "metadata-options"
        );
        assert_eq!(
            normalized_configuration_key("plugins.config_v1").unwrap(),
            "plugins.config_v1"
        );

        let err = normalized_configuration_key("").unwrap_err();
        assert_eq!(err.status_code(), StatusCode::UNPROCESSABLE_ENTITY);

        let err = normalized_configuration_key("bad/key").unwrap_err();
        assert_eq!(err.status_code(), StatusCode::UNPROCESSABLE_ENTITY);

        let err = normalized_configuration_key(&"x".repeat(MAX_SYSTEM_CONFIGURATION_KEY_LEN + 1))
            .unwrap_err();
        assert_eq!(err.status_code(), StatusCode::UNPROCESSABLE_ENTITY);
    }

    #[test]
    fn configuration_write_body_is_bounded() {
        assert!(ensure_configuration_body_within_limit(&Bytes::from_static(b"{}")).is_ok());
        assert!(
            ensure_configuration_body_within_limit(&Bytes::from(vec![
                b'x';
                MAX_SYSTEM_CONFIGURATION_BODY_BYTES
                    + 1
            ]))
            .is_err()
        );
    }

    #[test]
    fn log_name_accepts_bounded_path_safe_values() {
        assert_eq!(
            normalized_log_name(Some(" fbz-2026-06-28.log ")).unwrap(),
            "fbz-2026-06-28.log"
        );
    }

    #[test]
    fn log_name_rejects_empty_and_unsafe_values() {
        for bad in [None, Some(""), Some("   ")] {
            let err = normalized_log_name(bad).unwrap_err();
            assert_eq!(err.status_code(), StatusCode::UNPROCESSABLE_ENTITY);
        }

        for traversal in [
            "../secret",
            "..\\secret",
            "logs/app.log",
            "logs\\app.log",
            "a\0b",
            "line\nbreak",
        ] {
            let err = normalized_log_name(Some(traversal)).unwrap_err();
            assert_eq!(
                err.status_code(),
                StatusCode::UNPROCESSABLE_ENTITY,
                "expected {traversal} to be rejected"
            );
        }

        let err = normalized_log_name(Some(&"x".repeat(MAX_SYSTEM_LOG_NAME_LEN + 1))).unwrap_err();
        assert_eq!(err.status_code(), StatusCode::UNPROCESSABLE_ENTITY);
    }
}

async fn server_configuration_source(state: &AppState) -> ServerConfigurationSource {
    // 初始化状态随用户数动态变化；DB 不可用或查询失败时退守 false（视为未初始化），
    // 避免误报"已完成向导"把首次部署者挡在外面。
    let has_users = match state.database() {
        Some(database) => crate::setup::service::has_any_user(database)
            .await
            .unwrap_or(false),
        None => false,
    };
    ServerConfigurationSource {
        server_name: "FBZ".to_owned(),
        public_base_url: state.config().server.public_base_url.clone(),
        http_server_port_number: i32::from(state.config().server.port),
        cache_path: state
            .config()
            .storage
            .artwork_cache_dir
            .display()
            .to_string(),
        metadata_path: state
            .config()
            .storage
            .artwork_cache_dir
            .display()
            .to_string(),
        simultaneous_stream_limit: i32::from(state.config().transcode.max_concurrent),
        has_users,
    }
}
