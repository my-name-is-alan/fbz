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

/// Post-failure attempt counters returned by [`mark_job_failed`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct FailedJobOutcome {
    pub attempts: i32,
    pub max_attempts: i32,
}

impl FailedJobOutcome {
    /// A failed job is retryable while it still has attempts left; the generic
    /// worker claim queries re-pick `failed` jobs with `attempts < max_attempts`.
    pub fn retryable(&self) -> bool {
        self.attempts < self.max_attempts
    }
}

const MARK_JOB_FAILED_SQL: &str = r#"
update jobs
set status = 'failed',
    locked_by = null,
    locked_until = null,
    last_error = $2,
    updated_at = now()
where id = $1
returning attempts, max_attempts
"#;

/// Mark a job failed after its handler returned an error, release the lease, and
/// emit a structured warn log that distinguishes a scheduled retry from a
/// terminal failure — mirroring the lease-recovery, plugin-dispatch and
/// notification-delivery retry logs so handler failures (e.g. a NAS that is
/// temporarily unreachable mid-scan) are observable rather than silent.
///
/// Runs inside the caller's transaction; callers stay responsible for marking
/// the matching `job_runs` row failed (run-scoped error context lives there).
pub async fn mark_job_failed(
    tx: &mut Transaction<'_, Postgres>,
    job_type: &str,
    job_public_id: &str,
    job_id: i64,
    message: &str,
) -> Result<FailedJobOutcome, sqlx::Error> {
    let row = sqlx::query(MARK_JOB_FAILED_SQL)
        .bind(job_id)
        .bind(message)
        .fetch_one(&mut **tx)
        .await?;
    let outcome = FailedJobOutcome {
        attempts: row.try_get::<i32, _>("attempts")?,
        max_attempts: row.try_get::<i32, _>("max_attempts")?,
    };

    log_job_failure(job_type, job_public_id, outcome, message);

    Ok(outcome)
}

