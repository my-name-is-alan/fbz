use axum::{
    Json,
    extract::{Path, Query, State},
    http::{HeaderMap, Uri},
};
use serde::Deserialize;

use crate::{
    auth::service::AuthenticatedUser,
    compat::emby::dto::{
        BaseItemDto, BaseItemSource, ItemCountsDto, MediaSourceDto, QueryResultDto, UserItemDataDto,
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
const DEFAULT_IMAGE_TYPE_LIMIT: usize = 1;
const MAX_IMAGE_TYPE_LIMIT: usize = 10;
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
    pub parent_id: Option<String>,
    pub start_index: Option<u32>,
    pub limit: Option<u32>,
    pub recursive: Option<bool>,
    pub include_item_types: Option<String>,
    pub image_types: Option<String>,
    pub enable_images: Option<bool>,
    pub image_type_limit: Option<u32>,
    pub enable_image_types: Option<String>,
    pub any_provider_id_equals: Option<String>,
    pub sort_by: Option<String>,
    pub sort_order: Option<String>,
    pub fields: Option<String>,
    pub filters: Option<String>,
    pub is_played: Option<bool>,
    pub is_favorite: Option<bool>,
    pub is_folder: Option<bool>,
    pub is_movie: Option<bool>,
    pub is_series: Option<bool>,
    pub ids: Option<String>,
    pub exclude_item_ids: Option<String>,
    pub search_term: Option<String>,
    pub years: Option<String>,
    pub name_starts_with: Option<String>,
    pub name_starts_with_or_greater: Option<String>,
    pub name_less_than: Option<String>,
    pub genres: Option<String>,
    pub genre_ids: Option<String>,
    pub official_ratings: Option<String>,
    pub tags: Option<String>,
    pub exclude_tags: Option<String>,
    pub studios: Option<String>,
    pub studio_ids: Option<String>,
    pub person: Option<String>,
    pub person_ids: Option<String>,
    pub person_types: Option<String>,
    pub artists: Option<String>,
    pub artist_ids: Option<String>,
    pub albums: Option<String>,
    pub album_ids: Option<String>,
    pub media_types: Option<String>,
    pub containers: Option<String>,
    pub audio_codecs: Option<String>,
    pub video_codecs: Option<String>,
    pub subtitle_codecs: Option<String>,
}

#[derive(Clone, Debug, Default, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "PascalCase")]
pub struct MusicItemsQuery {
    pub user_id: Option<String>,
    pub parent_id: Option<String>,
    pub start_index: Option<u32>,
    pub limit: Option<u32>,
    pub sort_by: Option<String>,
    pub sort_order: Option<String>,
    pub fields: Option<String>,
    pub search_term: Option<String>,
    pub years: Option<String>,
    pub genres: Option<String>,
    pub genre_ids: Option<String>,
    pub artists: Option<String>,
    pub artist_ids: Option<String>,
    pub albums: Option<String>,
    pub album_ids: Option<String>,
    pub enable_images: Option<bool>,
    pub image_type_limit: Option<u32>,
    pub enable_image_types: Option<String>,
}

#[derive(Clone, Debug, Default, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "PascalCase")]
pub struct ItemByIdQuery {
    pub user_id: Option<String>,
}

#[derive(Clone, Debug, Default, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "PascalCase")]
pub struct AdditionalPartsQuery {
    pub user_id: Option<String>,
    pub fields: Option<String>,
    pub enable_images: Option<bool>,
    pub image_type_limit: Option<u32>,
    pub enable_image_types: Option<String>,
    pub enable_user_data: Option<bool>,
}

#[derive(Clone, Debug, Default, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "PascalCase")]
pub struct ItemCountsQuery {
    pub user_id: Option<String>,
}

#[derive(Clone, Debug, Default, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "PascalCase")]
pub(super) struct MediaListQuery {
    pub parent_id: Option<String>,
    pub start_index: Option<u32>,
    pub limit: Option<u32>,
    pub include_item_types: Option<String>,
    pub sort_by: Option<String>,
    pub sort_order: Option<String>,
    pub fields: Option<String>,
}

#[derive(Clone, Debug, Default, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "PascalCase")]
pub struct SimilarItemsQuery {
    pub user_id: Option<String>,
    pub start_index: Option<u32>,
    pub limit: Option<u32>,
    pub include_item_types: Option<String>,
    pub sort_by: Option<String>,
    pub sort_order: Option<String>,
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

async fn list_items_for_authenticated_user(
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

pub async fn resume_items(
    State(state): State<AppState>,
    Path(user_id): Path<String>,
    Query(query): Query<MediaListQuery>,
    headers: HeaderMap,
    uri: Uri,
) -> Result<Json<QueryResultDto<BaseItemDto>>, AppError> {
    let user = authenticate_route_user(&state, &user_id, &headers, &uri).await?;
    let Some(database) = state.database() else {
        return Err(AppError::internal("database is not configured"));
    };

    let window = ItemWindow::from_media_query(&query);
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
            parent_id: normalized_parent_id(query.parent_id),
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
    let Some(database) = state.database() else {
        return Err(AppError::internal("database is not configured"));
    };

    let window = ItemWindow::from_media_query(&query);
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
            parent_id: normalized_parent_id(query.parent_id),
            start_index: window.start_index,
            limit: window.limit,
            options,
        })
        .await
        .map_err(|err| AppError::internal(format!("failed to list latest items: {err}")))?;

    Ok(Json(media_items_to_dtos(result.items)))
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
            start_index: i64::from(query.start_index.unwrap_or(0)),
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
            start_index: i64::from(query.start_index.unwrap_or(0)),
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
            start_index: i64::from(query.start_index.unwrap_or(0)),
            limit: i64::from(
                query
                    .limit
                    .unwrap_or(DEFAULT_ITEMS_LIMIT)
                    .clamp(1, MAX_ITEMS_LIMIT),
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

fn media_type_list_filter(value: Option<&str>) -> StringListFilter {
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
        path: None,
        protocol: media_source_summary_protocol(record).to_owned(),
        container: record.media_file_container.clone(),
        run_time_ticks: record.run_time_ticks,
        size: record.media_file_size,
        bitrate: record.media_file_bitrate,
        media_streams: Vec::new(),
        supports_direct_play: true,
        supports_direct_stream: true,
        supports_transcoding: record.supports_transcoding,
        direct_stream_url: None,
        add_api_key_to_direct_stream_url: false,
        transcoding_url: None,
        transcoding_sub_protocol: None,
        transcoding_container: None,
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
        assert_eq!(item.media_sources[0].protocol, "File");
        assert_eq!(item.media_sources[0].path, None);
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
