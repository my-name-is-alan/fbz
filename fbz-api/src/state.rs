use std::sync::Arc;

use crate::{cache::RedisConnection, db::DbPool};

#[derive(Clone)]
pub struct AppState {
    config: Arc<crate::config::Config>,
    database: Option<DbPool>,
    redis: Option<RedisConnection>,
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
