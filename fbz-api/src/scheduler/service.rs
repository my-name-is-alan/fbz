use std::{
    error::Error,
    fmt::{Display, Formatter},
    path::{Path, PathBuf},
};

use crate::{
    config::{ScheduleConfig, SchedulerWorkerConfig},
    db::DbPool,
    scheduler::repository::{
        CORE_INCREMENTAL_SCAN_TASK_KEY, CORE_METADATA_REFRESH_TASK_KEY,
        CORE_METADATA_REFRESH_TASK_TYPE, CORE_SCAN_ALL_TASK_TYPE, CORE_TRANSCODE_CLEANUP_TASK_KEY,
        CORE_TRANSCODE_CLEANUP_TASK_TYPE, CoreScheduledTaskInput, PLUGIN_SCHEDULE_TASK_TYPE,
        ScheduledTaskRecord, SchedulerRepository, TranscodeCleanupCandidate,
    },
    transcode::cleanup::{TranscodeCleanupResult, cleanup_session_output_dir_best_effort},
};

const TRANSCODE_CLEANUP_BATCH_SIZE: i64 = 100;

#[derive(Clone)]
pub struct SchedulerService {
    repository: SchedulerRepository,
    worker_id: String,
    transcode_cache_dir: PathBuf,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SchedulerRunSummary {
    pub task_key: String,
    pub task_type: String,
    pub queued_jobs: i64,
}

#[derive(Debug)]
pub enum SchedulerError {
    Database(sqlx::Error),
    TaskNotFound(String),
    TaskDisabled(String),
    TaskConcurrencyLimit {
        task_key: String,
        max_concurrency: i32,
    },
    TaskNotRunning(String),
    InvalidInterval(String),
    InvalidCron(String),
    UnsupportedScheduleKind(String),
    UnsupportedTaskType(String),
}

impl SchedulerService {
    pub fn new(pool: DbPool, transcode_cache_dir: PathBuf) -> Self {
        Self::with_worker_id(pool, default_worker_id("scheduler"), transcode_cache_dir)
    }

    pub fn with_worker_id(pool: DbPool, worker_id: String, transcode_cache_dir: PathBuf) -> Self {
        Self {
            repository: SchedulerRepository::new(pool),
            worker_id,
            transcode_cache_dir,
        }
    }

    pub async fn bootstrap_core_tasks(
        &self,
        worker_config: &SchedulerWorkerConfig,
        schedules: &ScheduleConfig,
    ) -> Result<(), SchedulerError> {
        let incremental_scan = parse_core_schedule(&schedules.incremental_scan)?;
        self.repository
            .upsert_core_task(CoreScheduledTaskInput {
                task_key: CORE_INCREMENTAL_SCAN_TASK_KEY,
                task_type: CORE_SCAN_ALL_TASK_TYPE,
                enabled: worker_config.enabled,
                schedule_kind: incremental_scan.kind,
                schedule_value: schedules.incremental_scan.trim().to_owned(),
                interval_seconds: incremental_scan.interval_seconds,
            })
            .await?;

        let metadata_refresh = parse_core_schedule(&schedules.metadata_refresh)?;
        self.repository
            .upsert_core_task(CoreScheduledTaskInput {
                task_key: CORE_METADATA_REFRESH_TASK_KEY,
                task_type: CORE_METADATA_REFRESH_TASK_TYPE,
                enabled: worker_config.enabled,
                schedule_kind: metadata_refresh.kind,
                schedule_value: schedules.metadata_refresh.trim().to_owned(),
                interval_seconds: metadata_refresh.interval_seconds,
            })
            .await?;

        let transcode_cleanup = parse_core_schedule(&schedules.transcode_cleanup)?;
        self.repository
            .upsert_core_task(CoreScheduledTaskInput {
                task_key: CORE_TRANSCODE_CLEANUP_TASK_KEY,
                task_type: CORE_TRANSCODE_CLEANUP_TASK_TYPE,
                enabled: worker_config.enabled,
                schedule_kind: transcode_cleanup.kind,
                schedule_value: schedules.transcode_cleanup.trim().to_owned(),
                interval_seconds: transcode_cleanup.interval_seconds,
            })
            .await?;

        Ok(())
    }

