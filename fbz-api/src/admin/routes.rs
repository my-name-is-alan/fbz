use axum::{
    Json, Router,
    extract::{Path, Query, State},
    http::{HeaderMap, HeaderValue, StatusCode, Uri},
    routing::{get, post, put},
};
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::{
    admin::{
        access::authenticate_admin,
        repository::{
            AddLibraryPathInput, AdminJobDetailRecord, AdminJobEventFilter, AdminJobEventRecord,
            AdminJobFilter, AdminJobRecord, AdminJobRunFilter, AdminJobRunRecord, AdminRepository,
            AdminUserFilter, AdminUserLibraryPermissionFilter, AdminUserLibraryPermissionRecord,
            AdminUserRecord, CreateLibraryInput, EventStreamMirrorStatusRecord,
            LibraryMetadataRefreshQueueRecord, LibraryPathRecord, ManagedLibraryRecord,
            MetadataRefreshJobRecord, NotificationDeliveryAttemptFilter,
            NotificationDeliveryAttemptRecord, NotificationRequestFilter,
            NotificationRequestRecord, NotificationRetryError, NotificationTargetFilter,
            NotificationTargetRecord, PluginDispatchFilter, PluginDispatchRecord,
            PluginDispatchReplayError, PluginExecutionRunFilter, PluginExecutionRunRecord,
            PluginHostApiCallFilter, PluginHostApiCallRecord, QueueLibraryMetadataRefreshInput,
            QueueLibraryScanInput, QueueMetadataRefreshInput, ScanJobRecord,
            ScheduledTaskAdminRecord, ScheduledTaskFilter, ScheduledTaskRunFilter,
            ScheduledTaskRunRecord, UpdateUserLibraryPermissionInput, UpdateUserPolicyInput,
            UpsertNotificationTargetInput,
        },
    },
    config::{Config, MetadataConfig},
    error::AppError,
    notifications::{
        secrets::{SecretCipher, TargetSecretInput},
        target_config::{redacted_target_config, secretize_target_config},
    },
    scan::service::{ScanError, ScanRunSummary, ScanService},
    scheduler::service::{
        SchedulerError, SchedulerRunSummary, SchedulerService, default_worker_id,
    },
    state::AppState,
    transcode::repository::{TranscodeRepository, TranscodeSessionFilter, TranscodeSessionRecord},
};

const MAX_NOTIFICATION_TARGETS_LIST_LIMIT: i64 = 200;
const MAX_NOTIFICATION_REQUESTS_LIST_LIMIT: i64 = 200;
const MAX_NOTIFICATION_ATTEMPTS_LIST_LIMIT: i64 = 500;
const MAX_ADMIN_JOBS_LIST_LIMIT: i64 = 200;
const MAX_ADMIN_JOB_RUNS_LIST_LIMIT: i64 = 200;
const MAX_ADMIN_JOB_EVENTS_LIST_LIMIT: i64 = 200;
const MAX_ADMIN_USERS_LIST_LIMIT: i64 = 500;
const MAX_ADMIN_USER_LIBRARY_PERMISSIONS_LIMIT: i64 = 2_000;
const MAX_PLUGIN_DISPATCHES_LIST_LIMIT: i64 = 200;
const MAX_PLUGIN_EXECUTION_RUNS_LIST_LIMIT: i64 = 500;
const MAX_PLUGIN_HOST_API_CALLS_LIST_LIMIT: i64 = 1_000;
const MAX_SCHEDULED_TASKS_LIST_LIMIT: i64 = 200;
const MAX_SCHEDULED_TASK_RUNS_LIST_LIMIT: i64 = 500;
const MAX_TRANSCODE_SESSIONS_LIST_LIMIT: i64 = 200;
const DEFAULT_LIBRARY_METADATA_REFRESH_LIMIT: i64 = 1_000;
const MAX_LIBRARY_METADATA_REFRESH_LIMIT: i64 = 50_000;
const MAX_NOTIFICATION_TARGET_NAME_LEN: usize = 128;
const MAX_NOTIFICATION_TARGET_CHANNEL_LEN: usize = 64;
const MAX_USER_DISPLAY_NAME_LEN: usize = 128;
const MAX_ADMIN_USER_ROLE_NAME_LEN: usize = 128;
const MAX_SCHEDULED_TASK_KEY_LEN: usize = 256;
const MAX_ADMIN_JOB_FILTER_TEXT_LEN: usize = 128;
const MAX_TRANSCODE_HARDWARE_ACCELERATION_LEN: usize = 64;

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/api/admin/libraries", post(create_library))
        .route("/api/admin/libraries/{library_id}/paths", post(add_path))
        .route("/api/admin/libraries/{library_id}/scan", post(queue_scan))
        .route(
            "/api/admin/libraries/{library_id}/metadata/refresh",
            post(queue_library_metadata_refresh),
        )
        .route(
            "/api/admin/media-items/{item_id}/metadata/refresh",
            post(queue_item_metadata_refresh),
        )
        .route(
            "/api/admin/metadata/providers",
            get(list_metadata_providers),
        )
        .route("/api/admin/users", get(list_users))
        .route("/api/admin/users/{user_id}/policy", put(update_user_policy))
        .route(
            "/api/admin/users/{user_id}/libraries",
            get(list_user_library_permissions),
        )
        .route(
            "/api/admin/users/{user_id}/libraries/{library_id}/permissions",
            put(update_user_library_permission),
        )
        .route("/api/admin/jobs", get(list_jobs))
        .route("/api/admin/jobs/{job_id}", get(get_job_detail))
        .route("/api/admin/jobs/{job_id}/runs", get(list_job_runs))
        .route("/api/admin/jobs/{job_id}/events", get(list_job_events))
        .route("/api/admin/jobs/{job_id}/run", post(run_job))
        .route("/api/admin/scheduled-tasks", get(list_scheduled_tasks))
        .route(
            "/api/admin/scheduled-tasks/{task_key}/runs",
            get(list_scheduled_task_runs),
        )
        .route(
            "/api/admin/scheduled-tasks/{task_key}/run",
            post(run_scheduled_task),
        )
        .route(
            "/api/admin/transcoding-sessions",
            get(list_transcoding_sessions),
        )
        .route(
            "/api/admin/transcoding-sessions/{session_id}/cancel",
            post(cancel_transcoding_session),
        )
        .route(
            "/api/admin/notification-targets",
            get(list_notification_targets).post(create_notification_target),
        )
        .route(
            "/api/admin/notification-targets/{target_id}",
            put(replace_notification_target),
        )
        .route(
            "/api/admin/notification-targets/{target_id}/enable",
            post(enable_notification_target),
        )
        .route(
            "/api/admin/notification-targets/{target_id}/disable",
            post(disable_notification_target),
        )
        .route(
            "/api/admin/notification-requests",
            get(list_notification_requests),
        )
        .route(
            "/api/admin/notification-requests/{request_id}/attempts",
            get(list_notification_delivery_attempts),
        )
        .route(
            "/api/admin/notification-requests/{request_id}/retry",
            post(retry_notification_request),
        )
        .route("/api/admin/plugin-dispatches", get(list_plugin_dispatches))
        .route(
            "/api/admin/plugin-dispatches/{dispatch_id}/runs",
            get(list_plugin_execution_runs),
        )
        .route(
            "/api/admin/plugin-execution-runs/{run_id}/host-api-calls",
            get(list_plugin_host_api_calls_for_run),
        )
        .route(
            "/api/admin/plugin-host-api-calls",
            get(list_plugin_host_api_calls),
        )
        .route(
            "/api/admin/event-stream-mirror/status",
            get(event_stream_mirror_status),
        )
        .route(
            "/api/admin/plugin-dispatches/{dispatch_id}/replay",
            post(replay_plugin_dispatch),
        )
}

#[derive(Clone, Debug, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct CreateLibraryRequestDto {
    pub name: String,
    pub library_type: String,
    #[serde(default)]
    pub paths: Vec<String>,
    pub preferred_metadata_language: Option<String>,
    pub preferred_metadata_country: Option<String>,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct AddLibraryPathRequestDto {
    pub path: String,
}

#[derive(Clone, Debug, Default, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct QueueLibraryScanRequestDto {
    pub reason: Option<String>,
}

#[derive(Clone, Debug, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct QueueMetadataRefreshRequestDto {
    pub reason: Option<String>,
}

#[derive(Clone, Debug, Default, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct QueueLibraryMetadataRefreshRequestDto {
    pub reason: Option<String>,
    pub limit: Option<i64>,
}

#[derive(Clone, Debug, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct UpsertNotificationTargetRequestDto {
    pub name: String,
    pub target_type: String,
    #[serde(default)]
    pub channel: Option<String>,
    pub config: Value,
    #[serde(default)]
    pub is_enabled: Option<bool>,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct UpdateUserPolicyRequestDto {
    #[serde(default)]
    pub display_name: Option<String>,
    pub is_disabled: bool,
    pub allow_download: bool,
    pub allow_transcode: bool,
    pub allow_new_device_login: bool,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct UpdateUserLibraryPermissionRequestDto {
    pub can_view: bool,
    pub can_download: bool,
    pub can_transcode: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct UserPolicyInput {
    display_name: Option<String>,
    is_disabled: bool,
    allow_download: bool,
    allow_transcode: bool,
    allow_new_device_login: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct UserLibraryPermissionInput {
    can_view: bool,
    can_download: bool,
    can_transcode: bool,
}

#[derive(Clone, Debug, PartialEq)]
struct NotificationTargetInput {
    name: String,
    target_type: String,
    channel: Option<String>,
    config: Value,
    secrets: Vec<TargetSecretInput>,
    is_enabled: bool,
}

#[derive(Clone, Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ManagedLibraryDto {
    pub id: String,
    pub name: String,
    pub library_type: String,
}

#[derive(Clone, Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct LibraryPathDto {
    pub id: String,
    pub library_id: String,
    pub path: String,
    pub is_enabled: bool,
}

#[derive(Clone, Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ScanJobDto {
    pub id: String,
    pub status: String,
    pub queue_name: String,
    pub job_type: String,
}

#[derive(Clone, Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct MetadataRefreshJobDto {
    pub id: String,
    pub status: String,
    pub queue_name: String,
    pub job_type: String,
    pub item_id: String,
}

#[derive(Clone, Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct LibraryMetadataRefreshQueueDto {
    pub library_id: String,
    pub queued_jobs: i64,
}

#[derive(Clone, Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct AdminUserDto {
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

#[derive(Clone, Debug, Default, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct AdminUserQueryDto {
    pub role_name: Option<String>,
    pub is_disabled: Option<bool>,
    pub cursor: Option<String>,
    pub limit: Option<i64>,
}

#[derive(Clone, Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct AdminUserLibraryPermissionDto {
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

#[derive(Clone, Debug, Default, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct AdminUserLibraryPermissionQueryDto {
    pub library_type: Option<String>,
    pub permission_configured: Option<bool>,
    pub cursor: Option<String>,
    pub limit: Option<i64>,
}

#[derive(Clone, Debug, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct AdminJobDto {
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

#[derive(Clone, Debug, Default, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct AdminJobQueryDto {
    pub status: Option<String>,
    pub job_type: Option<String>,
    pub queue_name: Option<String>,
    pub cursor: Option<String>,
    pub limit: Option<i64>,
}

#[derive(Clone, Debug, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct AdminJobDetailDto {
    pub job: AdminJobDto,
    pub runs: Vec<AdminJobRunDto>,
    pub events: Vec<AdminJobEventDto>,
}

#[derive(Clone, Debug, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct AdminJobRunDto {
    pub id: i64,
    pub worker_id: String,
    pub status: String,
    pub started_at: String,
    pub finished_at: Option<String>,
    pub duration_ms: i64,
    pub error_message: Option<String>,
    pub metrics: Value,
}

#[derive(Clone, Debug, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct AdminJobEventDto {
    pub id: i64,
    pub run_id: Option<i64>,
    pub event_type: String,
    pub event_level: String,
    pub message: Option<String>,
    pub payload: Value,
    pub created_at: String,
}

#[derive(Clone, Debug, Default, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct AdminJobRunQueryDto {
    pub status: Option<String>,
    pub cursor: Option<String>,
    pub limit: Option<i64>,
}

#[derive(Clone, Debug, Default, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct AdminJobEventQueryDto {
    pub event_level: Option<String>,
    pub cursor: Option<String>,
    pub limit: Option<i64>,
}

#[derive(Clone, Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct MetadataProviderStatusDto {
    pub provider: String,
    pub enabled: bool,
    pub search_supported: bool,
    pub credential_configured: bool,
    pub api_base_url: String,
    pub image_base_url: Option<String>,
}

#[derive(Clone, Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct MetadataProviderStatusResponseDto {
    pub providers: Vec<MetadataProviderStatusDto>,
    pub proxy_policy: String,
    pub http_proxy_configured: bool,
    pub https_proxy_configured: bool,
}

#[derive(Clone, Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ScanRunSummaryDto {
    pub job_id: String,
    pub status: String,
    pub scanned_files: usize,
    pub created_items: usize,
    pub updated_files: usize,
    pub metadata_refresh_jobs: i64,
    pub probe_jobs: i64,
    pub missing_items: i64,
    pub missing_mark_skipped: bool,
    pub has_more: bool,
    pub continuation_job_id: Option<String>,
}

#[derive(Clone, Debug, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct NotificationTargetDto {
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

#[derive(Clone, Debug, Default, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct NotificationTargetQueryDto {
    pub target_type: Option<String>,
    pub channel: Option<String>,
    pub enabled: Option<bool>,
    pub cursor: Option<String>,
    pub limit: Option<i64>,
}

#[derive(Clone, Debug, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct NotificationRequestDto {
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

#[derive(Clone, Debug, Default, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct NotificationRequestQueryDto {
    pub status: Option<String>,
    pub channel: Option<String>,
    pub cursor: Option<String>,
    pub limit: Option<i64>,
}

#[derive(Clone, Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct NotificationDeliveryAttemptDto {
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

#[derive(Clone, Debug, Default, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct NotificationDeliveryAttemptQueryDto {
    pub status: Option<String>,
    pub cursor: Option<String>,
    pub limit: Option<i64>,
}

#[derive(Clone, Debug, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct PluginDispatchDto {
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

#[derive(Clone, Debug, Default, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct PluginDispatchQueryDto {
    pub status: Option<String>,
    pub cursor: Option<String>,
    pub limit: Option<i64>,
}

#[derive(Clone, Debug, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct PluginExecutionRunDto {
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

