use std::{sync::Arc, time::Duration};

use serde::{Serialize, Serializer};
use sqlx::{Row, postgres::PgRow};

use crate::{
    cache::RedisConnection,
    config::{Config, NodeRole},
    db::DbPool,
    realtime::SessionMessageHub,
};

#[derive(Clone)]
pub struct AppState {
    config: Arc<Config>,
    database: Option<DbPool>,
    redis: Option<RedisConnection>,
    session_hub: Arc<SessionMessageHub>,
    /// 管理端优雅退出触发器（Emby `System/Restart` / `System/Shutdown` 桥接）；
    /// 发送后 axum 优雅停机、workers 收尾，进程退出交给部署侧监管策略。
    shutdown_trigger: Option<tokio::sync::broadcast::Sender<()>>,
}

const READINESS_QUEUE_SUMMARY_SQL: &str = r#"
with readiness_sample_limit as (
    select 10000::bigint as lower_bound_count
),
job_queue_sample as (
    select 'queued'::text as status
    from (
        select 1
        from jobs
        where status = 'queued'
        limit 10001
    ) queued_jobs
    union all
    select 'running'::text as status
    from (
        select 1
        from jobs
        where status = 'running'
        limit 10001
    ) running_jobs
    union all
    select 'failed'::text as status
    from (
        select 1
        from jobs
        where status = 'failed'
        limit 10001
    ) failed_jobs
    union all
    select 'expired_lease'::text as status
    from (
        select 1
        from jobs
        where status = 'running'
          and locked_until <= now()
        limit 10001
    ) expired_job_leases
),
job_counts as (
    select least(
               count(*) filter (where status = 'queued'),
               (select lower_bound_count from readiness_sample_limit)
           )::bigint as queued,
           least(
               count(*) filter (where status = 'running'),
               (select lower_bound_count from readiness_sample_limit)
           )::bigint as running,
           least(
               count(*) filter (where status = 'failed'),
               (select lower_bound_count from readiness_sample_limit)
           )::bigint as failed,
           least(
               count(*) filter (where status = 'expired_lease'),
               (select lower_bound_count from readiness_sample_limit)
           )::bigint as expired_leases
    from job_queue_sample
),
event_outbox_sample as (
    select 'pending'::text as status
    from (
        select 1
        from event_outbox
        where status = 'pending'
        limit 10001
    ) pending_events
    union all
    select 'delivering'::text as status
    from (
        select 1
        from event_outbox
        where status = 'delivering'
        limit 10001
    ) delivering_events
    union all
    select 'failed'::text as status
    from (
        select 1
        from event_outbox
        where status = 'failed'
        limit 10001
    ) failed_events
    union all
    select 'expired_lease'::text as status
    from (
        select 1
        from event_outbox
        where status = 'delivering'
          and locked_until <= now()
        limit 10001
    ) expired_event_leases
),
event_counts as (
    select least(
               count(*) filter (where status = 'pending'),
               (select lower_bound_count from readiness_sample_limit)
           )::bigint as queued,
           least(
               count(*) filter (where status = 'delivering'),
               (select lower_bound_count from readiness_sample_limit)
           )::bigint as running,
           least(
               count(*) filter (where status = 'failed'),
               (select lower_bound_count from readiness_sample_limit)
           )::bigint as failed,
           least(
               count(*) filter (where status = 'expired_lease'),
               (select lower_bound_count from readiness_sample_limit)
           )::bigint as expired_leases
    from event_outbox_sample
),
transcode_queue_sample as (
    select 'queued'::text as status
    from (
        select 1
        from transcoding_sessions
        where status = 'queued'
        limit 10001
    ) queued_transcodes
    union all
    select 'running'::text as status
    from (
        select 1
        from transcoding_sessions
        where status = 'running'
        limit 10001
    ) running_transcodes
    union all
    select 'failed'::text as status
    from (
        select 1
        from transcoding_sessions
        where status = 'failed'
        limit 10001
    ) failed_transcodes
    union all
    select 'expired_lease'::text as status
    from (
        select 1
        from transcoding_sessions
        where status = 'running'
          and lease_expires_at <= now()
        limit 10001
    ) expired_transcode_leases
),
transcode_counts as (
    select least(
               count(*) filter (where status = 'queued'),
               (select lower_bound_count from readiness_sample_limit)
           )::bigint as queued,
           least(
               count(*) filter (where status = 'running'),
               (select lower_bound_count from readiness_sample_limit)
           )::bigint as running,
           least(
               count(*) filter (where status = 'failed'),
               (select lower_bound_count from readiness_sample_limit)
           )::bigint as failed,
           least(
               count(*) filter (where status = 'expired_lease'),
               (select lower_bound_count from readiness_sample_limit)
           )::bigint as expired_leases
    from transcode_queue_sample
),
notification_queue_sample as (
    select 'queued'::text as status
    from (
        select 1
        from plugin_notification_requests
        where status = 'queued'
        limit 10001
    ) queued_notifications
    union all
    select 'delivering'::text as status
    from (
        select 1
        from plugin_notification_requests
        where status = 'delivering'
        limit 10001
    ) delivering_notifications
    union all
    select 'failed'::text as status
    from (
        select 1
        from plugin_notification_requests
        where status = 'failed'
        limit 10001
    ) failed_notifications
),
notification_counts as (
    select least(
               count(*) filter (where status = 'queued'),
               (select lower_bound_count from readiness_sample_limit)
           )::bigint as queued,
           least(
               count(*) filter (where status = 'delivering'),
               (select lower_bound_count from readiness_sample_limit)
           )::bigint as running,
           least(
               count(*) filter (where status = 'failed'),
               (select lower_bound_count from readiness_sample_limit)
           )::bigint as failed,
           0::bigint as expired_leases
    from notification_queue_sample
),
event_stream_mirror_sample as (
    select 'unmirrored'::text as status,
           stream_mirror_attempts
    from (
        select stream_mirror_attempts
        from event_outbox
        where stream_mirrored_at is null
        limit 10001
    ) unmirrored_events
    union all
    select 'claimable'::text as status,
           stream_mirror_attempts
    from (
        select stream_mirror_attempts
        from event_outbox
        where stream_mirrored_at is null
          and (
              stream_mirror_locked_until is null
              or stream_mirror_locked_until <= now()
          )
        limit 10001
    ) claimable_events
    union all
    select 'locked'::text as status,
           stream_mirror_attempts
    from (
        select stream_mirror_attempts
        from event_outbox
        where stream_mirrored_at is null
          and stream_mirror_locked_by is not null
          and stream_mirror_locked_until > now()
        limit 10001
    ) locked_events
    union all
    select 'backoff'::text as status,
           stream_mirror_attempts
    from (
        select stream_mirror_attempts
        from event_outbox
        where stream_mirrored_at is null
          and stream_mirror_locked_by is null
          and stream_mirror_locked_until > now()
        limit 10001
    ) backoff_events
    union all
    select 'failed'::text as status,
           stream_mirror_attempts
    from (
        select stream_mirror_attempts
        from event_outbox
        where stream_mirrored_at is null
          and stream_mirror_last_error is not null
        limit 10001
    ) failed_mirror_events
    union all
    select 'expired_lease'::text as status,
           stream_mirror_attempts
    from (
        select stream_mirror_attempts
        from event_outbox
        where stream_mirrored_at is null
          and stream_mirror_locked_by is not null
          and stream_mirror_locked_until <= now()
        limit 10001
    ) expired_mirror_leases
),
event_stream_mirror_counts as (
    select least(
               count(*) filter (where status = 'unmirrored'),
               (select lower_bound_count from readiness_sample_limit)
           )::bigint as unmirrored,
           least(
               count(*) filter (where status = 'claimable'),
               (select lower_bound_count from readiness_sample_limit)
           )::bigint as claimable,
           least(
               count(*) filter (where status = 'locked'),
               (select lower_bound_count from readiness_sample_limit)
           )::bigint as locked,
           least(
               count(*) filter (where status = 'backoff'),
               (select lower_bound_count from readiness_sample_limit)
           )::bigint as backoff,
           least(
               count(*) filter (where status = 'failed'),
               (select lower_bound_count from readiness_sample_limit)
           )::bigint as failed,
           coalesce(
               max(stream_mirror_attempts) filter (where status = 'unmirrored'),
               0
           )::integer as max_attempts,
           least(
               count(*) filter (where status = 'expired_lease'),
               (select lower_bound_count from readiness_sample_limit)
           )::bigint as expired_leases
    from event_stream_mirror_sample
)
select job_counts.queued as jobs_queued,
       job_counts.running as jobs_running,
       job_counts.failed as jobs_failed,
       job_counts.expired_leases as jobs_expired_leases,
       event_counts.queued as event_outbox_queued,
       event_counts.running as event_outbox_running,
       event_counts.failed as event_outbox_failed,
       event_counts.expired_leases as event_outbox_expired_leases,
       transcode_counts.queued as transcodes_queued,
       transcode_counts.running as transcodes_running,
       transcode_counts.failed as transcodes_failed,
       transcode_counts.expired_leases as transcodes_expired_leases,
       notification_counts.queued as notifications_queued,
       notification_counts.running as notifications_running,
       notification_counts.failed as notifications_failed,
       notification_counts.expired_leases as notifications_expired_leases,
       event_stream_mirror_counts.unmirrored as event_stream_mirror_unmirrored,
       event_stream_mirror_counts.claimable as event_stream_mirror_claimable,
       event_stream_mirror_counts.locked as event_stream_mirror_locked,
       event_stream_mirror_counts.backoff as event_stream_mirror_backoff,
       event_stream_mirror_counts.failed as event_stream_mirror_failed,
       event_stream_mirror_counts.max_attempts as event_stream_mirror_max_attempts,
       event_stream_mirror_counts.expired_leases as event_stream_mirror_expired_leases
