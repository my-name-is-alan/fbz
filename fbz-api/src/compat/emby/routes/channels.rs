use axum::{
    Json,
    extract::{Query, State},
    http::{HeaderMap, Uri},
};
use serde::Deserialize;

use crate::{
    compat::emby::dto::{BaseItemDto, QueryResultDto},
    error::AppError,
    state::AppState,
};

use super::access::authenticate_query_user;

const MAX_CHANNEL_LIMIT: u32 = 200;
const MAX_CHANNEL_START_INDEX: u32 = 10_000;

#[derive(Clone, Debug, Default, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "PascalCase")]
pub struct ChannelQuery {
    #[serde(alias = "userId", alias = "user_id")]
    pub user_id: Option<String>,
    #[serde(alias = "startIndex", alias = "start_index")]
    pub start_index: Option<u32>,
    #[serde(alias = "limit")]
    pub limit: Option<u32>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct ChannelQueryInput {
    user_id: Option<String>,
    start_index: u32,
    limit: u32,
}

pub async fn channels(
    State(state): State<AppState>,
    Query(query): Query<ChannelQuery>,
    headers: HeaderMap,
    uri: Uri,
) -> Result<Json<QueryResultDto<BaseItemDto>>, AppError> {
    let input = channel_query_input(&query)?;
    authenticate_query_user(&state, input.user_id.as_deref(), &headers, &uri).await?;

    Ok(Json(empty_channels_result(input.start_index)))
}

fn channel_query_input(query: &ChannelQuery) -> Result<ChannelQueryInput, AppError> {
    Ok(ChannelQueryInput {
        user_id: normalize_optional_text(query.user_id.as_deref())?,
        start_index: query
            .start_index
            .unwrap_or_default()
            .min(MAX_CHANNEL_START_INDEX),
        limit: query
            .limit
            .unwrap_or(MAX_CHANNEL_LIMIT)
            .min(MAX_CHANNEL_LIMIT),
    })
}

fn empty_channels_result(start_index: u32) -> QueryResultDto<BaseItemDto> {
    QueryResultDto::new(Vec::new(), 0, start_index)
}

fn normalize_optional_text(value: Option<&str>) -> Result<Option<String>, AppError> {
    let Some(value) = value.map(str::trim).filter(|value| !value.is_empty()) else {
        return Ok(None);
    };
    if value.len() > 128 {
        return Err(AppError::unprocessable("channel query value is too long"));
    }

    Ok(Some(value.to_owned()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn channel_query_input_normalizes_paging_and_user_scope() {
        let input = channel_query_input(&ChannelQuery {
            user_id: Some(" user-1 ".to_owned()),
            start_index: Some(25),
            limit: Some(10),
        })
        .unwrap();

        assert_eq!(input.user_id.as_deref(), Some("user-1"));
        assert_eq!(input.start_index, 25);
        assert_eq!(input.limit, 10);

        let capped = channel_query_input(&ChannelQuery {
            user_id: None,
            start_index: None,
            limit: Some(999),
        })
        .unwrap();

        assert_eq!(capped.start_index, 0);
        assert_eq!(capped.limit, MAX_CHANNEL_LIMIT);

        let empty = empty_channels_result(capped.start_index);
        assert_eq!(empty.start_index, 0);
        assert_eq!(empty.total_record_count, 0);
        assert!(empty.items.is_empty());
    }

    #[test]
    fn channel_query_clamps_pathologically_large_start_index() {
        let input = channel_query_input(&ChannelQuery {
            user_id: None,
            start_index: Some(500_000),
            limit: Some(50),
        })
        .unwrap();

        assert_eq!(input.start_index, 10_000);
        assert_eq!(input.limit, 50);
    }
}