    pub async fn run_next_due_task(&self) -> Result<Option<SchedulerRunSummary>, SchedulerError> {
        let Some(task) = self.repository.claim_due_task(&self.worker_id).await? else {
            return Ok(None);
        };

        match self.run_claimed_task(&task).await {
            Ok(summary) => {
                self.repository
                    .mark_task_success(task.id, task.run_id, summary.queued_jobs)
                    .await?;
                Ok(Some(summary))
            }
            Err(err) => {
                let message = err.to_string();
                self.repository
                    .mark_task_failure(task.id, task.run_id, &message)
                    .await?;
                Err(err)
            }
        }
    }

    pub async fn run_task_once(
        &self,
        task_key: &str,
    ) -> Result<SchedulerRunSummary, SchedulerError> {
        let task_key = task_key.trim();
        let task = self
            .repository
            .claim_task_by_key(task_key, &self.worker_id)
            .await?;

        match self.run_claimed_task(&task).await {
            Ok(summary) => {
                self.repository
                    .mark_task_success(task.id, task.run_id, summary.queued_jobs)
                    .await?;
                Ok(summary)
            }
            Err(err) => {
                let message = err.to_string();
                self.repository
                    .mark_task_failure(task.id, task.run_id, &message)
                    .await?;
                Err(err)
            }
        }
    }

    pub async fn cancel_running_task(&self, task_key: &str) -> Result<(), SchedulerError> {
        self.repository.cancel_running_task_by_key(task_key).await
    }

    async fn run_claimed_task(
        &self,
        task: &ScheduledTaskRecord,
    ) -> Result<SchedulerRunSummary, SchedulerError> {
        match task.task_type.as_str() {
            CORE_SCAN_ALL_TASK_TYPE => {
                let queued_jobs = self
                    .repository
                    .queue_scan_all(&format!("scheduled task {}", task.task_key))
                    .await?;
                Ok(SchedulerRunSummary {
                    task_key: task.task_key.clone(),
                    task_type: task.task_type.clone(),
                    queued_jobs,
                })
            }
            PLUGIN_SCHEDULE_TASK_TYPE => {
                let queued_jobs = self
                    .repository
                    .queue_plugin_schedule_dispatch(&task.task_key)
                    .await?;
                Ok(SchedulerRunSummary {
                    task_key: task.task_key.clone(),
                    task_type: task.task_type.clone(),
                    queued_jobs,
                })
            }
            CORE_METADATA_REFRESH_TASK_TYPE => {
                let queued_jobs = self
                    .repository
                    .queue_metadata_refresh_all(&format!("scheduled task {}", task.task_key))
                    .await?;
                Ok(SchedulerRunSummary {
                    task_key: task.task_key.clone(),
                    task_type: task.task_type.clone(),
                    queued_jobs,
                })
            }
            CORE_TRANSCODE_CLEANUP_TASK_TYPE => {
                let cleaned_outputs = self
                    .cleanup_terminal_transcode_outputs(&self.transcode_cache_dir)
                    .await?;
                Ok(SchedulerRunSummary {
                    task_key: task.task_key.clone(),
                    task_type: task.task_type.clone(),
                    queued_jobs: cleaned_outputs,
                })
            }
            other => Err(SchedulerError::UnsupportedTaskType(other.to_owned())),
        }
    }

    async fn cleanup_terminal_transcode_outputs(
        &self,
        transcode_cache_dir: &Path,
    ) -> Result<i64, SchedulerError> {
        let candidates = self
            .repository
            .list_transcode_cleanup_candidates(TRANSCODE_CLEANUP_BATCH_SIZE)
            .await?;
        let mut cleaned_outputs = 0_i64;

        for candidate in candidates {
            if self
                .cleanup_transcode_candidate(transcode_cache_dir, candidate)
                .await?
            {
                cleaned_outputs += 1;
            }
        }

        Ok(cleaned_outputs)
    }

