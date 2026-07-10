use std::{
    io::Cursor,
    path::{Component, Path, PathBuf},
};

use axum::{
    Json,
    body::{Body, Bytes},
    extract::{Path as AxumPath, Query, State},
    http::{
        HeaderMap, HeaderValue, StatusCode, Uri,
        header::{CACHE_CONTROL, CONTENT_LENGTH, CONTENT_TYPE, LOCATION},
    },
    response::Response,
};
use image::{ImageReader, codecs::jpeg::JpegEncoder, imageops::FilterType};
use serde::{Deserialize, Serialize};
use tokio::fs::File;
use tokio_util::io::ReaderStream;

use crate::{
    auth::service::AuthenticatedUser,
    compat::emby::payload::parse_emby_body,
    config::StorageConfig,
    error::AppError,
    library::repository::LibraryRepository,
    media::repository::{ArtworkRecord, InsertArtworkInput, MediaRepository},
    state::AppState,
    users::repository::UsersRepository,
};

use super::access::authenticate_request_user;

const IMAGE_CACHE_CONTROL: &str = "public, max-age=86400";
const MAX_ITEM_ID_LEN: usize = 128;
const MAX_ITEM_IMAGE_MUTATION_BODY_BYTES: usize = 16 * 1024 * 1024;
const MAX_ENTITY_IMAGE_NAME_LEN: usize = 256;
const MAX_GLOBAL_IMAGE_TEXT_LEN: usize = 128;
const MAX_IMAGE_CACHE_TAG_LEN: usize = 128;
const MAX_IMAGE_DIMENSION: i64 = 8192;
const DEFAULT_DERIVED_IMAGE_QUALITY: u8 = 82;
const MIN_DERIVED_IMAGE_QUALITY: u8 = 35;
const MAX_DERIVED_IMAGE_QUALITY: u8 = 95;
const MAX_IMAGE_PERCENT_PLAYED: i64 = 100;
const MAX_IMAGE_UNPLAYED_COUNT: i64 = 100_000;
const MAX_USER_IMAGE_ID_LEN: usize = 128;
const MAX_REMOTE_IMAGE_LIMIT: u32 = 100;
const MAX_REMOTE_IMAGE_START_INDEX: u32 = 10_000;
const MAX_REMOTE_IMAGE_PROVIDER_NAME_LEN: usize = 128;
const MAX_REMOTE_IMAGE_URL_LEN: usize = 2048;
const REMOTE_IMAGE_TYPES: &[&str] = &[
    "Primary",
    "Art",
    "Backdrop",
    "Banner",
    "Logo",
    "Thumb",
    "Disc",
    "Box",
    "Screenshot",
    "Menu",
    "Chapter",
    "BoxRear",
    "Thumbnail",
    "LogoLight",
    "LogoLightColor",
];
const REMOTE_IMAGE_PROVIDERS: &[(&str, &[&str])] = &[
    (
        "TheMovieDb",
        &["Primary", "Backdrop", "Logo", "Thumb", "Banner"],
    ),
    (
        "TheTVDB",
        &["Primary", "Backdrop", "Logo", "Thumb", "Banner"],
    ),
    ("Fanart", &["Primary", "Backdrop", "Logo", "Thumb"]),
];

#[derive(Clone, Debug, Default, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "PascalCase")]
pub struct RemoteImagesQuery {
    #[serde(rename = "Type", alias = "type")]
    pub r#type: Option<String>,
    #[serde(alias = "startIndex", alias = "start_index")]
    pub start_index: Option<u32>,
    #[serde(alias = "limit")]
    pub limit: Option<u32>,
    #[serde(alias = "providerName", alias = "provider_name")]
    pub provider_name: Option<String>,
    #[serde(alias = "includeAllLanguages", alias = "include_all_languages")]
    pub include_all_languages: Option<bool>,
    #[serde(alias = "enableSeriesImages", alias = "enable_series_images")]
    pub enable_series_images: Option<bool>,
}

#[derive(Clone, Debug, Default, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "PascalCase")]
pub struct RemoteImageDownloadQuery {
    #[serde(rename = "Type", alias = "type")]
    pub r#type: Option<String>,
    #[serde(alias = "providerName", alias = "provider_name")]
    pub provider_name: Option<String>,
    #[serde(alias = "imageUrl", alias = "image_url")]
    pub image_url: Option<String>,
}

#[derive(Clone, Debug, Default, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "PascalCase")]
pub struct RemoteImageProxyQuery {
    #[serde(alias = "imageUrl", alias = "image_url")]
    pub image_url: Option<String>,
}

#[derive(Clone, Debug, Default, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "PascalCase")]
pub struct RemoteImageDownloadBody {
    #[serde(alias = "imageIndex", alias = "image_index")]
    pub image_index: Option<i32>,
}

#[derive(Clone, Debug, Default, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "PascalCase")]
pub struct ItemImageUploadQuery {
    #[serde(alias = "imageIndex", alias = "image_index")]
    pub index: Option<i32>,
}

#[derive(Clone, Debug, Default, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "PascalCase")]
pub struct ItemImageDeleteQuery {
    #[serde(alias = "imageIndex", alias = "image_index")]
    pub index: Option<i32>,
}

#[derive(Clone, Debug, Default, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "PascalCase")]
pub struct ItemImageIndexQuery {
    #[serde(alias = "newIndex", alias = "new_index")]
    pub new_index: Option<i32>,
}

#[derive(Clone, Debug, Default, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "PascalCase")]
pub struct ItemImageUrlQuery {
    #[serde(alias = "url")]
    pub url: Option<String>,
}

#[derive(Clone, Debug, Serialize, PartialEq)]
#[serde(rename_all = "PascalCase")]
pub struct RemoteImageResultDto {
    pub images: Vec<RemoteImageInfoDto>,
    pub total_record_count: u32,
    pub providers: Vec<String>,
}

#[derive(Clone, Debug, Serialize, PartialEq)]
#[serde(rename_all = "PascalCase")]
pub struct RemoteImageInfoDto {
    pub provider_name: Option<String>,
    pub url: Option<String>,
    pub thumbnail_url: Option<String>,
    pub height: Option<u32>,
    pub width: Option<u32>,
    pub community_rating: Option<f32>,
    pub vote_count: Option<u32>,
    pub language: Option<String>,
    pub display_language: Option<String>,
    pub r#type: String,
    pub rating_type: Option<String>,
}

#[derive(Clone, Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "PascalCase")]
pub struct ImageProviderInfoDto {
    pub name: String,
    pub supported_images: Vec<String>,
}

