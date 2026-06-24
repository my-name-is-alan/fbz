use axum::{
    Json,
    extract::{Query, State},
    http::{HeaderMap, Uri},
};
use serde::{Deserialize, Serialize};

use crate::{compat::emby::dto::QueryResultDto, error::AppError, state::AppState};

use super::access::authenticate_request_user;

const MAX_ACTIVITY_LOG_LIMIT: u32 = 100;
const MAX_ACTIVITY_LOG_MIN_DATE_LEN: usize = 64;

#[derive(Clone, Debug, Default, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "PascalCase")]
pub struct ActivityLogQuery {
    pub start_index: Option<u32>,
    pub limit: Option<u32>,
    pub min_date: Option<String>,
}

#[derive(Clone, Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "PascalCase")]
pub struct ActivityLogEntryDto {
    pub id: i64,
    pub name: String,
    pub overview: Option<String>,
    pub short_overview: Option<String>,
    #[serde(rename = "Type")]
    pub entry_type: String,
    pub item_id: Option<String>,
    pub date: String,
    pub user_id: Option<String>,
    pub user_primary_image_tag: Option<String>,
    pub severity: LogSeverityDto,
}

#[derive(Clone, Debug, Serialize, PartialEq, Eq)]
#[allow(dead_code)]
pub enum LogSeverityDto {
    Info,
    Debug,
    Warn,
    Error,
    Fatal,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct ActivityLogInput {
    start_index: u32,
    limit: u32,
    min_date: Option<String>,
}

pub async fn activity_log_entries(
    State(state): State<AppState>,
    Query(query): Query<ActivityLogQuery>,
    headers: HeaderMap,
    uri: Uri,
) -> Result<Json<QueryResultDto<ActivityLogEntryDto>>, AppError> {
    let user = authenticate_request_user(&state, &headers, &uri).await?;
    if !user.can_manage_server() {
        return Err(AppError::forbidden("server management permission required"));
    }

    let input = activity_log_input(&query)?;

    Ok(Json(empty_activity_log_result(&input)))
}

fn activity_log_input(query: &ActivityLogQuery) -> Result<ActivityLogInput, AppError> {
    Ok(ActivityLogInput {
        start_index: query.start_index.unwrap_or(0),
        limit: query
            .limit
            .unwrap_or(MAX_ACTIVITY_LOG_LIMIT)
            .min(MAX_ACTIVITY_LOG_LIMIT),
        min_date: normalize_min_date(query.min_date.as_deref())?,
    })
}

fn empty_activity_log_result(input: &ActivityLogInput) -> QueryResultDto<ActivityLogEntryDto> {
    let _window = input.limit;
    let _min_date = input.min_date.as_deref();

    QueryResultDto::new(Vec::new(), 0, input.start_index)
}

fn normalize_min_date(value: Option<&str>) -> Result<Option<String>, AppError> {
    let Some(raw_value) = value else {
        return Ok(None);
    };

    if raw_value.chars().any(char::is_control) {
        return Err(AppError::unprocessable(
            "MinDate contains invalid characters",
        ));
    }

    let value = raw_value.trim();
    if value.is_empty() {
        return Ok(None);
    }

    if value.chars().count() > MAX_ACTIVITY_LOG_MIN_DATE_LEN {
        return Err(AppError::unprocessable(format!(
            "MinDate must be at most {MAX_ACTIVITY_LOG_MIN_DATE_LEN} characters"
        )));
    }

    Ok(Some(value.to_owned()))
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    #[test]
    fn activity_log_query_clamps_limit_and_preserves_window_start() {
        let input = activity_log_input(&ActivityLogQuery {
            start_index: Some(25),
            limit: Some(10_000),
            min_date: Some(" 2024-01-01T00:00:00Z ".to_owned()),
        })
        .expect("activity log query should normalize");

        assert_eq!(input.start_index, 25);
        assert_eq!(input.limit, MAX_ACTIVITY_LOG_LIMIT);
        assert_eq!(input.min_date.as_deref(), Some("2024-01-01T00:00:00Z"));
    }

    #[test]
    fn activity_log_query_rejects_unsafe_min_date_text() {
        assert!(
            activity_log_input(&ActivityLogQuery {
                min_date: Some("2024-01-01T00:00:00Z\n".to_owned()),
                ..ActivityLogQuery::default()
            })
            .is_err()
        );
        assert!(
            activity_log_input(&ActivityLogQuery {
                min_date: Some("x".repeat(MAX_ACTIVITY_LOG_MIN_DATE_LEN + 1)),
                ..ActivityLogQuery::default()
            })
            .is_err()
        );
    }

    #[test]
    fn empty_activity_log_result_uses_requested_start_index() {
        let input = ActivityLogInput {
            start_index: 50,
            limit: 20,
            min_date: None,
        };

        let result = empty_activity_log_result(&input);

        assert!(result.items.is_empty());
        assert_eq!(result.total_record_count, 0);
        assert_eq!(result.start_index, 50);
    }

    #[test]
    fn activity_log_entry_serializes_official_pascal_shape() {
        let dto = ActivityLogEntryDto {
            id: 1,
            name: "Server started".to_owned(),
            overview: Some("FBZ API started".to_owned()),
            short_overview: None,
            entry_type: "System".to_owned(),
            item_id: None,
            date: "2024-01-01T00:00:00Z".to_owned(),
            user_id: Some("user-1".to_owned()),
            user_primary_image_tag: None,
            severity: LogSeverityDto::Info,
        };

        let value = serde_json::to_value(dto).unwrap();

        assert_eq!(
            value,
            json!({
                "Id": 1,
                "Name": "Server started",
                "Overview": "FBZ API started",
                "ShortOverview": null,
                "Type": "System",
                "ItemId": null,
                "Date": "2024-01-01T00:00:00Z",
                "UserId": "user-1",
                "UserPrimaryImageTag": null,
                "Severity": "Info"
            })
        );
    }
}
