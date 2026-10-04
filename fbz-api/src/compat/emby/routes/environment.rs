use std::{
    env, fs,
    path::{Path, PathBuf},
};

use axum::{
    Json,
    body::Bytes,
    extract::{Query, State},
    http::{HeaderMap, StatusCode, Uri},
};
use serde::{Deserialize, Serialize};

use crate::{compat::emby::payload::parse_emby_body, error::AppError, state::AppState};

use super::access::authenticate_request_user;

const MAX_ENVIRONMENT_PATH_LEN: usize = 4096;
const MAX_ENVIRONMENT_ENTRIES: usize = 1000;
const MAX_NETWORK_PATH_LEN: usize = 512;

#[derive(Clone, Debug, Default, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "PascalCase")]
pub struct DirectoryContentsQuery {
    #[serde(alias = "path")]
    pub path: Option<String>,
    #[serde(alias = "includeFiles", alias = "include_files")]
    pub include_files: Option<bool>,
    #[serde(alias = "includeDirectories", alias = "include_directories")]
    pub include_directories: Option<bool>,
}

#[derive(Clone, Debug, Default, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "PascalCase")]
pub struct PathQuery {
    #[serde(alias = "path")]
    pub path: Option<String>,
}

#[derive(Clone, Debug, Default, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "PascalCase")]
pub struct DirectoryCredentialsDto {
    #[serde(alias = "username")]
    pub username: Option<String>,
    #[serde(alias = "password")]
    pub password: Option<String>,
}

#[derive(Clone, Debug, Default, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "PascalCase")]
pub struct ValidatePathDto {
    #[serde(alias = "validateWriteable", alias = "validate_writeable")]
    pub validate_writeable: Option<bool>,
    #[serde(alias = "isFile", alias = "is_file")]
    pub is_file: Option<bool>,
    #[serde(alias = "username")]
    pub username: Option<String>,
    #[serde(alias = "password")]
    pub password: Option<String>,
}

#[derive(Clone, Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "PascalCase")]
pub struct DefaultDirectoryBrowserInfoDto {
    pub path: String,
}

#[derive(Clone, Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "PascalCase")]
pub struct FileSystemEntryInfoDto {
    pub name: String,
    pub path: String,
    #[serde(rename = "Type")]
    pub entry_type: FileSystemEntryTypeDto,
}

