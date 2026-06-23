use axum::{
    Json,
    extract::{Path, State},
    http::{HeaderMap, Uri},
};

use crate::{
    compat::emby::dto::{LibraryViewSource, QueryResultDto, UserViewDto},
    error::AppError,
    library::repository::{LibraryRepository, UserLibraryViewRecord},
    state::AppState,
};

use super::access::authenticate_route_user;

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

fn user_library_view_to_dto(record: UserLibraryViewRecord) -> UserViewDto {
    UserViewDto::from(LibraryViewSource {
        id: record.id,
        name: record.name,
        collection_type: record.library_type,
    })
}
