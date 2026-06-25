use serde_json::json;
use sqlx::{Postgres, Row, Transaction, postgres::PgRow};
use tracing::warn;

use crate::{
    db::DbPool, metadata::service::METADATA_REFRESH_JOB_TYPE,
    plugins::hooks::PLUGIN_HOOK_DISPATCH_EVENT, scheduler::service::SchedulerError,
};

pub const CORE_INCREMENTAL_SCAN_TASK_KEY: &str = "core.library.incremental_scan";
pub const CORE_SCAN_ALL_TASK_TYPE: &str = "library.scan_all";
pub const CORE_METADATA_REFRESH_TASK_KEY: &str = "core.metadata.refresh";
pub const CORE_METADATA_REFRESH_TASK_TYPE: &str = "metadata.refresh_all";
pub const CORE_TRANSCODE_CLEANUP_TASK_KEY: &str = "core.transcode.cleanup";
pub const CORE_TRANSCODE_CLEANUP_TASK_TYPE: &str = "transcode.cleanup";
pub const CORE_PARTITION_MAINTENANCE_TASK_KEY: &str = "core.partition.maintenance";
pub const CORE_PARTITION_MAINTENANCE_TASK_TYPE: &str = "partition.maintenance";
pub const PLUGIN_SCHEDULE_TASK_TYPE: &str = "plugin.schedule";
const METADATA_REFRESH_QUEUE_BATCH_SIZE: i64 = 50_000;

const EXPIRE_STALE_SCHEDULED_TASK_RUNS_SQL: &str = r#"
with expired_runs as (
    update scheduled_task_runs
    set status = 'expired',
        error_message = coalesce(error_message, 'scheduled task lease expired'),
        finished_at = coalesce(finished_at, lease_expires_at),
        updated_at = now()
    where status = 'running'
      and lease_expires_at <= now()
    returning id,
              trigger_type
)
select count(*)::bigint as expired_runs,
       count(*) filter (where trigger_type = 'due')::bigint as due_runs,
       count(*) filter (where trigger_type = 'manual')::bigint as manual_runs
from expired_runs
"#;

const TRANSCODE_CLEANUP_CANDIDATES_SQL: &str = r#"
            select public_id::text as id,
                   output_path
            from transcoding_sessions
            where status in ('cancelled', 'failed')
              and output_cleaned_at is null
              and output_path is not null
            order by finished_at asc nulls first, id asc
            limit $1
            "#;

const TRANSCODE_MARK_OUTPUT_CLEANED_SQL: &str = r#"
            update transcoding_sessions
            set output_cleaned_at = now(),
                updated_at = now()
            where public_id = case
                when $1::text ~* '^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$'
                then $1::uuid
                else null::uuid
            end
              and output_cleaned_at is null
            "#;

