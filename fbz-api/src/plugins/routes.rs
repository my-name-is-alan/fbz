use std::{
    collections::BTreeSet,
    fs as std_fs,
    io::ErrorKind,
    path::{Component, Path as FsPath, PathBuf},
};

use axum::{
    Json, Router,
    extract::{Path, Query, State},
    http::{HeaderMap, StatusCode, Uri},
    routing::{get, post},
};
use ed25519_dalek::{Signature, Verifier, VerifyingKey};
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};
use sha2::{Digest, Sha256};
use tokio::{fs, io::AsyncReadExt, task};
use zip::ZipArchive;

use crate::{
    admin::access::authenticate_admin,
    config::PluginConfig as PluginRuntimeConfig,
    error::AppError,
    notifications::secrets::{SecretCipher, secret_ref, secret_ref_key},
    plugins::{
        host::{PluginCapabilitiesDto, plugin_capabilities},
        manifest::{
            PluginConfigFieldManifest, PluginConfigOptionManifest, PluginManifest,
            PluginManifestError, ValidatedPluginManifest,
        },
        repository::{
            ActivePluginMenuItemRecord, InstallPluginPackageInput, InstalledPluginPackageRecord,
            PluginConfigRecord, PluginConfigSecretInput, PluginConfigSecretUpdate,
            PluginConfigUpdateError, PluginHookRecord, PluginMenuItemRecord,
            PluginPackageDetailRecord, PluginPermissionRecord, PluginRepository,
            PluginScheduleDefinitionRecord, PluginStateError, PluginStateRecord,
            PluginSummaryRecord,
        },
    },
    state::AppState,
};

const PLUGIN_PACKAGE_MANIFEST_PATH: &str = "manifest.json";
const PLUGIN_PACKAGE_EXTRACTED_DIR: &str = "extracted";
const MAX_PLUGIN_ZIP_ENTRIES: usize = 4096;
const MAX_PLUGIN_ZIP_UNCOMPRESSED_BYTES: u64 = 256 * 1024 * 1024;
const PLUGIN_PACKAGE_SIGNATURE_SCHEME: &str = "ed25519";
const PLUGIN_PACKAGE_SIGNATURE_CONTEXT: &str = "fbz-plugin-package-v1";

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/api/admin/plugins", get(list_plugins))
        .route("/api/admin/plugins/capabilities", get(admin_capabilities))
        .route("/api/admin/plugins/menu-items", get(list_menu_items))
        .route("/api/admin/plugins/packages", post(install_package))
        .route(
            "/api/admin/plugins/packages/{package_id}",
            get(package_detail),
        )
        .route(
            "/api/admin/plugins/packages/{package_id}/approve",
            post(approve_package),
        )
        .route(
            "/api/admin/plugins/packages/{package_id}/reject",
            post(reject_package),
        )
        .route(
            "/api/admin/plugins/packages/{package_id}/activate",
            post(activate_package),
        )
        .route("/api/admin/plugins/{plugin_id}/enable", post(enable_plugin))
        .route(
            "/api/admin/plugins/{plugin_id}/disable",
            post(disable_plugin),
        )
        .route(
            "/api/admin/plugins/{plugin_id}/config",
            get(plugin_config).put(update_plugin_config),
        )
}

#[derive(Clone, Debug, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct InstallPluginPackageRequestDto {
    pub package_path: String,
    pub manifest: PluginManifest,
    #[serde(default)]
    pub checksum_sha256: Option<String>,
    #[serde(default)]
    pub signature: Option<String>,
}

#[derive(Clone, Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct InstalledPluginPackageDto {
    pub package_id: String,
    pub plugin_id: String,
    pub package_version: String,
    pub package_status: String,
    pub approval_status: String,
}

#[derive(Clone, Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct PluginStateDto {
    pub plugin_id: String,
    pub package_id: Option<String>,
    pub package_version: Option<String>,
    pub package_status: Option<String>,
    pub approval_status: String,
    pub enabled: bool,
}

#[derive(Clone, Debug, Default, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ListPluginsQueryDto {
    pub limit: Option<u32>,
}

#[derive(Clone, Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct PluginSummaryDto {
    pub plugin_id: String,
    pub package_id: Option<String>,
    pub package_version: Option<String>,
    pub package_status: Option<String>,
    pub approval_status: String,
    pub enabled: bool,
    pub name: Option<String>,
    pub runtime: Option<String>,
}

#[derive(Clone, Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct PluginPackageDetailDto {
    pub package_id: String,
    pub plugin_id: String,
    pub package_version: String,
    pub api_version: String,
    pub runtime: String,
    pub name: String,
    pub description: Option<String>,
    pub entrypoint: String,
    pub package_path: String,
    pub package_status: String,
    pub signature_present: bool,
    pub approval_status: Option<String>,
    pub enabled: Option<bool>,
    pub permissions: Vec<PluginPermissionDto>,
    pub hooks: Vec<PluginHookDto>,
    pub menu: Vec<PluginMenuItemDto>,
    pub schedules: Vec<PluginScheduleDefinitionDto>,
}

#[derive(Clone, Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct PluginPermissionDto {
    pub permission_key: String,
    pub permission_scope: Option<String>,
    pub reason: Option<String>,
}

#[derive(Clone, Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct PluginHookDto {
    pub event_key: String,
    pub handler: String,
    pub priority: i32,
    pub enabled: bool,
}

#[derive(Clone, Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct PluginMenuItemDto {
    pub item_key: String,
    pub label: String,
    pub path: String,
    pub parent_key: Option<String>,
    pub required_permission: Option<String>,
    pub weight: i32,
    pub enabled: bool,
}

#[derive(Clone, Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ActivePluginMenuItemDto {
    pub plugin_id: String,
    pub package_id: String,
    pub plugin_name: String,
    pub item_key: String,
    pub label: String,
    pub path: String,
    pub parent_key: Option<String>,
    pub required_permission: Option<String>,
    pub weight: i32,
}

#[derive(Clone, Debug, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct PluginConfigDto {
    pub plugin_id: String,
    pub package_id: String,
    pub plugin_name: String,
    pub schema: Vec<PluginConfigFieldDto>,
    pub values: Value,
}

#[derive(Clone, Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct PluginConfigFieldDto {
    pub key: String,
    pub label: String,
    #[serde(rename = "type")]
    pub value_type: String,
    pub required: bool,
    pub help_text: Option<String>,
    pub options: Vec<PluginConfigOptionDto>,
}

#[derive(Clone, Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct PluginConfigOptionDto {
    pub value: String,
    pub label: String,
}

#[derive(Clone, Debug, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct UpdatePluginConfigRequestDto {
    pub values: Value,
}

#[derive(Clone, Debug, PartialEq)]
struct ValidatedPluginConfigValues {
    values: Value,
    secret_update: PluginConfigSecretUpdate,
}

#[derive(Clone, Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct PluginScheduleDefinitionDto {
    pub task_key: String,
    pub schedule_kind: String,
    pub schedule_value: String,
    pub handler: String,
    pub enabled_by_default: bool,
    pub timeout_seconds: i32,
}

