use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use sqlx::{Row, postgres::PgRow};

use crate::{
    db::DbPool,
    metadata::service::METADATA_REFRESH_JOB_TYPE,
    notifications::{
        delivery::NOTIFICATION_REQUESTED_EVENT,
        secrets::{SecretCipher, SecretError, TargetSecretInput},
    },
    plugins::hooks::PLUGIN_HOOK_DISPATCH_EVENT,
};

#[derive(Clone)]
pub struct AdminRepository {
    pool: DbPool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CreateLibraryInput {
    pub name: String,
    pub library_type: String,
    pub preferred_metadata_language: Option<String>,
    pub preferred_metadata_country: Option<String>,
    pub paths: Vec<String>,
    pub owner_user_id: i64,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AddLibraryPathInput {
    pub library_id: String,
    pub path: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct QueueLibraryScanInput {
    pub library_id: String,
    pub requested_by_user_id: i64,
    pub reason: Option<String>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct QueueMetadataRefreshInput {
    pub requested_by_user_id: i64,
    pub reason: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct QueueLibraryMetadataRefreshInput {
    pub library_id: String,
    pub requested_by_user_id: i64,
    pub reason: Option<String>,
    pub limit: i64,
}

#[derive(Clone, Debug, PartialEq)]
pub struct UpsertNotificationTargetInput {
    pub name: String,
    pub target_type: String,
    pub channel: Option<String>,
    pub config: Value,
    pub secrets: Vec<TargetSecretInput>,
    pub is_enabled: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct UpdateUserPolicyInput {
    pub display_name: Option<String>,
    pub is_disabled: bool,
    pub allow_download: bool,
    pub allow_transcode: bool,
    pub allow_new_device_login: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct UpdateUserLibraryPermissionInput {
    pub can_view: bool,
    pub can_download: bool,
    pub can_transcode: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ManagedLibraryRecord {
    pub id: String,
    pub name: String,
    pub library_type: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LibraryPathRecord {
    pub id: String,
    pub library_id: String,
    pub path: String,
    pub is_enabled: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ScanJobRecord {
    pub id: String,
    pub status: String,
    pub queue_name: String,
    pub job_type: String,
}

#[derive(Clone, Debug, PartialEq)]
pub struct MetadataRefreshJobRecord {
    pub id: String,
    pub status: String,
    pub queue_name: String,
    pub job_type: String,
    pub item_id: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LibraryMetadataRefreshQueueRecord {
    pub library_id: String,
    pub queued_jobs: i64,
}

const ADMIN_LIBRARY_BY_PUBLIC_ID_SQL: &str = r#"
            select id,
                   public_id::text as public_id
            from libraries
            where public_id = case
                when $1::text ~* '^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$'
                then $1::uuid
                else null::uuid
            end
            "#;

const ADMIN_QUEUE_METADATA_REFRESH_ITEM_SQL: &str = r#"
            with target_item as (
                select public_id::text as item_public_id
                from media_items
                where public_id = case
                    when $1::text ~* '^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$'
                    then $1::uuid
                    else null::uuid
                end
                  and is_deleted = false
            ),
            inserted as (
                insert into jobs (
                    job_type,
                    status,
                    queue_name,
                    priority,
                    payload
                )
                select
                    $2,
                    'queued',
                    'metadata',
                    0,
                    jsonb_build_object(
                        'itemId', target_item.item_public_id,
                        'requestedByUserId', $3::bigint,
                        'reason', $4::jsonb
                    )
                from target_item
                on conflict do nothing
                returning public_id::text as id,
                          status,
                          queue_name,
                          job_type,
                          payload->>'itemId' as item_id,
                          created_at
            ),
            existing as (
                select j.public_id::text as id,
                       j.status,
                       j.queue_name,
                       j.job_type,
                       j.payload->>'itemId' as item_id,
                       j.created_at
                from jobs j
                join target_item
                  on target_item.item_public_id = j.payload->>'itemId'
                where j.job_type = $2
                  and j.status in ('queued', 'running', 'failed')
                  and j.attempts < j.max_attempts
                order by j.created_at desc, j.id desc
                limit 1
            )
            select id,
                   status,
                   queue_name,
                   job_type,
                   item_id
            from inserted
            union all
            select id,
                   status,
                   queue_name,
                   job_type,
                   item_id
            from existing
            limit 1
            "#;

const ADMIN_QUEUE_METADATA_REFRESH_LIBRARY_SQL: &str = r#"
            with target_library as (
                select id,
                       public_id::text as library_public_id
                from libraries
                where public_id = case
                    when $1::text ~* '^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$'
                    then $1::uuid
                    else null::uuid
                end
                  and is_hidden = false
            ),
            eligible_items as (
                select mi.public_id::text as item_public_id
                from media_items mi
                join target_library
                  on target_library.id = mi.library_id
                where mi.is_deleted = false
                  and mi.metadata_status in ('pending', 'failed')
                  and mi.item_type in ('movie', 'series', 'episode')
                  and not exists (
                      select 1
                      from jobs j
                      where j.job_type = $2
                        and j.status in ('queued', 'running', 'failed')
                        and j.attempts < j.max_attempts
                        and j.payload->>'itemId' = mi.public_id::text
                  )
                order by mi.updated_at asc, mi.id asc
                limit $5
            ),
            inserted as (
                insert into jobs (
                    job_type,
                    status,
                    queue_name,
                    priority,
                    payload
                )
                select
                    $2,
                    'queued',
                    'metadata',
                    -5,
                    jsonb_build_object(
                        'itemId', eligible_items.item_public_id,
                        'requestedByUserId', $3::bigint,
                        'reason', $4::jsonb
                    )
                from eligible_items
                on conflict do nothing
                returning id
            )
            select target_library.library_public_id,
                   count(inserted.id)::bigint as queued_jobs
            from target_library
            left join inserted on true
            group by target_library.library_public_id
            "#;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AdminUserRecord {
    pub id: String,
    pub username: String,
    pub display_name: Option<String>,
    pub role_name: String,
    pub is_disabled: bool,
    pub allow_download: bool,
    pub allow_transcode: bool,
    pub allow_new_device_login: bool,
    pub has_password: bool,
    pub device_count: i64,
    pub active_session_count: i64,
    pub last_login_at: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AdminUserLibraryPermissionRecord {
    pub library_id: String,
    pub library_name: String,
    pub library_type: String,
    pub is_hidden: bool,
    pub permission_configured: bool,
    pub can_view: bool,
    pub can_download: bool,
    pub can_transcode: bool,
    pub effective_can_view: bool,
    pub effective_can_download: bool,
    pub effective_can_transcode: bool,
    pub permission_updated_at: Option<String>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct AdminJobRecord {
    pub id: String,
    pub job_type: String,
    pub status: String,
    pub queue_name: String,
    pub priority: i32,
    pub payload: Value,
    pub dedupe_key: Option<String>,
    pub run_at: String,
    pub locked_by: Option<String>,
    pub locked_until: Option<String>,
    pub lock_active: bool,
    pub attempts: i32,
    pub max_attempts: i32,
    pub last_error: Option<String>,
    pub created_at: String,
    pub updated_at: String,
    pub finished_at: Option<String>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct AdminJobDetailRecord {
    pub job: AdminJobRecord,
    pub runs: Vec<AdminJobRunRecord>,
    pub events: Vec<AdminJobEventRecord>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct AdminJobRunRecord {
    pub id: i64,
    pub worker_id: String,
    pub status: String,
    pub started_at: String,
    pub finished_at: Option<String>,
    pub duration_ms: i64,
    pub error_message: Option<String>,
    pub metrics: Value,
}

#[derive(Clone, Debug, PartialEq)]
pub struct AdminJobEventRecord {
    pub id: i64,
    pub run_id: Option<i64>,
    pub event_type: String,
    pub event_level: String,
    pub message: Option<String>,
    pub payload: Value,
    pub created_at: String,
}

#[derive(Clone, Debug, PartialEq)]
pub struct NotificationTargetRecord {
    pub id: String,
    pub name: String,
    pub target_type: String,
    pub channel: Option<String>,
    pub config: Value,
    pub is_enabled: bool,
    pub delivery_count: i64,
    pub failure_count: i64,
    pub last_error: Option<String>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct NotificationRequestRecord {
    pub id: String,
    pub plugin_id: String,
    pub package_id: String,
    pub title: String,
    pub message: String,
    pub level: String,
    pub channel: Option<String>,
    pub metadata: Value,
    pub status: String,
    pub outbox_event_id: Option<i64>,
    pub last_error: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NotificationDeliveryAttemptRecord {
    pub id: String,
    pub request_id: String,
    pub outbox_event_id: Option<i64>,
    pub target_id: Option<String>,
    pub target_type: String,
    pub target_name: String,
    pub attempt: i32,
    pub status: String,
    pub response_status: Option<i32>,
    pub error_message: Option<String>,
    pub duration_ms: Option<i32>,
    pub created_at: String,
    pub finished_at: Option<String>,
}

#[derive(Debug)]
pub enum NotificationRetryError {
    NotFound,
    InvalidStatus(String),
    Database(sqlx::Error),
}

#[derive(Clone, Debug, PartialEq)]
pub struct PluginDispatchRecord {
    pub id: String,
    pub plugin_id: Option<String>,
    pub package_id: Option<String>,
    pub hook_id: Option<String>,
    pub handler: Option<String>,
    pub hook_event: Option<String>,
    pub aggregate_type: String,
    pub aggregate_id: String,
    pub payload: Value,
    pub status: String,
    pub attempts: i32,
    pub max_attempts: i32,
    pub available_at: String,
    pub locked_until: Option<String>,
    pub last_error: Option<String>,
    pub created_at: String,
    pub delivered_at: Option<String>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct PluginExecutionRunRecord {
    pub id: String,
    pub dispatch_id: String,
    pub outbox_event_id: Option<i64>,
    pub attempt: i32,
    pub plugin_id: String,
    pub package_id: String,
    pub hook_id: Option<i64>,
    pub handler: String,
    pub event_key: String,
    pub runtime: String,
    pub entrypoint: String,
    pub status: String,
    pub request_payload: Value,
    pub response_status: Option<i32>,
    pub response_body: Option<String>,
    pub error_message: Option<String>,
    pub started_at: String,
    pub finished_at: Option<String>,
    pub duration_ms: Option<i32>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PluginHostApiCallFilter {
    pub plugin_id: Option<String>,
    pub execution_run_id: Option<String>,
    pub status_code: Option<i32>,
    pub limit: i64,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PluginHostApiCallRecord {
    pub id: String,
    pub plugin_id: String,
    pub package_id: String,
    pub host_token_id: Option<String>,
    pub execution_run_id: Option<String>,
    pub method: String,
    pub path: String,
    pub required_permission: Option<String>,
    pub status_code: i32,
    pub error_code: Option<String>,
    pub error_message: Option<String>,
    pub started_at: String,
    pub finished_at: String,
    pub duration_ms: i32,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ScheduledTaskAdminRecord {
    pub id: String,
    pub task_key: String,
    pub task_type: String,
    pub owner_type: String,
    pub owner_id: Option<String>,
    pub enabled: bool,
    pub schedule_kind: String,
    pub schedule_value: String,
    pub next_run_at: Option<String>,
    pub last_run_at: Option<String>,
    pub timeout_seconds: i32,
    pub max_concurrency: i32,
    pub active_run_count: i64,
    pub last_run_id: Option<String>,
    pub failure_count: i32,
    pub last_error: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ScheduledTaskRunRecord {
    pub id: String,
    pub task_key: String,
    pub trigger_type: String,
    pub worker_id: String,
    pub status: String,
    pub lease_expires_at: String,
    pub lease_active: bool,
    pub queued_jobs: Option<i64>,
    pub error_message: Option<String>,
    pub started_at: String,
    pub finished_at: Option<String>,
    pub duration_ms: i64,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug)]
pub enum PluginDispatchReplayError {
    NotFound,
    InvalidStatus(String),
    Database(sqlx::Error),
}

impl AdminRepository {
    pub fn new(pool: DbPool) -> Self {
        Self { pool }
    }

    pub async fn create_library(
        &self,
        input: CreateLibraryInput,
    ) -> Result<ManagedLibraryRecord, sqlx::Error> {
        let mut tx = self.pool.begin().await?;
        let library_row = sqlx::query(
            r#"
            insert into libraries (
                name,
                library_type,
                preferred_metadata_language,
                preferred_metadata_country
            )
            values ($1, $2, $3, $4)
            returning
                id,
                public_id::text as public_id,
                name,
                library_type
            "#,
        )
        .bind(input.name.trim())
        .bind(&input.library_type)
        .bind(input.preferred_metadata_language.as_deref().map(str::trim))
        .bind(input.preferred_metadata_country.as_deref().map(str::trim))
        .fetch_one(&mut *tx)
        .await?;

        let library_row_id = library_row.try_get::<i64, _>("id")?;
        sqlx::query(
            r#"
            insert into library_permissions (
                library_id,
                user_id,
                can_view,
                can_download,
                can_transcode
            )
            values ($1, $2, true, true, true)
            on conflict (library_id, user_id) do update
                set can_view = true,
                    can_download = true,
                    can_transcode = true,
                    updated_at = now()
            "#,
        )
        .bind(library_row_id)
        .bind(input.owner_user_id)
        .execute(&mut *tx)
        .await?;

        for path in input.paths {
            insert_library_path(&mut tx, library_row_id, &path).await?;
        }

        tx.commit().await?;

        ManagedLibraryRecord::from_row(library_row)
    }

    pub async fn add_library_path(
        &self,
        input: AddLibraryPathInput,
    ) -> Result<Option<LibraryPathRecord>, sqlx::Error> {
        let mut tx = self.pool.begin().await?;
        let Some(library_row) = sqlx::query(ADMIN_LIBRARY_BY_PUBLIC_ID_SQL)
            .bind(&input.library_id)
            .fetch_optional(&mut *tx)
            .await?
        else {
            return Ok(None);
        };

        let library_row_id = library_row.try_get::<i64, _>("id")?;
        let library_public_id = library_row.try_get::<String, _>("public_id")?;
        let path = insert_library_path(&mut tx, library_row_id, &input.path).await?;
        tx.commit().await?;

        Ok(Some(LibraryPathRecord {
            id: path.id,
            library_id: library_public_id,
            path: path.path,
            is_enabled: path.is_enabled,
        }))
    }

    pub async fn queue_library_scan(
        &self,
        input: QueueLibraryScanInput,
    ) -> Result<Option<ScanJobRecord>, sqlx::Error> {
        let Some(library_row) = sqlx::query(ADMIN_LIBRARY_BY_PUBLIC_ID_SQL)
            .bind(&input.library_id)
            .fetch_optional(&self.pool)
            .await?
        else {
            return Ok(None);
        };

        let library_row_id = library_row.try_get::<i64, _>("id")?;
        let library_public_id = library_row.try_get::<String, _>("public_id")?;
        let payload = json!({
            "libraryId": library_public_id,
            "requestedByUserId": input.requested_by_user_id,
            "reason": input.reason,
        });
        let job = sqlx::query(
            r#"
            insert into jobs (
                job_type,
                status,
                queue_name,
                priority,
                payload
            )
            values ('library.scan', 'queued', 'scan', 0, $1)
            returning
                public_id::text as id,
                status,
                queue_name,
                job_type
            "#,
        )
        .bind(payload)
        .fetch_one(&self.pool)
        .await?;

        let _ = library_row_id;
        ScanJobRecord::from_row(job).map(Some)
    }

    pub async fn queue_metadata_refresh_for_item(
        &self,
        item_id: &str,
        input: QueueMetadataRefreshInput,
    ) -> Result<Option<MetadataRefreshJobRecord>, sqlx::Error> {
        let payload_reason = json!(input.reason);
        let row = sqlx::query(ADMIN_QUEUE_METADATA_REFRESH_ITEM_SQL)
            .bind(item_id.trim())
            .bind(METADATA_REFRESH_JOB_TYPE)
            .bind(input.requested_by_user_id)
            .bind(payload_reason)
            .fetch_optional(&self.pool)
            .await?;

        row.map(MetadataRefreshJobRecord::from_row).transpose()
    }

    pub async fn queue_metadata_refresh_for_library(
        &self,
        input: QueueLibraryMetadataRefreshInput,
    ) -> Result<Option<LibraryMetadataRefreshQueueRecord>, sqlx::Error> {
        let payload_reason = json!(input.reason);
        let row = sqlx::query(ADMIN_QUEUE_METADATA_REFRESH_LIBRARY_SQL)
            .bind(input.library_id.trim())
            .bind(METADATA_REFRESH_JOB_TYPE)
            .bind(input.requested_by_user_id)
            .bind(payload_reason)
            .bind(input.limit)
            .fetch_optional(&self.pool)
            .await?;

        row.map(LibraryMetadataRefreshQueueRecord::from_row)
            .transpose()
    }

    pub async fn list_admin_users(&self, limit: i64) -> Result<Vec<AdminUserRecord>, sqlx::Error> {
        let rows = sqlx::query(
            r#"
            select u.public_id::text as id,
                   u.username,
                   u.display_name,
                   r.name as role_name,
                   u.is_disabled,
                   u.allow_download,
                   u.allow_transcode,
                   u.allow_new_device_login,
                   u.password_hash is not null as has_password,
                   (
                       select count(*)::bigint
                       from devices d
                       where d.user_id = u.id
                   ) as device_count,
                   (
                       select count(*)::bigint
                       from sessions s
                       where s.user_id = u.id
                         and s.revoked_at is null
                         and s.expires_at > now()
                   ) as active_session_count,
                   u.last_login_at::text as last_login_at,
                   u.created_at::text as created_at,
                   u.updated_at::text as updated_at
            from users u
            join roles r on r.id = u.role_id
            order by u.username_normalized asc, u.id asc
            limit $1
            "#,
        )
        .bind(limit)
        .fetch_all(&self.pool)
        .await?;

        rows.into_iter().map(AdminUserRecord::from_row).collect()
    }

    pub async fn update_user_policy(
        &self,
        user_id: &str,
        input: UpdateUserPolicyInput,
    ) -> Result<Option<AdminUserRecord>, sqlx::Error> {
        let row = sqlx::query(
            r#"
            with updated as (
                update users
                set display_name = $2,
                    is_disabled = $3,
                    allow_download = $4,
                    allow_transcode = $5,
                    allow_new_device_login = $6,
                    updated_at = now()
                where public_id = $1::uuid
                returning *
            )
            select u.public_id::text as id,
                   u.username,
                   u.display_name,
                   r.name as role_name,
                   u.is_disabled,
                   u.allow_download,
                   u.allow_transcode,
                   u.allow_new_device_login,
                   u.password_hash is not null as has_password,
                   (
                       select count(*)::bigint
                       from devices d
                       where d.user_id = u.id
                   ) as device_count,
                   (
                       select count(*)::bigint
                       from sessions s
                       where s.user_id = u.id
                         and s.revoked_at is null
                         and s.expires_at > now()
                   ) as active_session_count,
                   u.last_login_at::text as last_login_at,
                   u.created_at::text as created_at,
                   u.updated_at::text as updated_at
            from updated u
            join roles r on r.id = u.role_id
            "#,
        )
        .bind(user_id.trim())
        .bind(input.display_name.as_deref())
        .bind(input.is_disabled)
        .bind(input.allow_download)
        .bind(input.allow_transcode)
        .bind(input.allow_new_device_login)
        .fetch_optional(&self.pool)
        .await?;

        row.map(AdminUserRecord::from_row).transpose()
    }

    pub async fn list_user_library_permissions(
        &self,
        user_id: &str,
        limit: i64,
    ) -> Result<Option<Vec<AdminUserLibraryPermissionRecord>>, sqlx::Error> {
        let Some(user_row) = sqlx::query(
            r#"
            select id,
                   allow_download,
                   allow_transcode
            from users
            where public_id = $1::uuid
            "#,
        )
        .bind(user_id.trim())
        .fetch_optional(&self.pool)
        .await?
        else {
            return Ok(None);
        };

        let user_row_id = user_row.try_get::<i64, _>("id")?;
        let allow_download = user_row.try_get::<bool, _>("allow_download")?;
        let allow_transcode = user_row.try_get::<bool, _>("allow_transcode")?;
        let rows = sqlx::query(
            r#"
            select l.public_id::text as library_id,
                   l.name as library_name,
                   l.library_type,
                   l.is_hidden,
                   lp.id is not null as permission_configured,
                   coalesce(lp.can_view, false) as can_view,
                   coalesce(lp.can_download, false) as can_download,
                   coalesce(lp.can_transcode, false) as can_transcode,
                   (coalesce(lp.can_view, false) and not l.is_hidden) as effective_can_view,
                   ($2::boolean and coalesce(lp.can_download, false) and not l.is_hidden) as effective_can_download,
                   ($3::boolean and coalesce(lp.can_transcode, false) and not l.is_hidden) as effective_can_transcode,
                   lp.updated_at::text as permission_updated_at
            from libraries l
            left join library_permissions lp
              on lp.library_id = l.id
             and lp.user_id = $1
            order by l.name asc, l.id asc
            limit $4
            "#,
        )
        .bind(user_row_id)
        .bind(allow_download)
        .bind(allow_transcode)
        .bind(limit)
        .fetch_all(&self.pool)
        .await?;

        rows.into_iter()
            .map(AdminUserLibraryPermissionRecord::from_row)
            .collect::<Result<Vec<_>, _>>()
            .map(Some)
    }

    pub async fn update_user_library_permission(
        &self,
        user_id: &str,
        library_id: &str,
        input: UpdateUserLibraryPermissionInput,
    ) -> Result<Option<AdminUserLibraryPermissionRecord>, sqlx::Error> {
        let row = sqlx::query(
            r#"
            with target_user as (
                select id,
                       allow_download,
                       allow_transcode
                from users
                where public_id = $1::uuid
            ),
            target_library as (
                select id,
                       public_id,
                       name,
                       library_type,
                       is_hidden
                from libraries
                where public_id = $2::uuid
            ),
            upserted as (
                insert into library_permissions (
                    library_id,
                    user_id,
                    can_view,
                    can_download,
                    can_transcode
                )
                select target_library.id,
                       target_user.id,
                       $3,
                       $4,
                       $5
                from target_user
                cross join target_library
                on conflict (library_id, user_id) do update
                    set can_view = excluded.can_view,
                        can_download = excluded.can_download,
                        can_transcode = excluded.can_transcode,
                        updated_at = now()
                returning *
            )
            select target_library.public_id::text as library_id,
                   target_library.name as library_name,
                   target_library.library_type,
                   target_library.is_hidden,
                   true as permission_configured,
                   upserted.can_view,
                   upserted.can_download,
                   upserted.can_transcode,
                   (upserted.can_view and not target_library.is_hidden) as effective_can_view,
                   (target_user.allow_download and upserted.can_download and not target_library.is_hidden) as effective_can_download,
                   (target_user.allow_transcode and upserted.can_transcode and not target_library.is_hidden) as effective_can_transcode,
                   upserted.updated_at::text as permission_updated_at
            from upserted
            join target_user on target_user.id = upserted.user_id
            join target_library on target_library.id = upserted.library_id
            "#,
        )
        .bind(user_id.trim())
        .bind(library_id.trim())
        .bind(input.can_view)
        .bind(input.can_download)
        .bind(input.can_transcode)
        .fetch_optional(&self.pool)
        .await?;

        row.map(AdminUserLibraryPermissionRecord::from_row)
            .transpose()
    }

    pub async fn list_admin_jobs(&self, limit: i64) -> Result<Vec<AdminJobRecord>, sqlx::Error> {
        let rows = sqlx::query(
            r#"
            select public_id::text as id,
                   job_type,
                   status,
                   queue_name,
                   priority,
                   payload,
                   dedupe_key,
                   run_at::text as run_at,
                   locked_by,
                   locked_until::text as locked_until,
                   (status = 'running' and locked_until > now()) as lock_active,
                   attempts,
                   max_attempts,
                   last_error,
                   created_at::text as created_at,
                   updated_at::text as updated_at,
                   finished_at::text as finished_at
            from jobs
            order by id desc
            limit $1
            "#,
        )
        .bind(limit)
        .fetch_all(&self.pool)
        .await?;

        rows.into_iter().map(AdminJobRecord::from_row).collect()
    }

    pub async fn get_admin_job_detail(
        &self,
        job_id: &str,
        run_limit: i64,
        event_limit: i64,
    ) -> Result<Option<AdminJobDetailRecord>, sqlx::Error> {
        let Some(job_row) = sqlx::query(
            r#"
            select id as row_id,
                   public_id::text as id,
                   job_type,
                   status,
                   queue_name,
                   priority,
                   payload,
                   dedupe_key,
                   run_at::text as run_at,
                   locked_by,
                   locked_until::text as locked_until,
                   (status = 'running' and locked_until > now()) as lock_active,
                   attempts,
                   max_attempts,
                   last_error,
                   created_at::text as created_at,
                   updated_at::text as updated_at,
                   finished_at::text as finished_at
            from jobs
            where public_id = $1::uuid
            "#,
        )
        .bind(job_id.trim())
        .fetch_optional(&self.pool)
        .await?
        else {
            return Ok(None);
        };

        let row_id = job_row.try_get::<i64, _>("row_id")?;
        let job = AdminJobRecord::from_row(job_row)?;

        let run_rows = sqlx::query(
            r#"
            select id,
                   worker_id,
                   status,
                   started_at::text as started_at,
                   finished_at::text as finished_at,
                   floor(extract(epoch from (coalesce(finished_at, now()) - started_at)) * 1000)::bigint as duration_ms,
                   error_message,
                   metrics
            from job_runs
            where job_id = $1
            order by started_at desc, id desc
            limit $2
            "#,
        )
        .bind(row_id)
        .bind(run_limit)
        .fetch_all(&self.pool)
        .await?;

        let event_rows = sqlx::query(
            r#"
            select id,
                   job_run_id as run_id,
                   event_type,
                   event_level,
                   message,
                   payload,
                   created_at::text as created_at
            from job_events
            where job_id = $1
            order by created_at desc, id desc
            limit $2
            "#,
        )
        .bind(row_id)
        .bind(event_limit)
        .fetch_all(&self.pool)
        .await?;

        Ok(Some(AdminJobDetailRecord {
            job,
            runs: run_rows
                .into_iter()
                .map(AdminJobRunRecord::from_row)
                .collect::<Result<Vec<_>, _>>()?,
            events: event_rows
                .into_iter()
                .map(AdminJobEventRecord::from_row)
                .collect::<Result<Vec<_>, _>>()?,
        }))
    }

    pub async fn list_notification_targets(
        &self,
        limit: i64,
    ) -> Result<Vec<NotificationTargetRecord>, sqlx::Error> {
        let rows = sqlx::query(
            r#"
            select public_id::text as id,
                   name,
                   target_type,
                   channel,
                   config,
                   is_enabled,
                   delivery_count,
                   failure_count,
                   last_error
            from notification_targets
            order by target_type, name, id
            limit $1
            "#,
        )
        .bind(limit)
        .fetch_all(&self.pool)
        .await?;

        rows.into_iter()
            .map(NotificationTargetRecord::from_row)
            .collect()
    }

    pub async fn create_notification_target(
        &self,
        input: UpsertNotificationTargetInput,
        cipher: &SecretCipher,
    ) -> Result<NotificationTargetRecord, sqlx::Error> {
        let mut tx = self.pool.begin().await?;
        let row = sqlx::query(
            r#"
            insert into notification_targets (
                name,
                target_type,
                channel,
                config,
                is_enabled
            )
            values ($1, $2, $3, $4, $5)
            returning public_id::text as id,
                      id as internal_id,
                      name,
                      target_type,
                      channel,
                      config,
                      is_enabled,
                      delivery_count,
                      failure_count,
                      last_error
            "#,
        )
        .bind(input.name.trim())
        .bind(input.target_type.trim())
        .bind(input.channel.as_deref().map(str::trim))
        .bind(&input.config)
        .bind(input.is_enabled)
        .fetch_one(&mut *tx)
        .await?;
        let target_id = row.try_get::<i64, _>("internal_id")?;
        replace_notification_target_secrets(&mut tx, target_id, &input.secrets, cipher).await?;
        tx.commit().await?;

        NotificationTargetRecord::from_row(row)
    }

    pub async fn replace_notification_target(
        &self,
        target_id: &str,
        input: UpsertNotificationTargetInput,
        cipher: &SecretCipher,
    ) -> Result<Option<NotificationTargetRecord>, sqlx::Error> {
        let mut tx = self.pool.begin().await?;
        let row = sqlx::query(
            r#"
            update notification_targets
            set name = $2,
                target_type = $3,
                channel = $4,
                config = $5,
                is_enabled = $6,
                last_error = null,
                updated_at = now()
            where public_id = case
                when $1::text ~* '^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$'
                then $1::uuid
                else null::uuid
            end
            returning public_id::text as id,
                      id as internal_id,
                      name,
                      target_type,
                      channel,
                      config,
                      is_enabled,
                      delivery_count,
                      failure_count,
                      last_error
            "#,
        )
        .bind(target_id.trim())
        .bind(input.name.trim())
        .bind(input.target_type.trim())
        .bind(input.channel.as_deref().map(str::trim))
        .bind(&input.config)
        .bind(input.is_enabled)
        .fetch_optional(&mut *tx)
        .await?;

        let Some(row) = row else {
            tx.commit().await?;
            return Ok(None);
        };
        let internal_target_id = row.try_get::<i64, _>("internal_id")?;
        replace_notification_target_secrets(&mut tx, internal_target_id, &input.secrets, cipher)
            .await?;
        tx.commit().await?;

        NotificationTargetRecord::from_row(row).map(Some)
    }

    pub async fn set_notification_target_enabled(
        &self,
        target_id: &str,
        is_enabled: bool,
    ) -> Result<Option<NotificationTargetRecord>, sqlx::Error> {
        let row = sqlx::query(
            r#"
            update notification_targets
            set is_enabled = $2,
                updated_at = now()
            where public_id = case
                when $1::text ~* '^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$'
                then $1::uuid
                else null::uuid
            end
            returning public_id::text as id,
                      name,
                      target_type,
                      channel,
                      config,
                      is_enabled,
                      delivery_count,
                      failure_count,
                      last_error
            "#,
        )
        .bind(target_id.trim())
        .bind(is_enabled)
        .fetch_optional(&self.pool)
        .await?;

        row.map(NotificationTargetRecord::from_row).transpose()
    }

    pub async fn list_notification_requests(
        &self,
        limit: i64,
    ) -> Result<Vec<NotificationRequestRecord>, sqlx::Error> {
        let rows = sqlx::query(
            r#"
            select public_id::text as id,
                   plugin_id,
                   package_id,
                   title,
                   message,
                   level,
                   channel,
                   metadata,
                   status,
                   outbox_event_id,
                   last_error,
                   created_at::text as created_at,
                   updated_at::text as updated_at
            from plugin_notification_requests
            order by created_at desc, id desc
            limit $1
            "#,
        )
        .bind(limit)
        .fetch_all(&self.pool)
        .await?;

        rows.into_iter()
            .map(NotificationRequestRecord::from_row)
            .collect()
    }

    pub async fn list_notification_delivery_attempts(
        &self,
        request_id: &str,
        limit: i64,
    ) -> Result<Option<Vec<NotificationDeliveryAttemptRecord>>, sqlx::Error> {
        let request_row = sqlx::query(
            r#"
            select id,
                   public_id::text as public_id
            from plugin_notification_requests
            where public_id = case
                when $1::text ~* '^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$'
                then $1::uuid
                else null::uuid
            end
            "#,
        )
        .bind(request_id.trim())
        .fetch_optional(&self.pool)
        .await?;

        let Some(request_row) = request_row else {
            return Ok(None);
        };
        let internal_request_id = request_row.try_get::<i64, _>("id")?;
        let public_request_id = request_row.try_get::<String, _>("public_id")?;

        let rows = sqlx::query(
            r#"
            select attempts.public_id::text as id,
                   $2::text as request_id,
                   attempts.outbox_event_id,
                   attempts.target_public_id::text as target_id,
                   attempts.target_type,
                   attempts.target_name,
                   attempts.attempt,
                   attempts.status,
                   attempts.response_status,
                   attempts.error_message,
                   attempts.duration_ms,
                   attempts.created_at::text as created_at,
                   attempts.finished_at::text as finished_at
            from notification_delivery_attempts attempts
            where attempts.notification_request_id = $1
            order by attempts.created_at desc, attempts.id desc
            limit $3
            "#,
        )
        .bind(internal_request_id)
        .bind(public_request_id)
        .bind(limit)
        .fetch_all(&self.pool)
        .await?;

        rows.into_iter()
            .map(NotificationDeliveryAttemptRecord::from_row)
            .collect::<Result<Vec<_>, _>>()
            .map(Some)
    }

    pub async fn retry_notification_request(
        &self,
        request_id: &str,
    ) -> Result<NotificationRequestRecord, NotificationRetryError> {
        let mut tx = self.pool.begin().await?;
        let Some(row) = sqlx::query(
            r#"
            select id as internal_id,
                   public_id::text as id,
                   plugin_id,
                   package_id,
                   title,
                   message,
                   level,
                   channel,
                   metadata,
                   status
            from plugin_notification_requests
            where public_id = case
                when $1::text ~* '^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$'
                then $1::uuid
                else null::uuid
            end
            for update
            "#,
        )
        .bind(request_id.trim())
        .fetch_optional(&mut *tx)
        .await?
        else {
            tx.commit().await?;
            return Err(NotificationRetryError::NotFound);
        };

        let status = row.try_get::<String, _>("status")?;
        if !matches!(status.as_str(), "failed" | "discarded") {
            tx.commit().await?;
            return Err(NotificationRetryError::InvalidStatus(status));
        }

        let internal_id = row.try_get::<i64, _>("internal_id")?;
        let public_id = row.try_get::<String, _>("id")?;
        let plugin_id = row.try_get::<String, _>("plugin_id")?;
        let package_id = row.try_get::<String, _>("package_id")?;
        let title = row.try_get::<String, _>("title")?;
        let message = row.try_get::<String, _>("message")?;
        let level = row.try_get::<String, _>("level")?;
        let channel = row.try_get::<Option<String>, _>("channel")?;
        let metadata = row.try_get::<Value, _>("metadata")?;
        let payload = json!({
            "requestId": public_id,
            "pluginId": plugin_id,
            "packageId": package_id,
            "title": title,
            "message": message,
            "level": level,
            "channel": channel,
            "metadata": metadata,
        });
        let outbox_row = sqlx::query(
            r#"
            insert into event_outbox (
                event_type,
                aggregate_type,
                aggregate_id,
                payload
            )
            values ($1, 'plugin_notification', $2, $3)
            returning id
            "#,
        )
        .bind(NOTIFICATION_REQUESTED_EVENT)
        .bind(&public_id)
        .bind(payload)
        .fetch_one(&mut *tx)
        .await?;
        let outbox_id = outbox_row.try_get::<i64, _>("id")?;

        let row = sqlx::query(
            r#"
            update plugin_notification_requests
            set status = 'queued',
                outbox_event_id = $2,
                last_error = null,
                updated_at = now()
            where id = $1
            returning public_id::text as id,
                      plugin_id,
                      package_id,
                      title,
                      message,
                      level,
                      channel,
                      metadata,
                      status,
                      outbox_event_id,
                      last_error,
                      created_at::text as created_at,
                      updated_at::text as updated_at
            "#,
        )
        .bind(internal_id)
        .bind(outbox_id)
        .fetch_one(&mut *tx)
        .await?;

        tx.commit().await?;
        NotificationRequestRecord::from_row(row).map_err(NotificationRetryError::from)
    }

    pub async fn list_plugin_dispatches(
        &self,
        limit: i64,
    ) -> Result<Vec<PluginDispatchRecord>, sqlx::Error> {
        let rows = sqlx::query(
            r#"
            select public_id::text as id,
                   payload->>'pluginId' as plugin_id,
                   payload->>'packageId' as package_id,
                   payload->>'hookId' as hook_id,
                   payload->>'handler' as handler,
                   payload->>'hookEvent' as hook_event,
                   aggregate_type,
                   aggregate_id,
                   payload,
                   status,
                   attempts,
                   max_attempts,
                   available_at::text as available_at,
                   locked_until::text as locked_until,
                   last_error,
                   created_at::text as created_at,
                   delivered_at::text as delivered_at
            from event_outbox
            where event_type = $1
            order by created_at desc, id desc
            limit $2
            "#,
        )
        .bind(PLUGIN_HOOK_DISPATCH_EVENT)
        .bind(limit)
        .fetch_all(&self.pool)
        .await?;

        rows.into_iter()
            .map(PluginDispatchRecord::from_row)
            .collect()
    }

    pub async fn list_plugin_execution_runs(
        &self,
        dispatch_id: &str,
        limit: i64,
    ) -> Result<Option<Vec<PluginExecutionRunRecord>>, sqlx::Error> {
        let dispatch_exists = sqlx::query_scalar::<_, bool>(
            r#"
            select exists (
                select 1
                from event_outbox
                where public_id = case
                    when $1::text ~* '^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$'
                    then $1::uuid
                    else null::uuid
                end
                  and event_type = $2
            )
            "#,
        )
        .bind(dispatch_id.trim())
        .bind(PLUGIN_HOOK_DISPATCH_EVENT)
        .fetch_one(&self.pool)
        .await?;

        if !dispatch_exists {
            return Ok(None);
        }

        let rows = sqlx::query(
            r#"
            select public_id::text as id,
                   outbox_event_public_id as dispatch_id,
                   outbox_event_id,
                   attempt,
                   plugin_id,
                   package_id,
                   hook_id,
                   handler,
                   event_key,
                   runtime,
                   entrypoint,
                   status,
                   request_payload,
                   response_status,
                   response_body,
                   error_message,
                   started_at::text as started_at,
                   finished_at::text as finished_at,
                   duration_ms
            from plugin_execution_runs
            where outbox_event_public_id = $1
            order by started_at desc, id desc
            limit $2
            "#,
        )
        .bind(dispatch_id.trim())
        .bind(limit)
        .fetch_all(&self.pool)
        .await?;

        rows.into_iter()
            .map(PluginExecutionRunRecord::from_row)
            .collect::<Result<Vec<_>, _>>()
            .map(Some)
    }

    pub async fn list_plugin_host_api_calls(
        &self,
        filter: PluginHostApiCallFilter,
    ) -> Result<Vec<PluginHostApiCallRecord>, sqlx::Error> {
        let rows = sqlx::query(
            r#"
            select calls.public_id::text as id,
                   calls.plugin_id,
                   calls.package_id,
                   token.public_id::text as host_token_id,
                   run.public_id::text as execution_run_id,
                   calls.method,
                   calls.path,
                   calls.required_permission,
                   calls.status_code,
                   calls.error_code,
                   calls.error_message,
                   calls.started_at::text as started_at,
                   calls.finished_at::text as finished_at,
                   calls.duration_ms
            from plugin_host_api_calls calls
            left join plugin_host_tokens token on token.id = calls.host_token_id
            left join plugin_execution_runs run on run.id = calls.execution_run_id
            where ($1::text is null or calls.plugin_id = $1)
              and (
                  $2::text is null
                  or run.public_id = case
                      when $2::text ~* '^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$'
                      then $2::uuid
                      else null::uuid
                  end
              )
              and ($3::integer is null or calls.status_code = $3)
            order by calls.finished_at desc, calls.id desc
            limit $4
            "#,
        )
        .bind(filter.plugin_id.as_deref())
        .bind(filter.execution_run_id.as_deref())
        .bind(filter.status_code)
        .bind(filter.limit)
        .fetch_all(&self.pool)
        .await?;

        rows.into_iter()
            .map(PluginHostApiCallRecord::from_row)
            .collect()
    }

    pub async fn list_plugin_host_api_calls_for_run(
        &self,
        execution_run_id: &str,
        limit: i64,
    ) -> Result<Option<Vec<PluginHostApiCallRecord>>, sqlx::Error> {
        let run_exists = sqlx::query_scalar::<_, bool>(
            r#"
            select exists (
                select 1
                from plugin_execution_runs
                where public_id = case
                    when $1::text ~* '^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$'
                    then $1::uuid
                    else null::uuid
                end
            )
            "#,
        )
        .bind(execution_run_id.trim())
        .fetch_one(&self.pool)
        .await?;

        if !run_exists {
            return Ok(None);
        }

        self.list_plugin_host_api_calls(PluginHostApiCallFilter {
            plugin_id: None,
            execution_run_id: Some(execution_run_id.trim().to_owned()),
            status_code: None,
            limit,
        })
        .await
        .map(Some)
    }

    pub async fn replay_plugin_dispatch(
        &self,
        dispatch_id: &str,
    ) -> Result<PluginDispatchRecord, PluginDispatchReplayError> {
        let mut tx = self.pool.begin().await?;
        let Some(row) = sqlx::query(
            r#"
            select aggregate_type,
                   aggregate_id,
                   payload,
                   status,
                   max_attempts
            from event_outbox
            where public_id = case
                when $1::text ~* '^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$'
                then $1::uuid
                else null::uuid
            end
              and event_type = $2
            for update
            "#,
        )
        .bind(dispatch_id.trim())
        .bind(PLUGIN_HOOK_DISPATCH_EVENT)
        .fetch_optional(&mut *tx)
        .await?
        else {
            return Err(PluginDispatchReplayError::NotFound);
        };

        let status = row.try_get::<String, _>("status")?;
        if !matches!(status.as_str(), "failed" | "discarded") {
            return Err(PluginDispatchReplayError::InvalidStatus(status));
        }

        let aggregate_type = row.try_get::<String, _>("aggregate_type")?;
        let aggregate_id = row.try_get::<String, _>("aggregate_id")?;
        let payload = row.try_get::<Value, _>("payload")?;
        let max_attempts = row.try_get::<i32, _>("max_attempts")?;
        let replay_row = sqlx::query(
            r#"
            insert into event_outbox (
                event_type,
                aggregate_type,
                aggregate_id,
                payload,
                max_attempts
            )
            values ($1, $2, $3, $4, $5)
            returning public_id::text as id,
                      payload->>'pluginId' as plugin_id,
                      payload->>'packageId' as package_id,
                      payload->>'hookId' as hook_id,
                      payload->>'handler' as handler,
                      payload->>'hookEvent' as hook_event,
                      aggregate_type,
                      aggregate_id,
                      payload,
                      status,
                      attempts,
                      max_attempts,
                      available_at::text as available_at,
                      locked_until::text as locked_until,
                      last_error,
                      created_at::text as created_at,
                      delivered_at::text as delivered_at
            "#,
        )
        .bind(PLUGIN_HOOK_DISPATCH_EVENT)
        .bind(aggregate_type)
        .bind(aggregate_id)
        .bind(payload)
        .bind(max_attempts)
        .fetch_one(&mut *tx)
        .await?;

        tx.commit().await?;
        PluginDispatchRecord::from_row(replay_row).map_err(PluginDispatchReplayError::from)
    }

    pub async fn list_scheduled_tasks(
        &self,
        limit: i64,
    ) -> Result<Vec<ScheduledTaskAdminRecord>, sqlx::Error> {
        let rows = sqlx::query(
            r#"
            select public_id::text as id,
                   task_key,
                   task_type,
                   owner_type,
                   owner_id,
                   enabled,
                   schedule_kind,
                   schedule_value,
                   next_run_at::text as next_run_at,
                   last_run_at::text as last_run_at,
                   timeout_seconds,
                   max_concurrency,
                   (
                       select count(*)::bigint
                       from scheduled_task_runs runs
                       where runs.task_id = scheduled_tasks.id
                         and runs.status = 'running'
                         and runs.lease_expires_at > now()
                   ) as active_run_count,
                   (
                       select runs.public_id::text
                       from scheduled_task_runs runs
                       where runs.task_id = scheduled_tasks.id
                       order by runs.started_at desc, runs.id desc
                       limit 1
                   ) as last_run_id,
                   failure_count,
                   last_error,
                   created_at::text as created_at,
                   updated_at::text as updated_at
            from scheduled_tasks
            order by enabled desc,
                     next_run_at asc nulls last,
                     updated_at desc,
                     id desc
            limit $1
            "#,
        )
        .bind(limit)
        .fetch_all(&self.pool)
        .await?;

        rows.into_iter()
            .map(ScheduledTaskAdminRecord::from_row)
            .collect()
    }

    pub async fn find_scheduled_task(
        &self,
        id_or_key: &str,
    ) -> Result<Option<ScheduledTaskAdminRecord>, sqlx::Error> {
        let id_or_key = id_or_key.trim();
        let row = sqlx::query(
            r#"
            select public_id::text as id,
                   task_key,
                   task_type,
                   owner_type,
                   owner_id,
                   enabled,
                   schedule_kind,
                   schedule_value,
                   next_run_at::text as next_run_at,
                   last_run_at::text as last_run_at,
                   timeout_seconds,
                   max_concurrency,
                   (
                       select count(*)::bigint
                       from scheduled_task_runs runs
                       where runs.task_id = scheduled_tasks.id
                         and runs.status = 'running'
                         and runs.lease_expires_at > now()
                   ) as active_run_count,
                   (
                       select runs.public_id::text
                       from scheduled_task_runs runs
                       where runs.task_id = scheduled_tasks.id
                       order by runs.started_at desc, runs.id desc
                       limit 1
                   ) as last_run_id,
                   failure_count,
                   last_error,
                   created_at::text as created_at,
                   updated_at::text as updated_at
            from scheduled_tasks
            where public_id = case
                    when $1::text ~* '^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$'
                    then $1::uuid
                    else null::uuid
                end
               or task_key = $1
            limit 1
            "#,
        )
        .bind(id_or_key)
        .fetch_optional(&self.pool)
        .await?;

        row.map(ScheduledTaskAdminRecord::from_row).transpose()
    }

    pub async fn list_scheduled_task_runs(
        &self,
        task_key: &str,
        limit: i64,
    ) -> Result<Option<Vec<ScheduledTaskRunRecord>>, sqlx::Error> {
        let Some(task_id) = sqlx::query_scalar::<_, i64>(
            r#"
            select id
            from scheduled_tasks
            where task_key = $1
            "#,
        )
        .bind(task_key.trim())
        .fetch_optional(&self.pool)
        .await?
        else {
            return Ok(None);
        };

        let rows = sqlx::query(
            r#"
            select public_id::text as id,
                   task_key,
                   trigger_type,
                   worker_id,
                   status,
                   lease_expires_at::text as lease_expires_at,
                   (status = 'running' and lease_expires_at > now()) as lease_active,
                   queued_jobs,
                   error_message,
                   started_at::text as started_at,
                   finished_at::text as finished_at,
                   floor(extract(epoch from (coalesce(finished_at, now()) - started_at)) * 1000)::bigint as duration_ms,
                   created_at::text as created_at,
                   updated_at::text as updated_at
            from scheduled_task_runs
            where task_id = $1
            order by started_at desc, id desc
            limit $2
            "#,
        )
        .bind(task_id)
        .bind(limit)
        .fetch_all(&self.pool)
        .await?;

        rows.into_iter()
            .map(ScheduledTaskRunRecord::from_row)
            .collect::<Result<Vec<_>, _>>()
            .map(Some)
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct StoredLibraryPath {
    id: String,
    path: String,
    is_enabled: bool,
}

async fn insert_library_path(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    library_row_id: i64,
    path: &str,
) -> Result<StoredLibraryPath, sqlx::Error> {
    let path = path.trim();
    let normalized_path = normalize_path(path);
    let path_hash = sha256(normalized_path.as_bytes());
    let row = sqlx::query(
        r#"
        insert into library_paths (
            library_id,
            path,
            normalized_path,
            path_hash,
            is_enabled
        )
        values ($1, $2, $3, $4, true)
        on conflict (library_id, path_hash) do update
            set path = excluded.path,
                normalized_path = excluded.normalized_path,
                is_enabled = true,
                updated_at = now()
        returning
            id::text as id,
            path,
            is_enabled
        "#,
    )
    .bind(library_row_id)
    .bind(path)
    .bind(normalized_path)
    .bind(path_hash)
    .fetch_one(&mut **tx)
    .await?;

    Ok(StoredLibraryPath {
        id: row.try_get("id")?,
        path: row.try_get("path")?,
        is_enabled: row.try_get("is_enabled")?,
    })
}

async fn replace_notification_target_secrets(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    target_id: i64,
    secrets: &[TargetSecretInput],
    cipher: &SecretCipher,
) -> Result<(), sqlx::Error> {
    sqlx::query(
        r#"
        delete from notification_target_secrets
        where target_id = $1
        "#,
    )
    .bind(target_id)
    .execute(&mut **tx)
    .await?;

    for secret in secrets {
        let encrypted = cipher
            .encrypt(target_id, &secret.key, &secret.value)
            .map_err(secret_to_sqlx_error)?;
        sqlx::query(
            r#"
            insert into notification_target_secrets (
                target_id,
                secret_key,
                algorithm,
                nonce,
                ciphertext,
                value_hash
            )
            values ($1, $2, $3, $4, $5, $6)
            "#,
        )
        .bind(target_id)
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

fn normalize_path(path: &str) -> String {
    path.trim().replace('\\', "/").to_ascii_lowercase()
}

fn sha256(input: &[u8]) -> Vec<u8> {
    Sha256::digest(input).to_vec()
}

fn secret_to_sqlx_error(error: SecretError) -> sqlx::Error {
    sqlx::Error::Protocol(error.to_string())
}

impl From<sqlx::Error> for NotificationRetryError {
    fn from(error: sqlx::Error) -> Self {
        Self::Database(error)
    }
}

impl From<sqlx::Error> for PluginDispatchReplayError {
    fn from(error: sqlx::Error) -> Self {
        Self::Database(error)
    }
}

impl ManagedLibraryRecord {
    fn from_row(row: PgRow) -> Result<Self, sqlx::Error> {
        Ok(Self {
            id: row.try_get("public_id")?,
            name: row.try_get("name")?,
            library_type: row.try_get("library_type")?,
        })
    }
}

impl ScanJobRecord {
    fn from_row(row: PgRow) -> Result<Self, sqlx::Error> {
        Ok(Self {
            id: row.try_get("id")?,
            status: row.try_get("status")?,
            queue_name: row.try_get("queue_name")?,
            job_type: row.try_get("job_type")?,
        })
    }
}

impl MetadataRefreshJobRecord {
    fn from_row(row: PgRow) -> Result<Self, sqlx::Error> {
        Ok(Self {
            id: row.try_get("id")?,
            status: row.try_get("status")?,
            queue_name: row.try_get("queue_name")?,
            job_type: row.try_get("job_type")?,
            item_id: row.try_get("item_id")?,
        })
    }
}

impl LibraryMetadataRefreshQueueRecord {
    fn from_row(row: PgRow) -> Result<Self, sqlx::Error> {
        Ok(Self {
            library_id: row.try_get("library_public_id")?,
            queued_jobs: row.try_get("queued_jobs")?,
        })
    }
}

impl AdminUserRecord {
    fn from_row(row: PgRow) -> Result<Self, sqlx::Error> {
        Ok(Self {
            id: row.try_get("id")?,
            username: row.try_get("username")?,
            display_name: row.try_get("display_name")?,
            role_name: row.try_get("role_name")?,
            is_disabled: row.try_get("is_disabled")?,
            allow_download: row.try_get("allow_download")?,
            allow_transcode: row.try_get("allow_transcode")?,
            allow_new_device_login: row.try_get("allow_new_device_login")?,
            has_password: row.try_get("has_password")?,
            device_count: row.try_get("device_count")?,
            active_session_count: row.try_get("active_session_count")?,
            last_login_at: row.try_get("last_login_at")?,
            created_at: row.try_get("created_at")?,
            updated_at: row.try_get("updated_at")?,
        })
    }
}

impl AdminUserLibraryPermissionRecord {
    fn from_row(row: PgRow) -> Result<Self, sqlx::Error> {
        Ok(Self {
            library_id: row.try_get("library_id")?,
            library_name: row.try_get("library_name")?,
            library_type: row.try_get("library_type")?,
            is_hidden: row.try_get("is_hidden")?,
            permission_configured: row.try_get("permission_configured")?,
            can_view: row.try_get("can_view")?,
            can_download: row.try_get("can_download")?,
            can_transcode: row.try_get("can_transcode")?,
            effective_can_view: row.try_get("effective_can_view")?,
            effective_can_download: row.try_get("effective_can_download")?,
            effective_can_transcode: row.try_get("effective_can_transcode")?,
            permission_updated_at: row.try_get("permission_updated_at")?,
        })
    }
}

impl AdminJobRecord {
    fn from_row(row: PgRow) -> Result<Self, sqlx::Error> {
        Ok(Self {
            id: row.try_get("id")?,
            job_type: row.try_get("job_type")?,
            status: row.try_get("status")?,
            queue_name: row.try_get("queue_name")?,
            priority: row.try_get("priority")?,
            payload: row.try_get("payload")?,
            dedupe_key: row.try_get("dedupe_key")?,
            run_at: row.try_get("run_at")?,
            locked_by: row.try_get("locked_by")?,
            locked_until: row.try_get("locked_until")?,
            lock_active: row.try_get("lock_active")?,
            attempts: row.try_get("attempts")?,
            max_attempts: row.try_get("max_attempts")?,
            last_error: row.try_get("last_error")?,
            created_at: row.try_get("created_at")?,
            updated_at: row.try_get("updated_at")?,
            finished_at: row.try_get("finished_at")?,
        })
    }
}

impl AdminJobRunRecord {
    fn from_row(row: PgRow) -> Result<Self, sqlx::Error> {
        Ok(Self {
            id: row.try_get("id")?,
            worker_id: row.try_get("worker_id")?,
            status: row.try_get("status")?,
            started_at: row.try_get("started_at")?,
            finished_at: row.try_get("finished_at")?,
            duration_ms: row.try_get("duration_ms")?,
            error_message: row.try_get("error_message")?,
            metrics: row.try_get("metrics")?,
        })
    }
}

impl AdminJobEventRecord {
    fn from_row(row: PgRow) -> Result<Self, sqlx::Error> {
        Ok(Self {
            id: row.try_get("id")?,
            run_id: row.try_get("run_id")?,
            event_type: row.try_get("event_type")?,
            event_level: row.try_get("event_level")?,
            message: row.try_get("message")?,
            payload: row.try_get("payload")?,
            created_at: row.try_get("created_at")?,
        })
    }
}

impl PluginDispatchRecord {
    fn from_row(row: PgRow) -> Result<Self, sqlx::Error> {
        Ok(Self {
            id: row.try_get("id")?,
            plugin_id: row.try_get("plugin_id")?,
            package_id: row.try_get("package_id")?,
            hook_id: row.try_get("hook_id")?,
            handler: row.try_get("handler")?,
            hook_event: row.try_get("hook_event")?,
            aggregate_type: row.try_get("aggregate_type")?,
            aggregate_id: row.try_get("aggregate_id")?,
            payload: row.try_get("payload")?,
            status: row.try_get("status")?,
            attempts: row.try_get("attempts")?,
            max_attempts: row.try_get("max_attempts")?,
            available_at: row.try_get("available_at")?,
            locked_until: row.try_get("locked_until")?,
            last_error: row.try_get("last_error")?,
            created_at: row.try_get("created_at")?,
            delivered_at: row.try_get("delivered_at")?,
        })
    }
}

impl PluginExecutionRunRecord {
    fn from_row(row: PgRow) -> Result<Self, sqlx::Error> {
        Ok(Self {
            id: row.try_get("id")?,
            dispatch_id: row.try_get("dispatch_id")?,
            outbox_event_id: row.try_get("outbox_event_id")?,
            attempt: row.try_get("attempt")?,
            plugin_id: row.try_get("plugin_id")?,
            package_id: row.try_get("package_id")?,
            hook_id: row.try_get("hook_id")?,
            handler: row.try_get("handler")?,
            event_key: row.try_get("event_key")?,
            runtime: row.try_get("runtime")?,
            entrypoint: row.try_get("entrypoint")?,
            status: row.try_get("status")?,
            request_payload: row.try_get("request_payload")?,
            response_status: row.try_get("response_status")?,
            response_body: row.try_get("response_body")?,
            error_message: row.try_get("error_message")?,
            started_at: row.try_get("started_at")?,
            finished_at: row.try_get("finished_at")?,
            duration_ms: row.try_get("duration_ms")?,
        })
    }
}

impl PluginHostApiCallRecord {
    fn from_row(row: PgRow) -> Result<Self, sqlx::Error> {
        Ok(Self {
            id: row.try_get("id")?,
            plugin_id: row.try_get("plugin_id")?,
            package_id: row.try_get("package_id")?,
            host_token_id: row.try_get("host_token_id")?,
            execution_run_id: row.try_get("execution_run_id")?,
            method: row.try_get("method")?,
            path: row.try_get("path")?,
            required_permission: row.try_get("required_permission")?,
            status_code: row.try_get("status_code")?,
            error_code: row.try_get("error_code")?,
            error_message: row.try_get("error_message")?,
            started_at: row.try_get("started_at")?,
            finished_at: row.try_get("finished_at")?,
            duration_ms: row.try_get("duration_ms")?,
        })
    }
}

impl ScheduledTaskAdminRecord {
    fn from_row(row: PgRow) -> Result<Self, sqlx::Error> {
        Ok(Self {
            id: row.try_get("id")?,
            task_key: row.try_get("task_key")?,
            task_type: row.try_get("task_type")?,
            owner_type: row.try_get("owner_type")?,
            owner_id: row.try_get("owner_id")?,
            enabled: row.try_get("enabled")?,
            schedule_kind: row.try_get("schedule_kind")?,
            schedule_value: row.try_get("schedule_value")?,
            next_run_at: row.try_get("next_run_at")?,
            last_run_at: row.try_get("last_run_at")?,
            timeout_seconds: row.try_get("timeout_seconds")?,
            max_concurrency: row.try_get("max_concurrency")?,
            active_run_count: row.try_get("active_run_count")?,
            last_run_id: row.try_get("last_run_id")?,
            failure_count: row.try_get("failure_count")?,
            last_error: row.try_get("last_error")?,
            created_at: row.try_get("created_at")?,
            updated_at: row.try_get("updated_at")?,
        })
    }
}

impl ScheduledTaskRunRecord {
    fn from_row(row: PgRow) -> Result<Self, sqlx::Error> {
        Ok(Self {
            id: row.try_get("id")?,
            task_key: row.try_get("task_key")?,
            trigger_type: row.try_get("trigger_type")?,
            worker_id: row.try_get("worker_id")?,
            status: row.try_get("status")?,
            lease_expires_at: row.try_get("lease_expires_at")?,
            lease_active: row.try_get("lease_active")?,
            queued_jobs: row.try_get("queued_jobs")?,
            error_message: row.try_get("error_message")?,
            started_at: row.try_get("started_at")?,
            finished_at: row.try_get("finished_at")?,
            duration_ms: row.try_get("duration_ms")?,
            created_at: row.try_get("created_at")?,
            updated_at: row.try_get("updated_at")?,
        })
    }
}

impl NotificationRequestRecord {
    fn from_row(row: PgRow) -> Result<Self, sqlx::Error> {
        Ok(Self {
            id: row.try_get("id")?,
            plugin_id: row.try_get("plugin_id")?,
            package_id: row.try_get("package_id")?,
            title: row.try_get("title")?,
            message: row.try_get("message")?,
            level: row.try_get("level")?,
            channel: row.try_get("channel")?,
            metadata: row.try_get("metadata")?,
            status: row.try_get("status")?,
            outbox_event_id: row.try_get("outbox_event_id")?,
            last_error: row.try_get("last_error")?,
            created_at: row.try_get("created_at")?,
            updated_at: row.try_get("updated_at")?,
        })
    }
}

impl NotificationDeliveryAttemptRecord {
    fn from_row(row: PgRow) -> Result<Self, sqlx::Error> {
        Ok(Self {
            id: row.try_get("id")?,
            request_id: row.try_get("request_id")?,
            outbox_event_id: row.try_get("outbox_event_id")?,
            target_id: row.try_get("target_id")?,
            target_type: row.try_get("target_type")?,
            target_name: row.try_get("target_name")?,
            attempt: row.try_get("attempt")?,
            status: row.try_get("status")?,
            response_status: row.try_get("response_status")?,
            error_message: row.try_get("error_message")?,
            duration_ms: row.try_get("duration_ms")?,
            created_at: row.try_get("created_at")?,
            finished_at: row.try_get("finished_at")?,
        })
    }
}

impl NotificationTargetRecord {
    fn from_row(row: PgRow) -> Result<Self, sqlx::Error> {
        Ok(Self {
            id: row.try_get("id")?,
            name: row.try_get("name")?,
            target_type: row.try_get("target_type")?,
            channel: row.try_get("channel")?,
            config: row.try_get("config")?,
            is_enabled: row.try_get("is_enabled")?,
            delivery_count: row.try_get("delivery_count")?,
            failure_count: row.try_get("failure_count")?,
            last_error: row.try_get("last_error")?,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalize_path_is_stable_for_dedupe() {
        assert_eq!(normalize_path(" D:\\Media\\Movies "), "d:/media/movies");
    }

    #[test]
    fn sha256_returns_postgres_bytea_ready_hash() {
        assert_eq!(sha256(b"path").len(), 32);
    }

    #[test]
    fn scheduled_task_recent_index_matches_admin_queries() {
        let migration = include_str!("../../migrations/0038_scheduled_task_run_recent_index.sql");
        let repository = include_str!("repository.rs");

        assert!(migration.contains("idx_scheduled_task_runs_task_recent"));
        assert!(migration.contains("task_id, started_at desc, id desc"));
        assert!(migration.contains("include (public_id)"));
        assert!(repository.contains("from scheduled_task_runs runs"));
        assert!(repository.contains("order by runs.started_at desc, runs.id desc"));
        assert!(repository.contains("from scheduled_task_runs"));
        assert!(repository.contains("order by started_at desc, id desc"));
    }

    #[test]
    fn notification_admin_indexes_match_recent_audit_queries() {
        let migration = include_str!("../../migrations/0039_notification_admin_recent_indexes.sql");
        let repository = include_str!("repository.rs");

        assert!(migration.contains("idx_plugin_notification_requests_recent"));
        assert!(migration.contains("created_at desc, id desc"));
        assert!(migration.contains("idx_notification_delivery_attempts_request_recent"));
        assert!(migration.contains("notification_request_id, created_at desc, id desc"));
        assert!(repository.contains("from plugin_notification_requests"));
        assert!(repository.contains("order by created_at desc, id desc"));
        assert!(repository.contains("$2::text as request_id"));
        assert!(repository.contains("where attempts.notification_request_id = $1"));
    }

    #[test]
    fn admin_public_id_entrypoints_keep_uuid_index_shape() {
        let repository = include_str!("repository.rs");
        let bad_public_id_filter = format!("{}{}", "where public_id::text = ", "$1");
        let bad_run_filter = format!("{}{}", "run.public_id::text = ", "$2");

        assert!(!repository.contains(&bad_public_id_filter));
        assert!(!repository.contains(&bad_run_filter));
        assert!(
            repository
                .contains("from plugin_notification_requests\n            where public_id = case")
        );
        assert!(repository.contains("from event_outbox\n                where public_id = case"));
        assert!(repository.contains("or run.public_id = case"));
        assert!(repository.contains("from scheduled_tasks\n            where public_id = case"));
    }

    #[test]
    fn admin_queue_public_id_inputs_use_uuid_comparisons() {
        assert!(ADMIN_LIBRARY_BY_PUBLIC_ID_SQL.contains("where public_id = case"));
        assert!(ADMIN_LIBRARY_BY_PUBLIC_ID_SQL.contains("$1::uuid"));
        assert!(!ADMIN_LIBRARY_BY_PUBLIC_ID_SQL.contains("public_id::text = $1"));

        assert!(ADMIN_QUEUE_METADATA_REFRESH_ITEM_SQL.contains("where public_id = case"));
        assert!(ADMIN_QUEUE_METADATA_REFRESH_ITEM_SQL.contains("$1::uuid"));
        assert!(!ADMIN_QUEUE_METADATA_REFRESH_ITEM_SQL.contains("public_id::text = $1"));
        assert!(
            ADMIN_QUEUE_METADATA_REFRESH_ITEM_SQL
                .contains("target_item.item_public_id = j.payload->>'itemId'")
        );

        assert!(ADMIN_QUEUE_METADATA_REFRESH_LIBRARY_SQL.contains("where public_id = case"));
        assert!(ADMIN_QUEUE_METADATA_REFRESH_LIBRARY_SQL.contains("$1::uuid"));
        assert!(!ADMIN_QUEUE_METADATA_REFRESH_LIBRARY_SQL.contains("public_id::text = $1"));
        assert!(
            ADMIN_QUEUE_METADATA_REFRESH_LIBRARY_SQL
                .contains("j.payload->>'itemId' = mi.public_id::text")
        );
    }
}
