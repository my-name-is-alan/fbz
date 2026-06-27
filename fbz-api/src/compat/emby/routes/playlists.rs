use axum::{
    Json,
    extract::{Path, Query, State},
    http::{HeaderMap, StatusCode, Uri},
};
use serde::{Deserialize, Serialize};

use crate::{
    compat::emby::dto::{BaseItemDto, BaseItemSource, QueryResultDto},
    error::AppError,
    library::repository::{
        LibraryRepository, PlaylistItemsInput, PlaylistListInput, PlaylistRecord, SortDirection,
    },
    state::AppState,
};

use super::{
    access::{authenticate_query_user, authenticate_request_user},
    items::media_item_to_base_item,
};

const DEFAULT_PLAYLISTS_LIMIT: u32 = 100;
const MAX_PLAYLISTS_LIMIT: u32 = 200;
const MAX_PLAYLIST_START_INDEX: u32 = 10_000;
const MAX_PLAYLIST_IDS: usize = 256;
const MAX_PLAYLIST_ID_LEN: usize = 128;
const MAX_PLAYLIST_NAME_LEN: usize = 256;
const MAX_PLAYLIST_MEDIA_TYPE_LEN: usize = 64;

#[derive(Clone, Debug, Default, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "PascalCase")]
pub struct PlaylistsQuery {
    #[serde(alias = "userId", alias = "user_id")]
    pub user_id: Option<String>,
    #[serde(alias = "parentId", alias = "parent_id")]
    pub parent_id: Option<String>,
    #[serde(alias = "startIndex", alias = "start_index")]
    pub start_index: Option<u32>,
    #[serde(alias = "limit")]
    pub limit: Option<u32>,
    #[serde(alias = "searchTerm", alias = "search_term")]
    pub search_term: Option<String>,
    #[serde(alias = "sortOrder", alias = "sort_order")]
    pub sort_order: Option<String>,
    #[serde(alias = "fields")]
    pub fields: Option<String>,
    #[serde(alias = "enableImages", alias = "enable_images")]
    pub enable_images: Option<bool>,
}

#[derive(Clone, Debug, Default, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "PascalCase")]
pub struct PlaylistItemsQuery {
    #[serde(alias = "userId", alias = "user_id")]
    pub user_id: Option<String>,
    #[serde(alias = "startIndex", alias = "start_index")]
    pub start_index: Option<u32>,
    #[serde(alias = "limit")]
    pub limit: Option<u32>,
    #[serde(alias = "fields")]
    pub fields: Option<String>,
    #[serde(alias = "enableImages", alias = "enable_images")]
    pub enable_images: Option<bool>,
    #[serde(alias = "imageTypeLimit", alias = "image_type_limit")]
    pub image_type_limit: Option<u32>,
    #[serde(alias = "enableImageTypes", alias = "enable_image_types")]
    pub enable_image_types: Option<String>,
}

#[derive(Clone, Debug, Default, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "PascalCase")]
pub struct CreatePlaylistQuery {
    #[serde(alias = "name")]
    pub name: Option<String>,
    #[serde(alias = "ids")]
    pub ids: Option<String>,
    #[serde(alias = "mediaType", alias = "media_type")]
    pub media_type: Option<String>,
}

#[derive(Clone, Debug, Default, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "PascalCase")]
pub struct AddToPlaylistInfoQuery {
    #[serde(alias = "userId", alias = "user_id")]
    pub user_id: Option<String>,
    #[serde(alias = "ids")]
    pub ids: Option<String>,
}

#[derive(Clone, Debug, Default, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "PascalCase")]
pub struct AddPlaylistItemsQuery {
    #[serde(alias = "userId", alias = "user_id")]
    pub user_id: Option<String>,
    #[serde(alias = "ids")]
    pub ids: Option<String>,
}

#[derive(Clone, Debug, Default, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "PascalCase")]
pub struct RemovePlaylistItemsQuery {
    #[serde(alias = "entryIds", alias = "entry_ids")]
    pub entry_ids: Option<String>,
}

#[derive(Clone, Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "PascalCase")]
pub struct PlaylistCreationResultDto {
    pub id: String,
    pub name: String,
    pub item_added_count: i32,
}

