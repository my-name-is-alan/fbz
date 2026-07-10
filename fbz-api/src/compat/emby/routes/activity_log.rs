use axum::{
    Json,
    extract::{Query, State},
    http::{HeaderMap, Uri},
};
use serde::{Deserialize, Serialize};

use crate::{
    admin::repository::{ActivityLogEntryRecord, AdminRepository},
    compat::emby::dto::QueryResultDto,
    error::AppError,
    state::AppState,
};

use super::access::authenticate_request_user;

const MAX_ACTIVITY_LOG_LIMIT: u32 = 100;
const MAX_ACTIVITY_LOG_START_INDEX: u32 = 10_000;
const MAX_ACTIVITY_LOG_MIN_DATE_LEN: usize = 64;

#[derive(Clone, Debug, Default, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "PascalCase")]
pub struct ActivityLogQuery {
    #[serde(alias = "startIndex", alias = "start_index")]
    pub start_index: Option<u32>,
    #[serde(alias = "limit")]
    pub limit: Option<u32>,
    #[serde(alias = "minDate", alias = "min_date")]
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
    let Some(database) = state.database() else {
        return Err(AppError::internal("database is not configured"));
    };

    let input = activity_log_input(&query)?;

    let page = AdminRepository::new(database.clone())
        .list_activity_log_entries(
            input.min_date.as_deref(),
            i64::from(input.start_index),
            i64::from(input.limit),
        )
        .await
        .map_err(|err| AppError::internal(format!("failed to list activity log: {err}")))?;

    Ok(Json(QueryResultDto::new(
        page.entries
            .into_iter()
            .map(activity_log_entry_to_dto)
            .collect(),
        page.total_record_count,
        input.start_index,
    )))
}

fn activity_log_entry_to_dto(record: ActivityLogEntryRecord) -> ActivityLogEntryDto {
    ActivityLogEntryDto {
        id: record.id,
        name: record.name,
        overview: record.overview.clone(),
        short_overview: record.overview,
        entry_type: record.entry_type,
        item_id: record.item_id,
        date: record.date,
        user_id: record.user_id,
        user_primary_image_tag: None,
        severity: match record.severity.as_str() {
            "error" => LogSeverityDto::Error,
            "warn" => LogSeverityDto::Warn,
            _ => LogSeverityDto::Info,
        },
    }
}

fn activity_log_input(query: &ActivityLogQuery) -> Result<ActivityLogInput, AppError> {
    Ok(ActivityLogInput {
        start_index: query
            .start_index
            .unwrap_or(0)
            .min(MAX_ACTIVITY_LOG_START_INDEX),
        limit: query
            .limit
            .unwrap_or(MAX_ACTIVITY_LOG_LIMIT)
            .min(MAX_ACTIVITY_LOG_LIMIT),
        min_date: normalize_min_date(query.min_date.as_deref())?,
    })
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

    if !is_timestamp_shaped(value) {
        return Err(AppError::unprocessable("MinDate is invalid"));
    }

    Ok(Some(value.to_owned()))
}

/// MinDate 直接进 SQL `::timestamptz` 转换，这里限定 ISO-8601 形状防转换报错：
/// `YYYY-MM-DD[( |T)HH:MM[:SS[.fff]][Z|±HH[:MM]]]`。
fn is_timestamp_shaped(value: &str) -> bool {
    let bytes = value.as_bytes();
    let digits = |range: std::ops::Range<usize>| {
        bytes
            .get(range)
            .is_some_and(|part| part.iter().all(u8::is_ascii_digit))
    };
    if !(digits(0..4)
        && bytes.get(4) == Some(&b'-')
        && digits(5..7)
        && bytes.get(7) == Some(&b'-')
        && digits(8..10))
    {
        return false;
    }
    let rest = &value[10..];
    if rest.is_empty() {
        return true;
    }
    if !rest.starts_with('T') && !rest.starts_with(' ') {
        return false;
    }

    rest[1..]
        .chars()
        .all(|ch| ch.is_ascii_digit() || matches!(ch, ':' | '.' | '+' | '-' | 'Z'))
}

#[cfg(test)]
mod tests {
    use axum::extract::Query;
    use http::Uri;
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
    fn activity_log_query_clamps_pathologically_large_start_index() {
        let input = activity_log_input(&ActivityLogQuery {
            start_index: Some(500_000),
            limit: Some(50),
            min_date: None,
        })
        .expect("activity log query should normalize");

        assert_eq!(input.start_index, 10_000);
        assert_eq!(input.limit, 50);
    }

    #[test]
    fn activity_log_query_accepts_lower_camel_client_fields() {
        let uri =
            "/System/ActivityLog/Entries?startIndex=25&limit=10000&minDate=2024-01-01T00:00:00Z"
                .parse::<Uri>()
                .unwrap();
        let Query(query) = Query::<ActivityLogQuery>::try_from_uri(&uri).unwrap();
        let input = activity_log_input(&query).unwrap();

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
    fn min_date_requires_timestamp_shape() {
        assert!(is_timestamp_shaped("2024-01-01"));
        assert!(is_timestamp_shaped("2024-01-01T00:00:00Z"));
        assert!(is_timestamp_shaped("2024-01-01 12:30:45.123+08:00"));
        assert!(!is_timestamp_shaped("yesterday"));
        assert!(!is_timestamp_shaped("2024-01-01x"));
        assert!(!is_timestamp_shaped("2024-01-01T12:00; drop table users"));
        assert!(
            activity_log_input(&ActivityLogQuery {
                min_date: Some("not-a-date".to_owned()),
                ..ActivityLogQuery::default()
            })
            .is_err()
        );
    }

    #[test]
    fn activity_log_record_maps_severity_to_dto() {
        let dto = activity_log_entry_to_dto(ActivityLogEntryRecord {
            id: 2000000000042,
            name: "library.scan job failed".to_owned(),
            overview: Some("boom".to_owned()),
            entry_type: "ScheduledTaskFailed".to_owned(),
            item_id: None,
            date: "2026-01-01T00:00:00Z".to_owned(),
            user_id: None,
            severity: "error".to_owned(),
        });

        assert_eq!(dto.id, 2000000000042);
        assert_eq!(dto.severity, LogSeverityDto::Error);
        assert_eq!(dto.short_overview.as_deref(), Some("boom"));
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
