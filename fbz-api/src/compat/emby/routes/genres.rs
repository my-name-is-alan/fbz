use axum::{
    Json,
    extract::{Path, Query, State},
    http::{HeaderMap, Uri},
};
use serde::Deserialize;

use crate::{
    compat::emby::dto::{BaseItemDto, BaseItemSource, QueryResultDto},
    error::AppError,
    library::repository::{GenreListInput, GenreRecord, LibraryRepository, SortDirection},
    state::AppState,
};

use super::{access::authenticate_query_user, items::normalized_parent_id};

const DEFAULT_GENRES_LIMIT: u32 = 100;
const MAX_GENRES_LIMIT: u32 = 200;
const MAX_GENRES_START_INDEX: u32 = 10_000;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum GenreKind {
    General,
    Music,
}

impl GenreKind {
    fn emby_type(self) -> &'static str {
        match self {
            Self::General => "Genre",
            Self::Music => "MusicGenre",
        }
    }

    fn music_only(self) -> bool {
        matches!(self, Self::Music)
    }
}

#[derive(Clone, Debug, Default, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "PascalCase")]
pub struct GenresQuery {
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
pub struct GenreByNameQuery {
    #[serde(alias = "userId", alias = "user_id")]
    pub user_id: Option<String>,
}

pub async fn genres(
    State(state): State<AppState>,
    Query(query): Query<GenresQuery>,
    headers: HeaderMap,
    uri: Uri,
) -> Result<Json<QueryResultDto<BaseItemDto>>, AppError> {
    list_genres(state, query, headers, uri, GenreKind::General).await
}

pub async fn music_genres(
    State(state): State<AppState>,
    Query(query): Query<GenresQuery>,
    headers: HeaderMap,
    uri: Uri,
) -> Result<Json<QueryResultDto<BaseItemDto>>, AppError> {
    list_genres(state, query, headers, uri, GenreKind::Music).await
}

pub async fn genre_by_name(
    State(state): State<AppState>,
    Path(name): Path<String>,
    Query(query): Query<GenreByNameQuery>,
    headers: HeaderMap,
    uri: Uri,
) -> Result<Json<BaseItemDto>, AppError> {
    find_genre_by_name(state, name, query, headers, uri, GenreKind::General).await
}

pub async fn music_genre_by_name(
    State(state): State<AppState>,
    Path(name): Path<String>,
    Query(query): Query<GenreByNameQuery>,
    headers: HeaderMap,
    uri: Uri,
) -> Result<Json<BaseItemDto>, AppError> {
    find_genre_by_name(state, name, query, headers, uri, GenreKind::Music).await
}

async fn list_genres(
    state: AppState,
    query: GenresQuery,
    headers: HeaderMap,
    uri: Uri,
    kind: GenreKind,
) -> Result<Json<QueryResultDto<BaseItemDto>>, AppError> {
    let user = authenticate_query_user(&state, query.user_id.as_deref(), &headers, &uri).await?;
    let Some(database) = state.database() else {
        return Err(AppError::internal("database is not configured"));
    };

    let window = GenreWindow::from_query(&query);
    let _requested_fields = query.fields.as_deref().unwrap_or_default();
    let _enable_images = query.enable_images.unwrap_or(false);
    let result = LibraryRepository::new(database.clone())
        .list_user_genres(GenreListInput {
            user_id: user.id,
            parent_id: normalized_parent_id(query.parent_id),
            start_index: window.start_index,
            limit: window.limit,
            recursive: query.recursive.unwrap_or(true),
            music_only: kind.music_only(),
            search_term: normalized_text_filter(query.search_term),
            name_starts_with: normalized_text_filter(query.name_starts_with),
            name_starts_with_or_greater: normalized_text_filter(query.name_starts_with_or_greater),
            sort_direction: sort_direction_from_query(query.sort_order.as_deref()),
        })
        .await
        .map_err(|err| AppError::internal(format!("failed to list genres: {err}")))?;

    let items = result
        .items
        .into_iter()
        .map(|record| genre_to_base_item(record, kind))
        .collect();

    Ok(Json(QueryResultDto::new(
        items,
        result.total_record_count,
        window.start_index as u32,
    )))
}

async fn find_genre_by_name(
    state: AppState,
    name: String,
    query: GenreByNameQuery,
    headers: HeaderMap,
    uri: Uri,
    kind: GenreKind,
) -> Result<Json<BaseItemDto>, AppError> {
    let user = authenticate_query_user(&state, query.user_id.as_deref(), &headers, &uri).await?;
    let Some(database) = state.database() else {
        return Err(AppError::internal("database is not configured"));
    };

    let Some(record) = LibraryRepository::new(database.clone())
        .find_user_genre_by_name(user.id, &name, kind.music_only())
        .await
        .map_err(|err| AppError::internal(format!("failed to get genre: {err}")))?
    else {
        return Err(AppError::not_found("genre not found"));
    };

    Ok(Json(genre_to_base_item(record, kind)))
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct GenreWindow {
    start_index: i64,
    limit: i64,
}

impl GenreWindow {
    fn from_query(query: &GenresQuery) -> Self {
        Self {
            start_index: i64::from(query.start_index.unwrap_or(0).min(MAX_GENRES_START_INDEX)),
            limit: i64::from(
                query
                    .limit
                    .unwrap_or(DEFAULT_GENRES_LIMIT)
                    .clamp(1, MAX_GENRES_LIMIT),
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

fn genre_to_base_item(record: GenreRecord, kind: GenreKind) -> BaseItemDto {
    BaseItemDto::from(BaseItemSource {
        id: record.id,
        name: record.name,
        item_type: kind.emby_type().to_owned(),
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
    fn genres_query_accepts_common_emby_parameters() {
        let query = serde_json::from_value::<GenresQuery>(json!({
            "UserId": "user-1",
            "ParentId": "library-1",
            "Recursive": true,
            "StartIndex": 10,
            "Limit": 500,
            "SearchTerm": "roc",
            "SortOrder": "Descending",
            "NameStartsWith": "R",
            "NameStartsWithOrGreater": "Q",
            "Fields": "PrimaryImageAspectRatio",
            "EnableImages": true
        }))
        .unwrap();

        let window = GenreWindow::from_query(&query);
        assert_eq!(query.user_id.as_deref(), Some("user-1"));
        assert_eq!(query.parent_id.as_deref(), Some("library-1"));
        assert_eq!(query.recursive, Some(true));
        assert_eq!(window.start_index, 10);
        assert_eq!(window.limit, i64::from(MAX_GENRES_LIMIT));
        assert_eq!(query.search_term.as_deref(), Some("roc"));
        assert_eq!(
            sort_direction_from_query(query.sort_order.as_deref()),
            SortDirection::Desc
        );
    }

    #[test]
    fn genre_queries_accept_lower_camel_client_fields() {
        let uri = "/Genres?userId=user-1&parentId=library-1&recursive=true&startIndex=10&limit=500&searchTerm=roc&sortOrder=Descending&nameStartsWith=R&nameStartsWithOrGreater=Q&fields=PrimaryImageAspectRatio&enableImages=true"
            .parse::<Uri>()
            .unwrap();
        let Query(query) = Query::<GenresQuery>::try_from_uri(&uri).unwrap();

        let window = GenreWindow::from_query(&query);
        assert_eq!(query.user_id.as_deref(), Some("user-1"));
        assert_eq!(query.parent_id.as_deref(), Some("library-1"));
        assert_eq!(query.recursive, Some(true));
        assert_eq!(window.start_index, 10);
        assert_eq!(window.limit, i64::from(MAX_GENRES_LIMIT));
        assert_eq!(query.search_term.as_deref(), Some("roc"));
        assert_eq!(query.name_starts_with.as_deref(), Some("R"));
        assert_eq!(query.name_starts_with_or_greater.as_deref(), Some("Q"));
        assert_eq!(query.fields.as_deref(), Some("PrimaryImageAspectRatio"));
        assert_eq!(query.enable_images, Some(true));
        assert_eq!(
            sort_direction_from_query(query.sort_order.as_deref()),
            SortDirection::Desc
        );

        let uri = "/Genres/Action?userId=user-1".parse::<Uri>().unwrap();
        let Query(query) = Query::<GenreByNameQuery>::try_from_uri(&uri).unwrap();
        assert_eq!(query.user_id.as_deref(), Some("user-1"));
    }

    #[test]
    fn genre_mapping_uses_emby_item_types() {
        let general = genre_to_base_item(
            GenreRecord {
                id: "1".to_owned(),
                name: "Action".to_owned(),
                total_record_count: 1,
            },
            GenreKind::General,
        );
        let music = genre_to_base_item(
            GenreRecord {
                id: "2".to_owned(),
                name: "Rock".to_owned(),
                total_record_count: 1,
            },
            GenreKind::Music,
        );

        assert_eq!(general.item_type, "Genre");
        assert_eq!(music.item_type, "MusicGenre");
        assert!(general.is_folder);
        assert!(music.is_folder);
    }

    #[test]
    fn genre_window_clamps_pathologically_large_start_index() {
        let window = GenreWindow::from_query(&GenresQuery {
            start_index: Some(500_000),
            limit: Some(50),
            ..GenresQuery::default()
        });

        assert_eq!(window.start_index, 10_000);
        assert_eq!(window.limit, 50);
    }
}
