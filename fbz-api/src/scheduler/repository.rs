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
with stale_run_candidates as (
    select id,
           started_at
    from scheduled_task_runs
    where status = 'running'
      and lease_expires_at <= now()
    order by lease_expires_at asc, id asc
    limit 1000
    for update skip locked
),
expired_runs as (
    update scheduled_task_runs as runs
    set status = 'expired',
        error_message = coalesce(error_message, 'scheduled task lease expired'),
        finished_at = coalesce(finished_at, lease_expires_at),
        updated_at = now()
    from stale_run_candidates candidates
    where runs.id = candidates.id
      and runs.started_at = candidates.started_at
    returning runs.id,
              runs.trigger_type
)
select count(*)::bigint as expired_runs,
       count(*) filter (where trigger_type = 'due')::bigint as due_runs,
       count(*) filter (where trigger_type = 'manual')::bigint as manual_runs
from expired_runs
"#;

const CLAIM_DUE_SCHEDULED_TASK_SQL: &str = r#"
            select
                st.id,
                st.task_key,
                st.task_type,
                st.enabled,
                st.schedule_kind,
                st.schedule_value,
                st.timeout_seconds,
                st.max_concurrency
            from scheduled_tasks st
            where st.enabled = true
              and st.schedule_kind in ('interval', 'cron')
              and st.next_run_at is not null
              and st.next_run_at <= now()
              and (
                  select count(*)::bigint
                  from (
                      select 1
                      from scheduled_task_runs runs
                      where runs.task_id = st.id
                        and runs.status = 'running'
                        and runs.lease_expires_at > now()
                      order by runs.lease_expires_at asc, runs.id asc
                      limit st.max_concurrency
                  ) active_run_capacity_probe
              ) < st.max_concurrency
            order by st.next_run_at asc, st.id asc
            limit 1
            for update skip locked
            "#;

const ACTIVE_SCHEDULED_TASK_RUN_CAPACITY_PROBE_SQL: &str = r#"
        select count(*)::bigint
        from (
            select 1
            from scheduled_task_runs
            where task_id = $1
              and status = 'running'
              and lease_expires_at > now()
            order by lease_expires_at asc, id asc
            limit $2
        ) active_run_capacity_probe
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
                when $1::text ~* '^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$'
                then $1::uuid
                else null::uuid
            end
              and output_cleaned_at is null
            "#;

const QUEUE_STATS_ROLLUP_REFRESH_MONTH_SQL: &str = r#"
with target_month as (
    select date_trunc('month', $1::date)::date as bucket_date,
           (date_trunc('month', $1::date)::date + interval '1 month')::timestamptz as next_bucket_at
),
cleared as (
    delete from queue_stats_rollup rollup
    using target_month tm
    where rollup.bucket_date = tm.bucket_date
      and rollup.table_name in (
          'job_events',
          'plugin_host_api_calls',
          'scheduled_task_runs',
          'job_runs',
          'playback_sessions'
      )
    returning 1
),
source_counts as (
    select tm.bucket_date,
           'job_events'::text as table_name,
           event_level::text as status,
           count(*)::bigint as row_count,
           ('job_events_' || to_char(tm.bucket_date, 'YYYY"m"MM'))::text as source_partition
    from target_month tm
    join job_events
      on created_at >= tm.bucket_date::timestamptz
     and created_at < tm.next_bucket_at
    group by tm.bucket_date, event_level

    union all

    select tm.bucket_date,
           'plugin_host_api_calls'::text as table_name,
           status_code::text as status,
           count(*)::bigint as row_count,
           ('plugin_host_api_calls_' || to_char(tm.bucket_date, 'YYYY"m"MM'))::text as source_partition
    from target_month tm
    join plugin_host_api_calls
      on finished_at >= tm.bucket_date::timestamptz
     and finished_at < tm.next_bucket_at
    group by tm.bucket_date, status_code

    union all

    select tm.bucket_date,
           'scheduled_task_runs'::text as table_name,
           status::text as status,
           count(*)::bigint as row_count,
           ('scheduled_task_runs_' || to_char(tm.bucket_date, 'YYYY"m"MM'))::text as source_partition
    from target_month tm
    join scheduled_task_runs
      on started_at >= tm.bucket_date::timestamptz
     and started_at < tm.next_bucket_at
    group by tm.bucket_date, status

    union all

    select tm.bucket_date,
           'job_runs'::text as table_name,
           status::text as status,
           count(*)::bigint as row_count,
           ('job_runs_' || to_char(tm.bucket_date, 'YYYY"m"MM'))::text as source_partition
    from target_month tm
    join job_runs
      on started_at >= tm.bucket_date::timestamptz
     and started_at < tm.next_bucket_at
    group by tm.bucket_date, status

    union all

    select tm.bucket_date,
           'playback_sessions'::text as table_name,
           case when stopped_at is null then 'active' else 'stopped' end as status,
           count(*)::bigint as row_count,
           ('playback_sessions_' || to_char(tm.bucket_date, 'YYYY"m"MM'))::text as source_partition
    from target_month tm
    join playback_sessions
      on started_at >= tm.bucket_date::timestamptz
     and started_at < tm.next_bucket_at
    group by tm.bucket_date, case when stopped_at is null then 'active' else 'stopped' end
),
upserted as (
    insert into queue_stats_rollup (
        bucket_date,
        table_name,
        status,
        row_count,
        finalized_at,
        source_partition
    )
    select bucket_date,
           table_name,
           status,
           row_count,
           now(),
           source_partition
    from source_counts
    on conflict (bucket_date, table_name, status) do update
        set row_count = excluded.row_count,
            finalized_at = excluded.finalized_at,
            source_partition = excluded.source_partition
    returning 1
)
select count(*)::bigint from upserted
"#;

const QUEUE_STATS_ROLLUP_LIST_SQL: &str = r#"
select bucket_date::text as bucket_date,
       table_name,
       status,
       row_count,
       finalized_at::text as finalized_at,
       source_partition
from queue_stats_rollup
where ($1::text is null or table_name = $1)
  and bucket_date >= $2::date
  and bucket_date < $3::date
order by bucket_date desc, table_name asc, status asc
limit $4
"#;

const PARTITION_ARCHIVE_CANDIDATES_SQL: &str = r#"
with partition_catalog as (
    select parent.relname as table_name,
           child.relname as partition_name,
           regexp_match(child.relname, '_(\d{4})m(\d{2})$') as name_parts
    from pg_inherits
    join pg_class parent
      on parent.oid = pg_inherits.inhparent
    join pg_class child
      on child.oid = pg_inherits.inhrelid
    join pg_namespace namespace
      on namespace.oid = parent.relnamespace
    where namespace.nspname = 'public'
      and parent.relname in (
          'job_events',
          'plugin_host_api_calls',
          'scheduled_task_runs',
          'job_runs',
          'playback_sessions'
      )
      and child.relname !~ '_default$'
),
monthly_partitions as (
    select table_name,
           partition_name,
           make_date((name_parts)[1]::int, (name_parts)[2]::int, 1) as bucket_date
    from partition_catalog
    where name_parts is not null
),
archive_candidates as (
    select partitions.table_name,
           partitions.partition_name,
           partitions.bucket_date,
           (partitions.bucket_date + interval '1 month')::date as partition_end,
           (
               select count(*)::bigint
               from queue_stats_rollup rollup
               where rollup.table_name = partitions.table_name
                 and rollup.bucket_date = partitions.bucket_date
                 and rollup.source_partition = partitions.partition_name
           ) as rollup_statuses,
           (
               select coalesce(sum(rollup.row_count), 0)::bigint
               from queue_stats_rollup rollup
               where rollup.table_name = partitions.table_name
                 and rollup.bucket_date = partitions.bucket_date
                 and rollup.source_partition = partitions.partition_name
           ) as rollup_rows
    from monthly_partitions partitions
    where partitions.bucket_date < (
              date_trunc('month', $3::date)::date - make_interval(months => $1)
          )
      and exists (
          select 1
          from queue_stats_rollup rollup
          where rollup.table_name = partitions.table_name
            and rollup.bucket_date = partitions.bucket_date
            and rollup.source_partition = partitions.partition_name
      )
)
select table_name,
       partition_name,
       bucket_date::text as bucket_date,
       partition_end::text as partition_end,
       rollup_statuses,
       rollup_rows
