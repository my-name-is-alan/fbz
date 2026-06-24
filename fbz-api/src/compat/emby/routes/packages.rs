use axum::{
    Json,
    extract::{Path, Query, State},
    http::{HeaderMap, StatusCode, Uri},
    response::{IntoResponse, Response},
};
use serde::{Deserialize, Serialize};

use crate::{error::AppError, state::AppState};

use super::access::authenticate_request_user;

#[derive(Clone, Debug, Default, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "PascalCase")]
pub struct PackagesQuery {
    pub package_type: Option<String>,
    pub target_systems: Option<String>,
    pub is_premium: Option<bool>,
    pub is_adult: Option<bool>,
}

#[derive(Clone, Debug, Default, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "PascalCase")]
pub struct PackageByNameQuery {
    pub assembly_guid: Option<String>,
}

#[derive(Clone, Debug, Default, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "PascalCase")]
pub struct PackageUpdatesQuery {
    pub package_type: Option<String>,
}

#[derive(Clone, Debug, Default, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "PascalCase")]
pub struct InstallPackageQuery {
    pub assembly_guid: Option<String>,
    pub version: Option<String>,
    pub update_class: Option<String>,
}

#[derive(Clone, Debug, PartialEq)]
struct NormalizedPackagesQuery {
    package_type: Option<PackageTypeDto>,
    target_systems: Option<Vec<PackageTargetSystemDto>>,
    is_premium: Option<bool>,
    is_adult: Option<bool>,
}

#[derive(Clone, Debug, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct PackageInfoDto {
    pub id: String,
    pub name: String,
    pub short_description: String,
    pub overview: String,
    pub is_premium: bool,
    pub adult: bool,
    pub rich_desc_url: Option<String>,
    pub thumb_image: Option<String>,
    pub preview_image: Option<String>,
    #[serde(rename = "type")]
    pub package_type: PackageTypeDto,
    pub target_filename: String,
    pub owner: String,
    pub category: String,
    pub tile_color: Option<String>,
    pub feature_id: Option<String>,
    pub price: f32,
    pub target_system: PackageTargetSystemDto,
    pub guid: Option<String>,
    pub is_registered: bool,
    pub exp_date: Option<String>,
    pub versions: Vec<PackageVersionInfoDto>,
    pub enable_in_app_store: bool,
    pub installs: i32,
}

#[derive(Clone, Copy, Debug, Serialize, PartialEq, Eq)]
pub enum PackageTypeDto {
    System,
    UserInstalled,
}

#[derive(Clone, Copy, Debug, Serialize, PartialEq, Eq)]
pub enum PackageTargetSystemDto {
    Server,
    MBTheater,
    MBClassic,
    Other,
}

#[derive(Clone, Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct PackageVersionInfoDto {
    pub name: String,
    pub guid: String,
    pub version_str: String,
    pub classification: PackageVersionClassDto,
    pub description: String,
    pub required_version_str: String,
    pub source_url: String,
    pub checksum: String,
    pub target_filename: String,
    pub info_url: String,
    pub runtimes: String,
    pub timestamp: String,
}

#[derive(Clone, Copy, Debug, Serialize, PartialEq, Eq)]
pub enum PackageVersionClassDto {
    Release,
    Beta,
    Dev,
}

pub async fn packages(
    State(state): State<AppState>,
    Query(query): Query<PackagesQuery>,
    headers: HeaderMap,
    uri: Uri,
) -> Result<Json<Vec<PackageInfoDto>>, AppError> {
    authenticate_request_user(&state, &headers, &uri).await?;
    let query = normalize_packages_query(query)?;

    Ok(Json(package_catalog(&query)))
}

pub async fn package_by_name(
    State(state): State<AppState>,
    Path(package_name): Path<String>,
    Query(query): Query<PackageByNameQuery>,
    headers: HeaderMap,
    uri: Uri,
) -> Result<Json<PackageInfoDto>, AppError> {
    authenticate_request_user(&state, &headers, &uri).await?;
    let package_name = validate_package_path_value("package name", &package_name)?;
    let assembly_guid = query
        .assembly_guid
        .as_deref()
        .map(|value| validate_safe_query_text("AssemblyGuid", value, 128))
        .transpose()?;

    find_package(package_name, assembly_guid).map(Json)
}

pub async fn package_updates(
    State(state): State<AppState>,
    Query(query): Query<PackageUpdatesQuery>,
    headers: HeaderMap,
    uri: Uri,
) -> Result<Json<Vec<PackageVersionInfoDto>>, AppError> {
    authenticate_admin_compatible(&state, &headers, &uri).await?;
    if let Some(package_type) = query.package_type.as_deref() {
        normalize_package_type(package_type)?;
    }

    Ok(Json(Vec::new()))
}

