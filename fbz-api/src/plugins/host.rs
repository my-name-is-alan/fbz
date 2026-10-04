use std::{
    collections::{BTreeSet, HashMap},
    error::Error,
    fmt::{Display, Formatter},
    time::{Duration, Instant},
};

use axum::{
    Json, Router,
    extract::{Path, Query, State},
    http::{HeaderMap, StatusCode},
    routing::get,
};
use reqwest::Url;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sqlx::{Row, postgres::PgRow};
use tracing::warn;

use crate::{
    auth::token::{hash_token, issue_access_token},
    db::DbPool,
    error::AppError,
    metadata::write::{
        NamedMetadata, PersonMetadata, replace_item_genres, replace_item_people,
        replace_item_studios,
    },
    notifications::secrets::{
        SECRET_ALGORITHM, SecretCipher, SecretError, contains_secret_refs, materialize_secret_refs,
    },
    plugins::manifest::{
        SUPPORTED_PLUGIN_API_VERSION, SUPPORTED_PLUGIN_RUNTIMES, supported_plugin_hook_events,
        supported_plugin_permissions,
    },
    state::AppState,
};

const PLUGIN_TOKEN_HEADER: &str = "x-fbz-plugin-token";
const PLUGIN_HOST_API_VERSION: &str = "1";
const EXECUTABLE_PLUGIN_RUNTIMES: &[&str] = &["http", "wasi"];
const HTTP_PLUGIN_SCHEMES: &[&str] = &["http", "https"];
const MAX_KV_KEY_LEN: usize = 128;
const HOST_TOKEN_TTL_SECONDS: i64 = 300;
const DEFAULT_LIBRARY_ITEMS_LIMIT: u32 = 50;
const MAX_LIBRARY_ITEMS_LIMIT: u32 = 200;
const MAX_LIBRARY_ITEM_CURSOR_LEN: usize = 2048;
const LIBRARY_ITEM_CURSOR_VERSION: &str = "v1";
const PLUGIN_LIBRARY_ITEMS_FIRST_PAGE_SQL: &str = r#"
            with requested_library as (
                select case
                    when $1::text ~* '^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$'
                    then $1::uuid
                    else null::uuid
                end as public_id
            )
            select
                mi.id as cursor_id,
                coalesce(nullif(mi.sort_title, ''), mi.title) as sort_key,
                mi.public_id::text as id,
                l.public_id::text as library_id,
                parent.public_id::text as parent_id,
                mi.item_type,
                mi.title,
                mi.production_year,
                mi.runtime_ticks
            from requested_library
            join libraries l on l.public_id = requested_library.public_id
            join media_items mi on mi.library_id = l.id
            left join media_items parent on parent.id = mi.parent_id
            where l.is_hidden = false
              and mi.is_deleted = false
            order by coalesce(nullif(mi.sort_title, ''), mi.title), mi.id
            limit $2
            "#;
const PLUGIN_LIBRARY_ITEMS_AFTER_CURSOR_SQL: &str = r#"
            with requested_library as (
                select case
                    when $1::text ~* '^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$'
                    then $1::uuid
                    else null::uuid
                end as public_id
            )
            select
                mi.id as cursor_id,
                coalesce(nullif(mi.sort_title, ''), mi.title) as sort_key,
                mi.public_id::text as id,
                l.public_id::text as library_id,
                parent.public_id::text as parent_id,
                mi.item_type,
                mi.title,
                mi.production_year,
                mi.runtime_ticks
            from requested_library
            join libraries l on l.public_id = requested_library.public_id
            join media_items mi on mi.library_id = l.id
            left join media_items parent on parent.id = mi.parent_id
            where l.is_hidden = false
              and mi.is_deleted = false
              and (coalesce(nullif(mi.sort_title, ''), mi.title), mi.id) > ($2::text, $3::bigint)
            order by coalesce(nullif(mi.sort_title, ''), mi.title), mi.id
            limit $4
            "#;
const PLUGIN_LIBRARY_EXISTS_SQL: &str = r#"
            select exists (
                with requested_library as (
                    select case
                        when $1::text ~* '^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$'
                        then $1::uuid
                        else null::uuid
                    end as public_id
                )
                select 1
                from requested_library
                join libraries l on l.public_id = requested_library.public_id
                where l.is_hidden = false
            ) as found
            "#;
const AUTHENTICATE_PLUGIN_HOST_TOKEN_SQL: &str = r#"
            update plugin_host_tokens token
            set last_used_at = now()
            from plugin_installations pi
            join plugin_packages pkg on pkg.id = pi.active_package_id
            cross join plugin_execution_runs run
            where token.token_hash = $1
              and token.revoked_at is null
              and token.expires_at > now()
              and token.scope = 'execution'
              and token.plugin_id = pi.plugin_id
              and token.package_id = pkg.public_id::text
              and run.id = token.execution_run_id
              and run.plugin_id = token.plugin_id
              and run.package_id = token.package_id
              and run.status = 'running'
              and run.finished_at is null
              and pi.enabled = true
              and pi.approval_status = 'approved'
              and pkg.package_status = 'approved'
            returning
                token.id as token_id,
                token.plugin_id,
                token.package_id,
                token.execution_run_id,
                token.permission_snapshot
            "#;
const NOTIFICATION_REQUESTED_EVENT: &str = "notification.send.requested";
const MAX_NOTIFICATION_TITLE_LEN: usize = 160;
const MAX_NOTIFICATION_MESSAGE_LEN: usize = 4000;
const MAX_NOTIFICATION_CHANNEL_LEN: usize = 64;
const MAX_METADATA_TITLE_LEN: usize = 512;
const MAX_METADATA_OVERVIEW_LEN: usize = 20_000;
const MAX_METADATA_EXTERNAL_IDS: usize = 32;
const MAX_METADATA_PROVIDER_LEN: usize = 64;
const MAX_METADATA_EXTERNAL_ID_LEN: usize = 128;
const MAX_METADATA_CLASSIFICATION_ITEMS: usize = 128;
const MAX_METADATA_CLASSIFICATION_NAME_LEN: usize = 128;
const MAX_METADATA_PEOPLE_ITEMS: usize = 512;
const MAX_METADATA_PERSON_NAME_LEN: usize = 256;
const MAX_METADATA_PERSON_ROLE_NAME_LEN: usize = 128;
const MAX_METADATA_PERSON_SORT_ORDER: i32 = 1_000_000;
const MAX_PLUGIN_ARTWORK_PER_REQUEST: usize = 256;
const MAX_PLUGIN_ARTWORK_REMOTE_URL_LEN: usize = 2048;
const MAX_PLUGIN_ARTWORK_DIMENSION: i32 = 65_535;
const MAX_PLUGIN_MARKERS_PER_REQUEST: usize = 512;
const MAX_PLUGIN_SOURCE_SUFFIX_LEN: usize = 64;
const MAX_PLUGIN_SOURCE_LEN: usize = 256;
const MAX_HOST_API_AUDIT_ERROR_BYTES: usize = 1024;
const SUPPORTED_PLUGIN_ARTWORK_TYPES: &[&str] = &[
    "primary", "poster", "backdrop", "logo", "thumb", "banner", "disc", "artist", "album",
];
const SUPPORTED_PLUGIN_PERSON_ROLE_TYPES: &[&str] = &[
    "actor",
    "director",
    "writer",
    "producer",
    "composer",
    "artist",
    "guest_star",
];
const SUPPORTED_PLUGIN_MARKER_TYPES: &[&str] = &[
    "intro_start",
    "intro_end",
    "credits_start",
    "credits_end",
    "commercial",
    "chapter",
];

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/api/plugin/capabilities", get(get_capabilities))
        .route("/api/plugin/config", get(get_config))
        .route("/api/plugin/libraries", get(list_libraries))
        .route(
            "/api/plugin/libraries/{library_id}/items",
            get(list_library_items),
        )
        .route("/api/plugin/items/{item_id}", get(get_item))
        .route(
            "/api/plugin/items/{item_id}/metadata",
            axum::routing::patch(patch_item_metadata),
        )
        .route(
            "/api/plugin/items/{item_id}/artwork",
            axum::routing::put(put_item_artwork),
        )
        .route(
            "/api/plugin/items/{item_id}/markers",
            axum::routing::put(put_item_markers),
        )
        .route(
            "/api/plugin/notifications",
            axum::routing::post(send_notification),
        )
        .route(
            "/api/plugin/kv/{key}",
            get(get_kv).put(put_kv).delete(delete_kv),
        )
}

#[derive(Clone)]
pub struct PluginHostRepository {
    pool: DbPool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct IssuedPluginHostToken {
    pub id: i64,
    pub token: String,
    pub prefix: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct PluginHostContext {
    token_id: i64,
    plugin_id: String,
    package_id: String,
    execution_run_id: i64,
    permission_keys: Vec<String>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct PluginHostApiRoute {
    method: &'static str,
    path: &'static str,
    required_permission: Option<&'static str>,
    success_status: StatusCode,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct PluginHostApiCallAudit {
    route: PluginHostApiRoute,
    status_code: StatusCode,
    error_code: Option<String>,
    error_message: Option<String>,
    duration: Duration,
}

const HOST_API_CAPABILITIES: PluginHostApiRoute = PluginHostApiRoute {
    method: "GET",
    path: "/api/plugin/capabilities",
    required_permission: None,
    success_status: StatusCode::OK,
};
const HOST_API_CONFIG: PluginHostApiRoute = PluginHostApiRoute {
    method: "GET",
    path: "/api/plugin/config",
    required_permission: None,
    success_status: StatusCode::OK,
};
const HOST_API_LIST_LIBRARIES: PluginHostApiRoute = PluginHostApiRoute {
    method: "GET",
    path: "/api/plugin/libraries",
    required_permission: Some("library.read"),
    success_status: StatusCode::OK,
};
const HOST_API_LIST_LIBRARY_ITEMS: PluginHostApiRoute = PluginHostApiRoute {
    method: "GET",
    path: "/api/plugin/libraries/{libraryId}/items",
    required_permission: Some("library.read"),
    success_status: StatusCode::OK,
};
const HOST_API_GET_ITEM: PluginHostApiRoute = PluginHostApiRoute {
    method: "GET",
    path: "/api/plugin/items/{itemId}",
    required_permission: Some("media.read"),
    success_status: StatusCode::OK,
};
const HOST_API_PATCH_ITEM_METADATA: PluginHostApiRoute = PluginHostApiRoute {
    method: "PATCH",
    path: "/api/plugin/items/{itemId}/metadata",
    required_permission: Some("metadata.write"),
    success_status: StatusCode::OK,
};
const HOST_API_PUT_ITEM_ARTWORK: PluginHostApiRoute = PluginHostApiRoute {
    method: "PUT",
    path: "/api/plugin/items/{itemId}/artwork",
    required_permission: Some("metadata.write"),
    success_status: StatusCode::OK,
};
const HOST_API_PUT_ITEM_MARKERS: PluginHostApiRoute = PluginHostApiRoute {
    method: "PUT",
    path: "/api/plugin/items/{itemId}/markers",
    required_permission: Some("metadata.write"),
    success_status: StatusCode::OK,
};
const HOST_API_SEND_NOTIFICATION: PluginHostApiRoute = PluginHostApiRoute {
    method: "POST",
    path: "/api/plugin/notifications",
    required_permission: Some("notification.send"),
    success_status: StatusCode::ACCEPTED,
};
const HOST_API_GET_KV: PluginHostApiRoute = PluginHostApiRoute {
    method: "GET",
    path: "/api/plugin/kv/{key}",
    required_permission: None,
    success_status: StatusCode::OK,
};
const HOST_API_PUT_KV: PluginHostApiRoute = PluginHostApiRoute {
    method: "PUT",
    path: "/api/plugin/kv/{key}",
    required_permission: None,
    success_status: StatusCode::OK,
};
const HOST_API_DELETE_KV: PluginHostApiRoute = PluginHostApiRoute {
    method: "DELETE",
    path: "/api/plugin/kv/{key}",
    required_permission: None,
    success_status: StatusCode::OK,
};

#[derive(Clone, Copy)]
struct PluginPermissionCapability {
    key: &'static str,
    category: &'static str,
    risk_level: &'static str,
    description: &'static str,
    manifest_features: &'static [&'static str],
}

const PLUGIN_PERMISSION_CAPABILITIES: &[PluginPermissionCapability] = &[
    PluginPermissionCapability {
        key: "admin.menu",
        category: "admin",
        risk_level: "medium",
        description: "Allows the plugin to add entries to the admin navigation menu under its plugin namespace.",
        manifest_features: &["menu"],
    },
    PluginPermissionCapability {
        key: "library.read",
        category: "library",
        risk_level: "medium",
        description: "Allows the plugin to read non-hidden media library summaries and paged public item summaries.",
        manifest_features: &[],
    },
    PluginPermissionCapability {
        key: "library.write",
        category: "library",
        risk_level: "high",
        description: "Reserved for future controlled media library write operations.",
        manifest_features: &[],
    },
    PluginPermissionCapability {
        key: "media.read",
        category: "media",
        risk_level: "high",
        description: "Allows the plugin to read public media item metadata without exposing file paths or playback URLs.",
        manifest_features: &[],
    },
    PluginPermissionCapability {
        key: "metadata.read",
        category: "metadata",
        risk_level: "medium",
        description: "Read the scrape context and act as a metadata-provider source via the metadata.provider.query hook.",
        manifest_features: &["metadata.provider.query"],
    },
    PluginPermissionCapability {
        key: "metadata.write",
        category: "metadata",
        risk_level: "high",
        description: "Allows the plugin to patch whitelisted metadata fields and replace plugin-scoped artwork or markers.",
        manifest_features: &[],
    },
    PluginPermissionCapability {
        key: "notification.send",
        category: "notification",
        risk_level: "medium",
        description: "Allows the plugin to enqueue notification requests for administrator-configured delivery targets.",
        manifest_features: &[],
    },
    PluginPermissionCapability {
        key: "playback.read",
        category: "playback",
        risk_level: "medium",
        description: "Reserved for future read-only playback session and activity APIs.",
        manifest_features: &[],
    },
    PluginPermissionCapability {
        key: "scheduler.register",
        category: "scheduler",
        risk_level: "medium",
        description: "Allows the plugin manifest to register interval or cron scheduled tasks.",
        manifest_features: &["schedules"],
    },
    PluginPermissionCapability {
        key: "webhook.emit",
        category: "webhook",
        risk_level: "medium",
        description: "Reserved for future outbound webhook emission through host-managed delivery controls.",
        manifest_features: &[],
    },
];

#[derive(Clone, Debug, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct PutPluginKvRequestDto {
    pub value: Value,
}

#[derive(Clone, Debug, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct PluginKvValueDto {
    pub key: String,
    pub value: Value,
}

#[derive(Clone, Debug, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct PluginConfigDto {
    pub values: Value,
}

#[derive(Clone, Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct PluginCapabilitiesDto {
    pub api_version: String,
    pub host_api_version: String,
    pub manifest_runtimes: Vec<String>,
    pub executable_runtimes: Vec<String>,
    pub http_schemes: Vec<String>,
    pub permissions: Vec<String>,
    pub permission_details: Vec<PluginPermissionCapabilityDto>,
    pub hook_events: Vec<String>,
    pub host_apis: Vec<PluginHostApiCapabilityDto>,
}

#[derive(Clone, Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct PluginPermissionCapabilityDto {
    pub key: String,
    pub category: String,
    pub risk_level: String,
    pub description: String,
    pub manifest_features: Vec<String>,
    pub host_apis: Vec<PluginHostApiCapabilityDto>,
}

#[derive(Clone, Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct PluginHostApiCapabilityDto {
    pub method: String,
    pub path: String,
    pub required_permission: Option<String>,
}

#[derive(Clone, Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct DeletePluginKvResponseDto {
    pub deleted: bool,
}

#[derive(Clone, Debug, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct SendPluginNotificationRequestDto {
    pub title: String,
    pub message: String,
    #[serde(default)]
    pub level: Option<String>,
    #[serde(default)]
    pub channel: Option<String>,
    #[serde(default)]
    pub metadata: Option<Value>,
}

#[derive(Clone, Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct SendPluginNotificationResponseDto {
    pub request_id: String,
    pub status: String,
}

#[derive(Clone, Debug, Default, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct PatchPluginItemMetadataRequestDto {
    #[serde(default)]
    pub title: Option<String>,
    #[serde(default)]
    pub original_title: Option<String>,
    #[serde(default)]
    pub sort_title: Option<String>,
    #[serde(default)]
    pub overview: Option<String>,
    #[serde(default)]
    pub production_year: Option<i32>,
    #[serde(default)]
    pub premiere_date: Option<String>,
    #[serde(default)]
    pub official_rating: Option<String>,
    #[serde(default)]
    pub community_rating: Option<f64>,
    #[serde(default)]
    pub critic_rating: Option<f64>,
    #[serde(default)]
    pub runtime_ticks: Option<i64>,
    #[serde(default)]
    pub external_ids: Vec<PatchPluginItemExternalIdDto>,
    #[serde(default)]
    pub genres: Option<Vec<String>>,
    #[serde(default)]
    pub studios: Option<Vec<String>>,
    #[serde(default)]
    pub tags: Option<Vec<String>>,
    #[serde(default)]
    pub people: Option<Vec<PatchPluginItemPersonDto>>,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct PatchPluginItemExternalIdDto {
    pub provider: String,
    pub external_id: String,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct PatchPluginItemPersonDto {
    pub name: String,
    pub role_type: String,
    #[serde(default)]
    pub role_name: Option<String>,
    #[serde(default)]
    pub sort_order: Option<i32>,
}

#[derive(Clone, Debug, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct PatchPluginItemMetadataResponseDto {
    pub item: PluginMediaItemDetailDto,
    pub updated_fields: Vec<String>,
    pub external_id_count: usize,
}