fn log_job_failure(job_type: &str, job_public_id: &str, outcome: FailedJobOutcome, message: &str) {
    if outcome.retryable() {
        warn!(
            job_type = %job_type,
            job = %job_public_id,
            attempt = outcome.attempts,
            max_attempts = outcome.max_attempts,
            retryable = true,
            error = %message,
            "job failed; scheduled retry"
        );
    } else {
        warn!(
            job_type = %job_type,
            job = %job_public_id,
            attempt = outcome.attempts,
            max_attempts = outcome.max_attempts,
            retryable = false,
            error = %message,
            "job failed; max attempts reached"
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
    fn failed_job_outcome_is_retryable_until_max_attempts() {
        assert!(
            FailedJobOutcome {
                attempts: 1,
                max_attempts: 3,
            }
            .retryable()
        );
        assert!(
            FailedJobOutcome {
                attempts: 2,
                max_attempts: 3,
            }
            .retryable()
        );
        assert!(
            !FailedJobOutcome {
                attempts: 3,
                max_attempts: 3,
            }
            .retryable()
        );
        assert!(
            !FailedJobOutcome {
                attempts: 4,
                max_attempts: 3,
            }
            .retryable()
        );
    }

    #[test]
    fn mark_job_failed_sql_releases_lease_and_returns_attempt_counters() {
        assert!(MARK_JOB_FAILED_SQL.contains("update jobs"));
        assert!(MARK_JOB_FAILED_SQL.contains("status = 'failed'"));
        assert!(MARK_JOB_FAILED_SQL.contains("locked_by = null"));
        assert!(MARK_JOB_FAILED_SQL.contains("locked_until = null"));
        assert!(MARK_JOB_FAILED_SQL.contains("last_error = $2"));
        assert!(MARK_JOB_FAILED_SQL.contains("returning attempts, max_attempts"));
    }

    #[test]
    fn job_failure_logs_distinguish_retry_from_terminal() {
        let source = include_str!("jobs.rs");

        assert!(source.contains("log_job_failure"));
        assert!(source.contains("job failed; scheduled retry"));
        assert!(source.contains("job failed; max attempts reached"));
        assert!(source.contains("retryable = true"));
        assert!(source.contains("retryable = false"));
        assert!(source.contains("attempt = outcome.attempts"));
        assert!(source.contains("max_attempts = outcome.max_attempts"));
        assert!(source.contains("error = %message"));
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

    #[test]
    fn job_events_partition_migration_partitions_by_created_at() {
        let migration = include_str!("../migrations/0064_partition_job_events.sql");

        // Partitioned by the time key, with the partition key in the PK.
        assert!(migration.contains("partition by range (created_at)"));
        assert!(migration.contains("primary key (id, created_at)"));
        // The id sequence is preserved across the table swap.
        assert!(migration.contains("alter sequence job_events_id_seq owned by none"));
        assert!(migration.contains("alter sequence job_events_id_seq owned by job_events.id"));
        // Monthly partitions plus a default catch-all.
        assert!(migration.contains("partition of job_events default"));
        assert!(migration.contains("for values from ('2026-06-01') to ('2026-07-01')"));
        // Keyset indexes recreated under their original names (cascade to partitions).
        assert!(migration.contains("idx_job_events_job_created_keyset"));
        assert!(migration.contains("idx_job_events_job_level_created_keyset"));
        // Existing rows are backfilled, then the legacy table is dropped.
        assert!(migration.contains("from job_events_legacy"));
        assert!(migration.contains("drop table job_events_legacy"));
    }

    #[test]
    fn plugin_host_api_calls_partition_migration_relaxes_public_id_and_partitions_by_finished_at() {
        let migration = include_str!("../migrations/0065_partition_plugin_host_api_calls.sql");

        assert!(migration.contains("partition by range (finished_at)"));
        assert!(migration.contains("primary key (id, finished_at)"));
        // public_id UNIQUE is set aside (renamed to legacy) and replaced by a
        // plain index — a partitioned unique must include the partition key.
        assert!(migration.contains("phac_legacy_public_id_key"));
        assert!(migration.contains("idx_plugin_host_api_calls_public_id"));
        // The budget index that bounds Host API call-limit checks is preserved.
        assert!(migration.contains("idx_plugin_host_api_calls_execution_plugin_budget"));
        // Sequence preserved across the swap; rows backfilled; legacy dropped.
        assert!(migration.contains("alter sequence plugin_host_api_calls_id_seq owned by none"));
        assert!(migration.contains("from plugin_host_api_calls_legacy"));
        assert!(migration.contains("drop table plugin_host_api_calls_legacy"));
    }

    #[test]
    fn scheduled_task_runs_partition_migration_partitions_by_started_at() {
        let migration = include_str!("../migrations/0066_partition_scheduled_task_runs.sql");

        assert!(migration.contains("partition by range (started_at)"));
        assert!(migration.contains("primary key (id, started_at)"));
        // public_id UNIQUE set aside and replaced by a plain index.
        assert!(migration.contains("str_legacy_public_id_key"));
        assert!(migration.contains("idx_scheduled_task_runs_public_id"));
        // The active-run partial index (lease recovery / concurrency) is preserved.
        assert!(migration.contains("idx_scheduled_task_runs_active"));
        assert!(migration.contains("where status = 'running'"));
        assert!(migration.contains("alter sequence scheduled_task_runs_id_seq owned by none"));
        assert!(migration.contains("from scheduled_task_runs_legacy"));
        assert!(migration.contains("drop table scheduled_task_runs_legacy"));
    }

    #[test]
    fn job_runs_partition_migration_drops_inbound_fk_and_partitions_by_started_at() {
        let migration = include_str!("../migrations/0067_partition_job_runs.sql");

        // The inbound FK from job_events is dropped first (safe: the SET NULL path
        // is never exercised — job_runs only die via the jobs cascade that also
        // removes the referencing job_events).
        assert!(
            migration.contains("alter table job_events drop constraint job_events_job_run_id_fkey")
        );
        assert!(migration.contains("partition by range (started_at)"));
        assert!(migration.contains("primary key (id, started_at)"));
        assert!(migration.contains("idx_job_runs_job_status_started_keyset"));
        assert!(migration.contains("alter sequence job_runs_id_seq owned by none"));
        assert!(migration.contains("from job_runs_legacy"));
        assert!(migration.contains("drop table job_runs_legacy"));
    }

    #[test]
    fn partition_coverage_function_covers_all_partitioned_tables_idempotently() {
        let migration = include_str!("../migrations/0068_partition_coverage_function.sql");

        assert!(migration.contains("create or replace function ensure_partition_coverage"));
        // Covers every partitioned table (0064-0067).
        assert!(migration.contains("'job_events'"));
        assert!(migration.contains("'plugin_host_api_calls'"));
        assert!(migration.contains("'scheduled_task_runs'"));
        assert!(migration.contains("'job_runs'"));
        // Idempotent: only creates partitions that do not already exist.
        assert!(
            migration.contains("if not exists (select 1 from pg_class where relname = part_name)")
        );
        assert!(migration.contains("create table %I partition of %I"));
        // Applies initial forward coverage on migration.
        assert!(migration.contains("select ensure_partition_coverage(18)"));
    }

    #[test]
    fn playback_sessions_partition_migration_replicates_set_null_via_trigger() {
        let migration = include_str!("../migrations/0069_partition_playback_sessions.sql");

        // The inbound SET NULL is replicated by a BEFORE DELETE trigger before the
        // FK is dropped (its delete path is exercised, unlike job_runs).
        assert!(
            migration.contains("create or replace function playback_sessions_null_transcode_refs")
        );
        assert!(migration.contains("before delete on playback_sessions"));
        assert!(migration.contains(
            "alter table transcoding_sessions drop constraint transcoding_sessions_playback_session_id_fkey"
        ));
        assert!(migration.contains("partition by range (started_at)"));
        assert!(migration.contains("primary key (id, started_at)"));
        assert!(migration.contains("idx_playback_sessions_public_id"));
        // Added to the rolling coverage function.
        assert!(migration.contains("'playback_sessions'"));
    }

    #[test]
    fn library_scan_has_dedicated_claim_index_matching_metadata_and_probe() {
        let scan_claim = include_str!("../migrations/0063_library_scan_claim_index.sql");
        let metadata_index = include_str!("../migrations/0015_metadata_refresh.sql");
        let probe_index = include_str!("../migrations/0023_media_probe_jobs.sql");

        // Scan claim index mirrors the metadata/probe dedicated partial claim indexes:
        // same (status, run_at, priority desc, id) ordering and queued/failed filter,
        // scoped to the library.scan job_type. This is the shape the scan claim query
        // (status in ('queued','failed') ... order by priority desc, run_at asc, id asc)
        // needs, instead of falling back to the cross-job-type generic index.
        assert!(scan_claim.contains("idx_jobs_library_scan_claim"));
        assert!(scan_claim.contains("on jobs (status, run_at, priority desc, id)"));
        assert!(scan_claim.contains("where job_type = 'library.scan'"));
        assert!(scan_claim.contains("status in ('queued', 'failed')"));

        for claim_index in [metadata_index, probe_index] {
            assert!(claim_index.contains("on jobs (status, run_at, priority desc, id)"));
            assert!(claim_index.contains("status in ('queued', 'failed')"));
        }

        // The scan claim query this index serves uses the matching status filter.
        let scan_service = include_str!("scan/service.rs");
        assert!(scan_service.contains("status in ('queued', 'failed')"));
        assert!(scan_service.contains("order by priority desc, run_at asc, jobs.id asc"));
    }
}