pub async fn list_plugins(
    State(state): State<AppState>,
    headers: HeaderMap,
    uri: Uri,
    Query(query): Query<ListPluginsQueryDto>,
) -> Result<Json<Vec<PluginSummaryDto>>, AppError> {
    authenticate_admin(&state, &headers, &uri).await?;
    let Some(database) = state.database() else {
        return Err(AppError::internal("database is not configured"));
    };
    let plugins = PluginRepository::new(database.clone())
        .list_plugins(plugin_list_limit(query.limit))
        .await
        .map_err(plugin_read_error_to_app_error)?
        .into_iter()
        .map(PluginSummaryDto::from)
        .collect();

    Ok(Json(plugins))
}

pub async fn admin_capabilities(
    State(state): State<AppState>,
    headers: HeaderMap,
    uri: Uri,
) -> Result<Json<PluginCapabilitiesDto>, AppError> {
    authenticate_admin(&state, &headers, &uri).await?;

    Ok(Json(plugin_capabilities()))
}

pub async fn package_detail(
    State(state): State<AppState>,
    Path(package_id): Path<String>,
    headers: HeaderMap,
    uri: Uri,
) -> Result<Json<PluginPackageDetailDto>, AppError> {
    authenticate_admin(&state, &headers, &uri).await?;
    validate_non_empty_path_param("packageId", &package_id)?;
    let Some(database) = state.database() else {
        return Err(AppError::internal("database is not configured"));
    };
    let Some(detail) = PluginRepository::new(database.clone())
        .get_package_detail(&package_id)
        .await
        .map_err(plugin_read_error_to_app_error)?
    else {
        return Err(AppError::not_found("plugin package not found"));
    };

    Ok(Json(detail.into()))
}

pub async fn list_menu_items(
    State(state): State<AppState>,
    headers: HeaderMap,
    uri: Uri,
) -> Result<Json<Vec<ActivePluginMenuItemDto>>, AppError> {
    authenticate_admin(&state, &headers, &uri).await?;
    let Some(database) = state.database() else {
        return Err(AppError::internal("database is not configured"));
    };

    let items = PluginRepository::new(database.clone())
        .list_active_menu_items()
        .await
        .map_err(plugin_read_error_to_app_error)?
        .into_iter()
        .map(ActivePluginMenuItemDto::from)
        .collect();

    Ok(Json(items))
}

pub async fn install_package(
    State(state): State<AppState>,
    headers: HeaderMap,
    uri: Uri,
    Json(payload): Json<InstallPluginPackageRequestDto>,
) -> Result<(StatusCode, Json<InstalledPluginPackageDto>), AppError> {
    authenticate_admin(&state, &headers, &uri).await?;
    validate_package_path(&payload.package_path)?;
    let requested_checksum_sha256 = payload
        .checksum_sha256
        .as_deref()
        .map(parse_sha256_hex)
        .transpose()?;
    let checksum_sha256 = verify_plugin_package_file(
        &state.config().plugins,
        &payload.package_path,
        requested_checksum_sha256.as_deref(),
    )
    .await?;
    let validated_manifest = payload
        .manifest
        .validate()
        .map_err(plugin_manifest_error_to_app_error)?;
    let signature = validate_plugin_package_signature(
        &state.config().plugins,
        payload.signature.as_deref(),
        &validated_manifest,
        &checksum_sha256,
    )?;
    prepare_plugin_package_archive(
        &state.config().plugins,
        &payload.package_path,
        &validated_manifest.manifest,
    )
    .await?;

    let Some(database) = state.database() else {
        return Err(AppError::internal("database is not configured"));
    };

    let record = PluginRepository::new(database.clone())
        .install_package(InstallPluginPackageInput {
            package_path: payload.package_path,
            checksum_sha256: Some(checksum_sha256),
            signature,
            validated_manifest,
        })
        .await
        .map_err(plugin_repository_error_to_app_error)?;

    Ok((StatusCode::CREATED, Json(record.into())))
}

pub async fn approve_package(
    State(state): State<AppState>,
    Path(package_id): Path<String>,
    headers: HeaderMap,
    uri: Uri,
) -> Result<Json<PluginStateDto>, AppError> {
    let admin = authenticate_admin(&state, &headers, &uri).await?;
    validate_non_empty_path_param("packageId", &package_id)?;
    let Some(database) = state.database() else {
        return Err(AppError::internal("database is not configured"));
    };

    let state = PluginRepository::new(database.clone())
        .approve_package(&package_id, admin.id)
        .await
        .map_err(plugin_state_error_to_app_error)?;

    Ok(Json(state.into()))
}

pub async fn reject_package(
    State(state): State<AppState>,
    Path(package_id): Path<String>,
    headers: HeaderMap,
    uri: Uri,
) -> Result<Json<PluginStateDto>, AppError> {
    authenticate_admin(&state, &headers, &uri).await?;
    validate_non_empty_path_param("packageId", &package_id)?;
    let Some(database) = state.database() else {
        return Err(AppError::internal("database is not configured"));
    };

    let state = PluginRepository::new(database.clone())
        .reject_package(&package_id)
        .await
        .map_err(plugin_state_error_to_app_error)?;

    Ok(Json(state.into()))
}

pub async fn activate_package(
    State(state): State<AppState>,
    Path(package_id): Path<String>,
    headers: HeaderMap,
    uri: Uri,
) -> Result<Json<PluginStateDto>, AppError> {
    authenticate_admin(&state, &headers, &uri).await?;
    validate_non_empty_path_param("packageId", &package_id)?;
    let Some(database) = state.database() else {
        return Err(AppError::internal("database is not configured"));
    };

    let state = PluginRepository::new(database.clone())
        .activate_package(&package_id)
        .await
        .map_err(plugin_state_error_to_app_error)?;

    Ok(Json(state.into()))
}

pub async fn enable_plugin(
    State(state): State<AppState>,
    Path(plugin_id): Path<String>,
    headers: HeaderMap,
    uri: Uri,
) -> Result<Json<PluginStateDto>, AppError> {
    authenticate_admin(&state, &headers, &uri).await?;
    validate_non_empty_path_param("pluginId", &plugin_id)?;
    let Some(database) = state.database() else {
        return Err(AppError::internal("database is not configured"));
    };

    let state = PluginRepository::new(database.clone())
        .enable_plugin(&plugin_id)
        .await
        .map_err(plugin_state_error_to_app_error)?;

    Ok(Json(state.into()))
}

pub async fn disable_plugin(
    State(state): State<AppState>,
    Path(plugin_id): Path<String>,
    headers: HeaderMap,
    uri: Uri,
) -> Result<Json<PluginStateDto>, AppError> {
    authenticate_admin(&state, &headers, &uri).await?;
    validate_non_empty_path_param("pluginId", &plugin_id)?;
    let Some(database) = state.database() else {
        return Err(AppError::internal("database is not configured"));
    };

    let state = PluginRepository::new(database.clone())
        .disable_plugin(&plugin_id)
        .await
        .map_err(plugin_state_error_to_app_error)?;

    Ok(Json(state.into()))
}

pub async fn plugin_config(
    State(state): State<AppState>,
    Path(plugin_id): Path<String>,
    headers: HeaderMap,
    uri: Uri,
) -> Result<Json<PluginConfigDto>, AppError> {
    authenticate_admin(&state, &headers, &uri).await?;
    validate_non_empty_path_param("pluginId", &plugin_id)?;
    let Some(database) = state.database() else {
        return Err(AppError::internal("database is not configured"));
    };

    let Some(config) = PluginRepository::new(database.clone())
        .get_plugin_config(&plugin_id)
        .await
        .map_err(plugin_read_error_to_app_error)?
    else {
        return Err(AppError::not_found("plugin config not found"));
    };

    Ok(Json(PluginConfigDto::from_record(config)))
}

