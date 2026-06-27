use axum::{
    Json,
    extract::{Path, Query, State},
    http::{HeaderMap, StatusCode, Uri},
};
use serde::{Deserialize, Serialize};

use crate::{
    auth::service::AuthenticatedUser,
    compat::emby::dto::{
        BaseItemDto, BaseItemSource, DeleteInfoDto, ItemCountsDto, MediaSourceDto, QueryResultDto,
        RecommendationDto, UserItemDataDto,
    },
    db::DbPool,
    error::AppError,
    library::repository::{
        BrowseItemsInput, IntListFilter, ItemAssociationFilter, ItemCountsRecord, ItemImageFilter,
        ItemMediaFilter, ItemProviderFilter, ItemQueryOptions, ItemScalarFilter, ItemSortField,
        ItemStructureFilter, ItemTypeFilter, ItemUserDataFilter, LibraryRepository,
        MediaItemBrowseRecord, MediaQueryInput, PersonRoleFilter, PlaylistListInput,
        SimilarItemsInput, SortDirection, StringListFilter, UserItemAncestorRecord,
        UserLibraryViewRecord,
    },
    state::AppState,
};

use super::{
    access::{authenticate_query_user, authenticate_request_user, authenticate_route_user},
    playlists::playlist_to_base_item,
};

const DEFAULT_ITEMS_LIMIT: u32 = 100;
const MAX_ITEMS_LIMIT: u32 = 200;
const MAX_ITEMS_START_INDEX: u32 = 10_000;
const DEFAULT_SEARCH_HINTS_LIMIT: u32 = 20;
const MAX_SEARCH_HINTS_LIMIT: u32 = 50;
const DEFAULT_IMAGE_TYPE_LIMIT: usize = 1;
const MAX_IMAGE_TYPE_LIMIT: usize = 10;
const MAX_VIDEO_VERSION_IDS: usize = 64;
const MAX_VIDEO_VERSION_ID_LEN: usize = 128;
const PRIMARY_IMAGE_TYPES: &[&str] = &["primary", "poster"];
const POSTER_IMAGE_TYPES: &[&str] = &["poster", "primary"];
const BACKDROP_IMAGE_TYPES: &[&str] = &["backdrop"];
const LOGO_IMAGE_TYPES: &[&str] = &["logo"];
const THUMB_IMAGE_TYPES: &[&str] = &["thumb"];
const BANNER_IMAGE_TYPES: &[&str] = &["banner"];
const DISC_IMAGE_TYPES: &[&str] = &["disc"];
const ART_IMAGE_TYPES: &[&str] = &["artist", "album"];

#[derive(Clone, Debug, Default, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "PascalCase")]
pub struct ItemsQuery {
    #[serde(alias = "parentId", alias = "parent_id")]
    pub parent_id: Option<String>,
    #[serde(alias = "startIndex", alias = "start_index")]
    pub start_index: Option<u32>,
    #[serde(alias = "limit")]
    pub limit: Option<u32>,
    #[serde(alias = "recursive")]
    pub recursive: Option<bool>,
    #[serde(alias = "includeItemTypes", alias = "include_item_types")]
    pub include_item_types: Option<String>,
    #[serde(alias = "imageTypes", alias = "image_types")]
    pub image_types: Option<String>,
    #[serde(alias = "enableImages", alias = "enable_images")]
    pub enable_images: Option<bool>,
    #[serde(alias = "imageTypeLimit", alias = "image_type_limit")]
    pub image_type_limit: Option<u32>,
    #[serde(alias = "enableImageTypes", alias = "enable_image_types")]
    pub enable_image_types: Option<String>,
    #[serde(alias = "anyProviderIdEquals", alias = "any_provider_id_equals")]
    pub any_provider_id_equals: Option<String>,
    #[serde(alias = "sortBy", alias = "sort_by")]
    pub sort_by: Option<String>,
    #[serde(alias = "sortOrder", alias = "sort_order")]
    pub sort_order: Option<String>,
    #[serde(alias = "fields")]
    pub fields: Option<String>,
    #[serde(alias = "filters")]
    pub filters: Option<String>,
    #[serde(alias = "isPlayed", alias = "is_played")]
    pub is_played: Option<bool>,
    #[serde(alias = "isFavorite", alias = "is_favorite")]
    pub is_favorite: Option<bool>,
    #[serde(alias = "isFolder", alias = "is_folder")]
    pub is_folder: Option<bool>,
    #[serde(alias = "isMovie", alias = "is_movie")]
    pub is_movie: Option<bool>,
    #[serde(alias = "isSeries", alias = "is_series")]
    pub is_series: Option<bool>,
    #[serde(alias = "ids")]
    pub ids: Option<String>,
    #[serde(alias = "excludeItemIds", alias = "exclude_item_ids")]
    pub exclude_item_ids: Option<String>,
    #[serde(alias = "searchTerm", alias = "search_term")]
    pub search_term: Option<String>,
    #[serde(alias = "years")]
    pub years: Option<String>,
    #[serde(alias = "nameStartsWith", alias = "name_starts_with")]
    pub name_starts_with: Option<String>,
    #[serde(
        alias = "nameStartsWithOrGreater",
        alias = "name_starts_with_or_greater"
    )]
    pub name_starts_with_or_greater: Option<String>,
    #[serde(alias = "nameLessThan", alias = "name_less_than")]
    pub name_less_than: Option<String>,
    #[serde(alias = "genres")]
    pub genres: Option<String>,
    #[serde(alias = "genreIds", alias = "genre_ids")]
    pub genre_ids: Option<String>,
    #[serde(alias = "officialRatings", alias = "official_ratings")]
    pub official_ratings: Option<String>,
    #[serde(alias = "tags")]
    pub tags: Option<String>,
    #[serde(alias = "excludeTags", alias = "exclude_tags")]
    pub exclude_tags: Option<String>,
    #[serde(alias = "studios")]
    pub studios: Option<String>,
    #[serde(alias = "studioIds", alias = "studio_ids")]
    pub studio_ids: Option<String>,
    #[serde(alias = "person")]
    pub person: Option<String>,
    #[serde(alias = "personIds", alias = "person_ids")]
    pub person_ids: Option<String>,
    #[serde(alias = "personTypes", alias = "person_types")]
    pub person_types: Option<String>,
    #[serde(alias = "artists")]
    pub artists: Option<String>,
    #[serde(alias = "artistIds", alias = "artist_ids")]
    pub artist_ids: Option<String>,
    #[serde(alias = "albums")]
    pub albums: Option<String>,
    #[serde(alias = "albumIds", alias = "album_ids")]
    pub album_ids: Option<String>,
    #[serde(alias = "mediaTypes", alias = "media_types")]
    pub media_types: Option<String>,
    #[serde(alias = "containers")]
    pub containers: Option<String>,
    #[serde(alias = "audioCodecs", alias = "audio_codecs")]
    pub audio_codecs: Option<String>,
    #[serde(alias = "videoCodecs", alias = "video_codecs")]
    pub video_codecs: Option<String>,
    #[serde(alias = "subtitleCodecs", alias = "subtitle_codecs")]
    pub subtitle_codecs: Option<String>,
}

#[derive(Clone, Debug, Default, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "PascalCase")]
pub struct SearchHintsQuery {
    #[serde(alias = "userId", alias = "user_id")]
    pub user_id: Option<String>,
    #[serde(alias = "parentId", alias = "parent_id")]
    pub parent_id: Option<String>,
    #[serde(alias = "searchTerm", alias = "search_term")]
    pub search_term: Option<String>,
    #[serde(alias = "includeItemTypes", alias = "include_item_types")]
    pub include_item_types: Option<String>,
    #[serde(alias = "mediaTypes", alias = "media_types")]
    pub media_types: Option<String>,
    #[serde(alias = "startIndex", alias = "start_index")]
    pub start_index: Option<u32>,
    #[serde(alias = "limit")]
    pub limit: Option<u32>,
}

#[derive(Clone, Debug, Serialize, PartialEq)]
#[serde(rename_all = "PascalCase")]
pub struct SearchHintsResultDto {
    pub search_hints: Vec<SearchHintDto>,
    pub total_record_count: u32,
}

#[derive(Clone, Debug, Serialize, PartialEq)]
#[serde(rename_all = "PascalCase")]
pub struct SearchHintDto {
    pub item_id: String,
    pub id: String,
    pub name: String,
    #[serde(rename = "Type")]
    pub item_type: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub media_type: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub parent_id: Option<String>,
    pub is_folder: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub run_time_ticks: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub production_year: Option<i32>,
}

#[derive(Clone, Debug, Default, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "PascalCase")]
pub struct MusicItemsQuery {
    #[serde(alias = "userId", alias = "user_id")]
    pub user_id: Option<String>,
    #[serde(alias = "parentId", alias = "parent_id")]
    pub parent_id: Option<String>,
    #[serde(alias = "startIndex", alias = "start_index")]
    pub start_index: Option<u32>,
    #[serde(alias = "limit")]
    pub limit: Option<u32>,
    #[serde(alias = "sortBy", alias = "sort_by")]
    pub sort_by: Option<String>,
    #[serde(alias = "sortOrder", alias = "sort_order")]
    pub sort_order: Option<String>,
    #[serde(alias = "fields")]
    pub fields: Option<String>,
    #[serde(alias = "searchTerm", alias = "search_term")]
    pub search_term: Option<String>,
    #[serde(alias = "years")]
    pub years: Option<String>,
    #[serde(alias = "genres")]
    pub genres: Option<String>,
    #[serde(alias = "genreIds", alias = "genre_ids")]
    pub genre_ids: Option<String>,
    #[serde(alias = "artists")]
    pub artists: Option<String>,
    #[serde(alias = "artistIds", alias = "artist_ids")]
    pub artist_ids: Option<String>,
    #[serde(alias = "albums")]
    pub albums: Option<String>,
    #[serde(alias = "albumIds", alias = "album_ids")]
    pub album_ids: Option<String>,
    #[serde(alias = "enableImages", alias = "enable_images")]
    pub enable_images: Option<bool>,
    #[serde(alias = "imageTypeLimit", alias = "image_type_limit")]
    pub image_type_limit: Option<u32>,
    #[serde(alias = "enableImageTypes", alias = "enable_image_types")]
    pub enable_image_types: Option<String>,
}

#[derive(Clone, Debug, Default, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "PascalCase")]
pub struct ItemByIdQuery {
    #[serde(alias = "userId", alias = "user_id")]
    pub user_id: Option<String>,
}

