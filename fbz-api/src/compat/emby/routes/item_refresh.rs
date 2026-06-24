use axum::{
    body::Bytes,
    extract::{Path, Query, State},
    http::{HeaderMap, StatusCode, Uri},
    response::{IntoResponse, Response},
};
use serde::Deserialize;

use crate::{
    admin::repository::{AdminRepository, QueueMetadataRefreshInput},
    auth::service::AuthenticatedUser,
    compat::emby::payload::parse_emby_body,
    error::AppError,
    state::AppState,
};

use super::access::authenticate_request_user;

const MAX_ITEM_ID_LEN: usize = 128;
const MAX_REFRESH_MODE_LEN: usize = 64;

#[derive(Clone, Debug, Default, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "PascalCase")]
pub struct ItemRefreshQuery {
    #[serde(alias = "recursive")]
    pub recursive: Option<bool>,
    #[serde(alias = "metadataRefreshMode", alias = "metadata_refresh_mode")]
    pub metadata_refresh_mode: Option<String>,
    #[serde(alias = "imageRefreshMode", alias = "image_refresh_mode")]
    pub image_refresh_mode: Option<String>,
    #[serde(alias = "replaceAllMetadata", alias = "replace_all_metadata")]
    pub replace_all_metadata: Option<bool>,
    #[serde(alias = "replaceAllImages", alias = "replace_all_images")]
    pub replace_all_images: Option<bool>,
}

#[derive(Clone, Debug, Default, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "PascalCase")]
pub struct BaseRefreshRequestDto {
    #[serde(alias = "replaceThumbnailImages", alias = "replace_thumbnail_images")]
    pub replace_thumbnail_images: Option<bool>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct ItemRefreshInput {
    item_id: String,
    reason: String,
}

pub async fn refresh_item(
    State(state): State<AppState>,
    Path(item_id): Path<String>,
    Query(query): Query<ItemRefreshQuery>,
    headers: HeaderMap,
    uri: Uri,
    body: Bytes,
) -> Result<Response, AppError> {
    let user = authenticate_request_user(&state, &headers, &uri).await?;
    ensure_item_refresh_admin(&user)?;
    let body_payload = parse_optional_refresh_body(&headers, &body)?;
    let input = item_refresh_input(&item_id, query, body_payload)?;

    let Some(database) = state.database() else {
        return Err(AppError::internal("database is not configured"));
    };
    let Some(job) = AdminRepository::new(database.clone())
        .queue_metadata_refresh_for_item(
            &input.item_id,
            QueueMetadataRefreshInput {
                requested_by_user_id: user.id,
                reason: Some(input.reason),
            },
        )
        .await
        .map_err(|err| AppError::internal(format!("failed to queue metadata refresh: {err}")))?
    else {
        return Err(AppError::not_found("media item not found"));
    };

    let _ = job;
    Ok((StatusCode::OK, "").into_response())
}

fn parse_optional_refresh_body(
    headers: &HeaderMap,
    body: &Bytes,
) -> Result<BaseRefreshRequestDto, AppError> {
    if body.is_empty() {
        return Ok(BaseRefreshRequestDto::default());
    }

    parse_emby_body(headers, body)
}

fn item_refresh_input(
    item_id: &str,
    query: ItemRefreshQuery,
    body: BaseRefreshRequestDto,
) -> Result<ItemRefreshInput, AppError> {
    let item_id = normalize_required_path_text("item id", item_id, MAX_ITEM_ID_LEN)?;
    let metadata_refresh_mode =
        normalize_refresh_mode("MetadataRefreshMode", query.metadata_refresh_mode)?
            .unwrap_or_else(|| "Default".to_owned());
    let image_refresh_mode = normalize_refresh_mode("ImageRefreshMode", query.image_refresh_mode)?
        .unwrap_or_else(|| "Default".to_owned());
    let recursive = query.recursive.unwrap_or(false);
    let replace_all_metadata = query.replace_all_metadata.unwrap_or(false);
    let replace_all_images = query.replace_all_images.unwrap_or(false);
    let replace_thumbnail_images = body.replace_thumbnail_images.unwrap_or(false);
    let reason = format!(
        "emby item refresh: recursive={recursive}; metadataRefreshMode={metadata_refresh_mode}; imageRefreshMode={image_refresh_mode}; replaceAllMetadata={replace_all_metadata}; replaceAllImages={replace_all_images}; replaceThumbnailImages={replace_thumbnail_images}"
    );

    Ok(ItemRefreshInput { item_id, reason })
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

fn normalize_refresh_mode(
    field: &'static str,
    value: Option<String>,
) -> Result<Option<String>, AppError> {
    let Some(value) = value else {
        return Ok(None);
    };
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return Ok(None);
    }
    if trimmed.len() > MAX_REFRESH_MODE_LEN
        || !trimmed
            .chars()
            .all(|ch| ch.is_ascii_alphanumeric() || matches!(ch, '_' | '-'))
    {
        return Err(AppError::unprocessable(format!("{field} is invalid")));
    }

    Ok(Some(trimmed.to_owned()))
}

fn ensure_item_refresh_admin(user: &AuthenticatedUser) -> Result<(), AppError> {
    if user.can_manage_server() {
        return Ok(());
    }

    Err(AppError::forbidden("server management permission required"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn item_refresh_input_normalizes_official_query_and_body_fields() {
        let input = item_refresh_input(
            " item-1 ",
            ItemRefreshQuery {
                recursive: Some(true),
                metadata_refresh_mode: Some(" FullRefresh ".to_owned()),
                image_refresh_mode: Some("Default".to_owned()),
                replace_all_metadata: Some(true),
                replace_all_images: Some(false),
            },
            BaseRefreshRequestDto {
                replace_thumbnail_images: Some(true),
            },
        )
        .expect("official refresh fields should normalize");

        assert_eq!(input.item_id, "item-1");
        assert_eq!(
            input.reason,
            "emby item refresh: recursive=true; metadataRefreshMode=FullRefresh; imageRefreshMode=Default; replaceAllMetadata=true; replaceAllImages=false; replaceThumbnailImages=true"
        );
    }

    #[test]
    fn item_refresh_input_defaults_omitted_fields_to_safe_refresh_values() {
        let input = item_refresh_input(
            "item-1",
            ItemRefreshQuery::default(),
            BaseRefreshRequestDto::default(),
        )
        .expect("omitted refresh fields should default");

        assert_eq!(
            input.reason,
            "emby item refresh: recursive=false; metadataRefreshMode=Default; imageRefreshMode=Default; replaceAllMetadata=false; replaceAllImages=false; replaceThumbnailImages=false"
        );
    }

    #[test]
    fn item_refresh_input_rejects_unbounded_or_path_like_values() {
        assert!(
            item_refresh_input(
                "../item",
                ItemRefreshQuery::default(),
                BaseRefreshRequestDto::default(),
            )
            .is_err()
        );
        assert!(
            item_refresh_input(
                "item-1",
                ItemRefreshQuery {
                    metadata_refresh_mode: Some("../FullRefresh".to_owned()),
                    ..ItemRefreshQuery::default()
                },
                BaseRefreshRequestDto::default(),
            )
            .is_err()
        );
    }
}
