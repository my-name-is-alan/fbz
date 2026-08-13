//! 插件同步调用路径（provider 契约的执行地基）。
//!
//! 与异步 outbox 派发（`execution.rs` + `worker.rs`）不同，同步调用由核心服务在
//! 作业/请求上下文内直接调用已启用插件并等待响应，用于元数据刮削 provider、
//! 网盘 storage provider、弹幕 provider 等「必须拿到返回值」的扩展点。
//!
//! v1 边界：
//! - 仅 `http` runtime。WASIp1 无网络能力且模块冷启动成本高，不适合请求内查询；
//!   订阅了同步能力事件的 WASI 插件会在发现阶段被跳过。
//! - 复用异步派发同一套安全原语：entrypoint 校验、`PLUGIN_HTTP_ALLOWED_HOSTS`
//!   allowlist、HMAC 签名头、响应体大小上限（见 `execution.rs` 的 pub(crate) 原语）。
//! - 不签发 Host API token：同步 provider 从请求体拿到全部输入、直接返回结果，
//!   不回调宿主；响应由调用方按对应能力的字段白名单校验后才允许落库。
//! - per-plugin 并发预算（信号量）+ 连续失败熔断（阈值 + 冷却窗口），保证慢插件
//!   或宕机插件不会拖死刮削/扫描作业。
//! - 失败必审计、成功按调用方要求审计（`plugin_sync_invocations`，迁移 0096），
//!   高频 provider 查询不强制逐次写库。

use std::{
    collections::HashMap,
    error::Error,
    fmt::{Display, Formatter},
    sync::{
        Arc, Mutex,
        atomic::{AtomicU64, Ordering},
    },
    time::{Duration, Instant},
};

use reqwest::{
    Client,
    header::{CONTENT_TYPE, USER_AGENT},
};
use serde_json::{Value, json};
use sqlx::Row;
use tokio::sync::Semaphore;
use tracing::warn;

use crate::{
    config::PluginConfig,
    db::DbPool,
    plugins::execution::{
        ensure_http_host_allowed, plugin_http_signature_headers, read_limited_response_body,
        reqwest_runtime_error, unix_timestamp_seconds, validate_http_entrypoint,
    },
};

/// 同步调用协议标识，随请求体和 `x-fbz-plugin-invocation` 头下发给插件。
pub const PLUGIN_SYNC_INVOCATION_KIND: &str = "sync";
/// v1 仅支持 HTTP runtime 的同步调用。
pub const SYNC_INVOKE_SUPPORTED_RUNTIME: &str = "http";

const MAX_ERROR_BYTES: usize = 2048;

/// 同步调用审计粒度。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SyncAuditMode {
    /// 只审计失败（默认，适合高频 provider 查询）。
    FailuresOnly,
    /// 成功与失败都审计（适合低频、管理员触发的调用）。
    All,
}

/// 一个订阅了同步能力事件的可调用插件目标。
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PluginSyncSubscriber {
    pub plugin_id: String,
    pub package_id: String,
    pub hook_id: i64,
    pub handler: String,
    pub entrypoint: String,
    pub runtime: String,
}

/// 单次同步调用的结果（含目标身份，便于调用方合并多插件响应）。
#[derive(Debug)]
pub struct PluginSyncOutcome {
    pub plugin_id: String,
    pub handler: String,
    pub duration: Duration,
    pub result: Result<Value, PluginSyncError>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PluginSyncError {
    Database(String),
    /// 熔断打开：最近连续失败超过阈值，冷却窗口内直接拒绝。
    CircuitOpen {
        plugin_id: String,
    },
    /// 并发预算耗尽且等待超时。
    Busy {
        plugin_id: String,
    },
    UnsupportedRuntime(String),
    Runtime(String),
    /// 插件返回了无法解析为 JSON 的响应体。
    InvalidResponse(String),
}

/// 熔断器纯状态：跟踪连续失败次数与打开截止时间。
/// 转移逻辑是纯函数（`note_success` / `note_failure` / `is_open`），可穷举单测。
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct CircuitState {
    consecutive_failures: u32,
    open_until: Option<Instant>,
}

impl CircuitState {
    pub(crate) fn is_open(&self, now: Instant) -> bool {
        self.open_until.is_some_and(|until| now < until)
    }