pub async fn install_package(
    State(state): State<AppState>,
    Path(package_name): Path<String>,
    Query(query): Query<InstallPackageQuery>,
    headers: HeaderMap,
    uri: Uri,
) -> Result<Response, AppError> {
    authenticate_admin_compatible(&state, &headers, &uri).await?;
    validate_package_path_value("package name", &package_name)?;
    if let Some(assembly_guid) = query.assembly_guid.as_deref() {
        validate_safe_query_text("AssemblyGuid", assembly_guid, 128)?;
    }
    if let Some(version) = query.version.as_deref() {
        validate_safe_query_text("Version", version, 64)?;
    }
    if let Some(update_class) = query.update_class.as_deref() {
        normalize_update_class(update_class)?;
    }

    Err(AppError::conflict(
        "Emby package catalog installation is disabled; use FBZ signed plugin packages",
    ))
}

pub async fn cancel_package_installation(
    State(state): State<AppState>,
    Path(installation_id): Path<String>,
    headers: HeaderMap,
    uri: Uri,
) -> Result<Response, AppError> {
    authenticate_admin_compatible(&state, &headers, &uri).await?;
    validate_package_path_value("installation id", &installation_id)?;

    Ok((StatusCode::OK, "").into_response())
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

fn normalize_packages_query(query: PackagesQuery) -> Result<NormalizedPackagesQuery, AppError> {
    Ok(NormalizedPackagesQuery {
        package_type: query
            .package_type
            .as_deref()
            .map(normalize_package_type)
            .transpose()?,
        target_systems: query
            .target_systems
            .as_deref()
            .map(normalize_target_systems)
            .transpose()?,
        is_premium: query.is_premium,
        is_adult: query.is_adult,
    })
}

fn package_catalog(query: &NormalizedPackagesQuery) -> Vec<PackageInfoDto> {
    let package = fbz_core_package();
    if query
        .package_type
        .is_some_and(|package_type| package_type != package.package_type)
        || query
            .target_systems
            .as_ref()
            .is_some_and(|target_systems| !target_systems.contains(&package.target_system))
        || query
            .is_premium
            .is_some_and(|is_premium| is_premium != package.is_premium)
        || query
            .is_adult
            .is_some_and(|is_adult| is_adult != package.adult)
    {
        return Vec::new();
    }

    vec![package]
}

fn find_package(
    package_name: &str,
    assembly_guid: Option<&str>,
) -> Result<PackageInfoDto, AppError> {
    let package = fbz_core_package();
    let wanted = package_name.trim();
    let matches_name = package.id.eq_ignore_ascii_case(wanted)
        || package.name.eq_ignore_ascii_case(wanted)
        || package
            .guid
            .as_deref()
            .is_some_and(|guid| guid.eq_ignore_ascii_case(wanted));
    let matches_guid = assembly_guid.is_none_or(|assembly_guid| {
        package
            .guid
            .as_deref()
            .is_some_and(|guid| guid.eq_ignore_ascii_case(assembly_guid.trim()))
    });

    if matches_name && matches_guid {
        Ok(package)
    } else {
        Err(AppError::not_found("package not found"))
    }
}

fn fbz_core_package() -> PackageInfoDto {
    let version = env!("CARGO_PKG_VERSION").to_owned();
    PackageInfoDto {
        id: "fbz-core".to_owned(),
        name: "FBZ Core".to_owned(),
        short_description: "Core FBZ API server package".to_owned(),
        overview: "Provides the FBZ API server and Emby compatibility surface.".to_owned(),
        is_premium: false,
        adult: false,
        rich_desc_url: None,
        thumb_image: None,
        preview_image: None,
        package_type: PackageTypeDto::System,
        target_filename: "fbz-api".to_owned(),
        owner: "FBZ".to_owned(),
        category: "Server".to_owned(),
        tile_color: None,
        feature_id: None,
        price: 0.0,
        target_system: PackageTargetSystemDto::Server,
        guid: Some("fbz-core".to_owned()),
        is_registered: true,
        exp_date: None,
        versions: vec![PackageVersionInfoDto {
            name: "FBZ Core".to_owned(),
            guid: "fbz-core".to_owned(),
            version_str: version,
            classification: PackageVersionClassDto::Release,
            description: "Built-in FBZ API server package.".to_owned(),
            required_version_str: "0.0.0".to_owned(),
            source_url: String::new(),
            checksum: String::new(),
            target_filename: "fbz-api".to_owned(),
            info_url: String::new(),
            runtimes: "server".to_owned(),
            timestamp: "1970-01-01T00:00:00Z".to_owned(),
        }],
        enable_in_app_store: false,
        installs: 0,
    }
}

fn normalize_package_type(value: &str) -> Result<PackageTypeDto, AppError> {
    match value.trim() {
        "System" => Ok(PackageTypeDto::System),
        "UserInstalled" => Ok(PackageTypeDto::UserInstalled),
        _ => Err(AppError::unprocessable(
            "PackageType must be one of System or UserInstalled",
        )),
    }
}

fn normalize_target_systems(value: &str) -> Result<Vec<PackageTargetSystemDto>, AppError> {
    let mut systems = Vec::new();
    for raw_system in value.split(',') {
        let system = match raw_system.trim() {
            "Server" => PackageTargetSystemDto::Server,
            "MBTheater" => PackageTargetSystemDto::MBTheater,
            "MBClassic" => PackageTargetSystemDto::MBClassic,
            "Other" => PackageTargetSystemDto::Other,
            _ => {
                return Err(AppError::unprocessable(
                    "TargetSystems must contain only Server, MBTheater, MBClassic or Other",
                ));
            }
        };
        if !systems.contains(&system) {
            systems.push(system);
        }
    }

    if systems.is_empty() {
        return Err(AppError::unprocessable("TargetSystems must not be empty"));
    }

    Ok(systems)
}

fn normalize_update_class(value: &str) -> Result<PackageVersionClassDto, AppError> {
    match value.trim() {
        "Release" => Ok(PackageVersionClassDto::Release),
        "Beta" => Ok(PackageVersionClassDto::Beta),
        "Dev" => Ok(PackageVersionClassDto::Dev),
        _ => Err(AppError::unprocessable(
            "UpdateClass must be one of Release, Beta or Dev",
        )),
    }
}

fn validate_package_path_value<'a>(field: &str, value: &'a str) -> Result<&'a str, AppError> {
    let value = value.trim();
    if value.is_empty()
        || value.len() > 200
        || value
            .chars()
            .any(|ch| ch.is_control() || matches!(ch, '/' | '\\'))
    {
        return Err(AppError::unprocessable(format!("invalid {field}")));
    }

    Ok(value)
}