pub async fn update_plugin_config(
    State(state): State<AppState>,
    Path(plugin_id): Path<String>,
    headers: HeaderMap,
    uri: Uri,
    Json(payload): Json<UpdatePluginConfigRequestDto>,
) -> Result<Json<PluginConfigDto>, AppError> {
    authenticate_admin(&state, &headers, &uri).await?;
    validate_non_empty_path_param("pluginId", &plugin_id)?;
    let Some(database) = state.database() else {
        return Err(AppError::internal("database is not configured"));
    };

    let repository = PluginRepository::new(database.clone());
    let Some(current) = repository
        .get_plugin_config(&plugin_id)
        .await
        .map_err(plugin_read_error_to_app_error)?
    else {
        return Err(AppError::not_found("plugin config not found"));
    };
    let validated = validate_plugin_config_values(&current.config_schema, &payload.values)?;
    let cipher = if validated.secret_update.secrets.is_empty() {
        None
    } else {
        Some(secret_cipher_from_state(&state)?)
    };
    let Some(updated) = repository
        .update_plugin_config(
            &plugin_id,
            validated.values,
            validated.secret_update,
            cipher.as_ref(),
        )
        .await
        .map_err(plugin_config_update_error_to_app_error)?
    else {
        return Err(AppError::not_found("plugin config not found"));
    };

    Ok(Json(PluginConfigDto::from_record(updated)))
}

fn validate_package_path(value: &str) -> Result<(), AppError> {
    let value = value.trim();
    if value.is_empty() {
        return Err(AppError::unprocessable("packagePath is required"));
    }
    if value.len() > 512 {
        return Err(AppError::unprocessable(
            "packagePath must be at most 512 characters",
        ));
    }
    if value.starts_with(['/', '\\']) || value.contains(':') {
        return Err(AppError::unprocessable(
            "packagePath must be relative to the plugin package directory",
        ));
    }
    if value
        .split(['/', '\\'])
        .any(|segment| segment.trim() == "..")
    {
        return Err(AppError::unprocessable(
            "packagePath must not escape the plugin package directory",
        ));
    }
    if value.contains(char::is_whitespace) {
        return Err(AppError::unprocessable(
            "packagePath must not contain whitespace",
        ));
    }

    Ok(())
}

async fn verify_plugin_package_file(
    config: &PluginRuntimeConfig,
    package_path: &str,
    expected_checksum_sha256: Option<&[u8]>,
) -> Result<Vec<u8>, AppError> {
    let absolute_path = resolve_plugin_package_path(&config.package_dir, package_path)?;
    let metadata = fs::metadata(&absolute_path).await.map_err(|err| {
        if err.kind() == ErrorKind::NotFound {
            return AppError::unprocessable("plugin package file does not exist");
        }
        AppError::unprocessable(format!("plugin package file cannot be inspected: {err}"))
    })?;
    if !metadata.is_file() {
        return Err(AppError::unprocessable(
            "plugin package path must point to a file",
        ));
    }
    if metadata.len() == 0 {
        return Err(AppError::unprocessable(
            "plugin package file must not be empty",
        ));
    }

    let actual_checksum = sha256_file(&absolute_path).await?;
    if let Some(expected_checksum) = expected_checksum_sha256 {
        if actual_checksum != expected_checksum {
            return Err(AppError::unprocessable(
                "checksumSha256 does not match plugin package file",
            ));
        }
    }

    Ok(actual_checksum)
}

async fn prepare_plugin_package_archive(
    config: &PluginRuntimeConfig,
    package_path: &str,
    expected_manifest: &PluginManifest,
) -> Result<PathBuf, AppError> {
    let archive_path = resolve_plugin_package_path(&config.package_dir, package_path)?;
    let extract_dir = plugin_package_extract_dir(config, expected_manifest);
    let expected_manifest = expected_manifest.clone();
    task::spawn_blocking(move || {
        validate_and_extract_plugin_zip(&archive_path, &extract_dir, &expected_manifest)
            .map(|_| extract_dir)
    })
    .await
    .map_err(|err| AppError::internal(format!("plugin package validation task failed: {err}")))?
}

fn validate_and_extract_plugin_zip(
    archive_path: &FsPath,
    extract_dir: &FsPath,
    expected_manifest: &PluginManifest,
) -> Result<(), AppError> {
    let file = std_fs::File::open(archive_path).map_err(|err| {
        AppError::unprocessable(format!("plugin package cannot be opened: {err}"))
    })?;
    let mut archive = ZipArchive::new(file).map_err(|err| {
        AppError::unprocessable(format!("plugin package must be a valid zip: {err}"))
    })?;
    if archive.is_empty() {
        return Err(AppError::unprocessable(
            "plugin package zip must not be empty",
        ));
    }
    if archive.len() > MAX_PLUGIN_ZIP_ENTRIES {
        return Err(AppError::unprocessable(format!(
            "plugin package zip must contain at most {MAX_PLUGIN_ZIP_ENTRIES} entries"
        )));
    }

    let manifest = read_plugin_manifest_from_zip(&mut archive)?;
    if &manifest != expected_manifest {
        return Err(AppError::unprocessable(
            "plugin package manifest.json must match request manifest",
        ));
    }

    if extract_dir.exists() {
        return Err(AppError::conflict(
            "plugin package version has already been extracted",
        ));
    }
    if let Some(parent) = extract_dir.parent() {
        std_fs::create_dir_all(parent).map_err(|err| {
            AppError::internal(format!("failed to create plugin extraction parent: {err}"))
        })?;
    }
    std_fs::create_dir_all(extract_dir).map_err(|err| {
        AppError::internal(format!("failed to create plugin extraction dir: {err}"))
    })?;

    let result = extract_plugin_zip_entries(&mut archive, extract_dir);
    if result.is_err() {
        let _ = std_fs::remove_dir_all(extract_dir);
    }
    result
}

fn read_plugin_manifest_from_zip(
    archive: &mut ZipArchive<std_fs::File>,
) -> Result<PluginManifest, AppError> {
    let mut manifest_file = archive
        .by_name(PLUGIN_PACKAGE_MANIFEST_PATH)
        .map_err(|_| AppError::unprocessable("plugin package zip must contain manifest.json"))?;
    if manifest_file.size() > 1024 * 1024 {
        return Err(AppError::unprocessable(
            "plugin package manifest.json must be at most 1 MiB",
        ));
    }

    let mut manifest = String::new();
    std::io::Read::read_to_string(&mut manifest_file, &mut manifest)
        .map_err(|err| AppError::unprocessable(format!("manifest.json cannot be read: {err}")))?;
    serde_json::from_str::<PluginManifest>(&manifest)
        .map_err(|err| AppError::unprocessable(format!("manifest.json is invalid: {err}")))
}

