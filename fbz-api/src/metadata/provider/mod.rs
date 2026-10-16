//! Metadata provider subsystem.
//!
//! Defines the [`MetadataProvider`] trait and [`MetadataProviderRegistry`], the
//! orchestrator that replaces the hard-coded `match` dispatch the legacy
//! `provider.rs` used. Providers live in their own modules (`tmdb`, `tvdb`,
//! `fanart`); the registry walks them in configured order, applying the role
//! rules (first base-match wins, enrichment runs after a match) and recording a
//! per-provider attempt trail.
//!
//! Official packages (`plugins/tmdb-provider` etc.) subscribe to
//! `metadata.provider.query` and are discovered at job time via
//! [`plugin_sync::discover_plugin_providers`]. Match prefers enabled plugins
//! (SyncInvoker) over built-in clients; builtins remain transitional fallback.
//! Built-in modules below are **frozen for deepening** — see
//! `docs/plans/official-metadata-providers-migration.md`.
//!
//! # Provider roles
//!
//! - [`ProviderRole::BaseMatch`]: can find a match from scratch (TMDB, TVDB).
//!   The first base-match provider that returns a match wins; later base-match
//!   providers are skipped with "base metadata match already exists".
//! - [`ProviderRole::Enrichment`]: adds artwork/fields to an existing match
//!   (Fanart). Runs only after a base match is found.

pub mod fanart;
pub mod imdb;
pub mod plugin;
pub mod plugin_sync;
pub mod proxy;
pub mod retry;
pub mod shared;
pub mod spotify;
pub mod tmdb;
pub mod token_pool;
pub mod tvdb;

use std::collections::HashMap;
use std::sync::Arc;

use async_trait::async_trait;
use reqwest::Client;
use serde::Serialize;
use serde_json::json;
use tracing::warn;

use crate::config::{MetadataConfig, ProxyConfig};

pub use self::plugin::{
    DisabledPluginQuerier, PluginMetadataContribution, PluginMetadataProvider,
    PluginMetadataQuerier, merge_plugin_metadata,
};
pub use self::proxy::{ProviderProxyOverride, build_provider_clients, build_single_client};
pub use self::shared::{
    MetadataArtwork, MetadataCollection, MetadataExternalId, MetadataLookup, MetadataLookupReport,
    MetadataMatch, MetadataNamedValue, MetadataPerson, MetadataProviderAttempt,
    MetadataProviderAttemptStatus, MetadataProviderError, MetadataVideo, ProviderClients,
    ProviderContext, ProviderEnrichOutcome, ProviderMatchOutcome,
};

use self::shared::normalized_providers;

use crate::library::metadata_sources::{LibraryMatchStep, resolve_provider_source_key};
use crate::metadata::nfo_match;

/// Role of a provider in the scraping pipeline.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ProviderRole {
    /// Can find a match from scratch (TMDB, TVDB).
    BaseMatch,
    /// Only adds artwork or extra fields after a match exists (Fanart).
    Enrichment,
}

/// Core trait for metadata providers.
#[async_trait]
pub trait MetadataProvider: Send + Sync {
    /// Stable identifier, e.g. "tmdb", "tvdb", "imdb", "fanart", "plugin:{id}".
    fn id(&self) -> &str;

    /// Provider role: base-match source or enrichment-only.
    fn role(&self) -> ProviderRole;

    /// Supported item types (movie, series, season, episode, …).
    fn supports(&self, item_type: &str) -> bool;

    /// Base matching. Enrichment-only providers return
    /// [`ProviderMatchOutcome::Skipped`].
    async fn match_item(
        &self,
        ctx: &ProviderContext,
        input: &MetadataLookup,
    ) -> Result<ProviderMatchOutcome, MetadataProviderError>;