    pub(crate) fn note_success(&mut self) {
        self.consecutive_failures = 0;
        self.open_until = None;
    }

    pub(crate) fn note_failure(&mut self, now: Instant, threshold: u32, cooldown: Duration) {
        self.consecutive_failures = self.consecutive_failures.saturating_add(1);
        if self.consecutive_failures >= threshold.max(1) {
            self.open_until = Some(now + cooldown);
            // 打开后清零计数：冷却结束后的首个失败重新从 1 开始累计，
            // 避免半开状态下单次失败立即再次全额熔断循环。
            self.consecutive_failures = 0;
        }
    }
}

struct PluginSyncEntry {
    semaphore: Arc<Semaphore>,
    circuit: CircuitState,
}

#[derive(Clone)]
pub struct PluginSyncInvoker {
    pool: DbPool,
    config: PluginConfig,
    client: Client,
    state: Arc<Mutex<HashMap<String, PluginSyncEntry>>>,
    invocation_counter: Arc<AtomicU64>,
}

impl PluginSyncInvoker {
    pub fn new(pool: DbPool, config: PluginConfig) -> Self {
        Self {
            pool,
            config,
            client: Client::new(),
            state: Arc::new(Mutex::new(HashMap::new())),
            invocation_counter: Arc::new(AtomicU64::new(0)),
        }
    }

    /// 发现订阅了某个同步能力事件（hook event key）的可调用插件。
    ///
    /// 与异步 hook 发现同一套启用/审批边界；额外要求 runtime 为 `http`，
    /// 其余 runtime 的订阅在同步路径上不可执行，直接过滤。
    pub async fn list_subscribers(
        &self,
        capability: &str,
    ) -> Result<Vec<PluginSyncSubscriber>, PluginSyncError> {
        let rows = sqlx::query(SYNC_SUBSCRIBER_DISCOVERY_SQL)
            .bind(capability)
            .bind(SYNC_INVOKE_SUPPORTED_RUNTIME)
            .fetch_all(&self.pool)
            .await
            .map_err(|err| PluginSyncError::Database(err.to_string()))?;

        rows.into_iter()
            .map(|row| -> Result<PluginSyncSubscriber, sqlx::Error> {
                Ok(PluginSyncSubscriber {
                    plugin_id: row.try_get("plugin_id")?,
                    package_id: row.try_get("package_id")?,
                    hook_id: row.try_get("hook_id")?,
                    handler: row.try_get("handler")?,
                    entrypoint: row.try_get("entrypoint")?,
                    runtime: row.try_get("runtime")?,
                })
            })
            .collect::<Result<Vec<_>, _>>()
            .map_err(|err| PluginSyncError::Database(err.to_string()))
    }

    /// 依优先级顺序同步调用某能力的全部订阅插件，逐个返回结果。
    ///
    /// 顺序执行是有意的：provider 链语义（如元数据 BaseMatch）依赖 hook
    /// priority 顺序，且单插件失败不阻断后续插件。
    pub async fn invoke_all(
        &self,
        capability: &str,
        payload: &Value,
        audit: SyncAuditMode,
    ) -> Result<Vec<PluginSyncOutcome>, PluginSyncError> {
        let subscribers = self.list_subscribers(capability).await?;
        let mut outcomes = Vec::with_capacity(subscribers.len());
        for subscriber in &subscribers {
            outcomes.push(self.invoke(subscriber, capability, payload, audit).await);
        }
        Ok(outcomes)
    }

    /// 同步调用单个插件目标并等待 JSON 响应。
    pub async fn invoke(
        &self,
        subscriber: &PluginSyncSubscriber,
        capability: &str,
        payload: &Value,
        audit: SyncAuditMode,
    ) -> PluginSyncOutcome {
        let started = Instant::now();
        let result = self.invoke_inner(subscriber, capability, payload).await;
        let duration = started.elapsed();

        let should_audit = matches!(audit, SyncAuditMode::All) || result.is_err();
        if should_audit {
            self.audit_invocation(subscriber, capability, duration, &result)
                .await;
        }

        if let Err(err) = &result {
            warn!(
                plugin_id = %subscriber.plugin_id,
                capability,
                handler = %subscriber.handler,
                duration_ms = duration.as_millis() as u64,
                error = %err,
                "plugin sync invocation failed"
            );
        }

        PluginSyncOutcome {
            plugin_id: subscriber.plugin_id.clone(),
            handler: subscriber.handler.clone(),
            duration,
            result,
        }
    }

