use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use sqlx::{Postgres, QueryBuilder, Row, postgres::PgRow};

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

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct AdminUserFilter {
    pub role_name: Option<String>,
    pub is_disabled: Option<bool>,
    pub cursor: Option<String>,
    pub limit: i64,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AdminUserPage {
    pub records: Vec<AdminUserRecord>,
    pub next_cursor: Option<String>,
    pub has_more: bool,
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

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct AdminUserLibraryPermissionFilter {
    pub library_type: Option<String>,
    pub permission_configured: Option<bool>,
    pub cursor: Option<String>,
    pub limit: i64,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AdminUserLibraryPermissionPage {
    pub records: Vec<AdminUserLibraryPermissionRecord>,
    pub next_cursor: Option<String>,
    pub has_more: bool,
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

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct AdminJobFilter {
    pub status: Option<String>,
    pub job_type: Option<String>,
    pub queue_name: Option<String>,
    pub cursor: Option<String>,
    pub limit: i64,
}

#[derive(Clone, Debug, PartialEq)]
pub struct AdminJobPage {
    pub records: Vec<AdminJobRecord>,
    pub next_cursor: Option<String>,
    pub has_more: bool,
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

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct AdminJobRunFilter {
    pub status: Option<String>,
    pub cursor: Option<i64>,
    pub limit: i64,
}

#[derive(Clone, Debug, PartialEq)]
pub struct AdminJobRunPage {
    pub records: Vec<AdminJobRunRecord>,
    pub next_cursor: Option<String>,
    pub has_more: bool,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct AdminJobEventFilter {
    pub event_level: Option<String>,
    pub cursor: Option<i64>,
    pub limit: i64,
}

#[derive(Clone, Debug, PartialEq)]
pub struct AdminJobEventPage {
    pub records: Vec<AdminJobEventRecord>,
    pub next_cursor: Option<String>,
    pub has_more: bool,
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

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct NotificationTargetFilter {
    pub target_type: Option<String>,
    pub channel: Option<String>,
    pub is_enabled: Option<bool>,
    pub cursor: Option<String>,
    pub limit: i64,
}

#[derive(Clone, Debug, PartialEq)]
pub struct NotificationTargetPage {
    pub records: Vec<NotificationTargetRecord>,
    pub next_cursor: Option<String>,
    pub has_more: bool,
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

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct NotificationRequestFilter {
    pub status: Option<String>,
    pub channel: Option<String>,
    pub cursor: Option<String>,
    pub limit: i64,
}

#[derive(Clone, Debug, PartialEq)]
pub struct NotificationRequestPage {
    pub records: Vec<NotificationRequestRecord>,
    pub next_cursor: Option<String>,
    pub has_more: bool,
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

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct NotificationDeliveryAttemptFilter {
    pub status: Option<String>,
    pub cursor: Option<String>,
    pub limit: i64,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NotificationDeliveryAttemptPage {
    pub records: Vec<NotificationDeliveryAttemptRecord>,
    pub next_cursor: Option<String>,
    pub has_more: bool,
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

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct PluginDispatchFilter {
    pub status: Option<String>,
    pub cursor: Option<String>,
    pub limit: i64,
}

#[derive(Clone, Debug, PartialEq)]
pub struct PluginDispatchPage {
    pub records: Vec<PluginDispatchRecord>,
    pub next_cursor: Option<String>,
    pub has_more: bool,
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

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct PluginExecutionRunFilter {
    pub status: Option<String>,
    pub cursor: Option<String>,
    pub limit: i64,
}

#[derive(Clone, Debug, PartialEq)]
pub struct PluginExecutionRunPage {
    pub records: Vec<PluginExecutionRunRecord>,
    pub next_cursor: Option<String>,
    pub has_more: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PluginHostApiCallFilter {
    pub plugin_id: Option<String>,
    pub execution_run_id: Option<String>,
    pub status_code: Option<i32>,
    pub cursor: Option<String>,
    pub limit: i64,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PluginHostApiCallPage {
    pub records: Vec<PluginHostApiCallRecord>,
    pub next_cursor: Option<String>,
    pub has_more: bool,
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
pub struct EventStreamMirrorStatusRecord {
    pub unmirrored_count: i64,
    pub claimable_count: i64,
    pub locked_count: i64,
    pub backoff_count: i64,
    pub failed_count: i64,
    pub max_attempts: i32,
    pub oldest_unmirrored_created_at: Option<String>,
    pub next_retry_at: Option<String>,
    pub last_error: Option<String>,
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

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ScheduledTaskFilter {
    pub task_type: Option<String>,
    pub owner_type: Option<String>,
    pub enabled: Option<bool>,
    pub cursor: Option<String>,
    pub limit: i64,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ScheduledTaskPage {
    pub records: Vec<ScheduledTaskAdminRecord>,
    pub next_cursor: Option<String>,
    pub has_more: bool,
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

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ScheduledTaskRunFilter {
    pub status: Option<String>,
    pub cursor: Option<String>,
    pub limit: i64,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ScheduledTaskRunPage {
    pub records: Vec<ScheduledTaskRunRecord>,
    pub next_cursor: Option<String>,
    pub has_more: bool,
}

#[derive(Debug)]
pub enum PluginDispatchReplayError {
    NotFound,
    InvalidStatus(String),
    Database(sqlx::Error),
}

const ADMIN_EVENT_STREAM_MIRROR_STATUS_SQL: &str = r#"
            with unmirrored as (
                select id,
                       created_at,
                       stream_mirror_attempts,
                       stream_mirror_locked_by,
                       stream_mirror_locked_until,
                       stream_mirror_last_error
                from event_outbox
                where stream_mirrored_at is null
            ),
            newest_error as (
                select stream_mirror_last_error
                from unmirrored
                where stream_mirror_last_error is not null
                order by id desc
                limit 1
            )
            select count(*)::bigint as unmirrored_count,
                   count(*) filter (
                       where stream_mirror_locked_until is null
                          or stream_mirror_locked_until <= now()
                   )::bigint as claimable_count,
                   count(*) filter (
                       where stream_mirror_locked_by is not null
                         and stream_mirror_locked_until > now()
                   )::bigint as locked_count,
                   count(*) filter (
                       where stream_mirror_locked_by is null
                         and stream_mirror_locked_until > now()
                   )::bigint as backoff_count,
                   count(*) filter (
                       where stream_mirror_last_error is not null
                   )::bigint as failed_count,
                   coalesce(max(stream_mirror_attempts), 0)::integer as max_attempts,
                   min(created_at)::text as oldest_unmirrored_created_at,
                   min(stream_mirror_locked_until) filter (
                       where stream_mirror_locked_until > now()
                   )::text as next_retry_at,
                   (select stream_mirror_last_error from newest_error) as last_error
            from unmirrored
            "#;

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
        self.list_admin_users_page(AdminUserFilter {
            role_name: None,
            is_disabled: None,
            cursor: None,
            limit,
        })
        .await
        .map(|page| page.records)
    }

    pub async fn list_admin_users_page(
        &self,
        filter: AdminUserFilter,
    ) -> Result<AdminUserPage, sqlx::Error> {
        let page_limit = filter.limit.max(1);
        let fetch_limit = page_limit.saturating_add(1);
        let mut query = QueryBuilder::<Postgres>::new(
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
            "#,
        );

        if let Some(cursor) = filter.cursor.as_deref() {
            query.push(
                r#"
                join users cursor_user
                  on cursor_user.public_id = case
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

        query.push(" where true");

        if let Some(role_name) = filter.role_name.as_deref() {
            query.push(" and r.name_normalized = ");
            query.push_bind(role_name);
        }

        if let Some(is_disabled) = filter.is_disabled {
            query.push(" and u.is_disabled = ");
            query.push_bind(is_disabled);
        }

        if filter.cursor.is_some() {
            query.push(
                " and (u.username_normalized, u.id) > (cursor_user.username_normalized, cursor_user.id)",
            );
        }

        query.push(" order by u.username_normalized asc, u.id asc limit ");
        query.push_bind(fetch_limit);

        let rows = query.build().fetch_all(&self.pool).await?;

        let has_more = rows.len() as i64 > page_limit;
        let mut records = rows
            .into_iter()
            .map(AdminUserRecord::from_row)
            .collect::<Result<Vec<_>, _>>()?;
        if has_more {
            records.truncate(page_limit as usize);
        }
        let next_cursor = has_more
            .then(|| records.last().map(|record| record.id.clone()))
            .flatten();

        Ok(AdminUserPage {
            records,
            next_cursor,
            has_more,
        })
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
        self.list_user_library_permissions_page(
            user_id,
            AdminUserLibraryPermissionFilter {
                library_type: None,
                permission_configured: None,
                cursor: None,
                limit,
            },
        )
        .await
        .map(|page| page.map(|page| page.records))
    }

    pub async fn list_user_library_permissions_page(
        &self,
        user_id: &str,
        filter: AdminUserLibraryPermissionFilter,
    ) -> Result<Option<AdminUserLibraryPermissionPage>, sqlx::Error> {
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
        let page_limit = filter.limit.max(1);
        let fetch_limit = page_limit.saturating_add(1);
        let mut query = QueryBuilder::<Postgres>::new(
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
            "#,
        );
        query.push("(");
        query.push_bind(allow_download);
        query.push(
            r#"::boolean and coalesce(lp.can_download, false) and not l.is_hidden) as effective_can_download,
                   ("#,
        );
        query.push_bind(allow_transcode);
        query.push(
            r#"::boolean and coalesce(lp.can_transcode, false) and not l.is_hidden) as effective_can_transcode,
                   lp.updated_at::text as permission_updated_at
            from libraries l
            left join library_permissions lp
              on lp.library_id = l.id
            "#,
        );
        query.push(" and lp.user_id = ");
        query.push_bind(user_row_id);

        if let Some(cursor) = filter.cursor.as_deref() {
            query.push(
                r#"
                join libraries cursor_library
                  on cursor_library.public_id = case
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

        query.push(" where true");

        if let Some(library_type) = filter.library_type.as_deref() {
            query.push(" and l.library_type = ");
            query.push_bind(library_type);
        }

        if let Some(permission_configured) = filter.permission_configured {
            if permission_configured {
                query.push(" and lp.id is not null");
            } else {
                query.push(" and lp.id is null");
            }
        }

        if filter.cursor.is_some() {
            query.push(" and (l.name, l.id) > (cursor_library.name, cursor_library.id)");
        }

        query.push(" order by l.name asc, l.id asc limit ");
        query.push_bind(fetch_limit);

        let rows = query.build().fetch_all(&self.pool).await?;

        let has_more = rows.len() as i64 > page_limit;
        let mut records = rows
            .into_iter()
            .map(AdminUserLibraryPermissionRecord::from_row)
            .collect::<Result<Vec<_>, _>>()?;
        if has_more {
            records.truncate(page_limit as usize);
        }
        let next_cursor = has_more
            .then(|| records.last().map(|record| record.library_id.clone()))
            .flatten();

        Ok(Some(AdminUserLibraryPermissionPage {
            records,
            next_cursor,
            has_more,
        }))
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
        self.list_admin_jobs_page(AdminJobFilter {
            status: None,
            job_type: None,
            queue_name: None,
            cursor: None,
            limit,
        })
        .await
        .map(|page| page.records)
    }

    pub async fn list_admin_jobs_page(
        &self,
        filter: AdminJobFilter,
    ) -> Result<AdminJobPage, sqlx::Error> {
        let page_limit = filter.limit.max(1);
        let fetch_limit = page_limit.saturating_add(1);
        let mut query = QueryBuilder::<Postgres>::new(
            r#"
            select jobs.public_id::text as id,
                   jobs.job_type,
                   jobs.status,
                   jobs.queue_name,
                   jobs.priority,
                   jobs.payload,
                   jobs.dedupe_key,
                   jobs.run_at::text as run_at,
                   jobs.locked_by,
                   jobs.locked_until::text as locked_until,
                   (jobs.status = 'running' and jobs.locked_until > now()) as lock_active,
                   jobs.attempts,
                   jobs.max_attempts,
                   jobs.last_error,
                   jobs.created_at::text as created_at,
                   jobs.updated_at::text as updated_at,
                   jobs.finished_at::text as finished_at
            from jobs jobs
            "#,
        );

        if let Some(cursor) = filter.cursor.as_deref() {
            query.push(
                r#"
                join jobs cursor_job
                  on cursor_job.public_id = case
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

        query.push(" where true");

        if let Some(status) = filter.status.as_deref() {
            query.push(" and jobs.status = ");
            query.push_bind(status);
        }

        if let Some(job_type) = filter.job_type.as_deref() {
            query.push(" and jobs.job_type = ");
            query.push_bind(job_type);
        }

        if let Some(queue_name) = filter.queue_name.as_deref() {
            query.push(" and jobs.queue_name = ");
            query.push_bind(queue_name);
        }

        if filter.cursor.is_some() {
            query.push(" and (jobs.created_at, jobs.id) < (cursor_job.created_at, cursor_job.id)");
        }

        query.push(" order by jobs.created_at desc, jobs.id desc limit ");
        query.push_bind(fetch_limit);

        let rows = query.build().fetch_all(&self.pool).await?;

        let has_more = rows.len() as i64 > page_limit;
        let mut records = rows
            .into_iter()
            .map(AdminJobRecord::from_row)
            .collect::<Result<Vec<_>, _>>()?;
        if has_more {
            records.truncate(page_limit as usize);
        }
        let next_cursor = has_more
            .then(|| records.last().map(|record| record.id.clone()))
            .flatten();

        Ok(AdminJobPage {
            records,
            next_cursor,
            has_more,
        })
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

    pub async fn list_admin_job_runs_page(
        &self,
        job_id: &str,
        filter: AdminJobRunFilter,
    ) -> Result<Option<AdminJobRunPage>, sqlx::Error> {
        let Some(row_id) = sqlx::query_scalar::<_, i64>(
            r#"
            select id
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

        let fetch_limit = filter.limit.saturating_add(1);
        let mut query = QueryBuilder::<Postgres>::new(
            r#"
            select runs.id,
                   runs.worker_id,
                   runs.status,
                   runs.started_at::text as started_at,
                   runs.finished_at::text as finished_at,
                   floor(extract(epoch from (coalesce(runs.finished_at, now()) - runs.started_at)) * 1000)::bigint as duration_ms,
                   runs.error_message,
                   runs.metrics
            from job_runs runs
            "#,
        );

        if let Some(cursor_id) = filter.cursor {
            query.push(" join job_runs cursor_run on cursor_run.id = ");
            query.push_bind(cursor_id);
            query.push(" and cursor_run.job_id = ");
            query.push_bind(row_id);
        }

        query.push(" where runs.job_id = ");
        query.push_bind(row_id);

        if let Some(status) = filter.status.as_deref() {
            query.push(" and runs.status = ");
            query.push_bind(status);
        }

        if filter.cursor.is_some() {
            query.push(" and (runs.started_at, runs.id) < (cursor_run.started_at, cursor_run.id)");
        }

        query.push(" order by runs.started_at desc, runs.id desc limit ");
        query.push_bind(fetch_limit);

        let rows = query.build().fetch_all(&self.pool).await?;

        let has_more = rows.len() as i64 > filter.limit;
        let mut records = rows
            .into_iter()
            .map(AdminJobRunRecord::from_row)
            .collect::<Result<Vec<_>, _>>()?;
        if has_more {
            records.truncate(filter.limit as usize);
        }
        let next_cursor = has_more
            .then(|| records.last().map(|record| record.id.to_string()))
            .flatten();

        Ok(Some(AdminJobRunPage {
            records,
            next_cursor,
            has_more,
        }))
    }

    pub async fn list_admin_job_events_page(
        &self,
        job_id: &str,
        filter: AdminJobEventFilter,
    ) -> Result<Option<AdminJobEventPage>, sqlx::Error> {
        let Some(row_id) = sqlx::query_scalar::<_, i64>(
            r#"
            select id
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

        let fetch_limit = filter.limit.saturating_add(1);
        let mut query = QueryBuilder::<Postgres>::new(
            r#"
            select events.id,
                   events.job_run_id as run_id,
                   events.event_type,
                   events.event_level,
                   events.message,
                   events.payload,
                   events.created_at::text as created_at
            from job_events events
            "#,
        );

        if let Some(cursor_id) = filter.cursor {
            query.push(" join job_events cursor_event on cursor_event.id = ");
            query.push_bind(cursor_id);
            query.push(" and cursor_event.job_id = ");
            query.push_bind(row_id);
        }

        query.push(" where events.job_id = ");
        query.push_bind(row_id);

        if let Some(event_level) = filter.event_level.as_deref() {
            query.push(" and events.event_level = ");
            query.push_bind(event_level);
        }

        if filter.cursor.is_some() {
            query.push(
                " and (events.created_at, events.id) < (cursor_event.created_at, cursor_event.id)",
            );
        }

        query.push(" order by events.created_at desc, events.id desc limit ");
        query.push_bind(fetch_limit);

        let rows = query.build().fetch_all(&self.pool).await?;

        let has_more = rows.len() as i64 > filter.limit;
        let mut records = rows
            .into_iter()
            .map(AdminJobEventRecord::from_row)
            .collect::<Result<Vec<_>, _>>()?;
        if has_more {
            records.truncate(filter.limit as usize);
        }
        let next_cursor = has_more
            .then(|| records.last().map(|record| record.id.to_string()))
            .flatten();

        Ok(Some(AdminJobEventPage {
            records,
            next_cursor,
            has_more,
        }))
    }

    pub async fn list_notification_targets(
        &self,
        limit: i64,
    ) -> Result<Vec<NotificationTargetRecord>, sqlx::Error> {
        self.list_notification_targets_page(NotificationTargetFilter {
            target_type: None,
            channel: None,
            is_enabled: None,
            cursor: None,
            limit,
        })
        .await
        .map(|page| page.records)
    }

    pub async fn list_notification_targets_page(
        &self,
        filter: NotificationTargetFilter,
    ) -> Result<NotificationTargetPage, sqlx::Error> {
        let page_limit = filter.limit.max(1);
        let fetch_limit = page_limit.saturating_add(1);
        let mut query = QueryBuilder::<Postgres>::new(
            r#"
            select targets.public_id::text as id,
                   targets.name,
                   targets.target_type,
                   targets.channel,
                   targets.config,
                   targets.is_enabled,
                   targets.delivery_count,
                   targets.failure_count,
                   targets.last_error
            from notification_targets targets
            "#,
        );

        if let Some(cursor) = filter.cursor.as_deref() {
            query.push(
                r#"
                join notification_targets cursor_target
                  on cursor_target.public_id = case
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

        query.push(" where true");

        if let Some(target_type) = filter.target_type.as_deref() {
            query.push(" and targets.target_type = ");
            query.push_bind(target_type);
        }

        if let Some(channel) = filter.channel.as_deref() {
            query.push(" and targets.channel = ");
            query.push_bind(channel);
        }

        if let Some(is_enabled) = filter.is_enabled {
            query.push(" and targets.is_enabled = ");
            query.push_bind(is_enabled);
        }

        if filter.cursor.is_some() {
            query.push(
                " and (targets.target_type, targets.name, targets.id) > (cursor_target.target_type, cursor_target.name, cursor_target.id)",
            );
        }

        query.push(" order by targets.target_type asc, targets.name asc, targets.id asc limit ");
        query.push_bind(fetch_limit);

        let rows = query.build().fetch_all(&self.pool).await?;

        let has_more = rows.len() as i64 > page_limit;
        let mut records = rows
            .into_iter()
            .map(NotificationTargetRecord::from_row)
            .collect::<Result<Vec<_>, _>>()?;
        if has_more {
            records.truncate(page_limit as usize);
        }
        let next_cursor = has_more
            .then(|| records.last().map(|record| record.id.clone()))
            .flatten();

        Ok(NotificationTargetPage {
            records,
            next_cursor,
            has_more,
        })
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
        self.list_notification_requests_page(NotificationRequestFilter {
            status: None,
            channel: None,
            cursor: None,
            limit,
        })
        .await
        .map(|page| page.records)
    }

    pub async fn list_notification_requests_page(
        &self,
        filter: NotificationRequestFilter,
    ) -> Result<NotificationRequestPage, sqlx::Error> {
        let fetch_limit = filter.limit.saturating_add(1);
        let mut query = QueryBuilder::<Postgres>::new(
            r#"
            select requests.public_id::text as id,
                   requests.plugin_id,
                   requests.package_id,
                   requests.title,
                   requests.message,
                   requests.level,
                   requests.channel,
                   requests.metadata,
                   requests.status,
                   requests.outbox_event_id,
                   requests.last_error,
                   requests.created_at::text as created_at,
                   requests.updated_at::text as updated_at
            from plugin_notification_requests requests
            "#,
        );

        if let Some(cursor) = filter.cursor.as_deref() {
            query.push(
                r#"
                join plugin_notification_requests cursor_request
                  on cursor_request.public_id = case
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

        query.push(" where true");

        if let Some(status) = filter.status.as_deref() {
            query.push(" and requests.status = ");
            query.push_bind(status);
        }

        if let Some(channel) = filter.channel.as_deref() {
            query.push(" and requests.channel = ");
            query.push_bind(channel);
        }

        if filter.cursor.is_some() {
            query.push(
                " and (requests.created_at, requests.id) < (cursor_request.created_at, cursor_request.id)",
            );
        }

        query.push(" order by requests.created_at desc, requests.id desc limit ");
        query.push_bind(fetch_limit);

        let rows = query.build().fetch_all(&self.pool).await?;

        let has_more = rows.len() as i64 > filter.limit;
        let mut records = rows
            .into_iter()
            .map(NotificationRequestRecord::from_row)
            .collect::<Result<Vec<_>, _>>()?;
        if has_more {
            records.truncate(filter.limit as usize);
        }
        let next_cursor = has_more
            .then(|| records.last().map(|record| record.id.clone()))
            .flatten();

        Ok(NotificationRequestPage {
            records,
            next_cursor,
            has_more,
        })
    }

    pub async fn list_notification_delivery_attempts(
        &self,
        request_id: &str,
        limit: i64,
    ) -> Result<Option<Vec<NotificationDeliveryAttemptRecord>>, sqlx::Error> {
        self.list_notification_delivery_attempts_page(
            request_id,
            NotificationDeliveryAttemptFilter {
                status: None,
                cursor: None,
                limit,
            },
        )
        .await
        .map(|page| page.map(|page| page.records))
    }

    pub async fn list_notification_delivery_attempts_page(
        &self,
        request_id: &str,
        filter: NotificationDeliveryAttemptFilter,
    ) -> Result<Option<NotificationDeliveryAttemptPage>, sqlx::Error> {
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

        let fetch_limit = filter.limit.saturating_add(1);
        let mut query = QueryBuilder::<Postgres>::new(
            r#"
            select attempts.public_id::text as id,
                   "#,
        );
        query.push_bind(public_request_id);
        query.push(
            r#"::text as request_id,
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
            "#,
        );

        if let Some(cursor) = filter.cursor.as_deref() {
            query.push(
                r#"
                join notification_delivery_attempts cursor_attempt
                  on cursor_attempt.public_id = case
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

        query.push(" where attempts.notification_request_id = ");
        query.push_bind(internal_request_id);

        if let Some(status) = filter.status.as_deref() {
            query.push(" and attempts.status = ");
            query.push_bind(status);
        }

        if filter.cursor.is_some() {
            query.push(
                " and (attempts.created_at, attempts.id) < (cursor_attempt.created_at, cursor_attempt.id)",
            );
        }

        query.push(" order by attempts.created_at desc, attempts.id desc limit ");
        query.push_bind(fetch_limit);

        let rows = query.build().fetch_all(&self.pool).await?;

        let has_more = rows.len() as i64 > filter.limit;
        let mut records = rows
            .into_iter()
            .map(NotificationDeliveryAttemptRecord::from_row)
            .collect::<Result<Vec<_>, _>>()?;
        if has_more {
            records.truncate(filter.limit as usize);
        }
        let next_cursor = has_more
            .then(|| records.last().map(|record| record.id.clone()))
            .flatten();

        Ok(Some(NotificationDeliveryAttemptPage {
            records,
            next_cursor,
            has_more,
        }))
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
        self.list_plugin_dispatches_page(PluginDispatchFilter {
            status: None,
            cursor: None,
            limit,
        })
        .await
        .map(|page| page.records)
    }

    pub async fn list_plugin_dispatches_page(
        &self,
        filter: PluginDispatchFilter,
    ) -> Result<PluginDispatchPage, sqlx::Error> {
        let fetch_limit = filter.limit.saturating_add(1);
        let mut query = QueryBuilder::<Postgres>::new(
            r#"
            select outbox.public_id::text as id,
                   outbox.payload->>'pluginId' as plugin_id,
                   outbox.payload->>'packageId' as package_id,
                   outbox.payload->>'hookId' as hook_id,
                   outbox.payload->>'handler' as handler,
                   outbox.payload->>'hookEvent' as hook_event,
                   outbox.aggregate_type,
                   outbox.aggregate_id,
                   outbox.payload,
                   outbox.status,
                   outbox.attempts,
                   outbox.max_attempts,
                   outbox.available_at::text as available_at,
                   outbox.locked_until::text as locked_until,
                   outbox.last_error,
                   outbox.created_at::text as created_at,
                   outbox.delivered_at::text as delivered_at
            from event_outbox outbox
            "#,
        );

        if let Some(cursor) = filter.cursor.as_deref() {
            query.push(
                r#"
                join event_outbox cursor_outbox
                  on cursor_outbox.public_id = case
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

        query.push(" where outbox.event_type = ");
        query.push_bind(PLUGIN_HOOK_DISPATCH_EVENT);

        if let Some(status) = filter.status.as_deref() {
            query.push(" and outbox.status = ");
            query.push_bind(status);
        }

        if filter.cursor.is_some() {
            query.push(
                " and (outbox.created_at, outbox.id) < (cursor_outbox.created_at, cursor_outbox.id)",
            );
        }

        query.push(" order by outbox.created_at desc, outbox.id desc limit ");
        query.push_bind(fetch_limit);

        let rows = query.build().fetch_all(&self.pool).await?;

        let has_more = rows.len() as i64 > filter.limit;
        let mut records = rows
            .into_iter()
            .map(PluginDispatchRecord::from_row)
            .collect::<Result<Vec<_>, _>>()?;
        if has_more {
            records.truncate(filter.limit as usize);
        }
        let next_cursor = has_more
            .then(|| records.last().map(|record| record.id.clone()))
            .flatten();

        Ok(PluginDispatchPage {
            records,
            next_cursor,
            has_more,
        })
    }

    pub async fn list_plugin_execution_runs(
        &self,
        dispatch_id: &str,
        limit: i64,
    ) -> Result<Option<Vec<PluginExecutionRunRecord>>, sqlx::Error> {
        self.list_plugin_execution_runs_page(
            dispatch_id,
            PluginExecutionRunFilter {
                status: None,
                cursor: None,
                limit,
            },
        )
        .await
        .map(|page| page.map(|page| page.records))
    }

    pub async fn list_plugin_execution_runs_page(
        &self,
        dispatch_id: &str,
        filter: PluginExecutionRunFilter,
    ) -> Result<Option<PluginExecutionRunPage>, sqlx::Error> {
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

        let fetch_limit = filter.limit.saturating_add(1);
        let mut query = QueryBuilder::<Postgres>::new(
            r#"
            select runs.public_id::text as id,
                   runs.outbox_event_public_id as dispatch_id,
                   runs.outbox_event_id,
                   runs.attempt,
                   runs.plugin_id,
                   runs.package_id,
                   runs.hook_id,
                   runs.handler,
                   runs.event_key,
                   runs.runtime,
                   runs.entrypoint,
                   runs.status,
                   runs.request_payload,
                   runs.response_status,
                   runs.response_body,
                   runs.error_message,
                   runs.started_at::text as started_at,
                   runs.finished_at::text as finished_at,
                   runs.duration_ms
            from plugin_execution_runs runs
            "#,
        );

        if let Some(cursor) = filter.cursor.as_deref() {
            query.push(
                r#"
                join plugin_execution_runs cursor_run
                  on cursor_run.public_id = case
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

        query.push(" where runs.outbox_event_public_id = ");
        query.push_bind(dispatch_id.trim());

        if let Some(status) = filter.status.as_deref() {
            query.push(" and runs.status = ");
            query.push_bind(status);
        }

        if filter.cursor.is_some() {
            query.push(" and (runs.started_at, runs.id) < (cursor_run.started_at, cursor_run.id)");
        }

        query.push(" order by runs.started_at desc, runs.id desc limit ");
        query.push_bind(fetch_limit);

        let rows = query.build().fetch_all(&self.pool).await?;

        let has_more = rows.len() as i64 > filter.limit;
        let mut records = rows
            .into_iter()
            .map(PluginExecutionRunRecord::from_row)
            .collect::<Result<Vec<_>, _>>()?;
        if has_more {
            records.truncate(filter.limit as usize);
        }
        let next_cursor = has_more
            .then(|| records.last().map(|record| record.id.clone()))
            .flatten();

        Ok(Some(PluginExecutionRunPage {
            records,
            next_cursor,
            has_more,
        }))
    }

    pub async fn list_plugin_host_api_calls(
        &self,
        filter: PluginHostApiCallFilter,
    ) -> Result<Vec<PluginHostApiCallRecord>, sqlx::Error> {
        self.list_plugin_host_api_calls_page(filter)
            .await
            .map(|page| page.records)
    }

    pub async fn list_plugin_host_api_calls_page(
        &self,
        filter: PluginHostApiCallFilter,
    ) -> Result<PluginHostApiCallPage, sqlx::Error> {
        let fetch_limit = filter.limit.saturating_add(1);
        let mut query = QueryBuilder::<Postgres>::new(
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
            "#,
        );

        if let Some(cursor) = filter.cursor.as_deref() {
            query.push(
                r#"
                join plugin_host_api_calls cursor_call
                  on cursor_call.public_id = case
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
            left join plugin_host_tokens token on token.id = calls.host_token_id
            left join plugin_execution_runs run on run.id = calls.execution_run_id
            where true
            "#,
        );

        if let Some(plugin_id) = filter.plugin_id.as_deref() {
            query.push(" and calls.plugin_id = ");
            query.push_bind(plugin_id);
        }

        if let Some(execution_run_id) = filter.execution_run_id.as_deref() {
            query.push(
                r#"
                and calls.execution_run_id = (
                    select id
                    from plugin_execution_runs
                    where public_id = case
                        when
                "#,
            );
            query.push_bind(execution_run_id);
            query.push(
                r#"::text ~* '^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$'
                        then
                "#,
            );
            query.push_bind(execution_run_id);
            query.push(
                r#"::uuid
                        else null::uuid
                    end
                )
                "#,
            );
        }

        if let Some(status_code) = filter.status_code {
            query.push(" and calls.status_code = ");
            query.push_bind(status_code);
        }

        if filter.cursor.is_some() {
            query.push(
                " and (calls.finished_at, calls.id) < (cursor_call.finished_at, cursor_call.id)",
            );
        }

        query.push(" order by calls.finished_at desc, calls.id desc limit ");
        query.push_bind(fetch_limit);

        let rows = query.build().fetch_all(&self.pool).await?;
        let has_more = rows.len() as i64 > filter.limit;
        let mut records = rows
            .into_iter()
            .map(PluginHostApiCallRecord::from_row)
            .collect::<Result<Vec<_>, _>>()?;
        if has_more {
            records.truncate(filter.limit as usize);
        }
        let next_cursor = has_more
            .then(|| records.last().map(|record| record.id.clone()))
            .flatten();

        Ok(PluginHostApiCallPage {
            records,
            next_cursor,
            has_more,
        })
    }

    pub async fn list_plugin_host_api_calls_for_run(
        &self,
        execution_run_id: &str,
        limit: i64,
    ) -> Result<Option<Vec<PluginHostApiCallRecord>>, sqlx::Error> {
        self.list_plugin_host_api_calls_for_run_page(execution_run_id, None, limit)
            .await
            .map(|page| page.map(|page| page.records))
    }

    pub async fn list_plugin_host_api_calls_for_run_page(
        &self,
        execution_run_id: &str,
        cursor: Option<String>,
        limit: i64,
    ) -> Result<Option<PluginHostApiCallPage>, sqlx::Error> {
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

        self.list_plugin_host_api_calls_page(PluginHostApiCallFilter {
            plugin_id: None,
            execution_run_id: Some(execution_run_id.trim().to_owned()),
            status_code: None,
            cursor,
            limit,
        })
        .await
        .map(Some)
    }

    pub async fn event_stream_mirror_status(
        &self,
    ) -> Result<EventStreamMirrorStatusRecord, sqlx::Error> {
        let row = sqlx::query(ADMIN_EVENT_STREAM_MIRROR_STATUS_SQL)
            .fetch_one(&self.pool)
            .await?;

        EventStreamMirrorStatusRecord::from_row(row)
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
        self.list_scheduled_tasks_page(ScheduledTaskFilter {
            task_type: None,
            owner_type: None,
            enabled: None,
            cursor: None,
            limit,
        })
        .await
        .map(|page| page.records)
    }

    pub async fn list_scheduled_tasks_page(
        &self,
        filter: ScheduledTaskFilter,
    ) -> Result<ScheduledTaskPage, sqlx::Error> {
        let page_limit = filter.limit.max(1);
        let fetch_limit = page_limit.saturating_add(1);
        let mut query = QueryBuilder::<Postgres>::new(
            r#"
            select tasks.public_id::text as id,
                   tasks.task_key,
                   tasks.task_type,
                   tasks.owner_type,
                   tasks.owner_id,
                   tasks.enabled,
                   tasks.schedule_kind,
                   tasks.schedule_value,
                   tasks.next_run_at::text as next_run_at,
                   tasks.last_run_at::text as last_run_at,
                   tasks.timeout_seconds,
                   tasks.max_concurrency,
                   (
                       select count(*)::bigint
                       from scheduled_task_runs runs
                       where runs.task_id = tasks.id
                         and runs.status = 'running'
                         and runs.lease_expires_at > now()
                   ) as active_run_count,
                   (
                       select runs.public_id::text
                       from scheduled_task_runs runs
                       where runs.task_id = tasks.id
                       order by runs.started_at desc, runs.id desc
                       limit 1
                   ) as last_run_id,
                   tasks.failure_count,
                   tasks.last_error,
                   tasks.created_at::text as created_at,
                   tasks.updated_at::text as updated_at
            from scheduled_tasks tasks
            "#,
        );

        if let Some(cursor) = filter.cursor.as_deref() {
            query.push(
                r#"
                join scheduled_tasks cursor_task
                  on cursor_task.public_id = case
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

        query.push(" where true");

        if let Some(task_type) = filter.task_type.as_deref() {
            query.push(" and tasks.task_type = ");
            query.push_bind(task_type);
        }

        if let Some(owner_type) = filter.owner_type.as_deref() {
            query.push(" and tasks.owner_type = ");
            query.push_bind(owner_type);
        }

        if let Some(enabled) = filter.enabled {
            query.push(" and tasks.enabled = ");
            query.push_bind(enabled);
        }

        if filter.cursor.is_some() {
            query.push(
                r#"
                and (
                    tasks.enabled < cursor_task.enabled
                    or (
                        tasks.enabled = cursor_task.enabled
                        and (
                            (
                                cursor_task.next_run_at is null
                                and tasks.next_run_at is null
                                and (tasks.updated_at, tasks.id) < (cursor_task.updated_at, cursor_task.id)
                            )
                            or (
                                cursor_task.next_run_at is not null
                                and (
                                    tasks.next_run_at > cursor_task.next_run_at
                                    or tasks.next_run_at is null
                                    or (
                                        tasks.next_run_at = cursor_task.next_run_at
                                        and (tasks.updated_at, tasks.id) < (cursor_task.updated_at, cursor_task.id)
                                    )
                                )
                            )
                        )
                    )
                )
                "#,
            );
        }

        query.push(
            r#"
            order by tasks.enabled desc,
                     tasks.next_run_at asc nulls last,
                     tasks.updated_at desc,
                     tasks.id desc
            limit
            "#,
        );
        query.push_bind(fetch_limit);

        let rows = query.build().fetch_all(&self.pool).await?;
        let has_more = rows.len() as i64 > page_limit;
        let mut records = rows
            .into_iter()
            .map(ScheduledTaskAdminRecord::from_row)
            .collect::<Result<Vec<_>, _>>()?;
        if has_more {
            records.truncate(page_limit as usize);
        }
        let next_cursor = has_more
            .then(|| records.last().map(|record| record.id.clone()))
            .flatten();

        Ok(ScheduledTaskPage {
            records,
            next_cursor,
            has_more,
        })
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
        self.list_scheduled_task_runs_page(
            task_key,
            ScheduledTaskRunFilter {
                status: None,
                cursor: None,
                limit,
            },
        )
        .await
        .map(|page| page.map(|page| page.records))
    }

    pub async fn list_scheduled_task_runs_page(
        &self,
        task_key: &str,
        filter: ScheduledTaskRunFilter,
    ) -> Result<Option<ScheduledTaskRunPage>, sqlx::Error> {
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

        let fetch_limit = filter.limit.saturating_add(1);
        let mut query = QueryBuilder::<Postgres>::new(
            r#"
            select runs.public_id::text as id,
                   runs.task_key,
                   runs.trigger_type,
                   runs.worker_id,
                   runs.status,
                   runs.lease_expires_at::text as lease_expires_at,
                   (runs.status = 'running' and runs.lease_expires_at > now()) as lease_active,
                   runs.queued_jobs,
                   runs.error_message,
                   runs.started_at::text as started_at,
                   runs.finished_at::text as finished_at,
                   floor(extract(epoch from (coalesce(runs.finished_at, now()) - runs.started_at)) * 1000)::bigint as duration_ms,
                   runs.created_at::text as created_at,
                   runs.updated_at::text as updated_at
            from scheduled_task_runs runs
            "#,
        );

        if let Some(cursor) = filter.cursor.as_deref() {
            query.push(
                r#"
                join scheduled_task_runs cursor_run
                  on cursor_run.public_id = case
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

        query.push(" where runs.task_id = ");
        query.push_bind(task_id);

        if let Some(status) = filter.status.as_deref() {
            query.push(" and runs.status = ");
            query.push_bind(status);
        }

        if filter.cursor.is_some() {
            query.push(" and (runs.started_at, runs.id) < (cursor_run.started_at, cursor_run.id)");
        }

        query.push(" order by runs.started_at desc, runs.id desc limit ");
        query.push_bind(fetch_limit);

        let rows = query.build().fetch_all(&self.pool).await?;

        let has_more = rows.len() as i64 > filter.limit;
        let mut records = rows
            .into_iter()
            .map(ScheduledTaskRunRecord::from_row)
            .collect::<Result<Vec<_>, _>>()?;
        if has_more {
            records.truncate(filter.limit as usize);
        }
        let next_cursor = has_more
            .then(|| records.last().map(|record| record.id.clone()))
            .flatten();

        Ok(Some(ScheduledTaskRunPage {
            records,
            next_cursor,
            has_more,
        }))
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

impl EventStreamMirrorStatusRecord {
    fn from_row(row: PgRow) -> Result<Self, sqlx::Error> {
        Ok(Self {
            unmirrored_count: row.try_get("unmirrored_count")?,
            claimable_count: row.try_get("claimable_count")?,
            locked_count: row.try_get("locked_count")?,
            backoff_count: row.try_get("backoff_count")?,
            failed_count: row.try_get("failed_count")?,
            max_attempts: row.try_get("max_attempts")?,
            oldest_unmirrored_created_at: row.try_get("oldest_unmirrored_created_at")?,
            next_retry_at: row.try_get("next_retry_at")?,
            last_error: row.try_get("last_error")?,
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
        let status_migration =
            include_str!("../../migrations/0049_scheduled_task_run_admin_keyset_indexes.sql");
        let repository = include_str!("repository.rs");

        assert!(migration.contains("idx_scheduled_task_runs_task_recent"));
        assert!(migration.contains("task_id, started_at desc, id desc"));
        assert!(migration.contains("include (public_id)"));
        assert!(status_migration.contains("idx_scheduled_task_runs_task_status_recent_keyset"));
        assert!(status_migration.contains("task_id, status, started_at desc, id desc"));
        assert!(repository.contains("from scheduled_task_runs runs"));
        assert!(repository.contains("order by runs.started_at desc, runs.id desc"));

        let query_start = repository
            .find("pub async fn list_scheduled_task_runs_page")
            .expect("scheduled task run page query should exist");
        let query_end = repository[query_start..]
            .find("}\n}\n\n#[derive(Clone, Debug, PartialEq, Eq)]\nstruct StoredLibraryPath")
            .map(|offset| query_start + offset)
            .expect("scheduled task run page query should be near repository impl end");
        let run_query = &repository[query_start..query_end];

        assert!(run_query.contains("QueryBuilder::<Postgres>"));
        assert!(run_query.contains("cursor_run.public_id = case"));
        assert!(run_query.contains("(runs.started_at, runs.id) <"));
        assert!(run_query.contains("runs.task_id ="));
        assert!(run_query.contains("runs.status ="));
        assert!(run_query.contains("order by runs.started_at desc, runs.id desc"));
        assert!(!run_query.contains("offset "));
    }

    #[test]
    fn admin_job_run_event_keyset_indexes_match_query_shape() {
        let migration =
            include_str!("../../migrations/0050_admin_job_run_event_keyset_indexes.sql");
        let repository = include_str!("repository.rs");

        assert!(migration.contains("idx_job_runs_job_started_keyset"));
        assert!(migration.contains("job_id, started_at desc, id desc"));
        assert!(migration.contains("idx_job_runs_job_status_started_keyset"));
        assert!(migration.contains("job_id, status, started_at desc, id desc"));
        assert!(migration.contains("idx_job_events_job_created_keyset"));
        assert!(migration.contains("job_id, created_at desc, id desc"));
        assert!(migration.contains("idx_job_events_job_level_created_keyset"));
        assert!(migration.contains("job_id, event_level, created_at desc, id desc"));

        let run_query_start = repository
            .find("pub async fn list_admin_job_runs_page")
            .expect("admin job run page query should exist");
        let run_query_end = repository[run_query_start..]
            .find("pub async fn list_admin_job_events_page")
            .map(|offset| run_query_start + offset)
            .expect("job event page query should follow job run page query");
        let run_query = &repository[run_query_start..run_query_end];

        assert!(run_query.contains("QueryBuilder::<Postgres>"));
        assert!(run_query.contains("join job_runs cursor_run on cursor_run.id ="));
        assert!(run_query.contains("cursor_run.job_id ="));
        assert!(run_query.contains("(runs.started_at, runs.id) <"));
        assert!(run_query.contains("runs.job_id ="));
        assert!(run_query.contains("runs.status ="));
        assert!(run_query.contains("order by runs.started_at desc, runs.id desc"));
        assert!(!run_query.contains("offset "));

        let event_query_start = repository
            .find("pub async fn list_admin_job_events_page")
            .expect("admin job event page query should exist");
        let event_query_end = repository[event_query_start..]
            .find("pub async fn list_notification_targets")
            .map(|offset| event_query_start + offset)
            .expect("notification target query should follow job event page query");
        let event_query = &repository[event_query_start..event_query_end];

        assert!(event_query.contains("QueryBuilder::<Postgres>"));
        assert!(event_query.contains("join job_events cursor_event on cursor_event.id ="));
        assert!(event_query.contains("cursor_event.job_id ="));
        assert!(event_query.contains("(events.created_at, events.id) <"));
        assert!(event_query.contains("events.job_id ="));
        assert!(event_query.contains("events.event_level ="));
        assert!(event_query.contains("order by events.created_at desc, events.id desc"));
        assert!(!event_query.contains("offset "));
    }

    #[test]
    fn admin_job_list_keyset_indexes_match_query_shape() {
        let migration = include_str!("../../migrations/0054_admin_job_list_keyset_indexes.sql");
        let repository = include_str!("repository.rs");

        assert!(migration.contains("idx_jobs_admin_recent_keyset"));
        assert!(migration.contains("created_at desc, id desc"));
        assert!(migration.contains("idx_jobs_admin_status_recent_keyset"));
        assert!(migration.contains("status, created_at desc, id desc"));
        assert!(migration.contains("idx_jobs_admin_type_recent_keyset"));
        assert!(migration.contains("job_type, created_at desc, id desc"));
        assert!(migration.contains("idx_jobs_admin_queue_recent_keyset"));
        assert!(migration.contains("queue_name, created_at desc, id desc"));

        let query_start = repository
            .find("pub async fn list_admin_jobs_page")
            .expect("admin job page query should exist");
        let query_end = repository[query_start..]
            .find("pub async fn get_admin_job_detail")
            .map(|offset| query_start + offset)
            .expect("admin job detail query should follow job page query");
        let job_query = &repository[query_start..query_end];

        assert!(job_query.contains("QueryBuilder::<Postgres>"));
        assert!(job_query.contains("join jobs cursor_job"));
        assert!(job_query.contains("cursor_job.public_id = case"));
        assert!(job_query.contains("(jobs.created_at, jobs.id) <"));
        assert!(job_query.contains("jobs.status ="));
        assert!(job_query.contains("jobs.job_type ="));
        assert!(job_query.contains("jobs.queue_name ="));
        assert!(job_query.contains("order by jobs.created_at desc, jobs.id desc"));
        assert!(!job_query.contains("offset "));
    }

    #[test]
    fn admin_user_list_keyset_indexes_match_query_shape() {
        let migration = include_str!("../../migrations/0055_admin_user_list_keyset_indexes.sql");
        let repository = include_str!("repository.rs");

        assert!(migration.contains("idx_users_admin_username_keyset"));
        assert!(migration.contains("username_normalized asc, id asc"));
        assert!(migration.contains("idx_users_admin_role_username_keyset"));
        assert!(migration.contains("role_id, username_normalized asc, id asc"));
        assert!(migration.contains("idx_users_admin_disabled_username_keyset"));
        assert!(migration.contains("is_disabled, username_normalized asc, id asc"));
        assert!(migration.contains("idx_users_admin_role_disabled_username_keyset"));
        assert!(migration.contains("role_id, is_disabled, username_normalized asc, id asc"));

        let query_start = repository
            .find("pub async fn list_admin_users_page")
            .expect("admin user page query should exist");
        let query_end = repository[query_start..]
            .find("pub async fn update_user_policy")
            .map(|offset| query_start + offset)
            .expect("update user policy should follow admin user page query");
        let user_query = &repository[query_start..query_end];

        assert!(user_query.contains("QueryBuilder::<Postgres>"));
        assert!(user_query.contains("join users cursor_user"));
        assert!(user_query.contains("cursor_user.public_id = case"));
        assert!(user_query.contains("(u.username_normalized, u.id) >"));
        assert!(user_query.contains("r.name_normalized ="));
        assert!(user_query.contains("u.is_disabled ="));
        assert!(user_query.contains("order by u.username_normalized asc, u.id asc"));
        assert!(!user_query.contains("offset "));
    }

    #[test]
    fn admin_user_library_permission_keyset_indexes_match_query_shape() {
        let migration =
            include_str!("../../migrations/0056_admin_user_library_permission_keyset_indexes.sql");
        let repository = include_str!("repository.rs");

        assert!(migration.contains("idx_libraries_admin_name_keyset"));
        assert!(migration.contains("name asc, id asc"));
        assert!(migration.contains("idx_libraries_admin_type_name_keyset"));
        assert!(migration.contains("library_type, name asc, id asc"));

        let query_start = repository
            .find("pub async fn list_user_library_permissions_page")
            .expect("admin user library permission page query should exist");
        let query_end = repository[query_start..]
            .find("pub async fn update_user_library_permission")
            .map(|offset| query_start + offset)
            .expect("update user library permission should follow page query");
        let permission_query = &repository[query_start..query_end];

        assert!(permission_query.contains("QueryBuilder::<Postgres>"));
        assert!(permission_query.contains("join libraries cursor_library"));
        assert!(permission_query.contains("cursor_library.public_id = case"));
        assert!(permission_query.contains("(l.name, l.id) >"));
        assert!(permission_query.contains("l.library_type ="));
        assert!(permission_query.contains("lp.id is not null"));
        assert!(permission_query.contains("lp.id is null"));
        assert!(permission_query.contains("order by l.name asc, l.id asc"));
        assert!(!permission_query.contains("offset "));
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
    fn notification_target_admin_keyset_indexes_match_query_shape() {
        let migration =
            include_str!("../../migrations/0057_notification_target_admin_keyset_indexes.sql");
        let repository = include_str!("repository.rs");

        assert!(migration.contains("idx_notification_targets_admin_recent_keyset"));
        assert!(migration.contains("target_type, name asc, id asc"));
        assert!(migration.contains("idx_notification_targets_admin_enabled_recent_keyset"));
        assert!(migration.contains("is_enabled, target_type, name asc, id asc"));
        assert!(migration.contains("idx_notification_targets_admin_channel_recent_keyset"));
        assert!(migration.contains("channel, target_type, name asc, id asc"));
        assert!(migration.contains("idx_notification_targets_admin_channel_enabled_recent_keyset"));
        assert!(migration.contains("channel, is_enabled, target_type, name asc, id asc"));

        let query_start = repository
            .find("pub async fn list_notification_targets_page")
            .expect("notification target page query should exist");
        let query_end = repository[query_start..]
            .find("pub async fn create_notification_target")
            .map(|offset| query_start + offset)
            .expect("create notification target should follow list query");
        let target_query = &repository[query_start..query_end];

        assert!(target_query.contains("QueryBuilder::<Postgres>"));
        assert!(target_query.contains("join notification_targets cursor_target"));
        assert!(target_query.contains("cursor_target.public_id = case"));
        assert!(target_query.contains("(targets.target_type, targets.name, targets.id) >"));
        assert!(target_query.contains("targets.target_type ="));
        assert!(target_query.contains("targets.channel ="));
        assert!(target_query.contains("targets.is_enabled ="));
        assert!(
            target_query
                .contains("order by targets.target_type asc, targets.name asc, targets.id asc")
        );
        assert!(!target_query.contains("offset "));
    }

    #[test]
    fn notification_admin_keyset_filter_indexes_match_query_shape() {
        let migration =
            include_str!("../../migrations/0047_notification_admin_keyset_filter_indexes.sql");
        let repository = include_str!("repository.rs");

        assert!(migration.contains("idx_plugin_notification_requests_status_recent_keyset"));
        assert!(migration.contains("status, created_at desc, id desc"));
        assert!(migration.contains("idx_plugin_notification_requests_channel_recent_keyset"));
        assert!(migration.contains("channel, created_at desc, id desc"));
        assert!(
            migration.contains("idx_plugin_notification_requests_channel_status_recent_keyset")
        );
        assert!(migration.contains("channel, status, created_at desc, id desc"));
        assert!(
            migration.contains("idx_notification_delivery_attempts_request_status_recent_keyset")
        );
        assert!(migration.contains("notification_request_id, status, created_at desc, id desc"));

        let request_query_start = repository
            .find("pub async fn list_notification_requests_page")
            .expect("notification request page query should exist");
        let request_query_end = repository[request_query_start..]
            .find("pub async fn list_notification_delivery_attempts")
            .map(|offset| request_query_start + offset)
            .expect("delivery attempt query should follow notification request page query");
        let request_query = &repository[request_query_start..request_query_end];

        assert!(request_query.contains("QueryBuilder::<Postgres>"));
        assert!(request_query.contains("cursor_request.public_id = case"));
        assert!(request_query.contains("(requests.created_at, requests.id) <"));
        assert!(request_query.contains("requests.status ="));
        assert!(request_query.contains("requests.channel ="));
        assert!(request_query.contains("order by requests.created_at desc, requests.id desc"));
        assert!(!request_query.contains("offset "));

        let attempt_query_start = repository
            .find("pub async fn list_notification_delivery_attempts_page")
            .expect("notification delivery attempt page query should exist");
        let attempt_query_end = repository[attempt_query_start..]
            .find("pub async fn retry_notification_request")
            .map(|offset| attempt_query_start + offset)
            .expect("retry query should follow delivery attempt page query");
        let attempt_query = &repository[attempt_query_start..attempt_query_end];

        assert!(attempt_query.contains("QueryBuilder::<Postgres>"));
        assert!(attempt_query.contains("cursor_attempt.public_id = case"));
        assert!(attempt_query.contains("(attempts.created_at, attempts.id) <"));
        assert!(attempt_query.contains("attempts.notification_request_id ="));
        assert!(attempt_query.contains("attempts.status ="));
        assert!(attempt_query.contains("order by attempts.created_at desc, attempts.id desc"));
        assert!(!attempt_query.contains("offset "));
    }

    #[test]
    fn scheduled_task_admin_keyset_indexes_match_query_shape() {
        let migration =
            include_str!("../../migrations/0058_scheduled_task_admin_keyset_indexes.sql");
        let repository = include_str!("repository.rs");

        assert!(migration.contains("idx_scheduled_tasks_admin_keyset"));
        assert!(
            migration
                .contains("enabled desc, next_run_at asc nulls last, updated_at desc, id desc")
        );
        assert!(migration.contains("idx_scheduled_tasks_task_type_admin_keyset"));
        assert!(migration.contains(
            "task_type, enabled desc, next_run_at asc nulls last, updated_at desc, id desc"
        ));
        assert!(migration.contains("idx_scheduled_tasks_owner_type_admin_keyset"));
        assert!(migration.contains(
            "owner_type, enabled desc, next_run_at asc nulls last, updated_at desc, id desc"
        ));

        let query_start = repository
            .find("pub async fn list_scheduled_tasks_page")
            .expect("scheduled task page query should exist");
        let query_end = repository[query_start..]
            .find("pub async fn find_scheduled_task")
            .map(|offset| query_start + offset)
            .expect("find scheduled task should follow list query");
        let task_query = &repository[query_start..query_end];

        assert!(task_query.contains("QueryBuilder::<Postgres>"));
        assert!(task_query.contains("join scheduled_tasks cursor_task"));
        assert!(task_query.contains("cursor_task.public_id = case"));
        assert!(task_query.contains("tasks.task_type ="));
        assert!(task_query.contains("tasks.owner_type ="));
        assert!(task_query.contains("tasks.enabled ="));
        assert!(task_query.contains("tasks.enabled < cursor_task.enabled"));
        assert!(task_query.contains("tasks.next_run_at > cursor_task.next_run_at"));
        assert!(task_query.contains("(tasks.updated_at, tasks.id) <"));
        assert!(task_query.contains("order by tasks.enabled desc"));
        assert!(task_query.contains("tasks.next_run_at asc nulls last"));
        assert!(!task_query.contains("offset "));
    }

    #[test]
    fn plugin_host_api_call_keyset_indexes_match_admin_query_shape() {
        let migration =
            include_str!("../../migrations/0045_plugin_host_api_call_keyset_indexes.sql");
        let repository = include_str!("repository.rs");

        assert!(migration.contains("idx_plugin_host_api_calls_recent_keyset"));
        assert!(migration.contains("finished_at desc, id desc"));
        assert!(migration.contains("idx_plugin_host_api_calls_plugin_finished_keyset"));
        assert!(migration.contains("plugin_id, finished_at desc, id desc"));
        assert!(migration.contains("idx_plugin_host_api_calls_execution_finished_keyset"));
        assert!(migration.contains("execution_run_id, finished_at desc, id desc"));
        assert!(migration.contains("idx_plugin_host_api_calls_status_finished_keyset"));
        assert!(migration.contains("status_code, finished_at desc, id desc"));

        let query_start = repository
            .find("pub async fn list_plugin_host_api_calls_page")
            .expect("host api call page query should exist");
        let query_end = repository[query_start..]
            .find("pub async fn list_plugin_host_api_calls_for_run")
            .map(|offset| query_start + offset)
            .expect("host api call run query should follow page query");
        let host_api_call_query = &repository[query_start..query_end];

        assert!(repository.contains("QueryBuilder::<Postgres>"));
        assert!(host_api_call_query.contains("cursor_call"));
        assert!(host_api_call_query.contains("(calls.finished_at, calls.id) <"));
        assert!(host_api_call_query.contains("order by calls.finished_at desc, calls.id desc"));
        assert!(!host_api_call_query.contains("offset "));
        assert!(repository.contains("pub async fn list_plugin_host_api_calls_for_run_page"));
        assert!(repository.contains("list_plugin_host_api_calls(PluginHostApiCallFilter"));
    }

    #[test]
    fn plugin_dispatch_keyset_indexes_match_admin_query_shape() {
        let migration =
            include_str!("../../migrations/0046_plugin_dispatch_admin_keyset_indexes.sql");
        let repository = include_str!("repository.rs");

        assert!(migration.contains("idx_event_outbox_plugin_dispatch_recent_keyset"));
        assert!(migration.contains("created_at desc, id desc"));
        assert!(migration.contains("idx_event_outbox_plugin_dispatch_status_recent_keyset"));
        assert!(migration.contains("status, created_at desc, id desc"));
        assert!(migration.contains("where event_type = 'plugin.hook.dispatch'"));

        let query_start = repository
            .find("pub async fn list_plugin_dispatches_page")
            .expect("plugin dispatch page query should exist");
        let query_end = repository[query_start..]
            .find("pub async fn list_plugin_execution_runs")
            .map(|offset| query_start + offset)
            .expect("plugin execution run query should follow dispatch page query");
        let dispatch_query = &repository[query_start..query_end];

        assert!(dispatch_query.contains("QueryBuilder::<Postgres>"));
        assert!(dispatch_query.contains("cursor_outbox.public_id = case"));
        assert!(dispatch_query.contains("(outbox.created_at, outbox.id) <"));
        assert!(dispatch_query.contains("outbox.status ="));
        assert!(dispatch_query.contains("order by outbox.created_at desc, outbox.id desc"));
        assert!(!dispatch_query.contains("offset "));
    }

    #[test]
    fn plugin_execution_run_keyset_indexes_match_admin_query_shape() {
        let migration =
            include_str!("../../migrations/0048_plugin_execution_run_admin_keyset_indexes.sql");
        let repository = include_str!("repository.rs");

        assert!(migration.contains("idx_plugin_execution_runs_dispatch_started_keyset"));
        assert!(migration.contains("outbox_event_public_id, started_at desc, id desc"));
        assert!(migration.contains("idx_plugin_execution_runs_dispatch_status_started_keyset"));
        assert!(migration.contains("outbox_event_public_id, status, started_at desc, id desc"));

        let query_start = repository
            .find("pub async fn list_plugin_execution_runs_page")
            .expect("plugin execution run page query should exist");
        let query_end = repository[query_start..]
            .find("pub async fn list_plugin_host_api_calls")
            .map(|offset| query_start + offset)
            .expect("plugin host api call query should follow execution run page query");
        let run_query = &repository[query_start..query_end];

        assert!(run_query.contains("QueryBuilder::<Postgres>"));
        assert!(run_query.contains("cursor_run.public_id = case"));
        assert!(run_query.contains("(runs.started_at, runs.id) <"));
        assert!(run_query.contains("runs.outbox_event_public_id ="));
        assert!(run_query.contains("runs.status ="));
        assert!(run_query.contains("order by runs.started_at desc, runs.id desc"));
        assert!(!run_query.contains("offset "));
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
        assert!(
            repository
                .contains("from plugin_execution_runs\n                    where public_id = case")
        );
        assert!(repository.contains("cursor_call.public_id = case"));
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

    #[test]
    fn event_stream_mirror_status_stays_on_unmirrored_backlog() {
        assert!(
            ADMIN_EVENT_STREAM_MIRROR_STATUS_SQL
                .contains("from event_outbox\n                where stream_mirrored_at is null")
        );
        assert!(!ADMIN_EVENT_STREAM_MIRROR_STATUS_SQL.contains("stream_mirrored_at is not null"));
        assert!(!ADMIN_EVENT_STREAM_MIRROR_STATUS_SQL.contains("count(stream_mirrored_at"));
        assert!(ADMIN_EVENT_STREAM_MIRROR_STATUS_SQL.contains("from unmirrored"));
    }
}