    /// Enrichment: mutate an existing match in place. Base-match-only providers
    /// fall through to the default (skip).
    async fn enrich(
        &self,
        ctx: &ProviderContext,
        input: &MetadataLookup,
        current: &mut MetadataMatch,
    ) -> Result<ProviderEnrichOutcome, MetadataProviderError> {
        let _ = (ctx, input, current);
        Ok(ProviderEnrichOutcome::Skipped(
            "provider does not support enrichment".to_owned(),
        ))
    }
}

/// Registry that orchestrates providers in configured order and by role.
#[derive(Clone)]
pub struct MetadataProviderRegistry {
    /// Provider ids in the configured matching order (normalized, lowercased).
    order: Vec<String>,
    /// Provider implementations keyed by id.
    providers: HashMap<String, Arc<dyn MetadataProvider>>,
    /// 已启用插件 provider（`plugin:{id}`），由 service 按订阅动态附加。
    /// Match 编排**优先**走这些 SyncInvoker 适配器；内置客户端仅 fallback。
    plugin_providers: Vec<Arc<dyn MetadataProvider>>,
    /// 库级源链展开后的 Match 步骤；`None` 时按全局 `order` + 插件优先合成。
    match_chain: Option<Vec<LibraryMatchStep>>,
    /// Shared runtime context (HTTP client + resolved config).
    ctx: ProviderContext,
    /// 联网重试策略（瞬时故障有界重试 + 指数退避）。
    retry_policy: retry::RetryPolicy,
}

impl MetadataProviderRegistry {
    /// Builds a registry from the global proxy config with no per-provider
    /// overrides (every provider uses the global-baseline client).
    pub fn from_config(
        metadata: MetadataConfig,
        proxy: ProxyConfig,
    ) -> Result<Self, MetadataProviderError> {
        Self::from_config_with_overrides(metadata, proxy, &HashMap::new())
    }

    /// Builds a registry honoring per-provider proxy overrides (phase 2).
    /// `overrides` maps provider id → proxy mode (inherit/direct/custom).
    pub fn from_config_with_overrides(
        metadata: MetadataConfig,
        proxy: ProxyConfig,
        overrides: &HashMap<String, ProviderProxyOverride>,
    ) -> Result<Self, MetadataProviderError> {
        let clients = build_provider_clients(&proxy, overrides)?;
        let ctx = ProviderContext::new(clients, metadata.clone());

        let mut providers: HashMap<String, Arc<dyn MetadataProvider>> = HashMap::new();
        let tmdb: Arc<dyn MetadataProvider> = Arc::new(tmdb::TmdbProvider::new());
        let tvdb: Arc<dyn MetadataProvider> = Arc::new(tvdb::TvdbProvider::new());
        let fanart: Arc<dyn MetadataProvider> = Arc::new(fanart::FanartProvider::new());
        let imdb: Arc<dyn MetadataProvider> = Arc::new(imdb::ImdbProvider::new());
        let spotify: Arc<dyn MetadataProvider> = Arc::new(spotify::SpotifyProvider::new());
        providers.insert(tmdb.id().to_owned(), tmdb);
        providers.insert(tvdb.id().to_owned(), tvdb);
        providers.insert(fanart.id().to_owned(), fanart);
        providers.insert(imdb.id().to_owned(), imdb);
        providers.insert(spotify.id().to_owned(), spotify);

        Ok(Self {
            order: normalized_providers(&metadata.providers),
            providers,
            plugin_providers: Vec::new(),
            match_chain: None,
            ctx,
            retry_policy: retry::RetryPolicy::default(),
        })
    }

    /// 附加插件 provider（消费式 builder；service 在每个 job 前按订阅发现调用）。
    pub fn with_plugin_providers(mut self, providers: Vec<Arc<dyn MetadataProvider>>) -> Self {
        self.plugin_providers = providers;
        self
    }

    /// 附加库级源链 Match 步骤（消费式 builder；空切片视为未配置，回退合成链）。
    pub fn with_match_chain(mut self, steps: Vec<LibraryMatchStep>) -> Self {
        self.match_chain = if steps.is_empty() { None } else { Some(steps) };
        self
    }

