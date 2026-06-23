use axum::{
    Json,
    extract::{Query, State},
    http::{HeaderMap, Uri},
};
use serde::Deserialize;

use crate::{
    compat::emby::dto::NameValuePairDto,
    error::AppError,
    library::repository::{ItemPrefixesInput, ItemTypeFilter, LibraryRepository},
    state::AppState,
};

use super::{
    access::authenticate_query_user,
    items::{include_item_types_filter, normalized_parent_id, normalized_text_filter},
};

const DEFAULT_PREFIXES_LIMIT: u32 = 1_000;
const MAX_PREFIXES_LIMIT: u32 = 2_000;

#[derive(Clone, Debug, Default, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "PascalCase")]
pub struct PrefixesQuery {
    pub user_id: Option<String>,
    pub parent_id: Option<String>,
    pub recursive: Option<bool>,
    pub start_index: Option<u32>,
    pub limit: Option<u32>,
    pub include_item_types: Option<String>,
    pub search_term: Option<String>,
    pub name_starts_with: Option<String>,
    pub name_starts_with_or_greater: Option<String>,
    pub name_less_than: Option<String>,
    pub artist_type: Option<String>,
}

pub async fn item_prefixes(
    State(state): State<AppState>,
    Query(query): Query<PrefixesQuery>,
    headers: HeaderMap,
    uri: Uri,
) -> Result<Json<Vec<NameValuePairDto>>, AppError> {
    prefixes(state, query, headers, uri, PrefixKind::Items).await
}

pub async fn artist_prefixes(
    State(state): State<AppState>,
    Query(query): Query<PrefixesQuery>,
    headers: HeaderMap,
    uri: Uri,
) -> Result<Json<Vec<NameValuePairDto>>, AppError> {
    prefixes(state, query, headers, uri, PrefixKind::Artists).await
}

async fn prefixes(
    state: AppState,
    query: PrefixesQuery,
    headers: HeaderMap,
    uri: Uri,
    kind: PrefixKind,
) -> Result<Json<Vec<NameValuePairDto>>, AppError> {
    let user = authenticate_query_user(&state, query.user_id.as_deref(), &headers, &uri).await?;
    let Some(database) = state.database() else {
        return Err(AppError::internal("database is not configured"));
    };

    let window = PrefixWindow::from_query(&query);
    let result = LibraryRepository::new(database.clone())
        .list_user_item_prefixes(ItemPrefixesInput {
            user_id: user.id,
            parent_id: normalized_parent_id(query.parent_id),
            start_index: window.start_index,
            limit: window.limit,
            recursive: query.recursive.unwrap_or(true),
            type_filter: kind.type_filter(query.include_item_types.as_deref()),
            search_term: normalized_text_filter(query.search_term.as_deref()),
            name_starts_with: normalized_text_filter(query.name_starts_with.as_deref()),
            name_starts_with_or_greater: normalized_text_filter(
                query.name_starts_with_or_greater.as_deref(),
            ),
            name_less_than: normalized_text_filter(query.name_less_than.as_deref()),
        })
        .await
        .map_err(|err| AppError::internal(format!("failed to list item prefixes: {err}")))?;

    Ok(Json(
        result
            .into_iter()
            .map(|record| NameValuePairDto {
                name: record.name,
                value: record.value,
            })
            .collect(),
    ))
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum PrefixKind {
    Items,
    Artists,
}

impl PrefixKind {
    fn type_filter(self, include_item_types: Option<&str>) -> ItemTypeFilter {
        match self {
            Self::Items => include_item_types_filter(include_item_types),
            Self::Artists => ItemTypeFilter::enabled(vec!["artist".to_owned()]),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct PrefixWindow {
    start_index: i64,
    limit: i64,
}

impl PrefixWindow {
    fn from_query(query: &PrefixesQuery) -> Self {
        Self {
            start_index: i64::from(query.start_index.unwrap_or_default()),
            limit: i64::from(
                query
                    .limit
                    .unwrap_or(DEFAULT_PREFIXES_LIMIT)
                    .clamp(1, MAX_PREFIXES_LIMIT),
            ),
        }
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    #[test]
    fn prefixes_query_accepts_common_emby_parameters() {
        let query = serde_json::from_value::<PrefixesQuery>(json!({
            "UserId": "user-1",
            "ParentId": "library-1",
            "Recursive": true,
            "StartIndex": 10,
            "Limit": 5000,
            "IncludeItemTypes": "MusicAlbum,Audio",
            "SearchTerm": "bow",
            "NameStartsWith": "B",
            "NameStartsWithOrGreater": "A",
            "NameLessThan": "Z",
            "ArtistType": "AlbumArtist"
        }))
        .unwrap();

        let window = PrefixWindow::from_query(&query);
        assert_eq!(query.user_id.as_deref(), Some("user-1"));
        assert_eq!(query.parent_id.as_deref(), Some("library-1"));
        assert_eq!(query.recursive, Some(true));
        assert_eq!(window.start_index, 10);
        assert_eq!(window.limit, i64::from(MAX_PREFIXES_LIMIT));
        assert_eq!(
            query.include_item_types.as_deref(),
            Some("MusicAlbum,Audio")
        );
        assert_eq!(query.search_term.as_deref(), Some("bow"));
        assert_eq!(query.name_starts_with.as_deref(), Some("B"));
        assert_eq!(query.name_starts_with_or_greater.as_deref(), Some("A"));
        assert_eq!(query.name_less_than.as_deref(), Some("Z"));
        assert_eq!(query.artist_type.as_deref(), Some("AlbumArtist"));
    }

    #[test]
    fn artist_prefixes_force_artist_type_filter() {
        let filter = PrefixKind::Artists.type_filter(Some("Audio"));

        assert!(filter.enabled);
        assert_eq!(filter.item_types, ["artist"]);
    }
}