    async fn invoke_inner(
        &self,
        subscriber: &PluginSyncSubscriber,
        capability: &str,
        payload: &Value,
    ) -> Result<Value, PluginSyncError> {
        if subscriber.runtime.trim() != SYNC_INVOKE_SUPPORTED_RUNTIME {
            return Err(PluginSyncError::UnsupportedRuntime(
                subscriber.runtime.trim().to_owned(),
            ));
        }

        // 熔断检查（不发起任何 IO）。
        if self.circuit_is_open(&subscriber.plugin_id) {
            return Err(PluginSyncError::CircuitOpen {
                plugin_id: subscriber.plugin_id.clone(),
            });
        }

        // per-plugin 并发预算：等待窗口计入总调用预算，超时视为 Busy。
        let semaphore = self.semaphore_for(&subscriber.plugin_id);
        let timeout = Duration::from_millis(self.config.sync_timeout_ms);
        let permit = match tokio::time::timeout(timeout, semaphore.acquire_owned()).await {
            Ok(Ok(permit)) => permit,
            Ok(Err(_)) | Err(_) => {
                return Err(PluginSyncError::Busy {
                    plugin_id: subscriber.plugin_id.clone(),
                });
            }
        };

        let result = self
            .execute_http_sync(subscriber, capability, payload, timeout)
            .await;
        drop(permit);

        match &result {
            Ok(_) => self.record_success(&subscriber.plugin_id),
            // Busy/CircuitOpen 不计失败：它们是宿主侧保护，不代表插件不健康。
            Err(PluginSyncError::Busy { .. }) | Err(PluginSyncError::CircuitOpen { .. }) => {}
            Err(_) => self.record_failure(&subscriber.plugin_id),
        }

        result
    }

    async fn execute_http_sync(
        &self,
        subscriber: &PluginSyncSubscriber,
        capability: &str,
        payload: &Value,
        timeout: Duration,
    ) -> Result<Value, PluginSyncError> {
        let uri = validate_http_entrypoint(&subscriber.entrypoint)
            .map_err(|err| PluginSyncError::Runtime(err.to_string()))?;
        ensure_http_host_allowed(&uri, &self.config.http_allowed_hosts)
            .map_err(|err| PluginSyncError::Runtime(err.to_string()))?;

        let invocation_key = self.next_invocation_key();
        let body_value = sync_request_body(subscriber, capability, payload);
        let body = serde_json::to_vec(&body_value)
            .map_err(|err| PluginSyncError::Runtime(err.to_string()))?;

        let mut request = self
            .client
            .post(subscriber.entrypoint.trim())
            .timeout(timeout)
            .header(CONTENT_TYPE, "application/json")
            .header(USER_AGENT, "fbz-api-plugin-sync/0.1")
            .header("x-fbz-plugin-id", subscriber.plugin_id.as_str())
            .header("x-fbz-plugin-invocation", PLUGIN_SYNC_INVOCATION_KIND)
            .header("x-fbz-plugin-idempotency-key", invocation_key.as_str());
        if let Some(secret) = self
            .config
            .secret_key
            .as_deref()
            .filter(|value| !value.trim().is_empty())
        {
            let signature = plugin_http_signature_headers(
                secret,
                &subscriber.plugin_id,
                &invocation_key,
                unix_timestamp_seconds(),
                &body,
            );
            request = request
                .header("x-fbz-plugin-signature-version", signature.version)
                .header("x-fbz-plugin-signature-timestamp", signature.timestamp)
                .header("x-fbz-plugin-body-sha256", signature.body_sha256)
                .header("x-fbz-plugin-signature", signature.signature);
        }

        let response = request
            .body(body)
            .send()
            .await
            .map_err(|err| PluginSyncError::Runtime(reqwest_runtime_error(err).to_string()))?;
        let status = response.status();
        let response_body =
            read_limited_response_body(response, self.config.http_max_response_body_bytes)
                .await
                .map_err(|err| PluginSyncError::Runtime(err.to_string()))?;

        if !status.is_success() {
            let text = String::from_utf8_lossy(&response_body);
            return Err(PluginSyncError::Runtime(format!(
                "plugin sync endpoint returned {}: {}",
                status.as_u16(),
                truncate_str(&text, MAX_ERROR_BYTES)
            )));
        }

        serde_json::from_slice::<Value>(&response_body)
            .map_err(|err| PluginSyncError::InvalidResponse(err.to_string()))
    }