    /// 注入 API key 令牌池（从 resolved.keys 构造，每 provider 多 key 时启用轮转）。
    /// registry 构造后由 service 调用。返回带池的新 registry（消费式 builder）。
    pub fn with_token_pools(mut self, keys: &HashMap<String, Vec<String>>) -> Self {
        let pools: HashMap<String, Arc<token_pool::TokenPool>> = keys
            .iter()
            .filter(|(_, tokens)| !tokens.is_empty())
            .map(|(id, tokens)| {
                (
                    id.clone(),
                    Arc::new(token_pool::TokenPool::new(tokens.clone())),
                )
            })
            .collect();
        self.ctx = self.ctx.with_token_pools(pools);
        self
    }

    /// Looks up metadata, returning the match and the per-provider attempt trail.
    ///
    /// Transition policy (§5.9 / §6.4)：已启用插件 SyncInvoker **优先**；内置
    /// TMDB/TVDB/… 仅在插件未启用、返回空贡献或失败时作 fallback。若设置了
    /// [`Self::with_match_chain`]，严格按库级源链步骤编排（含 NFO pending 占位）。
    pub async fn match_item_with_report(
        &self,
        input: &MetadataLookup,
    ) -> Result<MetadataLookupReport, MetadataProviderError> {
        let mut attempts = Vec::new();
        let mut matched: Option<MetadataMatch> = None;
        let steps = self.effective_match_steps();

        for step in &steps {
            match step {
                LibraryMatchStep::NfoPending => {
                    if matched.is_some() {
                        attempts.push(MetadataProviderAttempt::skipped(
                            "nfo".to_owned(),
                            "skipped; already matched by earlier source",
                        ));
                        continue;
                    }
                    match nfo_match::try_match_local_nfo(input) {
                        Ok(Some(found)) => {
                            attempts.push(MetadataProviderAttempt::matched(
                                "nfo".to_owned(),
                                found.external_id.clone(),
                            ));
                            matched = Some(found);
                        }
                        Ok(None) => {
                            let reason = if input
                                .primary_path
                                .as_deref()
                                .map(str::trim)
                                .filter(|p| !p.is_empty())
                                .is_none()
                            {
                                "no primary media path for local nfo"
                            } else if input
                                .primary_path
                                .as_deref()
                                .is_some_and(|p| p.starts_with("fbz-storage://"))
                            {
                                "cloud storage path; local nfo skipped (use storage importer)"
                            } else {
                                "no local nfo sidecar found next to media"
                            };
                            attempts.push(MetadataProviderAttempt::skipped(
                                "nfo".to_owned(),
                                reason,
                            ));
                        }
                        Err(err) => {
                            attempts.push(MetadataProviderAttempt::failed(
                                "nfo".to_owned(),
                                err,
                            ));
                        }
                    }
                }
                LibraryMatchStep::Provider {
                    source_key: _,
                    prefer_plugin_id,
                    builtin_fallback,
                } => {
                    let mut plugin_base_matched = false;
                    if let Some(plugin_id) = prefer_plugin_id.as_deref() {
                        if let Some(provider) = self.find_plugin_provider(plugin_id) {
                            let before = matched.is_some();
                            self.run_plugin_provider(
                                provider.as_ref(),
                                input,
                                &mut matched,
                                &mut attempts,
                            )
                            .await;
                            plugin_base_matched = !before && matched.is_some();
                        } else {
                            attempts.push(MetadataProviderAttempt::skipped(
                                format!("plugin:{plugin_id}"),
                                "official plugin not enabled; falling back to built-in if available",
                            ));
                        }
                    }

                    if let Some(builtin_id) = builtin_fallback.as_deref() {
                        // BaseMatch 已被插件拿下时，跳过同语义内置，避免重复命中。
                        if plugin_base_matched {
                            attempts.push(MetadataProviderAttempt::skipped(
                                builtin_id.to_owned(),
                                "built-in skipped; official plugin already matched",
                            ));
                        } else {
                            self.run_builtin_provider(
                                builtin_id,
                                input,
                                &mut matched,
                                &mut attempts,
                            )
                            .await;
                        }
                    }
                }
            }
        }

        Ok(MetadataLookupReport { matched, attempts })
    }

