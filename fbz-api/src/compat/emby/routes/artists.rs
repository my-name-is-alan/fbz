use axum::{
    Json,
    extract::{Path, Query, State},
    http::{HeaderMap, Uri},
};
use serde::Deserialize;

use crate::{
    compat::emby::dto::{BaseItemDto, BaseItemSource, QueryResultDto},
    error::AppError,
    library::repository::{ArtistListInput, ArtistRecord, LibraryRepository, SortDirection},
    state::AppState,
};

use super::{
    access::authenticate_query_user,
    items::{id_list_filter, normalized_parent_id, pipe_name_list_filter},
};

const DEFAULT_ARTISTS_LIMIT: u32 = 100;
const MAX_ARTISTS_LIMIT: u32 = 200;
const MAX_ARTISTS_START_INDEX: u32 = 10_000;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ArtistKind {
    Artists,
    AlbumArtists,
}

#[derive(Clone, Debug, Default, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "PascalCase")]
pub struct ArtistsQuery {
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
    #[serde(alias = "artistType", alias = "artist_type")]
    pub artist_type: Option<String>,
    #[serde(alias = "nameStartsWith", alias = "name_starts_with")]
    pub name_starts_with: Option<String>,
    #[serde(
        alias = "nameStartsWithOrGreater",
        alias = "name_starts_with_or_greater"
    )]
    pub name_starts_with_or_greater: Option<String>,
    #[serde(
        alias = "artistStartsWithOrGreater",
        alias = "artist_starts_with_or_greater"
    )]
    pub artist_starts_with_or_greater: Option<String>,
    #[serde(
        alias = "albumArtistStartsWithOrGreater",
        alias = "album_artist_starts_with_or_greater"
    )]
    pub album_artist_starts_with_or_greater: Option<String>,
    #[serde(alias = "albums")]
    pub albums: Option<String>,
    #[serde(alias = "albumIds", alias = "album_ids")]
    pub album_ids: Option<String>,
    #[serde(alias = "fields")]
    pub fields: Option<String>,
    #[serde(alias = "enableImages", alias = "enable_images")]
    pub enable_images: Option<bool>,
}

#[derive(Clone, Debug, Default, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "PascalCase")]
pub struct ArtistByNameQuery {
    #[serde(alias = "userId", alias = "user_id")]
    pub user_id: Option<String>,
}

pub async fn artists(
    State(state): State<AppState>,
    Query(query): Query<ArtistsQuery>,
    headers: HeaderMap,
    uri: Uri,
) -> Result<Json<QueryResultDto<BaseItemDto>>, AppError> {
    list_artists(state, query, headers, uri, ArtistKind::Artists).await
}

pub async fn album_artists(
    State(state): State<AppState>,
    Query(query): Query<ArtistsQuery>,
    headers: HeaderMap,
    uri: Uri,
) -> Result<Json<QueryResultDto<BaseItemDto>>, AppError> {
    list_artists(state, query, headers, uri, ArtistKind::AlbumArtists).await
}

pub async fn artist_by_name(
    State(state): State<AppState>,
    Path(name): Path<String>,
    Query(query): Query<ArtistByNameQuery>,
    headers: HeaderMap,
    uri: Uri,
) -> Result<Json<BaseItemDto>, AppError> {
    let user = authenticate_query_user(&state, query.user_id.as_deref(), &headers, &uri).await?;
    let Some(database) = state.database() else {
        return Err(AppError::internal("database is not configured"));
    };

    let Some(record) = LibraryRepository::new(database.clone())
        .find_user_artist_by_name(user.id, &name, false)
        .await
        .map_err(|err| AppError::internal(format!("failed to get artist: {err}")))?
    else {
        return Err(AppError::not_found("artist not found"));
    };

    Ok(Json(artist_to_base_item(record)))
}

