use std::{
    collections::BTreeSet,
    fs as std_fs,
    io::ErrorKind,
    path::{Component, Path as FsPath, PathBuf},
    time::{SystemTime, UNIX_EPOCH},
};

use axum::{
    Json, Router,
    extract::{DefaultBodyLimit, Multipart, Path, Query, State},
    http::{HeaderMap, HeaderValue, StatusCode, Uri},
    routing::{delete, get, post},
};
use ed25519_dalek::{Signature, Verifier, VerifyingKey};
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};
use sha2::{Digest, Sha256};
use tokio::{fs, io::AsyncReadExt, task};
use tracing::warn;
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
            CreatePluginMarketSourceInput, NewPluginMarketEntry, PluginConfigRecord,
            PluginConfigSecretInput, PluginConfigSecretUpdate, PluginConfigUpdateError,
            PluginHookRecord, PluginListFilter, PluginMarketEntryRecord, PluginMarketSourceRecord,
            PluginMenuItemRecord, PluginPackageDetailRecord, PluginPackageListFilter,
            PluginPackageSummaryRecord, PluginPermissionRecord, PluginRepository,
            PluginScheduleDefinitionRecord, PluginStateError, PluginStateRecord,
            PluginSummaryRecord, PluginUninstallRecord,
        },
        signing::{
            PLUGIN_PACKAGE_SIGNATURE_SCHEME, parse_ed25519_signature_hex,
            plugin_package_signature_message, validate_plugin_signature_key_id,
        },
    },
    state::AppState,
};

const PLUGIN_PACKAGE_MANIFEST_PATH: &str = "manifest.json";
const PLUGIN_PACKAGE_EXTRACTED_DIR: &str = "extracted";
const MAX_PLUGIN_ZIP_ENTRIES: usize = 4096;
const MAX_PLUGIN_ZIP_UNCOMPRESSED_BYTES: u64 = 256 * 1024 * 1024;
/// 浏览器直传插件包（zip 压缩后大小）上限。
const MAX_PLUGIN_UPLOAD_BYTES: usize = 100 * 1024 * 1024;
/// 浏览器上传的包落在 PLUGIN_PACKAGE_DIR 的该子目录（与市场下载 market/ 平行）。
const UPLOAD_SUBDIR: &str = "uploads";

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/api/admin/plugins", get(list_plugins))
        .route("/api/admin/plugins/capabilities", get(admin_capabilities))
        .route("/api/admin/plugins/menu-items", get(list_menu_items))
        .route(
            "/api/admin/plugins/packages",
            get(list_plugin_packages).post(install_package),
        )
        .route(
            "/api/admin/plugins/packages/upload",
            post(upload_package).layer(DefaultBodyLimit::max(MAX_PLUGIN_UPLOAD_BYTES + 64 * 1024)),
        )
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
        .route(
            "/api/admin/plugins/market/sources",
            get(list_market_sources).post(create_market_source),
        )
        .route(
            "/api/admin/plugins/market/sources/{source_id}",
            delete(delete_market_source),
        )
        .route(
            "/api/admin/plugins/market/sources/{source_id}/enabled",
            post(set_market_source_enabled),
        )
        .route(
            "/api/admin/plugins/market/sources/{source_id}/sync",
            post(sync_market_source),
        )
        .route("/api/admin/plugins/market/catalog", get(market_catalog))
        .route(
            "/api/admin/plugins/market/install",
            post(install_market_plugin),
        )
        .route("/api/admin/plugins/{plugin_id}", delete(uninstall_plugin))
}

#[derive(Clone, Debug, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct InstallPluginPackageRequestDto {
    pub package_path: String,
    /// 可选：内联 manifest。省略时后端从包内 `plugin.json` 读取，
    /// 使前端只需提供 `packagePath` 即可手工安装。
    #[serde(default)]
    pub manifest: Option<PluginManifest>,
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

/// 浏览器直传插件包的落盘结果（安装入参 packagePath 即来自这里）。
#[derive(Clone, Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct UploadedPluginPackageDto {
    pub package_path: String,
    pub size_bytes: u64,
    pub checksum_sha256: String,
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
    pub approval_status: Option<String>,
    pub enabled: Option<bool>,
    pub runtime: Option<String>,
    pub cursor: Option<String>,
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

#[derive(Clone, Debug, Default, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ListPluginPackagesQueryDto {
    pub plugin_id: Option<String>,
    pub package_status: Option<String>,
    pub runtime: Option<String>,
    pub cursor: Option<String>,
    pub limit: Option<u32>,
}