    async fn audit_invocation(
        &self,
        subscriber: &PluginSyncSubscriber,
        capability: &str,
        duration: Duration,
        result: &Result<Value, PluginSyncError>,
    ) {
        let (status, error_message) = match result {
            Ok(_) => ("succeeded", None),
            Err(err) => (
                "failed",
                Some(truncate_str(&err.to_string(), MAX_ERROR_BYTES)),
            ),
        };

        let inserted = sqlx::query(
            r#"
            insert into plugin_sync_invocations (
                plugin_id,
                package_id,
                capability,
                handler,
                entrypoint,
                status,
                error_message,
                duration_ms
            )
            values ($1, $2, $3, $4, $5, $6, $7, $8)
            "#,
        )
        .bind(subscriber.plugin_id.trim())
        .bind(subscriber.package_id.trim())
        .bind(capability.trim())
        .bind(subscriber.handler.trim())
        .bind(subscriber.entrypoint.trim())
        .bind(status)
        .bind(error_message.as_deref())
        .bind(duration.as_millis().min(i32::MAX as u128) as i32)
        .execute(&self.pool)
        .await;

        if let Err(err) = inserted {
            warn!(
                plugin_id = %subscriber.plugin_id,
                capability,
                error = %err,
                "failed to audit plugin sync invocation"
            );
        }
    }

    fn semaphore_for(&self, plugin_id: &str) -> Arc<Semaphore> {
        let mut state = self.state.lock().expect("plugin sync state lock poisoned");
        state
            .entry(plugin_id.to_owned())
            .or_insert_with(|| PluginSyncEntry {
                semaphore: Arc::new(Semaphore::new(
                    self.config.sync_max_concurrency_per_plugin.max(1) as usize,
                )),
                circuit: CircuitState::default(),
            })
            .semaphore
            .clone()
    }

    fn circuit_is_open(&self, plugin_id: &str) -> bool {
        let state = self.state.lock().expect("plugin sync state lock poisoned");
        state
            .get(plugin_id)
            .is_some_and(|entry| entry.circuit.is_open(Instant::now()))
    }

    fn record_success(&self, plugin_id: &str) {
        let mut state = self.state.lock().expect("plugin sync state lock poisoned");
        if let Some(entry) = state.get_mut(plugin_id) {
            entry.circuit.note_success();
        }
    }

    fn record_failure(&self, plugin_id: &str) {
        let threshold = self.config.sync_circuit_failure_threshold;
        let cooldown = Duration::from_secs(self.config.sync_circuit_cooldown_seconds);
        let mut state = self.state.lock().expect("plugin sync state lock poisoned");
        if let Some(entry) = state.get_mut(plugin_id) {
            let was_open = entry.circuit.is_open(Instant::now());
            entry
                .circuit
                .note_failure(Instant::now(), threshold, cooldown);
            if !was_open && entry.circuit.is_open(Instant::now()) {
                warn!(
                    plugin_id,
                    threshold,
                    cooldown_seconds = cooldown.as_secs(),
                    "plugin sync circuit opened after consecutive failures"
                );
            }
        }
    }

