use axum::{
    Json,
    body::Bytes,
    extract::{Path, Query, State},
    http::{HeaderMap, StatusCode, Uri},
};
use serde::Deserialize;
use std::collections::HashSet;

use crate::{
    compat::emby::dto::UserItemDataDto,
    compat::emby::payload::parse_emby_body,
    error::AppError,
    media::repository::{MediaRepository, UserItemDataRecord, UserItemDataUpdateInput},
    state::AppState,
};

use super::access::authenticate_request_user;
use super::access::authenticate_route_user;

const MAX_LIBRARY_ACCESS_IDS: usize = 1000;
const MAX_LIBRARY_ACCESS_ID_LEN: usize = 128;

#[derive(Clone, Debug, Default, Deserialize, PartialEq)]
#[serde(rename_all = "PascalCase")]
pub struct RatingQuery {
    #[serde(alias = "likes")]
    pub likes: Option<bool>,
    #[serde(alias = "rating")]
    pub rating: Option<f64>,
}

#[derive(Clone, Debug, Default, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "PascalCase")]
pub struct HideFromResumeQuery {
    #[serde(alias = "hide")]
    pub hide: Option<bool>,
}

#[derive(Clone, Debug, Default, Deserialize, PartialEq)]
#[serde(rename_all = "PascalCase")]
pub struct UserItemDataUpdateDto {
    #[serde(alias = "playbackPositionTicks", alias = "playback_position_ticks")]
    pub playback_position_ticks: Option<i64>,
    #[serde(alias = "playCount", alias = "play_count")]
    pub play_count: Option<i32>,
    #[serde(alias = "played")]
    pub played: Option<bool>,
    #[serde(alias = "isFavorite", alias = "is_favorite")]
    pub is_favorite: Option<bool>,
    #[serde(alias = "rating")]
    pub rating: Option<f64>,
    #[serde(alias = "lastPlayedDate", alias = "last_played_date")]
    pub last_played_date: Option<String>,
}

#[derive(Clone, Debug, Default, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "PascalCase")]
pub struct UpdateUserItemAccessDto {
    #[serde(alias = "itemIds", alias = "item_ids")]
    pub item_ids: Vec<String>,
    #[serde(alias = "userIds", alias = "user_ids")]
    pub user_ids: Vec<String>,
    #[serde(alias = "itemAccess", alias = "item_access")]
    pub item_access: Option<String>,
}

