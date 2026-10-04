//! Shared types and helper functions used across metadata providers.
//!
//! This module hosts the public API types (`MetadataLookup`, `MetadataMatch`,
//! `MetadataLookupReport`, …), the provider runtime context, the orchestration
//! outcome enums, and the generic normalization helpers that more than one
//! provider relies on. Provider-specific request/response structs live in their
//! own modules (`tmdb`, `tvdb`, `fanart`).

use std::collections::{BTreeSet, HashMap};

use reqwest::{Client, Url};
use serde::Serialize;

use crate::config::MetadataConfig;

// Shared bounds and field maps (moved verbatim from the old `provider.rs`).
pub const HTTP_TIMEOUT_SECONDS: u64 = 10;
pub const MAX_METADATA_CLASSIFICATION_ITEMS: usize = 128;
pub const MAX_METADATA_CLASSIFICATION_NAME_LEN: usize = 128;
pub const MAX_METADATA_PEOPLE_ITEMS: usize = 512;
pub const MAX_METADATA_PERSON_NAME_LEN: usize = 256;
pub const MAX_METADATA_PERSON_ROLE_NAME_LEN: usize = 128;
pub const MAX_METADATA_PERSON_SORT_ORDER: i32 = 1_000_000;
pub const MAX_METADATA_EXTERNAL_ID_LEN: usize = 128;
pub const TVDB_TOKEN_CACHE_SECONDS: u64 = 25 * 24 * 60 * 60;
pub const MAX_FANART_IMAGES: usize = 64;

/// Input to a metadata lookup: the item we want to match plus locale hints.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MetadataLookup {
    pub item_type: String,
    pub title: String,
    /// 另一语言标题（双标题命名时由识别层提供，provider 消歧用）。
    pub original_title: Option<String>,
    pub production_year: Option<i32>,
    /// 剧集季号（识别层填；`item_type == "episode"` 时 provider 据此查剧集元数据，design §6.3）。
    pub season: Option<i32>,
    /// 剧集集号（同上）。
    pub episode: Option<i32>,
    /// 显式 TMDB id（识别层从 `{tmdb-XXX}` 解析）。有值时 provider 直接按 id 拉详情、
    /// 跳过模糊标题搜索——零歧义、最准、最快，媒体中心刮削的首选路径。
    pub tmdb_id: Option<String>,
    /// 显式 IMDb id（`{imdb-ttXXX}`）。
    pub imdb_id: Option<String>,
    /// 显式 TVDB id（`{tvdb-XXX}`）。
    pub tvdb_id: Option<String>,
    /// Preferred text-metadata language (e.g. `zh-CN`).
    pub language: Option<String>,
    /// Preferred text-metadata region (e.g. `CN`).
    pub country: Option<String>,
    /// Preferred image language, independent of text `language` (§7). Falls
    /// back to `language` when unset.
    pub image_language: Option<String>,
    /// Prefer artwork in the item's original language over `image_language`.
    pub image_prefer_original: bool,
    /// Ordered image-language fallbacks. The special value `none`/`xx` selects
    /// textless artwork (TMDB empty `iso_639_1`).
    pub image_fallback_languages: Vec<String>,
}

impl MetadataLookup {
    /// The effective image language: explicit `image_language`, else the text
    /// `language`.
    pub fn effective_image_language(&self) -> Option<&str> {
        self.image_language.as_deref().or(self.language.as_deref())
    }
}

/// A resolved metadata match (base fields + artwork + classifications + people).
#[derive(Clone, Debug, PartialEq)]
pub struct MetadataMatch {
    pub provider: String,
    pub external_id: String,
    pub external_ids: Vec<MetadataExternalId>,
    pub title: String,
    /// 所属剧名（仅 episode：`title` 是单集名时，这里存剧名供 Emby `SeriesName`）。
    /// 非 episode 或无剧名时为 None。
    pub series_title: Option<String>,
    pub original_title: Option<String>,
    pub overview: Option<String>,
    pub production_year: Option<i32>,
    pub premiere_date: Option<String>,
    pub official_rating: Option<String>,
    pub community_rating: Option<f32>,
    pub artwork: Vec<MetadataArtwork>,
    pub genres: Vec<MetadataNamedValue>,
    pub studios: Vec<MetadataNamedValue>,
    /// 播出/发行平台（TMDB tv networks：Netflix / 爱奇艺 / Disney+ 等）。
    pub networks: Vec<MetadataNamedValue>,
    /// 主题曲 / 宣传片 / 预告等附属视频（TMDB videos）。
    pub videos: Vec<MetadataVideo>,
    /// 所属系列/合集（TMDB movie `belongs_to_collection`，如「变形金刚系列」）。仅电影。
    pub collection: Option<MetadataCollection>,
    pub people: Vec<MetadataPerson>,
}