/// Query for the bare `Artists/InstantMix` and `MusicGenres/InstantMix`
/// endpoints, where the seed is supplied as `?Id=` (an artist `public_id` or a
/// music genre id) rather than in the path.
#[derive(Clone, Debug, Default, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "PascalCase")]
pub struct InstantMixByIdQuery {
    #[serde(alias = "id")]
    pub id: Option<String>,
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

/// Query for the path-seeded instant mix endpoints `Songs/{Id}/InstantMix`,
/// `Albums/{Id}/InstantMix` and `Items/{Id}/InstantMix`, where the seed is the
/// path item id. Mirrors the music seed mix query shape (paging + image fields);
/// client sort/type fields are intentionally not accepted because the mix is a
/// fixed Audio listing seeded by the item's genres.
#[derive(Clone, Debug, Default, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "PascalCase")]
pub struct ItemInstantMixQuery {
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
pub struct CriticReviewsQuery {
    #[serde(alias = "userId", alias = "user_id")]
    pub user_id: Option<String>,
    #[serde(alias = "startIndex", alias = "start_index")]
    pub start_index: Option<u32>,
    #[serde(alias = "limit")]
    pub limit: Option<u32>,
}

#[derive(Clone, Debug, Default, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "PascalCase")]
pub struct AdditionalPartsQuery {
    #[serde(alias = "userId", alias = "user_id")]
    pub user_id: Option<String>,
    #[serde(alias = "fields")]
    pub fields: Option<String>,
    #[serde(alias = "enableImages", alias = "enable_images")]
    pub enable_images: Option<bool>,
    #[serde(alias = "imageTypeLimit", alias = "image_type_limit")]
    pub image_type_limit: Option<u32>,
    #[serde(alias = "enableImageTypes", alias = "enable_image_types")]
    pub enable_image_types: Option<String>,
    #[serde(alias = "enableUserData", alias = "enable_user_data")]
    pub enable_user_data: Option<bool>,
}

#[derive(Clone, Debug, Default, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "PascalCase")]
pub struct MergeVersionsQuery {
    #[serde(alias = "ids")]
    pub ids: Option<String>,
}

#[derive(Clone, Debug, Default, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "PascalCase")]
pub struct SpecialFeaturesQuery {
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
    #[serde(alias = "enableUserData", alias = "enable_user_data")]
    pub enable_user_data: Option<bool>,
}

#[derive(Clone, Debug, Default, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "PascalCase")]
pub struct PlaybackExtrasQuery {
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
    #[serde(alias = "enableUserData", alias = "enable_user_data")]
    pub enable_user_data: Option<bool>,
}

#[derive(Clone, Debug, Default, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "PascalCase")]
pub struct ItemCountsQuery {
    #[serde(alias = "userId", alias = "user_id")]
    pub user_id: Option<String>,
}

#[derive(Clone, Debug, Default, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "PascalCase")]
pub(super) struct MediaListQuery {
    #[serde(alias = "userId", alias = "user_id")]
    pub user_id: Option<String>,
    #[serde(alias = "parentId", alias = "parent_id")]
    pub parent_id: Option<String>,
    #[serde(alias = "startIndex", alias = "start_index")]
    pub start_index: Option<u32>,
    #[serde(alias = "limit")]
    pub limit: Option<u32>,
    #[serde(alias = "includeItemTypes", alias = "include_item_types")]
    pub include_item_types: Option<String>,
    #[serde(alias = "sortBy", alias = "sort_by")]
    pub sort_by: Option<String>,
    #[serde(alias = "sortOrder", alias = "sort_order")]
    pub sort_order: Option<String>,
    #[serde(alias = "fields")]
    pub fields: Option<String>,
}

#[derive(Clone, Debug, Default, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "PascalCase")]
pub struct SuggestionsQuery {
    #[serde(alias = "parentId", alias = "parent_id")]
    pub parent_id: Option<String>,
    #[serde(alias = "startIndex", alias = "start_index")]
    pub start_index: Option<u32>,
    #[serde(alias = "limit")]
    pub limit: Option<u32>,
    #[serde(alias = "itemLimit", alias = "item_limit")]
    pub item_limit: Option<u32>,
    #[serde(alias = "includeItemTypes", alias = "include_item_types")]
    pub include_item_types: Option<String>,
    #[serde(alias = "mediaTypes", alias = "media_types")]
    pub media_types: Option<String>,
    #[serde(alias = "sortBy", alias = "sort_by")]
    pub sort_by: Option<String>,
    #[serde(alias = "sortOrder", alias = "sort_order")]
    pub sort_order: Option<String>,
    #[serde(alias = "fields")]
    pub fields: Option<String>,
}

#[derive(Clone, Debug, Default, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "PascalCase")]
pub struct MovieRecommendationsQuery {
    #[serde(alias = "categoryLimit", alias = "category_limit")]
    pub category_limit: Option<u32>,
    #[serde(alias = "itemLimit", alias = "item_limit")]
    pub item_limit: Option<u32>,
    #[serde(alias = "userId", alias = "user_id")]
    pub user_id: Option<String>,
    #[serde(alias = "parentId", alias = "parent_id")]
    pub parent_id: Option<String>,
    #[serde(alias = "enableImages", alias = "enable_images")]
    pub enable_images: Option<bool>,
    #[serde(alias = "enableUserData", alias = "enable_user_data")]
    pub enable_user_data: Option<bool>,
    #[serde(alias = "imageTypeLimit", alias = "image_type_limit")]
    pub image_type_limit: Option<u32>,
    #[serde(alias = "enableImageTypes", alias = "enable_image_types")]
    pub enable_image_types: Option<String>,
}

#[derive(Clone, Debug, Default, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "PascalCase")]
pub struct TrailersQuery {
    #[serde(alias = "userId", alias = "user_id")]
    pub user_id: Option<String>,
    #[serde(alias = "parentId", alias = "parent_id")]
    pub parent_id: Option<String>,
    #[serde(alias = "startIndex", alias = "start_index")]
    pub start_index: Option<u32>,
    #[serde(alias = "limit")]
    pub limit: Option<u32>,
    #[serde(alias = "recursive")]
    pub recursive: Option<bool>,
    #[serde(alias = "searchTerm", alias = "search_term")]
    pub search_term: Option<String>,
    #[serde(alias = "sortBy", alias = "sort_by")]
    pub sort_by: Option<String>,
    #[serde(alias = "sortOrder", alias = "sort_order")]
    pub sort_order: Option<String>,
    #[serde(alias = "fields")]
    pub fields: Option<String>,
    #[serde(alias = "includeItemTypes", alias = "include_item_types")]
    pub include_item_types: Option<String>,
    #[serde(alias = "mediaTypes", alias = "media_types")]
    pub media_types: Option<String>,
    #[serde(alias = "enableImages", alias = "enable_images")]
    pub enable_images: Option<bool>,
    #[serde(alias = "imageTypeLimit", alias = "image_type_limit")]
    pub image_type_limit: Option<u32>,
    #[serde(alias = "enableImageTypes", alias = "enable_image_types")]
    pub enable_image_types: Option<String>,
}

#[derive(Clone, Debug, Default, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "PascalCase")]
pub struct SimilarItemsQuery {
    #[serde(alias = "userId", alias = "user_id")]
    pub user_id: Option<String>,
    #[serde(alias = "startIndex", alias = "start_index")]
    pub start_index: Option<u32>,
    #[serde(alias = "limit")]
    pub limit: Option<u32>,
    #[serde(alias = "includeItemTypes", alias = "include_item_types")]
    pub include_item_types: Option<String>,
    #[serde(alias = "sortBy", alias = "sort_by")]
    pub sort_by: Option<String>,
    #[serde(alias = "sortOrder", alias = "sort_order")]
    pub sort_order: Option<String>,
    #[serde(alias = "fields")]
    pub fields: Option<String>,
}

pub async fn user_items(
    State(state): State<AppState>,
    Path(user_id): Path<String>,
    Query(query): Query<ItemsQuery>,
    headers: HeaderMap,
    uri: Uri,
) -> Result<Json<QueryResultDto<BaseItemDto>>, AppError> {
    let Some(database) = state.database() else {
        return Err(AppError::internal("database is not configured"));
    };

    let authenticated_user = authenticate_route_user(&state, &user_id, &headers, &uri).await?;
    let result =
        list_items_for_authenticated_user(database.clone(), authenticated_user, query).await?;

    Ok(Json(result))
}

pub async fn albums(
    State(state): State<AppState>,
    Query(query): Query<MusicItemsQuery>,
    headers: HeaderMap,
    uri: Uri,
) -> Result<Json<QueryResultDto<BaseItemDto>>, AppError> {
    let Some(database) = state.database() else {
        return Err(AppError::internal("database is not configured"));
    };
    let authenticated_user =
        authenticate_query_user(&state, query.user_id.as_deref(), &headers, &uri).await?;
    let items_query = music_items_query(query, "MusicAlbum", None);
    let result =
        list_items_for_authenticated_user(database.clone(), authenticated_user, items_query)
            .await?;

    Ok(Json(result))
}

pub async fn songs(
    State(state): State<AppState>,
    Query(query): Query<MusicItemsQuery>,
    headers: HeaderMap,
    uri: Uri,
) -> Result<Json<QueryResultDto<BaseItemDto>>, AppError> {
    let Some(database) = state.database() else {
        return Err(AppError::internal("database is not configured"));
    };
    let authenticated_user =
        authenticate_query_user(&state, query.user_id.as_deref(), &headers, &uri).await?;
    let items_query = music_items_query(query, "Audio", Some("Audio"));
    let result =
        list_items_for_authenticated_user(database.clone(), authenticated_user, items_query)
            .await?;

    Ok(Json(result))
}

pub async fn album_songs(
    State(state): State<AppState>,
    Path(album_id): Path<String>,
    Query(query): Query<MusicItemsQuery>,
    headers: HeaderMap,
    uri: Uri,
) -> Result<Json<QueryResultDto<BaseItemDto>>, AppError> {
    let Some(database) = state.database() else {
        return Err(AppError::internal("database is not configured"));
    };
    let authenticated_user =
        authenticate_query_user(&state, query.user_id.as_deref(), &headers, &uri).await?;
    let start_index = clamped_start_index_u32(query.start_index);
    let Some(items_query) = album_path_songs_query(&album_id, query) else {
        return Ok(Json(QueryResultDto::new(Vec::new(), 0, start_index)));
    };
    let result =
        list_items_for_authenticated_user(database.clone(), authenticated_user, items_query)
            .await?;

    Ok(Json(result))
}

pub async fn artist_songs(
    State(state): State<AppState>,
    Path(artist_id): Path<String>,
    Query(query): Query<MusicItemsQuery>,
    headers: HeaderMap,
    uri: Uri,
) -> Result<Json<QueryResultDto<BaseItemDto>>, AppError> {
    artist_music_items_response(
        state,
        artist_id,
        query,
        "Audio",
        Some("Audio"),
        headers,
        uri,
    )
    .await
}

pub async fn artist_albums(
    State(state): State<AppState>,
    Path(artist_id): Path<String>,
    Query(query): Query<MusicItemsQuery>,
    headers: HeaderMap,
    uri: Uri,
) -> Result<Json<QueryResultDto<BaseItemDto>>, AppError> {
    artist_music_items_response(state, artist_id, query, "MusicAlbum", None, headers, uri).await
}

async fn artist_music_items_response(
    state: AppState,
    artist_id: String,
    query: MusicItemsQuery,
    include_item_types: &'static str,
    media_types: Option<&'static str>,
    headers: HeaderMap,
    uri: Uri,
) -> Result<Json<QueryResultDto<BaseItemDto>>, AppError> {
    let Some(database) = state.database() else {
        return Err(AppError::internal("database is not configured"));
    };
    let authenticated_user =
        authenticate_query_user(&state, query.user_id.as_deref(), &headers, &uri).await?;
    let start_index = clamped_start_index_u32(query.start_index);
    let Some(items_query) =
        artist_path_music_query(&artist_id, query, include_item_types, media_types)
    else {
        return Ok(Json(QueryResultDto::new(Vec::new(), 0, start_index)));
    };
    let result =
        list_items_for_authenticated_user(database.clone(), authenticated_user, items_query)
            .await?;

    Ok(Json(result))
}

pub async fn music_genre_instant_mix(
    State(state): State<AppState>,
    Path(name): Path<String>,
    Query(query): Query<MusicItemsQuery>,
    headers: HeaderMap,
    uri: Uri,
) -> Result<Json<QueryResultDto<BaseItemDto>>, AppError> {
    let Some(database) = state.database() else {
        return Err(AppError::internal("database is not configured"));
    };
    let authenticated_user =
        authenticate_query_user(&state, query.user_id.as_deref(), &headers, &uri).await?;

    let start_index = clamped_start_index_u32(query.start_index);
    let Some(items_query) = music_genre_instant_mix_query(&name, query) else {
        return Ok(Json(QueryResultDto::new(Vec::new(), 0, start_index)));
    };

    let result =
        list_items_for_authenticated_user(database.clone(), authenticated_user, items_query)
            .await?;

    Ok(Json(result))
}

pub async fn artist_instant_mix(
    State(state): State<AppState>,
    Query(query): Query<InstantMixByIdQuery>,
    headers: HeaderMap,
    uri: Uri,
) -> Result<Json<QueryResultDto<BaseItemDto>>, AppError> {
    music_seed_instant_mix(state, MusicSeed::Artist, query, headers, uri).await
}

pub async fn artist_instant_mix_by_path(
    State(state): State<AppState>,
    Path(artist_id): Path<String>,
    Query(query): Query<InstantMixByIdQuery>,
    headers: HeaderMap,
    uri: Uri,
) -> Result<Json<QueryResultDto<BaseItemDto>>, AppError> {
    let user_id = query.user_id.clone();
    let start_index = clamped_start_index_u32(query.start_index);
    let items_query = instant_mix_by_path_id_query(MusicSeed::Artist, &artist_id, query);
    music_seed_instant_mix_response(
        state,
        user_id.as_deref(),
        start_index,
        items_query,
        headers,
        uri,
    )
    .await
}

pub async fn music_genre_instant_mix_by_id(
    State(state): State<AppState>,
    Query(query): Query<InstantMixByIdQuery>,
    headers: HeaderMap,
    uri: Uri,
) -> Result<Json<QueryResultDto<BaseItemDto>>, AppError> {
    music_seed_instant_mix(state, MusicSeed::Genre, query, headers, uri).await
}

async fn music_seed_instant_mix(
    state: AppState,
    seed: MusicSeed,
    query: InstantMixByIdQuery,
    headers: HeaderMap,
    uri: Uri,
) -> Result<Json<QueryResultDto<BaseItemDto>>, AppError> {
    let user_id = query.user_id.clone();
    let start_index = clamped_start_index_u32(query.start_index);
    let items_query = instant_mix_by_id_query(seed, query);
    music_seed_instant_mix_response(
        state,
        user_id.as_deref(),
        start_index,
        items_query,
        headers,
        uri,
    )
    .await
}

async fn music_seed_instant_mix_response(
    state: AppState,
    user_id: Option<&str>,
    start_index: u32,
    items_query: Option<ItemsQuery>,
    headers: HeaderMap,
    uri: Uri,
) -> Result<Json<QueryResultDto<BaseItemDto>>, AppError> {
    let Some(database) = state.database() else {
        return Err(AppError::internal("database is not configured"));
    };
    let authenticated_user = authenticate_query_user(&state, user_id, &headers, &uri).await?;

    let Some(items_query) = items_query else {
        return Ok(Json(QueryResultDto::new(Vec::new(), 0, start_index)));
    };

    let result =
        list_items_for_authenticated_user(database.clone(), authenticated_user, items_query)
            .await?;

    Ok(Json(result))
}

pub async fn item_instant_mix(
    State(state): State<AppState>,
    Path(item_id): Path<String>,
    Query(query): Query<ItemInstantMixQuery>,
    headers: HeaderMap,
    uri: Uri,
) -> Result<Json<QueryResultDto<BaseItemDto>>, AppError> {
    let Some(database) = state.database() else {
        return Err(AppError::internal("database is not configured"));
    };
    let authenticated_user =
        authenticate_query_user(&state, query.user_id.as_deref(), &headers, &uri).await?;

    let start_index = clamped_start_index_u32(query.start_index);
    let Some(genre_ids) = LibraryRepository::new(database.clone())
        .list_instant_mix_seed_genre_ids(authenticated_user.id, &item_id)
        .await
        .map_err(|err| AppError::internal(format!("failed to resolve instant mix seed: {err}")))?
    else {
        return Err(AppError::not_found("item not found"));
    };

    let Some(items_query) = item_instant_mix_query(&item_id, &genre_ids, query) else {
        return Ok(Json(QueryResultDto::new(Vec::new(), 0, start_index)));
    };

    let result =
        list_items_for_authenticated_user(database.clone(), authenticated_user, items_query)
            .await?;

    Ok(Json(result))
}

/// Build the Audio items query for an item-seeded instant mix. Returns `None`
/// when the seed item carries no genres so the caller answers with an empty mix
/// instead of an unfiltered Audio listing. The seed's genre ids become the
/// authoritative `genre_ids` filter, the seed item is excluded from its own mix,
/// and the query flows through the proven Audio listing path so permission
/// filtering and DTO mapping stay identical to `/Songs?GenreIds=...`.
fn item_instant_mix_query(
    item_id: &str,
    genre_ids: &[i64],
    query: ItemInstantMixQuery,
) -> Option<ItemsQuery> {
    if genre_ids.is_empty() {
        return None;
    }
    let genre_ids_csv = genre_ids
        .iter()
        .map(i64::to_string)
        .collect::<Vec<_>>()
        .join(",");
    Some(ItemsQuery {
        start_index: query.start_index,
        limit: query.limit,
        recursive: Some(true),
        include_item_types: Some("Audio".to_owned()),
        media_types: Some("Audio".to_owned()),
        genre_ids: Some(genre_ids_csv),
        exclude_item_ids: Some(item_id.to_owned()),
        fields: query.fields,
        enable_images: query.enable_images,
        image_type_limit: query.image_type_limit,
        enable_image_types: query.enable_image_types,
        ..ItemsQuery::default()
    })
}

pub async fn search_hints(
    State(state): State<AppState>,
    Query(query): Query<SearchHintsQuery>,
    headers: HeaderMap,
    uri: Uri,
) -> Result<Json<SearchHintsResultDto>, AppError> {
    let Some(database) = state.database() else {
        return Err(AppError::internal("database is not configured"));
    };

    let authenticated_user =
        authenticate_query_user(&state, query.user_id.as_deref(), &headers, &uri).await?;
    let result = list_items_for_authenticated_user(
        database.clone(),
        authenticated_user,
        search_hints_items_query(query),
    )
    .await?;

    Ok(Json(SearchHintsResultDto {
        search_hints: result
            .items
            .into_iter()
            .map(search_hint_from_item)
            .collect(),
        total_record_count: result.total_record_count,
    }))
}

pub async fn trailers(
    State(state): State<AppState>,
    Query(query): Query<TrailersQuery>,
    headers: HeaderMap,
    uri: Uri,
) -> Result<Json<QueryResultDto<BaseItemDto>>, AppError> {
    let _user = authenticate_query_user(&state, query.user_id.as_deref(), &headers, &uri).await?;
    Ok(Json(empty_trailers_result(&query)))
}

pub(super) async fn list_items_for_authenticated_user(
    database: DbPool,
    authenticated_user: AuthenticatedUser,
    query: ItemsQuery,
) -> Result<QueryResultDto<BaseItemDto>, AppError> {
    let repository = LibraryRepository::new(database);
    let window = ItemWindow::from_query(&query);
    if include_item_types_requests_playlists_only(query.include_item_types.as_deref()) {
        let result = repository
            .list_user_playlists(PlaylistListInput {
                user_id: authenticated_user.id,
                parent_id: normalized_parent_id(query.parent_id),
                start_index: window.start_index,
                limit: window.limit,
                search_term: normalized_text_filter(query.search_term.as_deref()),
                sort_direction: sort_direction_from_query(
                    query.sort_order.as_deref(),
                    SortDirection::Asc,
                ),
            })
            .await
            .map_err(|err| AppError::internal(format!("failed to list playlists: {err}")))?;
        let items = result
            .items
            .into_iter()
            .map(playlist_to_base_item)
            .collect();
        return Ok(QueryResultDto::new(
            items,
            result.total_record_count,
            window.start_index as u32,
        ));
    }

    let scalar_filter = scalar_filter_from_query(&query);
    let user_data_filter = user_data_filter_from_query(&query);
    let structure_filter = structure_filter_from_query(&query);
    let media_filter = media_filter_from_query(&query);
    let provider_filter = provider_filter_from_query(&query);
    let image_filter = image_filter_from_query(&query);
    let requested_images = requested_item_images(&query);
    let association_filter = association_filter_from_query(&query);
    let parent_id = normalized_parent_id(query.parent_id);
    let recursive = query.recursive.unwrap_or(false);
    let options = item_query_options_with_filters(
        query.include_item_types.as_deref(),
        query.sort_by.as_deref(),
        query.sort_order.as_deref(),
        ItemSortField::SortName,
        SortDirection::Asc,
        scalar_filter,
        user_data_filter,
        structure_filter,
        media_filter,
        provider_filter,
        image_filter,
        association_filter,
    );
    let should_return_views =
        should_return_library_views(parent_id.as_deref(), recursive, &options);
    let _requested_fields = requested_item_fields(query.fields.as_deref());

    if should_return_views {
        let views = repository
            .list_user_views(authenticated_user.id)
            .await
            .map_err(|err| AppError::internal(format!("failed to list user views: {err}")))?;
        return Ok(library_views_to_items(views, window));
    }

    let result = repository
        .list_user_items(BrowseItemsInput {
            user_id: authenticated_user.id,
            parent_id,
            start_index: window.start_index,
            limit: window.limit,
            recursive,
            include_image_tags: requested_images.enabled,
            options,
        })
        .await
        .map_err(|err| AppError::internal(format!("failed to list user items: {err}")))?;

    let items = result
        .items
        .into_iter()
        .map(|record| media_item_to_base_item_with_images(record, &requested_images))
        .collect::<Vec<_>>();

    Ok(QueryResultDto::new(
        items,
        result.total_record_count,
        window.start_index as u32,
    ))
}

fn music_items_query(
    query: MusicItemsQuery,
    include_item_type: &str,
    media_type: Option<&str>,
) -> ItemsQuery {
    ItemsQuery {
        parent_id: query.parent_id,
        start_index: query.start_index,
        limit: query.limit,
        recursive: Some(true),
        include_item_types: Some(include_item_type.to_owned()),
        sort_by: query.sort_by,
        sort_order: query.sort_order,
        fields: query.fields,
        search_term: query.search_term,
        years: query.years,
        genres: query.genres,
        genre_ids: query.genre_ids,
        artists: query.artists,
        artist_ids: query.artist_ids,
        albums: query.albums,
        album_ids: query.album_ids,
        media_types: media_type.map(str::to_owned),
        enable_images: query.enable_images,
        image_type_limit: query.image_type_limit,
        enable_image_types: query.enable_image_types,
        ..ItemsQuery::default()
    }
}

fn album_path_songs_query(album_id: &str, mut query: MusicItemsQuery) -> Option<ItemsQuery> {
    let album_id = album_id.trim();
    if album_id.is_empty() {
        return None;
    }
    query.albums = None;
    query.album_ids = Some(album_id.to_owned());
    Some(music_items_query(query, "Audio", Some("Audio")))
}

fn artist_path_music_query(
    artist_id: &str,
    mut query: MusicItemsQuery,
    include_item_types: &'static str,
    media_types: Option<&'static str>,
) -> Option<ItemsQuery> {
    let artist_id = artist_id.trim();
    if artist_id.is_empty() {
        return None;
    }
    query.artists = None;
    query.artist_ids = Some(artist_id.to_owned());
    Some(music_items_query(query, include_item_types, media_types))
}

/// Build the underlying Audio items query for a `MusicGenres/{Name}/InstantMix`
/// seed. The path genre name is the authoritative seed, so any client-supplied
/// genre filter is replaced; returns `None` when the seed name is blank so the
/// caller can answer with an empty mix instead of an unfiltered Audio listing.
/// Reuses the proven `music_items_query` path, so permission filtering and DTO
/// mapping stay identical to `/Songs?Genres=...`.
fn music_genre_instant_mix_query(name: &str, mut query: MusicItemsQuery) -> Option<ItemsQuery> {
    let seed_genre = name.trim();
    if seed_genre.is_empty() {
        return None;
    }
    query.genres = Some(seed_genre.to_owned());
    query.genre_ids = None;
    Some(music_items_query(query, "Audio", Some("Audio")))
}

/// Which music dimension seeds a bare `?Id=` instant mix.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum MusicSeed {
    Artist,
    Genre,
}

/// Build the underlying Audio items query for a bare `Artists/InstantMix?Id=`
/// (artist seed) or `MusicGenres/InstantMix?Id=` (genre seed). Returns `None`
/// when no seed id is supplied so the caller answers with an empty mix instead
/// of an unfiltered Audio listing. The artist id is an artist `public_id` and
/// the genre id a numeric genre id; both flow through the proven `artist_ids` /
/// `genre_ids` filters (invalid formats simply match nothing).
fn instant_mix_by_id_query(seed: MusicSeed, query: InstantMixByIdQuery) -> Option<ItemsQuery> {
    let seed_id = query
        .id
        .as_deref()
        .map(str::trim)
        .filter(|id| !id.is_empty())?
        .to_owned();

    let mut items_query = ItemsQuery {
        start_index: query.start_index,
        limit: query.limit,
        recursive: Some(true),
        include_item_types: Some("Audio".to_owned()),
        media_types: Some("Audio".to_owned()),
        fields: query.fields,
        enable_images: query.enable_images,
        image_type_limit: query.image_type_limit,
        enable_image_types: query.enable_image_types,
        ..ItemsQuery::default()
    };
    match seed {
        MusicSeed::Artist => items_query.artist_ids = Some(seed_id),
        MusicSeed::Genre => items_query.genre_ids = Some(seed_id),
    }
    Some(items_query)
}

fn instant_mix_by_path_id_query(
    seed: MusicSeed,
    path_id: &str,
    mut query: InstantMixByIdQuery,
) -> Option<ItemsQuery> {
    let seed_id = path_id.trim();
    if seed_id.is_empty() {
        return None;
    }
    query.id = Some(seed_id.to_owned());
    instant_mix_by_id_query(seed, query)
}

fn search_hints_items_query(query: SearchHintsQuery) -> ItemsQuery {
    let window = SearchHintsWindow::from_query(&query);
    ItemsQuery {
        parent_id: query.parent_id,
        start_index: Some(window.start_index as u32),
        limit: Some(window.limit as u32),
        recursive: Some(true),
        include_item_types: query.include_item_types,
        search_term: query.search_term,
        media_types: query.media_types,
        sort_by: Some("SortName".to_owned()),
        sort_order: Some("Ascending".to_owned()),
        ..ItemsQuery::default()
    }
}

fn suggestions_items_query(query: SuggestionsQuery) -> ItemsQuery {
    ItemsQuery {
        parent_id: query.parent_id,
        start_index: query.start_index,
        limit: query.limit.or(query.item_limit),
        recursive: Some(true),
        include_item_types: query.include_item_types,
        sort_by: Some(query.sort_by.unwrap_or_else(|| "DateCreated".to_owned())),
        sort_order: Some(query.sort_order.unwrap_or_else(|| "Descending".to_owned())),
        fields: query.fields,
        media_types: query.media_types,
        ..ItemsQuery::default()
    }
}

fn movie_recommendations_items_query(query: MovieRecommendationsQuery) -> ItemsQuery {
    ItemsQuery {
        parent_id: query.parent_id,
        limit: query.item_limit,
        recursive: Some(true),
        include_item_types: Some("Movie".to_owned()),
        media_types: Some("Video".to_owned()),
        sort_by: Some("DateCreated".to_owned()),
        sort_order: Some("Descending".to_owned()),
        enable_images: query.enable_images,
        image_type_limit: query.image_type_limit,
        enable_image_types: query.enable_image_types,
        ..ItemsQuery::default()
    }
}

fn empty_trailers_result(query: &TrailersQuery) -> QueryResultDto<BaseItemDto> {
    let window = TrailersWindow::from_query(query);
    let _input = trailers_query_input(query, window);

    QueryResultDto::new(Vec::new(), 0, window.start_index as u32)
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct TrailersWindow {
    start_index: usize,
    limit: usize,
}

impl TrailersWindow {
    fn from_query(query: &TrailersQuery) -> Self {
        Self {
            start_index: clamped_start_index_usize(query.start_index),
            limit: query
                .limit
                .unwrap_or(DEFAULT_ITEMS_LIMIT)
                .min(MAX_ITEMS_LIMIT) as usize,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct TrailersQueryInput {
    start_index: usize,
    limit: usize,
    parent_id: Option<String>,
    recursive: bool,
    search_term: Option<String>,
    include_item_types: ItemTypeFilter,
    media_types: StringListFilter,
}

fn trailers_query_input(query: &TrailersQuery, window: TrailersWindow) -> TrailersQueryInput {
    let _requested_fields = requested_item_fields(query.fields.as_deref());
    let _requested_images = requested_item_images(&ItemsQuery {
        enable_images: query.enable_images,
        image_type_limit: query.image_type_limit,
        enable_image_types: query.enable_image_types.clone(),
        ..ItemsQuery::default()
    });

    TrailersQueryInput {
        start_index: window.start_index,
        limit: window.limit,
        parent_id: normalized_parent_id(query.parent_id.clone()),
        recursive: query.recursive.unwrap_or(true),
        search_term: normalized_text_filter(query.search_term.as_deref()),
        include_item_types: include_item_types_filter(query.include_item_types.as_deref()),
        media_types: media_type_list_filter(query.media_types.as_deref()),
    }
}

fn clamped_start_index_i64(start_index: Option<u32>) -> i64 {
    i64::from(start_index.unwrap_or(0).min(MAX_ITEMS_START_INDEX))
}

fn clamped_start_index_usize(start_index: Option<u32>) -> usize {
    start_index.unwrap_or(0).min(MAX_ITEMS_START_INDEX) as usize
}

fn clamped_start_index_u32(start_index: Option<u32>) -> u32 {
    start_index.unwrap_or(0).min(MAX_ITEMS_START_INDEX)
}

pub async fn resume_items(
    State(state): State<AppState>,
    Path(user_id): Path<String>,
    Query(query): Query<MediaListQuery>,
    headers: HeaderMap,
    uri: Uri,
) -> Result<Json<QueryResultDto<BaseItemDto>>, AppError> {
    let user = authenticate_route_user(&state, &user_id, &headers, &uri).await?;
    resume_items_response(&state, user, &query).await
}

/// Query-parameter variant `/Items/Resume?UserId=...` of the continue-watching
/// row, alongside the path form `/Users/{UserId}/Items/Resume`. Emby/Jellyfin
/// clients commonly call this form for the home "Continue Watching" shelf.
pub async fn resume_items_for_query_user(
    State(state): State<AppState>,
    Query(query): Query<MediaListQuery>,
    headers: HeaderMap,
    uri: Uri,
) -> Result<Json<QueryResultDto<BaseItemDto>>, AppError> {
    let user = authenticate_query_user(&state, query.user_id.as_deref(), &headers, &uri).await?;
    resume_items_response(&state, user, &query).await
}

async fn resume_items_response(
    state: &AppState,
    user: AuthenticatedUser,
    query: &MediaListQuery,
) -> Result<Json<QueryResultDto<BaseItemDto>>, AppError> {
    let Some(database) = state.database() else {
        return Err(AppError::internal("database is not configured"));
    };

    let window = ItemWindow::from_media_query(query);
    let options = item_query_options(
        query.include_item_types.as_deref(),
        query.sort_by.as_deref(),
        query.sort_order.as_deref(),
        ItemSortField::DateCreated,
        SortDirection::Desc,
    );
    let _requested_fields = requested_item_fields(query.fields.as_deref());
    let result = LibraryRepository::new(database.clone())
        .list_resume_items(MediaQueryInput {
            user_id: user.id,
            parent_id: normalized_parent_id(query.parent_id.clone()),
            start_index: window.start_index,
            limit: window.limit,
            options,
        })
        .await
        .map_err(|err| AppError::internal(format!("failed to list resume items: {err}")))?;

    Ok(Json(media_query_result(result, window)))
}

pub async fn latest_items(
    State(state): State<AppState>,
    Path(user_id): Path<String>,
    Query(query): Query<MediaListQuery>,
    headers: HeaderMap,
    uri: Uri,
) -> Result<Json<Vec<BaseItemDto>>, AppError> {
    let user = authenticate_route_user(&state, &user_id, &headers, &uri).await?;
    latest_items_response(&state, user, &query).await
}

/// Query-parameter variant `/Items/Latest?UserId=...` of the latest-added home
/// endpoint, alongside the path form `/Users/{UserId}/Items/Latest`. Many Emby
/// clients call this form; it reuses the same permission-filtered listing.
pub async fn latest_items_for_query_user(
    State(state): State<AppState>,
    Query(query): Query<MediaListQuery>,
    headers: HeaderMap,
    uri: Uri,
) -> Result<Json<Vec<BaseItemDto>>, AppError> {
    let user = authenticate_query_user(&state, query.user_id.as_deref(), &headers, &uri).await?;
    latest_items_response(&state, user, &query).await
}

async fn latest_items_response(
    state: &AppState,
    user: AuthenticatedUser,
    query: &MediaListQuery,
) -> Result<Json<Vec<BaseItemDto>>, AppError> {
    let Some(database) = state.database() else {
        return Err(AppError::internal("database is not configured"));
    };

    let window = ItemWindow::from_media_query(query);
    let options = item_query_options(
        query.include_item_types.as_deref(),
        query.sort_by.as_deref(),
        query.sort_order.as_deref(),
        ItemSortField::DateCreated,
        SortDirection::Desc,
    );
    let _requested_fields = requested_item_fields(query.fields.as_deref());
    let result = LibraryRepository::new(database.clone())
        .list_latest_items(MediaQueryInput {
            user_id: user.id,
            parent_id: normalized_parent_id(query.parent_id.clone()),
            start_index: window.start_index,
            limit: window.limit,
            options,
        })
        .await
        .map_err(|err| AppError::internal(format!("failed to list latest items: {err}")))?;

    Ok(Json(media_items_to_dtos(result.items)))
}

pub async fn suggested_items(
    State(state): State<AppState>,
    Path(user_id): Path<String>,
    Query(query): Query<SuggestionsQuery>,
    headers: HeaderMap,
    uri: Uri,
) -> Result<Json<QueryResultDto<BaseItemDto>>, AppError> {
    let Some(database) = state.database() else {
        return Err(AppError::internal("database is not configured"));
    };

    let authenticated_user = authenticate_route_user(&state, &user_id, &headers, &uri).await?;
    let result = list_items_for_authenticated_user(
        database.clone(),
        authenticated_user,
        suggestions_items_query(query),
    )
    .await?;

    Ok(Json(result))
}

pub async fn movie_recommendations(
    State(state): State<AppState>,
    Query(query): Query<MovieRecommendationsQuery>,
    headers: HeaderMap,
    uri: Uri,
) -> Result<Json<Vec<RecommendationDto>>, AppError> {
    let Some(database) = state.database() else {
        return Err(AppError::internal("database is not configured"));
    };

    let authenticated_user =
        authenticate_query_user(&state, query.user_id.as_deref(), &headers, &uri).await?;
    let category_limit = query.category_limit.unwrap_or(1).clamp(0, MAX_ITEMS_LIMIT);
    if category_limit == 0 {
        return Ok(Json(Vec::new()));
    }
    let _include_user_data = query.enable_user_data.unwrap_or(true);

    let result = list_items_for_authenticated_user(
        database.clone(),
        authenticated_user,
        movie_recommendations_items_query(query),
    )
    .await?;

    if result.items.is_empty() {
        return Ok(Json(Vec::new()));
    }

    Ok(Json(vec![RecommendationDto::recently_added_movies(
        result.items,
    )]))
}

pub async fn user_item_counts(
    State(state): State<AppState>,
    Path(user_id): Path<String>,
    headers: HeaderMap,
    uri: Uri,
) -> Result<Json<ItemCountsDto>, AppError> {
    let user = authenticate_route_user(&state, &user_id, &headers, &uri).await?;
    item_counts_for_user(&state, user.id).await
}

pub async fn item_counts(
    State(state): State<AppState>,
    Query(query): Query<ItemCountsQuery>,
    headers: HeaderMap,
    uri: Uri,
) -> Result<Json<ItemCountsDto>, AppError> {
    let user = authenticate_request_user(&state, &headers, &uri).await?;
    if let Some(query_user_id) = query.user_id
        && query_user_id != user.public_id
    {
        return Err(AppError::forbidden(
            "authenticated user does not match query user",
        ));
    }

    item_counts_for_user(&state, user.id).await
}

pub async fn user_items_root(
    State(state): State<AppState>,
    Path(user_id): Path<String>,
    headers: HeaderMap,
    uri: Uri,
) -> Result<Json<BaseItemDto>, AppError> {
    authenticate_route_user(&state, &user_id, &headers, &uri).await?;

    Ok(Json(root_folder_item()))
}

async fn item_counts_for_user(
    state: &AppState,
    user_id: i64,
) -> Result<Json<ItemCountsDto>, AppError> {
    let Some(database) = state.database() else {
        return Err(AppError::internal("database is not configured"));
    };

    let counts = LibraryRepository::new(database.clone())
        .count_user_items(user_id)
        .await
        .map_err(|err| AppError::internal(format!("failed to count user items: {err}")))?;

    Ok(Json(item_counts_to_dto(counts)))
}

pub async fn user_item_by_id(
    State(state): State<AppState>,
    Path((user_id, item_id)): Path<(String, String)>,
    headers: HeaderMap,
    uri: Uri,
) -> Result<Json<BaseItemDto>, AppError> {
    let user = authenticate_route_user(&state, &user_id, &headers, &uri).await?;
    item_by_id_for_user(&state, user, item_id).await
}

pub async fn item_by_id(
    State(state): State<AppState>,
    Path(item_id): Path<String>,
    Query(query): Query<ItemByIdQuery>,
    headers: HeaderMap,
    uri: Uri,
) -> Result<Json<BaseItemDto>, AppError> {
    let user = authenticate_request_user(&state, &headers, &uri).await?;
    if let Some(query_user_id) = query.user_id
        && query_user_id != user.public_id
    {
        return Err(AppError::forbidden(
            "authenticated user does not match query user",
        ));
    }

    item_by_id_for_user(&state, user, item_id).await
}

pub async fn additional_video_parts(
    State(state): State<AppState>,
    Path(item_id): Path<String>,
    Query(query): Query<AdditionalPartsQuery>,
    headers: HeaderMap,
    uri: Uri,
) -> Result<Json<QueryResultDto<BaseItemDto>>, AppError> {
    let user = authenticate_request_user(&state, &headers, &uri).await?;
    if let Some(query_user_id) = query.user_id.as_deref()
        && query_user_id != user.public_id
    {
        return Err(AppError::forbidden(
            "authenticated user does not match query user",
        ));
    }

    let _requested_fields = requested_item_fields(query.fields.as_deref());
    let _requested_images = requested_item_images(&ItemsQuery {
        enable_images: query.enable_images,
        image_type_limit: query.image_type_limit,
        enable_image_types: query.enable_image_types,
        ..ItemsQuery::default()
    });
    let _include_user_data = query.enable_user_data.unwrap_or(true);

    let _ = item_by_id_for_user(&state, user, item_id).await?;

    Ok(Json(empty_additional_parts_result()))
}

pub async fn delete_video_alternate_sources(
    State(state): State<AppState>,
    Path(item_id): Path<String>,
    headers: HeaderMap,
    uri: Uri,
) -> Result<StatusCode, AppError> {
    let user = authenticate_request_user(&state, &headers, &uri).await?;
    ensure_server_admin(&user)?;
    let _item_id = video_version_item_id(&item_id)?;

    Err(AppError::conflict(
        "video alternate source deletion is not configured",
    ))
}

pub async fn merge_video_versions(
    State(state): State<AppState>,
    Query(query): Query<MergeVersionsQuery>,
    headers: HeaderMap,
    uri: Uri,
) -> Result<StatusCode, AppError> {
    let user = authenticate_request_user(&state, &headers, &uri).await?;
    ensure_server_admin(&user)?;
    let _input = merge_versions_input(&query)?;

    Err(AppError::conflict(
        "video version merging is not configured",
    ))
}

pub async fn item_delete_info(
    State(state): State<AppState>,
    Path(item_id): Path<String>,
    Query(query): Query<ItemByIdQuery>,
    headers: HeaderMap,
    uri: Uri,
) -> Result<Json<DeleteInfoDto>, AppError> {
    let user = authenticate_request_user(&state, &headers, &uri).await?;
    if let Some(query_user_id) = query.user_id
        && query_user_id != user.public_id
    {
        return Err(AppError::forbidden(
            "authenticated user does not match query user",
        ));
    }

    let _ = item_by_id_for_user(&state, user, item_id).await?;

    Ok(Json(DeleteInfoDto::empty()))
}

pub async fn item_critic_reviews(
    State(state): State<AppState>,
    Path(item_id): Path<String>,
    Query(query): Query<CriticReviewsQuery>,
    headers: HeaderMap,
    uri: Uri,
) -> Result<Json<QueryResultDto<BaseItemDto>>, AppError> {
    let user = authenticate_request_user(&state, &headers, &uri).await?;
    if let Some(query_user_id) = query.user_id.as_deref()
        && query_user_id != user.public_id
    {
        return Err(AppError::forbidden(
            "authenticated user does not match query user",
        ));
    }

    let _ = item_by_id_for_user(&state, user, item_id).await?;

    Ok(Json(empty_critic_reviews_result(&query)))
}

pub async fn item_special_features(
    State(state): State<AppState>,
    Path(item_id): Path<String>,
    Query(query): Query<SpecialFeaturesQuery>,
    headers: HeaderMap,
    uri: Uri,
) -> Result<Json<QueryResultDto<BaseItemDto>>, AppError> {
    let user = authenticate_request_user(&state, &headers, &uri).await?;
    if let Some(query_user_id) = query.user_id.as_deref()
        && query_user_id != user.public_id
    {
        return Err(AppError::forbidden(
            "authenticated user does not match query user",
        ));
    }

    special_features_for_user(&state, user, item_id, query).await
}

pub async fn user_item_special_features(
    State(state): State<AppState>,
    Path((user_id, item_id)): Path<(String, String)>,
    Query(query): Query<SpecialFeaturesQuery>,
    headers: HeaderMap,
    uri: Uri,
) -> Result<Json<QueryResultDto<BaseItemDto>>, AppError> {
    let user = authenticate_route_user(&state, &user_id, &headers, &uri).await?;
    if let Some(query_user_id) = query.user_id.as_deref()
        && query_user_id != user.public_id
    {
        return Err(AppError::forbidden(
            "authenticated user does not match query user",
        ));
    }

    special_features_for_user(&state, user, item_id, query).await
}

pub async fn item_intros(
    State(state): State<AppState>,
    Path(item_id): Path<String>,
    Query(query): Query<PlaybackExtrasQuery>,
    headers: HeaderMap,
    uri: Uri,
) -> Result<Json<QueryResultDto<BaseItemDto>>, AppError> {
    let user = authenticate_request_user(&state, &headers, &uri).await?;
    if let Some(query_user_id) = query.user_id.as_deref()
        && query_user_id != user.public_id
    {
        return Err(AppError::forbidden(
            "authenticated user does not match query user",
        ));
    }

    intros_for_user(&state, user, item_id, query).await
}

pub async fn user_item_intros(
    State(state): State<AppState>,
    Path((user_id, item_id)): Path<(String, String)>,
    Query(query): Query<PlaybackExtrasQuery>,
    headers: HeaderMap,
    uri: Uri,
) -> Result<Json<QueryResultDto<BaseItemDto>>, AppError> {
    let user = authenticate_route_user(&state, &user_id, &headers, &uri).await?;
    if let Some(query_user_id) = query.user_id.as_deref()
        && query_user_id != user.public_id
    {
        return Err(AppError::forbidden(
            "authenticated user does not match query user",
        ));
    }

    intros_for_user(&state, user, item_id, query).await
}

pub async fn item_local_trailers(
    State(state): State<AppState>,
    Path(item_id): Path<String>,
    Query(query): Query<PlaybackExtrasQuery>,
    headers: HeaderMap,
    uri: Uri,
) -> Result<Json<Vec<BaseItemDto>>, AppError> {
    let user = authenticate_request_user(&state, &headers, &uri).await?;
    if let Some(query_user_id) = query.user_id.as_deref()
        && query_user_id != user.public_id
    {
        return Err(AppError::forbidden(
            "authenticated user does not match query user",
        ));
    }

    local_trailers_for_user(&state, user, item_id, query).await
}

pub async fn user_item_local_trailers(
    State(state): State<AppState>,
    Path((user_id, item_id)): Path<(String, String)>,
    Query(query): Query<PlaybackExtrasQuery>,
    headers: HeaderMap,
    uri: Uri,
) -> Result<Json<Vec<BaseItemDto>>, AppError> {
    let user = authenticate_route_user(&state, &user_id, &headers, &uri).await?;
    if let Some(query_user_id) = query.user_id.as_deref()
        && query_user_id != user.public_id
    {
        return Err(AppError::forbidden(
            "authenticated user does not match query user",
        ));
    }

    local_trailers_for_user(&state, user, item_id, query).await
}

pub async fn item_ancestors(
    State(state): State<AppState>,
    Path(item_id): Path<String>,
    Query(query): Query<ItemByIdQuery>,
    headers: HeaderMap,
    uri: Uri,
) -> Result<Json<Vec<BaseItemDto>>, AppError> {
    let user = authenticate_request_user(&state, &headers, &uri).await?;
    if let Some(query_user_id) = query.user_id
        && query_user_id != user.public_id
    {
        return Err(AppError::forbidden(
            "authenticated user does not match query user",
        ));
    }

    let Some(database) = state.database() else {
        return Err(AppError::internal("database is not configured"));
    };

    let ancestors = LibraryRepository::new(database.clone())
        .list_user_item_ancestors(user.id, &item_id)
        .await
        .map_err(|err| AppError::internal(format!("failed to list item ancestors: {err}")))?;
    if ancestors.is_empty() {
        return Err(AppError::not_found("item not found"));
    }

    Ok(Json(
        ancestors
            .into_iter()
            .map(ancestor_record_to_base_item)
            .collect(),
    ))
}

pub async fn similar_items(
    State(state): State<AppState>,
    Path(item_id): Path<String>,
    Query(query): Query<SimilarItemsQuery>,
    headers: HeaderMap,
    uri: Uri,
) -> Result<Json<QueryResultDto<BaseItemDto>>, AppError> {
    let user = authenticate_request_user(&state, &headers, &uri).await?;
    if let Some(query_user_id) = query.user_id.as_deref()
        && query_user_id != user.public_id
    {
        return Err(AppError::forbidden(
            "authenticated user does not match query user",
        ));
    }

    let Some(database) = state.database() else {
        return Err(AppError::internal("database is not configured"));
    };

    let window = ItemWindow::from_similar_query(&query);
    let options = item_query_options(
        query.include_item_types.as_deref(),
        query.sort_by.as_deref(),
        query.sort_order.as_deref(),
        ItemSortField::SortName,
        SortDirection::Asc,
    );
    let _requested_fields = requested_item_fields(query.fields.as_deref());

    let Some(result) = LibraryRepository::new(database.clone())
        .list_similar_items(SimilarItemsInput {
            user_id: user.id,
            item_id,
            start_index: window.start_index,
            limit: window.limit,
            options,
        })
        .await
        .map_err(|err| AppError::internal(format!("failed to list similar items: {err}")))?
    else {
        return Err(AppError::not_found("item not found"));
    };

    Ok(Json(media_query_result(result, window)))
}

async fn special_features_for_user(
    state: &AppState,
    user: AuthenticatedUser,
    item_id: String,
    query: SpecialFeaturesQuery,
) -> Result<Json<QueryResultDto<BaseItemDto>>, AppError> {
    let _requested_fields = requested_item_fields(query.fields.as_deref());
    let _requested_images = requested_item_images(&ItemsQuery {
        enable_images: query.enable_images,
        image_type_limit: query.image_type_limit,
        enable_image_types: query.enable_image_types,
        ..ItemsQuery::default()
    });
    let _include_user_data = query.enable_user_data.unwrap_or(true);

    let _ = item_by_id_for_user(state, user, item_id).await?;

    Ok(Json(QueryResultDto::new(
        Vec::new(),
        0,
        clamped_start_index_u32(query.start_index),
    )))
}

async fn intros_for_user(
    state: &AppState,
    user: AuthenticatedUser,
    item_id: String,
    query: PlaybackExtrasQuery,
) -> Result<Json<QueryResultDto<BaseItemDto>>, AppError> {
    validate_playback_extras_request(state, user, item_id, &query).await?;

    Ok(Json(QueryResultDto::new(
        Vec::new(),
        0,
        clamped_start_index_u32(query.start_index),
    )))
}

async fn local_trailers_for_user(
    state: &AppState,
    user: AuthenticatedUser,
    item_id: String,
    query: PlaybackExtrasQuery,
) -> Result<Json<Vec<BaseItemDto>>, AppError> {
    validate_playback_extras_request(state, user, item_id, &query).await?;

    Ok(Json(Vec::new()))
}

fn empty_critic_reviews_result(query: &CriticReviewsQuery) -> QueryResultDto<BaseItemDto> {
    let window = CriticReviewsWindow::from_query(query);
    let _limit = window.limit;

    QueryResultDto::new(Vec::new(), 0, window.start_index as u32)
}

async fn validate_playback_extras_request(
    state: &AppState,
    user: AuthenticatedUser,
    item_id: String,
    query: &PlaybackExtrasQuery,
) -> Result<(), AppError> {
    let _requested_fields = requested_item_fields(query.fields.as_deref());
    let _requested_images = requested_item_images(&ItemsQuery {
        enable_images: query.enable_images,
        image_type_limit: query.image_type_limit,
        enable_image_types: query.enable_image_types.clone(),
        ..ItemsQuery::default()
    });
    let _include_user_data = query.enable_user_data.unwrap_or(true);
    let _limit = query.limit.map(|limit| limit.min(MAX_ITEMS_LIMIT));

    let _ = item_by_id_for_user(state, user, item_id).await?;

    Ok(())
}

async fn item_by_id_for_user(
    state: &AppState,
    user: AuthenticatedUser,
    item_id: String,
) -> Result<Json<BaseItemDto>, AppError> {
    let Some(database) = state.database() else {
        return Err(AppError::internal("database is not configured"));
    };

    let repository = LibraryRepository::new(database.clone());
    if let Some(view) = repository
        .find_user_view_by_id(user.id, &item_id)
        .await
        .map_err(|err| AppError::internal(format!("failed to get library view: {err}")))?
    {
        return Ok(Json(library_view_to_base_item(view)));
    }

    let Some(item) = repository
        .find_user_item_by_id(user.id, &item_id)
        .await
        .map_err(|err| AppError::internal(format!("failed to get media item: {err}")))?
    else {
        return Err(AppError::not_found("item not found"));
    };

    Ok(Json(media_item_to_base_item(item)))
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct ItemWindow {
    pub start_index: i64,
    pub limit: i64,
}

impl ItemWindow {
    fn from_query(query: &ItemsQuery) -> Self {
        Self {
            start_index: clamped_start_index_i64(query.start_index),
            limit: i64::from(
                query
                    .limit
                    .unwrap_or(DEFAULT_ITEMS_LIMIT)
                    .clamp(1, MAX_ITEMS_LIMIT),
            ),
        }
    }

    pub(super) fn from_media_query(query: &MediaListQuery) -> Self {
        Self {
            start_index: clamped_start_index_i64(query.start_index),
            limit: i64::from(
                query
                    .limit
                    .unwrap_or(DEFAULT_ITEMS_LIMIT)
                    .clamp(1, MAX_ITEMS_LIMIT),
            ),
        }
    }

    fn from_similar_query(query: &SimilarItemsQuery) -> Self {
        Self {
            start_index: clamped_start_index_i64(query.start_index),
            limit: i64::from(
                query
                    .limit
                    .unwrap_or(DEFAULT_ITEMS_LIMIT)
                    .clamp(1, MAX_ITEMS_LIMIT),
            ),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct CriticReviewsWindow {
    start_index: i64,
    limit: i64,
}

impl CriticReviewsWindow {
    fn from_query(query: &CriticReviewsQuery) -> Self {
        Self {
            start_index: clamped_start_index_i64(query.start_index),
            limit: i64::from(
                query
                    .limit
                    .unwrap_or(DEFAULT_ITEMS_LIMIT)
                    .clamp(1, MAX_ITEMS_LIMIT),
            ),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct SearchHintsWindow {
    start_index: i64,
    limit: i64,
}

impl SearchHintsWindow {
    fn from_query(query: &SearchHintsQuery) -> Self {
        Self {
            start_index: clamped_start_index_i64(query.start_index),
            limit: i64::from(
                query
                    .limit
                    .unwrap_or(DEFAULT_SEARCH_HINTS_LIMIT)
                    .clamp(1, MAX_SEARCH_HINTS_LIMIT),
            ),
        }
    }
}

fn library_views_to_items(
    records: Vec<UserLibraryViewRecord>,
    window: ItemWindow,
) -> QueryResultDto<BaseItemDto> {
    let total = records.len() as u32;
    let items = records
        .into_iter()
        .skip(window.start_index as usize)
        .take(window.limit as usize)
        .map(library_view_to_base_item)
        .collect::<Vec<_>>();

    QueryResultDto::new(items, total, window.start_index as u32)
}

fn empty_additional_parts_result() -> QueryResultDto<BaseItemDto> {
    QueryResultDto::new(Vec::new(), 0, 0)
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct MergeVersionsInput {
    ids: Vec<String>,
}

fn ensure_server_admin(user: &AuthenticatedUser) -> Result<(), AppError> {
    if !user.can_manage_server() {
        return Err(AppError::forbidden("server management permission required"));
    }

    Ok(())
}

fn video_version_item_id(value: &str) -> Result<String, AppError> {
    let value = value.trim();
    if value.is_empty() {
        return Err(AppError::unprocessable("video item id is required"));
    }
    if value.len() > MAX_VIDEO_VERSION_ID_LEN
        || !value
            .chars()
            .all(|ch| ch.is_ascii_alphanumeric() || matches!(ch, '-' | '_' | '.'))
    {
        return Err(AppError::unprocessable("video item id is invalid"));
    }

    Ok(value.to_owned())
}

fn merge_versions_input(query: &MergeVersionsQuery) -> Result<MergeVersionsInput, AppError> {
    let Some(raw_ids) = query.ids.as_deref() else {
        return Err(AppError::unprocessable("Ids is required"));
    };
    let mut ids = Vec::new();

    for raw_id in raw_ids.split(',') {
        let id = raw_id.trim();
        if id.is_empty() {
            continue;
        }
        let id = video_version_item_id(id)?;
        if ids.iter().all(|existing| existing != &id) {
            ids.push(id);
        }
        if ids.len() > MAX_VIDEO_VERSION_IDS {
            return Err(AppError::unprocessable("too many video version ids"));
        }
    }

    if ids.len() < 2 {
        return Err(AppError::unprocessable(
            "at least two video version ids are required",
        ));
    }

    Ok(MergeVersionsInput { ids })
}

pub(super) fn media_query_result(
    result: crate::library::repository::BrowseItemsResult,
    window: ItemWindow,
) -> QueryResultDto<BaseItemDto> {
    QueryResultDto::new(
        media_items_to_dtos(result.items),
        result.total_record_count,
        window.start_index as u32,
    )
}

pub(super) fn media_items_to_dtos(records: Vec<MediaItemBrowseRecord>) -> Vec<BaseItemDto> {
    records.into_iter().map(media_item_to_base_item).collect()
}

fn ancestor_record_to_base_item(record: UserItemAncestorRecord) -> BaseItemDto {
    match record {
        UserItemAncestorRecord::Library(record) => library_view_to_base_item(record),
        UserItemAncestorRecord::Media(record) => media_item_to_base_item(record),
    }
}

fn item_counts_to_dto(record: ItemCountsRecord) -> ItemCountsDto {
    ItemCountsDto {
        movie_count: record.movie_count,
        series_count: record.series_count,
        episode_count: record.episode_count,
        artist_count: record.artist_count,
        program_count: 0,
        trailer_count: 0,
        song_count: record.song_count,
        album_count: record.album_count,
        music_video_count: 0,
        box_set_count: record.box_set_count,
        book_count: 0,
        item_count: record.item_count,
    }
}

pub(super) fn normalized_parent_id(parent_id: Option<String>) -> Option<String> {
    parent_id.and_then(|parent_id| {
        let trimmed = parent_id.trim();
        (!trimmed.is_empty() && !trimmed.eq_ignore_ascii_case("root")).then(|| trimmed.to_owned())
    })
}

pub(super) fn item_query_options(
    include_item_types: Option<&str>,
    sort_by: Option<&str>,
    sort_order: Option<&str>,
    default_sort: ItemSortField,
    default_direction: SortDirection,
) -> ItemQueryOptions {
    item_query_options_with_filters(
        include_item_types,
        sort_by,
        sort_order,
        default_sort,
        default_direction,
        ItemScalarFilter::default(),
        ItemUserDataFilter::default(),
        ItemStructureFilter::default(),
        ItemMediaFilter::default(),
        ItemProviderFilter::default(),
        ItemImageFilter::default(),
        ItemAssociationFilter::default(),
    )
}

fn item_query_options_with_filters(
    include_item_types: Option<&str>,
    sort_by: Option<&str>,
    sort_order: Option<&str>,
    default_sort: ItemSortField,
    default_direction: SortDirection,
    scalar_filter: ItemScalarFilter,
    user_data_filter: ItemUserDataFilter,
    structure_filter: ItemStructureFilter,
    media_filter: ItemMediaFilter,
    provider_filter: ItemProviderFilter,
    image_filter: ItemImageFilter,
    association_filter: ItemAssociationFilter,
) -> ItemQueryOptions {
    ItemQueryOptions {
        type_filter: include_item_types_filter(include_item_types),
        scalar_filter,
        user_data_filter,
        structure_filter,
        media_filter,
        provider_filter,
        image_filter,
        association_filter,
        sort_field: sort_field_from_query(sort_by, default_sort),
        sort_direction: sort_direction_from_query(sort_order, default_direction),
    }
}

fn scalar_filter_from_query(query: &ItemsQuery) -> ItemScalarFilter {
    ItemScalarFilter {
        include_ids: id_list_filter(query.ids.as_deref()),
        exclude_ids: id_list_filter(query.exclude_item_ids.as_deref()),
        years: year_list_filter(query.years.as_deref()),
        search_term: normalized_text_filter(query.search_term.as_deref()),
        name_starts_with: normalized_text_filter(query.name_starts_with.as_deref()),
        name_starts_with_or_greater: normalized_text_filter(
            query.name_starts_with_or_greater.as_deref(),
        ),
        name_less_than: normalized_text_filter(query.name_less_than.as_deref()),
    }
}

fn association_filter_from_query(query: &ItemsQuery) -> ItemAssociationFilter {
    ItemAssociationFilter {
        genre_names: pipe_name_list_filter(query.genres.as_deref()),
        genre_ids: id_list_filter(query.genre_ids.as_deref()),
        official_ratings: pipe_name_list_filter(query.official_ratings.as_deref()),
        tag_names: pipe_name_list_filter(query.tags.as_deref()),
        exclude_tag_names: pipe_name_list_filter(query.exclude_tags.as_deref()),
        studio_names: pipe_name_list_filter(query.studios.as_deref()),
        studio_ids: id_list_filter(query.studio_ids.as_deref()),
        person_names: single_name_filter(query.person.as_deref()),
        person_ids: id_list_filter(query.person_ids.as_deref()),
        person_role_types: person_role_filter(query.person_types.as_deref()),
        artist_names: pipe_name_list_filter(query.artists.as_deref()),
        artist_ids: id_list_filter(query.artist_ids.as_deref()),
        album_names: pipe_name_list_filter(query.albums.as_deref()),
        album_ids: id_list_filter(query.album_ids.as_deref()),
    }
}

fn user_data_filter_from_query(query: &ItemsQuery) -> ItemUserDataFilter {
    let mut filter = ItemUserDataFilter {
        is_played: query.is_played,
        is_favorite: query.is_favorite,
        ..ItemUserDataFilter::default()
    };

    for token in query
        .filters
        .as_deref()
        .into_iter()
        .flat_map(|filters| filters.split(','))
        .map(str::trim)
        .filter(|token| !token.is_empty())
    {
        match token.to_ascii_lowercase().as_str() {
            "isplayed" => filter.require_played = true,
            "isunplayed" => filter.require_unplayed = true,
            "isfavorite" => filter.require_favorite = true,
            "isresumable" => filter.require_resumable = true,
            "likes" => filter.require_likes = true,
            "dislikes" => filter.require_dislikes = true,
            _ => {}
        }
    }

    filter
}

fn structure_filter_from_query(query: &ItemsQuery) -> ItemStructureFilter {
    let mut filter = ItemStructureFilter {
        is_folder: query.is_folder,
        is_movie: query.is_movie,
        is_series: query.is_series,
        ..ItemStructureFilter::default()
    };

    for token in query
        .filters
        .as_deref()
        .into_iter()
        .flat_map(|filters| filters.split(','))
        .map(str::trim)
        .filter(|token| !token.is_empty())
    {
        match token.to_ascii_lowercase().as_str() {
            "isfolder" => filter.require_folder = true,
            "isnotfolder" => filter.require_not_folder = true,
            _ => {}
        }
    }

    filter
}

fn media_filter_from_query(query: &ItemsQuery) -> ItemMediaFilter {
    ItemMediaFilter {
        media_types: media_type_list_filter(query.media_types.as_deref()),
        containers: container_list_filter(query.containers.as_deref()),
        audio_codecs: codec_list_filter(query.audio_codecs.as_deref()),
        video_codecs: codec_list_filter(query.video_codecs.as_deref()),
        subtitle_codecs: codec_list_filter(query.subtitle_codecs.as_deref()),
    }
}

fn provider_filter_from_query(query: &ItemsQuery) -> ItemProviderFilter {
    ItemProviderFilter {
        any_provider_id_equals: provider_id_list_filter(query.any_provider_id_equals.as_deref()),
    }
}

fn image_filter_from_query(query: &ItemsQuery) -> ItemImageFilter {
    ItemImageFilter {
        image_types: image_type_list_filter(query.image_types.as_deref()),
    }
}

fn image_type_list_filter(value: Option<&str>) -> StringListFilter {
    let Some(value) = value else {
        return StringListFilter::default();
    };
    if value.trim().is_empty() {
        return StringListFilter::default();
    }

    let mut values = Vec::<String>::new();
    for token in value
        .split([',', '|'])
        .map(str::trim)
        .filter(|token| !token.is_empty())
    {
        let Some(image_type) = requested_image_type_from_token(token) else {
            continue;
        };
        for internal_type in image_type.internal_types {
            push_unique_string(&mut values, internal_type);
        }
    }

    StringListFilter::enabled(values)
}

fn provider_id_list_filter(value: Option<&str>) -> StringListFilter {
    let Some(value) = value else {
        return StringListFilter::default();
    };
    if value.trim().is_empty() {
        return StringListFilter::default();
    }

    let mut values = Vec::<String>::new();
    for token in value
        .split(',')
        .map(str::trim)
        .filter(|token| !token.is_empty())
    {
        let Some((provider, external_id)) = token.split_once('.') else {
            continue;
        };
        let provider = provider.trim().to_ascii_lowercase();
        let external_id = external_id.trim().to_ascii_lowercase();
        if provider.is_empty() || external_id.is_empty() {
            continue;
        }

        push_unique_string(&mut values, &format!("{provider}.{external_id}"));
    }

    StringListFilter::enabled(values)
}

pub(super) fn media_type_list_filter(value: Option<&str>) -> StringListFilter {
    let Some(value) = value else {
        return StringListFilter::default();
    };
    if value.trim().is_empty() {
        return StringListFilter::default();
    }

    let mut values = Vec::<String>::new();
    for token in value
        .split([',', '|'])
        .map(str::trim)
        .filter(|token| !token.is_empty())
    {
        match token.to_ascii_lowercase().as_str() {
            "video" | "videos" => push_unique_string(&mut values, "video"),
            "audio" | "music" => push_unique_string(&mut values, "audio"),
            _ => {}
        }
    }

    StringListFilter::enabled(values)
}

fn container_list_filter(value: Option<&str>) -> StringListFilter {
    media_token_list_filter(value, container_aliases)
}

fn codec_list_filter(value: Option<&str>) -> StringListFilter {
    media_token_list_filter(value, codec_aliases)
}

fn media_token_list_filter(
    value: Option<&str>,
    aliases: fn(&str) -> &'static [&'static str],
) -> StringListFilter {
    let Some(value) = value else {
        return StringListFilter::default();
    };
    if value.trim().is_empty() {
        return StringListFilter::default();
    }

    let mut values = Vec::<String>::new();
    for token in value
        .split([',', '|'])
        .map(str::trim)
        .map(|token| token.trim_start_matches('.'))
        .filter(|token| !token.is_empty())
    {
        let normalized = token.to_ascii_lowercase();
        push_unique_string(&mut values, &normalized);
        for alias in aliases(&normalized) {
            push_unique_string(&mut values, alias);
        }
    }

    if values.is_empty() {
        StringListFilter::default()
    } else {
        StringListFilter::enabled(values)
    }
}

fn container_aliases(value: &str) -> &'static [&'static str] {
    match value {
        "mkv" => &["matroska", "webm"],
        "mp4" | "m4v" => &["mov", "m4a", "3gp", "3g2", "mj2"],
        "ts" => &["mpegts"],
        "mpegts" => &["ts"],
        "mpg" | "mpeg" => &["vob"],
        "oga" | "ogv" => &["ogg"],
        _ => &[],
    }
}

fn codec_aliases(value: &str) -> &'static [&'static str] {
    match value {
        "avc" | "avc1" => &["h264"],
        "h265" | "x265" => &["hevc"],
        "hevc" | "hvc1" => &["h265"],
        "e-ac-3" | "eac-3" => &["eac3"],
        "eac3" => &["e-ac-3"],
        "mp3" => &["mp3float"],
        _ => &[],
    }
}

fn push_unique_string(values: &mut Vec<String>, value: &str) {
    if !values.iter().any(|existing| existing == value) {
        values.push(value.to_owned());
    }
}

pub(super) fn pipe_name_list_filter(value: Option<&str>) -> StringListFilter {
    string_list_filter(value, &['|'], true)
}

pub(super) fn id_list_filter(value: Option<&str>) -> StringListFilter {
    string_list_filter(value, &[',', '|'], true)
}

fn year_list_filter(value: Option<&str>) -> IntListFilter {
    let Some(value) = value else {
        return IntListFilter::default();
    };
    if value.trim().is_empty() {
        return IntListFilter::default();
    }

    let mut values = Vec::<i32>::new();
    for token in value
        .split([',', '|'])
        .map(str::trim)
        .filter(|token| !token.is_empty())
    {
        let Ok(year) = token.parse::<i32>() else {
            continue;
        };
        if !values.contains(&year) {
            values.push(year);
        }
    }

    IntListFilter::enabled(values)
}

pub(super) fn normalized_text_filter(value: Option<&str>) -> Option<String> {
    value.and_then(|value| {
        let trimmed = value.trim();
        (!trimmed.is_empty()).then(|| trimmed.to_owned())
    })
}

fn single_name_filter(value: Option<&str>) -> StringListFilter {
    let Some(value) = value else {
        return StringListFilter::default();
    };
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return StringListFilter::default();
    }

    StringListFilter::enabled(vec![trimmed.to_ascii_lowercase()])
}

fn string_list_filter(
    value: Option<&str>,
    separators: &[char],
    lowercase: bool,
) -> StringListFilter {
    let Some(value) = value else {
        return StringListFilter::default();
    };
    if value.trim().is_empty() {
        return StringListFilter::default();
    }

    let mut values = Vec::<String>::new();
    for token in value
        .split(|character| separators.contains(&character))
        .map(str::trim)
        .filter(|token| !token.is_empty())
    {
        let normalized = if lowercase {
            token.to_ascii_lowercase()
        } else {
            token.to_owned()
        };
        if !values.iter().any(|existing| existing == &normalized) {
            values.push(normalized);
        }
    }

    if values.is_empty() {
        StringListFilter::default()
    } else {
        StringListFilter::enabled(values)
    }
}

fn person_role_filter(value: Option<&str>) -> PersonRoleFilter {
    let Some(value) = value else {
        return PersonRoleFilter::default();
    };
    if value.trim().is_empty() {
        return PersonRoleFilter::default();
    }

    let mut role_types = Vec::<String>::new();
    for token in value
        .split(',')
        .map(str::trim)
        .filter(|token| !token.is_empty())
    {
        let Some(role_type) = emby_person_type_to_role(token) else {
            continue;
        };
        if !role_types.iter().any(|existing| existing == role_type) {
            role_types.push(role_type.to_owned());
        }
    }

    PersonRoleFilter::enabled(role_types)
}

fn emby_person_type_to_role(value: &str) -> Option<&'static str> {
    let normalized = value
        .trim()
        .chars()
        .filter(|character| !matches!(character, ' ' | '_' | '-'))
        .flat_map(char::to_lowercase)
        .collect::<String>();

    match normalized.as_str() {
        "actor" => Some("actor"),
        "director" => Some("director"),
        "writer" => Some("writer"),
        "producer" => Some("producer"),
        "composer" => Some("composer"),
        "artist" => Some("artist"),
        "gueststar" => Some("guest_star"),
        _ => None,
    }
}

pub(super) fn include_item_types_filter(value: Option<&str>) -> ItemTypeFilter {
    let Some(value) = value else {
        return ItemTypeFilter::default();
    };
    if value.trim().is_empty() {
        return ItemTypeFilter::default();
    }

    let mut item_types = Vec::new();
    for token in value
        .split(',')
        .map(str::trim)
        .filter(|token| !token.is_empty())
    {
        let Some(item_type) = emby_filter_item_type(token) else {
            continue;
        };
        if !item_types.iter().any(|existing| existing == item_type) {
            item_types.push(item_type.to_owned());
        }
    }

    ItemTypeFilter::enabled(item_types)
}

fn include_item_types_requests_playlists_only(value: Option<&str>) -> bool {
    let Some(value) = value else {
        return false;
    };
    let mut saw_playlist = false;
    let mut saw_other_known_type = false;
    let mut saw_unknown_type = false;

    for token in value
        .split(',')
        .map(str::trim)
        .filter(|token| !token.is_empty())
    {
        if emby_filter_playlist_type(token) {
            saw_playlist = true;
        } else if emby_filter_item_type(token).is_some() {
            saw_other_known_type = true;
        } else {
            saw_unknown_type = true;
        }
    }

    saw_playlist && !saw_other_known_type && !saw_unknown_type
}

fn emby_filter_item_type(item_type: &str) -> Option<&'static str> {
    match item_type.to_ascii_lowercase().as_str() {
        "movie" => Some("movie"),
        "series" => Some("series"),
        "season" => Some("season"),
        "episode" => Some("episode"),
        "audio" | "musictrack" | "track" => Some("track"),
        "musicalbum" | "album" => Some("album"),
        "musicartist" | "artist" => Some("artist"),
        "boxset" | "collection" => Some("collection"),
        "folder" | "collectionfolder" => Some("folder"),
        _ => None,
    }
}

fn emby_filter_playlist_type(item_type: &str) -> bool {
    matches!(
        item_type
            .trim()
            .chars()
            .filter(|character| !matches!(character, ' ' | '_' | '-'))
            .flat_map(char::to_lowercase)
            .collect::<String>()
            .as_str(),
        "playlist" | "playlists" | "audioplaylist"
    )
}

fn sort_field_from_query(value: Option<&str>, default_sort: ItemSortField) -> ItemSortField {
    value
        .into_iter()
        .flat_map(|value| value.split(','))
        .map(str::trim)
        .find_map(emby_sort_field)
        .unwrap_or(default_sort)
}

fn emby_sort_field(sort_by: &str) -> Option<ItemSortField> {
    match sort_by.to_ascii_lowercase().as_str() {
        "name" | "sortname" => Some(ItemSortField::SortName),
        "datecreated" | "dateadded" => Some(ItemSortField::DateCreated),
        "runtime" | "runtimeticks" => Some(ItemSortField::Runtime),
        "productionyear" => Some(ItemSortField::ProductionYear),
        "indexnumber" | "parentindexnumber" => Some(ItemSortField::IndexNumber),
        _ => None,
    }
}

fn sort_direction_from_query(
    value: Option<&str>,
    default_direction: SortDirection,
) -> SortDirection {
    match value.map(str::trim) {
        Some(value) if value.eq_ignore_ascii_case("descending") => SortDirection::Desc,
        Some(value) if value.eq_ignore_ascii_case("desc") => SortDirection::Desc,
        Some(value) if value.eq_ignore_ascii_case("ascending") => SortDirection::Asc,
        Some(value) if value.eq_ignore_ascii_case("asc") => SortDirection::Asc,
        Some(_) | None => default_direction,
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(super) struct RequestedItemFields {
    pub media_sources: bool,
    pub media_streams: bool,
    pub chapters: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct RequestedImageType {
    output_key: &'static str,
    internal_types: &'static [&'static str],
    is_backdrop: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct RequestedItemImages {
    enabled: bool,
    image_types: Vec<RequestedImageType>,
    limit: usize,
}

impl RequestedItemImages {
    fn disabled() -> Self {
        Self {
            enabled: false,
            image_types: Vec::new(),
            limit: 0,
        }
    }
}

pub(super) fn requested_item_fields(fields: Option<&str>) -> RequestedItemFields {
    let mut requested = RequestedItemFields::default();
    for field in fields
        .into_iter()
        .flat_map(|fields| fields.split(','))
        .map(str::trim)
    {
        if field.eq_ignore_ascii_case("MediaSources") {
            requested.media_sources = true;
        }
        if field.eq_ignore_ascii_case("MediaStreams") {
            requested.media_streams = true;
        }
        if field.eq_ignore_ascii_case("Chapters") {
            requested.chapters = true;
        }
    }
    requested
}

fn requested_item_images(query: &ItemsQuery) -> RequestedItemImages {
    if query.enable_images == Some(false) {
        return RequestedItemImages::disabled();
    }

    let enable_image_types = query.enable_image_types.as_deref();
    let enabled = query.enable_images.unwrap_or(false)
        || enable_image_types.is_some_and(|value| !value.trim().is_empty())
        || query.image_type_limit.is_some();
    if !enabled {
        return RequestedItemImages::disabled();
    }

    let limit = query
        .image_type_limit
        .map(|limit| limit.clamp(1, MAX_IMAGE_TYPE_LIMIT as u32) as usize)
        .unwrap_or(DEFAULT_IMAGE_TYPE_LIMIT);
    let mut image_types = Vec::<RequestedImageType>::new();

    if let Some(value) = enable_image_types.filter(|value| !value.trim().is_empty()) {
        for token in value
            .split([',', '|'])
            .map(str::trim)
            .filter(|token| !token.is_empty())
        {
            if let Some(image_type) = requested_image_type_from_token(token) {
                push_unique_image_type(&mut image_types, image_type);
            }
        }
    } else {
        for image_type in ["Primary", "Backdrop", "Logo", "Thumb", "Banner"]
            .into_iter()
            .filter_map(requested_image_type_from_token)
        {
            push_unique_image_type(&mut image_types, image_type);
        }
    }

    RequestedItemImages {
        enabled: true,
        image_types,
        limit,
    }
}

fn requested_image_type_from_token(token: &str) -> Option<RequestedImageType> {
    let image_type = match token.trim().to_ascii_lowercase().as_str() {
        "primary" => RequestedImageType {
            output_key: "Primary",
            internal_types: PRIMARY_IMAGE_TYPES,
            is_backdrop: false,
        },
        "poster" => RequestedImageType {
            output_key: "Primary",
            internal_types: POSTER_IMAGE_TYPES,
            is_backdrop: false,
        },
        "backdrop" | "background" => RequestedImageType {
            output_key: "Backdrop",
            internal_types: BACKDROP_IMAGE_TYPES,
            is_backdrop: true,
        },
        "logo" => RequestedImageType {
            output_key: "Logo",
            internal_types: LOGO_IMAGE_TYPES,
            is_backdrop: false,
        },
        "thumb" | "thumbnail" => RequestedImageType {
            output_key: "Thumb",
            internal_types: THUMB_IMAGE_TYPES,
            is_backdrop: false,
        },
        "banner" => RequestedImageType {
            output_key: "Banner",
            internal_types: BANNER_IMAGE_TYPES,
            is_backdrop: false,
        },
        "disc" | "discart" => RequestedImageType {
            output_key: "Disc",
            internal_types: DISC_IMAGE_TYPES,
            is_backdrop: false,
        },
        "art" => RequestedImageType {
            output_key: "Art",
            internal_types: ART_IMAGE_TYPES,
            is_backdrop: false,
        },
        _ => return None,
    };

    Some(image_type)
}

fn push_unique_image_type(values: &mut Vec<RequestedImageType>, value: RequestedImageType) {
    if !values
        .iter()
        .any(|existing| existing.output_key == value.output_key)
    {
        values.push(value);
    }
}

fn should_return_library_views(
    parent_id: Option<&str>,
    recursive: bool,
    options: &ItemQueryOptions,
) -> bool {
    parent_id.is_none()
        && !recursive
        && !options.type_filter.enabled
        && !options.scalar_filter.has_any_filter()
        && !options.user_data_filter.has_any_filter()
        && !options.structure_filter.has_any_filter()
        && !options.media_filter.has_any_filter()
        && !options.provider_filter.has_any_filter()
        && !options.image_filter.has_any_filter()
        && !options.association_filter.has_any_filter()
}

fn library_view_to_base_item(record: UserLibraryViewRecord) -> BaseItemDto {
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

fn root_folder_item() -> BaseItemDto {
    BaseItemDto::from(BaseItemSource {
        id: "root".to_owned(),
        name: "Media Library".to_owned(),
        item_type: "Folder".to_owned(),
        media_type: None,
        parent_id: None,
        is_folder: true,
        run_time_ticks: None,
        production_year: None,
    })
}

pub(super) fn media_item_to_base_item(record: MediaItemBrowseRecord) -> BaseItemDto {
    let requested_images = RequestedItemImages::disabled();
    media_item_to_base_item_with_images(record, &requested_images)
}

fn search_hint_from_item(item: BaseItemDto) -> SearchHintDto {
    SearchHintDto {
        item_id: item.id.clone(),
        id: item.id,
        name: item.name,
        item_type: item.item_type,
        media_type: item.media_type,
        parent_id: item.parent_id,
        is_folder: item.is_folder,
        run_time_ticks: item.run_time_ticks,
        production_year: item.production_year,
    }
}

fn media_item_to_base_item_with_images(
    record: MediaItemBrowseRecord,
    requested_images: &RequestedItemImages,
) -> BaseItemDto {
    let item_type = emby_item_type(&record.item_type).to_owned();
    let media_source = media_item_source_summary(&record);
    let user_data = UserItemDataDto {
        rating: record.rating,
        playback_position_ticks: record.playback_position_ticks,
        play_count: record.play_count,
        is_favorite: record.is_favorite,
        played: record.played,
        item_id: Some(record.id.clone()),
    };
    let mut item = BaseItemDto::from(BaseItemSource {
        id: record.id,
        name: record.name,
        item_type,
        media_type: media_type(&record.item_type).map(str::to_owned),
        parent_id: record.parent_id,
        is_folder: is_folder(&record.item_type),
        run_time_ticks: record.run_time_ticks,
        production_year: record.production_year,
    });
    item.user_data = Some(user_data);
    item.size = record.media_file_size;
    item.container = record.media_file_container.clone();
    item.bitrate = record.media_file_bitrate;
    item.media_sources = media_source.into_iter().collect();
    apply_requested_image_tags(&mut item, &record.image_tags, requested_images);
    item
}

fn apply_requested_image_tags(
    item: &mut BaseItemDto,
    raw_tags: &[String],
    requested_images: &RequestedItemImages,
) {
    if !requested_images.enabled || requested_images.limit == 0 {
        return;
    }

    for image_type in &requested_images.image_types {
        let mut added = 0usize;
        for internal_type in image_type.internal_types {
            for raw_tag in raw_tags {
                let Some((artwork_type, tag)) = raw_tag.split_once('=') else {
                    continue;
                };
                if artwork_type != *internal_type || tag.trim().is_empty() {
                    continue;
                }

                if image_type.is_backdrop {
                    if !item
                        .backdrop_image_tags
                        .iter()
                        .any(|existing| existing == tag)
                    {
                        item.backdrop_image_tags.push(tag.to_owned());
                        added += 1;
                    }
                } else if !item.image_tags.contains_key(image_type.output_key) {
                    item.image_tags
                        .insert(image_type.output_key.to_owned(), tag.to_owned());
                    added += 1;
                }

                if added >= requested_images.limit {
                    break;
                }
            }

            if added >= requested_images.limit {
                break;
            }
        }
    }
}

fn media_item_source_summary(record: &MediaItemBrowseRecord) -> Option<MediaSourceDto> {
    let media_file_id = record.media_file_id?;
    Some(MediaSourceDto {
        id: media_file_id.to_string(),
        source_type: "Default".to_owned(),
        name: media_file_id.to_string(),
        item_id: Some(record.id.clone()),
        path: None,
        protocol: media_source_summary_protocol(record).to_owned(),
        is_remote: record.media_file_is_strm == Some(true),
        requires_opening: false,
        requires_closing: false,
        supports_probing: false,
        read_at_native_framerate: false,
        container: record.media_file_container.clone(),
        run_time_ticks: record.run_time_ticks,
        size: record.media_file_size,
        bitrate: record.media_file_bitrate,
        media_streams: Vec::new(),
        default_audio_stream_index: None,
        default_subtitle_stream_index: None,
        supports_direct_play: true,
        supports_direct_stream: true,
        supports_transcoding: record.supports_transcoding,
        direct_stream_url: None,
        add_api_key_to_direct_stream_url: false,
        transcoding_url: None,
        transcoding_sub_protocol: None,
        transcoding_container: None,
        chapters: Vec::new(),
    })
}

fn media_source_summary_protocol(record: &MediaItemBrowseRecord) -> &'static str {
    if record.media_file_is_strm == Some(true) {
        "Http"
    } else {
        "File"
    }
}

fn emby_item_type(item_type: &str) -> &'static str {
    match item_type {
        "movie" => "Movie",
        "series" => "Series",
        "season" => "Season",
        "episode" => "Episode",
        "artist" => "MusicArtist",
        "album" => "MusicAlbum",
        "track" => "Audio",
        "collection" => "BoxSet",
        _ => "Folder",
    }
}

fn media_type(item_type: &str) -> Option<&'static str> {
    match item_type {
        "movie" | "series" | "season" | "episode" => Some("Video"),
        "artist" | "album" | "track" => Some("Audio"),
        _ => None,
    }
}

fn is_folder(item_type: &str) -> bool {
    matches!(
        item_type,
        "folder" | "series" | "season" | "artist" | "album" | "collection"
    )
}

#[cfg(test)]
mod tests {
    use axum::{extract::Query, http::Uri};
    use serde_json::json;

    use super::*;

    #[test]
    fn item_window_clamps_large_limits() {
        let window = ItemWindow::from_query(&ItemsQuery {
            start_index: Some(20),
            limit: Some(10_000),
            recursive: None,
            parent_id: None,
            include_item_types: None,
            any_provider_id_equals: None,
            sort_by: None,
            sort_order: None,
            fields: None,
            filters: None,
            is_played: None,
            is_favorite: None,
            is_folder: None,
            is_movie: None,
            is_series: None,
            ids: None,
            exclude_item_ids: None,
            search_term: None,
            years: None,
            name_starts_with: None,
            name_starts_with_or_greater: None,
            name_less_than: None,
            genres: None,
            genre_ids: None,
            official_ratings: None,
            tags: None,
            exclude_tags: None,
            studios: None,
            studio_ids: None,
            person: None,
            person_ids: None,
            person_types: None,
            artists: None,
            artist_ids: None,
            albums: None,
            album_ids: None,
            media_types: None,
            containers: None,
            audio_codecs: None,
            video_codecs: None,
            subtitle_codecs: None,
            image_types: None,
            enable_images: None,
            image_type_limit: None,
            enable_image_types: None,
        });

        assert_eq!(window.start_index, 20);
        assert_eq!(window.limit, i64::from(MAX_ITEMS_LIMIT));
    }

    #[test]
    fn item_related_windows_clamp_pathologically_large_start_index() {
        let item_window = ItemWindow::from_query(&ItemsQuery {
            start_index: Some(500_000),
            limit: Some(50),
            ..ItemsQuery::default()
        });
        let media_window = ItemWindow::from_media_query(&MediaListQuery {
            start_index: Some(500_000),
            limit: Some(50),
            ..MediaListQuery::default()
        });
        let similar_window = ItemWindow::from_similar_query(&SimilarItemsQuery {
            start_index: Some(500_000),
            limit: Some(50),
            ..SimilarItemsQuery::default()
        });
        let search_window = SearchHintsWindow::from_query(&SearchHintsQuery {
            start_index: Some(500_000),
            limit: Some(25),
            ..SearchHintsQuery::default()
        });
        let critic_window = CriticReviewsWindow::from_query(&CriticReviewsQuery {
            start_index: Some(500_000),
            limit: Some(50),
            ..CriticReviewsQuery::default()
        });
        let trailers_window = TrailersWindow::from_query(&TrailersQuery {
            start_index: Some(500_000),
            limit: Some(50),
            ..TrailersQuery::default()
        });

        assert_eq!(item_window.start_index, 10_000);
        assert_eq!(media_window.start_index, 10_000);
        assert_eq!(similar_window.start_index, 10_000);
        assert_eq!(search_window.start_index, 10_000);
        assert_eq!(critic_window.start_index, 10_000);
        assert_eq!(trailers_window.start_index, 10_000);
    }

    #[test]
    fn item_empty_result_windows_clamp_pathologically_large_start_index() {
        assert_eq!(clamped_start_index_u32(Some(500_000)), 10_000);
        assert_eq!(clamped_start_index_u32(Some(25)), 25);
        assert_eq!(clamped_start_index_u32(None), 0);
    }

    #[test]
    fn similar_item_window_clamps_large_limits() {
        let window = ItemWindow::from_similar_query(&SimilarItemsQuery {
            user_id: Some("user-1".to_owned()),
            start_index: Some(5),
            limit: Some(10_000),
            include_item_types: None,
            sort_by: None,
            sort_order: None,
            fields: None,
        });

        assert_eq!(window.start_index, 5);
        assert_eq!(window.limit, i64::from(MAX_ITEMS_LIMIT));
    }

    #[test]
    fn include_item_types_filter_maps_emby_types() {
        let options = item_query_options(
            Some("Movie,Episode,Audio,Movie"),
            None,
            None,
            ItemSortField::SortName,
            SortDirection::Asc,
        );

        assert!(options.type_filter.enabled);
        assert_eq!(
            options.type_filter.item_types,
            ["movie", "episode", "track"]
        );
    }

    #[test]
    fn include_item_types_filter_keeps_unknown_only_filter_empty() {
        let options = item_query_options(
            Some("DefinitelyNotAType"),
            None,
            None,
            ItemSortField::SortName,
            SortDirection::Asc,
        );

        assert!(options.type_filter.enabled);
        assert!(options.type_filter.item_types.is_empty());
    }

    #[test]
    fn include_item_types_playlist_only_uses_collection_playlists() {
        assert!(include_item_types_requests_playlists_only(Some("Playlist")));
        assert!(include_item_types_requests_playlists_only(Some(
            " audio-playlist "
        )));
        assert!(!include_item_types_requests_playlists_only(Some(
            "Playlist,Audio"
        )));
        assert!(!include_item_types_requests_playlists_only(Some(
            "Playlist,Unknown"
        )));
        assert!(!include_item_types_requests_playlists_only(None));
    }

    #[test]
    fn music_item_queries_map_to_existing_item_filters() {
        let query = MusicItemsQuery {
            user_id: Some("user-1".to_owned()),
            parent_id: Some("library-1".to_owned()),
            start_index: Some(10),
            limit: Some(25),
            sort_by: Some("SortName".to_owned()),
            sort_order: Some("Ascending".to_owned()),
            fields: Some("PrimaryImageAspectRatio".to_owned()),
            search_term: Some("blue".to_owned()),
            years: Some("1999,2000".to_owned()),
            genres: Some("Jazz".to_owned()),
            genre_ids: Some("genre-1".to_owned()),
            artists: Some("Artist A".to_owned()),
            artist_ids: Some("artist-1".to_owned()),
            albums: Some("Album A".to_owned()),
            album_ids: Some("album-1".to_owned()),
            enable_images: Some(true),
            image_type_limit: Some(2),
            enable_image_types: Some("Primary,Backdrop".to_owned()),
        };

        let albums = music_items_query(query.clone(), "MusicAlbum", None);
        assert_eq!(albums.parent_id.as_deref(), Some("library-1"));
        assert_eq!(albums.start_index, Some(10));
        assert_eq!(albums.limit, Some(25));
        assert_eq!(albums.recursive, Some(true));
        assert_eq!(albums.include_item_types.as_deref(), Some("MusicAlbum"));
        assert_eq!(albums.media_types, None);
        assert_eq!(albums.search_term.as_deref(), Some("blue"));
        assert_eq!(albums.genres.as_deref(), Some("Jazz"));
        assert_eq!(albums.artist_ids.as_deref(), Some("artist-1"));
        assert_eq!(albums.albums.as_deref(), Some("Album A"));
        assert_eq!(albums.album_ids.as_deref(), Some("album-1"));
        assert_eq!(albums.enable_images, Some(true));
        assert_eq!(albums.image_type_limit, Some(2));

        let songs = music_items_query(query, "Audio", Some("Audio"));
        assert_eq!(songs.include_item_types.as_deref(), Some("Audio"));
        assert_eq!(songs.media_types.as_deref(), Some("Audio"));
        assert_eq!(songs.recursive, Some(true));
    }

    #[test]
    fn album_path_songs_query_uses_path_album_id_over_query_filters() {
        let query = MusicItemsQuery {
            start_index: Some(5),
            limit: Some(25),
            albums: Some("Client Album".to_owned()),
            album_ids: Some("query-album-id".to_owned()),
            artists: Some("Artist A".to_owned()),
            ..MusicItemsQuery::default()
        };

        let items_query = album_path_songs_query(" path-album-id ", query)
            .expect("non-empty album path id yields a songs query");

        assert_eq!(items_query.include_item_types.as_deref(), Some("Audio"));
        assert_eq!(items_query.media_types.as_deref(), Some("Audio"));
        assert_eq!(items_query.recursive, Some(true));
        assert_eq!(items_query.album_ids.as_deref(), Some("path-album-id"));
        assert_eq!(items_query.albums, None);
        assert_eq!(items_query.artists.as_deref(), Some("Artist A"));
        assert_eq!(items_query.start_index, Some(5));
        assert_eq!(items_query.limit, Some(25));
    }

    #[test]
    fn artist_path_songs_query_uses_path_artist_id_over_query_filters() {
        let query = MusicItemsQuery {
            start_index: Some(5),
            limit: Some(25),
            artists: Some("Client Artist".to_owned()),
            artist_ids: Some("query-artist-id".to_owned()),
            albums: Some("Album A".to_owned()),
            ..MusicItemsQuery::default()
        };

        let items_query =
            artist_path_music_query(" path-artist-id ", query, "Audio", Some("Audio"))
                .expect("non-empty artist path id yields a songs query");

        assert_eq!(items_query.include_item_types.as_deref(), Some("Audio"));
        assert_eq!(items_query.media_types.as_deref(), Some("Audio"));
        assert_eq!(items_query.recursive, Some(true));
        assert_eq!(items_query.artist_ids.as_deref(), Some("path-artist-id"));
        assert_eq!(items_query.artists, None);
        assert_eq!(items_query.albums.as_deref(), Some("Album A"));
        assert_eq!(items_query.start_index, Some(5));
        assert_eq!(items_query.limit, Some(25));
    }

    #[test]
    fn artist_path_albums_query_uses_path_artist_id_over_query_filters() {
        let query = MusicItemsQuery {
            limit: Some(25),
            artists: Some("Client Artist".to_owned()),
            artist_ids: Some("query-artist-id".to_owned()),
            genres: Some("Jazz".to_owned()),
            ..MusicItemsQuery::default()
        };

        let items_query = artist_path_music_query(" path-artist-id ", query, "MusicAlbum", None)
            .expect("non-empty artist path id yields an albums query");

        assert_eq!(
            items_query.include_item_types.as_deref(),
            Some("MusicAlbum")
        );
        assert_eq!(items_query.media_types, None);
        assert_eq!(items_query.recursive, Some(true));
        assert_eq!(items_query.artist_ids.as_deref(), Some("path-artist-id"));
        assert_eq!(items_query.artists, None);
        assert_eq!(items_query.genres.as_deref(), Some("Jazz"));
        assert_eq!(items_query.limit, Some(25));
    }

    #[test]
    fn music_genre_instant_mix_seeds_audio_query_from_path_genre() {
        let query = MusicItemsQuery {
            user_id: Some("user-1".to_owned()),
            limit: Some(50),
            // A client-supplied genre filter must be overridden by the path seed.
            genres: Some("Rock".to_owned()),
            genre_ids: Some("genre-9".to_owned()),
            ..MusicItemsQuery::default()
        };

        let items_query = music_genre_instant_mix_query("  Jazz  ", query)
            .expect("non-empty genre seed yields a query");
        assert_eq!(items_query.include_item_types.as_deref(), Some("Audio"));
        assert_eq!(items_query.media_types.as_deref(), Some("Audio"));
        assert_eq!(items_query.recursive, Some(true));
        assert_eq!(items_query.genres.as_deref(), Some("Jazz"));
        assert_eq!(items_query.genre_ids, None);
        assert_eq!(items_query.limit, Some(50));
    }

    #[test]
    fn music_genre_instant_mix_rejects_blank_genre_seed() {
        assert!(music_genre_instant_mix_query("   ", MusicItemsQuery::default()).is_none());
        assert!(music_genre_instant_mix_query("", MusicItemsQuery::default()).is_none());
    }

    #[test]
    fn artist_seed_instant_mix_filters_audio_by_artist_id() {
        let query = InstantMixByIdQuery {
            id: Some("11111111-2222-3333-4444-555555555555".to_owned()),
            limit: Some(30),
            ..InstantMixByIdQuery::default()
        };

        let items_query =
            instant_mix_by_id_query(MusicSeed::Artist, query).expect("artist seed yields a query");
        assert_eq!(items_query.include_item_types.as_deref(), Some("Audio"));
        assert_eq!(items_query.media_types.as_deref(), Some("Audio"));
        assert_eq!(items_query.recursive, Some(true));
        assert_eq!(
            items_query.artist_ids.as_deref(),
            Some("11111111-2222-3333-4444-555555555555")
        );
        assert_eq!(items_query.genre_ids, None);
        assert_eq!(items_query.limit, Some(30));
    }

    #[test]
    fn artist_path_seed_instant_mix_uses_path_id_over_query_id() {
        let query = InstantMixByIdQuery {
            id: Some("query-artist-id".to_owned()),
            start_index: Some(10),
            limit: Some(30),
            ..InstantMixByIdQuery::default()
        };

        let items_query =
            instant_mix_by_path_id_query(MusicSeed::Artist, " path-artist-id ", query)
                .expect("artist path seed yields a query");

        assert_eq!(items_query.include_item_types.as_deref(), Some("Audio"));
        assert_eq!(items_query.media_types.as_deref(), Some("Audio"));
        assert_eq!(items_query.recursive, Some(true));
        assert_eq!(items_query.artist_ids.as_deref(), Some("path-artist-id"));
        assert_eq!(items_query.genre_ids, None);
        assert_eq!(items_query.start_index, Some(10));
        assert_eq!(items_query.limit, Some(30));
    }

    #[test]
    fn genre_id_seed_instant_mix_filters_audio_by_genre_id() {
        let query = InstantMixByIdQuery {
            id: Some("  42  ".to_owned()),
            ..InstantMixByIdQuery::default()
        };

        let items_query =
            instant_mix_by_id_query(MusicSeed::Genre, query).expect("genre seed yields a query");
        assert_eq!(items_query.include_item_types.as_deref(), Some("Audio"));
        assert_eq!(items_query.genre_ids.as_deref(), Some("42"));
        assert_eq!(items_query.artist_ids, None);
    }

    #[test]
    fn bare_instant_mix_rejects_missing_or_blank_seed_id() {
        assert!(
            instant_mix_by_id_query(MusicSeed::Artist, InstantMixByIdQuery::default()).is_none()
        );
        assert!(
            instant_mix_by_id_query(
                MusicSeed::Genre,
                InstantMixByIdQuery {
                    id: Some("   ".to_owned()),
                    ..InstantMixByIdQuery::default()
                }
            )
            .is_none()
        );
    }

    #[test]
    fn item_seed_instant_mix_filters_audio_by_seed_genres_and_excludes_seed() {
        let query = ItemInstantMixQuery {
            user_id: Some("user-1".to_owned()),
            start_index: Some(5),
            limit: Some(40),
            ..ItemInstantMixQuery::default()
        };

        let items_query =
            item_instant_mix_query("11111111-2222-3333-4444-555555555555", &[7, 42], query)
                .expect("seed with genres yields a query");
        assert_eq!(items_query.include_item_types.as_deref(), Some("Audio"));
        assert_eq!(items_query.media_types.as_deref(), Some("Audio"));
        assert_eq!(items_query.recursive, Some(true));
        // Seed genre ids become the authoritative genre filter (comma-joined so
        // id_list_filter splits them back into a numeric id set).
        assert_eq!(items_query.genre_ids.as_deref(), Some("7,42"));
        // The seed item is excluded from its own mix.
        assert_eq!(
            items_query.exclude_item_ids.as_deref(),
            Some("11111111-2222-3333-4444-555555555555")
        );
        assert_eq!(items_query.start_index, Some(5));
        assert_eq!(items_query.limit, Some(40));
    }

    #[test]
    fn item_seed_instant_mix_returns_no_query_when_seed_has_no_genres() {
        assert!(
            item_instant_mix_query(
                "11111111-2222-3333-4444-555555555555",
                &[],
                ItemInstantMixQuery::default(),
            )
            .is_none()
        );
    }

    #[test]
    fn search_hints_query_maps_to_recursive_items_query() {
        let query = SearchHintsQuery {
            user_id: Some("user-1".to_owned()),
            parent_id: Some("library-1".to_owned()),
            search_term: Some(" alien ".to_owned()),
            include_item_types: Some("Movie,Series".to_owned()),
            media_types: Some("Video".to_owned()),
            start_index: Some(5),
            limit: Some(500),
        };

        let items_query = search_hints_items_query(query.clone());
        let window = SearchHintsWindow::from_query(&query);

        assert_eq!(items_query.parent_id.as_deref(), Some("library-1"));
        assert_eq!(items_query.search_term.as_deref(), Some(" alien "));
        assert_eq!(
            items_query.include_item_types.as_deref(),
            Some("Movie,Series")
        );
        assert_eq!(items_query.media_types.as_deref(), Some("Video"));
        assert_eq!(items_query.recursive, Some(true));
        assert_eq!(window.start_index, 5);
        assert_eq!(window.limit, i64::from(MAX_SEARCH_HINTS_LIMIT));
    }

    #[test]
    fn trailers_query_is_bounded_and_normalized() {
        let query = TrailersQuery {
            user_id: Some("user-1".to_owned()),
            parent_id: Some(" library-1 ".to_owned()),
            start_index: Some(7),
            limit: Some(10_000),
            recursive: Some(false),
            search_term: Some(" trailer ".to_owned()),
            sort_by: Some("DateCreated".to_owned()),
            sort_order: Some("Descending".to_owned()),
            fields: Some("MediaSources,PrimaryImageAspectRatio".to_owned()),
            include_item_types: Some("Movie,Trailer".to_owned()),
            media_types: Some("Video".to_owned()),
            enable_images: Some(true),
            image_type_limit: Some(2),
            enable_image_types: Some("Primary,Backdrop".to_owned()),
        };

        let window = TrailersWindow::from_query(&query);
        let input = trailers_query_input(&query, window);

        assert_eq!(input.start_index, 7);
        assert_eq!(input.limit, MAX_ITEMS_LIMIT as usize);
        assert_eq!(input.parent_id.as_deref(), Some("library-1"));
        assert!(!input.recursive);
        assert_eq!(input.search_term.as_deref(), Some("trailer"));
        assert!(input.include_item_types.enabled);
        assert_eq!(input.include_item_types.item_types, ["movie"]);
        assert_eq!(input.media_types.values, ["video"]);
    }

    #[test]
    fn trailers_response_is_empty_query_result_until_provider_exists() {
        let result = empty_trailers_result(&TrailersQuery {
            start_index: Some(12),
            limit: Some(5),
            ..TrailersQuery::default()
        });

        assert!(result.items.is_empty());
        assert_eq!(result.total_record_count, 0);
        assert_eq!(result.start_index, 12);
    }

    #[test]
    fn search_hint_response_uses_emby_legacy_shape() {
        let item = BaseItemDto::from(BaseItemSource {
            id: "item-1".to_owned(),
            name: "Alien".to_owned(),
            item_type: "Movie".to_owned(),
            media_type: Some("Video".to_owned()),
            parent_id: Some("library-1".to_owned()),
            is_folder: false,
            run_time_ticks: Some(7_000_000),
            production_year: Some(1979),
        });

        let hint = search_hint_from_item(item);
        let result = SearchHintsResultDto {
            search_hints: vec![hint],
            total_record_count: 1,
        };

        assert_eq!(
            serde_json::to_value(result).unwrap(),
            serde_json::json!({
                "SearchHints": [{
                    "ItemId": "item-1",
                    "Id": "item-1",
                    "Name": "Alien",
                    "Type": "Movie",
                    "MediaType": "Video",
                    "ParentId": "library-1",
                    "IsFolder": false,
                    "RunTimeTicks": 7000000,
                    "ProductionYear": 1979
                }],
                "TotalRecordCount": 1
            })
        );
    }

    #[test]
    fn suggestions_query_maps_to_recent_recursive_item_browse() {
        let query = suggestions_items_query(SuggestionsQuery {
            parent_id: Some("music-lib".to_owned()),
            start_index: Some(4),
            item_limit: Some(8),
            include_item_types: Some("Movie,Episode".to_owned()),
            media_types: Some("Video".to_owned()),
            fields: Some("PrimaryImageAspectRatio".to_owned()),
            ..SuggestionsQuery::default()
        });

        assert_eq!(query.parent_id.as_deref(), Some("music-lib"));
        assert_eq!(query.start_index, Some(4));
        assert_eq!(query.limit, Some(8));
        assert_eq!(query.recursive, Some(true));
        assert_eq!(query.include_item_types.as_deref(), Some("Movie,Episode"));
        assert_eq!(query.media_types.as_deref(), Some("Video"));
        assert_eq!(query.sort_by.as_deref(), Some("DateCreated"));
        assert_eq!(query.sort_order.as_deref(), Some("Descending"));
        assert_eq!(query.fields.as_deref(), Some("PrimaryImageAspectRatio"));
    }

    #[test]
    fn movie_recommendations_query_maps_to_recent_movie_window() {
        let query = movie_recommendations_items_query(MovieRecommendationsQuery {
            user_id: Some("user-1".to_owned()),
            parent_id: Some("movies-lib".to_owned()),
            category_limit: Some(3),
            item_limit: Some(9),
            enable_images: Some(true),
            enable_user_data: Some(true),
            image_type_limit: Some(2),
            enable_image_types: Some("Primary,Backdrop".to_owned()),
        });

        assert_eq!(query.parent_id.as_deref(), Some("movies-lib"));
        assert_eq!(query.limit, Some(9));
        assert_eq!(query.recursive, Some(true));
        assert_eq!(query.include_item_types.as_deref(), Some("Movie"));
        assert_eq!(query.media_types.as_deref(), Some("Video"));
        assert_eq!(query.sort_by.as_deref(), Some("DateCreated"));
        assert_eq!(query.sort_order.as_deref(), Some("Descending"));
        assert_eq!(query.enable_images, Some(true));
        assert_eq!(query.image_type_limit, Some(2));
        assert_eq!(
            query.enable_image_types.as_deref(),
            Some("Primary,Backdrop")
        );
    }

    #[test]
    fn item_helper_queries_accept_lower_camel_client_fields() {
        let uri: Uri = concat!(
            "/emby/Artists/InstantMix?",
            "id=artist-1&userId=user-1&startIndex=1&limit=20",
            "&fields=MediaSources&enableImages=true&imageTypeLimit=2",
            "&enableImageTypes=Primary,Backdrop"
        )
        .parse()
        .unwrap();
        let Query(instant_mix) = Query::<InstantMixByIdQuery>::try_from_uri(&uri).unwrap();

        assert_eq!(instant_mix.id.as_deref(), Some("artist-1"));
        assert_eq!(instant_mix.user_id.as_deref(), Some("user-1"));
        assert_eq!(instant_mix.start_index, Some(1));
        assert_eq!(instant_mix.limit, Some(20));
        assert_eq!(instant_mix.fields.as_deref(), Some("MediaSources"));
        assert_eq!(instant_mix.enable_images, Some(true));
        assert_eq!(instant_mix.image_type_limit, Some(2));
        assert_eq!(
            instant_mix.enable_image_types.as_deref(),
            Some("Primary,Backdrop")
        );

        let uri: Uri = concat!(
            "/emby/Items/item-1/InstantMix?",
            "userId=user-1&startIndex=2&limit=30&fields=MediaSources",
            "&enableImages=true&imageTypeLimit=3&enableImageTypes=Primary"
        )
        .parse()
        .unwrap();
        let Query(item_mix) = Query::<ItemInstantMixQuery>::try_from_uri(&uri).unwrap();

        assert_eq!(item_mix.user_id.as_deref(), Some("user-1"));
        assert_eq!(item_mix.start_index, Some(2));
        assert_eq!(item_mix.limit, Some(30));
        assert_eq!(item_mix.fields.as_deref(), Some("MediaSources"));
        assert_eq!(item_mix.enable_images, Some(true));
        assert_eq!(item_mix.image_type_limit, Some(3));
        assert_eq!(item_mix.enable_image_types.as_deref(), Some("Primary"));

        let uri: Uri = "/emby/Items/item-1/CriticReviews?userId=user-1&startIndex=4&limit=8"
            .parse()
            .unwrap();
        let Query(reviews) = Query::<CriticReviewsQuery>::try_from_uri(&uri).unwrap();

        assert_eq!(reviews.user_id.as_deref(), Some("user-1"));
        assert_eq!(reviews.start_index, Some(4));
        assert_eq!(reviews.limit, Some(8));

        let uri: Uri = concat!(
            "/emby/Videos/item-1/AdditionalParts?",
            "userId=user-1&fields=MediaSources&enableImages=true",
            "&imageTypeLimit=2&enableImageTypes=Primary&enableUserData=false"
        )
        .parse()
        .unwrap();
        let Query(parts) = Query::<AdditionalPartsQuery>::try_from_uri(&uri).unwrap();

        assert_eq!(parts.user_id.as_deref(), Some("user-1"));
        assert_eq!(parts.fields.as_deref(), Some("MediaSources"));
        assert_eq!(parts.enable_images, Some(true));
        assert_eq!(parts.image_type_limit, Some(2));
        assert_eq!(parts.enable_image_types.as_deref(), Some("Primary"));
        assert_eq!(parts.enable_user_data, Some(false));

        let uri: Uri = concat!(
            "/emby/Items/item-1/SpecialFeatures?",
            "userId=user-1&startIndex=5&limit=9&fields=Overview",
            "&enableImages=true&imageTypeLimit=2&enableImageTypes=Primary",
            "&enableUserData=true"
        )
        .parse()
        .unwrap();
        let Query(features) = Query::<SpecialFeaturesQuery>::try_from_uri(&uri).unwrap();

        assert_eq!(features.user_id.as_deref(), Some("user-1"));
        assert_eq!(features.start_index, Some(5));
        assert_eq!(features.limit, Some(9));
        assert_eq!(features.fields.as_deref(), Some("Overview"));
        assert_eq!(features.enable_images, Some(true));
        assert_eq!(features.image_type_limit, Some(2));
        assert_eq!(features.enable_image_types.as_deref(), Some("Primary"));
        assert_eq!(features.enable_user_data, Some(true));

        let uri: Uri = concat!(
            "/emby/Items/item-1/Intros?",
            "userId=user-1&startIndex=6&limit=10&fields=MediaSources",
            "&enableImages=true&imageTypeLimit=2&enableImageTypes=Primary",
            "&enableUserData=false"
        )
        .parse()
        .unwrap();
        let Query(extras) = Query::<PlaybackExtrasQuery>::try_from_uri(&uri).unwrap();

        assert_eq!(extras.user_id.as_deref(), Some("user-1"));
        assert_eq!(extras.start_index, Some(6));
        assert_eq!(extras.limit, Some(10));
        assert_eq!(extras.fields.as_deref(), Some("MediaSources"));
        assert_eq!(extras.enable_images, Some(true));
        assert_eq!(extras.image_type_limit, Some(2));
        assert_eq!(extras.enable_image_types.as_deref(), Some("Primary"));
        assert_eq!(extras.enable_user_data, Some(false));

        let uri: Uri = "/emby/Items/Counts?userId=user-1".parse().unwrap();
        let Query(counts) = Query::<ItemCountsQuery>::try_from_uri(&uri).unwrap();

        assert_eq!(counts.user_id.as_deref(), Some("user-1"));
    }

    #[test]
    fn item_helper_queries_accept_snake_case_client_fields() {
        let uri: Uri = concat!(
            "/emby/Items/item-1/InstantMix?",
            "user_id=user-1&start_index=2&limit=30&fields=MediaSources",
            "&enable_images=true&image_type_limit=3&enable_image_types=Primary"
        )
        .parse()
        .unwrap();
        let Query(item_mix) = Query::<ItemInstantMixQuery>::try_from_uri(&uri).unwrap();

        assert_eq!(item_mix.user_id.as_deref(), Some("user-1"));
        assert_eq!(item_mix.start_index, Some(2));
        assert_eq!(item_mix.limit, Some(30));
        assert_eq!(item_mix.fields.as_deref(), Some("MediaSources"));
        assert_eq!(item_mix.enable_images, Some(true));
        assert_eq!(item_mix.image_type_limit, Some(3));
        assert_eq!(item_mix.enable_image_types.as_deref(), Some("Primary"));

        let uri: Uri = concat!(
            "/emby/Items/item-1/SpecialFeatures?",
            "user_id=user-1&start_index=5&limit=9&fields=Overview",
            "&enable_images=true&image_type_limit=2&enable_image_types=Primary",
            "&enable_user_data=true"
        )
        .parse()
        .unwrap();
        let Query(features) = Query::<SpecialFeaturesQuery>::try_from_uri(&uri).unwrap();

        assert_eq!(features.user_id.as_deref(), Some("user-1"));
        assert_eq!(features.start_index, Some(5));
        assert_eq!(features.limit, Some(9));
        assert_eq!(features.fields.as_deref(), Some("Overview"));
        assert_eq!(features.enable_images, Some(true));
        assert_eq!(features.image_type_limit, Some(2));
        assert_eq!(features.enable_image_types.as_deref(), Some("Primary"));
        assert_eq!(features.enable_user_data, Some(true));

        let uri: Uri = "/emby/Items/Counts?user_id=user-1".parse().unwrap();
        let Query(counts) = Query::<ItemCountsQuery>::try_from_uri(&uri).unwrap();

        assert_eq!(counts.user_id.as_deref(), Some("user-1"));
    }

    #[test]
    fn recommendation_and_similarity_queries_accept_lower_camel_client_fields() {
        let uri: Uri = concat!(
            "/emby/Users/user-1/Suggestions?",
            "parentId=library-1&startIndex=1&limit=20&itemLimit=12",
            "&includeItemTypes=Movie,Episode&mediaTypes=Video",
            "&sortBy=DateCreated&sortOrder=Descending&fields=MediaSources"
        )
        .parse()
        .unwrap();
        let Query(suggestions) = Query::<SuggestionsQuery>::try_from_uri(&uri).unwrap();

        assert_eq!(suggestions.parent_id.as_deref(), Some("library-1"));
        assert_eq!(suggestions.start_index, Some(1));
        assert_eq!(suggestions.limit, Some(20));
        assert_eq!(suggestions.item_limit, Some(12));
        assert_eq!(
            suggestions.include_item_types.as_deref(),
            Some("Movie,Episode")
        );
        assert_eq!(suggestions.media_types.as_deref(), Some("Video"));
        assert_eq!(suggestions.sort_by.as_deref(), Some("DateCreated"));
        assert_eq!(suggestions.sort_order.as_deref(), Some("Descending"));
        assert_eq!(suggestions.fields.as_deref(), Some("MediaSources"));

        let uri: Uri = concat!(
            "/emby/Movies/Recommendations?",
            "categoryLimit=2&itemLimit=8&userId=user-1&parentId=movies",
            "&enableImages=true&enableUserData=false&imageTypeLimit=2",
            "&enableImageTypes=Primary,Backdrop"
        )
        .parse()
        .unwrap();
        let Query(recommendations) =
            Query::<MovieRecommendationsQuery>::try_from_uri(&uri).unwrap();

        assert_eq!(recommendations.category_limit, Some(2));
        assert_eq!(recommendations.item_limit, Some(8));
        assert_eq!(recommendations.user_id.as_deref(), Some("user-1"));
        assert_eq!(recommendations.parent_id.as_deref(), Some("movies"));
        assert_eq!(recommendations.enable_images, Some(true));
        assert_eq!(recommendations.enable_user_data, Some(false));
        assert_eq!(recommendations.image_type_limit, Some(2));
        assert_eq!(
            recommendations.enable_image_types.as_deref(),
            Some("Primary,Backdrop")
        );

        let uri: Uri = concat!(
            "/emby/Trailers?",
            "userId=user-1&parentId=library-1&startIndex=3&limit=11",
            "&recursive=false&searchTerm=signal&sortBy=DateCreated",
            "&sortOrder=Descending&fields=MediaSources",
            "&includeItemTypes=Trailer&mediaTypes=Video",
            "&enableImages=true&imageTypeLimit=2&enableImageTypes=Primary"
        )
        .parse()
        .unwrap();
        let Query(trailers) = Query::<TrailersQuery>::try_from_uri(&uri).unwrap();

        assert_eq!(trailers.user_id.as_deref(), Some("user-1"));
        assert_eq!(trailers.parent_id.as_deref(), Some("library-1"));
        assert_eq!(trailers.start_index, Some(3));
        assert_eq!(trailers.limit, Some(11));
        assert_eq!(trailers.recursive, Some(false));
        assert_eq!(trailers.search_term.as_deref(), Some("signal"));
        assert_eq!(trailers.sort_by.as_deref(), Some("DateCreated"));
        assert_eq!(trailers.sort_order.as_deref(), Some("Descending"));
        assert_eq!(trailers.fields.as_deref(), Some("MediaSources"));
        assert_eq!(trailers.include_item_types.as_deref(), Some("Trailer"));
        assert_eq!(trailers.media_types.as_deref(), Some("Video"));
        assert_eq!(trailers.enable_images, Some(true));
        assert_eq!(trailers.image_type_limit, Some(2));
        assert_eq!(trailers.enable_image_types.as_deref(), Some("Primary"));

        let uri: Uri = concat!(
            "/emby/Items/item-1/Similar?",
            "userId=user-1&startIndex=4&limit=12&includeItemTypes=Movie",
            "&sortBy=SortName&sortOrder=Ascending&fields=MediaSources"
        )
        .parse()
        .unwrap();
        let Query(similar) = Query::<SimilarItemsQuery>::try_from_uri(&uri).unwrap();

        assert_eq!(similar.user_id.as_deref(), Some("user-1"));
        assert_eq!(similar.start_index, Some(4));
        assert_eq!(similar.limit, Some(12));
        assert_eq!(similar.include_item_types.as_deref(), Some("Movie"));
        assert_eq!(similar.sort_by.as_deref(), Some("SortName"));
        assert_eq!(similar.sort_order.as_deref(), Some("Ascending"));
        assert_eq!(similar.fields.as_deref(), Some("MediaSources"));
    }

    #[test]
    fn recommendation_and_similarity_queries_accept_snake_case_client_fields() {
        let uri: Uri = concat!(
            "/emby/Users/user-1/Suggestions?",
            "parent_id=library-1&start_index=1&limit=20&item_limit=12",
            "&include_item_types=Movie,Episode&media_types=Video",
            "&sort_by=DateCreated&sort_order=Descending&fields=MediaSources"
        )
        .parse()
        .unwrap();
        let Query(suggestions) = Query::<SuggestionsQuery>::try_from_uri(&uri).unwrap();

        assert_eq!(suggestions.parent_id.as_deref(), Some("library-1"));
        assert_eq!(suggestions.start_index, Some(1));
        assert_eq!(suggestions.limit, Some(20));
        assert_eq!(suggestions.item_limit, Some(12));
        assert_eq!(
            suggestions.include_item_types.as_deref(),
            Some("Movie,Episode")
        );
        assert_eq!(suggestions.media_types.as_deref(), Some("Video"));
        assert_eq!(suggestions.sort_by.as_deref(), Some("DateCreated"));
        assert_eq!(suggestions.sort_order.as_deref(), Some("Descending"));
        assert_eq!(suggestions.fields.as_deref(), Some("MediaSources"));

        let uri: Uri = concat!(
            "/emby/Movies/Recommendations?",
            "category_limit=2&item_limit=8&user_id=user-1&parent_id=movies",
            "&enable_images=true&enable_user_data=false&image_type_limit=2",
            "&enable_image_types=Primary,Backdrop"
        )
        .parse()
        .unwrap();
        let Query(recommendations) =
            Query::<MovieRecommendationsQuery>::try_from_uri(&uri).unwrap();

        assert_eq!(recommendations.category_limit, Some(2));
        assert_eq!(recommendations.item_limit, Some(8));
        assert_eq!(recommendations.user_id.as_deref(), Some("user-1"));
        assert_eq!(recommendations.parent_id.as_deref(), Some("movies"));
        assert_eq!(recommendations.enable_images, Some(true));
        assert_eq!(recommendations.enable_user_data, Some(false));
        assert_eq!(recommendations.image_type_limit, Some(2));
        assert_eq!(
            recommendations.enable_image_types.as_deref(),
            Some("Primary,Backdrop")
        );

        let uri: Uri = concat!(
            "/emby/Trailers?",
            "user_id=user-1&parent_id=library-1&start_index=3&limit=11",
            "&recursive=false&search_term=signal&sort_by=DateCreated",
            "&sort_order=Descending&fields=MediaSources",
            "&include_item_types=Trailer&media_types=Video",
            "&enable_images=true&image_type_limit=2&enable_image_types=Primary"
        )
        .parse()
        .unwrap();
        let Query(trailers) = Query::<TrailersQuery>::try_from_uri(&uri).unwrap();

        assert_eq!(trailers.user_id.as_deref(), Some("user-1"));
        assert_eq!(trailers.parent_id.as_deref(), Some("library-1"));
        assert_eq!(trailers.start_index, Some(3));
        assert_eq!(trailers.limit, Some(11));
        assert_eq!(trailers.recursive, Some(false));
        assert_eq!(trailers.search_term.as_deref(), Some("signal"));
        assert_eq!(trailers.sort_by.as_deref(), Some("DateCreated"));
        assert_eq!(trailers.sort_order.as_deref(), Some("Descending"));
        assert_eq!(trailers.fields.as_deref(), Some("MediaSources"));
        assert_eq!(trailers.include_item_types.as_deref(), Some("Trailer"));
        assert_eq!(trailers.media_types.as_deref(), Some("Video"));
        assert_eq!(trailers.enable_images, Some(true));
        assert_eq!(trailers.image_type_limit, Some(2));
        assert_eq!(trailers.enable_image_types.as_deref(), Some("Primary"));

        let uri: Uri = concat!(
            "/emby/Items/item-1/Similar?",
            "user_id=user-1&start_index=4&limit=12&include_item_types=Movie",
            "&sort_by=SortName&sort_order=Ascending&fields=MediaSources"
        )
        .parse()
        .unwrap();
        let Query(similar) = Query::<SimilarItemsQuery>::try_from_uri(&uri).unwrap();

        assert_eq!(similar.user_id.as_deref(), Some("user-1"));
        assert_eq!(similar.start_index, Some(4));
        assert_eq!(similar.limit, Some(12));
        assert_eq!(similar.include_item_types.as_deref(), Some("Movie"));
        assert_eq!(similar.sort_by.as_deref(), Some("SortName"));
        assert_eq!(similar.sort_order.as_deref(), Some("Ascending"));
        assert_eq!(similar.fields.as_deref(), Some("MediaSources"));
    }

    #[test]
    fn item_list_queries_accept_lower_camel_client_fields() {
        let uri: Uri = concat!(
            "/emby/Users/user-1/Items?",
            "parentId=library-1&startIndex=3&limit=12&recursive=true",
            "&includeItemTypes=Movie,Episode&imageTypes=Primary,Backdrop",
            "&enableImages=true&imageTypeLimit=2&enableImageTypes=Primary,Logo",
            "&anyProviderIdEquals=tmdb.42&sortBy=DateCreated&sortOrder=Descending",
            "&fields=MediaSources&filters=IsPlayed&isPlayed=true&isFavorite=false",
            "&isFolder=false&isMovie=true&isSeries=false&ids=item-1,item-2",
            "&excludeItemIds=item-3&searchTerm=signal&nameStartsWith=A",
            "&nameStartsWithOrGreater=B&nameLessThan=Z&genreIds=1,2",
            "&officialRatings=PG-13&excludeTags=Hidden&studioIds=3",
            "&personIds=4&personTypes=Actor&artistIds=5&albumIds=6",
            "&mediaTypes=Video&audioCodecs=aac&videoCodecs=h264&subtitleCodecs=srt"
        )
        .parse()
        .unwrap();
        let Query(query) = Query::<ItemsQuery>::try_from_uri(&uri).unwrap();

        assert_eq!(query.parent_id.as_deref(), Some("library-1"));
        assert_eq!(query.start_index, Some(3));
        assert_eq!(query.limit, Some(12));
        assert_eq!(query.recursive, Some(true));
        assert_eq!(query.include_item_types.as_deref(), Some("Movie,Episode"));
        assert_eq!(query.image_types.as_deref(), Some("Primary,Backdrop"));
        assert_eq!(query.enable_images, Some(true));
        assert_eq!(query.image_type_limit, Some(2));
        assert_eq!(query.enable_image_types.as_deref(), Some("Primary,Logo"));
        assert_eq!(query.any_provider_id_equals.as_deref(), Some("tmdb.42"));
        assert_eq!(query.sort_by.as_deref(), Some("DateCreated"));
        assert_eq!(query.sort_order.as_deref(), Some("Descending"));
        assert_eq!(query.fields.as_deref(), Some("MediaSources"));
        assert_eq!(query.filters.as_deref(), Some("IsPlayed"));
        assert_eq!(query.is_played, Some(true));
        assert_eq!(query.is_favorite, Some(false));
        assert_eq!(query.is_folder, Some(false));
        assert_eq!(query.is_movie, Some(true));
        assert_eq!(query.is_series, Some(false));
        assert_eq!(query.ids.as_deref(), Some("item-1,item-2"));
        assert_eq!(query.exclude_item_ids.as_deref(), Some("item-3"));
        assert_eq!(query.search_term.as_deref(), Some("signal"));
        assert_eq!(query.name_starts_with.as_deref(), Some("A"));
        assert_eq!(query.name_starts_with_or_greater.as_deref(), Some("B"));
        assert_eq!(query.name_less_than.as_deref(), Some("Z"));
        assert_eq!(query.genre_ids.as_deref(), Some("1,2"));
        assert_eq!(query.official_ratings.as_deref(), Some("PG-13"));
        assert_eq!(query.exclude_tags.as_deref(), Some("Hidden"));
        assert_eq!(query.studio_ids.as_deref(), Some("3"));
        assert_eq!(query.person_ids.as_deref(), Some("4"));
        assert_eq!(query.person_types.as_deref(), Some("Actor"));
        assert_eq!(query.artist_ids.as_deref(), Some("5"));
        assert_eq!(query.album_ids.as_deref(), Some("6"));
        assert_eq!(query.media_types.as_deref(), Some("Video"));
        assert_eq!(query.audio_codecs.as_deref(), Some("aac"));
        assert_eq!(query.video_codecs.as_deref(), Some("h264"));
        assert_eq!(query.subtitle_codecs.as_deref(), Some("srt"));
    }

    #[test]
    fn music_and_home_item_queries_accept_lower_camel_client_fields() {
        let uri: Uri = concat!(
            "/emby/Songs?",
            "userId=user-1&parentId=library-1&startIndex=2&limit=20",
            "&sortBy=SortName&sortOrder=Ascending&fields=MediaSources",
            "&searchTerm=blue&genreIds=1&artistIds=2&albumIds=3",
            "&enableImages=true&imageTypeLimit=2&enableImageTypes=Primary,Backdrop"
        )
        .parse()
        .unwrap();
        let Query(music) = Query::<MusicItemsQuery>::try_from_uri(&uri).unwrap();

        assert_eq!(music.user_id.as_deref(), Some("user-1"));
        assert_eq!(music.parent_id.as_deref(), Some("library-1"));
        assert_eq!(music.start_index, Some(2));
        assert_eq!(music.limit, Some(20));
        assert_eq!(music.sort_by.as_deref(), Some("SortName"));
        assert_eq!(music.sort_order.as_deref(), Some("Ascending"));
        assert_eq!(music.fields.as_deref(), Some("MediaSources"));
        assert_eq!(music.search_term.as_deref(), Some("blue"));
        assert_eq!(music.genre_ids.as_deref(), Some("1"));
        assert_eq!(music.artist_ids.as_deref(), Some("2"));
        assert_eq!(music.album_ids.as_deref(), Some("3"));
        assert_eq!(music.enable_images, Some(true));
        assert_eq!(music.image_type_limit, Some(2));
        assert_eq!(
            music.enable_image_types.as_deref(),
            Some("Primary,Backdrop")
        );

        let uri: Uri = concat!(
            "/emby/Items/Latest?",
            "userId=user-1&parentId=library-1&startIndex=4&limit=10",
            "&includeItemTypes=Movie&sortBy=DateCreated&sortOrder=Descending&fields=MediaSources"
        )
        .parse()
        .unwrap();
        let Query(media) = Query::<MediaListQuery>::try_from_uri(&uri).unwrap();

        assert_eq!(media.user_id.as_deref(), Some("user-1"));
        assert_eq!(media.parent_id.as_deref(), Some("library-1"));
        assert_eq!(media.start_index, Some(4));
        assert_eq!(media.limit, Some(10));
        assert_eq!(media.include_item_types.as_deref(), Some("Movie"));
        assert_eq!(media.sort_by.as_deref(), Some("DateCreated"));
        assert_eq!(media.sort_order.as_deref(), Some("Descending"));
        assert_eq!(media.fields.as_deref(), Some("MediaSources"));
    }

    #[test]
    fn search_and_item_detail_queries_accept_lower_camel_client_fields() {
        let uri: Uri = concat!(
            "/emby/Search/Hints?",
            "userId=user-1&parentId=library-1&searchTerm=signal",
            "&includeItemTypes=Movie&mediaTypes=Video&startIndex=1&limit=5"
        )
        .parse()
        .unwrap();
        let Query(search) = Query::<SearchHintsQuery>::try_from_uri(&uri).unwrap();

        assert_eq!(search.user_id.as_deref(), Some("user-1"));
        assert_eq!(search.parent_id.as_deref(), Some("library-1"));
        assert_eq!(search.search_term.as_deref(), Some("signal"));
        assert_eq!(search.include_item_types.as_deref(), Some("Movie"));
        assert_eq!(search.media_types.as_deref(), Some("Video"));
        assert_eq!(search.start_index, Some(1));
        assert_eq!(search.limit, Some(5));

        let uri: Uri = "/emby/Items/item-1?userId=user-1".parse().unwrap();
        let Query(item) = Query::<ItemByIdQuery>::try_from_uri(&uri).unwrap();

        assert_eq!(item.user_id.as_deref(), Some("user-1"));
    }

    #[test]
    fn requested_item_fields_accepts_chapters_field() {
        let fields = requested_item_fields(Some("MediaSources,Chapters,MediaStreams"));

        assert!(fields.media_sources);
        assert!(fields.chapters);
        assert!(fields.media_streams);
    }

    #[test]
    fn special_features_query_accepts_common_emby_parameters() {
        let query = serde_json::from_value::<SpecialFeaturesQuery>(json!({
            "UserId": "user-1",
            "StartIndex": 3,
            "Limit": 12,
            "Fields": "MediaSources,PrimaryImageAspectRatio",
            "EnableImages": true,
            "ImageTypeLimit": 2,
            "EnableImageTypes": "Primary,Backdrop",
            "EnableUserData": true
        }))
        .unwrap();

        assert_eq!(query.user_id.as_deref(), Some("user-1"));
        assert_eq!(query.start_index, Some(3));
        assert_eq!(query.limit, Some(12));
        assert_eq!(
            query.fields.as_deref(),
            Some("MediaSources,PrimaryImageAspectRatio")
        );
        assert_eq!(query.enable_images, Some(true));
        assert_eq!(query.image_type_limit, Some(2));
        assert_eq!(
            query.enable_image_types.as_deref(),
            Some("Primary,Backdrop")
        );
        assert_eq!(query.enable_user_data, Some(true));
    }

    #[test]
    fn critic_reviews_query_accepts_official_paging_and_user_id() {
        let query = serde_json::from_value::<CriticReviewsQuery>(json!({
            "UserId": "user-1",
            "StartIndex": 4,
            "Limit": 8
        }))
        .unwrap();

        assert_eq!(query.user_id.as_deref(), Some("user-1"));
        assert_eq!(query.start_index, Some(4));
        assert_eq!(query.limit, Some(8));
    }

    #[test]
    fn empty_critic_reviews_result_preserves_window_start_index() {
        let query = CriticReviewsQuery {
            user_id: Some("user-1".to_owned()),
            start_index: Some(4),
            limit: Some(MAX_ITEMS_LIMIT + 20),
        };
        let window = CriticReviewsWindow::from_query(&query);
        let result = empty_critic_reviews_result(&query);

        assert_eq!(window.limit, i64::from(MAX_ITEMS_LIMIT));
        assert!(result.items.is_empty());
        assert_eq!(result.total_record_count, 0);
        assert_eq!(result.start_index, 4);
    }

    #[test]
    fn playback_extra_query_accepts_common_emby_parameters() {
        let query = serde_json::from_value::<PlaybackExtrasQuery>(json!({
            "UserId": "user-1",
            "StartIndex": 1,
            "Limit": 4,
            "Fields": "MediaSources,PrimaryImageAspectRatio",
            "EnableImages": true,
            "ImageTypeLimit": 2,
            "EnableImageTypes": "Primary,Backdrop",
            "EnableUserData": true
        }))
        .unwrap();

        assert_eq!(query.user_id.as_deref(), Some("user-1"));
        assert_eq!(query.start_index, Some(1));
        assert_eq!(query.limit, Some(4));
        assert_eq!(
            query.fields.as_deref(),
            Some("MediaSources,PrimaryImageAspectRatio")
        );
        assert_eq!(query.enable_images, Some(true));
        assert_eq!(query.image_type_limit, Some(2));
        assert_eq!(
            query.enable_image_types.as_deref(),
            Some("Primary,Backdrop")
        );
        assert_eq!(query.enable_user_data, Some(true));
    }

    #[test]
    fn sort_query_uses_allowlisted_first_known_field() {
        let options = item_query_options(
            None,
            Some("UnsafeSql,DateCreated"),
            Some("Descending"),
            ItemSortField::SortName,
            SortDirection::Asc,
        );

        assert_eq!(options.sort_field, ItemSortField::DateCreated);
        assert_eq!(options.sort_direction, SortDirection::Desc);
    }

    #[test]
    fn item_scalar_filter_parses_emby_query_values() {
        let query = ItemsQuery {
            ids: Some("BBBBBBBB-0000-0000-0000-000000000001,item-2".to_owned()),
            exclude_item_ids: Some("item-3,item-4".to_owned()),
            search_term: Some(" movie ".to_owned()),
            years: Some("2024,2025,invalid,2024".to_owned()),
            name_starts_with: Some("A".to_owned()),
            name_starts_with_or_greater: Some("B".to_owned()),
            name_less_than: Some("Z".to_owned()),
            ..ItemsQuery::default()
        };

        let filter = scalar_filter_from_query(&query);

        assert!(filter.has_any_filter());
        assert_eq!(
            filter.include_ids.values,
            ["bbbbbbbb-0000-0000-0000-000000000001", "item-2"]
        );
        assert_eq!(filter.exclude_ids.values, ["item-3", "item-4"]);
        assert_eq!(filter.years.values, [2024, 2025]);
        assert_eq!(filter.search_term.as_deref(), Some("movie"));
        assert_eq!(filter.name_starts_with.as_deref(), Some("A"));
        assert_eq!(filter.name_starts_with_or_greater.as_deref(), Some("B"));
        assert_eq!(filter.name_less_than.as_deref(), Some("Z"));
    }

    #[test]
    fn invalid_years_keep_empty_enabled_filter() {
        let query = ItemsQuery {
            years: Some("invalid".to_owned()),
            ..ItemsQuery::default()
        };

        let filter = scalar_filter_from_query(&query);

        assert!(filter.years.enabled);
        assert!(filter.years.values.is_empty());
        assert!(filter.has_any_filter());
    }

    #[test]
    fn item_user_data_filter_parses_emby_filters() {
        let query = ItemsQuery {
            filters: Some("IsPlayed,IsUnplayed,IsFavorite,IsResumable,Likes,Dislikes".to_owned()),
            is_played: Some(false),
            is_favorite: Some(true),
            ..ItemsQuery::default()
        };

        let filter = user_data_filter_from_query(&query);

        assert!(filter.has_any_filter());
        assert_eq!(filter.is_played, Some(false));
        assert_eq!(filter.is_favorite, Some(true));
        assert!(filter.require_played);
        assert!(filter.require_unplayed);
        assert!(filter.require_favorite);
        assert!(filter.require_resumable);
        assert!(filter.require_likes);
        assert!(filter.require_dislikes);
    }

    #[test]
    fn item_structure_filter_parses_emby_filters() {
        let query = ItemsQuery {
            filters: Some("IsFolder,IsNotFolder".to_owned()),
            is_folder: Some(true),
            is_movie: Some(false),
            is_series: Some(true),
            ..ItemsQuery::default()
        };

        let filter = structure_filter_from_query(&query);

        assert!(filter.has_any_filter());
        assert_eq!(filter.is_folder, Some(true));
        assert_eq!(filter.is_movie, Some(false));
        assert_eq!(filter.is_series, Some(true));
        assert!(filter.require_folder);
        assert!(filter.require_not_folder);
    }

    #[test]
    fn item_media_filter_parses_emby_query_values() {
        let query = ItemsQuery {
            media_types: Some("Video,Audio,Photo".to_owned()),
            containers: Some(".mp4|mkv|ts".to_owned()),
            audio_codecs: Some("AAC,E-AC-3".to_owned()),
            video_codecs: Some("AVC,H265".to_owned()),
            subtitle_codecs: Some("srt,ASS".to_owned()),
            ..ItemsQuery::default()
        };

        let filter = media_filter_from_query(&query);

        assert!(filter.has_any_filter());
        assert_eq!(filter.media_types.values, ["video", "audio"]);
        assert!(filter.containers.values.contains(&"mp4".to_owned()));
        assert!(filter.containers.values.contains(&"matroska".to_owned()));
        assert!(filter.containers.values.contains(&"mpegts".to_owned()));
        assert_eq!(filter.audio_codecs.values, ["aac", "e-ac-3", "eac3"]);
        assert_eq!(filter.video_codecs.values, ["avc", "h264", "h265", "hevc"]);
        assert_eq!(filter.subtitle_codecs.values, ["srt", "ass"]);
    }

    #[test]
    fn item_provider_filter_parses_emby_query_values() {
        let query = ItemsQuery {
            any_provider_id_equals: Some(
                " TMDB.123,imdb.tt7654321,invalid,TVDB. 456 ,tmdb.123 ".to_owned(),
            ),
            ..ItemsQuery::default()
        };

        let filter = provider_filter_from_query(&query);

        assert!(filter.has_any_filter());
        assert_eq!(
            filter.any_provider_id_equals.values,
            ["tmdb.123", "imdb.tt7654321", "tvdb.456"]
        );
    }

    #[test]
    fn invalid_provider_ids_keep_empty_enabled_filter() {
        let filter = provider_id_list_filter(Some("invalid,tmdb.,.123"));

        assert!(filter.enabled);
        assert!(filter.values.is_empty());
    }

    #[test]
    fn item_image_filter_parses_emby_query_values() {
        let query = ItemsQuery {
            image_types: Some("Primary,Backdrop,Poster,Unsupported".to_owned()),
            ..ItemsQuery::default()
        };

        let filter = image_filter_from_query(&query);

        assert!(filter.has_any_filter());
        assert_eq!(filter.image_types.values, ["primary", "poster", "backdrop"]);
    }

    #[test]
    fn unknown_image_type_keeps_empty_enabled_filter() {
        let filter = image_type_list_filter(Some("Unsupported"));

        assert!(filter.enabled);
        assert!(filter.values.is_empty());
    }

    #[test]
    fn requested_item_images_parse_enable_options() {
        let requested = requested_item_images(&ItemsQuery {
            enable_images: Some(true),
            image_type_limit: Some(2),
            enable_image_types: Some("Primary,Backdrop,Logo".to_owned()),
            ..ItemsQuery::default()
        });

        assert!(requested.enabled);
        assert_eq!(requested.limit, 2);
        assert_eq!(
            requested
                .image_types
                .iter()
                .map(|image_type| image_type.output_key)
                .collect::<Vec<_>>(),
            ["Primary", "Backdrop", "Logo"]
        );
    }

    #[test]
    fn enable_images_false_disables_requested_images() {
        let requested = requested_item_images(&ItemsQuery {
            enable_images: Some(false),
            enable_image_types: Some("Primary".to_owned()),
            image_type_limit: Some(2),
            ..ItemsQuery::default()
        });

        assert!(!requested.enabled);
        assert!(requested.image_types.is_empty());
        assert_eq!(requested.limit, 0);
    }

    #[test]
    fn unknown_media_type_keeps_empty_enabled_filter() {
        let filter = media_type_list_filter(Some("Photo"));

        assert!(filter.enabled);
        assert!(filter.values.is_empty());
    }

    #[test]
    fn item_association_filter_parses_emby_query_values() {
        let query = ItemsQuery {
            genres: Some("Action|Drama|Action".to_owned()),
            genre_ids: Some("1|2,3".to_owned()),
            official_ratings: Some("PG-13|TV-MA".to_owned()),
            tags: Some("HDR|IMAX".to_owned()),
            exclude_tags: Some("Blocked|Spoiler".to_owned()),
            studios: Some("Studio A|Studio B".to_owned()),
            studio_ids: Some("DDDDDDDD-0000-0000-0000-000000000001|2".to_owned()),
            person: Some("Tom Hanks".to_owned()),
            person_ids: Some("CCCCCCCC-0000-0000-0000-000000000001".to_owned()),
            person_types: Some("Actor,Guest Star".to_owned()),
            artists: Some("David Bowie|Queen".to_owned()),
            artist_ids: Some("AAAAAAAA-0000-0000-0000-000000000001|2".to_owned()),
            albums: Some("Low|Heroes".to_owned()),
            album_ids: Some("BBBBBBBB-0000-0000-0000-000000000001|3".to_owned()),
            ..ItemsQuery::default()
        };

        let filter = association_filter_from_query(&query);

        assert!(filter.has_any_filter());
        assert_eq!(filter.genre_names.values, ["action", "drama"]);
        assert_eq!(filter.genre_ids.values, ["1", "2", "3"]);
        assert_eq!(filter.official_ratings.values, ["pg-13", "tv-ma"]);
        assert_eq!(filter.tag_names.values, ["hdr", "imax"]);
        assert_eq!(filter.exclude_tag_names.values, ["blocked", "spoiler"]);
        assert_eq!(filter.studio_names.values, ["studio a", "studio b"]);
        assert_eq!(
            filter.studio_ids.values,
            ["dddddddd-0000-0000-0000-000000000001", "2"]
        );
        assert_eq!(filter.person_names.values, ["tom hanks"]);
        assert_eq!(
            filter.person_ids.values,
            ["cccccccc-0000-0000-0000-000000000001"]
        );
        assert_eq!(filter.person_role_types.role_types, ["actor", "guest_star"]);
        assert_eq!(filter.artist_names.values, ["david bowie", "queen"]);
        assert_eq!(
            filter.artist_ids.values,
            ["aaaaaaaa-0000-0000-0000-000000000001", "2"]
        );
        assert_eq!(filter.album_names.values, ["low", "heroes"]);
        assert_eq!(
            filter.album_ids.values,
            ["bbbbbbbb-0000-0000-0000-000000000001", "3"]
        );
    }

    #[test]
    fn unknown_person_type_keeps_empty_enabled_role_filter() {
        let query = ItemsQuery {
            person_types: Some("Conductor".to_owned()),
            ..ItemsQuery::default()
        };

        let filter = association_filter_from_query(&query);

        assert!(filter.person_role_types.enabled);
        assert!(filter.person_role_types.role_types.is_empty());
        assert!(filter.has_any_filter());
    }

    #[test]
    fn requested_item_fields_parse_media_source_fields() {
        let fields = requested_item_fields(Some("MediaSources,MediaStreams,Overview"));

        assert!(fields.media_sources);
        assert!(fields.media_streams);
    }

    #[test]
    fn root_query_returns_views_only_without_recursive_or_type_filter() {
        let default_options = item_query_options(
            None,
            None,
            None,
            ItemSortField::SortName,
            SortDirection::Asc,
        );
        assert!(should_return_library_views(None, false, &default_options));
        assert!(!should_return_library_views(None, true, &default_options));

        let type_filter_options = item_query_options(
            Some("Movie"),
            None,
            None,
            ItemSortField::SortName,
            SortDirection::Asc,
        );
        assert!(!should_return_library_views(
            None,
            false,
            &type_filter_options
        ));

        let scalar_filter_options = item_query_options_with_filters(
            None,
            None,
            None,
            ItemSortField::SortName,
            SortDirection::Asc,
            ItemScalarFilter {
                search_term: Some("movie".to_owned()),
                ..ItemScalarFilter::default()
            },
            ItemUserDataFilter::default(),
            ItemStructureFilter::default(),
            ItemMediaFilter::default(),
            ItemProviderFilter::default(),
            ItemImageFilter::default(),
            ItemAssociationFilter::default(),
        );
        assert!(!should_return_library_views(
            None,
            false,
            &scalar_filter_options
        ));

        let user_data_filter_options = item_query_options_with_filters(
            None,
            None,
            None,
            ItemSortField::SortName,
            SortDirection::Asc,
            ItemScalarFilter::default(),
            ItemUserDataFilter {
                is_favorite: Some(true),
                ..ItemUserDataFilter::default()
            },
            ItemStructureFilter::default(),
            ItemMediaFilter::default(),
            ItemProviderFilter::default(),
            ItemImageFilter::default(),
            ItemAssociationFilter::default(),
        );
        assert!(!should_return_library_views(
            None,
            false,
            &user_data_filter_options
        ));

        let association_filter_options = item_query_options_with_filters(
            None,
            None,
            None,
            ItemSortField::SortName,
            SortDirection::Asc,
            ItemScalarFilter::default(),
            ItemUserDataFilter::default(),
            ItemStructureFilter::default(),
            ItemMediaFilter::default(),
            ItemProviderFilter::default(),
            ItemImageFilter::default(),
            ItemAssociationFilter {
                genre_names: StringListFilter::enabled(vec!["action".to_owned()]),
                ..ItemAssociationFilter::default()
            },
        );
        assert!(!should_return_library_views(
            None,
            false,
            &association_filter_options
        ));

        let structure_filter_options = item_query_options_with_filters(
            None,
            None,
            None,
            ItemSortField::SortName,
            SortDirection::Asc,
            ItemScalarFilter::default(),
            ItemUserDataFilter::default(),
            ItemStructureFilter {
                is_folder: Some(true),
                ..ItemStructureFilter::default()
            },
            ItemMediaFilter::default(),
            ItemProviderFilter::default(),
            ItemImageFilter::default(),
            ItemAssociationFilter::default(),
        );
        assert!(!should_return_library_views(
            None,
            false,
            &structure_filter_options
        ));

        let media_filter_options = item_query_options_with_filters(
            None,
            None,
            None,
            ItemSortField::SortName,
            SortDirection::Asc,
            ItemScalarFilter::default(),
            ItemUserDataFilter::default(),
            ItemStructureFilter::default(),
            ItemMediaFilter {
                containers: StringListFilter::enabled(vec!["mkv".to_owned()]),
                ..ItemMediaFilter::default()
            },
            ItemProviderFilter::default(),
            ItemImageFilter::default(),
            ItemAssociationFilter::default(),
        );
        assert!(!should_return_library_views(
            None,
            false,
            &media_filter_options
        ));

        let provider_filter_options = item_query_options_with_filters(
            None,
            None,
            None,
            ItemSortField::SortName,
            SortDirection::Asc,
            ItemScalarFilter::default(),
            ItemUserDataFilter::default(),
            ItemStructureFilter::default(),
            ItemMediaFilter::default(),
            ItemProviderFilter {
                any_provider_id_equals: StringListFilter::enabled(vec!["tmdb.123".to_owned()]),
            },
            ItemImageFilter::default(),
            ItemAssociationFilter::default(),
        );
        assert!(!should_return_library_views(
            None,
            false,
            &provider_filter_options
        ));

        let image_filter_options = item_query_options_with_filters(
            None,
            None,
            None,
            ItemSortField::SortName,
            SortDirection::Asc,
            ItemScalarFilter::default(),
            ItemUserDataFilter::default(),
            ItemStructureFilter::default(),
            ItemMediaFilter::default(),
            ItemProviderFilter::default(),
            ItemImageFilter {
                image_types: StringListFilter::enabled(vec!["primary".to_owned()]),
            },
            ItemAssociationFilter::default(),
        );
        assert!(!should_return_library_views(
            None,
            false,
            &image_filter_options
        ));
    }

    #[test]
    fn root_parent_id_is_treated_as_library_root() {
        assert_eq!(normalized_parent_id(Some(" root ".to_owned())), None);
        assert_eq!(normalized_parent_id(Some("ROOT".to_owned())), None);
        assert_eq!(
            normalized_parent_id(Some("library-1".to_owned())).as_deref(),
            Some("library-1")
        );
    }

    #[test]
    fn root_folder_mapping_uses_emby_folder_shape() {
        let item = root_folder_item();

        assert_eq!(item.id, "root");
        assert_eq!(item.name, "Media Library");
        assert_eq!(item.item_type, "Folder");
        assert!(item.is_folder);
        assert_eq!(item.parent_id, None);
        assert_eq!(item.media_type, None);
        assert!(item.image_tags.is_empty());
        assert!(item.media_sources.is_empty());
    }

    #[test]
    fn additional_parts_result_uses_empty_query_result_shape() {
        let result = empty_additional_parts_result();

        assert!(result.items.is_empty());
        assert_eq!(result.total_record_count, 0);
        assert_eq!(result.start_index, 0);
    }

    #[test]
    fn video_version_inputs_normalize_item_ids() {
        assert_eq!(
            video_version_item_id(" movie-1 ").unwrap().as_str(),
            "movie-1"
        );
        assert_eq!(
            merge_versions_input(&MergeVersionsQuery {
                ids: Some(" movie-1,movie-2,,movie-1 ".to_owned()),
            })
            .unwrap()
            .ids,
            ["movie-1", "movie-2"]
        );
        assert!(video_version_item_id("bad/id").is_err());
        assert!(
            merge_versions_input(&MergeVersionsQuery {
                ids: Some("movie-1".to_owned()),
            })
            .is_err()
        );
    }

    #[test]
    fn item_counts_mapping_preserves_known_media_type_counts() {
        let dto = item_counts_to_dto(ItemCountsRecord {
            movie_count: 10,
            series_count: 2,
            episode_count: 30,
            artist_count: 4,
            song_count: 50,
            album_count: 6,
            box_set_count: 1,
            item_count: 103,
        });

        assert_eq!(dto.movie_count, 10);
        assert_eq!(dto.series_count, 2);
        assert_eq!(dto.episode_count, 30);
        assert_eq!(dto.song_count, 50);
        assert_eq!(dto.album_count, 6);
        assert_eq!(dto.box_set_count, 1);
        assert_eq!(dto.program_count, 0);
        assert_eq!(dto.item_count, 103);
    }

    #[test]
    fn media_item_mapping_uses_emby_types() {
        let item = media_item_to_base_item(MediaItemBrowseRecord {
            id: "item-1".to_owned(),
            name: "Movie".to_owned(),
            item_type: "movie".to_owned(),
            parent_id: None,
            run_time_ticks: Some(42),
            media_file_id: Some(7),
            media_file_size: Some(42_000_000),
            media_file_container: Some("mkv".to_owned()),
            media_file_bitrate: Some(12_000_000),
            media_file_is_strm: Some(false),
            supports_transcoding: true,
            production_year: Some(2026),
            playback_position_ticks: 100,
            play_count: 2,
            is_favorite: true,
            rating: Some(8.5),
            played: true,
            image_tags: Vec::new(),
            total_record_count: 1,
        });

        assert_eq!(item.item_type, "Movie");
        assert_eq!(item.media_type.as_deref(), Some("Video"));
        assert!(!item.is_folder);
        assert_eq!(item.size, Some(42_000_000));
        assert_eq!(item.container.as_deref(), Some("mkv"));
        assert_eq!(item.bitrate, Some(12_000_000));
        assert_eq!(item.media_sources.len(), 1);
        assert_eq!(item.media_sources[0].id, "7");
        assert_eq!(item.media_sources[0].source_type, "Default");
        assert_eq!(item.media_sources[0].name, "7");
        assert_eq!(item.media_sources[0].item_id.as_deref(), Some("item-1"));
        assert_eq!(item.media_sources[0].protocol, "File");
        assert!(!item.media_sources[0].is_remote);
        assert!(!item.media_sources[0].requires_opening);
        assert!(!item.media_sources[0].requires_closing);
        assert!(!item.media_sources[0].supports_probing);
        assert!(!item.media_sources[0].read_at_native_framerate);
        assert_eq!(item.media_sources[0].path, None);
        assert_eq!(item.media_sources[0].default_audio_stream_index, None);
        assert_eq!(item.media_sources[0].default_subtitle_stream_index, None);
        assert!(item.media_sources[0].supports_transcoding);
        assert_eq!(
            item.user_data.as_ref().unwrap().playback_position_ticks,
            100
        );
        assert_eq!(item.user_data.as_ref().unwrap().play_count, 2);
        assert_eq!(item.user_data.as_ref().unwrap().rating, Some(8.5));
        assert!(item.user_data.as_ref().unwrap().is_favorite);
        assert!(item.user_data.as_ref().unwrap().played);
        assert_eq!(
            item.user_data.as_ref().unwrap().item_id.as_deref(),
            Some("item-1")
        );
    }

    #[test]
    fn media_item_mapping_uses_requested_image_tags() {
        let requested = requested_item_images(&ItemsQuery {
            enable_images: Some(true),
            image_type_limit: Some(2),
            enable_image_types: Some("Primary,Backdrop,Logo".to_owned()),
            ..ItemsQuery::default()
        });
        let item = media_item_to_base_item_with_images(
            MediaItemBrowseRecord {
                id: "item-1".to_owned(),
                name: "Movie".to_owned(),
                item_type: "movie".to_owned(),
                parent_id: None,
                run_time_ticks: None,
                media_file_id: None,
                media_file_size: None,
                media_file_container: None,
                media_file_bitrate: None,
                media_file_is_strm: None,
                supports_transcoding: false,
                production_year: None,
                playback_position_ticks: 0,
                play_count: 0,
                is_favorite: false,
                rating: None,
                played: false,
                image_tags: vec![
                    "poster=poster-tag".to_owned(),
                    "primary=primary-tag".to_owned(),
                    "backdrop=backdrop-1".to_owned(),
                    "backdrop=backdrop-2".to_owned(),
                    "backdrop=backdrop-3".to_owned(),
                    "logo=logo-tag".to_owned(),
                ],
                total_record_count: 1,
            },
            &requested,
        );

        assert_eq!(
            item.image_tags.get("Primary").map(String::as_str),
            Some("primary-tag")
        );
        assert_eq!(
            item.image_tags.get("Logo").map(String::as_str),
            Some("logo-tag")
        );
        assert_eq!(item.backdrop_image_tags, ["backdrop-1", "backdrop-2"]);
    }

    #[test]
    fn library_view_mapping_uses_collection_folder_type() {
        let item = library_view_to_base_item(UserLibraryViewRecord {
            id: "library-1".to_owned(),
            name: "Movies".to_owned(),
            library_type: "movies".to_owned(),
        });

        assert_eq!(item.item_type, "CollectionFolder");
        assert!(item.is_folder);
        assert_eq!(item.parent_id, None);
        assert_eq!(item.collection_type.as_deref(), Some("movies"));
    }

    #[test]
    fn ancestor_mapping_preserves_library_and_media_shapes() {
        let library =
            ancestor_record_to_base_item(UserItemAncestorRecord::Library(UserLibraryViewRecord {
                id: "library-1".to_owned(),
                name: "Movies".to_owned(),
                library_type: "movies".to_owned(),
            }));
        let media =
            ancestor_record_to_base_item(UserItemAncestorRecord::Media(MediaItemBrowseRecord {
                id: "series-1".to_owned(),
                name: "Series".to_owned(),
                item_type: "series".to_owned(),
                parent_id: Some("library-1".to_owned()),
                run_time_ticks: None,
                media_file_id: None,
                media_file_size: None,
                media_file_container: None,
                media_file_bitrate: None,
                media_file_is_strm: None,
                supports_transcoding: false,
                production_year: Some(2026),
                playback_position_ticks: 0,
                play_count: 0,
                is_favorite: false,
                rating: None,
                played: false,
                image_tags: Vec::new(),
                total_record_count: 1,
            }));

        assert_eq!(library.item_type, "CollectionFolder");
        assert_eq!(library.collection_type.as_deref(), Some("movies"));
        assert_eq!(media.item_type, "Series");
        assert!(media.is_folder);
        assert_eq!(media.parent_id.as_deref(), Some("library-1"));
    }
}
