use axum::{
    Json,
    extract::{Path, Query, State},
    http::{HeaderMap, Uri},
};
use serde::Deserialize;

use crate::{
    compat::emby::dto::{BaseItemDto, BaseItemSource, QueryResultDto},
    error::AppError,
    library::repository::{LibraryRepository, SortDirection, StudioListInput, StudioRecord},
    state::AppState,
};

use super::{access::authenticate_query_user, items::normalized_parent_id};

const DEFAULT_STUDIOS_LIMIT: u32 = 100;
const MAX_STUDIOS_LIMIT: u32 = 200;
const MAX_STUDIOS_START_INDEX: u32 = 10_000;

#[derive(Clone, Debug, Default, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "PascalCase")]
pub struct StudiosQuery {
    #[serde(alias = "userId", alias = "user_id")]
    pub user_id: Option<String>,
    #[serde(alias = "parentId", alias = "parent_id")]
    pub parent_id: Option<String>,
    #[serde(alias = "recursive")]
    pub recursive: Option<bool>,
    #[serde(alias = "startIndex", alias = "start_index")]
    pub start_index: Option<u32>,
    #[serde(alias = "limit")]
    pub limit: Option<u32>,
    #[serde(alias = "searchTerm", alias = "search_term")]
    pub search_term: Option<String>,
    #[serde(alias = "sortOrder", alias = "sort_order")]
    pub sort_order: Option<String>,
    #[serde(alias = "nameStartsWith", alias = "name_starts_with")]
    pub name_starts_with: Option<String>,
    #[serde(
        alias = "nameStartsWithOrGreater",
        alias = "name_starts_with_or_greater"
    )]
    pub name_starts_with_or_greater: Option<String>,
    #[serde(alias = "fields")]
    pub fields: Option<String>,
    #[serde(alias = "enableImages", alias = "enable_images")]
    pub enable_images: Option<bool>,
}

#[derive(Clone, Debug, Default, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "PascalCase")]
pub struct StudioByNameQuery {
    #[serde(alias = "userId", alias = "user_id")]
    pub user_id: Option<String>,
}

pub async fn studios(
    State(state): State<AppState>,
    Query(query): Query<StudiosQuery>,
    headers: HeaderMap,
    uri: Uri,
) -> Result<Json<QueryResultDto<BaseItemDto>>, AppError> {
    let user = authenticate_query_user(&state, query.user_id.as_deref(), &headers, &uri).await?;
    let Some(database) = state.database() else {
        return Err(AppError::internal("database is not configured"));
    };

    let window = StudioWindow::from_query(&query);
    let _requested_fields = query.fields.as_deref().unwrap_or_default();
    let _enable_images = query.enable_images.unwrap_or(false);
    let result = LibraryRepository::new(database.clone())
        .list_user_studios(StudioListInput {
            user_id: user.id,
            parent_id: normalized_parent_id(query.parent_id),
            start_index: window.start_index,
            limit: window.limit,
            recursive: query.recursive.unwrap_or(true),
            search_term: normalized_text_filter(query.search_term),
            name_starts_with: normalized_text_filter(query.name_starts_with),
            name_starts_with_or_greater: normalized_text_filter(query.name_starts_with_or_greater),
            sort_direction: sort_direction_from_query(query.sort_order.as_deref()),
        })
        .await
        .map_err(|err| AppError::internal(format!("failed to list studios: {err}")))?;

    let items = result.items.into_iter().map(studio_to_base_item).collect();
    Ok(Json(QueryResultDto::new(
        items,
        result.total_record_count,
        window.start_index as u32,
    )))
}