#[derive(Clone, Debug, Default, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct PluginExecutionRunQueryDto {
    pub status: Option<String>,
    pub cursor: Option<String>,
    pub limit: Option<i64>,
}

#[derive(Clone, Debug, Default, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct PluginHostApiCallQueryDto {
    pub plugin_id: Option<String>,
    pub execution_run_id: Option<String>,
    pub status_code: Option<i32>,
    pub cursor: Option<String>,
    pub limit: Option<i64>,
}

#[derive(Clone, Debug, Default, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct PluginHostApiCallRunQueryDto {
    pub cursor: Option<String>,
    pub limit: Option<i64>,
}

#[derive(Clone, Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct PluginHostApiCallDto {
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

#[derive(Clone, Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct EventStreamMirrorStatusDto {
    pub enabled: bool,
    pub stream_key: String,
    pub batch_size: u16,
    pub interval_seconds: u64,
    pub lease_seconds: u64,
    pub operation_timeout_ms: u64,
    pub retry_base_seconds: u64,
    pub retry_max_seconds: u64,
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

#[derive(Clone, Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ScheduledTaskDto {
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

#[derive(Clone, Debug, Default, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ScheduledTaskQueryDto {
    pub task_type: Option<String>,
    pub owner_type: Option<String>,
    pub enabled: Option<bool>,
    pub cursor: Option<String>,
    pub limit: Option<i64>,
}

#[derive(Clone, Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ScheduledTaskRunDto {
    pub task_key: String,
    pub task_type: String,
    pub queued_jobs: i64,
}

#[derive(Clone, Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ScheduledTaskRunHistoryDto {
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

#[derive(Clone, Debug, Default, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ScheduledTaskRunQueryDto {
    pub status: Option<String>,
    pub cursor: Option<String>,
    pub limit: Option<i64>,
}

#[derive(Clone, Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct TranscodeSessionDto {
    pub id: String,
    pub status: String,
    pub hardware_acceleration: Option<String>,
    pub input_path: Option<String>,
    pub output_path: Option<String>,
    pub manifest_path: Option<String>,
    pub video_codec: Option<String>,
    pub audio_codec: Option<String>,
    pub container: Option<String>,
    pub bitrate: Option<i32>,
    pub worker_id: Option<String>,
    pub lease_expires_at: Option<String>,
    pub attempts: i32,
    pub max_attempts: i32,
    pub error_message: Option<String>,
    pub created_at: String,
    pub updated_at: String,
    pub started_at: Option<String>,
    pub finished_at: Option<String>,
}

#[derive(Clone, Debug, Default, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct TranscodeSessionQueryDto {
    pub status: Option<String>,
    pub hardware_acceleration: Option<String>,
    pub cursor: Option<String>,
    pub limit: Option<i64>,
}

pub async fn create_library(
    State(state): State<AppState>,
    headers: HeaderMap,
    uri: Uri,
    Json(payload): Json<CreateLibraryRequestDto>,
) -> Result<(StatusCode, Json<ManagedLibraryDto>), AppError> {
    let admin = authenticate_admin(&state, &headers, &uri).await?;
    let library_type = normalize_library_type(&payload.library_type);
    validate_library_type(&library_type)?;
    validate_non_empty("name", &payload.name)?;
    for path in &payload.paths {
        validate_non_empty("path", path)?;
    }

    let Some(database) = state.database() else {
        return Err(AppError::internal("database is not configured"));
    };
    let library = AdminRepository::new(database.clone())
        .create_library(CreateLibraryInput {
            name: payload.name,
            library_type,
            preferred_metadata_language: payload.preferred_metadata_language,
            preferred_metadata_country: payload.preferred_metadata_country,
            paths: payload.paths,
            owner_user_id: admin.id,
        })
        .await
        .map_err(|err| AppError::internal(format!("failed to create library: {err}")))?;

    Ok((StatusCode::CREATED, Json(ManagedLibraryDto::from(library))))
}

pub async fn add_path(
    State(state): State<AppState>,
    Path(library_id): Path<String>,
    headers: HeaderMap,
    uri: Uri,
    Json(payload): Json<AddLibraryPathRequestDto>,
) -> Result<(StatusCode, Json<LibraryPathDto>), AppError> {
    authenticate_admin(&state, &headers, &uri).await?;
    validate_non_empty("path", &payload.path)?;

    let Some(database) = state.database() else {
        return Err(AppError::internal("database is not configured"));
    };
    let Some(path) = AdminRepository::new(database.clone())
        .add_library_path(AddLibraryPathInput {
            library_id,
            path: payload.path,
        })
        .await
        .map_err(|err| AppError::internal(format!("failed to add library path: {err}")))?
    else {
        return Err(AppError::not_found("library not found"));
    };

    Ok((StatusCode::CREATED, Json(LibraryPathDto::from(path))))
}

pub async fn queue_scan(
    State(state): State<AppState>,
    Path(library_id): Path<String>,
    headers: HeaderMap,
    uri: Uri,
    Json(payload): Json<QueueLibraryScanRequestDto>,
) -> Result<(StatusCode, Json<ScanJobDto>), AppError> {
    let admin = authenticate_admin(&state, &headers, &uri).await?;
    let Some(database) = state.database() else {
        return Err(AppError::internal("database is not configured"));
    };
    let Some(job) = AdminRepository::new(database.clone())
        .queue_library_scan(QueueLibraryScanInput {
            library_id,
            requested_by_user_id: admin.id,
            reason: payload.reason,
        })
        .await
        .map_err(|err| AppError::internal(format!("failed to queue library scan: {err}")))?
    else {
        return Err(AppError::not_found("library not found"));
    };

    Ok((StatusCode::ACCEPTED, Json(ScanJobDto::from(job))))
}

pub async fn queue_item_metadata_refresh(
    State(state): State<AppState>,
    Path(item_id): Path<String>,
    headers: HeaderMap,
    uri: Uri,
    Json(payload): Json<QueueMetadataRefreshRequestDto>,
) -> Result<(StatusCode, Json<MetadataRefreshJobDto>), AppError> {
    let admin = authenticate_admin(&state, &headers, &uri).await?;
    let item_id = validate_public_id("itemId", &item_id)?;
    let Some(database) = state.database() else {
        return Err(AppError::internal("database is not configured"));
    };
    let Some(job) = AdminRepository::new(database.clone())
        .queue_metadata_refresh_for_item(
            item_id,
            QueueMetadataRefreshInput {
                requested_by_user_id: admin.id,
                reason: payload.reason,
            },
        )
        .await
        .map_err(|err| AppError::internal(format!("failed to queue metadata refresh: {err}")))?
    else {
        return Err(AppError::not_found("media item not found"));
    };

    Ok((StatusCode::ACCEPTED, Json(MetadataRefreshJobDto::from(job))))
}

pub async fn queue_library_metadata_refresh(
    State(state): State<AppState>,
    Path(library_id): Path<String>,
    headers: HeaderMap,
    uri: Uri,
    Json(payload): Json<QueueLibraryMetadataRefreshRequestDto>,
) -> Result<(StatusCode, Json<LibraryMetadataRefreshQueueDto>), AppError> {
    let admin = authenticate_admin(&state, &headers, &uri).await?;
    let library_id = validate_public_id("libraryId", &library_id)?;
    let Some(database) = state.database() else {
        return Err(AppError::internal("database is not configured"));
    };
    let Some(summary) = AdminRepository::new(database.clone())
        .queue_metadata_refresh_for_library(QueueLibraryMetadataRefreshInput {
            library_id: library_id.to_owned(),
            requested_by_user_id: admin.id,
            reason: payload.reason,
            limit: metadata_refresh_limit(payload.limit),
        })
        .await
        .map_err(|err| {
            AppError::internal(format!("failed to queue library metadata refresh: {err}"))
        })?
    else {
        return Err(AppError::not_found("library not found"));
    };

    Ok((
        StatusCode::ACCEPTED,
        Json(LibraryMetadataRefreshQueueDto::from(summary)),
    ))
}

pub async fn list_metadata_providers(
    State(state): State<AppState>,
    headers: HeaderMap,
    uri: Uri,
) -> Result<Json<MetadataProviderStatusResponseDto>, AppError> {
    authenticate_admin(&state, &headers, &uri).await?;
    Ok(Json(metadata_provider_status_response(state.config())))
}

pub async fn list_users(
    State(state): State<AppState>,
    Query(query): Query<AdminUserQueryDto>,
    headers: HeaderMap,
    uri: Uri,
) -> Result<(HeaderMap, Json<Vec<AdminUserDto>>), AppError> {
    authenticate_admin(&state, &headers, &uri).await?;
    let role_name = query
        .role_name
        .as_deref()
        .map(validate_admin_user_role_name)
        .transpose()?
        .map(str::to_ascii_lowercase);
    let cursor = query
        .cursor
        .as_deref()
        .map(|value| validate_uuid_public_id("cursor", value))
        .transpose()?
        .map(str::to_owned);
    let Some(database) = state.database() else {
        return Err(AppError::internal("database is not configured"));
    };

    let page = AdminRepository::new(database.clone())
        .list_admin_users_page(AdminUserFilter {
            role_name,
            is_disabled: query.is_disabled,
            cursor,
            limit: admin_user_list_limit(query.limit),
        })
        .await
        .map_err(|err| AppError::internal(format!("failed to list users: {err}")))?;
    let response_headers = pagination_headers(page.has_more, page.next_cursor.as_deref())?;

    Ok((
        response_headers,
        Json(page.records.into_iter().map(AdminUserDto::from).collect()),
    ))
}

pub async fn update_user_policy(
    State(state): State<AppState>,
    Path(user_id): Path<String>,
    headers: HeaderMap,
    uri: Uri,
    Json(payload): Json<UpdateUserPolicyRequestDto>,
) -> Result<Json<AdminUserDto>, AppError> {
    let admin = authenticate_admin(&state, &headers, &uri).await?;
    let user_id = validate_uuid_public_id("userId", &user_id)?;
    let input = UserPolicyInput::try_from(payload)?;
    if admin.public_id == user_id && input.is_disabled {
        return Err(AppError::conflict(
            "current admin user cannot disable itself",
        ));
    }

    let Some(database) = state.database() else {
        return Err(AppError::internal("database is not configured"));
    };
    let Some(user) = AdminRepository::new(database.clone())
        .update_user_policy(user_id, UpdateUserPolicyInput::from(input))
        .await
        .map_err(|err| AppError::internal(format!("failed to update user policy: {err}")))?
    else {
        return Err(AppError::not_found("user not found"));
    };

    Ok(Json(AdminUserDto::from(user)))
}

pub async fn list_user_library_permissions(
    State(state): State<AppState>,
    Path(user_id): Path<String>,
    Query(query): Query<AdminUserLibraryPermissionQueryDto>,
    headers: HeaderMap,
    uri: Uri,
) -> Result<(HeaderMap, Json<Vec<AdminUserLibraryPermissionDto>>), AppError> {
    authenticate_admin(&state, &headers, &uri).await?;
    let user_id = validate_uuid_public_id("userId", &user_id)?;
    let library_type = query
        .library_type
        .as_deref()
        .map(normalize_library_type)
        .map(|value| validate_library_type(&value).map(|_| value))
        .transpose()?;
    let cursor = query
        .cursor
        .as_deref()
        .map(|value| validate_uuid_public_id("cursor", value))
        .transpose()?
        .map(str::to_owned);
    let Some(database) = state.database() else {
        return Err(AppError::internal("database is not configured"));
    };

    let Some(page) = AdminRepository::new(database.clone())
        .list_user_library_permissions_page(
            user_id,
            AdminUserLibraryPermissionFilter {
                library_type,
                permission_configured: query.permission_configured,
                cursor,
                limit: admin_user_library_permission_list_limit(query.limit),
            },
        )
        .await
        .map_err(|err| {
            AppError::internal(format!("failed to list user library permissions: {err}"))
        })?
    else {
        return Err(AppError::not_found("user not found"));
    };
    let response_headers = pagination_headers(page.has_more, page.next_cursor.as_deref())?;

    Ok((
        response_headers,
        Json(
            page.records
                .into_iter()
                .map(AdminUserLibraryPermissionDto::from)
                .collect(),
        ),
    ))
}

pub async fn update_user_library_permission(
    State(state): State<AppState>,
    Path((user_id, library_id)): Path<(String, String)>,
    headers: HeaderMap,
    uri: Uri,
    Json(payload): Json<UpdateUserLibraryPermissionRequestDto>,
) -> Result<Json<AdminUserLibraryPermissionDto>, AppError> {
    authenticate_admin(&state, &headers, &uri).await?;
    let user_id = validate_uuid_public_id("userId", &user_id)?;
    let library_id = validate_uuid_public_id("libraryId", &library_id)?;
    let input = UserLibraryPermissionInput::from(payload);
    let Some(database) = state.database() else {
        return Err(AppError::internal("database is not configured"));
    };

    let Some(permission) = AdminRepository::new(database.clone())
        .update_user_library_permission(
            user_id,
            library_id,
            UpdateUserLibraryPermissionInput::from(input),
        )
        .await
        .map_err(|err| {
            AppError::internal(format!("failed to update user library permission: {err}"))
        })?
    else {
        return Err(AppError::not_found("user or library not found"));
    };

    Ok(Json(AdminUserLibraryPermissionDto::from(permission)))
}

pub async fn list_jobs(
    State(state): State<AppState>,
    Query(query): Query<AdminJobQueryDto>,
    headers: HeaderMap,
    uri: Uri,
) -> Result<(HeaderMap, Json<Vec<AdminJobDto>>), AppError> {
    authenticate_admin(&state, &headers, &uri).await?;
    let status = query
        .status
        .as_deref()
        .map(validate_admin_job_status)
        .transpose()?
        .map(str::to_owned);
    let job_type = query
        .job_type
        .as_deref()
        .map(|value| validate_admin_job_filter_text("jobType", value))
        .transpose()?
        .map(str::to_owned);
    let queue_name = query
        .queue_name
        .as_deref()
        .map(|value| validate_admin_job_filter_text("queueName", value))
        .transpose()?
        .map(str::to_owned);
    let cursor = query
        .cursor
        .as_deref()
        .map(|value| validate_uuid_public_id("cursor", value))
        .transpose()?
        .map(str::to_owned);
    let Some(database) = state.database() else {
        return Err(AppError::internal("database is not configured"));
    };

    let page = AdminRepository::new(database.clone())
        .list_admin_jobs_page(AdminJobFilter {
            status,
            job_type,
            queue_name,
            cursor,
            limit: admin_job_list_limit(query.limit),
        })
        .await
        .map_err(|err| AppError::internal(format!("failed to list jobs: {err}")))?;
    let response_headers = pagination_headers(page.has_more, page.next_cursor.as_deref())?;

    Ok((
        response_headers,
        Json(page.records.into_iter().map(AdminJobDto::from).collect()),
    ))
}

