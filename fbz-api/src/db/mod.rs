use std::{str::FromStr, time::Duration};

use log::LevelFilter;
use sqlx::{
    ConnectOptions, PgPool,
    postgres::{PgConnectOptions, PgPoolOptions},
};

use crate::config::DatabaseConfig;

pub type DbPool = PgPool;

pub async fn connect(config: &DatabaseConfig) -> Result<DbPool, sqlx::Error> {
    let statement_timeout = format!("{}ms", config.statement_timeout_ms);
    let connect_options = PgConnectOptions::from_str(&config.url)?.log_slow_statements(
        LevelFilter::Warn,
        Duration::from_millis(config.slow_log_threshold_ms),
    );

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
        .connect_with(connect_options)
        .await
}

pub async fn migrate(pool: &DbPool) -> Result<(), sqlx::migrate::MigrateError> {
    sqlx::migrate!("./migrations").run(pool).await
}

#[cfg(test)]
mod tests {
    #[test]
    fn database_connect_configures_sqlx_slow_statement_logging() {
        let source = include_str!("mod.rs");
        let production_source = source
            .split("#[cfg(test)]")
            .next()
            .expect("db source should include production section");

        assert!(production_source.contains("PgConnectOptions"));
        assert!(production_source.contains("ConnectOptions"));
        assert!(production_source.contains("log_slow_statements"));
        assert!(production_source.contains("LevelFilter::Warn"));
        assert!(production_source.contains("slow_log_threshold_ms"));
        assert!(production_source.contains("connect_with"));
    }
}