#[derive(Clone, Debug, Default, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct PutPluginItemArtworkRequestDto {
    #[serde(default)]
    pub source: Option<String>,
    #[serde(default)]
    pub artwork: Vec<PutPluginItemArtworkDto>,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct PutPluginItemArtworkDto {
    pub artwork_type: String,
    pub remote_url: String,
    #[serde(default)]
    pub width: Option<i32>,
    #[serde(default)]
    pub height: Option<i32>,
    #[serde(default)]
    pub is_primary: bool,
}

#[derive(Clone, Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct PutPluginItemArtworkResponseDto {
    pub item_id: String,
    pub source: String,
    pub artwork_count: usize,
    pub artwork: Vec<PluginArtworkDto>,
}

#[derive(Clone, Debug, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct PutPluginItemMarkersRequestDto {
    #[serde(default)]
    pub source: Option<String>,
    pub markers: Vec<PutPluginItemMarkerDto>,
}

#[derive(Clone, Debug, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct PutPluginItemMarkerDto {
    pub marker_type: String,
    pub start_ticks: i64,
    #[serde(default)]
    pub end_ticks: Option<i64>,
    #[serde(default)]
    pub confidence: Option<f64>,
}

#[derive(Clone, Debug, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct PutPluginItemMarkersResponseDto {
    pub item_id: String,
    pub source: String,
    pub marker_count: usize,
    pub markers: Vec<PluginMediaMarkerDto>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct PluginNotificationInput {
    title: String,
    message: String,
    level: String,
    channel: Option<String>,
    metadata: Value,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct PluginNotificationRecord {
    public_id: String,
    status: String,
}

#[derive(Clone, Debug, Default, PartialEq)]
struct PluginMetadataPatchInput {
    title: Option<String>,
    original_title: Option<String>,
    sort_title: Option<String>,
    overview: Option<String>,
    production_year: Option<i32>,
    premiere_date: Option<String>,
    official_rating: Option<String>,
    community_rating: Option<f64>,
    critic_rating: Option<f64>,
    runtime_ticks: Option<i64>,
    external_ids: Vec<PluginExternalIdInput>,
    genres: Option<Vec<PluginNamedMetadataInput>>,
    studios: Option<Vec<PluginNamedMetadataInput>>,
    tags: Option<Vec<PluginNamedMetadataInput>>,
    people: Option<Vec<PluginPersonInput>>,
    updated_fields: Vec<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct PluginExternalIdInput {
    provider: String,
    external_id: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct PluginNamedMetadataInput {
    name: String,
    name_normalized: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct PluginPersonInput {
    name: String,
    name_normalized: String,
    role_type: String,
    role_name: String,
    sort_order: i32,
}

impl NamedMetadata for PluginNamedMetadataInput {
    fn name(&self) -> &str {
        &self.name
    }

    fn name_normalized(&self) -> &str {
        &self.name_normalized
    }
}

impl PersonMetadata for PluginPersonInput {
    fn name(&self) -> &str {
        &self.name
    }

    fn name_normalized(&self) -> &str {
        &self.name_normalized
    }

    fn role_type(&self) -> &str {
        &self.role_type
    }

    fn role_name(&self) -> &str {
        &self.role_name
    }

    fn sort_order(&self) -> i32 {
        self.sort_order
    }

    // 插件 metadata.write 目前不携带人物头像，profile 图仅来自内置 provider（TMDB）。
    fn profile_image_url(&self) -> Option<&str> {
        None
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct PluginArtworkReplacementInput {
    source: String,
    artwork: Vec<PluginArtworkInput>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct PluginArtworkInput {
    artwork_type: String,
    remote_url: String,
    width: Option<i32>,
    height: Option<i32>,
    is_primary: bool,
}

#[derive(Clone, Debug, PartialEq)]
struct PluginMarkerReplacementInput {
    source: String,
    markers: Vec<PluginMediaMarkerInput>,
}

#[derive(Clone, Debug, PartialEq)]
struct PluginMediaMarkerInput {
    marker_type: String,
    start_ticks: i64,
    end_ticks: Option<i64>,
    confidence: Option<f64>,
}

#[derive(Debug)]
enum PluginHostConfigError {
    Database(sqlx::Error),
    Secret(SecretError),
    UnsupportedSecretAlgorithm(String),
}

#[derive(Debug)]
enum PluginMetadataWriteError {
    Database(sqlx::Error),
    ExternalIdConflict {
        provider: String,
        external_id: String,
    },
}

#[derive(Clone, Debug, Default, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ListPluginLibraryItemsQueryDto {
    pub start_index: Option<u32>,
    pub limit: Option<u32>,
    pub cursor: Option<String>,
}

#[derive(Clone, Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct PluginLibraryDto {
    pub id: String,
    pub name: String,
    pub library_type: String,
}

#[derive(Clone, Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct PluginMediaItemDto {
    pub id: String,
    pub library_id: String,
    pub parent_id: Option<String>,
    pub item_type: String,
    pub title: String,
    pub production_year: Option<i32>,
    pub runtime_ticks: Option<i64>,
}

#[derive(Clone, Debug, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct PluginMediaItemDetailDto {
    pub id: String,
    pub library_id: String,
    pub parent_id: Option<String>,
    pub item_type: String,
    pub title: String,
    pub original_title: Option<String>,
    pub sort_title: Option<String>,
    pub overview: Option<String>,
    pub production_year: Option<i32>,
    pub premiere_date: Option<String>,
    pub official_rating: Option<String>,
    pub community_rating: Option<f64>,
    pub critic_rating: Option<f64>,
    pub runtime_ticks: Option<i64>,
    pub index_number: Option<i32>,
    pub parent_index_number: Option<i32>,
    pub season_number: Option<i32>,
    pub episode_number: Option<i32>,
    pub metadata_status: String,
    pub scan_status: String,
    pub external_ids: Vec<PluginMediaExternalIdDto>,
    pub genres: Vec<String>,
    pub studios: Vec<String>,
    pub tags: Vec<String>,
    pub people: Vec<PluginPersonDto>,
    pub markers: Vec<PluginMediaMarkerDto>,
    pub artwork: Vec<PluginArtworkDto>,
}

#[derive(Clone, Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct PluginMediaExternalIdDto {
    pub provider: String,
    pub external_id: String,
}

#[derive(Clone, Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct PluginPersonDto {
    pub id: String,
    pub name: String,
    pub role_type: String,
    pub role_name: String,
    pub sort_order: i32,
}

#[derive(Clone, Debug, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct PluginMediaMarkerDto {
    pub marker_type: String,
    pub start_ticks: i64,
    pub end_ticks: Option<i64>,
    pub source: String,
    pub confidence: Option<f64>,
}

#[derive(Clone, Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct PluginArtworkDto {
    pub artwork_type: String,
    pub source: String,
    pub has_local_image: bool,
    pub remote_url: Option<String>,
    pub width: Option<i32>,
    pub height: Option<i32>,
    pub is_primary: bool,
}

