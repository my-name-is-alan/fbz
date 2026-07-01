//! Metadata provider subsystem.
//!
//! Defines the [`MetadataProvider`] trait and [`MetadataProviderRegistry`], the
//! orchestrator that replaces the hard-coded `match` dispatch the legacy
//! `provider.rs` used. Providers live in their own modules (`tmdb`, `tvdb`,
//! `fanart`); the registry walks them in configured order, applying the role
//! rules (first base-match wins, enrichment runs after a match) and recording a
//! per-provider attempt trail.
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
            ctx,
            retry_policy: retry::RetryPolicy::default(),
        })
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
    pub async fn match_item_with_report(
        &self,
        input: &MetadataLookup,
    ) -> Result<MetadataLookupReport, MetadataProviderError> {
        let mut attempts = Vec::new();
        let mut matched: Option<MetadataMatch> = None;

        for provider_id in &self.order {
            let Some(provider) = self.providers.get(provider_id) else {
                attempts.push(MetadataProviderAttempt::skipped(
                    provider_id.clone(),
                    "unsupported metadata provider",
                ));
                continue;
            };

            match provider.role() {
                ProviderRole::BaseMatch => {
                    if matched.is_some() {
                        attempts.push(MetadataProviderAttempt::skipped(
                            provider_id.clone(),
                            "base metadata match already exists",
                        ));
                        continue;
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
                                provider_id.clone(),
                                found.external_id.clone(),
                            ));
                            matched = Some(*found);
                        }
                        Ok(ProviderMatchOutcome::NotMatched(message)) => attempts.push(
                            MetadataProviderAttempt::not_matched(provider_id.clone(), message),
                        ),
                        Ok(ProviderMatchOutcome::Skipped(message)) => attempts.push(
                            MetadataProviderAttempt::skipped(provider_id.clone(), message),
                        ),
                        Err(err) => {
                            attempts.push(MetadataProviderAttempt::failed(
                                provider_id.clone(),
                                err.to_string(),
                            ));
                            return Err(err);
                        }
                    }
                }
                ProviderRole::Enrichment => {
                    let Some(current) = matched.as_mut() else {
                        attempts.push(MetadataProviderAttempt::skipped(
                            provider_id.clone(),
                            "requires a matched metadata item",
                        ));
                        continue;
                    };
                    match provider.enrich(&self.ctx, input, current).await {
                        Ok(ProviderEnrichOutcome::Matched { external_id }) => attempts.push(
                            MetadataProviderAttempt::matched(provider_id.clone(), external_id),
                        ),
                        Ok(ProviderEnrichOutcome::NotMatched(message)) => attempts.push(
                            MetadataProviderAttempt::not_matched(provider_id.clone(), message),
                        ),
                        Ok(ProviderEnrichOutcome::Skipped(message)) => attempts.push(
                            MetadataProviderAttempt::skipped(provider_id.clone(), message),
                        ),
                        Err(err) => {
                            attempts.push(MetadataProviderAttempt::failed(
                                provider_id.clone(),
                                err.to_string(),
                            ));
                            return Err(err);
                        }
                    }
                }
            }
        }

        Ok(MetadataLookupReport { matched, attempts })
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
            })
            .await
            .unwrap();

        assert_eq!(report.matched, None);
        assert_eq!(report.attempts.len(), 4);
        assert_eq!(report.attempts[0].provider, "tmdb");
        assert_eq!(
            report.attempts[0].status,
            MetadataProviderAttemptStatus::Skipped
        );
        assert_eq!(report.attempts[1].provider, "tvdb");
        assert_eq!(report.attempts[2].provider, "fanart");
        assert_eq!(report.attempts[3].provider, "unknown");
    }
}