from job_counts
cross join event_counts
cross join transcode_counts
cross join notification_counts
cross join event_stream_mirror_counts
"#;

const READINESS_SCHEDULER_SUMMARY_SQL: &str = r#"
with readiness_scheduler_sample_limit as (
    select 10000::bigint as lower_bound_count
),
due_task_candidates as (
    select tasks.id,
           tasks.max_concurrency
    from scheduled_tasks tasks
    where tasks.enabled = true
      and tasks.schedule_kind in ('interval', 'cron')
      and tasks.next_run_at is not null
      and tasks.next_run_at <= now()
    order by tasks.next_run_at asc, tasks.id asc
    limit 10001
),
due_task_sample as (
    select 'due'::text as status
    from due_task_candidates tasks
    cross join lateral (
        select count(*)::bigint as active_running_runs
        from (
            select 1
            from scheduled_task_runs runs
            where runs.task_id = tasks.id
              and runs.status = 'running'
              and runs.lease_expires_at > now()
            limit least(
                tasks.max_concurrency::bigint,
                (select lower_bound_count + 1 from readiness_scheduler_sample_limit)
            )
        ) active_run_sample
    ) active_runs
    where active_running_runs <= (select lower_bound_count from readiness_scheduler_sample_limit)
      and active_running_runs < tasks.max_concurrency
    limit 10001
),
due_counts as (
    select least(
               count(*) filter (where status = 'due'),
               (select lower_bound_count from readiness_scheduler_sample_limit)
           )::bigint as due_tasks
    from due_task_sample
),
scheduler_run_sample as (
    select 'running'::text as status
    from (
        select 1
        from scheduled_task_runs
        where status = 'running'
          and lease_expires_at > now()
        order by lease_expires_at asc, id asc
        limit 10001
    ) running_runs
    union all
    select 'expired'::text as status
    from (
        select 1
        from scheduled_task_runs
        where status = 'running'
          and lease_expires_at <= now()
        order by lease_expires_at asc, id asc
        limit 10001
    ) expired_runs
    union all
    select 'manual_running'::text as status
    from (
        select 1
        from scheduled_task_runs
        where status = 'running'
          and lease_expires_at > now()
          and trigger_type = 'manual'
        order by lease_expires_at asc, id asc
        limit 10001
    ) manual_running_runs
),
run_counts as (
    select least(
               count(*) filter (
                   where status = 'running'
               ),
               (select lower_bound_count from readiness_scheduler_sample_limit)
           )::bigint as running_runs,
           least(
               count(*) filter (
                   where status = 'expired'
               ),
               (select lower_bound_count from readiness_scheduler_sample_limit)
           )::bigint as expired_runs,
           least(
               count(*) filter (
                   where status = 'manual_running'
               ),
               (select lower_bound_count from readiness_scheduler_sample_limit)
           )::bigint as manual_running_runs
    from scheduler_run_sample
)
select due_counts.due_tasks,
       run_counts.running_runs,
       run_counts.expired_runs,
       run_counts.manual_running_runs
