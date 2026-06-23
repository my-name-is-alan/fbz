use sqlx::{PgPool, postgres::PgPoolOptions};

use crate::config::DatabaseConfig;

pub type DbPool = PgPool;

pub async fn connect(config: &DatabaseConfig) -> Result<DbPool, sqlx::Error> {
    PgPoolOptions::new()
        .min_connections(config.min_connections)
        .max_connections(config.max_connections)
        .connect(&config.url)
        .await
}

pub async fn migrate(pool: &DbPool) -> Result<(), sqlx::migrate::MigrateError> {
    sqlx::migrate!("./migrations").run(pool).await
}
