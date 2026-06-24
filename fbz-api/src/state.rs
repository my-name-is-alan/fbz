use std::{sync::Arc, time::Duration};

use serde::{Serialize, Serializer};
use sqlx::{Row, postgres::PgRow};

use crate::{
    cache::RedisConnection,
    config::{Config, NodeRole},
    db::DbPool,
};

#[derive(Clone)]
pub struct AppState {
    config: Arc<Config>,
    database: Option<DbPool>,
    redis: Option<RedisConnection>,
}

const READINESS_QUEUE_SUMMARY_SQL: &str = r#"
with job_counts as (
    select count(*) filter (where status = 'queued')::bigint as queued,
           count(*) filter (where status = 'running')::bigint as running,
           count(*) filter (where status = 'failed')::bigint as failed
    from jobs
    where status in ('queued', 'running', 'failed')
),
event_counts as (
    select count(*) filter (where status = 'pending')::bigint as queued,
           count(*) filter (where status = 'delivering')::bigint as running,
           count(*) filter (where status = 'failed')::bigint as failed
    from event_outbox
    where status in ('pending', 'delivering', 'failed')
),
transcode_counts as (
    select count(*) filter (where status = 'queued')::bigint as queued,
           count(*) filter (where status = 'running')::bigint as running,
           count(*) filter (where status = 'failed')::bigint as failed
    from transcoding_sessions
    where status in ('queued', 'running', 'failed')
),
notification_counts as (
    select count(*) filter (where status = 'queued')::bigint as queued,
           count(*) filter (where status = 'delivering')::bigint as running,
           count(*) filter (where status = 'failed')::bigint as failed
    from plugin_notification_requests
    where status in ('queued', 'delivering', 'failed')
),
event_stream_mirror_counts as (
    select count(*)::bigint as unmirrored,
           count(*) filter (
               where stream_mirror_locked_until is null
                  or stream_mirror_locked_until <= now()
           )::bigint as claimable,
           count(*) filter (
               where stream_mirror_locked_by is not null
                 and stream_mirror_locked_until > now()
           )::bigint as locked,
           count(*) filter (
               where stream_mirror_locked_by is null
                 and stream_mirror_locked_until > now()
           )::bigint as backoff,
           count(*) filter (
               where stream_mirror_last_error is not null
           )::bigint as failed,
           coalesce(max(stream_mirror_attempts), 0)::integer as max_attempts
    from event_outbox
    where stream_mirrored_at is null
)
select job_counts.queued as jobs_queued,
       job_counts.running as jobs_running,
       job_counts.failed as jobs_failed,
       event_counts.queued as event_outbox_queued,
       event_counts.running as event_outbox_running,
       event_counts.failed as event_outbox_failed,
       transcode_counts.queued as transcodes_queued,
       transcode_counts.running as transcodes_running,
       transcode_counts.failed as transcodes_failed,
       notification_counts.queued as notifications_queued,
       notification_counts.running as notifications_running,
       notification_counts.failed as notifications_failed,
       event_stream_mirror_counts.unmirrored as event_stream_mirror_unmirrored,
       event_stream_mirror_counts.claimable as event_stream_mirror_claimable,
       event_stream_mirror_counts.locked as event_stream_mirror_locked,
       event_stream_mirror_counts.backoff as event_stream_mirror_backoff,
       event_stream_mirror_counts.failed as event_stream_mirror_failed,
       event_stream_mirror_counts.max_attempts as event_stream_mirror_max_attempts
from job_counts
cross join event_counts
cross join transcode_counts
cross join notification_counts
cross join event_stream_mirror_counts
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
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct RuntimeReadinessSnapshot {
    pub roles: RoleReadinessSnapshot,
    pub workers: Vec<WorkerReadinessSnapshot>,
    pub queues: QueueReadinessSnapshot,
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
        }
    }

    fn from_row(row: &PgRow) -> Result<Self, sqlx::Error> {
        Ok(Self {
            status: DependencyStatus::Ok,
            jobs: QueueBacklogSnapshot {
                queued: row.try_get("jobs_queued")?,
                running: row.try_get("jobs_running")?,
                failed: row.try_get("jobs_failed")?,
            },
            event_outbox: QueueBacklogSnapshot {
                queued: row.try_get("event_outbox_queued")?,
                running: row.try_get("event_outbox_running")?,
                failed: row.try_get("event_outbox_failed")?,
            },
            transcodes: QueueBacklogSnapshot {
                queued: row.try_get("transcodes_queued")?,
                running: row.try_get("transcodes_running")?,
                failed: row.try_get("transcodes_failed")?,
            },
            notifications: QueueBacklogSnapshot {
                queued: row.try_get("notifications_queued")?,
                running: row.try_get("notifications_running")?,
                failed: row.try_get("notifications_failed")?,
            },
            event_stream_mirror: EventStreamMirrorBacklogSnapshot {
                unmirrored: row.try_get("event_stream_mirror_unmirrored")?,
                claimable: row.try_get("event_stream_mirror_claimable")?,
                locked: row.try_get("event_stream_mirror_locked")?,
                backoff: row.try_get("event_stream_mirror_backoff")?,
                failed: row.try_get("event_stream_mirror_failed")?,
                max_attempts: row.try_get("event_stream_mirror_max_attempts")?,
            },
        })
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize)]
pub struct QueueBacklogSnapshot {
    pub queued: i64,
    pub running: i64,
    pub failed: i64,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize)]
pub struct EventStreamMirrorBacklogSnapshot {
    pub unmirrored: i64,
    pub claimable: i64,
    pub locked: i64,
    pub backoff: i64,
    pub failed: i64,
    pub max_attempts: i32,
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
        }
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
        let (database, redis, queues) = tokio::join!(
            self.check_database(),
            self.check_redis(),
            self.summarize_queues()
        );

        ReadinessSnapshot {
            database,
            redis,
            runtime: RuntimeReadinessSnapshot {
                roles: role_summary(&self.config),
                workers: worker_summaries(&self.config),
                queues,
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
            return QueueReadinessSnapshot::not_configured();
        };

        match tokio::time::timeout(
            self.readiness_timeout(),
            sqlx::query(READINESS_QUEUE_SUMMARY_SQL).fetch_one(database),
        )
        .await
        {
            Ok(Ok(row)) => QueueReadinessSnapshot::from_row(&row)
                .unwrap_or_else(|_| QueueReadinessSnapshot::unhealthy()),
            _ => QueueReadinessSnapshot::unhealthy(),
        }
    }
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
        assert!(READINESS_QUEUE_SUMMARY_SQL.contains("filter (where status = 'queued')"));
        assert!(READINESS_QUEUE_SUMMARY_SQL.contains("filter (where status = 'pending')"));
        assert!(!READINESS_QUEUE_SUMMARY_SQL.contains("offset "));
    }
}
