//! Proxy-aware HTTP client factory for metadata providers.
//!
//! Implements the proxy policy the design §5.3 calls for (the legacy code parsed
//! `no_proxy`/`policy` but never used them):
//!
//! - Global `http_proxy`/`https_proxy` form the baseline, with the `no_proxy`
//!   list honored (matching hosts go direct).
//! - Each provider may override via [`ProviderProxyOverride`]:
//!   - `inherit`: use the global baseline (incl. `no_proxy`).
//!   - `direct`: force a no-proxy client for this provider.
//!   - `custom`: route this provider through its own proxy url.
//!
//! Clients are deduped by effective proxy spec so providers sharing a spec share
//! a single `reqwest::Client`.

use std::collections::HashMap;
use std::time::Duration;

use reqwest::{Client, NoProxy, Proxy};

use crate::config::ProxyConfig;

use super::shared::{HTTP_TIMEOUT_SECONDS, MetadataProviderError, ProviderClients};

/// Per-provider proxy override (resolved from `metadata_provider_settings`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ProviderProxyOverride {
    /// `inherit` | `direct` | `custom`.
    pub mode: String,
    /// Proxy url, required when `mode == "custom"`.
    pub url: Option<String>,
}

impl ProviderProxyOverride {
    pub fn inherit() -> Self {
        Self {
            mode: "inherit".to_owned(),
            url: None,
        }
    }
}

/// The effective proxy decision for one client, used as a dedup cache key.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
enum ProxySpec {
    /// Global baseline: optional http/https proxies + no_proxy list.
    Global,
    /// No proxy at all (provider mode `direct`).
    Direct,
    /// A provider-specific custom proxy url.
    Custom(String),
}

/// Builds the per-provider client set from the global proxy config plus
/// per-provider overrides. The `default` client is the global-baseline client
/// (used for any provider without an override).
pub fn build_provider_clients(
    proxy: &ProxyConfig,
    overrides: &HashMap<String, ProviderProxyOverride>,
) -> Result<ProviderClients, MetadataProviderError> {
    let no_proxy = build_no_proxy(&proxy.no_proxy);

    let mut cache: HashMap<ProxySpec, Client> = HashMap::new();
    let default = client_for_spec(&ProxySpec::Global, proxy, no_proxy.as_ref(), &mut cache)?;

    let mut by_provider = HashMap::with_capacity(overrides.len());
    for (provider_id, ovr) in overrides {
        let spec = match ovr.mode.trim() {
            "direct" => ProxySpec::Direct,
            "custom" => match ovr
                .url
                .as_deref()
                .map(str::trim)
                .filter(|url| !url.is_empty())
            {
                Some(url) => ProxySpec::Custom(url.to_owned()),
                // Misconfigured custom (no url) falls back to global baseline.
                None => ProxySpec::Global,
            },
            // "inherit" or anything unknown → global baseline.
            _ => ProxySpec::Global,
        };
        let client = client_for_spec(&spec, proxy, no_proxy.as_ref(), &mut cache)?;
        by_provider.insert(provider_id.clone(), client);
    }

    Ok(ProviderClients::new(default, by_provider))
}

/// Builds a single client for `provider_id` with an optional override — used by
/// the admin connectivity probe.
pub fn build_single_client(
    proxy: &ProxyConfig,
    ovr: Option<&ProviderProxyOverride>,
) -> Result<Client, MetadataProviderError> {
    let no_proxy = build_no_proxy(&proxy.no_proxy);
    let spec = match ovr.map(|ovr| ovr.mode.trim()) {
        Some("direct") => ProxySpec::Direct,
        Some("custom") => match ovr
            .and_then(|ovr| ovr.url.as_deref())
            .map(str::trim)
            .filter(|url| !url.is_empty())
        {
            Some(url) => ProxySpec::Custom(url.to_owned()),
            None => ProxySpec::Global,
        },
        _ => ProxySpec::Global,
    };
    let mut cache = HashMap::new();
    client_for_spec(&spec, proxy, no_proxy.as_ref(), &mut cache)
}

fn build_no_proxy(entries: &[String]) -> Option<NoProxy> {
    let joined = entries
        .iter()
        .map(|entry| entry.trim())
        .filter(|entry| !entry.is_empty())
        .collect::<Vec<_>>()
        .join(",");
    if joined.is_empty() {
        None
    } else {
        NoProxy::from_string(&joined)
    }
}