#[derive(Clone, Debug, Default, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "PascalCase")]
pub struct LeaveSharedItemsDto {
    #[serde(alias = "itemIds", alias = "item_ids")]
    pub item_ids: Vec<String>,
    #[serde(alias = "userId", alias = "user_id")]
    pub user_id: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct UserItemAccessUpdateInput {
    item_ids: Vec<String>,
    user_ids: Vec<String>,
    item_access: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct LeaveSharedItemsInput {
    item_ids: Vec<String>,
    user_id: Option<String>,
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

pub async fn item_user_data(
    State(state): State<AppState>,
    Path((user_id, item_id)): Path<(String, String)>,
    headers: HeaderMap,
    uri: Uri,
) -> Result<Json<UserItemDataDto>, AppError> {
    let user = authenticate_route_user(&state, &user_id, &headers, &uri).await?;
    let Some(database) = state.database() else {
        return Err(AppError::internal("database is not configured"));
    };

    let data = MediaRepository::new(database.clone())
        .find_user_item_data(user.id, &item_id)
        .await
        .map_err(|err| AppError::internal(format!("failed to get item user data: {err}")))?
        .ok_or_else(|| AppError::not_found("item not found"))?;

    Ok(Json(user_item_data_to_dto(data)))
}

pub async fn update_item_user_data(
    State(state): State<AppState>,
    Path((user_id, item_id)): Path<(String, String)>,
    Query(query): Query<UserItemDataUpdateDto>,
    headers: HeaderMap,
    uri: Uri,
    body: Bytes,
) -> Result<Json<UserItemDataDto>, AppError> {
    let body_payload = parse_optional_user_item_data_update_body(&headers, &body)?;
    let payload = merged_user_item_data_update_input(body_payload, query)?;
    let user = authenticate_route_user(&state, &user_id, &headers, &uri).await?;
    let Some(database) = state.database() else {
        return Err(AppError::internal("database is not configured"));
    };

    let input = user_item_data_update_input(user.id, &item_id, payload)?;
    let data = MediaRepository::new(database.clone())
        .update_user_item_data(input)
        .await
        .map_err(|err| AppError::internal(format!("failed to update item user data: {err}")))?
        .ok_or_else(|| AppError::not_found("item not found"))?;

    Ok(Json(user_item_data_to_dto(data)))
}

pub async fn update_item_access(
    State(state): State<AppState>,
    headers: HeaderMap,
    uri: Uri,
    body: Bytes,
) -> Result<StatusCode, AppError> {
    authenticate_request_user(&state, &headers, &uri).await?;
    let payload: UpdateUserItemAccessDto = parse_emby_body(&headers, &body)?;
    let input = user_item_access_update_input(payload)?;
    let _item_ids = input.item_ids;
    let _user_ids = input.user_ids;
    let _item_access = input.item_access;

    Err(library_access_write_disabled_error())
}

pub async fn make_item_private(
    State(state): State<AppState>,
    Path(item_id): Path<String>,
    headers: HeaderMap,
    uri: Uri,
) -> Result<StatusCode, AppError> {
    authenticate_request_user(&state, &headers, &uri).await?;
    let _item_id = normalize_library_access_id(&item_id, "Id")?;

    Err(library_access_write_disabled_error())
}

pub async fn make_item_public(
    State(state): State<AppState>,
    Path(item_id): Path<String>,
    headers: HeaderMap,
    uri: Uri,
) -> Result<StatusCode, AppError> {
    authenticate_request_user(&state, &headers, &uri).await?;
    let _item_id = normalize_library_access_id(&item_id, "Id")?;

    Err(library_access_write_disabled_error())
}

pub async fn leave_shared_items(
    State(state): State<AppState>,
    headers: HeaderMap,
    uri: Uri,
    body: Bytes,
) -> Result<StatusCode, AppError> {
    authenticate_request_user(&state, &headers, &uri).await?;
    let payload: LeaveSharedItemsDto = parse_emby_body(&headers, &body)?;
    let input = leave_shared_items_input(payload)?;
    let _item_ids = input.item_ids;
    let _user_id = input.user_id;

    Err(library_access_write_disabled_error())
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

pub async fn hide_from_resume(
    State(state): State<AppState>,
    Path((user_id, item_id)): Path<(String, String)>,
    Query(query): Query<HideFromResumeQuery>,
    headers: HeaderMap,
    uri: Uri,
) -> Result<Json<UserItemDataDto>, AppError> {
    let user = authenticate_route_user(&state, &user_id, &headers, &uri).await?;
    let Some(database) = state.database() else {
        return Err(AppError::internal("database is not configured"));
    };

    let input = hide_from_resume_update_input(user.id, &item_id, query)?;
    let data = MediaRepository::new(database.clone())
        .update_user_item_data(input)
        .await
        .map_err(|err| AppError::internal(format!("failed to update hide from resume: {err}")))?
        .ok_or_else(|| AppError::not_found("item not found"))?;

    Ok(Json(user_item_data_to_dto(data)))
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

fn user_item_data_update_input(
    user_id: i64,
    item_id: &str,
    payload: UserItemDataUpdateDto,
) -> Result<UserItemDataUpdateInput, AppError> {
    let item_id = item_id.trim();
    if item_id.is_empty() || item_id.len() > 128 {
        return Err(AppError::unprocessable("item id is invalid"));
    }

    Ok(UserItemDataUpdateInput {
        user_id,
        item_id: item_id.to_owned(),
        playback_position_ticks: payload.playback_position_ticks.map(|ticks| ticks.max(0)),
        play_count: payload.play_count.map(|count| count.max(0)),
        is_favorite: payload.is_favorite,
        rating: payload.rating.map(normalize_user_rating).transpose()?,
        played: payload.played,
    })
}

fn parse_optional_user_item_data_update_body(
    headers: &HeaderMap,
    body: &Bytes,
) -> Result<Option<UserItemDataUpdateDto>, AppError> {
    if body.is_empty() {
        return Ok(None);
    }

    parse_emby_body(headers, body).map(Some)
}

fn merged_user_item_data_update_input(
    body: Option<UserItemDataUpdateDto>,
    query: UserItemDataUpdateDto,
) -> Result<UserItemDataUpdateDto, AppError> {
    let Some(body) = body else {
        return Ok(query);
    };

    Ok(UserItemDataUpdateDto {
        playback_position_ticks: body
            .playback_position_ticks
            .or(query.playback_position_ticks),
        play_count: body.play_count.or(query.play_count),
        played: body.played.or(query.played),
        is_favorite: body.is_favorite.or(query.is_favorite),
        rating: body.rating.or(query.rating),
        last_played_date: body.last_played_date.or(query.last_played_date),
    })
}

fn hide_from_resume_update_input(
    user_id: i64,
    item_id: &str,
    query: HideFromResumeQuery,
) -> Result<UserItemDataUpdateInput, AppError> {
    let hide = query
        .hide
        .ok_or_else(|| AppError::unprocessable("Hide query parameter is required"))?;

    let mut input =
        user_item_data_update_input(user_id, item_id, UserItemDataUpdateDto::default())?;
    if hide {
        input.playback_position_ticks = Some(0);
    }

    Ok(input)
}

fn user_item_access_update_input(
    payload: UpdateUserItemAccessDto,
) -> Result<UserItemAccessUpdateInput, AppError> {
    Ok(UserItemAccessUpdateInput {
        item_ids: normalize_library_access_ids(payload.item_ids, "ItemIds")?,
        user_ids: normalize_library_access_ids(payload.user_ids, "UserIds")?,
        item_access: Some(normalize_item_access(payload.item_access.as_deref())?),
    })
}

fn leave_shared_items_input(
    payload: LeaveSharedItemsDto,
) -> Result<LeaveSharedItemsInput, AppError> {
    Ok(LeaveSharedItemsInput {
        item_ids: normalize_library_access_ids(payload.item_ids, "ItemIds")?,
        user_id: payload
            .user_id
            .as_deref()
            .map(|value| normalize_library_access_id(value, "UserId"))
            .transpose()?,
    })
}

fn normalize_library_access_ids(
    values: Vec<String>,
    field: &'static str,
) -> Result<Vec<String>, AppError> {
    if values.is_empty() {
        return Err(AppError::unprocessable(format!("{field} is required")));
    }
    if values.len() > MAX_LIBRARY_ACCESS_IDS {
        return Err(AppError::unprocessable(format!(
            "{field} has too many values"
        )));
    }

    let mut seen = HashSet::new();
    let mut normalized = Vec::new();
    for value in values {
        let value = normalize_library_access_id(&value, field)?;
        if seen.insert(value.clone()) {
            normalized.push(value);
        }
    }
    if normalized.is_empty() {
        return Err(AppError::unprocessable(format!("{field} is required")));
    }

    Ok(normalized)
}

fn normalize_library_access_id(value: &str, field: &'static str) -> Result<String, AppError> {
    let value = value.trim();
    if value.is_empty()
        || value.len() > MAX_LIBRARY_ACCESS_ID_LEN
        || value.chars().any(char::is_control)
    {
        return Err(AppError::unprocessable(format!("{field} is invalid")));
    }

    Ok(value.to_owned())
}

fn normalize_item_access(value: Option<&str>) -> Result<String, AppError> {
    let value = value
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| AppError::unprocessable("ItemAccess is required"))?;

    match value.to_ascii_lowercase().as_str() {
        "none" => Ok("None".to_owned()),
        "read" => Ok("Read".to_owned()),
        "write" => Ok("Write".to_owned()),
        "manage" => Ok("Manage".to_owned()),
        "managedelete" => Ok("ManageDelete".to_owned()),
        _ => Err(AppError::unprocessable("ItemAccess is invalid")),
    }
}

fn library_access_write_disabled_error() -> AppError {
    AppError::conflict(
        "Emby library access sharing writes are disabled; use FBZ library permission APIs",
    )
}

fn normalize_user_rating(rating: f64) -> Result<f64, AppError> {
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
    use axum::{extract::Query, http::Uri};
    use serde_json::json;

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
    fn user_data_queries_accept_lower_camel_client_fields() {
        let uri: Uri = "/emby/Users/user-1/Items/item-1/Rating?likes=true"
            .parse()
            .unwrap();
        let Query(query) = Query::<RatingQuery>::try_from_uri(&uri).unwrap();

        assert_eq!(rating_from_query(&query).unwrap(), 10.0);

        let uri: Uri = "/emby/Users/user-1/Items/item-1/HideFromResume?hide=true"
            .parse()
            .unwrap();
        let Query(query) = Query::<HideFromResumeQuery>::try_from_uri(&uri).unwrap();
        let input = hide_from_resume_update_input(42, "item-1", query).unwrap();

        assert_eq!(input.playback_position_ticks, Some(0));
    }

    #[test]
    fn user_data_update_query_accepts_lower_camel_and_snake_case_fields() {
        let uri: Uri = "/emby/Users/user-1/Items/item-1/UserData?playbackPositionTicks=120000&play_count=3&played=true&isFavorite=false&rating=8.5&last_played_date=2026-01-01T00%3A00%3A00Z"
            .parse()
            .unwrap();
        let Query(query) = Query::<UserItemDataUpdateDto>::try_from_uri(&uri).unwrap();

        assert_eq!(query.playback_position_ticks, Some(120000));
        assert_eq!(query.play_count, Some(3));
        assert_eq!(query.played, Some(true));
        assert_eq!(query.is_favorite, Some(false));
        assert_eq!(query.rating, Some(8.5));
        assert_eq!(
            query.last_played_date.as_deref(),
            Some("2026-01-01T00:00:00Z")
        );
    }

    #[test]
    fn user_data_bodies_accept_lower_camel_client_fields() {
        let payload = serde_json::from_value::<UserItemDataUpdateDto>(json!({
            "playbackPositionTicks": 120000,
            "playCount": 3,
            "played": false,
            "isFavorite": true,
            "rating": 8.5,
            "lastPlayedDate": "2026-01-01T00:00:00Z"
        }))
        .unwrap();
        let input = user_item_data_update_input(42, "item-1", payload).unwrap();

        assert_eq!(input.playback_position_ticks, Some(120000));
        assert_eq!(input.play_count, Some(3));
        assert_eq!(input.played, Some(false));
        assert_eq!(input.is_favorite, Some(true));
        assert_eq!(input.rating, Some(8.5));

        let payload = serde_json::from_value::<UpdateUserItemAccessDto>(json!({
            "itemIds": [" item-1 ", "item-2"],
            "userIds": [" user-1 "],
            "itemAccess": "Read"
        }))
        .unwrap();
        let input = user_item_access_update_input(payload).unwrap();

        assert_eq!(input.item_ids, ["item-1", "item-2"]);
        assert_eq!(input.user_ids, ["user-1"]);
        assert_eq!(input.item_access.as_deref(), Some("Read"));

        let payload = serde_json::from_value::<LeaveSharedItemsDto>(json!({
            "itemIds": [" item-1 "],
            "userId": " user-1 "
        }))
        .unwrap();
        let input = leave_shared_items_input(payload).unwrap();

        assert_eq!(input.item_ids, ["item-1"]);
        assert_eq!(input.user_id.as_deref(), Some("user-1"));
    }

    #[test]
    fn user_data_update_input_accepts_query_only_fields() {
        let query = UserItemDataUpdateDto {
            playback_position_ticks: Some(120000),
            play_count: Some(3),
            played: Some(true),
            is_favorite: Some(false),
            rating: Some(8.5),
            last_played_date: None,
        };
        let input = merged_user_item_data_update_input(None, query).unwrap();

        assert_eq!(input.playback_position_ticks, Some(120000));
        assert_eq!(input.play_count, Some(3));
        assert_eq!(input.played, Some(true));
        assert_eq!(input.is_favorite, Some(false));
        assert_eq!(input.rating, Some(8.5));
    }

    #[test]
    fn user_data_update_input_preserves_body_fields_over_query() {
        let body = UserItemDataUpdateDto {
            playback_position_ticks: Some(42),
            play_count: Some(1),
            played: Some(false),
            is_favorite: Some(true),
            rating: Some(9.0),
            last_played_date: None,
        };
        let query = UserItemDataUpdateDto {
            playback_position_ticks: Some(120000),
            play_count: Some(3),
            played: Some(true),
            is_favorite: Some(false),
            rating: Some(8.5),
            last_played_date: None,
        };
        let input = merged_user_item_data_update_input(Some(body), query).unwrap();

        assert_eq!(input.playback_position_ticks, Some(42));
        assert_eq!(input.play_count, Some(1));
        assert_eq!(input.played, Some(false));
        assert_eq!(input.is_favorite, Some(true));
        assert_eq!(input.rating, Some(9.0));
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

    #[test]
    fn user_item_data_update_payload_is_normalized_and_bounded() {
        let input = user_item_data_update_input(
            42,
            " item-1 ",
            UserItemDataUpdateDto {
                playback_position_ticks: Some(-1),
                play_count: Some(-2),
                played: Some(false),
                is_favorite: Some(true),
                rating: Some(8.456),
                last_played_date: Some("2026-01-01T00:00:00Z".to_owned()),
            },
        )
        .unwrap();

        assert_eq!(input.user_id, 42);
        assert_eq!(input.item_id, "item-1");
        assert_eq!(input.playback_position_ticks, Some(0));
        assert_eq!(input.play_count, Some(0));
        assert_eq!(input.played, Some(false));
        assert_eq!(input.is_favorite, Some(true));
        assert_eq!(input.rating, Some(8.46));

        assert!(
            user_item_data_update_input(
                42,
                "item-1",
                UserItemDataUpdateDto {
                    rating: Some(10.01),
                    ..UserItemDataUpdateDto::default()
                },
            )
            .is_err()
        );
    }

    #[test]
    fn hide_from_resume_query_clears_resume_position_when_hidden() {
        let input =
            hide_from_resume_update_input(42, " item-1 ", HideFromResumeQuery { hide: Some(true) })
                .unwrap();

        assert_eq!(input.user_id, 42);
        assert_eq!(input.item_id, "item-1");
        assert_eq!(input.playback_position_ticks, Some(0));
        assert_eq!(input.play_count, None);
        assert_eq!(input.played, None);
        assert_eq!(input.is_favorite, None);
        assert_eq!(input.rating, None);

        let err = hide_from_resume_update_input(42, "item-1", HideFromResumeQuery { hide: None })
            .unwrap_err();

        assert_eq!(err.status_code(), http::StatusCode::UNPROCESSABLE_ENTITY);
    }

    #[test]
    fn user_item_access_payload_is_normalized_and_bounded() {
        let input = user_item_access_update_input(UpdateUserItemAccessDto {
            item_ids: vec![
                " item-1 ".to_owned(),
                "item-2".to_owned(),
                "item-1".to_owned(),
            ],
            user_ids: vec![" user-1 ".to_owned(), "user-1".to_owned()],
            item_access: Some("ManageDelete".to_owned()),
        })
        .unwrap();

        assert_eq!(input.item_ids, ["item-1", "item-2"]);
        assert_eq!(input.user_ids, ["user-1"]);
        assert_eq!(input.item_access.as_deref(), Some("ManageDelete"));

        let err = user_item_access_update_input(UpdateUserItemAccessDto {
            item_ids: Vec::new(),
            user_ids: vec!["user-1".to_owned()],
            item_access: Some("Read".to_owned()),
        })
        .unwrap_err();

        assert_eq!(err.status_code(), http::StatusCode::UNPROCESSABLE_ENTITY);
    }

    #[test]
    fn leave_shared_items_payload_is_normalized_and_bounded() {
        let input = leave_shared_items_input(LeaveSharedItemsDto {
            item_ids: vec![
                " item-1 ".to_owned(),
                "item-2".to_owned(),
                "item-1".to_owned(),
            ],
            user_id: Some(" user-1 ".to_owned()),
        })
        .unwrap();

        assert_eq!(input.item_ids, ["item-1", "item-2"]);
        assert_eq!(input.user_id.as_deref(), Some("user-1"));

        let err = leave_shared_items_input(LeaveSharedItemsDto {
            item_ids: Vec::new(),
            user_id: Some("user-1".to_owned()),
        })
        .unwrap_err();

        assert_eq!(err.status_code(), http::StatusCode::UNPROCESSABLE_ENTITY);
    }

    #[test]
    fn library_access_write_disabled_error_is_conflict() {
        let err = library_access_write_disabled_error();

        assert_eq!(err.status_code(), http::StatusCode::CONFLICT);
        assert_eq!(err.code(), "conflict");
    }
}