    /// 库级链优先；否则按全局 `order` 合成「插件优先 + 内置 fallback」，并追加
    /// 链上未覆盖的已启用社区/官方插件。
    fn effective_match_steps(&self) -> Vec<LibraryMatchStep> {
        if let Some(steps) = &self.match_chain {
            return steps.clone();
        }

        let mut steps = Vec::new();
        let mut covered_plugins = std::collections::HashSet::new();
        for provider_id in &self.order {
            let (prefer_plugin_id, builtin_fallback) = resolve_provider_source_key(provider_id);
            if let Some(ref plugin_id) = prefer_plugin_id {
                covered_plugins.insert(plugin_id.clone());
            }
            steps.push(LibraryMatchStep::Provider {
                source_key: provider_id.clone(),
                prefer_plugin_id,
                builtin_fallback,
            });
        }

        for provider in &self.plugin_providers {
            let id = provider.id();
            let Some(raw) = id.strip_prefix("plugin:") else {
                continue;
            };
            if covered_plugins.contains(raw) {
                continue;
            }
            // 官方插件若对应 builtin 已在 order 中会已覆盖；此处追加社区包等。
            let (prefer_plugin_id, builtin_fallback) =
                resolve_provider_source_key(&format!("plugin:{raw}"));
            if let Some(ref plugin_id) = prefer_plugin_id {
                if covered_plugins.contains(plugin_id) {
                    continue;
                }
                covered_plugins.insert(plugin_id.clone());
            }
            steps.push(LibraryMatchStep::Provider {
                source_key: format!("plugin:{raw}"),
                prefer_plugin_id,
                builtin_fallback,
            });
        }

        steps
    }

    fn find_plugin_provider(&self, plugin_id: &str) -> Option<&Arc<dyn MetadataProvider>> {
        let want = format!("plugin:{}", plugin_id.trim());
        self.plugin_providers
            .iter()
            .find(|provider| provider.id() == want)
    }

    async fn run_plugin_provider(
        &self,
        provider: &dyn MetadataProvider,
        input: &MetadataLookup,
        matched: &mut Option<MetadataMatch>,
        attempts: &mut Vec<MetadataProviderAttempt>,
    ) {
        let provider_id = provider.id().to_owned();
        if let Some(current) = matched.as_mut() {
            match provider.enrich(&self.ctx, input, current).await {
                Ok(ProviderEnrichOutcome::Matched { external_id }) => {
                    attempts.push(MetadataProviderAttempt::matched(provider_id, external_id))
                }
                Ok(ProviderEnrichOutcome::NotMatched(message)) => {
                    attempts.push(MetadataProviderAttempt::not_matched(provider_id, message))
                }
                Ok(ProviderEnrichOutcome::Skipped(message)) => {
                    attempts.push(MetadataProviderAttempt::skipped(provider_id, message))
                }
                Err(err) => attempts.push(MetadataProviderAttempt::failed(
                    provider_id,
                    err.to_string(),
                )),
            }
        } else {
            match provider.match_item(&self.ctx, input).await {
                Ok(ProviderMatchOutcome::Matched(found)) => {
                    attempts.push(MetadataProviderAttempt::matched(
                        provider_id,
                        found.external_id.clone(),
                    ));
                    *matched = Some(*found);
                }
                Ok(ProviderMatchOutcome::NotMatched(message)) => {
                    attempts.push(MetadataProviderAttempt::not_matched(provider_id, message))
                }
                Ok(ProviderMatchOutcome::Skipped(message)) => {
                    attempts.push(MetadataProviderAttempt::skipped(provider_id, message))
                }
                Err(err) => attempts.push(MetadataProviderAttempt::failed(
                    provider_id,
                    err.to_string(),
                )),
            }
        }
    }

