use sqlx::{Postgres, Transaction};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ExpiredJobMessages<'a> {
    pub retry: &'a str,
    pub final_failure: &'a str,
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
    returning id, last_error
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
select count(*)::bigint
from expired_jobs
"#;

pub async fn expire_stale_running_jobs(
    tx: &mut Transaction<'_, Postgres>,
    job_type: &str,
    messages: ExpiredJobMessages<'_>,
) -> Result<u64, sqlx::Error> {
    let count = sqlx::query_scalar::<_, i64>(EXPIRE_STALE_RUNNING_JOBS_SQL)
        .bind(job_type.trim())
        .bind(messages.retry)
        .bind(messages.final_failure)
        .fetch_one(&mut **tx)
        .await?;

    Ok(count.max(0) as u64)
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
