//! TVDB metadata provider.
//!
//! Base-match provider for movies and series. Currently search-level only (no
//! detail enrichment yet — genres/people/studios stay empty; this is tracked as
//! a phase 4 follow-up). Holds a process-lifetime bearer-token cache keyed by
//! API key. Migrated from the legacy `provider.rs` with no behavioral change.

use std::sync::{Arc, RwLock};
use std::time::{Duration, Instant};

use async_trait::async_trait;
use serde::{Deserialize, Serialize};

use super::shared::*;
use super::{MetadataProvider, ProviderContext, ProviderRole};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TvdbSearchKind {
    Movie,
    Series,
}

impl TvdbSearchKind {
    fn as_query_value(self) -> &'static str {
        match self {
            Self::Movie => "movie",
            Self::Series => "series",
        }
    }
}

#[derive(Clone, Debug)]
struct CachedTvdbToken {
    api_key: String,
    token: String,
    expires_at: Instant,
}

/// TVDB base-match provider. Stateless except for the shared token cache.
#[derive(Clone, Default)]
pub struct TvdbProvider {
    token: Arc<RwLock<Option<CachedTvdbToken>>>,
}

impl TvdbProvider {
    pub fn new() -> Self {
        Self::default()
    }

    async fn search_tvdb(
        &self,
        ctx: &ProviderContext,
        input: &MetadataLookup,
        api_key: &str,
        search_kind: TvdbSearchKind,
    ) -> Result<Option<MetadataMatch>, MetadataProviderError> {
        let token = self.tvdb_bearer_token(ctx, api_key).await?;
        let mut query = vec![
            ("query", input.title.clone()),
            ("type", search_kind.as_query_value().to_owned()),
            ("limit", "10".to_owned()),
        ];
        if let Some(year) = input.production_year {
            query.push(("year", year.to_string()));
        }

        let response = ctx
            .client("tvdb")
            .get(tvdb_search_url(&ctx.metadata.tvdb_api_base_url))
            .bearer_auth(&token)
            .query(&query)
            .send()
            .await
            .map_err(MetadataProviderError::Http)?
            .error_for_status()
            .map_err(MetadataProviderError::Http)?
            .json::<TvdbSearchResponse>()
            .await
            .map_err(MetadataProviderError::Http)?;

        let Some(mut found) = response
            .data
            .into_iter()
            .filter(|result| tvdb_search_result_matches_kind(result, search_kind))
            .find_map(tvdb_result_to_match)
        else {
            return Ok(None);
        };

        // Detail enrichment (§4.3 / phase 4): fetch the v4 extended record to
        // fill genres/studios/people. Best-effort — on any error or missing
        // numeric id we keep the search-level match unchanged (zero regression).
        if let Some(numeric_id) = tvdb_numeric_id(&found.external_id) {
            if let Ok(extended) = self
                .fetch_tvdb_extended(ctx, &token, search_kind, numeric_id)
                .await
            {
                apply_tvdb_extended(&mut found, extended);
            }
        }

        Ok(Some(found))
    }

    async fn fetch_tvdb_extended(
        &self,
        ctx: &ProviderContext,
        token: &str,
        search_kind: TvdbSearchKind,
        id: i64,
    ) -> Result<TvdbExtendedResponse, MetadataProviderError> {
        ctx.client("tvdb")
            .get(tvdb_extended_url(
                &ctx.metadata.tvdb_api_base_url,
                search_kind,
                id,
            ))
            .bearer_auth(token)
            .query(&[("meta", "translations")])
            .send()
            .await
            .map_err(MetadataProviderError::Http)?
            .error_for_status()
            .map_err(MetadataProviderError::Http)?
            .json::<TvdbExtendedResponse>()
            .await
            .map_err(MetadataProviderError::Http)
    }

    async fn tvdb_bearer_token(
        &self,
        ctx: &ProviderContext,
        api_key: &str,
    ) -> Result<String, MetadataProviderError> {
        let api_key = api_key.trim();
        if api_key.is_empty() {
            return Err(MetadataProviderError::Client(
                "TVDB API key is empty".to_owned(),
            ));
        }

        if let Ok(cache) = self.token.read() {
            if let Some(cache) = cache.as_ref() {
                if cache.api_key == api_key && cache.expires_at > Instant::now() {
                    return Ok(cache.token.clone());
                }
            }
        }

        let response = ctx
            .client("tvdb")
            .post(tvdb_login_url(&ctx.metadata.tvdb_api_base_url))
            .json(&TvdbLoginRequest { apikey: api_key })
            .send()
            .await
            .map_err(MetadataProviderError::Http)?
            .error_for_status()
            .map_err(MetadataProviderError::Http)?
            .json::<TvdbLoginResponse>()
            .await
            .map_err(MetadataProviderError::Http)?;

        let token = response
            .data
            .and_then(|data| normalize_bounded_text(data.token.as_deref(), 4096))
            .ok_or_else(|| {
                MetadataProviderError::Client("TVDB login response missing token".to_owned())
            })?;

        if let Ok(mut cache) = self.token.write() {
            *cache = Some(CachedTvdbToken {
                api_key: api_key.to_owned(),
                token: token.clone(),
                expires_at: Instant::now() + Duration::from_secs(TVDB_TOKEN_CACHE_SECONDS),
            });
        }

        Ok(token)
    }
}

