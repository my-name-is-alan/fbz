use std::{
    error::Error,
    fmt::{Display, Formatter},
};

use serde_json::Value;
use sqlx::{Row, postgres::PgRow};

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
    pub plugin_id: String,
    pub package_id: Option<String>,
    pub package_version: Option<String>,
    pub package_status: Option<String>,
    pub approval_status: String,
    pub enabled: bool,
    pub name: Option<String>,
    pub runtime: Option<String>,
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
        let rows = sqlx::query(
            r#"
            select
                pi.plugin_id,
                pi.enabled,
                pi.approval_status,
                pkg.public_id::text as package_id,
                pkg.package_version,
                pkg.package_status,
                pkg.name,
                pkg.runtime
            from plugin_installations pi
            left join plugin_packages pkg on pkg.id = pi.active_package_id
            order by pi.updated_at desc, pi.id desc
            limit $1
            "#,
        )
        .bind(limit)
        .fetch_all(&self.pool)
        .await?;

        rows.into_iter()
            .map(PluginSummaryRecord::from_row)
            .collect()
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