#[derive(Clone, Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct PluginPackageSummaryDto {
    pub package_id: String,
    pub plugin_id: String,
    pub package_version: String,
    pub api_version: String,
    pub runtime: String,
    pub name: String,
    pub package_status: String,
    pub signature_present: bool,
    pub approval_status: Option<String>,
    pub enabled: Option<bool>,
    pub active: bool,
    pub created_at: String,
    pub updated_at: String,
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
) -> Result<(HeaderMap, Json<Vec<PluginSummaryDto>>), AppError> {
    authenticate_admin(&state, &headers, &uri).await?;
    let approval_status = query
        .approval_status
        .as_deref()
        .map(validate_plugin_approval_status)
        .transpose()?
        .map(str::to_owned);
    let runtime = query
        .runtime
        .as_deref()
        .map(validate_plugin_runtime_filter)
        .transpose()?
        .map(str::to_owned);
    let cursor = query
        .cursor
        .as_deref()
        .map(|value| validate_uuid_public_id("cursor", value))
        .transpose()?
        .map(str::to_owned);
    let Some(database) = state.database() else {
        return Err(AppError::internal("database is not configured"));
    };
    let page = PluginRepository::new(database.clone())
        .list_plugins_page(PluginListFilter {
            approval_status,
            enabled: query.enabled,
            runtime,
            cursor,
            limit: plugin_list_limit(query.limit),
        })
        .await
        .map_err(plugin_read_error_to_app_error)?;
    let response_headers = pagination_headers(page.has_more, page.next_cursor.as_deref())?;

    Ok((
        response_headers,
        Json(
            page.records
                .into_iter()
                .map(PluginSummaryDto::from)
                .collect(),
        ),
    ))
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

pub async fn list_plugin_packages(
    State(state): State<AppState>,
    headers: HeaderMap,
    uri: Uri,
    Query(query): Query<ListPluginPackagesQueryDto>,
) -> Result<(HeaderMap, Json<Vec<PluginPackageSummaryDto>>), AppError> {
    authenticate_admin(&state, &headers, &uri).await?;
    let plugin_id = query
        .plugin_id
        .as_deref()
        .map(validate_plugin_id_filter)
        .transpose()?
        .map(str::to_owned);
    let package_status = query
        .package_status
        .as_deref()
        .map(validate_plugin_package_status)
        .transpose()?
        .map(str::to_owned);
    let runtime = query
        .runtime
        .as_deref()
        .map(validate_plugin_runtime_filter)
        .transpose()?
        .map(str::to_owned);
    let cursor = query
        .cursor
        .as_deref()
        .map(|value| validate_uuid_public_id("cursor", value))
        .transpose()?
        .map(str::to_owned);
    let Some(database) = state.database() else {
        return Err(AppError::internal("database is not configured"));
    };

    let page = PluginRepository::new(database.clone())
        .list_plugin_packages_page(PluginPackageListFilter {
            plugin_id,
            package_status,
            runtime,
            cursor,
            limit: plugin_package_list_limit(query.limit),
        })
        .await
        .map_err(plugin_read_error_to_app_error)?;
    let response_headers = pagination_headers(page.has_more, page.next_cursor.as_deref())?;

    Ok((
        response_headers,
        Json(
            page.records
                .into_iter()
                .map(PluginPackageSummaryDto::from)
                .collect(),
        ),
    ))
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
    let validated_manifest = match payload.manifest {
        Some(manifest) => manifest,
        // 前端手工安装通常只给 packagePath；此时从包内读取 manifest。
        None => read_plugin_manifest_from_package(&state.config().plugins, &payload.package_path)
            .await?,
    }
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

/// `POST /api/admin/plugins/packages/upload`（multipart）：浏览器直传插件 zip。
/// 只负责把文件安全落到 `PLUGIN_PACKAGE_DIR/uploads/`，返回相对 packagePath；
/// 安装仍走 `POST /api/admin/plugins/packages`（校验和/签名/manifest 校验不旁路）。
pub async fn upload_package(
    State(state): State<AppState>,
    headers: HeaderMap,
    uri: Uri,
    mut multipart: Multipart,
) -> Result<(StatusCode, Json<UploadedPluginPackageDto>), AppError> {
    authenticate_admin(&state, &headers, &uri).await?;

    let mut uploaded: Option<(String, Vec<u8>)> = None;
    while let Some(field) = multipart
        .next_field()
        .await
        .map_err(|err| AppError::unprocessable(format!("invalid multipart payload: {err}")))?
    {
        let is_file_field = field.file_name().is_some()
            || matches!(field.name(), Some("file" | "package" | "plugin"));
        if !is_file_field {
            continue;
        }
        let file_name = field.file_name().unwrap_or("plugin.zip").to_owned();
        let bytes = field
            .bytes()
            .await
            .map_err(|err| AppError::unprocessable(format!("failed to read upload: {err}")))?;
        uploaded = Some((file_name, bytes.to_vec()));
        break;
    }
    let Some((file_name, bytes)) = uploaded else {
        return Err(AppError::unprocessable(
            "multipart payload must contain a plugin package file field",
        ));
    };
    if bytes.is_empty() {
        return Err(AppError::unprocessable(
            "plugin package upload must not be empty",
        ));
    }
    if bytes.len() > MAX_PLUGIN_UPLOAD_BYTES {
        return Err(AppError::unprocessable(format!(
            "plugin package upload must be at most {MAX_PLUGIN_UPLOAD_BYTES} bytes"
        )));
    }
    // zip 魔数（PK\x03\x04）快速把关；结构性校验在安装阶段完整执行。
    if bytes.len() < 4 || &bytes[0..4] != b"PK\x03\x04" {
        return Err(AppError::unprocessable(
            "plugin package upload must be a zip archive",
        ));
    }

    let nonce = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|value| value.as_nanos())
        .unwrap_or_default();
    let stem = sanitize_path_segment(file_name.trim_end_matches(".zip"));
    let package_path = format!("{UPLOAD_SUBDIR}/{nonce}-{stem}.zip");

    let absolute_path =
        resolve_plugin_package_path(&state.config().plugins.package_dir, &package_path)?;
    if let Some(parent) = absolute_path.parent() {
        fs::create_dir_all(parent).await.map_err(|err| {
            AppError::internal(format!("failed to create upload directory: {err}"))
        })?;
    }
    fs::write(&absolute_path, &bytes)
        .await
        .map_err(|err| AppError::internal(format!("failed to write uploaded package: {err}")))?;

    let checksum_sha256 = Sha256::digest(&bytes)
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect::<String>();

    Ok((
        StatusCode::CREATED,
        Json(UploadedPluginPackageDto {
            package_path,
            size_bytes: bytes.len() as u64,
            checksum_sha256,
        }),
    ))
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

// ---------------------------------------------------------------------------
// Plugin marketplace (remote registry) + uninstall
// ---------------------------------------------------------------------------

const MARKET_CATALOG_MAX_BYTES: usize = 8 * 1024 * 1024;
const MARKET_PACKAGE_MAX_BYTES: usize = 50 * 1024 * 1024;
const MARKET_HTTP_TIMEOUT_SECONDS: u64 = 30;
const MARKET_DOWNLOAD_SUBDIR: &str = "market";

#[derive(Clone, Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct PluginMarketSourceDto {
    pub id: String,
    pub name: String,
    pub url: String,
    pub enabled: bool,
    pub last_synced_at: Option<String>,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct CreatePluginMarketSourceRequestDto {
    pub name: String,
    pub url: String,
}

#[derive(Clone, Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct PluginMarketSyncResponseDto {
    pub synced: i64,
}

#[derive(Clone, Debug, Default, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct MarketCatalogQueryDto {
    pub source_id: Option<String>,
    pub q: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct PluginMarketPermissionDto {
    pub key: String,
    pub reason: Option<String>,
}

#[derive(Clone, Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct PluginMarketCatalogEntryDto {
    pub source_id: String,
    pub plugin_id: String,
    pub name: String,
    pub version: String,
    pub description: Option<String>,
    pub author: Option<String>,
    pub permissions: Vec<PluginMarketPermissionDto>,
    pub icon_url: Option<String>,
    pub download_url: String,
    pub checksum_sha256: Option<String>,
    pub signature: Option<String>,
    /// 本机已安装该插件时的活动包版本（未安装为 None）。
    pub installed_version: Option<String>,
    pub is_installed: bool,
    /// 目录条目版本比已安装版本新（语义化数字段比较）。
    pub has_update: bool,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct InstallMarketPluginRequestDto {
    pub source_id: String,
    pub plugin_id: String,
    pub version: String,
}

/// Remote catalog JSON document format fetched from a market source `url`:
///
/// ```json
/// {
///   "plugins": [
///     {
///       "id": "dev.fbz.notify",
///       "name": "Notify",
///       "version": "1.2.3",
///       "description": "…",
///       "author": "…",
///       "permissions": [ { "key": "notification.send", "reason": "…" } ],
///       "iconUrl": "https://…/icon.png",
///       "downloadUrl": "https://…/notify-1.2.3.zip",
///       "checksumSha256": "<64 hex>",
///       "signature": "ed25519:keyId:signatureHex"
///     }
///   ]
/// }
/// ```
#[derive(Clone, Debug, Deserialize)]
struct RemoteCatalogDocument {
    #[serde(default)]
    plugins: Vec<RemoteCatalogEntry>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
struct RemoteCatalogEntry {
    id: String,
    name: String,
    version: String,
    #[serde(default)]
    description: Option<String>,
    #[serde(default)]
    author: Option<String>,
    #[serde(default)]
    permissions: Vec<RemoteCatalogPermission>,
    #[serde(default)]
    icon_url: Option<String>,
    download_url: String,
    #[serde(default)]
    checksum_sha256: Option<String>,
    #[serde(default)]
    signature: Option<String>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
struct RemoteCatalogPermission {
    key: String,
    #[serde(default)]
    reason: Option<String>,
}

pub async fn list_market_sources(
    State(state): State<AppState>,
    headers: HeaderMap,
    uri: Uri,
) -> Result<Json<Vec<PluginMarketSourceDto>>, AppError> {
    authenticate_admin(&state, &headers, &uri).await?;
    let Some(database) = state.database() else {
        return Err(AppError::internal("database is not configured"));
    };

    let sources = PluginRepository::new(database.clone())
        .list_market_sources()
        .await
        .map_err(plugin_read_error_to_app_error)?
        .into_iter()
        .map(PluginMarketSourceDto::from)
        .collect();

    Ok(Json(sources))
}

pub async fn create_market_source(
    State(state): State<AppState>,
    headers: HeaderMap,
    uri: Uri,
    Json(payload): Json<CreatePluginMarketSourceRequestDto>,
) -> Result<(StatusCode, Json<PluginMarketSourceDto>), AppError> {
    authenticate_admin(&state, &headers, &uri).await?;
    let name = validate_market_source_name(&payload.name)?;
    let url = validate_market_url("url", &payload.url)?;
    let Some(database) = state.database() else {
        return Err(AppError::internal("database is not configured"));
    };

    let source = PluginRepository::new(database.clone())
        .create_market_source(CreatePluginMarketSourceInput { name, url })
        .await
        .map_err(market_source_write_error_to_app_error)?;

    Ok((StatusCode::CREATED, Json(source.into())))
}

pub async fn delete_market_source(
    State(state): State<AppState>,
    Path(source_id): Path<String>,
    headers: HeaderMap,
    uri: Uri,
) -> Result<StatusCode, AppError> {
    authenticate_admin(&state, &headers, &uri).await?;
    let source_id = validate_uuid_public_id("sourceId", &source_id)?.to_owned();
    let Some(database) = state.database() else {
        return Err(AppError::internal("database is not configured"));
    };

    let deleted = PluginRepository::new(database.clone())
        .delete_market_source(&source_id)
        .await
        .map_err(plugin_read_error_to_app_error)?;
    if !deleted {
        return Err(AppError::not_found("plugin market source not found"));
    }

    Ok(StatusCode::NO_CONTENT)
}

#[derive(Clone, Debug, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct SetMarketSourceEnabledRequestDto {
    pub enabled: bool,
}

/// `POST /api/admin/plugins/market/sources/{source_id}/enabled`：启停市场源。
/// 停用的源保留缓存条目，但目录浏览与市场安装不再露出。
pub async fn set_market_source_enabled(
    State(state): State<AppState>,
    Path(source_id): Path<String>,
    headers: HeaderMap,
    uri: Uri,
    Json(payload): Json<SetMarketSourceEnabledRequestDto>,
) -> Result<Json<PluginMarketSourceDto>, AppError> {
    authenticate_admin(&state, &headers, &uri).await?;
    let source_id = validate_uuid_public_id("sourceId", &source_id)?.to_owned();
    let Some(database) = state.database() else {
        return Err(AppError::internal("database is not configured"));
    };

    let record = PluginRepository::new(database.clone())
        .set_market_source_enabled(&source_id, payload.enabled)
        .await
        .map_err(plugin_read_error_to_app_error)?
        .ok_or_else(|| AppError::not_found("plugin market source not found"))?;

    Ok(Json(record.into()))
}

pub async fn sync_market_source(
    State(state): State<AppState>,
    Path(source_id): Path<String>,
    headers: HeaderMap,
    uri: Uri,
) -> Result<Json<PluginMarketSyncResponseDto>, AppError> {
    authenticate_admin(&state, &headers, &uri).await?;
    let source_id = validate_uuid_public_id("sourceId", &source_id)?.to_owned();
    let Some(database) = state.database() else {
        return Err(AppError::internal("database is not configured"));
    };

    let repository = PluginRepository::new(database.clone());
    let Some(target) = repository
        .get_market_source_sync_target(&source_id)
        .await
        .map_err(plugin_read_error_to_app_error)?
    else {
        return Err(AppError::not_found("plugin market source not found"));
    };

    let document = fetch_remote_catalog(&target.url).await?;
    let entries = normalize_remote_catalog_entries(document);
    let synced = repository
        .replace_market_entries(target.internal_id, &entries)
        .await
        .map_err(plugin_read_error_to_app_error)?;

    Ok(Json(PluginMarketSyncResponseDto { synced }))
}

pub async fn market_catalog(
    State(state): State<AppState>,
    headers: HeaderMap,
    uri: Uri,
    Query(query): Query<MarketCatalogQueryDto>,
) -> Result<Json<Vec<PluginMarketCatalogEntryDto>>, AppError> {
    authenticate_admin(&state, &headers, &uri).await?;
    let source_id = query
        .source_id
        .as_deref()
        .map(|value| validate_uuid_public_id("sourceId", value))
        .transpose()?
        .map(str::to_owned);
    let query_text = query
        .q
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(|value| validate_bounded_query_text("q", value, 200))
        .transpose()?
        .map(str::to_owned);
    let Some(database) = state.database() else {
        return Err(AppError::internal("database is not configured"));
    };

    let repository = PluginRepository::new(database.clone());
    let installed_versions: std::collections::HashMap<String, Option<String>> = repository
        .list_installed_plugin_versions()
        .await
        .map_err(plugin_read_error_to_app_error)?
        .into_iter()
        .collect();

    let entries = repository
        .list_market_entries(source_id.as_deref(), query_text.as_deref())
        .await
        .map_err(plugin_read_error_to_app_error)?
        .into_iter()
        .map(|record| {
            let mut entry = PluginMarketCatalogEntryDto::from(record);
            if let Some(installed) = installed_versions.get(&entry.plugin_id) {
                entry.is_installed = true;
                entry.installed_version = installed.clone();
                entry.has_update = installed
                    .as_deref()
                    .is_some_and(|installed| version_is_newer(&entry.version, installed));
            }
            entry
        })
        .collect();

    Ok(Json(entries))
}

pub async fn install_market_plugin(
    State(state): State<AppState>,
    headers: HeaderMap,
    uri: Uri,
    Json(payload): Json<InstallMarketPluginRequestDto>,
) -> Result<(StatusCode, Json<InstalledPluginPackageDto>), AppError> {
    authenticate_admin(&state, &headers, &uri).await?;
    let source_id = validate_uuid_public_id("sourceId", &payload.source_id)?.to_owned();
    let plugin_id = validate_plugin_id_filter(&payload.plugin_id)?.to_owned();
    let version = validate_market_version(&payload.version)?;
    let Some(database) = state.database() else {
        return Err(AppError::internal("database is not configured"));
    };

    let repository = PluginRepository::new(database.clone());
    let Some(target) = repository
        .get_market_entry_install_target(&source_id, &plugin_id, &version)
        .await
        .map_err(plugin_read_error_to_app_error)?
    else {
        return Err(AppError::not_found("plugin market entry not found"));
    };

    // Reject non-http(s) download URLs (blocks file:// / SSRF vectors).
    validate_market_url("downloadUrl", &target.download_url)?;
    let requested_checksum_sha256 = target
        .checksum_sha256
        .as_deref()
        .map(parse_sha256_hex)
        .transpose()?;

    let config = &state.config().plugins;
    // Download to a unique relative path inside the plugin package dir so a
    // failed install can be cleaned up without clobbering an existing package.
    let package_path = market_download_relative_path(&target.plugin_id, &target.version);
    download_market_package(config, &package_path, &target.download_url).await?;

    let install_result = install_downloaded_market_package(
        &state,
        &repository,
        &package_path,
        requested_checksum_sha256.as_deref(),
        target.signature.as_deref(),
        &target.plugin_id,
        &target.version,
    )
    .await;

    match install_result {
        Ok(record) => Ok((StatusCode::CREATED, Json(record.into()))),
        Err(err) => {
            // Remove the freshly downloaded artifact; it never became a package.
            remove_market_download(config, &package_path).await;
            Err(err)
        }
    }
}

pub async fn uninstall_plugin(
    State(state): State<AppState>,
    Path(plugin_id): Path<String>,
    headers: HeaderMap,
    uri: Uri,
) -> Result<StatusCode, AppError> {
    authenticate_admin(&state, &headers, &uri).await?;
    validate_non_empty_path_param("pluginId", &plugin_id)?;
    let Some(database) = state.database() else {
        return Err(AppError::internal("database is not configured"));
    };

    let Some(record) = PluginRepository::new(database.clone())
        .uninstall_plugin(&plugin_id)
        .await
        .map_err(plugin_read_error_to_app_error)?
    else {
        return Err(AppError::not_found("plugin installation not found"));
    };

    cleanup_uninstalled_plugin_files(&state.config().plugins, &record).await;

    Ok(StatusCode::NO_CONTENT)
}

#[allow(clippy::too_many_arguments)]
async fn install_downloaded_market_package(
    state: &AppState,
    repository: &PluginRepository,
    package_path: &str,
    requested_checksum_sha256: Option<&[u8]>,
    signature: Option<&str>,
    expected_plugin_id: &str,
    expected_version: &str,
) -> Result<InstalledPluginPackageRecord, AppError> {
    let config = &state.config().plugins;
    let checksum_sha256 =
        verify_plugin_package_file(config, package_path, requested_checksum_sha256).await?;
    let manifest = read_plugin_manifest_from_package(config, package_path).await?;

    // The catalog metadata must describe the package it points at.
    if manifest.id.trim() != expected_plugin_id.trim() {
        return Err(AppError::unprocessable(
            "downloaded package manifest id does not match market entry",
        ));
    }
    if manifest.version.trim() != expected_version.trim() {
        return Err(AppError::unprocessable(
            "downloaded package manifest version does not match market entry",
        ));
    }

    let validated_manifest = manifest
        .validate()
        .map_err(plugin_manifest_error_to_app_error)?;
    let signature = validate_plugin_package_signature(
        config,
        signature,
        &validated_manifest,
        &checksum_sha256,
    )?;
    prepare_plugin_package_archive(config, package_path, &validated_manifest.manifest).await?;

    repository
        .install_package(InstallPluginPackageInput {
            package_path: package_path.to_owned(),
            checksum_sha256: Some(checksum_sha256),
            signature,
            validated_manifest,
        })
        .await
        .map_err(plugin_repository_error_to_app_error)
}

async fn fetch_remote_catalog(url: &str) -> Result<RemoteCatalogDocument, AppError> {
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(MARKET_HTTP_TIMEOUT_SECONDS))
        .build()
        .map_err(|err| AppError::internal(format!("failed to build HTTP client: {err}")))?;
    let response = client
        .get(url)
        .send()
        .await
        .map_err(|err| AppError::unprocessable(format!("market source request failed: {err}")))?;
    if !response.status().is_success() {
        return Err(AppError::unprocessable(format!(
            "market source returned status {}",
            response.status()
        )));
    }
    let bytes = read_response_body_bounded(response, MARKET_CATALOG_MAX_BYTES).await?;
    serde_json::from_slice::<RemoteCatalogDocument>(&bytes)
        .map_err(|err| AppError::unprocessable(format!("market catalog JSON is invalid: {err}")))
}

fn normalize_remote_catalog_entries(document: RemoteCatalogDocument) -> Vec<NewPluginMarketEntry> {
    let mut entries = Vec::new();
    for entry in document.plugins {
        // Skip entries missing required fields or with unusable download URLs.
        if entry.id.trim().is_empty()
            || entry.id.trim().len() > 128
            || entry.name.trim().is_empty()
            || entry.version.trim().is_empty()
        {
            continue;
        }
        if !is_http_url(&entry.download_url) || entry.download_url.trim().len() > 2048 {
            continue;
        }
        let checksum_sha256 = match entry.checksum_sha256.as_deref() {
            Some(value) => match normalize_checksum_hex(value) {
                Some(value) => Some(value),
                None => continue,
            },
            None => None,
        };
        let icon_url = entry
            .icon_url
            .as_deref()
            .filter(|value| is_http_url(value) && value.len() <= 2048)
            .map(str::to_owned);
        let permissions = serde_json::to_value(&entry.permissions).unwrap_or(Value::Array(vec![]));
        let raw = serde_json::to_value(&entry).unwrap_or(Value::Object(Map::new()));

        entries.push(NewPluginMarketEntry {
            plugin_id: entry.id.trim().to_owned(),
            name: entry.name.trim().to_owned(),
            version: entry.version.trim().to_owned(),
            description: normalize_optional_text(entry.description),
            author: normalize_optional_text(entry.author),
            permissions,
            icon_url,
            download_url: entry.download_url.trim().to_owned(),
            checksum_sha256,
            signature: normalize_optional_text(entry.signature),
            raw,
        });
    }
    entries
}

async fn read_response_body_bounded(
    response: reqwest::Response,
    max_bytes: usize,
) -> Result<Vec<u8>, AppError> {
    // Reject early when the server advertises an oversized body.
    if let Some(content_length) = response.content_length()
        && content_length > max_bytes as u64
    {
        return Err(AppError::unprocessable(format!(
            "download exceeds maximum size of {max_bytes} bytes"
        )));
    }
    let bytes = response
        .bytes()
        .await
        .map_err(|err| AppError::unprocessable(format!("download failed: {err}")))?;
    if bytes.len() > max_bytes {
        return Err(AppError::unprocessable(format!(
            "download exceeds maximum size of {max_bytes} bytes"
        )));
    }
    Ok(bytes.to_vec())
}

async fn download_market_package(
    config: &PluginRuntimeConfig,
    package_path: &str,
    download_url: &str,
) -> Result<(), AppError> {
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(MARKET_HTTP_TIMEOUT_SECONDS))
        .build()
        .map_err(|err| AppError::internal(format!("failed to build HTTP client: {err}")))?;
    let response = client
        .get(download_url)
        .send()
        .await
        .map_err(|err| AppError::unprocessable(format!("package download request failed: {err}")))?;
    if !response.status().is_success() {
        return Err(AppError::unprocessable(format!(
            "package download returned status {}",
            response.status()
        )));
    }
    let bytes = read_response_body_bounded(response, MARKET_PACKAGE_MAX_BYTES).await?;
    if bytes.is_empty() {
        return Err(AppError::unprocessable("downloaded package is empty"));
    }

    let absolute_path = resolve_plugin_package_path(&config.package_dir, package_path)?;
    if let Some(parent) = absolute_path.parent() {
        fs::create_dir_all(parent).await.map_err(|err| {
            AppError::internal(format!("failed to create plugin package directory: {err}"))
        })?;
    }
    fs::write(&absolute_path, &bytes)
        .await
        .map_err(|err| AppError::internal(format!("failed to write downloaded package: {err}")))?;

    Ok(())
}