#[async_trait]
impl MetadataProvider for TvdbProvider {
    fn id(&self) -> &str {
        "tvdb"
    }

    fn role(&self) -> ProviderRole {
        ProviderRole::BaseMatch
    }

    fn supports(&self, item_type: &str) -> bool {
        matches!(item_type, "movie" | "series" | "season" | "episode")
    }

    async fn match_item(
        &self,
        ctx: &ProviderContext,
        input: &MetadataLookup,
    ) -> Result<ProviderMatchOutcome, MetadataProviderError> {
        let Some(api_key) = ctx.metadata.tvdb_api_key.as_deref() else {
            return Ok(ProviderMatchOutcome::Skipped(
                "missing TVDB API key".to_owned(),
            ));
        };
        let Some(search_kind) = tvdb_search_kind(&input.item_type) else {
            return Ok(ProviderMatchOutcome::Skipped(format!(
                "unsupported item type `{}`",
                input.item_type
            )));
        };

        match self.search_tvdb(ctx, input, api_key, search_kind).await? {
            Some(found) => Ok(ProviderMatchOutcome::Matched(Box::new(found))),
            None => Ok(ProviderMatchOutcome::NotMatched(
                "no TVDB search result".to_owned(),
            )),
        }
    }
}

// ---------------------------------------------------------------------------
// TVDB-specific helpers (migrated verbatim from provider.rs).
// ---------------------------------------------------------------------------

fn tvdb_login_url(base_url: &str) -> String {
    format!("{}/login", base_url.trim_end_matches('/'))
}

fn tvdb_search_url(base_url: &str) -> String {
    format!("{}/search", base_url.trim_end_matches('/'))
}

/// Builds the v4 extended-record URL: `/movies/{id}/extended` or
/// `/series/{id}/extended`.
fn tvdb_extended_url(base_url: &str, search_kind: TvdbSearchKind, id: i64) -> String {
    let path = match search_kind {
        TvdbSearchKind::Movie => "movies",
        TvdbSearchKind::Series => "series",
    };
    format!(
        "{}/{}/{}/extended",
        base_url.trim_end_matches('/'),
        path,
        id
    )
}

/// Extracts the numeric TVDB id from an external id like `121361` or
/// `series-121361`, for use in the extended-record URL.
fn tvdb_numeric_id(external_id: &str) -> Option<i64> {
    let trimmed = external_id.trim();
    if let Ok(id) = trimmed.parse::<i64>() {
        return (id > 0).then_some(id);
    }
    trimmed
        .rsplit(['-', '/'])
        .next()
        .and_then(|tail| tail.parse::<i64>().ok())
        .filter(|id| *id > 0)
}

/// Folds the extended record into the match: genres/studios/people, plus
/// overview as a fallback. Defensive — every field is optional, so an
/// unexpected shape just yields empty additions.
fn apply_tvdb_extended(found: &mut MetadataMatch, response: TvdbExtendedResponse) {
    let Some(data) = response.data else {
        return;
    };

    if found.overview.is_none() {
        found.overview = data.overview.and_then(normalize_overview);
    }

    let genres = dedupe_named_values(data.genres.into_iter().map(|genre| genre.name));
    if !genres.is_empty() {
        found.genres = genres;
    }

    let studios = dedupe_named_values(data.companies.into_iter().map(|company| company.name));
    if !studios.is_empty() {
        found.studios = studios;
    }

    let people = tvdb_people(data.characters);
    if !people.is_empty() {
        found.people = people;
    }
}

