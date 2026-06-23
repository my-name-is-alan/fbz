use axum::{
    Json,
    extract::{Path, Query, State},
    http::{HeaderMap, StatusCode, Uri},
    response::{IntoResponse, Response},
};
use serde::Deserialize;

use crate::{
    admin::repository::{AdminRepository, ScheduledTaskAdminRecord},
    compat::emby::dto::{
        ScheduledTaskInfoDto, ScheduledTaskInfoSource, ScheduledTaskResultSource,
        ScheduledTaskTriggerSource,
    },
    error::AppError,
    scheduler::{
        repository::{
            CORE_INCREMENTAL_SCAN_TASK_KEY, CORE_METADATA_REFRESH_TASK_KEY,
            PLUGIN_SCHEDULE_TASK_TYPE,
        },
        service::{SchedulerError, SchedulerService, default_worker_id, parse_interval_seconds},
    },
    state::AppState,
};

use super::access::authenticate_request_user;

const MAX_EMBY_SCHEDULED_TASKS_LIST_LIMIT: i64 = 200;
const TICKS_PER_SECOND: i64 = 10_000_000;

#[derive(Clone, Debug, Default, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "PascalCase")]
pub struct ScheduledTasksQuery {
    pub is_hidden: Option<bool>,
    pub is_enabled: Option<bool>,
}

pub async fn list_scheduled_tasks(
    State(state): State<AppState>,
    Query(query): Query<ScheduledTasksQuery>,
    headers: HeaderMap,
    uri: Uri,
) -> Result<Json<Vec<ScheduledTaskInfoDto>>, AppError> {
    authenticate_admin_compatible(&state, &headers, &uri).await?;
    let Some(database) = state.database() else {
        return Err(AppError::internal("database is not configured"));
    };

    if query.is_hidden == Some(true) {
        return Ok(Json(Vec::new()));
    }

    let tasks = AdminRepository::new(database.clone())
        .list_scheduled_tasks(MAX_EMBY_SCHEDULED_TASKS_LIST_LIMIT)
        .await
        .map_err(|err| AppError::internal(format!("failed to list scheduled tasks: {err}")))?;

    Ok(Json(
        tasks
            .into_iter()
            .filter(|task| {
                query
                    .is_enabled
                    .is_none_or(|enabled| task.enabled == enabled)
            })
            .map(scheduled_task_to_dto)
            .collect(),
    ))
}

pub async fn scheduled_task_by_id(
    State(state): State<AppState>,
    Path(task_id): Path<String>,
    headers: HeaderMap,
    uri: Uri,
) -> Result<Json<ScheduledTaskInfoDto>, AppError> {
    authenticate_admin_compatible(&state, &headers, &uri).await?;
    let task_id = validate_scheduled_task_id(&task_id)?;
    let Some(database) = state.database() else {
        return Err(AppError::internal("database is not configured"));
    };

    let Some(task) = AdminRepository::new(database.clone())
        .find_scheduled_task(task_id)
        .await
        .map_err(|err| AppError::internal(format!("failed to get scheduled task: {err}")))?
    else {
        return Err(AppError::not_found("scheduled task not found"));
    };

    Ok(Json(scheduled_task_to_dto(task)))
}

pub async fn run_scheduled_task(
    State(state): State<AppState>,
    Path(task_id): Path<String>,
    headers: HeaderMap,
    uri: Uri,
) -> Result<Response, AppError> {
    authenticate_admin_compatible(&state, &headers, &uri).await?;
    let task_id = validate_scheduled_task_id(&task_id)?;
    let Some(database) = state.database() else {
        return Err(AppError::internal("database is not configured"));
    };

    let Some(task) = AdminRepository::new(database.clone())
        .find_scheduled_task(task_id)
        .await
        .map_err(|err| AppError::internal(format!("failed to get scheduled task: {err}")))?
    else {
        return Err(AppError::not_found("scheduled task not found"));
    };

    SchedulerService::with_worker_id(database.clone(), default_worker_id("emby-manual"))
        .run_task_once(&task.task_key)
        .await
        .map_err(scheduler_error_to_app_error)?;

    Ok((StatusCode::OK, "").into_response())
}