#[derive(Clone, Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct PluginMediaItemsResultDto {
    pub items: Vec<PluginMediaItemDto>,
    pub total_record_count: u32,
    pub total_record_count_is_exact: bool,
    pub start_index: u32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub next_cursor: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct PluginItemCursor {
    sort_key: String,
    cursor_id: i64,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct PluginItemWindow {
    start_index: i64,
    limit: i64,
    cursor: Option<PluginItemCursor>,
}

impl PluginHostRepository {
    pub fn new(pool: DbPool) -> Self {
        Self { pool }
    }

    pub async fn issue_execution_token(
        &self,
        plugin_id: &str,
        package_id: &str,
        execution_run_id: i64,
    ) -> Result<IssuedPluginHostToken, sqlx::Error> {
        let issued = issue_access_token();
        let row = sqlx::query(
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
            select $1,
                   $2,
                   $3,
                   $4,
                   $5,
                   coalesce(
                       jsonb_agg(
                           jsonb_build_object(
                               'key', permission.permission_key,
                               'scope', permission.permission_scope
                           )
                           order by permission.permission_key, permission.permission_scope nulls first
                       ) filter (where permission.id is not null),
                       '[]'::jsonb
                   ),
                   now() + ($6::bigint * interval '1 second')
            from plugin_packages pkg
            left join plugin_permissions permission on permission.package_id = pkg.id
            where pkg.public_id = case
                when $4::text ~* '^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$'
                then $4::uuid
                else null::uuid
            end
            group by pkg.id
            returning id
            "#,
        )
        .bind(&issued.hash)
        .bind(&issued.prefix)
        .bind(plugin_id.trim())
        .bind(package_id.trim())
        .bind(execution_run_id)
        .bind(HOST_TOKEN_TTL_SECONDS)
        .fetch_one(&self.pool)
        .await?;

        Ok(IssuedPluginHostToken {
            id: row.try_get("id")?,
            token: issued.token,
            prefix: issued.prefix,
        })
    }

    pub async fn revoke_token(&self, token_id: i64) -> Result<(), sqlx::Error> {
        sqlx::query(
            r#"
            update plugin_host_tokens
            set revoked_at = coalesce(revoked_at, now())
            where id = $1
            "#,
        )
        .bind(token_id)
        .execute(&self.pool)
        .await?;

        Ok(())
    }

    async fn record_host_api_call(
        &self,
        context: &PluginHostContext,
        audit: PluginHostApiCallAudit,
    ) -> Result<(), sqlx::Error> {
        let duration_ms = duration_millis_i32(audit.duration);
        let error_message = audit.error_message.map(truncate_audit_error);
        sqlx::query(
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
                $5,
                $6,
                $7,
                $8,
                $9,
                $10,
                now() - ($11::bigint * interval '1 millisecond'),
                now(),
                $11
            )
            "#,
        )
        .bind(&context.plugin_id)
        .bind(&context.package_id)
        .bind(context.token_id)
        .bind(context.execution_run_id)
        .bind(audit.route.method)
        .bind(audit.route.path)
        .bind(audit.route.required_permission)
        .bind(audit.status_code.as_u16() as i32)
        .bind(audit.error_code)
        .bind(error_message.as_deref())
        .bind(duration_ms)
        .execute(&self.pool)
        .await?;

        Ok(())
    }

    async fn host_api_call_limit_reached(
        &self,
        context: &PluginHostContext,
        max_calls: u32,
    ) -> Result<bool, sqlx::Error> {
        let row = sqlx::query(
            r#"
            select count(*) as call_count
            from (
                select 1
                from plugin_host_api_calls
                where execution_run_id = $1
                  and plugin_id = $2
                limit $3
            ) counted
            "#,
        )
        .bind(context.execution_run_id)
        .bind(&context.plugin_id)
        .bind(i64::from(max_calls))
        .fetch_one(&self.pool)
        .await?;

        let call_count = row.try_get::<i64, _>("call_count")?;
        Ok(call_count >= i64::from(max_calls))
    }

    async fn authenticate_token(
        &self,
        token: &str,
    ) -> Result<Option<PluginHostContext>, sqlx::Error> {
        let row = sqlx::query(AUTHENTICATE_PLUGIN_HOST_TOKEN_SQL)
            .bind(hash_token(token))
            .fetch_optional(&self.pool)
            .await?;

        row.map(|row| {
            let permission_snapshot = row.try_get::<Value, _>("permission_snapshot")?;
            Ok(PluginHostContext {
                token_id: row.try_get("token_id")?,
                plugin_id: row.try_get("plugin_id")?,
                package_id: row.try_get("package_id")?,
                execution_run_id: row.try_get("execution_run_id")?,
                permission_keys: permission_keys_from_snapshot(&permission_snapshot),
            })
        })
        .transpose()
    }

    async fn get_config(
        &self,
        plugin_id: &str,
        cipher: Option<&SecretCipher>,
    ) -> Result<Value, PluginHostConfigError> {
        let row = sqlx::query(
            r#"
            select config
            from plugin_installations
            where plugin_id = $1
              and enabled = true
              and approval_status = 'approved'
            "#,
        )
        .bind(plugin_id.trim())
        .fetch_one(&self.pool)
        .await?;
        let config = row.try_get::<Value, _>("config")?;
        if !contains_secret_refs(&config) {
            return Ok(config);
        }

        let Some(cipher) = cipher else {
            return Err(PluginHostConfigError::Secret(SecretError::MissingKey));
        };
        let secrets = self.load_config_secret_values(plugin_id, cipher).await?;
        materialize_secret_refs(&config, &secrets).map_err(PluginHostConfigError::Secret)
    }

    async fn load_config_secret_values(
        &self,
        plugin_id: &str,
        cipher: &SecretCipher,
    ) -> Result<HashMap<String, String>, PluginHostConfigError> {
        let rows = sqlx::query(
            r#"
            select secret_key,
                   algorithm,
                   nonce,
                   ciphertext
            from plugin_config_secrets
            where plugin_id = $1
            order by secret_key
            "#,
        )
        .bind(plugin_id.trim())
        .fetch_all(&self.pool)
        .await?;

        let mut secrets = HashMap::with_capacity(rows.len());
        for row in rows {
            let secret_key = row.try_get::<String, _>("secret_key")?;
            let algorithm = row.try_get::<String, _>("algorithm")?;
            if algorithm != SECRET_ALGORITHM {
                return Err(PluginHostConfigError::UnsupportedSecretAlgorithm(algorithm));
            }
            let nonce = row.try_get::<Vec<u8>, _>("nonce")?;
            let ciphertext = row.try_get::<Vec<u8>, _>("ciphertext")?;
            let value = cipher
                .decrypt_scoped(
                    "plugin-config",
                    plugin_id.trim(),
                    &secret_key,
                    &nonce,
                    &ciphertext,
                )
                .map_err(PluginHostConfigError::Secret)?;
            secrets.insert(secret_key, value);
        }

        Ok(secrets)
    }

    async fn list_libraries(&self) -> Result<Vec<PluginLibraryDto>, sqlx::Error> {
        let rows = sqlx::query(
            r#"
            select
                public_id::text as id,
                name,
                library_type
            from libraries
            where is_hidden = false
            order by name, id
            limit 1000
            "#,
        )
        .fetch_all(&self.pool)
        .await?;

        rows.into_iter()
            .map(|row| {
                Ok(PluginLibraryDto {
                    id: row.try_get("id")?,
                    name: row.try_get("name")?,
                    library_type: row.try_get("library_type")?,
                })
            })
            .collect()
    }

    async fn library_exists(&self, library_id: &str) -> Result<bool, sqlx::Error> {
        let row = sqlx::query(PLUGIN_LIBRARY_EXISTS_SQL)
            .bind(library_id)
            .fetch_one(&self.pool)
            .await?;

        row.try_get("found")
    }

    async fn list_library_items(
        &self,
        library_id: &str,
        window: PluginItemWindow,
    ) -> Result<PluginMediaItemsResultDto, sqlx::Error> {
        let (rows, total_record_count, total_record_count_is_exact) =
            self.fetch_library_item_rows(library_id, &window).await?;
        let has_more = rows.len() > window.limit as usize;
        let rows = rows
            .into_iter()
            .take(window.limit as usize)
            .collect::<Vec<_>>();
        let next_cursor = if has_more {
            rows.last()
                .map(|row| {
                    let sort_key = row.try_get::<String, _>("sort_key")?;
                    let cursor_id = row.try_get::<i64, _>("cursor_id")?;
                    Ok::<String, sqlx::Error>(encode_plugin_item_cursor(&sort_key, cursor_id))
                })
                .transpose()?
        } else {
            None
        };
        let items = rows
            .into_iter()
            .map(|row| {
                Ok(PluginMediaItemDto {
                    id: row.try_get("id")?,
                    library_id: row.try_get("library_id")?,
                    parent_id: row.try_get("parent_id")?,
                    item_type: row.try_get("item_type")?,
                    title: row.try_get("title")?,
                    production_year: row.try_get("production_year")?,
                    runtime_ticks: row.try_get("runtime_ticks")?,
                })
            })
            .collect::<Result<Vec<_>, sqlx::Error>>()?;

        Ok(PluginMediaItemsResultDto {
            items,
            total_record_count,
            total_record_count_is_exact,
            start_index: window.start_index as u32,
            next_cursor,
        })
    }

    async fn fetch_library_item_rows(
        &self,
        library_id: &str,
        window: &PluginItemWindow,
    ) -> Result<(Vec<PgRow>, u32, bool), sqlx::Error> {
        let rows = if let Some(cursor) = &window.cursor {
            sqlx::query(PLUGIN_LIBRARY_ITEMS_AFTER_CURSOR_SQL)
                .bind(library_id)
                .bind(cursor.sort_key.as_str())
                .bind(cursor.cursor_id)
                .bind(window.fetch_limit())
                .fetch_all(&self.pool)
                .await?
        } else {
            sqlx::query(PLUGIN_LIBRARY_ITEMS_FIRST_PAGE_SQL)
                .bind(library_id)
                .bind(window.fetch_limit())
                .fetch_all(&self.pool)
                .await?
        };
        let visible_lower_bound = rows.len().min(window.fetch_limit() as usize) as u32;
        Ok((rows, visible_lower_bound, false))
    }

    async fn get_item(
        &self,
        item_id: &str,
    ) -> Result<Option<PluginMediaItemDetailDto>, sqlx::Error> {
        let Some(row) = sqlx::query(
            r#"
            select
                mi.id as internal_id,
                mi.public_id::text as id,
                l.public_id::text as library_id,
                parent.public_id::text as parent_id,
                mi.item_type,
                mi.title,
                mi.original_title,
                mi.sort_title,
                mi.overview,
                mi.production_year,
                mi.premiere_date::text as premiere_date,
                mi.official_rating,
                mi.community_rating::float8 as community_rating,
                mi.critic_rating::float8 as critic_rating,
                mi.runtime_ticks,
                mi.index_number,
                mi.parent_index_number,
                mi.season_number,
                mi.episode_number,
                mi.metadata_status,
                mi.scan_status
            from media_items mi
            join libraries l on l.id = mi.library_id
            left join media_items parent on parent.id = mi.parent_id
            where mi.public_id = case
                when $1 ~* '^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$'
                then $1::uuid
                else null::uuid
            end
              and mi.is_deleted = false
              and l.is_hidden = false
            limit 1
            "#,
        )
        .bind(item_id)
        .fetch_optional(&self.pool)
        .await?
        else {
            return Ok(None);
        };

        let internal_id = row.try_get::<i64, _>("internal_id")?;
        let external_ids = self.load_item_external_ids(internal_id).await?;
        let genres = self.load_item_genres(internal_id).await?;
        let studios = self.load_item_studios(internal_id).await?;
        let tags = self.load_item_tags(internal_id).await?;
        let people = self.load_item_people(internal_id).await?;
        let markers = self.load_item_markers(internal_id).await?;
        let artwork = self.load_item_artwork(internal_id).await?;

        Ok(Some(PluginMediaItemDetailDto {
            id: row.try_get("id")?,
            library_id: row.try_get("library_id")?,
            parent_id: row.try_get("parent_id")?,
            item_type: row.try_get("item_type")?,
            title: row.try_get("title")?,
            original_title: row.try_get("original_title")?,
            sort_title: row.try_get("sort_title")?,
            overview: row.try_get("overview")?,
            production_year: row.try_get("production_year")?,
            premiere_date: row.try_get("premiere_date")?,
            official_rating: row.try_get("official_rating")?,
            community_rating: row.try_get("community_rating")?,
            critic_rating: row.try_get("critic_rating")?,
            runtime_ticks: row.try_get("runtime_ticks")?,
            index_number: row.try_get("index_number")?,
            parent_index_number: row.try_get("parent_index_number")?,
            season_number: row.try_get("season_number")?,
            episode_number: row.try_get("episode_number")?,
            metadata_status: row.try_get("metadata_status")?,
            scan_status: row.try_get("scan_status")?,
            external_ids,
            genres,
            studios,
            tags,
            people,
            markers,
            artwork,
        }))
    }

    async fn patch_item_metadata(
        &self,
        item_id: &str,
        input: &PluginMetadataPatchInput,
    ) -> Result<Option<PluginMediaItemDetailDto>, PluginMetadataWriteError> {
        let mut tx = self.pool.begin().await?;
        let Some(row) = sqlx::query(
            r#"
            select mi.id
            from media_items mi
            join libraries l on l.id = mi.library_id
            where mi.public_id = case
                when $1 ~* '^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$'
                then $1::uuid
                else null::uuid
            end
              and mi.is_deleted = false
              and l.is_hidden = false
            for update of mi
            limit 1
            "#,
        )
        .bind(item_id)
        .fetch_optional(&mut *tx)
        .await?
        else {
            return Ok(None);
        };
        let internal_id = row.try_get::<i64, _>("id")?;
        let provider_fingerprint = input
            .external_ids
            .first()
            .map(|external_id| format!("{}:{}", external_id.provider, external_id.external_id));

        sqlx::query(
            r#"
            update media_items
            set title = coalesce($2, title),
                original_title = coalesce($3, original_title),
                sort_title = coalesce($4, sort_title),
                overview = coalesce($5, overview),
                production_year = coalesce($6, production_year),
                premiere_date = coalesce($7::date, premiere_date),
                official_rating = coalesce($8, official_rating),
                community_rating = coalesce($9::numeric, community_rating),
                critic_rating = coalesce($10::numeric, critic_rating),
                runtime_ticks = coalesce($11, runtime_ticks),
                provider_fingerprint = coalesce($12, provider_fingerprint),
                metadata_status = 'manual',
                updated_at = now()
            where id = $1
            "#,
        )
        .bind(internal_id)
        .bind(input.title.as_deref())
        .bind(input.original_title.as_deref())
        .bind(input.sort_title.as_deref())
        .bind(input.overview.as_deref())
        .bind(input.production_year)
        .bind(input.premiere_date.as_deref())
        .bind(input.official_rating.as_deref())
        .bind(input.community_rating)
        .bind(input.critic_rating)
        .bind(input.runtime_ticks)
        .bind(provider_fingerprint.as_deref())
        .execute(&mut *tx)
        .await?;

        for external_id in &input.external_ids {
            let conflict = sqlx::query_scalar::<_, bool>(
                r#"
                select exists (
                    select 1
                    from media_external_ids
                    where provider = $1
                      and external_id = $2
                      and media_item_id <> $3
                )
                "#,
            )
            .bind(&external_id.provider)
            .bind(&external_id.external_id)
            .bind(internal_id)
            .fetch_one(&mut *tx)
            .await?;
            if conflict {
                return Err(PluginMetadataWriteError::ExternalIdConflict {
                    provider: external_id.provider.clone(),
                    external_id: external_id.external_id.clone(),
                });
            }

            sqlx::query(
                r#"
                insert into media_external_ids (
                    media_item_id,
                    provider,
                    external_id
                )
                values ($1, $2, $3)
                on conflict (media_item_id, provider) do update
                    set external_id = excluded.external_id
                "#,
            )
            .bind(internal_id)
            .bind(&external_id.provider)
            .bind(&external_id.external_id)
            .execute(&mut *tx)
            .await?;
        }

        if let Some(genres) = &input.genres {
            replace_item_genres(&mut tx, internal_id, genres).await?;
        }

        if let Some(tags) = &input.tags {
            sqlx::query(
                r#"
                delete from media_item_tags
                where media_item_id = $1
                "#,
            )
            .bind(internal_id)
            .execute(&mut *tx)
            .await?;

            for tag in tags {
                let tag_id = sqlx::query_scalar::<_, i64>(
                    r#"
                    insert into tags (name, name_normalized)
                    values ($1, $2)
                    on conflict (name_normalized) do update
                        set name = tags.name
                    returning id
                    "#,
                )
                .bind(&tag.name)
                .bind(&tag.name_normalized)
                .fetch_one(&mut *tx)
                .await?;

                sqlx::query(
                    r#"
                    insert into media_item_tags (media_item_id, tag_id)
                    values ($1, $2)
                    on conflict do nothing
                    "#,
                )
                .bind(internal_id)
                .bind(tag_id)
                .execute(&mut *tx)
                .await?;
            }
        }

        if let Some(studios) = &input.studios {
            replace_item_studios(&mut tx, internal_id, studios).await?;
        }

        if let Some(people) = &input.people {
            replace_item_people(&mut tx, internal_id, people).await?;
        }

        tx.commit().await?;

        self.get_item(item_id)
            .await
            .map_err(PluginMetadataWriteError::Database)
    }

    async fn load_item_external_ids(
        &self,
        internal_id: i64,
    ) -> Result<Vec<PluginMediaExternalIdDto>, sqlx::Error> {
        let rows = sqlx::query(
            r#"
            select provider, external_id
            from media_external_ids
            where media_item_id = $1
            order by provider, external_id
            "#,
        )
        .bind(internal_id)
        .fetch_all(&self.pool)
        .await?;

        rows.into_iter()
            .map(|row| {
                Ok(PluginMediaExternalIdDto {
                    provider: row.try_get("provider")?,
                    external_id: row.try_get("external_id")?,
                })
            })
            .collect()
    }

    async fn load_item_genres(&self, internal_id: i64) -> Result<Vec<String>, sqlx::Error> {
        let rows = sqlx::query(
            r#"
            select g.name
            from media_item_genres mig
            join genres g on g.id = mig.genre_id
            where mig.media_item_id = $1
            order by g.name
            "#,
        )
        .bind(internal_id)
        .fetch_all(&self.pool)
        .await?;

        rows.into_iter().map(|row| row.try_get("name")).collect()
    }

    async fn load_item_studios(&self, internal_id: i64) -> Result<Vec<String>, sqlx::Error> {
        let rows = sqlx::query(
            r#"
            select s.name
            from media_item_studios mis
            join studios s on s.id = mis.studio_id
            where mis.media_item_id = $1
            order by s.name
            "#,
        )
        .bind(internal_id)
        .fetch_all(&self.pool)
        .await?;

        rows.into_iter().map(|row| row.try_get("name")).collect()
    }

    async fn load_item_tags(&self, internal_id: i64) -> Result<Vec<String>, sqlx::Error> {
        let rows = sqlx::query(
            r#"
            select t.name
            from media_item_tags mit
            join tags t on t.id = mit.tag_id
            where mit.media_item_id = $1
            order by t.name
            "#,
        )
        .bind(internal_id)
        .fetch_all(&self.pool)
        .await?;

        rows.into_iter().map(|row| row.try_get("name")).collect()
    }

    async fn load_item_people(
        &self,
        internal_id: i64,
    ) -> Result<Vec<PluginPersonDto>, sqlx::Error> {
        let rows = sqlx::query(
            r#"
            select
                p.public_id::text as id,
                p.name,
                mip.role_type,
                mip.role_name,
                mip.sort_order
            from media_item_people mip
            join people p on p.id = mip.person_id
            where mip.media_item_id = $1
            order by mip.role_type, mip.sort_order, p.name, mip.id
            "#,
        )
        .bind(internal_id)
        .fetch_all(&self.pool)
        .await?;

        rows.into_iter()
            .map(|row| {
                Ok(PluginPersonDto {
                    id: row.try_get("id")?,
                    name: row.try_get("name")?,
                    role_type: row.try_get("role_type")?,
                    role_name: row.try_get("role_name")?,
                    sort_order: row.try_get("sort_order")?,
                })
            })
            .collect()
    }

    async fn load_item_markers(
        &self,
        internal_id: i64,
    ) -> Result<Vec<PluginMediaMarkerDto>, sqlx::Error> {
        let rows = sqlx::query(
            r#"
            select
                marker_type,
                start_ticks,
                end_ticks,
                source,
                confidence::float8 as confidence
            from media_markers
            where media_item_id = $1
            order by start_ticks, marker_type, source
            "#,
        )
        .bind(internal_id)
        .fetch_all(&self.pool)
        .await?;

        rows.into_iter()
            .map(|row| {
                Ok(PluginMediaMarkerDto {
                    marker_type: row.try_get("marker_type")?,
                    start_ticks: row.try_get("start_ticks")?,
                    end_ticks: row.try_get("end_ticks")?,
                    source: row.try_get("source")?,
                    confidence: row.try_get("confidence")?,
                })
            })
            .collect()
    }

    async fn load_item_artwork(
        &self,
        internal_id: i64,
    ) -> Result<Vec<PluginArtworkDto>, sqlx::Error> {
        let rows = sqlx::query(
            r#"
            select
                artwork_type,
                source,
                storage_key is not null as has_local_image,
                remote_url,
                width,
                height,
                is_primary
            from artwork
            where media_item_id = $1
            order by artwork_type, is_primary desc, id
            "#,
        )
        .bind(internal_id)
        .fetch_all(&self.pool)
        .await?;

        rows.into_iter()
            .map(|row| {
                Ok(PluginArtworkDto {
                    artwork_type: row.try_get("artwork_type")?,
                    source: row.try_get("source")?,
                    has_local_image: row.try_get("has_local_image")?,
                    remote_url: row.try_get("remote_url")?,
                    width: row.try_get("width")?,
                    height: row.try_get("height")?,
                    is_primary: row.try_get("is_primary")?,
                })
            })
            .collect()
    }

    async fn replace_item_artwork(
        &self,
        item_id: &str,
        input: &PluginArtworkReplacementInput,
    ) -> Result<Option<Vec<PluginArtworkDto>>, sqlx::Error> {
        let mut tx = self.pool.begin().await?;
        let Some(row) = sqlx::query(
            r#"
            select mi.id
            from media_items mi
            join libraries l on l.id = mi.library_id
            where mi.public_id = case
                when $1 ~* '^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$'
                then $1::uuid
                else null::uuid
            end
              and mi.is_deleted = false
              and l.is_hidden = false
            for update of mi
            limit 1
            "#,
        )
        .bind(item_id)
        .fetch_optional(&mut *tx)
        .await?
        else {
            return Ok(None);
        };
        let internal_id = row.try_get::<i64, _>("id")?;

        sqlx::query(
            r#"
            delete from artwork
            where media_item_id = $1
              and source = $2
            "#,
        )
        .bind(internal_id)
        .bind(&input.source)
        .execute(&mut *tx)
        .await?;

        let mut artwork = Vec::with_capacity(input.artwork.len());
        for image in &input.artwork {
            let row = sqlx::query(
                r#"
                insert into artwork (
                    media_item_id,
                    artwork_type,
                    source,
                    remote_url,
                    width,
                    height,
                    is_primary
                )
                values ($1, $2, $3, $4, $5, $6, $7)
                returning
                    artwork_type,
                    source,
                    storage_key is not null as has_local_image,
                    remote_url,
                    width,
                    height,
                    is_primary
                "#,
            )
            .bind(internal_id)
            .bind(&image.artwork_type)
            .bind(&input.source)
            .bind(&image.remote_url)
            .bind(image.width)
            .bind(image.height)
            .bind(image.is_primary)
            .fetch_one(&mut *tx)
            .await?;

            artwork.push(PluginArtworkDto {
                artwork_type: row.try_get("artwork_type")?,
                source: row.try_get("source")?,
                has_local_image: row.try_get("has_local_image")?,
                remote_url: row.try_get("remote_url")?,
                width: row.try_get("width")?,
                height: row.try_get("height")?,
                is_primary: row.try_get("is_primary")?,
            });
        }

        tx.commit().await?;

        Ok(Some(artwork))
    }

    async fn replace_item_markers(
        &self,
        item_id: &str,
        input: &PluginMarkerReplacementInput,
    ) -> Result<Option<Vec<PluginMediaMarkerDto>>, sqlx::Error> {
        let mut tx = self.pool.begin().await?;
        let Some(row) = sqlx::query(
            r#"
            select mi.id
            from media_items mi
            join libraries l on l.id = mi.library_id
            where mi.public_id = case
                when $1 ~* '^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$'
                then $1::uuid
                else null::uuid
            end
              and mi.is_deleted = false
              and l.is_hidden = false
            for update of mi
            limit 1
            "#,
        )
        .bind(item_id)
        .fetch_optional(&mut *tx)
        .await?
        else {
            return Ok(None);
        };
        let internal_id = row.try_get::<i64, _>("id")?;

        sqlx::query(
            r#"
            delete from media_markers
            where media_item_id = $1
              and source = $2
            "#,
        )
        .bind(internal_id)
        .bind(&input.source)
        .execute(&mut *tx)
        .await?;

        let mut markers = Vec::with_capacity(input.markers.len());
        for marker in &input.markers {
            let row = sqlx::query(
                r#"
                insert into media_markers (
                    media_item_id,
                    marker_type,
                    start_ticks,
                    end_ticks,
                    source,
                    confidence
                )
                values ($1, $2, $3, $4, $5, $6::numeric)
                returning
                    marker_type,
                    start_ticks,
                    end_ticks,
                    source,
                    confidence::float8 as confidence
                "#,
            )
            .bind(internal_id)
            .bind(&marker.marker_type)
            .bind(marker.start_ticks)
            .bind(marker.end_ticks)
            .bind(&input.source)
            .bind(marker.confidence)
            .fetch_one(&mut *tx)
            .await?;

            markers.push(PluginMediaMarkerDto {
                marker_type: row.try_get("marker_type")?,
                start_ticks: row.try_get("start_ticks")?,
                end_ticks: row.try_get("end_ticks")?,
                source: row.try_get("source")?,
                confidence: row.try_get("confidence")?,
            });
        }

        tx.commit().await?;

        Ok(Some(markers))
    }

    async fn enqueue_notification(
        &self,
        context: &PluginHostContext,
        input: PluginNotificationInput,
    ) -> Result<PluginNotificationRecord, sqlx::Error> {
        let mut tx = self.pool.begin().await?;
        let request_row = sqlx::query(
            r#"
            insert into plugin_notification_requests (
                plugin_id,
                package_id,
                title,
                message,
                level,
                channel,
                metadata
            )
            values ($1, $2, $3, $4, $5, $6, $7)
            returning
                id,
                public_id::text as public_id,
                status
            "#,
        )
        .bind(&context.plugin_id)
        .bind(&context.package_id)
        .bind(&input.title)
        .bind(&input.message)
        .bind(&input.level)
        .bind(input.channel.as_deref())
        .bind(&input.metadata)
        .fetch_one(&mut *tx)
        .await?;

        let request_id = request_row.try_get::<i64, _>("id")?;
        let public_id = request_row.try_get::<String, _>("public_id")?;
        let status = request_row.try_get::<String, _>("status")?;
        let payload = json!({
            "requestId": public_id,
            "pluginId": context.plugin_id,
            "packageId": context.package_id,
            "title": input.title,
            "message": input.message,
            "level": input.level,
            "channel": input.channel,
            "metadata": input.metadata,
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

        sqlx::query(
            r#"
            update plugin_notification_requests
            set outbox_event_id = $2,
                updated_at = now()
            where id = $1
            "#,
        )
        .bind(request_id)
        .bind(outbox_id)
        .execute(&mut *tx)
        .await?;

        tx.commit().await?;

        Ok(PluginNotificationRecord { public_id, status })
    }

    async fn get_kv(&self, plugin_id: &str, key: &str) -> Result<Option<Value>, sqlx::Error> {
        let row = sqlx::query(
            r#"
            select value
            from plugin_kv
            where plugin_id = $1
              and key = $2
            "#,
        )
        .bind(plugin_id)
        .bind(key)
        .fetch_optional(&self.pool)
        .await?;

        row.map(|row| row.try_get("value")).transpose()
    }

    async fn put_kv(
        &self,
        plugin_id: &str,
        key: &str,
        value: &Value,
    ) -> Result<Value, sqlx::Error> {
        let row = sqlx::query(
            r#"
            insert into plugin_kv (plugin_id, key, value)
            values ($1, $2, $3)
            on conflict (plugin_id, key) do update
                set value = excluded.value,
                    updated_at = now()
            returning value
            "#,
        )
        .bind(plugin_id)
        .bind(key)
        .bind(value)
        .fetch_one(&self.pool)
        .await?;

        row.try_get("value")
    }

    async fn delete_kv(&self, plugin_id: &str, key: &str) -> Result<bool, sqlx::Error> {
        let result = sqlx::query(
            r#"
            delete from plugin_kv
            where plugin_id = $1
              and key = $2
            "#,
        )
        .bind(plugin_id)
        .bind(key)
        .execute(&self.pool)
        .await?;

        Ok(result.rows_affected() > 0)
    }
}

async fn get_capabilities(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<Json<PluginCapabilitiesDto>, AppError> {
    let Some(database) = state.database() else {
        return Err(AppError::internal("database is not configured"));
    };
    let repository = PluginHostRepository::new(database.clone());
    let context = authenticate_plugin_host(&repository, &headers).await?;
    let started = Instant::now();
    let max_calls = state.config().plugins.host_api_max_calls_per_run;
    let result = async {
        enforce_host_api_call_budget(&repository, &context, max_calls).await?;
        Ok(Json(plugin_capabilities()))
    }
    .await;

    finish_audited_host_api(
        &repository,
        &context,
        HOST_API_CAPABILITIES,
        started,
        result,
    )
    .await
}

async fn get_config(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<Json<PluginConfigDto>, AppError> {
    let Some(database) = state.database() else {
        return Err(AppError::internal("database is not configured"));
    };
    let repository = PluginHostRepository::new(database.clone());
    let context = authenticate_plugin_host(&repository, &headers).await?;
    let started = Instant::now();
    let max_calls = state.config().plugins.host_api_max_calls_per_run;
    let result = async {
        enforce_host_api_call_budget(&repository, &context, max_calls).await?;
        let cipher = SecretCipher::from_config(&state.config().secrets).ok();
        let values = repository
            .get_config(&context.plugin_id, cipher.as_ref())
            .await
            .map_err(host_config_error)?;

        Ok(Json(PluginConfigDto { values }))
    }
    .await;

    finish_audited_host_api(&repository, &context, HOST_API_CONFIG, started, result).await
}

async fn send_notification(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(payload): Json<SendPluginNotificationRequestDto>,
) -> Result<(StatusCode, Json<SendPluginNotificationResponseDto>), AppError> {
    let Some(database) = state.database() else {
        return Err(AppError::internal("database is not configured"));
    };
    let repository = PluginHostRepository::new(database.clone());
    let context = authenticate_plugin_host(&repository, &headers).await?;
    let started = Instant::now();
    let max_calls = state.config().plugins.host_api_max_calls_per_run;
    let result = async {
        enforce_host_api_call_budget(&repository, &context, max_calls).await?;
        require_plugin_permission(&context, "notification.send")?;
        let input = PluginNotificationInput::try_from(payload)?;
        let record = repository
            .enqueue_notification(&context, input)
            .await
            .map_err(host_database_error)?;

        Ok((
            StatusCode::ACCEPTED,
            Json(SendPluginNotificationResponseDto {
                request_id: record.public_id,
                status: record.status,
            }),
        ))
    }
    .await;

    finish_audited_host_api(
        &repository,
        &context,
        HOST_API_SEND_NOTIFICATION,
        started,
        result,
    )
    .await
}

async fn list_libraries(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<Json<Vec<PluginLibraryDto>>, AppError> {
    let Some(database) = state.database() else {
        return Err(AppError::internal("database is not configured"));
    };
    let repository = PluginHostRepository::new(database.clone());
    let context = authenticate_plugin_host(&repository, &headers).await?;
    let started = Instant::now();
    let max_calls = state.config().plugins.host_api_max_calls_per_run;
    let result = async {
        enforce_host_api_call_budget(&repository, &context, max_calls).await?;
        require_plugin_permission(&context, "library.read")?;
        let libraries = repository
            .list_libraries()
            .await
            .map_err(host_database_error)?;

        Ok(Json(libraries))
    }
    .await;

    finish_audited_host_api(
        &repository,
        &context,
        HOST_API_LIST_LIBRARIES,
        started,
        result,
    )
    .await
}

async fn list_library_items(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(library_id): Path<String>,
    Query(query): Query<ListPluginLibraryItemsQueryDto>,
) -> Result<Json<PluginMediaItemsResultDto>, AppError> {
    let Some(database) = state.database() else {
        return Err(AppError::internal("database is not configured"));
    };
    let repository = PluginHostRepository::new(database.clone());
    let context = authenticate_plugin_host(&repository, &headers).await?;
    let started = Instant::now();
    let max_calls = state.config().plugins.host_api_max_calls_per_run;
    let result = async {
        enforce_host_api_call_budget(&repository, &context, max_calls).await?;
        require_plugin_permission(&context, "library.read")?;
        let library_id = validate_public_id("libraryId", &library_id)?;
        if !repository
            .library_exists(library_id)
            .await
            .map_err(host_database_error)?
        {
            return Err(AppError::not_found("library not found"));
        }

        let window = PluginItemWindow::from_query(&query)?;
        let result = repository
            .list_library_items(library_id, window)
            .await
            .map_err(host_database_error)?;

        Ok(Json(result))
    }
    .await;

    finish_audited_host_api(
        &repository,
        &context,
        HOST_API_LIST_LIBRARY_ITEMS,
        started,
        result,
    )
    .await
}

async fn get_item(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(item_id): Path<String>,
) -> Result<Json<PluginMediaItemDetailDto>, AppError> {
    let Some(database) = state.database() else {
        return Err(AppError::internal("database is not configured"));
    };
    let repository = PluginHostRepository::new(database.clone());
    let context = authenticate_plugin_host(&repository, &headers).await?;
    let started = Instant::now();
    let max_calls = state.config().plugins.host_api_max_calls_per_run;
    let result = async {
        enforce_host_api_call_budget(&repository, &context, max_calls).await?;
        require_plugin_permission(&context, "media.read")?;
        let item_id = validate_public_id("itemId", &item_id)?;
        let Some(item) = repository
            .get_item(item_id)
            .await
            .map_err(host_database_error)?
        else {
            return Err(AppError::not_found("media item not found"));
        };

        Ok(Json(item))
    }
    .await;

    finish_audited_host_api(&repository, &context, HOST_API_GET_ITEM, started, result).await
}

async fn patch_item_metadata(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(item_id): Path<String>,
    Json(payload): Json<PatchPluginItemMetadataRequestDto>,
) -> Result<Json<PatchPluginItemMetadataResponseDto>, AppError> {
    let Some(database) = state.database() else {
        return Err(AppError::internal("database is not configured"));
    };
    let repository = PluginHostRepository::new(database.clone());
    let context = authenticate_plugin_host(&repository, &headers).await?;
    let started = Instant::now();
    let max_calls = state.config().plugins.host_api_max_calls_per_run;
    let result = async {
        enforce_host_api_call_budget(&repository, &context, max_calls).await?;
        require_plugin_permission(&context, "metadata.write")?;
        let item_id = validate_public_id("itemId", &item_id)?;
        let input = validate_plugin_metadata_patch(payload)?;
        let Some(item) = repository
            .patch_item_metadata(item_id, &input)
            .await
            .map_err(plugin_metadata_write_error_to_app_error)?
        else {
            return Err(AppError::not_found("media item not found"));
        };

        Ok(Json(PatchPluginItemMetadataResponseDto {
            item,
            updated_fields: input.updated_fields,
            external_id_count: input.external_ids.len(),
        }))
    }
    .await;

    finish_audited_host_api(
        &repository,
        &context,
        HOST_API_PATCH_ITEM_METADATA,
        started,
        result,
    )
    .await
}

async fn put_item_artwork(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(item_id): Path<String>,
    Json(payload): Json<PutPluginItemArtworkRequestDto>,
) -> Result<Json<PutPluginItemArtworkResponseDto>, AppError> {
    let Some(database) = state.database() else {
        return Err(AppError::internal("database is not configured"));
    };
    let repository = PluginHostRepository::new(database.clone());
    let context = authenticate_plugin_host(&repository, &headers).await?;
    let started = Instant::now();
    let max_calls = state.config().plugins.host_api_max_calls_per_run;
    let result = async {
        enforce_host_api_call_budget(&repository, &context, max_calls).await?;
        require_plugin_permission(&context, "metadata.write")?;
        let item_id = validate_public_id("itemId", &item_id)?;
        let input = validate_plugin_artwork_replacement(&context.plugin_id, payload)?;
        let Some(artwork) = repository
            .replace_item_artwork(item_id, &input)
            .await
            .map_err(host_database_error)?
        else {
            return Err(AppError::not_found("media item not found"));
        };

        Ok(Json(PutPluginItemArtworkResponseDto {
            item_id: item_id.to_owned(),
            source: input.source,
            artwork_count: artwork.len(),
            artwork,
        }))
    }
    .await;

    finish_audited_host_api(
        &repository,
        &context,
        HOST_API_PUT_ITEM_ARTWORK,
        started,
        result,
    )
    .await
}

async fn put_item_markers(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(item_id): Path<String>,
    Json(payload): Json<PutPluginItemMarkersRequestDto>,
) -> Result<Json<PutPluginItemMarkersResponseDto>, AppError> {
    let Some(database) = state.database() else {
        return Err(AppError::internal("database is not configured"));
    };
    let repository = PluginHostRepository::new(database.clone());
    let context = authenticate_plugin_host(&repository, &headers).await?;
    let started = Instant::now();
    let max_calls = state.config().plugins.host_api_max_calls_per_run;
    let result = async {
        enforce_host_api_call_budget(&repository, &context, max_calls).await?;
        require_plugin_permission(&context, "metadata.write")?;
        let item_id = validate_public_id("itemId", &item_id)?;
        let input = validate_plugin_marker_replacement(&context.plugin_id, payload)?;
        let Some(markers) = repository
            .replace_item_markers(item_id, &input)
            .await
            .map_err(host_database_error)?
        else {
            return Err(AppError::not_found("media item not found"));
        };

        Ok(Json(PutPluginItemMarkersResponseDto {
            item_id: item_id.to_owned(),
            source: input.source,
            marker_count: markers.len(),
            markers,
        }))
    }
    .await;

    finish_audited_host_api(
        &repository,
        &context,
        HOST_API_PUT_ITEM_MARKERS,
        started,
        result,
    )
    .await
}

async fn get_kv(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(key): Path<String>,
) -> Result<Json<PluginKvValueDto>, AppError> {
    let Some(database) = state.database() else {
        return Err(AppError::internal("database is not configured"));
    };
    let repository = PluginHostRepository::new(database.clone());
    let context = authenticate_plugin_host(&repository, &headers).await?;
    let started = Instant::now();
    let max_calls = state.config().plugins.host_api_max_calls_per_run;
    let result = async {
        enforce_host_api_call_budget(&repository, &context, max_calls).await?;
        let key = validate_kv_key(&key)?;
        let Some(value) = repository
            .get_kv(&context.plugin_id, key)
            .await
            .map_err(host_database_error)?
        else {
            return Err(AppError::not_found("plugin kv key not found"));
        };

        Ok(Json(PluginKvValueDto {
            key: key.to_owned(),
            value,
        }))
    }
    .await;

    finish_audited_host_api(&repository, &context, HOST_API_GET_KV, started, result).await
}

async fn put_kv(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(key): Path<String>,
    Json(payload): Json<PutPluginKvRequestDto>,
) -> Result<Json<PluginKvValueDto>, AppError> {
    let Some(database) = state.database() else {
        return Err(AppError::internal("database is not configured"));
    };
    let repository = PluginHostRepository::new(database.clone());
    let context = authenticate_plugin_host(&repository, &headers).await?;
    let started = Instant::now();
    let max_calls = state.config().plugins.host_api_max_calls_per_run;
    let result = async {
        enforce_host_api_call_budget(&repository, &context, max_calls).await?;
        let key = validate_kv_key(&key)?;
        let value = repository
            .put_kv(&context.plugin_id, key, &payload.value)
            .await
            .map_err(host_database_error)?;

        Ok(Json(PluginKvValueDto {
            key: key.to_owned(),
            value,
        }))
    }
    .await;

    finish_audited_host_api(&repository, &context, HOST_API_PUT_KV, started, result).await
}

async fn delete_kv(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(key): Path<String>,
) -> Result<(StatusCode, Json<DeletePluginKvResponseDto>), AppError> {
    let Some(database) = state.database() else {
        return Err(AppError::internal("database is not configured"));
    };
    let repository = PluginHostRepository::new(database.clone());
    let context = authenticate_plugin_host(&repository, &headers).await?;
    let started = Instant::now();
    let max_calls = state.config().plugins.host_api_max_calls_per_run;
    let result = async {
        enforce_host_api_call_budget(&repository, &context, max_calls).await?;
        let key = validate_kv_key(&key)?;
        let deleted = repository
            .delete_kv(&context.plugin_id, key)
            .await
            .map_err(host_database_error)?;

        Ok((StatusCode::OK, Json(DeletePluginKvResponseDto { deleted })))
    }
    .await;

    finish_audited_host_api(&repository, &context, HOST_API_DELETE_KV, started, result).await
}

async fn finish_audited_host_api<T>(
    repository: &PluginHostRepository,
    context: &PluginHostContext,
    route: PluginHostApiRoute,
    started: Instant,
    result: Result<T, AppError>,
) -> Result<T, AppError> {
    let audit = host_api_call_audit(route, started.elapsed(), &result);
    if let Err(err) = repository.record_host_api_call(context, audit).await {
        warn!(
            error = %err,
            plugin_id = %context.plugin_id,
            execution_run_id = context.execution_run_id,
            method = route.method,
            path = route.path,
            "failed to record plugin host api call"
        );
    }
    result
}

fn host_api_call_audit<T>(
    route: PluginHostApiRoute,
    duration: Duration,
    result: &Result<T, AppError>,
) -> PluginHostApiCallAudit {
    match result {
        Ok(_) => PluginHostApiCallAudit {
            route,
            status_code: route.success_status,
            error_code: None,
            error_message: None,
            duration,
        },
        Err(err) => PluginHostApiCallAudit {
            route,
            status_code: err.status_code(),
            error_code: Some(err.code().to_owned()),
            error_message: Some(err.message().to_owned()),
            duration,
        },
    }
}

async fn authenticate_plugin_host(
    repository: &PluginHostRepository,
    headers: &HeaderMap,
) -> Result<PluginHostContext, AppError> {
    let token = headers
        .get(PLUGIN_TOKEN_HEADER)
        .ok_or_else(|| AppError::unauthorized("missing plugin host token"))?
        .to_str()
        .map_err(|_| AppError::unauthorized("invalid plugin host token"))?
        .trim();

    if token.is_empty() {
        return Err(AppError::unauthorized("missing plugin host token"));
    }

    repository
        .authenticate_token(token)
        .await
        .map_err(host_database_error)?
        .ok_or_else(|| AppError::unauthorized("invalid or expired plugin host token"))
}

async fn enforce_host_api_call_budget(
    repository: &PluginHostRepository,
    context: &PluginHostContext,
    max_calls: u32,
) -> Result<(), AppError> {
    if repository
        .host_api_call_limit_reached(context, max_calls)
        .await
        .map_err(host_database_error)?
    {
        return Err(host_api_call_limit_error(max_calls));
    }

    Ok(())
}

fn host_api_call_limit_error(max_calls: u32) -> AppError {
    AppError::too_many_requests(format!(
        "plugin host api call limit of {max_calls} per execution run exceeded"
    ))
}

fn require_plugin_permission(
    context: &PluginHostContext,
    permission_key: &'static str,
) -> Result<(), AppError> {
    if !context
        .permission_keys
        .iter()
        .any(|permission| permission == permission_key)
    {
        return Err(AppError::forbidden(format!(
            "plugin permission `{permission_key}` is required"
        )));
    }
    Ok(())
}

fn permission_keys_from_snapshot(snapshot: &Value) -> Vec<String> {
    let Some(items) = snapshot.as_array() else {
        return Vec::new();
    };

    let mut keys = Vec::new();
    for item in items {
        let Some(key) = item
            .get("key")
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|value| !value.is_empty())
        else {
            continue;
        };
        if !keys.iter().any(|existing| existing == key) {
            keys.push(key.to_owned());
        }
    }
    keys
}

fn duration_millis_i32(duration: Duration) -> i32 {
    duration.as_millis().min(i32::MAX as u128) as i32
}

fn truncate_audit_error(message: String) -> String {
    truncate_str(&message, MAX_HOST_API_AUDIT_ERROR_BYTES)
}

fn truncate_str(value: &str, max_bytes: usize) -> String {
    if value.len() <= max_bytes {
        return value.to_owned();
    }

    let mut end = max_bytes;
    while !value.is_char_boundary(end) {
        end -= 1;
    }
    value[..end].to_owned()
}

impl PluginItemWindow {
    fn from_query(query: &ListPluginLibraryItemsQueryDto) -> Result<Self, AppError> {
        let cursor = query
            .cursor
            .as_deref()
            .map(parse_plugin_item_cursor)
            .transpose()?;
        Ok(Self {
            start_index: 0,
            limit: i64::from(
                query
                    .limit
                    .unwrap_or(DEFAULT_LIBRARY_ITEMS_LIMIT)
                    .clamp(1, MAX_LIBRARY_ITEMS_LIMIT),
            ),
            cursor,
        })
    }

    fn fetch_limit(&self) -> i64 {
        self.limit.saturating_add(1)
    }
}

fn encode_plugin_item_cursor(sort_key: &str, cursor_id: i64) -> String {
    format!(
        "{}:{}:{}",
        LIBRARY_ITEM_CURSOR_VERSION,
        cursor_id,
        hex_lower(sort_key.as_bytes())
    )
}

fn parse_plugin_item_cursor(value: &str) -> Result<PluginItemCursor, AppError> {
    let cursor = value.trim();
    if cursor.is_empty() {
        return Err(AppError::unprocessable("plugin item cursor is required"));
    }
    if cursor.len() > MAX_LIBRARY_ITEM_CURSOR_LEN {
        return Err(AppError::unprocessable("plugin item cursor is too long"));
    }

    let mut parts = cursor.split(':');
    let version = parts.next().unwrap_or_default();
    let cursor_id = parts.next().unwrap_or_default();
    let sort_key = parts.next().unwrap_or_default();
    if parts.next().is_some() || version != LIBRARY_ITEM_CURSOR_VERSION {
        return Err(AppError::unprocessable("plugin item cursor is invalid"));
    }
    let cursor_id = cursor_id
        .parse::<i64>()
        .map_err(|_| AppError::unprocessable("plugin item cursor is invalid"))?;
    if cursor_id <= 0 {
        return Err(AppError::unprocessable("plugin item cursor is invalid"));
    }
    let sort_key = String::from_utf8(parse_hex_bytes(sort_key)?)
        .map_err(|_| AppError::unprocessable("plugin item cursor is invalid"))?;
    if sort_key.is_empty() {
        return Err(AppError::unprocessable("plugin item cursor is invalid"));
    }

    Ok(PluginItemCursor {
        sort_key,
        cursor_id,
    })
}

fn parse_hex_bytes(value: &str) -> Result<Vec<u8>, AppError> {
    let value = value.trim();
    if value.is_empty() || value.len() % 2 != 0 {
        return Err(AppError::unprocessable("plugin item cursor is invalid"));
    }

    let mut bytes = Vec::with_capacity(value.len() / 2);
    for pair in value.as_bytes().chunks_exact(2) {
        let high = hex_nibble(pair[0])
            .ok_or_else(|| AppError::unprocessable("plugin item cursor is invalid"))?;
        let low = hex_nibble(pair[1])
            .ok_or_else(|| AppError::unprocessable("plugin item cursor is invalid"))?;
        bytes.push((high << 4) | low);
    }

    Ok(bytes)
}

fn hex_lower(bytes: &[u8]) -> String {
    let mut encoded = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        encoded.push(hex_char(byte >> 4));
        encoded.push(hex_char(byte & 0x0f));
    }
    encoded
}

fn hex_char(value: u8) -> char {
    match value {
        0..=9 => (b'0' + value) as char,
        10..=15 => (b'a' + (value - 10)) as char,
        _ => unreachable!("hex nibbles are in range"),
    }
}

fn hex_nibble(byte: u8) -> Option<u8> {
    match byte {
        b'0'..=b'9' => Some(byte - b'0'),
        b'a'..=b'f' => Some(byte - b'a' + 10),
        b'A'..=b'F' => Some(byte - b'A' + 10),
        _ => None,
    }
}

fn validate_plugin_metadata_patch(
    payload: PatchPluginItemMetadataRequestDto,
) -> Result<PluginMetadataPatchInput, AppError> {
    if payload.external_ids.len() > MAX_METADATA_EXTERNAL_IDS {
        return Err(AppError::unprocessable(format!(
            "externalIds must contain at most {MAX_METADATA_EXTERNAL_IDS} items"
        )));
    }

    let mut input = PluginMetadataPatchInput::default();
    if let Some(value) = payload.title {
        input.title = Some(validate_metadata_text(
            "title",
            &value,
            MAX_METADATA_TITLE_LEN,
        )?);
        input.updated_fields.push("title".to_owned());
    }
    if let Some(value) = payload.original_title {
        input.original_title = Some(validate_metadata_text(
            "originalTitle",
            &value,
            MAX_METADATA_TITLE_LEN,
        )?);
        input.updated_fields.push("originalTitle".to_owned());
    }
    if let Some(value) = payload.sort_title {
        input.sort_title = Some(validate_metadata_text(
            "sortTitle",
            &value,
            MAX_METADATA_TITLE_LEN,
        )?);
        input.updated_fields.push("sortTitle".to_owned());
    }
    if let Some(value) = payload.overview {
        input.overview = Some(validate_metadata_text(
            "overview",
            &value,
            MAX_METADATA_OVERVIEW_LEN,
        )?);
        input.updated_fields.push("overview".to_owned());
    }
    if let Some(value) = payload.production_year {
        validate_metadata_year(value)?;
        input.production_year = Some(value);
        input.updated_fields.push("productionYear".to_owned());
    }
    if let Some(value) = payload.premiere_date {
        input.premiere_date = Some(validate_metadata_date(&value)?);
        input.updated_fields.push("premiereDate".to_owned());
    }
    if let Some(value) = payload.official_rating {
        input.official_rating = Some(validate_metadata_text(
            "officialRating",
            &value,
            MAX_METADATA_CLASSIFICATION_NAME_LEN,
        )?);
        input.updated_fields.push("officialRating".to_owned());
    }
    if let Some(value) = payload.community_rating {
        input.community_rating = Some(validate_rating("communityRating", value, 10.0)?);
        input.updated_fields.push("communityRating".to_owned());
    }
    if let Some(value) = payload.critic_rating {
        input.critic_rating = Some(validate_rating("criticRating", value, 100.0)?);
        input.updated_fields.push("criticRating".to_owned());
    }
    if let Some(value) = payload.runtime_ticks {
        if value < 0 {
            return Err(AppError::unprocessable(
                "runtimeTicks must be greater than or equal to 0",
            ));
        }
        input.runtime_ticks = Some(value);
        input.updated_fields.push("runtimeTicks".to_owned());
    }

    let mut providers = BTreeSet::new();
    for external_id in payload.external_ids {
        let provider = normalize_external_id_provider(&external_id.provider)?;
        if !providers.insert(provider.clone()) {
            return Err(AppError::unprocessable(
                "externalIds must not contain duplicate providers",
            ));
        }
        let external_id = validate_external_id_value(&external_id.external_id)?;
        input.external_ids.push(PluginExternalIdInput {
            provider,
            external_id,
        });
    }
    if !input.external_ids.is_empty() {
        input.updated_fields.push("externalIds".to_owned());
    }
    if let Some(values) = payload.genres {
        input.genres = Some(validate_metadata_name_list("genres", values)?);
        input.updated_fields.push("genres".to_owned());
    }
    if let Some(values) = payload.studios {
        input.studios = Some(validate_metadata_name_list("studios", values)?);
        input.updated_fields.push("studios".to_owned());
    }
    if let Some(values) = payload.tags {
        input.tags = Some(validate_metadata_name_list("tags", values)?);
        input.updated_fields.push("tags".to_owned());
    }
    if let Some(values) = payload.people {
        input.people = Some(validate_metadata_people(values)?);
        input.updated_fields.push("people".to_owned());
    }
    if input.updated_fields.is_empty() {
        return Err(AppError::unprocessable(
            "metadata patch must include at least one field, externalId, genre, studio, tag, or person",
        ));
    }

    Ok(input)
}

fn validate_metadata_text(field: &str, value: &str, max_len: usize) -> Result<String, AppError> {
    let value = value.trim();
    if value.is_empty() {
        return Err(AppError::unprocessable(format!(
            "{field} must not be empty"
        )));
    }
    if value.len() > max_len {
        return Err(AppError::unprocessable(format!(
            "{field} must be at most {max_len} characters"
        )));
    }
    Ok(value.to_owned())
}

fn validate_metadata_name_list(
    field: &str,
    values: Vec<String>,
) -> Result<Vec<PluginNamedMetadataInput>, AppError> {
    if values.len() > MAX_METADATA_CLASSIFICATION_ITEMS {
        return Err(AppError::unprocessable(format!(
            "{field} must contain at most {MAX_METADATA_CLASSIFICATION_ITEMS} items"
        )));
    }

    let mut seen = BTreeSet::new();
    let mut output = Vec::with_capacity(values.len());
    for value in values {
        let name = validate_metadata_text(field, &value, MAX_METADATA_CLASSIFICATION_NAME_LEN)?;
        let name_normalized = normalize_metadata_name(&name);
        if !seen.insert(name_normalized.clone()) {
            return Err(AppError::unprocessable(format!(
                "{field} must not contain duplicate values"
            )));
        }
        output.push(PluginNamedMetadataInput {
            name,
            name_normalized,
        });
    }
    Ok(output)
}

fn normalize_metadata_name(value: &str) -> String {
    value.trim().to_lowercase()
}

fn validate_metadata_people(
    values: Vec<PatchPluginItemPersonDto>,
) -> Result<Vec<PluginPersonInput>, AppError> {
    if values.len() > MAX_METADATA_PEOPLE_ITEMS {
        return Err(AppError::unprocessable(format!(
            "people must contain at most {MAX_METADATA_PEOPLE_ITEMS} items"
        )));
    }

    let mut seen = BTreeSet::new();
    let mut people = Vec::with_capacity(values.len());
    for (index, value) in values.into_iter().enumerate() {
        let name =
            validate_metadata_text("people.name", &value.name, MAX_METADATA_PERSON_NAME_LEN)?;
        let name_normalized = normalize_metadata_name(&name);
        let role_type = normalize_plugin_person_role_type(&value.role_type)?;
        let role_name = match value.role_name {
            Some(role_name) => validate_optional_metadata_text(
                "people.roleName",
                &role_name,
                MAX_METADATA_PERSON_ROLE_NAME_LEN,
            )?,
            None => String::new(),
        };
        let sort_order = value
            .sort_order
            .unwrap_or_else(|| i32::try_from(index).unwrap_or(MAX_METADATA_PERSON_SORT_ORDER));
        if !(0..=MAX_METADATA_PERSON_SORT_ORDER).contains(&sort_order) {
            return Err(AppError::unprocessable(format!(
                "people.sortOrder must be between 0 and {MAX_METADATA_PERSON_SORT_ORDER}"
            )));
        }
        let key = (
            name_normalized.clone(),
            role_type.clone(),
            role_name.to_lowercase(),
        );
        if !seen.insert(key) {
            return Err(AppError::unprocessable(
                "people must not contain duplicate name, roleType, and roleName entries",
            ));
        }

        people.push(PluginPersonInput {
            name,
            name_normalized,
            role_type,
            role_name,
            sort_order,
        });
    }

    Ok(people)
}

fn validate_optional_metadata_text(
    field: &str,
    value: &str,
    max_len: usize,
) -> Result<String, AppError> {
    let value = value.trim();
    if value.len() > max_len {
        return Err(AppError::unprocessable(format!(
            "{field} must be at most {max_len} characters"
        )));
    }
    Ok(value.to_owned())
}

fn normalize_plugin_person_role_type(value: &str) -> Result<String, AppError> {
    let role_type = value.trim().to_ascii_lowercase();
    if SUPPORTED_PLUGIN_PERSON_ROLE_TYPES.contains(&role_type.as_str()) {
        return Ok(role_type);
    }

    Err(AppError::unprocessable("people.roleType is not supported"))
}

fn validate_metadata_year(value: i32) -> Result<(), AppError> {
    if !(1800..=3000).contains(&value) {
        return Err(AppError::unprocessable(
            "productionYear must be between 1800 and 3000",
        ));
    }
    Ok(())
}

fn validate_metadata_date(value: &str) -> Result<String, AppError> {
    let value = value.trim();
    if value.len() != 10 {
        return Err(AppError::unprocessable(
            "premiereDate must use YYYY-MM-DD format",
        ));
    }
    let bytes = value.as_bytes();
    if bytes.get(4) != Some(&b'-')
        || bytes.get(7) != Some(&b'-')
        || !bytes
            .iter()
            .enumerate()
            .all(|(index, byte)| matches!(index, 4 | 7) || byte.is_ascii_digit())
    {
        return Err(AppError::unprocessable(
            "premiereDate must use YYYY-MM-DD format",
        ));
    }
    let year = value[0..4]
        .parse::<i32>()
        .map_err(|_| AppError::unprocessable("premiereDate must use YYYY-MM-DD format"))?;
    let month = value[5..7]
        .parse::<u32>()
        .map_err(|_| AppError::unprocessable("premiereDate must use YYYY-MM-DD format"))?;
    let day = value[8..10]
        .parse::<u32>()
        .map_err(|_| AppError::unprocessable("premiereDate must use YYYY-MM-DD format"))?;
    if !(1..=12).contains(&month) {
        return Err(AppError::unprocessable("premiereDate month is invalid"));
    }
    let max_day = days_in_month(year, month);
    if day == 0 || day > max_day {
        return Err(AppError::unprocessable("premiereDate day is invalid"));
    }
    Ok(value.to_owned())
}

fn days_in_month(year: i32, month: u32) -> u32 {
    match month {
        1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
        4 | 6 | 9 | 11 => 30,
        2 if is_leap_year(year) => 29,
        2 => 28,
        _ => 0,
    }
}

fn is_leap_year(year: i32) -> bool {
    (year % 4 == 0 && year % 100 != 0) || year % 400 == 0
}

fn validate_rating(field: &str, value: f64, max: f64) -> Result<f64, AppError> {
    if !value.is_finite() || value < 0.0 || value > max {
        return Err(AppError::unprocessable(format!(
            "{field} must be between 0 and {max}"
        )));
    }
    Ok(value)
}

fn normalize_external_id_provider(value: &str) -> Result<String, AppError> {
    let provider = value.trim().to_ascii_lowercase();
    if provider.is_empty() {
        return Err(AppError::unprocessable(
            "externalIds.provider must not be empty",
        ));
    }
    if provider.len() > MAX_METADATA_PROVIDER_LEN {
        return Err(AppError::unprocessable(format!(
            "externalIds.provider must be at most {MAX_METADATA_PROVIDER_LEN} characters"
        )));
    }
    if !provider.bytes().all(|byte| {
        byte.is_ascii_lowercase() || byte.is_ascii_digit() || matches!(byte, b'.' | b'-' | b'_')
    }) {
        return Err(AppError::unprocessable(
            "externalIds.provider may only contain letters, numbers, dot, dash, or underscore",
        ));
    }
    Ok(provider)
}

fn validate_external_id_value(value: &str) -> Result<String, AppError> {
    let external_id = value.trim();
    if external_id.is_empty() {
        return Err(AppError::unprocessable(
            "externalIds.externalId must not be empty",
        ));
    }
    if external_id.len() > MAX_METADATA_EXTERNAL_ID_LEN {
        return Err(AppError::unprocessable(format!(
            "externalIds.externalId must be at most {MAX_METADATA_EXTERNAL_ID_LEN} characters"
        )));
    }
    if external_id.contains(char::is_whitespace) {
        return Err(AppError::unprocessable(
            "externalIds.externalId must not contain whitespace",
        ));
    }
    Ok(external_id.to_owned())
}

fn validate_plugin_artwork_replacement(
    plugin_id: &str,
    payload: PutPluginItemArtworkRequestDto,
) -> Result<PluginArtworkReplacementInput, AppError> {
    if payload.artwork.len() > MAX_PLUGIN_ARTWORK_PER_REQUEST {
        return Err(AppError::unprocessable(format!(
            "artwork must contain at most {MAX_PLUGIN_ARTWORK_PER_REQUEST} items"
        )));
    }

    let source = plugin_artwork_source(plugin_id, payload.source.as_deref())?;
    let mut seen_images = BTreeSet::new();
    let mut primary_types = BTreeSet::new();
    let mut artwork = Vec::with_capacity(payload.artwork.len());
    for image in payload.artwork {
        let artwork_type = normalize_plugin_artwork_type(&image.artwork_type)?;
        let remote_url = validate_artwork_remote_url(&image.remote_url)?;
        let key = (artwork_type.clone(), remote_url.clone());
        if !seen_images.insert(key) {
            return Err(AppError::unprocessable(
                "artwork must not contain duplicate type and remoteUrl pairs",
            ));
        }
        if image.is_primary && !primary_types.insert(artwork_type.clone()) {
            return Err(AppError::unprocessable(
                "artwork must not contain multiple primary images for the same type",
            ));
        }
        validate_artwork_dimension("width", image.width)?;
        validate_artwork_dimension("height", image.height)?;

        artwork.push(PluginArtworkInput {
            artwork_type,
            remote_url,
            width: image.width,
            height: image.height,
            is_primary: image.is_primary,
        });
    }

    Ok(PluginArtworkReplacementInput { source, artwork })
}

fn plugin_artwork_source(plugin_id: &str, source_suffix: Option<&str>) -> Result<String, AppError> {
    plugin_scoped_source(plugin_id, source_suffix, "artwork")
}

fn normalize_plugin_artwork_type(value: &str) -> Result<String, AppError> {
    let artwork_type = value.trim().to_ascii_lowercase();
    if SUPPORTED_PLUGIN_ARTWORK_TYPES.contains(&artwork_type.as_str()) {
        return Ok(artwork_type);
    }

    Err(AppError::unprocessable("artworkType is not supported"))
}

fn validate_artwork_remote_url(value: &str) -> Result<String, AppError> {
    let remote_url = value.trim();
    if remote_url.is_empty() {
        return Err(AppError::unprocessable("remoteUrl must not be empty"));
    }
    if remote_url.len() > MAX_PLUGIN_ARTWORK_REMOTE_URL_LEN {
        return Err(AppError::unprocessable(format!(
            "remoteUrl must be at most {MAX_PLUGIN_ARTWORK_REMOTE_URL_LEN} characters"
        )));
    }
    let parsed =
        Url::parse(remote_url).map_err(|_| AppError::unprocessable("remoteUrl is invalid"))?;
    if !matches!(parsed.scheme(), "http" | "https") || parsed.host_str().is_none() {
        return Err(AppError::unprocessable(
            "remoteUrl must be an absolute http or https URL",
        ));
    }
    if !parsed.username().is_empty() || parsed.password().is_some() {
        return Err(AppError::unprocessable(
            "remoteUrl must not contain credentials",
        ));
    }

    Ok(parsed.to_string())
}

fn validate_artwork_dimension(field: &str, value: Option<i32>) -> Result<(), AppError> {
    if let Some(value) = value {
        if !(1..=MAX_PLUGIN_ARTWORK_DIMENSION).contains(&value) {
            return Err(AppError::unprocessable(format!(
                "{field} must be between 1 and {MAX_PLUGIN_ARTWORK_DIMENSION}"
            )));
        }
    }
    Ok(())
}

fn validate_plugin_marker_replacement(
    plugin_id: &str,
    payload: PutPluginItemMarkersRequestDto,
) -> Result<PluginMarkerReplacementInput, AppError> {
    if payload.markers.len() > MAX_PLUGIN_MARKERS_PER_REQUEST {
        return Err(AppError::unprocessable(format!(
            "markers must contain at most {MAX_PLUGIN_MARKERS_PER_REQUEST} items"
        )));
    }

    let source = plugin_marker_source(plugin_id, payload.source.as_deref())?;
    let mut seen = BTreeSet::new();
    let mut markers = Vec::with_capacity(payload.markers.len());
    for marker in payload.markers {
        let marker_type = normalize_plugin_marker_type(&marker.marker_type)?;
        if marker.start_ticks < 0 {
            return Err(AppError::unprocessable(
                "marker startTicks must be greater than or equal to 0",
            ));
        }
        if let Some(end_ticks) = marker.end_ticks {
            if end_ticks < marker.start_ticks {
                return Err(AppError::unprocessable(
                    "marker endTicks must be greater than or equal to startTicks",
                ));
            }
        }
        if let Some(confidence) = marker.confidence {
            if !confidence.is_finite() || !(0.0..=1.0).contains(&confidence) {
                return Err(AppError::unprocessable(
                    "marker confidence must be between 0 and 1",
                ));
            }
        }

        let key = (marker_type.clone(), marker.start_ticks);
        if !seen.insert(key) {
            return Err(AppError::unprocessable(
                "markers must not contain duplicate markerType and startTicks pairs",
            ));
        }

        markers.push(PluginMediaMarkerInput {
            marker_type,
            start_ticks: marker.start_ticks,
            end_ticks: marker.end_ticks,
            confidence: marker.confidence,
        });
    }

    Ok(PluginMarkerReplacementInput { source, markers })
}

fn plugin_marker_source(plugin_id: &str, source_suffix: Option<&str>) -> Result<String, AppError> {
    plugin_scoped_source(plugin_id, source_suffix, "marker")
}

fn plugin_scoped_source(
    plugin_id: &str,
    source_suffix: Option<&str>,
    source_kind: &str,
) -> Result<String, AppError> {
    let plugin_id = plugin_id.trim();
    if plugin_id.is_empty() {
        return Err(AppError::internal("plugin id is missing from host context"));
    }
    let source = match source_suffix
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        Some(suffix) => {
            validate_plugin_source_suffix(source_kind, suffix)?;
            format!("plugin:{plugin_id}:{suffix}")
        }
        None => format!("plugin:{plugin_id}"),
    };
    if source.len() > MAX_PLUGIN_SOURCE_LEN {
        return Err(AppError::unprocessable(format!(
            "{source_kind} source must be at most {MAX_PLUGIN_SOURCE_LEN} characters"
        )));
    }
    Ok(source)
}

fn validate_plugin_source_suffix(source_kind: &str, value: &str) -> Result<(), AppError> {
    if value.len() > MAX_PLUGIN_SOURCE_SUFFIX_LEN {
        return Err(AppError::unprocessable(format!(
            "{source_kind} source must be at most {MAX_PLUGIN_SOURCE_SUFFIX_LEN} characters"
        )));
    }
    if !value
        .bytes()
        .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'-' | b'_' | b':'))
    {
        return Err(AppError::unprocessable(format!(
            "{source_kind} source may only contain letters, numbers, dot, dash, underscore, or colon"
        )));
    }
    Ok(())
}

