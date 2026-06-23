use axum::{
    Json,
    extract::{Path, Query, State},
    http::{HeaderMap, Uri},
};
use serde::Deserialize;

use crate::{
    auth::service::AuthenticatedUser,
    compat::emby::dto::{BaseItemDto, BaseItemSource, QueryResultDto},
    error::AppError,
    library::repository::{ArtistListInput, ArtistRecord, LibraryRepository, SortDirection},
    state::AppState,
};

use super::{access::authenticate_request_user, items::normalized_parent_id};

const DEFAULT_ARTISTS_LIMIT: u32 = 100;
const MAX_ARTISTS_LIMIT: u32 = 200;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ArtistKind {
    Artists,
    AlbumArtists,
}

#[derive(Clone, Debug, Default, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "PascalCase")]
pub struct ArtistsQuery {
    pub user_id: Option<String>,
    pub parent_id: Option<String>,
    pub recursive: Option<bool>,
    pub start_index: Option<u32>,
    pub limit: Option<u32>,
    pub search_term: Option<String>,
    pub sort_order: Option<String>,
    pub artist_type: Option<String>,
    pub name_starts_with: Option<String>,
    pub name_starts_with_or_greater: Option<String>,
    pub artist_starts_with_or_greater: Option<String>,
    pub album_artist_starts_with_or_greater: Option<String>,
    pub fields: Option<String>,
    pub enable_images: Option<bool>,
}

#[derive(Clone, Debug, Default, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "PascalCase")]
pub struct ArtistByNameQuery {
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

async fn authenticate_query_user(
    state: &AppState,
    query_user_id: Option<&str>,
    headers: &HeaderMap,
    uri: &Uri,
) -> Result<AuthenticatedUser, AppError> {
    let user = authenticate_request_user(state, headers, uri).await?;
    if let Some(query_user_id) = query_user_id
        && query_user_id != user.public_id
    {
        return Err(AppError::forbidden(
            "authenticated user does not match query user",
        ));
    }

    Ok(user)
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct ArtistWindow {
    start_index: i64,
    limit: i64,
}

impl ArtistWindow {
    fn from_query(query: &ArtistsQuery) -> Self {
        Self {
            start_index: i64::from(query.start_index.unwrap_or(0)),
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
        assert_eq!(
            sort_direction_from_query(query.sort_order.as_deref()),
            SortDirection::Desc
        );
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
