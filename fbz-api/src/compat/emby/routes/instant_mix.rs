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

const DEFAULT_INSTANT_MIX_LIMIT: u32 = 100;
const MAX_INSTANT_MIX_LIMIT: u32 = 200;

#[derive(Clone, Debug, Default, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "PascalCase")]
pub struct InstantMixQuery {
    pub user_id: Option<String>,
    pub start_index: Option<u32>,
    pub limit: Option<u32>,
    pub fields: Option<String>,
}

pub async fn empty_instant_mix(
    State(state): State<AppState>,
    Query(query): Query<InstantMixQuery>,
    headers: HeaderMap,
    uri: Uri,
) -> Result<Json<QueryResultDto<BaseItemDto>>, AppError> {
    authenticate_query_user(&state, query.user_id.as_deref(), &headers, &uri).await?;
    let window = InstantMixWindow::from_query(&query);
    let _requested_fields = query.fields.as_deref().unwrap_or_default();

    Ok(Json(QueryResultDto::new(
        Vec::new(),
        0,
        window.start_index as u32,
    )))
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct InstantMixWindow {
    start_index: i64,
    limit: i64,
}

impl InstantMixWindow {
    fn from_query(query: &InstantMixQuery) -> Self {
        Self {
            start_index: i64::from(query.start_index.unwrap_or_default()),
            limit: i64::from(
                query
                    .limit
                    .unwrap_or(DEFAULT_INSTANT_MIX_LIMIT)
                    .clamp(1, MAX_INSTANT_MIX_LIMIT),
            ),
        }
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    #[test]
    fn instant_mix_query_accepts_common_emby_parameters() {
        let query = serde_json::from_value::<InstantMixQuery>(json!({
            "UserId": "user-1",
            "StartIndex": 5,
            "Limit": 500,
            "Fields": "MediaSources,PrimaryImageAspectRatio"
        }))
        .unwrap();

        let window = InstantMixWindow::from_query(&query);
        assert_eq!(query.user_id.as_deref(), Some("user-1"));
        assert_eq!(window.start_index, 5);
        assert_eq!(window.limit, i64::from(MAX_INSTANT_MIX_LIMIT));
        assert_eq!(
            query.fields.as_deref(),
            Some("MediaSources,PrimaryImageAspectRatio")
        );
    }
}
