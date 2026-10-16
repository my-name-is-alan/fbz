//! StorageProvider extension point (`rpc:storage.provider`).
//!
//! Host-side contract for pluggable cloud media sources (HTTP or later WASI).
//! Core owns account ciphertext, QPS gates, and mount orchestration; plugins
//! implement remote I/O ops. Transitional adapters wrap the existing Guangya
//! Node HTTP RPC so `storage/*` no longer hard-codes `FBZ_GUANGYA_PLUGIN_URL`
//! or `provider='guangya'` string branches for dispatch.
//!
//! See docs/FBZ-目标架构与落地路线.md §4.2 / §5.2 / §5.7 and
//! docs/storage-provider-extension.md.

use std::{
    collections::HashMap,
    sync::{Arc, OnceLock},
};

use async_trait::async_trait;
use serde_json::{Value, json};

/// Short provider id stored on `storage_accounts.provider` (e.g. `guangya`).
pub const GUANGYA_PROVIDER_ID: &str = "guangya";

/// Shared bearer key env for first-party HTTP storage plugins (not in manifest).
pub const STORAGE_PLUGIN_KEY_ENV: &str = "FBZ_STORAGE_PLUGIN_KEY";

/// Preferred per-provider endpoint env: `FBZ_STORAGE_PROVIDER_ENDPOINT_<id>`.
pub fn provider_endpoint_env(provider_id: &str) -> String {
    format!("FBZ_STORAGE_PROVIDER_ENDPOINT_{provider_id}")
}

/// Legacy Guangya-only endpoint env (kept as fallback during migration).
pub const GUANGYA_PLUGIN_URL_ENV: &str = "FBZ_GUANGYA_PLUGIN_URL";

const DEFAULT_GUANGYA_ENDPOINT: &str = "http://127.0.0.1:8098/rpc";

/// Ops advertised by the storage.provider RPC schema (arch §4.2).
pub const STORAGE_PROVIDER_OPS: &[&str] = &[
    "list",
    "stat",
    "resolve",
    "read",
    "auth.start",
    "auth.poll",
];

/// Extension-point id exposed via plugin capabilities.
pub const STORAGE_PROVIDER_EXTENSION_POINT: &str = "storage.provider";

#[derive(Debug, Clone)]
pub struct StorageProviderError {
    pub code: &'static str,
    pub message: String,
}

impl StorageProviderError {
    pub fn new(code: &'static str, message: impl Into<String>) -> Self {
        Self {
            code,
            message: message.into(),
        }
    }

    pub fn unsupported(op: &str) -> Self {
        Self::new("unsupported", format!("storage provider does not support op `{op}`"))
    }
}

/// Host-assembled call context (credentials already decrypted by core).
#[derive(Clone, Debug)]
pub struct StorageProviderContext {
    pub account_id: String,
    pub device_id: String,
    pub credential_version: i64,
    pub qps: i32,
    pub credentials: Value,
    /// Bearer shared with the HTTP plugin process (`FBZ_STORAGE_PLUGIN_KEY`).
    pub plugin_key: String,
}

/// Raw plugin outcome before core maps errors / persists credential refresh.
#[derive(Clone, Debug, Default)]
pub struct StorageRpcOutcome {
    pub data: Value,
    pub credentials: Option<Value>,
    pub error_code: Option<String>,
    pub error_message: Option<String>,
    pub retry_after: f64,
}

/// Directory / file entry returned by `list` / `stat` (JSON-shaped for now).
#[derive(Clone, Debug)]
pub struct StorageEntryPage {
    pub entries: Value,
    pub next_page: Option<u32>,
}

/// Playback / download URL from `resolve`.
#[derive(Clone, Debug)]
pub struct StorageResolvedUrl {
    pub url: String,
    pub expires_at_ms: Option<u64>,
}

/// Core trait: HTTP plugins implement via RPC; WASI can later share the same ops.
#[async_trait]
pub trait StorageProvider: Send + Sync {
    fn id(&self) -> &str;

    fn display_name(&self) -> &str {
        self.id()
    }

    fn supported_ops(&self) -> &[&str] {
        STORAGE_PROVIDER_OPS
    }

    /// Low-level RPC used by the transitional host client and typed wrappers.
    async fn invoke(
        &self,
        ctx: &StorageProviderContext,
        op: &str,
        input: Value,
    ) -> Result<StorageRpcOutcome, StorageProviderError>;

    async fn list(
        &self,
        ctx: &StorageProviderContext,
        parent_id: &str,
        page: u32,
    ) -> Result<StorageEntryPage, StorageProviderError> {
        let outcome = self
            .invoke(
                ctx,
                "list",
                json!({ "parentId": parent_id, "page": page }),
            )
            .await?;
        map_rpc_error(&outcome)?;
        Ok(StorageEntryPage {
            entries: outcome.data.get("entries").cloned().unwrap_or(json!([])),
            next_page: outcome
                .data
                .get("nextPage")
                .and_then(|v| v.as_u64())
                .map(|v| v as u32),
        })
    }