pub async fn get_job_detail(
    State(state): State<AppState>,
    Path(job_id): Path<String>,
    headers: HeaderMap,
    uri: Uri,
) -> Result<Json<AdminJobDetailDto>, AppError> {
    authenticate_admin(&state, &headers, &uri).await?;
    let job_id = validate_uuid_public_id("jobId", &job_id)?;
    let Some(database) = state.database() else {
        return Err(AppError::internal("database is not configured"));
    };

    let Some(detail) = AdminRepository::new(database.clone())
        .get_admin_job_detail(
            job_id,
            MAX_ADMIN_JOB_RUNS_LIST_LIMIT,
            MAX_ADMIN_JOB_EVENTS_LIST_LIMIT,
        )
        .await
        .map_err(|err| AppError::internal(format!("failed to get job detail: {err}")))?
    else {
        return Err(AppError::not_found("job not found"));
    };

    Ok(Json(AdminJobDetailDto::from(detail)))
}

pub async fn list_job_runs(
    State(state): State<AppState>,
    Path(job_id): Path<String>,
    Query(query): Query<AdminJobRunQueryDto>,
    headers: HeaderMap,
    uri: Uri,
) -> Result<(HeaderMap, Json<Vec<AdminJobRunDto>>), AppError> {
    authenticate_admin(&state, &headers, &uri).await?;
    let job_id = validate_uuid_public_id("jobId", &job_id)?;
    let status = query
        .status
        .as_deref()
        .map(validate_admin_job_run_status)
        .transpose()?
        .map(str::to_owned);
    let cursor = query
        .cursor
        .as_deref()
        .map(|value| validate_positive_i64_cursor("cursor", value))
        .transpose()?;
    let Some(database) = state.database() else {
        return Err(AppError::internal("database is not configured"));
    };
    let Some(page) = AdminRepository::new(database.clone())
        .list_admin_job_runs_page(
            job_id,
            AdminJobRunFilter {
                status,
                cursor,
                limit: admin_job_run_list_limit(query.limit),
            },
        )
        .await
        .map_err(|err| AppError::internal(format!("failed to list job runs: {err}")))?
    else {
        return Err(AppError::not_found("job not found"));
    };
    let response_headers = pagination_headers(page.has_more, page.next_cursor.as_deref())?;

    Ok((
        response_headers,
        Json(page.records.into_iter().map(AdminJobRunDto::from).collect()),
    ))
}

pub async fn list_job_events(
    State(state): State<AppState>,
    Path(job_id): Path<String>,
    Query(query): Query<AdminJobEventQueryDto>,
    headers: HeaderMap,
    uri: Uri,
) -> Result<(HeaderMap, Json<Vec<AdminJobEventDto>>), AppError> {
    authenticate_admin(&state, &headers, &uri).await?;
    let job_id = validate_uuid_public_id("jobId", &job_id)?;
    let event_level = query
        .event_level
        .as_deref()
        .map(validate_admin_job_event_level)
        .transpose()?
        .map(str::to_owned);
    let cursor = query
        .cursor
        .as_deref()
        .map(|value| validate_positive_i64_cursor("cursor", value))
        .transpose()?;
    let Some(database) = state.database() else {
        return Err(AppError::internal("database is not configured"));
    };
    let Some(page) = AdminRepository::new(database.clone())
        .list_admin_job_events_page(
            job_id,
            AdminJobEventFilter {
                event_level,
                cursor,
                limit: admin_job_event_list_limit(query.limit),
            },
        )
        .await
        .map_err(|err| AppError::internal(format!("failed to list job events: {err}")))?
    else {
        return Err(AppError::not_found("job not found"));
    };
    let response_headers = pagination_headers(page.has_more, page.next_cursor.as_deref())?;

    Ok((
        response_headers,
        Json(
            page.records
                .into_iter()
                .map(AdminJobEventDto::from)
                .collect(),
        ),
    ))
}

pub async fn run_job(
    State(state): State<AppState>,
    Path(job_id): Path<String>,
    headers: HeaderMap,
    uri: Uri,
) -> Result<Json<ScanRunSummaryDto>, AppError> {
    authenticate_admin(&state, &headers, &uri).await?;
    let Some(database) = state.database() else {
        return Err(AppError::internal("database is not configured"));
    };

    let summary = ScanService::new(database.clone())
        .run_scan_job(&job_id)
        .await
        .map_err(scan_error_to_app_error)?;

    Ok(Json(ScanRunSummaryDto::from(summary)))
}

pub async fn list_scheduled_tasks(
    State(state): State<AppState>,
    Query(query): Query<ScheduledTaskQueryDto>,
    headers: HeaderMap,
    uri: Uri,
) -> Result<(HeaderMap, Json<Vec<ScheduledTaskDto>>), AppError> {
    authenticate_admin(&state, &headers, &uri).await?;
    let task_type = query
        .task_type
        .as_deref()
        .map(validate_scheduled_task_filter_text)
        .transpose()?
        .map(str::to_owned);
    let owner_type = query
        .owner_type
        .as_deref()
        .map(validate_scheduled_task_owner_type)
        .transpose()?
        .map(str::to_owned);
    let cursor = query
        .cursor
        .as_deref()
        .map(|value| validate_uuid_public_id("cursor", value))
        .transpose()?
        .map(str::to_owned);
    let Some(database) = state.database() else {
        return Err(AppError::internal("database is not configured"));
    };
    let page = AdminRepository::new(database.clone())
        .list_scheduled_tasks_page(ScheduledTaskFilter {
            task_type,
            owner_type,
            enabled: query.enabled,
            cursor,
            limit: scheduled_task_list_limit(query.limit),
        })
        .await
        .map_err(|err| AppError::internal(format!("failed to list scheduled tasks: {err}")))?;
    let response_headers = pagination_headers(page.has_more, page.next_cursor.as_deref())?;

    Ok((
        response_headers,
        Json(
            page.records
                .into_iter()
                .map(ScheduledTaskDto::from)
                .collect(),
        ),
    ))
}

pub async fn list_scheduled_task_runs(
    State(state): State<AppState>,
    Path(task_key): Path<String>,
    Query(query): Query<ScheduledTaskRunQueryDto>,
    headers: HeaderMap,
    uri: Uri,
) -> Result<(HeaderMap, Json<Vec<ScheduledTaskRunHistoryDto>>), AppError> {
    authenticate_admin(&state, &headers, &uri).await?;
    let task_key = validate_scheduled_task_key(&task_key)?;
    let status = query
        .status
        .as_deref()
        .map(validate_scheduled_task_run_status)
        .transpose()?
        .map(str::to_owned);
    let cursor = query
        .cursor
        .as_deref()
        .map(|value| validate_uuid_public_id("cursor", value))
        .transpose()?
        .map(str::to_owned);
    let Some(database) = state.database() else {
        return Err(AppError::internal("database is not configured"));
    };
    let Some(page) = AdminRepository::new(database.clone())
        .list_scheduled_task_runs_page(
            task_key,
            ScheduledTaskRunFilter {
                status,
                cursor,
                limit: scheduled_task_run_list_limit(query.limit),
            },
        )
        .await
        .map_err(|err| AppError::internal(format!("failed to list scheduled task runs: {err}")))?
    else {
        return Err(AppError::not_found("scheduled task not found"));
    };
    let response_headers = pagination_headers(page.has_more, page.next_cursor.as_deref())?;

    Ok((
        response_headers,
        Json(
            page.records
                .into_iter()
                .map(ScheduledTaskRunHistoryDto::from)
                .collect(),
        ),
    ))
}

pub async fn run_scheduled_task(
    State(state): State<AppState>,
    Path(task_key): Path<String>,
    headers: HeaderMap,
    uri: Uri,
) -> Result<Json<ScheduledTaskRunDto>, AppError> {
    authenticate_admin(&state, &headers, &uri).await?;
    let task_key = validate_scheduled_task_key(&task_key)?;
    let Some(database) = state.database() else {
        return Err(AppError::internal("database is not configured"));
    };
    let summary =
        SchedulerService::with_worker_id(database.clone(), default_worker_id("admin-manual"))
            .run_task_once(task_key)
            .await
            .map_err(scheduler_error_to_app_error)?;

    Ok(Json(ScheduledTaskRunDto::from(summary)))
}

pub async fn list_transcoding_sessions(
    State(state): State<AppState>,
    Query(query): Query<TranscodeSessionQueryDto>,
    headers: HeaderMap,
    uri: Uri,
) -> Result<(HeaderMap, Json<Vec<TranscodeSessionDto>>), AppError> {
    authenticate_admin(&state, &headers, &uri).await?;
    let status = query
        .status
        .as_deref()
        .map(validate_transcode_session_status)
        .transpose()?
        .map(str::to_owned);
    let hardware_acceleration = query
        .hardware_acceleration
        .as_deref()
        .map(validate_transcode_hardware_acceleration)
        .transpose()?
        .map(str::to_owned);
    let cursor = query
        .cursor
        .as_deref()
        .map(|value| validate_uuid_public_id("cursor", value))
        .transpose()?
        .map(str::to_owned);
    let Some(database) = state.database() else {
        return Err(AppError::internal("database is not configured"));
    };
    let page = TranscodeRepository::new(database.clone())
        .list_sessions_page(TranscodeSessionFilter {
            status,
            hardware_acceleration,
            cursor,
            limit: transcode_session_list_limit(query.limit),
        })
        .await
        .map_err(|err| AppError::internal(format!("failed to list transcoding sessions: {err}")))?;
    let response_headers = pagination_headers(page.has_more, page.next_cursor.as_deref())?;

    Ok((
        response_headers,
        Json(
            page.records
                .into_iter()
                .map(TranscodeSessionDto::from)
                .collect(),
        ),
    ))
}

pub async fn cancel_transcoding_session(
    State(state): State<AppState>,
    Path(session_id): Path<String>,
    headers: HeaderMap,
    uri: Uri,
) -> Result<Json<TranscodeSessionDto>, AppError> {
    authenticate_admin(&state, &headers, &uri).await?;
    let session_id = validate_public_id("sessionId", &session_id)?;
    let Some(database) = state.database() else {
        return Err(AppError::internal("database is not configured"));
    };
    let repository = TranscodeRepository::new(database.clone());
    if let Some(session) = repository
        .cancel_session(session_id)
        .await
        .map_err(|err| AppError::internal(format!("failed to cancel transcoding session: {err}")))?
    {
        return Ok(Json(TranscodeSessionDto::from(session)));
    }

    let exists = repository.session_exists(session_id).await.map_err(|err| {
        AppError::internal(format!("failed to check transcoding session status: {err}"))
    })?;
    if exists {
        return Err(AppError::conflict(
            "transcoding session cannot be cancelled from its current status",
        ));
    }

    Err(AppError::not_found("transcoding session not found"))
}

pub async fn list_notification_targets(
    State(state): State<AppState>,
    Query(query): Query<NotificationTargetQueryDto>,
    headers: HeaderMap,
    uri: Uri,
) -> Result<(HeaderMap, Json<Vec<NotificationTargetDto>>), AppError> {
    authenticate_admin(&state, &headers, &uri).await?;
    let target_type = query
        .target_type
        .as_deref()
        .map(normalize_notification_target_type)
        .map(|value| validate_notification_target_type(&value).map(|_| value))
        .transpose()?;
    let channel = query
        .channel
        .as_deref()
        .map(validate_notification_channel)
        .transpose()?
        .map(str::to_owned);
    let cursor = query
        .cursor
        .as_deref()
        .map(|value| validate_uuid_public_id("cursor", value))
        .transpose()?
        .map(str::to_owned);
    let Some(database) = state.database() else {
        return Err(AppError::internal("database is not configured"));
    };
    let page = AdminRepository::new(database.clone())
        .list_notification_targets_page(NotificationTargetFilter {
            target_type,
            channel,
            is_enabled: query.enabled,
            cursor,
            limit: notification_target_list_limit(query.limit),
        })
        .await
        .map_err(|err| AppError::internal(format!("failed to list notification targets: {err}")))?;
    let response_headers = pagination_headers(page.has_more, page.next_cursor.as_deref())?;

    Ok((
        response_headers,
        Json(
            page.records
                .into_iter()
                .map(NotificationTargetDto::from)
                .collect(),
        ),
    ))
}

pub async fn list_notification_requests(
    State(state): State<AppState>,
    Query(query): Query<NotificationRequestQueryDto>,
    headers: HeaderMap,
    uri: Uri,
) -> Result<(HeaderMap, Json<Vec<NotificationRequestDto>>), AppError> {
    authenticate_admin(&state, &headers, &uri).await?;
    let status = query
        .status
        .as_deref()
        .map(validate_notification_request_status)
        .transpose()?
        .map(str::to_owned);
    let channel = query
        .channel
        .as_deref()
        .map(validate_notification_channel)
        .transpose()?
        .map(str::to_owned);
    let cursor = query
        .cursor
        .as_deref()
        .map(|value| validate_uuid_public_id("cursor", value))
        .transpose()?
        .map(str::to_owned);
    let Some(database) = state.database() else {
        return Err(AppError::internal("database is not configured"));
    };
    let page = AdminRepository::new(database.clone())
        .list_notification_requests_page(NotificationRequestFilter {
            status,
            channel,
            cursor,
            limit: notification_request_list_limit(query.limit),
        })
        .await
        .map_err(|err| {
            AppError::internal(format!("failed to list notification requests: {err}"))
        })?;
    let response_headers = pagination_headers(page.has_more, page.next_cursor.as_deref())?;

    Ok((
        response_headers,
        Json(
            page.records
                .into_iter()
                .map(NotificationRequestDto::from)
                .collect(),
        ),
    ))
}

