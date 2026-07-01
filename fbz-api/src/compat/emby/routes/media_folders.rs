use axum::{
    Json,
    extract::{Query, State},
    http::{HeaderMap, StatusCode, Uri},
    response::{IntoResponse, Response},
};
use serde::Deserialize;
use std::collections::BTreeSet;

use crate::{
    auth::service::AuthenticatedUser,
    compat::emby::dto::{
        BaseItemDto, BaseItemSource, LibraryDefaultOptionsDto, LibraryMediaPathInfoDto,
        LibraryOptionsResultDto, MediaFolderDto, MediaSubFolderDto, QueryResultDto,
        VirtualFolderInfoDto,
    },
    error::AppError,
    library::repository::{LibraryRepository, UserMediaFolderRecord, UserMediaSubFolderRecord},
    scheduler::{
        repository::CORE_INCREMENTAL_SCAN_TASK_KEY,
        service::{SchedulerError, SchedulerService, default_worker_id},
    },
    state::AppState,
};

use super::access::authenticate_request_user;

const MAX_VIRTUAL_FOLDERS_LIMIT: u32 = 200;
const MAX_VIRTUAL_FOLDERS_START_INDEX: u32 = 10_000;

#[derive(Clone, Debug, Default, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "PascalCase")]
pub struct MediaFoldersQuery {
    #[serde(alias = "isHidden", alias = "is_hidden")]
    pub is_hidden: Option<bool>,
}

#[derive(Clone, Debug, Default, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "PascalCase")]
pub struct VirtualFoldersQuery {
    #[serde(alias = "startIndex", alias = "start_index")]
    pub start_index: Option<u32>,
    #[serde(alias = "limit")]
    pub limit: Option<u32>,
}

pub async fn media_folders(
    State(state): State<AppState>,
    Query(query): Query<MediaFoldersQuery>,
    headers: HeaderMap,
    uri: Uri,
) -> Result<Json<QueryResultDto<BaseItemDto>>, AppError> {
    let user = authenticate_request_user(&state, &headers, &uri).await?;
    if query.is_hidden.unwrap_or(false) {
        return Ok(Json(QueryResultDto::new(Vec::new(), 0, 0)));
    }

    let folders = list_media_folders(&state, user.id).await?;
    let items = folders
        .into_iter()
        .map(media_folder_to_base_item)
        .collect::<Vec<_>>();
    let total = items.len() as u32;

    Ok(Json(QueryResultDto::new(items, total, 0)))
}

pub async fn selectable_media_folders(
    State(state): State<AppState>,
    headers: HeaderMap,
    uri: Uri,
) -> Result<Json<Vec<MediaFolderDto>>, AppError> {
    let user = authenticate_request_user(&state, &headers, &uri).await?;
    let folders = list_media_folders(&state, user.id).await?;

    Ok(Json(
        folders
            .into_iter()
            .map(media_folder_to_selectable_folder)
            .collect(),
    ))
}

pub async fn virtual_folders(
    State(state): State<AppState>,
    headers: HeaderMap,
    uri: Uri,
) -> Result<Json<Vec<VirtualFolderInfoDto>>, AppError> {
    let user = authenticate_request_user(&state, &headers, &uri).await?;
    let folders = list_media_folders(&state, user.id).await?;

    Ok(Json(
        folders
            .into_iter()
            .map(media_folder_to_virtual_folder)
            .collect(),
    ))
}

pub async fn virtual_folders_query(
    State(state): State<AppState>,
    Query(query): Query<VirtualFoldersQuery>,
    headers: HeaderMap,
    uri: Uri,
) -> Result<Json<QueryResultDto<VirtualFolderInfoDto>>, AppError> {
    let user = authenticate_request_user(&state, &headers, &uri).await?;
    let folders = list_media_folders(&state, user.id).await?;

    Ok(Json(virtual_folders_query_result(folders, query)))
}