    fn next_invocation_key(&self) -> String {
        let count = self.invocation_counter.fetch_add(1, Ordering::Relaxed);
        format!("sync-{}-{}", unix_timestamp_seconds(), count)
    }
}

/// 同步调用请求体：插件按 `invocation == "sync"` 区分同步查询与异步 hook 派发。
fn sync_request_body(
    subscriber: &PluginSyncSubscriber,
    capability: &str,
    payload: &Value,
) -> Value {
    json!({
        "invocation": PLUGIN_SYNC_INVOCATION_KIND,
        "pluginId": subscriber.plugin_id,
        "packageId": subscriber.package_id,
        "hookId": subscriber.hook_id,
        "handler": subscriber.handler,
        "hookEvent": capability,
        "request": payload,
    })
}

/// 订阅发现 SQL：与异步 hook 发现同一套启用/审批边界 + runtime 过滤。
const SYNC_SUBSCRIBER_DISCOVERY_SQL: &str = r#"
    select
        pi.plugin_id,
        pkg.public_id::text as package_id,
        h.id as hook_id,
        h.handler,
        pkg.entrypoint,
        pkg.runtime
    from plugin_hooks h
    join plugin_packages pkg on pkg.id = h.package_id
    join plugin_installations pi on pi.active_package_id = pkg.id
    where h.event_key = $1
      and h.enabled = true
      and pi.enabled = true
      and pi.approval_status = 'approved'
      and pkg.package_status = 'approved'
      and pkg.runtime = $2
    order by h.priority desc, h.id asc
    "#;

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

impl Display for PluginSyncError {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Database(err) => write!(f, "database error: {err}"),
            Self::CircuitOpen { plugin_id } => {
                write!(
                    f,
                    "plugin `{plugin_id}` sync circuit is open; skipping call"
                )
            }
            Self::Busy { plugin_id } => {
                write!(f, "plugin `{plugin_id}` sync concurrency budget exhausted")
            }
            Self::UnsupportedRuntime(runtime) => {
                write!(
                    f,
                    "plugin runtime `{runtime}` does not support sync invocation"
                )
            }
            Self::Runtime(err) => f.write_str(err),
            Self::InvalidResponse(err) => {
                write!(f, "plugin sync response is not valid JSON: {err}")
            }
        }
    }
}