async fn read_plugin_manifest_from_package(
    config: &PluginRuntimeConfig,
    package_path: &str,
) -> Result<PluginManifest, AppError> {
    let absolute_path = resolve_plugin_package_path(&config.package_dir, package_path)?;
    task::spawn_blocking(move || {
        let file = std_fs::File::open(&absolute_path).map_err(|err| {
            AppError::unprocessable(format!("plugin package cannot be opened: {err}"))
        })?;
        let mut archive = ZipArchive::new(file).map_err(|err| {
            AppError::unprocessable(format!("plugin package must be a valid zip: {err}"))
        })?;
        read_plugin_manifest_from_zip(&mut archive)
    })
    .await
    .map_err(|err| AppError::internal(format!("plugin manifest read task failed: {err}")))?
}

async fn remove_market_download(config: &PluginRuntimeConfig, package_path: &str) {
    let Ok(absolute_path) = resolve_plugin_package_path(&config.package_dir, package_path) else {
        return;
    };
    if let Err(err) = fs::remove_file(&absolute_path).await
        && err.kind() != ErrorKind::NotFound
    {
        warn!(
            error = %err,
            path = %absolute_path.display(),
            "failed to remove downloaded plugin package after failed install"
        );
    }
}

async fn cleanup_uninstalled_plugin_files(
    config: &PluginRuntimeConfig,
    record: &PluginUninstallRecord,
) {
    for package_path in &record.package_paths {
        let Ok(absolute_path) = resolve_plugin_package_path(&config.package_dir, package_path)
        else {
            continue;
        };
        if let Err(err) = fs::remove_file(&absolute_path).await
            && err.kind() != ErrorKind::NotFound
        {
            warn!(
                error = %err,
                path = %absolute_path.display(),
                plugin_id = %record.plugin_id,
                "failed to remove plugin package file during uninstall"
            );
        }
    }

    // Remove the extracted tree for this plugin (all versions).
    let extract_root = config
        .package_dir
        .join(PLUGIN_PACKAGE_EXTRACTED_DIR)
        .join(record.plugin_id.trim());
    if let Err(err) = fs::remove_dir_all(&extract_root).await
        && err.kind() != ErrorKind::NotFound
    {
        warn!(
            error = %err,
            path = %extract_root.display(),
            plugin_id = %record.plugin_id,
            "failed to remove extracted plugin directory during uninstall"
        );
    }
}