pub async fn physical_paths(
    State(state): State<AppState>,
    headers: HeaderMap,
    uri: Uri,
) -> Result<Json<Vec<String>>, AppError> {
    let user = authenticate_request_user(&state, &headers, &uri).await?;
    ensure_physical_paths_admin(&user)?;
    let Some(database) = state.database() else {
        return Err(AppError::internal("database is not configured"));
    };

    let paths = LibraryRepository::new(database.clone())
        .list_admin_physical_paths()
        .await
        .map_err(|err| AppError::internal(format!("failed to list physical paths: {err}")))?;

    Ok(Json(normalize_physical_paths(paths)))
}

pub async fn available_options(
    State(state): State<AppState>,
    headers: HeaderMap,
    uri: Uri,
) -> Result<Json<LibraryOptionsResultDto>, AppError> {
    authenticate_request_user(&state, &headers, &uri).await?;

    Ok(Json(LibraryOptionsResultDto::fbz_default()))
}

pub async fn refresh_library(
    State(state): State<AppState>,
    headers: HeaderMap,
    uri: Uri,
) -> Result<Response, AppError> {
    let user = authenticate_request_user(&state, &headers, &uri).await?;
    ensure_library_refresh_admin(&user)?;
    let Some(database) = state.database() else {
        return Err(AppError::internal("database is not configured"));
    };

    SchedulerService::with_worker_id(
        database.clone(),
        default_worker_id("emby-library-refresh"),
        state.config().storage.transcode_cache_dir.clone(),
    )
    .run_task_once(library_refresh_task_key())
    .await
    .map_err(library_refresh_error_to_app_error)?;

    Ok((StatusCode::OK, "").into_response())
}

async fn list_media_folders(
    state: &AppState,
    user_id: i64,
) -> Result<Vec<UserMediaFolderRecord>, AppError> {
    let Some(database) = state.database() else {
        return Err(AppError::internal("database is not configured"));
    };

    LibraryRepository::new(database.clone())
        .list_user_media_folders(user_id)
        .await
        .map_err(|err| AppError::internal(format!("failed to list media folders: {err}")))
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct VirtualFoldersWindow {
    start_index: usize,
    limit: usize,
    response_start_index: u32,
}

fn virtual_folders_window(query: VirtualFoldersQuery) -> VirtualFoldersWindow {
    let response_start_index = query
        .start_index
        .unwrap_or_default()
        .min(MAX_VIRTUAL_FOLDERS_START_INDEX);
    let limit = query
        .limit
        .unwrap_or(MAX_VIRTUAL_FOLDERS_LIMIT)
        .clamp(1, MAX_VIRTUAL_FOLDERS_LIMIT);

    VirtualFoldersWindow {
        start_index: response_start_index as usize,
        limit: limit as usize,
        response_start_index,
    }
}

fn virtual_folders_query_result(
    folders: Vec<UserMediaFolderRecord>,
    query: VirtualFoldersQuery,
) -> QueryResultDto<VirtualFolderInfoDto> {
    let total = folders.len() as u32;
    let window = virtual_folders_window(query);
    let items = folders
        .into_iter()
        .skip(window.start_index)
        .take(window.limit)
        .map(media_folder_to_virtual_folder)
        .collect();

    QueryResultDto::new(items, total, window.response_start_index)
}

fn ensure_physical_paths_admin(user: &AuthenticatedUser) -> Result<(), AppError> {
    if user.can_manage_server() {
        return Ok(());
    }

    Err(AppError::forbidden("server management permission required"))
}

fn ensure_library_refresh_admin(user: &AuthenticatedUser) -> Result<(), AppError> {
    if user.can_manage_server() {
        return Ok(());
    }

    Err(AppError::forbidden("server management permission required"))
}

fn library_refresh_task_key() -> &'static str {
    CORE_INCREMENTAL_SCAN_TASK_KEY
}

fn library_refresh_error_to_app_error(error: SchedulerError) -> AppError {
    let message = error.to_string();
    match error {
        SchedulerError::TaskNotFound(_) | SchedulerError::TaskNotRunning(_) => {
            AppError::not_found(message)
        }
        SchedulerError::TaskDisabled(_)
        | SchedulerError::TaskConcurrencyLimit { .. }
        | SchedulerError::InvalidInterval(_)
        | SchedulerError::InvalidCron(_)
        | SchedulerError::UnsupportedScheduleKind(_)
        | SchedulerError::UnsupportedTaskType(_) => AppError::conflict(message),
        SchedulerError::Database(_) => AppError::internal(message),
    }
}