from archive_candidates
order by bucket_date asc, table_name asc, partition_name asc
limit $2
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

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PartitionArchiveCandidate {
    pub table_name: String,
    pub partition_name: String,
    pub bucket_date: String,
    pub partition_end: String,
    pub rollup_statuses: i64,
    pub rollup_rows: i64,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct QueueStatsRollupRecord {
    pub bucket_date: String,
    pub table_name: String,
    pub status: String,
    pub row_count: i64,
    pub finalized_at: String,
    pub source_partition: Option<String>,
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
        let row = sqlx::query(CLAIM_DUE_SCHEDULED_TASK_SQL)
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
        let active_runs = active_run_count(&mut tx, task.id, task.max_concurrency).await?;
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

    /// Refresh the monthly materialized count bucket for the already-partitioned
    /// high-growth history tables. This is intentionally non-destructive: it
    /// rewrites only `queue_stats_rollup`, leaving partition detach/drop to the
    /// future archive task.
    pub async fn refresh_queue_stats_rollup_for_month(
        &self,
        bucket_date: &str,
    ) -> Result<i64, SchedulerError> {
        sqlx::query_scalar::<_, i64>(QUEUE_STATS_ROLLUP_REFRESH_MONTH_SQL)
            .bind(bucket_date.trim())
            .fetch_one(&self.pool)
            .await
            .map_err(SchedulerError::Database)
    }

    pub async fn list_queue_stats_rollup(
        &self,
        table_name: Option<&str>,
        from_bucket_date: &str,
        to_bucket_date: &str,
        limit: i64,
    ) -> Result<Vec<QueueStatsRollupRecord>, SchedulerError> {
        let rows = sqlx::query(QUEUE_STATS_ROLLUP_LIST_SQL)
            .bind(table_name.map(str::trim).filter(|value| !value.is_empty()))
            .bind(from_bucket_date.trim())
            .bind(to_bucket_date.trim())
            .bind(limit.clamp(1, 1_000))
            .fetch_all(&self.pool)
            .await
            .map_err(SchedulerError::Database)?;

        rows.into_iter()
            .map(QueueStatsRollupRecord::from_row)
            .collect::<Result<Vec<_>, _>>()
            .map_err(SchedulerError::Database)
    }

    /// List cold monthly partitions that are old enough to archive and already
    /// have a matching `queue_stats_rollup.source_partition` bucket. This is a
    /// read-only planning query; archive execution remains a separate, explicit
    /// task because it will run `DETACH`/`DROP`.
    pub async fn list_partition_archive_candidates(
        &self,
        retention_months: i32,
        limit: i64,
    ) -> Result<Vec<PartitionArchiveCandidate>, SchedulerError> {
        self.list_partition_archive_candidates_as_of(retention_months, limit, "today")
            .await
    }

    async fn list_partition_archive_candidates_as_of(
        &self,
        retention_months: i32,
        limit: i64,
        reference_date: &str,
    ) -> Result<Vec<PartitionArchiveCandidate>, SchedulerError> {
        let rows = sqlx::query(PARTITION_ARCHIVE_CANDIDATES_SQL)
            .bind(retention_months.max(0))
            .bind(limit.clamp(1, 500))
            .bind(reference_date.trim())
            .fetch_all(&self.pool)
            .await
            .map_err(SchedulerError::Database)?;

        rows.into_iter()
            .map(PartitionArchiveCandidate::from_row)
            .collect::<Result<Vec<_>, _>>()
            .map_err(SchedulerError::Database)
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
                      and j.status in ('queued', 'running', 'failed')
                      and (j.status <> 'failed' or j.attempts < j.max_attempts)
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
    max_concurrency: i32,
) -> Result<i64, SchedulerError> {
    sqlx::query_scalar::<_, i64>(ACTIVE_SCHEDULED_TASK_RUN_CAPACITY_PROBE_SQL)
        .bind(task_id)
        .bind(i64::from(max_concurrency))
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

impl PartitionArchiveCandidate {
    fn from_row(row: PgRow) -> Result<Self, sqlx::Error> {
        Ok(Self {
            table_name: row.try_get("table_name")?,
            partition_name: row.try_get("partition_name")?,
            bucket_date: row.try_get("bucket_date")?,
            partition_end: row.try_get("partition_end")?,
            rollup_statuses: row.try_get("rollup_statuses")?,
            rollup_rows: row.try_get("rollup_rows")?,
        })
    }
}

impl QueueStatsRollupRecord {
    fn from_row(row: PgRow) -> Result<Self, sqlx::Error> {
        Ok(Self {
            bucket_date: row.try_get("bucket_date")?,
            table_name: row.try_get("table_name")?,
            status: row.try_get("status")?,
            row_count: row.try_get("row_count")?,
            finalized_at: row.try_get("finalized_at")?,
            source_partition: row.try_get("source_partition")?,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const REPOSITORY_SOURCE: &str = include_str!("repository.rs");
    const TRANSCODE_CLEANUP_MIGRATION: &str =
        include_str!("../../migrations/0062_transcode_output_cleanup_marker.sql");
    const READINESS_SCHEDULED_TASK_INDEX_MIGRATION: &str =
        include_str!("../../migrations/0072_readiness_scheduled_task_run_indexes.sql");

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
    fn transcode_cleanup_sql_has_live_schema_smoke() {
        let smoke_name = ["transcode_cleanup_sql", "executes_against_live_schema"].join("_");

        assert!(
            REPOSITORY_SOURCE.contains(&format!("async fn {smoke_name}")),
            "transcode cleanup candidate and marker SQL should have an ignored live-DB smoke"
        );
    }

    #[test]
    fn queue_stats_rollup_refresh_materializes_partitioned_table_counts() {
        let production_source = REPOSITORY_SOURCE
            .split("#[cfg(test)]")
            .next()
            .expect("repository source should include production section");
        let normalized = production_source
            .split_whitespace()
            .collect::<Vec<_>>()
            .join(" ");

        assert!(production_source.contains("QUEUE_STATS_ROLLUP_REFRESH_MONTH_SQL"));
        assert!(production_source.contains("refresh_queue_stats_rollup_for_month"));
        assert!(normalized.contains("date_trunc('month', $1::date)::date"));
        assert!(normalized.contains("on conflict (bucket_date, table_name, status) do update"));
        assert!(normalized.contains("row_count = excluded.row_count"));
        assert!(normalized.contains("source_partition = excluded.source_partition"));

        for table in [
            "job_events",
            "plugin_host_api_calls",
            "scheduled_task_runs",
            "job_runs",
            "playback_sessions",
        ] {
            assert!(
                normalized.contains(&format!("'{table}'::text")),
                "rollup refresh should cover partitioned table {table}"
            );
        }

        assert!(normalized.contains("event_level::text as status"));
        assert!(normalized.contains("status_code::text as status"));
        assert!(normalized.contains("scheduled_task_runs"));
        assert!(normalized.contains("job_runs"));
        assert!(
            normalized.contains(
                "case when stopped_at is null then 'active' else 'stopped' end as status"
            )
        );
        assert!(
            !normalized.contains("detach partition"),
            "rollup refresh should not archive or detach partitions"
        );
        assert!(
            !normalized.contains("drop table"),
            "rollup refresh should be non-destructive"
        );
    }

    #[test]
    fn queue_stats_rollup_refresh_has_live_schema_smoke() {
        let smoke_name = ["queue_stats_rollup_refresh", "writes_against_live_schema"].join("_");

        assert!(
            REPOSITORY_SOURCE.contains(&format!("async fn {smoke_name}")),
            "queue stats rollup refresh should have an ignored live-DB smoke"
        );
    }

    #[test]
    fn queue_stats_rollup_read_query_is_bounded_and_read_only() {
        let production_source = REPOSITORY_SOURCE
            .split("#[cfg(test)]")
            .next()
            .expect("repository source should include production section");

        assert!(production_source.contains("QUEUE_STATS_ROLLUP_LIST_SQL"));
        assert!(production_source.contains("pub struct QueueStatsRollupRecord"));
        assert!(production_source.contains("list_queue_stats_rollup"));

        let sql_start = production_source
            .find("const QUEUE_STATS_ROLLUP_LIST_SQL")
            .expect("queue stats rollup list SQL constant should exist");
        let sql_end = production_source[sql_start..]
            .find("\"#;")
            .map(|offset| sql_start + offset)
            .expect("queue stats rollup list SQL constant should end");
        let normalized = production_source[sql_start..sql_end]
            .split_whitespace()
            .collect::<Vec<_>>()
            .join(" ")
            .to_ascii_lowercase();

        assert!(normalized.contains("from queue_stats_rollup"));
        assert!(normalized.contains("where ($1::text is null or table_name = $1)"));
        assert!(normalized.contains("and bucket_date >= $2::date"));
        assert!(normalized.contains("and bucket_date < $3::date"));
        assert!(normalized.contains("order by bucket_date desc, table_name asc, status asc"));
        assert!(normalized.contains("limit $4"));
        assert!(!normalized.contains("detach partition"));
        assert!(!normalized.contains("drop table"));
        assert!(!normalized.contains("delete from"));
        assert!(!normalized.contains("insert into"));
        assert!(!normalized.contains("update "));
    }

    #[test]
    fn queue_stats_rollup_read_query_has_live_schema_smoke() {
        let smoke_name = [
            "queue_stats_rollup_read_query",
            "executes_against_live_schema",
        ]
        .join("_");

        assert!(
            REPOSITORY_SOURCE.contains(&format!("async fn {smoke_name}")),
            "queue stats rollup read query should have an ignored live-DB smoke"
        );
    }

    #[test]
    fn partition_archive_candidates_are_read_only_and_require_rollup_buckets() {
        let production_source = REPOSITORY_SOURCE
            .split("#[cfg(test)]")
            .next()
            .expect("repository source should include production section");

        assert!(production_source.contains("PARTITION_ARCHIVE_CANDIDATES_SQL"));
        assert!(production_source.contains("pub struct PartitionArchiveCandidate"));
        assert!(production_source.contains("list_partition_archive_candidates"));

        let sql_start = production_source
            .find("const PARTITION_ARCHIVE_CANDIDATES_SQL")
            .expect("archive candidate SQL constant should exist");
        let sql_end = production_source[sql_start..]
            .find("\"#;")
            .map(|offset| sql_start + offset)
            .expect("archive candidate SQL constant should end");
        let normalized = production_source[sql_start..sql_end]
            .split_whitespace()
            .collect::<Vec<_>>()
            .join(" ")
            .to_ascii_lowercase();

        assert!(normalized.contains("from pg_inherits"));
        assert!(normalized.contains("join pg_class parent"));
        assert!(normalized.contains("join pg_class child"));
        assert!(normalized.contains("child.relname !~ '_default$'"));
        assert!(normalized.contains("make_interval(months => $1)"));
        assert!(normalized.contains("date_trunc('month', $3::date)::date"));
        assert!(normalized.contains("exists ( select 1 from queue_stats_rollup"));
        assert!(normalized.contains("rollup.source_partition = partitions.partition_name"));

        for table in [
            "job_events",
            "plugin_host_api_calls",
            "scheduled_task_runs",
            "job_runs",
            "playback_sessions",
        ] {
            assert!(
                normalized.contains(&format!("'{table}'")),
                "archive candidate discovery should cover {table}"
            );
        }

        assert!(
            !normalized.contains("detach partition"),
            "candidate discovery must not detach partitions"
        );
        assert!(
            !normalized.contains("drop table"),
            "candidate discovery must not drop partitions"
        );
        assert!(
            !normalized.contains("delete from"),
            "candidate discovery must not mutate data"
        );
    }

    #[test]
    fn partition_archive_candidates_have_live_schema_smoke() {
        let smoke_name = ["partition_archive_candidates", "plan_against_live_schema"].join("_");

        assert!(
            REPOSITORY_SOURCE.contains(&format!("async fn {smoke_name}")),
            "partition archive candidate discovery should have an ignored live-DB smoke"
        );
    }

    // Live-DB smoke: validates the queue stats rollup read query plans and
    // returns a bounded marker row through the production repository method.
    // It only writes and removes one queue_stats_rollup row.
    //   cargo test -- --ignored queue_stats_rollup_read_query_executes_against_live_schema
    #[tokio::test]
    #[ignore = "requires a running PostgreSQL from ./scripts/dev-deps.ps1"]
    async fn queue_stats_rollup_read_query_executes_against_live_schema() {
        use sqlx::postgres::PgPoolOptions;

        let url = std::env::var("DATABASE_URL")
            .unwrap_or_else(|_| "postgres://fbz:fbz@127.0.0.1:5432/fbz".to_owned());
        let pool = PgPoolOptions::new()
            .max_connections(2)
            .connect(&url)
            .await
            .expect("connect to live PostgreSQL");
        crate::db::migrate(&pool).await.expect("run migrations");

        let plan_rows = sqlx::query(&format!("explain {QUEUE_STATS_ROLLUP_LIST_SQL}"))
            .bind(Some("job_events"))
            .bind("2099-01-01")
            .bind("2099-02-01")
            .bind(10_i64)
            .fetch_all(&pool)
            .await
            .expect("queue stats rollup read SQL should parse and plan");
        assert!(
            !plan_rows.is_empty(),
            "EXPLAIN should return a plan for queue stats rollup reads"
        );

        let suffix = format!(
            "{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .expect("system clock should be after epoch")
                .as_nanos()
        );
        let status = format!("read_smoke_{suffix}");
        let source_partition = format!("job_events_read_smoke_{suffix}");

        sqlx::query(
            r#"
            insert into queue_stats_rollup (
                bucket_date,
                table_name,
                status,
                row_count,
                source_partition
            )
            values ('2099-01-01'::date, 'job_events', $1, 42, $2)
            on conflict (bucket_date, table_name, status) do update
                set row_count = excluded.row_count,
                    finalized_at = now(),
                    source_partition = excluded.source_partition
            "#,
        )
        .bind(&status)
        .bind(&source_partition)
        .execute(&pool)
        .await
        .expect("insert smoke queue stats rollup row");

        let repository = SchedulerRepository::new(pool.clone());
        let rows = repository
            .list_queue_stats_rollup(Some("job_events"), "2099-01-01", "2099-02-01", 10)
            .await
            .expect("queue stats rollup read should execute");
        let smoke_row = rows
            .iter()
            .find(|row| row.status == status)
            .expect("smoke rollup row should be returned");
        assert_eq!(smoke_row.bucket_date, "2099-01-01");
        assert_eq!(smoke_row.table_name, "job_events");
        assert_eq!(smoke_row.row_count, 42);
        assert_eq!(
            smoke_row.source_partition.as_deref(),
            Some(source_partition.as_str())
        );

        let limited_rows = repository
            .list_queue_stats_rollup(None, "2099-01-01", "2099-02-01", 1)
            .await
            .expect("queue stats rollup bounded read should execute");
        assert_eq!(limited_rows.len(), 1);

        sqlx::query(
            r#"
            delete from queue_stats_rollup
            where bucket_date = '2099-01-01'::date
              and table_name = 'job_events'
              and status = $1
              and source_partition = $2
            "#,
        )
        .bind(&status)
        .bind(&source_partition)
        .execute(&pool)
        .await
        .expect("delete smoke queue stats rollup row");
    }

    // Live-DB smoke: validates transcode cleanup candidate and marker SQL
    // parse, plan, and execute against the migrated schema. This inserts one
    // uniquely named terminal session and only mutates that smoke row.
    //   cargo test -- --ignored transcode_cleanup_sql_executes_against_live_schema
    #[tokio::test]
    #[ignore = "requires a running PostgreSQL from ./scripts/dev-deps.ps1"]
    async fn transcode_cleanup_sql_executes_against_live_schema() {
        use sqlx::postgres::PgPoolOptions;

        let url = std::env::var("DATABASE_URL")
            .unwrap_or_else(|_| "postgres://fbz:fbz@127.0.0.1:5432/fbz".to_owned());
        let pool = PgPoolOptions::new()
            .max_connections(2)
            .connect(&url)
            .await
            .expect("connect to live PostgreSQL");
        crate::db::migrate(&pool).await.expect("run migrations");

        let cleanup_index_def = sqlx::query_scalar::<_, String>(
            r#"
            select indexdef
            from pg_indexes
            where schemaname = 'public'
              and indexname = 'idx_transcoding_sessions_output_cleanup_pending'
            "#,
        )
        .fetch_one(&pool)
        .await
        .expect("transcode cleanup pending index should exist");
        let normalized_index_def = cleanup_index_def.to_ascii_lowercase();
        assert!(normalized_index_def.contains("finished_at"));
        assert!(normalized_index_def.contains("nulls first"));
        assert!(normalized_index_def.contains("id"));
        assert!(normalized_index_def.contains("status"));
        assert!(normalized_index_def.contains("cancelled"));
        assert!(normalized_index_def.contains("failed"));
        assert!(normalized_index_def.contains("output_cleaned_at is null"));
        assert!(normalized_index_def.contains("output_path is not null"));

        let plan_rows = sqlx::query(&format!("explain {TRANSCODE_CLEANUP_CANDIDATES_SQL}"))
            .bind(1_i64)
            .fetch_all(&pool)
            .await
            .expect("transcode cleanup candidate SQL should parse and plan");
        assert!(
            !plan_rows.is_empty(),
            "EXPLAIN should return a query plan for transcode cleanup candidates"
        );

        let suffix = format!(
            "{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .expect("system clock should be after epoch")
                .as_nanos()
        );
        let role_name = format!("transcode-cleanup-smoke-role-{suffix}");
        let username = format!("transcode-cleanup-smoke-user-{suffix}");
        let library_name = format!("Transcode cleanup smoke {suffix}");
        let output_path = format!("H:/fbz-smoke/transcode-cleanup/{suffix}/master.m3u8");

        let role_id = sqlx::query_scalar::<_, i64>(
            r#"
            insert into roles (name, name_normalized, description)
            values ($1, lower($1), 'transcode cleanup live schema smoke')
            returning id
            "#,
        )
        .bind(&role_name)
        .fetch_one(&pool)
        .await
        .expect("create smoke role");

        let user_id = sqlx::query_scalar::<_, i64>(
            r#"
            insert into users (username, username_normalized, role_id)
            values ($1, lower($1), $2)
            returning id
            "#,
        )
        .bind(&username)
        .bind(role_id)
        .fetch_one(&pool)
        .await
        .expect("create smoke user");

        let library_id = sqlx::query_scalar::<_, i64>(
            r#"
            insert into libraries (name, library_type)
            values ($1, 'mixed')
            returning id
            "#,
        )
        .bind(&library_name)
        .fetch_one(&pool)
        .await
        .expect("create smoke library");

        let media_item_id = sqlx::query_scalar::<_, i64>(
            r#"
            insert into media_items (library_id, item_type, title, sort_title, scan_status)
            values ($1, 'movie', $2, $2, 'scanned')
            returning id
            "#,
        )
        .bind(library_id)
        .bind(&library_name)
        .fetch_one(&pool)
        .await
        .expect("create smoke media item");

        let session_id = sqlx::query_scalar::<_, String>(
            r#"
            insert into transcoding_sessions (
                user_id,
                media_item_id,
                status,
                output_path,
                manifest_path,
                started_at,
                finished_at
            )
            values (
                $1,
                $2,
                'failed',
                $3,
                $3,
                now() - interval '2 minutes',
                now() - interval '1 minute'
            )
            returning public_id::text
            "#,
        )
        .bind(user_id)
        .bind(media_item_id)
        .bind(&output_path)
        .fetch_one(&pool)
        .await
        .expect("create terminal transcode smoke session");

        let repository = SchedulerRepository::new(pool.clone());
        let candidates = repository
            .list_transcode_cleanup_candidates(50)
            .await
            .expect("transcode cleanup candidate query should execute");
        assert!(
            candidates
                .iter()
                .any(|candidate| candidate.id == session_id && candidate.output_path == output_path),
            "terminal smoke session should be returned as cleanup candidate"
        );

        let marked = repository
            .mark_transcode_output_cleaned(&session_id)
            .await
            .expect("mark transcode output cleaned should execute");
        assert!(marked, "smoke session should be marked cleaned");

        let marked_again = repository
            .mark_transcode_output_cleaned(&session_id)
            .await
            .expect("idempotent mark should execute");
        assert!(
            !marked_again,
            "already-cleaned smoke session should not be marked again"
        );

        let cleaned_at = sqlx::query_scalar::<_, Option<String>>(
            r#"
            select output_cleaned_at::text
            from transcoding_sessions
            where public_id = $1::uuid
            "#,
        )
        .bind(&session_id)
        .fetch_one(&pool)
        .await
        .expect("read smoke cleanup marker");
        assert!(cleaned_at.is_some());

        let invalid_mark = repository
            .mark_transcode_output_cleaned("not-a-uuid")
            .await
            .expect("invalid public id should be safely handled");
        assert!(
            !invalid_mark,
            "invalid public id should not update any transcode cleanup marker"
        );
    }

    // Live-DB smoke: validates the queue stats rollup refresh writes
    // idempotent monthly materialized counts for the five partitioned tables.
    // It inserts uniquely marked rows, refreshes one covered future bucket,
    // then removes the smoke rows and refreshes again to restore the bucket.
    //   cargo test -- --ignored queue_stats_rollup_refresh_writes_against_live_schema
    #[tokio::test]
    #[ignore = "requires a running PostgreSQL from ./scripts/dev-deps.ps1"]
    async fn queue_stats_rollup_refresh_writes_against_live_schema() {
        use sqlx::postgres::PgPoolOptions;

        let url = std::env::var("DATABASE_URL")
            .unwrap_or_else(|_| "postgres://fbz:fbz@127.0.0.1:5432/fbz".to_owned());
        let pool = PgPoolOptions::new()
            .max_connections(2)
            .connect(&url)
            .await
            .expect("connect to live PostgreSQL");
        crate::db::migrate(&pool).await.expect("run migrations");

        let plan_rows = sqlx::query(&format!("explain {QUEUE_STATS_ROLLUP_REFRESH_MONTH_SQL}"))
            .bind("today")
            .fetch_all(&pool)
            .await
            .expect("queue stats rollup refresh SQL should parse and plan");
        assert!(
            !plan_rows.is_empty(),
            "EXPLAIN should return a query plan for queue stats rollup refresh"
        );

        let target_bucket = sqlx::query_scalar::<_, String>(
            "select ((date_trunc('month', now()) + interval '5 months')::date)::text",
        )
        .fetch_one(&pool)
        .await
        .expect("compute smoke target bucket");
        let target_partition_suffix =
            sqlx::query_scalar::<_, String>(r#"select to_char($1::date, 'YYYY"m"MM')"#)
                .bind(&target_bucket)
                .fetch_one(&pool)
                .await
                .expect("compute smoke partition suffix");

        let baseline_job_events_warn = count_job_events(&pool, &target_bucket, "warn").await;
        let baseline_host_202 = count_plugin_host_api_calls(&pool, &target_bucket, 202).await;
        let baseline_scheduled_succeeded =
            count_scheduled_task_runs(&pool, &target_bucket, "succeeded").await;
        let baseline_job_runs_failed = count_job_runs(&pool, &target_bucket, "failed").await;
        let baseline_playback_active = count_playback_sessions(&pool, &target_bucket, true).await;

        let suffix = format!(
            "{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .expect("system clock should be after epoch")
                .as_nanos()
        );
        let marker = format!("rollup-smoke-{suffix}");
        let plugin_id = format!("rollup-smoke-plugin-{suffix}");
        let role_name = format!("rollup smoke role {suffix}");
        let username = format!("rollup-smoke-user-{suffix}");
        let library_name = format!("Rollup smoke library {suffix}");

        sqlx::query(
            r#"
            insert into job_events (event_type, event_level, message, payload, created_at)
            values ($1, 'warn', $1, jsonb_build_object('smoke', $1), $2::date + interval '15 days')
            "#,
        )
        .bind(&marker)
        .bind(&target_bucket)
        .execute(&pool)
        .await
        .expect("insert smoke job event");

        sqlx::query(
            r#"
            insert into plugin_installations (plugin_id, permission_fingerprint)
            values ($1, gen_random_bytes(32))
            "#,
        )
        .bind(&plugin_id)
        .execute(&pool)
        .await
        .expect("insert smoke plugin installation");

        sqlx::query(
            r#"
            insert into plugin_host_api_calls (
                plugin_id,
                package_id,
                method,
                path,
                status_code,
                started_at,
                finished_at,
                duration_ms
            )
            values (
                $1,
                $2,
                'GET',
                '/rollup-smoke',
                202,
                $3::date + interval '15 days',
                $3::date + interval '15 days 1 minute',
                60000
            )
            "#,
        )
        .bind(&plugin_id)
        .bind(&marker)
        .bind(&target_bucket)
        .execute(&pool)
        .await
        .expect("insert smoke Host API call");

        let task_id = sqlx::query_scalar::<_, i64>(
            r#"
            insert into scheduled_tasks (task_key, task_type, schedule_kind, schedule_value)
            values ($1, 'rollup.smoke', 'interval', 'daily')
            returning id
            "#,
        )
        .bind(&marker)
        .fetch_one(&pool)
        .await
        .expect("insert smoke scheduled task");

        sqlx::query(
            r#"
            insert into scheduled_task_runs (
                task_id,
                task_key,
                trigger_type,
                worker_id,
                status,
                lease_expires_at,
                started_at,
                finished_at
            )
            values (
                $1,
                $2,
                'manual',
                $2,
                'succeeded',
                $3::date + interval '15 days 1 hour',
                $3::date + interval '15 days',
                $3::date + interval '15 days 10 minutes'
            )
            "#,
        )
        .bind(task_id)
        .bind(&marker)
        .bind(&target_bucket)
        .execute(&pool)
        .await
        .expect("insert smoke scheduled task run");

        let job_id = sqlx::query_scalar::<_, i64>(
            r#"
            insert into jobs (job_type, status, queue_name, payload, finished_at)
            values ($1, 'failed', 'rollup-smoke', jsonb_build_object('smoke', $1), $2::date + interval '15 days 5 minutes')
            returning id
            "#,
        )
        .bind(&marker)
        .bind(&target_bucket)
        .fetch_one(&pool)
        .await
        .expect("insert smoke job");

        sqlx::query(
            r#"
            insert into job_runs (job_id, worker_id, status, started_at, finished_at, error_message)
            values (
                $1,
                $2,
                'failed',
                $3::date + interval '15 days',
                $3::date + interval '15 days 5 minutes',
                'rollup smoke'
            )
            "#,
        )
        .bind(job_id)
        .bind(&marker)
        .bind(&target_bucket)
        .execute(&pool)
        .await
        .expect("insert smoke job run");

        let role_id = sqlx::query_scalar::<_, i64>(
            r#"
            insert into roles (name, name_normalized, description)
            values ($1, lower($1), 'queue stats rollup live schema smoke')
            returning id
            "#,
        )
        .bind(&role_name)
        .fetch_one(&pool)
        .await
        .expect("insert smoke role");

        let user_id = sqlx::query_scalar::<_, i64>(
            r#"
            insert into users (username, username_normalized, role_id)
            values ($1, lower($1), $2)
            returning id
            "#,
        )
        .bind(&username)
        .bind(role_id)
        .fetch_one(&pool)
        .await
        .expect("insert smoke user");

        let library_id = sqlx::query_scalar::<_, i64>(
            r#"
            insert into libraries (name, library_type)
            values ($1, 'mixed')
            returning id
            "#,
        )
        .bind(&library_name)
        .fetch_one(&pool)
        .await
        .expect("insert smoke library");

        let media_item_id = sqlx::query_scalar::<_, i64>(
            r#"
            insert into media_items (library_id, item_type, title, sort_title, scan_status)
            values ($1, 'movie', $2, $2, 'scanned')
            returning id
            "#,
        )
        .bind(library_id)
        .bind(&library_name)
        .fetch_one(&pool)
        .await
        .expect("insert smoke media item");

        sqlx::query(
            r#"
            insert into playback_sessions (
                user_id,
                media_item_id,
                play_method,
                started_at,
                last_progress_at
            )
            values (
                $1,
                $2,
                'direct_play',
                $3::date + interval '15 days',
                $3::date + interval '15 days 5 minutes'
            )
            "#,
        )
        .bind(user_id)
        .bind(media_item_id)
        .bind(&target_bucket)
        .execute(&pool)
        .await
        .expect("insert smoke playback session");

        let repository = SchedulerRepository::new(pool.clone());
        let refreshed = repository
            .refresh_queue_stats_rollup_for_month(&target_bucket)
            .await
            .expect("refresh smoke queue stats rollup bucket");
        assert!(
            refreshed >= 5,
            "refresh should write at least one row for each smoke table/status"
        );

        assert_eq!(
            rollup_count(&pool, &target_bucket, "job_events", "warn").await,
            baseline_job_events_warn + 1
        );
        assert_eq!(
            rollup_count(&pool, &target_bucket, "plugin_host_api_calls", "202").await,
            baseline_host_202 + 1
        );
        assert_eq!(
            rollup_count(&pool, &target_bucket, "scheduled_task_runs", "succeeded").await,
            baseline_scheduled_succeeded + 1
        );
        assert_eq!(
            rollup_count(&pool, &target_bucket, "job_runs", "failed").await,
            baseline_job_runs_failed + 1
        );
        assert_eq!(
            rollup_count(&pool, &target_bucket, "playback_sessions", "active").await,
            baseline_playback_active + 1
        );

        let job_events_source_partition = sqlx::query_scalar::<_, String>(
            r#"
            select source_partition
            from queue_stats_rollup
            where bucket_date = $1::date
              and table_name = 'job_events'
              and status = 'warn'
            "#,
        )
        .bind(&target_bucket)
        .fetch_one(&pool)
        .await
        .expect("read job_events rollup source partition");
        assert_eq!(
            job_events_source_partition,
            format!("job_events_{target_partition_suffix}")
        );

        let refreshed_again = repository
            .refresh_queue_stats_rollup_for_month(&target_bucket)
            .await
            .expect("refresh smoke queue stats rollup bucket idempotently");
        assert_eq!(refreshed_again, refreshed);
        assert_eq!(
            rollup_count(&pool, &target_bucket, "job_events", "warn").await,
            baseline_job_events_warn + 1
        );

        sqlx::query("delete from job_events where payload->>'smoke' = $1")
            .bind(&marker)
            .execute(&pool)
            .await
            .expect("delete smoke job event");
        sqlx::query("delete from jobs where payload->>'smoke' = $1")
            .bind(&marker)
            .execute(&pool)
            .await
            .expect("delete smoke job and run");
        sqlx::query("delete from scheduled_tasks where task_key = $1")
            .bind(&marker)
            .execute(&pool)
            .await
            .expect("delete smoke scheduled task and run");
        sqlx::query("delete from plugin_installations where plugin_id = $1")
            .bind(&plugin_id)
            .execute(&pool)
            .await
            .expect("delete smoke plugin installation and Host API call");
        sqlx::query("delete from users where id = $1")
            .bind(user_id)
            .execute(&pool)
            .await
            .expect("delete smoke user and playback session");
        sqlx::query("delete from libraries where id = $1")
            .bind(library_id)
            .execute(&pool)
            .await
            .expect("delete smoke library and media item");
        sqlx::query("delete from roles where id = $1")
            .bind(role_id)
            .execute(&pool)
            .await
            .expect("delete smoke role");

        repository
            .refresh_queue_stats_rollup_for_month(&target_bucket)
            .await
            .expect("restore smoke queue stats rollup bucket after cleanup");

        assert_eq!(
            rollup_count(&pool, &target_bucket, "job_events", "warn").await,
            baseline_job_events_warn
        );
    }

    // Live-DB smoke: validates cold partition archive candidate discovery stays
    // read-only and only reports monthly partitions that have materialized
    // rollup evidence. It writes and removes one rollup row only.
    //   cargo test -- --ignored partition_archive_candidates_plan_against_live_schema
    #[tokio::test]
    #[ignore = "requires a running PostgreSQL from ./scripts/dev-deps.ps1"]
    async fn partition_archive_candidates_plan_against_live_schema() {
        use sqlx::postgres::PgPoolOptions;

        let url = std::env::var("DATABASE_URL")
            .unwrap_or_else(|_| "postgres://fbz:fbz@127.0.0.1:5432/fbz".to_owned());
        let pool = PgPoolOptions::new()
            .max_connections(2)
            .connect(&url)
            .await
            .expect("connect to live PostgreSQL");
        crate::db::migrate(&pool).await.expect("run migrations");

        let plan_rows = sqlx::query(&format!("explain {PARTITION_ARCHIVE_CANDIDATES_SQL}"))
            .bind(0_i32)
            .bind(10_i64)
            .bind("today")
            .fetch_all(&pool)
            .await
            .expect("partition archive candidate SQL should parse and plan");
        assert!(
            !plan_rows.is_empty(),
            "EXPLAIN should return a plan for archive candidate discovery"
        );

        let cold_partition = sqlx::query(
            r#"
            select parent.relname as table_name,
                   child.relname as partition_name,
                   make_date((parts)[1]::int, (parts)[2]::int, 1)::text as bucket_date
            from pg_inherits
            join pg_class parent
              on parent.oid = pg_inherits.inhparent
            join pg_class child
              on child.oid = pg_inherits.inhrelid
            cross join lateral regexp_match(child.relname, '_(\d{4})m(\d{2})$') parts
            where parent.relname = 'job_events'
              and child.relname !~ '_default$'
            order by bucket_date asc
            limit 1
            "#,
        )
        .fetch_one(&pool)
        .await
        .expect("find a migrated monthly job_events partition");
        let table_name = cold_partition
            .try_get::<String, _>("table_name")
            .expect("table name");
        let partition_name = cold_partition
            .try_get::<String, _>("partition_name")
            .expect("partition name");
        let bucket_date = cold_partition
            .try_get::<String, _>("bucket_date")
            .expect("bucket date");

        sqlx::query(
            r#"
            insert into queue_stats_rollup (
                bucket_date,
                table_name,
                status,
                row_count,
                source_partition
            )
            values ($1::date, $2, 'archive_smoke', 0, $3)
            on conflict (bucket_date, table_name, status) do update
                set row_count = excluded.row_count,
                    finalized_at = now(),
                    source_partition = excluded.source_partition
            "#,
        )
        .bind(&bucket_date)
        .bind(&table_name)
        .bind(&partition_name)
        .execute(&pool)
        .await
        .expect("insert archive candidate smoke rollup bucket");

        let repository = SchedulerRepository::new(pool.clone());
        let current_reference_candidates = repository
            .list_partition_archive_candidates(0, 100)
            .await
            .expect("list archive candidates with today reference");
        assert!(
            current_reference_candidates.iter().all(|candidate| {
                candidate.partition_name != partition_name || candidate.bucket_date != bucket_date
            }),
            "the current month partition should not be a candidate when referenced from today"
        );

        let reference_date_after_partition = add_month_date_text(&pool, &bucket_date).await;
        let candidates = repository
            .list_partition_archive_candidates_as_of(0, 100, &reference_date_after_partition)
            .await
            .expect("list archive candidates");
        let smoke_candidate = candidates
            .iter()
            .find(|candidate| {
                candidate.table_name == table_name
                    && candidate.partition_name == partition_name
                    && candidate.bucket_date == bucket_date
            })
            .expect("smoke partition should be an archive candidate after rollup exists");

        assert_eq!(smoke_candidate.rollup_statuses, 1);
        assert_eq!(smoke_candidate.rollup_rows, 0);
        assert_eq!(
            smoke_candidate.partition_end,
            reference_date_after_partition
        );

        sqlx::query(
            r#"
            delete from queue_stats_rollup
            where bucket_date = $1::date
              and table_name = $2
              and status = 'archive_smoke'
              and source_partition = $3
            "#,
        )
        .bind(&bucket_date)
        .bind(&table_name)
        .bind(&partition_name)
        .execute(&pool)
        .await
        .expect("delete archive candidate smoke rollup bucket");
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
        assert!(EXPIRE_STALE_SCHEDULED_TASK_RUNS_SQL.contains("returning runs.id,"));
        assert!(EXPIRE_STALE_SCHEDULED_TASK_RUNS_SQL.contains("trigger_type"));
    }

    #[test]
    fn stale_scheduled_task_run_recovery_uses_bounded_locked_candidate_batch() {
        let normalized = EXPIRE_STALE_SCHEDULED_TASK_RUNS_SQL
            .split_whitespace()
            .collect::<Vec<_>>()
            .join(" ");

        assert!(normalized.contains("with stale_run_candidates as"));
        assert!(normalized.contains("from scheduled_task_runs"));
        assert!(normalized.contains("where status = 'running'"));
        assert!(normalized.contains("lease_expires_at <= now()"));
        assert!(normalized.contains("order by lease_expires_at asc, id asc"));
        assert!(normalized.contains("limit 1000"));
        assert!(normalized.contains("for update skip locked"));
        assert!(normalized.contains("from stale_run_candidates candidates"));
        assert!(normalized.contains("runs.id = candidates.id"));
        assert!(normalized.contains("runs.started_at = candidates.started_at"));
        assert!(
            !normalized.contains("), with expired_runs as"),
            "stale run recovery should use one WITH clause with comma-separated CTEs"
        );
        assert!(
            !normalized.contains("update scheduled_task_runs set status = 'expired' error_message"),
            "stale scheduled task run recovery should not update every expired running run directly"
        );
    }

    #[test]
    fn stale_scheduled_task_run_recovery_index_matches_candidate_batch_shape() {
        assert!(
            READINESS_SCHEDULED_TASK_INDEX_MIGRATION
                .contains("idx_scheduled_task_runs_readiness_running_lease")
        );
        assert!(
            READINESS_SCHEDULED_TASK_INDEX_MIGRATION
                .contains("on scheduled_task_runs (lease_expires_at, id)")
        );
        assert!(READINESS_SCHEDULED_TASK_INDEX_MIGRATION.contains("where status = 'running'"));
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

    #[test]
    fn scheduled_task_claim_concurrency_uses_bounded_probes() {
        let production_source = REPOSITORY_SOURCE
            .split("#[cfg(test)]")
            .next()
            .expect("repository source should include production section");

        assert!(production_source.contains("CLAIM_DUE_SCHEDULED_TASK_SQL"));
        assert!(production_source.contains("ACTIVE_SCHEDULED_TASK_RUN_CAPACITY_PROBE_SQL"));
        assert!(production_source.contains("limit st.max_concurrency"));
        assert!(production_source.contains("limit $2"));
        assert!(production_source.contains("order by lease_expires_at asc, id asc"));

        let normalized = production_source
            .split_whitespace()
            .collect::<Vec<_>>()
            .join(" ");
        assert!(
            !normalized.contains(
                "select count(*)::bigint from scheduled_task_runs runs where runs.task_id = st.id"
            ),
            "due task claim should not exact-count all active runs for a task"
        );
        assert!(
            !normalized
                .contains("select count(*)::bigint from scheduled_task_runs where task_id = $1"),
            "manual task claim should not exact-count all active runs for a task"
        );
    }

    // Live-DB smoke: validates stale scheduled-task run recovery parses and
    // plans against the migrated schema. Plain EXPLAIN does not execute the
    // UPDATE, so this does not expire any real task runs.
    //   cargo test -- --ignored stale_scheduled_task_run_recovery_sql_plans_against_live_schema
    #[tokio::test]
    #[ignore = "requires a running PostgreSQL from ./scripts/dev-deps.ps1"]
    async fn stale_scheduled_task_run_recovery_sql_plans_against_live_schema() {
        use sqlx::postgres::PgPoolOptions;

        let url = std::env::var("DATABASE_URL")
            .unwrap_or_else(|_| "postgres://fbz:fbz@127.0.0.1:5432/fbz".to_owned());
        let pool = PgPoolOptions::new()
            .max_connections(2)
            .connect(&url)
            .await
            .expect("connect to live PostgreSQL");
        crate::db::migrate(&pool).await.expect("run migrations");

        let plan_rows = sqlx::query(&format!("explain {EXPIRE_STALE_SCHEDULED_TASK_RUNS_SQL}"))
            .fetch_all(&pool)
            .await
            .expect("stale scheduled task run recovery SQL should parse and plan");
        assert!(
            !plan_rows.is_empty(),
            "EXPLAIN should return a query plan for stale scheduled task run recovery"
        );
    }

    // Live-DB smoke: validates the scheduler claim capacity probes parse and
    // plan against the real migrated schema via EXPLAIN. Plain EXPLAIN does
    // not execute the SELECT, so it does not claim or mutate scheduled tasks.
    //   cargo test -- --ignored scheduled_task_claim_capacity_probes_execute_against_live_schema
    #[tokio::test]
    #[ignore = "requires a running PostgreSQL from ./scripts/dev-deps.ps1"]
    async fn scheduled_task_claim_capacity_probes_execute_against_live_schema() {
        use sqlx::postgres::PgPoolOptions;

        let url = std::env::var("DATABASE_URL")
            .unwrap_or_else(|_| "postgres://fbz:fbz@127.0.0.1:5432/fbz".to_owned());
        let pool = PgPoolOptions::new()
            .max_connections(2)
            .connect(&url)
            .await
            .expect("connect to live PostgreSQL");
        crate::db::migrate(&pool).await.expect("run migrations");

        let due_plan_rows = sqlx::query(&format!("explain {CLAIM_DUE_SCHEDULED_TASK_SQL}"))
            .fetch_all(&pool)
            .await
            .expect("due scheduled task claim SQL should parse and plan");
        assert!(
            !due_plan_rows.is_empty(),
            "EXPLAIN should return a query plan for due scheduled task claim"
        );

        let manual_plan_rows = sqlx::query(&format!(
            "explain {ACTIVE_SCHEDULED_TASK_RUN_CAPACITY_PROBE_SQL}"
        ))
        .bind(1_i64)
        .bind(1_i64)
        .fetch_all(&pool)
        .await
        .expect("manual scheduled task capacity probe should parse and plan");
        assert!(
            !manual_plan_rows.is_empty(),
            "EXPLAIN should return a query plan for manual scheduled task capacity probe"
        );
    }

    // Live-DB smoke: retryable failed library.scan jobs are still active
    // dedupe entries. This exercises both the admin manual enqueue path and
    // the scheduler scan-all enqueue path against the migrated schema.
    //   cargo test -- --ignored scan_queueing_reuses_retryable_failed_jobs_against_live_schema
    #[tokio::test]
    #[ignore = "requires a running PostgreSQL from ./scripts/dev-deps.ps1"]
    async fn scan_queueing_reuses_retryable_failed_jobs_against_live_schema() {
        use crate::admin::repository::{AdminRepository, QueueLibraryScanInput};
        use sqlx::postgres::PgPoolOptions;

        let url = std::env::var("DATABASE_URL")
            .unwrap_or_else(|_| "postgres://fbz:fbz@127.0.0.1:5432/fbz".to_owned());
        let pool = PgPoolOptions::new()
            .max_connections(2)
            .connect(&url)
            .await
            .expect("connect to live PostgreSQL");
        crate::db::migrate(&pool).await.expect("run migrations");

        let suffix = format!(
            "{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .expect("system clock should be after epoch")
                .as_nanos()
        );
        let marker = format!("scan-dedupe-smoke-{suffix}");
        let library_name = format!("Scan dedupe smoke {suffix}");
        let library_path = format!("H:/fbz-smoke/scan-dedupe/{suffix}");

        let library_row = sqlx::query(
            r#"
            insert into libraries (name, library_type)
            values ($1, 'mixed')
            returning id, public_id::text as public_id
            "#,
        )
        .bind(&library_name)
        .fetch_one(&pool)
        .await
        .expect("create scan dedupe smoke library");
        let library_id = library_row
            .try_get::<i64, _>("id")
            .expect("library id should be returned");
        let library_public_id = library_row
            .try_get::<String, _>("public_id")
            .expect("library public id should be returned");

        sqlx::query(
            r#"
            insert into library_paths (
                library_id,
                path,
                normalized_path,
                path_hash,
                is_enabled
            )
            values ($1, $2, lower($2), gen_random_bytes(32), true)
            "#,
        )
        .bind(library_id)
        .bind(&library_path)
        .execute(&pool)
        .await
        .expect("create enabled smoke library path");

        let failed_job_id = sqlx::query_scalar::<_, String>(
            r#"
            insert into jobs (
                job_type,
                status,
                queue_name,
                priority,
                payload,
                attempts,
                max_attempts,
                last_error,
                finished_at
            )
            values (
                'library.scan',
                'failed',
                'scan',
                0,
                jsonb_build_object(
                    'libraryId', $1::text,
                    'requestedByUserId', null,
                    'reason', $2::text
                ),
                1,
                3,
                'scan dedupe smoke retryable failure',
                now()
            )
            returning public_id::text
            "#,
        )
        .bind(&library_public_id)
        .bind(&marker)
        .fetch_one(&pool)
        .await
        .expect("create retryable failed scan job");

        let admin_repository = AdminRepository::new(pool.clone());
        let manual_job = admin_repository
            .queue_library_scan(QueueLibraryScanInput {
                library_id: library_public_id.clone(),
                requested_by_user_id: 0,
                reason: Some("manual scan dedupe smoke".to_owned()),
            })
            .await
            .expect("manual scan enqueue should execute against live schema")
            .expect("manual scan enqueue should return an active job");

        let inserted_blockers = sqlx::query_scalar::<_, i64>(
            r#"
            with eligible_libraries as (
                select l.public_id::text as library_public_id
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
                    payload,
                    attempts,
                    max_attempts,
                    last_error,
                    finished_at
                )
                select
                    'library.scan',
                    'failed',
                    'scan',
                    0,
                    jsonb_build_object(
                        'libraryId', eligible_libraries.library_public_id,
                        'requestedByUserId', null,
                        'reason', $2::text
                    ),
                    1,
                    3,
                    'scan dedupe smoke temporary blocker',
                    now()
                from eligible_libraries
                where eligible_libraries.library_public_id <> $1
                  and not exists (
                      select 1
                      from jobs j
                      where j.job_type = 'library.scan'
                        and j.status in ('queued', 'running', 'failed')
                        and (j.status <> 'failed' or j.attempts < j.max_attempts)
                        and j.payload->>'libraryId' = eligible_libraries.library_public_id
                  )
                returning 1
            )
            select count(*)::bigint from inserted
            "#,
        )
        .bind(&library_public_id)
        .bind(&marker)
        .fetch_one(&pool)
        .await
        .expect("insert temporary scan dedupe blockers");

        let scheduler_repository = SchedulerRepository::new(pool.clone());
        let queued_by_scheduler = scheduler_repository
            .queue_scan_all("scan dedupe smoke")
            .await
            .expect("scheduler scan-all enqueue should execute against live schema");

        let active_target_jobs = sqlx::query_scalar::<_, String>(
            r#"
            select public_id::text
            from jobs
            where job_type = 'library.scan'
              and status in ('queued', 'running', 'failed')
              and (status <> 'failed' or attempts < max_attempts)
              and payload->>'libraryId' = $1
            order by created_at desc, id desc
            "#,
        )
        .bind(&library_public_id)
        .fetch_all(&pool)
        .await
        .expect("list active scan jobs for smoke library");

        sqlx::query("delete from jobs where job_type = 'library.scan' and payload->>'reason' = $1")
            .bind(&marker)
            .execute(&pool)
            .await
            .expect("delete temporary scan blocker jobs");
        sqlx::query(
            "delete from jobs where job_type = 'library.scan' and payload->>'libraryId' = $1",
        )
        .bind(&library_public_id)
        .execute(&pool)
        .await
        .expect("delete smoke scan jobs");
        sqlx::query("delete from libraries where id = $1")
            .bind(library_id)
            .execute(&pool)
            .await
            .expect("delete smoke library");

        assert_eq!(manual_job.id, failed_job_id);
        assert_eq!(manual_job.status, "failed");
        assert_eq!(manual_job.queue_name, "scan");
        assert_eq!(manual_job.job_type, "library.scan");
        assert!(
            inserted_blockers >= 0,
            "temporary blocker count should be a valid lower-bound sanity check"
        );
        assert_eq!(
            queued_by_scheduler, 0,
            "scan-all should not enqueue any library while retryable failed scan jobs are active"
        );
        assert_eq!(active_target_jobs, vec![failed_job_id]);
    }

    async fn count_job_events(pool: &DbPool, bucket_date: &str, event_level: &str) -> i64 {
        sqlx::query_scalar::<_, i64>(
            r#"
            select count(*)::bigint
            from job_events
            where created_at >= $1::date
              and created_at < $1::date + interval '1 month'
              and event_level = $2
            "#,
        )
        .bind(bucket_date)
        .bind(event_level)
        .fetch_one(pool)
        .await
        .expect("count job_events")
    }

    async fn count_plugin_host_api_calls(
        pool: &DbPool,
        bucket_date: &str,
        status_code: i32,
    ) -> i64 {
        sqlx::query_scalar::<_, i64>(
            r#"
            select count(*)::bigint
            from plugin_host_api_calls
            where finished_at >= $1::date
              and finished_at < $1::date + interval '1 month'
              and status_code = $2
            "#,
        )
        .bind(bucket_date)
        .bind(status_code)
        .fetch_one(pool)
        .await
        .expect("count plugin_host_api_calls")
    }

    async fn count_scheduled_task_runs(pool: &DbPool, bucket_date: &str, status: &str) -> i64 {
        sqlx::query_scalar::<_, i64>(
            r#"
            select count(*)::bigint
            from scheduled_task_runs
            where started_at >= $1::date
              and started_at < $1::date + interval '1 month'
              and status = $2
            "#,
        )
        .bind(bucket_date)
        .bind(status)
        .fetch_one(pool)
        .await
        .expect("count scheduled_task_runs")
    }

    async fn count_job_runs(pool: &DbPool, bucket_date: &str, status: &str) -> i64 {
        sqlx::query_scalar::<_, i64>(
            r#"
            select count(*)::bigint
            from job_runs
            where started_at >= $1::date
              and started_at < $1::date + interval '1 month'
              and status = $2
            "#,
        )
        .bind(bucket_date)
        .bind(status)
        .fetch_one(pool)
        .await
        .expect("count job_runs")
    }

    async fn count_playback_sessions(pool: &DbPool, bucket_date: &str, active: bool) -> i64 {
        sqlx::query_scalar::<_, i64>(
            r#"
            select count(*)::bigint
            from playback_sessions
            where started_at >= $1::date
              and started_at < $1::date + interval '1 month'
              and (($2 and stopped_at is null) or (not $2 and stopped_at is not null))
            "#,
        )
        .bind(bucket_date)
        .bind(active)
        .fetch_one(pool)
        .await
        .expect("count playback_sessions")
    }

    async fn rollup_count(pool: &DbPool, bucket_date: &str, table_name: &str, status: &str) -> i64 {
        sqlx::query_scalar::<_, Option<i64>>(
            r#"
            select row_count
            from queue_stats_rollup
            where bucket_date = $1::date
              and table_name = $2
              and status = $3
            "#,
        )
        .bind(bucket_date)
        .bind(table_name)
        .bind(status)
        .fetch_optional(pool)
        .await
        .expect("read queue_stats_rollup")
        .flatten()
        .unwrap_or(0)
    }

    async fn add_month_date_text(pool: &DbPool, bucket_date: &str) -> String {
        sqlx::query_scalar::<_, String>("select (($1::date + interval '1 month')::date)::text")
            .bind(bucket_date)
            .fetch_one(pool)
            .await
            .expect("compute next month date")
    }
}