/// Maps TVDB extended `characters` into people (actors only; TVDB's crew shape
/// is inconsistent, deferred). De-duplicates by (name, role).
fn tvdb_people(characters: Vec<TvdbCharacter>) -> Vec<MetadataPerson> {
    let mut seen = std::collections::BTreeSet::new();
    let mut people = Vec::new();
    for (index, character) in characters.into_iter().enumerate() {
        if people.len() >= MAX_METADATA_PEOPLE_ITEMS {
            break;
        }
        let Some(name) = normalize_bounded_text(
            character.person_name.as_deref(),
            MAX_METADATA_PERSON_NAME_LEN,
        ) else {
            continue;
        };
        let role_name = normalize_optional_bounded_text(
            character.name.as_deref(),
            MAX_METADATA_PERSON_ROLE_NAME_LEN,
        )
        .unwrap_or_default();
        let name_normalized = normalize_metadata_name(&name);
        if !seen.insert((name_normalized.clone(), role_name.to_lowercase())) {
            continue;
        }
        people.push(MetadataPerson {
            name,
            name_normalized,
            role_type: "actor".to_owned(),
            role_name,
            sort_order: bounded_sort_order(index),
        });
    }
    people
}

fn tvdb_search_kind(item_type: &str) -> Option<TvdbSearchKind> {
    match item_type {
        "movie" => Some(TvdbSearchKind::Movie),
        "series" | "season" | "episode" => Some(TvdbSearchKind::Series),
        _ => None,
    }
}

fn tvdb_search_result_matches_kind(result: &TvdbSearchResult, search_kind: TvdbSearchKind) -> bool {
    result
        .entity_type
        .as_deref()
        .map(|value| {
            value
                .trim()
                .eq_ignore_ascii_case(search_kind.as_query_value())
        })
        .unwrap_or(true)
}

fn tvdb_result_to_match(result: TvdbSearchResult) -> Option<MetadataMatch> {
    let external_id = result
        .tvdb_id
        .as_deref()
        .or(result.id.as_deref())
        .and_then(|value| normalize_external_id("tvdb", value))?;
    let title = result
        .title
        .or(result.name_translated)
        .or(result.name)
        .and_then(normalize_text_title)?;
    let premiere_date = result.first_air_time.and_then(normalize_tvdb_date);
    let production_year = premiere_date
        .as_deref()
        .and_then(|date| date.get(..4))
        .and_then(|year| year.parse::<i32>().ok())
        .or_else(|| result.year.and_then(normalize_tvdb_year));

    Some(MetadataMatch {
        provider: "tvdb".to_owned(),
        external_id,
        external_ids: tvdb_remote_external_ids(result.remote_ids),
        title,
        series_title: None,
        original_title: None,
        overview: result.overview.and_then(normalize_overview),
        production_year,
        premiere_date,
        official_rating: None,
        community_rating: None,
        artwork: tvdb_artwork(result.image_url, result.poster, result.thumbnail),
        genres: Vec::new(),
        studios: Vec::new(),
        networks: Vec::new(),
        videos: Vec::new(),
        collection: None,
        people: Vec::new(),
    })
}

fn tvdb_artwork(
    image_url: Option<String>,
    poster: Option<String>,
    thumbnail: Option<String>,
) -> Vec<MetadataArtwork> {
    let mut seen_urls = std::collections::BTreeSet::new();
    let mut artwork: Vec<MetadataArtwork> = Vec::new();
    for (artwork_type, url) in [
        ("poster", image_url),
        ("poster", poster),
        ("thumb", thumbnail),
    ] {
        let Some(remote_url) = url.as_deref().and_then(safe_remote_image_url) else {
            continue;
        };
        if !seen_urls.insert(remote_url.clone()) {
            continue;
        }
        artwork.push(MetadataArtwork {
            artwork_type: artwork_type.to_owned(),
            source: None,
            remote_url,
            is_primary: !artwork.iter().any(|item| item.artwork_type == artwork_type),
        });
    }
    artwork
}

fn tvdb_remote_external_ids(remote_ids: Vec<TvdbRemoteId>) -> Vec<MetadataExternalId> {
    let mut external_ids = Vec::new();
    for remote_id in remote_ids {
        let Some(provider) = tvdb_remote_id_provider(remote_id.source_name.as_deref()) else {
            continue;
        };
        let Some(external_id) = remote_id
            .id
            .as_deref()
            .and_then(|value| normalize_external_id(provider, value))
        else {
            continue;
        };
        push_metadata_external_id(&mut external_ids, provider, external_id);
    }
    external_ids
}

fn tvdb_remote_id_provider(source_name: Option<&str>) -> Option<&'static str> {
    let source_name = source_name?.trim().to_ascii_lowercase();
    if source_name.contains("imdb") {
        Some("imdb")
    } else if source_name.contains("tmdb")
        || source_name.contains("themoviedb")
        || source_name.contains("the movie db")
    {
        Some("tmdb")
    } else if source_name.contains("eidr") {
        Some("eidr")
    } else {
        None
    }
}