fn normalize_plugin_marker_type(value: &str) -> Result<String, AppError> {
    let marker_type = value.trim().to_ascii_lowercase();
    if SUPPORTED_PLUGIN_MARKER_TYPES.contains(&marker_type.as_str()) {
        return Ok(marker_type);
    }

    Err(AppError::unprocessable("markerType is not supported"))
}

impl TryFrom<SendPluginNotificationRequestDto> for PluginNotificationInput {
    type Error = AppError;

    fn try_from(value: SendPluginNotificationRequestDto) -> Result<Self, Self::Error> {
        let title = validate_bounded_text("title", &value.title, MAX_NOTIFICATION_TITLE_LEN)?;
        let message =
            validate_bounded_text("message", &value.message, MAX_NOTIFICATION_MESSAGE_LEN)?;
        let level = normalize_notification_level(value.level.as_deref())?;
        let channel = value
            .channel
            .as_deref()
            .map(validate_notification_channel)
            .transpose()?
            .map(str::to_owned);
        let metadata = value.metadata.unwrap_or_else(|| json!({}));
        if !metadata.is_object() {
            return Err(AppError::unprocessable(
                "notification metadata must be a JSON object",
            ));
        }

        Ok(Self {
            title: title.to_owned(),
            message: message.to_owned(),
            level,
            channel,
            metadata,
        })
    }
}

