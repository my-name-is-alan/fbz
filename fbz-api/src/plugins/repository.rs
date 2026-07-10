use std::{
    error::Error,
    fmt::{Display, Formatter},
};

use serde_json::Value;
use sqlx::{Postgres, QueryBuilder, Row, postgres::PgRow};

use crate::{
    db::DbPool,
    notifications::secrets::{SecretCipher, SecretError},
    plugins::manifest::{PluginConfigFieldManifest, PluginManifest, ValidatedPluginManifest},
    scheduler::{repository::PLUGIN_SCHEDULE_TASK_TYPE, service::parse_interval_seconds},
};

#[derive(Clone)]
pub struct PluginRepository {
    pool: DbPool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct InstallPluginPackageInput {
    pub package_path: String,
    pub checksum_sha256: Option<Vec<u8>>,
    pub signature: Option<String>,
    pub validated_manifest: ValidatedPluginManifest,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct InstalledPluginPackageRecord {
    pub package_id: String,
    pub plugin_id: String,
    pub package_version: String,
    pub package_status: String,
    pub approval_status: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PluginStateRecord {
    pub plugin_id: String,
    pub package_id: Option<String>,
    pub package_version: Option<String>,
    pub package_status: Option<String>,
    pub approval_status: String,
    pub enabled: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PluginSummaryRecord {
    pub installation_id: String,
    pub plugin_id: String,
    pub package_id: Option<String>,
    pub package_version: Option<String>,
    pub package_status: Option<String>,
    pub approval_status: String,
    pub enabled: bool,
    pub name: Option<String>,
    pub runtime: Option<String>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct PluginListFilter {
    pub approval_status: Option<String>,
    pub enabled: Option<bool>,
    pub runtime: Option<String>,
    pub cursor: Option<String>,
    pub limit: i64,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PluginListPage {
    pub records: Vec<PluginSummaryRecord>,
    pub next_cursor: Option<String>,
    pub has_more: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PluginPackageDetailRecord {
    pub package_id: String,
    pub plugin_id: String,
    pub package_version: String,
    pub api_version: String,
    pub runtime: String,
    pub name: String,
    pub description: Option<String>,
    pub entrypoint: String,
    pub package_path: String,
    pub package_status: String,
    pub signature_present: bool,
    pub approval_status: Option<String>,
    pub enabled: Option<bool>,
    pub permissions: Vec<PluginPermissionRecord>,
    pub hooks: Vec<PluginHookRecord>,
    pub menu: Vec<PluginMenuItemRecord>,
    pub schedules: Vec<PluginScheduleDefinitionRecord>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct PluginPackageListFilter {
    pub plugin_id: Option<String>,
    pub package_status: Option<String>,
    pub runtime: Option<String>,
    pub cursor: Option<String>,
    pub limit: i64,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PluginPackageListPage {
    pub records: Vec<PluginPackageSummaryRecord>,
    pub next_cursor: Option<String>,
    pub has_more: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PluginPackageSummaryRecord {
    pub package_id: String,
    pub plugin_id: String,
    pub package_version: String,
    pub api_version: String,
    pub runtime: String,
    pub name: String,
    pub package_status: String,
    pub signature_present: bool,
    pub approval_status: Option<String>,
    pub enabled: Option<bool>,
    pub active: bool,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PluginPackageLifecycleRule {
    pub install_preserves_active_package: bool,
    pub approve_switches_active_package: bool,
    pub activate_requires_approved_package: bool,
    pub reject_preserves_other_active_package: bool,
}

pub const PLUGIN_PACKAGE_LIFECYCLE_RULE: PluginPackageLifecycleRule = PluginPackageLifecycleRule {
    install_preserves_active_package: true,
    approve_switches_active_package: true,
    activate_requires_approved_package: true,
    reject_preserves_other_active_package: true,
};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PluginPermissionRecord {
    pub permission_key: String,
    pub permission_scope: Option<String>,
    pub reason: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PluginHookRecord {
    pub event_key: String,
    pub handler: String,
    pub priority: i32,
    pub enabled: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PluginMenuItemRecord {
    pub item_key: String,
    pub label: String,
    pub path: String,
    pub parent_key: Option<String>,
    pub required_permission: Option<String>,
    pub weight: i32,
    pub enabled: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ActivePluginMenuItemRecord {
    pub plugin_id: String,
    pub package_id: String,
    pub plugin_name: String,
    pub item_key: String,
    pub label: String,
    pub path: String,
    pub parent_key: Option<String>,
    pub required_permission: Option<String>,
    pub weight: i32,
}

#[derive(Clone, Debug, PartialEq)]
pub struct PluginConfigRecord {
    pub plugin_id: String,
    pub package_id: String,
    pub plugin_name: String,
    pub config_schema: Vec<PluginConfigFieldManifest>,
    pub values: Value,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct PluginConfigSecretUpdate {
    pub configured_keys: Vec<String>,
    pub retained_keys: Vec<String>,
    pub secrets: Vec<PluginConfigSecretInput>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PluginConfigSecretInput {
    pub key: String,
    pub value: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PluginScheduleDefinitionRecord {
    pub task_key: String,
    pub schedule_kind: String,
    pub schedule_value: String,
    pub handler: String,
    pub enabled_by_default: bool,
    pub timeout_seconds: i32,
}

#[derive(Debug)]
pub enum PluginStateError {
    PackageNotFound,
    PluginNotFound,
    InvalidState(String),
    Database(sqlx::Error),
}

#[derive(Debug)]
pub enum PluginConfigUpdateError {
    Database(sqlx::Error),
    Secret(SecretError),
    MissingRetainedSecret(String),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PluginMarketSourceRecord {
    pub id: String,
    pub name: String,
    pub url: String,
    pub enabled: bool,
    pub last_synced_at: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CreatePluginMarketSourceInput {
    pub name: String,
    pub url: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PluginMarketSourceSyncTarget {
    pub internal_id: i64,
    pub url: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NewPluginMarketEntry {
    pub plugin_id: String,
    pub name: String,
    pub version: String,
    pub description: Option<String>,
    pub author: Option<String>,
    pub permissions: Value,
    pub icon_url: Option<String>,
    pub download_url: String,
    pub checksum_sha256: Option<String>,
    pub signature: Option<String>,
    pub raw: Value,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PluginMarketEntryRecord {
    pub source_id: String,
    pub plugin_id: String,
    pub name: String,
    pub version: String,
    pub description: Option<String>,
    pub author: Option<String>,
    pub permissions: Value,
    pub icon_url: Option<String>,
    pub download_url: String,
    pub checksum_sha256: Option<String>,
    pub signature: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PluginMarketEntryInstallTarget {
    pub plugin_id: String,
    pub version: String,
    pub download_url: String,
    pub checksum_sha256: Option<String>,
    pub signature: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PluginUninstallRecord {
    pub plugin_id: String,
    pub package_paths: Vec<String>,
}

impl PluginRepository {
    pub fn new(pool: DbPool) -> Self {
        Self { pool }
    }

    pub async fn install_package(
        &self,
        input: InstallPluginPackageInput,
    ) -> Result<InstalledPluginPackageRecord, sqlx::Error> {
        let mut tx = self.pool.begin().await?;
        let manifest = input.validated_manifest.manifest;
        let manifest_value = manifest_to_json(&manifest)?;

        let package_row = sqlx::query(
            r#"
            insert into plugin_packages (
                plugin_id,
                package_version,
                api_version,
                runtime,
                name,
                description,
                entrypoint,
                package_path,
                manifest,
                manifest_hash,
                permission_fingerprint,
                checksum_sha256,
                signature,
                package_status
            )
            values ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, 'pending_approval')
            returning
                id,
                public_id::text as public_id,
                plugin_id,
                package_version,
                package_status
            "#,
        )
        .bind(manifest.id.trim())
        .bind(manifest.version.trim())
        .bind(manifest.api_version.trim())
        .bind(manifest.runtime.trim())
        .bind(manifest.name.trim())
        .bind(manifest.description.as_deref().map(str::trim))
        .bind(manifest.entrypoint.trim())
        .bind(input.package_path.trim())
        .bind(manifest_value)
        .bind(input.validated_manifest.manifest_hash)
        .bind(input.validated_manifest.permission_fingerprint)
        .bind(input.checksum_sha256)
        .bind(input.signature.as_deref().map(str::trim))
        .fetch_one(&mut *tx)
        .await?;

        let package_id = package_row.try_get::<i64, _>("id")?;
        let plugin_id = package_row.try_get::<String, _>("plugin_id")?;
        let package_public_id = package_row.try_get::<String, _>("public_id")?;
        let package_version = package_row.try_get::<String, _>("package_version")?;
        let package_status = package_row.try_get::<String, _>("package_status")?;

        ensure_plugin_installation(&mut tx, &plugin_id).await?;

        insert_permissions(&mut tx, package_id, &manifest).await?;
        insert_hooks(&mut tx, package_id, &manifest).await?;
        insert_menu_items(&mut tx, package_id, &manifest).await?;
        insert_schedule_definitions(&mut tx, package_id, &manifest).await?;

        tx.commit().await?;

        Ok(InstalledPluginPackageRecord {
            package_id: package_public_id,
            plugin_id,
            package_version,
            package_status,
            approval_status: "pending_approval".to_owned(),
        })
    }

    pub async fn list_plugins(&self, limit: i64) -> Result<Vec<PluginSummaryRecord>, sqlx::Error> {
        self.list_plugins_page(PluginListFilter {
            approval_status: None,
            enabled: None,
            runtime: None,
            cursor: None,
            limit,
        })
        .await
        .map(|page| page.records)
    }

    pub async fn list_plugins_page(
        &self,
        filter: PluginListFilter,
    ) -> Result<PluginListPage, sqlx::Error> {
        let fetch_limit = filter.limit.saturating_add(1);
        let mut query = QueryBuilder::<Postgres>::new(
            r#"
            select
                pi.public_id::text as installation_id,
                pi.plugin_id,
                pi.enabled,
                pi.approval_status,
                pkg.public_id::text as package_id,
                pkg.package_version,
                pkg.package_status,
                pkg.name,
                pkg.runtime
            from plugin_installations pi
            "#,
        );

        if let Some(cursor) = filter.cursor.as_deref() {
            query.push(
                r#"
                join plugin_installations cursor_installation
                  on cursor_installation.public_id = case
                      when
                "#,
            );
            query.push_bind(cursor);
            query.push(
                r#"::text ~* '^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$'
                      then
                "#,
            );
            query.push_bind(cursor);
            query.push(
                r#"::uuid
                      else null::uuid
                  end
                "#,
            );
        }

        query.push(
            r#"
            left join plugin_packages pkg on pkg.id = pi.active_package_id
            where true
            "#,
        );

        if let Some(approval_status) = filter.approval_status.as_deref() {
            query.push(" and pi.approval_status = ");
            query.push_bind(approval_status);
        }

        if let Some(enabled) = filter.enabled {
            query.push(" and pi.enabled = ");
            query.push_bind(enabled);
        }

        if let Some(runtime) = filter.runtime.as_deref() {
            query.push(" and pkg.runtime = ");
            query.push_bind(runtime);
        }

        if filter.cursor.is_some() {
            query.push(
                " and (pi.updated_at, pi.id) < (cursor_installation.updated_at, cursor_installation.id)",
            );
        }

        query.push(" order by pi.updated_at desc, pi.id desc limit ");
        query.push_bind(fetch_limit);

        let rows = query.build().fetch_all(&self.pool).await?;

        let has_more = rows.len() as i64 > filter.limit;
        let mut records = rows
            .into_iter()
            .map(PluginSummaryRecord::from_row)
            .collect::<Result<Vec<_>, _>>()?;
        if has_more {
            records.truncate(filter.limit as usize);
        }
        let next_cursor = has_more
            .then(|| records.last().map(|record| record.installation_id.clone()))
            .flatten();

        Ok(PluginListPage {
            records,
            next_cursor,
            has_more,
        })
    }

    pub async fn list_plugin_packages_page(
        &self,
        filter: PluginPackageListFilter,
    ) -> Result<PluginPackageListPage, sqlx::Error> {
        let fetch_limit = filter.limit.saturating_add(1);
        let mut query = QueryBuilder::<Postgres>::new(
            r#"
            select
                pkg.public_id::text as package_id,
                pkg.plugin_id,
                pkg.package_version,
                pkg.api_version,
                pkg.runtime,
                pkg.name,
                pkg.package_status,
                pkg.signature is not null as signature_present,
                pi.approval_status,
                pi.enabled,
                (pi.active_package_id = pkg.id) as active,
                pkg.created_at::text as created_at,
                pkg.updated_at::text as updated_at
            from plugin_packages pkg
            "#,
        );

        if let Some(cursor) = filter.cursor.as_deref() {
            query.push(
                r#"
                join plugin_packages cursor_package
                  on cursor_package.public_id = case
                      when
                "#,
            );
            query.push_bind(cursor);
            query.push(
                r#"::text ~* '^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$'
                      then
                "#,
            );
            query.push_bind(cursor);
            query.push(
                r#"::uuid
                      else null::uuid
                  end
                "#,
            );
        }

        query.push(
            r#"
            left join plugin_installations pi on pi.plugin_id = pkg.plugin_id
            where true
            "#,
        );

        if let Some(plugin_id) = filter.plugin_id.as_deref() {
            query.push(" and pkg.plugin_id = ");
            query.push_bind(plugin_id);
        }

        if let Some(package_status) = filter.package_status.as_deref() {
            query.push(" and pkg.package_status = ");
            query.push_bind(package_status);
        }

        if let Some(runtime) = filter.runtime.as_deref() {
            query.push(" and pkg.runtime = ");
            query.push_bind(runtime);
        }

        if filter.cursor.is_some() {
            query.push(
                " and (pkg.created_at, pkg.id) < (cursor_package.created_at, cursor_package.id)",
            );
        }

        query.push(" order by pkg.created_at desc, pkg.id desc limit ");
        query.push_bind(fetch_limit);

        let rows = query.build().fetch_all(&self.pool).await?;

        let has_more = rows.len() as i64 > filter.limit;
        let mut records = rows
            .into_iter()
            .map(PluginPackageSummaryRecord::from_row)
            .collect::<Result<Vec<_>, _>>()?;
        if has_more {
            records.truncate(filter.limit as usize);
        }
        let next_cursor = has_more
            .then(|| records.last().map(|record| record.package_id.clone()))
            .flatten();

        Ok(PluginPackageListPage {
            records,
            next_cursor,
            has_more,
        })
    }

    pub async fn get_package_detail(
        &self,
        package_id: &str,
    ) -> Result<Option<PluginPackageDetailRecord>, sqlx::Error> {
        let Some(base) = load_package_detail_base(&self.pool, package_id).await? else {
            return Ok(None);
        };

        let permissions = load_permissions(&self.pool, base.internal_id).await?;
        let hooks = load_hooks(&self.pool, base.internal_id).await?;
        let menu = load_menu_items(&self.pool, base.internal_id).await?;
        let schedules = load_schedule_definitions(&self.pool, base.internal_id).await?;

        Ok(Some(PluginPackageDetailRecord {
            package_id: base.package_id,
            plugin_id: base.plugin_id,
            package_version: base.package_version,
            api_version: base.api_version,
            runtime: base.runtime,
            name: base.name,
            description: base.description,
            entrypoint: base.entrypoint,
            package_path: base.package_path,
            package_status: base.package_status,
            signature_present: base.signature_present,
            approval_status: base.approval_status,
            enabled: base.enabled,
            permissions,
            hooks,
            menu,
            schedules,
        }))
    }

    pub async fn list_active_menu_items(
        &self,
    ) -> Result<Vec<ActivePluginMenuItemRecord>, sqlx::Error> {
        let rows = sqlx::query(
            r#"
            select
                pi.plugin_id,
                pkg.public_id::text as package_id,
                pkg.name as plugin_name,
                menu.item_key,
                menu.label,
                menu.path,
                menu.parent_key,
                menu.required_permission,
                menu.weight
            from plugin_menu_items menu
            join plugin_packages pkg on pkg.id = menu.package_id
            join plugin_installations pi on pi.active_package_id = pkg.id
            where menu.enabled = true
              and pi.enabled = true
              and pi.approval_status = 'approved'
              and pkg.package_status = 'approved'
              and exists (
                  select 1
                  from plugin_permissions permission
                  where permission.package_id = pkg.id
                    and permission.permission_key = 'admin.menu'
              )
            order by menu.weight asc, pi.plugin_id asc, menu.item_key asc
            "#,
        )
        .fetch_all(&self.pool)
        .await?;

        rows.into_iter()
            .map(ActivePluginMenuItemRecord::from_row)
            .collect()
    }

    pub async fn get_plugin_config(
        &self,
        plugin_id: &str,
    ) -> Result<Option<PluginConfigRecord>, sqlx::Error> {
        let Some(row) = load_plugin_config_row(&self.pool, plugin_id).await? else {
            return Ok(None);
        };

        PluginConfigRecord::from_row(row).map(Some)
    }

    pub async fn update_plugin_config(
        &self,
        plugin_id: &str,
        values: Value,
        secret_update: PluginConfigSecretUpdate,
        cipher: Option<&SecretCipher>,
    ) -> Result<Option<PluginConfigRecord>, PluginConfigUpdateError> {
        let plugin_id = plugin_id.trim();
        let mut tx = self
            .pool
            .begin()
            .await
            .map_err(PluginConfigUpdateError::Database)?;
        for retained_key in &secret_update.retained_keys {
            if !plugin_config_secret_exists(&mut tx, plugin_id, retained_key).await? {
                return Err(PluginConfigUpdateError::MissingRetainedSecret(
                    retained_key.clone(),
                ));
            }
        }

        let updated = sqlx::query(
            r#"
            update plugin_installations pi
            set config = $2,
                updated_at = now()
            from plugin_packages pkg
            where pi.active_package_id = pkg.id
              and pi.plugin_id = $1
              and pi.approval_status = 'approved'
              and pkg.package_status = 'approved'
            returning
                pi.plugin_id,
                pkg.public_id::text as package_id,
                pkg.name as plugin_name,
                pkg.manifest,
                pi.config
            "#,
        )
        .bind(plugin_id)
        .bind(values)
        .fetch_optional(&mut *tx)
        .await
        .map_err(PluginConfigUpdateError::Database)?;

        let Some(updated) = updated else {
            return Ok(None);
        };

        replace_plugin_config_secrets(&mut tx, plugin_id, secret_update, cipher).await?;
        let updated =
            PluginConfigRecord::from_row(updated).map_err(PluginConfigUpdateError::Database)?;
        tx.commit()
            .await
            .map_err(PluginConfigUpdateError::Database)?;

        Ok(Some(updated))
    }

    pub async fn approve_package(
        &self,
        package_id: &str,
        approved_by_user_id: i64,
    ) -> Result<PluginStateRecord, PluginStateError> {
        let mut tx = self
            .pool
            .begin()
            .await
            .map_err(PluginStateError::Database)?;
        let package = load_package_for_update(&mut tx, package_id).await?;

        if !matches!(
            package.package_status.as_str(),
            "pending_approval" | "approved"
        ) {
            return Err(PluginStateError::InvalidState(format!(
                "package status `{}` cannot be approved",
                package.package_status
            )));
        }

        sqlx::query(
            r#"
            update plugin_packages
            set package_status = 'approved',
                updated_at = now()
            where id = $1
            "#,
        )
        .bind(package.id)
        .execute(&mut *tx)
        .await
        .map_err(PluginStateError::Database)?;

        let updated = sqlx::query(
            r#"
            update plugin_installations
            set active_package_id = $1,
                enabled = false,
                approval_status = 'approved',
                permission_fingerprint = $2,
                approved_by = $3,
                approved_at = now(),
                disabled_at = now(),
                last_error = null,
                updated_at = now()
            where plugin_id = $4
            returning plugin_id
            "#,
        )
        .bind(package.id)
        .bind(&package.permission_fingerprint)
        .bind(approved_by_user_id)
        .bind(&package.plugin_id)
        .fetch_optional(&mut *tx)
        .await
        .map_err(PluginStateError::Database)?;

        if updated.is_some() {
            disable_plugin_scheduled_tasks(&mut tx, &package.plugin_id)
                .await
                .map_err(PluginStateError::Database)?;
        }
        let state = load_state_for_update(&mut tx, &package.plugin_id).await?;
        tx.commit().await.map_err(PluginStateError::Database)?;
        Ok(state)
    }

    pub async fn activate_package(
        &self,
        package_id: &str,
    ) -> Result<PluginStateRecord, PluginStateError> {
        let mut tx = self
            .pool
            .begin()
            .await
            .map_err(PluginStateError::Database)?;
        let package = load_package_for_update(&mut tx, package_id).await?;
        if package.package_status != "approved" {
            return Err(PluginStateError::InvalidState(format!(
                "package status `{}` cannot be activated",
                package.package_status
            )));
        }

        let current_state = load_state_for_update(&mut tx, &package.plugin_id).await?;
        if current_state.approval_status != "approved" {
            return Err(PluginStateError::InvalidState(
                "plugin must have an approved installation before package activation".to_owned(),
            ));
        }

        sqlx::query(
            r#"
            update plugin_installations
            set active_package_id = $1,
                permission_fingerprint = $2,
                approval_status = 'approved',
                last_error = null,
                updated_at = now()
            where plugin_id = $3
            "#,
        )
        .bind(package.id)
        .bind(&package.permission_fingerprint)
        .bind(&package.plugin_id)
        .execute(&mut *tx)
        .await
        .map_err(PluginStateError::Database)?;

        if current_state.enabled {
            sync_active_plugin_scheduled_tasks(&mut tx, &package.plugin_id).await?;
        } else {
            disable_plugin_scheduled_tasks(&mut tx, &package.plugin_id)
                .await
                .map_err(PluginStateError::Database)?;
        }

        let state = load_state_for_update(&mut tx, &package.plugin_id).await?;
        tx.commit().await.map_err(PluginStateError::Database)?;
        Ok(state)
    }

    pub async fn reject_package(
        &self,
        package_id: &str,
    ) -> Result<PluginStateRecord, PluginStateError> {
        let mut tx = self
            .pool
            .begin()
            .await
            .map_err(PluginStateError::Database)?;
        let package = load_package_for_update(&mut tx, package_id).await?;

        if package.package_status != "pending_approval" {
            return Err(PluginStateError::InvalidState(format!(
                "package status `{}` cannot be rejected",
                package.package_status
            )));
        }

        sqlx::query(
            r#"
            update plugin_packages
            set package_status = 'rejected',
                updated_at = now()
            where id = $1
            "#,
        )
        .bind(package.id)
        .execute(&mut *tx)
        .await
        .map_err(PluginStateError::Database)?;

        let updated = sqlx::query(
            r#"
            update plugin_installations
            set enabled = false,
                approval_status = 'rejected',
                approved_by = null,
                approved_at = null,
                disabled_at = now(),
                last_error = null,
                updated_at = now()
            where plugin_id = $1
              and active_package_id = $2
            returning plugin_id
            "#,
        )
        .bind(&package.plugin_id)
        .bind(package.id)
        .fetch_optional(&mut *tx)
        .await
        .map_err(PluginStateError::Database)?;

        if updated.is_none() {
            return Err(PluginStateError::PluginNotFound);
        }

        disable_plugin_scheduled_tasks(&mut tx, &package.plugin_id)
            .await
            .map_err(PluginStateError::Database)?;
        let state = load_state_for_update(&mut tx, &package.plugin_id).await?;
        tx.commit().await.map_err(PluginStateError::Database)?;
        Ok(state)
    }

    pub async fn enable_plugin(
        &self,
        plugin_id: &str,
    ) -> Result<PluginStateRecord, PluginStateError> {
        let mut tx = self
            .pool
            .begin()
            .await
            .map_err(PluginStateError::Database)?;
        let state = load_state_for_update(&mut tx, plugin_id).await?;
        if state.approval_status != "approved"
            || state.package_status.as_deref() != Some("approved")
        {
            return Err(PluginStateError::InvalidState(
                "plugin must be approved before it can be enabled".to_owned(),
            ));
        }

        sqlx::query(
            r#"
            update plugin_installations
            set enabled = true,
                disabled_at = null,
                last_error = null,
                updated_at = now()
            where plugin_id = $1
            "#,
        )
        .bind(plugin_id)
        .execute(&mut *tx)
        .await
        .map_err(PluginStateError::Database)?;

        sync_active_plugin_scheduled_tasks(&mut tx, plugin_id).await?;
        let state = load_state_for_update(&mut tx, plugin_id).await?;
        tx.commit().await.map_err(PluginStateError::Database)?;
        Ok(state)
    }

    pub async fn disable_plugin(
        &self,
        plugin_id: &str,
    ) -> Result<PluginStateRecord, PluginStateError> {
        let mut tx = self
            .pool
            .begin()
            .await
            .map_err(PluginStateError::Database)?;
        let _ = load_state_for_update(&mut tx, plugin_id).await?;

        sqlx::query(
            r#"
            update plugin_installations
            set enabled = false,
                disabled_at = now(),
                updated_at = now()
            where plugin_id = $1
            "#,
        )
        .bind(plugin_id)
        .execute(&mut *tx)
        .await
        .map_err(PluginStateError::Database)?;

        disable_plugin_scheduled_tasks(&mut tx, plugin_id)
            .await
            .map_err(PluginStateError::Database)?;
        let state = load_state_for_update(&mut tx, plugin_id).await?;
        tx.commit().await.map_err(PluginStateError::Database)?;
        Ok(state)
    }

    pub async fn list_market_sources(
        &self,
    ) -> Result<Vec<PluginMarketSourceRecord>, sqlx::Error> {
        let rows = sqlx::query(
            r#"
            select
                public_id::text as id,
                name,
                url,
                enabled,
                last_synced_at::text as last_synced_at
            from plugin_market_sources
            order by created_at desc, id desc
            "#,
        )
        .fetch_all(&self.pool)
        .await?;

        rows.into_iter()
            .map(PluginMarketSourceRecord::from_row)
            .collect()
    }

    pub async fn create_market_source(
        &self,
        input: CreatePluginMarketSourceInput,
    ) -> Result<PluginMarketSourceRecord, sqlx::Error> {
        let row = sqlx::query(
            r#"
            insert into plugin_market_sources (name, url)
            values ($1, $2)
            returning
                public_id::text as id,
                name,
                url,
                enabled,
                last_synced_at::text as last_synced_at
            "#,
        )
        .bind(input.name.trim())
        .bind(input.url.trim())
        .fetch_one(&self.pool)
        .await?;

        PluginMarketSourceRecord::from_row(row)
    }

    /// 启停市场源。停用的源保留缓存目录但从浏览/安装里隐藏（路由层过滤）。
    pub async fn set_market_source_enabled(
        &self,
        source_id: &str,
        enabled: bool,
    ) -> Result<Option<PluginMarketSourceRecord>, sqlx::Error> {
        let row = sqlx::query(
            r#"
            update plugin_market_sources
            set enabled = $2,
                updated_at = now()
            where public_id = case
                when $1::text ~* '^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$'
                then $1::uuid
                else null::uuid
            end
            returning
                public_id::text as id,
                name,
                url,
                enabled,
                last_synced_at::text as last_synced_at
            "#,
        )
        .bind(source_id)
        .bind(enabled)
        .fetch_optional(&self.pool)
        .await?;

        row.map(PluginMarketSourceRecord::from_row).transpose()
    }

    pub async fn delete_market_source(&self, source_id: &str) -> Result<bool, sqlx::Error> {
        let result = sqlx::query(
            r#"
            delete from plugin_market_sources
            where public_id = case
                when $1::text ~* '^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$'
                then $1::uuid
                else null::uuid
            end
            "#,
        )
        .bind(source_id)
        .execute(&self.pool)
        .await?;

        Ok(result.rows_affected() > 0)
    }

    pub async fn get_market_source_sync_target(
        &self,
        source_id: &str,
    ) -> Result<Option<PluginMarketSourceSyncTarget>, sqlx::Error> {
        let row = sqlx::query(
            r#"
            select id, url
            from plugin_market_sources
            where public_id = case
                when $1::text ~* '^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$'
                then $1::uuid
                else null::uuid
            end
            "#,
        )
        .bind(source_id)
        .fetch_optional(&self.pool)
        .await?;

        row.map(|row| {
            Ok(PluginMarketSourceSyncTarget {
                internal_id: row.try_get("id")?,
                url: row.try_get("url")?,
            })
        })
        .transpose()
    }

    /// Replace all cached catalog entries for a source and stamp `last_synced_at`.
    /// Runs in a single transaction so browsers never observe a partial catalog.
    pub async fn replace_market_entries(
        &self,
        source_internal_id: i64,
        entries: &[NewPluginMarketEntry],
    ) -> Result<i64, sqlx::Error> {
        let mut tx = self.pool.begin().await?;

        sqlx::query(
            r#"
            delete from plugin_market_entries
            where source_id = $1
            "#,
        )
        .bind(source_internal_id)
        .execute(&mut *tx)
        .await?;

        for entry in entries {
            sqlx::query(
                r#"
                insert into plugin_market_entries (
                    source_id,
                    plugin_id,
                    name,
                    version,
                    description,
                    author,
                    permissions,
                    icon_url,
                    download_url,
                    checksum_sha256,
                    signature,
                    raw
                )
                values ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12)
                "#,
            )
            .bind(source_internal_id)
            .bind(entry.plugin_id.trim())
            .bind(entry.name.trim())
            .bind(entry.version.trim())
            .bind(entry.description.as_deref().map(str::trim))
            .bind(entry.author.as_deref().map(str::trim))
            .bind(&entry.permissions)
            .bind(entry.icon_url.as_deref().map(str::trim))
            .bind(entry.download_url.trim())
            .bind(entry.checksum_sha256.as_deref().map(str::trim))
            .bind(entry.signature.as_deref().map(str::trim))
            .bind(&entry.raw)
            .execute(&mut *tx)
            .await?;
        }

        sqlx::query(
            r#"
            update plugin_market_sources
            set last_synced_at = now(),
                updated_at = now()
            where id = $1
            "#,
        )
        .bind(source_internal_id)
        .execute(&mut *tx)
        .await?;

        tx.commit().await?;

        Ok(entries.len() as i64)
    }

    pub async fn list_market_entries(
        &self,
        source_id: Option<&str>,
        query_text: Option<&str>,
    ) -> Result<Vec<PluginMarketEntryRecord>, sqlx::Error> {
        let mut query = QueryBuilder::<Postgres>::new(
            r#"
            select
                src.public_id::text as source_id,
                entry.plugin_id,
                entry.name,
                entry.version,
                entry.description,
                entry.author,
                entry.permissions,
                entry.icon_url,
                entry.download_url,
                entry.checksum_sha256,
                entry.signature
            from plugin_market_entries entry
            join plugin_market_sources src on src.id = entry.source_id
            where src.enabled = true
            "#,
        );

        if let Some(source_id) = source_id {
            query.push(
                r#"
                and src.public_id = case
                    when
                "#,
            );
            query.push_bind(source_id);
            query.push(
                r#"::text ~* '^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$'
                    then
                "#,
            );
            query.push_bind(source_id);
            query.push(
                r#"::uuid
                    else null::uuid
                end
                "#,
            );
        }

        if let Some(query_text) = query_text {
            let pattern = format!("%{}%", escape_like_pattern(query_text));
            query.push(" and (entry.name ilike ");
            query.push_bind(pattern.clone());
            query.push(" escape '\\' or entry.description ilike ");
            query.push_bind(pattern);
            query.push(" escape '\\')");
        }

        query.push(" order by entry.name asc, entry.plugin_id asc, entry.version desc");

        let rows = query.build().fetch_all(&self.pool).await?;
        rows.into_iter()
            .map(PluginMarketEntryRecord::from_row)
            .collect()
    }

    /// 已安装插件的活动包版本映射（市场目录标注"已安装/可升级"用）。
    pub async fn list_installed_plugin_versions(
        &self,
    ) -> Result<Vec<(String, Option<String>)>, sqlx::Error> {
        let rows = sqlx::query(
            r#"
            select
                pi.plugin_id,
                pkg.package_version
            from plugin_installations pi
            left join plugin_packages pkg on pkg.id = pi.active_package_id
            "#,
        )
        .fetch_all(&self.pool)
        .await?;

        rows.into_iter()
            .map(|row| {
                Ok((
                    row.try_get::<String, _>("plugin_id")?,
                    row.try_get::<Option<String>, _>("package_version")?,
                ))
            })
            .collect()
    }

    pub async fn get_market_entry_install_target(
        &self,
        source_id: &str,
        plugin_id: &str,
        version: &str,
    ) -> Result<Option<PluginMarketEntryInstallTarget>, sqlx::Error> {
        let row = sqlx::query(
            r#"
            select
                entry.plugin_id,
                entry.version,
                entry.download_url,
                entry.checksum_sha256,
                entry.signature
            from plugin_market_entries entry
            join plugin_market_sources src on src.id = entry.source_id
            where src.public_id = case
                when $1::text ~* '^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$'
                then $1::uuid
                else null::uuid
            end
              and src.enabled = true
              and entry.plugin_id = $2
              and entry.version = $3
            "#,
        )
        .bind(source_id)
        .bind(plugin_id.trim())
        .bind(version.trim())
        .fetch_optional(&self.pool)
        .await?;

        row.map(|row| {
            Ok(PluginMarketEntryInstallTarget {
                plugin_id: row.try_get("plugin_id")?,
                version: row.try_get("version")?,
                download_url: row.try_get("download_url")?,
                checksum_sha256: row.try_get("checksum_sha256")?,
                signature: row.try_get("signature")?,
            })
        })
        .transpose()
    }

    /// Uninstall a plugin: disable it, remove its installation + all packages
    /// (cascading permissions/hooks/menu/schedule definitions and ephemeral
    /// state), and drop any plugin-owned scheduled tasks. Dispatch/run audit
    /// history is preserved (its FKs were relaxed in migration 0089). Returns
    /// the on-disk package paths so callers can clean up the filesystem after
    /// the transaction commits. `Ok(None)` means the plugin does not exist.
    pub async fn uninstall_plugin(
        &self,
        plugin_id: &str,
    ) -> Result<Option<PluginUninstallRecord>, sqlx::Error> {
        let plugin_id = plugin_id.trim();
        let mut tx = self.pool.begin().await?;

        let installed = sqlx::query_scalar::<_, i64>(
            r#"
            select id
            from plugin_installations
            where plugin_id = $1
            for update
            "#,
        )
        .bind(plugin_id)
        .fetch_optional(&mut *tx)
        .await?;

        if installed.is_none() {
            tx.rollback().await?;
            return Ok(None);
        }

        // Disable first so no worker picks the plugin up mid-teardown.
        sqlx::query(
            r#"
            update plugin_installations
            set enabled = false,
                active_package_id = null,
                disabled_at = now(),
                updated_at = now()
            where plugin_id = $1
            "#,
        )
        .bind(plugin_id)
        .execute(&mut *tx)
        .await?;

        // Drop plugin-owned scheduled tasks (cascades scheduled_task_runs).
        sqlx::query(
            r#"
            delete from scheduled_tasks
            where owner_type = 'plugin'
              and owner_id = $1
            "#,
        )
        .bind(plugin_id)
        .execute(&mut *tx)
        .await?;

        // Collect on-disk package paths before deleting the rows.
        let path_rows = sqlx::query(
            r#"
            select package_path
            from plugin_packages
            where plugin_id = $1
            "#,
        )
        .bind(plugin_id)
        .fetch_all(&mut *tx)
        .await?;
        let mut package_paths = Vec::with_capacity(path_rows.len());
        for row in path_rows {
            package_paths.push(row.try_get::<String, _>("package_path")?);
        }

        // Now that active_package_id is cleared, packages can be deleted (the
        // installations.active_package_id FK is ON DELETE RESTRICT). This
        // cascades plugin_permissions/hooks/menu_items/schedule_definitions.
        sqlx::query(
            r#"
            delete from plugin_packages
            where plugin_id = $1
            "#,
        )
        .bind(plugin_id)
        .execute(&mut *tx)
        .await?;

        // Delete the installation last; cascades ephemeral state
        // (plugin_kv, plugin_config_secrets, plugin_host_tokens).
        sqlx::query(
            r#"
            delete from plugin_installations
            where plugin_id = $1
            "#,
        )
        .bind(plugin_id)
        .execute(&mut *tx)
        .await?;

        tx.commit().await?;

        Ok(Some(PluginUninstallRecord {
            plugin_id: plugin_id.to_owned(),
            package_paths,
        }))
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct PluginPackageDetailBase {
    internal_id: i64,
    package_id: String,
    plugin_id: String,
    package_version: String,
    api_version: String,
    runtime: String,
    name: String,
    description: Option<String>,
    entrypoint: String,
    package_path: String,
    package_status: String,
    signature_present: bool,
    approval_status: Option<String>,
    enabled: Option<bool>,
}

async fn load_package_detail_base(
    pool: &DbPool,
    package_id: &str,
) -> Result<Option<PluginPackageDetailBase>, sqlx::Error> {
    let row = sqlx::query(
        r#"
        select
            pkg.id,
            pkg.public_id::text as package_id,
            pkg.plugin_id,
            pkg.package_version,
            pkg.api_version,
            pkg.runtime,
            pkg.name,
            pkg.description,
            pkg.entrypoint,
            pkg.package_path,
            pkg.package_status,
            (pkg.signature is not null and length(trim(pkg.signature)) > 0) as signature_present,
            pi.approval_status,
            pi.enabled
        from plugin_packages pkg
        left join plugin_installations pi on pi.active_package_id = pkg.id
        where pkg.public_id = case
            when $1::text ~* '^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$'
            then $1::uuid
            else null::uuid
        end
        "#,
    )
    .bind(package_id)
    .fetch_optional(pool)
    .await?;

    row.map(PluginPackageDetailBase::from_row).transpose()
}

async fn load_permissions(
    pool: &DbPool,
    package_id: i64,
) -> Result<Vec<PluginPermissionRecord>, sqlx::Error> {
    let rows = sqlx::query(
        r#"
        select permission_key,
               permission_scope,
               reason
        from plugin_permissions
        where package_id = $1
        order by permission_key, permission_scope nulls first
        "#,
    )
    .bind(package_id)
    .fetch_all(pool)
    .await?;

    rows.into_iter()
        .map(PluginPermissionRecord::from_row)
        .collect()
}

async fn load_hooks(pool: &DbPool, package_id: i64) -> Result<Vec<PluginHookRecord>, sqlx::Error> {
    let rows = sqlx::query(
        r#"
        select event_key,
               handler,
               priority,
               enabled
        from plugin_hooks
        where package_id = $1
        order by event_key, priority desc, handler
        "#,
    )
    .bind(package_id)
    .fetch_all(pool)
    .await?;

    rows.into_iter().map(PluginHookRecord::from_row).collect()
}

async fn load_menu_items(
    pool: &DbPool,
    package_id: i64,
) -> Result<Vec<PluginMenuItemRecord>, sqlx::Error> {
    let rows = sqlx::query(
        r#"
        select item_key,
               label,
               path,
               parent_key,
               required_permission,
               weight,
               enabled
        from plugin_menu_items
        where package_id = $1
        order by weight, item_key
        "#,
    )
    .bind(package_id)
    .fetch_all(pool)
    .await?;

    rows.into_iter()
        .map(PluginMenuItemRecord::from_row)
        .collect()
}

async fn load_plugin_config_row(
    pool: &DbPool,
    plugin_id: &str,
) -> Result<Option<PgRow>, sqlx::Error> {
    sqlx::query(
        r#"
        select
            pi.plugin_id,
            pkg.public_id::text as package_id,
            pkg.name as plugin_name,
            pkg.manifest,
            pi.config
        from plugin_installations pi
        join plugin_packages pkg on pkg.id = pi.active_package_id
        where pi.plugin_id = $1
          and pi.approval_status = 'approved'
          and pkg.package_status = 'approved'
        "#,
    )
    .bind(plugin_id.trim())
    .fetch_optional(pool)
    .await
}

async fn plugin_config_secret_exists(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    plugin_id: &str,
    secret_key: &str,
) -> Result<bool, sqlx::Error> {
    sqlx::query_scalar(
        r#"
        select exists (
            select 1
            from plugin_config_secrets
            where plugin_id = $1
              and secret_key = $2
        )
        "#,
    )
    .bind(plugin_id.trim())
    .bind(secret_key.trim())
    .fetch_one(&mut **tx)
    .await
}

async fn replace_plugin_config_secrets(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    plugin_id: &str,
    secret_update: PluginConfigSecretUpdate,
    cipher: Option<&SecretCipher>,
) -> Result<(), PluginConfigUpdateError> {
    if secret_update.configured_keys.is_empty() {
        sqlx::query(
            r#"
            delete from plugin_config_secrets
            where plugin_id = $1
            "#,
        )
        .bind(plugin_id.trim())
        .execute(&mut **tx)
        .await?;
    } else {
        sqlx::query(
            r#"
            delete from plugin_config_secrets
            where plugin_id = $1
              and not (secret_key = any($2))
            "#,
        )
        .bind(plugin_id.trim())
        .bind(&secret_update.configured_keys)
        .execute(&mut **tx)
        .await?;
    }

    if secret_update.secrets.is_empty() {
        return Ok(());
    }
    let Some(cipher) = cipher else {
        return Err(PluginConfigUpdateError::Secret(SecretError::MissingKey));
    };

    for secret in secret_update.secrets {
        let encrypted = cipher
            .encrypt_scoped(
                "plugin-config",
                plugin_id.trim(),
                &secret.key,
                &secret.value,
            )
            .map_err(PluginConfigUpdateError::Secret)?;
        sqlx::query(
            r#"
            insert into plugin_config_secrets (
                plugin_id,
                secret_key,
                algorithm,
                nonce,
                ciphertext,
                value_hash
            )
            values ($1, $2, $3, $4, $5, $6)
            on conflict (plugin_id, secret_key) do update
                set algorithm = excluded.algorithm,
                    nonce = excluded.nonce,
                    ciphertext = excluded.ciphertext,
                    value_hash = excluded.value_hash,
                    updated_at = now()
            "#,
        )
        .bind(plugin_id.trim())
        .bind(secret.key.trim())
        .bind(encrypted.algorithm)
        .bind(encrypted.nonce)
        .bind(encrypted.ciphertext)
        .bind(encrypted.value_hash)
        .execute(&mut **tx)
        .await?;
    }

    Ok(())
}

async fn load_schedule_definitions(
    pool: &DbPool,
    package_id: i64,
) -> Result<Vec<PluginScheduleDefinitionRecord>, sqlx::Error> {
    let rows = sqlx::query(
        r#"
        select task_key,
               schedule_kind,
               schedule_value,
               handler,
               enabled_by_default,
               timeout_seconds
        from plugin_schedule_definitions
        where package_id = $1
        order by task_key
        "#,
    )
    .bind(package_id)
    .fetch_all(pool)
    .await?;

    rows.into_iter()
        .map(PluginScheduleDefinitionRecord::from_row)
        .collect()
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct PackageState {
    id: i64,
    plugin_id: String,
    package_status: String,
    permission_fingerprint: Vec<u8>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct PluginScheduleTaskDefinition {
    task_key: String,
    schedule_kind: String,
    schedule_value: String,
    enabled_by_default: bool,
    timeout_seconds: i32,
}

async fn load_package_for_update(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    package_id: &str,
) -> Result<PackageState, PluginStateError> {
    let row = sqlx::query(
        r#"
        select id,
               plugin_id,
               package_status,
               permission_fingerprint
        from plugin_packages
        where public_id = case
            when $1::text ~* '^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$'
            then $1::uuid
            else null::uuid
        end
        for update
        "#,
    )
    .bind(package_id)
    .fetch_optional(&mut **tx)
    .await
    .map_err(PluginStateError::Database)?
    .ok_or(PluginStateError::PackageNotFound)?;

    Ok(PackageState {
        id: row.try_get("id").map_err(PluginStateError::Database)?,
        plugin_id: row
            .try_get("plugin_id")
            .map_err(PluginStateError::Database)?,
        package_status: row
            .try_get("package_status")
            .map_err(PluginStateError::Database)?,
        permission_fingerprint: row
            .try_get("permission_fingerprint")
            .map_err(PluginStateError::Database)?,
    })
}

async fn ensure_plugin_installation(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    plugin_id: &str,
) -> Result<(), sqlx::Error> {
    sqlx::query(
        r#"
        insert into plugin_installations (
            plugin_id,
            active_package_id,
            enabled,
            approval_status,
            permission_fingerprint
        )
        values ($1, null, false, 'pending_approval', decode(repeat('00', 32), 'hex'))
        on conflict (plugin_id) do update
            set last_error = null,
                updated_at = now()
        "#,
    )
    .bind(plugin_id.trim())
    .execute(&mut **tx)
    .await?;

    Ok(())
}

async fn load_state_for_update(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    plugin_id: &str,
) -> Result<PluginStateRecord, PluginStateError> {
    let row = sqlx::query(
        r#"
        select
            pi.plugin_id,
            pi.enabled,
            pi.approval_status,
            pkg.public_id::text as package_id,
            pkg.package_version,
            pkg.package_status
        from plugin_installations pi
        left join plugin_packages pkg on pkg.id = pi.active_package_id
        where pi.plugin_id = $1
        for update of pi
        "#,
    )
    .bind(plugin_id)
    .fetch_optional(&mut **tx)
    .await
    .map_err(PluginStateError::Database)?
    .ok_or(PluginStateError::PluginNotFound)?;

    PluginStateRecord::from_row(row).map_err(PluginStateError::Database)
}

async fn disable_plugin_scheduled_tasks(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    plugin_id: &str,
) -> Result<(), sqlx::Error> {
    sqlx::query(
        r#"
        update scheduled_tasks
        set enabled = false,
            next_run_at = null,
            updated_at = now()
        where owner_type = 'plugin'
          and owner_id = $1
        "#,
    )
    .bind(plugin_id.trim())
    .execute(&mut **tx)
    .await?;

    Ok(())
}

async fn sync_active_plugin_scheduled_tasks(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    plugin_id: &str,
) -> Result<(), PluginStateError> {
    disable_plugin_scheduled_tasks(tx, plugin_id)
        .await
        .map_err(PluginStateError::Database)?;

    let rows = sqlx::query(
        r#"
        select psd.task_key,
               psd.schedule_kind,
               psd.schedule_value,
               psd.enabled_by_default,
               psd.timeout_seconds
        from plugin_installations pi
        join plugin_schedule_definitions psd
          on psd.package_id = pi.active_package_id
        where pi.plugin_id = $1
        order by psd.task_key
        "#,
    )
    .bind(plugin_id.trim())
    .fetch_all(&mut **tx)
    .await
    .map_err(PluginStateError::Database)?;

    for row in rows {
        let schedule =
            PluginScheduleTaskDefinition::from_row(row).map_err(PluginStateError::Database)?;
        let next_run_delay_seconds = match schedule.schedule_kind.as_str() {
            "interval" if schedule.enabled_by_default => Some(
                parse_interval_seconds(&schedule.schedule_value)
                    .map_err(|err| PluginStateError::InvalidState(err.to_string()))?
                    as i64,
            ),
            "interval" | "cron" => None,
            other => {
                return Err(PluginStateError::InvalidState(format!(
                    "unsupported plugin schedule kind `{other}`"
                )));
            }
        };

        sqlx::query(
            r#"
            insert into scheduled_tasks (
                task_key,
                task_type,
                owner_type,
                owner_id,
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
                'plugin',
                $3,
                $4,
                $5,
                $6,
                case
                    when $4 and $5 = 'interval' then now() + ($7::bigint * interval '1 second')
                    when $4 and $5 = 'cron' then fbz_next_cron_run_at($6, now())
                    else null
                end,
                $8,
                1
            )
            on conflict (task_key) do update
                set task_type = excluded.task_type,
                    owner_type = excluded.owner_type,
                    owner_id = excluded.owner_id,
                    enabled = excluded.enabled,
                    schedule_kind = excluded.schedule_kind,
                    schedule_value = excluded.schedule_value,
                    next_run_at = case
                        when excluded.enabled = false then null
                        when excluded.schedule_kind = 'cron'
                             and scheduled_tasks.enabled = true
                             and scheduled_tasks.next_run_at is not null
                             and scheduled_tasks.schedule_value = excluded.schedule_value then scheduled_tasks.next_run_at
                        when excluded.schedule_kind = 'cron' then excluded.next_run_at
                        when excluded.schedule_kind <> 'interval' then null
                        when scheduled_tasks.enabled = false then excluded.next_run_at
                        when scheduled_tasks.next_run_at is null then excluded.next_run_at
                        when scheduled_tasks.schedule_value <> excluded.schedule_value then excluded.next_run_at
                        else scheduled_tasks.next_run_at
                    end,
                    timeout_seconds = excluded.timeout_seconds,
                    max_concurrency = excluded.max_concurrency,
                    last_error = null,
                    updated_at = now()
            "#,
        )
        .bind(schedule.task_key.trim())
        .bind(PLUGIN_SCHEDULE_TASK_TYPE)
        .bind(plugin_id.trim())
        .bind(schedule.enabled_by_default)
        .bind(schedule.schedule_kind.trim())
        .bind(schedule.schedule_value.trim())
        .bind(next_run_delay_seconds)
        .bind(schedule.timeout_seconds)
        .execute(&mut **tx)
        .await
        .map_err(PluginStateError::Database)?;
    }

    Ok(())
}

async fn insert_permissions(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    package_id: i64,
    manifest: &PluginManifest,
) -> Result<(), sqlx::Error> {
    for permission in &manifest.permissions {
        sqlx::query(
            r#"
            insert into plugin_permissions (
                package_id,
                permission_key,
                permission_scope,
                reason
            )
            values ($1, $2, $3, $4)
            "#,
        )
        .bind(package_id)
        .bind(permission.key.trim())
        .bind(permission.scope.as_deref().map(str::trim))
        .bind(permission.reason.as_deref().map(str::trim))
        .execute(&mut **tx)
        .await?;
    }

    Ok(())
}

async fn insert_hooks(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    package_id: i64,
    manifest: &PluginManifest,
) -> Result<(), sqlx::Error> {
    for hook in &manifest.hooks {
        sqlx::query(
            r#"
            insert into plugin_hooks (
                package_id,
                event_key,
                handler,
                priority
            )
            values ($1, $2, $3, $4)
            "#,
        )
        .bind(package_id)
        .bind(hook.event.trim())
        .bind(hook.handler.trim())
        .bind(i32::from(hook.priority))
        .execute(&mut **tx)
        .await?;
    }

    Ok(())
}

async fn insert_menu_items(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    package_id: i64,
    manifest: &PluginManifest,
) -> Result<(), sqlx::Error> {
    for item in &manifest.menu {
        sqlx::query(
            r#"
            insert into plugin_menu_items (
                package_id,
                item_key,
                label,
                path,
                parent_key,
                required_permission,
                weight
            )
            values ($1, $2, $3, $4, $5, $6, $7)
            "#,
        )
        .bind(package_id)
        .bind(item.key.trim())
        .bind(item.label.trim())
        .bind(item.path.trim())
        .bind(item.parent_key.as_deref().map(str::trim))
        .bind(item.required_permission.as_deref().map(str::trim))
        .bind(i32::from(item.weight))
        .execute(&mut **tx)
        .await?;
    }

    Ok(())
}

async fn insert_schedule_definitions(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    package_id: i64,
    manifest: &PluginManifest,
) -> Result<(), sqlx::Error> {
    for schedule in &manifest.schedules {
        sqlx::query(
            r#"
            insert into plugin_schedule_definitions (
                package_id,
                task_key,
                schedule_kind,
                schedule_value,
                handler,
                enabled_by_default
            )
            values ($1, $2, $3, $4, $5, $6)
            "#,
        )
        .bind(package_id)
        .bind(schedule.key.trim())
        .bind(schedule.schedule_kind.trim())
        .bind(schedule.schedule_value.trim())
        .bind(schedule.handler.trim())
        .bind(schedule.enabled_by_default)
        .execute(&mut **tx)
        .await?;
    }

    Ok(())
}

fn manifest_to_json(manifest: &PluginManifest) -> Result<Value, sqlx::Error> {
    serde_json::to_value(manifest).map_err(|err| sqlx::Error::Encode(Box::new(err)))
}

impl InstalledPluginPackageRecord {
    pub fn from_row(row: PgRow) -> Result<Self, sqlx::Error> {
        Ok(Self {
            package_id: row.try_get("package_id")?,
            plugin_id: row.try_get("plugin_id")?,
            package_version: row.try_get("package_version")?,
            package_status: row.try_get("package_status")?,
            approval_status: row.try_get("approval_status")?,
        })
    }
}

impl PluginStateRecord {
    fn from_row(row: PgRow) -> Result<Self, sqlx::Error> {
        Ok(Self {
            plugin_id: row.try_get("plugin_id")?,
            package_id: row.try_get("package_id")?,
            package_version: row.try_get("package_version")?,
            package_status: row.try_get("package_status")?,
            approval_status: row.try_get("approval_status")?,
            enabled: row.try_get("enabled")?,
        })
    }
}

impl PluginSummaryRecord {
    fn from_row(row: PgRow) -> Result<Self, sqlx::Error> {
        Ok(Self {
            installation_id: row.try_get("installation_id")?,
            plugin_id: row.try_get("plugin_id")?,
            package_id: row.try_get("package_id")?,
            package_version: row.try_get("package_version")?,
            package_status: row.try_get("package_status")?,
            approval_status: row.try_get("approval_status")?,
            enabled: row.try_get("enabled")?,
            name: row.try_get("name")?,
            runtime: row.try_get("runtime")?,
        })
    }
}

impl PluginPackageSummaryRecord {
    fn from_row(row: PgRow) -> Result<Self, sqlx::Error> {
        Ok(Self {
            package_id: row.try_get("package_id")?,
            plugin_id: row.try_get("plugin_id")?,
            package_version: row.try_get("package_version")?,
            api_version: row.try_get("api_version")?,
            runtime: row.try_get("runtime")?,
            name: row.try_get("name")?,
            package_status: row.try_get("package_status")?,
            signature_present: row.try_get("signature_present")?,
            approval_status: row.try_get("approval_status")?,
            enabled: row.try_get("enabled")?,
            active: row.try_get("active")?,
            created_at: row.try_get("created_at")?,
            updated_at: row.try_get("updated_at")?,
        })
    }
}

impl PluginPackageDetailBase {
    fn from_row(row: PgRow) -> Result<Self, sqlx::Error> {
        Ok(Self {
            internal_id: row.try_get("id")?,
            package_id: row.try_get("package_id")?,
            plugin_id: row.try_get("plugin_id")?,
            package_version: row.try_get("package_version")?,
            api_version: row.try_get("api_version")?,
            runtime: row.try_get("runtime")?,
            name: row.try_get("name")?,
            description: row.try_get("description")?,
            entrypoint: row.try_get("entrypoint")?,
            package_path: row.try_get("package_path")?,
            package_status: row.try_get("package_status")?,
            signature_present: row.try_get("signature_present")?,
            approval_status: row.try_get("approval_status")?,
            enabled: row.try_get("enabled")?,
        })
    }
}

impl PluginScheduleTaskDefinition {
    fn from_row(row: PgRow) -> Result<Self, sqlx::Error> {
        Ok(Self {
            task_key: row.try_get("task_key")?,
            schedule_kind: row.try_get("schedule_kind")?,
            schedule_value: row.try_get("schedule_value")?,
            enabled_by_default: row.try_get("enabled_by_default")?,
            timeout_seconds: row.try_get("timeout_seconds")?,
        })
    }
}

impl PluginPermissionRecord {
    fn from_row(row: PgRow) -> Result<Self, sqlx::Error> {
        Ok(Self {
            permission_key: row.try_get("permission_key")?,
            permission_scope: row.try_get("permission_scope")?,
            reason: row.try_get("reason")?,
        })
    }
}

impl PluginHookRecord {
    fn from_row(row: PgRow) -> Result<Self, sqlx::Error> {
        Ok(Self {
            event_key: row.try_get("event_key")?,
            handler: row.try_get("handler")?,
            priority: row.try_get("priority")?,
            enabled: row.try_get("enabled")?,
        })
    }
}

impl PluginMenuItemRecord {
    fn from_row(row: PgRow) -> Result<Self, sqlx::Error> {
        Ok(Self {
            item_key: row.try_get("item_key")?,
            label: row.try_get("label")?,
            path: row.try_get("path")?,
            parent_key: row.try_get("parent_key")?,
            required_permission: row.try_get("required_permission")?,
            weight: row.try_get("weight")?,
            enabled: row.try_get("enabled")?,
        })
    }
}

impl ActivePluginMenuItemRecord {
    fn from_row(row: PgRow) -> Result<Self, sqlx::Error> {
        Ok(Self {
            plugin_id: row.try_get("plugin_id")?,
            package_id: row.try_get("package_id")?,
            plugin_name: row.try_get("plugin_name")?,
            item_key: row.try_get("item_key")?,
            label: row.try_get("label")?,
            path: row.try_get("path")?,
            parent_key: row.try_get("parent_key")?,
            required_permission: row.try_get("required_permission")?,
            weight: row.try_get("weight")?,
        })
    }
}

impl PluginConfigRecord {
    fn from_row(row: PgRow) -> Result<Self, sqlx::Error> {
        let manifest: Value = row.try_get("manifest")?;
        let manifest = serde_json::from_value::<PluginManifest>(manifest)
            .map_err(|err| sqlx::Error::Decode(Box::new(err)))?;

        Ok(Self {
            plugin_id: row.try_get("plugin_id")?,
            package_id: row.try_get("package_id")?,
            plugin_name: row.try_get("plugin_name")?,
            config_schema: manifest.config_schema,
            values: row.try_get("config")?,
        })
    }
}

impl PluginScheduleDefinitionRecord {
    fn from_row(row: PgRow) -> Result<Self, sqlx::Error> {
        Ok(Self {
            task_key: row.try_get("task_key")?,
            schedule_kind: row.try_get("schedule_kind")?,
            schedule_value: row.try_get("schedule_value")?,
            handler: row.try_get("handler")?,
            enabled_by_default: row.try_get("enabled_by_default")?,
            timeout_seconds: row.try_get("timeout_seconds")?,
        })
    }
}

impl PluginMarketSourceRecord {
    fn from_row(row: PgRow) -> Result<Self, sqlx::Error> {
        Ok(Self {
            id: row.try_get("id")?,
            name: row.try_get("name")?,
            url: row.try_get("url")?,
            enabled: row.try_get("enabled")?,
            last_synced_at: row.try_get("last_synced_at")?,
        })
    }
}

impl PluginMarketEntryRecord {
    fn from_row(row: PgRow) -> Result<Self, sqlx::Error> {
        Ok(Self {
            source_id: row.try_get("source_id")?,
            plugin_id: row.try_get("plugin_id")?,
            name: row.try_get("name")?,
            version: row.try_get("version")?,
            description: row.try_get("description")?,
            author: row.try_get("author")?,
            permissions: row.try_get("permissions")?,
            icon_url: row.try_get("icon_url")?,
            download_url: row.try_get("download_url")?,
            checksum_sha256: row.try_get("checksum_sha256")?,
            signature: row.try_get("signature")?,
        })
    }
}

/// Escape LIKE/ILIKE metacharacters so user search text matches literally.
/// Pair with `escape '\\'` in the query.
fn escape_like_pattern(value: &str) -> String {
    let mut escaped = String::with_capacity(value.len());
    for ch in value.chars() {
        if matches!(ch, '\\' | '%' | '_') {
            escaped.push('\\');
        }
        escaped.push(ch);
    }
    escaped
}

impl Display for PluginStateError {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::PackageNotFound => f.write_str("plugin package not found"),
            Self::PluginNotFound => f.write_str("plugin installation not found"),
            Self::InvalidState(message) => write!(f, "invalid plugin state: {message}"),
            Self::Database(err) => write!(f, "database error: {err}"),
        }
    }
}

impl Error for PluginStateError {}

impl Display for PluginConfigUpdateError {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Database(err) => write!(f, "database error: {err}"),
            Self::Secret(err) => write!(f, "secret error: {err}"),
            Self::MissingRetainedSecret(secret_key) => {
                write!(f, "plugin config secret `{secret_key}` is not configured")
            }
        }
    }
}

impl Error for PluginConfigUpdateError {}

impl From<sqlx::Error> for PluginConfigUpdateError {
    fn from(error: sqlx::Error) -> Self {
        Self::Database(error)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn plugin_package_lifecycle_rule_keeps_upgrade_and_rollback_boundaries() {
        assert!(PLUGIN_PACKAGE_LIFECYCLE_RULE.install_preserves_active_package);
        assert!(PLUGIN_PACKAGE_LIFECYCLE_RULE.approve_switches_active_package);
        assert!(PLUGIN_PACKAGE_LIFECYCLE_RULE.activate_requires_approved_package);
        assert!(PLUGIN_PACKAGE_LIFECYCLE_RULE.reject_preserves_other_active_package);
    }

    #[test]
    fn plugin_installation_admin_keyset_indexes_match_query_shape() {
        let migration =
            include_str!("../../migrations/0052_plugin_installation_admin_keyset_indexes.sql");
        let repository = include_str!("repository.rs");

        assert!(migration.contains("idx_plugin_installations_recent_keyset"));
        assert!(migration.contains("updated_at desc, id desc"));
        assert!(migration.contains("idx_plugin_installations_approval_recent_keyset"));
        assert!(migration.contains("approval_status, updated_at desc, id desc"));
        assert!(migration.contains("idx_plugin_installations_enabled_recent_keyset"));
        assert!(migration.contains("enabled, updated_at desc, id desc"));

        let query_start = repository
            .find("pub async fn list_plugins_page")
            .expect("plugin page query should exist");
        let query_end = repository[query_start..]
            .find("pub async fn list_plugin_packages_page")
            .map(|offset| query_start + offset)
            .expect("plugin package list query should follow plugin list query");
        let plugin_query = &repository[query_start..query_end];

        assert!(plugin_query.contains("QueryBuilder::<Postgres>"));
        assert!(plugin_query.contains("join plugin_installations cursor_installation"));
        assert!(plugin_query.contains("(pi.updated_at, pi.id) <"));
        assert!(plugin_query.contains("pi.approval_status ="));
        assert!(plugin_query.contains("pi.enabled ="));
        assert!(plugin_query.contains("pkg.runtime ="));
        assert!(plugin_query.contains("order by pi.updated_at desc, pi.id desc"));
        assert!(!plugin_query.contains("offset "));
    }

    #[test]
    fn plugin_package_admin_keyset_indexes_match_query_shape() {
        let migration =
            include_str!("../../migrations/0051_plugin_package_admin_keyset_indexes.sql");
        let repository = include_str!("repository.rs");

        assert!(migration.contains("idx_plugin_packages_recent_keyset"));
        assert!(migration.contains("created_at desc, id desc"));
        assert!(migration.contains("idx_plugin_packages_status_recent_keyset"));
        assert!(migration.contains("package_status, created_at desc, id desc"));
        assert!(migration.contains("idx_plugin_packages_plugin_recent_keyset"));
        assert!(migration.contains("plugin_id, created_at desc, id desc"));
        assert!(migration.contains("idx_plugin_packages_runtime_recent_keyset"));
        assert!(migration.contains("runtime, created_at desc, id desc"));

        let query_start = repository
            .find("pub async fn list_plugin_packages_page")
            .expect("plugin package page query should exist");
        let query_end = repository[query_start..]
            .find("pub async fn get_package_detail")
            .map(|offset| query_start + offset)
            .expect("package detail query should follow package list query");
        let package_query = &repository[query_start..query_end];

        assert!(package_query.contains("QueryBuilder::<Postgres>"));
        assert!(package_query.contains("join plugin_packages cursor_package"));
        assert!(package_query.contains("(pkg.created_at, pkg.id) <"));
        assert!(package_query.contains("pkg.plugin_id ="));
        assert!(package_query.contains("pkg.package_status ="));
        assert!(package_query.contains("pkg.runtime ="));
        assert!(package_query.contains("order by pkg.created_at desc, pkg.id desc"));
        assert!(!package_query.contains("offset "));
    }

    #[test]
    fn plugin_package_public_id_entrypoints_keep_uuid_index_shape() {
        let repository = include_str!("repository.rs");
        let bad_package_filter = format!("{}{}", "where pkg.public_id::text = ", "$1");
        let bad_for_update_filter = format!("{}{}", "where public_id::text = ", "$1");

        assert!(repository.contains("where pkg.public_id = case"));
        assert!(repository.contains("where public_id = case"));
        assert!(repository.contains("then $1::uuid"));
        assert!(!repository.contains(&bad_package_filter));
        assert!(!repository.contains(&bad_for_update_filter));
    }
}