#[derive(Clone, Copy, Debug, Serialize, PartialEq, Eq)]
#[allow(dead_code)]
pub enum FileSystemEntryTypeDto {
    File,
    Directory,
    NetworkComputer,
    NetworkShare,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct DirectoryListingOptions {
    include_files: bool,
    include_directories: bool,
}

pub async fn default_directory_browser(
    State(state): State<AppState>,
    headers: HeaderMap,
    uri: Uri,
) -> Result<Json<DefaultDirectoryBrowserInfoDto>, AppError> {
    authenticate_admin_compatible(&state, &headers, &uri).await?;

    Ok(Json(DefaultDirectoryBrowserInfoDto {
        path: current_directory_string()?,
    }))
}

pub async fn drives(
    State(state): State<AppState>,
    headers: HeaderMap,
    uri: Uri,
) -> Result<Json<Vec<FileSystemEntryInfoDto>>, AppError> {
    authenticate_admin_compatible(&state, &headers, &uri).await?;

    Ok(Json(drive_entries()))
}

pub async fn directory_contents(
    State(state): State<AppState>,
    Query(query): Query<DirectoryContentsQuery>,
    headers: HeaderMap,
    uri: Uri,
) -> Result<Json<Vec<FileSystemEntryInfoDto>>, AppError> {
    authenticate_admin_compatible(&state, &headers, &uri).await?;
    let input = directory_listing_input(&query)?;

    Ok(Json(read_directory_entries(&input.path, input.options)?))
}

pub async fn post_directory_contents(
    State(state): State<AppState>,
    Query(query): Query<DirectoryContentsQuery>,
    headers: HeaderMap,
    uri: Uri,
    body: Bytes,
) -> Result<Json<Vec<FileSystemEntryInfoDto>>, AppError> {
    authenticate_admin_compatible(&state, &headers, &uri).await?;
    let _credentials = parse_optional_emby_body::<DirectoryCredentialsDto>(&headers, &body)?;
    let input = directory_listing_input(&query)?;

    Ok(Json(read_directory_entries(&input.path, input.options)?))
}

pub async fn parent_path(
    State(state): State<AppState>,
    Query(query): Query<PathQuery>,
    headers: HeaderMap,
    uri: Uri,
) -> Result<Json<String>, AppError> {
    authenticate_admin_compatible(&state, &headers, &uri).await?;
    let path = normalize_existing_path(query.path.as_deref(), "Path")?;

    Ok(Json(parent_path_string(&path)))
}

pub async fn network_devices(
    State(state): State<AppState>,
    headers: HeaderMap,
    uri: Uri,
) -> Result<Json<Vec<FileSystemEntryInfoDto>>, AppError> {
    authenticate_admin_compatible(&state, &headers, &uri).await?;

    Ok(Json(Vec::new()))
}

pub async fn network_shares(
    State(state): State<AppState>,
    Query(query): Query<PathQuery>,
    headers: HeaderMap,
    uri: Uri,
) -> Result<Json<Vec<FileSystemEntryInfoDto>>, AppError> {
    authenticate_admin_compatible(&state, &headers, &uri).await?;
    let _path = normalize_network_path(query.path.as_deref())?;

    Ok(Json(Vec::new()))
}

pub async fn validate_path(
    State(state): State<AppState>,
    Query(query): Query<PathQuery>,
    headers: HeaderMap,
    uri: Uri,
    body: Bytes,
) -> Result<StatusCode, AppError> {
    authenticate_admin_compatible(&state, &headers, &uri).await?;
    let request = parse_optional_emby_body::<ValidatePathDto>(&headers, &body)?;
    validate_environment_path(query.path.as_deref(), &request)?;

    Ok(StatusCode::OK)
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

#[derive(Clone, Debug, PartialEq, Eq)]
struct DirectoryListingInput {
    path: PathBuf,
    options: DirectoryListingOptions,
}

fn directory_listing_input(
    query: &DirectoryContentsQuery,
) -> Result<DirectoryListingInput, AppError> {
    Ok(DirectoryListingInput {
        path: normalize_existing_directory(query.path.as_deref(), "Path")?,
        options: DirectoryListingOptions {
            include_files: query.include_files.unwrap_or(true),
            include_directories: query.include_directories.unwrap_or(true),
        },
    })
}

fn read_directory_entries(
    path: &Path,
    options: DirectoryListingOptions,
) -> Result<Vec<FileSystemEntryInfoDto>, AppError> {
    let mut entries = Vec::new();
    let read_dir = fs::read_dir(path)
        .map_err(|err| AppError::not_found(format!("directory cannot be read: {err}")))?;

    for entry in read_dir {
        let Ok(entry) = entry else {
            continue;
        };
        let Ok(file_type) = entry.file_type() else {
            continue;
        };
        let entry_type = if file_type.is_dir() {
            if !options.include_directories {
                continue;
            }
            FileSystemEntryTypeDto::Directory
        } else if file_type.is_file() {
            if !options.include_files {
                continue;
            }
            FileSystemEntryTypeDto::File
        } else {
            continue;
        };

        entries.push(FileSystemEntryInfoDto {
            name: entry.file_name().to_string_lossy().into_owned(),
            path: path_to_string(entry.path()),
            entry_type,
        });

        if entries.len() >= MAX_ENVIRONMENT_ENTRIES {
            break;
        }
    }

    entries.sort_by(|left, right| {
        let left_rank = match left.entry_type {
            FileSystemEntryTypeDto::Directory => 0,
            FileSystemEntryTypeDto::File => 1,
            FileSystemEntryTypeDto::NetworkComputer => 2,
            FileSystemEntryTypeDto::NetworkShare => 3,
        };
        let right_rank = match right.entry_type {
            FileSystemEntryTypeDto::Directory => 0,
            FileSystemEntryTypeDto::File => 1,
            FileSystemEntryTypeDto::NetworkComputer => 2,
            FileSystemEntryTypeDto::NetworkShare => 3,
        };
        left_rank
            .cmp(&right_rank)
            .then_with(|| left.name.to_lowercase().cmp(&right.name.to_lowercase()))
    });

    Ok(entries)
}

fn drive_entries() -> Vec<FileSystemEntryInfoDto> {
    let mut entries = platform_drive_paths()
        .into_iter()
        .map(|path| FileSystemEntryInfoDto {
            name: path_to_string(&path),
            path: path_to_string(path),
            entry_type: FileSystemEntryTypeDto::Directory,
        })
        .collect::<Vec<_>>();
    entries.sort_by(|left, right| left.name.cmp(&right.name));
    entries
}

#[cfg(windows)]
fn platform_drive_paths() -> Vec<PathBuf> {
    ('A'..='Z')
        .map(|drive| PathBuf::from(format!("{drive}:\\")))
        .filter(|path| path.exists())
        .collect()
}

#[cfg(not(windows))]
fn platform_drive_paths() -> Vec<PathBuf> {
    vec![PathBuf::from("/")]
}

fn validate_environment_path(
    path: Option<&str>,
    request: &ValidatePathDto,
) -> Result<(), AppError> {
    let path = normalize_existing_path(path, "Path")?;
    let metadata = fs::metadata(&path)
        .map_err(|err| AppError::not_found(format!("path cannot be inspected: {err}")))?;
    if request.is_file.unwrap_or(false) && !metadata.is_file() {
        return Err(AppError::unprocessable("path must be a file"));
    }
    if !request.is_file.unwrap_or(false) && !metadata.is_dir() {
        return Err(AppError::unprocessable("path must be a directory"));
    }
    if request.validate_writeable.unwrap_or(false) && metadata.permissions().readonly() {
        return Err(AppError::forbidden("path is not writeable"));
    }

    Ok(())
}

fn normalize_existing_directory(
    value: Option<&str>,
    field: &'static str,
) -> Result<PathBuf, AppError> {
    let path = normalize_existing_path(value, field)?;
    if !path.is_dir() {
        return Err(AppError::unprocessable(format!(
            "{field} must be a directory"
        )));
    }

    Ok(path)
}

fn normalize_existing_path(value: Option<&str>, field: &'static str) -> Result<PathBuf, AppError> {
    let Some(value) = value.map(str::trim).filter(|value| !value.is_empty()) else {
        return Err(AppError::unprocessable(format!("{field} is required")));
    };
    if value.len() > MAX_ENVIRONMENT_PATH_LEN || value.chars().any(|ch| ch == '\0') {
        return Err(AppError::unprocessable(format!("{field} is invalid")));
    }

    let path = PathBuf::from(value);
    let path = if path.is_absolute() {
        path
    } else {
        env::current_dir()
            .map_err(|err| AppError::internal(format!("current directory cannot be read: {err}")))?
            .join(path)
    };

    fs::canonicalize(&path)
        .map_err(|err| AppError::not_found(format!("{field} does not exist: {err}")))
}

fn normalize_network_path(value: Option<&str>) -> Result<String, AppError> {
    let Some(value) = value.map(str::trim).filter(|value| !value.is_empty()) else {
        return Err(AppError::unprocessable("Path is required"));
    };
    if value.len() > MAX_NETWORK_PATH_LEN || value.chars().any(|ch| ch.is_control()) {
        return Err(AppError::unprocessable("Path is invalid"));
    }

    Ok(value.to_owned())
}

fn parent_path_string(path: &Path) -> String {
    path.parent().map(path_to_string).unwrap_or_default()
}

fn current_directory_string() -> Result<String, AppError> {
    env::current_dir()
        .map(path_to_string)
        .map_err(|err| AppError::internal(format!("current directory cannot be read: {err}")))
}

fn path_to_string(path: impl AsRef<Path>) -> String {
    strip_verbatim_prefix(path.as_ref().to_string_lossy().as_ref())
}

/// 去掉 Windows 扩展长度路径前缀（verbatim prefix）。`fs::canonicalize` 在 Windows 上会返回
/// `\\?\C:\Media` 或 `\\?\UNC\server\share` 这类前缀路径，直接回给前端会显示成"乱码"般的
/// `\\?\`。这里把它还原成用户熟悉的 `C:\Media` / `\\server\share`。非 Windows 或无前缀路径原样返回。
fn strip_verbatim_prefix(path: &str) -> String {
    if let Some(rest) = path.strip_prefix(r"\\?\UNC\") {
        format!(r"\\{rest}")
    } else if let Some(rest) = path.strip_prefix(r"\\?\") {
        rest.to_owned()
    } else {
        path.to_owned()
    }
}

fn parse_optional_emby_body<T>(headers: &HeaderMap, body: &Bytes) -> Result<T, AppError>
where
    T: serde::de::DeserializeOwned + Default,
{
    if body.is_empty() {
        return Ok(T::default());
    }

    parse_emby_body(headers, body)
}

#[cfg(test)]
mod tests {
    use axum::extract::Query;
    use axum::http::StatusCode;
    use http::Uri;
    use serde_json::json;

    use super::*;

    #[test]
    fn strip_verbatim_prefix_normalizes_windows_extended_paths() {
        assert_eq!(strip_verbatim_prefix(r"\\?\C:\Media"), r"C:\Media");
        assert_eq!(
            strip_verbatim_prefix(r"\\?\UNC\server\share"),
            r"\\server\share"
        );
        // 无前缀 / 非 Windows 路径原样返回。
        assert_eq!(strip_verbatim_prefix(r"C:\Media"), r"C:\Media");
        assert_eq!(strip_verbatim_prefix("/mnt/media"), "/mnt/media");
    }

    #[test]
    fn file_system_entry_serializes_pascal_case_with_official_enum() {
        let value = serde_json::to_value(FileSystemEntryInfoDto {
            name: "media".to_owned(),
            path: "/mnt/media".to_owned(),
            entry_type: FileSystemEntryTypeDto::Directory,
        })
        .unwrap();

        assert_eq!(value["Name"], "media");
        assert_eq!(value["Path"], "/mnt/media");
        assert_eq!(value["Type"], "Directory");
    }

    #[test]
    fn directory_query_defaults_to_files_and_directories() {
        let query = DirectoryContentsQuery {
            path: Some(".".to_owned()),
            include_files: None,
            include_directories: None,
        };

        let input = directory_listing_input(&query).unwrap();

        assert!(input.options.include_files);
        assert!(input.options.include_directories);
        assert!(input.path.is_absolute());
    }

    #[test]
    fn environment_queries_accept_lower_camel_client_fields() {
        let uri =
            "/Environment/DirectoryContents?path=.&includeFiles=false&includeDirectories=true"
                .parse::<Uri>()
                .unwrap();
        let Query(query) = Query::<DirectoryContentsQuery>::try_from_uri(&uri).unwrap();
        assert_eq!(query.path.as_deref(), Some("."));
        assert_eq!(query.include_files, Some(false));
        assert_eq!(query.include_directories, Some(true));

        let uri = "/Environment/ParentPath?path=.".parse::<Uri>().unwrap();
        let Query(query) = Query::<PathQuery>::try_from_uri(&uri).unwrap();
        assert_eq!(query.path.as_deref(), Some("."));
    }

    #[test]
    fn environment_bodies_accept_lower_camel_client_fields() {
        let credentials = serde_json::from_value::<DirectoryCredentialsDto>(json!({
            "username": "admin",
            "password": "secret"
        }))
        .unwrap();
        assert_eq!(credentials.username.as_deref(), Some("admin"));
        assert_eq!(credentials.password.as_deref(), Some("secret"));

        let request = serde_json::from_value::<ValidatePathDto>(json!({
            "validateWriteable": true,
            "isFile": false,
            "username": "admin",
            "password": "secret"
        }))
        .unwrap();
        assert_eq!(request.validate_writeable, Some(true));
        assert_eq!(request.is_file, Some(false));
        assert_eq!(request.username.as_deref(), Some("admin"));
        assert_eq!(request.password.as_deref(), Some("secret"));
    }

    #[test]
    fn path_normalization_rejects_missing_or_unsafe_paths() {
        assert_eq!(
            normalize_existing_path(None, "Path")
                .unwrap_err()
                .status_code(),
            StatusCode::UNPROCESSABLE_ENTITY
        );
        assert_eq!(
            normalize_existing_path(Some("bad\0path"), "Path")
                .unwrap_err()
                .status_code(),
            StatusCode::UNPROCESSABLE_ENTITY
        );
        assert_eq!(
            normalize_existing_path(Some("definitely-missing-fbz-path"), "Path")
                .unwrap_err()
                .status_code(),
            StatusCode::NOT_FOUND
        );
    }

    #[test]
    fn validate_path_distinguishes_files_and_directories() {
        let err = validate_environment_path(
            Some("."),
            &ValidatePathDto {
                is_file: Some(true),
                validate_writeable: Some(false),
                ..ValidatePathDto::default()
            },
        )
        .unwrap_err();

        assert_eq!(err.status_code(), StatusCode::UNPROCESSABLE_ENTITY);
        assert!(
            validate_environment_path(
                Some("."),
                &ValidatePathDto {
                    is_file: Some(false),
                    validate_writeable: Some(false),
                    ..ValidatePathDto::default()
                },
            )
            .is_ok()
        );
    }

    #[test]
    fn network_path_requires_bounded_safe_text() {
        assert_eq!(normalize_network_path(Some(" server ")).unwrap(), "server");
        assert_eq!(
            normalize_network_path(Some("bad\nserver"))
                .unwrap_err()
                .status_code(),
            StatusCode::UNPROCESSABLE_ENTITY
        );
    }
}