#[derive(Clone, Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "PascalCase")]
pub struct AddToPlaylistResultDto {
    pub id: String,
    pub item_added_count: i32,
}

#[derive(Clone, Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "PascalCase")]
pub struct AddToPlaylistInfoDto {
    pub item_count: i32,
    pub contains_duplicates: bool,
}

pub async fn playlists(
    State(state): State<AppState>,
    Query(query): Query<PlaylistsQuery>,
    headers: HeaderMap,
    uri: Uri,
) -> Result<Json<QueryResultDto<BaseItemDto>>, AppError> {
    let user = authenticate_query_user(&state, query.user_id.as_deref(), &headers, &uri).await?;
    let Some(database) = state.database() else {
        return Err(AppError::internal("database is not configured"));
    };

    let window = PlaylistWindow::from_parts(query.start_index, query.limit);
    let _requested_fields = query.fields.as_deref().unwrap_or_default();
    let _enable_images = query.enable_images.unwrap_or(false);
    let result = LibraryRepository::new(database.clone())
        .list_user_playlists(PlaylistListInput {
            user_id: user.id,
            parent_id: normalized_text(query.parent_id),
            start_index: window.start_index,
            limit: window.limit,
            search_term: normalized_text(query.search_term),
            sort_direction: sort_direction_from_query(query.sort_order.as_deref()),
        })
        .await
        .map_err(|err| AppError::internal(format!("failed to list playlists: {err}")))?;

    let items = result
        .items
        .into_iter()
        .map(playlist_to_base_item)
        .collect();
    Ok(Json(QueryResultDto::new(
        items,
        result.total_record_count,
        window.start_index as u32,
    )))
}

pub async fn create_playlist(
    State(state): State<AppState>,
    Query(query): Query<CreatePlaylistQuery>,
    headers: HeaderMap,
    uri: Uri,
) -> Result<Json<PlaylistCreationResultDto>, AppError> {
    authenticate_request_user(&state, &headers, &uri).await?;
    let input = create_playlist_input(query)?;
    let _name = input.name;
    let _ids = input.ids;
    let _media_type = input.media_type;

    Err(playlist_write_disabled_error())
}

pub async fn playlist_items(
    State(state): State<AppState>,
    Path(playlist_id): Path<String>,
    Query(query): Query<PlaylistItemsQuery>,
    headers: HeaderMap,
    uri: Uri,
) -> Result<Json<QueryResultDto<BaseItemDto>>, AppError> {
    let user = authenticate_query_user(&state, query.user_id.as_deref(), &headers, &uri).await?;
    let Some(database) = state.database() else {
        return Err(AppError::internal("database is not configured"));
    };

    let window = PlaylistWindow::from_parts(query.start_index, query.limit);
    let _requested_fields = query.fields.as_deref().unwrap_or_default();
    let _image_type_limit = query.image_type_limit.unwrap_or(1);
    let _enable_image_types = query.enable_image_types.as_deref().unwrap_or_default();
    let result = LibraryRepository::new(database.clone())
        .list_user_playlist_items(PlaylistItemsInput {
            user_id: user.id,
            playlist_id,
            start_index: window.start_index,
            limit: window.limit,
            include_image_tags: query.enable_images.unwrap_or(false),
        })
        .await
        .map_err(|err| AppError::internal(format!("failed to list playlist items: {err}")))?;

    let items = result
        .items
        .into_iter()
        .map(media_item_to_base_item)
        .collect();
    Ok(Json(QueryResultDto::new(
        items,
        result.total_record_count,
        window.start_index as u32,
    )))
}

pub async fn add_to_playlist_info(
    State(state): State<AppState>,
    Path(playlist_id): Path<String>,
    Query(query): Query<AddToPlaylistInfoQuery>,
    headers: HeaderMap,
    uri: Uri,
) -> Result<Json<AddToPlaylistInfoDto>, AppError> {
    authenticate_query_user(&state, query.user_id.as_deref(), &headers, &uri).await?;
    let input = add_to_playlist_input(&playlist_id, query.ids.as_deref())?;

    Ok(Json(AddToPlaylistInfoDto {
        item_count: input.ids.len() as i32,
        contains_duplicates: input.contains_duplicates,
    }))
}