pub async fn list_notification_delivery_attempts(
    State(state): State<AppState>,
    Path(request_id): Path<String>,
    Query(query): Query<NotificationDeliveryAttemptQueryDto>,
    headers: HeaderMap,
    uri: Uri,
) -> Result<(HeaderMap, Json<Vec<NotificationDeliveryAttemptDto>>), AppError> {
    authenticate_admin(&state, &headers, &uri).await?;
    let request_id = validate_public_id("requestId", &request_id)?;
    let status = query
        .status
        .as_deref()
        .map(validate_notification_delivery_attempt_status)
        .transpose()?
        .map(str::to_owned);
    let cursor = query
        .cursor
        .as_deref()
        .map(|value| validate_uuid_public_id("cursor", value))
        .transpose()?
        .map(str::to_owned);
    let Some(database) = state.database() else {
        return Err(AppError::internal("database is not configured"));
    };
    let Some(page) = AdminRepository::new(database.clone())
        .list_notification_delivery_attempts_page(
            request_id,
            NotificationDeliveryAttemptFilter {
                status,
                cursor,
                limit: notification_attempt_list_limit(query.limit),
            },
        )
        .await
        .map_err(|err| {
            AppError::internal(format!(
                "failed to list notification delivery attempts: {err}"
            ))
        })?
    else {
        return Err(AppError::not_found("notification request not found"));
    };
    let response_headers = pagination_headers(page.has_more, page.next_cursor.as_deref())?;

    Ok((
        response_headers,
        Json(
            page.records
                .into_iter()
                .map(NotificationDeliveryAttemptDto::from)
                .collect(),
        ),
    ))
}

pub async fn retry_notification_request(
    State(state): State<AppState>,
    Path(request_id): Path<String>,
    headers: HeaderMap,
    uri: Uri,
) -> Result<Json<NotificationRequestDto>, AppError> {
    authenticate_admin(&state, &headers, &uri).await?;
    let request_id = validate_public_id("requestId", &request_id)?;
    let Some(database) = state.database() else {
        return Err(AppError::internal("database is not configured"));
    };
    let request = AdminRepository::new(database.clone())
        .retry_notification_request(request_id)
        .await
        .map_err(notification_retry_error_to_app_error)?;

    Ok(Json(NotificationRequestDto::from(request)))
}

pub async fn list_plugin_dispatches(
    State(state): State<AppState>,
    Query(query): Query<PluginDispatchQueryDto>,
    headers: HeaderMap,
    uri: Uri,
) -> Result<(HeaderMap, Json<Vec<PluginDispatchDto>>), AppError> {
    authenticate_admin(&state, &headers, &uri).await?;
    let status = query
        .status
        .as_deref()
        .map(validate_event_outbox_status)
        .transpose()?
        .map(str::to_owned);
    let cursor = query
        .cursor
        .as_deref()
        .map(|value| validate_uuid_public_id("cursor", value))
        .transpose()?
        .map(str::to_owned);
    let Some(database) = state.database() else {
        return Err(AppError::internal("database is not configured"));
    };
    let page = AdminRepository::new(database.clone())
        .list_plugin_dispatches_page(PluginDispatchFilter {
            status,
            cursor,
            limit: plugin_dispatch_list_limit(query.limit),
        })
        .await
        .map_err(|err| AppError::internal(format!("failed to list plugin dispatches: {err}")))?;

    let response_headers = pagination_headers(page.has_more, page.next_cursor.as_deref())?;

    Ok((
        response_headers,
        Json(
            page.records
                .into_iter()
                .map(PluginDispatchDto::from)
                .collect(),
        ),
    ))
}

pub async fn list_plugin_execution_runs(
    State(state): State<AppState>,
    Path(dispatch_id): Path<String>,
    Query(query): Query<PluginExecutionRunQueryDto>,
    headers: HeaderMap,
    uri: Uri,
) -> Result<(HeaderMap, Json<Vec<PluginExecutionRunDto>>), AppError> {
    authenticate_admin(&state, &headers, &uri).await?;
    let dispatch_id = validate_public_id("dispatchId", &dispatch_id)?;
    let status = query
        .status
        .as_deref()
        .map(validate_plugin_execution_run_status)
        .transpose()?
        .map(str::to_owned);
    let cursor = query
        .cursor
        .as_deref()
        .map(|value| validate_uuid_public_id("cursor", value))
        .transpose()?
        .map(str::to_owned);
    let Some(database) = state.database() else {
        return Err(AppError::internal("database is not configured"));
    };
    let Some(page) = AdminRepository::new(database.clone())
        .list_plugin_execution_runs_page(
            dispatch_id,
            PluginExecutionRunFilter {
                status,
                cursor,
                limit: plugin_execution_run_list_limit(query.limit),
            },
        )
        .await
        .map_err(|err| {
            AppError::internal(format!("failed to list plugin execution runs: {err}"))
        })?
    else {
        return Err(AppError::not_found("plugin dispatch not found"));
    };
    let response_headers = pagination_headers(page.has_more, page.next_cursor.as_deref())?;

    Ok((
        response_headers,
        Json(
            page.records
                .into_iter()
                .map(PluginExecutionRunDto::from)
                .collect(),
        ),
    ))
}

pub async fn list_plugin_host_api_calls(
    State(state): State<AppState>,
    Query(query): Query<PluginHostApiCallQueryDto>,
    headers: HeaderMap,
    uri: Uri,
) -> Result<(HeaderMap, Json<Vec<PluginHostApiCallDto>>), AppError> {
    authenticate_admin(&state, &headers, &uri).await?;
    let plugin_id = query
        .plugin_id
        .as_deref()
        .map(|value| validate_public_id("pluginId", value))
        .transpose()?
        .map(str::to_owned);
    let execution_run_id = query
        .execution_run_id
        .as_deref()
        .map(|value| validate_public_id("executionRunId", value))
        .transpose()?
        .map(str::to_owned);
    let status_code = query
        .status_code
        .map(validate_http_status_code)
        .transpose()?;
    let cursor = query
        .cursor
        .as_deref()
        .map(|value| validate_uuid_public_id("cursor", value))
        .transpose()?
        .map(str::to_owned);
    let Some(database) = state.database() else {
        return Err(AppError::internal("database is not configured"));
    };
    let page = AdminRepository::new(database.clone())
        .list_plugin_host_api_calls_page(PluginHostApiCallFilter {
            plugin_id,
            execution_run_id,
            status_code,
            cursor,
            limit: plugin_host_api_call_limit(query.limit),
        })
        .await
        .map_err(|err| {
            AppError::internal(format!("failed to list plugin host api calls: {err}"))
        })?;
    let response_headers = pagination_headers(page.has_more, page.next_cursor.as_deref())?;

    Ok((
        response_headers,
        Json(
            page.records
                .into_iter()
                .map(PluginHostApiCallDto::from)
                .collect(),
        ),
    ))
}

pub async fn list_plugin_host_api_calls_for_run(
    State(state): State<AppState>,
    Path(run_id): Path<String>,
    Query(query): Query<PluginHostApiCallRunQueryDto>,
    headers: HeaderMap,
    uri: Uri,
) -> Result<(HeaderMap, Json<Vec<PluginHostApiCallDto>>), AppError> {
    authenticate_admin(&state, &headers, &uri).await?;
    let run_id = validate_public_id("runId", &run_id)?;
    let cursor = query
        .cursor
        .as_deref()
        .map(|value| validate_uuid_public_id("cursor", value))
        .transpose()?
        .map(str::to_owned);
    let Some(database) = state.database() else {
        return Err(AppError::internal("database is not configured"));
    };
    let Some(page) = AdminRepository::new(database.clone())
        .list_plugin_host_api_calls_for_run_page(
            run_id,
            cursor,
            plugin_host_api_call_limit(query.limit),
        )
        .await
        .map_err(|err| {
            AppError::internal(format!(
                "failed to list plugin host api calls for run: {err}"
            ))
        })?
    else {
        return Err(AppError::not_found("plugin execution run not found"));
    };
    let response_headers = pagination_headers(page.has_more, page.next_cursor.as_deref())?;

    Ok((
        response_headers,
        Json(
            page.records
                .into_iter()
                .map(PluginHostApiCallDto::from)
                .collect(),
        ),
    ))
}

pub async fn event_stream_mirror_status(
    State(state): State<AppState>,
    headers: HeaderMap,
    uri: Uri,
) -> Result<Json<EventStreamMirrorStatusDto>, AppError> {
    authenticate_admin(&state, &headers, &uri).await?;
    let Some(database) = state.database() else {
        return Err(AppError::internal("database is not configured"));
    };
    let status = AdminRepository::new(database.clone())
        .event_stream_mirror_status()
        .await
        .map_err(|err| {
            AppError::internal(format!("failed to load event stream mirror status: {err}"))
        })?;

    Ok(Json(EventStreamMirrorStatusDto::from_config_and_record(
        state.config(),
        status,
    )))
}

pub async fn replay_plugin_dispatch(
    State(state): State<AppState>,
    Path(dispatch_id): Path<String>,
    headers: HeaderMap,
    uri: Uri,
) -> Result<Json<PluginDispatchDto>, AppError> {
    authenticate_admin(&state, &headers, &uri).await?;
    let dispatch_id = validate_public_id("dispatchId", &dispatch_id)?;
    let Some(database) = state.database() else {
        return Err(AppError::internal("database is not configured"));
    };
    let dispatch = AdminRepository::new(database.clone())
        .replay_plugin_dispatch(dispatch_id)
        .await
        .map_err(plugin_dispatch_replay_error_to_app_error)?;

    Ok(Json(PluginDispatchDto::from(dispatch)))
}

pub async fn create_notification_target(
    State(state): State<AppState>,
    headers: HeaderMap,
    uri: Uri,
    Json(payload): Json<UpsertNotificationTargetRequestDto>,
) -> Result<(StatusCode, Json<NotificationTargetDto>), AppError> {
    authenticate_admin(&state, &headers, &uri).await?;
    let input = NotificationTargetInput::try_from(payload)?;
    let cipher = secret_cipher_from_state(&state)?;
    let Some(database) = state.database() else {
        return Err(AppError::internal("database is not configured"));
    };
    let target = AdminRepository::new(database.clone())
        .create_notification_target(input.into(), &cipher)
        .await
        .map_err(|err| {
            AppError::internal(format!("failed to create notification target: {err}"))
        })?;

    Ok((
        StatusCode::CREATED,
        Json(NotificationTargetDto::from(target)),
    ))
}

pub async fn replace_notification_target(
    State(state): State<AppState>,
    Path(target_id): Path<String>,
    headers: HeaderMap,
    uri: Uri,
    Json(payload): Json<UpsertNotificationTargetRequestDto>,
) -> Result<Json<NotificationTargetDto>, AppError> {
    authenticate_admin(&state, &headers, &uri).await?;
    let target_id = validate_public_id("targetId", &target_id)?;
    let input = NotificationTargetInput::try_from(payload)?;
    let cipher = secret_cipher_from_state(&state)?;
    let Some(database) = state.database() else {
        return Err(AppError::internal("database is not configured"));
    };
    let Some(target) = AdminRepository::new(database.clone())
        .replace_notification_target(target_id, input.into(), &cipher)
        .await
        .map_err(|err| {
            AppError::internal(format!("failed to replace notification target: {err}"))
        })?
    else {
        return Err(AppError::not_found("notification target not found"));
    };

    Ok(Json(NotificationTargetDto::from(target)))
}

pub async fn enable_notification_target(
    State(state): State<AppState>,
    Path(target_id): Path<String>,
    headers: HeaderMap,
    uri: Uri,
) -> Result<Json<NotificationTargetDto>, AppError> {
    set_notification_target_enabled(state, target_id, headers, uri, true).await
}

pub async fn disable_notification_target(
    State(state): State<AppState>,
    Path(target_id): Path<String>,
    headers: HeaderMap,
    uri: Uri,
) -> Result<Json<NotificationTargetDto>, AppError> {
    set_notification_target_enabled(state, target_id, headers, uri, false).await
}

fn secret_cipher_from_state(state: &AppState) -> Result<SecretCipher, AppError> {
    SecretCipher::from_config(&state.config().secrets)
        .map_err(|err| AppError::unprocessable(err.to_string()))
}

async fn set_notification_target_enabled(
    state: AppState,
    target_id: String,
    headers: HeaderMap,
    uri: Uri,
    is_enabled: bool,
) -> Result<Json<NotificationTargetDto>, AppError> {
    authenticate_admin(&state, &headers, &uri).await?;
    let target_id = validate_public_id("targetId", &target_id)?;
    let Some(database) = state.database() else {
        return Err(AppError::internal("database is not configured"));
    };
    let Some(target) = AdminRepository::new(database.clone())
        .set_notification_target_enabled(target_id, is_enabled)
        .await
        .map_err(|err| {
            AppError::internal(format!("failed to update notification target: {err}"))
        })?
    else {
        return Err(AppError::not_found("notification target not found"));
    };

    Ok(Json(NotificationTargetDto::from(target)))
}

fn validate_non_empty(field: &str, value: &str) -> Result<(), AppError> {
    if value.trim().is_empty() {
        return Err(AppError::unprocessable(format!("{field} is required")));
    }

    Ok(())
}

fn validate_bounded_text<'a>(
    field: &str,
    value: &'a str,
    max_len: usize,
) -> Result<&'a str, AppError> {
    let value = value.trim();
    if value.is_empty() {
        return Err(AppError::unprocessable(format!("{field} is required")));
    }
    if value.len() > max_len {
        return Err(AppError::unprocessable(format!(
            "{field} must be at most {max_len} characters"
        )));
    }
    Ok(value)
}

fn validate_optional_bounded_text<'a>(
    field: &str,
    value: &'a str,
    max_len: usize,
) -> Result<Option<&'a str>, AppError> {
    let value = value.trim();
    if value.is_empty() {
        return Ok(None);
    }
    if value.len() > max_len {
        return Err(AppError::unprocessable(format!(
            "{field} must be at most {max_len} characters"
        )));
    }
    Ok(Some(value))
}

fn validate_library_type(value: &str) -> Result<(), AppError> {
    match value {
        "movies" | "tv" | "music" | "mixed" => Ok(()),
        _ => Err(AppError::unprocessable(
            "libraryType must be one of movies, tv, music, mixed",
        )),
    }
}

fn validate_notification_target_type(value: &str) -> Result<(), AppError> {
    match value {
        "telegram" | "wecom" | "webhook" => Ok(()),
        _ => Err(AppError::unprocessable(
            "targetType must be one of telegram, wecom, webhook",
        )),
    }
}

fn validate_notification_channel(value: &str) -> Result<&str, AppError> {
    let channel = validate_bounded_text("channel", value, MAX_NOTIFICATION_TARGET_CHANNEL_LEN)?;
    if !channel
        .bytes()
        .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'-' | b'_' | b':'))
    {
        return Err(AppError::unprocessable(
            "channel may only contain letters, numbers, dot, dash, underscore, or colon",
        ));
    }
    Ok(channel)
}

fn validate_public_id<'a>(field: &str, value: &'a str) -> Result<&'a str, AppError> {
    let value = validate_bounded_text(field, value, 128)?;
    if value.contains(char::is_whitespace) {
        return Err(AppError::unprocessable(format!(
            "{field} must not contain whitespace"
        )));
    }
    Ok(value)
}

