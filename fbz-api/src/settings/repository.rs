use serde_json::Value;
use sqlx::{Row, postgres::PgRow};

use crate::{db::DbPool, settings::SettingDefinition};

#[derive(Clone)]
pub struct SettingsRepository {
    pool: DbPool,
}

#[derive(Clone, Debug, PartialEq)]
pub struct StoredSetting {
    pub key: String,
    pub value: Value,
    pub requires_restart: bool,
    pub value_version: i64,
}

impl SettingsRepository {
    pub fn new(pool: DbPool) -> Self {
        Self { pool }
    }

    #[allow(dead_code)]
    pub async fn get(&self, key: &str) -> Result<Option<StoredSetting>, sqlx::Error> {
        sqlx::query(
            r#"
            select key, value, requires_restart, value_version
            from server_settings
            where key = $1
            "#,
        )
        .bind(key)
        .fetch_optional(&self.pool)
        .await?
        .map(StoredSetting::from_row)
        .transpose()
    }

    pub async fn list(&self) -> Result<Vec<StoredSetting>, sqlx::Error> {
        let rows = sqlx::query(
            r#"
            select key, value, requires_restart, value_version
            from server_settings
            order by key
            "#,
        )
        .fetch_all(&self.pool)
        .await?;

        rows.into_iter().map(StoredSetting::from_row).collect()
    }

    pub async fn insert_bootstrap_defaults(
        &self,
        settings: &[SettingDefinition],
    ) -> Result<(), sqlx::Error> {
        for setting in settings {
            sqlx::query(
                r#"
                insert into server_settings (key, value, requires_restart)
                values ($1, $2, $3)
                on conflict (key) do nothing
                "#,
            )
            .bind(setting.key)
            .bind(setting.value.clone())
            .bind(setting.requires_restart)
            .execute(&self.pool)
            .await?;
        }

        Ok(())
    }

    #[allow(dead_code)]
    pub async fn update_admin_setting(
        &self,
        key: &str,
        value: Value,
        changed_by: &str,
        change_reason: Option<&str>,
    ) -> Result<StoredSetting, sqlx::Error> {
        let mut tx = self.pool.begin().await?;
        let current = sqlx::query(
            r#"
            select value, value_version
            from server_settings
            where key = $1
            for update
            "#,
        )
        .bind(key)
        .fetch_optional(&mut *tx)
        .await?;

        let (old_value, next_version) = match current {
            Some(row) => {
                let version = row.try_get::<i64, _>("value_version")?;
                (Some(row.try_get::<Value, _>("value")?), version + 1)
            }
            None => (None, 1),
        };

        let setting = sqlx::query(
            r#"
            insert into server_settings (key, value, requires_restart, value_version)
            values ($1, $2, false, $3)
            on conflict (key) do update
                set value = excluded.value,
                    value_version = excluded.value_version,
                    updated_at = now()
            returning key, value, requires_restart, value_version
            "#,
        )
        .bind(key)
        .bind(value.clone())
        .bind(next_version)
        .fetch_one(&mut *tx)
        .await
        .and_then(StoredSetting::from_row)?;

        sqlx::query(
            r#"
            insert into server_setting_audit (
                setting_key,
                old_value,
                new_value,
                changed_by,
                change_reason,
                value_version
            )
            values ($1, $2, $3, $4, $5, $6)
            "#,
        )
        .bind(key)
        .bind(old_value)
        .bind(value)
        .bind(changed_by)
        .bind(change_reason)
        .bind(next_version)
        .execute(&mut *tx)
        .await?;

        tx.commit().await?;

        Ok(setting)
    }
}

impl StoredSetting {
    fn from_row(row: PgRow) -> Result<Self, sqlx::Error> {
        Ok(Self {
            key: row.try_get("key")?,
            value: row.try_get("value")?,
            requires_restart: row.try_get("requires_restart")?,
            value_version: row.try_get("value_version")?,
        })
    }
}