    async fn run_builtin_provider(
        &self,
        provider_id: &str,
        input: &MetadataLookup,
        matched: &mut Option<MetadataMatch>,
        attempts: &mut Vec<MetadataProviderAttempt>,
    ) {
        let Some(provider) = self.providers.get(provider_id) else {
            attempts.push(MetadataProviderAttempt::skipped(
                provider_id.to_owned(),
                "unsupported metadata provider",
            ));
            return;
        };

        match provider.role() {
            ProviderRole::BaseMatch => {
                if matched.is_some() {
                    attempts.push(MetadataProviderAttempt::skipped(
                        provider_id.to_owned(),
                        "base metadata match already exists",
                    ));
                    return;
                }
                match retry::retry_async(
                    &self.retry_policy,
                    || provider.match_item(&self.ctx, input),
                    tokio::time::sleep,
                )
                .await
                {
                    Ok(ProviderMatchOutcome::Matched(found)) => {
                        attempts.push(MetadataProviderAttempt::matched(
                            provider_id.to_owned(),
                            found.external_id.clone(),
                        ));
                        *matched = Some(*found);
                    }
                    Ok(ProviderMatchOutcome::NotMatched(message)) => attempts.push(
                        MetadataProviderAttempt::not_matched(provider_id.to_owned(), message),
                    ),
                    Ok(ProviderMatchOutcome::Skipped(message)) => attempts
                        .push(MetadataProviderAttempt::skipped(provider_id.to_owned(), message)),
                    Err(err) => {
                        warn!(
                            provider = %provider_id,
                            error = %err,
                            "metadata provider failed; continuing with next provider"
                        );
                        attempts.push(MetadataProviderAttempt::failed(
                            provider_id.to_owned(),
                            err.to_string(),
                        ));
                    }
                }
            }
            ProviderRole::Enrichment => {
                let Some(current) = matched.as_mut() else {
                    attempts.push(MetadataProviderAttempt::skipped(
                        provider_id.to_owned(),
                        "requires a matched metadata item",
                    ));
                    return;
                };
                match provider.enrich(&self.ctx, input, current).await {
                    Ok(ProviderEnrichOutcome::Matched { external_id }) => attempts.push(
                        MetadataProviderAttempt::matched(provider_id.to_owned(), external_id),
                    ),
                    Ok(ProviderEnrichOutcome::NotMatched(message)) => attempts.push(
                        MetadataProviderAttempt::not_matched(provider_id.to_owned(), message),
                    ),
                    Ok(ProviderEnrichOutcome::Skipped(message)) => attempts
                        .push(MetadataProviderAttempt::skipped(provider_id.to_owned(), message)),
                    Err(err) => {
                        warn!(
                            provider = %provider_id,
                            error = %err,
                            "metadata enrichment failed; keeping base match"
                        );
                        attempts.push(MetadataProviderAttempt::failed(
                            provider_id.to_owned(),
                            err.to_string(),
                        ));
                    }
                }
            }
        }
    }

    /// Looks up metadata, returning only the match (if any).
    pub async fn match_item(
        &self,
        input: &MetadataLookup,
    ) -> Result<Option<MetadataMatch>, MetadataProviderError> {
        self.match_item_with_report(input)
            .await
            .map(|report| report.matched)
    }
}

/// Result of an admin-triggered connectivity / auth probe for one provider.
#[derive(Clone, Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ProviderProbeResult {
    pub provider: String,
    pub ok: bool,
    pub message: String,
}

