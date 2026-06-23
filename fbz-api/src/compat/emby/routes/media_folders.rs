use axum::{
    Json,
    extract::{Query, State},
    http::{HeaderMap, Uri},
};
use serde::Deserialize;

use crate::{
    compat::emby::dto::{
        BaseItemDto, BaseItemSource, MediaFolderDto, MediaSubFolderDto, QueryResultDto,
    },
    error::AppError,
    library::repository::{LibraryRepository, UserMediaFolderRecord, UserMediaSubFolderRecord},
    state::AppState,
};

use super::access::authenticate_request_user;

#[derive(Clone, Debug, Default, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "PascalCase")]
pub struct MediaFoldersQuery {
    pub is_hidden: Option<bool>,
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
    fn subfolder_name_falls_back_to_trimmed_path() {
        assert_eq!(subfolder_name("D:/Media/Movies"), "Movies");
        assert_eq!(subfolder_name(r"D:\Media\TV"), "TV");
        assert_eq!(subfolder_name("NAS"), "NAS");
        assert_eq!(subfolder_name("   "), "");
    }
}
