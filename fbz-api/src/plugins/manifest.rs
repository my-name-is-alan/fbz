use std::{
    collections::BTreeSet,
    error::Error,
    fmt::{Display, Formatter},
};

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::scheduler::service::{parse_interval_seconds, validate_cron_expression};

pub const SUPPORTED_PLUGIN_API_VERSION: &str = "1";
pub const SUPPORTED_PLUGIN_RUNTIMES: &[&str] = &["http", "wasi"];
const MAX_PERMISSIONS: usize = 64;
const MAX_HOOKS: usize = 128;
const MAX_SCHEDULES: usize = 32;
const MAX_MENU_ITEMS: usize = 32;
const MAX_CONFIG_FIELDS: usize = 64;
const MAX_CONFIG_OPTIONS: usize = 64;

const SUPPORTED_PERMISSIONS: &[&str] = &[
    "admin.menu",
    "library.read",
    "library.write",
    "media.read",
    "metadata.read",
    "metadata.write",
    "notification.send",
    "playback.read",
    "scheduler.register",
    "webhook.emit",
];

const SUPPORTED_HOOK_EVENTS: &[&str] = &[
    "library.scan.started",
    "library.scan.completed",
    "library.scan.failed",
    "media.item.created",
    "media.item.updated",
    "media.download.started",
    "metadata.refresh.completed",
    "metadata.refresh.failed",
    "metadata.provider.query",
    "playback.started",
    "playback.stopped",
    "scheduler.tick",
    "transcode.started",
    "transcode.completed",
    "transcode.failed",
    "user.login",
    "webhook.received",
];

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct PluginManifest {
    pub id: String,
    pub name: String,
    pub version: String,
    #[serde(rename = "apiVersion")]
    pub api_version: String,
    pub runtime: String,
    pub entrypoint: String,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub permissions: Vec<PluginPermissionManifest>,
    #[serde(default)]
    pub hooks: Vec<PluginHookManifest>,
    #[serde(default)]
    pub schedules: Vec<PluginScheduleManifest>,
    #[serde(default)]
    pub menu: Vec<PluginMenuItemManifest>,
    #[serde(default, rename = "configSchema")]
    pub config_schema: Vec<PluginConfigFieldManifest>,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct PluginPermissionManifest {
    pub key: String,
    #[serde(default)]
    pub scope: Option<String>,
    #[serde(default)]
    pub reason: Option<String>,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct PluginHookManifest {
    pub event: String,
    pub handler: String,
    #[serde(default)]
    pub priority: i16,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct PluginScheduleManifest {
    pub key: String,
    pub schedule_kind: String,
    pub schedule_value: String,
    pub handler: String,
    #[serde(default)]
    pub enabled_by_default: bool,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct PluginMenuItemManifest {
    pub key: String,
    pub label: String,
    pub path: String,
    #[serde(default)]
    pub parent_key: Option<String>,
    #[serde(default)]
    pub required_permission: Option<String>,
    #[serde(default)]
    pub weight: i16,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct PluginConfigFieldManifest {
    pub key: String,
    pub label: String,
    #[serde(rename = "type")]
    pub value_type: String,
    #[serde(default)]
    pub required: bool,
    #[serde(default)]
    pub help_text: Option<String>,
    #[serde(default)]
    pub options: Vec<PluginConfigOptionManifest>,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct PluginConfigOptionManifest {
    pub value: String,
    pub label: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ValidatedPluginManifest {
    pub manifest: PluginManifest,
    pub manifest_hash: Vec<u8>,
    pub permission_fingerprint: Vec<u8>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PluginManifestError {
    InvalidField {
        field: &'static str,
        message: String,
    },
    UnsupportedApiVersion(String),
    UnsupportedRuntime(String),
    UnsupportedPermission(String),
    UnsupportedHookEvent(String),
    MissingPermission {
        permission: &'static str,
        feature: &'static str,
    },
}

impl PluginManifest {
    pub fn validate(self) -> Result<ValidatedPluginManifest, PluginManifestError> {
        validate_identifier("id", &self.id)?;
        if self.id.starts_with("core.") {
            return Err(invalid_field("id", "the core. namespace is reserved"));
        }
        validate_non_empty("name", &self.name, 128)?;
        validate_version(&self.version)?;

        if self.api_version.trim() != SUPPORTED_PLUGIN_API_VERSION {
            return Err(PluginManifestError::UnsupportedApiVersion(
                self.api_version.trim().to_owned(),
            ));
        }

        validate_runtime(&self.runtime, &self.entrypoint)?;
        validate_count("permissions", self.permissions.len(), MAX_PERMISSIONS)?;
        validate_count("hooks", self.hooks.len(), MAX_HOOKS)?;
        validate_count("schedules", self.schedules.len(), MAX_SCHEDULES)?;
        validate_count("menu", self.menu.len(), MAX_MENU_ITEMS)?;
        validate_count("configSchema", self.config_schema.len(), MAX_CONFIG_FIELDS)?;

        for permission in &self.permissions {
            validate_permission(permission)?;
        }
        validate_permissions(&self.permissions)?;
        for hook in &self.hooks {
            validate_hook(hook)?;
        }
        validate_hooks(&self.hooks)?;
        for schedule in &self.schedules {
            validate_schedule(&self.id, schedule)?;
        }
        validate_schedules(&self.schedules)?;
        for item in &self.menu {
            validate_menu_item(&self.id, item)?;
        }
        validate_menu_items(&self.menu, &self.permissions)?;
        validate_config_schema(&self.config_schema)?;

        if !self.menu.is_empty() && !has_permission(&self.permissions, "admin.menu") {
            return Err(PluginManifestError::MissingPermission {
                permission: "admin.menu",
                feature: "menu",
            });
        }

        if !self.schedules.is_empty() && !has_permission(&self.permissions, "scheduler.register") {
            return Err(PluginManifestError::MissingPermission {
                permission: "scheduler.register",
                feature: "schedules",
            });
        }

        let manifest_hash = hash_json(&self)?;
        let permission_fingerprint = hash_permissions(&self.permissions);

        Ok(ValidatedPluginManifest {
            manifest: self,
            manifest_hash,
            permission_fingerprint,
        })
    }
}

pub fn supported_plugin_permissions() -> &'static [&'static str] {
    SUPPORTED_PERMISSIONS
}

pub fn supported_plugin_hook_events() -> &'static [&'static str] {
    SUPPORTED_HOOK_EVENTS
}

fn validate_runtime(runtime: &str, entrypoint: &str) -> Result<(), PluginManifestError> {
    match runtime.trim() {
        "wasi" => validate_relative_entrypoint(entrypoint),
        "http" => validate_http_entrypoint(entrypoint),
        other => Err(PluginManifestError::UnsupportedRuntime(other.to_owned())),
    }
}

fn validate_permission(permission: &PluginPermissionManifest) -> Result<(), PluginManifestError> {
    let key = permission.key.trim();
    if !SUPPORTED_PERMISSIONS.contains(&key) {
        return Err(PluginManifestError::UnsupportedPermission(key.to_owned()));
    }
    if let Some(scope) = &permission.scope {
        validate_non_empty("permissions.scope", scope, 256)?;
    }
    if let Some(reason) = &permission.reason {
        validate_non_empty("permissions.reason", reason, 512)?;
    }
    Ok(())
}

fn validate_permissions(
    permissions: &[PluginPermissionManifest],
) -> Result<(), PluginManifestError> {
    let mut seen = BTreeSet::new();
    for permission in permissions {
        let permission_key = permission.key.trim();
        let permission_scope = permission.scope.as_deref().unwrap_or("").trim();
        if !seen.insert(format!("{permission_key}\u{0}{permission_scope}")) {
            return Err(invalid_field(
                "permissions.key",
                "permission key and scope pairs must be unique",
            ));
        }
    }

    Ok(())
}

fn validate_hook(hook: &PluginHookManifest) -> Result<(), PluginManifestError> {
    let event = hook.event.trim();
    if !SUPPORTED_HOOK_EVENTS.contains(&event) {
        return Err(PluginManifestError::UnsupportedHookEvent(event.to_owned()));
    }
    validate_handler("hooks.handler", &hook.handler)
}

fn validate_hooks(hooks: &[PluginHookManifest]) -> Result<(), PluginManifestError> {
    let mut seen = BTreeSet::new();
    for hook in hooks {
        let event = hook.event.trim();
        let handler = hook.handler.trim();
        if !seen.insert(format!("{event}\u{0}{handler}")) {
            return Err(invalid_field(
                "hooks.handler",
                "event and handler pairs must be unique",
            ));
        }
    }

    Ok(())
}

fn validate_schedule(
    plugin_id: &str,
    schedule: &PluginScheduleManifest,
) -> Result<(), PluginManifestError> {
    validate_prefixed_key("schedules.key", plugin_id, &schedule.key)?;
    validate_handler("schedules.handler", &schedule.handler)?;

    match schedule.schedule_kind.trim() {
        "interval" => {
            parse_interval_seconds(&schedule.schedule_value)
                .map_err(|err| invalid_field("schedules.scheduleValue", err.to_string()))?;
            Ok(())
        }
        "cron" => {
            validate_non_empty("schedules.scheduleValue", &schedule.schedule_value, 128)?;
            validate_cron_expression(&schedule.schedule_value)
                .map_err(|err| invalid_field("schedules.scheduleValue", err.to_string()))
        }
        other => Err(invalid_field(
            "schedules.scheduleKind",
            format!("unsupported schedule kind `{other}`"),
        )),
    }
}

fn validate_schedules(schedules: &[PluginScheduleManifest]) -> Result<(), PluginManifestError> {
    let mut task_keys = BTreeSet::new();
    for schedule in schedules {
        let task_key = schedule.key.trim();
        if !task_keys.insert(task_key.to_owned()) {
            return Err(invalid_field("schedules.key", "must be unique"));
        }
    }

    Ok(())
}

fn validate_menu_item(
    plugin_id: &str,
    item: &PluginMenuItemManifest,
) -> Result<(), PluginManifestError> {
    validate_prefixed_key("menu.key", plugin_id, &item.key)?;
    validate_non_empty("menu.label", &item.label, 64)?;
    let expected_prefix = format!("/admin/plugins/{plugin_id}");
    if !menu_path_is_in_plugin_namespace(&item.path, &expected_prefix) {
        return Err(invalid_field(
            "menu.path",
            format!("must be `{expected_prefix}` or a child path under it"),
        ));
    }
    if item.path.contains(char::is_whitespace) {
        return Err(invalid_field("menu.path", "must not contain whitespace"));
    }
    if let Some(parent_key) = &item.parent_key {
        validate_prefixed_key("menu.parentKey", plugin_id, parent_key)?;
    }
    if let Some(required_permission) = &item.required_permission {
        validate_permission_key("menu.requiredPermission", required_permission)?;
    }
    Ok(())
}

fn validate_menu_items(
    items: &[PluginMenuItemManifest],
    permissions: &[PluginPermissionManifest],
) -> Result<(), PluginManifestError> {
    let mut item_keys = BTreeSet::new();
    for item in items {
        let item_key = item.key.trim();
        if !item_keys.insert(item_key.to_owned()) {
            return Err(invalid_field("menu.key", "must be unique"));
        }
    }

    for item in items {
        if let Some(parent_key) = item.parent_key.as_deref().map(str::trim) {
            if parent_key == item.key.trim() {
                return Err(invalid_field("menu.parentKey", "must not reference itself"));
            }
            if !item_keys.contains(parent_key) {
                return Err(invalid_field(
                    "menu.parentKey",
                    "must reference another declared menu key",
                ));
            }
        }
        if let Some(required_permission) = item.required_permission.as_deref().map(str::trim) {
            if !has_permission(permissions, required_permission) {
                return Err(invalid_field(
                    "menu.requiredPermission",
                    "must be declared in permissions",
                ));
            }
        }
    }

    Ok(())
}

fn menu_path_is_in_plugin_namespace(path: &str, expected_prefix: &str) -> bool {
    let path = path.trim();
    path == expected_prefix || path.starts_with(&format!("{expected_prefix}/"))
}

fn validate_config_schema(fields: &[PluginConfigFieldManifest]) -> Result<(), PluginManifestError> {
    let mut keys = BTreeSet::new();
    for field in fields {
        validate_config_key("configSchema.key", &field.key)?;
        if !keys.insert(field.key.trim().to_owned()) {
            return Err(invalid_field("configSchema.key", "must be unique"));
        }
        validate_non_empty("configSchema.label", &field.label, 64)?;
        validate_config_type(&field.value_type)?;
        if let Some(help_text) = &field.help_text {
            validate_non_empty("configSchema.helpText", help_text, 256)?;
        }
        validate_config_options(field)?;
    }

    Ok(())
}

fn validate_config_key(field: &'static str, value: &str) -> Result<(), PluginManifestError> {
    let value = value.trim();
    if value.is_empty() || value.len() > 64 {
        return Err(invalid_field(field, "must be 1 to 64 characters"));
    }
    if value.contains("..") {
        return Err(invalid_field(field, "must not contain consecutive dots"));
    }
    if !value.bytes().all(|byte| {
        byte.is_ascii_lowercase() || byte.is_ascii_digit() || matches!(byte, b'.' | b'_' | b'-')
    }) {
        return Err(invalid_field(
            field,
            "must contain only lowercase ascii letters, digits, dot, underscore, or dash",
        ));
    }
    if value.starts_with(['.', '_', '-']) || value.ends_with(['.', '_', '-']) {
        return Err(invalid_field(
            field,
            "must start and end with a letter or digit",
        ));
    }
    Ok(())
}

fn validate_config_type(value: &str) -> Result<(), PluginManifestError> {
    match value.trim() {
        "string" | "number" | "boolean" | "url" | "select" | "secret" | "password" => Ok(()),
        other => Err(invalid_field(
            "configSchema.type",
            format!("unsupported config field type `{other}`"),
        )),
    }
}

fn validate_config_options(field: &PluginConfigFieldManifest) -> Result<(), PluginManifestError> {
    let is_select = field.value_type.trim() == "select";
    if !is_select && !field.options.is_empty() {
        return Err(invalid_field(
            "configSchema.options",
            "only select fields may declare options",
        ));
    }
    if is_select && field.options.is_empty() {
        return Err(invalid_field(
            "configSchema.options",
            "select fields must declare at least one option",
        ));
    }
    validate_count(
        "configSchema.options",
        field.options.len(),
        MAX_CONFIG_OPTIONS,
    )?;

    let mut values = BTreeSet::new();
    for option in &field.options {
        validate_non_empty("configSchema.options.value", &option.value, 128)?;
        if !values.insert(option.value.trim().to_owned()) {
            return Err(invalid_field(
                "configSchema.options.value",
                "must be unique per field",
            ));
        }
        validate_non_empty("configSchema.options.label", &option.label, 64)?;
    }

    Ok(())
}

fn validate_identifier(field: &'static str, value: &str) -> Result<(), PluginManifestError> {
    let value = value.trim();
    if value.len() < 3 || value.len() > 128 {
        return Err(invalid_field(field, "must be 3 to 128 characters"));
    }
    if value.contains("..") {
        return Err(invalid_field(field, "must not contain consecutive dots"));
    }
    if !value.bytes().all(|byte| {
        byte.is_ascii_lowercase() || byte.is_ascii_digit() || matches!(byte, b'.' | b'_' | b'-')
    }) {
        return Err(invalid_field(
            field,
            "must contain only lowercase ascii letters, digits, dot, underscore, or dash",
        ));
    }
    if value.starts_with(['.', '_', '-']) || value.ends_with(['.', '_', '-']) {
        return Err(invalid_field(
            field,
            "must start and end with a letter or digit",
        ));
    }
    Ok(())
}

fn validate_prefixed_key(
    field: &'static str,
    plugin_id: &str,
    value: &str,
) -> Result<(), PluginManifestError> {
    validate_identifier(field, value)?;
    let prefix = format!("{plugin_id}.");
    if !value.starts_with(&prefix) {
        return Err(invalid_field(field, format!("must start with `{prefix}`")));
    }
    Ok(())
}

fn validate_permission_key(field: &'static str, value: &str) -> Result<(), PluginManifestError> {
    let value = value.trim();
    if SUPPORTED_PERMISSIONS.contains(&value) {
        return Ok(());
    }
    Err(invalid_field(
        field,
        "must reference a supported permission",
    ))
}

fn validate_non_empty(
    field: &'static str,
    value: &str,
    max_len: usize,
) -> Result<(), PluginManifestError> {
    let value = value.trim();
    if value.is_empty() {
        return Err(invalid_field(field, "must not be empty"));
    }
    if value.len() > max_len {
        return Err(invalid_field(
            field,
            format!("must be at most {max_len} characters"),
        ));
    }
    Ok(())
}

fn validate_version(value: &str) -> Result<(), PluginManifestError> {
    let value = value.trim();
    validate_non_empty("version", value, 64)?;
    let mut parts = value.split('.');
    let Some(major) = parts.next() else {
        return Err(invalid_field("version", "must be semver-like"));
    };
    let Some(minor) = parts.next() else {
        return Err(invalid_field("version", "must include major and minor"));
    };
    if major.parse::<u64>().is_err() || minor.parse::<u64>().is_err() {
        return Err(invalid_field("version", "major and minor must be numeric"));
    }
    Ok(())
}

fn validate_relative_entrypoint(entrypoint: &str) -> Result<(), PluginManifestError> {
    validate_non_empty("entrypoint", entrypoint, 256)?;
    if entrypoint.starts_with(['/', '\\']) || entrypoint.contains(':') {
        return Err(invalid_field(
            "entrypoint",
            "must be a relative package path",
        ));
    }
    let has_parent_segment = entrypoint
        .split(['/', '\\'])
        .any(|segment| segment.trim() == "..");
    if has_parent_segment {
        return Err(invalid_field("entrypoint", "must not escape the package"));
    }
    Ok(())
}

fn validate_http_entrypoint(entrypoint: &str) -> Result<(), PluginManifestError> {
    validate_non_empty("entrypoint", entrypoint, 2048)?;
    if !entrypoint.starts_with("http://") && !entrypoint.starts_with("https://") {
        return Err(invalid_field(
            "entrypoint",
            "http runtime entrypoint must be an http or https URL",
        ));
    }
    if entrypoint.contains(char::is_whitespace) {
        return Err(invalid_field("entrypoint", "must not contain whitespace"));
    }
    Ok(())
}

fn validate_handler(field: &'static str, handler: &str) -> Result<(), PluginManifestError> {
    validate_non_empty(field, handler, 256)?;
    if handler.contains("..") || handler.contains(char::is_whitespace) {
        return Err(invalid_field(
            field,
            "must not contain whitespace or parent traversal",
        ));
    }
    if !handler.bytes().all(|byte| {
        byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'_' | b'-' | b':' | b'/')
    }) {
        return Err(invalid_field(field, "contains unsupported characters"));
    }
    Ok(())
}

fn validate_count(
    field: &'static str,
    count: usize,
    max: usize,
) -> Result<(), PluginManifestError> {
    if count > max {
        return Err(invalid_field(
            field,
            format!("must contain at most {max} items"),
        ));
    }
    Ok(())
}

fn has_permission(permissions: &[PluginPermissionManifest], key: &str) -> bool {
    permissions
        .iter()
        .any(|permission| permission.key.trim() == key)
}

fn hash_json(manifest: &PluginManifest) -> Result<Vec<u8>, PluginManifestError> {
    let bytes = serde_json::to_vec(manifest)
        .map_err(|err| invalid_field("manifest", format!("failed to serialize: {err}")))?;
    Ok(Sha256::digest(bytes).to_vec())
}

fn hash_permissions(permissions: &[PluginPermissionManifest]) -> Vec<u8> {
    let mut entries = permissions
        .iter()
        .map(|permission| {
            format!(
                "{}\u{0}{}",
                permission.key.trim(),
                permission.scope.as_deref().unwrap_or("").trim()
            )
        })
        .collect::<Vec<_>>();
    entries.sort();
    Sha256::digest(entries.join("\n").as_bytes()).to_vec()
}

fn invalid_field(field: &'static str, message: impl Into<String>) -> PluginManifestError {
    PluginManifestError::InvalidField {
        field,
        message: message.into(),
    }
}

impl Display for PluginManifestError {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::InvalidField { field, message } => {
                write!(f, "invalid plugin manifest field {field}: {message}")
            }
            Self::UnsupportedApiVersion(version) => {
                write!(f, "unsupported plugin api version `{version}`")
            }
            Self::UnsupportedRuntime(runtime) => {
                write!(f, "unsupported plugin runtime `{runtime}`")
            }
            Self::UnsupportedPermission(permission) => {
                write!(f, "unsupported plugin permission `{permission}`")
            }
            Self::UnsupportedHookEvent(event) => {
                write!(f, "unsupported plugin hook event `{event}`")
            }
            Self::MissingPermission {
                permission,
                feature,
            } => {
                write!(
                    f,
                    "plugin feature `{feature}` requires permission `{permission}`"
                )
            }
        }
    }
}

impl Error for PluginManifestError {}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    #[test]
    fn valid_manifest_is_accepted_and_fingerprinted() {
        let manifest: PluginManifest = serde_json::from_value(json!({
            "id": "dev.fbz.notify",
            "name": "Notify",
            "version": "1.0.0",
            "apiVersion": "1",
            "runtime": "wasi",
            "entrypoint": "plugin.wasm",
            "permissions": [
                { "key": "admin.menu", "reason": "adds management view" },
                { "key": "scheduler.register", "reason": "runs periodic checks" },
                { "key": "notification.send", "scope": "telegram" }
            ],
            "hooks": [
                { "event": "library.scan.completed", "handler": "hooks.onScanCompleted" }
            ],
            "schedules": [
                {
                    "key": "dev.fbz.notify.digest",
                    "scheduleKind": "interval",
                    "scheduleValue": "15m",
                    "handler": "jobs.digest"
                }
            ],
            "menu": [
                {
                    "key": "dev.fbz.notify.settings",
                    "label": "Notify",
                    "path": "/admin/plugins/dev.fbz.notify/settings",
                    "requiredPermission": "admin.menu"
                }
            ]
        }))
        .unwrap();

        let validated = manifest.validate().unwrap();

        assert_eq!(validated.manifest.id, "dev.fbz.notify");
        assert_eq!(validated.manifest_hash.len(), 32);
        assert_eq!(validated.permission_fingerprint.len(), 32);
    }

    #[test]
    fn first_party_http_notification_bridge_manifest_is_valid() {
        let manifest: PluginManifest = serde_json::from_str(include_str!(
            "../../examples/plugins/http-notification-bridge/manifest.json"
        ))
        .unwrap();

        let validated = manifest.validate().unwrap();

        assert_eq!(validated.manifest.id, "dev.fbz.notify.bridge");
        assert!(
            validated
                .manifest
                .permissions
                .iter()
                .any(|permission| permission.key == "notification.send")
        );
        assert!(
            validated
                .manifest
                .hooks
                .iter()
                .any(|hook| hook.event == "library.scan.completed")
        );
    }

    #[test]
    fn first_party_http_marker_importer_manifest_is_valid() {
        let manifest: PluginManifest = serde_json::from_str(include_str!(
            "../../examples/plugins/http-marker-importer/manifest.json"
        ))
        .unwrap();

        let validated = manifest.validate().unwrap();

        assert_eq!(validated.manifest.id, "dev.fbz.marker.importer");
        assert!(
            validated
                .manifest
                .permissions
                .iter()
                .any(|permission| permission.key == "media.read")
        );
        assert!(
            validated
                .manifest
                .permissions
                .iter()
                .any(|permission| permission.key == "metadata.write")
        );
        assert!(
            validated
                .manifest
                .hooks
                .iter()
                .any(|hook| hook.event == "metadata.refresh.completed")
        );
    }

    #[test]
    fn first_party_wasi_scan_logger_manifest_is_valid() {
        let manifest: PluginManifest = serde_json::from_str(include_str!(
            "../../examples/plugins/wasi-scan-logger-template/manifest.json"
        ))
        .unwrap();

        let validated = manifest.validate().unwrap();

        assert_eq!(validated.manifest.id, "dev.fbz.wasi.scan-logger");
        assert_eq!(validated.manifest.runtime, "wasi");
        // Relative in-package entrypoint (the compiled wasm), not an http URL.
        assert_eq!(validated.manifest.entrypoint, "plugin.wasm");
        // A no-network compute plugin needs no Host API permissions.
        assert!(validated.manifest.permissions.is_empty());
        assert!(
            validated
                .manifest
                .hooks
                .iter()
                .any(|hook| hook.event == "library.scan.completed")
        );
    }

    #[test]
    fn menu_requires_admin_menu_permission() {
        let manifest: PluginManifest = serde_json::from_value(json!({
            "id": "dev.fbz.menu",
            "name": "Menu",
            "version": "1.0.0",
            "apiVersion": "1",
            "runtime": "wasi",
            "entrypoint": "plugin.wasm",
            "menu": [
                {
                    "key": "dev.fbz.menu.settings",
                    "label": "Menu",
                    "path": "/admin/plugins/dev.fbz.menu/settings"
                }
            ]
        }))
        .unwrap();

        assert!(matches!(
            manifest.validate(),
            Err(PluginManifestError::MissingPermission {
                permission: "admin.menu",
                feature: "menu"
            })
        ));
    }

    #[test]
    fn duplicate_permission_key_scope_pairs_are_rejected() {
        let duplicate_without_scope: PluginManifest = serde_json::from_value(json!({
            "id": "dev.fbz.permissions",
            "name": "Permissions",
            "version": "1.0.0",
            "apiVersion": "1",
            "runtime": "wasi",
            "entrypoint": "plugin.wasm",
            "permissions": [
                { "key": "notification.send" },
                { "key": "notification.send" }
            ]
        }))
        .unwrap();
        assert!(matches!(
            duplicate_without_scope.validate(),
            Err(PluginManifestError::InvalidField {
                field: "permissions.key",
                ..
            })
        ));

        let duplicate_with_scope: PluginManifest = serde_json::from_value(json!({
            "id": "dev.fbz.permissions",
            "name": "Permissions",
            "version": "1.0.0",
            "apiVersion": "1",
            "runtime": "wasi",
            "entrypoint": "plugin.wasm",
            "permissions": [
                { "key": "notification.send", "scope": "telegram" },
                { "key": "notification.send", "scope": "telegram" }
            ]
        }))
        .unwrap();
        assert!(matches!(
            duplicate_with_scope.validate(),
            Err(PluginManifestError::InvalidField {
                field: "permissions.key",
                ..
            })
        ));
    }

    #[test]
    fn menu_path_must_stay_inside_plugin_namespace() {
        let manifest: PluginManifest = serde_json::from_value(json!({
            "id": "dev.fbz.menu",
            "name": "Menu",
            "version": "1.0.0",
            "apiVersion": "1",
            "runtime": "wasi",
            "entrypoint": "plugin.wasm",
            "permissions": [
                { "key": "admin.menu" }
            ],
            "menu": [
                {
                    "key": "dev.fbz.menu.settings",
                    "label": "Menu",
                    "path": "/admin/plugins/dev.fbz.menu-evil/settings"
                }
            ]
        }))
        .unwrap();

        assert!(matches!(
            manifest.validate(),
            Err(PluginManifestError::InvalidField {
                field: "menu.path",
                ..
            })
        ));
    }

    #[test]
    fn menu_parent_must_reference_declared_sibling() {
        let missing_parent: PluginManifest = serde_json::from_value(json!({
            "id": "dev.fbz.menu",
            "name": "Menu",
            "version": "1.0.0",
            "apiVersion": "1",
            "runtime": "wasi",
            "entrypoint": "plugin.wasm",
            "permissions": [
                { "key": "admin.menu" }
            ],
            "menu": [
                {
                    "key": "dev.fbz.menu.settings",
                    "label": "Menu",
                    "path": "/admin/plugins/dev.fbz.menu/settings",
                    "parentKey": "dev.fbz.menu.missing"
                }
            ]
        }))
        .unwrap();
        let self_parent: PluginManifest = serde_json::from_value(json!({
            "id": "dev.fbz.menu",
            "name": "Menu",
            "version": "1.0.0",
            "apiVersion": "1",
            "runtime": "wasi",
            "entrypoint": "plugin.wasm",
            "permissions": [
                { "key": "admin.menu" }
            ],
            "menu": [
                {
                    "key": "dev.fbz.menu.settings",
                    "label": "Menu",
                    "path": "/admin/plugins/dev.fbz.menu/settings",
                    "parentKey": "dev.fbz.menu.settings"
                }
            ]
        }))
        .unwrap();
        let valid_parent: PluginManifest = serde_json::from_value(json!({
            "id": "dev.fbz.menu",
            "name": "Menu",
            "version": "1.0.0",
            "apiVersion": "1",
            "runtime": "wasi",
            "entrypoint": "plugin.wasm",
            "permissions": [
                { "key": "admin.menu" }
            ],
            "menu": [
                {
                    "key": "dev.fbz.menu.root",
                    "label": "Menu",
                    "path": "/admin/plugins/dev.fbz.menu"
                },
                {
                    "key": "dev.fbz.menu.settings",
                    "label": "Settings",
                    "path": "/admin/plugins/dev.fbz.menu/settings",
                    "parentKey": "dev.fbz.menu.root"
                }
            ]
        }))
        .unwrap();

        assert!(matches!(
            missing_parent.validate(),
            Err(PluginManifestError::InvalidField {
                field: "menu.parentKey",
                ..
            })
        ));
        assert!(matches!(
            self_parent.validate(),
            Err(PluginManifestError::InvalidField {
                field: "menu.parentKey",
                ..
            })
        ));
        assert!(valid_parent.validate().is_ok());
    }

    #[test]
    fn menu_keys_are_unique_and_required_permissions_must_be_declared() {
        let duplicate_key: PluginManifest = serde_json::from_value(json!({
            "id": "dev.fbz.menu",
            "name": "Menu",
            "version": "1.0.0",
            "apiVersion": "1",
            "runtime": "wasi",
            "entrypoint": "plugin.wasm",
            "permissions": [
                { "key": "admin.menu" }
            ],
            "menu": [
                {
                    "key": "dev.fbz.menu.settings",
                    "label": "Menu",
                    "path": "/admin/plugins/dev.fbz.menu/settings"
                },
                {
                    "key": "dev.fbz.menu.settings",
                    "label": "Menu 2",
                    "path": "/admin/plugins/dev.fbz.menu/settings-2"
                }
            ]
        }))
        .unwrap();
        let missing_permission: PluginManifest = serde_json::from_value(json!({
            "id": "dev.fbz.menu",
            "name": "Menu",
            "version": "1.0.0",
            "apiVersion": "1",
            "runtime": "wasi",
            "entrypoint": "plugin.wasm",
            "permissions": [
                { "key": "admin.menu" }
            ],
            "menu": [
                {
                    "key": "dev.fbz.menu.settings",
                    "label": "Menu",
                    "path": "/admin/plugins/dev.fbz.menu/settings",
                    "requiredPermission": "library.write"
                }
            ]
        }))
        .unwrap();

        assert!(matches!(
            duplicate_key.validate(),
            Err(PluginManifestError::InvalidField {
                field: "menu.key",
                ..
            })
        ));
        assert!(matches!(
            missing_permission.validate(),
            Err(PluginManifestError::InvalidField {
                field: "menu.requiredPermission",
                ..
            })
        ));
    }

    #[test]
    fn config_schema_accepts_supported_field_types() {
        let manifest: PluginManifest = serde_json::from_value(json!({
            "id": "dev.fbz.config",
            "name": "Config",
            "version": "1.0.0",
            "apiVersion": "1",
            "runtime": "http",
            "entrypoint": "https://plugins.example.test/hook",
            "configSchema": [
                {
                    "key": "endpoint",
                    "label": "Endpoint",
                    "type": "url",
                    "required": true,
                    "helpText": "Webhook endpoint"
                },
                {
                    "key": "mode",
                    "label": "Mode",
                    "type": "select",
                    "options": [
                        { "value": "safe", "label": "Safe" },
                        { "value": "fast", "label": "Fast" }
                    ]
                },
                {
                    "key": "enabled",
                    "label": "Enabled",
                    "type": "boolean"
                },
                {
                    "key": "api_token",
                    "label": "API Token",
                    "type": "secret"
                },
                {
                    "key": "password",
                    "label": "Password",
                    "type": "password"
                }
            ]
        }))
        .unwrap();

        let validated = manifest.validate().unwrap();

        assert_eq!(validated.manifest.config_schema.len(), 5);
    }

    #[test]
    fn config_schema_rejects_duplicate_keys_or_invalid_select_options() {
        let duplicate: PluginManifest = serde_json::from_value(json!({
            "id": "dev.fbz.config",
            "name": "Config",
            "version": "1.0.0",
            "apiVersion": "1",
            "runtime": "http",
            "entrypoint": "https://plugins.example.test/hook",
            "configSchema": [
                { "key": "endpoint", "label": "Endpoint", "type": "string" },
                { "key": "endpoint", "label": "Endpoint 2", "type": "string" }
            ]
        }))
        .unwrap();
        let bad_select: PluginManifest = serde_json::from_value(json!({
            "id": "dev.fbz.config",
            "name": "Config",
            "version": "1.0.0",
            "apiVersion": "1",
            "runtime": "http",
            "entrypoint": "https://plugins.example.test/hook",
            "configSchema": [
                { "key": "mode", "label": "Mode", "type": "select" }
            ]
        }))
        .unwrap();

        assert!(duplicate.validate().is_err());
        assert!(bad_select.validate().is_err());
    }

    #[test]
    fn schedule_keys_must_be_plugin_scoped() {
        let manifest: PluginManifest = serde_json::from_value(json!({
            "id": "dev.fbz.jobs",
            "name": "Jobs",
            "version": "1.0.0",
            "apiVersion": "1",
            "runtime": "wasi",
            "entrypoint": "plugin.wasm",
            "permissions": [
                { "key": "scheduler.register" }
            ],
            "schedules": [
                {
                    "key": "other.digest",
                    "scheduleKind": "interval",
                    "scheduleValue": "15m",
                    "handler": "jobs.digest"
                }
            ]
        }))
        .unwrap();

        assert!(matches!(
            manifest.validate(),
            Err(PluginManifestError::InvalidField {
                field: "schedules.key",
                ..
            })
        ));
    }

    #[test]
    fn duplicate_schedule_keys_are_rejected() {
        let manifest: PluginManifest = serde_json::from_value(json!({
            "id": "dev.fbz.schedule",
            "name": "Schedule",
            "version": "1.0.0",
            "apiVersion": "1",
            "runtime": "wasi",
            "entrypoint": "plugin.wasm",
            "permissions": [
                { "key": "scheduler.register" }
            ],
            "schedules": [
                {
                    "key": "dev.fbz.schedule.tick",
                    "scheduleKind": "interval",
                    "scheduleValue": "300",
                    "handler": "hooks.onTick"
                },
                {
                    "key": "dev.fbz.schedule.tick",
                    "scheduleKind": "interval",
                    "scheduleValue": "600",
                    "handler": "hooks.onOtherTick"
                }
            ]
        }))
        .unwrap();

        assert!(matches!(
            manifest.validate(),
            Err(PluginManifestError::InvalidField {
                field: "schedules.key",
                ..
            })
        ));
    }

    #[test]
    fn cron_schedules_are_validated() {
        let manifest: PluginManifest = serde_json::from_value(json!({
            "id": "dev.fbz.cron",
            "name": "Cron",
            "version": "1.0.0",
            "apiVersion": "1",
            "runtime": "wasi",
            "entrypoint": "plugin.wasm",
            "permissions": [
                { "key": "scheduler.register" }
            ],
            "schedules": [
                {
                    "key": "dev.fbz.cron.bad",
                    "scheduleKind": "cron",
                    "scheduleValue": "60 4 * * *",
                    "handler": "jobs.digest"
                }
            ]
        }))
        .unwrap();

        assert!(matches!(
            manifest.validate(),
            Err(PluginManifestError::InvalidField {
                field: "schedules.scheduleValue",
                ..
            })
        ));
    }

    #[test]
    fn unsupported_permissions_are_rejected() {
        let manifest: PluginManifest = serde_json::from_value(json!({
            "id": "dev.fbz.bad",
            "name": "Bad",
            "version": "1.0.0",
            "apiVersion": "1",
            "runtime": "wasi",
            "entrypoint": "plugin.wasm",
            "permissions": [
                { "key": "database.raw" }
            ]
        }))
        .unwrap();

        assert!(matches!(
            manifest.validate(),
            Err(PluginManifestError::UnsupportedPermission(permission))
                if permission == "database.raw"
        ));
    }

    #[test]
    fn media_download_hook_event_is_supported() {
        let manifest: PluginManifest = serde_json::from_value(json!({
            "id": "dev.fbz.audit",
            "name": "Audit",
            "version": "1.0.0",
            "apiVersion": "1",
            "runtime": "http",
            "entrypoint": "https://plugins.example.test/hook",
            "hooks": [
                { "event": "media.download.started", "handler": "hooks.onDownloadStarted" }
            ]
        }))
        .unwrap();

        let validated = manifest.validate().unwrap();

        assert_eq!(validated.manifest.hooks[0].event, "media.download.started");
    }

    #[test]
    fn duplicate_hook_event_handler_pairs_are_rejected() {
        let manifest: PluginManifest = serde_json::from_value(json!({
            "id": "dev.fbz.hooks",
            "name": "Hooks",
            "version": "1.0.0",
            "apiVersion": "1",
            "runtime": "wasi",
            "entrypoint": "plugin.wasm",
            "hooks": [
                { "event": "library.scan.completed", "handler": "hooks.onScanCompleted" },
                { "event": "library.scan.completed", "handler": "hooks.onScanCompleted" }
            ]
        }))
        .unwrap();

        assert!(matches!(
            manifest.validate(),
            Err(PluginManifestError::InvalidField {
                field: "hooks.handler",
                ..
            })
        ));
    }

    #[test]
    fn library_scan_hook_events_are_supported() {
        let manifest: PluginManifest = serde_json::from_value(json!({
            "id": "dev.fbz.scan",
            "name": "Scan Hooks",
            "version": "1.0.0",
            "apiVersion": "1",
            "runtime": "http",
            "entrypoint": "https://plugins.example.test/hook",
            "hooks": [
                { "event": "library.scan.started", "handler": "hooks.onScanStarted" },
                { "event": "library.scan.completed", "handler": "hooks.onScanCompleted" },
                { "event": "library.scan.failed", "handler": "hooks.onScanFailed" }
            ]
        }))
        .unwrap();

        let validated = manifest.validate().unwrap();

        assert_eq!(validated.manifest.hooks[0].event, "library.scan.started");
        assert_eq!(validated.manifest.hooks[1].event, "library.scan.completed");
        assert_eq!(validated.manifest.hooks[2].event, "library.scan.failed");
    }

    #[test]
    fn metadata_refresh_hook_events_are_supported() {
        let manifest: PluginManifest = serde_json::from_value(json!({
            "id": "dev.fbz.metadata",
            "name": "Metadata Hooks",
            "version": "1.0.0",
            "apiVersion": "1",
            "runtime": "http",
            "entrypoint": "https://plugins.example.test/hook",
            "hooks": [
                { "event": "metadata.refresh.completed", "handler": "hooks.onRefreshCompleted" },
                { "event": "metadata.refresh.failed", "handler": "hooks.onRefreshFailed" }
            ]
        }))
        .unwrap();

        let validated = manifest.validate().unwrap();

        assert_eq!(
            validated.manifest.hooks[0].event,
            "metadata.refresh.completed"
        );
        assert_eq!(validated.manifest.hooks[1].event, "metadata.refresh.failed");
    }

    #[test]
    fn transcode_hook_events_are_supported() {
        let manifest: PluginManifest = serde_json::from_value(json!({
            "id": "dev.fbz.transcode",
            "name": "Transcode Hooks",
            "version": "1.0.0",
            "apiVersion": "1",
            "runtime": "http",
            "entrypoint": "https://plugins.example.test/hook",
            "hooks": [
                { "event": "transcode.started", "handler": "hooks.onStarted" },
                { "event": "transcode.completed", "handler": "hooks.onCompleted" },
                { "event": "transcode.failed", "handler": "hooks.onFailed" }
            ]
        }))
        .unwrap();

        let validated = manifest.validate().unwrap();

        assert_eq!(validated.manifest.hooks[0].event, "transcode.started");
        assert_eq!(validated.manifest.hooks[1].event, "transcode.completed");
        assert_eq!(validated.manifest.hooks[2].event, "transcode.failed");
    }

    #[test]
    fn hook_event_database_constraint_tracks_supported_events() {
        // Migration 0078 re-defines the allow-list constraint with the current
        // full event set (0019 was the original; later migrations extend it).
        let migration = include_str!("../../migrations/0078_metadata_provider_query_hook.sql");

        assert!(migration.contains("plugin_hooks_event_key_allowed"));
        for event in supported_plugin_hook_events() {
            let quoted_event = format!("'{event}'");
            assert!(
                migration.contains(&quoted_event),
                "migration is missing hook event {event}"
            );
        }
    }

    // Live-DB smoke: validates migration 0078's dynamic drop+re-add of the
    // plugin_hooks event_key allow-list against the real migrated schema. The
    // migration finds the prior constraint by signature and re-adds it with
    // `metadata.provider.query` included; a static string check cannot prove the
    // dynamic DO block actually landed the new constraint. This inserts a hook
    // at the new event (must be accepted) and a hook at a bogus event (must be
    // rejected by the DB CHECK), then cleans up via the package cascade.
    //   cargo test -- --ignored hook_event_constraint_executes_against_live_schema
    #[tokio::test]
    #[ignore = "requires a running PostgreSQL from ./scripts/dev-deps.ps1"]
    async fn hook_event_constraint_executes_against_live_schema() {
        use sqlx::postgres::PgPoolOptions;

        let url = std::env::var("DATABASE_URL")
            .unwrap_or_else(|_| "postgres://fbz:fbz@127.0.0.1:5432/fbz".to_owned());
        let pool = PgPoolOptions::new()
            .max_connections(2)
            .connect(&url)
            .await
            .expect("connect to live PostgreSQL");
        crate::db::migrate(&pool).await.expect("run migrations");

        // Insert a minimal, distinctly-named package so cleanup never touches
        // real plugin rows. The hook FK cascades on package delete.
        let package_id: i64 = sqlx::query_scalar(
            r#"
            insert into plugin_packages (
                plugin_id, package_version, api_version, runtime, name,
                entrypoint, package_path, manifest, manifest_hash,
                permission_fingerprint, package_status
            )
            values (
                'dev.fbz.metadata-hook-smoke', '0.0.0-smoke', '1', 'http',
                'Metadata Hook Smoke', 'server.mjs', '/tmp/metadata-hook-smoke',
                '{}'::jsonb, decode(repeat('00', 32), 'hex'),
                decode(repeat('00', 32), 'hex'), 'pending_approval'
            )
            on conflict (plugin_id, package_version) do update
                set updated_at = now()
            returning id
            "#,
        )
        .fetch_one(&pool)
        .await
        .expect("insert smoke plugin package");

        // The new event must satisfy the re-added 0078 constraint.
        sqlx::query(
            r#"
            insert into plugin_hooks (package_id, event_key, handler)
            values ($1, 'metadata.provider.query', 'onQuery')
            on conflict (package_id, event_key, handler) do nothing
            "#,
        )
        .bind(package_id)
        .execute(&pool)
        .await
        .expect("the metadata.provider.query event must satisfy the live constraint");

        // A bogus event must still be rejected by the DB CHECK constraint.
        let bogus = sqlx::query(
            r#"
            insert into plugin_hooks (package_id, event_key, handler)
            values ($1, 'metadata.provider.bogus', 'onBogus')
            "#,
        )
        .bind(package_id)
        .execute(&pool)
        .await;
        assert!(
            bogus.is_err(),
            "an unknown event_key must be rejected by plugin_hooks_event_key_allowed"
        );

        // Cleanup: deleting the package cascades to its hooks.
        sqlx::query("delete from plugin_packages where id = $1")
            .bind(package_id)
            .execute(&pool)
            .await
            .expect("cleanup smoke plugin package");
    }

    #[test]
    fn permission_database_index_treats_null_scope_as_duplicate() {
        let migration = include_str!("../../migrations/0028_plugin_permission_uniqueness.sql");

        assert!(migration.contains("coalesce(permission_scope, '')"));
        assert!(migration.contains("idx_plugin_permissions_package_key_scope_normalized"));
    }

    #[test]
    fn wasi_entrypoint_cannot_escape_package() {
        let manifest: PluginManifest = serde_json::from_value(json!({
            "id": "dev.fbz.escape",
            "name": "Escape",
            "version": "1.0.0",
            "apiVersion": "1",
            "runtime": "wasi",
            "entrypoint": "../plugin.wasm"
        }))
        .unwrap();

        assert!(matches!(
            manifest.validate(),
            Err(PluginManifestError::InvalidField {
                field: "entrypoint",
                ..
            })
        ));
    }
}