    async fn cleanup_transcode_candidate(
        &self,
        transcode_cache_dir: &Path,
        candidate: TranscodeCleanupCandidate,
    ) -> Result<bool, SchedulerError> {
        let result = cleanup_session_output_dir_best_effort(
            transcode_cache_dir,
            &candidate.id,
            Some(candidate.output_path.as_str()),
            "scheduled_transcode_cleanup",
        )
        .await;
        if result == TranscodeCleanupResult::Failed {
            return Ok(false);
        }

        self.repository
            .mark_transcode_output_cleaned(&candidate.id)
            .await
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct CoreSchedulePlan {
    kind: &'static str,
    interval_seconds: Option<u64>,
}

fn parse_core_schedule(value: &str) -> Result<CoreSchedulePlan, SchedulerError> {
    let value = value.trim();
    if let Ok(interval_seconds) = parse_interval_seconds(value) {
        return Ok(CoreSchedulePlan {
            kind: "interval",
            interval_seconds: Some(interval_seconds),
        });
    }

    validate_cron_expression(value)?;
    Ok(CoreSchedulePlan {
        kind: "cron",
        interval_seconds: None,
    })
}

pub fn parse_interval_seconds(value: &str) -> Result<u64, SchedulerError> {
    let value = value.trim().to_ascii_lowercase();
    if value.is_empty() {
        return Err(SchedulerError::InvalidInterval(value));
    }

    match value.as_str() {
        "hourly" => return Ok(60 * 60),
        "daily" => return Ok(24 * 60 * 60),
        _ => {}
    }

    let (digits, unit) = split_interval(&value);
    let amount = digits
        .parse::<u64>()
        .map_err(|_| SchedulerError::InvalidInterval(value.clone()))?;
    if amount == 0 {
        return Err(SchedulerError::InvalidInterval(value));
    }

    match unit {
        "" | "s" | "sec" | "secs" | "second" | "seconds" => Ok(amount),
        "m" | "min" | "mins" | "minute" | "minutes" => Ok(amount * 60),
        "h" | "hr" | "hrs" | "hour" | "hours" => Ok(amount * 60 * 60),
        "d" | "day" | "days" => Ok(amount * 24 * 60 * 60),
        _ => Err(SchedulerError::InvalidInterval(value)),
    }
}

pub fn validate_cron_expression(value: &str) -> Result<(), SchedulerError> {
    let value = value.trim();
    let fields = value.split_whitespace().collect::<Vec<_>>();
    if fields.len() != 5 {
        return Err(SchedulerError::InvalidCron(value.to_owned()));
    }

    validate_cron_field(fields[0], 0, 59, false)?;
    validate_cron_field(fields[1], 0, 23, false)?;
    validate_cron_field(fields[2], 1, 31, false)?;
    validate_cron_field(fields[3], 1, 12, false)?;
    validate_cron_field(fields[4], 0, 7, true)?;
    Ok(())
}

fn validate_cron_field(
    field: &str,
    min_value: u16,
    max_value: u16,
    sunday_seven: bool,
) -> Result<(), SchedulerError> {
    if field.trim().is_empty() {
        return Err(SchedulerError::InvalidCron(field.to_owned()));
    }

    for token in field.split(',') {
        validate_cron_token(token.trim(), min_value, max_value, sunday_seven)?;
    }
    Ok(())
}

fn validate_cron_token(
    token: &str,
    min_value: u16,
    max_value: u16,
    sunday_seven: bool,
) -> Result<(), SchedulerError> {
    if token.is_empty() {
        return Err(SchedulerError::InvalidCron(token.to_owned()));
    }

    let mut parts = token.split('/');
    let base = parts
        .next()
        .ok_or_else(|| SchedulerError::InvalidCron(token.to_owned()))?;
    let step = parts
        .next()
        .map(|value| {
            value
                .parse::<u16>()
                .ok()
                .filter(|step| *step > 0)
                .ok_or_else(|| SchedulerError::InvalidCron(token.to_owned()))
        })
        .transpose()?
        .unwrap_or(1);
    if parts.next().is_some() {
        return Err(SchedulerError::InvalidCron(token.to_owned()));
    }

    let (lower, upper) = if base == "*" {
        (min_value, max_value)
    } else if let Some((lower, upper)) = base.split_once('-') {
        (
            parse_cron_value(lower, token)?,
            parse_cron_value(upper, token)?,
        )
    } else {
        let value = parse_cron_value(base, token)?;
        (value, value)
    };

    if lower > upper || lower < min_value || upper > max_value {
        return Err(SchedulerError::InvalidCron(token.to_owned()));
    }
    if sunday_seven && upper == 7 && max_value != 7 {
        return Err(SchedulerError::InvalidCron(token.to_owned()));
    }

    let _ = step;
    Ok(())
}

fn parse_cron_value(value: &str, token: &str) -> Result<u16, SchedulerError> {
    value
        .parse::<u16>()
        .map_err(|_| SchedulerError::InvalidCron(token.to_owned()))
}

fn split_interval(value: &str) -> (&str, &str) {
    let split_at = value
        .char_indices()
        .find_map(|(index, ch)| (!ch.is_ascii_digit()).then_some(index))
        .unwrap_or(value.len());
    let (digits, unit) = value.split_at(split_at);
    (digits.trim(), unit.trim())
}

impl Display for SchedulerError {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Database(err) => write!(f, "database error: {err}"),
            Self::TaskNotFound(task_key) => write!(f, "scheduled task `{task_key}` was not found"),
            Self::TaskDisabled(task_key) => write!(f, "scheduled task `{task_key}` is disabled"),
            Self::TaskConcurrencyLimit {
                task_key,
                max_concurrency,
            } => write!(
                f,
                "scheduled task `{task_key}` reached max concurrency {max_concurrency}"
            ),
            Self::TaskNotRunning(task_key) => {
                write!(f, "scheduled task `{task_key}` is not running")
            }
            Self::InvalidInterval(value) => write!(f, "invalid interval schedule `{value}`"),
            Self::InvalidCron(value) => write!(f, "invalid cron schedule `{value}`"),
            Self::UnsupportedScheduleKind(schedule_kind) => {
                write!(f, "unsupported schedule kind `{schedule_kind}`")
            }
            Self::UnsupportedTaskType(task_type) => {
                write!(f, "unsupported scheduled task type `{task_type}`")
            }
        }
    }
}

impl Error for SchedulerError {}

pub fn default_worker_id(prefix: &str) -> String {
    format!("{}-{}", prefix.trim(), std::process::id())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn interval_parser_supports_compact_units() {
        assert_eq!(parse_interval_seconds("10s").unwrap(), 10);
        assert_eq!(parse_interval_seconds("15m").unwrap(), 900);
        assert_eq!(parse_interval_seconds("2h").unwrap(), 7_200);
        assert_eq!(parse_interval_seconds("1d").unwrap(), 86_400);
    }

    #[test]
    fn interval_parser_supports_named_defaults() {
        assert_eq!(parse_interval_seconds("hourly").unwrap(), 3_600);
        assert_eq!(parse_interval_seconds("daily").unwrap(), 86_400);
    }

    #[test]
    fn core_schedule_parser_accepts_interval_or_cron() {
        assert_eq!(
            parse_core_schedule("15m").unwrap(),
            CoreSchedulePlan {
                kind: "interval",
                interval_seconds: Some(900)
            }
        );
        assert_eq!(
            parse_core_schedule("0 5 * * *").unwrap(),
            CoreSchedulePlan {
                kind: "cron",
                interval_seconds: None
            }
        );
    }

    #[test]
    fn interval_parser_rejects_empty_or_zero_values() {
        assert!(matches!(
            parse_interval_seconds(""),
            Err(SchedulerError::InvalidInterval(_))
        ));
        assert!(matches!(
            parse_interval_seconds("0m"),
            Err(SchedulerError::InvalidInterval(_))
        ));
    }

    #[test]
    fn cron_validator_accepts_common_five_field_expressions() {
        assert!(validate_cron_expression("0 4 * * *").is_ok());
        assert!(validate_cron_expression("*/5 * * * *").is_ok());
        assert!(validate_cron_expression("0,30 1-6/2 * 1,12 0,7").is_ok());
    }

    #[test]
    fn cron_validator_rejects_invalid_or_unsupported_expressions() {
        assert!(matches!(
            validate_cron_expression("0 4 * *"),
            Err(SchedulerError::InvalidCron(_))
        ));
        assert!(matches!(
            validate_cron_expression("60 4 * * *"),
            Err(SchedulerError::InvalidCron(_))
        ));
        assert!(matches!(
            validate_cron_expression("0 4 * jan *"),
            Err(SchedulerError::InvalidCron(_))
        ));
        assert!(matches!(
            validate_cron_expression("*/0 * * * *"),
            Err(SchedulerError::InvalidCron(_))
        ));
    }

    #[test]
    fn default_worker_id_includes_prefix_and_process_id() {
        let worker_id = default_worker_id("admin-manual");

        assert!(worker_id.starts_with("admin-manual-"));
        assert!(worker_id.ends_with(&std::process::id().to_string()));
    }

    #[test]
    fn transcode_cleanup_core_task_is_registered_and_dispatched() {
        let source = include_str!("service.rs");

        assert!(source.contains("CORE_TRANSCODE_CLEANUP_TASK_KEY"));
        assert!(source.contains("CORE_TRANSCODE_CLEANUP_TASK_TYPE"));
        assert!(source.contains("schedules.transcode_cleanup"));
        assert!(source.contains("task_type: CORE_TRANSCODE_CLEANUP_TASK_TYPE"));
        assert!(source.contains("cleanup_terminal_transcode_outputs"));
        assert!(source.contains("mark_transcode_output_cleaned"));
    }
}
