//! Open plugin metadata-provider contract (design §8, foundation layer).
//!
//! Lets a third-party plugin participate in the scraping pipeline as a
//! [`MetadataProvider`]. A plugin returns a [`PluginMetadataContribution`] (a
//! subset of the `metadata.write` field whitelist); [`merge_plugin_metadata`]
//! folds it into the current match under the §9 priority rules:
//!
//! - Built-in providers win: a plugin only fills **empty** base fields.
//! - External ids accumulate (de-duplicated by provider).
//! - Artwork is appended under a `plugin:{id}` source namespace, never
//!   replacing built-in artwork.
//! - genres/studios/people: only filled when the current match has none.
//!
//! The actual synchronous plugin invocation is isolated behind the
//! [`PluginMetadataQuerier`] trait so the security-sensitive HTTP execution path
//! can be wired in later without touching the (unit-tested) merge logic. The
//! default querier returns `None` — no plugin is called until that path lands.

use async_trait::async_trait;

use super::shared::*;
use super::{MetadataProvider, ProviderContext, ProviderRole};

/// A plugin's metadata contribution: the subset of fields a plugin may supply.
/// Mirrors the `metadata.write` whitelist shape (title/overview/ids/artwork/…).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct PluginMetadataContribution {
    pub title: Option<String>,
    pub original_title: Option<String>,
    pub overview: Option<String>,
    pub production_year: Option<i32>,
    pub premiere_date: Option<String>,
    pub official_rating: Option<String>,
    pub community_rating: Option<f32>,
    pub external_ids: Vec<MetadataExternalId>,
    pub artwork: Vec<MetadataArtwork>,
    pub genres: Vec<MetadataNamedValue>,
    pub studios: Vec<MetadataNamedValue>,
    pub people: Vec<MetadataPerson>,
}

/// Synchronous plugin querier seam. Implementors invoke a plugin (HTTP runtime,
/// bounded timeout) and return its contribution. The default returns `None`.
#[async_trait]
pub trait PluginMetadataQuerier: Send + Sync {
    /// Queries the plugin for a contribution to `current`. `None` means the
    /// plugin declined / produced nothing usable.
    async fn query(
        &self,
        ctx: &ProviderContext,
        input: &MetadataLookup,
        current: Option<&MetadataMatch>,
    ) -> Result<Option<PluginMetadataContribution>, MetadataProviderError>;
}

/// A querier that never calls out — the foundation default until the
/// synchronous HTTP execution path is wired in.
#[derive(Clone, Default)]
pub struct DisabledPluginQuerier;

#[async_trait]
impl PluginMetadataQuerier for DisabledPluginQuerier {
    async fn query(
        &self,
        _ctx: &ProviderContext,
        _input: &MetadataLookup,
        _current: Option<&MetadataMatch>,
    ) -> Result<Option<PluginMetadataContribution>, MetadataProviderError> {
        Ok(None)
    }
}

/// Adapts a plugin into a [`MetadataProvider`]. Enrichment-role: built-in
/// base-match providers run first, and the plugin only augments the result.
pub struct PluginMetadataProvider {
    id: String,
    querier: std::sync::Arc<dyn PluginMetadataQuerier>,
}

impl PluginMetadataProvider {
    /// Builds an adapter for plugin `plugin_id` (the registry id becomes
    /// `plugin:{plugin_id}`).
    pub fn new(plugin_id: &str, querier: std::sync::Arc<dyn PluginMetadataQuerier>) -> Self {
        Self {
            id: format!("plugin:{}", plugin_id.trim()),
            querier,
        }
    }
}

#[async_trait]
impl MetadataProvider for PluginMetadataProvider {
    fn id(&self) -> &str {
        &self.id
    }

    fn role(&self) -> ProviderRole {
        ProviderRole::Enrichment
    }

    fn supports(&self, _item_type: &str) -> bool {
        true
    }

    async fn match_item(
        &self,
        ctx: &ProviderContext,
        input: &MetadataLookup,
    ) -> Result<ProviderMatchOutcome, MetadataProviderError> {
        // 兜底 base match：registry 只在全部内置 BaseMatch provider 未命中后
        // 才调用到这里（内置优先原则不变），插件此时可以用自己的数据源兜底。
        match self.querier.query(ctx, input, None).await? {
            Some(contribution) => match contribution_to_base_match(&self.id, input, contribution) {
                Some(found) => Ok(ProviderMatchOutcome::Matched(Box::new(found))),
                None => Ok(ProviderMatchOutcome::NotMatched(
                    "plugin contribution lacks a usable title".to_owned(),
                )),
            },
            None => Ok(ProviderMatchOutcome::NotMatched(
                "plugin returned no metadata".to_owned(),
            )),
        }
    }

