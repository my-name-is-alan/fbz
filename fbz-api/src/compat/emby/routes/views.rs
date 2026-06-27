use axum::{
    Json,
    extract::{Path, Query, State},
    http::{HeaderMap, Uri},
};
use serde::Deserialize;

use crate::{
    auth::service::AuthenticatedUser,
    compat::emby::dto::{BaseItemDto, LibraryViewSource, QueryResultDto, UserViewDto},
    db::DbPool,
    error::AppError,
    library::repository::{LibraryRepository, UserLibraryViewRecord},
    state::AppState,
};

use super::access::{authenticate_query_user, authenticate_route_user};

#[derive(Clone, Debug, Default, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "PascalCase")]
pub struct UserViewsQuery {
    #[serde(alias = "userId", alias = "user_id")]
    pub user_id: Option<String>,
}

pub async fn user_views(
    State(state): State<AppState>,
    Path(user_id): Path<String>,
    headers: HeaderMap,
    uri: Uri,
) -> Result<Json<QueryResultDto<UserViewDto>>, AppError> {
    let Some(database) = state.database() else {
        return Err(AppError::internal("database is not configured"));
    };

    let authenticated_user = authenticate_route_user(&state, &user_id, &headers, &uri).await?;
    list_views_for_user(database.clone(), authenticated_user).await
}

pub async fn user_views_by_query(
    State(state): State<AppState>,
    Query(query): Query<UserViewsQuery>,
    headers: HeaderMap,
    uri: Uri,
) -> Result<Json<QueryResultDto<UserViewDto>>, AppError> {
    let Some(database) = state.database() else {
        return Err(AppError::internal("database is not configured"));
    };

    let authenticated_user =
        authenticate_query_user(&state, user_views_query_user_id(&query), &headers, &uri).await?;
    list_views_for_user(database.clone(), authenticated_user).await
}

pub async fn grouping_options(
    Path(user_id): Path<String>,
) -> Result<Json<Vec<BaseItemDto>>, AppError> {
    let _user_id = grouping_options_user_id(&user_id)?;

    Ok(Json(grouping_options_response()))
}

async fn list_views_for_user(
    database: DbPool,
    authenticated_user: AuthenticatedUser,
) -> Result<Json<QueryResultDto<UserViewDto>>, AppError> {
    let views = LibraryRepository::new(database.clone())
        .list_user_views(authenticated_user.id)
        .await
        .map_err(|err| AppError::internal(format!("failed to list user views: {err}")))?
        .into_iter()
        .map(user_library_view_to_dto)
        .collect::<Vec<_>>();
    let total = views.len() as u32;

    Ok(Json(QueryResultDto::new(views, total, 0)))
}

fn user_views_query_user_id(query: &UserViewsQuery) -> Option<&str> {
    query
        .user_id
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
}

fn grouping_options_user_id(user_id: &str) -> Result<String, AppError> {
    let user_id = user_id.trim();
    if user_id.is_empty()
        || user_id.len() > 256
        || user_id
            .chars()
            .any(|ch| ch.is_control() || matches!(ch, '/' | '\\'))
    {
        return Err(AppError::unprocessable("UserId is invalid"));
    }

    Ok(user_id.to_owned())
}

fn grouping_options_response() -> Vec<BaseItemDto> {
    Vec::new()
}

fn user_library_view_to_dto(record: UserLibraryViewRecord) -> UserViewDto {
    UserViewDto::from(LibraryViewSource {
        id: record.id,
        name: record.name,
        collection_type: record.library_type,
    })
}

#[cfg(test)]
mod tests {
    use axum::extract::Query;
    use http::Uri;

    use super::*;

    #[test]
    fn user_views_query_user_id_trims_optional_query_scope() {
        let query = UserViewsQuery {
            user_id: Some(" user-1 ".to_owned()),
        };

        assert_eq!(user_views_query_user_id(&query), Some("user-1"));

        let query = UserViewsQuery {
            user_id: Some("   ".to_owned()),
        };

        assert_eq!(user_views_query_user_id(&query), None);
    }

    #[test]
    fn user_views_query_accepts_lower_camel_client_fields() {
        let uri = "/UserViews?userId=user-1".parse::<Uri>().unwrap();
        let Query(query) = Query::<UserViewsQuery>::try_from_uri(&uri).unwrap();

        assert_eq!(user_views_query_user_id(&query), Some("user-1"));
    }

    #[test]
    fn grouping_options_normalizes_user_id_and_returns_empty_array() {
        let user_id = grouping_options_user_id(" user-1 ").unwrap();
        let options = grouping_options_response();

        assert_eq!(user_id, "user-1");
        assert!(options.is_empty());
    }
}
