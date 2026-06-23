use axum::{
    Json,
    extract::{Path, Query, State},
    http::{HeaderMap, Uri},
};
use serde::Deserialize;

use crate::{
    compat::emby::dto::{AllThemeMediaResultDto, ThemeMediaResultDto},
    error::AppError,
    library::repository::LibraryRepository,
    state::AppState,
};

use super::access::authenticate_request_user;

#[derive(Clone, Debug, Default, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "PascalCase")]
pub struct ThemeMediaQuery {
    pub user_id: Option<String>,
    pub inherit_from_parent: Option<bool>,
    pub start_index: Option<u32>,
    pub limit: Option<u32>,
    pub fields: Option<String>,
}

pub async fn theme_media(
    State(state): State<AppState>,
    Path(item_id): Path<String>,
    Query(query): Query<ThemeMediaQuery>,
    headers: HeaderMap,
    uri: Uri,
) -> Result<Json<AllThemeMediaResultDto>, AppError> {
    ensure_theme_media_owner_visible(&state, &item_id, &query, &headers, &uri).await?;

    Ok(Json(AllThemeMediaResultDto::empty(item_id)))
}

pub async fn theme_songs(
    State(state): State<AppState>,
    Path(item_id): Path<String>,
    Query(query): Query<ThemeMediaQuery>,
    headers: HeaderMap,
    uri: Uri,
) -> Result<Json<ThemeMediaResultDto>, AppError> {
    ensure_theme_media_owner_visible(&state, &item_id, &query, &headers, &uri).await?;

    Ok(Json(ThemeMediaResultDto::empty(item_id)))
}

pub async fn theme_videos(
    State(state): State<AppState>,
    Path(item_id): Path<String>,
    Query(query): Query<ThemeMediaQuery>,
    headers: HeaderMap,
    uri: Uri,
) -> Result<Json<ThemeMediaResultDto>, AppError> {
    ensure_theme_media_owner_visible(&state, &item_id, &query, &headers, &uri).await?;

    Ok(Json(ThemeMediaResultDto::empty(item_id)))
}

async fn ensure_theme_media_owner_visible(
    state: &AppState,
    item_id: &str,
    query: &ThemeMediaQuery,
    headers: &HeaderMap,
    uri: &Uri,
) -> Result<(), AppError> {
    let user = authenticate_request_user(state, headers, uri).await?;
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

    let exists = LibraryRepository::new(database.clone())
        .find_user_item_by_id(user.id, item_id)
        .await
        .map_err(|err| AppError::internal(format!("failed to get theme media owner: {err}")))?
        .is_some();
    if !exists {
        return Err(AppError::not_found("item not found"));
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    #[test]
    fn theme_media_query_accepts_common_emby_parameters() {
        let query = serde_json::from_value::<ThemeMediaQuery>(json!({
            "UserId": "user-1",
            "InheritFromParent": true,
            "StartIndex": 2,
            "Limit": 10,
            "Fields": "MediaSources"
        }))
        .unwrap();

        assert_eq!(query.user_id.as_deref(), Some("user-1"));
        assert_eq!(query.inherit_from_parent, Some(true));
        assert_eq!(query.start_index, Some(2));
        assert_eq!(query.limit, Some(10));
        assert_eq!(query.fields.as_deref(), Some("MediaSources"));
    }
}