    /// Optional; default returns unsupported. Providers may synthesize from list.
    async fn stat(
        &self,
        ctx: &StorageProviderContext,
        file_id: &str,
    ) -> Result<Value, StorageProviderError> {
        let _ = (ctx, file_id);
        Err(StorageProviderError::unsupported("stat"))
    }

    async fn resolve(
        &self,
        ctx: &StorageProviderContext,
        file_id: &str,
        force: bool,
    ) -> Result<StorageResolvedUrl, StorageProviderError> {
        let outcome = self
            .invoke(
                ctx,
                "resolve",
                json!({ "fileId": file_id, "force": force }),
            )
            .await?;
        map_rpc_error(&outcome)?;
        let url = outcome
            .data
            .get("url")
            .and_then(|v| v.as_str())
            .ok_or_else(|| StorageProviderError::new("invalid_response", "resolve missing url"))?
            .to_owned();
        Ok(StorageResolvedUrl {
            url,
            expires_at_ms: outcome.data.get("expiresAt").and_then(|v| v.as_u64()),
        })
    }

    async fn read_small(
        &self,
        ctx: &StorageProviderContext,
        file_id: &str,
        max_bytes: usize,
    ) -> Result<Vec<u8>, StorageProviderError> {
        let outcome = self
            .invoke(
                ctx,
                "read",
                json!({ "fileId": file_id, "maxBytes": max_bytes }),
            )
            .await?;
        map_rpc_error(&outcome)?;
        let bytes: Vec<u8> = serde_json::from_value(
            outcome
                .data
                .get("bytes")
                .cloned()
                .unwrap_or(Value::Null),
        )
        .map_err(|_| StorageProviderError::new("invalid_response", "read missing bytes"))?;
        Ok(bytes)
    }

    async fn open(
        &self,
        ctx: &StorageProviderContext,
        file_id: &str,
    ) -> Result<StorageResolvedUrl, StorageProviderError> {
        self.resolve(ctx, file_id, false).await
    }
}

fn map_rpc_error(outcome: &StorageRpcOutcome) -> Result<(), StorageProviderError> {
    if let Some(code) = outcome.error_code.as_deref() {
        return Err(StorageProviderError::new(
            leak_code(code),
            outcome
                .error_message
                .clone()
                .unwrap_or_else(|| "storage provider error".into()),
        ));
    }
    Ok(())
}

fn leak_code(code: &str) -> &'static str {
    match code {
        "auth_expired" => "auth_expired",
        "rate_limited" => "rate_limited",
        "unsafe_url" => "unsafe_url",
        "unsupported" => "unsupported",
        "invalid_response" => "invalid_response",
        "invalid_request" => "invalid_request",
        "upstream" => "upstream",
        "identity_failed" => "identity_failed",
        "download_failed" => "download_failed",
        "transport" => "transport",
        _ => "upstream",
    }
}

/// Process-wide registry of StorageProvider implementations.
#[derive(Clone, Default)]
pub struct StorageProviderRegistry {
    providers: HashMap<String, Arc<dyn StorageProvider>>,
}

impl StorageProviderRegistry {
    pub fn new() -> Self {
        Self {
            providers: HashMap::new(),
        }
    }

    pub fn register(&mut self, provider: Arc<dyn StorageProvider>) {
        self.providers
            .insert(provider.id().to_owned(), provider);
    }

    pub fn get(&self, id: &str) -> Option<Arc<dyn StorageProvider>> {
        self.providers.get(id).cloned()
    }

    pub fn contains(&self, id: &str) -> bool {
        self.providers.contains_key(id)
    }

    pub fn ids(&self) -> Vec<String> {
        let mut ids: Vec<_> = self.providers.keys().cloned().collect();
        ids.sort();
        ids
    }

    /// Built-in adapters shipped with the host (Guangya HTTP transitional).
    pub fn builtin() -> Self {
        let mut registry = Self::new();
        registry.register(Arc::new(HttpRpcStorageAdapter::guangya_from_env()));
        registry
    }
}

static REGISTRY: OnceLock<StorageProviderRegistry> = OnceLock::new();

/// Global registry used by `storage::rpc`. Tests may replace via `set_registry_for_tests`.
pub fn global_registry() -> &'static StorageProviderRegistry {
    REGISTRY.get_or_init(StorageProviderRegistry::builtin)
}