fn market_download_relative_path(plugin_id: &str, version: &str) -> String {
    let nonce = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|value| value.as_nanos())
        .unwrap_or_default();
    format!(
        "{}/{}/{}-{}.zip",
        MARKET_DOWNLOAD_SUBDIR,
        sanitize_path_segment(plugin_id),
        sanitize_path_segment(version),
        nonce
    )
}

fn sanitize_path_segment(value: &str) -> String {
    let sanitized: String = value
        .trim()
        .chars()
        .map(|ch| {
            if ch.is_ascii_alphanumeric() || matches!(ch, '.' | '-' | '_') {
                ch
            } else {
                '_'
            }
        })
        .collect();
    // Never allow a segment that reduces to a traversal token or is empty.
    if sanitized.is_empty() || sanitized == "." || sanitized == ".." {
        "plugin".to_owned()
    } else {
        sanitized
    }
}

fn is_http_url(value: &str) -> bool {
    let value = value.trim();
    match reqwest::Url::parse(value) {
        Ok(url) => matches!(url.scheme(), "http" | "https") && url.has_host(),
        Err(_) => false,
    }
}

fn normalize_checksum_hex(value: &str) -> Option<String> {
    let value = value.trim().to_ascii_lowercase();
    if value.len() == 64 && value.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        Some(value)
    } else {
        None
    }
}

