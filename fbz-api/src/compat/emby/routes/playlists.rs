use axum::{
    Json,
    extract::{Path, Query, State},
    http::{HeaderMap, Uri},
};
use serde::Deserialize;

use crate::{
    compat::emby::dto::{BaseItemDto, BaseItemSource, QueryResultDto},
    error::AppError,
    library::repository::{
        LibraryRepository, PlaylistItemsInput, PlaylistListInput, PlaylistRecord, SortDirection,
    },
    state::AppState,
};

use super::{access::authenticate_query_user, items::media_item_to_base_item};

const DEFAULT_PLAYLISTS_LIMIT: u32 = 100;
const MAX_PLAYLISTS_LIMIT: u32 = 200;

#[derive(Clone, Debug, Default, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "PascalCase")]
pub struct PlaylistsQuery {
    pub user_id: Option<String>,
    pub parent_id: Option<String>,
    pub start_index: Option<u32>,
    pub limit: Option<u32>,
    pub search_term: Option<String>,
    pub sort_order: Option<String>,
    pub fields: Option<String>,
    pub enable_images: Option<bool>,
}

#[derive(Clone, Debug, Default, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "PascalCase")]
pub struct PlaylistItemsQuery {
    pub user_id: Option<String>,
    pub start_index: Option<u32>,
    pub limit: Option<u32>,
    pub fields: Option<String>,
    pub enable_images: Option<bool>,
    pub image_type_limit: Option<u32>,
    pub enable_image_types: Option<String>,
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

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct PlaylistWindow {
    start_index: i64,
    limit: i64,
}

impl PlaylistWindow {
    fn from_parts(start_index: Option<u32>, limit: Option<u32>) -> Self {
        Self {
            start_index: i64::from(start_index.unwrap_or(0)),
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

#[cfg(test)]
mod tests {
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
}
