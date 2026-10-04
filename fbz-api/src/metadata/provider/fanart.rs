//! Fanart metadata provider.
//!
//! Enrichment-only provider: requires an existing base match (TMDB id for
//! movies, TVDB id for TV) and adds localized artwork (poster, backdrop, logo,
//! thumb, banner, disc). Migrated from the legacy `provider.rs` with no
//! behavioral change.

use std::collections::BTreeSet;

use async_trait::async_trait;
use serde_json::Value;

use super::shared::*;
use super::{MetadataProvider, ProviderContext, ProviderRole};

const FANART_MOVIE_FIELDS: &[(&str, &str)] = &[
    ("movieposter", "poster"),
    ("moviebackground", "backdrop"),
    ("hdmovielogo", "logo"),
    ("movielogo", "logo"),
    ("moviethumb", "thumb"),
    ("moviebanner", "banner"),
    ("moviedisc", "disc"),
    ("hdmovieclearart", "thumb"),
    ("movieart", "thumb"),
];
const FANART_TV_FIELDS: &[(&str, &str)] = &[
    ("tvposter", "poster"),
    ("seasonposter", "poster"),
    ("showbackground", "backdrop"),
    ("hdtvlogo", "logo"),
    ("clearlogo", "logo"),
    ("tvthumb", "thumb"),
    ("seasonthumb", "thumb"),
    ("tvbanner", "banner"),
    ("seasonbanner", "banner"),
    ("hdclearart", "thumb"),
    ("characterart", "thumb"),
];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FanartKind {
    Movie,
    Tv,
}

impl FanartKind {
    fn as_path(self) -> &'static str {
        match self {
            Self::Movie => "movies",
            Self::Tv => "tv",
        }
    }

    fn required_provider(self) -> &'static str {
        match self {
            Self::Movie => "tmdb",
            Self::Tv => "tvdb",
        }
    }

    fn fields(self) -> &'static [(&'static str, &'static str)] {
        match self {
            Self::Movie => FANART_MOVIE_FIELDS,
            Self::Tv => FANART_TV_FIELDS,
        }
    }
}

/// Fanart enrichment provider. Stateless.
#[derive(Clone, Default)]
pub struct FanartProvider;

impl FanartProvider {
    pub fn new() -> Self {
        Self
    }

    async fn search_fanart(
        &self,
        ctx: &ProviderContext,
        input: &MetadataLookup,
        found: &MetadataMatch,
        token: &str,
    ) -> Result<FanartSearchOutcome, MetadataProviderError> {
        let Some(kind) = fanart_kind(&input.item_type) else {
            return Ok(FanartSearchOutcome::Skipped(format!(
                "unsupported item type `{}`",
                input.item_type
            )));
        };
        let Some(external_id) = fanart_external_id(found, kind) else {
            return Ok(FanartSearchOutcome::Skipped(format!(
                "missing {} external id",
                kind.required_provider()
            )));
        };

        let response = ctx
            .client("fanart")
            .get(fanart_url(
                &ctx.metadata.fanart_api_base_url,
                kind,
                &external_id,
            ))
            .query(&[("api_key", token)])
            .send()
            .await
            .map_err(MetadataProviderError::Http)?
            .error_for_status()
            .map_err(MetadataProviderError::Http)?
            .json::<Value>()
            .await
            .map_err(MetadataProviderError::Http)?;

        let artwork = fanart_artwork(response, kind, input.language.as_deref());
        if artwork.is_empty() {
            return Ok(FanartSearchOutcome::NotMatched(
                "no supported Fanart artwork".to_owned(),
            ));
        }

        Ok(FanartSearchOutcome::Matched {
            external_id,
            artwork,
        })
    }
}

#[async_trait]
impl MetadataProvider for FanartProvider {
    fn id(&self) -> &str {
        "fanart"
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
        // Enrichment-only: cannot be used as a base match.
        Ok(ProviderMatchOutcome::Skipped(
            "fanart is enrichment-only".to_owned(),
        ))
    }