fn validate_uuid_public_id<'a>(field: &str, value: &'a str) -> Result<&'a str, AppError> {
    let value = validate_public_id(field, value)?;
    let bytes = value.as_bytes();
    let has_uuid_shape = bytes.len() == 36
        && [8, 13, 18, 23]
            .into_iter()
            .all(|index| bytes[index] == b'-')
        && bytes
            .iter()
            .enumerate()
            .all(|(index, byte)| matches!(index, 8 | 13 | 18 | 23) || byte.is_ascii_hexdigit());
    if !has_uuid_shape {
        return Err(AppError::unprocessable(format!(
            "{field} must be a UUID public id"
        )));
    }
    Ok(value)
}

fn validate_scheduled_task_key(value: &str) -> Result<&str, AppError> {
    let value = validate_bounded_text("taskKey", value, MAX_SCHEDULED_TASK_KEY_LEN)?;
    if value.contains(char::is_whitespace) || value.contains('/') || value.contains('\\') {
        return Err(AppError::unprocessable(
            "taskKey must not contain whitespace or path separators",
        ));
    }
    Ok(value)
}

fn validate_scheduled_task_filter_text(value: &str) -> Result<&str, AppError> {
    validate_admin_job_filter_text("taskType", value)
}

fn validate_scheduled_task_owner_type(value: &str) -> Result<&str, AppError> {
    let value = validate_bounded_text("ownerType", value, 32)?;
    if matches!(value, "core" | "plugin") {
        return Ok(value);
    }

    Err(AppError::unprocessable(
        "ownerType must be one of core or plugin",
    ))
}

fn metadata_refresh_limit(value: Option<i64>) -> i64 {
    value
        .unwrap_or(DEFAULT_LIBRARY_METADATA_REFRESH_LIMIT)
        .clamp(1, MAX_LIBRARY_METADATA_REFRESH_LIMIT)
}

fn notification_target_list_limit(value: Option<i64>) -> i64 {
    value
        .unwrap_or(MAX_NOTIFICATION_TARGETS_LIST_LIMIT)
        .clamp(1, MAX_NOTIFICATION_TARGETS_LIST_LIMIT)
}

fn plugin_host_api_call_limit(value: Option<i64>) -> i64 {
    value
        .unwrap_or(MAX_PLUGIN_HOST_API_CALLS_LIST_LIMIT)
        .clamp(1, MAX_PLUGIN_HOST_API_CALLS_LIST_LIMIT)
}

fn plugin_dispatch_list_limit(value: Option<i64>) -> i64 {
    value
        .unwrap_or(MAX_PLUGIN_DISPATCHES_LIST_LIMIT)
        .clamp(1, MAX_PLUGIN_DISPATCHES_LIST_LIMIT)
}

fn plugin_execution_run_list_limit(value: Option<i64>) -> i64 {
    value
        .unwrap_or(MAX_PLUGIN_EXECUTION_RUNS_LIST_LIMIT)
        .clamp(1, MAX_PLUGIN_EXECUTION_RUNS_LIST_LIMIT)
}

fn admin_job_list_limit(value: Option<i64>) -> i64 {
    value
        .unwrap_or(MAX_ADMIN_JOBS_LIST_LIMIT)
        .clamp(1, MAX_ADMIN_JOBS_LIST_LIMIT)
}

fn admin_user_list_limit(value: Option<i64>) -> i64 {
    value
        .unwrap_or(MAX_ADMIN_USERS_LIST_LIMIT)
        .clamp(1, MAX_ADMIN_USERS_LIST_LIMIT)
}

fn admin_user_library_permission_list_limit(value: Option<i64>) -> i64 {
    value
        .unwrap_or(MAX_ADMIN_USER_LIBRARY_PERMISSIONS_LIMIT)
        .clamp(1, MAX_ADMIN_USER_LIBRARY_PERMISSIONS_LIMIT)
}

fn admin_job_run_list_limit(value: Option<i64>) -> i64 {
    value
        .unwrap_or(MAX_ADMIN_JOB_RUNS_LIST_LIMIT)
        .clamp(1, MAX_ADMIN_JOB_RUNS_LIST_LIMIT)
}

fn admin_job_event_list_limit(value: Option<i64>) -> i64 {
    value
        .unwrap_or(MAX_ADMIN_JOB_EVENTS_LIST_LIMIT)
        .clamp(1, MAX_ADMIN_JOB_EVENTS_LIST_LIMIT)
}

fn scheduled_task_list_limit(value: Option<i64>) -> i64 {
    value
        .unwrap_or(MAX_SCHEDULED_TASKS_LIST_LIMIT)
        .clamp(1, MAX_SCHEDULED_TASKS_LIST_LIMIT)
}

fn scheduled_task_run_list_limit(value: Option<i64>) -> i64 {
    value
        .unwrap_or(MAX_SCHEDULED_TASK_RUNS_LIST_LIMIT)
        .clamp(1, MAX_SCHEDULED_TASK_RUNS_LIST_LIMIT)
}

fn transcode_session_list_limit(value: Option<i64>) -> i64 {
    value
        .unwrap_or(MAX_TRANSCODE_SESSIONS_LIST_LIMIT)
        .clamp(1, MAX_TRANSCODE_SESSIONS_LIST_LIMIT)
}

fn notification_request_list_limit(value: Option<i64>) -> i64 {
    value
        .unwrap_or(MAX_NOTIFICATION_REQUESTS_LIST_LIMIT)
        .clamp(1, MAX_NOTIFICATION_REQUESTS_LIST_LIMIT)
}

fn notification_attempt_list_limit(value: Option<i64>) -> i64 {
    value
        .unwrap_or(MAX_NOTIFICATION_ATTEMPTS_LIST_LIMIT)
        .clamp(1, MAX_NOTIFICATION_ATTEMPTS_LIST_LIMIT)
}

fn validate_notification_request_status(value: &str) -> Result<&str, AppError> {
    let value = validate_bounded_text("status", value, 32)?;
    if matches!(
        value,
        "queued" | "delivering" | "delivered" | "failed" | "discarded"
    ) {
        return Ok(value);
    }

    Err(AppError::unprocessable(
        "status must be one of queued, delivering, delivered, failed, or discarded",
    ))
}

fn validate_notification_delivery_attempt_status(value: &str) -> Result<&str, AppError> {
    let value = validate_bounded_text("status", value, 32)?;
    if matches!(value, "running" | "succeeded" | "failed" | "skipped") {
        return Ok(value);
    }

    Err(AppError::unprocessable(
        "status must be one of running, succeeded, failed, or skipped",
    ))
}

fn validate_event_outbox_status(value: &str) -> Result<&str, AppError> {
    let value = validate_bounded_text("status", value, 32)?;
    if matches!(
        value,
        "pending" | "delivering" | "delivered" | "failed" | "discarded"
    ) {
        return Ok(value);
    }

    Err(AppError::unprocessable(
        "status must be one of pending, delivering, delivered, failed, or discarded",
    ))
}

fn validate_plugin_execution_run_status(value: &str) -> Result<&str, AppError> {
    let value = validate_bounded_text("status", value, 32)?;
    if matches!(value, "running" | "succeeded" | "failed") {
        return Ok(value);
    }

    Err(AppError::unprocessable(
        "status must be one of running, succeeded, or failed",
    ))
}

fn validate_admin_job_run_status(value: &str) -> Result<&str, AppError> {
    let value = validate_bounded_text("status", value, 32)?;
    if matches!(value, "running" | "succeeded" | "failed" | "cancelled") {
        return Ok(value);
    }

    Err(AppError::unprocessable(
        "status must be one of running, succeeded, failed, or cancelled",
    ))
}

fn validate_admin_job_status(value: &str) -> Result<&str, AppError> {
    let value = validate_bounded_text("status", value, 32)?;
    if matches!(
        value,
        "queued" | "running" | "succeeded" | "failed" | "cancelled"
    ) {
        return Ok(value);
    }

    Err(AppError::unprocessable(
        "status must be one of queued, running, succeeded, failed, or cancelled",
    ))
}

fn validate_admin_job_filter_text<'a>(field: &str, value: &'a str) -> Result<&'a str, AppError> {
    let value = validate_bounded_text(field, value, MAX_ADMIN_JOB_FILTER_TEXT_LEN)?;
    if !value
        .bytes()
        .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'-' | b'_' | b':'))
    {
        return Err(AppError::unprocessable(format!(
            "{field} may only contain letters, numbers, dot, dash, underscore, or colon"
        )));
    }
    Ok(value)
}

fn validate_admin_user_role_name(value: &str) -> Result<&str, AppError> {
    let value = validate_bounded_text("roleName", value, MAX_ADMIN_USER_ROLE_NAME_LEN)?;
    if !value
        .bytes()
        .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'-' | b'_'))
    {
        return Err(AppError::unprocessable(
            "roleName may only contain letters, numbers, dot, dash, or underscore",
        ));
    }
    Ok(value)
}

fn validate_admin_job_event_level(value: &str) -> Result<&str, AppError> {
    let value = validate_bounded_text("eventLevel", value, 32)?;
    if matches!(value, "debug" | "info" | "warn" | "error") {
        return Ok(value);
    }

    Err(AppError::unprocessable(
        "eventLevel must be one of debug, info, warn, or error",
    ))
}

fn validate_scheduled_task_run_status(value: &str) -> Result<&str, AppError> {
    let value = validate_bounded_text("status", value, 32)?;
    if matches!(value, "running" | "succeeded" | "failed" | "expired") {
        return Ok(value);
    }

    Err(AppError::unprocessable(
        "status must be one of running, succeeded, failed, or expired",
    ))
}

fn validate_transcode_session_status(value: &str) -> Result<&str, AppError> {
    let value = validate_bounded_text("status", value, 32)?;
    if matches!(
        value,
        "queued" | "running" | "succeeded" | "failed" | "cancelled"
    ) {
        return Ok(value);
    }

    Err(AppError::unprocessable(
        "status must be one of queued, running, succeeded, failed, or cancelled",
    ))
}

fn validate_transcode_hardware_acceleration(value: &str) -> Result<&str, AppError> {
    let value = validate_bounded_text(
        "hardwareAcceleration",
        value,
        MAX_TRANSCODE_HARDWARE_ACCELERATION_LEN,
    )?;
    if !value
        .bytes()
        .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'-' | b'_'))
    {
        return Err(AppError::unprocessable(
            "hardwareAcceleration may only contain letters, numbers, dot, dash, or underscore",
        ));
    }
    Ok(value)
}

fn validate_positive_i64_cursor(field: &str, value: &str) -> Result<i64, AppError> {
    let value = validate_bounded_text(field, value, 32)?;
    let cursor = value.parse::<i64>().map_err(|_| {
        AppError::unprocessable(format!("{field} must be a positive integer cursor"))
    })?;
    if cursor <= 0 {
        return Err(AppError::unprocessable(format!(
            "{field} must be a positive integer cursor"
        )));
    }
    Ok(cursor)
}

fn pagination_headers(has_more: bool, next_cursor: Option<&str>) -> Result<HeaderMap, AppError> {
    let mut headers = HeaderMap::new();
    headers.insert(
        "x-fbz-has-more",
        HeaderValue::from_static(if has_more { "true" } else { "false" }),
    );
    if let Some(next_cursor) = next_cursor {
        headers.insert(
            "x-fbz-next-cursor",
            HeaderValue::from_str(next_cursor).map_err(|err| {
                AppError::internal(format!("failed to encode next cursor header: {err}"))
            })?,
        );
    }

    Ok(headers)
}

fn validate_http_status_code(value: i32) -> Result<i32, AppError> {
    if (100..=599).contains(&value) {
        return Ok(value);
    }

    Err(AppError::unprocessable(
        "statusCode must be between 100 and 599",
    ))
}

fn metadata_provider_status_response(config: &Config) -> MetadataProviderStatusResponseDto {
    MetadataProviderStatusResponseDto {
        providers: metadata_provider_statuses(&config.metadata),
        proxy_policy: config.proxy.policy.clone(),
        http_proxy_configured: config.proxy.http_proxy.is_some(),
        https_proxy_configured: config.proxy.https_proxy.is_some(),
    }
}

fn metadata_provider_statuses(metadata: &MetadataConfig) -> Vec<MetadataProviderStatusDto> {
    ["tmdb", "tvdb", "fanart"]
        .into_iter()
        .map(|provider| metadata_provider_status(metadata, provider))
        .collect()
}

fn metadata_provider_status(
    metadata: &MetadataConfig,
    provider: &str,
) -> MetadataProviderStatusDto {
    let enabled = metadata
        .providers
        .iter()
        .any(|item| item.trim().eq_ignore_ascii_case(provider));

    match provider {
        "tmdb" => MetadataProviderStatusDto {
            provider: provider.to_owned(),
            enabled,
            search_supported: true,
            credential_configured: metadata.tmdb_access_token.is_some(),
            api_base_url: metadata.tmdb_api_base_url.clone(),
            image_base_url: Some(metadata.tmdb_image_base_url.clone()),
        },
        "tvdb" => MetadataProviderStatusDto {
            provider: provider.to_owned(),
            enabled,
            search_supported: false,
            credential_configured: metadata.tvdb_api_key.is_some(),
            api_base_url: metadata.tvdb_api_base_url.clone(),
            image_base_url: None,
        },
        "fanart" => MetadataProviderStatusDto {
            provider: provider.to_owned(),
            enabled,
            search_supported: false,
            credential_configured: metadata.fanart_api_key.is_some(),
            api_base_url: metadata.fanart_api_base_url.clone(),
            image_base_url: None,
        },
        _ => MetadataProviderStatusDto {
            provider: provider.to_owned(),
            enabled: false,
            search_supported: false,
            credential_configured: false,
            api_base_url: String::new(),
            image_base_url: None,
        },
    }
}

fn normalize_library_type(value: &str) -> String {
    value.trim().to_ascii_lowercase()
}

fn normalize_notification_target_type(value: &str) -> String {
    value.trim().to_ascii_lowercase()
}

impl From<ManagedLibraryRecord> for ManagedLibraryDto {
    fn from(record: ManagedLibraryRecord) -> Self {
        Self {
            id: record.id,
            name: record.name,
            library_type: record.library_type,
        }
    }
}

impl From<LibraryPathRecord> for LibraryPathDto {
    fn from(record: LibraryPathRecord) -> Self {
        Self {
            id: record.id,
            library_id: record.library_id,
            path: record.path,
            is_enabled: record.is_enabled,
        }
    }
}

impl From<ScanJobRecord> for ScanJobDto {
    fn from(record: ScanJobRecord) -> Self {
        Self {
            id: record.id,
            status: record.status,
            queue_name: record.queue_name,
            job_type: record.job_type,
        }
    }
}

