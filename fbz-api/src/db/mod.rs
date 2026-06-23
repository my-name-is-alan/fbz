use std::time::Duration;

use sqlx::{PgPool, postgres::PgPoolOptions};

use crate::config::DatabaseConfig;

pub type DbPool = PgPool;

pub async fn connect(config: &DatabaseConfig) -> Result<DbPool, sqlx::Error> {
    let statement_timeout = format!("{}ms", config.statement_timeout_ms);

    PgPoolOptions::new()
        .min_connections(config.min_connections)
        .max_connections(config.max_connections)
        .acquire_timeout(Duration::from_secs(config.acquire_timeout_seconds))
        .idle_timeout(Duration::from_secs(config.idle_timeout_seconds))
        .max_lifetime(Duration::from_secs(config.max_lifetime_seconds))
        .after_connect(move |connection, _metadata| {
            let statement_timeout = statement_timeout.clone();
            Box::pin(async move {
                sqlx::query("select set_config('statement_timeout', $1, false)")
                    .bind(statement_timeout)
                    .execute(connection)
                    .await?;
                Ok(())
            })
        })
        .connect(&config.url)
        .await
}

pub async fn migrate(pool: &DbPool) -> Result<(), sqlx::migrate::MigrateError> {
    sqlx::migrate!("./migrations").run(pool).await
}
