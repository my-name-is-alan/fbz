use axum::{
    Json,
    extract::{Path, Query, State},
    http::{HeaderMap, Uri},
};
use serde::Deserialize;

use crate::{
    auth::service::AuthenticatedUser,
    compat::emby::dto::{BaseItemDto, QueryResultDto},
    error::AppError,
    library::repository::{
        ItemSortField, LibraryRepository, NextUpInput, ShowItemsInput, SortDirection, UpcomingInput,
    },
    state::AppState,
};

use super::{
    access::authenticate_request_user,
    items::{
        ItemWindow, MediaListQuery, item_query_options, media_items_to_dtos, media_query_result,
        normalized_parent_id, requested_item_fields,
    },
};

#[derive(Clone, Debug, Default, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "PascalCase")]
pub struct ShowItemsQuery {
    #[serde(alias = "userId", alias = "user_id")]
    pub user_id: Option<String>,
    #[serde(alias = "seasonId", alias = "season_id")]
    pub season_id: Option<String>,
    #[serde(alias = "startIndex", alias = "start_index")]
    pub start_index: Option<u32>,
    #[serde(alias = "limit")]
    pub limit: Option<u32>,
    #[serde(alias = "includeItemTypes", alias = "include_item_types")]
    pub include_item_types: Option<String>,
    #[serde(alias = "sortBy", alias = "sort_by")]
    pub sort_by: Option<String>,
    #[serde(alias = "sortOrder", alias = "sort_order")]
    pub sort_order: Option<String>,
    #[serde(alias = "fields")]
    pub fields: Option<String>,
}

#[derive(Clone, Debug, Default, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "PascalCase")]
pub struct NextUpQuery {
    #[serde(alias = "userId", alias = "user_id")]
    pub user_id: Option<String>,
    #[serde(alias = "seriesId", alias = "series_id")]
    pub series_id: Option<String>,
    #[serde(alias = "parentId", alias = "parent_id")]
    pub parent_id: Option<String>,
    #[serde(alias = "startIndex", alias = "start_index")]
    pub start_index: Option<u32>,
    #[serde(alias = "limit")]
    pub limit: Option<u32>,
    #[serde(alias = "includeItemTypes", alias = "include_item_types")]
    pub include_item_types: Option<String>,
    #[serde(alias = "sortBy", alias = "sort_by")]
    pub sort_by: Option<String>,
    #[serde(alias = "sortOrder", alias = "sort_order")]
    pub sort_order: Option<String>,
    #[serde(alias = "fields")]
    pub fields: Option<String>,
}

#[derive(Clone, Debug, Default, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "PascalCase")]
pub struct UpcomingQuery {
    #[serde(alias = "userId", alias = "user_id")]
    pub user_id: Option<String>,
    #[serde(alias = "parentId", alias = "parent_id")]
    pub parent_id: Option<String>,
    #[serde(alias = "startIndex", alias = "start_index")]
    pub start_index: Option<u32>,
    #[serde(alias = "limit")]
    pub limit: Option<u32>,
    #[serde(alias = "includeItemTypes", alias = "include_item_types")]
    pub include_item_types: Option<String>,
    #[serde(alias = "sortBy", alias = "sort_by")]
    pub sort_by: Option<String>,
    #[serde(alias = "sortOrder", alias = "sort_order")]
    pub sort_order: Option<String>,
    #[serde(alias = "fields")]
    pub fields: Option<String>,
}

pub async fn seasons(
    State(state): State<AppState>,
    Path(series_id): Path<String>,
    Query(query): Query<ShowItemsQuery>,
    headers: HeaderMap,
    uri: Uri,
) -> Result<Json<QueryResultDto<BaseItemDto>>, AppError> {
    let user = authenticated_query_user(&state, &headers, &uri, query.user_id.as_deref()).await?;
    let Some(database) = state.database() else {
        return Err(AppError::internal("database is not configured"));
    };

    let window = ItemWindow::from_media_query(&MediaListQuery {
        user_id: None,
        parent_id: None,
        start_index: query.start_index,
        limit: query.limit,
        include_item_types: query.include_item_types.clone(),
        sort_by: query.sort_by.clone(),
        sort_order: query.sort_order.clone(),
        fields: query.fields.clone(),
    });
    let options = item_query_options(
        query.include_item_types.as_deref(),
        query.sort_by.as_deref(),
        query.sort_order.as_deref(),
        ItemSortField::IndexNumber,
        SortDirection::Asc,
    );
    let _requested_fields = requested_item_fields(query.fields.as_deref());
    let result = LibraryRepository::new(database.clone())
        .list_series_seasons(ShowItemsInput {
            user_id: user.id,
            series_id,
            season_id: None,
            start_index: window.start_index,
            limit: window.limit,
            options,
        })
        .await
        .map_err(|err| AppError::internal(format!("failed to list seasons: {err}")))?;

    Ok(Json(media_query_result(result, window)))
}