fn normalize_tvdb_date(value: String) -> Option<String> {
    let value = value.trim();
    value
        .get(..10)
        .map(str::to_owned)
        .filter(|date| normalize_tmdb_date(date.clone()).is_some())
}

fn normalize_tvdb_year(value: String) -> Option<i32> {
    let value = value.trim();
    (value.len() == 4 && value.bytes().all(|byte| byte.is_ascii_digit()))
        .then(|| value.parse::<i32>().ok())
        .flatten()
}

// ---------------------------------------------------------------------------
// TVDB request/response structs.
// ---------------------------------------------------------------------------

#[derive(Debug, Serialize)]
struct TvdbLoginRequest<'a> {
    apikey: &'a str,
}

#[derive(Debug, Deserialize)]
struct TvdbLoginResponse {
    data: Option<TvdbLoginData>,
}

#[derive(Debug, Deserialize)]
struct TvdbLoginData {
    token: Option<String>,
}

#[derive(Debug, Deserialize)]
struct TvdbSearchResponse {
    #[serde(default)]
    data: Vec<TvdbSearchResult>,
}

#[derive(Debug, Deserialize)]
struct TvdbSearchResult {
    id: Option<String>,
    tvdb_id: Option<String>,
    title: Option<String>,
    name: Option<String>,
    name_translated: Option<String>,
    overview: Option<String>,
    first_air_time: Option<String>,
    image_url: Option<String>,
    poster: Option<String>,
    thumbnail: Option<String>,
    #[serde(default)]
    remote_ids: Vec<TvdbRemoteId>,
    #[serde(rename = "type")]
    entity_type: Option<String>,
    year: Option<String>,
}

#[derive(Debug, Deserialize)]
struct TvdbRemoteId {
    id: Option<String>,
    #[serde(rename = "sourceName")]
    source_name: Option<String>,
}

#[derive(Debug, Deserialize, Default)]
struct TvdbExtendedResponse {
    data: Option<TvdbExtendedData>,
}

#[derive(Debug, Deserialize, Default)]
struct TvdbExtendedData {
    overview: Option<String>,
    #[serde(default)]
    genres: Vec<TvdbGenre>,
    #[serde(default)]
    companies: Vec<TvdbCompany>,
    #[serde(default)]
    characters: Vec<TvdbCharacter>,
}

#[derive(Debug, Deserialize)]
struct TvdbGenre {
    name: Option<String>,
}

#[derive(Debug, Deserialize)]
struct TvdbCompany {
    name: Option<String>,
}

#[derive(Debug, Deserialize)]
struct TvdbCharacter {
    name: Option<String>,
    #[serde(rename = "personName")]
    person_name: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tvdb_url_uses_configurable_base_url() {
        assert_eq!(
            tvdb_login_url("https://tvdb.example.test/v4/"),
            "https://tvdb.example.test/v4/login"
        );
        assert_eq!(
            tvdb_search_url("https://tvdb.example.test/v4"),
            "https://tvdb.example.test/v4/search"
        );
    }

    #[test]
    fn tvdb_result_maps_series_metadata_and_remote_ids() {
        let mapped = tvdb_result_to_match(TvdbSearchResult {
            id: Some("series-121361".to_owned()),
            tvdb_id: Some("121361".to_owned()),
            title: None,
            name: Some("Fallback Title".to_owned()),
            name_translated: Some(" Translated Title ".to_owned()),
            overview: Some(" Overview ".to_owned()),
            first_air_time: Some("2011-04-17T00:00:00Z".to_owned()),
            image_url: Some("https://art.example.test/poster.jpg".to_owned()),
            poster: Some("https://art.example.test/poster.jpg".to_owned()),
            thumbnail: Some("https://art.example.test/thumb.jpg".to_owned()),
            remote_ids: vec![
                TvdbRemoteId {
                    id: Some("tt0944947".to_owned()),
                    source_name: Some("IMDB".to_owned()),
                },
                TvdbRemoteId {
                    id: Some("1399".to_owned()),
                    source_name: Some("TheMovieDB.com".to_owned()),
                },
                TvdbRemoteId {
                    id: Some("ignored".to_owned()),
                    source_name: Some("Unknown".to_owned()),
                },
            ],
            entity_type: Some("series".to_owned()),
            year: Some("2010".to_owned()),
        })
        .unwrap();

        assert_eq!(mapped.provider, "tvdb");
        assert_eq!(mapped.external_id, "121361");
        assert_eq!(mapped.title, "Translated Title");
        assert_eq!(mapped.overview.as_deref(), Some("Overview"));
        assert_eq!(mapped.production_year, Some(2011));
        assert_eq!(mapped.premiere_date.as_deref(), Some("2011-04-17"));
        assert_eq!(
            mapped.external_ids,
            vec![
                MetadataExternalId {
                    provider: "imdb".to_owned(),
                    external_id: "tt0944947".to_owned(),
                },
                MetadataExternalId {
                    provider: "tmdb".to_owned(),
                    external_id: "1399".to_owned(),
                },
            ]
        );
        assert_eq!(mapped.artwork.len(), 2);
        assert_eq!(mapped.artwork[0].artwork_type, "poster");
        assert_eq!(
            mapped.artwork[0].remote_url,
            "https://art.example.test/poster.jpg"
        );
        assert_eq!(mapped.artwork[1].artwork_type, "thumb");
    }