pub async fn stop_scheduled_task(
    State(state): State<AppState>,
    Path(task_id): Path<String>,
    headers: HeaderMap,
    uri: Uri,
) -> Result<Response, AppError> {
    authenticate_admin_compatible(&state, &headers, &uri).await?;
    let task_id = validate_scheduled_task_id(&task_id)?;
    let Some(database) = state.database() else {
        return Err(AppError::internal("database is not configured"));
    };

    let Some(task) = AdminRepository::new(database.clone())
        .find_scheduled_task(task_id)
        .await
        .map_err(|err| AppError::internal(format!("failed to get scheduled task: {err}")))?
    else {
        return Err(AppError::not_found("scheduled task not found"));
    };

    SchedulerService::with_worker_id(database.clone(), default_worker_id("emby-manual"))
        .cancel_running_task(&task.task_key)
        .await
        .map_err(scheduler_error_to_app_error)?;

    Ok((StatusCode::OK, "").into_response())
}

async fn authenticate_admin_compatible(
    state: &AppState,
    headers: &HeaderMap,
    uri: &Uri,
) -> Result<(), AppError> {
    let user = authenticate_request_user(state, headers, uri).await?;

    if !user.can_manage_server() {
        return Err(AppError::forbidden("server management permission required"));
    }

    Ok(())
}

fn validate_scheduled_task_id(value: &str) -> Result<&str, AppError> {
    let value = value.trim();
    if value.is_empty() || value.len() > 200 {
        return Err(AppError::unprocessable("invalid scheduled task id"));
    }

    Ok(value)
}

fn scheduler_error_to_app_error(error: SchedulerError) -> AppError {
    match error {
        SchedulerError::TaskNotFound(_) => AppError::not_found(error.to_string()),
        SchedulerError::TaskNotRunning(_) => AppError::not_found(error.to_string()),
        SchedulerError::TaskDisabled(_)
        | SchedulerError::TaskConcurrencyLimit { .. }
        | SchedulerError::InvalidInterval(_)
        | SchedulerError::InvalidCron(_)
        | SchedulerError::UnsupportedScheduleKind(_)
        | SchedulerError::UnsupportedTaskType(_) => AppError::conflict(error.to_string()),
        SchedulerError::Database(_) => AppError::internal(error.to_string()),
    }
}

fn scheduled_task_to_dto(record: ScheduledTaskAdminRecord) -> ScheduledTaskInfoDto {
    let name = scheduled_task_name(&record);
    let key = record.task_key.clone();
    let description = scheduled_task_description(&record);
    let category = scheduled_task_category(&record);
    let state = if record.active_run_count > 0 {
        "Running".to_owned()
    } else {
        "Idle".to_owned()
    };
    let trigger = scheduled_task_trigger(&record);
    let last_execution_result = record.last_run_at.clone().map(|last_run_at| {
        let status = if record.last_error.is_some() {
            "Failed"
        } else {
            "Completed"
        };

        ScheduledTaskResultSource {
            start_time_utc: last_run_at.clone(),
            end_time_utc: Some(last_run_at),
            status: status.to_owned(),
            name: name.clone(),
            key: key.clone(),
            id: record
                .last_run_id
                .clone()
                .unwrap_or_else(|| record.id.clone()),
            error_message: record.last_error.clone(),
            long_error_message: record.last_error.clone(),
        }
    });

    ScheduledTaskInfoDto::from(ScheduledTaskInfoSource {
        id: record.id,
        key: record.task_key,
        name,
        description,
        category,
        state,
        current_progress_percentage: None,
        last_execution_result,
        triggers: vec![trigger],
        is_hidden: false,
    })
}

fn scheduled_task_name(record: &ScheduledTaskAdminRecord) -> String {
    match record.task_key.as_str() {
        CORE_INCREMENTAL_SCAN_TASK_KEY => "Incremental library scan".to_owned(),
        CORE_METADATA_REFRESH_TASK_KEY => "Metadata refresh".to_owned(),
        _ if record.task_type == PLUGIN_SCHEDULE_TASK_TYPE => {
            format!("Plugin schedule: {}", record.task_key)
        }
        _ => record.task_key.replace(['.', '_'], " "),
    }
}

fn scheduled_task_description(record: &ScheduledTaskAdminRecord) -> String {
    match record.task_key.as_str() {
        CORE_INCREMENTAL_SCAN_TASK_KEY => "Scans enabled media libraries for changes.".to_owned(),
        CORE_METADATA_REFRESH_TASK_KEY => "Refreshes metadata for existing media items.".to_owned(),
        _ if record.task_type == PLUGIN_SCHEDULE_TASK_TYPE => {
            "Runs a plugin-owned scheduled hook.".to_owned()
        }
        _ => format!("Scheduled task {}", record.task_key),
    }
}

fn scheduled_task_category(record: &ScheduledTaskAdminRecord) -> String {
    if record.owner_type == "plugin" || record.task_type == PLUGIN_SCHEDULE_TASK_TYPE {
        "Plugins".to_owned()
    } else {
        "Library".to_owned()
    }
}

