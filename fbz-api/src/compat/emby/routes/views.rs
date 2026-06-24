use axum::{
    Json,
    extract::{Path, Query, State},
    http::{HeaderMap, Uri},
};
use serde::Deserialize;

use crate::{
    auth::service::AuthenticatedUser,
    compat::emby::dto::{LibraryViewSource, QueryResultDto, UserViewDto},
    db::DbPool,
    error::AppError,
    library::repository::{LibraryRepository, UserLibraryViewRecord},
    state::AppState,
};

use super::access::{authenticate_query_user, authenticate_route_user};

#[derive(Clone, Debug, Default, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "PascalCase")]
pub struct UserViewsQuery {
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

fn user_library_view_to_dto(record: UserLibraryViewRecord) -> UserViewDto {
    UserViewDto::from(LibraryViewSource {
        id: record.id,
        name: record.name,
        collection_type: record.library_type,
    })
}

#[cfg(test)]
mod tests {
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
}
