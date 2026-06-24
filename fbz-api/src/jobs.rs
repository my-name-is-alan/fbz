use sqlx::{Postgres, Row, Transaction};
use tracing::warn;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ExpiredJobMessages<'a> {
    pub retry: &'a str,
    pub final_failure: &'a str,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ExpiredJobSummary {
    pub expired_jobs: u64,
    pub retryable_jobs: u64,
    pub terminal_jobs: u64,
}

const EXPIRE_STALE_RUNNING_JOBS_SQL: &str = r#"
with expired_jobs as (
    update jobs
    set status = 'failed',
        locked_by = null,
        locked_until = null,
        last_error = case
            when attempts >= max_attempts then $3
            else $2
        end,
        finished_at = case
            when attempts >= max_attempts then coalesce(finished_at, now())
            else finished_at
        end,
        updated_at = now()
    where job_type = $1
      and status = 'running'
      and locked_until <= now()
    returning id,
              last_error,
              attempts < max_attempts as retryable
),
expired_runs as (
    update job_runs jr
    set status = 'failed',
        error_message = expired_jobs.last_error,
        finished_at = coalesce(jr.finished_at, now())
    from expired_jobs
    where jr.job_id = expired_jobs.id
      and jr.status = 'running'
    returning jr.id
)
select count(*)::bigint as expired_jobs,
       count(*) filter (where retryable)::bigint as retryable_jobs,
       count(*) filter (where not retryable)::bigint as terminal_jobs
from expired_jobs
"#;

pub async fn expire_stale_running_jobs(
    tx: &mut Transaction<'_, Postgres>,
    job_type: &str,
    messages: ExpiredJobMessages<'_>,
) -> Result<ExpiredJobSummary, sqlx::Error> {
    let row = sqlx::query(EXPIRE_STALE_RUNNING_JOBS_SQL)
        .bind(job_type.trim())
        .bind(messages.retry)
        .bind(messages.final_failure)
        .fetch_one(&mut **tx)
        .await?;
    let summary = ExpiredJobSummary {
        expired_jobs: row.try_get::<i64, _>("expired_jobs")?.max(0) as u64,
        retryable_jobs: row.try_get::<i64, _>("retryable_jobs")?.max(0) as u64,
        terminal_jobs: row.try_get::<i64, _>("terminal_jobs")?.max(0) as u64,
    };

    log_expired_job_summary(job_type, summary);

    Ok(summary)
}

impl ExpiredJobSummary {
    pub fn has_work(&self) -> bool {
        self.expired_jobs > 0
    }
}

fn log_expired_job_summary(job_type: &str, summary: ExpiredJobSummary) {
    if summary.has_work() {
        warn!(
            job_type = %job_type,
            expired_jobs = summary.expired_jobs,
            retryable_jobs = summary.retryable_jobs,
            terminal_jobs = summary.terminal_jobs,
            "recovered stale running jobs"
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn expired_job_messages_keep_retry_and_terminal_errors_separate() {
        let messages = ExpiredJobMessages {
            retry: "scan job lease expired; will retry",
            final_failure: "scan job lease expired; max attempts reached",
        };

        assert_ne!(messages.retry, messages.final_failure);
        assert!(messages.retry.contains("retry"));
        assert!(messages.final_failure.contains("max attempts"));
    }

    #[test]
    fn stale_job_recovery_reports_retryable_and_terminal_counts() {
        let summary = ExpiredJobSummary {
            expired_jobs: 3,
            retryable_jobs: 2,
            terminal_jobs: 1,
        };

        assert!(summary.has_work());
        assert_eq!(
            summary.expired_jobs,
            summary.retryable_jobs + summary.terminal_jobs
        );
        assert!(EXPIRE_STALE_RUNNING_JOBS_SQL.contains("retryable_jobs"));
        assert!(EXPIRE_STALE_RUNNING_JOBS_SQL.contains("terminal_jobs"));
    }

    #[test]
    fn stale_job_recovery_logs_structured_summary_fields() {
        let source = include_str!("jobs.rs");

        assert!(source.contains("log_expired_job_summary"));
        assert!(source.contains("job_type = %job_type"));
        assert!(source.contains("expired_jobs = summary.expired_jobs"));
        assert!(source.contains("retryable_jobs = summary.retryable_jobs"));
        assert!(source.contains("terminal_jobs = summary.terminal_jobs"));
        assert!(source.contains("recovered stale running jobs"));
    }

    #[test]
    fn stale_job_recovery_sql_updates_job_and_run_leases() {
        assert!(EXPIRE_STALE_RUNNING_JOBS_SQL.contains("update jobs"));
        assert!(EXPIRE_STALE_RUNNING_JOBS_SQL.contains("status = 'running'"));
        assert!(EXPIRE_STALE_RUNNING_JOBS_SQL.contains("locked_until <= now()"));
        assert!(EXPIRE_STALE_RUNNING_JOBS_SQL.contains("update job_runs"));
        assert!(EXPIRE_STALE_RUNNING_JOBS_SQL.contains("jr.status = 'running'"));
    }

    #[test]
    fn job_payload_dedupe_indexes_match_queue_queries() {
        let metadata_index = include_str!("../migrations/0015_metadata_refresh.sql");
        let probe_index = include_str!("../migrations/0023_media_probe_jobs.sql");
        let scan_index = include_str!("../migrations/0021_scan_job_scale_indexes.sql");
        let scan_service = include_str!("scan/service.rs");
        let scheduler_repository = include_str!("scheduler/repository.rs");
        let admin_repository = include_str!("admin/repository.rs");

        assert!(metadata_index.contains("idx_jobs_metadata_refresh_active_item"));
        assert!(metadata_index.contains("(payload->>'itemId')"));
        assert!(metadata_index.contains("job_type = 'metadata.refresh'"));
        assert!(metadata_index.contains("status in ('queued', 'running', 'failed')"));
        assert!(metadata_index.contains("attempts < max_attempts"));
        assert!(scan_service.contains("j.payload->>'itemId' = mi.public_id::text"));
        assert!(scheduler_repository.contains("j.payload->>'itemId' = mi.public_id::text"));
        assert!(admin_repository.contains("j.payload->>'itemId' = mi.public_id::text"));

        assert!(probe_index.contains("idx_jobs_media_probe_active_file"));
        assert!(probe_index.contains("(payload->>'mediaFileId')"));
        assert!(probe_index.contains("job_type = 'media.probe'"));
        assert!(probe_index.contains("status in ('queued', 'running', 'failed')"));
        assert!(scan_service.contains("j.payload->>'mediaFileId' = mf.id::text"));

        assert!(scan_index.contains("idx_jobs_library_scan_library_active"));
        assert!(scan_index.contains("(payload->>'libraryId')"));
        assert!(scan_index.contains("job_type = 'library.scan'"));
        assert!(scan_index.contains("status in ('queued', 'running', 'failed')"));
        assert!(
            scheduler_repository
                .contains("j.payload->>'libraryId' = eligible_libraries.library_public_id")
        );
    }
}