fn normalize_optional_text(value: Option<String>) -> Option<String> {
    value
        .map(|value| value.trim().to_owned())
        .filter(|value| !value.is_empty())
}

fn validate_market_source_name(value: &str) -> Result<String, AppError> {
    let value = value.trim();
    if value.is_empty() {
        return Err(AppError::unprocessable("name is required"));
    }
    if value.len() > 200 {
        return Err(AppError::unprocessable(
            "name must be at most 200 characters",
        ));
    }
    Ok(value.to_owned())
}

fn validate_market_version(value: &str) -> Result<String, AppError> {
    let value = value.trim();
    if value.is_empty() {
        return Err(AppError::unprocessable("version is required"));
    }
    if value.len() > 64 {
        return Err(AppError::unprocessable(
            "version must be at most 64 characters",
        ));
    }
    if value.contains(char::is_whitespace) || value.contains('/') || value.contains('\\') {
        return Err(AppError::unprocessable(
            "version must not contain whitespace or path separators",
        ));
    }
    Ok(value.to_owned())
}

fn validate_market_url(field: &str, value: &str) -> Result<String, AppError> {
    let value = value.trim();
    if value.is_empty() {
        return Err(AppError::unprocessable(format!("{field} is required")));
    }
    if value.len() > 2048 {
        return Err(AppError::unprocessable(format!(
            "{field} must be at most 2048 characters"
        )));
    }
    if !is_http_url(value) {
        return Err(AppError::unprocessable(format!(
            "{field} must be an absolute http or https URL"
        )));
    }
    Ok(value.to_owned())
}