/// 电影所属系列/合集（对应 collections 表）。
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MetadataCollection {
    pub name: String,
    pub name_normalized: String,
    pub overview: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MetadataArtwork {
    pub artwork_type: String,
    pub source: Option<String>,
    pub remote_url: String,
    pub is_primary: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MetadataExternalId {
    pub provider: String,
    pub external_id: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MetadataNamedValue {
    pub name: String,
    pub name_normalized: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MetadataPerson {
    pub name: String,
    pub name_normalized: String,
    pub role_type: String,
    pub role_name: String,
    pub sort_order: i32,
    pub profile_image_url: Option<String>,
}

/// 附属视频（主题曲 / 宣传片 / 预告等，对应 media_videos 表）。
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MetadataVideo {
    /// trailer / teaser / clip / featurette / behind_the_scenes / opening_theme / ending_theme / theme。
    pub video_type: String,
    pub name: Option<String>,
    /// 站点：youtube / vimeo / bilibili 等。
    pub site: Option<String>,
    /// 站点内视频 key（id）。
    pub site_key: Option<String>,
    /// 可直接打开的完整链接（由 site+key 拼出或 provider 给出）。
    pub url: Option<String>,
    pub is_official: bool,
    pub sort_order: i32,
}

/// The full report of a lookup: the matched item (if any) plus a per-provider
/// attempt trail. This shape is part of the public contract (it is serialized
/// into job metrics and plugin hook payloads) and must not change.
#[derive(Clone, Debug, PartialEq)]
pub struct MetadataLookupReport {
    pub matched: Option<MetadataMatch>,
    pub attempts: Vec<MetadataProviderAttempt>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MetadataProviderAttempt {
    pub provider: String,
    pub status: MetadataProviderAttemptStatus,
    pub message: Option<String>,
    pub external_id: Option<String>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum MetadataProviderAttemptStatus {
    Matched,
    NotMatched,
    Skipped,
    Failed,
}

impl MetadataProviderAttempt {
    pub fn matched(provider: impl Into<String>, external_id: impl Into<String>) -> Self {
        Self {
            provider: provider.into(),
            status: MetadataProviderAttemptStatus::Matched,
            message: None,
            external_id: Some(external_id.into()),
        }
    }

    pub fn not_matched(provider: impl Into<String>, message: impl Into<String>) -> Self {
        Self {
            provider: provider.into(),
            status: MetadataProviderAttemptStatus::NotMatched,
            message: Some(message.into()),
            external_id: None,
        }
    }

    pub fn skipped(provider: impl Into<String>, message: impl Into<String>) -> Self {
        Self {
            provider: provider.into(),
            status: MetadataProviderAttemptStatus::Skipped,
            message: Some(message.into()),
            external_id: None,
        }
    }

    pub fn failed(provider: impl Into<String>, message: impl Into<String>) -> Self {
        Self {
            provider: provider.into(),
            status: MetadataProviderAttemptStatus::Failed,
            message: Some(message.into()),
            external_id: None,
        }
    }
}

/// Error surface shared by every provider.
#[derive(Debug)]
pub enum MetadataProviderError {
    Client(String),
    Http(reqwest::Error),
}

impl std::fmt::Display for MetadataProviderError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Client(err) => write!(f, "metadata provider client error: {err}"),
            Self::Http(err) => write!(f, "metadata provider http error: {err}"),
        }
    }
}

impl std::error::Error for MetadataProviderError {}

/// Per-provider HTTP clients, keyed by provider id. Built by the proxy factory
/// so each provider can honor its own proxy mode (inherit/direct/custom) and the
/// global `no_proxy` list. Clients are deduped by effective proxy spec, so
/// providers sharing a spec share one client (cheap: `reqwest::Client` is `Arc`).
#[derive(Clone)]
pub struct ProviderClients {
    default: Client,
    by_provider: HashMap<String, Client>,
}

impl ProviderClients {
    pub fn new(default: Client, by_provider: HashMap<String, Client>) -> Self {
        Self {
            default,
            by_provider,
        }
    }

    /// A single shared client for every provider (phase 0 / test fallback).
    pub fn shared(client: Client) -> Self {
        Self {
            default: client,
            by_provider: HashMap::new(),
        }
    }

    /// The client for `provider_id`, falling back to the default (global) client.
    pub fn client(&self, provider_id: &str) -> &Client {
        self.by_provider.get(provider_id).unwrap_or(&self.default)
    }
}

/// Runtime context handed to each provider for a lookup.
///
/// Carries the proxy-aware per-provider HTTP clients plus the resolved metadata
/// config. Providers fetch their client via [`ProviderContext::client`].
#[derive(Clone)]
pub struct ProviderContext {
    pub clients: ProviderClients,
    pub metadata: MetadataConfig,
    /// provider_id -> API key 令牌池（可选）。配多 key 时 provider 从池轮转取 token、
    /// 遇 429 标记冷却；无池或单 key 时退化为 `metadata` 里的单 token。共享 `Arc` 以便
    /// registry 缓存复用时冷却状态延续。
    token_pools: std::collections::HashMap<String, std::sync::Arc<super::token_pool::TokenPool>>,
}

impl ProviderContext {
    pub fn new(clients: ProviderClients, metadata: MetadataConfig) -> Self {
        Self {
            clients,
            metadata,
            token_pools: std::collections::HashMap::new(),
        }
    }

    /// The HTTP client for the given provider id.
    pub fn client(&self, provider_id: &str) -> &Client {
        self.clients.client(provider_id)
    }

    /// 用令牌池替换当前上下文（registry 构造后注入）。
    pub fn with_token_pools(
        mut self,
        pools: std::collections::HashMap<String, std::sync::Arc<super::token_pool::TokenPool>>,
    ) -> Self {
        self.token_pools = pools;
        self
    }

    /// 取某 provider 的令牌池（若配置了多 key）。
    pub fn token_pool(
        &self,
        provider_id: &str,
    ) -> Option<&std::sync::Arc<super::token_pool::TokenPool>> {
        self.token_pools.get(provider_id)
    }
}

/// Outcome of a base-match attempt, rich enough for the registry to record the
/// right `MetadataProviderAttempt` without knowing anything provider-specific.
#[derive(Clone, Debug, PartialEq)]
pub enum ProviderMatchOutcome {
    Matched(Box<MetadataMatch>),
    NotMatched(String),
    Skipped(String),
}

/// Outcome of an enrichment attempt. `Matched` carries the external id that the
/// enrichment keyed off (used for the attempt trail); the match itself is
/// mutated in place.
#[derive(Clone, Debug, PartialEq)]
pub enum ProviderEnrichOutcome {
    Matched { external_id: String },
    NotMatched(String),
    Skipped(String),
}

// ---------------------------------------------------------------------------
// Generic normalization helpers (shared by more than one provider).
// ---------------------------------------------------------------------------

pub fn normalized_providers(providers: &[String]) -> Vec<String> {
    providers
        .iter()
        .map(|provider| provider.trim().to_ascii_lowercase())
        .filter(|provider| !provider.is_empty())
        .collect()
}

pub fn normalize_bounded_text(value: Option<&str>, max_len: usize) -> Option<String> {
    let value = value?.trim();
    (!value.is_empty() && value.len() <= max_len).then(|| value.to_owned())
}

pub fn normalize_optional_bounded_text(value: Option<&str>, max_len: usize) -> Option<String> {
    let value = value.unwrap_or_default().trim();
    (value.len() <= max_len).then(|| value.to_owned())
}

pub fn normalize_metadata_name(value: &str) -> String {
    value.trim().to_lowercase()
}

pub fn normalize_text_title(value: String) -> Option<String> {
    normalize_bounded_text(Some(value.as_str()), 512)
}

pub fn normalize_overview(value: String) -> Option<String> {
    normalize_bounded_text(Some(value.as_str()), 20_000)
}

pub fn normalize_language(value: &str) -> Option<String> {
    let value = value.trim().replace('_', "-");
    (!value.is_empty()
        && value.len() <= 16
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphabetic() || byte == b'-'))
    .then_some(value)
}

pub fn normalize_country(value: &str) -> Option<String> {
    let value = value.trim().to_ascii_uppercase();
    (value.len() == 2 && value.bytes().all(|byte| byte.is_ascii_uppercase())).then_some(value)
}

pub fn normalize_tmdb_date(value: String) -> Option<String> {
    let value = value.trim();
    let bytes = value.as_bytes();
    (bytes.len() == 10
        && bytes[0..4].iter().all(u8::is_ascii_digit)
        && bytes[4] == b'-'
        && bytes[5..7].iter().all(u8::is_ascii_digit)
        && bytes[7] == b'-'
        && bytes[8..10].iter().all(u8::is_ascii_digit))
    .then(|| value.to_owned())
}

pub fn normalize_external_id(_provider: &str, value: &str) -> Option<String> {
    normalize_bounded_text(Some(value), MAX_METADATA_EXTERNAL_ID_LEN)
}

pub fn bounded_sort_order(index: usize) -> i32 {
    i32::try_from(index)
        .unwrap_or(MAX_METADATA_PERSON_SORT_ORDER)
        .min(MAX_METADATA_PERSON_SORT_ORDER)
}

pub fn safe_remote_image_url(value: &str) -> Option<String> {
    let value = value.trim();
    if value.is_empty() || value.contains(char::is_whitespace) {
        return None;
    }
    let url = Url::parse(value).ok()?;
    if !matches!(url.scheme(), "http" | "https")
        || url.host_str().is_none()
        || !url.username().is_empty()
        || url.password().is_some()
    {
        return None;
    }

    Some(url.to_string())
}

/// Push a `(provider, external_id)` pair into a match's external-id list,
/// normalizing and de-duplicating. Used by both TMDB and TVDB.
pub fn push_metadata_external_id(
    external_ids: &mut Vec<MetadataExternalId>,
    provider: &str,
    external_id: String,
) {
    let provider = provider.trim().to_ascii_lowercase();
    let Some(external_id) = normalize_external_id(&provider, external_id.as_str()) else {
        return;
    };
    if external_ids
        .iter()
        .any(|value| value.provider == provider && value.external_id == external_id)
    {
        return;
    }

    external_ids.push(MetadataExternalId {
        provider,
        external_id,
    });
}

/// Look up the external id a match exposes for a given provider, preferring the
/// match's own provider id, falling back to its accumulated external ids.
pub fn metadata_external_id(found: &MetadataMatch, provider: &str) -> Option<String> {
    if found.provider.trim().eq_ignore_ascii_case(provider) {
        return normalize_external_id(provider, &found.external_id);
    }

    found
        .external_ids
        .iter()
        .find(|external_id| external_id.provider.trim().eq_ignore_ascii_case(provider))
        .and_then(|external_id| normalize_external_id(provider, &external_id.external_id))
}

/// De-duplicating builder for classification lists (genres, studios).
pub fn dedupe_named_values(
    values: impl Iterator<Item = Option<String>>,
) -> Vec<MetadataNamedValue> {
    let mut seen = BTreeSet::new();
    let mut output = Vec::new();
    for value in values {
        if output.len() >= MAX_METADATA_CLASSIFICATION_ITEMS {
            break;
        }
        let Some(name) =
            normalize_bounded_text(value.as_deref(), MAX_METADATA_CLASSIFICATION_NAME_LEN)
        else {
            continue;
        };
        let name_normalized = normalize_metadata_name(&name);
        if seen.insert(name_normalized.clone()) {
            output.push(MetadataNamedValue {
                name,
                name_normalized,
            });
        }
    }
    output
}

/// Image-language selection policy (§7), shared across providers that support
/// per-language artwork (TMDB `/images`, and reusable by others).
#[derive(Clone, Debug, Default)]
pub struct ImageLanguagePolicy {
    /// Item's original language, used when `prefer_original` is set.
    pub original_language: Option<String>,
    /// Preferred image language (already normalized to the primary subtag).
    pub image_language: Option<String>,
    /// Prefer the original language over `image_language`.
    pub prefer_original: bool,
    /// Ordered fallbacks (primary subtags; `none`/`xx` selects textless art).
    pub fallback_languages: Vec<String>,
}

/// Sentinel fallback values selecting textless artwork (no `iso_639_1`).
pub fn is_textless_token(value: &str) -> bool {
    matches!(
        value.trim().to_ascii_lowercase().as_str(),
        "none" | "xx" | ""
    )
}

/// Normalizes a language tag to its lowercase primary subtag (e.g. `zh-CN` →
/// `zh`), for language-vs-language comparison of artwork.
pub fn image_language_primary_subtag(value: &str) -> Option<String> {
    normalize_language(value).and_then(|language| {
        language
            .split('-')
            .next()
            .map(|subtag| subtag.to_ascii_lowercase())
            .filter(|subtag| !subtag.is_empty())
    })
}

/// Computes the selection rank for an artwork's language under the policy.
/// Lower is better; ties are broken by the caller (e.g. by vote average).
///
/// - `prefer_original` + match on original language → rank 0.
/// - match on `image_language` → rank 1.
/// - match on a `fallback_languages[i]` → rank `2 + i` (incl. textless tokens).
/// - everything else → a large sentinel (least preferred).
pub fn image_language_rank(policy: &ImageLanguagePolicy, artwork_language: Option<&str>) -> i32 {
    const UNRANKED: i32 = 10_000;
    let candidate = artwork_language
        .map(|value| value.trim().to_ascii_lowercase())
        .unwrap_or_default();
    let candidate_is_textless = candidate.is_empty();

    if policy.prefer_original {
        if let Some(original) = policy
            .original_language
            .as_deref()
            .and_then(image_language_primary_subtag)
        {
            if !candidate_is_textless && candidate == original {
                return 0;
            }
        }
    }

    if let Some(preferred) = policy
        .image_language
        .as_deref()
        .and_then(image_language_primary_subtag)
    {
        if !candidate_is_textless && candidate == preferred {
            return 1;
        }
    }

    for (index, fallback) in policy.fallback_languages.iter().enumerate() {
        let rank = 2 + index as i32;
        if is_textless_token(fallback) {
            if candidate_is_textless {
                return rank;
            }
        } else if let Some(fallback) = image_language_primary_subtag(fallback) {
            if !candidate_is_textless && candidate == fallback {
                return rank;
            }
        }
    }

    UNRANKED
}

#[cfg(test)]
mod tests {
    use super::*;

    fn policy(
        original: Option<&str>,
        image: Option<&str>,
        prefer_original: bool,
        fallbacks: &[&str],
    ) -> ImageLanguagePolicy {
        ImageLanguagePolicy {
            original_language: original.map(str::to_owned),
            image_language: image.map(str::to_owned),
            prefer_original,
            fallback_languages: fallbacks.iter().map(|value| value.to_string()).collect(),
        }
    }

    #[test]
    fn prefer_original_ranks_original_language_first() {
        let policy = policy(Some("ja"), Some("zh-CN"), true, &["en", "none"]);
        assert_eq!(image_language_rank(&policy, Some("ja")), 0);
        assert_eq!(image_language_rank(&policy, Some("zh")), 1);
        assert_eq!(image_language_rank(&policy, Some("en")), 2);
        assert_eq!(image_language_rank(&policy, None), 3); // textless via "none"
        assert_eq!(image_language_rank(&policy, Some("fr")), 10_000);
    }

    #[test]
    fn without_prefer_original_image_language_wins() {
        let policy = policy(Some("ja"), Some("zh-CN"), false, &["en"]);
        // Original no longer rank 0; zh-CN preferred wins.
        assert_eq!(image_language_rank(&policy, Some("ja")), 10_000);
        assert_eq!(image_language_rank(&policy, Some("zh")), 1);
        assert_eq!(image_language_rank(&policy, Some("en")), 2);
    }

    #[test]
    fn textless_token_matches_empty_iso() {
        let policy = policy(None, Some("zh"), false, &["xx"]);
        assert_eq!(image_language_rank(&policy, None), 2);
        assert_eq!(image_language_rank(&policy, Some("")), 2);
        // A textless artwork should not match a non-textless preferred language.
        assert_eq!(image_language_rank(&policy, Some("zh")), 1);
    }

    #[test]
    fn primary_subtag_normalization() {
        assert_eq!(
            image_language_primary_subtag("zh-CN").as_deref(),
            Some("zh")
        );
        assert_eq!(image_language_primary_subtag("EN").as_deref(), Some("en"));
        assert_eq!(image_language_primary_subtag(""), None);
    }

    #[test]
    fn effective_image_language_falls_back_to_text_language() {
        let lookup = MetadataLookup {
            item_type: "movie".to_owned(),
            title: "X".to_owned(),
            original_title: None,
            production_year: None,
            season: None,
            episode: None,
            tmdb_id: None,
            imdb_id: None,
            tvdb_id: None,
            language: Some("zh-CN".to_owned()),
            country: None,
            image_language: None,
            image_prefer_original: false,
            image_fallback_languages: Vec::new(),
        };
        assert_eq!(lookup.effective_image_language(), Some("zh-CN"));
    }
}