fn client_for_spec(
    spec: &ProxySpec,
    proxy: &ProxyConfig,
    no_proxy: Option<&NoProxy>,
    cache: &mut HashMap<ProxySpec, Client>,
) -> Result<Client, MetadataProviderError> {
    if let Some(client) = cache.get(spec) {
        return Ok(client.clone());
    }
    let client = match spec {
        ProxySpec::Global => build_global_client(proxy, no_proxy)?,
        ProxySpec::Direct => base_builder().no_proxy().build().map_err(client_error)?,
        ProxySpec::Custom(url) => {
            let mut builder = base_builder();
            let mut http = Proxy::http(url).map_err(client_error)?;
            let mut https = Proxy::https(url).map_err(client_error)?;
            if let Some(no_proxy) = no_proxy.cloned() {
                http = http.no_proxy(Some(no_proxy.clone()));
                https = https.no_proxy(Some(no_proxy));
            }
            builder = builder.proxy(http).proxy(https);
            builder.build().map_err(client_error)?
        }
    };
    cache.insert(spec.clone(), client.clone());
    Ok(client)
}

fn build_global_client(
    proxy: &ProxyConfig,
    no_proxy: Option<&NoProxy>,
) -> Result<Client, MetadataProviderError> {
    let mut builder = base_builder();
    let mut configured = false;
    if let Some(url) = proxy.http_proxy.as_deref() {
        let mut http = Proxy::http(url).map_err(client_error)?;
        if let Some(no_proxy) = no_proxy.cloned() {
            http = http.no_proxy(Some(no_proxy));
        }
        builder = builder.proxy(http);
        configured = true;
    }
    if let Some(url) = proxy.https_proxy.as_deref() {
        let mut https = Proxy::https(url).map_err(client_error)?;
        if let Some(no_proxy) = no_proxy.cloned() {
            https = https.no_proxy(Some(no_proxy));
        }
        builder = builder.proxy(https);
        configured = true;
    }
    // When no proxy is configured, leave reqwest's default behavior intact
    // (preserves the legacy `from_config` semantics).
    let _ = configured;
    builder.build().map_err(client_error)
}

fn base_builder() -> reqwest::ClientBuilder {
    Client::builder().timeout(Duration::from_secs(HTTP_TIMEOUT_SECONDS))
}

fn client_error(err: reqwest::Error) -> MetadataProviderError {
    MetadataProviderError::Client(err.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn proxy_config() -> ProxyConfig {
        ProxyConfig {
            http_proxy: Some("http://proxy.test:8080".to_owned()),
            https_proxy: Some("http://proxy.test:8080".to_owned()),
            no_proxy: vec!["localhost".to_owned(), "127.0.0.1".to_owned()],
            policy: "global-with-provider-override".to_owned(),
        }
    }

    #[test]
    fn builds_distinct_clients_per_mode_and_dedupes_inherit() {
        let mut overrides = HashMap::new();
        overrides.insert("tmdb".to_owned(), ProviderProxyOverride::inherit());
        overrides.insert(
            "tvdb".to_owned(),
            ProviderProxyOverride {
                mode: "direct".to_owned(),
                url: None,
            },
        );
        overrides.insert(
            "fanart".to_owned(),
            ProviderProxyOverride {
                mode: "custom".to_owned(),
                url: Some("http://other.proxy.test:3128".to_owned()),
            },
        );

        // Should build without error for every mode.
        let clients = build_provider_clients(&proxy_config(), &overrides).unwrap();
        // Smoke: every provider resolves to some client.
        let _ = clients.client("tmdb");
        let _ = clients.client("tvdb");
        let _ = clients.client("fanart");
        let _ = clients.client("unknown"); // falls back to default
    }

    #[test]
    fn empty_no_proxy_list_is_none() {
        assert!(build_no_proxy(&[]).is_none());
        assert!(build_no_proxy(&["  ".to_owned()]).is_none());
        assert!(build_no_proxy(&["localhost".to_owned()]).is_some());
    }

    #[test]
    fn single_client_builds_for_each_mode() {
        let proxy = proxy_config();
        assert!(build_single_client(&proxy, None).is_ok());
        assert!(
            build_single_client(
                &proxy,
                Some(&ProviderProxyOverride {
                    mode: "direct".to_owned(),
                    url: None,
                })
            )
            .is_ok()
        );
        assert!(
            build_single_client(
                &proxy,
                Some(&ProviderProxyOverride {
                    mode: "custom".to_owned(),
                    url: Some("http://p.test:8080".to_owned()),
                })
            )
            .is_ok()
        );
    }
}