impl From<MetadataRefreshJobRecord> for MetadataRefreshJobDto {
    fn from(record: MetadataRefreshJobRecord) -> Self {
        Self {
            id: record.id,
            status: record.status,
            queue_name: record.queue_name,
            job_type: record.job_type,
            item_id: record.item_id,
        }
    }
}

impl From<LibraryMetadataRefreshQueueRecord> for LibraryMetadataRefreshQueueDto {
    fn from(record: LibraryMetadataRefreshQueueRecord) -> Self {
        Self {
            library_id: record.library_id,
            queued_jobs: record.queued_jobs,
        }
    }
}

impl From<AdminUserRecord> for AdminUserDto {
    fn from(record: AdminUserRecord) -> Self {
        Self {
            id: record.id,
            username: record.username,
            display_name: record.display_name,
            role_name: record.role_name,
            is_disabled: record.is_disabled,
            allow_download: record.allow_download,
            allow_transcode: record.allow_transcode,
            allow_new_device_login: record.allow_new_device_login,
            has_password: record.has_password,
            device_count: record.device_count,
            active_session_count: record.active_session_count,
            last_login_at: record.last_login_at,
            created_at: record.created_at,
            updated_at: record.updated_at,
        }
    }
}

impl From<AdminUserLibraryPermissionRecord> for AdminUserLibraryPermissionDto {
    fn from(record: AdminUserLibraryPermissionRecord) -> Self {
        Self {
            library_id: record.library_id,
            library_name: record.library_name,
            library_type: record.library_type,
            is_hidden: record.is_hidden,
            permission_configured: record.permission_configured,
            can_view: record.can_view,
            can_download: record.can_download,
            can_transcode: record.can_transcode,
            effective_can_view: record.effective_can_view,
            effective_can_download: record.effective_can_download,
            effective_can_transcode: record.effective_can_transcode,
            permission_updated_at: record.permission_updated_at,
        }
    }
}

impl From<AdminJobRecord> for AdminJobDto {
    fn from(record: AdminJobRecord) -> Self {
        Self {
            id: record.id,
            job_type: record.job_type,
            status: record.status,
            queue_name: record.queue_name,
            priority: record.priority,
            payload: record.payload,
            dedupe_key: record.dedupe_key,
            run_at: record.run_at,
            locked_by: record.locked_by,
            locked_until: record.locked_until,
            lock_active: record.lock_active,
            attempts: record.attempts,
            max_attempts: record.max_attempts,
            last_error: record.last_error,
            created_at: record.created_at,
            updated_at: record.updated_at,
            finished_at: record.finished_at,
        }
    }
}

impl From<AdminJobDetailRecord> for AdminJobDetailDto {
    fn from(record: AdminJobDetailRecord) -> Self {
        Self {
            job: AdminJobDto::from(record.job),
            runs: record.runs.into_iter().map(AdminJobRunDto::from).collect(),
            events: record
                .events
                .into_iter()
                .map(AdminJobEventDto::from)
                .collect(),
        }
    }
}

impl From<AdminJobRunRecord> for AdminJobRunDto {
    fn from(record: AdminJobRunRecord) -> Self {
        Self {
            id: record.id,
            worker_id: record.worker_id,
            status: record.status,
            started_at: record.started_at,
            finished_at: record.finished_at,
            duration_ms: record.duration_ms,
            error_message: record.error_message,
            metrics: record.metrics,
        }
    }
}

impl From<AdminJobEventRecord> for AdminJobEventDto {
    fn from(record: AdminJobEventRecord) -> Self {
        Self {
            id: record.id,
            run_id: record.run_id,
            event_type: record.event_type,
            event_level: record.event_level,
            message: record.message,
            payload: record.payload,
            created_at: record.created_at,
        }
    }
}

impl From<ScanRunSummary> for ScanRunSummaryDto {
    fn from(summary: ScanRunSummary) -> Self {
        Self {
            job_id: summary.job_id,
            status: summary.status,
            scanned_files: summary.scanned_files,
            created_items: summary.created_items,
            updated_files: summary.updated_files,
            metadata_refresh_jobs: summary.metadata_refresh_jobs,
            probe_jobs: summary.probe_jobs,
            missing_items: summary.missing_items,
            missing_mark_skipped: summary.missing_mark_skipped,
            has_more: summary.has_more,
            continuation_job_id: summary.continuation_job_id,
        }
    }
}

impl TryFrom<UpdateUserPolicyRequestDto> for UserPolicyInput {
    type Error = AppError;

    fn try_from(value: UpdateUserPolicyRequestDto) -> Result<Self, Self::Error> {
        let display_name = value
            .display_name
            .as_deref()
            .map(|name| {
                validate_optional_bounded_text("displayName", name, MAX_USER_DISPLAY_NAME_LEN)
            })
            .transpose()?
            .flatten()
            .map(str::to_owned);

        Ok(Self {
            display_name,
            is_disabled: value.is_disabled,
            allow_download: value.allow_download,
            allow_transcode: value.allow_transcode,
            allow_new_device_login: value.allow_new_device_login,
        })
    }
}

impl From<UserPolicyInput> for UpdateUserPolicyInput {
    fn from(input: UserPolicyInput) -> Self {
        Self {
            display_name: input.display_name,
            is_disabled: input.is_disabled,
            allow_download: input.allow_download,
            allow_transcode: input.allow_transcode,
            allow_new_device_login: input.allow_new_device_login,
        }
    }
}

impl From<UpdateUserLibraryPermissionRequestDto> for UserLibraryPermissionInput {
    fn from(value: UpdateUserLibraryPermissionRequestDto) -> Self {
        Self {
            can_view: value.can_view,
            can_download: value.can_download,
            can_transcode: value.can_transcode,
        }
    }
}

impl From<UserLibraryPermissionInput> for UpdateUserLibraryPermissionInput {
    fn from(input: UserLibraryPermissionInput) -> Self {
        Self {
            can_view: input.can_view,
            can_download: input.can_download,
            can_transcode: input.can_transcode,
        }
    }
}

impl TryFrom<UpsertNotificationTargetRequestDto> for NotificationTargetInput {
    type Error = AppError;

    fn try_from(value: UpsertNotificationTargetRequestDto) -> Result<Self, Self::Error> {
        let name = validate_bounded_text("name", &value.name, MAX_NOTIFICATION_TARGET_NAME_LEN)?
            .to_owned();
        let target_type = normalize_notification_target_type(&value.target_type);
        validate_notification_target_type(&target_type)?;
        let channel = value
            .channel
            .as_deref()
            .map(validate_notification_channel)
            .transpose()?
            .map(str::to_owned);
        let secretized = secretize_target_config(&target_type, &value.config)
            .map_err(|err| AppError::unprocessable(err.to_string()))?;

        Ok(Self {
            name,
            target_type,
            channel,
            config: secretized.config,
            secrets: secretized.secrets,
            is_enabled: value.is_enabled.unwrap_or(true),
        })
    }
}

impl From<NotificationTargetInput> for UpsertNotificationTargetInput {
    fn from(input: NotificationTargetInput) -> Self {
        Self {
            name: input.name,
            target_type: input.target_type,
            channel: input.channel,
            config: input.config,
            secrets: input.secrets,
            is_enabled: input.is_enabled,
        }
    }
}

impl From<NotificationTargetRecord> for NotificationTargetDto {
    fn from(record: NotificationTargetRecord) -> Self {
        Self {
            id: record.id,
            name: record.name,
            target_type: record.target_type.clone(),
            channel: record.channel,
            config: redacted_target_config(&record.target_type, &record.config),
            is_enabled: record.is_enabled,
            delivery_count: record.delivery_count,
            failure_count: record.failure_count,
            last_error: record.last_error,
        }
    }
}

impl From<NotificationRequestRecord> for NotificationRequestDto {
    fn from(record: NotificationRequestRecord) -> Self {
        Self {
            id: record.id,
            plugin_id: record.plugin_id,
            package_id: record.package_id,
            title: record.title,
            message: record.message,
            level: record.level,
            channel: record.channel,
            metadata: record.metadata,
            status: record.status,
            outbox_event_id: record.outbox_event_id,
            last_error: record.last_error,
            created_at: record.created_at,
            updated_at: record.updated_at,
        }
    }
}

impl From<NotificationDeliveryAttemptRecord> for NotificationDeliveryAttemptDto {
    fn from(record: NotificationDeliveryAttemptRecord) -> Self {
        Self {
            id: record.id,
            request_id: record.request_id,
            outbox_event_id: record.outbox_event_id,
            target_id: record.target_id,
            target_type: record.target_type,
            target_name: record.target_name,
            attempt: record.attempt,
            status: record.status,
            response_status: record.response_status,
            error_message: record.error_message,
            duration_ms: record.duration_ms,
            created_at: record.created_at,
            finished_at: record.finished_at,
        }
    }
}

impl From<PluginDispatchRecord> for PluginDispatchDto {
    fn from(record: PluginDispatchRecord) -> Self {
        Self {
            id: record.id,
            plugin_id: record.plugin_id,
            package_id: record.package_id,
            hook_id: record.hook_id,
            handler: record.handler,
            hook_event: record.hook_event,
            aggregate_type: record.aggregate_type,
            aggregate_id: record.aggregate_id,
            payload: record.payload,
            status: record.status,
            attempts: record.attempts,
            max_attempts: record.max_attempts,
            available_at: record.available_at,
            locked_until: record.locked_until,
            last_error: record.last_error,
            created_at: record.created_at,
            delivered_at: record.delivered_at,
        }
    }
}

impl From<PluginExecutionRunRecord> for PluginExecutionRunDto {
    fn from(record: PluginExecutionRunRecord) -> Self {
        Self {
            id: record.id,
            dispatch_id: record.dispatch_id,
            outbox_event_id: record.outbox_event_id,
            attempt: record.attempt,
            plugin_id: record.plugin_id,
            package_id: record.package_id,
            hook_id: record.hook_id,
            handler: record.handler,
            event_key: record.event_key,
            runtime: record.runtime,
            entrypoint: record.entrypoint,
            status: record.status,
            request_payload: record.request_payload,
            response_status: record.response_status,
            response_body: record.response_body,
            error_message: record.error_message,
            started_at: record.started_at,
            finished_at: record.finished_at,
            duration_ms: record.duration_ms,
        }
    }
}

impl From<PluginHostApiCallRecord> for PluginHostApiCallDto {
    fn from(record: PluginHostApiCallRecord) -> Self {
        Self {
            id: record.id,
            plugin_id: record.plugin_id,
            package_id: record.package_id,
            host_token_id: record.host_token_id,
            execution_run_id: record.execution_run_id,
            method: record.method,
            path: record.path,
            required_permission: record.required_permission,
            status_code: record.status_code,
            error_code: record.error_code,
            error_message: record.error_message,
            started_at: record.started_at,
            finished_at: record.finished_at,
            duration_ms: record.duration_ms,
        }
    }
}

impl EventStreamMirrorStatusDto {
    fn from_config_and_record(config: &Config, record: EventStreamMirrorStatusRecord) -> Self {
        Self {
            enabled: config.redis.event_streams_enabled,
            stream_key: config.redis.event_stream_key.clone(),
            batch_size: config.redis.event_stream_batch_size,
            interval_seconds: config.redis.event_stream_interval_seconds,
            lease_seconds: config.redis.event_stream_lease_seconds,
            operation_timeout_ms: config.redis.operation_timeout_ms,
            retry_base_seconds: config.redis.event_stream_retry_base_seconds,
            retry_max_seconds: config.redis.event_stream_retry_max_seconds,
            unmirrored_count: record.unmirrored_count,
            claimable_count: record.claimable_count,
            locked_count: record.locked_count,
            backoff_count: record.backoff_count,
            failed_count: record.failed_count,
            max_attempts: record.max_attempts,
            oldest_unmirrored_created_at: record.oldest_unmirrored_created_at,
            next_retry_at: record.next_retry_at,
            last_error: record.last_error,
        }
    }
}

impl From<ScheduledTaskAdminRecord> for ScheduledTaskDto {
    fn from(record: ScheduledTaskAdminRecord) -> Self {
        Self {
            id: record.id,
            task_key: record.task_key,
            task_type: record.task_type,
            owner_type: record.owner_type,
            owner_id: record.owner_id,
            enabled: record.enabled,
            schedule_kind: record.schedule_kind,
            schedule_value: record.schedule_value,
            next_run_at: record.next_run_at,
            last_run_at: record.last_run_at,
            timeout_seconds: record.timeout_seconds,
            max_concurrency: record.max_concurrency,
            active_run_count: record.active_run_count,
            last_run_id: record.last_run_id,
            failure_count: record.failure_count,
            last_error: record.last_error,
            created_at: record.created_at,
            updated_at: record.updated_at,
        }
    }
}

impl From<SchedulerRunSummary> for ScheduledTaskRunDto {
    fn from(summary: SchedulerRunSummary) -> Self {
        Self {
            task_key: summary.task_key,
            task_type: summary.task_type,
            queued_jobs: summary.queued_jobs,
        }
    }
}

impl From<ScheduledTaskRunRecord> for ScheduledTaskRunHistoryDto {
    fn from(record: ScheduledTaskRunRecord) -> Self {
        Self {
            id: record.id,
            task_key: record.task_key,
            trigger_type: record.trigger_type,
            worker_id: record.worker_id,
            status: record.status,
            lease_expires_at: record.lease_expires_at,
            lease_active: record.lease_active,
            queued_jobs: record.queued_jobs,
            error_message: record.error_message,
            started_at: record.started_at,
            finished_at: record.finished_at,
            duration_ms: record.duration_ms,
            created_at: record.created_at,
            updated_at: record.updated_at,
        }
    }
}

impl From<TranscodeSessionRecord> for TranscodeSessionDto {
    fn from(record: TranscodeSessionRecord) -> Self {
        Self {
            id: record.id,
            status: record.status,
            hardware_acceleration: record.hardware_acceleration,
            input_path: record.input_path,
            output_path: record.output_path,
            manifest_path: record.manifest_path,
            video_codec: record.video_codec,
            audio_codec: record.audio_codec,
            container: record.container,
            bitrate: record.bitrate,
            worker_id: record.worker_id,
            lease_expires_at: record.lease_expires_at,
            attempts: record.attempts,
            max_attempts: record.max_attempts,
            error_message: record.error_message,
            created_at: record.created_at,
            updated_at: record.updated_at,
            started_at: record.started_at,
            finished_at: record.finished_at,
        }
    }
}