fn validate_bounded_text<'a>(
    field: &str,
    value: &'a str,
    max_len: usize,
) -> Result<&'a str, AppError> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return Err(AppError::unprocessable(format!("{field} is required")));
    }
    if trimmed.len() > max_len {
        return Err(AppError::unprocessable(format!(
            "{field} must be at most {max_len} characters"
        )));
    }
    Ok(trimmed)
}

fn normalize_notification_level(value: Option<&str>) -> Result<String, AppError> {
    let level = value
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .unwrap_or("info")
        .to_ascii_lowercase();
    match level.as_str() {
        "info" | "success" | "warning" | "error" => Ok(level),
        _ => Err(AppError::unprocessable(
            "notification level must be one of info, success, warning, or error",
        )),
    }
}

fn validate_notification_channel(value: &str) -> Result<&str, AppError> {
    let channel = value.trim();
    if channel.is_empty() {
        return Err(AppError::unprocessable(
            "notification channel must not be empty",
        ));
    }
    if channel.len() > MAX_NOTIFICATION_CHANNEL_LEN {
        return Err(AppError::unprocessable(format!(
            "notification channel must be at most {MAX_NOTIFICATION_CHANNEL_LEN} characters"
        )));
    }
    if !channel
        .bytes()
        .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'-' | b'_' | b':'))
    {
        return Err(AppError::unprocessable(
            "notification channel may only contain letters, numbers, dot, dash, underscore, or colon",
        ));
    }
    Ok(channel)
}