fn validate_safe_query_text<'a>(
    field: &str,
    value: &'a str,
    max_len: usize,
) -> Result<&'a str, AppError> {
    let value = value.trim();
    if value.is_empty()
        || value.len() > max_len
        || value
            .chars()
            .any(|ch| ch.is_control() || matches!(ch, '/' | '\\'))
    {
        return Err(AppError::unprocessable(format!("invalid {field}")));
    }

    Ok(value)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn package_info_serializes_camel_case_with_official_enums() {
        let value = serde_json::to_value(fbz_core_package()).unwrap();

        assert_eq!(value["id"], "fbz-core");
        assert_eq!(value["name"], "FBZ Core");
        assert_eq!(value["shortDescription"], "Core FBZ API server package");
        assert_eq!(value["isPremium"], false);
        assert_eq!(value["adult"], false);
        assert_eq!(value["type"], "System");
        assert_eq!(value["targetSystem"], "Server");
        assert_eq!(value["versions"][0]["classification"], "Release");
        assert_eq!(
            value["versions"][0]["versionStr"],
            env!("CARGO_PKG_VERSION")
        );
    }

    #[test]
    fn package_catalog_filters_by_type_target_and_flags() {
        let system_server = normalize_packages_query(PackagesQuery {
            package_type: Some("System".to_owned()),
            target_systems: Some("Server,Other".to_owned()),
            is_premium: Some(false),
            is_adult: Some(false),
        })
        .unwrap();
        let user_installed = normalize_packages_query(PackagesQuery {
            package_type: Some("UserInstalled".to_owned()),
            ..PackagesQuery::default()
        })
        .unwrap();
        let other_target = normalize_packages_query(PackagesQuery {
            target_systems: Some("Other".to_owned()),
            ..PackagesQuery::default()
        })
        .unwrap();

        assert_eq!(package_catalog(&system_server).len(), 1);
        assert!(package_catalog(&user_installed).is_empty());
        assert!(package_catalog(&other_target).is_empty());
    }

    #[test]
    fn package_lookup_matches_name_id_or_guid() {
        assert!(find_package("fbz-core", None).is_ok());
        assert!(find_package("FBZ Core", None).is_ok());
        assert!(find_package("other", None).is_err());
        assert!(find_package("fbz-core", Some("fbz-core")).is_ok());
        assert!(find_package("fbz-core", Some("other-guid")).is_err());
    }

    #[test]
    fn package_query_values_are_allowlisted() {
        assert_eq!(
            normalize_package_type("System").unwrap(),
            PackageTypeDto::System
        );
        assert_eq!(
            normalize_update_class("Beta").unwrap(),
            PackageVersionClassDto::Beta
        );
        assert!(normalize_package_type("Plugin").is_err());
        assert!(normalize_update_class("Nightly").is_err());
        assert!(normalize_target_systems("Server,Other").is_ok());
        assert!(normalize_target_systems("").is_err());
        assert!(normalize_target_systems("Server,Browser").is_err());
    }

    #[test]
    fn package_path_and_query_values_reject_unsafe_text() {
        assert_eq!(
            validate_package_path_value("package name", " FBZ Core ").unwrap(),
            "FBZ Core"
        );
        assert!(validate_package_path_value("package name", "").is_err());
        assert!(validate_package_path_value("package name", "bad/name").is_err());
        assert!(validate_safe_query_text("Version", "1.0.0", 64).is_ok());
        assert!(validate_safe_query_text("Version", "bad\\version", 64).is_err());
    }
}