pub async fn add_playlist_items(
    State(state): State<AppState>,
    Path(playlist_id): Path<String>,
    Query(query): Query<AddPlaylistItemsQuery>,
    headers: HeaderMap,
    uri: Uri,
) -> Result<Json<AddToPlaylistResultDto>, AppError> {
    authenticate_query_user(&state, query.user_id.as_deref(), &headers, &uri).await?;
    let input = add_to_playlist_input(&playlist_id, query.ids.as_deref())?;
    let _playlist_id = input.playlist_id;
    let _ids = input.ids;

    Err(playlist_write_disabled_error())
}

pub async fn remove_playlist_items(
    State(state): State<AppState>,
    Path(playlist_id): Path<String>,
    Query(query): Query<RemovePlaylistItemsQuery>,
    headers: HeaderMap,
    uri: Uri,
) -> Result<StatusCode, AppError> {
    authenticate_request_user(&state, &headers, &uri).await?;
    let input = remove_playlist_items_input(&playlist_id, query.entry_ids.as_deref())?;
    let _playlist_id = input.playlist_id;
    let _entry_ids = input.ids;

    Err(playlist_write_disabled_error())
}

pub async fn move_playlist_item(
    State(state): State<AppState>,
    Path((playlist_id, item_id, new_index)): Path<(String, String, String)>,
    headers: HeaderMap,
    uri: Uri,
) -> Result<StatusCode, AppError> {
    authenticate_request_user(&state, &headers, &uri).await?;
    let input = move_playlist_item_input(&playlist_id, &item_id, &new_index)?;
    let _playlist_id = input.playlist_id;
    let _item_id = input.item_id;
    let _new_index = input.new_index;

    Err(playlist_write_disabled_error())
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct PlaylistWindow {
    start_index: i64,
    limit: i64,
}

impl PlaylistWindow {
    fn from_parts(start_index: Option<u32>, limit: Option<u32>) -> Self {
        Self {
            start_index: i64::from(start_index.unwrap_or(0).min(MAX_PLAYLIST_START_INDEX)),
            limit: i64::from(
                limit
                    .unwrap_or(DEFAULT_PLAYLISTS_LIMIT)
                    .clamp(1, MAX_PLAYLISTS_LIMIT),
            ),
        }
    }
}

pub(super) fn playlist_to_base_item(record: PlaylistRecord) -> BaseItemDto {
    let mut item = BaseItemDto::from(BaseItemSource {
        id: record.id,
        name: record.name,
        item_type: "Playlist".to_owned(),
        media_type: None,
        parent_id: None,
        is_folder: true,
        run_time_ticks: None,
        production_year: None,
    });
    item.collection_type = Some("playlists".to_owned());
    item
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct CreatePlaylistInput {
    name: String,
    ids: Vec<String>,
    media_type: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct PlaylistIdsInput {
    playlist_id: String,
    ids: Vec<String>,
    contains_duplicates: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct MovePlaylistItemInput {
    playlist_id: String,
    item_id: String,
    new_index: u32,
}

fn create_playlist_input(query: CreatePlaylistQuery) -> Result<CreatePlaylistInput, AppError> {
    Ok(CreatePlaylistInput {
        name: normalize_required_playlist_name(query.name.as_deref())?,
        ids: normalize_playlist_ids(query.ids.as_deref(), false)?.ids,
        media_type: normalize_optional_media_type(query.media_type.as_deref())?,
    })
}

fn add_to_playlist_input(
    playlist_id: &str,
    ids: Option<&str>,
) -> Result<PlaylistIdsInput, AppError> {
    Ok(PlaylistIdsInput {
        playlist_id: normalize_playlist_id(playlist_id, "Id")?,
        ..normalize_playlist_ids(ids, true)?
    })
}

fn remove_playlist_items_input(
    playlist_id: &str,
    entry_ids: Option<&str>,
) -> Result<PlaylistIdsInput, AppError> {
    Ok(PlaylistIdsInput {
        playlist_id: normalize_playlist_id(playlist_id, "Id")?,
        ..normalize_playlist_ids(entry_ids, true)?
    })
}

fn move_playlist_item_input(
    playlist_id: &str,
    item_id: &str,
    new_index: &str,
) -> Result<MovePlaylistItemInput, AppError> {
    Ok(MovePlaylistItemInput {
        playlist_id: normalize_playlist_id(playlist_id, "Id")?,
        item_id: normalize_playlist_id(item_id, "ItemId")?,
        new_index: new_index
            .parse::<u32>()
            .map_err(|_| AppError::unprocessable("NewIndex is invalid"))?,
    })
}

fn normalized_text(value: Option<String>) -> Option<String> {
    value.and_then(|value| {
        let trimmed = value.trim();
        (!trimmed.is_empty()).then(|| trimmed.to_owned())
    })
}

fn sort_direction_from_query(value: Option<&str>) -> SortDirection {
    match value.map(str::trim) {
        Some(value) if value.eq_ignore_ascii_case("Descending") => SortDirection::Desc,
        Some(value) if value.eq_ignore_ascii_case("Desc") => SortDirection::Desc,
        _ => SortDirection::Asc,
    }
}

fn normalize_playlist_ids(
    value: Option<&str>,
    required: bool,
) -> Result<PlaylistIdsInput, AppError> {
    let raw = value.unwrap_or_default();
    let mut ids = Vec::new();
    let mut contains_duplicates = false;

    for part in raw.split(',') {
        let value = part.trim();
        if value.is_empty() {
            continue;
        }
        let value = normalize_playlist_id(value, "Ids")?;
        if ids.iter().any(|existing| existing == &value) {
            contains_duplicates = true;
            continue;
        }
        ids.push(value);
        if ids.len() > MAX_PLAYLIST_IDS {
            return Err(AppError::unprocessable("too many playlist item ids"));
        }
    }

    if required && ids.is_empty() {
        return Err(AppError::unprocessable("Ids are required"));
    }

    Ok(PlaylistIdsInput {
        playlist_id: String::new(),
        ids,
        contains_duplicates,
    })
}

fn normalize_playlist_id(value: &str, field: &'static str) -> Result<String, AppError> {
    let value = value.trim();
    if value.is_empty() {
        return Err(AppError::unprocessable(format!("{field} is required")));
    }
    if value.len() > MAX_PLAYLIST_ID_LEN
        || !value
            .chars()
            .all(|ch| ch.is_ascii_alphanumeric() || matches!(ch, '-' | '_' | '.'))
    {
        return Err(AppError::unprocessable(format!("{field} is invalid")));
    }

    Ok(value.to_owned())
}

fn normalize_required_playlist_name(value: Option<&str>) -> Result<String, AppError> {
    let Some(value) = value.map(str::trim).filter(|value| !value.is_empty()) else {
        return Err(AppError::unprocessable("Name is required"));
    };
    if value.len() > MAX_PLAYLIST_NAME_LEN || value.chars().any(char::is_control) {
        return Err(AppError::unprocessable("Name is invalid"));
    }

    Ok(value.to_owned())
}

fn normalize_optional_media_type(value: Option<&str>) -> Result<Option<String>, AppError> {
    let Some(value) = value.map(str::trim).filter(|value| !value.is_empty()) else {
        return Ok(None);
    };
    if value.len() > MAX_PLAYLIST_MEDIA_TYPE_LEN
        || !value
            .chars()
            .all(|ch| ch.is_ascii_alphanumeric() || matches!(ch, '-' | '_'))
    {
        return Err(AppError::unprocessable("MediaType is invalid"));
    }

    Ok(Some(value.to_owned()))
}

fn playlist_write_disabled_error() -> AppError {
    AppError::conflict("Emby playlist writes are disabled; use FBZ collection management APIs")
}

#[cfg(test)]
mod tests {
    use axum::extract::Query;
    use http::Uri;
    use serde_json::json;

    use super::*;

    #[test]
    fn playlists_query_accepts_common_emby_parameters() {
        let query = serde_json::from_value::<PlaylistsQuery>(json!({
            "UserId": "user-1",
            "ParentId": "library-1",
            "StartIndex": 10,
            "Limit": 500,
            "SearchTerm": "mix",
            "SortOrder": "Descending",
            "Fields": "PrimaryImageAspectRatio",
            "EnableImages": true
        }))
        .unwrap();

        let window = PlaylistWindow::from_parts(query.start_index, query.limit);
        assert_eq!(query.user_id.as_deref(), Some("user-1"));
        assert_eq!(query.parent_id.as_deref(), Some("library-1"));
        assert_eq!(window.start_index, 10);
        assert_eq!(window.limit, i64::from(MAX_PLAYLISTS_LIMIT));
        assert_eq!(query.search_term.as_deref(), Some("mix"));
        assert_eq!(
            sort_direction_from_query(query.sort_order.as_deref()),
            SortDirection::Desc
        );
    }

    #[test]
    fn playlist_queries_accept_lower_camel_client_fields() {
        let uri = "/Playlists?userId=user-1&parentId=library-1&startIndex=10&limit=500&searchTerm=mix&sortOrder=Descending&fields=PrimaryImageAspectRatio&enableImages=true"
            .parse::<Uri>()
            .unwrap();
        let Query(query) = Query::<PlaylistsQuery>::try_from_uri(&uri).unwrap();

        let window = PlaylistWindow::from_parts(query.start_index, query.limit);
        assert_eq!(query.user_id.as_deref(), Some("user-1"));
        assert_eq!(query.parent_id.as_deref(), Some("library-1"));
        assert_eq!(window.start_index, 10);
        assert_eq!(window.limit, i64::from(MAX_PLAYLISTS_LIMIT));
        assert_eq!(query.search_term.as_deref(), Some("mix"));
        assert_eq!(query.fields.as_deref(), Some("PrimaryImageAspectRatio"));
        assert_eq!(query.enable_images, Some(true));
        assert_eq!(
            sort_direction_from_query(query.sort_order.as_deref()),
            SortDirection::Desc
        );

        let uri = "/Playlists/playlist-1/Items?userId=user-1&startIndex=10&limit=500&fields=MediaSources&enableImages=true&imageTypeLimit=2&enableImageTypes=Primary,Backdrop"
            .parse::<Uri>()
            .unwrap();
        let Query(query) = Query::<PlaylistItemsQuery>::try_from_uri(&uri).unwrap();
        let window = PlaylistWindow::from_parts(query.start_index, query.limit);

        assert_eq!(query.user_id.as_deref(), Some("user-1"));
        assert_eq!(window.start_index, 10);
        assert_eq!(window.limit, i64::from(MAX_PLAYLISTS_LIMIT));
        assert_eq!(query.fields.as_deref(), Some("MediaSources"));
        assert_eq!(query.enable_images, Some(true));
        assert_eq!(query.image_type_limit, Some(2));
        assert_eq!(
            query.enable_image_types.as_deref(),
            Some("Primary,Backdrop")
        );
    }

    #[test]
    fn playlist_window_clamps_pathologically_large_start_index() {
        let window = PlaylistWindow::from_parts(Some(500_000), Some(50));

        assert_eq!(window.start_index, 10_000);
        assert_eq!(window.limit, 50);
    }

    #[test]
    fn playlist_mapping_uses_emby_playlist_shape() {
        let item = playlist_to_base_item(PlaylistRecord {
            id: "playlist-1".to_owned(),
            name: "Favorites".to_owned(),
            total_record_count: 1,
        });

        assert_eq!(item.item_type, "Playlist");
        assert!(item.is_folder);
        assert_eq!(item.collection_type.as_deref(), Some("playlists"));
        assert!(item.media_type.is_none());
    }

    #[test]
    fn playlist_write_dtos_serialize_with_official_pascal_case() {
        assert_eq!(
            serde_json::to_value(PlaylistCreationResultDto {
                id: "playlist-1".to_owned(),
                name: "Road Trip".to_owned(),
                item_added_count: 2,
            })
            .unwrap(),
            json!({
                "Id": "playlist-1",
                "Name": "Road Trip",
                "ItemAddedCount": 2
            })
        );
        assert_eq!(
            serde_json::to_value(AddToPlaylistResultDto {
                id: "playlist-1".to_owned(),
                item_added_count: 2,
            })
            .unwrap(),
            json!({
                "Id": "playlist-1",
                "ItemAddedCount": 2
            })
        );
        assert_eq!(
            serde_json::to_value(AddToPlaylistInfoDto {
                item_count: 2,
                contains_duplicates: true,
            })
            .unwrap(),
            json!({
                "ItemCount": 2,
                "ContainsDuplicates": true
            })
        );
    }

    #[test]
    fn playlist_write_inputs_normalize_official_query_parameters() {
        let create = create_playlist_input(CreatePlaylistQuery {
            name: Some(" Road Trip ".to_owned()),
            ids: Some(" item-1,item-2,item-1 ".to_owned()),
            media_type: Some(" Audio ".to_owned()),
        })
        .unwrap();

        assert_eq!(create.name, "Road Trip");
        assert_eq!(create.ids, ["item-1", "item-2"]);
        assert_eq!(create.media_type.as_deref(), Some("Audio"));

        let add = add_to_playlist_input(" playlist-1 ", Some("item-1,item-2,item-1")).unwrap();

        assert_eq!(add.playlist_id, "playlist-1");
        assert_eq!(add.ids, ["item-1", "item-2"]);
        assert!(add.contains_duplicates);

        let moved = move_playlist_item_input("playlist-1", "entry-1", "3").unwrap();

        assert_eq!(moved.new_index, 3);
    }

    #[test]
    fn playlist_write_queries_accept_lower_camel_client_fields() {
        let uri = "/Playlists?name=Road%20Trip&ids=item-1,item-2,item-1&mediaType=Audio"
            .parse::<Uri>()
            .unwrap();
        let Query(query) = Query::<CreatePlaylistQuery>::try_from_uri(&uri).unwrap();
        let create = create_playlist_input(query).unwrap();

        assert_eq!(create.name, "Road Trip");
        assert_eq!(create.ids, ["item-1", "item-2"]);
        assert_eq!(create.media_type.as_deref(), Some("Audio"));

        let uri = "/Playlists/playlist-1/AddToPlaylistInfo?userId=user-1&ids=item-1,item-2,item-1"
            .parse::<Uri>()
            .unwrap();
        let Query(query) = Query::<AddToPlaylistInfoQuery>::try_from_uri(&uri).unwrap();
        assert_eq!(query.user_id.as_deref(), Some("user-1"));
        let add = add_to_playlist_input("playlist-1", query.ids.as_deref()).unwrap();
        assert_eq!(add.ids, ["item-1", "item-2"]);
        assert!(add.contains_duplicates);

        let uri = "/Playlists/playlist-1/Items?userId=user-1&ids=item-1,item-2"
            .parse::<Uri>()
            .unwrap();
        let Query(query) = Query::<AddPlaylistItemsQuery>::try_from_uri(&uri).unwrap();
        assert_eq!(query.user_id.as_deref(), Some("user-1"));
        assert_eq!(query.ids.as_deref(), Some("item-1,item-2"));

        let uri = "/Playlists/playlist-1/Items?entryIds=entry-1,entry-2"
            .parse::<Uri>()
            .unwrap();
        let Query(query) = Query::<RemovePlaylistItemsQuery>::try_from_uri(&uri).unwrap();
        let remove = remove_playlist_items_input("playlist-1", query.entry_ids.as_deref()).unwrap();
        assert_eq!(remove.ids, ["entry-1", "entry-2"]);
    }

    #[test]
    fn playlist_write_inputs_reject_missing_or_unsafe_values() {
        assert_eq!(
            create_playlist_input(CreatePlaylistQuery {
                name: Some(" ".to_owned()),
                ids: None,
                media_type: None,
            })
            .unwrap_err()
            .status_code(),
            StatusCode::UNPROCESSABLE_ENTITY
        );
        assert_eq!(
            add_to_playlist_input("playlist-1", None)
                .unwrap_err()
                .status_code(),
            StatusCode::UNPROCESSABLE_ENTITY
        );
        assert_eq!(
            remove_playlist_items_input("playlist-1", Some("bad/id"))
                .unwrap_err()
                .status_code(),
            StatusCode::UNPROCESSABLE_ENTITY
        );
        assert_eq!(
            move_playlist_item_input("playlist-1", "entry-1", "-1")
                .unwrap_err()
                .status_code(),
            StatusCode::UNPROCESSABLE_ENTITY
        );
        assert_eq!(
            create_playlist_input(CreatePlaylistQuery {
                name: Some("Road Trip".to_owned()),
                ids: None,
                media_type: Some("Audio/Video".to_owned()),
            })
            .unwrap_err()
            .status_code(),
            StatusCode::UNPROCESSABLE_ENTITY
        );
    }
}