fn market_source_write_error_to_app_error(error: sqlx::Error) -> AppError {
    if is_unique_violation(&error) {
        return AppError::conflict("plugin market source url already exists");
    }
    AppError::internal(format!("failed to create plugin market source: {error}"))
}

impl From<PluginMarketSourceRecord> for PluginMarketSourceDto {
    fn from(record: PluginMarketSourceRecord) -> Self {
        Self {
            id: record.id,
            name: record.name,
            url: record.url,
            enabled: record.enabled,
            last_synced_at: record.last_synced_at,
        }
    }
}

impl From<PluginMarketEntryRecord> for PluginMarketCatalogEntryDto {
    fn from(record: PluginMarketEntryRecord) -> Self {
        let permissions = serde_json::from_value::<Vec<PluginMarketPermissionDto>>(
            record.permissions,
        )
        .unwrap_or_default();
        Self {
            source_id: record.source_id,
            plugin_id: record.plugin_id,
            name: record.name,
            version: record.version,
            description: record.description,
            author: record.author,
            permissions,
            icon_url: record.icon_url,
            download_url: record.download_url,
            checksum_sha256: record.checksum_sha256,
            signature: record.signature,
            installed_version: None,
            is_installed: false,
            has_update: false,
        }
    }
}

/// 宽松语义化版本比较：按 `.` 切段做数字比较（非数字段退化为字典序），
/// 段数不足按 0 补齐。返回 `left` 是否比 `right` 新。
fn version_is_newer(left: &str, right: &str) -> bool {
    let left_segments: Vec<&str> = left.trim().trim_start_matches('v').split('.').collect();
    let right_segments: Vec<&str> = right.trim().trim_start_matches('v').split('.').collect();
    let segments = left_segments.len().max(right_segments.len());

    for index in 0..segments {
        let left_segment = left_segments.get(index).copied().unwrap_or("0");
        let right_segment = right_segments.get(index).copied().unwrap_or("0");
        match (
            left_segment.parse::<u64>().ok(),
            right_segment.parse::<u64>().ok(),
        ) {
            (Some(left_num), Some(right_num)) => {
                if left_num != right_num {
                    return left_num > right_num;
                }
            }
            _ => {
                if left_segment != right_segment {
                    return left_segment > right_segment;
                }
            }
        }
    }

    false
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

fn validate_bounded_query_text<'a>(
    field: &str,
    value: &'a str,
    max_len: usize,
) -> Result<&'a str, AppError> {
    let value = value.trim();
    if value.is_empty() {
        return Err(AppError::unprocessable(format!("{field} is required")));
    }
    if value.len() > max_len {
        return Err(AppError::unprocessable(format!(
            "{field} must be at most {max_len} characters"
        )));
    }
    Ok(value)
}