fn scan_error_to_app_error(error: ScanError) -> AppError {
    match error {
        ScanError::JobNotFound => AppError::not_found(error.to_string()),
        ScanError::MissingLibraryId
        | ScanError::InvalidCursor(_)
        | ScanError::LibraryNotFound(_) => AppError::unprocessable(error.to_string()),
        ScanError::Database(_) | ScanError::Io(_) | ScanError::Join(_) => {
            AppError::internal(error.to_string())
        }
    }
}

fn scheduler_error_to_app_error(error: SchedulerError) -> AppError {
    match error {
        SchedulerError::TaskNotFound(_) => AppError::not_found(error.to_string()),
        SchedulerError::TaskNotRunning(_) => AppError::not_found(error.to_string()),
        SchedulerError::TaskDisabled(_)
        | SchedulerError::TaskConcurrencyLimit { .. }
        | SchedulerError::InvalidInterval(_)
        | SchedulerError::InvalidCron(_)
        | SchedulerError::UnsupportedScheduleKind(_)
        | SchedulerError::UnsupportedTaskType(_) => AppError::conflict(error.to_string()),
        SchedulerError::Database(_) => AppError::internal(error.to_string()),
    }
}

fn notification_retry_error_to_app_error(error: NotificationRetryError) -> AppError {
    match error {
        NotificationRetryError::NotFound => AppError::not_found("notification request not found"),
        NotificationRetryError::InvalidStatus(status) => AppError::conflict(format!(
            "notification request cannot be retried from status `{status}`"
        )),
        NotificationRetryError::Database(err) => {
            AppError::internal(format!("failed to retry notification request: {err}"))
        }
    }
}