fn validate_public_id<'a>(field: &str, value: &'a str) -> Result<&'a str, AppError> {
    let value = value.trim();
    if value.is_empty() {
        return Err(AppError::unprocessable(format!("{field} is required")));
    }
    if value.len() > 128 {
        return Err(AppError::unprocessable(format!(
            "{field} must be at most 128 characters"
        )));
    }
    if value.contains(char::is_whitespace) {
        return Err(AppError::unprocessable(format!(
            "{field} must not contain whitespace"
        )));
    }
    Ok(value)
}

pub fn plugin_capabilities() -> PluginCapabilitiesDto {
    PluginCapabilitiesDto {
        api_version: SUPPORTED_PLUGIN_API_VERSION.to_owned(),
        host_api_version: PLUGIN_HOST_API_VERSION.to_owned(),
        manifest_runtimes: string_list(SUPPORTED_PLUGIN_RUNTIMES),
        executable_runtimes: string_list(EXECUTABLE_PLUGIN_RUNTIMES),
        http_schemes: string_list(HTTP_PLUGIN_SCHEMES),
        permissions: string_list(supported_plugin_permissions()),
        permission_details: plugin_permission_capabilities(),
        hook_events: string_list(supported_plugin_hook_events()),
        host_apis: plugin_host_api_capabilities(),
    }
}

fn plugin_host_api_routes() -> [PluginHostApiRoute; 12] {
    [
        HOST_API_CAPABILITIES,
        HOST_API_CONFIG,
        HOST_API_GET_KV,
        HOST_API_PUT_KV,
        HOST_API_DELETE_KV,
        HOST_API_LIST_LIBRARIES,
        HOST_API_LIST_LIBRARY_ITEMS,
        HOST_API_GET_ITEM,
        HOST_API_PATCH_ITEM_METADATA,
        HOST_API_PUT_ITEM_ARTWORK,
        HOST_API_PUT_ITEM_MARKERS,
        HOST_API_SEND_NOTIFICATION,
    ]
}

fn plugin_permission_capabilities() -> Vec<PluginPermissionCapabilityDto> {
    let host_api_routes = plugin_host_api_routes();

    PLUGIN_PERMISSION_CAPABILITIES
        .iter()
        .map(|permission| PluginPermissionCapabilityDto {
            key: permission.key.to_owned(),
            category: permission.category.to_owned(),
            risk_level: permission.risk_level.to_owned(),
            description: permission.description.to_owned(),
            manifest_features: string_list(permission.manifest_features),
            host_apis: host_api_routes
                .iter()
                .copied()
                .filter(|route| route.required_permission == Some(permission.key))
                .map(plugin_host_api_capability_from_route)
                .collect(),
        })
        .collect()
}

fn plugin_host_api_capabilities() -> Vec<PluginHostApiCapabilityDto> {
    plugin_host_api_routes()
        .into_iter()
        .map(plugin_host_api_capability_from_route)
        .collect()
}

fn plugin_host_api_capability_from_route(route: PluginHostApiRoute) -> PluginHostApiCapabilityDto {
    PluginHostApiCapabilityDto {
        method: route.method.to_owned(),
        path: route.path.to_owned(),
        required_permission: route.required_permission.map(str::to_owned),
    }
}

fn string_list(values: &'static [&'static str]) -> Vec<String> {
    values.iter().map(|value| (*value).to_owned()).collect()
}

fn validate_kv_key(value: &str) -> Result<&str, AppError> {
    let key = value.trim();
    if key.is_empty() {
        return Err(AppError::unprocessable("plugin kv key is required"));
    }
    if key.len() > MAX_KV_KEY_LEN {
        return Err(AppError::unprocessable(format!(
            "plugin kv key must be at most {MAX_KV_KEY_LEN} characters"
        )));
    }
    if !key
        .bytes()
        .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'-' | b'_' | b':'))
    {
        return Err(AppError::unprocessable(
            "plugin kv key may only contain letters, numbers, dot, dash, underscore, or colon",
        ));
    }

    Ok(key)
}

fn host_database_error(error: sqlx::Error) -> AppError {
    AppError::internal(format!("plugin host api database error: {error}"))
}

fn host_config_error(error: PluginHostConfigError) -> AppError {
    AppError::internal(format!("plugin host config error: {error}"))
}

fn plugin_metadata_write_error_to_app_error(error: PluginMetadataWriteError) -> AppError {
    match error {
        PluginMetadataWriteError::Database(error) => host_database_error(error),
        PluginMetadataWriteError::ExternalIdConflict {
            provider,
            external_id,
        } => AppError::conflict(format!(
            "external id `{provider}:{external_id}` already belongs to another media item"
        )),
    }
}

impl Display for PluginHostConfigError {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Database(err) => write!(f, "database error: {err}"),
            Self::Secret(err) => write!(f, "secret error: {err}"),
            Self::UnsupportedSecretAlgorithm(algorithm) => {
                write!(
                    f,
                    "unsupported plugin config secret algorithm `{algorithm}`"
                )
            }
        }
    }
}

impl Error for PluginHostConfigError {}

impl From<sqlx::Error> for PluginHostConfigError {
    fn from(error: sqlx::Error) -> Self {
        Self::Database(error)
    }
}

impl Display for PluginMetadataWriteError {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Database(err) => write!(f, "database error: {err}"),
            Self::ExternalIdConflict {
                provider,
                external_id,
            } => write!(
                f,
                "external id `{provider}:{external_id}` already belongs to another media item"
            ),
        }
    }
}

impl Error for PluginMetadataWriteError {}