#[cfg(test)]
fn physical_paths_from_folders(folders: Vec<UserMediaFolderRecord>) -> Vec<String> {
    normalize_physical_paths(
        folders
            .into_iter()
            .flat_map(|folder| {
                folder
                    .subfolders
                    .into_iter()
                    .map(|subfolder| subfolder.path)
            })
            .collect(),
    )
}

fn normalize_physical_paths(paths: Vec<String>) -> Vec<String> {
    let mut seen = BTreeSet::new();
    paths
        .into_iter()
        .filter_map(|path| {
            let trimmed = path.trim();
            if trimmed.is_empty() || !seen.insert(trimmed.to_owned()) {
                return None;
            }
            Some(trimmed.to_owned())
        })
        .collect()
}

fn media_folder_to_virtual_folder(record: UserMediaFolderRecord) -> VirtualFolderInfoDto {
    let locations = normalize_physical_paths(
        record
            .subfolders
            .into_iter()
            .map(|subfolder| subfolder.path)
            .collect(),
    );
    // 经单一事实源把存储的 library_type 规范化为 Emby CollectionType；未知值原样保留。
    let collection_type = crate::media_types::LibraryType::parse(&record.library_type)
        .map(|kind| kind.collection_type().to_owned())
        .unwrap_or(record.library_type);
    let mut library_options = LibraryDefaultOptionsDto::for_content_type(&collection_type);
    library_options.path_infos = locations
        .iter()
        .map(|path| LibraryMediaPathInfoDto {
            path: path.clone(),
            network_path: None,
            username: None,
            password: None,
        })
        .collect();

    VirtualFolderInfoDto {
        name: record.name,
        locations,
        collection_type,
        library_options,
        item_id: record.id.clone(),
        id: record.id.clone(),
        guid: record.id,
        primary_image_item_id: None,
        primary_image_tag: None,
        refresh_progress: None,
        refresh_status: "Idle".to_owned(),
    }
}

fn media_folder_to_base_item(record: UserMediaFolderRecord) -> BaseItemDto {
    let mut item = BaseItemDto::from(BaseItemSource {
        id: record.id,
        name: record.name,
        item_type: "CollectionFolder".to_owned(),
        media_type: None,
        parent_id: None,
        is_folder: true,
        run_time_ticks: None,
        production_year: None,
    });
    item.collection_type = Some(record.library_type);
    item
}

fn media_folder_to_selectable_folder(record: UserMediaFolderRecord) -> MediaFolderDto {
    MediaFolderDto {
        guid: record.id.clone(),
        id: record.id,
        name: record.name,
        sub_folders: record
            .subfolders
            .into_iter()
            .map(media_subfolder_to_dto)
            .collect(),
        is_user_access_configurable: true,
    }
}

fn media_subfolder_to_dto(record: UserMediaSubFolderRecord) -> MediaSubFolderDto {
    MediaSubFolderDto {
        name: subfolder_name(&record.path),
        id: record.id,
        path: record.path,
        is_user_access_configurable: true,
    }
}

fn subfolder_name(path: &str) -> String {
    let trimmed = path.trim();
    if trimmed.is_empty() {
        return String::new();
    }

    trimmed
        .trim_end_matches(['/', '\\'])
        .rsplit(['/', '\\'])
        .next()
        .filter(|name| !name.trim().is_empty())
        .unwrap_or(trimmed)
        .to_owned()
}

#[cfg(test)]
mod tests {
    use axum::extract::Query;
    use http::Uri;
    use serde_json::json;

    use super::*;