pub async fn episodes(
    State(state): State<AppState>,
    Path(series_id): Path<String>,
    Query(query): Query<ShowItemsQuery>,
    headers: HeaderMap,
    uri: Uri,
) -> Result<Json<QueryResultDto<BaseItemDto>>, AppError> {
    let user = authenticated_query_user(&state, &headers, &uri, query.user_id.as_deref()).await?;
    let Some(database) = state.database() else {
        return Err(AppError::internal("database is not configured"));
    };

    let window = ItemWindow::from_media_query(&MediaListQuery {
        user_id: None,
        parent_id: None,
        start_index: query.start_index,
        limit: query.limit,
        include_item_types: query.include_item_types.clone(),
        sort_by: query.sort_by.clone(),
        sort_order: query.sort_order.clone(),
        fields: query.fields.clone(),
    });
    let options = item_query_options(
        query.include_item_types.as_deref(),
        query.sort_by.as_deref(),
        query.sort_order.as_deref(),
        ItemSortField::IndexNumber,
        SortDirection::Asc,
    );
    let _requested_fields = requested_item_fields(query.fields.as_deref());
    let result = LibraryRepository::new(database.clone())
        .list_series_episodes(ShowItemsInput {
            user_id: user.id,
            series_id,
            season_id: normalized_parent_id(query.season_id),
            start_index: window.start_index,
            limit: window.limit,
            options,
        })
        .await
        .map_err(|err| AppError::internal(format!("failed to list episodes: {err}")))?;

    Ok(Json(media_query_result(result, window)))
}

pub async fn next_up(
    State(state): State<AppState>,
    Query(query): Query<NextUpQuery>,
    headers: HeaderMap,
    uri: Uri,
) -> Result<Json<QueryResultDto<BaseItemDto>>, AppError> {
    let user = authenticated_query_user(&state, &headers, &uri, query.user_id.as_deref()).await?;
    let Some(database) = state.database() else {
        return Err(AppError::internal("database is not configured"));
    };

    let window = ItemWindow::from_media_query(&MediaListQuery {
        user_id: None,
        parent_id: None,
        start_index: query.start_index,
        limit: query.limit,
        include_item_types: query.include_item_types.clone(),
        sort_by: query.sort_by.clone(),
        sort_order: query.sort_order.clone(),
        fields: query.fields.clone(),
    });
    let options = item_query_options(
        query.include_item_types.as_deref(),
        query.sort_by.as_deref(),
        query.sort_order.as_deref(),
        ItemSortField::IndexNumber,
        SortDirection::Asc,
    );
    let _requested_fields = requested_item_fields(query.fields.as_deref());
    let result = LibraryRepository::new(database.clone())
        .list_next_up_items(NextUpInput {
            user_id: user.id,
            series_id: normalized_parent_id(query.series_id),
            parent_id: normalized_parent_id(query.parent_id),
            start_index: window.start_index,
            limit: window.limit,
            options,
        })
        .await
        .map_err(|err| AppError::internal(format!("failed to list next up items: {err}")))?;

    Ok(Json(QueryResultDto::new(
        media_items_to_dtos(result.items),
        result.total_record_count,
        window.start_index as u32,
    )))
}

pub async fn upcoming(
    State(state): State<AppState>,
    Query(query): Query<UpcomingQuery>,
    headers: HeaderMap,
    uri: Uri,
) -> Result<Json<QueryResultDto<BaseItemDto>>, AppError> {
    let user = authenticated_query_user(&state, &headers, &uri, query.user_id.as_deref()).await?;
    let Some(database) = state.database() else {
        return Err(AppError::internal("database is not configured"));
    };

    // Reuse the shared list window for start_index/limit normalization. Sort and
    // type filtering are fixed for upcoming episodes (premiere-date ascending),
    // so the client-supplied sort fields are parsed but intentionally not applied.
    let window = ItemWindow::from_media_query(&MediaListQuery {
        user_id: None,
        parent_id: None,
        start_index: query.start_index,
        limit: query.limit,
        include_item_types: query.include_item_types.clone(),
        sort_by: query.sort_by.clone(),
        sort_order: query.sort_order.clone(),
        fields: query.fields.clone(),
    });
    let _requested_fields = requested_item_fields(query.fields.as_deref());
    let result = LibraryRepository::new(database.clone())
        .list_upcoming_episodes(UpcomingInput {
            user_id: user.id,
            parent_id: normalized_parent_id(query.parent_id),
            start_index: window.start_index,
            limit: window.limit,
        })
        .await
        .map_err(|err| AppError::internal(format!("failed to list upcoming items: {err}")))?;

    Ok(Json(QueryResultDto::new(
        media_items_to_dtos(result.items),
        result.total_record_count,
        window.start_index as u32,
    )))
}

async fn authenticated_query_user(
    state: &AppState,
    headers: &HeaderMap,
    uri: &Uri,
    query_user_id: Option<&str>,
) -> Result<AuthenticatedUser, AppError> {
    let user = authenticate_request_user(state, headers, uri).await?;
    if let Some(query_user_id) = query_user_id
        && query_user_id != user.public_id
    {
        return Err(AppError::forbidden(
            "authenticated user does not match query user",
        ));
    }

    Ok(user)
}