#[derive(Clone, Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "PascalCase")]
pub struct ItemImageInfoDto {
    pub image_type: String,
    pub image_index: i32,
    pub path: Option<String>,
    pub filename: Option<String>,
    pub height: Option<i32>,
    pub width: Option<i32>,
    pub size: Option<i64>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct RemoteImagesInput {
    image_type: Option<String>,
    start_index: u32,
    limit: u32,
    provider_name: Option<String>,
    include_all_languages: bool,
    enable_series_images: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct RemoteImageDownloadInput {
    item_id: String,
    image_type: String,
    provider_name: Option<String>,
    image_url: Option<String>,
    image_index: Option<i32>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct ItemImageMutationInput {
    item_id: String,
    image_type: String,
    image_index: Option<i32>,
    new_index: Option<i32>,
    image_url: Option<String>,
    body_len: Option<usize>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum NamedEntityImageKind {
    Artist,
    Genre,
    MusicGenre,
    Person,
    Studio,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct NamedEntityImageInput {
    kind: NamedEntityImageKind,
    name: String,
    image_type: String,
    index: i32,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct UserImageInput {
    user_id: String,
    image_type: String,
    index: i32,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct LongFormItemImageInput {
    item_id: String,
    image_type: String,
    image_index: i32,
    tag: String,
    format: String,
    max_width: i32,
    max_height: i32,
    percent_played: i32,
    unplayed_count: i32,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
struct ImageTransformRequest {
    max_width: Option<u32>,
    max_height: Option<u32>,
    quality: u8,
}

impl ImageTransformRequest {
    fn new(max_width: Option<u32>, max_height: Option<u32>, quality: Option<u8>) -> Self {
        Self {
            max_width,
            max_height,
            quality: quality.unwrap_or(DEFAULT_DERIVED_IMAGE_QUALITY),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
enum GlobalImageKind {
    General,
    MediaInfo,
    Ratings,
}

#[derive(Clone, Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "PascalCase")]
pub struct GlobalImageInfoDto {
    name: String,
    theme: Option<String>,
    r#type: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct GeneralImageInput {
    name: String,
    image_type: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct ThemedGlobalImageInput {
    kind: GlobalImageKind,
    theme: String,
    name: String,
}

pub async fn item_image(
    State(state): State<AppState>,
    AxumPath((item_id, image_type)): AxumPath<(String, String)>,
    headers: HeaderMap,
    uri: Uri,
) -> Result<Response, AppError> {
    let transform = image_transform_from_uri(&uri)?;
    item_image_by_index(state, item_id, image_type, 0, headers, uri, transform).await
}

pub async fn item_image_index(
    State(state): State<AppState>,
    AxumPath((item_id, image_type, index)): AxumPath<(String, String, i64)>,
    headers: HeaderMap,
    uri: Uri,
) -> Result<Response, AppError> {
    let transform = image_transform_from_uri(&uri)?;
    item_image_by_index(state, item_id, image_type, index, headers, uri, transform).await
}

pub async fn item_image_long_form(
    State(state): State<AppState>,
    AxumPath((
        item_id,
        image_type,
        index,
        tag,
        format,
        max_width,
        max_height,
        percent_played,
        unplayed_count,
    )): AxumPath<(String, String, i64, String, String, i64, i64, i64, i64)>,
    headers: HeaderMap,
    uri: Uri,
) -> Result<Response, AppError> {
    let input = long_form_item_image_input(
        &item_id,
        &image_type,
        index,
        &tag,
        &format,
        max_width,
        max_height,
        percent_played,
        unplayed_count,
    )?;
    let _ = (
        &input.tag,
        &input.format,
        input.percent_played,
        input.unplayed_count,
    );
    let transform = Some(ImageTransformRequest::new(
        Some(input.max_width as u32),
        Some(input.max_height as u32),
        None,
    ));

    item_image_by_index(
        state,
        input.item_id,
        input.image_type,
        i64::from(input.image_index),
        headers,
        uri,
        transform,
    )
    .await
}

pub async fn named_entity_image(
    State(state): State<AppState>,
    AxumPath((name, image_type)): AxumPath<(String, String)>,
    headers: HeaderMap,
    uri: Uri,
) -> Result<Response, AppError> {
    named_entity_image_response(state, name, image_type, None, headers, uri).await
}

pub async fn named_entity_image_index(
    State(state): State<AppState>,
    AxumPath((name, image_type, index)): AxumPath<(String, String, i64)>,
    headers: HeaderMap,
    uri: Uri,
) -> Result<Response, AppError> {
    named_entity_image_response(state, name, image_type, Some(index), headers, uri).await
}

pub async fn user_image(
    State(state): State<AppState>,
    AxumPath((user_id, image_type)): AxumPath<(String, String)>,
    headers: HeaderMap,
    uri: Uri,
) -> Result<Response, AppError> {
    user_image_response(state, user_id, image_type, None, headers, uri).await
}

pub async fn user_image_index(
    State(state): State<AppState>,
    AxumPath((user_id, image_type, index)): AxumPath<(String, String, i64)>,
    headers: HeaderMap,
    uri: Uri,
) -> Result<Response, AppError> {
    user_image_response(state, user_id, image_type, Some(index), headers, uri).await
}

pub async fn upload_item_image(
    State(state): State<AppState>,
    AxumPath((item_id, image_type)): AxumPath<(String, String)>,
    Query(query): Query<ItemImageUploadQuery>,
    headers: HeaderMap,
    uri: Uri,
    body: Bytes,
) -> Result<StatusCode, AppError> {
    let user = authenticate_request_user(&state, &headers, &uri).await?;
    ensure_remote_image_admin(&user)?;
    let input = item_image_upload_input(&item_id, &image_type, None, query, &body)?;
    ensure_user_can_access_item(&state, user.id, &input.item_id).await?;

    apply_item_image_upload(&state, input, &body).await
}

pub async fn upload_item_image_index(
    State(state): State<AppState>,
    AxumPath((item_id, image_type, index)): AxumPath<(String, String, i64)>,
    Query(query): Query<ItemImageUploadQuery>,
    headers: HeaderMap,
    uri: Uri,
    body: Bytes,
) -> Result<StatusCode, AppError> {
    let user = authenticate_request_user(&state, &headers, &uri).await?;
    ensure_remote_image_admin(&user)?;
    let input = item_image_upload_input(&item_id, &image_type, Some(index), query, &body)?;
    ensure_user_can_access_item(&state, user.id, &input.item_id).await?;

    apply_item_image_upload(&state, input, &body).await
}

pub async fn delete_item_image(
    State(state): State<AppState>,
    AxumPath((item_id, image_type)): AxumPath<(String, String)>,
    Query(query): Query<ItemImageDeleteQuery>,
    headers: HeaderMap,
    uri: Uri,
) -> Result<StatusCode, AppError> {
    let user = authenticate_request_user(&state, &headers, &uri).await?;
    ensure_remote_image_admin(&user)?;
    let input = item_image_delete_input(&item_id, &image_type, None, query)?;
    ensure_user_can_access_item(&state, user.id, &input.item_id).await?;

    apply_item_image_delete(&state, input).await
}

pub async fn delete_item_image_index(
    State(state): State<AppState>,
    AxumPath((item_id, image_type, index)): AxumPath<(String, String, i64)>,
    Query(query): Query<ItemImageDeleteQuery>,
    headers: HeaderMap,
    uri: Uri,
) -> Result<StatusCode, AppError> {
    let user = authenticate_request_user(&state, &headers, &uri).await?;
    ensure_remote_image_admin(&user)?;
    let input = item_image_delete_input(&item_id, &image_type, Some(index), query)?;
    ensure_user_can_access_item(&state, user.id, &input.item_id).await?;

    apply_item_image_delete(&state, input).await
}

pub async fn reindex_item_image(
    State(state): State<AppState>,
    AxumPath((item_id, image_type, index)): AxumPath<(String, String, i64)>,
    Query(query): Query<ItemImageIndexQuery>,
    headers: HeaderMap,
    uri: Uri,
) -> Result<StatusCode, AppError> {
    let user = authenticate_request_user(&state, &headers, &uri).await?;
    ensure_remote_image_admin(&user)?;
    let input = item_image_reindex_input(&item_id, &image_type, index, query)?;
    ensure_user_can_access_item(&state, user.id, &input.item_id).await?;
    let Some(database) = state.database() else {
        return Err(AppError::internal("database is not configured"));
    };
    let artwork_types = artwork_types_for_emby(&input.image_type)
        .ok_or_else(|| AppError::unprocessable("unsupported image type"))?;

    let moved = MediaRepository::new(database.clone())
        .reindex_item_artwork(
            &input.item_id,
            &artwork_types,
            i64::from(input.image_index.unwrap_or(0)),
            i64::from(input.new_index.unwrap_or(0)),
        )
        .await
        .map_err(|err| AppError::internal(format!("failed to reindex item image: {err}")))?;
    if !moved {
        return Err(AppError::not_found("item image not found"));
    }

    Ok(StatusCode::NO_CONTENT)
}

pub async fn update_item_image_url(
    State(state): State<AppState>,
    AxumPath((item_id, image_type, index)): AxumPath<(String, String, i64)>,
    Query(query): Query<ItemImageUrlQuery>,
    headers: HeaderMap,
    uri: Uri,
) -> Result<StatusCode, AppError> {
    let user = authenticate_request_user(&state, &headers, &uri).await?;
    ensure_remote_image_admin(&user)?;
    let input = item_image_url_input(&item_id, &image_type, index, query)?;
    ensure_user_can_access_item(&state, user.id, &input.item_id).await?;
    let Some(database) = state.database() else {
        return Err(AppError::internal("database is not configured"));
    };
    let artwork_types = artwork_types_for_emby(&input.image_type)
        .ok_or_else(|| AppError::unprocessable("unsupported image type"))?;
    let image_url = input
        .image_url
        .as_deref()
        .ok_or_else(|| AppError::unprocessable("Url is required"))?;
    ensure_public_remote_image_url(image_url)?;

    let applied = MediaRepository::new(database.clone())
        .update_item_artwork_url(
            &input.item_id,
            &artwork_types,
            i64::from(input.image_index.unwrap_or(0)),
            image_url,
        )
        .await
        .map_err(|err| AppError::internal(format!("failed to update item image url: {err}")))?;
    if !applied {
        return Err(AppError::not_found("item not found"));
    }

    Ok(StatusCode::NO_CONTENT)
}

pub async fn item_images(
    State(state): State<AppState>,
    AxumPath(item_id): AxumPath<String>,
    headers: HeaderMap,
    uri: Uri,
) -> Result<Json<Vec<ItemImageInfoDto>>, AppError> {
    let user = authenticate_request_user(&state, &headers, &uri).await?;
    ensure_user_can_access_item(&state, user.id, &item_id).await?;
    let Some(database) = state.database() else {
        return Err(AppError::internal("database is not configured"));
    };

    let artwork = MediaRepository::new(database.clone())
        .list_item_artwork(user.id, &item_id)
        .await
        .map_err(|err| AppError::internal(format!("failed to list item images: {err}")))?;

    Ok(Json(image_infos_from_artwork(artwork)))
}

pub async fn remote_images(
    State(state): State<AppState>,
    AxumPath(item_id): AxumPath<String>,
    Query(query): Query<RemoteImagesQuery>,
    headers: HeaderMap,
    uri: Uri,
) -> Result<Json<RemoteImageResultDto>, AppError> {
    let user = authenticate_request_user(&state, &headers, &uri).await?;
    let input = remote_images_input(query)?;
    ensure_user_can_access_item(&state, user.id, &item_id).await?;
    let Some(database) = state.database() else {
        return Err(AppError::internal("database is not configured"));
    };

    // Provider 过滤：显式指定且不是 TMDB → 空结果（当前只接了 TMDB 的图片目录）。
    if let Some(provider) = input.provider_name.as_deref()
        && !matches!(
            provider.trim().to_ascii_lowercase().as_str(),
            "tmdb" | "themoviedb"
        )
    {
        return Ok(Json(empty_remote_image_result(input)));
    }

    let Some(item) = LibraryRepository::new(database.clone())
        .find_user_item_by_id(user.id, &item_id)
        .await
        .map_err(|err| AppError::internal(format!("failed to get media item: {err}")))?
    else {
        return Err(AppError::not_found("item not found"));
    };
    let Some(tmdb_id) = MediaRepository::new(database.clone())
        .find_item_external_id(&item_id, "tmdb")
        .await
        .map_err(|err| AppError::internal(format!("failed to get external id: {err}")))?
    else {
        return Ok(Json(empty_remote_image_result(input)));
    };

    let config = state.config();
    let candidates = crate::metadata::remote_search::RemoteSearchService::new(
        database.clone(),
        config.metadata.clone(),
        config.proxy.clone(),
        config.secrets.clone(),
    )
    .list_images(&item.item_type, &tmdb_id)
    .await
    .map_err(|err| AppError::internal(format!("remote image listing failed: {err}")))?;

    let type_filter = input.image_type.clone();
    let filtered = candidates
        .into_iter()
        .filter(|candidate| {
            type_filter
                .as_deref()
                .is_none_or(|image_type| candidate.image_type.eq_ignore_ascii_case(image_type))
        })
        .collect::<Vec<_>>();
    let total = filtered.len() as u32;
    let images = filtered
        .into_iter()
        .skip(input.start_index as usize)
        .take(input.limit.max(1) as usize)
        .map(|candidate| RemoteImageInfoDto {
            provider_name: Some("TheMovieDb".to_owned()),
            url: Some(candidate.url),
            thumbnail_url: candidate.thumbnail_url,
            height: candidate.height,
            width: candidate.width,
            community_rating: candidate.community_rating,
            vote_count: candidate.vote_count,
            language: candidate.language.clone(),
            display_language: candidate.language,
            r#type: candidate.image_type,
            rating_type: Some("Score".to_owned()),
        })
        .collect();

    Ok(Json(RemoteImageResultDto {
        images,
        total_record_count: total,
        providers: vec!["TheMovieDb".to_owned()],
    }))
}

pub async fn remote_image_providers(
    State(state): State<AppState>,
    AxumPath(item_id): AxumPath<String>,
    headers: HeaderMap,
    uri: Uri,
) -> Result<Json<Vec<ImageProviderInfoDto>>, AppError> {
    let user = authenticate_request_user(&state, &headers, &uri).await?;
    ensure_user_can_access_item(&state, user.id, &item_id).await?;

    Ok(Json(remote_image_provider_infos()))
}

pub async fn download_remote_image(
    State(state): State<AppState>,
    AxumPath(item_id): AxumPath<String>,
    Query(query): Query<RemoteImageDownloadQuery>,
    headers: HeaderMap,
    uri: Uri,
    body: Bytes,
) -> Result<StatusCode, AppError> {
    let user = authenticate_request_user(&state, &headers, &uri).await?;
    ensure_remote_image_admin(&user)?;
    let body = parse_optional_remote_image_download_body(&headers, &body)?;
    let input = remote_image_download_input(&item_id, query, body)?;
    ensure_user_can_access_item(&state, user.id, &input.item_id).await?;
    let Some(database) = state.database() else {
        return Err(AppError::internal("database is not configured"));
    };
    let artwork_types = artwork_types_for_emby(&input.image_type)
        .ok_or_else(|| AppError::unprocessable("unsupported image type"))?;
    let image_url = input
        .image_url
        .as_deref()
        .ok_or_else(|| AppError::unprocessable("ImageUrl is required"))?;
    ensure_public_remote_image_url(image_url)?;

    let bytes = fetch_remote_image_bytes(image_url).await?;
    let decoded = decode_item_image_payload(&bytes)?;
    let storage_key = store_item_image_file(
        &state.config().storage.artwork_cache_dir,
        &input.item_id,
        &decoded,
    )
    .await?;

    let inserted = MediaRepository::new(database.clone())
        .insert_item_artwork(InsertArtworkInput {
            item_id: input.item_id.clone(),
            artwork_type: artwork_types[0].clone(),
            source: input
                .provider_name
                .as_deref()
                .unwrap_or("remote")
                .to_ascii_lowercase(),
            storage_key: Some(storage_key),
            remote_url: Some(image_url.to_owned()),
            width: decoded.width,
            height: decoded.height,
            make_primary: true,
        })
        .await
        .map_err(|err| AppError::internal(format!("failed to save downloaded image: {err}")))?;
    if !inserted {
        return Err(AppError::not_found("item not found"));
    }

    Ok(StatusCode::NO_CONTENT)
}

pub async fn remote_image_proxy(
    State(state): State<AppState>,
    Query(query): Query<RemoteImageProxyQuery>,
    headers: HeaderMap,
    uri: Uri,
) -> Result<Response, AppError> {
    let user = authenticate_request_user(&state, &headers, &uri).await?;
    ensure_remote_image_admin(&user)?;
    let image_url = normalize_required_remote_image_url(query.image_url.as_deref())?;
    ensure_public_remote_image_url(&image_url)?;

    remote_image_passthrough_response(&image_url).await
}

/// 拉取远端图片并作为图片响应回传（RemoteSearch/Image 与 Images/Remote 共用）。
pub(super) async fn remote_image_passthrough_response(image_url: &str) -> Result<Response, AppError> {
    let bytes = fetch_remote_image_bytes(image_url).await?;
    let content_type = probe_image_bytes(&bytes)
        .map(|(extension, _, _)| match extension {
            "png" => "image/png",
            "webp" => "image/webp",
            "gif" => "image/gif",
            _ => "image/jpeg",
        })
        .ok_or_else(|| AppError::unprocessable("remote content is not a supported image"))?;

    let mut response = Response::new(Body::from(bytes));
    *response.status_mut() = StatusCode::OK;
    response
        .headers_mut()
        .insert(CONTENT_TYPE, HeaderValue::from_static(content_type));
    response
        .headers_mut()
        .insert(CACHE_CONTROL, HeaderValue::from_static(IMAGE_CACHE_CONTROL));

    Ok(response)
}

pub async fn global_image_catalog(
    State(state): State<AppState>,
    headers: HeaderMap,
    uri: Uri,
) -> Result<Json<Vec<GlobalImageInfoDto>>, AppError> {
    authenticate_request_user(&state, &headers, &uri).await?;
    let _kind = global_image_catalog_kind(uri.path())?;

    Ok(Json(Vec::new()))
}

pub async fn general_image(
    State(state): State<AppState>,
    AxumPath((name, image_type)): AxumPath<(String, String)>,
    headers: HeaderMap,
    uri: Uri,
) -> Result<Response, AppError> {
    authenticate_request_user(&state, &headers, &uri).await?;
    let input = general_image_input(&name, &image_type)?;
    let _ = (&input.name, &input.image_type);

    Err(AppError::not_found("general image not found"))
}

pub async fn themed_global_image(
    State(state): State<AppState>,
    AxumPath((theme, name)): AxumPath<(String, String)>,
    headers: HeaderMap,
    uri: Uri,
) -> Result<Response, AppError> {
    authenticate_request_user(&state, &headers, &uri).await?;
    let input = themed_global_image_input(uri.path(), &theme, &name)?;
    let _ = (&input.kind, &input.theme, &input.name);

    Err(AppError::not_found("global image not found"))
}

async fn named_entity_image_response(
    state: AppState,
    name: String,
    image_type: String,
    index: Option<i64>,
    headers: HeaderMap,
    uri: Uri,
) -> Result<Response, AppError> {
    let user = authenticate_request_user(&state, &headers, &uri).await?;
    let input = named_entity_image_input(uri.path(), &name, &image_type, index)?;
    ensure_named_entity_visible(&state, user.id, &input).await?;

    // 人物头像：查 artwork 表并直通 remote_url（TMDB CDN），跟条目海报同机制。
    // 其他 kind（Artist/Genre/Studio）本次不支持，仍返回 not_found。
    if input.kind == NamedEntityImageKind::Person {
        let artwork_types = artwork_types_for_emby(&input.image_type)
            .ok_or_else(|| AppError::unprocessable("unsupported image type"))?;
        let Some(database) = state.database() else {
            return Err(AppError::internal("database is not configured"));
        };

        let artwork = MediaRepository::new(database.clone())
            .find_person_artwork_by_name(
                user.id,
                &input.name,
                &artwork_types,
                i64::from(input.index),
            )
            .await
            .map_err(|err| AppError::internal(format!("failed to get person image: {err}")))?;

        if let Some(artwork) = artwork {
            return artwork_response(&state.config().storage, artwork, None).await;
        }
    }

    Err(AppError::not_found("named entity image not found"))
}

async fn user_image_response(
    state: AppState,
    user_id: String,
    image_type: String,
    index: Option<i64>,
    headers: HeaderMap,
    uri: Uri,
) -> Result<Response, AppError> {
    let user = authenticate_request_user(&state, &headers, &uri).await?;
    let input = user_image_input(&user_id, &image_type, index)?;
    ensure_user_image_target_visible(&state, &user, &input.user_id).await?;
    let _ = (&input.image_type, input.index);

    Err(AppError::not_found("user image not found"))
}

async fn item_image_by_index(
    state: AppState,
    item_id: String,
    image_type: String,
    index: i64,
    headers: HeaderMap,
    uri: Uri,
    transform: Option<ImageTransformRequest>,
) -> Result<Response, AppError> {
    let user = authenticate_request_user(&state, &headers, &uri).await?;
    let artwork_types = artwork_types_for_emby(&image_type)
        .ok_or_else(|| AppError::unprocessable("unsupported image type"))?;
    let Some(database) = state.database() else {
        return Err(AppError::internal("database is not configured"));
    };

    let repository = MediaRepository::new(database.clone());
    let artwork = repository
        .find_item_artwork(user.id, &item_id, &artwork_types, index)
        .await
        .map_err(|err| AppError::internal(format!("failed to get item image: {err}")))?;

    if let Some(artwork) = artwork {
        return artwork_response(&state.config().storage, artwork, transform).await;
    }

    // 回退：照片条目的缩略图不在 artwork 表，而在 photo-thumbnails/{internal_id}.jpg
    // （photo worker 生成）。对 Primary/Thumb 的首图请求回退到它，让 Emby 客户端能渲染照片。
    let normalized = image_type.trim().to_ascii_lowercase();
    let thumbnail_eligible = index == 0
        && matches!(
            normalized.as_str(),
            "primary" | "poster" | "thumb" | "thumbnail"
        );
    if thumbnail_eligible
        && let Some(internal_id) = repository
            .find_photo_thumbnail_item_id(user.id, &item_id)
            .await
            .map_err(|err| {
                AppError::internal(format!("failed to resolve photo thumbnail: {err}"))
            })?
    {
        let storage_key = format!("photo-thumbnails/{internal_id}.jpg");
        return local_artwork_response(
            &state.config().storage.artwork_cache_dir,
            &storage_key,
            transform,
        )
        .await;
    }

    Err(AppError::not_found("item image not found"))
}

async fn artwork_response(
    storage: &StorageConfig,
    artwork: ArtworkRecord,
    transform: Option<ImageTransformRequest>,
) -> Result<Response, AppError> {
    if let Some(storage_key) = artwork.storage_key.as_deref() {
        match local_artwork_response(&storage.artwork_cache_dir, storage_key, transform).await {
            Ok(response) => return Ok(response),
            Err(err)
                if artwork.remote_url.is_some() && err.status_code() == StatusCode::NOT_FOUND => {}
            Err(err) => return Err(err),
        }
    }

    let Some(remote_url) = artwork.remote_url else {
        return Err(AppError::not_found("item image file not found"));
    };

    remote_artwork_response(&remote_url)
}

async fn ensure_user_can_access_item(
    state: &AppState,
    user_id: i64,
    item_id: &str,
) -> Result<(), AppError> {
    let Some(database) = state.database() else {
        return Err(AppError::internal("database is not configured"));
    };

    let item = LibraryRepository::new(database.clone())
        .find_user_item_by_id(user_id, item_id)
        .await
        .map_err(|err| AppError::internal(format!("failed to get media item: {err}")))?;
    if item.is_none() {
        return Err(AppError::not_found("item not found"));
    }

    Ok(())
}

async fn ensure_named_entity_visible(
    state: &AppState,
    user_id: i64,
    input: &NamedEntityImageInput,
) -> Result<(), AppError> {
    let Some(database) = state.database() else {
        return Err(AppError::internal("database is not configured"));
    };

    let repository = LibraryRepository::new(database.clone());
    let visible = match input.kind {
        NamedEntityImageKind::Artist => repository
            .find_user_artist_by_name(user_id, &input.name, false)
            .await
            .map_err(|err| AppError::internal(format!("failed to get artist: {err}")))?
            .is_some(),
        NamedEntityImageKind::Genre => repository
            .find_user_genre_by_name(user_id, &input.name, false)
            .await
            .map_err(|err| AppError::internal(format!("failed to get genre: {err}")))?
            .is_some(),
        NamedEntityImageKind::MusicGenre => repository
            .find_user_genre_by_name(user_id, &input.name, true)
            .await
            .map_err(|err| AppError::internal(format!("failed to get music genre: {err}")))?
            .is_some(),
        NamedEntityImageKind::Person => repository
            .find_user_person_by_name(user_id, &input.name)
            .await
            .map_err(|err| AppError::internal(format!("failed to get person: {err}")))?
            .is_some(),
        NamedEntityImageKind::Studio => repository
            .find_user_studio_by_name(user_id, &input.name)
            .await
            .map_err(|err| AppError::internal(format!("failed to get studio: {err}")))?
            .is_some(),
    };

    if !visible {
        return Err(AppError::not_found("named entity not found"));
    }

    Ok(())
}

async fn ensure_user_image_target_visible(
    state: &AppState,
    authenticated: &AuthenticatedUser,
    requested_user_id: &str,
) -> Result<(), AppError> {
    if authenticated.public_id != requested_user_id && !authenticated.can_manage_server() {
        return Err(AppError::forbidden(
            "authenticated user does not match requested user",
        ));
    }

    let Some(database) = state.database() else {
        return Err(AppError::internal("database is not configured"));
    };

    let exists = UsersRepository::new(database.clone())
        .find_user_by_public_id(requested_user_id)
        .await
        .map_err(|err| AppError::internal(format!("failed to get user: {err}")))?
        .is_some();
    if !exists {
        return Err(AppError::not_found("user not found"));
    }

    Ok(())
}

async fn local_artwork_response(
    artwork_cache_dir: &Path,
    storage_key: &str,
    transform: Option<ImageTransformRequest>,
) -> Result<Response, AppError> {
    let relative_path = safe_storage_key_path(storage_key)?;
    let base = tokio::fs::canonicalize(artwork_cache_dir)
        .await
        .map_err(|err| artwork_io_error(err, "artwork cache directory not found"))?;
    let path = tokio::fs::canonicalize(base.join(&relative_path))
        .await
        .map_err(|err| artwork_io_error(err, "item image file not found"))?;
    if !path.starts_with(&base) {
        return Err(AppError::forbidden(
            "artwork path is outside cache directory",
        ));
    }

    let metadata = tokio::fs::metadata(&path)
        .await
        .map_err(|err| artwork_io_error(err, "item image file not found"))?;
    if !metadata.is_file() {
        return Err(AppError::not_found("item image file not found"));
    }

    if let Some(transform) = transform {
        let derived_path = derived_artwork_path(&base, &relative_path, transform)?;
        ensure_derived_artwork(&path, &derived_path, transform).await?;
        let derived_path = tokio::fs::canonicalize(&derived_path)
            .await
            .map_err(|err| artwork_io_error(err, "item image file not found"))?;
        if !derived_path.starts_with(&base) {
            return Err(AppError::forbidden(
                "derived artwork path is outside cache directory",
            ));
        }
        return local_file_response(&derived_path).await;
    }

    local_file_response(&path).await
}

async fn local_file_response(path: &Path) -> Result<Response, AppError> {
    let metadata = tokio::fs::metadata(path)
        .await
        .map_err(|err| artwork_io_error(err, "item image file not found"))?;
    if !metadata.is_file() {
        return Err(AppError::not_found("item image file not found"));
    }
    let file = File::open(&path)
        .await
        .map_err(|err| artwork_io_error(err, "item image file not found"))?;
    let stream = ReaderStream::new(file);
    let mut response = Response::new(Body::from_stream(stream));
    *response.status_mut() = StatusCode::OK;
    response.headers_mut().insert(
        CONTENT_TYPE,
        HeaderValue::from_static(image_content_type(&path)),
    );
    response
        .headers_mut()
        .insert(CACHE_CONTROL, HeaderValue::from_static(IMAGE_CACHE_CONTROL));
    if let Ok(value) = HeaderValue::from_str(&metadata.len().to_string()) {
        response.headers_mut().insert(CONTENT_LENGTH, value);
    }

    Ok(response)
}

async fn ensure_derived_artwork(
    source_path: &Path,
    derived_path: &Path,
    transform: ImageTransformRequest,
) -> Result<(), AppError> {
    if tokio::fs::metadata(derived_path)
        .await
        .map(|metadata| metadata.is_file())
        .unwrap_or(false)
    {
        return Ok(());
    }

    let source_path = source_path.to_owned();
    let derived_path = derived_path.to_owned();
    tokio::task::spawn_blocking(move || -> Result<(), String> {
        let bytes =
            std::fs::read(&source_path).map_err(|err| format!("failed to read source: {err}"))?;
        let output = encode_transformed_artwork(&bytes, transform)?;
        if let Some(parent) = derived_path.parent() {
            std::fs::create_dir_all(parent)
                .map_err(|err| format!("failed to create derived artwork directory: {err}"))?;
        }
        std::fs::write(&derived_path, output)
            .map_err(|err| format!("failed to write derived artwork: {err}"))?;
        Ok(())
    })
    .await
    .map_err(|err| AppError::internal(format!("failed to render derived artwork: {err}")))?
    .map_err(|err| AppError::internal(format!("failed to render derived artwork: {err}")))
}

fn encode_transformed_artwork(
    bytes: &[u8],
    transform: ImageTransformRequest,
) -> Result<Vec<u8>, String> {
    let image = ImageReader::new(Cursor::new(bytes))
        .with_guessed_format()
        .map_err(|err| format!("image format detection failed: {err}"))?
        .decode()
        .map_err(|err| format!("image decode failed: {err}"))?;
    let (target_width, target_height) = transformed_dimensions(
        image.width(),
        image.height(),
        transform.max_width,
        transform.max_height,
    );
    let transformed = if target_width == image.width() && target_height == image.height() {
        image
    } else {
        image.resize(target_width, target_height, FilterType::Lanczos3)
    };

    let mut output = Vec::new();
    let mut encoder = JpegEncoder::new_with_quality(&mut output, transform.quality);
    encoder
        .encode_image(&transformed.to_rgb8())
        .map_err(|err| format!("image encode failed: {err}"))?;
    Ok(output)
}

fn image_transform_from_uri(uri: &Uri) -> Result<Option<ImageTransformRequest>, AppError> {
    let Some(query) = uri.query() else {
        return Ok(None);
    };

    let mut max_width = None;
    let mut max_height = None;
    let mut quality = None;
    for pair in query.split('&') {
        let Some((raw_key, raw_value)) = pair.split_once('=') else {
            continue;
        };
        let key = raw_key.trim();
        let value = raw_value.trim();
        if key.eq_ignore_ascii_case("maxWidth") || key.eq_ignore_ascii_case("max_width") {
            max_width = Some(normalize_query_dimension(value, "MaxWidth")?);
        } else if key.eq_ignore_ascii_case("maxHeight") || key.eq_ignore_ascii_case("max_height") {
            max_height = Some(normalize_query_dimension(value, "MaxHeight")?);
        } else if key.eq_ignore_ascii_case("width") {
            max_width = Some(normalize_query_dimension(value, "Width")?);
        } else if key.eq_ignore_ascii_case("height") {
            max_height = Some(normalize_query_dimension(value, "Height")?);
        } else if key.eq_ignore_ascii_case("quality") {
            quality = Some(normalize_query_quality(value)?);
        }
    }

    if max_width.is_none() && max_height.is_none() && quality.is_none() {
        return Ok(None);
    }

    Ok(Some(ImageTransformRequest::new(
        max_width, max_height, quality,
    )))
}

fn normalize_query_dimension(value: &str, field: &'static str) -> Result<u32, AppError> {
    let parsed = value
        .parse::<i64>()
        .map_err(|_| AppError::unprocessable(format!("{field} is invalid")))?;
    normalize_bounded_positive_i32(parsed, field, MAX_IMAGE_DIMENSION).map(|value| value as u32)
}

fn normalize_query_quality(value: &str) -> Result<u8, AppError> {
    let parsed = value
        .parse::<u8>()
        .map_err(|_| AppError::unprocessable("Quality is invalid"))?;
    Ok(parsed.clamp(MIN_DERIVED_IMAGE_QUALITY, MAX_DERIVED_IMAGE_QUALITY))
}

fn derived_artwork_path(
    base: &Path,
    relative_path: &Path,
    transform: ImageTransformRequest,
) -> Result<PathBuf, AppError> {
    let source_file = relative_path
        .file_name()
        .and_then(|value| value.to_str())
        .filter(|value| !value.is_empty())
        .ok_or_else(|| AppError::unprocessable("artwork storage key is invalid"))?;
    let source_stem = relative_path
        .file_stem()
        .and_then(|value| value.to_str())
        .filter(|value| !value.is_empty())
        .unwrap_or(source_file);
    let parent = relative_path.parent().unwrap_or_else(|| Path::new(""));
    let width = transform
        .max_width
        .map(|value| value.to_string())
        .unwrap_or_else(|| "auto".to_owned());
    let height = transform
        .max_height
        .map(|value| value.to_string())
        .unwrap_or_else(|| "auto".to_owned());
    let derived_file = format!(
        "{source_stem}-mw{width}-mh{height}-q{}.jpg",
        transform.quality
    );
    Ok(base.join("_derived").join(parent).join(derived_file))
}

fn transformed_dimensions(
    original_width: u32,
    original_height: u32,
    max_width: Option<u32>,
    max_height: Option<u32>,
) -> (u32, u32) {
    if original_width == 0 || original_height == 0 {
        return (1, 1);
    }
    let width_ratio = max_width
        .filter(|value| *value > 0)
        .map(|value| value as f64 / original_width as f64)
        .unwrap_or(1.0);
    let height_ratio = max_height
        .filter(|value| *value > 0)
        .map(|value| value as f64 / original_height as f64)
        .unwrap_or(1.0);
    let scale = width_ratio.min(height_ratio).min(1.0);
    let width = ((original_width as f64 * scale).round() as u32).max(1);
    let height = ((original_height as f64 * scale).round() as u32).max(1);
    (width, height)
}

fn parse_optional_remote_image_download_body(
    headers: &HeaderMap,
    body: &Bytes,
) -> Result<RemoteImageDownloadBody, AppError> {
    if body.is_empty() {
        return Ok(RemoteImageDownloadBody::default());
    }

    parse_emby_body(headers, body)
}

fn remote_images_input(query: RemoteImagesQuery) -> Result<RemoteImagesInput, AppError> {
    Ok(RemoteImagesInput {
        image_type: normalize_optional_remote_image_type(query.r#type.as_deref())?,
        start_index: query
            .start_index
            .unwrap_or(0)
            .min(MAX_REMOTE_IMAGE_START_INDEX),
        limit: query.limit.unwrap_or(20).min(MAX_REMOTE_IMAGE_LIMIT),
        provider_name: normalize_optional_remote_image_text(
            "ProviderName",
            query.provider_name.as_deref(),
            MAX_REMOTE_IMAGE_PROVIDER_NAME_LEN,
        )?,
        include_all_languages: query.include_all_languages.unwrap_or(false),
        enable_series_images: query.enable_series_images.unwrap_or(false),
    })
}

fn remote_image_download_input(
    item_id: &str,
    query: RemoteImageDownloadQuery,
    body: RemoteImageDownloadBody,
) -> Result<RemoteImageDownloadInput, AppError> {
    let image_index = body.image_index;
    if matches!(image_index, Some(value) if value < 0) {
        return Err(AppError::unprocessable("ImageIndex is invalid"));
    }

    Ok(RemoteImageDownloadInput {
        item_id: normalize_required_path_text("item id", item_id, MAX_ITEM_ID_LEN)?,
        image_type: normalize_required_remote_image_type(query.r#type.as_deref())?,
        provider_name: normalize_optional_remote_image_text(
            "ProviderName",
            query.provider_name.as_deref(),
            MAX_REMOTE_IMAGE_PROVIDER_NAME_LEN,
        )?,
        image_url: normalize_optional_remote_image_url(query.image_url.as_deref())?,
        image_index,
    })
}

fn item_image_upload_input(
    item_id: &str,
    image_type: &str,
    path_index: Option<i64>,
    query: ItemImageUploadQuery,
    body: &Bytes,
) -> Result<ItemImageMutationInput, AppError> {
    ensure_item_image_body_size(body)?;
    Ok(ItemImageMutationInput {
        item_id: normalize_required_path_text("item id", item_id, MAX_ITEM_ID_LEN)?,
        image_type: normalize_required_remote_image_type(Some(image_type))?,
        image_index: normalize_item_image_index(path_index, query.index)?,
        new_index: None,
        image_url: None,
        body_len: Some(body.len()),
    })
}

fn item_image_delete_input(
    item_id: &str,
    image_type: &str,
    path_index: Option<i64>,
    query: ItemImageDeleteQuery,
) -> Result<ItemImageMutationInput, AppError> {
    Ok(ItemImageMutationInput {
        item_id: normalize_required_path_text("item id", item_id, MAX_ITEM_ID_LEN)?,
        image_type: normalize_required_remote_image_type(Some(image_type))?,
        image_index: normalize_item_image_index(path_index, query.index)?,
        new_index: None,
        image_url: None,
        body_len: None,
    })
}

fn item_image_reindex_input(
    item_id: &str,
    image_type: &str,
    path_index: i64,
    query: ItemImageIndexQuery,
) -> Result<ItemImageMutationInput, AppError> {
    let new_index = query
        .new_index
        .ok_or_else(|| AppError::unprocessable("NewIndex is required"))
        .and_then(|index| normalize_nonnegative_i32_index(i64::from(index), "NewIndex"))?;

    Ok(ItemImageMutationInput {
        item_id: normalize_required_path_text("item id", item_id, MAX_ITEM_ID_LEN)?,
        image_type: normalize_required_remote_image_type(Some(image_type))?,
        image_index: Some(normalize_nonnegative_i32_index(path_index, "Index")?),
        new_index: Some(new_index),
        image_url: None,
        body_len: None,
    })
}

fn item_image_url_input(
    item_id: &str,
    image_type: &str,
    path_index: i64,
    query: ItemImageUrlQuery,
) -> Result<ItemImageMutationInput, AppError> {
    Ok(ItemImageMutationInput {
        item_id: normalize_required_path_text("item id", item_id, MAX_ITEM_ID_LEN)?,
        image_type: normalize_required_remote_image_type(Some(image_type))?,
        image_index: Some(normalize_nonnegative_i32_index(path_index, "Index")?),
        new_index: None,
        image_url: Some(normalize_required_remote_image_url(query.url.as_deref())?),
        body_len: None,
    })
}

fn named_entity_image_input(
    path: &str,
    name: &str,
    image_type: &str,
    index: Option<i64>,
) -> Result<NamedEntityImageInput, AppError> {
    Ok(NamedEntityImageInput {
        kind: named_entity_image_kind_from_path(path)?,
        name: normalize_required_path_text("Name", name, MAX_ENTITY_IMAGE_NAME_LEN)?,
        image_type: normalize_required_remote_image_type(Some(image_type))?,
        index: normalize_nonnegative_i32_index(index.unwrap_or(0), "Index")?,
    })
}

fn user_image_input(
    user_id: &str,
    image_type: &str,
    index: Option<i64>,
) -> Result<UserImageInput, AppError> {
    Ok(UserImageInput {
        user_id: normalize_required_path_text("Id", user_id, MAX_USER_IMAGE_ID_LEN)?,
        image_type: normalize_required_remote_image_type(Some(image_type))?,
        index: normalize_nonnegative_i32_index(index.unwrap_or(0), "Index")?,
    })
}

#[allow(clippy::too_many_arguments)]
fn long_form_item_image_input(
    item_id: &str,
    image_type: &str,
    index: i64,
    tag: &str,
    format: &str,
    max_width: i64,
    max_height: i64,
    percent_played: i64,
    unplayed_count: i64,
) -> Result<LongFormItemImageInput, AppError> {
    Ok(LongFormItemImageInput {
        item_id: normalize_required_path_text("item id", item_id, MAX_ITEM_ID_LEN)?,
        image_type: normalize_required_remote_image_type(Some(image_type))?,
        image_index: normalize_nonnegative_i32_index(index, "Index")?,
        tag: normalize_required_path_text("Tag", tag, MAX_IMAGE_CACHE_TAG_LEN)?,
        format: normalize_image_format(format)?,
        max_width: normalize_bounded_positive_i32(max_width, "MaxWidth", MAX_IMAGE_DIMENSION)?,
        max_height: normalize_bounded_positive_i32(max_height, "MaxHeight", MAX_IMAGE_DIMENSION)?,
        percent_played: normalize_bounded_nonnegative_i32(
            percent_played,
            "PercentPlayed",
            MAX_IMAGE_PERCENT_PLAYED,
        )?,
        unplayed_count: normalize_bounded_nonnegative_i32(
            unplayed_count,
            "UnplayedCount",
            MAX_IMAGE_UNPLAYED_COUNT,
        )?,
    })
}

fn global_image_catalog_kind(path: &str) -> Result<GlobalImageKind, AppError> {
    global_image_kind_from_path(path)
}

fn general_image_input(name: &str, image_type: &str) -> Result<GeneralImageInput, AppError> {
    Ok(GeneralImageInput {
        name: normalize_required_path_text("Name", name, MAX_GLOBAL_IMAGE_TEXT_LEN)?,
        image_type: normalize_image_format(image_type)?,
    })
}

fn themed_global_image_input(
    path: &str,
    theme: &str,
    name: &str,
) -> Result<ThemedGlobalImageInput, AppError> {
    Ok(ThemedGlobalImageInput {
        kind: global_image_kind_from_path(path)?,
        theme: normalize_required_path_text("Theme", theme, MAX_GLOBAL_IMAGE_TEXT_LEN)?,
        name: normalize_required_path_text("Name", name, MAX_GLOBAL_IMAGE_TEXT_LEN)?,
    })
}

fn global_image_kind_from_path(path: &str) -> Result<GlobalImageKind, AppError> {
    let mut segments = path.trim_start_matches('/').split('/');
    let first = segments.next().unwrap_or_default();
    let image_segment = if first.eq_ignore_ascii_case("emby") {
        segments.next().unwrap_or_default()
    } else {
        first
    };
    if !image_segment.eq_ignore_ascii_case("images") {
        return Err(AppError::not_found("global image route not found"));
    }
    let kind = segments.next().unwrap_or_default();

    match kind.to_ascii_lowercase().as_str() {
        "general" => Ok(GlobalImageKind::General),
        "mediainfo" => Ok(GlobalImageKind::MediaInfo),
        "ratings" => Ok(GlobalImageKind::Ratings),
        _ => Err(AppError::not_found("global image route not found")),
    }
}

fn named_entity_image_kind_from_path(path: &str) -> Result<NamedEntityImageKind, AppError> {
    let mut segments = path.trim_start_matches('/').split('/');
    let first = segments.next().unwrap_or_default();
    let segment = if first.eq_ignore_ascii_case("emby") {
        segments.next().unwrap_or_default()
    } else {
        first
    };

    match segment.to_ascii_lowercase().as_str() {
        "artists" => Ok(NamedEntityImageKind::Artist),
        "genres" => Ok(NamedEntityImageKind::Genre),
        "musicgenres" => Ok(NamedEntityImageKind::MusicGenre),
        "persons" => Ok(NamedEntityImageKind::Person),
        "studios" => Ok(NamedEntityImageKind::Studio),
        _ => Err(AppError::not_found("named entity image route not found")),
    }
}

fn normalize_image_format(value: &str) -> Result<String, AppError> {
    let format = value.trim().trim_start_matches('.').to_ascii_lowercase();
    match format.as_str() {
        "jpg" | "jpeg" | "png" | "webp" | "gif" | "bmp" | "avif" => Ok(format),
        _ => Err(AppError::unprocessable("Format is invalid")),
    }
}

fn normalize_bounded_positive_i32(
    value: i64,
    field: &'static str,
    max: i64,
) -> Result<i32, AppError> {
    if value <= 0 || value > max {
        return Err(AppError::unprocessable(format!("{field} is invalid")));
    }

    Ok(value as i32)
}

fn normalize_bounded_nonnegative_i32(
    value: i64,
    field: &'static str,
    max: i64,
) -> Result<i32, AppError> {
    if value < 0 || value > max {
        return Err(AppError::unprocessable(format!("{field} is invalid")));
    }

    Ok(value as i32)
}

fn normalize_item_image_index(
    path_index: Option<i64>,
    query_index: Option<i32>,
) -> Result<Option<i32>, AppError> {
    match (path_index, query_index) {
        (Some(path_index), Some(query_index)) => {
            let path_index = normalize_nonnegative_i32_index(path_index, "Index")?;
            let query_index = normalize_nonnegative_i32_index(i64::from(query_index), "Index")?;
            if path_index != query_index {
                return Err(AppError::unprocessable("Index is invalid"));
            }
            Ok(Some(path_index))
        }
        (Some(path_index), None) => Ok(Some(normalize_nonnegative_i32_index(path_index, "Index")?)),
        (None, Some(query_index)) => Ok(Some(normalize_nonnegative_i32_index(
            i64::from(query_index),
            "Index",
        )?)),
        (None, None) => Ok(None),
    }
}

fn normalize_nonnegative_i32_index(value: i64, field: &'static str) -> Result<i32, AppError> {
    if value < 0 || value > i64::from(i32::MAX) {
        return Err(AppError::unprocessable(format!("{field} is invalid")));
    }

    Ok(value as i32)
}

fn ensure_item_image_body_size(body: &Bytes) -> Result<(), AppError> {
    if body.len() > MAX_ITEM_IMAGE_MUTATION_BODY_BYTES {
        return Err(AppError::unprocessable("item image payload is too large"));
    }

    Ok(())
}

fn empty_remote_image_result(input: RemoteImagesInput) -> RemoteImageResultDto {
    let _ = (
        input.image_type,
        input.start_index,
        input.limit,
        input.provider_name,
        input.include_all_languages,
        input.enable_series_images,
    );

    RemoteImageResultDto {
        images: Vec::new(),
        total_record_count: 0,
        providers: remote_image_provider_names(),
    }
}

fn remote_image_provider_infos() -> Vec<ImageProviderInfoDto> {
    REMOTE_IMAGE_PROVIDERS
        .iter()
        .map(|(name, supported_images)| ImageProviderInfoDto {
            name: (*name).to_owned(),
            supported_images: supported_images
                .iter()
                .map(|image_type| (*image_type).to_owned())
                .collect(),
        })
        .collect()
}

fn remote_image_provider_names() -> Vec<String> {
    REMOTE_IMAGE_PROVIDERS
        .iter()
        .map(|(name, _)| (*name).to_owned())
        .collect()
}

fn image_infos_from_artwork(artwork: Vec<ArtworkRecord>) -> Vec<ItemImageInfoDto> {
    let mut type_counts: std::collections::BTreeMap<String, i32> =
        std::collections::BTreeMap::new();
    artwork
        .into_iter()
        .filter_map(|artwork| {
            let image_type = emby_image_type_from_artwork_type(&artwork.artwork_type)?;
            let image_index = type_counts.entry(image_type.clone()).or_insert(0);
            let path = artwork.storage_key.or(artwork.remote_url);
            let filename = path.as_deref().and_then(filename_from_artwork_path);
            let info = ItemImageInfoDto {
                image_type,
                image_index: *image_index,
                path,
                filename,
                height: artwork.height,
                width: artwork.width,
                size: None,
            };
            *image_index += 1;
            Some(info)
        })
        .collect()
}

fn emby_image_type_from_artwork_type(artwork_type: &str) -> Option<String> {
    let image_type = match artwork_type.trim().to_ascii_lowercase().as_str() {
        "primary" | "poster" => "Primary",
        "backdrop" => "Backdrop",
        "logo" => "Logo",
        "thumb" => "Thumb",
        "banner" => "Banner",
        "disc" => "Disc",
        "artist" | "album" => "Art",
        _ => return None,
    };

    Some(image_type.to_owned())
}

fn filename_from_artwork_path(path: &str) -> Option<String> {
    let without_query = path.split(['?', '#']).next().unwrap_or(path);
    without_query
        .split(['/', '\\'])
        .next_back()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned)
}

fn normalize_optional_remote_image_type(value: Option<&str>) -> Result<Option<String>, AppError> {
    value.map(normalize_remote_image_type).transpose()
}

fn normalize_required_remote_image_type(value: Option<&str>) -> Result<String, AppError> {
    let Some(value) = value else {
        return Err(AppError::unprocessable("Type is required"));
    };

    normalize_remote_image_type(value)
}

fn normalize_remote_image_type(value: &str) -> Result<String, AppError> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return Err(AppError::unprocessable("Type is required"));
    }

    REMOTE_IMAGE_TYPES
        .iter()
        .find(|candidate| candidate.eq_ignore_ascii_case(trimmed))
        .map(|candidate| (*candidate).to_owned())
        .ok_or_else(|| AppError::unprocessable("Type is invalid"))
}

fn normalize_optional_remote_image_text(
    field: &'static str,
    value: Option<&str>,
    max_len: usize,
) -> Result<Option<String>, AppError> {
    let Some(value) = value else {
        return Ok(None);
    };
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return Ok(None);
    }
    if trimmed.len() > max_len || trimmed.chars().any(char::is_control) {
        return Err(AppError::unprocessable(format!("{field} is invalid")));
    }

    Ok(Some(trimmed.to_owned()))
}

fn normalize_optional_remote_image_url(value: Option<&str>) -> Result<Option<String>, AppError> {
    value.map(normalize_remote_image_url).transpose()
}

fn normalize_required_remote_image_url(value: Option<&str>) -> Result<String, AppError> {
    let Some(value) = value else {
        return Err(AppError::unprocessable("ImageUrl is required"));
    };

    normalize_remote_image_url(value)
}

fn normalize_remote_image_url(value: &str) -> Result<String, AppError> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return Err(AppError::unprocessable("ImageUrl is required"));
    }
    if trimmed.len() > MAX_REMOTE_IMAGE_URL_LEN
        || trimmed
            .chars()
            .any(|ch| ch.is_control() || ch.is_whitespace())
    {
        return Err(AppError::unprocessable("ImageUrl is invalid"));
    }

    let uri = trimmed
        .parse::<Uri>()
        .map_err(|_| AppError::unprocessable("ImageUrl is invalid"))?;
    if !matches!(uri.scheme_str(), Some("http" | "https")) || uri.host().is_none() {
        return Err(AppError::unprocessable("ImageUrl is invalid"));
    }

    Ok(trimmed.to_owned())
}

fn normalize_required_path_text(
    field: &'static str,
    value: &str,
    max_len: usize,
) -> Result<String, AppError> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return Err(AppError::unprocessable(format!("{field} is required")));
    }
    if trimmed.len() > max_len
        || trimmed
            .chars()
            .any(|ch| ch.is_control() || matches!(ch, '/' | '\\'))
    {
        return Err(AppError::unprocessable(format!("{field} is invalid")));
    }

    Ok(trimmed.to_owned())
}

fn ensure_remote_image_admin(user: &AuthenticatedUser) -> Result<(), AppError> {
    if user.can_manage_server() {
        return Ok(());
    }

    Err(AppError::forbidden("server management permission required"))
}

/// 上传主流程：解码（裸字节或 base64）→ 落盘 artwork 缓存 → 写 artwork 行。
/// 无 Index 或 Index=0 时该图成为该类型主图（旧主图降级为备选）。
async fn apply_item_image_upload(
    state: &AppState,
    input: ItemImageMutationInput,
    body: &Bytes,
) -> Result<StatusCode, AppError> {
    let Some(database) = state.database() else {
        return Err(AppError::internal("database is not configured"));
    };
    let artwork_types = artwork_types_for_emby(&input.image_type)
        .ok_or_else(|| AppError::unprocessable("unsupported image type"))?;

    let decoded = decode_item_image_payload(body)?;
    let storage_key = store_item_image_file(
        &state.config().storage.artwork_cache_dir,
        &input.item_id,
        &decoded,
    )
    .await?;

    let make_primary = input.image_index.unwrap_or(0) == 0;
    let inserted = MediaRepository::new(database.clone())
        .insert_item_artwork(InsertArtworkInput {
            item_id: input.item_id.clone(),
            artwork_type: artwork_types[0].clone(),
            source: "upload".to_owned(),
            storage_key: Some(storage_key),
            remote_url: None,
            width: decoded.width,
            height: decoded.height,
            make_primary,
        })
        .await
        .map_err(|err| AppError::internal(format!("failed to save uploaded image: {err}")))?;
    if !inserted {
        return Err(AppError::not_found("item not found"));
    }

    Ok(StatusCode::NO_CONTENT)
}

/// 删除类型序中第 Index 张图；本地缓存文件一并尽力删除（失败不影响响应）。
async fn apply_item_image_delete(
    state: &AppState,
    input: ItemImageMutationInput,
) -> Result<StatusCode, AppError> {
    let Some(database) = state.database() else {
        return Err(AppError::internal("database is not configured"));
    };
    let artwork_types = artwork_types_for_emby(&input.image_type)
        .ok_or_else(|| AppError::unprocessable("unsupported image type"))?;

    let removed = MediaRepository::new(database.clone())
        .delete_item_artwork_at_index(
            &input.item_id,
            &artwork_types,
            i64::from(input.image_index.unwrap_or(0)),
        )
        .await
        .map_err(|err| AppError::internal(format!("failed to delete item image: {err}")))?;

    match removed {
        Some(storage_key) => {
            if let Some(storage_key) = storage_key {
                remove_artwork_cache_file(&state.config().storage.artwork_cache_dir, &storage_key)
                    .await;
            }
            Ok(StatusCode::NO_CONTENT)
        }
        None => Err(AppError::not_found("item image not found")),
    }
}

/// 已解码的上传图片：有效字节（base64 已剥壳）+ 规范扩展名 + 尺寸。
struct DecodedItemImage {
    bytes: Vec<u8>,
    extension: &'static str,
    width: Option<i32>,
    height: Option<i32>,
}

/// Emby 客户端上传图片走 base64 文本体（官方行为），也有客户端直接发裸字节。
/// 先按裸字节识别格式，识别失败再尝试 base64 解码后识别。
fn decode_item_image_payload(body: &[u8]) -> Result<DecodedItemImage, AppError> {
    if body.is_empty() {
        return Err(AppError::unprocessable("image body is required"));
    }

    if let Some(decoded) = probe_image_bytes(body) {
        return Ok(DecodedItemImage {
            bytes: body.to_vec(),
            extension: decoded.0,
            width: decoded.1,
            height: decoded.2,
        });
    }

    let raw = decode_base64_payload(body)
        .ok_or_else(|| AppError::unprocessable("image body is not a supported image"))?;
    let decoded = probe_image_bytes(&raw)
        .ok_or_else(|| AppError::unprocessable("image body is not a supported image"))?;

    Ok(DecodedItemImage {
        bytes: raw,
        extension: decoded.0,
        width: decoded.1,
        height: decoded.2,
    })
}

/// 识别图片格式与尺寸；仅接受 artwork 缓存支持的格式。
fn probe_image_bytes(bytes: &[u8]) -> Option<(&'static str, Option<i32>, Option<i32>)> {
    let reader = ImageReader::new(Cursor::new(bytes))
        .with_guessed_format()
        .ok()?;
    let extension = match reader.format()? {
        image::ImageFormat::Jpeg => "jpg",
        image::ImageFormat::Png => "png",
        image::ImageFormat::WebP => "webp",
        image::ImageFormat::Gif => "gif",
        _ => return None,
    };
    let (width, height) = reader.into_dimensions().ok()?;

    Some((
        extension,
        i32::try_from(width).ok().filter(|value| *value > 0),
        i32::try_from(height).ok().filter(|value| *value > 0),
    ))
}

/// 标准 base64（含 URL-safe 变体）解码，忽略空白，容忍缺省 padding。
fn decode_base64_payload(body: &[u8]) -> Option<Vec<u8>> {
    fn value_of(byte: u8) -> Option<u8> {
        match byte {
            b'A'..=b'Z' => Some(byte - b'A'),
            b'a'..=b'z' => Some(byte - b'a' + 26),
            b'0'..=b'9' => Some(byte - b'0' + 52),
            b'+' | b'-' => Some(62),
            b'/' | b'_' => Some(63),
            _ => None,
        }
    }

    let mut output = Vec::with_capacity(body.len() / 4 * 3);
    let mut buffer = 0u32;
    let mut bits = 0u8;
    for &byte in body {
        if byte.is_ascii_whitespace() || byte == b'=' {
            continue;
        }
        let value = value_of(byte)?;
        buffer = (buffer << 6) | u32::from(value);
        bits += 6;
        if bits >= 8 {
            bits -= 8;
            output.push((buffer >> bits) as u8);
        }
    }

    (!output.is_empty()).then_some(output)
}

/// 上传/下载图片落盘：`uploads/{item uuid}/{纳秒时间戳}.{ext}`，返回 storage_key。
async fn store_item_image_file(
    artwork_cache_dir: &Path,
    item_id: &str,
    decoded: &DecodedItemImage,
) -> Result<String, AppError> {
    let item_segment = item_id
        .chars()
        .filter(|ch| ch.is_ascii_alphanumeric() || *ch == '-')
        .collect::<String>();
    if item_segment.is_empty() {
        return Err(AppError::unprocessable("item id is invalid"));
    }
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|duration| duration.as_nanos())
        .unwrap_or_default();
    let storage_key = format!(
        "uploads/{item_segment}/{nanos}.{extension}",
        extension = decoded.extension
    );

    let output_path = artwork_cache_dir.join(&storage_key);
    if let Some(parent) = output_path.parent() {
        tokio::fs::create_dir_all(parent)
            .await
            .map_err(|err| AppError::internal(format!("failed to create image directory: {err}")))?;
    }
    tokio::fs::write(&output_path, &decoded.bytes)
        .await
        .map_err(|err| AppError::internal(format!("failed to write image file: {err}")))?;

    Ok(storage_key)
}

/// 尽力删除 artwork 缓存文件（拒绝越界 storage_key；失败仅记日志语义，静默）。
async fn remove_artwork_cache_file(artwork_cache_dir: &Path, storage_key: &str) {
    let Ok(relative_path) = safe_storage_key_path(storage_key) else {
        return;
    };
    let _ = tokio::fs::remove_file(artwork_cache_dir.join(relative_path)).await;
}

/// 管理员触发的远端图片抓取仍要做基本 SSRF 防护：仅 http(s)、无凭据、
/// 拒绝回环/私网/链路本地字面量主机。
pub(super) fn ensure_public_remote_image_url(url: &str) -> Result<(), AppError> {
    let parsed = url
        .parse::<Uri>()
        .map_err(|_| AppError::unprocessable("ImageUrl is invalid"))?;
    if !matches!(parsed.scheme_str(), Some("http" | "https")) {
        return Err(AppError::unprocessable("ImageUrl is invalid"));
    }
    let Some(host) = parsed.host() else {
        return Err(AppError::unprocessable("ImageUrl is invalid"));
    };
    let host = host.trim_start_matches('[').trim_end_matches(']');
    if is_private_or_loopback_host(host) {
        return Err(AppError::unprocessable(
            "ImageUrl must point to a public host",
        ));
    }

    Ok(())
}

fn is_private_or_loopback_host(host: &str) -> bool {
    let normalized = host.to_ascii_lowercase();
    if normalized == "localhost" || normalized.ends_with(".localhost") || normalized.ends_with(".local")
    {
        return true;
    }
    if let Ok(address) = normalized.parse::<std::net::IpAddr>() {
        return match address {
            std::net::IpAddr::V4(v4) => {
                v4.is_loopback()
                    || v4.is_private()
                    || v4.is_link_local()
                    || v4.is_unspecified()
                    || v4.is_broadcast()
            }
            std::net::IpAddr::V6(v6) => {
                v6.is_loopback()
                    || v6.is_unspecified()
                    || (v6.segments()[0] & 0xfe00) == 0xfc00
                    || (v6.segments()[0] & 0xffc0) == 0xfe80
            }
        };
    }

    false
}

/// 拉取远端图片字节：20 秒超时、16MiB 上限、要求 2xx。
async fn fetch_remote_image_bytes(url: &str) -> Result<Vec<u8>, AppError> {
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(20))
        .build()
        .map_err(|err| AppError::internal(format!("failed to build HTTP client: {err}")))?;
    let response = client
        .get(url)
        .send()
        .await
        .map_err(|err| AppError::unprocessable(format!("image download failed: {err}")))?;
    if !response.status().is_success() {
        return Err(AppError::unprocessable(format!(
            "image download returned status {}",
            response.status()
        )));
    }
    let bytes = response
        .bytes()
        .await
        .map_err(|err| AppError::unprocessable(format!("image download read failed: {err}")))?;
    if bytes.len() > MAX_ITEM_IMAGE_MUTATION_BODY_BYTES {
        return Err(AppError::unprocessable("downloaded image is too large"));
    }

    Ok(bytes.to_vec())
}

fn remote_artwork_response(remote_url: &str) -> Result<Response, AppError> {
    let location = remote_url.trim();
    let uri = location
        .parse::<Uri>()
        .map_err(|_| AppError::unprocessable("artwork remote URL is invalid"))?;
    if !matches!(uri.scheme_str(), Some("http" | "https")) || uri.host().is_none() {
        return Err(AppError::unprocessable("artwork remote URL is invalid"));
    }

    let location = HeaderValue::from_str(location)
        .map_err(|_| AppError::unprocessable("artwork remote URL is invalid"))?;
    let mut response = Response::new(Body::empty());
    *response.status_mut() = StatusCode::FOUND;
    response.headers_mut().insert(LOCATION, location);
    response
        .headers_mut()
        .insert(CACHE_CONTROL, HeaderValue::from_static(IMAGE_CACHE_CONTROL));
    Ok(response)
}

fn artwork_types_for_emby(image_type: &str) -> Option<Vec<String>> {
    let types = match image_type.trim().to_ascii_lowercase().as_str() {
        "primary" => ["primary", "poster"].as_slice(),
        "poster" => ["poster", "primary"].as_slice(),
        "backdrop" | "background" => ["backdrop"].as_slice(),
        "logo" => ["logo"].as_slice(),
        "thumb" | "thumbnail" => ["thumb"].as_slice(),
        "banner" => ["banner"].as_slice(),
        "disc" | "discart" => ["disc"].as_slice(),
        "art" => ["artist", "album"].as_slice(),
        _ => return None,
    };

    Some(types.iter().map(|value| (*value).to_owned()).collect())
}

fn safe_storage_key_path(storage_key: &str) -> Result<PathBuf, AppError> {
    let trimmed = storage_key.trim();
    if trimmed.is_empty() {
        return Err(AppError::unprocessable("artwork storage key is required"));
    }

    let path = Path::new(trimmed);
    let mut safe = PathBuf::new();
    for component in path.components() {
        match component {
            Component::Normal(part) => safe.push(part),
            Component::CurDir => {}
            Component::ParentDir | Component::RootDir | Component::Prefix(_) => {
                return Err(AppError::forbidden("artwork storage key is invalid"));
            }
        }
    }

    if safe.as_os_str().is_empty() {
        return Err(AppError::unprocessable("artwork storage key is required"));
    }

    Ok(safe)
}

fn image_content_type(path: &Path) -> &'static str {
    match path
        .extension()
        .and_then(|value| value.to_str())
        .unwrap_or_default()
        .to_ascii_lowercase()
        .as_str()
    {
        "jpg" | "jpeg" => "image/jpeg",
        "png" => "image/png",
        "webp" => "image/webp",
        "gif" => "image/gif",
        "bmp" => "image/bmp",
        "avif" => "image/avif",
        _ => "application/octet-stream",
    }
}

fn artwork_io_error(error: std::io::Error, not_found_message: &'static str) -> AppError {
    if error.kind() == std::io::ErrorKind::NotFound {
        return AppError::not_found(not_found_message);
    }

    AppError::internal(format!("failed to read item image: {error}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn emby_image_type_mapping_supports_common_item_images() {
        assert_eq!(
            artwork_types_for_emby("Primary").unwrap(),
            ["primary", "poster"]
        );
        assert_eq!(
            artwork_types_for_emby("Poster").unwrap(),
            ["poster", "primary"]
        );
        assert_eq!(artwork_types_for_emby("Backdrop").unwrap(), ["backdrop"]);
        assert!(artwork_types_for_emby("Unsupported").is_none());
    }

    #[test]
    fn storage_key_rejects_absolute_and_parent_paths() {
        assert!(safe_storage_key_path("../poster.jpg").is_err());
        assert!(safe_storage_key_path("C:/poster.jpg").is_err());
        assert_eq!(
            safe_storage_key_path("movies/1/poster.jpg").unwrap(),
            PathBuf::from("movies").join("1").join("poster.jpg")
        );
    }

    #[test]
    fn image_content_type_uses_extension_allowlist() {
        assert_eq!(image_content_type(Path::new("poster.jpg")), "image/jpeg");
        assert_eq!(image_content_type(Path::new("poster.webp")), "image/webp");
        assert_eq!(
            image_content_type(Path::new("poster.unknown")),
            "application/octet-stream"
        );
    }

    #[test]
    fn remote_artwork_redirect_rejects_non_http_urls() {
        assert!(remote_artwork_response("file:///tmp/poster.jpg").is_err());
        assert!(remote_artwork_response("https://image.example.test/poster.jpg").is_ok());
    }

    #[test]
    fn image_transform_query_accepts_responsive_params_and_clamps_quality() {
        let uri = "/Items/item-1/Images/Primary?api_key=token&maxWidth=500&quality=100"
            .parse::<Uri>()
            .unwrap();
        let transform = image_transform_from_uri(&uri)
            .expect("query should parse")
            .expect("transform should exist");

        assert_eq!(transform.max_width, Some(500));
        assert_eq!(transform.max_height, None);
        assert_eq!(transform.quality, MAX_DERIVED_IMAGE_QUALITY);

        let uri = "/Items/item-1/Images/Primary?width=780&height=1170&quality=20"
            .parse::<Uri>()
            .unwrap();
        let transform = image_transform_from_uri(&uri)
            .expect("query should parse")
            .expect("transform should exist");
        assert_eq!(transform.max_width, Some(780));
        assert_eq!(transform.max_height, Some(1170));
        assert_eq!(transform.quality, MIN_DERIVED_IMAGE_QUALITY);
    }

    #[test]
    fn image_transform_query_ignores_auth_only_query() {
        let uri = "/Items/item-1/Images/Primary?api_key=token"
            .parse::<Uri>()
            .unwrap();

        assert_eq!(image_transform_from_uri(&uri).unwrap(), None);
    }

    #[test]
    fn derived_artwork_path_stays_under_cache_and_uses_jpeg_variant() {
        let path = derived_artwork_path(
            Path::new("var/artwork"),
            Path::new("metadata/42/tmdb/poster.jpg"),
            ImageTransformRequest::new(Some(500), None, Some(78)),
        )
        .expect("derived path should build");

        assert_eq!(
            path,
            PathBuf::from("var/artwork")
                .join("_derived")
                .join("metadata")
                .join("42")
                .join("tmdb")
                .join("poster-mw500-mhauto-q78.jpg")
        );
    }

    #[test]
    fn transformed_dimensions_downscale_without_upscale() {
        assert_eq!(
            transformed_dimensions(1000, 1500, Some(500), None),
            (500, 750)
        );
        assert_eq!(
            transformed_dimensions(1000, 1500, None, Some(300)),
            (200, 300)
        );
        assert_eq!(
            transformed_dimensions(1000, 1500, Some(2000), Some(3000)),
            (1000, 1500)
        );
    }

    #[test]
    fn remote_image_type_normalizes_official_values() {
        assert_eq!(normalize_remote_image_type(" primary ").unwrap(), "Primary");
        assert_eq!(
            normalize_remote_image_type("LogoLightColor").unwrap(),
            "LogoLightColor"
        );
        assert!(normalize_remote_image_type("../Primary").is_err());
    }

    #[test]
    fn remote_image_query_window_clamps_limit() {
        let input = remote_images_input(RemoteImagesQuery {
            r#type: Some("Backdrop".to_owned()),
            start_index: Some(4),
            limit: Some(10_000),
            provider_name: Some(" TheMovieDb ".to_owned()),
            include_all_languages: Some(true),
            enable_series_images: Some(true),
        })
        .expect("remote image query should normalize");

        assert_eq!(input.image_type.as_deref(), Some("Backdrop"));
        assert_eq!(input.start_index, 4);
        assert_eq!(input.limit, MAX_REMOTE_IMAGE_LIMIT);
        assert_eq!(input.provider_name.as_deref(), Some("TheMovieDb"));
        assert!(input.include_all_languages);
        assert!(input.enable_series_images);
    }

    #[test]
    fn remote_image_query_clamps_pathologically_large_start_index() {
        let input = remote_images_input(RemoteImagesQuery {
            start_index: Some(500_000),
            limit: Some(20),
            ..RemoteImagesQuery::default()
        })
        .expect("remote image query should normalize");

        assert_eq!(input.start_index, 10_000);
        assert_eq!(input.limit, 20);
    }

    #[test]
    fn remote_image_result_serializes_official_pascal_case() {
        let value = serde_json::to_value(RemoteImageResultDto {
            images: Vec::new(),
            total_record_count: 0,
            providers: vec!["TheMovieDb".to_owned()],
        })
        .expect("remote image result should serialize");

        assert_eq!(value["Images"], serde_json::json!([]));
        assert_eq!(value["TotalRecordCount"], 0);
        assert_eq!(value["Providers"], serde_json::json!(["TheMovieDb"]));
    }

    #[test]
    fn item_image_infos_serialize_official_shape_and_indexes() {
        let infos = image_infos_from_artwork(vec![
            ArtworkRecord {
                artwork_type: "poster".to_owned(),
                storage_key: Some("movies/1/poster.jpg".to_owned()),
                remote_url: None,
                width: Some(600),
                height: Some(900),
            },
            ArtworkRecord {
                artwork_type: "backdrop".to_owned(),
                storage_key: None,
                remote_url: Some("https://image.example.test/backdrop.jpg".to_owned()),
                width: Some(1920),
                height: Some(1080),
            },
            ArtworkRecord {
                artwork_type: "backdrop".to_owned(),
                storage_key: None,
                remote_url: Some("https://image.example.test/backdrop-2.jpg".to_owned()),
                width: None,
                height: None,
            },
        ]);

        assert_eq!(infos[0].image_type, "Primary");
        assert_eq!(infos[0].image_index, 0);
        assert_eq!(infos[0].path.as_deref(), Some("movies/1/poster.jpg"));
        assert_eq!(infos[0].width, Some(600));
        assert_eq!(infos[0].height, Some(900));
        assert_eq!(infos[1].image_type, "Backdrop");
        assert_eq!(infos[1].image_index, 0);
        assert_eq!(infos[2].image_type, "Backdrop");
        assert_eq!(infos[2].image_index, 1);

        let value = serde_json::to_value(&infos[0]).expect("image info should serialize");
        assert_eq!(value["ImageType"], "Primary");
        assert_eq!(value["ImageIndex"], 0);
        assert_eq!(value["Path"], "movies/1/poster.jpg");
        assert_eq!(value["Filename"], "poster.jpg");
        assert_eq!(value["Width"], 600);
        assert_eq!(value["Height"], 900);
        assert!(value.get("Size").is_some());
    }

    #[test]
    fn item_image_mutation_inputs_normalize_official_fields() {
        let upload = item_image_upload_input(
            " item-1 ",
            " Primary ",
            None,
            ItemImageUploadQuery { index: Some(2) },
            &Bytes::from_static(b"base64-image"),
        )
        .expect("upload input should normalize");

        assert_eq!(upload.item_id, "item-1");
        assert_eq!(upload.image_type, "Primary");
        assert_eq!(upload.image_index, Some(2));
        assert_eq!(upload.body_len, Some("base64-image".len()));

        let delete = item_image_delete_input(
            "item-1",
            "Backdrop",
            Some(1),
            ItemImageDeleteQuery::default(),
        )
        .expect("delete input should normalize");

        assert_eq!(delete.image_type, "Backdrop");
        assert_eq!(delete.image_index, Some(1));

        let reindex = item_image_reindex_input(
            "item-1",
            "Backdrop",
            2,
            ItemImageIndexQuery { new_index: Some(0) },
        )
        .expect("reindex input should normalize");

        assert_eq!(reindex.image_index, Some(2));
        assert_eq!(reindex.new_index, Some(0));

        let url = item_image_url_input(
            "item-1",
            "Backdrop",
            2,
            ItemImageUrlQuery {
                url: Some(" https://image.example.test/backdrop.jpg ".to_owned()),
            },
        )
        .expect("url input should normalize");

        assert_eq!(
            url.image_url.as_deref(),
            Some("https://image.example.test/backdrop.jpg")
        );
    }

    #[test]
    fn item_image_mutation_inputs_reject_unsafe_values() {
        assert_eq!(
            item_image_upload_input(
                "../item",
                "Primary",
                None,
                ItemImageUploadQuery::default(),
                &Bytes::from_static(b"image"),
            )
            .unwrap_err()
            .status_code(),
            StatusCode::UNPROCESSABLE_ENTITY
        );
        assert_eq!(
            item_image_upload_input(
                "item-1",
                "Primary",
                Some(-1),
                ItemImageUploadQuery::default(),
                &Bytes::from_static(b"image"),
            )
            .unwrap_err()
            .status_code(),
            StatusCode::UNPROCESSABLE_ENTITY
        );
        assert_eq!(
            item_image_reindex_input(
                "item-1",
                "Backdrop",
                2,
                ItemImageIndexQuery { new_index: None },
            )
            .unwrap_err()
            .status_code(),
            StatusCode::UNPROCESSABLE_ENTITY
        );
        assert_eq!(
            item_image_url_input(
                "item-1",
                "Backdrop",
                2,
                ItemImageUrlQuery {
                    url: Some("file:///tmp/backdrop.jpg".to_owned()),
                },
            )
            .unwrap_err()
            .status_code(),
            StatusCode::UNPROCESSABLE_ENTITY
        );
        assert_eq!(
            item_image_upload_input(
                "item-1",
                "Primary",
                None,
                ItemImageUploadQuery::default(),
                &Bytes::from(vec![0; MAX_ITEM_IMAGE_MUTATION_BODY_BYTES + 1]),
            )
            .unwrap_err()
            .status_code(),
            StatusCode::UNPROCESSABLE_ENTITY
        );
    }

    #[test]
    fn named_entity_image_input_normalizes_kind_name_type_and_index() {
        let input = named_entity_image_input(
            "/emby/MusicGenres/Rock/Images/primary/2",
            " Rock ",
            " primary ",
            Some(2),
        )
        .expect("named entity image path should normalize");

        assert_eq!(input.kind, NamedEntityImageKind::MusicGenre);
        assert_eq!(input.name, "Rock");
        assert_eq!(input.image_type, "Primary");
        assert_eq!(input.index, 2);
    }

    #[test]
    fn user_image_input_normalizes_user_type_and_rejects_unsafe_values() {
        let input = user_image_input(" user-1 ", "primary", Some(0))
            .expect("user image path should normalize");

        assert_eq!(input.user_id, "user-1");
        assert_eq!(input.image_type, "Primary");
        assert_eq!(input.index, 0);

        assert!(
            named_entity_image_input("/Artists/Bad/Images/Primary", "bad/name", "Primary", None)
                .is_err()
        );
        assert!(user_image_input("user-1", "../Primary", Some(0)).is_err());
        assert!(user_image_input("user-1", "Primary", Some(-1)).is_err());
    }

    #[test]
    fn long_form_item_image_input_normalizes_cache_path_fields() {
        let input =
            long_form_item_image_input(" item-1 ", "primary", 1, " tag-1 ", "jpg", 640, 360, 42, 1)
                .expect("long-form image path should normalize");

        assert_eq!(input.item_id, "item-1");
        assert_eq!(input.image_type, "Primary");
        assert_eq!(input.image_index, 1);
        assert_eq!(input.tag, "tag-1");
        assert_eq!(input.format, "jpg");
        assert_eq!(input.max_width, 640);
        assert_eq!(input.max_height, 360);
        assert_eq!(input.percent_played, 42);
        assert_eq!(input.unplayed_count, 1);

        assert!(
            long_form_item_image_input("item-1", "Primary", 0, "../tag", "jpg", 0, 360, 0, 0,)
                .is_err()
        );
        assert!(
            long_form_item_image_input(
                "item-1",
                "Primary",
                0,
                "tag",
                "bad/format",
                640,
                360,
                0,
                0,
            )
            .is_err()
        );
        assert!(
            long_form_item_image_input("item-1", "Primary", 0, "tag", "jpg", 100_000, 360, 0, 0,)
                .is_err()
        );
    }

    #[test]
    fn global_image_inputs_normalize_catalog_and_detail_paths() {
        assert_eq!(
            global_image_catalog_kind("/emby/Images/MediaInfo").unwrap(),
            GlobalImageKind::MediaInfo
        );
        assert_eq!(
            global_image_catalog_kind("/Images/Ratings").unwrap(),
            GlobalImageKind::Ratings
        );

        let general = general_image_input(" logo ", "png").expect("general image should normalize");
        assert_eq!(general.name, "logo");
        assert_eq!(general.image_type, "png");

        let themed =
            themed_global_image_input("/emby/Images/Ratings/Dark/PG-13", " Dark ", " PG-13 ")
                .expect("themed image should normalize");
        assert_eq!(themed.kind, GlobalImageKind::Ratings);
        assert_eq!(themed.theme, "Dark");
        assert_eq!(themed.name, "PG-13");

        assert!(general_image_input("../logo", "png").is_err());
        assert!(general_image_input("logo", "bad/type").is_err());
        assert!(
            themed_global_image_input("/Images/MediaInfo/Dark/Play", "Dark/Mode", "Play").is_err()
        );
    }

    #[test]
    fn remote_image_url_rejects_non_http_urls() {
        assert!(normalize_remote_image_url("file:///tmp/poster.jpg").is_err());
        assert!(normalize_remote_image_url("https://image.example.test/poster.jpg").is_ok());
    }
}