    #[test]
    fn tvdb_result_rejects_wrong_kind_and_unsafe_artwork() {
        let result = TvdbSearchResult {
            id: Some("movie-42".to_owned()),
            tvdb_id: Some("42".to_owned()),
            title: Some("Movie".to_owned()),
            name: None,
            name_translated: None,
            overview: None,
            first_air_time: None,
            image_url: Some("https://user:pass@art.example.test/poster.jpg".to_owned()),
            poster: Some("file:///tmp/poster.jpg".to_owned()),
            thumbnail: Some("https://art.example.test/has space.jpg".to_owned()),
            remote_ids: Vec::new(),
            entity_type: Some("movie".to_owned()),
            year: Some("2026".to_owned()),
        };

        assert!(!tvdb_search_result_matches_kind(
            &result,
            TvdbSearchKind::Series
        ));
        let mapped = tvdb_result_to_match(result).unwrap();
        assert_eq!(mapped.production_year, Some(2026));
        assert!(mapped.artwork.is_empty());
    }

    #[test]
    fn tvdb_extended_url_and_numeric_id() {
        assert_eq!(
            tvdb_extended_url(
                "https://tvdb.example.test/v4",
                TvdbSearchKind::Series,
                121361
            ),
            "https://tvdb.example.test/v4/series/121361/extended"
        );
        assert_eq!(
            tvdb_extended_url("https://tvdb.example.test/v4/", TvdbSearchKind::Movie, 42),
            "https://tvdb.example.test/v4/movies/42/extended"
        );
        assert_eq!(tvdb_numeric_id("121361"), Some(121361));
        assert_eq!(tvdb_numeric_id("series-121361"), Some(121361));
        assert_eq!(tvdb_numeric_id("abc"), None);
        assert_eq!(tvdb_numeric_id("0"), None);
    }

    #[test]
    fn tvdb_extended_fills_genres_studios_people() {
        let mut found = tvdb_result_to_match(TvdbSearchResult {
            id: Some("121361".to_owned()),
            tvdb_id: Some("121361".to_owned()),
            title: Some("Show".to_owned()),
            name: None,
            name_translated: None,
            overview: None,
            first_air_time: None,
            image_url: None,
            poster: None,
            thumbnail: None,
            remote_ids: Vec::new(),
            entity_type: Some("series".to_owned()),
            year: Some("2011".to_owned()),
        })
        .unwrap();

        apply_tvdb_extended(
            &mut found,
            TvdbExtendedResponse {
                data: Some(TvdbExtendedData {
                    overview: Some(" Detailed overview ".to_owned()),
                    genres: vec![
                        TvdbGenre {
                            name: Some("Drama".to_owned()),
                        },
                        TvdbGenre {
                            name: Some(" drama ".to_owned()),
                        },
                    ],
                    companies: vec![TvdbCompany {
                        name: Some("HBO".to_owned()),
                    }],
                    characters: vec![
                        TvdbCharacter {
                            name: Some("Jon".to_owned()),
                            person_name: Some("Kit H".to_owned()),
                        },
                        TvdbCharacter {
                            name: None,
                            person_name: None,
                        },
                    ],
                }),
            },
        );

        assert_eq!(found.overview.as_deref(), Some("Detailed overview"));
        assert_eq!(found.genres.len(), 1);
        assert_eq!(found.genres[0].name, "Drama");
        assert_eq!(found.studios.len(), 1);
        assert_eq!(found.studios[0].name, "HBO");
        assert_eq!(found.people.len(), 1);
        assert_eq!(found.people[0].name, "Kit H");
        assert_eq!(found.people[0].role_name, "Jon");
        assert_eq!(found.people[0].role_type, "actor");
    }
}