fn extract_plugin_zip_entries(
    archive: &mut ZipArchive<std_fs::File>,
    extract_dir: &FsPath,
) -> Result<(), AppError> {
    let mut seen_paths = BTreeSet::new();
    let mut total_uncompressed = 0_u64;
    for index in 0..archive.len() {
        let mut entry = archive.by_index(index).map_err(|err| {
            AppError::unprocessable(format!("plugin package zip entry cannot be read: {err}"))
        })?;
        if entry.unix_mode().is_some_and(is_unix_symlink_mode) {
            return Err(AppError::unprocessable(
                "plugin package zip must not contain symbolic links",
            ));
        }
        let Some(enclosed_name) = entry.enclosed_name() else {
            return Err(AppError::unprocessable(
                "plugin package zip contains an unsafe entry path",
            ));
        };
        validate_zip_entry_path(&enclosed_name)?;
        if !seen_paths.insert(enclosed_name.clone()) {
            return Err(AppError::unprocessable(
                "plugin package zip contains duplicate entry paths",
            ));
        }

        let output_path = extract_dir.join(&enclosed_name);
        if entry.is_dir() {
            std_fs::create_dir_all(&output_path).map_err(|err| {
                AppError::internal(format!("failed to create plugin package directory: {err}"))
            })?;
            continue;
        }

        total_uncompressed = total_uncompressed
            .checked_add(entry.size())
            .ok_or_else(|| AppError::unprocessable("plugin package zip is too large"))?;
        if total_uncompressed > MAX_PLUGIN_ZIP_UNCOMPRESSED_BYTES {
            return Err(AppError::unprocessable(format!(
                "plugin package zip uncompressed size must be at most {MAX_PLUGIN_ZIP_UNCOMPRESSED_BYTES} bytes"
            )));
        }

        if let Some(parent) = output_path.parent() {
            std_fs::create_dir_all(parent).map_err(|err| {
                AppError::internal(format!("failed to create plugin package directory: {err}"))
            })?;
        }
        let mut output = std_fs::File::create(&output_path)
            .map_err(|err| AppError::internal(format!("failed to extract plugin file: {err}")))?;
        std::io::copy(&mut entry, &mut output)
            .map_err(|err| AppError::internal(format!("failed to write plugin file: {err}")))?;
    }

    Ok(())
}

fn validate_zip_entry_path(path: &FsPath) -> Result<(), AppError> {
    for component in path.components() {
        match component {
            Component::Normal(segment) => {
                if segment.is_empty() {
                    return Err(AppError::unprocessable(
                        "plugin package zip contains an empty path segment",
                    ));
                }
            }
            Component::CurDir => {}
            Component::ParentDir | Component::RootDir | Component::Prefix(_) => {
                return Err(AppError::unprocessable(
                    "plugin package zip contains an unsafe entry path",
                ));
            }
        }
    }
    Ok(())
}

fn plugin_package_extract_dir(config: &PluginRuntimeConfig, manifest: &PluginManifest) -> PathBuf {
    config
        .package_dir
        .join(PLUGIN_PACKAGE_EXTRACTED_DIR)
        .join(manifest.id.trim())
        .join(manifest.version.trim())
}

fn is_unix_symlink_mode(mode: u32) -> bool {
    mode & 0o170000 == 0o120000
}

fn resolve_plugin_package_path(
    package_dir: &FsPath,
    package_path: &str,
) -> Result<PathBuf, AppError> {
    let mut absolute_path = package_dir.to_path_buf();
    let relative_path = FsPath::new(package_path.trim());
    for component in relative_path.components() {
        match component {
            Component::Normal(segment) => absolute_path.push(segment),
            Component::CurDir => {}
            Component::ParentDir | Component::RootDir | Component::Prefix(_) => {
                return Err(AppError::unprocessable(
                    "packagePath must be relative to the plugin package directory",
                ));
            }
        }
    }

    Ok(absolute_path)
}

async fn sha256_file(path: &FsPath) -> Result<Vec<u8>, AppError> {
    let mut file = fs::File::open(path).await.map_err(|err| {
        AppError::unprocessable(format!("plugin package file cannot be read: {err}"))
    })?;
    let mut hasher = Sha256::new();
    let mut buffer = [0_u8; 64 * 1024];
    loop {
        let read = file.read(&mut buffer).await.map_err(|err| {
            AppError::unprocessable(format!("plugin package file cannot be read: {err}"))
        })?;
        if read == 0 {
            break;
        }
        hasher.update(&buffer[..read]);
    }

    Ok(hasher.finalize().to_vec())
}

fn validate_non_empty_path_param(field: &str, value: &str) -> Result<(), AppError> {
    if value.trim().is_empty() {
        return Err(AppError::unprocessable(format!("{field} is required")));
    }
    Ok(())
}

fn validate_plugin_config_values(
    schema: &[PluginConfigFieldManifest],
    input: &Value,
) -> Result<ValidatedPluginConfigValues, AppError> {
    let object = input
        .as_object()
        .ok_or_else(|| AppError::unprocessable("plugin config values must be a JSON object"))?;
    let schema_keys = schema
        .iter()
        .map(|field| field.key.trim())
        .collect::<BTreeSet<_>>();
    for key in object.keys() {
        if !schema_keys.contains(key.as_str()) {
            return Err(AppError::unprocessable(format!(
                "unknown plugin config key `{key}`"
            )));
        }
    }

    let mut output = Map::new();
    let mut secret_update = PluginConfigSecretUpdate::default();
    for field in schema {
        let key = field.key.trim();
        match object.get(key) {
            Some(value) if !value.is_null() => {
                if is_plugin_secret_config_type(field) {
                    let public_value =
                        normalize_plugin_config_secret_value(field, value, &mut secret_update)?;
                    output.insert(key.to_owned(), public_value);
                } else {
                    validate_plugin_config_value(field, value)?;
                    output.insert(key.to_owned(), value.clone());
                }
            }
            _ if field.required => {
                return Err(AppError::unprocessable(format!(
                    "plugin config key `{key}` is required"
                )));
            }
            _ => {}
        }
    }

    Ok(ValidatedPluginConfigValues {
        values: Value::Object(output),
        secret_update,
    })
}

fn visible_plugin_config_values(schema: &[PluginConfigFieldManifest], values: &Value) -> Value {
    let Some(object) = values.as_object() else {
        return Value::Object(Map::new());
    };
    let mut output = Map::new();
    for field in schema {
        let key = field.key.trim();
        if let Some(value) = object.get(key)
            && !value.is_null()
            && plugin_config_value_is_valid(field, value)
        {
            output.insert(key.to_owned(), value.clone());
        }
    }

    Value::Object(output)
}

fn validate_plugin_config_value(
    field: &PluginConfigFieldManifest,
    value: &Value,
) -> Result<(), AppError> {
    if plugin_config_value_is_valid(field, value) {
        return Ok(());
    }

    Err(AppError::unprocessable(format!(
        "plugin config key `{}` must be a valid {} value",
        field.key.trim(),
        field.value_type.trim()
    )))
}

fn plugin_config_value_is_valid(field: &PluginConfigFieldManifest, value: &Value) -> bool {
    match field.value_type.trim() {
        "string" => value
            .as_str()
            .is_some_and(|value| value.len() <= 4096 && !value.contains('\0')),
        "number" => value.is_number(),
        "boolean" => value.is_boolean(),
        "url" => value.as_str().is_some_and(is_plugin_config_url),
        "select" => value.as_str().is_some_and(|value| {
            field
                .options
                .iter()
                .any(|option| option.value.trim() == value)
        }),
        "secret" | "password" => secret_ref_key(value).is_some_and(|key| key == field.key.trim()),
        _ => false,
    }
}