fn validate_plugin_id_filter(value: &str) -> Result<&str, AppError> {
    let value = validate_bounded_query_text("pluginId", value, 128)?;
    if value.contains(char::is_whitespace) || value.contains('/') || value.contains('\\') {
        return Err(AppError::unprocessable(
            "pluginId must not contain whitespace or path separators",
        ));
    }
    Ok(value)
}

fn validate_plugin_package_status(value: &str) -> Result<&str, AppError> {
    let value = validate_bounded_query_text("packageStatus", value, 32)?;
    if matches!(
        value,
        "pending_approval" | "approved" | "rejected" | "disabled"
    ) {
        return Ok(value);
    }

    Err(AppError::unprocessable(
        "packageStatus must be one of pending_approval, approved, rejected, or disabled",
    ))
}

fn validate_plugin_approval_status(value: &str) -> Result<&str, AppError> {
    let value = validate_bounded_query_text("approvalStatus", value, 32)?;
    if matches!(
        value,
        "pending_approval" | "approved" | "rejected" | "requires_reapproval"
    ) {
        return Ok(value);
    }

    Err(AppError::unprocessable(
        "approvalStatus must be one of pending_approval, approved, rejected, or requires_reapproval",
    ))
}

fn validate_plugin_runtime_filter(value: &str) -> Result<&str, AppError> {
    let value = validate_bounded_query_text("runtime", value, 16)?;
    if matches!(value, "wasi" | "http") {
        return Ok(value);
    }

    Err(AppError::unprocessable(
        "runtime must be one of wasi or http",
    ))
}

fn validate_uuid_public_id<'a>(field: &str, value: &'a str) -> Result<&'a str, AppError> {
    let value = validate_bounded_query_text(field, value, 128)?;
    let bytes = value.as_bytes();
    let has_uuid_shape = bytes.len() == 36
        && [8, 13, 18, 23]
            .into_iter()
            .all(|index| bytes[index] == b'-')
        && bytes
            .iter()
            .enumerate()
            .all(|(index, byte)| matches!(index, 8 | 13 | 18 | 23) || byte.is_ascii_hexdigit());
    if !has_uuid_shape {
        return Err(AppError::unprocessable(format!(
            "{field} must be a UUID public id"
        )));
    }
    Ok(value)
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