fn scheduled_task_trigger(record: &ScheduledTaskAdminRecord) -> ScheduledTaskTriggerSource {
    let max_runtime_ticks = Some(i64::from(record.timeout_seconds) * TICKS_PER_SECOND);
    match record.schedule_kind.as_str() {
        "interval" => ScheduledTaskTriggerSource {
            trigger_type: "IntervalTrigger".to_owned(),
            time_of_day_ticks: None,
            interval_ticks: parse_interval_seconds(&record.schedule_value)
                .ok()
                .map(|seconds| seconds as i64 * TICKS_PER_SECOND),
            system_event: None,
            day_of_week: None,
            max_runtime_ticks,
        },
        "cron" => ScheduledTaskTriggerSource {
            trigger_type: "CronTrigger".to_owned(),
            time_of_day_ticks: None,
            interval_ticks: None,
            system_event: None,
            day_of_week: None,
            max_runtime_ticks,
        },
        _ => ScheduledTaskTriggerSource {
            trigger_type: "ScheduledTrigger".to_owned(),
            time_of_day_ticks: None,
            interval_ticks: None,
            system_event: None,
            day_of_week: None,
            max_runtime_ticks,
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn record() -> ScheduledTaskAdminRecord {
        ScheduledTaskAdminRecord {
            id: "task-1".to_owned(),
            task_key: CORE_INCREMENTAL_SCAN_TASK_KEY.to_owned(),
            task_type: "library.scan_all".to_owned(),
            owner_type: "core".to_owned(),
            owner_id: None,
            enabled: true,
            schedule_kind: "interval".to_owned(),
            schedule_value: "15m".to_owned(),
            next_run_at: Some("2026-06-22T02:00:00Z".to_owned()),
            last_run_at: Some("2026-06-22T01:00:00Z".to_owned()),
            timeout_seconds: 300,
            max_concurrency: 1,
            active_run_count: 0,
            last_run_id: Some("run-1".to_owned()),
            failure_count: 0,
            last_error: None,
            created_at: "2026-06-22T00:00:00Z".to_owned(),
            updated_at: "2026-06-22T00:00:00Z".to_owned(),
        }
    }

    #[test]
    fn scheduled_task_mapping_preserves_emby_shape() {
        let dto = scheduled_task_to_dto(record());

        assert_eq!(dto.id, "task-1");
        assert_eq!(dto.key, CORE_INCREMENTAL_SCAN_TASK_KEY);
        assert_eq!(dto.name, "Incremental library scan");
        assert_eq!(dto.category, "Library");
        assert_eq!(dto.state, "Idle");
        assert_eq!(dto.triggers[0].interval_ticks, Some(9_000_000_000));
        assert_eq!(dto.last_execution_result.unwrap().status, "Completed");
    }

    #[test]
    fn scheduled_task_mapping_marks_active_runs_as_running() {
        let mut record = record();
        record.active_run_count = 2;

        let dto = scheduled_task_to_dto(record);

        assert_eq!(dto.state, "Running");
    }

    #[test]
    fn scheduled_task_mapping_marks_failed_last_run() {
        let mut record = record();
        record.last_error = Some("boom".to_owned());

        let dto = scheduled_task_to_dto(record);
        let result = dto.last_execution_result.unwrap();

        assert_eq!(result.status, "Failed");
        assert_eq!(result.error_message.as_deref(), Some("boom"));
    }

    #[test]
    fn scheduled_task_id_validation_rejects_empty_values() {
        assert!(validate_scheduled_task_id("task-1").is_ok());
        assert!(validate_scheduled_task_id(" ").is_err());
    }

    #[test]
    fn scheduler_errors_map_to_emby_statuses() {
        let missing =
            scheduler_error_to_app_error(SchedulerError::TaskNotFound("missing".to_owned()));
        assert_eq!(missing.status_code(), StatusCode::NOT_FOUND);

        let disabled =
            scheduler_error_to_app_error(SchedulerError::TaskDisabled("disabled".to_owned()));
        assert_eq!(disabled.status_code(), StatusCode::CONFLICT);

        let concurrency = scheduler_error_to_app_error(SchedulerError::TaskConcurrencyLimit {
            task_key: "task-1".to_owned(),
            max_concurrency: 1,
        });
        assert_eq!(concurrency.status_code(), StatusCode::CONFLICT);

        let not_running =
            scheduler_error_to_app_error(SchedulerError::TaskNotRunning("task-1".to_owned()));
        assert_eq!(not_running.status_code(), StatusCode::NOT_FOUND);
    }
}