#[cfg(test)]
mod tests {
    use axum::extract::Query;
    use http::Uri;

    use super::*;
    use serde_json::json;

    #[test]
    fn show_item_queries_accept_lower_camel_client_fields() {
        let uri = "/Shows/series-1/Seasons?userId=user-1&seasonId=season-1&startIndex=10&limit=25&includeItemTypes=Season&sortBy=SortName&sortOrder=Descending&fields=Overview"
            .parse::<Uri>()
            .unwrap();
        let Query(query) = Query::<ShowItemsQuery>::try_from_uri(&uri).unwrap();

        assert_eq!(query.user_id.as_deref(), Some("user-1"));
        assert_eq!(query.season_id.as_deref(), Some("season-1"));
        assert_eq!(query.start_index, Some(10));
        assert_eq!(query.limit, Some(25));
        assert_eq!(query.include_item_types.as_deref(), Some("Season"));
        assert_eq!(query.sort_by.as_deref(), Some("SortName"));
        assert_eq!(query.sort_order.as_deref(), Some("Descending"));
        assert_eq!(query.fields.as_deref(), Some("Overview"));
    }

    #[test]
    fn next_up_query_accepts_lower_camel_client_fields() {
        let uri = "/Shows/NextUp?userId=user-1&seriesId=series-1&parentId=library-1&startIndex=10&limit=25&includeItemTypes=Episode&sortBy=IndexNumber&sortOrder=Descending&fields=Overview"
            .parse::<Uri>()
            .unwrap();
        let Query(query) = Query::<NextUpQuery>::try_from_uri(&uri).unwrap();

        assert_eq!(query.user_id.as_deref(), Some("user-1"));
        assert_eq!(query.series_id.as_deref(), Some("series-1"));
        assert_eq!(query.parent_id.as_deref(), Some("library-1"));
        assert_eq!(query.start_index, Some(10));
        assert_eq!(query.limit, Some(25));
        assert_eq!(query.include_item_types.as_deref(), Some("Episode"));
        assert_eq!(query.sort_by.as_deref(), Some("IndexNumber"));
        assert_eq!(query.sort_order.as_deref(), Some("Descending"));
        assert_eq!(query.fields.as_deref(), Some("Overview"));
    }

    #[test]
    fn upcoming_query_accepts_lower_camel_client_fields() {
        let uri = "/Shows/Upcoming?userId=user-1&parentId=library-1&startIndex=10&limit=25&includeItemTypes=Episode&sortBy=PremiereDate&sortOrder=Ascending&fields=Overview"
            .parse::<Uri>()
            .unwrap();
        let Query(query) = Query::<UpcomingQuery>::try_from_uri(&uri).unwrap();

        assert_eq!(query.user_id.as_deref(), Some("user-1"));
        assert_eq!(query.parent_id.as_deref(), Some("library-1"));
        assert_eq!(query.start_index, Some(10));
        assert_eq!(query.limit, Some(25));
        assert_eq!(query.include_item_types.as_deref(), Some("Episode"));
        assert_eq!(query.sort_by.as_deref(), Some("PremiereDate"));
        assert_eq!(query.sort_order.as_deref(), Some("Ascending"));
        assert_eq!(query.fields.as_deref(), Some("Overview"));
    }

    #[test]
    fn upcoming_query_parses_official_pascal_case_fields() {
        let query = serde_json::from_value::<UpcomingQuery>(json!({
            "UserId": "user-1",
            "ParentId": "lib-1",
            "StartIndex": 10,
            "Limit": 25,
            "Fields": "Overview",
        }))
        .expect("upcoming query should deserialize");

        assert_eq!(query.user_id.as_deref(), Some("user-1"));
        assert_eq!(query.parent_id.as_deref(), Some("lib-1"));
        assert_eq!(query.start_index, Some(10));
        assert_eq!(query.limit, Some(25));
        assert_eq!(query.fields.as_deref(), Some("Overview"));
    }

    #[test]
    fn upcoming_window_normalizes_start_index_and_limit() {
        let query = UpcomingQuery {
            start_index: Some(5),
            limit: Some(40),
            ..Default::default()
        };
        let window = ItemWindow::from_media_query(&MediaListQuery {
            user_id: None,
            parent_id: None,
            start_index: query.start_index,
            limit: query.limit,
            include_item_types: None,
            sort_by: None,
            sort_order: None,
            fields: None,
        });

        assert_eq!(window.start_index, 5);
        assert_eq!(window.limit, 40);
    }

    #[test]
    fn upcoming_window_applies_default_limit_when_absent() {
        let window = ItemWindow::from_media_query(&MediaListQuery::default());

        assert_eq!(window.start_index, 0);
        assert_eq!(window.limit, 100);
    }
}
