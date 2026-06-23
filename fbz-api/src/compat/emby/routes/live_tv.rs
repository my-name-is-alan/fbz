use axum::{
    Json,
    extract::{Path, Query},
    response::{IntoResponse, Response},
};
use http::StatusCode;
use serde::Deserialize;
use serde_json::Value;

use crate::compat::emby::dto::{LiveTvInfoDto, QueryResultDto};

const MAX_LIVE_TV_EMPTY_LIMIT: u32 = 200;

#[derive(Clone, Debug, Default, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "PascalCase")]
pub struct LiveTvListQuery {
    #[serde(alias = "startIndex", alias = "start_index")]
    pub start_index: Option<u32>,
    #[serde(alias = "limit")]
    pub limit: Option<u32>,
}

pub async fn live_tv_info() -> Json<LiveTvInfoDto> {
    Json(LiveTvInfoDto::disabled())
}

pub async fn empty_query(Query(query): Query<LiveTvListQuery>) -> Json<QueryResultDto<Value>> {
    let start_index = query.start_index.unwrap_or_default();
    let _limit = query.limit.unwrap_or(MAX_LIVE_TV_EMPTY_LIMIT);
    Json(QueryResultDto::new(Vec::new(), 0, start_index))
}

pub async fn empty_item(Path(_id): Path<String>) -> Response {
    (StatusCode::NOT_FOUND, "").into_response()
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    #[test]
    fn disabled_info_matches_emby_live_tv_probe_shape() {
        let value = serde_json::to_value(LiveTvInfoDto::disabled()).unwrap();

        assert_eq!(value["IsEnabled"], json!(false));
        assert_eq!(value["EnabledUsers"], json!([]));
    }

    #[tokio::test]
    async fn empty_query_preserves_requested_start_index() {
        let query = LiveTvListQuery {
            start_index: Some(25),
            limit: Some(10),
        };

        let Json(result) = empty_query(Query(query)).await;

        assert_eq!(result.start_index, 25);
        assert_eq!(result.total_record_count, 0);
        assert!(result.items.is_empty());
    }
}
