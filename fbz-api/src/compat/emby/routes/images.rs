use std::path::{Component, Path, PathBuf};

use axum::{
    body::Body,
    extract::{Path as AxumPath, State},
    http::{
        HeaderMap, HeaderValue, StatusCode, Uri,
        header::{CACHE_CONTROL, CONTENT_LENGTH, CONTENT_TYPE, LOCATION},
    },
    response::Response,
};
use tokio::fs::File;
use tokio_util::io::ReaderStream;

use crate::{
    config::StorageConfig,
    error::AppError,
    media::repository::{ArtworkRecord, MediaRepository},
    state::AppState,
};

use super::access::authenticate_request_user;

const IMAGE_CACHE_CONTROL: &str = "public, max-age=86400";

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
}
