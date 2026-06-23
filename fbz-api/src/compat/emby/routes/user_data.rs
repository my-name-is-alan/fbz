use axum::{
    Json,
    extract::{Path, Query, State},
    http::{HeaderMap, Uri},
};
use serde::Deserialize;

use crate::{
    compat::emby::dto::UserItemDataDto,
    error::AppError,
    media::repository::{MediaRepository, UserItemDataRecord},
    state::AppState,
};

use super::access::authenticate_route_user;

#[derive(Clone, Debug, Default, Deserialize, PartialEq)]
#[serde(rename_all = "PascalCase")]
pub struct RatingQuery {
    pub likes: Option<bool>,
    pub rating: Option<f64>,
}

pub async fn mark_played(
    State(state): State<AppState>,
    Path((user_id, item_id)): Path<(String, String)>,
    headers: HeaderMap,
    uri: Uri,
) -> Result<Json<UserItemDataDto>, AppError> {
    set_played(&state, &user_id, &item_id, true, &headers, &uri).await
}

pub async fn mark_unplayed(
    State(state): State<AppState>,
    Path((user_id, item_id)): Path<(String, String)>,
    headers: HeaderMap,
    uri: Uri,
) -> Result<Json<UserItemDataDto>, AppError> {
    set_played(&state, &user_id, &item_id, false, &headers, &uri).await
}

pub async fn mark_favorite(
    State(state): State<AppState>,
    Path((user_id, item_id)): Path<(String, String)>,
    headers: HeaderMap,
    uri: Uri,
) -> Result<Json<UserItemDataDto>, AppError> {
    set_favorite(&state, &user_id, &item_id, true, &headers, &uri).await
}

pub async fn unmark_favorite(
    State(state): State<AppState>,
    Path((user_id, item_id)): Path<(String, String)>,
    headers: HeaderMap,
    uri: Uri,
) -> Result<Json<UserItemDataDto>, AppError> {
    set_favorite(&state, &user_id, &item_id, false, &headers, &uri).await
}

pub async fn set_rating(
    State(state): State<AppState>,
    Path((user_id, item_id)): Path<(String, String)>,
    Query(query): Query<RatingQuery>,
    headers: HeaderMap,
    uri: Uri,
) -> Result<Json<UserItemDataDto>, AppError> {
    let rating = rating_from_query(&query)?;
    update_rating(&state, &user_id, &item_id, Some(rating), &headers, &uri).await
}

pub async fn delete_rating(
    State(state): State<AppState>,
    Path((user_id, item_id)): Path<(String, String)>,
    headers: HeaderMap,
    uri: Uri,
) -> Result<Json<UserItemDataDto>, AppError> {
    update_rating(&state, &user_id, &item_id, None, &headers, &uri).await
}

async fn set_played(
    state: &AppState,
    route_user_id: &str,
    item_id: &str,
    played: bool,
    headers: &HeaderMap,
    uri: &Uri,
) -> Result<Json<UserItemDataDto>, AppError> {
    let user = authenticate_route_user(state, route_user_id, headers, uri).await?;
    let Some(database) = state.database() else {
        return Err(AppError::internal("database is not configured"));
    };

    let data = MediaRepository::new(database.clone())
        .set_item_played(user.id, item_id, played)
        .await
        .map_err(|err| AppError::internal(format!("failed to update played state: {err}")))?
        .ok_or_else(|| AppError::not_found("item not found"))?;

    Ok(Json(user_item_data_to_dto(data)))
}

async fn set_favorite(
    state: &AppState,
    route_user_id: &str,
    item_id: &str,
    is_favorite: bool,
    headers: &HeaderMap,
    uri: &Uri,
) -> Result<Json<UserItemDataDto>, AppError> {
    let user = authenticate_route_user(state, route_user_id, headers, uri).await?;
    let Some(database) = state.database() else {
        return Err(AppError::internal("database is not configured"));
    };

    let data = MediaRepository::new(database.clone())
        .set_item_favorite(user.id, item_id, is_favorite)
        .await
        .map_err(|err| AppError::internal(format!("failed to update favorite state: {err}")))?
        .ok_or_else(|| AppError::not_found("item not found"))?;

    Ok(Json(user_item_data_to_dto(data)))
}

async fn update_rating(
    state: &AppState,
    route_user_id: &str,
    item_id: &str,
    rating: Option<f64>,
    headers: &HeaderMap,
    uri: &Uri,
) -> Result<Json<UserItemDataDto>, AppError> {
    let user = authenticate_route_user(state, route_user_id, headers, uri).await?;
    let Some(database) = state.database() else {
        return Err(AppError::internal("database is not configured"));
    };

    let data = MediaRepository::new(database.clone())
        .set_item_rating(user.id, item_id, rating)
        .await
        .map_err(|err| AppError::internal(format!("failed to update item rating: {err}")))?
        .ok_or_else(|| AppError::not_found("item not found"))?;

    Ok(Json(user_item_data_to_dto(data)))
}

fn rating_from_query(query: &RatingQuery) -> Result<f64, AppError> {
    let rating = query
        .rating
        .or_else(|| query.likes.map(|likes| if likes { 10.0 } else { 0.0 }))
        .ok_or_else(|| AppError::unprocessable("Likes or Rating query parameter is required"))?;

    if !(0.0..=10.0).contains(&rating) {
        return Err(AppError::unprocessable("rating must be between 0 and 10"));
    }

    Ok((rating * 100.0).round() / 100.0)
}

fn user_item_data_to_dto(record: UserItemDataRecord) -> UserItemDataDto {
    UserItemDataDto {
        rating: record.rating,
        playback_position_ticks: record.playback_position_ticks,
        play_count: record.play_count,
        is_favorite: record.is_favorite,
        played: record.played,
        item_id: Some(record.item_id),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rating_query_accepts_likes_and_numeric_rating() {
        assert_eq!(
            rating_from_query(&RatingQuery {
                likes: Some(true),
                rating: None
            })
            .unwrap(),
            10.0
        );
        assert_eq!(
            rating_from_query(&RatingQuery {
                likes: Some(false),
                rating: None
            })
            .unwrap(),
            0.0
        );
        assert_eq!(
            rating_from_query(&RatingQuery {
                likes: None,
                rating: Some(8.456)
            })
            .unwrap(),
            8.46
        );
    }

    #[test]
    fn rating_query_rejects_missing_or_out_of_range_rating() {
        assert!(rating_from_query(&RatingQuery::default()).is_err());
        assert!(
            rating_from_query(&RatingQuery {
                likes: None,
                rating: Some(10.01)
            })
            .is_err()
        );
    }

    #[test]
    fn user_item_data_mapping_preserves_playstate_fields() {
        let dto = user_item_data_to_dto(UserItemDataRecord {
            item_id: "item-1".to_owned(),
            playback_position_ticks: 42,
            play_count: 2,
            is_favorite: true,
            rating: Some(9.0),
            played: true,
        });

        assert_eq!(dto.item_id.as_deref(), Some("item-1"));
        assert_eq!(dto.playback_position_ticks, 42);
        assert_eq!(dto.play_count, 2);
        assert!(dto.is_favorite);
        assert_eq!(dto.rating, Some(9.0));
        assert!(dto.played);
    }
}