    async fn enrich(
        &self,
        ctx: &ProviderContext,
        input: &MetadataLookup,
        current: &mut MetadataMatch,
    ) -> Result<ProviderEnrichOutcome, MetadataProviderError> {
        let Some(token) = ctx.metadata.fanart_api_key.as_deref() else {
            return Ok(ProviderEnrichOutcome::Skipped(
                "missing Fanart API key".to_owned(),
            ));
        };

        match self.search_fanart(ctx, input, current, token).await? {
            FanartSearchOutcome::Matched {
                external_id,
                artwork,
            } => {
                current.artwork.extend(artwork);
                Ok(ProviderEnrichOutcome::Matched { external_id })
            }
            FanartSearchOutcome::NotMatched(message) => {
                Ok(ProviderEnrichOutcome::NotMatched(message))
            }
            FanartSearchOutcome::Skipped(message) => Ok(ProviderEnrichOutcome::Skipped(message)),
        }
    }
}

// ---------------------------------------------------------------------------
// Fanart-specific helpers (migrated verbatim from provider.rs).
// ---------------------------------------------------------------------------

#[derive(Clone, Debug, PartialEq)]
enum FanartSearchOutcome {
    Matched {
        external_id: String,
        artwork: Vec<MetadataArtwork>,
    },
    NotMatched(String),
    Skipped(String),
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct FanartCandidate {
    url: String,
    language_rank: i32,
    likes: i32,
}

fn fanart_url(base_url: &str, kind: FanartKind, external_id: &str) -> String {
    format!(
        "{}/{}/{}",
        base_url.trim_end_matches('/'),
        kind.as_path(),
        external_id.trim()
    )
}

fn fanart_kind(item_type: &str) -> Option<FanartKind> {
    match item_type {
        "movie" => Some(FanartKind::Movie),
        "series" | "season" | "episode" => Some(FanartKind::Tv),
        _ => None,
    }
}

fn fanart_external_id(found: &MetadataMatch, kind: FanartKind) -> Option<String> {
    metadata_external_id(found, kind.required_provider())
}

fn fanart_artwork(
    response: Value,
    kind: FanartKind,
    language: Option<&str>,
) -> Vec<MetadataArtwork> {
    let Some(response) = response.as_object() else {
        return Vec::new();
    };
    let preferred_language = language.and_then(fanart_preferred_language);
    let mut seen_urls = BTreeSet::new();
    let mut primary_types = BTreeSet::new();
    let mut artwork = Vec::new();

    for (field, artwork_type) in kind.fields() {
        let Some(values) = response.get(*field).and_then(Value::as_array) else {
            continue;
        };
        let mut candidates = fanart_candidates(values, preferred_language.as_deref());
        for candidate in candidates.drain(..) {
            if artwork.len() >= MAX_FANART_IMAGES {
                return artwork;
            }
            let Some(remote_url) = fanart_image_url(candidate.url.as_str()) else {
                continue;
            };
            if !seen_urls.insert(remote_url.clone()) {
                continue;
            }
            let is_primary = primary_types.insert((*artwork_type).to_owned());
            artwork.push(MetadataArtwork {
                artwork_type: (*artwork_type).to_owned(),
                source: Some("fanart".to_owned()),
                remote_url,
                is_primary,
            });
        }
    }

    artwork
}

fn fanart_candidates(values: &[Value], preferred_language: Option<&str>) -> Vec<FanartCandidate> {
    let mut candidates = values
        .iter()
        .filter_map(|value| {
            let url = value.get("url").and_then(Value::as_str)?.trim();
            (!url.is_empty()).then(|| FanartCandidate {
                url: url.to_owned(),
                language_rank: fanart_language_rank(
                    value.get("lang").and_then(Value::as_str),
                    preferred_language,
                ),
                likes: fanart_likes(value.get("likes").and_then(Value::as_str)),
            })
        })
        .collect::<Vec<_>>();

    candidates.sort_by(|left, right| {
        left.language_rank
            .cmp(&right.language_rank)
            .then_with(|| right.likes.cmp(&left.likes))
            .then_with(|| left.url.cmp(&right.url))
    });
    candidates
}

fn fanart_image_url(value: &str) -> Option<String> {
    safe_remote_image_url(value)
}

fn fanart_preferred_language(value: &str) -> Option<String> {
    normalize_language(value).and_then(|language| {
        language
            .split('-')
            .next()
            .map(|language| language.to_ascii_lowercase())
            .filter(|language| !language.is_empty())
    })
}

fn fanart_language_rank(language: Option<&str>, preferred_language: Option<&str>) -> i32 {
    let language = language.unwrap_or_default().trim().to_ascii_lowercase();
    if preferred_language.is_some_and(|preferred| language == preferred) {
        return 0;
    }
    match language.as_str() {
        "en" => 1,
        "" | "00" => 2,
        _ => 3,
    }
}

fn fanart_likes(value: Option<&str>) -> i32 {
    value
        .and_then(|value| value.trim().parse::<i32>().ok())
        .unwrap_or_default()
        .max(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fanart_url_uses_configurable_base_url() {
        assert_eq!(
            fanart_url("https://fanart.example.test/v3/", FanartKind::Movie, "42"),
            "https://fanart.example.test/v3/movies/42"
        );
        assert_eq!(
            fanart_url("https://fanart.example.test/v3", FanartKind::Tv, "121361"),
            "https://fanart.example.test/v3/tv/121361"
        );
    }

    #[test]
    fn fanart_artwork_maps_supported_fields_and_prefers_requested_language() {
        let artwork = fanart_artwork(
            serde_json::json!({
                "movieposter": [
                    {"url": "https://assets.fanart.tv/fanart/movies/42/movieposter/en.jpg", "lang": "en", "likes": "9"},
                    {"url": "https://assets.fanart.tv/fanart/movies/42/movieposter/zh.jpg", "lang": "zh", "likes": "1"},
                    {"url": "ftp://assets.fanart.tv/bad.jpg", "lang": "zh", "likes": "99"}
                ],
                "moviebackground": [
                    {"url": "https://assets.fanart.tv/fanart/movies/42/moviebackground/bg.jpg", "lang": "00", "likes": "3"}
                ],
                "unsupported": [
                    {"url": "https://assets.fanart.tv/fanart/movies/42/other/ignored.jpg", "lang": "zh", "likes": "100"}
                ]
            }),
            FanartKind::Movie,
            Some("zh-CN"),
        );

        assert_eq!(artwork.len(), 3);
        assert_eq!(artwork[0].artwork_type, "poster");
        assert_eq!(
            artwork[0].remote_url,
            "https://assets.fanart.tv/fanart/movies/42/movieposter/zh.jpg"
        );
        assert_eq!(artwork[0].source.as_deref(), Some("fanart"));
        assert!(artwork[0].is_primary);
        assert_eq!(
            artwork[1].remote_url,
            "https://assets.fanart.tv/fanart/movies/42/movieposter/en.jpg"
        );
        assert!(!artwork[1].is_primary);
        assert_eq!(artwork[2].artwork_type, "backdrop");
        assert!(artwork[2].is_primary);
    }

    #[test]
    fn fanart_external_id_uses_tmdb_for_movies_and_tvdb_for_tv() {
        let matched = MetadataMatch {
            provider: "tmdb".to_owned(),
            external_id: "42".to_owned(),
            external_ids: vec![MetadataExternalId {
                provider: "tvdb".to_owned(),
                external_id: "121361".to_owned(),
            }],
            title: "Title".to_owned(),
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
        };

        assert_eq!(
            fanart_external_id(&matched, FanartKind::Movie).as_deref(),
            Some("42")
        );
        assert_eq!(
            fanart_external_id(&matched, FanartKind::Tv).as_deref(),
            Some("121361")
        );
    }
}