from due_counts
cross join run_counts
"#;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ReadinessSnapshot {
    pub database: DependencyStatus,
    pub redis: DependencyStatus,
    pub runtime: RuntimeReadinessSnapshot,
}

impl ReadinessSnapshot {
    pub fn is_ready(&self) -> bool {
        self.database == DependencyStatus::Ok
            && self.redis == DependencyStatus::Ok
            && self.runtime.queues.status == DependencyStatus::Ok
            && self.runtime.scheduler.status == DependencyStatus::Ok
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct RuntimeReadinessSnapshot {
    pub roles: RoleReadinessSnapshot,
    pub workers: Vec<WorkerReadinessSnapshot>,
    pub queues: QueueReadinessSnapshot,
    pub scheduler: SchedulerReadinessSnapshot,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
pub struct RoleReadinessSnapshot {
    pub api: bool,
    pub worker: bool,
    pub scheduler: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
pub struct WorkerReadinessSnapshot {
    pub name: &'static str,
    pub enabled: bool,
    pub should_run: bool,
    pub interval_seconds: u64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
pub struct QueueReadinessSnapshot {
    pub status: DependencyStatus,
    pub jobs: QueueBacklogSnapshot,
    pub event_outbox: QueueBacklogSnapshot,
    pub transcodes: QueueBacklogSnapshot,
    pub notifications: QueueBacklogSnapshot,
    pub event_stream_mirror: EventStreamMirrorBacklogSnapshot,
    pub assigned_backlog: i64,
    pub has_assigned_backlog: bool,
    pub assigned_expired_leases: i64,
    pub has_assigned_expired_leases: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
pub struct SchedulerReadinessSnapshot {
    pub status: DependencyStatus,
    pub due_tasks: i64,
    pub running_runs: i64,
    pub expired_runs: i64,
    pub manual_running_runs: i64,
    pub drained_by_node: bool,
}

impl SchedulerReadinessSnapshot {
    fn not_configured() -> Self {
        Self {
            status: DependencyStatus::NotConfigured,
            due_tasks: 0,
            running_runs: 0,
            expired_runs: 0,
            manual_running_runs: 0,
            drained_by_node: false,
        }
    }

    fn unhealthy(config: &Config) -> Self {
        let mut snapshot = Self {
            status: DependencyStatus::Unhealthy,
            due_tasks: 0,
            running_runs: 0,
            expired_runs: 0,
            manual_running_runs: 0,
            drained_by_node: false,
        };
        annotate_scheduler_responsibility(&mut snapshot, config);
        snapshot
    }

    fn from_row(row: &PgRow, config: &Config) -> Result<Self, sqlx::Error> {
        let mut snapshot = Self {
            status: DependencyStatus::Ok,
            due_tasks: row.try_get("due_tasks")?,
            running_runs: row.try_get("running_runs")?,
            expired_runs: row.try_get("expired_runs")?,
            manual_running_runs: row.try_get("manual_running_runs")?,
            drained_by_node: false,
        };
        annotate_scheduler_responsibility(&mut snapshot, config);
        Ok(snapshot)
    }
}

impl QueueReadinessSnapshot {
    fn not_configured() -> Self {
        Self {
            status: DependencyStatus::NotConfigured,
            jobs: QueueBacklogSnapshot::default(),
            event_outbox: QueueBacklogSnapshot::default(),
            transcodes: QueueBacklogSnapshot::default(),
            notifications: QueueBacklogSnapshot::default(),
            event_stream_mirror: EventStreamMirrorBacklogSnapshot::default(),
            assigned_backlog: 0,
            has_assigned_backlog: false,
            assigned_expired_leases: 0,
            has_assigned_expired_leases: false,
        }
    }

    fn unhealthy() -> Self {
        Self {
            status: DependencyStatus::Unhealthy,
            jobs: QueueBacklogSnapshot::default(),
            event_outbox: QueueBacklogSnapshot::default(),
            transcodes: QueueBacklogSnapshot::default(),
            notifications: QueueBacklogSnapshot::default(),
            event_stream_mirror: EventStreamMirrorBacklogSnapshot::default(),
            assigned_backlog: 0,
            has_assigned_backlog: false,
            assigned_expired_leases: 0,
            has_assigned_expired_leases: false,
        }
    }

    fn from_row(row: &PgRow) -> Result<Self, sqlx::Error> {
        Ok(Self {
            status: DependencyStatus::Ok,
            jobs: QueueBacklogSnapshot {
                queued: row.try_get("jobs_queued")?,
                running: row.try_get("jobs_running")?,
                failed: row.try_get("jobs_failed")?,
                expired_leases: row.try_get("jobs_expired_leases")?,
                drained_by_node: false,
            },
            event_outbox: QueueBacklogSnapshot {
                queued: row.try_get("event_outbox_queued")?,
                running: row.try_get("event_outbox_running")?,
                failed: row.try_get("event_outbox_failed")?,
                expired_leases: row.try_get("event_outbox_expired_leases")?,
                drained_by_node: false,
            },
            transcodes: QueueBacklogSnapshot {
                queued: row.try_get("transcodes_queued")?,
                running: row.try_get("transcodes_running")?,
                failed: row.try_get("transcodes_failed")?,
                expired_leases: row.try_get("transcodes_expired_leases")?,
                drained_by_node: false,
            },
            notifications: QueueBacklogSnapshot {
                queued: row.try_get("notifications_queued")?,
                running: row.try_get("notifications_running")?,
                failed: row.try_get("notifications_failed")?,
                expired_leases: row.try_get("notifications_expired_leases")?,
                drained_by_node: false,
            },
            event_stream_mirror: EventStreamMirrorBacklogSnapshot {
                unmirrored: row.try_get("event_stream_mirror_unmirrored")?,
                claimable: row.try_get("event_stream_mirror_claimable")?,
                locked: row.try_get("event_stream_mirror_locked")?,
                backoff: row.try_get("event_stream_mirror_backoff")?,
                failed: row.try_get("event_stream_mirror_failed")?,
                max_attempts: row.try_get("event_stream_mirror_max_attempts")?,
                expired_leases: row.try_get("event_stream_mirror_expired_leases")?,
                drained_by_node: false,
            },
            assigned_backlog: 0,
            has_assigned_backlog: false,
            assigned_expired_leases: 0,
            has_assigned_expired_leases: false,
        })
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize)]
pub struct QueueBacklogSnapshot {
    pub queued: i64,
    pub running: i64,
    pub failed: i64,
    pub expired_leases: i64,
    /// Whether the current node runs a worker that drains this queue. The
    /// backlog counts are global (the queue lives in PostgreSQL), so this flag
    /// lets operators tell "backlog this node is responsible for" apart from
    /// "global backlog this node can see but does not drain" (e.g. an api-only
    /// node sees the job backlog but never processes it).
    pub drained_by_node: bool,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize)]
pub struct EventStreamMirrorBacklogSnapshot {
    pub unmirrored: i64,
    pub claimable: i64,
    pub locked: i64,
    pub backoff: i64,
    pub failed: i64,
    pub max_attempts: i32,
    pub expired_leases: i64,
    /// Whether this node runs the Redis Streams mirror worker (worker role with
    /// event streams enabled) and therefore drains this backlog.
    pub drained_by_node: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DependencyStatus {
    Ok,
    NotConfigured,
    Unhealthy,
}

impl DependencyStatus {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Ok => "ok",
            Self::NotConfigured => "not_configured",
            Self::Unhealthy => "unhealthy",
        }
    }
}

impl Serialize for DependencyStatus {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(self.as_str())
    }
}

impl AppState {
    pub fn new(config: Config, database: DbPool, redis: RedisConnection) -> Self {
        Self {
            config: Arc::new(config),
            database: Some(database),
            redis: Some(redis),
            session_hub: Arc::new(SessionMessageHub::default()),
            shutdown_trigger: None,
        }
    }

    /// 挂上进程优雅退出触发器（main 在构建 router 前调用）。
    pub fn with_shutdown_trigger(mut self, trigger: tokio::sync::broadcast::Sender<()>) -> Self {
        self.shutdown_trigger = Some(trigger);
        self
    }

    /// 请求进程优雅退出。返回是否成功触发（未接线或通道关闭返回 false）。
    pub fn trigger_shutdown(&self) -> bool {
        self.shutdown_trigger
            .as_ref()
            .is_some_and(|trigger| trigger.send(()).is_ok())
    }

    /// 进程内会话实时通道（Emby websocket 远程控制指令分发）。
    pub fn session_hub(&self) -> &SessionMessageHub {
        &self.session_hub
    }

    pub fn config(&self) -> &Config {
        &self.config
    }

    pub fn database_ready(&self) -> bool {
        self.database.is_some()
    }

    pub fn database(&self) -> Option<&DbPool> {
        self.database.as_ref()
    }

    pub fn redis_ready(&self) -> bool {
        self.redis.is_some()
    }

    pub async fn readiness(&self) -> ReadinessSnapshot {
        let (database, redis, queues, scheduler) = tokio::join!(
            self.check_database(),
            self.check_redis(),
            self.summarize_queues(),
            self.summarize_scheduler()
        );

        ReadinessSnapshot {
            database,
            redis,
            runtime: RuntimeReadinessSnapshot {
                roles: role_summary(&self.config),
                workers: worker_summaries(&self.config),
                queues,
                scheduler,
            },
        }
    }

    fn readiness_timeout(&self) -> Duration {
        Duration::from_millis(self.config.server.readiness_timeout_ms)
    }

    async fn check_database(&self) -> DependencyStatus {
        let Some(database) = &self.database else {
            return DependencyStatus::NotConfigured;
        };

        match tokio::time::timeout(
            self.readiness_timeout(),
            sqlx::query_scalar::<_, i64>("select 1::bigint").fetch_one(database),
        )
        .await
        {
            Ok(Ok(1)) => DependencyStatus::Ok,
            _ => DependencyStatus::Unhealthy,
        }
    }

    async fn check_redis(&self) -> DependencyStatus {
        let Some(redis) = &self.redis else {
            return DependencyStatus::NotConfigured;
        };
        let mut redis = redis.clone();

        match tokio::time::timeout(
            self.readiness_timeout(),
            crate::cache::ping(&mut redis, self.config.redis.operation_timeout_ms),
        )
        .await
        {
            Ok(Ok(value)) if value == "PONG" => DependencyStatus::Ok,
            _ => DependencyStatus::Unhealthy,
        }
    }

    async fn summarize_queues(&self) -> QueueReadinessSnapshot {
        let Some(database) = &self.database else {
            let mut snapshot = QueueReadinessSnapshot::not_configured();
            annotate_queue_responsibilities(&mut snapshot, &self.config);
            return snapshot;
        };

        let mut snapshot = match tokio::time::timeout(
            self.readiness_timeout(),
            sqlx::query(READINESS_QUEUE_SUMMARY_SQL).fetch_one(database),
        )
        .await
        {
            Ok(Ok(row)) => QueueReadinessSnapshot::from_row(&row)
                .unwrap_or_else(|_| QueueReadinessSnapshot::unhealthy()),
            _ => QueueReadinessSnapshot::unhealthy(),
        };
        annotate_queue_responsibilities(&mut snapshot, &self.config);
        snapshot
    }

    async fn summarize_scheduler(&self) -> SchedulerReadinessSnapshot {
        let Some(database) = &self.database else {
            let mut snapshot = SchedulerReadinessSnapshot::not_configured();
            annotate_scheduler_responsibility(&mut snapshot, &self.config);
            return snapshot;
        };

        match tokio::time::timeout(
            self.readiness_timeout(),
            sqlx::query(READINESS_SCHEDULER_SUMMARY_SQL).fetch_one(database),
        )
        .await
        {
            Ok(Ok(row)) => SchedulerReadinessSnapshot::from_row(&row, &self.config)
                .unwrap_or_else(|_| SchedulerReadinessSnapshot::unhealthy(&self.config)),
            _ => SchedulerReadinessSnapshot::unhealthy(&self.config),
        }
    }
}

/// Mark which readiness queues the current node actually drains, so the
/// per-node `/ready` summary surfaces role-relevant backlog. Every drainer
/// requires the worker role (api-only and scheduler-only nodes enqueue or only
/// serve, they do not consume these queues); each queue additionally requires
/// its specific worker to be enabled. Generic `jobs` is drained by the
/// scan/metadata/probe workers; `transcodes`, `notifications` and the event
/// stream mirror by their dedicated workers.
fn annotate_queue_responsibilities(snapshot: &mut QueueReadinessSnapshot, config: &Config) {
    let worker_role = matches!(&config.node.role, NodeRole::All | NodeRole::Worker);

    snapshot.jobs.drained_by_node = worker_role
        && (config.scan_worker.enabled
            || config.metadata_worker.enabled
            || config.probe_worker.enabled);
    snapshot.transcodes.drained_by_node = worker_role && config.transcode_worker.enabled;
    snapshot.notifications.drained_by_node = worker_role && config.notification_worker.enabled;

    // event_outbox is consumed by three workers: the plugin hook dispatcher
    // (plugins::execution), the notification delivery worker
    // (notifications::delivery), and the Redis Streams mirror (events). Any of
    // them, on a worker node, drains it.
    snapshot.event_outbox.drained_by_node = worker_role
        && (config.plugin_worker.enabled
            || config.notification_worker.enabled
            || config.redis.event_streams_enabled);
    // The mirror backlog (stream_mirrored_at is null) is specifically the Redis
    // Streams mirror worker's responsibility.
    snapshot.event_stream_mirror.drained_by_node =
        worker_role && config.redis.event_streams_enabled;
    snapshot.assigned_backlog = assigned_queue_backlog(snapshot);
    snapshot.has_assigned_backlog = snapshot.assigned_backlog > 0;
    snapshot.assigned_expired_leases =
        assigned_expired_leases(snapshot, event_outbox_dispatch_drained_by_node(config));
    snapshot.has_assigned_expired_leases = snapshot.assigned_expired_leases > 0;
}

fn assigned_queue_backlog(snapshot: &QueueReadinessSnapshot) -> i64 {
    let mut total = 0;
    for queue in [
        snapshot.jobs,
        snapshot.event_outbox,
        snapshot.transcodes,
        snapshot.notifications,
    ] {
        if queue.drained_by_node {
            total += queue.queued + queue.running + queue.failed;
        }
    }
    if snapshot.event_stream_mirror.drained_by_node {
        total += snapshot.event_stream_mirror.unmirrored;
    }
    total
}

fn assigned_expired_leases(
    snapshot: &QueueReadinessSnapshot,
    event_outbox_dispatch_drained_by_node: bool,
) -> i64 {
    let mut total = 0;
    for queue in [snapshot.jobs, snapshot.transcodes, snapshot.notifications] {
        if queue.drained_by_node {
            total += queue.expired_leases;
        }
    }
    if event_outbox_dispatch_drained_by_node {
        total += snapshot.event_outbox.expired_leases;
    }
    if snapshot.event_stream_mirror.drained_by_node {
        total += snapshot.event_stream_mirror.expired_leases;
    }
    total
}

fn event_outbox_dispatch_drained_by_node(config: &Config) -> bool {
    let worker_role = matches!(&config.node.role, NodeRole::All | NodeRole::Worker);
    worker_role && (config.plugin_worker.enabled || config.notification_worker.enabled)
}

fn annotate_scheduler_responsibility(snapshot: &mut SchedulerReadinessSnapshot, config: &Config) {
    let scheduler_role = matches!(&config.node.role, NodeRole::All | NodeRole::Scheduler);
    snapshot.drained_by_node = scheduler_role && config.scheduler.enabled;
}

fn role_summary(config: &Config) -> RoleReadinessSnapshot {
    RoleReadinessSnapshot {
        api: matches!(&config.node.role, NodeRole::All | NodeRole::Api),
        worker: matches!(&config.node.role, NodeRole::All | NodeRole::Worker),
        scheduler: matches!(&config.node.role, NodeRole::All | NodeRole::Scheduler),
    }
}

fn worker_summaries(config: &Config) -> Vec<WorkerReadinessSnapshot> {
    let worker_role = matches!(&config.node.role, NodeRole::All | NodeRole::Worker);
    let scheduler_role = matches!(&config.node.role, NodeRole::All | NodeRole::Scheduler);

    vec![
        WorkerReadinessSnapshot {
            name: "scan",
            enabled: config.scan_worker.enabled,
            should_run: config.scan_worker.enabled && worker_role,
            interval_seconds: config.scan_worker.interval_seconds,
        },
        WorkerReadinessSnapshot {
            name: "scheduler",
            enabled: config.scheduler.enabled,
            should_run: config.scheduler.enabled && scheduler_role,
            interval_seconds: config.scheduler.interval_seconds,
        },
        WorkerReadinessSnapshot {
            name: "plugin",
            enabled: config.plugin_worker.enabled,
            should_run: config.plugin_worker.enabled && worker_role,
            interval_seconds: config.plugin_worker.interval_seconds,
        },
        WorkerReadinessSnapshot {
            name: "transcode",
            enabled: config.transcode_worker.enabled,
            should_run: config.transcode_worker.enabled && worker_role,
            interval_seconds: config.transcode_worker.interval_seconds,
        },
        WorkerReadinessSnapshot {
            name: "probe",
            enabled: config.probe_worker.enabled,
            should_run: config.probe_worker.enabled && worker_role,
            interval_seconds: config.probe_worker.interval_seconds,
        },
        WorkerReadinessSnapshot {
            name: "metadata",
            enabled: config.metadata_worker.enabled,
            should_run: config.metadata_worker.enabled && worker_role,
            interval_seconds: config.metadata_worker.interval_seconds,
        },
        WorkerReadinessSnapshot {
            name: "notification",
            enabled: config.notification_worker.enabled,
            should_run: config.notification_worker.enabled && worker_role,
            interval_seconds: config.notification_worker.interval_seconds,
        },
        WorkerReadinessSnapshot {
            name: "event_stream_mirror",
            enabled: config.redis.event_streams_enabled,
            should_run: config.redis.event_streams_enabled && worker_role,
            interval_seconds: config.redis.event_stream_interval_seconds,
        },
    ]
}

#[cfg(test)]
impl AppState {
    pub fn for_tests(config: Config) -> Self {
        Self {
            config: Arc::new(config),
            database: None,
            redis: None,
            session_hub: Arc::new(SessionMessageHub::default()),
            shutdown_trigger: None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::{Config, NodeRole};

    #[tokio::test]
    async fn readiness_reports_unconfigured_dependencies_for_test_state() {
        let snapshot = AppState::for_tests(Config::default()).readiness().await;

        assert_eq!(snapshot.database, DependencyStatus::NotConfigured);
        assert_eq!(snapshot.redis, DependencyStatus::NotConfigured);
        assert_eq!(snapshot.runtime.queues.assigned_backlog, 0);
        assert!(!snapshot.runtime.queues.has_assigned_backlog);
        assert_eq!(
            snapshot.runtime.scheduler.status,
            DependencyStatus::NotConfigured
        );
        assert_eq!(snapshot.runtime.scheduler.due_tasks, 0);
        assert_eq!(snapshot.runtime.scheduler.running_runs, 0);
        assert!(!snapshot.runtime.scheduler.drained_by_node);
        assert!(!snapshot.is_ready());
    }

    #[tokio::test]
    async fn readiness_reports_configured_workers_for_current_node_role() {
        let mut config = Config::default();
        config.node.role = NodeRole::Worker;
        config.scan_worker.enabled = true;
        config.scheduler.enabled = true;
        config.plugin_worker.enabled = true;
        config.redis.event_streams_enabled = true;

        let snapshot = AppState::for_tests(config).readiness().await;

        assert_eq!(
            snapshot.runtime.roles,
            RoleReadinessSnapshot {
                api: false,
                worker: true,
                scheduler: false,
            }
        );

        let scan = snapshot
            .runtime
            .workers
            .iter()
            .find(|worker| worker.name == "scan")
            .expect("scan worker should be reported");
        assert!(scan.enabled);
        assert!(scan.should_run);

        let scheduler = snapshot
            .runtime
            .workers
            .iter()
            .find(|worker| worker.name == "scheduler")
            .expect("scheduler worker should be reported");
        assert!(scheduler.enabled);
        assert!(!scheduler.should_run);

        let event_stream_mirror = snapshot
            .runtime
            .workers
            .iter()
            .find(|worker| worker.name == "event_stream_mirror")
            .expect("event stream mirror worker should be reported");
        assert!(event_stream_mirror.enabled);
        assert!(event_stream_mirror.should_run);
    }

    #[tokio::test]
    async fn readiness_reports_runtime_roles_for_each_node_role() {
        let cases = [
            (
                NodeRole::All,
                RoleReadinessSnapshot {
                    api: true,
                    worker: true,
                    scheduler: true,
                },
            ),
            (
                NodeRole::Api,
                RoleReadinessSnapshot {
                    api: true,
                    worker: false,
                    scheduler: false,
                },
            ),
            (
                NodeRole::Worker,
                RoleReadinessSnapshot {
                    api: false,
                    worker: true,
                    scheduler: false,
                },
            ),
            (
                NodeRole::Scheduler,
                RoleReadinessSnapshot {
                    api: false,
                    worker: false,
                    scheduler: true,
                },
            ),
        ];

        for (role, expected) in cases {
            let mut config = Config::default();
            config.node.role = role;

            let snapshot = AppState::for_tests(config).readiness().await;

            assert_eq!(snapshot.runtime.roles, expected);
        }
    }

    #[tokio::test]
    async fn readiness_reports_scheduler_responsibility_by_node_role() {
        let mut scheduler = Config::default();
        scheduler.node.role = NodeRole::Scheduler;
        scheduler.scheduler.enabled = true;

        let summary = AppState::for_tests(scheduler)
            .readiness()
            .await
            .runtime
            .scheduler;
        assert_eq!(summary.status, DependencyStatus::NotConfigured);
        assert!(summary.drained_by_node);
        assert_eq!(summary.due_tasks, 0);
        assert_eq!(summary.running_runs, 0);
        assert_eq!(summary.expired_runs, 0);
        assert_eq!(summary.manual_running_runs, 0);

        let mut api = Config::default();
        api.node.role = NodeRole::Api;
        api.scheduler.enabled = true;

        let summary = AppState::for_tests(api).readiness().await.runtime.scheduler;
        assert!(!summary.drained_by_node);
    }

    #[tokio::test]
    async fn readiness_marks_queue_drain_responsibility_by_node_role() {
        // Worker node with the relevant workers enabled drains those queues.
        let mut worker = Config::default();
        worker.node.role = NodeRole::Worker;
        worker.scan_worker.enabled = true;
        worker.transcode_worker.enabled = true;
        worker.notification_worker.enabled = true;
        worker.redis.event_streams_enabled = true;

        let queues = AppState::for_tests(worker).readiness().await.runtime.queues;
        assert!(queues.jobs.drained_by_node);
        assert!(queues.transcodes.drained_by_node);
        assert!(queues.notifications.drained_by_node);
        assert!(queues.event_outbox.drained_by_node);
        assert!(queues.event_stream_mirror.drained_by_node);

        // Api-only node can see the global backlog but drains nothing locally,
        // even with the worker flags enabled.
        let mut api = Config::default();
        api.node.role = NodeRole::Api;
        api.scan_worker.enabled = true;
        api.transcode_worker.enabled = true;
        api.notification_worker.enabled = true;
        api.redis.event_streams_enabled = true;

        let queues = AppState::for_tests(api).readiness().await.runtime.queues;
        assert!(!queues.jobs.drained_by_node);
        assert!(!queues.transcodes.drained_by_node);
        assert!(!queues.notifications.drained_by_node);
        assert!(!queues.event_outbox.drained_by_node);
        assert!(!queues.event_stream_mirror.drained_by_node);
        assert_eq!(queues.assigned_backlog, 0);
        assert!(!queues.has_assigned_backlog);

        // Worker role but the specific workers disabled => queue not drained here.
        let mut idle_worker = Config::default();
        idle_worker.node.role = NodeRole::Worker;
        idle_worker.scan_worker.enabled = false;
        idle_worker.metadata_worker.enabled = false;
        idle_worker.probe_worker.enabled = false;
        idle_worker.transcode_worker.enabled = false;

        let queues = AppState::for_tests(idle_worker)
            .readiness()
            .await
            .runtime
            .queues;
        assert!(!queues.jobs.drained_by_node);
        assert!(!queues.transcodes.drained_by_node);

        // event_outbox is drained by any of its three consumers; a plugin-only
        // worker drains it (plugin hook dispatch) without draining the dedicated
        // mirror backlog (which needs event streams enabled).
        let mut plugin_only = Config::default();
        plugin_only.node.role = NodeRole::Worker;
        plugin_only.plugin_worker.enabled = true;

        let queues = AppState::for_tests(plugin_only)
            .readiness()
            .await
            .runtime
            .queues;
        assert!(queues.event_outbox.drained_by_node);
        assert!(!queues.event_stream_mirror.drained_by_node);
        assert!(!queues.jobs.drained_by_node);
        assert_eq!(queues.assigned_backlog, 0);
        assert!(!queues.has_assigned_backlog);
    }

    #[test]
    fn queue_readiness_summarizes_backlog_assigned_to_this_node() {
        let mut queues = QueueReadinessSnapshot {
            status: DependencyStatus::Ok,
            jobs: QueueBacklogSnapshot {
                queued: 2,
                running: 1,
                failed: 1,
                expired_leases: 1,
                drained_by_node: true,
            },
            event_outbox: QueueBacklogSnapshot {
                queued: 4,
                running: 0,
                failed: 1,
                expired_leases: 2,
                drained_by_node: false,
            },
            transcodes: QueueBacklogSnapshot {
                queued: 1,
                running: 1,
                failed: 0,
                expired_leases: 0,
                drained_by_node: true,
            },
            notifications: QueueBacklogSnapshot {
                queued: 9,
                running: 0,
                failed: 0,
                expired_leases: 3,
                drained_by_node: false,
            },
            event_stream_mirror: EventStreamMirrorBacklogSnapshot {
                unmirrored: 3,
                claimable: 2,
                locked: 1,
                backoff: 0,
                failed: 0,
                max_attempts: 2,
                expired_leases: 4,
                drained_by_node: true,
            },
            assigned_backlog: 0,
            has_assigned_backlog: false,
            assigned_expired_leases: 0,
            has_assigned_expired_leases: false,
        };

        queues.assigned_backlog = assigned_queue_backlog(&queues);
        queues.has_assigned_backlog = queues.assigned_backlog > 0;
        queues.assigned_expired_leases = assigned_expired_leases(&queues, false);
        queues.has_assigned_expired_leases = queues.assigned_expired_leases > 0;

        assert_eq!(queues.assigned_backlog, 9);
        assert!(queues.has_assigned_backlog);
        assert_eq!(queues.assigned_expired_leases, 5);
        assert!(queues.has_assigned_expired_leases);
    }

    #[test]
    fn queue_readiness_assigns_expired_leases_by_worker_kind() {
        let mut queues = QueueReadinessSnapshot::not_configured();
        queues.event_outbox.expired_leases = 7;
        queues.event_stream_mirror.expired_leases = 3;

        let mut mirror_only = Config::default();
        mirror_only.node.role = NodeRole::Worker;
        mirror_only.redis.event_streams_enabled = true;
        annotate_queue_responsibilities(&mut queues, &mirror_only);

        assert!(queues.event_outbox.drained_by_node);
        assert!(queues.event_stream_mirror.drained_by_node);
        assert_eq!(queues.assigned_expired_leases, 3);
        assert!(queues.has_assigned_expired_leases);
    }

    #[test]
    fn readiness_queue_summary_sql_uses_status_aggregates() {
        assert!(READINESS_QUEUE_SUMMARY_SQL.contains("from jobs"));
        assert!(READINESS_QUEUE_SUMMARY_SQL.contains("from event_outbox"));
        assert!(READINESS_QUEUE_SUMMARY_SQL.contains("from transcoding_sessions"));
        assert!(READINESS_QUEUE_SUMMARY_SQL.contains("from plugin_notification_requests"));
        assert!(READINESS_QUEUE_SUMMARY_SQL.contains("where stream_mirrored_at is null"));
        assert!(READINESS_QUEUE_SUMMARY_SQL.contains("event_stream_mirror_unmirrored"));
        assert!(READINESS_QUEUE_SUMMARY_SQL.contains("event_stream_mirror_claimable"));
        assert!(READINESS_QUEUE_SUMMARY_SQL.contains("event_stream_mirror_locked"));
        assert!(READINESS_QUEUE_SUMMARY_SQL.contains("event_stream_mirror_backoff"));
        assert!(READINESS_QUEUE_SUMMARY_SQL.contains("event_stream_mirror_failed"));
        assert!(READINESS_QUEUE_SUMMARY_SQL.contains("jobs_expired_leases"));
        assert!(READINESS_QUEUE_SUMMARY_SQL.contains("event_outbox_expired_leases"));
        assert!(READINESS_QUEUE_SUMMARY_SQL.contains("transcodes_expired_leases"));
        assert!(READINESS_QUEUE_SUMMARY_SQL.contains("event_stream_mirror_expired_leases"));
        assert!(READINESS_QUEUE_SUMMARY_SQL.contains("filter (where status = 'queued')"));
        assert!(READINESS_QUEUE_SUMMARY_SQL.contains("filter (where status = 'pending')"));
        assert!(READINESS_QUEUE_SUMMARY_SQL.contains("locked_until <= now()"));
        assert!(READINESS_QUEUE_SUMMARY_SQL.contains("lease_expires_at <= now()"));
        assert!(READINESS_QUEUE_SUMMARY_SQL.contains("stream_mirror_locked_until <= now()"));
        assert!(!READINESS_QUEUE_SUMMARY_SQL.contains("offset "));
    }

    #[test]
    fn readiness_queue_summary_sql_bounds_high_growth_counts() {
        assert!(READINESS_QUEUE_SUMMARY_SQL.contains("readiness_sample_limit"));
        assert!(READINESS_QUEUE_SUMMARY_SQL.contains("limit 10001"));

        for cte in [
            "job_queue_sample as",
            "event_outbox_sample as",
            "transcode_queue_sample as",
            "notification_queue_sample as",
            "event_stream_mirror_sample as",
        ] {
            assert!(
                READINESS_QUEUE_SUMMARY_SQL.contains(cte),
                "missing bounded readiness CTE: {cte}"
            );
        }

        let normalized = READINESS_QUEUE_SUMMARY_SQL
            .split_whitespace()
            .collect::<Vec<_>>()
            .join(" ")
            .to_ascii_lowercase();

        assert!(
            !normalized.contains("from jobs where status in ('queued', 'running', 'failed')"),
            "readiness should not exact-count the full jobs queue"
        );
        assert!(
            !normalized
                .contains("from event_outbox where status in ('pending', 'delivering', 'failed')"),
            "readiness should not exact-count the full event outbox"
        );
        assert!(
            !normalized.contains(
                "from transcoding_sessions where status in ('queued', 'running', 'failed')"
            ),
            "readiness should not exact-count the full transcode queue"
        );
        assert!(
            !normalized.contains(
                "from plugin_notification_requests where status in ('queued', 'delivering', 'failed')"
            ),
            "readiness should not exact-count the full notification queue"
        );
    }

    #[test]
    fn readiness_event_outbox_indexes_match_bounded_summary_shapes() {
        let migration = include_str!("../migrations/0070_readiness_event_outbox_indexes.sql");

        assert!(migration.contains("idx_event_outbox_readiness_status"));
        assert!(migration.contains("on event_outbox (status, locked_until, id)"));
        assert!(migration.contains("where status in ('pending', 'delivering', 'failed')"));
        assert!(migration.contains("idx_event_outbox_stream_mirror_failed"));
        assert!(migration.contains("on event_outbox (id)"));
        assert!(migration.contains("stream_mirrored_at is null"));
        assert!(migration.contains("stream_mirror_last_error is not null"));

        assert!(READINESS_QUEUE_SUMMARY_SQL.contains("where status = 'delivering'"));
        assert!(READINESS_QUEUE_SUMMARY_SQL.contains("and locked_until <= now()"));
        assert!(READINESS_QUEUE_SUMMARY_SQL.contains("stream_mirror_last_error is not null"));
    }

    #[test]
    fn readiness_scheduler_summary_sql_uses_due_and_lease_aggregates() {
        assert!(READINESS_SCHEDULER_SUMMARY_SQL.contains("from scheduled_tasks"));
        assert!(READINESS_SCHEDULER_SUMMARY_SQL.contains("from scheduled_task_runs"));
        assert!(READINESS_SCHEDULER_SUMMARY_SQL.contains("next_run_at <= now()"));
        assert!(READINESS_SCHEDULER_SUMMARY_SQL.contains("status = 'running'"));
        assert!(READINESS_SCHEDULER_SUMMARY_SQL.contains("lease_expires_at > now()"));
        assert!(READINESS_SCHEDULER_SUMMARY_SQL.contains("lease_expires_at <= now()"));
        assert!(READINESS_SCHEDULER_SUMMARY_SQL.contains("trigger_type = 'manual'"));
        assert!(READINESS_SCHEDULER_SUMMARY_SQL.contains("due_tasks"));
        assert!(READINESS_SCHEDULER_SUMMARY_SQL.contains("manual_running_runs"));
        assert!(!READINESS_SCHEDULER_SUMMARY_SQL.contains("offset "));
    }

    #[test]
    fn readiness_scheduler_summary_uses_bounded_lower_bound_samples() {
        assert!(READINESS_SCHEDULER_SUMMARY_SQL.contains("readiness_scheduler_sample_limit"));
        assert!(READINESS_SCHEDULER_SUMMARY_SQL.contains("limit 10001"));

        for cte in [
            "due_task_candidates",
            "due_task_sample",
            "active_run_sample",
            "scheduler_run_sample",
        ] {
            assert!(
                READINESS_SCHEDULER_SUMMARY_SQL.contains(cte),
                "missing bounded scheduler readiness CTE: {cte}"
            );
        }

        assert!(
            READINESS_SCHEDULER_SUMMARY_SQL
                .contains("active_running_runs <= (select lower_bound_count from readiness_scheduler_sample_limit)")
        );

        let normalized = READINESS_SCHEDULER_SUMMARY_SQL
            .split_whitespace()
            .collect::<Vec<_>>()
            .join(" ");
        assert!(
            !normalized.contains("select count(*)::bigint as due_tasks from scheduled_tasks"),
            "scheduler due readiness should not exact-count all due tasks"
        );
        assert!(
            !normalized.contains("run_counts as ( select count(*) filter"),
            "scheduler run readiness should not aggregate directly over all running task runs"
        );
    }

    #[test]
    fn readiness_scheduler_indexes_match_bounded_summary_shapes() {
        let migration = include_str!("../migrations/0072_readiness_scheduled_task_run_indexes.sql");

        assert!(migration.contains("idx_scheduled_task_runs_readiness_running_lease"));
        assert!(migration.contains("on scheduled_task_runs (lease_expires_at, id)"));
        assert!(migration.contains("where status = 'running'"));
        assert!(migration.contains("idx_scheduled_task_runs_readiness_manual_running_lease"));
        assert!(migration.contains("on scheduled_task_runs (trigger_type, lease_expires_at, id)"));

        assert!(READINESS_SCHEDULER_SUMMARY_SQL.contains("order by lease_expires_at asc, id asc"));
        assert!(READINESS_SCHEDULER_SUMMARY_SQL.contains("and trigger_type = 'manual'"));
    }

    // Live-DB smoke: validates the exact production readiness SQL parses, plans
    // and type-checks against the real migrated schema via EXPLAIN. Plain
    // EXPLAIN does not execute the SELECT, so it is non-mutating.
    //   cargo test -- --ignored readiness_queue_summary_executes_against_live_schema
    #[tokio::test]
    #[ignore = "requires a running PostgreSQL from ./scripts/dev-deps.ps1"]
    async fn readiness_queue_summary_executes_against_live_schema() {
        use sqlx::postgres::PgPoolOptions;

        let url = std::env::var("DATABASE_URL")
            .unwrap_or_else(|_| "postgres://fbz:fbz@127.0.0.1:5432/fbz".to_owned());
        let pool = PgPoolOptions::new()
            .max_connections(2)
            .connect(&url)
            .await
            .expect("connect to live PostgreSQL");
        crate::db::migrate(&pool).await.expect("run migrations");

        let plan_rows = sqlx::query(&format!("explain {READINESS_QUEUE_SUMMARY_SQL}"))
            .fetch_all(&pool)
            .await
            .expect("readiness SQL should parse and plan against the live schema");
        assert!(
            !plan_rows.is_empty(),
            "EXPLAIN should return a query plan for the readiness summary"
        );
    }

    // Live-DB smoke: validates the scheduler readiness SQL against the real
    // migrated schema without mutating scheduler state.
    //   cargo test -- --ignored readiness_scheduler_summary_executes_against_live_schema
    #[tokio::test]
    #[ignore = "requires a running PostgreSQL from ./scripts/dev-deps.ps1"]
    async fn readiness_scheduler_summary_executes_against_live_schema() {
        use sqlx::postgres::PgPoolOptions;

        let url = std::env::var("DATABASE_URL")
            .unwrap_or_else(|_| "postgres://fbz:fbz@127.0.0.1:5432/fbz".to_owned());
        let pool = PgPoolOptions::new()
            .max_connections(2)
            .connect(&url)
            .await
            .expect("connect to live PostgreSQL");
        crate::db::migrate(&pool).await.expect("run migrations");

        let plan_rows = sqlx::query(&format!("explain {READINESS_SCHEDULER_SUMMARY_SQL}"))
            .fetch_all(&pool)
            .await
            .expect("scheduler readiness SQL should parse and plan against the live schema");
        assert!(
            !plan_rows.is_empty(),
            "EXPLAIN should return a query plan for the scheduler readiness summary"
        );
    }
}