fn plugin_package_list_limit(limit: Option<u32>) -> i64 {
    i64::from(limit.unwrap_or(100).clamp(1, 500))
}

fn pagination_headers(has_more: bool, next_cursor: Option<&str>) -> Result<HeaderMap, AppError> {
    let mut headers = HeaderMap::new();
    headers.insert(
        "x-fbz-has-more",
        HeaderValue::from_static(if has_more { "true" } else { "false" }),
    );
    if let Some(next_cursor) = next_cursor {
        headers.insert(
            "x-fbz-next-cursor",
            HeaderValue::from_str(next_cursor).map_err(|err| {
                AppError::internal(format!("failed to encode next cursor header: {err}"))
            })?,
        );
    }

    Ok(headers)
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

    let signature_bytes = parse_ed25519_signature_hex(signature_hex)
        .map_err(|err| AppError::unprocessable(err.message()))?;
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
    validate_plugin_signature_key_id(key_id)
        .map_err(|err| AppError::unprocessable(err.message()))?;
    Ok((scheme, key_id, signature_hex))
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

impl From<PluginPackageSummaryRecord> for PluginPackageSummaryDto {
    fn from(record: PluginPackageSummaryRecord) -> Self {
        Self {
            package_id: record.package_id,
            plugin_id: record.plugin_id,
            package_version: record.package_version,
            api_version: record.api_version,
            runtime: record.runtime,
            name: record.name,
            package_status: record.package_status,
            signature_present: record.signature_present,
            approval_status: record.approval_status,
            enabled: record.enabled,
            active: record.active,
            created_at: record.created_at,
            updated_at: record.updated_at,
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
    use crate::plugins::signing::hex_encode;

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
    fn plugin_list_filters_are_validated() {
        assert_eq!(
            validate_plugin_approval_status("pending_approval").unwrap(),
            "pending_approval"
        );
        assert_eq!(
            validate_plugin_approval_status("requires_reapproval").unwrap(),
            "requires_reapproval"
        );
        assert!(validate_plugin_approval_status("pending").is_err());
        assert!(validate_plugin_approval_status("").is_err());

        assert_eq!(validate_plugin_runtime_filter("wasi").unwrap(), "wasi");
        assert_eq!(validate_plugin_runtime_filter("http").unwrap(), "http");
        assert!(validate_plugin_runtime_filter("native").is_err());

        assert_eq!(
            validate_uuid_public_id("cursor", "00000000-0000-0000-0000-000000000001").unwrap(),
            "00000000-0000-0000-0000-000000000001"
        );
        assert!(validate_uuid_public_id("cursor", "plugin-1").is_err());
    }

    #[test]
    fn plugin_package_list_limit_is_bounded() {
        assert_eq!(plugin_package_list_limit(None), 100);
        assert_eq!(plugin_package_list_limit(Some(0)), 1);
        assert_eq!(plugin_package_list_limit(Some(10)), 10);
        assert_eq!(plugin_package_list_limit(Some(5_000)), 500);
    }

    #[test]
    fn plugin_package_list_filters_are_validated() {
        assert_eq!(
            validate_plugin_id_filter("dev.fbz.notify").unwrap(),
            "dev.fbz.notify"
        );
        assert!(validate_plugin_id_filter("").is_err());
        assert!(validate_plugin_id_filter("dev fbz notify").is_err());
        assert!(validate_plugin_id_filter("dev/fbz/notify").is_err());

        assert_eq!(
            validate_plugin_package_status("pending_approval").unwrap(),
            "pending_approval"
        );
        assert_eq!(
            validate_plugin_package_status("approved").unwrap(),
            "approved"
        );
        assert!(validate_plugin_package_status("pending").is_err());

        assert_eq!(validate_plugin_runtime_filter("wasi").unwrap(), "wasi");
        assert_eq!(validate_plugin_runtime_filter("http").unwrap(), "http");
        assert!(validate_plugin_runtime_filter("native").is_err());

        assert_eq!(
            validate_uuid_public_id("cursor", "00000000-0000-0000-0000-000000000001").unwrap(),
            "00000000-0000-0000-0000-000000000001"
        );
        assert!(validate_uuid_public_id("cursor", "package-1").is_err());
    }

    #[test]
    fn plugin_package_summary_dto_preserves_review_context() {
        let dto = PluginPackageSummaryDto::from(PluginPackageSummaryRecord {
            package_id: "00000000-0000-0000-0000-000000000001".to_owned(),
            plugin_id: "dev.fbz.notify".to_owned(),
            package_version: "1.2.3".to_owned(),
            api_version: "1".to_owned(),
            runtime: "http".to_owned(),
            name: "Notify".to_owned(),
            package_status: "pending_approval".to_owned(),
            signature_present: true,
            approval_status: Some("approved".to_owned()),
            enabled: Some(false),
            active: false,
            created_at: "2026-06-23T00:00:00Z".to_owned(),
            updated_at: "2026-06-23T00:00:00Z".to_owned(),
        });

        assert_eq!(dto.plugin_id, "dev.fbz.notify");
        assert_eq!(dto.package_status, "pending_approval");
        assert!(dto.signature_present);
        assert_eq!(dto.approval_status.as_deref(), Some("approved"));
        assert_eq!(dto.enabled, Some(false));
        assert!(!dto.active);
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