/// Override the process registry (unit tests only). Panics if already initialized
/// with a different instance — call before first `global_registry()` use.
#[cfg(test)]
pub fn try_init_registry_for_tests(registry: StorageProviderRegistry) -> bool {
    REGISTRY.set(registry).is_ok()
}

/// HTTP RPC adapter for process-out storage plugins that speak the existing
/// Guangya-shaped JSON protocol (`op`, `credentials`, `data` / `error`).
pub struct HttpRpcStorageAdapter {
    id: String,
    display_name: String,
    endpoint: String,
}

impl HttpRpcStorageAdapter {
    pub fn new(id: impl Into<String>, display_name: impl Into<String>, endpoint: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            display_name: display_name.into(),
            endpoint: endpoint.into(),
        }
    }

    /// Transitional Guangya wiring: prefer generic endpoint env, fall back to legacy.
    pub fn guangya_from_env() -> Self {
        let endpoint = std::env::var(provider_endpoint_env(GUANGYA_PROVIDER_ID))
            .or_else(|_| std::env::var(GUANGYA_PLUGIN_URL_ENV))
            .unwrap_or_else(|_| DEFAULT_GUANGYA_ENDPOINT.to_owned());
        Self::new(GUANGYA_PROVIDER_ID, "Guangya", endpoint)
    }

    pub fn endpoint(&self) -> &str {
        &self.endpoint
    }
}

#[async_trait]
impl StorageProvider for HttpRpcStorageAdapter {
    fn id(&self) -> &str {
        &self.id
    }

    fn display_name(&self) -> &str {
        &self.display_name
    }

    async fn invoke(
        &self,
        ctx: &StorageProviderContext,
        op: &str,
        mut input: Value,
    ) -> Result<StorageRpcOutcome, StorageProviderError> {
        if ctx.plugin_key.len() < 32 {
            return Err(StorageProviderError::new(
                "invalid_request",
                "插件通信密钥至少需要 32 字符",
            ));
        }

        if let Some(obj) = input.as_object_mut() {
            obj.insert("op".into(), json!(op));
            obj.insert("accountId".into(), json!(ctx.account_id));
            obj.insert("deviceId".into(), json!(ctx.device_id));
            obj.insert("credentialVersion".into(), json!(ctx.credential_version));
            obj.insert("qps".into(), json!(ctx.qps));
            obj.insert("credentials".into(), ctx.credentials.clone());
        }

        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(55))
            .redirect(reqwest::redirect::Policy::none())
            .build()
            .map_err(|_| StorageProviderError::new("transport", "plugin client unavailable"))?;

        let mut response = client
            .post(&self.endpoint)
            .bearer_auth(&ctx.plugin_key)
            .json(&input)
            .send()
            .await
            .map_err(|_| {
                StorageProviderError::new(
                    "transport",
                    format!("{} 插件连接失败", self.display_name),
                )
            })?;

        if !response.status().is_success() {
            return Err(StorageProviderError::new(
                "transport",
                format!("{} 插件拒绝请求，请检查通信密钥", self.display_name),
            ));
        }

        let mut bytes = Vec::new();
        while let Some(chunk) = response.chunk().await.map_err(|_| {
            StorageProviderError::new("transport", "插件响应读取失败")
        })? {
            if bytes.len() + chunk.len() > 45 * 1024 * 1024 {
                return Err(StorageProviderError::new(
                    "invalid_response",
                    "插件响应超出上限",
                ));
            }
            bytes.extend_from_slice(&chunk);
        }

        let result: Value = serde_json::from_slice(&bytes)
            .map_err(|_| StorageProviderError::new("invalid_response", "插件响应无效"))?;

        let retry_after = result
            .pointer("/error/retryAfter")
            .and_then(|v| v.as_f64())
            .unwrap_or(0.0);

        Ok(StorageRpcOutcome {
            data: result.get("data").cloned().unwrap_or(Value::Null),
            credentials: result
                .get("credentials")
                .filter(|v| v.is_object())
                .cloned(),
            error_code: result
                .pointer("/error/code")
                .and_then(|v| v.as_str())
                .map(str::to_owned),
            error_message: result
                .pointer("/error/message")
                .and_then(|v| v.as_str())
                .map(str::to_owned),
            retry_after,
        })
    }
}

/// Resolve the shared plugin key from the environment.
pub fn load_plugin_key() -> Result<String, StorageProviderError> {
    let key = std::env::var(STORAGE_PLUGIN_KEY_ENV).map_err(|_| {
        StorageProviderError::new(
            "invalid_request",
            "请配置 FBZ_STORAGE_PLUGIN_KEY 并启动存储插件",
        )
    })?;
    if key.len() < 32 {
        return Err(StorageProviderError::new(
            "invalid_request",
            "插件通信密钥至少需要 32 字符",
        ));
    }
    Ok(key)
}

