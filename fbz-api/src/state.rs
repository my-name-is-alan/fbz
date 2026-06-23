use std::{sync::Arc, time::Duration};

use crate::{cache::RedisConnection, db::DbPool};

#[derive(Clone)]
pub struct AppState {
    config: Arc<crate::config::Config>,
    database: Option<DbPool>,
    redis: Option<RedisConnection>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ReadinessSnapshot {
    pub database: DependencyStatus,
    pub redis: DependencyStatus,
}

impl ReadinessSnapshot {
    pub fn is_ready(&self) -> bool {
        self.database == DependencyStatus::Ok && self.redis == DependencyStatus::Ok
    }
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

impl AppState {
    pub fn new(config: crate::config::Config, database: DbPool, redis: RedisConnection) -> Self {
        Self {
            config: Arc::new(config),
            database: Some(database),
            redis: Some(redis),
        }
    }

    pub fn config(&self) -> &crate::config::Config {
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
        let (database, redis) = tokio::join!(self.check_database(), self.check_redis());

        ReadinessSnapshot { database, redis }
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
}

#[cfg(test)]
impl AppState {
    pub fn for_tests(config: crate::config::Config) -> Self {
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
    use crate::config::Config;

    #[tokio::test]
    async fn readiness_reports_unconfigured_dependencies_for_test_state() {
        let snapshot = AppState::for_tests(Config::default()).readiness().await;

        assert_eq!(snapshot.database, DependencyStatus::NotConfigured);
        assert_eq!(snapshot.redis, DependencyStatus::NotConfigured);
        assert!(!snapshot.is_ready());
    }
}