    async fn enrich(
        &self,
        ctx: &ProviderContext,
        input: &MetadataLookup,
        current: &mut MetadataMatch,
    ) -> Result<ProviderEnrichOutcome, MetadataProviderError> {
        match self.querier.query(ctx, input, Some(current)).await? {
            Some(contribution) => {
                let changed = merge_plugin_metadata(current, &self.id, contribution);
                if changed {
                    Ok(ProviderEnrichOutcome::Matched {
                        external_id: current.external_id.clone(),
                    })
                } else {
                    Ok(ProviderEnrichOutcome::NotMatched(
                        "plugin contribution added nothing new".to_owned(),
                    ))
                }
            }
            None => Ok(ProviderEnrichOutcome::NotMatched(
                "plugin returned no metadata".to_owned(),
            )),
        }
    }
}

/// Builds a base [`MetadataMatch`] from a plugin contribution（兜底 base match，
/// 仅当内置 provider 全部未命中时由 registry 调用）。要求贡献里有可用标题，
/// 否则不成立。artwork 全部落在 `plugin:{id}` scoped source 下。
///
/// 插件未提供外部 ID 时，用 lookup 上下文合成一个**条目级唯一**的兜底 ID：
/// `media_external_ids` 有全局 `(provider, external_id)` 唯一约束，纯标题
/// 兜底会让同名的剧/分集互相冲突并打穿写入。
pub fn contribution_to_base_match(
    source_id: &str,
    input: &MetadataLookup,
    contribution: PluginMetadataContribution,
) -> Option<MetadataMatch> {
    let title = contribution.title.and_then(normalize_text_title)?;
    let external_id = contribution
        .external_ids
        .first()
        .map(|id| id.external_id.clone())
        .unwrap_or_else(|| synthesized_external_id(&title, input));

    let mut external_ids = Vec::new();
    for id in contribution.external_ids {
        push_metadata_external_id(&mut external_ids, &id.provider, id.external_id);
    }

    let artwork = contribution
        .artwork
        .into_iter()
        .filter(|art| safe_remote_image_url(&art.remote_url).is_some())
        .map(|mut art| {
            art.source = Some(source_id.to_owned());
            art
        })
        .collect();

    Some(MetadataMatch {
        provider: source_id.to_owned(),
        external_id,
        external_ids,
        title,
        series_title: None,
        original_title: contribution.original_title.and_then(normalize_text_title),
        overview: contribution.overview.and_then(normalize_overview),
        production_year: contribution.production_year,
        premiere_date: contribution.premiere_date.and_then(normalize_tmdb_date),
        official_rating: normalize_bounded_text(contribution.official_rating.as_deref(), 64),
        community_rating: contribution
            .community_rating
            .map(|rating| rating.clamp(0.0, 10.0)),
        artwork,
        genres: contribution.genres,
        studios: contribution.studios,
        networks: Vec::new(),
        videos: Vec::new(),
        collection: None,
        people: contribution.people,
    })
}

/// 合成条目级唯一的兜底外部 ID：标题 + 条目类型 + 年份 + 季集号。
/// 同一 fixture/数据源命中多个逻辑条目（剧 + 各分集）时不会互相冲突。
fn synthesized_external_id(title: &str, input: &MetadataLookup) -> String {
    let mut id = format!("{}#{}", title, input.item_type.trim());
    if let Some(year) = input.production_year {
        id.push_str(&format!("#{year}"));
    }
    match (input.season, input.episode) {
        (Some(season), Some(episode)) => id.push_str(&format!("#s{season:02}e{episode:02}")),
        (Some(season), None) => id.push_str(&format!("#s{season:02}")),
        _ => {}
    }
    id
}