fn normalize_plugin_config_secret_value(
    field: &PluginConfigFieldManifest,
    value: &Value,
    secret_update: &mut PluginConfigSecretUpdate,
) -> Result<Value, AppError> {
    let key = field.key.trim();
    if let Some(secret_key) = secret_ref_key(value) {
        if secret_key != key {
            return Err(AppError::unprocessable(format!(
                "plugin config key `{key}` must keep secretRef `{key}`"
            )));
        }
        secret_update.configured_keys.push(key.to_owned());
        secret_update.retained_keys.push(key.to_owned());
        return Ok(secret_ref(key));
    }

    let Some(secret_value) = value.as_str() else {
        return Err(AppError::unprocessable(format!(
            "plugin config key `{key}` must be a secret string or matching secretRef"
        )));
    };
    if secret_value.is_empty() || secret_value.len() > 4096 || secret_value.contains('\0') {
        return Err(AppError::unprocessable(format!(
            "plugin config key `{key}` must be a non-empty secret string up to 4096 bytes"
        )));
    }

    secret_update.configured_keys.push(key.to_owned());
    secret_update.secrets.push(PluginConfigSecretInput {
        key: key.to_owned(),
        value: secret_value.to_owned(),
    });
    Ok(secret_ref(key))
}

fn is_plugin_secret_config_type(field: &PluginConfigFieldManifest) -> bool {
    matches!(field.value_type.trim(), "secret" | "password")
}

fn is_plugin_config_url(value: &str) -> bool {
    let value = value.trim();
    value.len() <= 2048 && (value.starts_with("http://") || value.starts_with("https://"))
}

fn plugin_list_limit(limit: Option<u32>) -> i64 {
    i64::from(limit.unwrap_or(100).clamp(1, 500))
}

fn parse_sha256_hex(value: &str) -> Result<Vec<u8>, AppError> {
    let value = value.trim();
    if value.len() != 64 {
        return Err(AppError::unprocessable(
            "checksumSha256 must be a 64 character hex string",
        ));
    }

    let mut bytes = Vec::with_capacity(32);
    for index in (0..value.len()).step_by(2) {
        let byte = u8::from_str_radix(&value[index..index + 2], 16).map_err(|_| {
            AppError::unprocessable(
                "checksumSha256 must be a valid lowercase or uppercase hex string",
            )
        })?;
        bytes.push(byte);
    }

    Ok(bytes)
}

fn validate_plugin_package_signature(
    config: &PluginRuntimeConfig,
    signature: Option<&str>,
    validated_manifest: &ValidatedPluginManifest,
    checksum_sha256: &[u8],
) -> Result<Option<String>, AppError> {
    let signature = signature.map(str::trim).filter(|value| !value.is_empty());
    let Some(signature) = signature else {
        return if config.allow_unsigned {
            Ok(None)
        } else {
            Err(AppError::unprocessable(
                "plugin package signature is required unless PLUGIN_ALLOW_UNSIGNED=true",
            ))
        };
    };

    let (scheme, key_id, signature_hex) = parse_plugin_signature_envelope(signature)?;
    if scheme != PLUGIN_PACKAGE_SIGNATURE_SCHEME {
        return Err(AppError::unprocessable(
            "plugin package signature scheme must be ed25519",
        ));
    }
    let Some(trusted_key) = config
        .trusted_signature_keys
        .iter()
        .find(|trusted_key| trusted_key.key_id == key_id)
    else {
        return Err(AppError::unprocessable(
            "plugin package signature key is not trusted",
        ));
    };

    let signature_bytes = parse_signature_hex(signature_hex)?;
    let verifying_key = VerifyingKey::from_bytes(&trusted_key.public_key)
        .map_err(|_| AppError::internal("trusted plugin signature public key is invalid"))?;
    let signature_value = Signature::from_bytes(&signature_bytes);
    verifying_key
        .verify(
            plugin_package_signature_message(validated_manifest, checksum_sha256).as_bytes(),
            &signature_value,
        )
        .map_err(|_| AppError::unprocessable("plugin package signature is invalid"))?;

    Ok(Some(format!("{scheme}:{key_id}:{signature_hex}")))
}

fn parse_plugin_signature_envelope(signature: &str) -> Result<(&str, &str, &str), AppError> {
    let mut parts = signature.split(':');
    let scheme = parts.next().unwrap_or_default();
    let key_id = parts.next().unwrap_or_default();
    let signature_hex = parts.next().unwrap_or_default();
    if parts.next().is_some() || scheme.is_empty() || key_id.is_empty() || signature_hex.is_empty()
    {
        return Err(AppError::unprocessable(
            "plugin package signature must use ed25519:keyId:signatureHex",
        ));
    }
    validate_plugin_signature_key_id(key_id)?;
    Ok((scheme, key_id, signature_hex))
}

fn validate_plugin_signature_key_id(key_id: &str) -> Result<(), AppError> {
    if key_id.len() > 64
        || !key_id
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'_' | b'-'))
    {
        return Err(AppError::unprocessable(
            "plugin package signature key id is invalid",
        ));
    }
    Ok(())
}

fn parse_signature_hex(value: &str) -> Result<[u8; 64], AppError> {
    if value.len() != 128 {
        return Err(AppError::unprocessable(
            "plugin package signature hex must be 128 characters",
        ));
    }
    let bytes = parse_hex_bytes(value, "plugin package signature hex is invalid")?;
    bytes
        .try_into()
        .map_err(|_| AppError::unprocessable("plugin package signature hex is invalid"))
}

fn parse_hex_bytes(value: &str, invalid_message: &'static str) -> Result<Vec<u8>, AppError> {
    let mut bytes = Vec::with_capacity(value.len() / 2);
    for index in (0..value.len()).step_by(2) {
        let byte = u8::from_str_radix(&value[index..index + 2], 16)
            .map_err(|_| AppError::unprocessable(invalid_message))?;
        bytes.push(byte);
    }
    Ok(bytes)
}

fn plugin_package_signature_message(
    validated_manifest: &ValidatedPluginManifest,
    checksum_sha256: &[u8],
) -> String {
    format!(
        "{}\n{}\n{}\n{}\n{}",
        PLUGIN_PACKAGE_SIGNATURE_CONTEXT,
        validated_manifest.manifest.id.trim(),
        validated_manifest.manifest.version.trim(),
        hex_encode(checksum_sha256),
        hex_encode(&validated_manifest.manifest_hash),
    )
}

fn hex_encode(bytes: &[u8]) -> String {
    let mut encoded = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        use std::fmt::Write as _;
        let _ = write!(encoded, "{byte:02x}");
    }
    encoded
}

fn plugin_manifest_error_to_app_error(error: PluginManifestError) -> AppError {
    AppError::unprocessable(error.to_string())
}

fn plugin_repository_error_to_app_error(error: sqlx::Error) -> AppError {
    if is_unique_violation(&error) {
        return AppError::conflict("plugin package version already exists");
    }
    AppError::internal(format!("failed to install plugin package: {error}"))
}

fn plugin_read_error_to_app_error(error: sqlx::Error) -> AppError {
    AppError::internal(format!("failed to read plugin data: {error}"))
}