    #[test]
    fn media_folder_base_item_keeps_collection_type() {
        let item = media_folder_to_base_item(UserMediaFolderRecord {
            id: "library-1".to_owned(),
            name: "Movies".to_owned(),
            library_type: "movies".to_owned(),
            subfolders: Vec::new(),
        });

        let value = serde_json::to_value(item).unwrap();

        assert_eq!(value["Type"], "CollectionFolder");
        assert_eq!(value["CollectionType"], "movies");
        assert_eq!(value["IsFolder"], true);
    }

    #[test]
    fn selectable_media_folder_serializes_emby_shape() {
        let folder = media_folder_to_selectable_folder(UserMediaFolderRecord {
            id: "library-1".to_owned(),
            name: "Movies".to_owned(),
            library_type: "movies".to_owned(),
            subfolders: vec![UserMediaSubFolderRecord {
                id: "7".to_owned(),
                path: "D:/Media/Movies".to_owned(),
            }],
        });

        let value = serde_json::to_value(folder).unwrap();

        assert_eq!(value["Id"], "library-1");
        assert_eq!(value["Guid"], "library-1");
        assert_eq!(value["SubFolders"][0]["Id"], "7");
        assert_eq!(value["SubFolders"][0]["Name"], "Movies");
        assert_eq!(value["SubFolders"][0]["Path"], "D:/Media/Movies");
        assert_eq!(value["IsUserAccessConfigurable"], true);
        assert_eq!(
            value["SubFolders"][0]["IsUserAccessConfigurable"],
            json!(true)
        );
    }

    #[test]
    fn virtual_folder_mapping_serializes_locations_and_library_options() {
        let folder = media_folder_to_virtual_folder(UserMediaFolderRecord {
            id: "library-1".to_owned(),
            name: "Movies".to_owned(),
            library_type: "movies".to_owned(),
            subfolders: vec![UserMediaSubFolderRecord {
                id: "7".to_owned(),
                path: "D:/Media/Movies".to_owned(),
            }],
        });

        let value = serde_json::to_value(folder).unwrap();

        assert_eq!(value["Name"], "Movies");
        assert_eq!(value["Locations"], json!(["D:/Media/Movies"]));
        assert_eq!(value["CollectionType"], "movies");
        assert_eq!(value["ItemId"], "library-1");
        assert_eq!(value["Id"], "library-1");
        assert_eq!(value["Guid"], "library-1");
        assert_eq!(value["LibraryOptions"]["ContentType"], "movies");
        assert_eq!(
            value["LibraryOptions"]["PathInfos"][0]["Path"],
            "D:/Media/Movies"
        );
        assert_eq!(value["RefreshStatus"], "Idle");
    }

    #[test]
    fn virtual_folders_query_window_caps_limit() {
        let window = virtual_folders_window(VirtualFoldersQuery {
            start_index: Some(10),
            limit: Some(500),
        });

        assert_eq!(window.start_index, 10);
        assert_eq!(window.limit, MAX_VIRTUAL_FOLDERS_LIMIT as usize);
        assert_eq!(window.response_start_index, 10);
    }

    #[test]
    fn virtual_folders_query_window_clamps_pathologically_large_start_index() {
        let window = virtual_folders_window(VirtualFoldersQuery {
            start_index: Some(500_000),
            limit: Some(50),
        });

        assert_eq!(window.start_index, 10_000);
        assert_eq!(window.limit, 50);
        assert_eq!(window.response_start_index, 10_000);
    }

    #[test]
    fn media_folder_queries_accept_lower_camel_client_fields() {
        let uri = "/Library/MediaFolders?isHidden=true"
            .parse::<Uri>()
            .unwrap();
        let Query(query) = Query::<MediaFoldersQuery>::try_from_uri(&uri).unwrap();

        assert_eq!(query.is_hidden, Some(true));

        let uri = "/Library/VirtualFolders/Query?startIndex=10&limit=500"
            .parse::<Uri>()
            .unwrap();
        let Query(query) = Query::<VirtualFoldersQuery>::try_from_uri(&uri).unwrap();
        let window = virtual_folders_window(query);

        assert_eq!(window.start_index, 10);
        assert_eq!(window.limit, MAX_VIRTUAL_FOLDERS_LIMIT as usize);
        assert_eq!(window.response_start_index, 10);
    }