impl From<sqlx::Error> for PluginMetadataWriteError {
    fn from(error: sqlx::Error) -> Self {
        Self::Database(error)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn kv_key_validation_allows_stable_namespaced_keys() {
        assert_eq!(
            validate_kv_key("notify.last-scan").unwrap(),
            "notify.last-scan"
        );
        assert_eq!(validate_kv_key("state:cursor_1").unwrap(), "state:cursor_1");
    }

    #[test]
    fn kv_key_validation_rejects_path_like_or_blank_keys() {
        assert!(validate_kv_key("").is_err());
        assert!(validate_kv_key("a/b").is_err());
        assert!(validate_kv_key("has space").is_err());
    }

    #[test]
    fn library_item_window_ignores_start_index_and_clamps_limits() {
        let window = PluginItemWindow::from_query(&ListPluginLibraryItemsQueryDto {
            start_index: Some(7),
            limit: Some(10_000),
            cursor: None,
        })
        .unwrap();

        assert_eq!(window.start_index, 0);
        assert_eq!(window.limit, i64::from(MAX_LIBRARY_ITEMS_LIMIT));
        assert_eq!(window.fetch_limit(), i64::from(MAX_LIBRARY_ITEMS_LIMIT) + 1);
    }

    #[test]
    fn library_item_window_defaults_to_keyset_without_exact_count() {
        let window = PluginItemWindow::from_query(&ListPluginLibraryItemsQueryDto {
            start_index: None,
            limit: Some(50),
            cursor: None,
        })
        .unwrap();

        assert_eq!(window.start_index, 0);
        assert_eq!(window.limit, 50);
        assert_eq!(window.fetch_limit(), 51);
        assert_eq!(window.cursor, None);
    }

    #[test]
    fn library_item_window_uses_cursor_without_offset() {
        let cursor = encode_plugin_item_cursor("Movie 2", 42);
        let window = PluginItemWindow::from_query(&ListPluginLibraryItemsQueryDto {
            start_index: Some(10),
            limit: Some(50),
            cursor: Some(cursor),
        })
        .unwrap();

        assert_eq!(window.start_index, 0);
        assert_eq!(window.limit, 50);
        assert_eq!(window.fetch_limit(), 51);
        assert_eq!(
            window.cursor,
            Some(PluginItemCursor {
                sort_key: "Movie 2".to_owned(),
                cursor_id: 42,
            })
        );
    }

    #[test]
    fn plugin_item_cursor_roundtrips_unicode_sort_keys() {
        let cursor = encode_plugin_item_cursor("电影 A", 123);
        let parsed = parse_plugin_item_cursor(&cursor).unwrap();

        assert_eq!(parsed.sort_key, "电影 A");
        assert_eq!(parsed.cursor_id, 123);
    }

    #[test]
    fn plugin_item_cursor_rejects_invalid_tokens() {
        assert!(parse_plugin_item_cursor("").is_err());
        assert!(parse_plugin_item_cursor("v2:1:4142").is_err());
        assert!(parse_plugin_item_cursor("v1:0:4142").is_err());
        assert!(parse_plugin_item_cursor("v1:1:not-hex").is_err());
        assert!(parse_plugin_item_cursor("v1:1:").is_err());
    }

    #[test]
    fn plugin_library_item_keyset_queries_do_not_force_full_counts_or_offsets() {
        let host = include_str!("host.rs");
        let bad_offset_query = format!("{}{}", "PLUGIN_LIBRARY_ITEMS_", "OFFSET_SQL");
        let bad_offset_mode = format!("{}{}", "PluginItemPaginationMode::", "Offset");

        assert!(!host.contains(&bad_offset_query));
        assert!(!host.contains(&bad_offset_mode));
        for sql in [
            PLUGIN_LIBRARY_ITEMS_FIRST_PAGE_SQL,
            PLUGIN_LIBRARY_ITEMS_AFTER_CURSOR_SQL,
        ] {
            assert!(!sql.contains("count(*) over()"));
            assert!(!sql.contains("offset"));
        }
        assert!(PLUGIN_LIBRARY_ITEMS_AFTER_CURSOR_SQL.contains("> ($2::text, $3::bigint)"));
    }

    #[test]
    fn plugin_library_public_id_queries_keep_uuid_index_shape() {
        for sql in [
            PLUGIN_LIBRARY_ITEMS_FIRST_PAGE_SQL,
            PLUGIN_LIBRARY_ITEMS_AFTER_CURSOR_SQL,
            PLUGIN_LIBRARY_EXISTS_SQL,
        ] {
            assert!(sql.contains("with requested_library as"));
            assert!(sql.contains("$1::uuid"));
            assert!(sql.contains("join libraries l on l.public_id = requested_library.public_id"));
            assert!(!sql.contains("l.public_id::text = $1"));
            assert!(!sql.contains("where public_id::text = $1"));
        }
    }

    #[test]
    fn plugin_host_package_public_id_inputs_keep_uuid_index_shape() {
        let host = include_str!("host.rs");
        let bad_package_filter = format!("{}{}", "pkg.public_id::text = ", "$4");

        assert!(host.contains("where pkg.public_id = case"));
        assert!(host.contains("then $4::uuid"));
        assert!(!host.contains(&bad_package_filter));
    }

    #[test]
    fn public_id_validation_rejects_blank_or_whitespace() {
        assert!(validate_public_id("libraryId", "").is_err());
        assert!(validate_public_id("libraryId", "has space").is_err());
        assert_eq!(
            validate_public_id("libraryId", "library-1").unwrap(),
            "library-1"
        );
    }

    #[test]
    fn plugin_capabilities_reflect_manifest_and_host_boundaries() {
        let capabilities = plugin_capabilities();

        assert_eq!(capabilities.api_version, SUPPORTED_PLUGIN_API_VERSION);
        assert_eq!(capabilities.host_api_version, PLUGIN_HOST_API_VERSION);
        assert!(capabilities.manifest_runtimes.contains(&"wasi".to_owned()));
        assert!(capabilities.manifest_runtimes.contains(&"http".to_owned()));
        assert_eq!(
            capabilities.executable_runtimes,
            vec!["http".to_owned(), "wasi".to_owned()]
        );
        assert!(capabilities.http_schemes.contains(&"https".to_owned()));
        assert!(
            capabilities
                .permissions
                .contains(&"notification.send".to_owned())
        );
        assert!(capabilities.permission_details.iter().any(|permission| {
            permission.key == "notification.send"
                && permission.risk_level == "medium"
                && permission
                    .host_apis
                    .iter()
                    .any(|api| api.method == "POST" && api.path == "/api/plugin/notifications")
        }));
        assert!(
            capabilities
                .hook_events
                .contains(&"library.scan.completed".to_owned())
        );
        assert!(capabilities.host_apis.iter().any(|api| {
            api.method == "GET"
                && api.path == "/api/plugin/capabilities"
                && api.required_permission.is_none()
        }));
        assert!(capabilities.host_apis.iter().any(|api| {
            api.method == "GET"
                && api.path == "/api/plugin/items/{itemId}"
                && api.required_permission.as_deref() == Some("media.read")
        }));
        assert!(capabilities.host_apis.iter().any(|api| {
            api.method == "PATCH"
                && api.path == "/api/plugin/items/{itemId}/metadata"
                && api.required_permission.as_deref() == Some("metadata.write")
        }));
        assert!(capabilities.host_apis.iter().any(|api| {
            api.method == "PUT"
                && api.path == "/api/plugin/items/{itemId}/artwork"
                && api.required_permission.as_deref() == Some("metadata.write")
        }));
        assert!(capabilities.host_apis.iter().any(|api| {
            api.method == "PUT"
                && api.path == "/api/plugin/items/{itemId}/markers"
                && api.required_permission.as_deref() == Some("metadata.write")
        }));
    }

    #[test]
    fn plugin_permission_details_cover_supported_permissions_and_host_api_requirements() {
        let capabilities = plugin_capabilities();
        let supported_permissions = supported_plugin_permissions()
            .iter()
            .copied()
            .collect::<BTreeSet<_>>();
        let described_permissions = capabilities
            .permission_details
            .iter()
            .map(|permission| permission.key.as_str())
            .collect::<BTreeSet<_>>();

        assert_eq!(described_permissions, supported_permissions);

        for permission in &capabilities.permission_details {
            assert!(!permission.category.trim().is_empty());
            assert!(!permission.risk_level.trim().is_empty());
            assert!(!permission.description.trim().is_empty());
        }

        for api in &capabilities.host_apis {
            let Some(required_permission) = api.required_permission.as_deref() else {
                continue;
            };
            let permission = capabilities
                .permission_details
                .iter()
                .find(|permission| permission.key == required_permission)
                .expect("host api permission should be described");

            assert!(
                permission.host_apis.iter().any(|permission_api| {
                    permission_api.method == api.method && permission_api.path == api.path
                }),
                "permission detail for `{required_permission}` should list {} {}",
                api.method,
                api.path
            );
        }

        let admin_menu = capabilities
            .permission_details
            .iter()
            .find(|permission| permission.key == "admin.menu")
            .expect("admin.menu should be described");
        assert!(admin_menu.manifest_features.contains(&"menu".to_owned()));

        let scheduler = capabilities
            .permission_details
            .iter()
            .find(|permission| permission.key == "scheduler.register")
            .expect("scheduler.register should be described");
        assert!(
            scheduler
                .manifest_features
                .contains(&"schedules".to_owned())
        );
    }

    #[test]
    fn host_api_audit_method_constraint_tracks_capabilities() {
        let upgrade_migration =
            include_str!("../../migrations/0043_plugin_host_api_patch_method.sql");

        for api in plugin_capabilities().host_apis {
            let quoted_method = format!("'{}'", api.method);
            assert!(
                upgrade_migration.contains(&quoted_method),
                "upgrade plugin host api audit method constraint is missing {}",
                api.method
            );
        }
    }

    #[test]
    fn media_item_detail_dto_exposes_public_metadata_only() {
        let detail = PluginMediaItemDetailDto {
            id: "item-1".to_owned(),
            library_id: "library-1".to_owned(),
            parent_id: None,
            item_type: "movie".to_owned(),
            title: "Movie".to_owned(),
            original_title: Some("Original Movie".to_owned()),
            sort_title: Some("Movie".to_owned()),
            overview: Some("Overview".to_owned()),
            production_year: Some(2026),
            premiere_date: Some("2026-06-22".to_owned()),
            official_rating: Some("PG-13".to_owned()),
            community_rating: Some(8.1),
            critic_rating: Some(92.0),
            runtime_ticks: Some(7_200_000_000),
            index_number: None,
            parent_index_number: None,
            season_number: None,
            episode_number: None,
            metadata_status: "matched".to_owned(),
            scan_status: "scanned".to_owned(),
            external_ids: vec![PluginMediaExternalIdDto {
                provider: "tmdb".to_owned(),
                external_id: "123".to_owned(),
            }],
            genres: vec!["Drama".to_owned()],
            studios: vec!["Studio A".to_owned()],
            tags: vec!["Favorite".to_owned()],
            people: vec![PluginPersonDto {
                id: "person-1".to_owned(),
                name: "Jane Doe".to_owned(),
                role_type: "actor".to_owned(),
                role_name: "Lead".to_owned(),
                sort_order: 0,
            }],
            markers: vec![PluginMediaMarkerDto {
                marker_type: "intro_start".to_owned(),
                start_ticks: 10,
                end_ticks: Some(20),
                source: "plugin".to_owned(),
                confidence: Some(0.9),
            }],
            artwork: vec![PluginArtworkDto {
                artwork_type: "poster".to_owned(),
                source: "tmdb".to_owned(),
                has_local_image: true,
                remote_url: Some("https://image.example.test/poster.jpg".to_owned()),
                width: Some(1000),
                height: Some(1500),
                is_primary: true,
            }],
        };

        let value = serde_json::to_value(detail).unwrap();

        assert_eq!(value["externalIds"][0]["provider"], "tmdb");
        assert_eq!(value["officialRating"], "PG-13");
        assert_eq!(value["studios"][0], "Studio A");
        assert_eq!(value["people"][0]["roleType"], "actor");
        assert_eq!(value["artwork"][0]["hasLocalImage"], true);
        assert!(value.get("path").is_none());
        assert!(value.get("normalizedPath").is_none());
        assert!(value.get("strmTarget").is_none());
    }

    #[test]
    fn metadata_patch_validation_normalizes_fields_and_external_ids() {
        let input = validate_plugin_metadata_patch(PatchPluginItemMetadataRequestDto {
            title: Some(" Movie ".to_owned()),
            original_title: Some(" Original Movie ".to_owned()),
            sort_title: None,
            overview: None,
            production_year: Some(2024),
            premiere_date: Some("2024-02-29".to_owned()),
            official_rating: Some(" PG-13 ".to_owned()),
            community_rating: Some(8.5),
            critic_rating: Some(92.0),
            runtime_ticks: Some(7_200_000_000),
            external_ids: vec![PatchPluginItemExternalIdDto {
                provider: "TMDB".to_owned(),
                external_id: "123".to_owned(),
            }],
            genres: Some(vec![" Drama ".to_owned(), "科幻".to_owned()]),
            studios: Some(vec![" Studio A ".to_owned(), "工作室".to_owned()]),
            tags: Some(vec![" Favorite ".to_owned()]),
            people: Some(vec![
                PatchPluginItemPersonDto {
                    name: " Jane Doe ".to_owned(),
                    role_type: "Actor".to_owned(),
                    role_name: Some(" Lead ".to_owned()),
                    sort_order: None,
                },
                PatchPluginItemPersonDto {
                    name: "John Writer".to_owned(),
                    role_type: "writer".to_owned(),
                    role_name: None,
                    sort_order: Some(10),
                },
            ]),
        })
        .unwrap();

        assert_eq!(input.title.as_deref(), Some("Movie"));
        assert_eq!(input.original_title.as_deref(), Some("Original Movie"));
        assert_eq!(input.production_year, Some(2024));
        assert_eq!(input.premiere_date.as_deref(), Some("2024-02-29"));
        assert_eq!(input.official_rating.as_deref(), Some("PG-13"));
        assert_eq!(input.community_rating, Some(8.5));
        assert_eq!(input.critic_rating, Some(92.0));
        assert_eq!(input.runtime_ticks, Some(7_200_000_000));
        assert_eq!(input.external_ids[0].provider, "tmdb");
        assert_eq!(input.external_ids[0].external_id, "123");
        let genres = input.genres.as_ref().unwrap();
        assert_eq!(genres[0].name, "Drama");
        assert_eq!(genres[0].name_normalized, "drama");
        assert_eq!(genres[1].name, "科幻");
        assert_eq!(genres[1].name_normalized, "科幻");
        let studios = input.studios.as_ref().unwrap();
        assert_eq!(studios[0].name, "Studio A");
        assert_eq!(studios[0].name_normalized, "studio a");
        assert_eq!(studios[1].name, "工作室");
        assert_eq!(studios[1].name_normalized, "工作室");
        let tags = input.tags.as_ref().unwrap();
        assert_eq!(tags[0].name, "Favorite");
        assert_eq!(tags[0].name_normalized, "favorite");
        let people = input.people.as_ref().unwrap();
        assert_eq!(people[0].name, "Jane Doe");
        assert_eq!(people[0].name_normalized, "jane doe");
        assert_eq!(people[0].role_type, "actor");
        assert_eq!(people[0].role_name, "Lead");
        assert_eq!(people[0].sort_order, 0);
        assert_eq!(people[1].role_type, "writer");
        assert_eq!(people[1].role_name, "");
        assert_eq!(people[1].sort_order, 10);
        assert_eq!(
            input.updated_fields,
            vec![
                "title".to_owned(),
                "originalTitle".to_owned(),
                "productionYear".to_owned(),
                "premiereDate".to_owned(),
                "officialRating".to_owned(),
                "communityRating".to_owned(),
                "criticRating".to_owned(),
                "runtimeTicks".to_owned(),
                "externalIds".to_owned(),
                "genres".to_owned(),
                "studios".to_owned(),
                "tags".to_owned(),
                "people".to_owned(),
            ]
        );
    }

    #[test]
    fn metadata_patch_validation_allows_empty_lists_to_clear() {
        let input = validate_plugin_metadata_patch(PatchPluginItemMetadataRequestDto {
            genres: Some(Vec::new()),
            studios: Some(Vec::new()),
            tags: Some(Vec::new()),
            people: Some(Vec::new()),
            ..Default::default()
        })
        .unwrap();

        assert_eq!(input.genres, Some(Vec::new()));
        assert_eq!(input.studios, Some(Vec::new()));
        assert_eq!(input.tags, Some(Vec::new()));
        assert_eq!(input.people, Some(Vec::new()));
        assert_eq!(
            input.updated_fields,
            vec![
                "genres".to_owned(),
                "studios".to_owned(),
                "tags".to_owned(),
                "people".to_owned()
            ]
        );
    }

    #[test]
    fn metadata_patch_validation_rejects_invalid_values() {
        assert!(
            validate_plugin_metadata_patch(PatchPluginItemMetadataRequestDto::default()).is_err()
        );
        assert!(
            validate_plugin_metadata_patch(PatchPluginItemMetadataRequestDto {
                title: Some(" ".to_owned()),
                ..Default::default()
            })
            .is_err()
        );
        assert!(
            validate_plugin_metadata_patch(PatchPluginItemMetadataRequestDto {
                premiere_date: Some("2023-02-29".to_owned()),
                ..Default::default()
            })
            .is_err()
        );
        assert!(
            validate_plugin_metadata_patch(PatchPluginItemMetadataRequestDto {
                official_rating: Some(" ".to_owned()),
                ..Default::default()
            })
            .is_err()
        );
        assert!(
            validate_plugin_metadata_patch(PatchPluginItemMetadataRequestDto {
                community_rating: Some(10.1),
                ..Default::default()
            })
            .is_err()
        );
        assert!(
            validate_plugin_metadata_patch(PatchPluginItemMetadataRequestDto {
                runtime_ticks: Some(-1),
                ..Default::default()
            })
            .is_err()
        );
        assert!(
            validate_plugin_metadata_patch(PatchPluginItemMetadataRequestDto {
                external_ids: vec![
                    PatchPluginItemExternalIdDto {
                        provider: "tmdb".to_owned(),
                        external_id: "123".to_owned(),
                    },
                    PatchPluginItemExternalIdDto {
                        provider: "TMDB".to_owned(),
                        external_id: "456".to_owned(),
                    },
                ],
                ..Default::default()
            })
            .is_err()
        );
        assert!(
            validate_plugin_metadata_patch(PatchPluginItemMetadataRequestDto {
                external_ids: vec![PatchPluginItemExternalIdDto {
                    provider: "tmdb".to_owned(),
                    external_id: "has space".to_owned(),
                }],
                ..Default::default()
            })
            .is_err()
        );
        assert!(
            validate_plugin_metadata_patch(PatchPluginItemMetadataRequestDto {
                genres: Some(vec!["Drama".to_owned(), " drama ".to_owned()]),
                ..Default::default()
            })
            .is_err()
        );
        assert!(
            validate_plugin_metadata_patch(PatchPluginItemMetadataRequestDto {
                studios: Some(vec!["Studio".to_owned(), " studio ".to_owned()]),
                ..Default::default()
            })
            .is_err()
        );
        assert!(
            validate_plugin_metadata_patch(PatchPluginItemMetadataRequestDto {
                tags: Some(vec![" ".to_owned()]),
                ..Default::default()
            })
            .is_err()
        );
        assert!(
            validate_plugin_metadata_patch(PatchPluginItemMetadataRequestDto {
                people: Some(vec![PatchPluginItemPersonDto {
                    name: " ".to_owned(),
                    role_type: "actor".to_owned(),
                    role_name: None,
                    sort_order: None,
                }]),
                ..Default::default()
            })
            .is_err()
        );
        assert!(
            validate_plugin_metadata_patch(PatchPluginItemMetadataRequestDto {
                people: Some(vec![PatchPluginItemPersonDto {
                    name: "Jane Doe".to_owned(),
                    role_type: "unknown".to_owned(),
                    role_name: None,
                    sort_order: None,
                }]),
                ..Default::default()
            })
            .is_err()
        );
        assert!(
            validate_plugin_metadata_patch(PatchPluginItemMetadataRequestDto {
                people: Some(vec![PatchPluginItemPersonDto {
                    name: "Jane Doe".to_owned(),
                    role_type: "actor".to_owned(),
                    role_name: None,
                    sort_order: Some(-1),
                }]),
                ..Default::default()
            })
            .is_err()
        );
        assert!(
            validate_plugin_metadata_patch(PatchPluginItemMetadataRequestDto {
                people: Some(vec![
                    PatchPluginItemPersonDto {
                        name: "Jane Doe".to_owned(),
                        role_type: "actor".to_owned(),
                        role_name: Some("Lead".to_owned()),
                        sort_order: None,
                    },
                    PatchPluginItemPersonDto {
                        name: " jane doe ".to_owned(),
                        role_type: "ACTOR".to_owned(),
                        role_name: Some("lead".to_owned()),
                        sort_order: Some(1),
                    },
                ]),
                ..Default::default()
            })
            .is_err()
        );
    }

    #[test]
    fn metadata_write_conflict_maps_to_conflict_error() {
        let error = plugin_metadata_write_error_to_app_error(
            PluginMetadataWriteError::ExternalIdConflict {
                provider: "tmdb".to_owned(),
                external_id: "123".to_owned(),
            },
        );

        assert_eq!(error.status_code(), StatusCode::CONFLICT);
    }

    #[test]
    fn artwork_replacement_validation_scopes_source_and_accepts_remote_images() {
        let input = validate_plugin_artwork_replacement(
            "dev.fbz.fanart",
            PutPluginItemArtworkRequestDto {
                source: Some("fanart".to_owned()),
                artwork: vec![
                    PutPluginItemArtworkDto {
                        artwork_type: "Poster".to_owned(),
                        remote_url: "https://image.example.test/poster.jpg".to_owned(),
                        width: Some(1000),
                        height: Some(1500),
                        is_primary: true,
                    },
                    PutPluginItemArtworkDto {
                        artwork_type: "backdrop".to_owned(),
                        remote_url: "http://image.example.test/backdrop.jpg".to_owned(),
                        width: None,
                        height: None,
                        is_primary: false,
                    },
                ],
            },
        )
        .unwrap();

        assert_eq!(input.source, "plugin:dev.fbz.fanart:fanart");
        assert_eq!(input.artwork[0].artwork_type, "poster");
        assert_eq!(
            input.artwork[0].remote_url,
            "https://image.example.test/poster.jpg"
        );
        assert_eq!(input.artwork[0].width, Some(1000));
        assert!(input.artwork[0].is_primary);

        let clear = validate_plugin_artwork_replacement(
            "dev.fbz.fanart",
            PutPluginItemArtworkRequestDto {
                source: None,
                artwork: Vec::new(),
            },
        )
        .unwrap();

        assert_eq!(clear.source, "plugin:dev.fbz.fanart");
        assert!(clear.artwork.is_empty());
    }

    #[test]
    fn artwork_replacement_validation_rejects_unsafe_values() {
        assert!(
            validate_plugin_artwork_replacement(
                "dev.fbz.fanart",
                PutPluginItemArtworkRequestDto {
                    source: Some("../other".to_owned()),
                    artwork: Vec::new(),
                },
            )
            .is_err()
        );
        assert!(
            validate_plugin_artwork_replacement(
                "dev.fbz.fanart",
                PutPluginItemArtworkRequestDto {
                    source: None,
                    artwork: vec![PutPluginItemArtworkDto {
                        artwork_type: "unknown".to_owned(),
                        remote_url: "https://image.example.test/poster.jpg".to_owned(),
                        width: None,
                        height: None,
                        is_primary: false,
                    }],
                },
            )
            .is_err()
        );
        assert!(
            validate_plugin_artwork_replacement(
                "dev.fbz.fanart",
                PutPluginItemArtworkRequestDto {
                    source: None,
                    artwork: vec![PutPluginItemArtworkDto {
                        artwork_type: "poster".to_owned(),
                        remote_url: "file:///tmp/poster.jpg".to_owned(),
                        width: None,
                        height: None,
                        is_primary: false,
                    }],
                },
            )
            .is_err()
        );
        assert!(
            validate_plugin_artwork_replacement(
                "dev.fbz.fanart",
                PutPluginItemArtworkRequestDto {
                    source: None,
                    artwork: vec![PutPluginItemArtworkDto {
                        artwork_type: "poster".to_owned(),
                        remote_url: "https://user:secret@image.example.test/poster.jpg".to_owned(),
                        width: None,
                        height: None,
                        is_primary: false,
                    }],
                },
            )
            .is_err()
        );
        assert!(
            validate_plugin_artwork_replacement(
                "dev.fbz.fanart",
                PutPluginItemArtworkRequestDto {
                    source: None,
                    artwork: vec![PutPluginItemArtworkDto {
                        artwork_type: "poster".to_owned(),
                        remote_url: "https://image.example.test/poster.jpg".to_owned(),
                        width: Some(0),
                        height: None,
                        is_primary: false,
                    }],
                },
            )
            .is_err()
        );
        assert!(
            validate_plugin_artwork_replacement(
                "dev.fbz.fanart",
                PutPluginItemArtworkRequestDto {
                    source: None,
                    artwork: vec![
                        PutPluginItemArtworkDto {
                            artwork_type: "poster".to_owned(),
                            remote_url: "https://image.example.test/poster-a.jpg".to_owned(),
                            width: None,
                            height: None,
                            is_primary: true,
                        },
                        PutPluginItemArtworkDto {
                            artwork_type: "poster".to_owned(),
                            remote_url: "https://image.example.test/poster-b.jpg".to_owned(),
                            width: None,
                            height: None,
                            is_primary: true,
                        },
                    ],
                },
            )
            .is_err()
        );
        assert!(
            validate_plugin_artwork_replacement(
                "dev.fbz.fanart",
                PutPluginItemArtworkRequestDto {
                    source: None,
                    artwork: vec![
                        PutPluginItemArtworkDto {
                            artwork_type: "poster".to_owned(),
                            remote_url: "https://image.example.test/poster.jpg".to_owned(),
                            width: None,
                            height: None,
                            is_primary: false,
                        },
                        PutPluginItemArtworkDto {
                            artwork_type: "poster".to_owned(),
                            remote_url: "https://image.example.test/poster.jpg".to_owned(),
                            width: None,
                            height: None,
                            is_primary: false,
                        },
                    ],
                },
            )
            .is_err()
        );
    }

    #[test]
    fn marker_replacement_validation_scopes_source_and_accepts_supported_markers() {
        let input = validate_plugin_marker_replacement(
            "dev.fbz.tidb",
            PutPluginItemMarkersRequestDto {
                source: Some("tidb".to_owned()),
                markers: vec![
                    PutPluginItemMarkerDto {
                        marker_type: "Intro_Start".to_owned(),
                        start_ticks: 10,
                        end_ticks: Some(20),
                        confidence: Some(0.95),
                    },
                    PutPluginItemMarkerDto {
                        marker_type: "credits_end".to_owned(),
                        start_ticks: 100,
                        end_ticks: None,
                        confidence: None,
                    },
                ],
            },
        )
        .unwrap();

        assert_eq!(input.source, "plugin:dev.fbz.tidb:tidb");
        assert_eq!(input.markers[0].marker_type, "intro_start");
        assert_eq!(input.markers[0].confidence, Some(0.95));

        let clear = validate_plugin_marker_replacement(
            "dev.fbz.tidb",
            PutPluginItemMarkersRequestDto {
                source: None,
                markers: Vec::new(),
            },
        )
        .unwrap();

        assert_eq!(clear.source, "plugin:dev.fbz.tidb");
        assert!(clear.markers.is_empty());
    }

    #[test]
    fn marker_replacement_validation_rejects_unsafe_values() {
        assert!(
            validate_plugin_marker_replacement(
                "dev.fbz.tidb",
                PutPluginItemMarkersRequestDto {
                    source: Some("../other".to_owned()),
                    markers: Vec::new(),
                },
            )
            .is_err()
        );
        assert!(
            validate_plugin_marker_replacement(
                "dev.fbz.tidb",
                PutPluginItemMarkersRequestDto {
                    source: None,
                    markers: vec![PutPluginItemMarkerDto {
                        marker_type: "unknown".to_owned(),
                        start_ticks: 0,
                        end_ticks: None,
                        confidence: None,
                    }],
                },
            )
            .is_err()
        );
        assert!(
            validate_plugin_marker_replacement(
                "dev.fbz.tidb",
                PutPluginItemMarkersRequestDto {
                    source: None,
                    markers: vec![PutPluginItemMarkerDto {
                        marker_type: "intro_start".to_owned(),
                        start_ticks: 20,
                        end_ticks: Some(10),
                        confidence: None,
                    }],
                },
            )
            .is_err()
        );
        assert!(
            validate_plugin_marker_replacement(
                "dev.fbz.tidb",
                PutPluginItemMarkersRequestDto {
                    source: None,
                    markers: vec![PutPluginItemMarkerDto {
                        marker_type: "intro_start".to_owned(),
                        start_ticks: 0,
                        end_ticks: None,
                        confidence: Some(1.2),
                    }],
                },
            )
            .is_err()
        );
        assert!(
            validate_plugin_marker_replacement(
                "dev.fbz.tidb",
                PutPluginItemMarkersRequestDto {
                    source: None,
                    markers: vec![
                        PutPluginItemMarkerDto {
                            marker_type: "intro_start".to_owned(),
                            start_ticks: 0,
                            end_ticks: None,
                            confidence: None,
                        },
                        PutPluginItemMarkerDto {
                            marker_type: "intro_start".to_owned(),
                            start_ticks: 0,
                            end_ticks: Some(10),
                            confidence: None,
                        },
                    ],
                },
            )
            .is_err()
        );
    }

    #[test]
    fn library_items_index_matches_host_api_ordering() {
        let migration = include_str!("../../migrations/0029_plugin_host_library_items_index.sql");

        assert!(migration.contains("idx_media_items_library_sort_visible"));
        assert!(migration.contains("library_id"));
        assert!(migration.contains("coalesce(nullif(sort_title, ''), title)"));
        assert!(migration.contains("where is_deleted = false"));
    }

    #[test]
    fn host_api_call_budget_index_matches_limit_query() {
        let migration = include_str!("../../migrations/0030_plugin_host_api_call_budget_index.sql");

        assert!(migration.contains("idx_plugin_host_api_calls_execution_plugin_budget"));
        assert!(migration.contains("execution_run_id, plugin_id, id"));
        assert!(migration.contains("where execution_run_id is not null"));
    }

    #[test]
    fn authenticate_token_sql_keeps_update_target_alias_out_of_from_join_predicates() {
        assert!(AUTHENTICATE_PLUGIN_HOST_TOKEN_SQL.contains("update plugin_host_tokens token"));
        assert!(
            AUTHENTICATE_PLUGIN_HOST_TOKEN_SQL.contains("cross join plugin_execution_runs run")
        );
        assert!(AUTHENTICATE_PLUGIN_HOST_TOKEN_SQL.contains("and run.id = token.execution_run_id"));
        assert!(
            !AUTHENTICATE_PLUGIN_HOST_TOKEN_SQL
                .contains("join plugin_execution_runs run on run.id = token.execution_run_id")
        );
    }

    #[test]
    fn permission_snapshot_extracts_unique_non_empty_keys() {
        let snapshot = json!([
            {"key": "library.read", "scope": null},
            {"key": " notification.send "},
            {"key": "library.read"},
            {"key": ""},
            {"scope": "missing-key"},
            "invalid"
        ]);

        let keys = permission_keys_from_snapshot(&snapshot);

        assert_eq!(
            keys,
            vec!["library.read".to_owned(), "notification.send".to_owned()]
        );
    }

    #[test]
    fn permission_snapshot_rejects_missing_required_permission() {
        let context = PluginHostContext {
            token_id: 1,
            plugin_id: "dev.fbz.notify".to_owned(),
            package_id: "package-1".to_owned(),
            execution_run_id: 2,
            permission_keys: vec!["library.read".to_owned()],
        };

        assert!(require_plugin_permission(&context, "library.read").is_ok());
        assert!(require_plugin_permission(&context, "notification.send").is_err());
    }

    #[test]
    fn host_api_audit_maps_success_and_error_statuses() {
        let success = host_api_call_audit(
            HOST_API_LIST_LIBRARIES,
            Duration::from_millis(12),
            &Ok::<_, AppError>(()),
        );

        assert_eq!(success.status_code, StatusCode::OK);
        assert_eq!(success.error_code, None);
        assert_eq!(success.duration, Duration::from_millis(12));

        let failure = host_api_call_audit(
            HOST_API_SEND_NOTIFICATION,
            Duration::from_millis(7),
            &Err::<(), _>(AppError::forbidden("missing permission")),
        );

        assert_eq!(failure.status_code, StatusCode::FORBIDDEN);
        assert_eq!(failure.error_code.as_deref(), Some("forbidden"));
        assert_eq!(failure.error_message.as_deref(), Some("missing permission"));

        let limited = host_api_call_audit(
            HOST_API_CAPABILITIES,
            Duration::from_millis(3),
            &Err::<(), _>(host_api_call_limit_error(10)),
        );

        assert_eq!(limited.status_code, StatusCode::TOO_MANY_REQUESTS);
        assert_eq!(limited.error_code.as_deref(), Some("too_many_requests"));
        assert_eq!(
            limited.error_message.as_deref(),
            Some("plugin host api call limit of 10 per execution run exceeded")
        );
    }

    #[test]
    fn audit_error_truncation_preserves_utf8_boundary() {
        let value = "错误错误错误";

        assert_eq!(truncate_str(value, 7), "错误");
    }

    #[test]
    fn notification_input_normalizes_and_validates_payload() {
        let input = PluginNotificationInput::try_from(SendPluginNotificationRequestDto {
            title: "  Scan complete  ".to_owned(),
            message: "  2 new items  ".to_owned(),
            level: Some("SUCCESS".to_owned()),
            channel: Some("tg.primary".to_owned()),
            metadata: Some(json!({ "libraryId": "library-1" })),
        })
        .unwrap();

        assert_eq!(input.title, "Scan complete");
        assert_eq!(input.message, "2 new items");
        assert_eq!(input.level, "success");
        assert_eq!(input.channel.as_deref(), Some("tg.primary"));
    }

    #[test]
    fn notification_input_rejects_invalid_level_channel_or_metadata() {
        assert!(
            PluginNotificationInput::try_from(SendPluginNotificationRequestDto {
                title: "title".to_owned(),
                message: "message".to_owned(),
                level: Some("panic".to_owned()),
                channel: None,
                metadata: None,
            })
            .is_err()
        );
        assert!(
            PluginNotificationInput::try_from(SendPluginNotificationRequestDto {
                title: "title".to_owned(),
                message: "message".to_owned(),
                level: None,
                channel: Some("../bad".to_owned()),
                metadata: None,
            })
            .is_err()
        );
        assert!(
            PluginNotificationInput::try_from(SendPluginNotificationRequestDto {
                title: "title".to_owned(),
                message: "message".to_owned(),
                level: None,
                channel: None,
                metadata: Some(json!(["not", "object"])),
            })
            .is_err()
        );
    }
}