fn plugin_config_update_error_to_app_error(error: PluginConfigUpdateError) -> AppError {
    match error {
        PluginConfigUpdateError::MissingRetainedSecret(_) | PluginConfigUpdateError::Secret(_) => {
            AppError::unprocessable(error.to_string())
        }
        PluginConfigUpdateError::Database(err) => {
            AppError::internal(format!("failed to update plugin config: {err}"))
        }
    }
}

fn plugin_state_error_to_app_error(error: PluginStateError) -> AppError {
    match error {
        PluginStateError::PackageNotFound | PluginStateError::PluginNotFound => {
            AppError::not_found(error.to_string())
        }
        PluginStateError::InvalidState(_) => AppError::unprocessable(error.to_string()),
        PluginStateError::Database(err) => {
            AppError::internal(format!("failed to update plugin state: {err}"))
        }
    }
}

fn secret_cipher_from_state(state: &AppState) -> Result<SecretCipher, AppError> {
    SecretCipher::from_config(&state.config().secrets)
        .map_err(|err| AppError::unprocessable(err.to_string()))
}

fn is_unique_violation(error: &sqlx::Error) -> bool {
    error
        .as_database_error()
        .and_then(|database_error| database_error.code())
        .is_some_and(|code| code == "23505")
}

impl From<InstalledPluginPackageRecord> for InstalledPluginPackageDto {
    fn from(record: InstalledPluginPackageRecord) -> Self {
        Self {
            package_id: record.package_id,
            plugin_id: record.plugin_id,
            package_version: record.package_version,
            package_status: record.package_status,
            approval_status: record.approval_status,
        }
    }
}

impl From<PluginStateRecord> for PluginStateDto {
    fn from(record: PluginStateRecord) -> Self {
        Self {
            plugin_id: record.plugin_id,
            package_id: record.package_id,
            package_version: record.package_version,
            package_status: record.package_status,
            approval_status: record.approval_status,
            enabled: record.enabled,
        }
    }
}

impl From<PluginSummaryRecord> for PluginSummaryDto {
    fn from(record: PluginSummaryRecord) -> Self {
        Self {
            plugin_id: record.plugin_id,
            package_id: record.package_id,
            package_version: record.package_version,
            package_status: record.package_status,
            approval_status: record.approval_status,
            enabled: record.enabled,
            name: record.name,
            runtime: record.runtime,
        }
    }
}

impl From<PluginPackageDetailRecord> for PluginPackageDetailDto {
    fn from(record: PluginPackageDetailRecord) -> Self {
        Self {
            package_id: record.package_id,
            plugin_id: record.plugin_id,
            package_version: record.package_version,
            api_version: record.api_version,
            runtime: record.runtime,
            name: record.name,
            description: record.description,
            entrypoint: record.entrypoint,
            package_path: record.package_path,
            package_status: record.package_status,
            signature_present: record.signature_present,
            approval_status: record.approval_status,
            enabled: record.enabled,
            permissions: record
                .permissions
                .into_iter()
                .map(PluginPermissionDto::from)
                .collect(),
            hooks: record.hooks.into_iter().map(PluginHookDto::from).collect(),
            menu: record
                .menu
                .into_iter()
                .map(PluginMenuItemDto::from)
                .collect(),
            schedules: record
                .schedules
                .into_iter()
                .map(PluginScheduleDefinitionDto::from)
                .collect(),
        }
    }
}

impl From<PluginPermissionRecord> for PluginPermissionDto {
    fn from(record: PluginPermissionRecord) -> Self {
        Self {
            permission_key: record.permission_key,
            permission_scope: record.permission_scope,
            reason: record.reason,
        }
    }
}

impl From<PluginHookRecord> for PluginHookDto {
    fn from(record: PluginHookRecord) -> Self {
        Self {
            event_key: record.event_key,
            handler: record.handler,
            priority: record.priority,
            enabled: record.enabled,
        }
    }
}

impl From<PluginMenuItemRecord> for PluginMenuItemDto {
    fn from(record: PluginMenuItemRecord) -> Self {
        Self {
            item_key: record.item_key,
            label: record.label,
            path: record.path,
            parent_key: record.parent_key,
            required_permission: record.required_permission,
            weight: record.weight,
            enabled: record.enabled,
        }
    }
}

impl From<ActivePluginMenuItemRecord> for ActivePluginMenuItemDto {
    fn from(record: ActivePluginMenuItemRecord) -> Self {
        Self {
            plugin_id: record.plugin_id,
            package_id: record.package_id,
            plugin_name: record.plugin_name,
            item_key: record.item_key,
            label: record.label,
            path: record.path,
            parent_key: record.parent_key,
            required_permission: record.required_permission,
            weight: record.weight,
        }
    }
}

impl PluginConfigDto {
    fn from_record(record: PluginConfigRecord) -> Self {
        let values = visible_plugin_config_values(&record.config_schema, &record.values);
        Self {
            plugin_id: record.plugin_id,
            package_id: record.package_id,
            plugin_name: record.plugin_name,
            schema: record
                .config_schema
                .into_iter()
                .map(PluginConfigFieldDto::from)
                .collect(),
            values,
        }
    }
}

impl From<PluginConfigFieldManifest> for PluginConfigFieldDto {
    fn from(field: PluginConfigFieldManifest) -> Self {
        Self {
            key: field.key,
            label: field.label,
            value_type: field.value_type,
            required: field.required,
            help_text: field.help_text,
            options: field
                .options
                .into_iter()
                .map(PluginConfigOptionDto::from)
                .collect(),
        }
    }
}

impl From<PluginConfigOptionManifest> for PluginConfigOptionDto {
    fn from(option: PluginConfigOptionManifest) -> Self {
        Self {
            value: option.value,
            label: option.label,
        }
    }
}

impl From<PluginScheduleDefinitionRecord> for PluginScheduleDefinitionDto {
    fn from(record: PluginScheduleDefinitionRecord) -> Self {
        Self {
            task_key: record.task_key,
            schedule_kind: record.schedule_kind,
            schedule_value: record.schedule_value,
            handler: record.handler,
            enabled_by_default: record.enabled_by_default,
            timeout_seconds: record.timeout_seconds,
        }
    }
}

#[cfg(test)]
mod tests {
    use std::{
        fs as std_fs,
        io::Write,
        process,
        time::{SystemTime, UNIX_EPOCH},
    };

    use ed25519_dalek::{Signer, SigningKey};
    use serde_json::json;
    use zip::{ZipWriter, write::SimpleFileOptions};

    use super::*;

    #[test]
    fn package_path_must_be_relative_and_contained() {
        assert!(validate_package_path("notify/plugin.zip").is_ok());
        assert!(validate_package_path("../plugin.zip").is_err());
        assert!(validate_package_path("C:/plugin.zip").is_err());
        assert!(validate_package_path("/plugin.zip").is_err());
    }

