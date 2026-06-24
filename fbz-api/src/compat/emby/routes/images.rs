use std::path::{Component, Path, PathBuf};

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
use serde::{Deserialize, Serialize};
use tokio::fs::File;
use tokio_util::io::ReaderStream;

use crate::{
    auth::service::AuthenticatedUser,
    compat::emby::payload::parse_emby_body,
    config::StorageConfig,
    error::AppError,
    library::repository::LibraryRepository,
    media::repository::{ArtworkRecord, MediaRepository},
    state::AppState,
};

use super::access::authenticate_request_user;

const IMAGE_CACHE_CONTROL: &str = "public, max-age=86400";
const MAX_ITEM_ID_LEN: usize = 128;
const MAX_ITEM_IMAGE_MUTATION_BODY_BYTES: usize = 16 * 1024 * 1024;
const MAX_REMOTE_IMAGE_LIMIT: u32 = 100;
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

pub async fn item_image(
    State(state): State<AppState>,
    AxumPath((item_id, image_type)): AxumPath<(String, String)>,
    headers: HeaderMap,
    uri: Uri,
) -> Result<Response, AppError> {
    item_image_by_index(state, item_id, image_type, 0, headers, uri).await
}

pub async fn item_image_index(
    State(state): State<AppState>,
    AxumPath((item_id, image_type, index)): AxumPath<(String, String, i64)>,
    headers: HeaderMap,
    uri: Uri,
) -> Result<Response, AppError> {
    item_image_by_index(state, item_id, image_type, index, headers, uri).await
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
    let _ = (
        input.image_type,
        input.image_index,
        input.body_len,
        input.new_index,
        input.image_url,
    );

    Err(item_image_write_disabled_error())
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
    let _ = (
        input.image_type,
        input.image_index,
        input.body_len,
        input.new_index,
        input.image_url,
    );

    Err(item_image_write_disabled_error())
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
    let _ = (
        input.image_type,
        input.image_index,
        input.new_index,
        input.image_url,
        input.body_len,
    );

    Err(item_image_write_disabled_error())
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
    let _ = (
        input.image_type,
        input.image_index,
        input.new_index,
        input.image_url,
        input.body_len,
    );

    Err(item_image_write_disabled_error())
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
    let _ = (
        input.image_type,
        input.image_index,
        input.new_index,
        input.image_url,
        input.body_len,
    );

    Err(item_image_write_disabled_error())
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
    let _ = (
        input.image_type,
        input.image_index,
        input.new_index,
        input.image_url,
        input.body_len,
    );

    Err(item_image_write_disabled_error())
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

    Ok(Json(empty_remote_image_result(input)))
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
    let _ = (
        &input.image_type,
        &input.provider_name,
        &input.image_url,
        input.image_index,
    );

    Err(AppError::conflict(
        "remote image downloads are not configured",
    ))
}

pub async fn remote_image_proxy(
    State(state): State<AppState>,
    Query(query): Query<RemoteImageProxyQuery>,
    headers: HeaderMap,
    uri: Uri,
) -> Result<StatusCode, AppError> {
    let user = authenticate_request_user(&state, &headers, &uri).await?;
    ensure_remote_image_admin(&user)?;
    normalize_required_remote_image_url(query.image_url.as_deref())?;

    Err(AppError::conflict(
        "remote image proxying is not configured",
    ))
}

async fn item_image_by_index(
    state: AppState,
    item_id: String,
    image_type: String,
    index: i64,
    headers: HeaderMap,
    uri: Uri,
) -> Result<Response, AppError> {
    let user = authenticate_request_user(&state, &headers, &uri).await?;
    let artwork_types = artwork_types_for_emby(&image_type)
        .ok_or_else(|| AppError::unprocessable("unsupported image type"))?;
    let Some(database) = state.database() else {
        return Err(AppError::internal("database is not configured"));
    };

    let artwork = MediaRepository::new(database.clone())
        .find_item_artwork(user.id, &item_id, &artwork_types, index)
        .await
        .map_err(|err| AppError::internal(format!("failed to get item image: {err}")))?
        .ok_or_else(|| AppError::not_found("item image not found"))?;

    artwork_response(&state.config().storage, artwork).await
}

async fn artwork_response(
    storage: &StorageConfig,
    artwork: ArtworkRecord,
) -> Result<Response, AppError> {
    if let Some(storage_key) = artwork.storage_key.as_deref() {
        match local_artwork_response(&storage.artwork_cache_dir, storage_key).await {
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

async fn local_artwork_response(
    artwork_cache_dir: &Path,
    storage_key: &str,
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
        start_index: query.start_index.unwrap_or(0),
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

fn item_image_write_disabled_error() -> AppError {
    AppError::conflict("item image mutations are not configured")
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
    fn remote_image_url_rejects_non_http_urls() {
        assert!(normalize_remote_image_url("file:///tmp/poster.jpg").is_err());
        assert!(normalize_remote_image_url("https://image.example.test/poster.jpg").is_ok());
    }
}