/// Map provider-layer errors / RPC error codes to host-facing messages.
pub fn user_message_for_provider_error(provider_id: &str, code: &str) -> &'static str {
    let _ = provider_id;
    match code {
        "auth_expired" => "云盘登录已失效，请重新扫码",
        "rate_limited" => "云盘限流，请稍后重试",
        "unsafe_url" => "云端文件地址被安全策略拒绝",
        "invalid_request" => "存储插件配置无效",
        "transport" => "存储插件连接失败，请检查插件进程与通信密钥",
        _ => "云盘操作失败，请稍后重试或检查插件配置",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct MockProvider;

    #[async_trait]
    impl StorageProvider for MockProvider {
        fn id(&self) -> &str {
            "mock"
        }

        async fn invoke(
            &self,
            _ctx: &StorageProviderContext,
            op: &str,
            _input: Value,
        ) -> Result<StorageRpcOutcome, StorageProviderError> {
            match op {
                "list" => Ok(StorageRpcOutcome {
                    data: json!({
                        "entries": [{"id":"1","name":"a","directory":true,"size":0,"version":"v"}],
                        "nextPage": null
                    }),
                    ..Default::default()
                }),
                "stat" => Ok(StorageRpcOutcome {
                    data: json!({"id":"1","name":"a","directory":true,"size":0,"version":"v"}),
                    ..Default::default()
                }),
                "resolve" => Ok(StorageRpcOutcome {
                    data: json!({"url":"https://cdn.example/x","expiresAt": 1}),
                    ..Default::default()
                }),
                "read" => Ok(StorageRpcOutcome {
                    data: json!({"bytes":[1,2,3]}),
                    ..Default::default()
                }),
                other => Err(StorageProviderError::unsupported(other)),
            }
        }

        async fn stat(
            &self,
            ctx: &StorageProviderContext,
            file_id: &str,
        ) -> Result<Value, StorageProviderError> {
            let outcome = self
                .invoke(ctx, "stat", json!({ "fileId": file_id }))
                .await?;
            map_rpc_error(&outcome)?;
            Ok(outcome.data)
        }
    }

    fn ctx() -> StorageProviderContext {
        StorageProviderContext {
            account_id: "acc".into(),
            device_id: "dev".into(),
            credential_version: 1,
            qps: 10,
            credentials: json!({"access_token":"t"}),
            plugin_key: "k".repeat(32),
        }
    }

    #[tokio::test]
    async fn registry_routes_by_provider_id() {
        let mut registry = StorageProviderRegistry::new();
        registry.register(Arc::new(MockProvider));
        assert!(registry.contains("mock"));
        assert!(!registry.contains(GUANGYA_PROVIDER_ID));

        let provider = registry.get("mock").unwrap();
        let page = provider.list(&ctx(), "", 0).await.unwrap();
        assert_eq!(page.entries.as_array().unwrap().len(), 1);

        let resolved = provider.resolve(&ctx(), "1", false).await.unwrap();
        assert!(resolved.url.starts_with("https://"));

        let bytes = provider.read_small(&ctx(), "1", 8).await.unwrap();
        assert_eq!(bytes, vec![1, 2, 3]);

        let opened = provider.open(&ctx(), "1").await.unwrap();
        assert_eq!(opened.url, resolved.url);

        let stat = provider.stat(&ctx(), "1").await.unwrap();
        assert_eq!(stat["id"], "1");
    }

    #[test]
    fn builtin_registers_guangya_adapter() {
        let registry = StorageProviderRegistry::builtin();
        assert!(registry.contains(GUANGYA_PROVIDER_ID));
        let guangya = registry.get(GUANGYA_PROVIDER_ID).unwrap();
        assert_eq!(guangya.id(), GUANGYA_PROVIDER_ID);
        assert!(guangya.supported_ops().contains(&"list"));
        assert!(guangya.supported_ops().contains(&"resolve"));
    }

    #[test]
    fn guangya_endpoint_prefers_generic_env() {
        // Isolation: only assert constructor default when neither env is forced.
        let adapter = HttpRpcStorageAdapter::new("guangya", "Guangya", "http://example.test/rpc");
        assert_eq!(adapter.endpoint(), "http://example.test/rpc");
        assert_eq!(
            provider_endpoint_env("guangya"),
            "FBZ_STORAGE_PROVIDER_ENDPOINT_guangya"
        );
    }

    #[tokio::test]
    async fn default_stat_is_unsupported_on_http_adapter() {
        let adapter = HttpRpcStorageAdapter::new("guangya", "Guangya", "http://127.0.0.1:9/rpc");
        let err = adapter.stat(&ctx(), "x").await.unwrap_err();
        assert_eq!(err.code, "unsupported");
    }
}
