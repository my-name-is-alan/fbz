use axum::{
    Json,
    extract::{Path, Query, State},
    http::{HeaderMap, Uri},
    response::Response,
};
use serde::{Deserialize, Serialize};

use crate::{error::AppError, library::repository::LibraryRepository, state::AppState};

use super::access::authenticate_request_user;

const MAX_BIF_ITEM_ID_LEN: usize = 128;
const MAX_BIF_WIDTH: u32 = 4096;

#[derive(Clone, Debug, Default, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "PascalCase")]
pub struct BifQuery {
    #[serde(alias = "width")]
    pub width: Option<u32>,
}

#[derive(Clone, Debug, Serialize, PartialEq)]
#[serde(rename_all = "PascalCase")]
pub struct ThumbnailSetInfoDto {
    pub aspect_ratio: f64,
    pub thumbnails: Vec<ThumbnailInfoDto>,
}

#[derive(Clone, Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "PascalCase")]
pub struct ThumbnailInfoDto {
    pub position_ticks: i64,
    pub image_tag: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct BifInput {
    item_id: String,
    width: u32,
}

pub async fn video_index_bif(
    State(state): State<AppState>,
    Path(item_id): Path<String>,
    Query(query): Query<BifQuery>,
    headers: HeaderMap,
    uri: Uri,
) -> Result<Response, AppError> {
    let input = bif_input(&item_id, query)?;
    let user = authenticate_request_user(&state, &headers, &uri).await?;
    ensure_user_can_access_item(&state, user.id, &input.item_id).await?;
    let _ = input.width;

    Err(AppError::not_found("bif index not found"))
}

pub async fn thumbnail_set(
    State(state): State<AppState>,
    Path(item_id): Path<String>,
    Query(query): Query<BifQuery>,
    headers: HeaderMap,
    uri: Uri,
) -> Result<Json<ThumbnailSetInfoDto>, AppError> {
    let input = bif_input(&item_id, query)?;
    let user = authenticate_request_user(&state, &headers, &uri).await?;
    ensure_user_can_access_item(&state, user.id, &input.item_id).await?;
    let _ = input.width;

    Ok(Json(empty_thumbnail_set()))
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

fn bif_input(item_id: &str, query: BifQuery) -> Result<BifInput, AppError> {
    Ok(BifInput {
        item_id: normalize_required_bif_text("Id", item_id, MAX_BIF_ITEM_ID_LEN)?,
        width: normalize_bif_width(query.width)?,
    })
}

fn empty_thumbnail_set() -> ThumbnailSetInfoDto {
    ThumbnailSetInfoDto {
        aspect_ratio: 16.0 / 9.0,
        thumbnails: Vec::new(),
    }
}

fn normalize_bif_width(width: Option<u32>) -> Result<u32, AppError> {
    let Some(width) = width else {
        return Err(AppError::unprocessable("Width is required"));
    };
    if width == 0 || width > MAX_BIF_WIDTH {
        return Err(AppError::unprocessable("Width is invalid"));
    }

    Ok(width)
}

fn normalize_required_bif_text(
    field: &'static str,
    value: &str,
    max_len: usize,
) -> Result<String, AppError> {
    let value = value.trim();
    if value.is_empty() {
        return Err(AppError::unprocessable(format!("{field} is required")));
    }
    if value.len() > max_len
        || value
            .chars()
            .any(|ch| ch.is_control() || matches!(ch, '/' | '\\'))
    {
        return Err(AppError::unprocessable(format!("{field} is invalid")));
    }

    Ok(value.to_owned())
}

#[cfg(test)]
mod tests {
    use axum::http::StatusCode;
    use serde_json::json;

    use super::*;

    #[test]
    fn bif_input_requires_positive_bounded_width() {
        assert!(bif_input("item-1", BifQuery { width: None }).is_err());
        assert!(bif_input("item-1", BifQuery { width: Some(0) }).is_err());
        assert!(
            bif_input(
                "item-1",
                BifQuery {
                    width: Some(MAX_BIF_WIDTH + 1)
                }
            )
            .is_err()
        );
        assert_eq!(
            bif_input(" item-1 ", BifQuery { width: Some(320) }).unwrap(),
            BifInput {
                item_id: "item-1".to_owned(),
                width: 320
            }
        );
    }

    #[test]
    fn bif_input_rejects_path_like_item_ids() {
        assert!(bif_input("../item", BifQuery { width: Some(320) }).is_err());
    }

    #[test]
    fn thumbnail_set_serializes_official_pascal_case_shape() {
        let value = serde_json::to_value(ThumbnailSetInfoDto {
            aspect_ratio: 16.0 / 9.0,
            thumbnails: vec![ThumbnailInfoDto {
                position_ticks: 1_000,
                image_tag: "thumb-1".to_owned(),
            }],
        })
        .expect("thumbnail set should serialize");

        assert_eq!(value["AspectRatio"], json!(16.0 / 9.0));
        assert_eq!(value["Thumbnails"][0]["PositionTicks"], 1_000);
        assert_eq!(value["Thumbnails"][0]["ImageTag"], "thumb-1");
    }

    #[test]
    fn empty_thumbnail_set_has_no_synthetic_frames() {
        let value = empty_thumbnail_set();

        assert_eq!(value.aspect_ratio, 16.0 / 9.0);
        assert!(value.thumbnails.is_empty());
    }

    #[test]
    fn app_errors_preserve_official_invalid_width_status() {
        let err = normalize_bif_width(Some(0)).expect_err("zero width should fail");

        assert_eq!(err.status_code(), StatusCode::UNPROCESSABLE_ENTITY);
    }
}