impl Error for PluginSyncError {}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    fn subscriber() -> PluginSyncSubscriber {
        PluginSyncSubscriber {
            plugin_id: "dev.fbz.scraper".to_owned(),
            package_id: "package-1".to_owned(),
            hook_id: 7,
            handler: "providers.match".to_owned(),
            entrypoint: "https://plugins.internal/scraper".to_owned(),
            runtime: "http".to_owned(),
        }
    }

    #[test]
    fn sync_request_body_marks_invocation_kind_and_capability() {
        let body = sync_request_body(
            &subscriber(),
            "metadata.provider.query",
            &json!({"title": "Dune"}),
        );

        assert_eq!(body["invocation"], "sync");
        assert_eq!(body["pluginId"], "dev.fbz.scraper");
        assert_eq!(body["hookEvent"], "metadata.provider.query");
        assert_eq!(body["handler"], "providers.match");
        assert_eq!(body["request"]["title"], "Dune");
    }

    #[test]
    fn circuit_stays_closed_below_threshold() {
        let mut circuit = CircuitState::default();
        let now = Instant::now();

        circuit.note_failure(now, 3, Duration::from_secs(60));
        circuit.note_failure(now, 3, Duration::from_secs(60));

        assert!(!circuit.is_open(now));
    }

    #[test]
    fn circuit_opens_at_threshold_and_closes_after_cooldown() {
        let mut circuit = CircuitState::default();
        let now = Instant::now();

        for _ in 0..3 {
            circuit.note_failure(now, 3, Duration::from_secs(60));
        }

        assert!(circuit.is_open(now));
        assert!(circuit.is_open(now + Duration::from_secs(59)));
        assert!(!circuit.is_open(now + Duration::from_secs(61)));
    }

    #[test]
    fn circuit_success_resets_failure_streak() {
        let mut circuit = CircuitState::default();
        let now = Instant::now();

        circuit.note_failure(now, 3, Duration::from_secs(60));
        circuit.note_failure(now, 3, Duration::from_secs(60));
        circuit.note_success();
        circuit.note_failure(now, 3, Duration::from_secs(60));

        assert!(!circuit.is_open(now));
    }

    #[test]
    fn circuit_reopen_requires_full_streak_after_cooldown() {
        let mut circuit = CircuitState::default();
        let now = Instant::now();

        for _ in 0..3 {
            circuit.note_failure(now, 3, Duration::from_secs(1));
        }
        assert!(circuit.is_open(now));

        // 冷却结束后的单次失败不应立即再次熔断（计数已在打开时清零）。
        let later = now + Duration::from_secs(2);
        circuit.note_failure(later, 3, Duration::from_secs(1));
        assert!(!circuit.is_open(later));
    }

    #[test]
    fn circuit_threshold_is_clamped_to_at_least_one() {
        let mut circuit = CircuitState::default();
        let now = Instant::now();

        circuit.note_failure(now, 0, Duration::from_secs(60));

        assert!(circuit.is_open(now));
    }

    #[test]
    fn subscriber_discovery_sql_enforces_enablement_and_runtime_boundary() {
        let normalized = SYNC_SUBSCRIBER_DISCOVERY_SQL
            .split_whitespace()
            .collect::<Vec<_>>()
            .join(" ");

        assert!(normalized.contains("from plugin_hooks h"));
        assert!(normalized.contains("join plugin_packages pkg on pkg.id = h.package_id"));
        assert!(
            normalized.contains("join plugin_installations pi on pi.active_package_id = pkg.id")
        );
        assert!(normalized.contains("h.event_key = $1"));
        assert!(normalized.contains("h.enabled = true"));
        assert!(normalized.contains("pi.enabled = true"));
        assert!(normalized.contains("pi.approval_status = 'approved'"));
        assert!(normalized.contains("pkg.package_status = 'approved'"));
        assert!(normalized.contains("pkg.runtime = $2"));
        assert!(normalized.contains("order by h.priority desc, h.id asc"));
    }

    #[test]
    fn sync_invocation_audit_migration_shape_matches_insert() {
        let migration = include_str!("../../migrations/0096_plugin_sync_invocations.sql");

        for column in [
            "plugin_id",
            "package_id",
            "capability",
            "handler",
            "entrypoint",
            "status",
            "response_status",
            "error_message",
            "duration_ms",
        ] {
            assert!(
                migration.contains(column),
                "migration should define column {column}"
            );
        }
        assert!(migration.contains("check (status in ('succeeded', 'failed'))"));
        assert!(migration.contains("idx_plugin_sync_invocations_recent"));
        assert!(migration.contains("idx_plugin_sync_invocations_plugin_recent"));
        assert!(migration.contains("idx_plugin_sync_invocations_capability_recent"));
    }

    #[test]
    fn truncation_preserves_utf8_boundary() {
        assert_eq!(truncate_str("同步调用", 7), "同步");
    }

    // Live-DB smoke: validates the subscriber discovery SQL and the audit
    // insert execute against the migrated schema (the insert is rolled back).
    //   cargo test -- --ignored plugin_sync_invocation_sql_executes_against_live_schema
    #[tokio::test]
    #[ignore = "requires a running PostgreSQL from ./scripts/dev-deps.ps1"]
    async fn plugin_sync_invocation_sql_executes_against_live_schema() {
        use sqlx::postgres::PgPoolOptions;

        let url = std::env::var("DATABASE_URL")
            .unwrap_or_else(|_| "postgres://fbz:fbz@127.0.0.1:5432/fbz".to_owned());
        let pool = PgPoolOptions::new()
            .max_connections(2)
            .connect(&url)
            .await
            .expect("connect to live PostgreSQL");
        crate::db::migrate(&pool).await.expect("run migrations");

        // 订阅发现 SQL 必须能在真实 schema 上解析执行（无订阅时返回空集）。
        let rows = sqlx::query(SYNC_SUBSCRIBER_DISCOVERY_SQL)
            .bind("metadata.provider.query")
            .bind(SYNC_INVOKE_SUPPORTED_RUNTIME)
            .fetch_all(&pool)
            .await
            .expect("subscriber discovery SQL should execute");
        let _ = rows;

        // 审计 insert 走事务并回滚，不污染 dev DB。
        let mut tx = pool.begin().await.expect("begin tx");
        sqlx::query(
            r#"
            insert into plugin_sync_invocations (
                plugin_id, package_id, capability, handler, entrypoint,
                status, error_message, duration_ms
            )
            values ($1, $2, $3, $4, $5, $6, $7, $8)
            "#,
        )
        .bind("dev.fbz.sync-smoke")
        .bind("package-smoke")
        .bind("metadata.provider.query")
        .bind("providers.match")
        .bind("https://plugins.internal/smoke")
        .bind("failed")
        .bind(Some("smoke"))
        .bind(12_i32)
        .execute(&mut *tx)
        .await
        .expect("audit insert should execute against live schema");
        tx.rollback().await.expect("rollback");
    }
}