fn plugin_dispatch_replay_error_to_app_error(error: PluginDispatchReplayError) -> AppError {
    match error {
        PluginDispatchReplayError::NotFound => AppError::not_found("plugin dispatch not found"),
        PluginDispatchReplayError::InvalidStatus(status) => AppError::conflict(format!(
            "plugin dispatch cannot be replayed from status `{status}`"
        )),
        PluginDispatchReplayError::Database(err) => {
            AppError::internal(format!("failed to replay plugin dispatch: {err}"))
        }
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;
    use crate::config::Config;

    #[test]
    fn validates_supported_library_types() {
        assert!(validate_library_type("movies").is_ok());
        assert!(validate_library_type("tv").is_ok());
        assert!(validate_library_type("music").is_ok());
        assert!(validate_library_type("mixed").is_ok());
        assert!(validate_library_type("books").is_err());
    }

    #[test]
    fn non_empty_validation_trims_input() {
        assert!(validate_non_empty("name", " Movies ").is_ok());
        assert!(validate_non_empty("name", " ").is_err());
    }

    #[test]
    fn metadata_refresh_limit_is_bounded() {
        assert_eq!(
            metadata_refresh_limit(None),
            DEFAULT_LIBRARY_METADATA_REFRESH_LIMIT
        );
        assert_eq!(metadata_refresh_limit(Some(0)), 1);
        assert_eq!(
            metadata_refresh_limit(Some(MAX_LIBRARY_METADATA_REFRESH_LIMIT + 1)),
            MAX_LIBRARY_METADATA_REFRESH_LIMIT
        );
    }

    #[test]
    fn plugin_host_api_call_limit_is_bounded() {
        assert_eq!(
            plugin_host_api_call_limit(None),
            MAX_PLUGIN_HOST_API_CALLS_LIST_LIMIT
        );
        assert_eq!(plugin_host_api_call_limit(Some(0)), 1);
        assert_eq!(
            plugin_host_api_call_limit(Some(MAX_PLUGIN_HOST_API_CALLS_LIST_LIMIT + 1)),
            MAX_PLUGIN_HOST_API_CALLS_LIST_LIMIT
        );
    }

    #[test]
    fn plugin_host_api_call_run_route_uses_keyset_pagination() {
        let routes = include_str!("routes.rs");

        assert!(routes.contains("pub struct PluginHostApiCallRunQueryDto"));
        assert!(routes.contains("Query(query): Query<PluginHostApiCallRunQueryDto>"));
        assert!(routes.contains("list_plugin_host_api_calls_for_run_page"));
        assert!(routes.contains("plugin_host_api_call_limit(query.limit)"));
        assert!(routes.contains("pagination_headers(page.has_more, page.next_cursor.as_deref())"));
    }

    #[test]
    fn plugin_dispatch_list_limit_is_bounded() {
        assert_eq!(
            plugin_dispatch_list_limit(None),
            MAX_PLUGIN_DISPATCHES_LIST_LIMIT
        );
        assert_eq!(plugin_dispatch_list_limit(Some(0)), 1);
        assert_eq!(
            plugin_dispatch_list_limit(Some(MAX_PLUGIN_DISPATCHES_LIST_LIMIT + 1)),
            MAX_PLUGIN_DISPATCHES_LIST_LIMIT
        );
    }

    #[test]
    fn plugin_dispatch_status_filter_requires_outbox_status() {
        assert_eq!(validate_event_outbox_status("pending").unwrap(), "pending");
        assert_eq!(
            validate_event_outbox_status("discarded").unwrap(),
            "discarded"
        );
        assert!(validate_event_outbox_status("").is_err());
        assert!(validate_event_outbox_status("queued").is_err());
        assert!(validate_event_outbox_status("pending now").is_err());
    }

    #[test]
    fn plugin_execution_run_list_limit_is_bounded() {
        assert_eq!(
            plugin_execution_run_list_limit(None),
            MAX_PLUGIN_EXECUTION_RUNS_LIST_LIMIT
        );
        assert_eq!(plugin_execution_run_list_limit(Some(0)), 1);
        assert_eq!(
            plugin_execution_run_list_limit(Some(MAX_PLUGIN_EXECUTION_RUNS_LIST_LIMIT + 1)),
            MAX_PLUGIN_EXECUTION_RUNS_LIST_LIMIT
        );
    }

    #[test]
    fn plugin_execution_run_status_filter_requires_declared_status() {
        assert_eq!(
            validate_plugin_execution_run_status("running").unwrap(),
            "running"
        );
        assert_eq!(
            validate_plugin_execution_run_status("succeeded").unwrap(),
            "succeeded"
        );
        assert_eq!(
            validate_plugin_execution_run_status("failed").unwrap(),
            "failed"
        );
        assert!(validate_plugin_execution_run_status("").is_err());
        assert!(validate_plugin_execution_run_status("pending").is_err());
        assert!(validate_plugin_execution_run_status("running now").is_err());
    }

    #[test]
    fn admin_user_list_limit_and_role_filter_are_bounded() {
        assert_eq!(admin_user_list_limit(None), MAX_ADMIN_USERS_LIST_LIMIT);
        assert_eq!(admin_user_list_limit(Some(0)), 1);
        assert_eq!(
            admin_user_list_limit(Some(MAX_ADMIN_USERS_LIST_LIMIT + 1)),
            MAX_ADMIN_USERS_LIST_LIMIT
        );

        assert_eq!(validate_admin_user_role_name("admin").unwrap(), "admin");
        assert_eq!(
            validate_admin_user_role_name("server-admin_1").unwrap(),
            "server-admin_1"
        );
        assert!(validate_admin_user_role_name("").is_err());
        assert!(validate_admin_user_role_name("server admin").is_err());
        assert!(validate_admin_user_role_name("../admin").is_err());
        assert!(validate_admin_user_role_name("admin\\role").is_err());
        assert!(validate_admin_user_role_name("admin\nrole").is_err());
    }

    #[test]
    fn admin_user_library_permission_list_limit_is_bounded() {
        assert_eq!(
            admin_user_library_permission_list_limit(None),
            MAX_ADMIN_USER_LIBRARY_PERMISSIONS_LIMIT
        );
        assert_eq!(admin_user_library_permission_list_limit(Some(0)), 1);
        assert_eq!(
            admin_user_library_permission_list_limit(Some(
                MAX_ADMIN_USER_LIBRARY_PERMISSIONS_LIMIT + 1
            )),
            MAX_ADMIN_USER_LIBRARY_PERMISSIONS_LIMIT
        );
        let library_type = normalize_library_type(" Movies ");
        assert_eq!(library_type, "movies");
        assert!(validate_library_type(&library_type).is_ok());
        assert!(validate_library_type(&normalize_library_type("books")).is_err());
    }

    #[test]
    fn admin_job_list_limit_is_bounded() {
        assert_eq!(admin_job_list_limit(None), MAX_ADMIN_JOBS_LIST_LIMIT);
        assert_eq!(admin_job_list_limit(Some(0)), 1);
        assert_eq!(
            admin_job_list_limit(Some(MAX_ADMIN_JOBS_LIST_LIMIT + 1)),
            MAX_ADMIN_JOBS_LIST_LIMIT
        );
    }

    #[test]
    fn admin_job_list_filters_require_safe_values() {
        assert_eq!(validate_admin_job_status("queued").unwrap(), "queued");
        assert_eq!(validate_admin_job_status("running").unwrap(), "running");
        assert_eq!(validate_admin_job_status("succeeded").unwrap(), "succeeded");
        assert_eq!(validate_admin_job_status("cancelled").unwrap(), "cancelled");
        assert!(validate_admin_job_status("").is_err());
        assert!(validate_admin_job_status("pending").is_err());
        assert!(validate_admin_job_status("running now").is_err());

        assert_eq!(
            validate_admin_job_filter_text("jobType", "metadata.refresh").unwrap(),
            "metadata.refresh"
        );
        assert_eq!(
            validate_admin_job_filter_text("queueName", "plugin:hook_dispatch").unwrap(),
            "plugin:hook_dispatch"
        );
        assert!(validate_admin_job_filter_text("jobType", "").is_err());
        assert!(validate_admin_job_filter_text("jobType", "metadata refresh").is_err());
        assert!(validate_admin_job_filter_text("jobType", "../metadata.refresh").is_err());
        assert!(validate_admin_job_filter_text("queueName", "worker\\queue").is_err());
        assert!(validate_admin_job_filter_text("queueName", "worker\nqueue").is_err());
    }

    #[test]
    fn admin_job_run_and_event_list_limits_are_bounded() {
        assert_eq!(
            admin_job_run_list_limit(None),
            MAX_ADMIN_JOB_RUNS_LIST_LIMIT
        );
        assert_eq!(admin_job_run_list_limit(Some(0)), 1);
        assert_eq!(
            admin_job_run_list_limit(Some(MAX_ADMIN_JOB_RUNS_LIST_LIMIT + 1)),
            MAX_ADMIN_JOB_RUNS_LIST_LIMIT
        );

        assert_eq!(
            admin_job_event_list_limit(None),
            MAX_ADMIN_JOB_EVENTS_LIST_LIMIT
        );
        assert_eq!(admin_job_event_list_limit(Some(0)), 1);
        assert_eq!(
            admin_job_event_list_limit(Some(MAX_ADMIN_JOB_EVENTS_LIST_LIMIT + 1)),
            MAX_ADMIN_JOB_EVENTS_LIST_LIMIT
        );
    }

    #[test]
    fn admin_job_run_and_event_filters_require_declared_values() {
        assert_eq!(validate_admin_job_run_status("running").unwrap(), "running");
        assert_eq!(
            validate_admin_job_run_status("cancelled").unwrap(),
            "cancelled"
        );
        assert!(validate_admin_job_run_status("").is_err());
        assert!(validate_admin_job_run_status("queued").is_err());
        assert!(validate_admin_job_run_status("running now").is_err());

        assert_eq!(validate_admin_job_event_level("info").unwrap(), "info");
        assert_eq!(validate_admin_job_event_level("error").unwrap(), "error");
        assert!(validate_admin_job_event_level("").is_err());
        assert!(validate_admin_job_event_level("fatal").is_err());
        assert!(validate_admin_job_event_level("info now").is_err());
    }

    #[test]
    fn positive_i64_cursor_rejects_invalid_values() {
        assert_eq!(validate_positive_i64_cursor("cursor", "42").unwrap(), 42);
        assert!(validate_positive_i64_cursor("cursor", "0").is_err());
        assert!(validate_positive_i64_cursor("cursor", "-1").is_err());
        assert!(validate_positive_i64_cursor("cursor", "abc").is_err());
    }

    #[test]
    fn scheduled_task_list_limit_and_filters_are_bounded() {
        assert_eq!(
            scheduled_task_list_limit(None),
            MAX_SCHEDULED_TASKS_LIST_LIMIT
        );
        assert_eq!(scheduled_task_list_limit(Some(0)), 1);
        assert_eq!(
            scheduled_task_list_limit(Some(MAX_SCHEDULED_TASKS_LIST_LIMIT + 1)),
            MAX_SCHEDULED_TASKS_LIST_LIMIT
        );

        assert_eq!(
            validate_scheduled_task_filter_text("plugin.schedule").unwrap(),
            "plugin.schedule"
        );
        assert_eq!(
            validate_scheduled_task_filter_text("library:scan_all").unwrap(),
            "library:scan_all"
        );
        assert!(validate_scheduled_task_filter_text("").is_err());
        assert!(validate_scheduled_task_filter_text("plugin schedule").is_err());
        assert!(validate_scheduled_task_filter_text("../plugin.schedule").is_err());

        assert_eq!(validate_scheduled_task_owner_type("core").unwrap(), "core");
        assert_eq!(
            validate_scheduled_task_owner_type("plugin").unwrap(),
            "plugin"
        );
        assert!(validate_scheduled_task_owner_type("").is_err());
        assert!(validate_scheduled_task_owner_type("system").is_err());
        assert!(validate_scheduled_task_owner_type("plugin worker").is_err());
    }

    #[test]
    fn scheduled_task_run_list_limit_is_bounded() {
        assert_eq!(
            scheduled_task_run_list_limit(None),
            MAX_SCHEDULED_TASK_RUNS_LIST_LIMIT
        );
        assert_eq!(scheduled_task_run_list_limit(Some(0)), 1);
        assert_eq!(
            scheduled_task_run_list_limit(Some(MAX_SCHEDULED_TASK_RUNS_LIST_LIMIT + 1)),
            MAX_SCHEDULED_TASK_RUNS_LIST_LIMIT
        );
    }

    #[test]
    fn scheduled_task_run_status_filter_requires_declared_status() {
        assert_eq!(
            validate_scheduled_task_run_status("running").unwrap(),
            "running"
        );
        assert_eq!(
            validate_scheduled_task_run_status("expired").unwrap(),
            "expired"
        );
        assert!(validate_scheduled_task_run_status("").is_err());
        assert!(validate_scheduled_task_run_status("pending").is_err());
        assert!(validate_scheduled_task_run_status("running now").is_err());
    }

    #[test]
    fn transcode_session_list_limit_is_bounded() {
        assert_eq!(
            transcode_session_list_limit(None),
            MAX_TRANSCODE_SESSIONS_LIST_LIMIT
        );
        assert_eq!(transcode_session_list_limit(Some(0)), 1);
        assert_eq!(
            transcode_session_list_limit(Some(MAX_TRANSCODE_SESSIONS_LIST_LIMIT + 1)),
            MAX_TRANSCODE_SESSIONS_LIST_LIMIT
        );
    }

    #[test]
    fn transcode_session_filters_require_safe_values() {
        assert_eq!(
            validate_transcode_session_status("queued").unwrap(),
            "queued"
        );
        assert_eq!(
            validate_transcode_session_status("running").unwrap(),
            "running"
        );
        assert_eq!(
            validate_transcode_session_status("succeeded").unwrap(),
            "succeeded"
        );
        assert_eq!(
            validate_transcode_session_status("cancelled").unwrap(),
            "cancelled"
        );
        assert!(validate_transcode_session_status("").is_err());
        assert!(validate_transcode_session_status("pending").is_err());
        assert!(validate_transcode_session_status("running now").is_err());

        assert_eq!(
            validate_transcode_hardware_acceleration("intel-gpu_0").unwrap(),
            "intel-gpu_0"
        );
        assert_eq!(
            validate_transcode_hardware_acceleration("cuda.12").unwrap(),
            "cuda.12"
        );
        assert!(validate_transcode_hardware_acceleration("").is_err());
        assert!(validate_transcode_hardware_acceleration("nvidia gpu").is_err());
        assert!(validate_transcode_hardware_acceleration("../nvidia").is_err());
        assert!(validate_transcode_hardware_acceleration("nvidia\\gpu").is_err());
        assert!(validate_transcode_hardware_acceleration("nvidia\ngpu").is_err());
    }

    #[test]
    fn notification_target_list_limit_and_filters_are_bounded() {
        assert_eq!(
            notification_target_list_limit(None),
            MAX_NOTIFICATION_TARGETS_LIST_LIMIT
        );
        assert_eq!(notification_target_list_limit(Some(0)), 1);
        assert_eq!(
            notification_target_list_limit(Some(MAX_NOTIFICATION_TARGETS_LIST_LIMIT + 1)),
            MAX_NOTIFICATION_TARGETS_LIST_LIMIT
        );

        let target_type = normalize_notification_target_type(" WebHook ");
        assert_eq!(target_type, "webhook");
        assert!(validate_notification_target_type(&target_type).is_ok());
        assert!(
            validate_notification_target_type(&normalize_notification_target_type("email"))
                .is_err()
        );

        assert_eq!(validate_notification_channel("ops:tg").unwrap(), "ops:tg");
        assert!(validate_notification_channel("").is_err());
        assert!(validate_notification_channel("ops tg").is_err());
        assert!(validate_notification_channel("../ops").is_err());
        assert!(validate_notification_channel("ops\\tg").is_err());
    }

    #[test]
    fn notification_list_limits_are_bounded() {
        assert_eq!(
            notification_request_list_limit(None),
            MAX_NOTIFICATION_REQUESTS_LIST_LIMIT
        );
        assert_eq!(notification_request_list_limit(Some(0)), 1);
        assert_eq!(
            notification_request_list_limit(Some(MAX_NOTIFICATION_REQUESTS_LIST_LIMIT + 1)),
            MAX_NOTIFICATION_REQUESTS_LIST_LIMIT
        );

        assert_eq!(
            notification_attempt_list_limit(None),
            MAX_NOTIFICATION_ATTEMPTS_LIST_LIMIT
        );
        assert_eq!(notification_attempt_list_limit(Some(0)), 1);
        assert_eq!(
            notification_attempt_list_limit(Some(MAX_NOTIFICATION_ATTEMPTS_LIST_LIMIT + 1)),
            MAX_NOTIFICATION_ATTEMPTS_LIST_LIMIT
        );
    }

    #[test]
    fn notification_status_filters_require_declared_statuses() {
        assert_eq!(
            validate_notification_request_status("queued").unwrap(),
            "queued"
        );
        assert_eq!(
            validate_notification_request_status("discarded").unwrap(),
            "discarded"
        );
        assert!(validate_notification_request_status("").is_err());
        assert!(validate_notification_request_status("pending").is_err());
        assert!(validate_notification_request_status("queued now").is_err());

        assert_eq!(
            validate_notification_delivery_attempt_status("running").unwrap(),
            "running"
        );
        assert_eq!(
            validate_notification_delivery_attempt_status("skipped").unwrap(),
            "skipped"
        );
        assert!(validate_notification_delivery_attempt_status("").is_err());
        assert!(validate_notification_delivery_attempt_status("delivered").is_err());
        assert!(validate_notification_delivery_attempt_status("running now").is_err());
    }

    #[test]
    fn plugin_host_api_status_code_filter_requires_http_status_range() {
        assert_eq!(validate_http_status_code(200).unwrap(), 200);
        assert_eq!(validate_http_status_code(599).unwrap(), 599);
        assert!(validate_http_status_code(99).is_err());
        assert!(validate_http_status_code(600).is_err());
    }

    #[test]
    fn metadata_provider_statuses_reflect_config() {
        let config = Config::from_source(|key| match key {
            "TMDB_ACCESS_TOKEN" => Some("token".to_owned()),
            "METADATA_PROVIDERS" => Some("tmdb,fanart".to_owned()),
            _ => None,
        })
        .unwrap();

        let response = metadata_provider_status_response(&config);
        let tmdb = response
            .providers
            .iter()
            .find(|provider| provider.provider == "tmdb")
            .unwrap();
        let tvdb = response
            .providers
            .iter()
            .find(|provider| provider.provider == "tvdb")
            .unwrap();

        assert!(tmdb.enabled);
        assert!(tmdb.search_supported);
        assert!(tmdb.credential_configured);
        assert!(!tvdb.enabled);
    }

    #[test]
    fn notification_retry_errors_map_to_admin_statuses() {
        let not_found = notification_retry_error_to_app_error(NotificationRetryError::NotFound);
        assert_eq!(not_found.status_code(), StatusCode::NOT_FOUND);

        let conflict = notification_retry_error_to_app_error(
            NotificationRetryError::InvalidStatus("delivered".to_owned()),
        );
        assert_eq!(conflict.status_code(), StatusCode::CONFLICT);
        assert!(conflict.message().contains("delivered"));
    }

    #[test]
    fn plugin_dispatch_replay_errors_map_to_admin_statuses() {
        let not_found =
            plugin_dispatch_replay_error_to_app_error(PluginDispatchReplayError::NotFound);
        assert_eq!(not_found.status_code(), StatusCode::NOT_FOUND);

        let conflict = plugin_dispatch_replay_error_to_app_error(
            PluginDispatchReplayError::InvalidStatus("pending".to_owned()),
        );
        assert_eq!(conflict.status_code(), StatusCode::CONFLICT);
        assert!(conflict.message().contains("pending"));
    }

    #[test]
    fn scheduler_errors_map_to_admin_statuses() {
        let not_found =
            scheduler_error_to_app_error(SchedulerError::TaskNotFound("missing".to_owned()));
        assert_eq!(not_found.status_code(), StatusCode::NOT_FOUND);

        let disabled =
            scheduler_error_to_app_error(SchedulerError::TaskDisabled("disabled".to_owned()));
        assert_eq!(disabled.status_code(), StatusCode::CONFLICT);
        assert!(disabled.message().contains("disabled"));

        let concurrency = scheduler_error_to_app_error(SchedulerError::TaskConcurrencyLimit {
            task_key: "busy".to_owned(),
            max_concurrency: 1,
        });
        assert_eq!(concurrency.status_code(), StatusCode::CONFLICT);
        assert!(concurrency.message().contains("max concurrency 1"));

        let unsupported = scheduler_error_to_app_error(SchedulerError::UnsupportedTaskType(
            "custom.task".to_owned(),
        ));
        assert_eq!(unsupported.status_code(), StatusCode::CONFLICT);
        assert!(unsupported.message().contains("custom.task"));
    }

    #[test]
    fn scheduled_task_key_validation_rejects_path_like_values() {
        assert!(validate_scheduled_task_key("core.library.incremental_scan").is_ok());
        assert!(validate_scheduled_task_key("plugin.task/run").is_err());
        assert!(validate_scheduled_task_key("plugin task").is_err());
    }

    #[test]
    fn job_public_id_validation_requires_uuid_shape() {
        assert!(validate_uuid_public_id("jobId", "00000000-0000-0000-0000-000000000001").is_ok());
        assert!(validate_uuid_public_id("jobId", "job-1").is_err());
        assert!(validate_uuid_public_id("jobId", "00000000-0000-0000-0000-00000000000x").is_err());
    }

    #[test]
    fn user_policy_input_trims_optional_display_name() {
        let input = UserPolicyInput::try_from(UpdateUserPolicyRequestDto {
            display_name: Some("  Alice  ".to_owned()),
            is_disabled: false,
            allow_download: true,
            allow_transcode: false,
            allow_new_device_login: true,
        })
        .unwrap();

        assert_eq!(input.display_name.as_deref(), Some("Alice"));
        assert!(input.allow_download);
        assert!(!input.allow_transcode);

        let blank = UserPolicyInput::try_from(UpdateUserPolicyRequestDto {
            display_name: Some("   ".to_owned()),
            is_disabled: false,
            allow_download: false,
            allow_transcode: true,
            allow_new_device_login: false,
        })
        .unwrap();
        assert_eq!(blank.display_name, None);
    }

    #[test]
    fn user_policy_input_rejects_long_display_name() {
        let err = UserPolicyInput::try_from(UpdateUserPolicyRequestDto {
            display_name: Some("x".repeat(MAX_USER_DISPLAY_NAME_LEN + 1)),
            is_disabled: false,
            allow_download: false,
            allow_transcode: true,
            allow_new_device_login: true,
        })
        .unwrap_err();

        assert_eq!(err.status_code(), StatusCode::UNPROCESSABLE_ENTITY);
    }

    #[test]
    fn user_library_permission_input_preserves_flags() {
        let input = UserLibraryPermissionInput::from(UpdateUserLibraryPermissionRequestDto {
            can_view: true,
            can_download: false,
            can_transcode: true,
        });
        let repository_input = UpdateUserLibraryPermissionInput::from(input);

        assert!(repository_input.can_view);
        assert!(!repository_input.can_download);
        assert!(repository_input.can_transcode);
    }

    #[test]
    fn user_library_permission_dto_preserves_effective_permissions() {
        let dto = AdminUserLibraryPermissionDto::from(AdminUserLibraryPermissionRecord {
            library_id: "library-1".to_owned(),
            library_name: "Movies".to_owned(),
            library_type: "movies".to_owned(),
            is_hidden: false,
            permission_configured: true,
            can_view: true,
            can_download: true,
            can_transcode: false,
            effective_can_view: true,
            effective_can_download: false,
            effective_can_transcode: false,
            permission_updated_at: Some("2026-06-19 00:00:00+00".to_owned()),
        });

        assert_eq!(dto.library_name, "Movies");
        assert!(dto.permission_configured);
        assert!(dto.can_download);
        assert!(!dto.effective_can_download);
    }

    #[test]
    fn notification_target_input_validates_and_normalizes() {
        let input = NotificationTargetInput::try_from(UpsertNotificationTargetRequestDto {
            name: "  Telegram Primary  ".to_owned(),
            target_type: "TELEGRAM".to_owned(),
            channel: Some("tg.primary".to_owned()),
            config: json!({
                "botToken": "secret-token",
                "chatId": "chat-1",
                "apiBaseUrl": "https://api.telegram.org"
            }),
            is_enabled: None,
        })
        .unwrap();

        assert_eq!(input.name, "Telegram Primary");
        assert_eq!(input.target_type, "telegram");
        assert_eq!(input.channel.as_deref(), Some("tg.primary"));
        assert_eq!(input.config["botToken"]["secretRef"], "telegram.botToken");
        assert_eq!(input.config["chatId"], "chat-1");
        assert_eq!(input.secrets.len(), 1);
        assert_eq!(input.secrets[0].key, "telegram.botToken");
        assert_eq!(input.secrets[0].value, "secret-token");
        assert!(input.is_enabled);
    }

    #[test]
    fn notification_target_input_rejects_invalid_type_channel_or_config() {
        assert!(
            NotificationTargetInput::try_from(UpsertNotificationTargetRequestDto {
                name: "Plugin".to_owned(),
                target_type: "plugin".to_owned(),
                channel: None,
                config: json!({}),
                is_enabled: None,
            })
            .is_err()
        );
        assert!(
            NotificationTargetInput::try_from(UpsertNotificationTargetRequestDto {
                name: "Webhook".to_owned(),
                target_type: "webhook".to_owned(),
                channel: Some("../bad".to_owned()),
                config: json!({
                    "url": "https://notify.example.test/hook"
                }),
                is_enabled: None,
            })
            .is_err()
        );
        assert!(
            NotificationTargetInput::try_from(UpsertNotificationTargetRequestDto {
                name: "Webhook".to_owned(),
                target_type: "webhook".to_owned(),
                channel: None,
                config: json!({
                    "url": "https://notify.example.test/hook",
                    "headers": {
                        "host": "example.test"
                    }
                }),
                is_enabled: None,
            })
            .is_err()
        );
    }

    #[test]
    fn notification_target_dto_redacts_sensitive_config() {
        let dto = NotificationTargetDto::from(NotificationTargetRecord {
            id: "target-1".to_owned(),
            name: "Webhook".to_owned(),
            target_type: "webhook".to_owned(),
            channel: Some("ops".to_owned()),
            config: json!({
                "url": "https://notify.example.test/hook?token=secret",
                "headers": {
                    "x-api-key": "secret"
                }
            }),
            is_enabled: true,
            delivery_count: 2,
            failure_count: 1,
            last_error: Some("previous failure".to_owned()),
        });

        assert_eq!(dto.config["url"], "[redacted]");
        assert_eq!(dto.config["headers"]["x-api-key"], "[redacted]");
        assert_eq!(dto.delivery_count, 2);
        assert_eq!(dto.failure_count, 1);
    }

    #[test]
    fn notification_secret_writes_require_configured_secret_key() {
        let state = AppState::for_tests(Config::default());

        let err = match secret_cipher_from_state(&state) {
            Ok(_) => panic!("missing secret key should be rejected"),
            Err(err) => err,
        };

        assert!(err.message().contains("FBZ_SECRET_KEY"));
    }
}