/// Runs a controlled connectivity/auth probe against a provider using the given
/// effective config, proxy, and optional per-provider proxy override. Does not
/// write to the DB. TMDB and TVDB perform a real authenticated request; Fanart
/// and IMDb report a config-only verdict (no cheap unauthenticated endpoint).
pub async fn probe_provider(
    provider_id: &str,
    metadata: &MetadataConfig,
    proxy: &ProxyConfig,
    proxy_override: Option<&ProviderProxyOverride>,
) -> ProviderProbeResult {
    let id = provider_id.trim().to_ascii_lowercase();
    let client = match build_single_client(proxy, proxy_override) {
        Ok(client) => client,
        Err(err) => {
            return ProviderProbeResult {
                provider: id,
                ok: false,
                message: format!("failed to build HTTP client: {err}"),
            };
        }
    };

    match id.as_str() {
        "tmdb" => probe_tmdb(&id, metadata, &client).await,
        "tvdb" => probe_tvdb(&id, metadata, &client).await,
        "fanart" => config_only_probe(&id, metadata.fanart_api_key.is_some(), "Fanart"),
        "imdb" => ProviderProbeResult {
            provider: id,
            ok: true,
            message: "IMDb is an enrichment-only provider; no live probe".to_owned(),
        },
        other => ProviderProbeResult {
            provider: other.to_owned(),
            ok: false,
            message: "unknown metadata provider".to_owned(),
        },
    }
}

async fn probe_tmdb(id: &str, metadata: &MetadataConfig, client: &Client) -> ProviderProbeResult {
    let Some(token) = metadata.tmdb_access_token.as_deref() else {
        return ProviderProbeResult {
            provider: id.to_owned(),
            ok: false,
            message: "missing TMDB access token".to_owned(),
        };
    };
    let url = format!(
        "{}/authentication",
        metadata.tmdb_api_base_url.trim_end_matches('/')
    );
    match client.get(url).bearer_auth(token).send().await {
        Ok(response) if response.status().is_success() => ProviderProbeResult {
            provider: id.to_owned(),
            ok: true,
            message: "TMDB authentication succeeded".to_owned(),
        },
        Ok(response) => ProviderProbeResult {
            provider: id.to_owned(),
            ok: false,
            message: format!("TMDB returned status {}", response.status()),
        },
        Err(err) => ProviderProbeResult {
            provider: id.to_owned(),
            ok: false,
            message: format!("TMDB request failed: {err}"),
        },
    }
}

async fn probe_tvdb(id: &str, metadata: &MetadataConfig, client: &Client) -> ProviderProbeResult {
    let Some(api_key) = metadata.tvdb_api_key.as_deref() else {
        return ProviderProbeResult {
            provider: id.to_owned(),
            ok: false,
            message: "missing TVDB API key".to_owned(),
        };
    };
    let url = format!("{}/login", metadata.tvdb_api_base_url.trim_end_matches('/'));
    match client
        .post(url)
        .json(&json!({ "apikey": api_key }))
        .send()
        .await
    {
        Ok(response) if response.status().is_success() => ProviderProbeResult {
            provider: id.to_owned(),
            ok: true,
            message: "TVDB login succeeded".to_owned(),
        },
        Ok(response) => ProviderProbeResult {
            provider: id.to_owned(),
            ok: false,
            message: format!("TVDB returned status {}", response.status()),
        },
        Err(err) => ProviderProbeResult {
            provider: id.to_owned(),
            ok: false,
            message: format!("TVDB request failed: {err}"),
        },
    }
}

