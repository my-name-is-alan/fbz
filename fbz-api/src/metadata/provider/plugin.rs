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
        _ctx: &ProviderContext,
        _input: &MetadataLookup,
    ) -> Result<ProviderMatchOutcome, MetadataProviderError> {
        // Plugins augment an existing match; built-in providers own base match.
        Ok(ProviderMatchOutcome::Skipped(
            "plugin metadata providers are enrichment-only".to_owned(),
        ))
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

    #[test]
    fn adapter_id_is_namespaced() {
        let provider =
            PluginMetadataProvider::new("acme", std::sync::Arc::new(DisabledPluginQuerier));
        assert_eq!(provider.id(), "plugin:acme");
        assert_eq!(provider.role(), ProviderRole::Enrichment);
    }
}