    #[test]
    fn virtual_folders_query_result_applies_window() {
        let result = virtual_folders_query_result(
            vec![
                UserMediaFolderRecord {
                    id: "library-1".to_owned(),
                    name: "Movies".to_owned(),
                    library_type: "movies".to_owned(),
                    subfolders: Vec::new(),
                },
                UserMediaFolderRecord {
                    id: "library-2".to_owned(),
                    name: "TV".to_owned(),
                    library_type: "tvshows".to_owned(),
                    subfolders: Vec::new(),
                },
            ],
            VirtualFoldersQuery {
                start_index: Some(1),
                limit: Some(1),
            },
        );

        assert_eq!(result.total_record_count, 2);
        assert_eq!(result.start_index, 1);
        assert_eq!(result.items.len(), 1);
        assert_eq!(result.items[0].name, "TV");
    }

    #[test]
    fn physical_paths_flatten_enabled_subfolders_and_drop_empty_values() {
        let paths = physical_paths_from_folders(vec![
            UserMediaFolderRecord {
                id: "library-1".to_owned(),
                name: "Movies".to_owned(),
                library_type: "movies".to_owned(),
                subfolders: vec![
                    UserMediaSubFolderRecord {
                        id: "1".to_owned(),
                        path: " D:/Media/Movies ".to_owned(),
                    },
                    UserMediaSubFolderRecord {
                        id: "2".to_owned(),
                        path: "D:/Media/Movies".to_owned(),
                    },
                    UserMediaSubFolderRecord {
                        id: "3".to_owned(),
                        path: "   ".to_owned(),
                    },
                ],
            },
            UserMediaFolderRecord {
                id: "library-2".to_owned(),
                name: "TV".to_owned(),
                library_type: "tvshows".to_owned(),
                subfolders: vec![UserMediaSubFolderRecord {
                    id: "4".to_owned(),
                    path: r"\\NAS\TV".to_owned(),
                }],
            },
        ]);

        assert_eq!(paths, ["D:/Media/Movies", r"\\NAS\TV"]);
    }

    #[test]
    fn physical_paths_admin_boundary_requires_server_manager() {
        let admin = crate::auth::service::AuthenticatedUser {
            id: 1,
            public_id: "admin-1".to_owned(),
            username: "admin".to_owned(),
            role_name: "Administrator".to_owned(),
            role_name_normalized: "administrator".to_owned(),
        };
        let user = crate::auth::service::AuthenticatedUser {
            role_name: "User".to_owned(),
            role_name_normalized: "user".to_owned(),
            ..admin.clone()
        };

        assert!(ensure_physical_paths_admin(&admin).is_ok());
        assert!(ensure_physical_paths_admin(&user).is_err());
    }

    #[test]
    fn library_refresh_uses_core_incremental_scan_task() {
        assert_eq!(
            library_refresh_task_key(),
            crate::scheduler::repository::CORE_INCREMENTAL_SCAN_TASK_KEY
        );
    }

    #[test]
    fn library_refresh_admin_boundary_requires_server_manager() {
        let admin = crate::auth::service::AuthenticatedUser {
            id: 1,
            public_id: "admin-1".to_owned(),
            username: "admin".to_owned(),
            role_name: "Administrator".to_owned(),
            role_name_normalized: "administrator".to_owned(),
        };
        let user = crate::auth::service::AuthenticatedUser {
            role_name: "User".to_owned(),
            role_name_normalized: "user".to_owned(),
            ..admin.clone()
        };

        assert!(ensure_library_refresh_admin(&admin).is_ok());
        assert!(ensure_library_refresh_admin(&user).is_err());
    }

    #[test]
    fn subfolder_name_falls_back_to_trimmed_path() {
        assert_eq!(subfolder_name("D:/Media/Movies"), "Movies");
        assert_eq!(subfolder_name(r"D:\Media\TV"), "TV");
        assert_eq!(subfolder_name("NAS"), "NAS");
        assert_eq!(subfolder_name("   "), "");
    }
}