/// Folds a plugin contribution into `current` under the §9 priority rules.
/// Returns whether anything changed. `source_id` is the `plugin:{id}` namespace
/// used to scope appended artwork.
pub fn merge_plugin_metadata(
    current: &mut MetadataMatch,
    source_id: &str,
    contribution: PluginMetadataContribution,
) -> bool {
    let mut changed = false;

    // Base fields: only fill when the built-in match left them empty.
    if current.original_title.is_none() {
        if let Some(value) = contribution.original_title.and_then(normalize_text_title) {
            current.original_title = Some(value);
            changed = true;
        }
    }
    if current.overview.is_none() {
        if let Some(value) = contribution.overview.and_then(normalize_overview) {
            current.overview = Some(value);
            changed = true;
        }
    }
    if current.production_year.is_none() {
        if let Some(year) = contribution.production_year {
            current.production_year = Some(year);
            changed = true;
        }
    }
    if current.premiere_date.is_none() {
        if let Some(date) = contribution.premiere_date.and_then(normalize_tmdb_date) {
            current.premiere_date = Some(date);
            changed = true;
        }
    }
    if current.official_rating.is_none() {
        if let Some(rating) = normalize_bounded_text(contribution.official_rating.as_deref(), 64) {
            current.official_rating = Some(rating);
            changed = true;
        }
    }
    if current.community_rating.is_none() {
        if let Some(rating) = contribution.community_rating {
            current.community_rating = Some(rating.clamp(0.0, 10.0));
            changed = true;
        }
    }
    // Title: only when the current title is empty (built-in title wins).
    if current.title.trim().is_empty() {
        if let Some(title) = contribution.title.and_then(normalize_text_title) {
            current.title = title;
            changed = true;
        }
    }

    // External ids accumulate (de-duplicated by provider+id).
    for external_id in contribution.external_ids {
        let before = current.external_ids.len();
        push_metadata_external_id(
            &mut current.external_ids,
            &external_id.provider,
            external_id.external_id,
        );
        if current.external_ids.len() != before {
            changed = true;
        }
    }

    // Artwork appended under the plugin's scoped source, never replacing.
    for mut artwork in contribution.artwork {
        if safe_remote_image_url(&artwork.remote_url).is_none() {
            continue;
        }
        artwork.source = Some(source_id.to_owned());
        artwork.is_primary = false;
        current.artwork.push(artwork);
        changed = true;
    }

    // genres/studios/people: only when the built-in match supplied none.
    if current.genres.is_empty() && !contribution.genres.is_empty() {
        current.genres = contribution.genres;
        changed = true;
    }
    if current.studios.is_empty() && !contribution.studios.is_empty() {
        current.studios = contribution.studios;
        changed = true;
    }
    if current.people.is_empty() && !contribution.people.is_empty() {
        current.people = contribution.people;
        changed = true;
    }

    changed
}

#[cfg(test)]
mod tests {
    use super::*;

    fn base_match() -> MetadataMatch {
        MetadataMatch {
            provider: "tmdb".to_owned(),
            external_id: "42".to_owned(),
            external_ids: Vec::new(),
            title: "Built-in Title".to_owned(),
            series_title: None,
            original_title: None,
            overview: None,
            production_year: Some(2020),
            premiere_date: None,
            official_rating: None,
            community_rating: None,
            artwork: vec![MetadataArtwork {
                artwork_type: "poster".to_owned(),
                source: None,
                remote_url: "https://img.test/builtin.jpg".to_owned(),
                is_primary: true,
            }],
            genres: vec![MetadataNamedValue {
                name: "Drama".to_owned(),
                name_normalized: "drama".to_owned(),
            }],
            studios: Vec::new(),
            networks: Vec::new(),
            videos: Vec::new(),
            collection: None,
            people: Vec::new(),
        }
    }

    #[test]
    fn plugin_only_fills_empty_base_fields() {
        let mut found = base_match();
        let contribution = PluginMetadataContribution {
            title: Some("Plugin Title".to_owned()),
            overview: Some("Plugin overview".to_owned()),
            production_year: Some(1999),
            ..PluginMetadataContribution::default()
        };

        let changed = merge_plugin_metadata(&mut found, "plugin:acme", contribution);
        assert!(changed);
        // Built-in title and year preserved.
        assert_eq!(found.title, "Built-in Title");
        assert_eq!(found.production_year, Some(2020));
        // Empty overview filled by plugin.
        assert_eq!(found.overview.as_deref(), Some("Plugin overview"));
    }

    #[test]
    fn plugin_artwork_is_scoped_and_appended() {
        let mut found = base_match();
        let contribution = PluginMetadataContribution {
            artwork: vec![
                MetadataArtwork {
                    artwork_type: "poster".to_owned(),
                    source: Some("ignored".to_owned()),
                    remote_url: "https://img.test/plugin.jpg".to_owned(),
                    is_primary: true,
                },
                MetadataArtwork {
                    artwork_type: "poster".to_owned(),
                    source: None,
                    remote_url: "not a url".to_owned(),
                    is_primary: true,
                },
            ],
            ..PluginMetadataContribution::default()
        };

        merge_plugin_metadata(&mut found, "plugin:acme", contribution);
        // Built-in poster kept + one valid plugin poster appended (bad url dropped).
        assert_eq!(found.artwork.len(), 2);
        let plugin_art = &found.artwork[1];
        assert_eq!(plugin_art.source.as_deref(), Some("plugin:acme"));
        assert!(!plugin_art.is_primary);
        assert_eq!(plugin_art.remote_url, "https://img.test/plugin.jpg");
    }

