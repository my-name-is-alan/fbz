//! IMDb metadata provider.
//!
//! IMDb has no official public JSON API, so this provider is **enrichment-only**
//! (per the design §4.3): it operates on an already-matched item's IMDb external
//! id rather than searching. First version normalizes the imdb id to the
//! canonical `tt` prefix and reserves a point for future rating/certification
//! enrichment from a compliant data source.

use async_trait::async_trait;

use super::shared::*;
use super::{MetadataProvider, ProviderContext, ProviderRole};

/// IMDb enrichment provider. Stateless.
#[derive(Clone, Default)]
pub struct ImdbProvider;

impl ImdbProvider {
    pub fn new() -> Self {
        Self
    }
}

#[async_trait]
impl MetadataProvider for ImdbProvider {
    fn id(&self) -> &str {
        "imdb"
    }

    fn role(&self) -> ProviderRole {
        ProviderRole::Enrichment
    }

    fn supports(&self, item_type: &str) -> bool {
        matches!(item_type, "movie" | "series" | "season" | "episode")
    }

    async fn match_item(
        &self,
        _ctx: &ProviderContext,
        _input: &MetadataLookup,
    ) -> Result<ProviderMatchOutcome, MetadataProviderError> {
        // IMDb cannot find a match from scratch (no public search API).
        Ok(ProviderMatchOutcome::Skipped(
            "imdb is enrichment-only".to_owned(),
        ))
    }

    async fn enrich(
        &self,
        _ctx: &ProviderContext,
        _input: &MetadataLookup,
        current: &mut MetadataMatch,
    ) -> Result<ProviderEnrichOutcome, MetadataProviderError> {
        let Some(raw) = imdb_external_id(current) else {
            return Ok(ProviderEnrichOutcome::Skipped(
                "no imdb external id to normalize".to_owned(),
            ));
        };
        let Some(canonical) = normalize_imdb_id(&raw) else {
            return Ok(ProviderEnrichOutcome::NotMatched(
                "imdb external id is not a valid title id".to_owned(),
            ));
        };

        let changed = upsert_imdb_external_id(current, &canonical);
        // Future enrichment (ratings, certifications) plugs in here once a
        // compliant data source is chosen (design §12 risk note).
        if changed {
            Ok(ProviderEnrichOutcome::Matched {
                external_id: canonical,
            })
        } else {
            Ok(ProviderEnrichOutcome::NotMatched(
                "imdb external id already canonical".to_owned(),
            ))
        }
    }
}

/// Reads the current match's raw imdb external id, if any.
fn imdb_external_id(found: &MetadataMatch) -> Option<String> {
    metadata_external_id(found, "imdb")
}

/// Normalizes an IMDb title id to the canonical `tt#######` form. Accepts a
/// bare numeric id (zero-padded to 7 digits) or an existing `tt`-prefixed id.
/// Returns `None` for anything that isn't a title id.
fn normalize_imdb_id(value: &str) -> Option<String> {
    let trimmed = value.trim();
    let digits = trimmed.strip_prefix("tt").unwrap_or(trimmed);
    if digits.is_empty() || !digits.bytes().all(|byte| byte.is_ascii_digit()) {
        return None;
    }
    // IMDb title ids are at least 7 digits, zero-padded.
    let normalized = if digits.len() < 7 {
        format!("tt{digits:0>7}")
    } else {
        format!("tt{digits}")
    };
    Some(normalized)
}

/// Replaces the imdb external id with its canonical form. Returns whether a
/// change was made (also true when adding a missing entry).
fn upsert_imdb_external_id(found: &mut MetadataMatch, canonical: &str) -> bool {
    if found.provider.trim().eq_ignore_ascii_case("imdb") {
        if found.external_id == canonical {
            return false;
        }
        found.external_id = canonical.to_owned();
        return true;
    }

    if let Some(existing) = found
        .external_ids
        .iter_mut()
        .find(|external_id| external_id.provider.trim().eq_ignore_ascii_case("imdb"))
    {
        if existing.external_id == canonical {
            return false;
        }
        existing.external_id = canonical.to_owned();
        return true;
    }

    push_metadata_external_id(&mut found.external_ids, "imdb", canonical.to_owned());
    true
}

#[cfg(test)]
mod tests {
    use super::*;

    fn match_with_imdb(provider: &str, id: &str) -> MetadataMatch {
        MetadataMatch {
            provider: provider.to_owned(),
            external_id: if provider == "imdb" {
                id.to_owned()
            } else {
                "999".to_owned()
            },
            external_ids: if provider == "imdb" {
                Vec::new()
            } else {
                vec![MetadataExternalId {
                    provider: "imdb".to_owned(),
                    external_id: id.to_owned(),
                }]
            },
            title: "X".to_owned(),
            series_title: None,
            original_title: None,
            overview: None,
            production_year: None,
            premiere_date: None,
            official_rating: None,
            community_rating: None,
            artwork: Vec::new(),
            genres: Vec::new(),
            studios: Vec::new(),
            networks: Vec::new(),
            videos: Vec::new(),
            collection: None,
            people: Vec::new(),
        }
    }

    #[test]
    fn normalizes_bare_and_prefixed_ids() {
        assert_eq!(normalize_imdb_id("tt0944947").as_deref(), Some("tt0944947"));
        assert_eq!(normalize_imdb_id("944947").as_deref(), Some("tt0944947"));
        assert_eq!(normalize_imdb_id(" 123 ").as_deref(), Some("tt0000123"));
        assert_eq!(
            normalize_imdb_id("tt12345678").as_deref(),
            Some("tt12345678")
        );
        assert_eq!(normalize_imdb_id("nm123"), None);
        assert_eq!(normalize_imdb_id("abc"), None);
        assert_eq!(normalize_imdb_id(""), None);
    }

    #[test]
    fn upsert_canonicalizes_in_external_ids_list() {
        let mut found = match_with_imdb("tmdb", "123");
        assert!(upsert_imdb_external_id(&mut found, "tt0000123"));
        assert_eq!(found.external_ids[0].external_id, "tt0000123");
        // Idempotent second pass.
        assert!(!upsert_imdb_external_id(&mut found, "tt0000123"));
    }

    #[test]
    fn upsert_canonicalizes_primary_provider_id() {
        let mut found = match_with_imdb("imdb", "123");
        assert!(upsert_imdb_external_id(&mut found, "tt0000123"));
        assert_eq!(found.external_id, "tt0000123");
    }
}