    #[tokio::test]
    async fn package_file_verification_hashes_and_checks_expected_checksum() {
        let base_dir = std::env::temp_dir().join(format!(
            "fbz-plugin-package-test-{}-{}",
            process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let package_dir = base_dir.join("packages");
        std_fs::create_dir_all(package_dir.join("notify")).unwrap();
        let package_file = package_dir.join("notify").join("plugin.zip");
        std_fs::write(&package_file, b"plugin package").unwrap();
        let config = PluginRuntimeConfig {
            dir: base_dir.join("plugins"),
            package_dir,
            data_dir: base_dir.join("data"),
            cache_dir: base_dir.join("cache"),
            tmp_dir: base_dir.join("tmp"),
            tmp_max_age_seconds: 86_400,
            runtime_default: "wasi".to_owned(),
            require_approval: true,
            require_reapproval_on_permission_change: true,
            allow_unsigned: false,
            trusted_signature_keys: Vec::new(),
            timeout_ms: 5_000,
            max_concurrency: 4,
            memory_limit_mb: 128,
            wasi_fuel: 100_000_000,
            wasi_stdio_max_bytes: 64 * 1024,
            wasi_max_module_bytes: 64 * 1024 * 1024,
            http_max_response_body_bytes: 64 * 1024,
            host_api_max_calls_per_run: 10_000,
            secret_key: None,
            http_allowed_hosts: vec!["127.0.0.1".to_owned(), "localhost".to_owned()],
        };

        let checksum = verify_plugin_package_file(&config, "notify/plugin.zip", None)
            .await
            .unwrap();
        let expected = Sha256::digest(b"plugin package").to_vec();

        assert_eq!(checksum, expected);
        assert!(
            verify_plugin_package_file(&config, "notify/plugin.zip", Some(&[0_u8; 32]))
                .await
                .is_err()
        );
        assert!(
            verify_plugin_package_file(&config, "notify/missing.zip", None)
                .await
                .is_err()
        );

        let _ = std_fs::remove_dir_all(base_dir);
    }

    #[test]
    fn package_signature_policy_requires_trusted_valid_signature() {
        let base_dir = unique_test_dir("fbz-plugin-signature-test");
        let mut config = test_plugin_runtime_config(&base_dir);
        let signing_key = SigningKey::from_bytes(&[7_u8; 32]);
        let trusted_key = signing_key.verifying_key();
        config.trusted_signature_keys = vec![crate::config::PluginTrustedSignatureKey {
            key_id: "dev-key".to_owned(),
            public_key: trusted_key.to_bytes(),
        }];

        let manifest = test_plugin_manifest("dev.fbz.signed").validate().unwrap();
        let checksum = Sha256::digest(b"plugin zip").to_vec();
        let message = plugin_package_signature_message(&manifest, &checksum);
        let signature = signing_key.sign(message.as_bytes());
        let envelope = format!("ed25519:dev-key:{}", hex_encode(&signature.to_bytes()));

        let normalized =
            validate_plugin_package_signature(&config, Some(&envelope), &manifest, &checksum)
                .unwrap();

        assert_eq!(normalized.as_deref(), Some(envelope.as_str()));
        assert!(validate_plugin_package_signature(&config, None, &manifest, &checksum).is_err());
        assert!(
            validate_plugin_package_signature(
                &config,
                Some(&envelope),
                &manifest,
                &Sha256::digest(b"other zip").to_vec(),
            )
            .is_err()
        );
        assert!(
            validate_plugin_package_signature(
                &config,
                Some(&envelope.replace("dev-key", "other-key")),
                &manifest,
                &checksum,
            )
            .is_err()
        );

        let _ = std_fs::remove_dir_all(base_dir);
    }

    #[test]
    fn unsigned_package_is_allowed_only_when_configured() {
        let base_dir = unique_test_dir("fbz-plugin-unsigned-test");
        let mut config = test_plugin_runtime_config(&base_dir);
        let manifest = test_plugin_manifest("dev.fbz.unsigned").validate().unwrap();
        let checksum = Sha256::digest(b"plugin zip").to_vec();

        assert!(validate_plugin_package_signature(&config, None, &manifest, &checksum).is_err());

        config.allow_unsigned = true;
        let normalized =
            validate_plugin_package_signature(&config, None, &manifest, &checksum).unwrap();

        assert_eq!(normalized, None);

        let _ = std_fs::remove_dir_all(base_dir);
    }

    #[tokio::test]
    async fn package_archive_preparation_extracts_matching_manifest() {
        let base_dir = unique_test_dir("fbz-plugin-archive-test");
        let package_dir = base_dir.join("packages");
        std_fs::create_dir_all(package_dir.join("notify")).unwrap();
        let package_file = package_dir.join("notify").join("plugin.zip");
        let manifest = test_plugin_manifest("dev.fbz.archive");
        let manifest_json = serde_json::to_vec(&manifest).unwrap();
        write_zip_entries(
            &package_file,
            &[
                (PLUGIN_PACKAGE_MANIFEST_PATH, manifest_json.as_slice()),
                ("handler.js", b"export default {}"),
            ],
        );
        let config = test_plugin_runtime_config(&base_dir);

        let extract_dir = prepare_plugin_package_archive(&config, "notify/plugin.zip", &manifest)
            .await
            .unwrap();

        assert_eq!(
            std_fs::read_to_string(extract_dir.join("handler.js")).unwrap(),
            "export default {}"
        );
        assert!(
            prepare_plugin_package_archive(&config, "notify/plugin.zip", &manifest)
                .await
                .is_err()
        );

        std_fs::remove_dir_all(base_dir).unwrap();
    }

    #[tokio::test]
    async fn package_archive_preparation_rejects_mismatch_and_unsafe_paths() {
        let base_dir = unique_test_dir("fbz-plugin-archive-bad-test");
        let package_dir = base_dir.join("packages");
        std_fs::create_dir_all(package_dir.join("notify")).unwrap();
        let mismatch_file = package_dir.join("notify").join("mismatch.zip");
        let safe_manifest = test_plugin_manifest("dev.fbz.safe");
        let expected_manifest = test_plugin_manifest("dev.fbz.expected");
        let safe_manifest_json = serde_json::to_vec(&safe_manifest).unwrap();
        write_zip_entries(
            &mismatch_file,
            &[(PLUGIN_PACKAGE_MANIFEST_PATH, safe_manifest_json.as_slice())],
        );
        let unsafe_file = package_dir.join("notify").join("unsafe.zip");
        let expected_manifest_json = serde_json::to_vec(&expected_manifest).unwrap();
        write_zip_entries(
            &unsafe_file,
            &[
                (
                    PLUGIN_PACKAGE_MANIFEST_PATH,
                    expected_manifest_json.as_slice(),
                ),
                ("../escape.txt", b"nope"),
            ],
        );
        let config = test_plugin_runtime_config(&base_dir);

        assert!(
            prepare_plugin_package_archive(&config, "notify/mismatch.zip", &expected_manifest)
                .await
                .is_err()
        );
        assert!(
            prepare_plugin_package_archive(&config, "notify/unsafe.zip", &expected_manifest)
                .await
                .is_err()
        );
        assert!(!base_dir.join("packages").join("escape.txt").exists());

        std_fs::remove_dir_all(base_dir).unwrap();
    }

    #[test]
    fn sha256_hex_parser_requires_exact_hash() {
        let hash = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
        assert_eq!(parse_sha256_hex(hash).unwrap().len(), 32);
        assert!(parse_sha256_hex("aa").is_err());
        assert!(parse_sha256_hex(&"z".repeat(64)).is_err());
    }

    #[test]
    fn plugin_list_limit_is_bounded() {
        assert_eq!(plugin_list_limit(None), 100);
        assert_eq!(plugin_list_limit(Some(0)), 1);
        assert_eq!(plugin_list_limit(Some(10)), 10);
        assert_eq!(plugin_list_limit(Some(5_000)), 500);
    }

    #[test]
    fn active_menu_item_dto_preserves_plugin_context() {
        let dto = ActivePluginMenuItemDto::from(ActivePluginMenuItemRecord {
            plugin_id: "dev.fbz.menu".to_owned(),
            package_id: "package-1".to_owned(),
            plugin_name: "Menu Plugin".to_owned(),
            item_key: "dev.fbz.menu.settings".to_owned(),
            label: "Settings".to_owned(),
            path: "/admin/plugins/dev.fbz.menu/settings".to_owned(),
            parent_key: None,
            required_permission: Some("admin.menu".to_owned()),
            weight: 10,
        });

        assert_eq!(dto.plugin_id, "dev.fbz.menu");
        assert_eq!(dto.package_id, "package-1");
        assert_eq!(dto.plugin_name, "Menu Plugin");
        assert_eq!(dto.item_key, "dev.fbz.menu.settings");
        assert_eq!(dto.path, "/admin/plugins/dev.fbz.menu/settings");
        assert_eq!(dto.required_permission.as_deref(), Some("admin.menu"));
        assert_eq!(dto.weight, 10);
    }

    #[test]
    fn plugin_config_values_are_validated_against_schema() {
        let schema = vec![
            PluginConfigFieldManifest {
                key: "endpoint".to_owned(),
                label: "Endpoint".to_owned(),
                value_type: "url".to_owned(),
                required: true,
                help_text: None,
                options: vec![],
            },
            PluginConfigFieldManifest {
                key: "mode".to_owned(),
                label: "Mode".to_owned(),
                value_type: "select".to_owned(),
                required: false,
                help_text: None,
                options: vec![
                    PluginConfigOptionManifest {
                        value: "safe".to_owned(),
                        label: "Safe".to_owned(),
                    },
                    PluginConfigOptionManifest {
                        value: "fast".to_owned(),
                        label: "Fast".to_owned(),
                    },
                ],
            },
            PluginConfigFieldManifest {
                key: "enabled".to_owned(),
                label: "Enabled".to_owned(),
                value_type: "boolean".to_owned(),
                required: false,
                help_text: None,
                options: vec![],
            },
            PluginConfigFieldManifest {
                key: "api_token".to_owned(),
                label: "API Token".to_owned(),
                value_type: "secret".to_owned(),
                required: false,
                help_text: None,
                options: vec![],
            },
        ];

        let values = validate_plugin_config_values(
            &schema,
            &json!({
                "endpoint": "https://notify.example.test/hook",
                "mode": "safe",
                "enabled": true,
                "api_token": "secret-token"
            }),
        )
        .unwrap();

        assert_eq!(
            values.values["endpoint"],
            "https://notify.example.test/hook"
        );
        assert_eq!(
            values.values["api_token"],
            json!({"secretRef": "api_token"})
        );
        assert_eq!(values.secret_update.configured_keys, vec!["api_token"]);
        assert_eq!(values.secret_update.secrets.len(), 1);
        assert_eq!(values.secret_update.secrets[0].key, "api_token");
        assert!(validate_plugin_config_values(&schema, &json!({"endpoint": "ftp://bad"})).is_err());
        assert!(
            validate_plugin_config_values(
                &schema,
                &json!({"endpoint": "https://ok.test", "unknown": true})
            )
            .is_err()
        );
        assert!(validate_plugin_config_values(&schema, &json!({"mode": "safe"})).is_err());
        assert!(
            validate_plugin_config_values(
                &schema,
                &json!({
                    "endpoint": "https://ok.test",
                    "api_token": { "secretRef": "other" }
                })
            )
            .is_err()
        );
        let retained = validate_plugin_config_values(
            &schema,
            &json!({
                "endpoint": "https://ok.test",
                "api_token": { "secretRef": "api_token" }
            }),
        )
        .unwrap();
        assert_eq!(retained.secret_update.retained_keys, vec!["api_token"]);
        assert!(retained.secret_update.secrets.is_empty());
    }

    #[test]
    fn visible_plugin_config_values_filters_stale_or_invalid_values() {
        let schema = vec![
            PluginConfigFieldManifest {
                key: "enabled".to_owned(),
                label: "Enabled".to_owned(),
                value_type: "boolean".to_owned(),
                required: false,
                help_text: None,
                options: vec![],
            },
            PluginConfigFieldManifest {
                key: "api_token".to_owned(),
                label: "API Token".to_owned(),
                value_type: "secret".to_owned(),
                required: false,
                help_text: None,
                options: vec![],
            },
        ];

        let values = visible_plugin_config_values(
            &schema,
            &json!({
                "enabled": true,
                "api_token": { "secretRef": "api_token" },
                "raw_token": "secret",
                "stale": "removed",
            }),
        );

        assert_eq!(
            values,
            json!({
                "enabled": true,
                "api_token": { "secretRef": "api_token" }
            })
        );
    }

    #[test]
    fn plugin_state_errors_map_to_client_safe_statuses() {
        assert_eq!(
            plugin_state_error_to_app_error(PluginStateError::PackageNotFound).status_code(),
            StatusCode::NOT_FOUND
        );
        assert_eq!(
            plugin_state_error_to_app_error(PluginStateError::InvalidState(
                "not approved".to_owned()
            ))
            .status_code(),
            StatusCode::UNPROCESSABLE_ENTITY
        );
    }

    fn unique_test_dir(prefix: &str) -> PathBuf {
        std::env::temp_dir().join(format!(
            "{}-{}-{}",
            prefix,
            process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ))
    }

    fn test_plugin_runtime_config(base_dir: &FsPath) -> PluginRuntimeConfig {
        PluginRuntimeConfig {
            dir: base_dir.join("plugins"),
            package_dir: base_dir.join("packages"),
            data_dir: base_dir.join("data"),
            cache_dir: base_dir.join("cache"),
            tmp_dir: base_dir.join("tmp"),
            tmp_max_age_seconds: 86_400,
            runtime_default: "wasi".to_owned(),
            require_approval: true,
            require_reapproval_on_permission_change: true,
            allow_unsigned: false,
            trusted_signature_keys: Vec::new(),
            timeout_ms: 5_000,
            max_concurrency: 4,
            memory_limit_mb: 128,
            wasi_fuel: 100_000_000,
            wasi_stdio_max_bytes: 64 * 1024,
            wasi_max_module_bytes: 64 * 1024 * 1024,
            http_max_response_body_bytes: 64 * 1024,
            host_api_max_calls_per_run: 10_000,
            secret_key: None,
            http_allowed_hosts: vec!["127.0.0.1".to_owned(), "localhost".to_owned()],
        }
    }

    fn test_plugin_manifest(plugin_id: &str) -> PluginManifest {
        serde_json::from_value(json!({
            "id": plugin_id,
            "name": "Archive",
            "version": "0.0.1",
            "apiVersion": "1",
            "runtime": "http",
            "entrypoint": "http://127.0.0.1:18187/hook"
        }))
        .unwrap()
    }

    fn write_zip_entries(path: &FsPath, entries: &[(&str, &[u8])]) {
        let file = std_fs::File::create(path).unwrap();
        let mut writer = ZipWriter::new(file);
        let options =
            SimpleFileOptions::default().compression_method(zip::CompressionMethod::Deflated);
        for (name, contents) in entries {
            writer.start_file(*name, options).unwrap();
            writer.write_all(contents).unwrap();
        }
        writer.finish().unwrap();
    }
}