fn config_only_probe(id: &str, has_key: bool, label: &str) -> ProviderProbeResult {
    ProviderProbeResult {
        provider: id.to_owned(),
        ok: has_key,
        message: if has_key {
            format!("{label} API key is configured (no live probe)")
        } else {
            format!("missing {label} API key")
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn metadata_config(providers: &[&str]) -> MetadataConfig {
        MetadataConfig {
            providers: providers
                .iter()
                .map(|provider| provider.to_string())
                .collect(),
            tmdb_access_token: None,
            tmdb_api_base_url: "https://api.themoviedb.org/3".to_owned(),
            tmdb_image_base_url: "https://image.tmdb.org/t/p".to_owned(),
            tvdb_api_key: None,
            tvdb_api_base_url: "https://api4.thetvdb.com/v4".to_owned(),
            fanart_api_key: None,
            fanart_api_base_url: "https://webservice.fanart.tv/v3".to_owned(),
            spotify_client_id: None,
            spotify_client_secret: None,
            spotify_api_base_url: "https://api.spotify.com/v1".to_owned(),
            spotify_auth_url: "https://accounts.spotify.com/api/token".to_owned(),
        }
    }

    fn proxy_config() -> ProxyConfig {
        ProxyConfig {
            http_proxy: None,
            https_proxy: None,
            no_proxy: Vec::new(),
            policy: "system".to_owned(),
        }
    }

    #[test]
    fn provider_trait_is_object_safe() {
        fn assert_object_safe(_: &dyn MetadataProvider) {}
        let _ = assert_object_safe;
    }

    #[test]
    fn with_token_pools_exposes_rotating_pool_on_context() {
        let mut keys = HashMap::new();
        keys.insert("tmdb".to_owned(), vec!["k1".to_owned(), "k2".to_owned()]);
        // 空池的 provider 不应建池。
        keys.insert("tvdb".to_owned(), Vec::new());
        let registry =
            MetadataProviderRegistry::from_config(metadata_config(&["tmdb"]), proxy_config())
                .unwrap()
                .with_token_pools(&keys);

        let pool = registry.ctx.token_pool("tmdb").expect("tmdb pool present");
        assert_eq!(pool.len(), 2);
        // 轮转：连续 acquire 取到不同 key。
        assert_eq!(pool.acquire().unwrap().token, "k1");
        assert_eq!(pool.acquire().unwrap().token, "k2");
        // 空 key 列表不建池。
        assert!(registry.ctx.token_pool("tvdb").is_none());
        // 未配置的 provider 无池（退化为单 token）。
        assert!(registry.ctx.token_pool("fanart").is_none());
    }

    #[tokio::test]
    async fn lookup_report_records_skipped_provider_boundaries() {
        let registry = MetadataProviderRegistry::from_config(
            metadata_config(&[" TMDB ", "tvdb", "fanart", "unknown"]),
            proxy_config(),
        )
        .unwrap();

        let report = registry
            .match_item_with_report(&MetadataLookup {
                item_type: "movie".to_owned(),
                title: "Movie".to_owned(),
                original_title: None,
                production_year: Some(2026),
                season: None,
                episode: None,
                tmdb_id: None,
                imdb_id: None,
                tvdb_id: None,
                language: Some("zh-CN".to_owned()),
                country: Some("CN".to_owned()),
                image_language: None,
                image_prefer_original: false,
                image_fallback_languages: Vec::new(),
            primary_path: None,
            })
            .await
            .unwrap();

        assert_eq!(report.matched, None);
        // Plugin-first: each official builtin also records a "plugin not enabled" skip.
        let providers: Vec<_> = report
            .attempts
            .iter()
            .map(|attempt| attempt.provider.as_str())
            .collect();
        assert!(providers.contains(&"plugin:fbz.provider.tmdb"));
        assert!(providers.contains(&"tmdb"));
        assert!(providers.contains(&"plugin:fbz.provider.tvdb"));
        assert!(providers.contains(&"tvdb"));
        assert!(providers.contains(&"plugin:fbz.provider.fanart"));
        assert!(providers.contains(&"fanart"));
        assert!(providers.contains(&"unknown"));
    }

    #[tokio::test]
    async fn match_prefers_plugin_over_builtin_when_plugin_contributes() {
        use super::plugin::{PluginMetadataContribution, PluginMetadataProvider, PluginMetadataQuerier};
        use async_trait::async_trait;

        struct FixedQuerier;

        #[async_trait]
        impl PluginMetadataQuerier for FixedQuerier {
            async fn query(
                &self,
                _ctx: &ProviderContext,
                _input: &MetadataLookup,
                _current: Option<&MetadataMatch>,
            ) -> Result<Option<PluginMetadataContribution>, MetadataProviderError> {
                Ok(Some(PluginMetadataContribution {
                    title: Some("From Plugin".to_owned()),
                    original_title: None,
                    overview: Some("plugin overview".to_owned()),
                    production_year: Some(2026),
                    premiere_date: None,
                    official_rating: None,
                    community_rating: None,
                    external_ids: Vec::new(),
                    artwork: Vec::new(),
                    genres: Vec::new(),
                    studios: Vec::new(),
                    people: Vec::new(),
                }))
            }
        }

        let plugin: std::sync::Arc<dyn MetadataProvider> = std::sync::Arc::new(
            PluginMetadataProvider::new("fbz.provider.tmdb", std::sync::Arc::new(FixedQuerier)),
        );
        let registry = MetadataProviderRegistry::from_config(
            metadata_config(&["tmdb", "tvdb"]),
            proxy_config(),
        )
        .unwrap()
        .with_plugin_providers(vec![plugin]);

        let report = registry
            .match_item_with_report(&MetadataLookup {
                item_type: "movie".to_owned(),
                title: "Movie".to_owned(),
                original_title: None,
                production_year: Some(2026),
                season: None,
                episode: None,
                tmdb_id: None,
                imdb_id: None,
                tvdb_id: None,
                language: None,
                country: None,
                image_language: None,
                image_prefer_original: false,
                image_fallback_languages: Vec::new(),
            primary_path: None,
            })
            .await
            .unwrap();

        let matched = report.matched.expect("plugin should match");
        assert_eq!(matched.title, "From Plugin");
        assert_eq!(matched.provider, "plugin:fbz.provider.tmdb");
        // First attempt is the preferred plugin; builtin tmdb is skipped after plugin match.
        assert_eq!(report.attempts[0].provider, "plugin:fbz.provider.tmdb");
        assert_eq!(
            report.attempts[0].status,
            MetadataProviderAttemptStatus::Matched
        );
        assert!(
            report
                .attempts
                .iter()
                .any(|a| a.provider == "tmdb"
                    && a.status == MetadataProviderAttemptStatus::Skipped)
        );
    }

    #[tokio::test]
    async fn match_chain_nfo_pending_is_recorded() {
        let registry = MetadataProviderRegistry::from_config(
            metadata_config(&["tmdb"]),
            proxy_config(),
        )
        .unwrap()
        .with_match_chain(vec![
            LibraryMatchStep::NfoPending,
            LibraryMatchStep::Provider {
                source_key: "tmdb".to_owned(),
                prefer_plugin_id: Some("fbz.provider.tmdb".to_owned()),
                builtin_fallback: Some("tmdb".to_owned()),
            },
        ]);

        let report = registry
            .match_item_with_report(&MetadataLookup {
                item_type: "movie".to_owned(),
                title: "Movie".to_owned(),
                original_title: None,
                production_year: Some(2026),
                season: None,
                episode: None,
                tmdb_id: None,
                imdb_id: None,
                tvdb_id: None,
                language: None,
                country: None,
                image_language: None,
                image_prefer_original: false,
                image_fallback_languages: Vec::new(),
            primary_path: None,
            })
            .await
            .unwrap();

        assert_eq!(report.attempts[0].provider, "nfo");
        assert_eq!(
            report.attempts[0].status,
            MetadataProviderAttemptStatus::Skipped
        );
        assert!(
            report.attempts[0]
                .message
                .as_deref()
                .unwrap_or("")
                .contains("no primary media path")
        );
    }
}