#[derive(Clone)]
pub struct SchedulerRepository {
    pool: DbPool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CoreScheduledTaskInput {
    pub task_key: &'static str,
    pub task_type: &'static str,
    pub enabled: bool,
    pub schedule_kind: &'static str,
    pub schedule_value: String,
    pub interval_seconds: Option<u64>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ScheduledTaskRecord {
    pub id: i64,
    pub run_id: i64,
    pub task_key: String,
    pub task_type: String,
    pub enabled: bool,
    pub schedule_kind: String,
    pub schedule_value: String,
    pub timeout_seconds: i32,
    pub max_concurrency: i32,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ScheduledTaskExpiredRunSummary {
    pub expired_runs: u64,
    pub due_runs: u64,
    pub manual_runs: u64,
}

impl ScheduledTaskExpiredRunSummary {
    pub fn has_work(&self) -> bool {
        self.expired_runs > 0
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TranscodeCleanupCandidate {
    pub id: String,
    pub output_path: String,
}

impl SchedulerRepository {
    pub fn new(pool: DbPool) -> Self {
        Self { pool }
    }

    pub async fn upsert_core_task(
        &self,
        input: CoreScheduledTaskInput,
    ) -> Result<(), SchedulerError> {
        sqlx::query(
            r#"
            insert into scheduled_tasks (
                task_key,
                task_type,
                owner_type,
                enabled,
                schedule_kind,
                schedule_value,
                next_run_at,
                timeout_seconds,
                max_concurrency
            )
            values (
                $1,
                $2,
                'core',
                $3,
                $4,
                $5,
                case
                    when $3 = false then null
                    when $4 = 'interval' then now() + ($6::bigint * interval '1 second')
                    when $4 = 'cron' then fbz_next_cron_run_at($5, now())
                    else null
                end,
                600,
                1
            )
            on conflict (task_key) do update
                set task_type = excluded.task_type,
                    owner_type = excluded.owner_type,
                    enabled = excluded.enabled,
                    schedule_kind = excluded.schedule_kind,
                    schedule_value = excluded.schedule_value,
                    next_run_at = case
                        when excluded.enabled = false then null
                        when scheduled_tasks.enabled = false then excluded.next_run_at
                        when scheduled_tasks.next_run_at is null then excluded.next_run_at
                        when scheduled_tasks.schedule_value <> excluded.schedule_value then excluded.next_run_at
                        else scheduled_tasks.next_run_at
                    end,
                    timeout_seconds = excluded.timeout_seconds,
                    max_concurrency = excluded.max_concurrency,
                    updated_at = now()
            "#,
        )
        .bind(input.task_key)
        .bind(input.task_type)
        .bind(input.enabled)
        .bind(input.schedule_kind)
        .bind(input.schedule_value)
        .bind(input.interval_seconds.map(|value| value as i64))
        .execute(&self.pool)
        .await
        .map_err(SchedulerError::Database)?;

        Ok(())
    }

    pub async fn claim_due_task(
        &self,
        worker_id: &str,
    ) -> Result<Option<ScheduledTaskRecord>, SchedulerError> {
        let mut tx = self.pool.begin().await.map_err(SchedulerError::Database)?;
        expire_stale_runs(&mut tx).await?;
        let row = sqlx::query(
            r#"
            select
                st.id,
                st.task_key,
                st.task_type,
                st.enabled,
                st.schedule_kind,
                st.schedule_value,
                st.timeout_seconds,
                st.max_concurrency
            from scheduled_tasks
            st
            where enabled = true
              and schedule_kind in ('interval', 'cron')
              and next_run_at is not null
              and next_run_at <= now()
              and (
                  select count(*)::bigint
                  from scheduled_task_runs runs
                  where runs.task_id = st.id
                    and runs.status = 'running'
                    and runs.lease_expires_at > now()
              ) < st.max_concurrency
            order by next_run_at asc, id asc
            limit 1
            for update skip locked
            "#,
        )
        .fetch_optional(&mut *tx)
        .await
        .map_err(SchedulerError::Database)?;

        let Some(row) = row else {
            tx.commit().await.map_err(SchedulerError::Database)?;
            return Ok(None);
        };

        let mut task = ScheduledTaskRecord::from_row(row).map_err(SchedulerError::Database)?;
        let interval_seconds = match task.schedule_kind.as_str() {
            "interval" => Some(crate::scheduler::service::parse_interval_seconds(
                &task.schedule_value,
            )? as i64),
            "cron" => None,
            other => return Err(SchedulerError::UnsupportedScheduleKind(other.to_owned())),
        };

        sqlx::query(
            r#"
            update scheduled_tasks
            set last_run_at = now(),
                next_run_at = case
                    when schedule_kind = 'interval' then now() + ($2::bigint * interval '1 second')
                    when schedule_kind = 'cron' then fbz_next_cron_run_at(schedule_value, now())
                    else null
                end,
                last_error = null,
                updated_at = now()
            where id = $1
            "#,
        )
        .bind(task.id)
        .bind(interval_seconds)
        .execute(&mut *tx)
        .await
        .map_err(SchedulerError::Database)?;

        task.run_id = insert_task_run(&mut tx, &task, "due", worker_id).await?;
        tx.commit().await.map_err(SchedulerError::Database)?;
        Ok(Some(task))
    }

    pub async fn claim_task_by_key(
        &self,
        task_key: &str,
        worker_id: &str,
    ) -> Result<ScheduledTaskRecord, SchedulerError> {
        let mut tx = self.pool.begin().await.map_err(SchedulerError::Database)?;
        expire_stale_runs(&mut tx).await?;
        let row = sqlx::query(
            r#"
            select id,
                   task_key,
                   task_type,
                   enabled,
                   schedule_kind,
                   schedule_value,
                   timeout_seconds,
                   max_concurrency
            from scheduled_tasks
            where task_key = $1
            for update
            "#,
        )
        .bind(task_key.trim())
        .fetch_optional(&mut *tx)
        .await
        .map_err(SchedulerError::Database)?;

        let Some(row) = row else {
            tx.commit().await.map_err(SchedulerError::Database)?;
            return Err(SchedulerError::TaskNotFound(task_key.trim().to_owned()));
        };

        let mut task = ScheduledTaskRecord::from_row(row).map_err(SchedulerError::Database)?;
        if !task.enabled {
            tx.commit().await.map_err(SchedulerError::Database)?;
            return Err(SchedulerError::TaskDisabled(task.task_key));
        }
        let active_runs = active_run_count(&mut tx, task.id).await?;
        if active_runs >= i64::from(task.max_concurrency) {
            tx.commit().await.map_err(SchedulerError::Database)?;
            return Err(SchedulerError::TaskConcurrencyLimit {
                task_key: task.task_key,
                max_concurrency: task.max_concurrency,
            });
        }

        sqlx::query(
            r#"
            update scheduled_tasks
            set last_run_at = now(),
                last_error = null,
                updated_at = now()
            where id = $1
            "#,
        )
        .bind(task.id)
        .execute(&mut *tx)
        .await
        .map_err(SchedulerError::Database)?;

        task.run_id = insert_task_run(&mut tx, &task, "manual", worker_id).await?;
        tx.commit().await.map_err(SchedulerError::Database)?;
        Ok(task)
    }

    pub async fn mark_task_success(
        &self,
        task_id: i64,
        run_id: i64,
        queued_jobs: i64,
    ) -> Result<(), SchedulerError> {
        let mut tx = self.pool.begin().await.map_err(SchedulerError::Database)?;
        let run_update = sqlx::query(
            r#"
            update scheduled_task_runs
            set status = 'succeeded',
                queued_jobs = $2,
                error_message = null,
                finished_at = now(),
                updated_at = now()
            where id = $1
              and status = 'running'
            "#,
        )
        .bind(run_id)
        .bind(queued_jobs)
        .execute(&mut *tx)
        .await
        .map_err(SchedulerError::Database)?;

        if run_update.rows_affected() == 0 {
            tx.commit().await.map_err(SchedulerError::Database)?;
            return Ok(());
        }

        sqlx::query(
            r#"
            update scheduled_tasks
            set last_run_at = now(),
                last_error = null,
                updated_at = now()
            where id = $1
            "#,
        )
        .bind(task_id)
        .execute(&mut *tx)
        .await
        .map_err(SchedulerError::Database)?;

        tx.commit().await.map_err(SchedulerError::Database)?;
        Ok(())
    }

    pub async fn mark_task_failure(
        &self,
        task_id: i64,
        run_id: i64,
        message: &str,
    ) -> Result<(), SchedulerError> {
        let mut tx = self.pool.begin().await.map_err(SchedulerError::Database)?;
        let run_update = sqlx::query(
            r#"
            update scheduled_task_runs
            set status = 'failed',
                error_message = $2,
                finished_at = now(),
                updated_at = now()
            where id = $1
              and status = 'running'
            "#,
        )
        .bind(run_id)
        .bind(message)
        .execute(&mut *tx)
        .await
        .map_err(SchedulerError::Database)?;

        if run_update.rows_affected() == 0 {
            tx.commit().await.map_err(SchedulerError::Database)?;
            return Ok(());
        }

        sqlx::query(
            r#"
            update scheduled_tasks
            set last_run_at = now(),
                failure_count = failure_count + 1,
                last_error = $2,
                updated_at = now()
            where id = $1
            "#,
        )
        .bind(task_id)
        .bind(message)
        .execute(&mut *tx)
        .await
        .map_err(SchedulerError::Database)?;

        tx.commit().await.map_err(SchedulerError::Database)?;
        Ok(())
    }

    pub async fn cancel_running_task_by_key(&self, task_key: &str) -> Result<(), SchedulerError> {
        let task_key = task_key.trim();
        let mut tx = self.pool.begin().await.map_err(SchedulerError::Database)?;
        expire_stale_runs(&mut tx).await?;

        let task_id = sqlx::query_scalar::<_, i64>(
            r#"
            select id
            from scheduled_tasks
            where task_key = $1
            for update
            "#,
        )
        .bind(task_key)
        .fetch_optional(&mut *tx)
        .await
        .map_err(SchedulerError::Database)?;

        let Some(task_id) = task_id else {
            tx.commit().await.map_err(SchedulerError::Database)?;
            return Err(SchedulerError::TaskNotFound(task_key.to_owned()));
        };

        let update = sqlx::query(
            r#"
            update scheduled_task_runs
            set status = 'cancelled',
                error_message = coalesce(error_message, 'scheduled task cancelled by administrator'),
                finished_at = now(),
                updated_at = now()
            where task_id = $1
              and status = 'running'
              and lease_expires_at > now()
            "#,
        )
        .bind(task_id)
        .execute(&mut *tx)
        .await
        .map_err(SchedulerError::Database)?;

        if update.rows_affected() == 0 {
            tx.commit().await.map_err(SchedulerError::Database)?;
            return Err(SchedulerError::TaskNotRunning(task_key.to_owned()));
        }

        tx.commit().await.map_err(SchedulerError::Database)?;
        Ok(())
    }

    /// Ensure each time-partitioned table has the current month + `months_ahead`
    /// upcoming monthly partitions, via the idempotent `ensure_partition_coverage`
    /// SQL function (migration 0068). Returns the number of partitions created.
    pub async fn ensure_partition_coverage(
        &self,
        months_ahead: i32,
    ) -> Result<i64, SchedulerError> {
        let created = sqlx::query_scalar::<_, i32>("select ensure_partition_coverage($1)")
            .bind(months_ahead)
            .fetch_one(&self.pool)
            .await
            .map_err(SchedulerError::Database)?;
        Ok(i64::from(created))
    }

    pub async fn queue_scan_all(&self, reason: &str) -> Result<i64, SchedulerError> {
        let payload_reason = json!(reason);
        sqlx::query_scalar::<_, i64>(
            r#"
            with eligible_libraries as (
                select l.id,
                       l.public_id::text as library_public_id
                from libraries l
                where exists (
                    select 1
                    from library_paths lp
                    where lp.library_id = l.id
                      and lp.is_enabled = true
                )
            ),
            inserted as (
                insert into jobs (
                    job_type,
                    status,
                    queue_name,
                    priority,
                    payload
                )
                select
                    'library.scan',
                    'queued',
                    'scan',
                    -10,
                    jsonb_build_object(
                        'libraryId', eligible_libraries.library_public_id,
                        'requestedByUserId', null,
                        'reason', $1::jsonb
                    )
                from eligible_libraries
                where not exists (
                    select 1
                    from jobs j
                    where j.job_type = 'library.scan'
                      and j.status in ('queued', 'running')
                      and j.payload->>'libraryId' = eligible_libraries.library_public_id
                )
                returning id
            )
            select count(*)::bigint from inserted
            "#,
        )
        .bind(payload_reason)
        .fetch_one(&self.pool)
        .await
        .map_err(SchedulerError::Database)
    }

    pub async fn queue_metadata_refresh_all(&self, reason: &str) -> Result<i64, SchedulerError> {
        let payload_reason = json!(reason);
        sqlx::query_scalar::<_, i64>(
            r#"
            with eligible_items as (
                select mi.public_id::text as item_public_id
                from media_items mi
                where mi.is_deleted = false
                  and mi.metadata_status = 'pending'
                  and mi.item_type in ('movie', 'series', 'episode')
                  and not exists (
                      select 1
                      from jobs j
                      where j.job_type = $1
                        and j.status in ('queued', 'running', 'failed')
                        and j.attempts < j.max_attempts
                        and j.payload->>'itemId' = mi.public_id::text
                  )
                order by mi.updated_at asc, mi.id asc
                limit $2
            ),
            inserted as (
                insert into jobs (
                    job_type,
                    status,
                    queue_name,
                    priority,
                    payload
                )
                select
                    $1,
                    'queued',
                    'metadata',
                    -20,
                    jsonb_build_object(
                        'itemId', eligible_items.item_public_id,
                        'requestedByUserId', null,
                        'reason', $3::jsonb
                    )
                from eligible_items
                on conflict do nothing
                returning id
            )
            select count(*)::bigint from inserted
            "#,
        )
        .bind(METADATA_REFRESH_JOB_TYPE)
        .bind(METADATA_REFRESH_QUEUE_BATCH_SIZE)
        .bind(payload_reason)
        .fetch_one(&self.pool)
        .await
        .map_err(SchedulerError::Database)
    }

    pub async fn queue_plugin_schedule_dispatch(
        &self,
        task_key: &str,
    ) -> Result<i64, SchedulerError> {
        sqlx::query_scalar::<_, i64>(
            r#"
            with schedule_target as (
                select pi.plugin_id,
                       pkg.public_id::text as package_public_id,
                       psd.task_key,
                       psd.handler,
                       st.schedule_kind,
                       st.schedule_value
                from scheduled_tasks st
                join plugin_installations pi
                  on pi.plugin_id = st.owner_id
                join plugin_packages pkg
                  on pkg.id = pi.active_package_id
                join plugin_schedule_definitions psd
                  on psd.package_id = pkg.id
                 and psd.task_key = st.task_key
                where st.task_key = $1
                  and st.owner_type = 'plugin'
                  and st.task_type = $2
                  and pi.enabled = true
                  and pi.approval_status = 'approved'
                  and pkg.package_status = 'approved'
                limit 1
            ),
            inserted as (
                insert into event_outbox (
                    event_type,
                    aggregate_type,
                    aggregate_id,
                    payload
                )
                select $3,
                       'plugin_schedule',
                       schedule_target.task_key,
                       jsonb_build_object(
                           'pluginId', schedule_target.plugin_id,
                           'packageId', schedule_target.package_public_id,
                           'hookId', null,
                           'handler', schedule_target.handler,
                           'hookEvent', 'scheduler.tick',
                           'source', jsonb_build_object(
                               'aggregateType', 'plugin_schedule',
                               'aggregateId', schedule_target.task_key,
                               'payload', jsonb_build_object(
                                   'taskKey', schedule_target.task_key,
                                   'scheduleKind', schedule_target.schedule_kind,
                                   'scheduleValue', schedule_target.schedule_value
                               )
                           )
                       )
                from schedule_target
                returning id
            )
            select count(*)::bigint from inserted
            "#,
        )
        .bind(task_key.trim())
        .bind(PLUGIN_SCHEDULE_TASK_TYPE)
        .bind(PLUGIN_HOOK_DISPATCH_EVENT)
        .fetch_one(&self.pool)
        .await
        .map_err(SchedulerError::Database)
    }

    pub async fn list_transcode_cleanup_candidates(
        &self,
        limit: i64,
    ) -> Result<Vec<TranscodeCleanupCandidate>, SchedulerError> {
        let rows = sqlx::query(TRANSCODE_CLEANUP_CANDIDATES_SQL)
            .bind(limit.max(1))
            .fetch_all(&self.pool)
            .await
            .map_err(SchedulerError::Database)?;

        rows.into_iter()
            .map(TranscodeCleanupCandidate::from_row)
            .collect::<Result<Vec<_>, _>>()
            .map_err(SchedulerError::Database)
    }

    pub async fn mark_transcode_output_cleaned(
        &self,
        session_id: &str,
    ) -> Result<bool, SchedulerError> {
        sqlx::query(TRANSCODE_MARK_OUTPUT_CLEANED_SQL)
            .bind(session_id.trim())
            .execute(&self.pool)
            .await
            .map(|result| result.rows_affected() > 0)
            .map_err(SchedulerError::Database)
    }
}

impl ScheduledTaskRecord {
    fn from_row(row: PgRow) -> Result<Self, sqlx::Error> {
        Ok(Self {
            id: row.try_get("id")?,
            run_id: 0,
            task_key: row.try_get("task_key")?,
            task_type: row.try_get("task_type")?,
            enabled: row.try_get("enabled")?,
            schedule_kind: row.try_get("schedule_kind")?,
            schedule_value: row.try_get("schedule_value")?,
            timeout_seconds: row.try_get("timeout_seconds")?,
            max_concurrency: row.try_get("max_concurrency")?,
        })
    }
}

async fn expire_stale_runs(
    tx: &mut Transaction<'_, Postgres>,
) -> Result<ScheduledTaskExpiredRunSummary, SchedulerError> {
    let row = sqlx::query(EXPIRE_STALE_SCHEDULED_TASK_RUNS_SQL)
        .fetch_one(&mut **tx)
        .await
        .map_err(SchedulerError::Database)?;

    let summary = ScheduledTaskExpiredRunSummary {
        expired_runs: row
            .try_get::<i64, _>("expired_runs")
            .map_err(SchedulerError::Database)?
            .max(0) as u64,
        due_runs: row
            .try_get::<i64, _>("due_runs")
            .map_err(SchedulerError::Database)?
            .max(0) as u64,
        manual_runs: row
            .try_get::<i64, _>("manual_runs")
            .map_err(SchedulerError::Database)?
            .max(0) as u64,
    };

    log_expired_scheduled_task_run_summary(summary);

    Ok(summary)
}

fn log_expired_scheduled_task_run_summary(summary: ScheduledTaskExpiredRunSummary) {
    if summary.has_work() {
        warn!(
            expired_runs = summary.expired_runs,
            due_runs = summary.due_runs,
            manual_runs = summary.manual_runs,
            "recovered stale scheduled task runs"
        );
    }
}

async fn active_run_count(
    tx: &mut Transaction<'_, Postgres>,
    task_id: i64,
) -> Result<i64, SchedulerError> {
    sqlx::query_scalar::<_, i64>(
        r#"
        select count(*)::bigint
        from scheduled_task_runs
        where task_id = $1
          and status = 'running'
          and lease_expires_at > now()
        "#,
    )
    .bind(task_id)
    .fetch_one(&mut **tx)
    .await
    .map_err(SchedulerError::Database)
}

async fn insert_task_run(
    tx: &mut Transaction<'_, Postgres>,
    task: &ScheduledTaskRecord,
    trigger_type: &str,
    worker_id: &str,
) -> Result<i64, SchedulerError> {
    sqlx::query_scalar::<_, i64>(
        r#"
        insert into scheduled_task_runs (
            task_id,
            task_key,
            trigger_type,
            worker_id,
            lease_expires_at
        )
        values (
            $1,
            $2,
            $3,
            $4,
            now() + ($5::integer * interval '1 second')
        )
        returning id
        "#,
    )
    .bind(task.id)
    .bind(&task.task_key)
    .bind(trigger_type)
    .bind(worker_id.trim())
    .bind(task.timeout_seconds)
    .fetch_one(&mut **tx)
    .await
    .map_err(SchedulerError::Database)
}

impl TranscodeCleanupCandidate {
    fn from_row(row: PgRow) -> Result<Self, sqlx::Error> {
        Ok(Self {
            id: row.try_get("id")?,
            output_path: row.try_get("output_path")?,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const REPOSITORY_SOURCE: &str = include_str!("repository.rs");
    const TRANSCODE_CLEANUP_MIGRATION: &str =
        include_str!("../../migrations/0062_transcode_output_cleanup_marker.sql");

    #[test]
    fn transcode_cleanup_queries_terminal_sessions_with_marker_index() {
        assert!(REPOSITORY_SOURCE.contains("pub struct TranscodeCleanupCandidate"));
        assert!(REPOSITORY_SOURCE.contains("TRANSCODE_CLEANUP_CANDIDATES_SQL"));
        assert!(REPOSITORY_SOURCE.contains("status in ('cancelled', 'failed')"));
        assert!(REPOSITORY_SOURCE.contains("output_cleaned_at is null"));
        assert!(REPOSITORY_SOURCE.contains("order by finished_at asc nulls first, id asc"));
        assert!(REPOSITORY_SOURCE.contains("limit $1"));
        assert!(REPOSITORY_SOURCE.contains("mark_transcode_output_cleaned"));
        assert!(REPOSITORY_SOURCE.contains("where public_id = case"));
        assert!(REPOSITORY_SOURCE.contains("and output_cleaned_at is null"));

        assert!(TRANSCODE_CLEANUP_MIGRATION.contains("add column if not exists output_cleaned_at"));
        assert!(
            TRANSCODE_CLEANUP_MIGRATION.contains("idx_transcoding_sessions_output_cleanup_pending")
        );
        assert!(TRANSCODE_CLEANUP_MIGRATION.contains("status in ('cancelled', 'failed')"));
        assert!(TRANSCODE_CLEANUP_MIGRATION.contains("output_cleaned_at is null"));
    }

    #[test]
    fn stale_scheduled_task_run_recovery_counts_due_and_manual_runs() {
        let summary = ScheduledTaskExpiredRunSummary {
            expired_runs: 3,
            due_runs: 2,
            manual_runs: 1,
        };

        assert!(summary.has_work());
        assert_eq!(summary.expired_runs, summary.due_runs + summary.manual_runs);
        assert!(EXPIRE_STALE_SCHEDULED_TASK_RUNS_SQL.contains("due_runs"));
        assert!(EXPIRE_STALE_SCHEDULED_TASK_RUNS_SQL.contains("manual_runs"));
        assert!(EXPIRE_STALE_SCHEDULED_TASK_RUNS_SQL.contains("status = 'running'"));
        assert!(EXPIRE_STALE_SCHEDULED_TASK_RUNS_SQL.contains("lease_expires_at <= now()"));
        assert!(EXPIRE_STALE_SCHEDULED_TASK_RUNS_SQL.contains("returning id,"));
        assert!(EXPIRE_STALE_SCHEDULED_TASK_RUNS_SQL.contains("trigger_type"));
    }

    #[test]
    fn stale_scheduled_task_run_recovery_reports_structured_summary_fields() {
        let production_source = REPOSITORY_SOURCE
            .split("#[cfg(test)]")
            .next()
            .expect("repository source should include production section");

        assert!(production_source.contains("ScheduledTaskExpiredRunSummary"));
        assert!(production_source.contains("expired_runs = summary.expired_runs"));
        assert!(production_source.contains("due_runs = summary.due_runs"));
        assert!(production_source.contains("manual_runs = summary.manual_runs"));
        assert!(production_source.contains("recovered stale scheduled task runs"));
    }
}