pub async fn studio_by_name(
    State(state): State<AppState>,
    Path(name): Path<String>,
    Query(query): Query<StudioByNameQuery>,
    headers: HeaderMap,
    uri: Uri,
) -> Result<Json<BaseItemDto>, AppError> {
    let user = authenticate_query_user(&state, query.user_id.as_deref(), &headers, &uri).await?;
    let Some(database) = state.database() else {
        return Err(AppError::internal("database is not configured"));
    };

    let Some(record) = LibraryRepository::new(database.clone())
        .find_user_studio_by_name(user.id, &name)
        .await
        .map_err(|err| AppError::internal(format!("failed to get studio: {err}")))?
    else {
        return Err(AppError::not_found("studio not found"));
    };

    Ok(Json(studio_to_base_item(record)))
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct StudioWindow {
    start_index: i64,
    limit: i64,
}

impl StudioWindow {
    fn from_query(query: &StudiosQuery) -> Self {
        Self {
            start_index: i64::from(query.start_index.unwrap_or(0).min(MAX_STUDIOS_START_INDEX)),
            limit: i64::from(
                query
                    .limit
                    .unwrap_or(DEFAULT_STUDIOS_LIMIT)
                    .clamp(1, MAX_STUDIOS_LIMIT),
            ),
        }
    }
}

fn normalized_text_filter(value: Option<String>) -> Option<String> {
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

fn studio_to_base_item(record: StudioRecord) -> BaseItemDto {
    BaseItemDto::from(BaseItemSource {
        id: record.id,
        name: record.name,
        item_type: "Studio".to_owned(),
        media_type: None,
        parent_id: None,
        is_folder: true,
        run_time_ticks: None,
        production_year: None,
    })
}

#[cfg(test)]
mod tests {
    use axum::extract::Query;
    use http::Uri;
    use serde_json::json;

    use super::*;

    #[test]
    fn studios_query_accepts_common_emby_parameters() {
        let query = serde_json::from_value::<StudiosQuery>(json!({
            "UserId": "user-1",
            "ParentId": "library-1",
            "Recursive": true,
            "StartIndex": 10,
            "Limit": 500,
            "SearchTerm": "studio",
            "SortOrder": "Descending",
            "NameStartsWith": "S",
            "NameStartsWithOrGreater": "M",
            "Fields": "PrimaryImageAspectRatio",
            "EnableImages": true
        }))
        .unwrap();

        let window = StudioWindow::from_query(&query);
        assert_eq!(query.user_id.as_deref(), Some("user-1"));
        assert_eq!(query.parent_id.as_deref(), Some("library-1"));
        assert_eq!(query.recursive, Some(true));
        assert_eq!(window.start_index, 10);
        assert_eq!(window.limit, i64::from(MAX_STUDIOS_LIMIT));
        assert_eq!(query.search_term.as_deref(), Some("studio"));
        assert_eq!(
            sort_direction_from_query(query.sort_order.as_deref()),
            SortDirection::Desc
        );
    }

    #[test]
    fn studio_queries_accept_lower_camel_client_fields() {
        let uri = "/Studios?userId=user-1&parentId=library-1&recursive=true&startIndex=10&limit=500&searchTerm=studio&sortOrder=Descending&nameStartsWith=S&nameStartsWithOrGreater=M&fields=PrimaryImageAspectRatio&enableImages=true"
            .parse::<Uri>()
            .unwrap();
        let Query(query) = Query::<StudiosQuery>::try_from_uri(&uri).unwrap();

        let window = StudioWindow::from_query(&query);
        assert_eq!(query.user_id.as_deref(), Some("user-1"));
        assert_eq!(query.parent_id.as_deref(), Some("library-1"));
        assert_eq!(query.recursive, Some(true));
        assert_eq!(window.start_index, 10);
        assert_eq!(window.limit, i64::from(MAX_STUDIOS_LIMIT));
        assert_eq!(query.search_term.as_deref(), Some("studio"));
        assert_eq!(query.name_starts_with.as_deref(), Some("S"));
        assert_eq!(query.name_starts_with_or_greater.as_deref(), Some("M"));
        assert_eq!(query.fields.as_deref(), Some("PrimaryImageAspectRatio"));
        assert_eq!(query.enable_images, Some(true));
        assert_eq!(
            sort_direction_from_query(query.sort_order.as_deref()),
            SortDirection::Desc
        );

        let uri = "/Studios/Studio%20A?userId=user-1".parse::<Uri>().unwrap();
        let Query(query) = Query::<StudioByNameQuery>::try_from_uri(&uri).unwrap();
        assert_eq!(query.user_id.as_deref(), Some("user-1"));
    }

    #[test]
    fn studio_mapping_uses_studio_shape() {
        let item = studio_to_base_item(StudioRecord {
            id: "studio-1".to_owned(),
            name: "Studio A".to_owned(),
            total_record_count: 1,
        });

        assert_eq!(item.item_type, "Studio");
        assert_eq!(item.media_type, None);
        assert!(item.is_folder);
    }

    #[test]
    fn studio_window_clamps_pathologically_large_start_index() {
        let window = StudioWindow::from_query(&StudiosQuery {
            start_index: Some(500_000),
            limit: Some(50),
            ..StudiosQuery::default()
        });

        assert_eq!(window.start_index, 10_000);
        assert_eq!(window.limit, 50);
    }
}
