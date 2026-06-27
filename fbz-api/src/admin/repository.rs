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

const SCHEDULED_TASK_ACTIVE_RUN_COUNT_SQL: &str = r#"
                   (
                       select count(*)::bigint
                       from (
                           select 1
                           from scheduled_task_runs runs
                           where runs.task_id = tasks.id
                             and runs.status = 'running'
                             and runs.lease_expires_at > now()
                           order by runs.lease_expires_at asc, runs.id asc
                           limit tasks.max_concurrency
                       ) active_run_capacity_probe
                   ) as active_run_count,
"#;

const ADMIN_USER_SUMMARY_SAMPLE_LIMIT: i64 = 10_000;
const ADMIN_USER_SUMMARY_FETCH_LIMIT: i64 = ADMIN_USER_SUMMARY_SAMPLE_LIMIT + 1;

fn push_admin_user_counts_sql(query: &mut QueryBuilder<'_, Postgres>) {
    query.push(
        r#"
                   (
                       select least(count(*), "#,
    );
    query.push_bind(ADMIN_USER_SUMMARY_SAMPLE_LIMIT);
    query.push(
        r#")::bigint
                       from (
                           select 1
                           from devices d
                           where d.user_id = u.id
                           order by d.last_seen_at desc, d.id desc
                           limit "#,
    );
    query.push_bind(ADMIN_USER_SUMMARY_FETCH_LIMIT);
    query.push(
        r#"
                       ) device_count_probe
                   ) as device_count,
                   (
                       select least(count(*), "#,
    );
    query.push_bind(ADMIN_USER_SUMMARY_SAMPLE_LIMIT);
    query.push(
        r#")::bigint
                       from (
                           select 1
                           from sessions s
                           where s.user_id = u.id
                             and s.revoked_at is null
                             and s.expires_at > now()
                           order by s.expires_at asc, s.id asc
                           limit "#,
    );
    query.push_bind(ADMIN_USER_SUMMARY_FETCH_LIMIT);
    query.push(
        r#"
                       ) active_session_count_probe
                   ) as active_session_count,
"#,
    );
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

const ADMIN_QUEUE_LIBRARY_SCAN_SQL: &str = r#"
            with target_library as (
                select public_id::text as public_id
                from libraries
                where public_id = case
                    when $1::text ~* '^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$'
                    then $1::uuid
                    else null::uuid
                end
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
                    'library.scan',
                    'queued',
                    'scan',
                    0,
                    jsonb_build_object(
                        'libraryId', target_library.public_id,
                        'requestedByUserId', $2::bigint,
                        'reason', $3::jsonb
                    )
                from target_library
                where not exists (
                    select 1
                    from jobs j
                    where j.job_type = 'library.scan'
                      and j.status in ('queued', 'running', 'failed')
                      and (j.status <> 'failed' or j.attempts < j.max_attempts)
                      and j.payload->>'libraryId' = target_library.public_id
                )
                returning public_id::text as id,
                          status,
                          queue_name,
                          job_type
            ),
            existing as (
                select j.public_id::text as id,
                       j.status,
                       j.queue_name,
                       j.job_type
                from jobs j
                join target_library
                  on j.payload->>'libraryId' = target_library.public_id
                where j.job_type = 'library.scan'
                  and j.status in ('queued', 'running', 'failed')
                  and (j.status <> 'failed' or j.attempts < j.max_attempts)
                order by j.created_at desc, j.id desc
                limit 1
            )
            select id,
                   status,
                   queue_name,
                   job_type
            from inserted
            union all
            select id,
                   status,
                   queue_name,
                   job_type
            from existing
            limit 1
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
    pub counts_are_exact: bool,
    pub sample_limit: i64,
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
            with admin_event_stream_mirror_sample_limit as (
                select 10000::bigint as lower_bound_count
            ),
            event_stream_mirror_sample as (
                select 'unmirrored'::text as status,
                       stream_mirror_attempts
                from (
                    select stream_mirror_attempts
                    from event_outbox
                    where stream_mirrored_at is null
                    limit 10001
                ) unmirrored_events
                union all
                select 'claimable'::text as status,
                       stream_mirror_attempts
                from (
                    select stream_mirror_attempts
                    from event_outbox
                    where stream_mirrored_at is null
                      and (
                          stream_mirror_locked_until is null
                          or stream_mirror_locked_until <= now()
                      )
                    limit 10001
                ) claimable_events
                union all
                select 'locked'::text as status,
                       stream_mirror_attempts
                from (
                    select stream_mirror_attempts
                    from event_outbox
                    where stream_mirrored_at is null
                      and stream_mirror_locked_by is not null
                      and stream_mirror_locked_until > now()
                    limit 10001
                ) locked_events
                union all
                select 'backoff'::text as status,
                       stream_mirror_attempts
                from (
                    select stream_mirror_attempts
                    from event_outbox
                    where stream_mirrored_at is null
                      and stream_mirror_locked_by is null
                      and stream_mirror_locked_until > now()
                    limit 10001
                ) backoff_events
                union all
                select 'failed'::text as status,
                       stream_mirror_attempts
                from (
                    select stream_mirror_attempts
                    from event_outbox
                    where stream_mirrored_at is null
                      and stream_mirror_last_error is not null
                    limit 10001
                ) failed_mirror_events
            ),
            sampled_counts as (
                select count(*) filter (where status = 'unmirrored')::bigint as unmirrored_sample,
                       count(*) filter (where status = 'claimable')::bigint as claimable_sample,
                       count(*) filter (where status = 'locked')::bigint as locked_sample,
                       count(*) filter (where status = 'backoff')::bigint as backoff_sample,
                       count(*) filter (where status = 'failed')::bigint as failed_sample,
                       coalesce(
                           max(stream_mirror_attempts) filter (where status = 'unmirrored'),
                           0
                       )::integer as max_attempts
                from event_stream_mirror_sample
            ),
            oldest_unmirrored as (
                select created_at::text as oldest_unmirrored_created_at
                from event_outbox
                where stream_mirrored_at is null
                order by created_at asc, id asc
                limit 1
            ),
            next_retry as (
                select stream_mirror_locked_until::text as next_retry_at
                from event_outbox
                where stream_mirrored_at is null
                  and stream_mirror_locked_until > now()
                order by stream_mirror_locked_until asc, id asc
                limit 1
            ),
            last_error as (
                select stream_mirror_last_error as last_error
                from event_outbox
                where stream_mirrored_at is null
                  and stream_mirror_last_error is not null
                order by id desc
                limit 1
            )
            select least(
                       sampled_counts.unmirrored_sample,
                       (select lower_bound_count from admin_event_stream_mirror_sample_limit)
                   )::bigint as unmirrored_count,
                   least(
                       sampled_counts.claimable_sample,
                       (select lower_bound_count from admin_event_stream_mirror_sample_limit)
                   )::bigint as claimable_count,
                   least(
                       sampled_counts.locked_sample,
                       (select lower_bound_count from admin_event_stream_mirror_sample_limit)
                   )::bigint as locked_count,
                   least(
                       sampled_counts.backoff_sample,
                       (select lower_bound_count from admin_event_stream_mirror_sample_limit)
                   )::bigint as backoff_count,
                   least(
                       sampled_counts.failed_sample,
                       (select lower_bound_count from admin_event_stream_mirror_sample_limit)
                   )::bigint as failed_count,
                   sampled_counts.max_attempts,
                   (select oldest_unmirrored_created_at from oldest_unmirrored)
                       as oldest_unmirrored_created_at,
                   (select next_retry_at from next_retry) as next_retry_at,
                   (select last_error from last_error) as last_error,
                   (
                       sampled_counts.unmirrored_sample
                           <= (select lower_bound_count from admin_event_stream_mirror_sample_limit)
                       and sampled_counts.claimable_sample
                           <= (select lower_bound_count from admin_event_stream_mirror_sample_limit)
                       and sampled_counts.locked_sample
                           <= (select lower_bound_count from admin_event_stream_mirror_sample_limit)
                       and sampled_counts.backoff_sample
                           <= (select lower_bound_count from admin_event_stream_mirror_sample_limit)
                       and sampled_counts.failed_sample
                           <= (select lower_bound_count from admin_event_stream_mirror_sample_limit)
                   ) as counts_are_exact,
                   (select lower_bound_count from admin_event_stream_mirror_sample_limit)
                       as sample_limit
            from sampled_counts
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
        let payload_reason = json!(input.reason);
        let row = sqlx::query(ADMIN_QUEUE_LIBRARY_SCAN_SQL)
            .bind(input.library_id.trim())
            .bind(input.requested_by_user_id)
            .bind(payload_reason)
            .fetch_optional(&self.pool)
            .await?;

        row.map(ScanJobRecord::from_row).transpose()
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
"#,
        );
        push_admin_user_counts_sql(&mut query);
        query.push(
            r#"
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
        let mut query = QueryBuilder::<Postgres>::new(
            r#"
            with updated as (
                update users
                set display_name = "#,
        );
        query.push_bind(input.display_name.as_deref());
        query.push(
            r#",
                    is_disabled = "#,
        );
        query.push_bind(input.is_disabled);
        query.push(
            r#",
                    allow_download = "#,
        );
        query.push_bind(input.allow_download);
        query.push(
            r#",
                    allow_transcode = "#,
        );
        query.push_bind(input.allow_transcode);
        query.push(
            r#",
                    allow_new_device_login = "#,
        );
        query.push_bind(input.allow_new_device_login);
        query.push(
            r#",
                    updated_at = now()
                where public_id = "#,
        );
        query.push_bind(user_id.trim());
        query.push(
            r#"::uuid
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
"#,
        );
        push_admin_user_counts_sql(&mut query);
        query.push(
            r#"
                   u.last_login_at::text as last_login_at,
                   u.created_at::text as created_at,
                   u.updated_at::text as updated_at
            from updated u
            join roles r on r.id = u.role_id
            "#,
        );

        let row = query.build().fetch_optional(&self.pool).await?;

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
                    when $1::text ~* '^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$'
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
"#,
        );
        query.push(SCHEDULED_TASK_ACTIVE_RUN_COUNT_SQL);
        query.push(
            r#"
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
"#,
        );
        query.push(SCHEDULED_TASK_ACTIVE_RUN_COUNT_SQL);
        query.push(
            r#"
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
            where tasks.public_id = case
                    when
"#,
        );
        query.push_bind(id_or_key);
        query.push(
            r#"::text ~* '^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$'
                    then
"#,
        );
        query.push_bind(id_or_key);
        query.push(
            r#"::uuid
                    else null::uuid
                end
               or tasks.task_key = "#,
        );
        query.push_bind(id_or_key);
        query.push(
            r#"
            limit 1
            "#,
        );

        let row = query.build().fetch_optional(&self.pool).await?;

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
            counts_are_exact: row.try_get("counts_are_exact")?,
            sample_limit: row.try_get("sample_limit")?,
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

    fn repository_source() -> String {
        include_str!("repository.rs").replace("\r\n", "\n")
    }

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
        let repository = repository_source();

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
    fn scheduled_task_run_history_queries_have_live_schema_smoke() {
        let repository = repository_source();
        let smoke_name = [
            "scheduled_task_run_history_queries",
            "execute_against_live_schema",
        ]
        .join("_");

        assert!(
            repository.contains(&format!("async fn {smoke_name}")),
            "scheduled task run history queries should have an ignored live-DB smoke"
        );
    }

    // Live-DB smoke: executes the Admin scheduled-task run history keyset query
    // against the migrated schema. The smoke inserts an isolated task and two
    // partitioned scheduled_task_runs rows, then exercises status filters,
    // cursor pagination, missing task keys, and invalid cursors through the
    // production repository method.
    //   cargo test -- --ignored scheduled_task_run_history_queries_execute_against_live_schema
    #[tokio::test]
    #[ignore = "requires a running PostgreSQL from ./scripts/dev-deps.ps1"]
    async fn scheduled_task_run_history_queries_execute_against_live_schema() {
        use sqlx::postgres::PgPoolOptions;
        use std::time::{SystemTime, UNIX_EPOCH};

        let url = std::env::var("DATABASE_URL")
            .unwrap_or_else(|_| "postgres://fbz:fbz@127.0.0.1:5432/fbz".to_owned());
        let pool = PgPoolOptions::new()
            .max_connections(2)
            .connect(&url)
            .await
            .expect("connect to live PostgreSQL");
        crate::db::migrate(&pool).await.expect("run migrations");

        let recent_index_def = sqlx::query_scalar::<_, String>(
            r#"
            select indexdef
            from pg_indexes
            where schemaname = 'public'
              and indexname = 'idx_scheduled_task_runs_task_recent'
            "#,
        )
        .fetch_one(&pool)
        .await
        .expect("scheduled task run recent index should exist");
        let recent_index_def = recent_index_def.to_ascii_lowercase();
        assert!(recent_index_def.contains("task_id"));
        assert!(recent_index_def.contains("started_at desc"));
        assert!(recent_index_def.contains("id desc"));
        assert!(recent_index_def.contains("public_id"));

        let status_index_def = sqlx::query_scalar::<_, String>(
            r#"
            select indexdef
            from pg_indexes
            where schemaname = 'public'
              and indexname = 'idx_scheduled_task_runs_task_status_recent_keyset'
            "#,
        )
        .fetch_one(&pool)
        .await
        .expect("scheduled task run task/status keyset index should exist");
        let status_index_def = status_index_def.to_ascii_lowercase();
        assert!(status_index_def.contains("task_id"));
        assert!(status_index_def.contains("status"));
        assert!(status_index_def.contains("started_at desc"));
        assert!(status_index_def.contains("id desc"));

        let suffix = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system clock should be after unix epoch")
            .as_nanos();
        let task_key = format!("admin.scheduled-run.smoke.{suffix}");

        let task_id = sqlx::query_scalar::<_, i64>(
            r#"
            insert into scheduled_tasks (
                task_key,
                task_type,
                owner_type,
                enabled,
                schedule_kind,
                schedule_value,
                next_run_at,
                timeout_seconds,
                max_concurrency,
                created_at,
                updated_at
            )
            values (
                $1,
                'admin.scheduled-run.smoke',
                'core',
                true,
                'interval',
                '3600',
                now() + interval '1 hour',
                300,
                2,
                now() - interval '3 minutes',
                now() - interval '3 minutes'
            )
            returning id
            "#,
        )
        .bind(&task_key)
        .fetch_one(&pool)
        .await
        .expect("create scheduled task run history smoke task");

        let older_run_id = sqlx::query_scalar::<_, String>(
            r#"
            insert into scheduled_task_runs (
                task_id,
                task_key,
                trigger_type,
                worker_id,
                status,
                lease_expires_at,
                queued_jobs,
                error_message,
                started_at,
                finished_at,
                created_at,
                updated_at
            )
            values (
                $1,
                $2,
                'due',
                $3,
                'failed',
                now() - interval '90 seconds',
                3,
                'scheduled task run history smoke failure',
                now() - interval '2 minutes',
                now() - interval '119 seconds',
                now() - interval '2 minutes',
                now() - interval '119 seconds'
            )
            returning public_id::text
            "#,
        )
        .bind(task_id)
        .bind(&task_key)
        .bind(format!("scheduled-run-smoke-worker-old-{suffix}"))
        .fetch_one(&pool)
        .await
        .expect("create older scheduled task run smoke row");

        let newer_run_id = sqlx::query_scalar::<_, String>(
            r#"
            insert into scheduled_task_runs (
                task_id,
                task_key,
                trigger_type,
                worker_id,
                status,
                lease_expires_at,
                queued_jobs,
                started_at,
                finished_at,
                created_at,
                updated_at
            )
            values (
                $1,
                $2,
                'manual',
                $3,
                'succeeded',
                now() + interval '5 minutes',
                1,
                now() - interval '1 minute',
                now() - interval '59 seconds',
                now() - interval '1 minute',
                now() - interval '59 seconds'
            )
            returning public_id::text
            "#,
        )
        .bind(task_id)
        .bind(&task_key)
        .bind(format!("scheduled-run-smoke-worker-new-{suffix}"))
        .fetch_one(&pool)
        .await
        .expect("create newer scheduled task run smoke row");

        let repository = AdminRepository::new(pool.clone());
        let runs_page = repository
            .list_scheduled_task_runs_page(
                &task_key,
                ScheduledTaskRunFilter {
                    status: None,
                    cursor: None,
                    limit: 10,
                },
            )
            .await
            .expect("scheduled task run history list should execute")
            .expect("smoke task should exist");
        assert!(
            runs_page.records.iter().any(|run| run.id == older_run_id),
            "older smoke scheduled task run should be listed"
        );
        assert!(
            runs_page.records.iter().any(|run| run.id == newer_run_id),
            "newer smoke scheduled task run should be listed"
        );

        let failed_runs_page = repository
            .list_scheduled_task_runs_page(
                &task_key,
                ScheduledTaskRunFilter {
                    status: Some("failed".to_owned()),
                    cursor: None,
                    limit: 10,
                },
            )
            .await
            .expect("filtered scheduled task run history list should execute")
            .expect("smoke task should exist");
        assert_eq!(failed_runs_page.records.len(), 1);
        assert_eq!(failed_runs_page.records[0].id, older_run_id);
        assert_eq!(failed_runs_page.records[0].status, "failed");
        assert_eq!(failed_runs_page.records[0].queued_jobs, Some(3));

        let cursor_runs_page = repository
            .list_scheduled_task_runs_page(
                &task_key,
                ScheduledTaskRunFilter {
                    status: None,
                    cursor: Some(newer_run_id),
                    limit: 10,
                },
            )
            .await
            .expect("cursor scheduled task run history list should execute")
            .expect("smoke task should exist");
        assert!(
            cursor_runs_page
                .records
                .iter()
                .any(|run| run.id == older_run_id),
            "older scheduled task run should be returned after newest run cursor"
        );

        let invalid_cursor_page = repository
            .list_scheduled_task_runs_page(
                &task_key,
                ScheduledTaskRunFilter {
                    status: None,
                    cursor: Some("not-a-uuid".to_owned()),
                    limit: 10,
                },
            )
            .await
            .expect("invalid scheduled task run cursor should execute safely")
            .expect("smoke task should exist");
        assert!(invalid_cursor_page.records.is_empty());

        let missing_task_page = repository
            .list_scheduled_task_runs_page(
                "admin.scheduled-run.smoke.missing",
                ScheduledTaskRunFilter {
                    status: None,
                    cursor: None,
                    limit: 10,
                },
            )
            .await
            .expect("missing scheduled task key should be safely handled");
        assert!(missing_task_page.is_none());

        sqlx::query("delete from scheduled_tasks where id = $1")
            .bind(task_id)
            .execute(&pool)
            .await
            .expect("delete smoke scheduled task and cascaded run history");
    }

    #[test]
    fn admin_job_run_event_keyset_indexes_match_query_shape() {
        let migration =
            include_str!("../../migrations/0050_admin_job_run_event_keyset_indexes.sql");
        let repository = repository_source();

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
    fn admin_job_run_event_queries_have_live_schema_smoke() {
        let repository = repository_source();
        let smoke_name = ["admin_job_run_event_queries", "execute_against_live_schema"].join("_");

        assert!(
            repository.contains(&format!("async fn {smoke_name}")),
            "Admin job run/event history queries should have an ignored live-DB smoke"
        );
    }

    // Live-DB smoke: executes the Admin job run and job event history keyset
    // queries against the migrated schema. This covers the partitioned
    // job_runs/job_events parents, their keyset indexes, detail summaries,
    // status/level filters, cursor joins, and valid-but-missing job ids through
    // the production repository methods.
    //   cargo test -- --ignored admin_job_run_event_queries_execute_against_live_schema
    #[tokio::test]
    #[ignore = "requires a running PostgreSQL from ./scripts/dev-deps.ps1"]
    async fn admin_job_run_event_queries_execute_against_live_schema() {
        use sqlx::postgres::PgPoolOptions;
        use std::time::{SystemTime, UNIX_EPOCH};

        let url = std::env::var("DATABASE_URL")
            .unwrap_or_else(|_| "postgres://fbz:fbz@127.0.0.1:5432/fbz".to_owned());
        let pool = PgPoolOptions::new()
            .max_connections(2)
            .connect(&url)
            .await
            .expect("connect to live PostgreSQL");
        crate::db::migrate(&pool).await.expect("run migrations");

        let run_index_def = sqlx::query_scalar::<_, String>(
            r#"
            select indexdef
            from pg_indexes
            where schemaname = 'public'
              and indexname = 'idx_job_runs_job_status_started_keyset'
            "#,
        )
        .fetch_one(&pool)
        .await
        .expect("job run job/status keyset index should exist");
        let run_index_def = run_index_def.to_ascii_lowercase();
        assert!(run_index_def.contains("job_id"));
        assert!(run_index_def.contains("status"));
        assert!(run_index_def.contains("started_at desc"));
        assert!(run_index_def.contains("id desc"));

        let event_index_def = sqlx::query_scalar::<_, String>(
            r#"
            select indexdef
            from pg_indexes
            where schemaname = 'public'
              and indexname = 'idx_job_events_job_level_created_keyset'
            "#,
        )
        .fetch_one(&pool)
        .await
        .expect("job event job/level keyset index should exist");
        let event_index_def = event_index_def.to_ascii_lowercase();
        assert!(event_index_def.contains("job_id"));
        assert!(event_index_def.contains("event_level"));
        assert!(event_index_def.contains("created_at desc"));
        assert!(event_index_def.contains("id desc"));

        let suffix = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system clock should be after unix epoch")
            .as_nanos();
        let job_type = format!("admin.history.smoke.{suffix}");
        let dedupe_key = format!("admin-history-smoke-{suffix}");

        let job_row = sqlx::query(
            r#"
            insert into jobs (
                job_type,
                status,
                queue_name,
                priority,
                payload,
                dedupe_key,
                run_at,
                attempts,
                max_attempts,
                created_at,
                updated_at,
                finished_at
            )
            values (
                $1,
                'succeeded',
                'admin-smoke',
                7,
                jsonb_build_object('smoke', true, 'kind', 'admin-job-history'),
                $2,
                now() - interval '3 minutes',
                1,
                3,
                now() - interval '3 minutes',
                now() - interval '1 minute',
                now() - interval '1 minute'
            )
            returning id, public_id::text as public_id
            "#,
        )
        .bind(&job_type)
        .bind(&dedupe_key)
        .fetch_one(&pool)
        .await
        .expect("create admin job history smoke job");
        let job_internal_id = job_row
            .try_get::<i64, _>("id")
            .expect("job internal id should be returned");
        let job_id = job_row
            .try_get::<String, _>("public_id")
            .expect("job public id should be returned");

        let older_run_id = sqlx::query_scalar::<_, i64>(
            r#"
            insert into job_runs (
                job_id,
                worker_id,
                status,
                started_at,
                finished_at,
                error_message,
                metrics
            )
            values (
                $1,
                $2,
                'failed',
                now() - interval '2 minutes',
                now() - interval '119 seconds',
                'admin job history smoke failure',
                jsonb_build_object('attempt', 1, 'smoke', true)
            )
            returning id
            "#,
        )
        .bind(job_internal_id)
        .bind(format!("admin-history-smoke-worker-old-{suffix}"))
        .fetch_one(&pool)
        .await
        .expect("create older admin job run smoke row");

        let newer_run_id = sqlx::query_scalar::<_, i64>(
            r#"
            insert into job_runs (
                job_id,
                worker_id,
                status,
                started_at,
                finished_at,
                metrics
            )
            values (
                $1,
                $2,
                'succeeded',
                now() - interval '1 minute',
                now() - interval '59 seconds',
                jsonb_build_object('attempt', 2, 'smoke', true)
            )
            returning id
            "#,
        )
        .bind(job_internal_id)
        .bind(format!("admin-history-smoke-worker-new-{suffix}"))
        .fetch_one(&pool)
        .await
        .expect("create newer admin job run smoke row");

        let older_event_id = sqlx::query_scalar::<_, i64>(
            r#"
            insert into job_events (
                job_id,
                job_run_id,
                event_type,
                event_level,
                message,
                payload,
                created_at
            )
            values (
                $1,
                $2,
                'admin.history.smoke.failed',
                'warn',
                'older admin job history smoke event',
                jsonb_build_object('ordinal', 1, 'smoke', true),
                now() - interval '40 seconds'
            )
            returning id
            "#,
        )
        .bind(job_internal_id)
        .bind(older_run_id)
        .fetch_one(&pool)
        .await
        .expect("create older admin job event smoke row");

        let newer_event_id = sqlx::query_scalar::<_, i64>(
            r#"
            insert into job_events (
                job_id,
                job_run_id,
                event_type,
                event_level,
                message,
                payload,
                created_at
            )
            values (
                $1,
                $2,
                'admin.history.smoke.succeeded',
                'info',
                'newer admin job history smoke event',
                jsonb_build_object('ordinal', 2, 'smoke', true),
                now() - interval '20 seconds'
            )
            returning id
            "#,
        )
        .bind(job_internal_id)
        .bind(newer_run_id)
        .fetch_one(&pool)
        .await
        .expect("create newer admin job event smoke row");

        let repository = AdminRepository::new(pool.clone());
        let detail = repository
            .get_admin_job_detail(&job_id, 10, 10)
            .await
            .expect("admin job detail should execute against live schema")
            .expect("smoke job should exist");
        assert_eq!(detail.job.id, job_id);
        assert!(detail.runs.iter().any(|run| run.id == older_run_id));
        assert!(detail.runs.iter().any(|run| run.id == newer_run_id));
        assert!(detail.events.iter().any(|event| event.id == older_event_id));
        assert!(detail.events.iter().any(|event| event.id == newer_event_id));

        let runs_page = repository
            .list_admin_job_runs_page(
                &job_id,
                AdminJobRunFilter {
                    status: None,
                    cursor: None,
                    limit: 10,
                },
            )
            .await
            .expect("admin job run list should execute")
            .expect("smoke job should exist");
        assert!(
            runs_page.records.iter().any(|run| run.id == older_run_id),
            "older smoke job run should be listed"
        );
        assert!(
            runs_page.records.iter().any(|run| run.id == newer_run_id),
            "newer smoke job run should be listed"
        );

        let failed_runs_page = repository
            .list_admin_job_runs_page(
                &job_id,
                AdminJobRunFilter {
                    status: Some("failed".to_owned()),
                    cursor: None,
                    limit: 10,
                },
            )
            .await
            .expect("filtered admin job run list should execute")
            .expect("smoke job should exist");
        assert_eq!(failed_runs_page.records.len(), 1);
        assert_eq!(failed_runs_page.records[0].id, older_run_id);
        assert_eq!(failed_runs_page.records[0].status, "failed");

        let cursor_runs_page = repository
            .list_admin_job_runs_page(
                &job_id,
                AdminJobRunFilter {
                    status: None,
                    cursor: Some(newer_run_id),
                    limit: 10,
                },
            )
            .await
            .expect("cursor admin job run list should execute")
            .expect("smoke job should exist");
        assert!(
            cursor_runs_page
                .records
                .iter()
                .any(|run| run.id == older_run_id),
            "older job run should be returned after newest run cursor"
        );

        let events_page = repository
            .list_admin_job_events_page(
                &job_id,
                AdminJobEventFilter {
                    event_level: None,
                    cursor: None,
                    limit: 10,
                },
            )
            .await
            .expect("admin job event list should execute")
            .expect("smoke job should exist");
        assert!(
            events_page
                .records
                .iter()
                .any(|event| event.id == older_event_id),
            "older smoke job event should be listed"
        );
        assert!(
            events_page
                .records
                .iter()
                .any(|event| event.id == newer_event_id),
            "newer smoke job event should be listed"
        );

        let warn_events_page = repository
            .list_admin_job_events_page(
                &job_id,
                AdminJobEventFilter {
                    event_level: Some("warn".to_owned()),
                    cursor: None,
                    limit: 10,
                },
            )
            .await
            .expect("filtered admin job event list should execute")
            .expect("smoke job should exist");
        assert_eq!(warn_events_page.records.len(), 1);
        assert_eq!(warn_events_page.records[0].id, older_event_id);
        assert_eq!(warn_events_page.records[0].event_level, "warn");

        let cursor_events_page = repository
            .list_admin_job_events_page(
                &job_id,
                AdminJobEventFilter {
                    event_level: None,
                    cursor: Some(newer_event_id),
                    limit: 10,
                },
            )
            .await
            .expect("cursor admin job event list should execute")
            .expect("smoke job should exist");
        assert!(
            cursor_events_page
                .records
                .iter()
                .any(|event| event.id == older_event_id),
            "older job event should be returned after newest event cursor"
        );

        let missing_job_id = "00000000-0000-0000-0000-000000000000";
        let missing_runs_page = repository
            .list_admin_job_runs_page(
                missing_job_id,
                AdminJobRunFilter {
                    status: None,
                    cursor: None,
                    limit: 10,
                },
            )
            .await
            .expect("valid missing job id should be safely handled for runs");
        assert!(missing_runs_page.is_none());

        let missing_events_page = repository
            .list_admin_job_events_page(
                missing_job_id,
                AdminJobEventFilter {
                    event_level: None,
                    cursor: None,
                    limit: 10,
                },
            )
            .await
            .expect("valid missing job id should be safely handled for events");
        assert!(missing_events_page.is_none());

        sqlx::query("delete from jobs where id = $1")
            .bind(job_internal_id)
            .execute(&pool)
            .await
            .expect("delete smoke job and cascaded run/event history");
    }

    #[test]
    fn admin_job_list_keyset_indexes_match_query_shape() {
        let migration = include_str!("../../migrations/0054_admin_job_list_keyset_indexes.sql");
        let repository = repository_source();

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
    fn admin_job_list_queries_have_live_schema_smoke() {
        let repository = repository_source();
        let smoke_name = ["admin_job_list_queries", "execute_against_live_schema"].join("_");

        assert!(
            repository.contains(&format!("async fn {smoke_name}")),
            "Admin job list queries should have an ignored live-DB smoke"
        );
    }

    // Live-DB smoke: executes the Admin jobs keyset list query against the
    // migrated schema. The smoke inserts two isolated jobs and exercises
    // status/type/queue filters, cursor pagination, and an invalid cursor
    // through the production repository method.
    //   cargo test -- --ignored admin_job_list_queries_execute_against_live_schema
    #[tokio::test]
    #[ignore = "requires a running PostgreSQL from ./scripts/dev-deps.ps1"]
    async fn admin_job_list_queries_execute_against_live_schema() {
        use sqlx::postgres::PgPoolOptions;
        use std::time::{SystemTime, UNIX_EPOCH};

        let url = std::env::var("DATABASE_URL")
            .unwrap_or_else(|_| "postgres://fbz:fbz@127.0.0.1:5432/fbz".to_owned());
        let pool = PgPoolOptions::new()
            .max_connections(2)
            .connect(&url)
            .await
            .expect("connect to live PostgreSQL");
        crate::db::migrate(&pool).await.expect("run migrations");

        let status_index_def = sqlx::query_scalar::<_, String>(
            r#"
            select indexdef
            from pg_indexes
            where schemaname = 'public'
              and indexname = 'idx_jobs_admin_status_recent_keyset'
            "#,
        )
        .fetch_one(&pool)
        .await
        .expect("admin job status keyset index should exist");
        let status_index_def = status_index_def.to_ascii_lowercase();
        assert!(status_index_def.contains("status"));
        assert!(status_index_def.contains("created_at desc"));
        assert!(status_index_def.contains("id desc"));

        let queue_index_def = sqlx::query_scalar::<_, String>(
            r#"
            select indexdef
            from pg_indexes
            where schemaname = 'public'
              and indexname = 'idx_jobs_admin_queue_recent_keyset'
            "#,
        )
        .fetch_one(&pool)
        .await
        .expect("admin job queue keyset index should exist");
        let queue_index_def = queue_index_def.to_ascii_lowercase();
        assert!(queue_index_def.contains("queue_name"));
        assert!(queue_index_def.contains("created_at desc"));
        assert!(queue_index_def.contains("id desc"));

        let suffix = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system clock should be after unix epoch")
            .as_nanos();
        let job_type = format!("admin.job-list.smoke.{suffix}");
        let queue_name = format!("admin-smoke-{suffix}");
        let older_dedupe_key = format!("admin-job-list-smoke-older-{suffix}");
        let newer_dedupe_key = format!("admin-job-list-smoke-newer-{suffix}");

        let older_job_row = sqlx::query(
            r#"
            insert into jobs (
                job_type,
                status,
                queue_name,
                priority,
                payload,
                dedupe_key,
                run_at,
                attempts,
                max_attempts,
                last_error,
                created_at,
                updated_at
            )
            values (
                $1,
                'failed',
                $2,
                3,
                jsonb_build_object('smoke', true, 'ordinal', 1),
                $3,
                now() - interval '3 minutes',
                1,
                3,
                'admin job list smoke failure',
                now() - interval '3 minutes',
                now() - interval '3 minutes'
            )
            returning id, public_id::text as public_id
            "#,
        )
        .bind(&job_type)
        .bind(&queue_name)
        .bind(&older_dedupe_key)
        .fetch_one(&pool)
        .await
        .expect("create older admin job list smoke row");
        let older_job_internal_id = older_job_row
            .try_get::<i64, _>("id")
            .expect("older job internal id should be returned");
        let older_job_id = older_job_row
            .try_get::<String, _>("public_id")
            .expect("older job public id should be returned");

        let newer_job_row = sqlx::query(
            r#"
            insert into jobs (
                job_type,
                status,
                queue_name,
                priority,
                payload,
                dedupe_key,
                run_at,
                attempts,
                max_attempts,
                created_at,
                updated_at,
                finished_at
            )
            values (
                $1,
                'succeeded',
                $2,
                8,
                jsonb_build_object('smoke', true, 'ordinal', 2),
                $3,
                now() - interval '1 minute',
                1,
                3,
                now() - interval '1 minute',
                now() - interval '1 minute',
                now() - interval '30 seconds'
            )
            returning id, public_id::text as public_id
            "#,
        )
        .bind(&job_type)
        .bind(&queue_name)
        .bind(&newer_dedupe_key)
        .fetch_one(&pool)
        .await
        .expect("create newer admin job list smoke row");
        let newer_job_internal_id = newer_job_row
            .try_get::<i64, _>("id")
            .expect("newer job internal id should be returned");
        let newer_job_id = newer_job_row
            .try_get::<String, _>("public_id")
            .expect("newer job public id should be returned");

        let repository = AdminRepository::new(pool.clone());
        let job_page = repository
            .list_admin_jobs_page(AdminJobFilter {
                status: None,
                job_type: Some(job_type.clone()),
                queue_name: Some(queue_name.clone()),
                cursor: None,
                limit: 10,
            })
            .await
            .expect("admin job list should execute");
        assert!(
            job_page.records.iter().any(|job| job.id == older_job_id),
            "older smoke job should be listed"
        );
        assert!(
            job_page.records.iter().any(|job| job.id == newer_job_id),
            "newer smoke job should be listed"
        );

        let failed_page = repository
            .list_admin_jobs_page(AdminJobFilter {
                status: Some("failed".to_owned()),
                job_type: Some(job_type.clone()),
                queue_name: Some(queue_name.clone()),
                cursor: None,
                limit: 10,
            })
            .await
            .expect("filtered admin job list should execute");
        assert_eq!(failed_page.records.len(), 1);
        assert_eq!(failed_page.records[0].id, older_job_id);
        assert_eq!(failed_page.records[0].status, "failed");

        let cursor_page = repository
            .list_admin_jobs_page(AdminJobFilter {
                status: None,
                job_type: Some(job_type.clone()),
                queue_name: Some(queue_name.clone()),
                cursor: Some(newer_job_id),
                limit: 10,
            })
            .await
            .expect("cursor admin job list should execute");
        assert!(
            cursor_page.records.iter().any(|job| job.id == older_job_id),
            "older job should be returned after newest job cursor"
        );

        let invalid_cursor_page = repository
            .list_admin_jobs_page(AdminJobFilter {
                status: None,
                job_type: Some(job_type.clone()),
                queue_name: Some(queue_name.clone()),
                cursor: Some("not-a-uuid".to_owned()),
                limit: 10,
            })
            .await
            .expect("invalid cursor admin job list should execute safely");
        assert!(invalid_cursor_page.records.is_empty());

        sqlx::query("delete from jobs where id = any($1)")
            .bind(&[older_job_internal_id, newer_job_internal_id][..])
            .execute(&pool)
            .await
            .expect("delete smoke jobs");
    }

    #[test]
    fn admin_user_list_keyset_indexes_match_query_shape() {
        let migration = include_str!("../../migrations/0055_admin_user_list_keyset_indexes.sql");
        let repository = repository_source();

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
    fn admin_user_list_queries_have_live_schema_smoke() {
        let repository = repository_source();
        let smoke_name = ["admin_user_list_queries", "execute_against_live_schema"].join("_");

        assert!(
            repository.contains(&format!("async fn {smoke_name}")),
            "admin user list keyset queries should have an ignored live-DB smoke"
        );
    }

    // Live-DB smoke: executes the Admin user list keyset query against the
    // migrated schema. The smoke inserts isolated users and exercises role,
    // disabled, role+disabled, cursor, and invalid cursor behavior through the
    // repository method.
    //   cargo test -- --ignored admin_user_list_queries_execute_against_live_schema
    #[tokio::test]
    #[ignore = "requires a running PostgreSQL from ./scripts/dev-deps.ps1"]
    async fn admin_user_list_queries_execute_against_live_schema() {
        use sqlx::postgres::PgPoolOptions;
        use std::time::{SystemTime, UNIX_EPOCH};

        let url = std::env::var("DATABASE_URL")
            .unwrap_or_else(|_| "postgres://fbz:fbz@127.0.0.1:5432/fbz".to_owned());
        let pool = PgPoolOptions::new()
            .max_connections(2)
            .connect(&url)
            .await
            .expect("connect to live PostgreSQL");
        crate::db::migrate(&pool).await.expect("run migrations");

        let username_index_def = sqlx::query_scalar::<_, String>(
            r#"
            select indexdef
            from pg_indexes
            where schemaname = 'public'
              and indexname = 'idx_users_admin_username_keyset'
            "#,
        )
        .fetch_one(&pool)
        .await
        .expect("admin user username keyset index should exist");
        let username_index_def = username_index_def.to_ascii_lowercase();
        assert!(username_index_def.contains("username_normalized"));
        assert!(username_index_def.contains("id"));

        let role_index_def = sqlx::query_scalar::<_, String>(
            r#"
            select indexdef
            from pg_indexes
            where schemaname = 'public'
              and indexname = 'idx_users_admin_role_username_keyset'
            "#,
        )
        .fetch_one(&pool)
        .await
        .expect("admin user role/username keyset index should exist");
        let role_index_def = role_index_def.to_ascii_lowercase();
        assert!(role_index_def.contains("role_id"));
        assert!(role_index_def.contains("username_normalized"));
        assert!(role_index_def.contains("id"));

        let disabled_index_def = sqlx::query_scalar::<_, String>(
            r#"
            select indexdef
            from pg_indexes
            where schemaname = 'public'
              and indexname = 'idx_users_admin_disabled_username_keyset'
            "#,
        )
        .fetch_one(&pool)
        .await
        .expect("admin user disabled/username keyset index should exist");
        let disabled_index_def = disabled_index_def.to_ascii_lowercase();
        assert!(disabled_index_def.contains("is_disabled"));
        assert!(disabled_index_def.contains("username_normalized"));
        assert!(disabled_index_def.contains("id"));

        let role_disabled_index_def = sqlx::query_scalar::<_, String>(
            r#"
            select indexdef
            from pg_indexes
            where schemaname = 'public'
              and indexname = 'idx_users_admin_role_disabled_username_keyset'
            "#,
        )
        .fetch_one(&pool)
        .await
        .expect("admin user role/disabled/username keyset index should exist");
        let role_disabled_index_def = role_disabled_index_def.to_ascii_lowercase();
        assert!(role_disabled_index_def.contains("role_id"));
        assert!(role_disabled_index_def.contains("is_disabled"));
        assert!(role_disabled_index_def.contains("username_normalized"));
        assert!(role_disabled_index_def.contains("id"));

        let suffix = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system clock should be after unix epoch")
            .as_nanos();
        let role_name = format!("Admin User List Smoke {suffix}");
        let role_normalized = format!("admin-user-list-smoke-{suffix}");
        let other_role_name = format!("Admin User List Other Smoke {suffix}");
        let other_role_normalized = format!("admin-user-list-other-smoke-{suffix}");
        let first_username = format!("0000_admin_user_list_smoke_a_{suffix}");
        let second_username = format!("0000_admin_user_list_smoke_b_{suffix}");
        let disabled_username = format!("0000_admin_user_list_smoke_c_{suffix}");
        let other_username = format!("0000_admin_user_list_smoke_d_{suffix}");

        let role_id = sqlx::query_scalar::<_, i64>(
            r#"
            insert into roles (name, name_normalized, description, is_builtin)
            values ($1, $2, 'admin user list smoke role', false)
            returning id
            "#,
        )
        .bind(&role_name)
        .bind(&role_normalized)
        .fetch_one(&pool)
        .await
        .expect("create admin user list smoke role");

        let other_role_id = sqlx::query_scalar::<_, i64>(
            r#"
            insert into roles (name, name_normalized, description, is_builtin)
            values ($1, $2, 'admin user list other smoke role', false)
            returning id
            "#,
        )
        .bind(&other_role_name)
        .bind(&other_role_normalized)
        .fetch_one(&pool)
        .await
        .expect("create admin user list other smoke role");

        let first_user_id = sqlx::query_scalar::<_, String>(
            r#"
            insert into users (
                username,
                username_normalized,
                display_name,
                role_id,
                is_disabled,
                allow_download,
                allow_transcode,
                allow_new_device_login
            )
            values ($1, $1, 'Admin user list smoke A', $2, false, true, true, true)
            returning public_id::text
            "#,
        )
        .bind(&first_username)
        .bind(role_id)
        .fetch_one(&pool)
        .await
        .expect("create first admin user list smoke user");

        let second_user_id = sqlx::query_scalar::<_, String>(
            r#"
            insert into users (
                username,
                username_normalized,
                display_name,
                role_id,
                is_disabled,
                allow_download,
                allow_transcode,
                allow_new_device_login
            )
            values ($1, $1, 'Admin user list smoke B', $2, false, true, true, true)
            returning public_id::text
            "#,
        )
        .bind(&second_username)
        .bind(role_id)
        .fetch_one(&pool)
        .await
        .expect("create second admin user list smoke user");

        let disabled_user_id = sqlx::query_scalar::<_, String>(
            r#"
            insert into users (
                username,
                username_normalized,
                display_name,
                role_id,
                is_disabled,
                allow_download,
                allow_transcode,
                allow_new_device_login
            )
            values ($1, $1, 'Admin user list smoke C', $2, true, false, false, false)
            returning public_id::text
            "#,
        )
        .bind(&disabled_username)
        .bind(role_id)
        .fetch_one(&pool)
        .await
        .expect("create disabled admin user list smoke user");

        let other_user_id = sqlx::query_scalar::<_, String>(
            r#"
            insert into users (
                username,
                username_normalized,
                display_name,
                role_id,
                is_disabled,
                allow_download,
                allow_transcode,
                allow_new_device_login
            )
            values ($1, $1, 'Admin user list smoke D', $2, false, true, true, true)
            returning public_id::text
            "#,
        )
        .bind(&other_username)
        .bind(other_role_id)
        .fetch_one(&pool)
        .await
        .expect("create other-role admin user list smoke user");

        let repository = AdminRepository::new(pool.clone());
        let role_page = repository
            .list_admin_users_page(AdminUserFilter {
                role_name: Some(role_normalized.clone()),
                is_disabled: None,
                cursor: None,
                limit: 10,
            })
            .await
            .expect("admin user role filter list should execute");
        assert!(
            role_page
                .records
                .iter()
                .any(|user| user.id == first_user_id && user.role_name == role_name)
        );
        assert!(
            role_page
                .records
                .iter()
                .any(|user| user.id == second_user_id && user.role_name == role_name)
        );
        assert!(
            role_page
                .records
                .iter()
                .any(|user| user.id == disabled_user_id && user.is_disabled)
        );
        assert!(
            !role_page
                .records
                .iter()
                .any(|user| user.id == other_user_id),
            "role filter should exclude users from other roles"
        );
        assert!(
            role_page
                .records
                .iter()
                .all(|user| user.role_name == role_name)
        );

        let enabled_page = repository
            .list_admin_users_page(AdminUserFilter {
                role_name: Some(role_normalized.clone()),
                is_disabled: Some(false),
                cursor: None,
                limit: 10,
            })
            .await
            .expect("admin user role+enabled filter list should execute");
        assert!(
            enabled_page
                .records
                .iter()
                .any(|user| user.id == first_user_id && !user.is_disabled)
        );
        assert!(
            enabled_page
                .records
                .iter()
                .all(|user| user.role_name == role_name && !user.is_disabled)
        );
        assert!(
            !enabled_page
                .records
                .iter()
                .any(|user| user.id == disabled_user_id),
            "disabled smoke user should be excluded by enabled filter"
        );

        let disabled_page = repository
            .list_admin_users_page(AdminUserFilter {
                role_name: Some(role_normalized.clone()),
                is_disabled: Some(true),
                cursor: None,
                limit: 10,
            })
            .await
            .expect("admin user role+disabled filter list should execute");
        assert_eq!(disabled_page.records.len(), 1);
        assert_eq!(disabled_page.records[0].id, disabled_user_id);
        assert!(disabled_page.records[0].is_disabled);

        let cursor_page = repository
            .list_admin_users_page(AdminUserFilter {
                role_name: Some(role_normalized.clone()),
                is_disabled: Some(false),
                cursor: Some(first_user_id.clone()),
                limit: 10,
            })
            .await
            .expect("admin user cursor list should execute");
        assert!(
            cursor_page
                .records
                .iter()
                .any(|user| user.id == second_user_id),
            "later same-role enabled user should be returned after first user cursor"
        );
        assert!(
            !cursor_page
                .records
                .iter()
                .any(|user| user.id == first_user_id),
            "cursor page should not include the cursor user"
        );

        let invalid_cursor_page = repository
            .list_admin_users_page(AdminUserFilter {
                role_name: Some(role_normalized),
                is_disabled: Some(false),
                cursor: Some("not-a-uuid".to_owned()),
                limit: 10,
            })
            .await
            .expect("invalid admin user cursor should be safely handled");
        assert!(invalid_cursor_page.records.is_empty());
        assert!(!invalid_cursor_page.has_more);
        assert!(invalid_cursor_page.next_cursor.is_none());

        sqlx::query("delete from users where username_normalized = any($1)")
            .bind(
                &[
                    first_username,
                    second_username,
                    disabled_username,
                    other_username,
                ][..],
            )
            .execute(&pool)
            .await
            .expect("delete admin user list smoke users");
        sqlx::query("delete from roles where id = any($1)")
            .bind(&[role_id, other_role_id][..])
            .execute(&pool)
            .await
            .expect("delete admin user list smoke roles");
    }

    #[test]
    fn admin_user_summaries_use_bounded_probes() {
        let repository = repository_source();
        let production_source = repository
            .split("#[cfg(test)]")
            .next()
            .expect("repository source should include production section");

        assert!(production_source.contains("ADMIN_USER_SUMMARY_SAMPLE_LIMIT"));
        assert!(production_source.contains("ADMIN_USER_SUMMARY_FETCH_LIMIT"));
        assert!(production_source.contains("push_admin_user_counts_sql"));
        assert!(production_source.contains("device_count_probe"));
        assert!(production_source.contains("active_session_count_probe"));
        assert!(production_source.contains("query.push_bind(ADMIN_USER_SUMMARY_SAMPLE_LIMIT)"));
        assert!(production_source.contains("query.push_bind(ADMIN_USER_SUMMARY_FETCH_LIMIT)"));

        let normalized = production_source
            .split_whitespace()
            .collect::<Vec<_>>()
            .join(" ");
        assert!(
            !normalized.contains("select count(*)::bigint from devices d where d.user_id = u.id"),
            "admin user list should not exact-count every device per user"
        );
        assert!(
            !normalized.contains("select count(*)::bigint from sessions s where s.user_id = u.id"),
            "admin user list should not exact-count every active session per user"
        );
    }

    // Live-DB smoke: executes the Admin user list and policy update paths
    // against the real migrated schema. It creates and removes an isolated
    // smoke user so the repository methods, QueryBuilder binds, and shared
    // bounded summary probes are exercised end to end.
    //   cargo test -- --ignored admin_user_summary_probes_execute_against_live_schema
    #[tokio::test]
    #[ignore = "requires a running PostgreSQL from ./scripts/dev-deps.ps1"]
    async fn admin_user_summary_probes_execute_against_live_schema() {
        use sqlx::postgres::PgPoolOptions;
        use std::time::{SystemTime, UNIX_EPOCH};

        let url = std::env::var("DATABASE_URL")
            .unwrap_or_else(|_| "postgres://fbz:fbz@127.0.0.1:5432/fbz".to_owned());
        let pool = PgPoolOptions::new()
            .max_connections(2)
            .connect(&url)
            .await
            .expect("connect to live PostgreSQL");
        crate::db::migrate(&pool).await.expect("run migrations");

        let suffix = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system clock should be after unix epoch")
            .as_nanos();
        let role_name = format!("admin_summary_smoke_{suffix}");
        let username = format!("admin_summary_user_{suffix}");

        let role_id = sqlx::query_scalar::<_, i64>(
            r#"
            insert into roles (name, name_normalized, description, is_builtin)
            values ($1, $1, 'admin user summary smoke role', false)
            returning id
            "#,
        )
        .bind(&role_name)
        .fetch_one(&pool)
        .await
        .expect("create smoke role");

        let (user_id, user_public_id) = sqlx::query_as::<_, (i64, String)>(
            r#"
            insert into users (
                username,
                username_normalized,
                display_name,
                role_id,
                allow_download,
                allow_transcode,
                allow_new_device_login
            )
            values ($1, $1, 'Admin summary smoke user', $2, true, true, true)
            returning id, public_id::text
            "#,
        )
        .bind(&username)
        .bind(role_id)
        .fetch_one(&pool)
        .await
        .expect("create smoke user");

        for index in 0..2 {
            sqlx::query(
                r#"
                insert into devices (user_id, device_id, device_name, last_seen_at)
                values ($1, $2, $3, now())
                "#,
            )
            .bind(user_id)
            .bind(format!("admin-summary-device-{suffix}-{index}"))
            .bind(format!("Admin summary device {index}"))
            .execute(&pool)
            .await
            .expect("create smoke device");

            sqlx::query(
                r#"
                insert into sessions (user_id, access_token_hash, expires_at)
                values ($1, gen_random_bytes(32), now() + interval '1 hour')
                "#,
            )
            .bind(user_id)
            .execute(&pool)
            .await
            .expect("create smoke session");
        }

        let repository = AdminRepository::new(pool.clone());
        let page = repository
            .list_admin_users_page(AdminUserFilter {
                role_name: Some(role_name.clone()),
                limit: 5,
                ..AdminUserFilter::default()
            })
            .await
            .expect("admin user list should execute against live schema");
        let listed = page
            .records
            .iter()
            .find(|record| record.id == user_public_id)
            .cloned();

        let updated = repository
            .update_user_policy(
                &user_public_id,
                UpdateUserPolicyInput {
                    display_name: Some("Updated admin summary smoke user".to_owned()),
                    is_disabled: false,
                    allow_download: true,
                    allow_transcode: true,
                    allow_new_device_login: true,
                },
            )
            .await
            .expect("admin user policy update should execute against live schema");

        sqlx::query("delete from users where id = $1")
            .bind(user_id)
            .execute(&pool)
            .await
            .expect("delete smoke user");
        sqlx::query("delete from roles where id = $1")
            .bind(role_id)
            .execute(&pool)
            .await
            .expect("delete smoke role");

        let listed = listed.expect("smoke user should be present in filtered admin list");
        assert_eq!(listed.device_count, 2);
        assert_eq!(listed.active_session_count, 2);

        let updated = updated.expect("smoke user should be updated by public id");
        assert_eq!(updated.device_count, 2);
        assert_eq!(updated.active_session_count, 2);
    }

    #[test]
    fn admin_user_library_permission_keyset_indexes_match_query_shape() {
        let migration =
            include_str!("../../migrations/0056_admin_user_library_permission_keyset_indexes.sql");
        let repository = repository_source();

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
    fn admin_user_library_permission_queries_have_live_schema_smoke() {
        let repository = repository_source();
        let smoke_name = [
            "admin_user_library_permission_queries",
            "execute_against_live_schema",
        ]
        .join("_");

        assert!(
            repository.contains(&format!("async fn {smoke_name}")),
            "admin user library-permission keyset queries should have an ignored live-DB smoke"
        );
    }

    // Live-DB smoke: executes the Admin user library-permission keyset query
    // against the migrated schema. The smoke inserts an isolated user and
    // libraries, then exercises type/configured filters, cursor pagination,
    // hidden-library effective permissions, and missing user behavior through
    // the repository method.
    //   cargo test -- --ignored admin_user_library_permission_queries_execute_against_live_schema
    #[tokio::test]
    #[ignore = "requires a running PostgreSQL from ./scripts/dev-deps.ps1"]
    async fn admin_user_library_permission_queries_execute_against_live_schema() {
        use sqlx::postgres::PgPoolOptions;
        use std::time::{SystemTime, UNIX_EPOCH};

        let url = std::env::var("DATABASE_URL")
            .unwrap_or_else(|_| "postgres://fbz:fbz@127.0.0.1:5432/fbz".to_owned());
        let pool = PgPoolOptions::new()
            .max_connections(2)
            .connect(&url)
            .await
            .expect("connect to live PostgreSQL");
        crate::db::migrate(&pool).await.expect("run migrations");

        let name_index_def = sqlx::query_scalar::<_, String>(
            r#"
            select indexdef
            from pg_indexes
            where schemaname = 'public'
              and indexname = 'idx_libraries_admin_name_keyset'
            "#,
        )
        .fetch_one(&pool)
        .await
        .expect("admin library name keyset index should exist");
        let name_index_def = name_index_def.to_ascii_lowercase();
        assert!(name_index_def.contains("name"));
        assert!(name_index_def.contains("id"));

        let type_index_def = sqlx::query_scalar::<_, String>(
            r#"
            select indexdef
            from pg_indexes
            where schemaname = 'public'
              and indexname = 'idx_libraries_admin_type_name_keyset'
            "#,
        )
        .fetch_one(&pool)
        .await
        .expect("admin library type/name keyset index should exist");
        let type_index_def = type_index_def.to_ascii_lowercase();
        assert!(type_index_def.contains("library_type"));
        assert!(type_index_def.contains("name"));
        assert!(type_index_def.contains("id"));

        let suffix = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system clock should be after unix epoch")
            .as_nanos();
        let role_name = format!("Library permission smoke {suffix}");
        let role_name_normalized = format!("library-permission-smoke-{suffix}");
        let username = format!("library-permission-smoke-{suffix}");
        let first_library_name = format!("0000 admin permission smoke a {suffix}");
        let second_library_name = format!("0000 admin permission smoke b {suffix}");
        let hidden_library_name = format!("0000 admin permission smoke c {suffix}");
        let unconfigured_library_name = format!("0000 admin permission smoke d {suffix}");

        let role_id = sqlx::query_scalar::<_, i64>(
            r#"
            insert into roles (name, name_normalized, description)
            values ($1, $2, 'admin user library permission smoke role')
            returning id
            "#,
        )
        .bind(&role_name)
        .bind(&role_name_normalized)
        .fetch_one(&pool)
        .await
        .expect("create library permission smoke role");

        let user_row = sqlx::query(
            r#"
            insert into users (
                username,
                username_normalized,
                role_id,
                allow_download,
                allow_transcode,
                allow_new_device_login
            )
            values ($1, $1, $2, true, true, true)
            returning id, public_id::text as public_id
            "#,
        )
        .bind(&username)
        .bind(role_id)
        .fetch_one(&pool)
        .await
        .expect("create library permission smoke user");
        let user_id = user_row
            .try_get::<i64, _>("id")
            .expect("smoke user internal id should be returned");
        let user_public_id = user_row
            .try_get::<String, _>("public_id")
            .expect("smoke user public id should be returned");

        let first_library_row = sqlx::query(
            r#"
            insert into libraries (name, library_type, is_hidden)
            values ($1, 'movies', false)
            returning id, public_id::text as public_id
            "#,
        )
        .bind(&first_library_name)
        .fetch_one(&pool)
        .await
        .expect("create first smoke library");
        let first_library_internal_id = first_library_row
            .try_get::<i64, _>("id")
            .expect("first smoke library internal id should be returned");
        let first_library_id = first_library_row
            .try_get::<String, _>("public_id")
            .expect("first smoke library public id should be returned");

        let second_library_row = sqlx::query(
            r#"
            insert into libraries (name, library_type, is_hidden)
            values ($1, 'movies', false)
            returning id, public_id::text as public_id
            "#,
        )
        .bind(&second_library_name)
        .fetch_one(&pool)
        .await
        .expect("create second smoke library");
        let second_library_internal_id = second_library_row
            .try_get::<i64, _>("id")
            .expect("second smoke library internal id should be returned");
        let second_library_id = second_library_row
            .try_get::<String, _>("public_id")
            .expect("second smoke library public id should be returned");

        let hidden_library_row = sqlx::query(
            r#"
            insert into libraries (name, library_type, is_hidden)
            values ($1, 'music', true)
            returning id, public_id::text as public_id
            "#,
        )
        .bind(&hidden_library_name)
        .fetch_one(&pool)
        .await
        .expect("create hidden smoke library");
        let hidden_library_internal_id = hidden_library_row
            .try_get::<i64, _>("id")
            .expect("hidden smoke library internal id should be returned");
        let hidden_library_id = hidden_library_row
            .try_get::<String, _>("public_id")
            .expect("hidden smoke library public id should be returned");

        let unconfigured_library_id = sqlx::query_scalar::<_, String>(
            r#"
            insert into libraries (name, library_type, is_hidden)
            values ($1, 'movies', false)
            returning public_id::text
            "#,
        )
        .bind(&unconfigured_library_name)
        .fetch_one(&pool)
        .await
        .expect("create unconfigured smoke library");

        sqlx::query(
            r#"
            insert into library_permissions (
                library_id,
                user_id,
                can_view,
                can_download,
                can_transcode
            )
            values
                ($1, $4, true, true, false),
                ($2, $4, true, false, true),
                ($3, $4, true, true, true)
            "#,
        )
        .bind(first_library_internal_id)
        .bind(second_library_internal_id)
        .bind(hidden_library_internal_id)
        .bind(user_id)
        .execute(&pool)
        .await
        .expect("create smoke library permissions");

        let repository = AdminRepository::new(pool.clone());
        let configured_movies = repository
            .list_user_library_permissions_page(
                &user_public_id,
                AdminUserLibraryPermissionFilter {
                    library_type: Some("movies".to_owned()),
                    permission_configured: Some(true),
                    cursor: None,
                    limit: 10,
                },
            )
            .await
            .expect("configured movie library permission list should execute")
            .expect("smoke user should exist");
        assert!(
            configured_movies
                .records
                .iter()
                .any(|permission| permission.library_id == first_library_id
                    && permission.permission_configured
                    && permission.can_download
                    && !permission.can_transcode
                    && permission.effective_can_download
                    && !permission.effective_can_transcode),
            "first configured movie permission should be listed with effective permissions"
        );
        assert!(
            configured_movies
                .records
                .iter()
                .any(|permission| permission.library_id == second_library_id
                    && permission.permission_configured
                    && !permission.can_download
                    && permission.can_transcode
                    && !permission.effective_can_download
                    && permission.effective_can_transcode),
            "second configured movie permission should be listed with effective permissions"
        );
        assert!(
            configured_movies.records.iter().all(|permission| {
                permission.library_type == "movies" && permission.permission_configured
            }),
            "configured movie filter should only return configured movie permissions"
        );

        let cursor_page = repository
            .list_user_library_permissions_page(
                &user_public_id,
                AdminUserLibraryPermissionFilter {
                    library_type: Some("movies".to_owned()),
                    permission_configured: Some(true),
                    cursor: Some(first_library_id.clone()),
                    limit: 10,
                },
            )
            .await
            .expect("cursor library permission list should execute")
            .expect("smoke user should exist");
        assert!(
            cursor_page
                .records
                .iter()
                .any(|permission| permission.library_id == second_library_id),
            "later configured library should be returned after first library cursor"
        );
        assert!(
            !cursor_page
                .records
                .iter()
                .any(|permission| permission.library_id == first_library_id),
            "cursor page should not include the cursor library"
        );

        let hidden_music = repository
            .list_user_library_permissions_page(
                &user_public_id,
                AdminUserLibraryPermissionFilter {
                    library_type: Some("music".to_owned()),
                    permission_configured: Some(true),
                    cursor: None,
                    limit: 10,
                },
            )
            .await
            .expect("hidden library permission list should execute")
            .expect("smoke user should exist");
        let hidden_permission = hidden_music
            .records
            .iter()
            .find(|permission| permission.library_id == hidden_library_id)
            .expect("hidden configured library should be listed");
        assert!(hidden_permission.is_hidden);
        assert!(hidden_permission.can_view);
        assert!(hidden_permission.can_download);
        assert!(hidden_permission.can_transcode);
        assert!(!hidden_permission.effective_can_view);
        assert!(!hidden_permission.effective_can_download);
        assert!(!hidden_permission.effective_can_transcode);

        let unconfigured_movies = repository
            .list_user_library_permissions_page(
                &user_public_id,
                AdminUserLibraryPermissionFilter {
                    library_type: Some("movies".to_owned()),
                    permission_configured: Some(false),
                    cursor: None,
                    limit: 50,
                },
            )
            .await
            .expect("unconfigured movie library permission list should execute")
            .expect("smoke user should exist");
        assert!(
            unconfigured_movies
                .records
                .iter()
                .any(
                    |permission| permission.library_id == unconfigured_library_id
                        && !permission.permission_configured
                        && !permission.can_view
                        && !permission.effective_can_view
                ),
            "unconfigured smoke movie library should be listed as unconfigured"
        );

        let invalid_cursor_page = repository
            .list_user_library_permissions_page(
                &user_public_id,
                AdminUserLibraryPermissionFilter {
                    library_type: Some("movies".to_owned()),
                    permission_configured: Some(true),
                    cursor: Some("not-a-uuid".to_owned()),
                    limit: 10,
                },
            )
            .await
            .expect("invalid library cursor should be safely handled")
            .expect("smoke user should exist");
        assert!(invalid_cursor_page.records.is_empty());
        assert!(!invalid_cursor_page.has_more);
        assert!(invalid_cursor_page.next_cursor.is_none());

        let missing_user_id = sqlx::query_scalar::<_, String>("select gen_random_uuid()::text")
            .fetch_one(&pool)
            .await
            .expect("generate missing user public id");
        let missing_user_page = repository
            .list_user_library_permissions_page(
                &missing_user_id,
                AdminUserLibraryPermissionFilter {
                    library_type: Some("movies".to_owned()),
                    permission_configured: None,
                    cursor: None,
                    limit: 10,
                },
            )
            .await
            .expect("valid missing user id should be safely handled");
        assert!(missing_user_page.is_none());

        sqlx::query("delete from users where id = $1")
            .bind(user_id)
            .execute(&pool)
            .await
            .expect("delete smoke user");
        sqlx::query(
            r#"
            delete from libraries
            where name in ($1, $2, $3, $4)
            "#,
        )
        .bind(&first_library_name)
        .bind(&second_library_name)
        .bind(&hidden_library_name)
        .bind(&unconfigured_library_name)
        .execute(&pool)
        .await
        .expect("delete smoke libraries");
        sqlx::query("delete from roles where id = $1")
            .bind(role_id)
            .execute(&pool)
            .await
            .expect("delete smoke role");
    }

    #[test]
    fn notification_admin_indexes_match_recent_audit_queries() {
        let migration = include_str!("../../migrations/0039_notification_admin_recent_indexes.sql");
        let repository = repository_source();

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
        let repository = repository_source();

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
    fn notification_target_admin_queries_have_live_schema_smoke() {
        let repository = repository_source();
        let smoke_name = [
            "notification_target_admin_queries",
            "execute_against_live_schema",
        ]
        .join("_");

        assert!(
            repository.contains(&format!("async fn {smoke_name}")),
            "notification target admin keyset queries should have an ignored live-DB smoke"
        );
    }

    // Live-DB smoke: executes notification target Admin keyset queries against
    // the migrated schema. The smoke inserts isolated targets and exercises
    // type/channel/enabled filters, cursor pagination, and invalid cursor
    // handling through the repository methods.
    //   cargo test -- --ignored notification_target_admin_queries_execute_against_live_schema
    #[tokio::test]
    #[ignore = "requires a running PostgreSQL from ./scripts/dev-deps.ps1"]
    async fn notification_target_admin_queries_execute_against_live_schema() {
        use sqlx::postgres::PgPoolOptions;
        use std::time::{SystemTime, UNIX_EPOCH};

        let url = std::env::var("DATABASE_URL")
            .unwrap_or_else(|_| "postgres://fbz:fbz@127.0.0.1:5432/fbz".to_owned());
        let pool = PgPoolOptions::new()
            .max_connections(2)
            .connect(&url)
            .await
            .expect("connect to live PostgreSQL");
        crate::db::migrate(&pool).await.expect("run migrations");

        let recent_index_def = sqlx::query_scalar::<_, String>(
            r#"
            select indexdef
            from pg_indexes
            where schemaname = 'public'
              and indexname = 'idx_notification_targets_admin_recent_keyset'
            "#,
        )
        .fetch_one(&pool)
        .await
        .expect("notification target recent keyset index should exist");
        let recent_index_def = recent_index_def.to_ascii_lowercase();
        assert!(recent_index_def.contains("target_type"));
        assert!(recent_index_def.contains("name"));
        assert!(recent_index_def.contains("id"));

        let enabled_index_def = sqlx::query_scalar::<_, String>(
            r#"
            select indexdef
            from pg_indexes
            where schemaname = 'public'
              and indexname = 'idx_notification_targets_admin_enabled_recent_keyset'
            "#,
        )
        .fetch_one(&pool)
        .await
        .expect("notification target enabled keyset index should exist");
        let enabled_index_def = enabled_index_def.to_ascii_lowercase();
        assert!(enabled_index_def.contains("is_enabled"));
        assert!(enabled_index_def.contains("target_type"));
        assert!(enabled_index_def.contains("name"));
        assert!(enabled_index_def.contains("id"));

        let channel_index_def = sqlx::query_scalar::<_, String>(
            r#"
            select indexdef
            from pg_indexes
            where schemaname = 'public'
              and indexname = 'idx_notification_targets_admin_channel_recent_keyset'
            "#,
        )
        .fetch_one(&pool)
        .await
        .expect("notification target channel keyset index should exist");
        let channel_index_def = channel_index_def.to_ascii_lowercase();
        assert!(channel_index_def.contains("channel"));
        assert!(channel_index_def.contains("target_type"));
        assert!(channel_index_def.contains("name"));
        assert!(channel_index_def.contains("id"));
        assert!(channel_index_def.contains("where (channel is not null)"));

        let channel_enabled_index_def = sqlx::query_scalar::<_, String>(
            r#"
            select indexdef
            from pg_indexes
            where schemaname = 'public'
              and indexname = 'idx_notification_targets_admin_channel_enabled_recent_keyset'
            "#,
        )
        .fetch_one(&pool)
        .await
        .expect("notification target channel/enabled keyset index should exist");
        let channel_enabled_index_def = channel_enabled_index_def.to_ascii_lowercase();
        assert!(channel_enabled_index_def.contains("channel"));
        assert!(channel_enabled_index_def.contains("is_enabled"));
        assert!(channel_enabled_index_def.contains("target_type"));
        assert!(channel_enabled_index_def.contains("name"));
        assert!(channel_enabled_index_def.contains("id"));
        assert!(channel_enabled_index_def.contains("where (channel is not null)"));

        let suffix = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system clock should be after unix epoch")
            .as_nanos();
        let channel = format!("smoke-{suffix}");
        let first_name = format!("aa notification target smoke {suffix}");
        let second_name = format!("bb notification target smoke {suffix}");
        let disabled_name = format!("cc notification target smoke {suffix}");
        let email_name = format!("dd notification target smoke {suffix}");

        let first_target_id = sqlx::query_scalar::<_, String>(
            r#"
            insert into notification_targets (
                name,
                target_type,
                channel,
                config,
                is_enabled,
                delivery_count,
                failure_count,
                last_error
            )
            values (
                $1,
                'webhook',
                $2,
                jsonb_build_object('smoke', true, 'ordinal', 1),
                true,
                3,
                0,
                null
            )
            returning public_id::text
            "#,
        )
        .bind(&first_name)
        .bind(&channel)
        .fetch_one(&pool)
        .await
        .expect("create first notification target smoke fixture");

        let second_target_id = sqlx::query_scalar::<_, String>(
            r#"
            insert into notification_targets (
                name,
                target_type,
                channel,
                config,
                is_enabled,
                delivery_count,
                failure_count,
                last_error
            )
            values (
                $1,
                'webhook',
                $2,
                jsonb_build_object('smoke', true, 'ordinal', 2),
                true,
                5,
                1,
                'previous smoke failure'
            )
            returning public_id::text
            "#,
        )
        .bind(&second_name)
        .bind(&channel)
        .fetch_one(&pool)
        .await
        .expect("create second notification target smoke fixture");

        let disabled_target_id = sqlx::query_scalar::<_, String>(
            r#"
            insert into notification_targets (
                name,
                target_type,
                channel,
                config,
                is_enabled,
                delivery_count,
                failure_count,
                last_error
            )
            values (
                $1,
                'webhook',
                $2,
                jsonb_build_object('smoke', true, 'ordinal', 3),
                false,
                0,
                2,
                'disabled smoke target'
            )
            returning public_id::text
            "#,
        )
        .bind(&disabled_name)
        .bind(&channel)
        .fetch_one(&pool)
        .await
        .expect("create disabled notification target smoke fixture");

        let email_target_id = sqlx::query_scalar::<_, String>(
            r#"
            insert into notification_targets (
                name,
                target_type,
                channel,
                config,
                is_enabled,
                delivery_count,
                failure_count,
                last_error
            )
            values (
                $1,
                'telegram',
                $2,
                jsonb_build_object('smoke', true, 'ordinal', 4),
                true,
                1,
                0,
                null
            )
            returning public_id::text
            "#,
        )
        .bind(&email_name)
        .bind(&channel)
        .fetch_one(&pool)
        .await
        .expect("create typed notification target smoke fixture");

        let repository = AdminRepository::new(pool.clone());
        let webhook_page = repository
            .list_notification_targets_page(NotificationTargetFilter {
                target_type: Some("webhook".to_owned()),
                channel: Some(channel.clone()),
                is_enabled: None,
                cursor: None,
                limit: 10,
            })
            .await
            .expect("notification target type/channel list should execute");
        assert!(
            webhook_page
                .records
                .iter()
                .any(|target| target.id == first_target_id),
            "first smoke notification target should be listed"
        );
        assert!(
            webhook_page
                .records
                .iter()
                .any(|target| target.id == second_target_id),
            "second smoke notification target should be listed"
        );
        assert!(
            webhook_page
                .records
                .iter()
                .any(|target| target.id == disabled_target_id),
            "disabled smoke notification target should be listed without enabled filter"
        );
        assert!(
            webhook_page
                .records
                .iter()
                .all(|target| target.target_type == "webhook"
                    && target.channel.as_deref() == Some(channel.as_str()))
        );

        let enabled_page = repository
            .list_notification_targets_page(NotificationTargetFilter {
                target_type: Some("webhook".to_owned()),
                channel: Some(channel.clone()),
                is_enabled: Some(true),
                cursor: None,
                limit: 10,
            })
            .await
            .expect("notification target type/channel/enabled list should execute");
        assert!(
            enabled_page
                .records
                .iter()
                .any(|target| target.id == first_target_id && target.is_enabled)
        );
        assert!(enabled_page.records.iter().all(|target| target.is_enabled
            && target.target_type == "webhook"
            && target.channel.as_deref() == Some(channel.as_str())));
        assert!(
            !enabled_page
                .records
                .iter()
                .any(|target| target.id == disabled_target_id),
            "enabled filter should exclude disabled smoke target"
        );

        let telegram_page = repository
            .list_notification_targets_page(NotificationTargetFilter {
                target_type: Some("telegram".to_owned()),
                channel: Some(channel.clone()),
                is_enabled: Some(true),
                cursor: None,
                limit: 10,
            })
            .await
            .expect("notification target alternate type list should execute");
        assert_eq!(telegram_page.records.len(), 1);
        assert_eq!(telegram_page.records[0].id, email_target_id);
        assert_eq!(telegram_page.records[0].target_type, "telegram");

        let cursor_page = repository
            .list_notification_targets_page(NotificationTargetFilter {
                target_type: Some("webhook".to_owned()),
                channel: Some(channel.clone()),
                is_enabled: Some(true),
                cursor: Some(first_target_id.clone()),
                limit: 10,
            })
            .await
            .expect("notification target cursor list should execute");
        assert!(
            cursor_page
                .records
                .iter()
                .any(|target| target.id == second_target_id),
            "later target should be returned after first target cursor"
        );
        assert!(
            cursor_page
                .records
                .iter()
                .all(|target| target.target_type == "webhook"
                    && target.channel.as_deref() == Some(channel.as_str())
                    && target.is_enabled)
        );

        let invalid_cursor_page = repository
            .list_notification_targets_page(NotificationTargetFilter {
                target_type: Some("webhook".to_owned()),
                channel: Some(channel.clone()),
                is_enabled: Some(true),
                cursor: Some("not-a-uuid".to_owned()),
                limit: 10,
            })
            .await
            .expect("invalid notification target cursor should be safely handled");
        assert!(invalid_cursor_page.records.is_empty());
        assert!(!invalid_cursor_page.has_more);
        assert!(invalid_cursor_page.next_cursor.is_none());

        sqlx::query(
            r#"
            delete from notification_targets
            where channel = $1
              and name in ($2, $3, $4, $5)
            "#,
        )
        .bind(&channel)
        .bind(&first_name)
        .bind(&second_name)
        .bind(&disabled_name)
        .bind(&email_name)
        .execute(&pool)
        .await
        .expect("delete smoke notification targets");
    }

    #[test]
    fn notification_admin_keyset_filter_indexes_match_query_shape() {
        let migration =
            include_str!("../../migrations/0047_notification_admin_keyset_filter_indexes.sql");
        let repository = repository_source();

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
    fn notification_admin_audit_queries_have_live_schema_smoke() {
        let repository = repository_source();
        let smoke_name = [
            "notification_admin_audit_queries",
            "execute_against_live_schema",
        ]
        .join("_");

        assert!(
            repository.contains(&format!("async fn {smoke_name}")),
            "notification request and delivery-attempt admin queries should have an ignored live-DB smoke"
        );
    }

    // Live-DB smoke: executes the Admin notification request and delivery
    // attempt keyset queries against the migrated schema. The smoke inserts
    // an isolated plugin/request/attempt set and exercises filters, cursors,
    // and invalid public-id handling through the repository methods.
    //   cargo test -- --ignored notification_admin_audit_queries_execute_against_live_schema
    #[tokio::test]
    #[ignore = "requires a running PostgreSQL from ./scripts/dev-deps.ps1"]
    async fn notification_admin_audit_queries_execute_against_live_schema() {
        use sqlx::postgres::PgPoolOptions;
        use std::time::{SystemTime, UNIX_EPOCH};

        let url = std::env::var("DATABASE_URL")
            .unwrap_or_else(|_| "postgres://fbz:fbz@127.0.0.1:5432/fbz".to_owned());
        let pool = PgPoolOptions::new()
            .max_connections(2)
            .connect(&url)
            .await
            .expect("connect to live PostgreSQL");
        crate::db::migrate(&pool).await.expect("run migrations");

        let recent_index_def = sqlx::query_scalar::<_, String>(
            r#"
            select indexdef
            from pg_indexes
            where schemaname = 'public'
              and indexname = 'idx_plugin_notification_requests_recent'
            "#,
        )
        .fetch_one(&pool)
        .await
        .expect("notification request recent index should exist");
        let recent_index_def = recent_index_def.to_ascii_lowercase();
        assert!(recent_index_def.contains("created_at desc"));
        assert!(recent_index_def.contains("id desc"));

        let attempt_index_def = sqlx::query_scalar::<_, String>(
            r#"
            select indexdef
            from pg_indexes
            where schemaname = 'public'
              and indexname = 'idx_notification_delivery_attempts_request_status_recent_keyset'
            "#,
        )
        .fetch_one(&pool)
        .await
        .expect("notification attempt request/status keyset index should exist");
        let attempt_index_def = attempt_index_def.to_ascii_lowercase();
        assert!(attempt_index_def.contains("notification_request_id"));
        assert!(attempt_index_def.contains("status"));
        assert!(attempt_index_def.contains("created_at desc"));
        assert!(attempt_index_def.contains("id desc"));

        let suffix = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system clock should be after unix epoch")
            .as_nanos();
        let plugin_id = format!("dev.fbz.notification.smoke.{suffix}");
        let package_version = format!("0.0.{suffix}");
        let package_path = format!("H:/fbz-smoke/plugins/{suffix}.zip");
        let notification_channel = format!("smoke-{suffix}");

        let package_id = sqlx::query_scalar::<_, i64>(
            r#"
            insert into plugin_packages (
                plugin_id,
                package_version,
                api_version,
                runtime,
                name,
                entrypoint,
                package_path,
                manifest,
                manifest_hash,
                permission_fingerprint,
                checksum_sha256,
                package_status
            )
            values (
                $1,
                $2,
                '1',
                'http',
                'Notification audit smoke',
                'http://127.0.0.1:19999/fbz-plugin',
                $3,
                jsonb_build_object('id', $1::text, 'version', $2::text),
                gen_random_bytes(32),
                gen_random_bytes(32),
                gen_random_bytes(32),
                'approved'
            )
            returning id
            "#,
        )
        .bind(&plugin_id)
        .bind(&package_version)
        .bind(&package_path)
        .fetch_one(&pool)
        .await
        .expect("create notification audit smoke plugin package");

        sqlx::query(
            r#"
            insert into plugin_installations (
                plugin_id,
                active_package_id,
                enabled,
                approval_status,
                permission_fingerprint,
                approved_at
            )
            values ($1, $2, true, 'approved', gen_random_bytes(32), now())
            "#,
        )
        .bind(&plugin_id)
        .bind(package_id)
        .execute(&pool)
        .await
        .expect("create notification audit smoke plugin installation");

        let first_request_id = sqlx::query_scalar::<_, String>(
            r#"
            insert into plugin_notification_requests (
                plugin_id,
                package_id,
                title,
                message,
                level,
                channel,
                metadata,
                status,
                created_at,
                updated_at
            )
            values (
                $1,
                $2,
                'Notification audit smoke first',
                'first smoke request',
                'warning',
                $3,
                jsonb_build_object('smoke', true, 'ordinal', 1),
                'failed',
                now() - interval '2 minutes',
                now() - interval '2 minutes'
            )
            returning public_id::text
            "#,
        )
        .bind(&plugin_id)
        .bind(&package_version)
        .bind(&notification_channel)
        .fetch_one(&pool)
        .await
        .expect("create first notification request");

        let second_request_id = sqlx::query_scalar::<_, String>(
            r#"
            insert into plugin_notification_requests (
                plugin_id,
                package_id,
                title,
                message,
                level,
                channel,
                metadata,
                status,
                created_at,
                updated_at
            )
            values (
                $1,
                $2,
                'Notification audit smoke second',
                'second smoke request',
                'info',
                $3,
                jsonb_build_object('smoke', true, 'ordinal', 2),
                'delivered',
                now() - interval '1 minute',
                now() - interval '1 minute'
            )
            returning public_id::text
            "#,
        )
        .bind(&plugin_id)
        .bind(&package_version)
        .bind(&notification_channel)
        .fetch_one(&pool)
        .await
        .expect("create second notification request");

        let request_row = sqlx::query(
            r#"
            select id, public_id::text as public_id
            from plugin_notification_requests
            where public_id = $1::uuid
            "#,
        )
        .bind(&second_request_id)
        .fetch_one(&pool)
        .await
        .expect("read second notification request");
        let second_request_internal_id = request_row
            .try_get::<i64, _>("id")
            .expect("request id should be returned");

        let first_attempt_id = sqlx::query_scalar::<_, String>(
            r#"
            insert into notification_delivery_attempts (
                notification_request_id,
                target_public_id,
                target_type,
                target_name,
                attempt,
                status,
                response_status,
                duration_ms,
                created_at,
                finished_at
            )
            values (
                $1,
                gen_random_uuid(),
                'webhook',
                'Notification smoke target A',
                1,
                'failed',
                500,
                25,
                now() - interval '30 seconds',
                now() - interval '29 seconds'
            )
            returning public_id::text
            "#,
        )
        .bind(second_request_internal_id)
        .fetch_one(&pool)
        .await
        .expect("create failed notification attempt");

        let second_attempt_id = sqlx::query_scalar::<_, String>(
            r#"
            insert into notification_delivery_attempts (
                notification_request_id,
                target_public_id,
                target_type,
                target_name,
                attempt,
                status,
                response_status,
                duration_ms,
                created_at,
                finished_at
            )
            values (
                $1,
                gen_random_uuid(),
                'webhook',
                'Notification smoke target B',
                2,
                'succeeded',
                200,
                10,
                now() - interval '10 seconds',
                now() - interval '9 seconds'
            )
            returning public_id::text
            "#,
        )
        .bind(second_request_internal_id)
        .fetch_one(&pool)
        .await
        .expect("create succeeded notification attempt");

        let repository = AdminRepository::new(pool.clone());
        let request_page = repository
            .list_notification_requests_page(NotificationRequestFilter {
                status: None,
                channel: Some(notification_channel.clone()),
                cursor: None,
                limit: 10,
            })
            .await
            .expect("notification request list should execute");
        assert!(
            request_page
                .records
                .iter()
                .any(|request| request.id == first_request_id),
            "first smoke notification request should be listed"
        );
        assert!(
            request_page
                .records
                .iter()
                .any(|request| request.id == second_request_id),
            "second smoke notification request should be listed"
        );

        let delivered_page = repository
            .list_notification_requests_page(NotificationRequestFilter {
                status: Some("delivered".to_owned()),
                channel: Some(notification_channel.clone()),
                cursor: None,
                limit: 10,
            })
            .await
            .expect("filtered notification request list should execute");
        assert!(
            delivered_page
                .records
                .iter()
                .any(|request| request.id == second_request_id && request.status == "delivered")
        );
        assert!(
            delivered_page
                .records
                .iter()
                .all(|request| request.status == "delivered")
        );

        let cursor_page = repository
            .list_notification_requests_page(NotificationRequestFilter {
                status: None,
                channel: Some(notification_channel.clone()),
                cursor: Some(second_request_id.clone()),
                limit: 10,
            })
            .await
            .expect("cursor notification request list should execute");
        assert!(
            cursor_page
                .records
                .iter()
                .any(|request| request.id == first_request_id),
            "older request should be returned after newest request cursor"
        );

        let attempts_page = repository
            .list_notification_delivery_attempts_page(
                &second_request_id,
                NotificationDeliveryAttemptFilter {
                    status: None,
                    cursor: None,
                    limit: 10,
                },
            )
            .await
            .expect("notification delivery attempt list should execute")
            .expect("second request should exist");
        assert!(
            attempts_page
                .records
                .iter()
                .any(|attempt| attempt.id == first_attempt_id),
            "first smoke delivery attempt should be listed"
        );
        assert!(
            attempts_page
                .records
                .iter()
                .any(|attempt| attempt.id == second_attempt_id),
            "second smoke delivery attempt should be listed"
        );

        let failed_attempts_page = repository
            .list_notification_delivery_attempts_page(
                &second_request_id,
                NotificationDeliveryAttemptFilter {
                    status: Some("failed".to_owned()),
                    cursor: None,
                    limit: 10,
                },
            )
            .await
            .expect("filtered notification delivery attempt list should execute")
            .expect("second request should exist");
        assert_eq!(failed_attempts_page.records.len(), 1);
        assert_eq!(failed_attempts_page.records[0].id, first_attempt_id);
        assert_eq!(failed_attempts_page.records[0].status, "failed");

        let attempt_cursor_page = repository
            .list_notification_delivery_attempts_page(
                &second_request_id,
                NotificationDeliveryAttemptFilter {
                    status: None,
                    cursor: Some(second_attempt_id),
                    limit: 10,
                },
            )
            .await
            .expect("cursor notification delivery attempt list should execute")
            .expect("second request should exist");
        assert!(
            attempt_cursor_page
                .records
                .iter()
                .any(|attempt| attempt.id == first_attempt_id),
            "older delivery attempt should be returned after newest attempt cursor"
        );

        let missing_attempts = repository
            .list_notification_delivery_attempts_page(
                "not-a-uuid",
                NotificationDeliveryAttemptFilter {
                    status: None,
                    cursor: None,
                    limit: 10,
                },
            )
            .await
            .expect("invalid notification request id should be safely handled");
        assert!(missing_attempts.is_none());

        sqlx::query("delete from plugin_installations where plugin_id = $1")
            .bind(&plugin_id)
            .execute(&pool)
            .await
            .expect("delete smoke plugin installation");
        sqlx::query("delete from plugin_packages where plugin_id = $1")
            .bind(&plugin_id)
            .execute(&pool)
            .await
            .expect("delete smoke plugin package");
    }

    #[test]
    fn scheduled_task_admin_keyset_indexes_match_query_shape() {
        let migration =
            include_str!("../../migrations/0058_scheduled_task_admin_keyset_indexes.sql");
        let repository = repository_source();

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
    fn scheduled_task_admin_queries_have_live_schema_smoke() {
        let repository = repository_source();
        let smoke_name = [
            "scheduled_task_admin_queries",
            "execute_against_live_schema",
        ]
        .join("_");

        assert!(
            repository.contains(&format!("async fn {smoke_name}")),
            "scheduled-task admin keyset queries should have an ignored live-DB smoke"
        );
    }

    // Live-DB smoke: executes the Admin scheduled-task keyset list query
    // against the migrated schema. The smoke inserts isolated tasks and
    // exercises task_type, owner_type, enabled, cursor, and invalid cursor
    // behavior through the repository method.
    //   cargo test -- --ignored scheduled_task_admin_queries_execute_against_live_schema
    #[tokio::test]
    #[ignore = "requires a running PostgreSQL from ./scripts/dev-deps.ps1"]
    async fn scheduled_task_admin_queries_execute_against_live_schema() {
        use sqlx::postgres::PgPoolOptions;
        use std::time::{SystemTime, UNIX_EPOCH};

        let url = std::env::var("DATABASE_URL")
            .unwrap_or_else(|_| "postgres://fbz:fbz@127.0.0.1:5432/fbz".to_owned());
        let pool = PgPoolOptions::new()
            .max_connections(2)
            .connect(&url)
            .await
            .expect("connect to live PostgreSQL");
        crate::db::migrate(&pool).await.expect("run migrations");

        let admin_index_def = sqlx::query_scalar::<_, String>(
            r#"
            select indexdef
            from pg_indexes
            where schemaname = 'public'
              and indexname = 'idx_scheduled_tasks_admin_keyset'
            "#,
        )
        .fetch_one(&pool)
        .await
        .expect("scheduled task admin keyset index should exist");
        let admin_index_def = admin_index_def.to_ascii_lowercase();
        assert!(admin_index_def.contains("enabled desc"));
        assert!(admin_index_def.contains("next_run_at"));
        assert!(admin_index_def.contains("updated_at desc"));
        assert!(admin_index_def.contains("id desc"));

        let task_type_index_def = sqlx::query_scalar::<_, String>(
            r#"
            select indexdef
            from pg_indexes
            where schemaname = 'public'
              and indexname = 'idx_scheduled_tasks_task_type_admin_keyset'
            "#,
        )
        .fetch_one(&pool)
        .await
        .expect("scheduled task type keyset index should exist");
        let task_type_index_def = task_type_index_def.to_ascii_lowercase();
        assert!(task_type_index_def.contains("task_type"));
        assert!(task_type_index_def.contains("enabled desc"));
        assert!(task_type_index_def.contains("next_run_at"));
        assert!(task_type_index_def.contains("updated_at desc"));
        assert!(task_type_index_def.contains("id desc"));

        let owner_type_index_def = sqlx::query_scalar::<_, String>(
            r#"
            select indexdef
            from pg_indexes
            where schemaname = 'public'
              and indexname = 'idx_scheduled_tasks_owner_type_admin_keyset'
            "#,
        )
        .fetch_one(&pool)
        .await
        .expect("scheduled task owner-type keyset index should exist");
        let owner_type_index_def = owner_type_index_def.to_ascii_lowercase();
        assert!(owner_type_index_def.contains("owner_type"));
        assert!(owner_type_index_def.contains("enabled desc"));
        assert!(owner_type_index_def.contains("next_run_at"));
        assert!(owner_type_index_def.contains("updated_at desc"));
        assert!(owner_type_index_def.contains("id desc"));

        let suffix = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system clock should be after unix epoch")
            .as_nanos();
        let first_task_key = format!("core.admin-list-smoke-a.{suffix}");
        let second_task_key = format!("core.admin-list-smoke-b.{suffix}");
        let disabled_task_key = format!("core.admin-list-smoke-c.{suffix}");
        let plugin_task_key = format!("plugin.admin-list-smoke-d.{suffix}");
        let plugin_owner_id = format!("dev.fbz.admin-list-smoke.{suffix}");

        let first_task_id = sqlx::query_scalar::<_, String>(
            r#"
            insert into scheduled_tasks (
                task_key,
                task_type,
                owner_type,
                enabled,
                schedule_kind,
                schedule_value,
                next_run_at,
                timeout_seconds,
                max_concurrency,
                updated_at
            )
            values (
                $1,
                'admin.list.smoke',
                'core',
                true,
                'interval',
                'PT10M',
                now() + interval '10 minutes',
                300,
                2,
                now() - interval '1 minute'
            )
            returning public_id::text
            "#,
        )
        .bind(&first_task_key)
        .fetch_one(&pool)
        .await
        .expect("create first scheduled task smoke fixture");

        let second_task_id = sqlx::query_scalar::<_, String>(
            r#"
            insert into scheduled_tasks (
                task_key,
                task_type,
                owner_type,
                enabled,
                schedule_kind,
                schedule_value,
                next_run_at,
                timeout_seconds,
                max_concurrency,
                updated_at
            )
            values (
                $1,
                'admin.list.smoke',
                'core',
                true,
                'interval',
                'PT10M',
                now() + interval '20 minutes',
                300,
                1,
                now() - interval '2 minutes'
            )
            returning public_id::text
            "#,
        )
        .bind(&second_task_key)
        .fetch_one(&pool)
        .await
        .expect("create second scheduled task smoke fixture");

        let disabled_task_id = sqlx::query_scalar::<_, String>(
            r#"
            insert into scheduled_tasks (
                task_key,
                task_type,
                owner_type,
                enabled,
                schedule_kind,
                schedule_value,
                next_run_at,
                timeout_seconds,
                max_concurrency,
                updated_at
            )
            values (
                $1,
                'admin.list.smoke',
                'core',
                false,
                'interval',
                'PT10M',
                now() + interval '30 minutes',
                300,
                1,
                now() - interval '3 minutes'
            )
            returning public_id::text
            "#,
        )
        .bind(&disabled_task_key)
        .fetch_one(&pool)
        .await
        .expect("create disabled scheduled task smoke fixture");

        let plugin_task_id = sqlx::query_scalar::<_, String>(
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
                max_concurrency,
                updated_at
            )
            values (
                $1,
                'plugin.admin.list.smoke',
                'plugin',
                $2,
                true,
                'interval',
                'PT10M',
                now() + interval '5 minutes',
                300,
                1,
                now() - interval '4 minutes'
            )
            returning public_id::text
            "#,
        )
        .bind(&plugin_task_key)
        .bind(&plugin_owner_id)
        .fetch_one(&pool)
        .await
        .expect("create plugin scheduled task smoke fixture");

        let repository = AdminRepository::new(pool.clone());
        let core_enabled_page = repository
            .list_scheduled_tasks_page(ScheduledTaskFilter {
                task_type: Some("admin.list.smoke".to_owned()),
                owner_type: Some("core".to_owned()),
                enabled: Some(true),
                cursor: None,
                limit: 10,
            })
            .await
            .expect("scheduled task type/owner/enabled list should execute");
        assert!(
            core_enabled_page
                .records
                .iter()
                .any(|task| task.id == first_task_id && task.enabled)
        );
        assert!(
            core_enabled_page
                .records
                .iter()
                .any(|task| task.id == second_task_id && task.enabled)
        );
        assert!(
            core_enabled_page.records.iter().all(|task| {
                task.task_type == "admin.list.smoke" && task.owner_type == "core" && task.enabled
            }),
            "type/owner/enabled filter should only return enabled core smoke tasks"
        );
        assert!(
            !core_enabled_page
                .records
                .iter()
                .any(|task| task.id == disabled_task_id),
            "enabled filter should exclude disabled smoke task"
        );

        let disabled_page = repository
            .list_scheduled_tasks_page(ScheduledTaskFilter {
                task_type: Some("admin.list.smoke".to_owned()),
                owner_type: Some("core".to_owned()),
                enabled: Some(false),
                cursor: None,
                limit: 10,
            })
            .await
            .expect("disabled scheduled task list should execute");
        assert_eq!(disabled_page.records.len(), 1);
        assert_eq!(disabled_page.records[0].id, disabled_task_id);
        assert!(!disabled_page.records[0].enabled);

        let plugin_page = repository
            .list_scheduled_tasks_page(ScheduledTaskFilter {
                task_type: Some("plugin.admin.list.smoke".to_owned()),
                owner_type: Some("plugin".to_owned()),
                enabled: Some(true),
                cursor: None,
                limit: 10,
            })
            .await
            .expect("plugin scheduled task list should execute");
        assert_eq!(plugin_page.records.len(), 1);
        assert_eq!(plugin_page.records[0].id, plugin_task_id);
        assert_eq!(
            plugin_page.records[0].owner_id.as_deref(),
            Some(plugin_owner_id.as_str())
        );

        let cursor_page = repository
            .list_scheduled_tasks_page(ScheduledTaskFilter {
                task_type: Some("admin.list.smoke".to_owned()),
                owner_type: Some("core".to_owned()),
                enabled: Some(true),
                cursor: Some(first_task_id.clone()),
                limit: 10,
            })
            .await
            .expect("scheduled task cursor list should execute");
        assert!(
            cursor_page
                .records
                .iter()
                .any(|task| task.id == second_task_id),
            "later enabled core task should be returned after first task cursor"
        );
        assert!(
            !cursor_page
                .records
                .iter()
                .any(|task| task.id == first_task_id),
            "cursor page should not include the cursor scheduled task"
        );

        let invalid_cursor_page = repository
            .list_scheduled_tasks_page(ScheduledTaskFilter {
                task_type: Some("admin.list.smoke".to_owned()),
                owner_type: Some("core".to_owned()),
                enabled: Some(true),
                cursor: Some("not-a-uuid".to_owned()),
                limit: 10,
            })
            .await
            .expect("invalid scheduled task cursor should be safely handled");
        assert!(invalid_cursor_page.records.is_empty());
        assert!(!invalid_cursor_page.has_more);
        assert!(invalid_cursor_page.next_cursor.is_none());

        sqlx::query("delete from scheduled_tasks where task_key = any($1)")
            .bind(
                &[
                    first_task_key,
                    second_task_key,
                    disabled_task_key,
                    plugin_task_key,
                ][..],
            )
            .execute(&pool)
            .await
            .expect("delete scheduled task smoke fixtures");
    }

    #[test]
    fn scheduled_task_admin_active_run_counts_are_bounded() {
        let repository = repository_source();
        let production_source = repository
            .split("#[cfg(test)]")
            .next()
            .expect("repository source should include production section");

        assert!(production_source.contains("SCHEDULED_TASK_ACTIVE_RUN_COUNT_SQL"));
        assert!(production_source.contains("active_run_capacity_probe"));
        assert!(production_source.contains("limit tasks.max_concurrency"));

        let normalized = production_source
            .split_whitespace()
            .collect::<Vec<_>>()
            .join(" ");
        assert!(
            !normalized.contains(
                "select count(*)::bigint from scheduled_task_runs runs where runs.task_id = tasks.id"
            ),
            "scheduled task list should not exact-count every active run per task"
        );
        assert!(
            !normalized.contains(
                "select count(*)::bigint from scheduled_task_runs runs where runs.task_id = scheduled_tasks.id"
            ),
            "scheduled task detail should not exact-count every active run for the task"
        );
    }

    // Live-DB smoke: executes the Admin scheduled-task list/detail queries
    // against the real migrated schema. This covers the QueryBuilder wiring
    // for the shared bounded active-run count snippet.
    //   cargo test -- --ignored scheduled_task_admin_active_run_counts_execute_against_live_schema
    #[tokio::test]
    #[ignore = "requires a running PostgreSQL from ./scripts/dev-deps.ps1"]
    async fn scheduled_task_admin_active_run_counts_execute_against_live_schema() {
        use sqlx::postgres::PgPoolOptions;

        let url = std::env::var("DATABASE_URL")
            .unwrap_or_else(|_| "postgres://fbz:fbz@127.0.0.1:5432/fbz".to_owned());
        let pool = PgPoolOptions::new()
            .max_connections(2)
            .connect(&url)
            .await
            .expect("connect to live PostgreSQL");
        crate::db::migrate(&pool).await.expect("run migrations");

        let repository = AdminRepository::new(pool);
        let page = repository
            .list_scheduled_tasks_page(ScheduledTaskFilter {
                limit: 2,
                ..ScheduledTaskFilter::default()
            })
            .await
            .expect("scheduled task list should execute against live schema");

        if let Some(task) = page.records.first() {
            let detail = repository
                .find_scheduled_task(&task.id)
                .await
                .expect("scheduled task detail by id should execute against live schema");
            assert!(
                detail.is_some(),
                "listed scheduled task should be readable by public id"
            );
        }
    }

    #[test]
    fn plugin_host_api_call_keyset_indexes_match_admin_query_shape() {
        let migration =
            include_str!("../../migrations/0045_plugin_host_api_call_keyset_indexes.sql");
        let combined_filter_migration =
            include_str!("../../migrations/0060_plugin_host_api_call_combined_filter_indexes.sql");
        let repository = repository_source();

        assert!(migration.contains("idx_plugin_host_api_calls_recent_keyset"));
        assert!(migration.contains("finished_at desc, id desc"));
        assert!(migration.contains("idx_plugin_host_api_calls_plugin_finished_keyset"));
        assert!(migration.contains("plugin_id, finished_at desc, id desc"));
        assert!(migration.contains("idx_plugin_host_api_calls_execution_finished_keyset"));
        assert!(migration.contains("execution_run_id, finished_at desc, id desc"));
        assert!(migration.contains("idx_plugin_host_api_calls_status_finished_keyset"));
        assert!(migration.contains("status_code, finished_at desc, id desc"));
        assert!(
            combined_filter_migration
                .contains("idx_plugin_host_api_calls_plugin_status_finished_keyset")
        );
        assert!(
            combined_filter_migration.contains("plugin_id, status_code, finished_at desc, id desc")
        );
        assert!(
            combined_filter_migration
                .contains("idx_plugin_host_api_calls_execution_status_finished_keyset")
        );
        assert!(
            combined_filter_migration
                .contains("execution_run_id, status_code, finished_at desc, id desc")
        );

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
        assert!(host_api_call_query.contains("calls.plugin_id ="));
        assert!(host_api_call_query.contains("calls.execution_run_id ="));
        assert!(host_api_call_query.contains("calls.status_code ="));
        assert!(host_api_call_query.contains("order by calls.finished_at desc, calls.id desc"));
        assert!(!host_api_call_query.contains("offset "));
        assert!(repository.contains("pub async fn list_plugin_host_api_calls_for_run_page"));
        assert!(repository.contains("list_plugin_host_api_calls(PluginHostApiCallFilter"));
    }

    #[test]
    fn plugin_host_api_admin_queries_have_live_schema_smoke() {
        let repository = repository_source();
        let smoke_name = [
            "plugin_host_api_admin_queries",
            "execute_against_live_schema",
        ]
        .join("_");

        assert!(
            repository.contains(&format!("async fn {smoke_name}")),
            "plugin Host API admin queries should have an ignored live-DB smoke"
        );
    }

    // Live-DB smoke: executes the Admin Host API call audit keyset queries
    // against the migrated schema. The smoke inserts an isolated approved
    // plugin, execution run, host token, and two Host API call rows, then
    // exercises plugin/status filters, run-scoped filters, cursors, and
    // invalid execution-run ids through the repository methods.
    //   cargo test -- --ignored plugin_host_api_admin_queries_execute_against_live_schema
    #[tokio::test]
    #[ignore = "requires a running PostgreSQL from ./scripts/dev-deps.ps1"]
    async fn plugin_host_api_admin_queries_execute_against_live_schema() {
        use sqlx::postgres::PgPoolOptions;
        use std::time::{SystemTime, UNIX_EPOCH};

        let url = std::env::var("DATABASE_URL")
            .unwrap_or_else(|_| "postgres://fbz:fbz@127.0.0.1:5432/fbz".to_owned());
        let pool = PgPoolOptions::new()
            .max_connections(2)
            .connect(&url)
            .await
            .expect("connect to live PostgreSQL");
        crate::db::migrate(&pool).await.expect("run migrations");

        let recent_index_def = sqlx::query_scalar::<_, String>(
            r#"
            select indexdef
            from pg_indexes
            where schemaname = 'public'
              and indexname = 'idx_plugin_host_api_calls_recent_keyset'
            "#,
        )
        .fetch_one(&pool)
        .await
        .expect("host API call recent keyset index should exist");
        let recent_index_def = recent_index_def.to_ascii_lowercase();
        assert!(recent_index_def.contains("finished_at desc"));
        assert!(recent_index_def.contains("id desc"));

        let plugin_status_index_def = sqlx::query_scalar::<_, String>(
            r#"
            select indexdef
            from pg_indexes
            where schemaname = 'public'
              and indexname = 'idx_plugin_host_api_calls_plugin_status_finished_keyset'
            "#,
        )
        .fetch_one(&pool)
        .await
        .expect("host API call plugin/status keyset index should exist");
        let plugin_status_index_def = plugin_status_index_def.to_ascii_lowercase();
        assert!(plugin_status_index_def.contains("plugin_id"));
        assert!(plugin_status_index_def.contains("status_code"));
        assert!(plugin_status_index_def.contains("finished_at desc"));
        assert!(plugin_status_index_def.contains("id desc"));

        let run_status_index_def = sqlx::query_scalar::<_, String>(
            r#"
            select indexdef
            from pg_indexes
            where schemaname = 'public'
              and indexname = 'idx_plugin_host_api_calls_execution_status_finished_keyset'
            "#,
        )
        .fetch_one(&pool)
        .await
        .expect("host API call execution/status keyset index should exist");
        let run_status_index_def = run_status_index_def.to_ascii_lowercase();
        assert!(run_status_index_def.contains("execution_run_id"));
        assert!(run_status_index_def.contains("status_code"));
        assert!(run_status_index_def.contains("finished_at desc"));
        assert!(run_status_index_def.contains("id desc"));
        assert!(run_status_index_def.contains("execution_run_id is not null"));

        let suffix = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system clock should be after unix epoch")
            .as_nanos();
        let plugin_id = format!("dev.fbz.host-api.smoke.{suffix}");
        let package_version = format!("0.0.{suffix}");
        let package_path = format!("H:/fbz-smoke/plugins/{suffix}.zip");

        let package_id = sqlx::query_scalar::<_, i64>(
            r#"
            insert into plugin_packages (
                plugin_id,
                package_version,
                api_version,
                runtime,
                name,
                entrypoint,
                package_path,
                manifest,
                manifest_hash,
                permission_fingerprint,
                checksum_sha256,
                package_status
            )
            values (
                $1,
                $2,
                '1',
                'http',
                'Host API audit smoke',
                'http://127.0.0.1:19997/fbz-plugin',
                $3,
                jsonb_build_object('id', $1::text, 'version', $2::text),
                gen_random_bytes(32),
                gen_random_bytes(32),
                gen_random_bytes(32),
                'approved'
            )
            returning id
            "#,
        )
        .bind(&plugin_id)
        .bind(&package_version)
        .bind(&package_path)
        .fetch_one(&pool)
        .await
        .expect("create host API audit smoke package");

        sqlx::query(
            r#"
            insert into plugin_installations (
                plugin_id,
                active_package_id,
                enabled,
                approval_status,
                permission_fingerprint,
                approved_at
            )
            values ($1, $2, true, 'approved', gen_random_bytes(32), now())
            "#,
        )
        .bind(&plugin_id)
        .bind(package_id)
        .execute(&pool)
        .await
        .expect("create host API audit smoke installation");

        let dispatch_row = sqlx::query(
            r#"
            insert into event_outbox (
                event_type,
                aggregate_type,
                aggregate_id,
                payload,
                status,
                attempts,
                max_attempts,
                created_at,
                available_at
            )
            values (
                $1,
                'plugin',
                $2,
                jsonb_build_object(
                    'pluginId', $2::text,
                    'packageId', $3::text,
                    'hookId', null,
                    'handler', 'hooks.onHostApiSmoke',
                    'hookEvent', 'library.scan.completed'
                ),
                'delivered',
                1,
                5,
                now() - interval '2 minutes',
                now() - interval '2 minutes'
            )
            returning id, public_id::text as public_id
            "#,
        )
        .bind(PLUGIN_HOOK_DISPATCH_EVENT)
        .bind(&plugin_id)
        .bind(&package_version)
        .fetch_one(&pool)
        .await
        .expect("create host API audit smoke dispatch row");
        let dispatch_internal_id = dispatch_row
            .try_get::<i64, _>("id")
            .expect("dispatch internal id should be returned");
        let dispatch_id = dispatch_row
            .try_get::<String, _>("public_id")
            .expect("dispatch public id should be returned");

        let run_row = sqlx::query(
            r#"
            insert into plugin_execution_runs (
                outbox_event_id,
                outbox_event_public_id,
                attempt,
                plugin_id,
                package_id,
                handler,
                event_key,
                runtime,
                entrypoint,
                status,
                request_payload,
                response_status,
                response_body,
                started_at,
                finished_at,
                duration_ms
            )
            values (
                $1,
                $2,
                1,
                $3,
                $4,
                'hooks.onHostApiSmoke',
                'library.scan.completed',
                'http',
                'http://127.0.0.1:19997/fbz-plugin',
                'succeeded',
                jsonb_build_object('smoke', true),
                200,
                'ok',
                now() - interval '90 seconds',
                now() - interval '89 seconds',
                10
            )
            returning id, public_id::text as public_id
            "#,
        )
        .bind(dispatch_internal_id)
        .bind(&dispatch_id)
        .bind(&plugin_id)
        .bind(&package_version)
        .fetch_one(&pool)
        .await
        .expect("create host API audit smoke execution run");
        let run_internal_id = run_row
            .try_get::<i64, _>("id")
            .expect("execution run internal id should be returned");
        let run_id = run_row
            .try_get::<String, _>("public_id")
            .expect("execution run public id should be returned");

        let token_row = sqlx::query(
            r#"
            insert into plugin_host_tokens (
                token_hash,
                token_prefix,
                plugin_id,
                package_id,
                execution_run_id,
                permission_snapshot,
                expires_at
            )
            values (
                gen_random_bytes(32),
                $1,
                $2,
                $3,
                $4,
                jsonb_build_array(jsonb_build_object('key', 'library.read')),
                now() + interval '1 hour'
            )
            returning id, public_id::text as public_id
            "#,
        )
        .bind(format!("smoke-{suffix}"))
        .bind(&plugin_id)
        .bind(&package_version)
        .bind(run_internal_id)
        .fetch_one(&pool)
        .await
        .expect("create host API audit smoke host token");
        let token_internal_id = token_row
            .try_get::<i64, _>("id")
            .expect("host token internal id should be returned");
        let token_id = token_row
            .try_get::<String, _>("public_id")
            .expect("host token public id should be returned");

        let older_call_id = sqlx::query_scalar::<_, String>(
            r#"
            insert into plugin_host_api_calls (
                plugin_id,
                package_id,
                host_token_id,
                execution_run_id,
                method,
                path,
                required_permission,
                status_code,
                error_code,
                error_message,
                started_at,
                finished_at,
                duration_ms
            )
            values (
                $1,
                $2,
                $3,
                $4,
                'GET',
                '/library/items',
                'library.read',
                500,
                'host_api_smoke_failed',
                'host API smoke failure',
                now() - interval '40 seconds',
                now() - interval '39 seconds',
                31
            )
            returning public_id::text
            "#,
        )
        .bind(&plugin_id)
        .bind(&package_version)
        .bind(token_internal_id)
        .bind(run_internal_id)
        .fetch_one(&pool)
        .await
        .expect("create older host API audit smoke call");

        let newer_call_id = sqlx::query_scalar::<_, String>(
            r#"
            insert into plugin_host_api_calls (
                plugin_id,
                package_id,
                host_token_id,
                execution_run_id,
                method,
                path,
                required_permission,
                status_code,
                started_at,
                finished_at,
                duration_ms
            )
            values (
                $1,
                $2,
                $3,
                $4,
                'POST',
                '/notifications/send',
                'notification.send',
                202,
                now() - interval '20 seconds',
                now() - interval '19 seconds',
                12
            )
            returning public_id::text
            "#,
        )
        .bind(&plugin_id)
        .bind(&package_version)
        .bind(token_internal_id)
        .bind(run_internal_id)
        .fetch_one(&pool)
        .await
        .expect("create newer host API audit smoke call");

        let repository = AdminRepository::new(pool.clone());
        let plugin_page = repository
            .list_plugin_host_api_calls_page(PluginHostApiCallFilter {
                plugin_id: Some(plugin_id.clone()),
                execution_run_id: None,
                status_code: None,
                cursor: None,
                limit: 25,
            })
            .await
            .expect("plugin Host API call list should execute");
        assert!(
            plugin_page.records.iter().any(|call| {
                call.id == older_call_id
                    && call.execution_run_id.as_deref() == Some(run_id.as_str())
                    && call.host_token_id.as_deref() == Some(token_id.as_str())
            }),
            "older smoke Host API call should be listed with joined run/token ids"
        );
        assert!(
            plugin_page
                .records
                .iter()
                .any(|call| call.id == newer_call_id),
            "newer smoke Host API call should be listed"
        );

        let failed_plugin_page = repository
            .list_plugin_host_api_calls_page(PluginHostApiCallFilter {
                plugin_id: Some(plugin_id.clone()),
                execution_run_id: None,
                status_code: Some(500),
                cursor: None,
                limit: 25,
            })
            .await
            .expect("plugin/status Host API call list should execute");
        assert_eq!(failed_plugin_page.records.len(), 1);
        assert_eq!(failed_plugin_page.records[0].id, older_call_id);
        assert_eq!(failed_plugin_page.records[0].status_code, 500);

        let cursor_plugin_page = repository
            .list_plugin_host_api_calls_page(PluginHostApiCallFilter {
                plugin_id: Some(plugin_id.clone()),
                execution_run_id: None,
                status_code: None,
                cursor: Some(newer_call_id.clone()),
                limit: 25,
            })
            .await
            .expect("cursor Host API call list should execute");
        assert!(
            cursor_plugin_page
                .records
                .iter()
                .any(|call| call.id == older_call_id),
            "older Host API call should be returned after newest call cursor"
        );

        let run_status_page = repository
            .list_plugin_host_api_calls_page(PluginHostApiCallFilter {
                plugin_id: None,
                execution_run_id: Some(run_id.clone()),
                status_code: Some(202),
                cursor: None,
                limit: 25,
            })
            .await
            .expect("execution/status Host API call list should execute");
        assert_eq!(run_status_page.records.len(), 1);
        assert_eq!(run_status_page.records[0].id, newer_call_id);
        assert_eq!(
            run_status_page.records[0].execution_run_id.as_deref(),
            Some(run_id.as_str())
        );

        let run_page = repository
            .list_plugin_host_api_calls_for_run_page(&run_id, None, 25)
            .await
            .expect("run-scoped Host API call list should execute")
            .expect("execution run should exist");
        assert!(
            run_page.records.iter().any(|call| call.id == older_call_id),
            "older run-scoped Host API call should be listed"
        );
        assert!(
            run_page.records.iter().any(|call| call.id == newer_call_id),
            "newer run-scoped Host API call should be listed"
        );

        let run_cursor_page = repository
            .list_plugin_host_api_calls_for_run_page(&run_id, Some(newer_call_id), 25)
            .await
            .expect("cursor run-scoped Host API call list should execute")
            .expect("execution run should exist");
        assert!(
            run_cursor_page
                .records
                .iter()
                .any(|call| call.id == older_call_id),
            "older run-scoped Host API call should be returned after newest call cursor"
        );

        let invalid_run_page = repository
            .list_plugin_host_api_calls_for_run_page("not-a-uuid", None, 25)
            .await
            .expect("invalid execution run id should be safely handled");
        assert!(invalid_run_page.is_none());

        let invalid_global_page = repository
            .list_plugin_host_api_calls_page(PluginHostApiCallFilter {
                plugin_id: None,
                execution_run_id: Some("not-a-uuid".to_owned()),
                status_code: None,
                cursor: None,
                limit: 25,
            })
            .await
            .expect("invalid global execution run filter should execute safely");
        assert!(invalid_global_page.records.is_empty());

        sqlx::query("delete from event_outbox where id = $1")
            .bind(dispatch_internal_id)
            .execute(&pool)
            .await
            .expect("delete smoke dispatch outbox row");
        sqlx::query("delete from plugin_installations where plugin_id = $1")
            .bind(&plugin_id)
            .execute(&pool)
            .await
            .expect("delete smoke plugin installation");
        sqlx::query("delete from plugin_packages where plugin_id = $1")
            .bind(&plugin_id)
            .execute(&pool)
            .await
            .expect("delete smoke plugin package");
    }

    #[test]
    fn plugin_dispatch_keyset_indexes_match_admin_query_shape() {
        let migration =
            include_str!("../../migrations/0046_plugin_dispatch_admin_keyset_indexes.sql");
        let repository = repository_source();

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
        let repository = repository_source();

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
    fn plugin_dispatch_admin_queries_have_live_schema_smoke() {
        let repository = repository_source();
        let smoke_name = [
            "plugin_dispatch_admin_queries",
            "execute_against_live_schema",
        ]
        .join("_");

        assert!(
            repository.contains(&format!("async fn {smoke_name}")),
            "plugin dispatch and execution-run admin queries should have an ignored live-DB smoke"
        );
    }

    // Live-DB smoke: executes the Admin plugin dispatch and execution-run
    // keyset queries against the migrated schema. The smoke inserts an
    // isolated approved plugin, two dispatch outbox rows, and two execution
    // runs, then exercises status filters, cursor pagination, and invalid
    // dispatch ids through the repository methods.
    //   cargo test -- --ignored plugin_dispatch_admin_queries_execute_against_live_schema
    #[tokio::test]
    #[ignore = "requires a running PostgreSQL from ./scripts/dev-deps.ps1"]
    async fn plugin_dispatch_admin_queries_execute_against_live_schema() {
        use sqlx::postgres::PgPoolOptions;
        use std::time::{SystemTime, UNIX_EPOCH};

        let url = std::env::var("DATABASE_URL")
            .unwrap_or_else(|_| "postgres://fbz:fbz@127.0.0.1:5432/fbz".to_owned());
        let pool = PgPoolOptions::new()
            .max_connections(2)
            .connect(&url)
            .await
            .expect("connect to live PostgreSQL");
        crate::db::migrate(&pool).await.expect("run migrations");

        let dispatch_index_def = sqlx::query_scalar::<_, String>(
            r#"
            select indexdef
            from pg_indexes
            where schemaname = 'public'
              and indexname = 'idx_event_outbox_plugin_dispatch_status_recent_keyset'
            "#,
        )
        .fetch_one(&pool)
        .await
        .expect("plugin dispatch status keyset index should exist");
        let dispatch_index_def = dispatch_index_def.to_ascii_lowercase();
        assert!(dispatch_index_def.contains("status"));
        assert!(dispatch_index_def.contains("created_at desc"));
        assert!(dispatch_index_def.contains("id desc"));
        assert!(dispatch_index_def.contains("plugin.hook.dispatch"));

        let run_index_def = sqlx::query_scalar::<_, String>(
            r#"
            select indexdef
            from pg_indexes
            where schemaname = 'public'
              and indexname = 'idx_plugin_execution_runs_dispatch_status_started_keyset'
            "#,
        )
        .fetch_one(&pool)
        .await
        .expect("plugin execution run dispatch/status keyset index should exist");
        let run_index_def = run_index_def.to_ascii_lowercase();
        assert!(run_index_def.contains("outbox_event_public_id"));
        assert!(run_index_def.contains("status"));
        assert!(run_index_def.contains("started_at desc"));
        assert!(run_index_def.contains("id desc"));

        let suffix = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system clock should be after unix epoch")
            .as_nanos();
        let plugin_id = format!("dev.fbz.dispatch.smoke.{suffix}");
        let package_version = format!("0.0.{suffix}");
        let package_path = format!("H:/fbz-smoke/plugins/{suffix}.zip");

        let package_id = sqlx::query_scalar::<_, i64>(
            r#"
            insert into plugin_packages (
                plugin_id,
                package_version,
                api_version,
                runtime,
                name,
                entrypoint,
                package_path,
                manifest,
                manifest_hash,
                permission_fingerprint,
                checksum_sha256,
                package_status
            )
            values (
                $1,
                $2,
                '1',
                'http',
                'Plugin dispatch audit smoke',
                'http://127.0.0.1:19998/fbz-plugin',
                $3,
                jsonb_build_object('id', $1::text, 'version', $2::text),
                gen_random_bytes(32),
                gen_random_bytes(32),
                gen_random_bytes(32),
                'approved'
            )
            returning id
            "#,
        )
        .bind(&plugin_id)
        .bind(&package_version)
        .bind(&package_path)
        .fetch_one(&pool)
        .await
        .expect("create plugin dispatch smoke package");

        sqlx::query(
            r#"
            insert into plugin_installations (
                plugin_id,
                active_package_id,
                enabled,
                approval_status,
                permission_fingerprint,
                approved_at
            )
            values ($1, $2, true, 'approved', gen_random_bytes(32), now())
            "#,
        )
        .bind(&plugin_id)
        .bind(package_id)
        .execute(&pool)
        .await
        .expect("create plugin dispatch smoke installation");

        let older_dispatch_row = sqlx::query(
            r#"
            insert into event_outbox (
                event_type,
                aggregate_type,
                aggregate_id,
                payload,
                status,
                attempts,
                max_attempts,
                created_at,
                available_at,
                last_error
            )
            values (
                $1,
                'plugin',
                $2,
                jsonb_build_object(
                    'pluginId', $2::text,
                    'packageId', $3::text,
                    'hookId', null,
                    'handler', 'hooks.onSmokeOlder',
                    'hookEvent', 'library.scan.completed'
                ),
                'failed',
                2,
                5,
                now() - interval '2 minutes',
                now() - interval '2 minutes',
                'plugin dispatch smoke failure'
            )
            returning id, public_id::text as public_id
            "#,
        )
        .bind(PLUGIN_HOOK_DISPATCH_EVENT)
        .bind(&plugin_id)
        .bind(&package_version)
        .fetch_one(&pool)
        .await
        .expect("create older plugin dispatch smoke row");
        let older_dispatch_internal_id = older_dispatch_row
            .try_get::<i64, _>("id")
            .expect("older dispatch internal id should be returned");
        let older_dispatch_id = older_dispatch_row
            .try_get::<String, _>("public_id")
            .expect("older dispatch public id should be returned");

        let newer_dispatch_row = sqlx::query(
            r#"
            insert into event_outbox (
                event_type,
                aggregate_type,
                aggregate_id,
                payload,
                status,
                attempts,
                max_attempts,
                created_at,
                available_at
            )
            values (
                $1,
                'plugin',
                $2,
                jsonb_build_object(
                    'pluginId', $2::text,
                    'packageId', $3::text,
                    'hookId', null,
                    'handler', 'hooks.onSmokeNewer',
                    'hookEvent', 'library.scan.completed'
                ),
                'delivered',
                1,
                5,
                now() - interval '1 minute',
                now() - interval '1 minute'
            )
            returning id, public_id::text as public_id
            "#,
        )
        .bind(PLUGIN_HOOK_DISPATCH_EVENT)
        .bind(&plugin_id)
        .bind(&package_version)
        .fetch_one(&pool)
        .await
        .expect("create newer plugin dispatch smoke row");
        let newer_dispatch_internal_id = newer_dispatch_row
            .try_get::<i64, _>("id")
            .expect("newer dispatch internal id should be returned");
        let newer_dispatch_id = newer_dispatch_row
            .try_get::<String, _>("public_id")
            .expect("newer dispatch public id should be returned");

        let older_run_id = sqlx::query_scalar::<_, String>(
            r#"
            insert into plugin_execution_runs (
                outbox_event_id,
                outbox_event_public_id,
                attempt,
                plugin_id,
                package_id,
                handler,
                event_key,
                runtime,
                entrypoint,
                status,
                request_payload,
                response_status,
                response_body,
                started_at,
                finished_at,
                duration_ms
            )
            values (
                $1,
                $2,
                1,
                $3,
                $4,
                'hooks.onSmokeNewer',
                'library.scan.completed',
                'http',
                'http://127.0.0.1:19998/fbz-plugin',
                'failed',
                jsonb_build_object('smoke', true, 'attempt', 1),
                500,
                'failure',
                now() - interval '40 seconds',
                now() - interval '39 seconds',
                20
            )
            returning public_id::text
            "#,
        )
        .bind(newer_dispatch_internal_id)
        .bind(&newer_dispatch_id)
        .bind(&plugin_id)
        .bind(&package_version)
        .fetch_one(&pool)
        .await
        .expect("create older plugin execution run smoke row");

        let newer_run_id = sqlx::query_scalar::<_, String>(
            r#"
            insert into plugin_execution_runs (
                outbox_event_id,
                outbox_event_public_id,
                attempt,
                plugin_id,
                package_id,
                handler,
                event_key,
                runtime,
                entrypoint,
                status,
                request_payload,
                response_status,
                response_body,
                started_at,
                finished_at,
                duration_ms
            )
            values (
                $1,
                $2,
                2,
                $3,
                $4,
                'hooks.onSmokeNewer',
                'library.scan.completed',
                'http',
                'http://127.0.0.1:19998/fbz-plugin',
                'succeeded',
                jsonb_build_object('smoke', true, 'attempt', 2),
                200,
                'ok',
                now() - interval '20 seconds',
                now() - interval '19 seconds',
                10
            )
            returning public_id::text
            "#,
        )
        .bind(newer_dispatch_internal_id)
        .bind(&newer_dispatch_id)
        .bind(&plugin_id)
        .bind(&package_version)
        .fetch_one(&pool)
        .await
        .expect("create newer plugin execution run smoke row");

        let repository = AdminRepository::new(pool.clone());
        let dispatch_page = repository
            .list_plugin_dispatches_page(PluginDispatchFilter {
                status: None,
                cursor: None,
                limit: 25,
            })
            .await
            .expect("plugin dispatch list should execute");
        assert!(
            dispatch_page
                .records
                .iter()
                .any(|dispatch| dispatch.id == older_dispatch_id),
            "older smoke dispatch should be listed"
        );
        assert!(
            dispatch_page
                .records
                .iter()
                .any(|dispatch| dispatch.id == newer_dispatch_id),
            "newer smoke dispatch should be listed"
        );

        let failed_dispatch_page = repository
            .list_plugin_dispatches_page(PluginDispatchFilter {
                status: Some("failed".to_owned()),
                cursor: None,
                limit: 25,
            })
            .await
            .expect("filtered plugin dispatch list should execute");
        assert!(
            failed_dispatch_page
                .records
                .iter()
                .any(|dispatch| dispatch.id == older_dispatch_id && dispatch.status == "failed"),
            "failed smoke dispatch should be returned by status filter"
        );

        let cursor_dispatch_page = repository
            .list_plugin_dispatches_page(PluginDispatchFilter {
                status: None,
                cursor: Some(newer_dispatch_id.clone()),
                limit: 25,
            })
            .await
            .expect("cursor plugin dispatch list should execute");
        assert!(
            cursor_dispatch_page
                .records
                .iter()
                .any(|dispatch| dispatch.id == older_dispatch_id),
            "older dispatch should be returned after newest dispatch cursor"
        );

        let runs_page = repository
            .list_plugin_execution_runs_page(
                &newer_dispatch_id,
                PluginExecutionRunFilter {
                    status: None,
                    cursor: None,
                    limit: 10,
                },
            )
            .await
            .expect("plugin execution run list should execute")
            .expect("newer dispatch should exist");
        assert!(
            runs_page.records.iter().any(|run| run.id == older_run_id),
            "older smoke execution run should be listed"
        );
        assert!(
            runs_page.records.iter().any(|run| run.id == newer_run_id),
            "newer smoke execution run should be listed"
        );

        let failed_runs_page = repository
            .list_plugin_execution_runs_page(
                &newer_dispatch_id,
                PluginExecutionRunFilter {
                    status: Some("failed".to_owned()),
                    cursor: None,
                    limit: 10,
                },
            )
            .await
            .expect("filtered plugin execution run list should execute")
            .expect("newer dispatch should exist");
        assert_eq!(failed_runs_page.records.len(), 1);
        assert_eq!(failed_runs_page.records[0].id, older_run_id);
        assert_eq!(failed_runs_page.records[0].status, "failed");

        let cursor_runs_page = repository
            .list_plugin_execution_runs_page(
                &newer_dispatch_id,
                PluginExecutionRunFilter {
                    status: None,
                    cursor: Some(newer_run_id),
                    limit: 10,
                },
            )
            .await
            .expect("cursor plugin execution run list should execute")
            .expect("newer dispatch should exist");
        assert!(
            cursor_runs_page
                .records
                .iter()
                .any(|run| run.id == older_run_id),
            "older execution run should be returned after newest run cursor"
        );

        let missing_runs_page = repository
            .list_plugin_execution_runs_page(
                "not-a-uuid",
                PluginExecutionRunFilter {
                    status: None,
                    cursor: None,
                    limit: 10,
                },
            )
            .await
            .expect("invalid plugin dispatch id should be safely handled");
        assert!(missing_runs_page.is_none());

        sqlx::query("delete from event_outbox where id = any($1)")
            .bind(&[older_dispatch_internal_id, newer_dispatch_internal_id][..])
            .execute(&pool)
            .await
            .expect("delete smoke dispatch outbox rows");
        sqlx::query("delete from plugin_installations where plugin_id = $1")
            .bind(&plugin_id)
            .execute(&pool)
            .await
            .expect("delete smoke plugin installation");
        sqlx::query("delete from plugin_packages where plugin_id = $1")
            .bind(&plugin_id)
            .execute(&pool)
            .await
            .expect("delete smoke plugin package");
    }

    #[test]
    fn admin_public_id_entrypoints_keep_uuid_index_shape() {
        let repository = repository_source();
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
        assert!(
            repository
                .contains("from scheduled_tasks tasks\n            where tasks.public_id = case")
        );
    }

    #[test]
    fn admin_queue_public_id_inputs_use_uuid_comparisons() {
        const UUID_REGEX: &str = "'^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$'";

        assert!(ADMIN_LIBRARY_BY_PUBLIC_ID_SQL.contains("where public_id = case"));
        assert!(ADMIN_LIBRARY_BY_PUBLIC_ID_SQL.contains("$1::uuid"));
        assert!(ADMIN_LIBRARY_BY_PUBLIC_ID_SQL.contains(UUID_REGEX));
        assert!(!ADMIN_LIBRARY_BY_PUBLIC_ID_SQL.contains("public_id::text = $1"));

        assert!(ADMIN_QUEUE_LIBRARY_SCAN_SQL.contains("target_library as"));
        assert!(ADMIN_QUEUE_LIBRARY_SCAN_SQL.contains("where public_id = case"));
        assert!(ADMIN_QUEUE_LIBRARY_SCAN_SQL.contains("$1::uuid"));
        assert!(ADMIN_QUEUE_LIBRARY_SCAN_SQL.contains(UUID_REGEX));
        assert!(ADMIN_QUEUE_LIBRARY_SCAN_SQL.contains("existing as"));
        assert!(
            ADMIN_QUEUE_LIBRARY_SCAN_SQL.contains("j.status in ('queued', 'running', 'failed')")
        );
        assert!(ADMIN_QUEUE_LIBRARY_SCAN_SQL.contains("j.attempts < j.max_attempts"));
        assert!(
            ADMIN_QUEUE_LIBRARY_SCAN_SQL
                .contains("j.payload->>'libraryId' = target_library.public_id")
        );
        assert!(!ADMIN_QUEUE_LIBRARY_SCAN_SQL.contains("values ('library.scan'"));

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
        assert!(ADMIN_EVENT_STREAM_MIRROR_STATUS_SQL.contains("event_stream_mirror_sample"));
    }

    #[test]
    fn event_stream_mirror_status_uses_bounded_lower_bound_counts() {
        let migration =
            include_str!("../../migrations/0071_admin_event_stream_mirror_status_indexes.sql");
        let normalized = ADMIN_EVENT_STREAM_MIRROR_STATUS_SQL
            .split_whitespace()
            .collect::<Vec<_>>()
            .join(" ")
            .to_ascii_lowercase();

        assert!(
            ADMIN_EVENT_STREAM_MIRROR_STATUS_SQL.contains("admin_event_stream_mirror_sample_limit")
        );
        assert!(ADMIN_EVENT_STREAM_MIRROR_STATUS_SQL.contains("event_stream_mirror_sample as"));
        assert!(ADMIN_EVENT_STREAM_MIRROR_STATUS_SQL.contains("limit 10001"));
        assert!(ADMIN_EVENT_STREAM_MIRROR_STATUS_SQL.contains("counts_are_exact"));
        assert!(ADMIN_EVENT_STREAM_MIRROR_STATUS_SQL.contains("sample_limit"));
        assert!(migration.contains("idx_event_outbox_stream_mirror_created"));
        assert!(migration.contains("on event_outbox (created_at asc, id asc)"));
        assert!(migration.contains("where stream_mirrored_at is null"));

        assert!(
            !normalized.contains("select count(*)::bigint as unmirrored_count"),
            "admin mirror status should not exact-count the full unmirrored backlog"
        );
        assert!(
            !normalized.contains("from unmirrored"),
            "admin mirror status should aggregate from a bounded sample"
        );
    }

    // Live-DB smoke: validates the admin mirror status SQL parses and plans
    // against the real migrated schema via EXPLAIN. Plain EXPLAIN does not
    // execute the SELECT, so it is non-mutating.
    //   cargo test -- --ignored admin_event_stream_mirror_status_executes_against_live_schema
    #[tokio::test]
    #[ignore = "requires a running PostgreSQL from ./scripts/dev-deps.ps1"]
    async fn admin_event_stream_mirror_status_executes_against_live_schema() {
        use sqlx::postgres::PgPoolOptions;

        let url = std::env::var("DATABASE_URL")
            .unwrap_or_else(|_| "postgres://fbz:fbz@127.0.0.1:5432/fbz".to_owned());
        let pool = PgPoolOptions::new()
            .max_connections(2)
            .connect(&url)
            .await
            .expect("connect to live PostgreSQL");
        crate::db::migrate(&pool).await.expect("run migrations");

        let plan_rows = sqlx::query(&format!("explain {ADMIN_EVENT_STREAM_MIRROR_STATUS_SQL}"))
            .fetch_all(&pool)
            .await
            .expect("admin mirror status SQL should parse and plan against the live schema");
        assert!(
            !plan_rows.is_empty(),
            "EXPLAIN should return a query plan for the admin mirror status summary"
        );
    }
}