    #[test]
    fn plugin_does_not_replace_existing_genres_but_fills_studios() {
        let mut found = base_match();
        let contribution = PluginMetadataContribution {
            genres: vec![MetadataNamedValue {
                name: "Sci-Fi".to_owned(),
                name_normalized: "sci-fi".to_owned(),
            }],
            studios: vec![MetadataNamedValue {
                name: "Acme".to_owned(),
                name_normalized: "acme".to_owned(),
            }],
            ..PluginMetadataContribution::default()
        };

        merge_plugin_metadata(&mut found, "plugin:acme", contribution);
        // Genres unchanged (built-in had some); studios filled (built-in empty).
        assert_eq!(found.genres.len(), 1);
        assert_eq!(found.genres[0].name, "Drama");
        assert_eq!(found.studios.len(), 1);
        assert_eq!(found.studios[0].name, "Acme");
    }

    #[test]
    fn external_ids_accumulate_and_dedup() {
        let mut found = base_match();
        let contribution = PluginMetadataContribution {
            external_ids: vec![
                MetadataExternalId {
                    provider: "imdb".to_owned(),
                    external_id: "tt0000001".to_owned(),
                },
                MetadataExternalId {
                    provider: "imdb".to_owned(),
                    external_id: "tt0000001".to_owned(),
                },
            ],
            ..PluginMetadataContribution::default()
        };

        let changed = merge_plugin_metadata(&mut found, "plugin:acme", contribution);
        assert!(changed);
        assert_eq!(found.external_ids.len(), 1);
        assert_eq!(found.external_ids[0].provider, "imdb");
    }

    #[test]
    fn no_op_contribution_reports_unchanged() {
        let mut found = base_match();
        let changed = merge_plugin_metadata(
            &mut found,
            "plugin:acme",
            PluginMetadataContribution::default(),
        );
        assert!(!changed);
    }

    fn lookup(item_type: &str, season: Option<i32>, episode: Option<i32>) -> MetadataLookup {
        MetadataLookup {
            item_type: item_type.to_owned(),
            title: "Buck Bunny TV".to_owned(),
            original_title: None,
            production_year: None,
            season,
            episode,
            tmdb_id: None,
            imdb_id: None,
            tvdb_id: None,
            language: None,
            country: None,
            image_language: None,
            image_prefer_original: false,
            image_fallback_languages: Vec::new(),
        }
    }

    #[test]
    fn synthesized_fallback_ids_are_unique_per_logical_item() {
        // media_external_ids 有全局 (provider, external_id) 唯一约束：
        // 同一数据源命中剧 + 各分集时，兜底 ID 必须互不相同。
        let contribution = || PluginMetadataContribution {
            title: Some("Buck Bunny TV".to_owned()),
            ..PluginMetadataContribution::default()
        };
        let series = contribution_to_base_match(
            "plugin:demo",
            &lookup("series", None, None),
            contribution(),
        )
        .unwrap();
        let e1 = contribution_to_base_match(
            "plugin:demo",
            &lookup("episode", Some(1), Some(1)),
            contribution(),
        )
        .unwrap();
        let e2 = contribution_to_base_match(
            "plugin:demo",
            &lookup("episode", Some(1), Some(2)),
            contribution(),
        )
        .unwrap();

        assert_ne!(series.external_id, e1.external_id);
        assert_ne!(e1.external_id, e2.external_id);
        // 插件显式给了外部 ID 时优先使用。
        let explicit = contribution_to_base_match(
            "plugin:demo",
            &lookup("movie", None, None),
            PluginMetadataContribution {
                title: Some("Buck Bunny TV".to_owned()),
                external_ids: vec![MetadataExternalId {
                    provider: "imdb".to_owned(),
                    external_id: "tt1254207".to_owned(),
                }],
                ..PluginMetadataContribution::default()
            },
        )
        .unwrap();
        assert_eq!(explicit.external_id, "tt1254207");
    }

    #[test]
    fn adapter_id_is_namespaced() {
        let provider =
            PluginMetadataProvider::new("acme", std::sync::Arc::new(DisabledPluginQuerier));
        assert_eq!(provider.id(), "plugin:acme");
        assert_eq!(provider.role(), ProviderRole::Enrichment);
    }
}