async fn list_artists(
    state: AppState,
    query: ArtistsQuery,
    headers: HeaderMap,
    uri: Uri,
    kind: ArtistKind,
) -> Result<Json<QueryResultDto<BaseItemDto>>, AppError> {
    let user = authenticate_query_user(&state, query.user_id.as_deref(), &headers, &uri).await?;
    let Some(database) = state.database() else {
        return Err(AppError::internal("database is not configured"));
    };

    let window = ArtistWindow::from_query(&query);
    let album_artists_only =
        kind == ArtistKind::AlbumArtists || is_album_artist_query(query.artist_type.as_deref());
    let name_starts_with_or_greater = normalized_text_filter(artist_prefix_filter(&query));
    let _requested_fields = query.fields.as_deref().unwrap_or_default();
    let _enable_images = query.enable_images.unwrap_or(false);
    let result = LibraryRepository::new(database.clone())
        .list_user_artists(ArtistListInput {
            user_id: user.id,
            parent_id: normalized_parent_id(query.parent_id),
            start_index: window.start_index,
            limit: window.limit,
            recursive: query.recursive.unwrap_or(true),
            album_artists_only,
            search_term: normalized_text_filter(query.search_term),
            name_starts_with: normalized_text_filter(query.name_starts_with),
            name_starts_with_or_greater,
            album_names: pipe_name_list_filter(query.albums.as_deref()),
            album_ids: id_list_filter(query.album_ids.as_deref()),
            sort_direction: sort_direction_from_query(query.sort_order.as_deref()),
        })
        .await
        .map_err(|err| AppError::internal(format!("failed to list artists: {err}")))?;

    let items = result.items.into_iter().map(artist_to_base_item).collect();
    Ok(Json(QueryResultDto::new(
        items,
        result.total_record_count,
        window.start_index as u32,
    )))
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct ArtistWindow {
    start_index: i64,
    limit: i64,
}

impl ArtistWindow {
    fn from_query(query: &ArtistsQuery) -> Self {
        Self {
            start_index: i64::from(query.start_index.unwrap_or(0).min(MAX_ARTISTS_START_INDEX)),
            limit: i64::from(
                query
                    .limit
                    .unwrap_or(DEFAULT_ARTISTS_LIMIT)
                    .clamp(1, MAX_ARTISTS_LIMIT),
            ),
        }
    }
}

fn artist_prefix_filter(query: &ArtistsQuery) -> Option<String> {
    query
        .name_starts_with_or_greater
        .clone()
        .or_else(|| query.artist_starts_with_or_greater.clone())
        .or_else(|| query.album_artist_starts_with_or_greater.clone())
}

fn normalized_text_filter(value: Option<String>) -> Option<String> {
    value.and_then(|value| {
        let trimmed = value.trim();
        (!trimmed.is_empty()).then(|| trimmed.to_owned())
    })
}

fn is_album_artist_query(value: Option<&str>) -> bool {
    value
        .map(str::trim)
        .is_some_and(|value| value.eq_ignore_ascii_case("AlbumArtist"))
}

fn sort_direction_from_query(value: Option<&str>) -> SortDirection {
    match value.map(str::trim) {
        Some(value) if value.eq_ignore_ascii_case("Descending") => SortDirection::Desc,
        Some(value) if value.eq_ignore_ascii_case("Desc") => SortDirection::Desc,
        _ => SortDirection::Asc,
    }
}

fn artist_to_base_item(record: ArtistRecord) -> BaseItemDto {
    BaseItemDto::from(BaseItemSource {
        id: record.id,
        name: record.name,
        item_type: "MusicArtist".to_owned(),
        media_type: Some("Audio".to_owned()),
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
    fn artists_query_accepts_common_emby_parameters() {
        let query = serde_json::from_value::<ArtistsQuery>(json!({
            "UserId": "user-1",
            "ParentId": "library-1",
            "Recursive": true,
            "StartIndex": 10,
            "Limit": 500,
            "SearchTerm": "bow",
            "SortOrder": "Descending",
            "ArtistType": "AlbumArtist",
            "NameStartsWith": "B",
            "ArtistStartsWithOrGreater": "A",
            "AlbumArtistStartsWithOrGreater": "C",
            "Albums": "Low|Heroes",
            "AlbumIds": "bbbbbbbb-0000-0000-0000-000000000001|invalid",
            "Fields": "PrimaryImageAspectRatio",
            "EnableImages": true
        }))
        .unwrap();

        let window = ArtistWindow::from_query(&query);
        assert_eq!(query.user_id.as_deref(), Some("user-1"));
        assert_eq!(query.parent_id.as_deref(), Some("library-1"));
        assert_eq!(query.recursive, Some(true));
        assert_eq!(window.start_index, 10);
        assert_eq!(window.limit, i64::from(MAX_ARTISTS_LIMIT));
        assert_eq!(query.search_term.as_deref(), Some("bow"));
        assert!(is_album_artist_query(query.artist_type.as_deref()));
        assert_eq!(artist_prefix_filter(&query).as_deref(), Some("A"),);
        assert_eq!(query.albums.as_deref(), Some("Low|Heroes"));
        assert_eq!(
            query.album_ids.as_deref(),
            Some("bbbbbbbb-0000-0000-0000-000000000001|invalid")
        );
        assert_eq!(
            sort_direction_from_query(query.sort_order.as_deref()),
            SortDirection::Desc
        );
    }

    #[test]
    fn artist_queries_accept_lower_camel_client_fields() {
        let uri = "/Artists?userId=user-1&parentId=library-1&recursive=true&startIndex=10&limit=500&searchTerm=bow&sortOrder=Descending&artistType=AlbumArtist&nameStartsWith=B&artistStartsWithOrGreater=A&albumArtistStartsWithOrGreater=C&albums=Low%7CHeroes&albumIds=bbbbbbbb-0000-0000-0000-000000000001%7Cinvalid&fields=PrimaryImageAspectRatio&enableImages=true"
            .parse::<Uri>()
            .unwrap();
        let Query(query) = Query::<ArtistsQuery>::try_from_uri(&uri).unwrap();

        let window = ArtistWindow::from_query(&query);
        assert_eq!(query.user_id.as_deref(), Some("user-1"));
        assert_eq!(query.parent_id.as_deref(), Some("library-1"));
        assert_eq!(query.recursive, Some(true));
        assert_eq!(window.start_index, 10);
        assert_eq!(window.limit, i64::from(MAX_ARTISTS_LIMIT));
        assert_eq!(query.search_term.as_deref(), Some("bow"));
        assert!(is_album_artist_query(query.artist_type.as_deref()));
        assert_eq!(artist_prefix_filter(&query).as_deref(), Some("A"));
        assert_eq!(query.albums.as_deref(), Some("Low|Heroes"));
        assert_eq!(
            query.album_ids.as_deref(),
            Some("bbbbbbbb-0000-0000-0000-000000000001|invalid")
        );
        assert_eq!(query.fields.as_deref(), Some("PrimaryImageAspectRatio"));
        assert_eq!(query.enable_images, Some(true));
        assert_eq!(
            sort_direction_from_query(query.sort_order.as_deref()),
            SortDirection::Desc
        );

        let uri = "/Artists/David%20Bowie?userId=user-1"
            .parse::<Uri>()
            .unwrap();
        let Query(query) = Query::<ArtistByNameQuery>::try_from_uri(&uri).unwrap();
        assert_eq!(query.user_id.as_deref(), Some("user-1"));
    }

    #[test]
    fn artists_query_album_filters_reuse_item_filter_parsing() {
        let query = ArtistsQuery {
            albums: Some(" Low | Heroes |LOW ".to_owned()),
            album_ids: Some("BBBBBBBB-0000-0000-0000-000000000001|bad".to_owned()),
            ..ArtistsQuery::default()
        };

        let album_names = pipe_name_list_filter(query.albums.as_deref());
        let album_ids = id_list_filter(query.album_ids.as_deref());

        assert!(album_names.enabled);
        assert_eq!(album_names.values, ["low", "heroes"]);
        assert!(album_ids.enabled);
        assert_eq!(
            album_ids.values,
            ["bbbbbbbb-0000-0000-0000-000000000001", "bad"]
        );
    }

    #[test]
    fn artist_window_clamps_pathologically_large_start_index() {
        let window = ArtistWindow::from_query(&ArtistsQuery {
            start_index: Some(500_000),
            limit: Some(50),
            ..ArtistsQuery::default()
        });

        assert_eq!(window.start_index, 10_000);
        assert_eq!(window.limit, 50);
    }

    #[test]
    fn artist_mapping_uses_music_artist_shape() {
        let item = artist_to_base_item(ArtistRecord {
            id: "artist-1".to_owned(),
            name: "Artist".to_owned(),
            total_record_count: 1,
        });

        assert_eq!(item.item_type, "MusicArtist");
        assert_eq!(item.media_type.as_deref(), Some("Audio"));
        assert!(item.is_folder);
    }
}
