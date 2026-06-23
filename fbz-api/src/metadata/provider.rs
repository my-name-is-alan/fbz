use std::{
    collections::BTreeSet,
    error::Error,
    fmt::{Display, Formatter},
    time::Duration,
};

use reqwest::{Client, Proxy};
use serde::{Deserialize, Serialize};

use crate::config::{MetadataConfig, ProxyConfig};

const HTTP_TIMEOUT_SECONDS: u64 = 10;
const MAX_METADATA_CLASSIFICATION_ITEMS: usize = 128;
const MAX_METADATA_CLASSIFICATION_NAME_LEN: usize = 128;
const MAX_METADATA_PEOPLE_ITEMS: usize = 512;
const MAX_METADATA_PERSON_NAME_LEN: usize = 256;
const MAX_METADATA_PERSON_ROLE_NAME_LEN: usize = 128;
const MAX_METADATA_PERSON_SORT_ORDER: i32 = 1_000_000;

#[derive(Clone)]
pub struct MetadataProviderClient {
    client: Client,
    metadata: MetadataConfig,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MetadataLookup {
    pub item_type: String,
    pub title: String,
    pub production_year: Option<i32>,
    pub language: Option<String>,
    pub country: Option<String>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct MetadataMatch {
    pub provider: String,
    pub external_id: String,
    pub title: String,
    pub original_title: Option<String>,
    pub overview: Option<String>,
    pub production_year: Option<i32>,
    pub premiere_date: Option<String>,
    pub official_rating: Option<String>,
    pub community_rating: Option<f32>,
    pub artwork: Vec<MetadataArtwork>,
    pub genres: Vec<MetadataNamedValue>,
    pub studios: Vec<MetadataNamedValue>,
    pub people: Vec<MetadataPerson>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MetadataArtwork {
    pub artwork_type: String,
    pub remote_url: String,
    pub is_primary: bool,
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
}

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

#[derive(Debug)]
pub enum MetadataProviderError {
    Client(String),
    Http(reqwest::Error),
}

impl MetadataProviderClient {
    pub fn from_config(
        metadata: MetadataConfig,
        proxy: ProxyConfig,
    ) -> Result<Self, MetadataProviderError> {
        let mut builder = Client::builder().timeout(Duration::from_secs(HTTP_TIMEOUT_SECONDS));
        if let Some(proxy) = proxy.http_proxy.as_deref() {
            builder = builder.proxy(
                Proxy::http(proxy).map_err(|err| MetadataProviderError::Client(err.to_string()))?,
            );
        }
        if let Some(proxy) = proxy.https_proxy.as_deref() {
            builder = builder.proxy(
                Proxy::https(proxy)
                    .map_err(|err| MetadataProviderError::Client(err.to_string()))?,
            );
        }

        let client = builder
            .build()
            .map_err(|err| MetadataProviderError::Client(err.to_string()))?;

        Ok(Self { client, metadata })
    }

    pub async fn match_item(
        &self,
        input: &MetadataLookup,
    ) -> Result<Option<MetadataMatch>, MetadataProviderError> {
        self.match_item_with_report(input)
            .await
            .map(|report| report.matched)
    }

    pub async fn match_item_with_report(
        &self,
        input: &MetadataLookup,
    ) -> Result<MetadataLookupReport, MetadataProviderError> {
        let mut attempts = Vec::new();

        for provider in normalized_providers(&self.metadata.providers) {
            match provider.as_str() {
                "tmdb" => {
                    let Some(token) = self.metadata.tmdb_access_token.as_deref() else {
                        attempts.push(MetadataProviderAttempt::skipped(
                            provider,
                            "missing TMDB access token",
                        ));
                        continue;
                    };
                    let Some(search_kind) = tmdb_search_kind(&input.item_type) else {
                        attempts.push(MetadataProviderAttempt::skipped(
                            provider,
                            format!("unsupported item type `{}`", input.item_type),
                        ));
                        continue;
                    };

                    match self.search_tmdb(input, token, search_kind).await {
                        Ok(Some(found)) => {
                            attempts.push(MetadataProviderAttempt::matched(
                                provider,
                                found.external_id.clone(),
                            ));
                            return Ok(MetadataLookupReport {
                                matched: Some(found),
                                attempts,
                            });
                        }
                        Ok(None) => attempts.push(MetadataProviderAttempt::not_matched(
                            provider,
                            "no TMDB search result",
                        )),
                        Err(err) => {
                            attempts
                                .push(MetadataProviderAttempt::failed(provider, err.to_string()));
                            return Err(err);
                        }
                    }
                }
                "tvdb" => attempts.push(MetadataProviderAttempt::skipped(
                    provider,
                    "TVDB provider execution is not implemented yet",
                )),
                "fanart" => attempts.push(MetadataProviderAttempt::skipped(
                    provider,
                    "Fanart provider execution is not implemented yet",
                )),
                _ => attempts.push(MetadataProviderAttempt::skipped(
                    provider,
                    "unsupported metadata provider",
                )),
            }
        }

        Ok(MetadataLookupReport {
            matched: None,
            attempts,
        })
    }

    async fn search_tmdb(
        &self,
        input: &MetadataLookup,
        token: &str,
        search_kind: TmdbSearchKind,
    ) -> Result<Option<MetadataMatch>, MetadataProviderError> {
        let mut query = vec![
            ("query", input.title.clone()),
            ("include_adult", "false".to_owned()),
        ];
        if let Some(year) = input.production_year {
            query.push((tmdb_year_param(search_kind), year.to_string()));
        }
        if let Some(language) = input.language.as_deref().and_then(normalize_language) {
            query.push(("language", language));
        }
        if let Some(country) = input.country.as_deref().and_then(normalize_country) {
            query.push(("region", country));
        }

        let response = self
            .client
            .get(tmdb_search_url(
                &self.metadata.tmdb_api_base_url,
                search_kind,
            ))
            .bearer_auth(token)
            .query(&query)
            .send()
            .await
            .map_err(MetadataProviderError::Http)?
            .error_for_status()
            .map_err(MetadataProviderError::Http)?
            .json::<TmdbSearchResponse>()
            .await
            .map_err(MetadataProviderError::Http)?;

        let Some(result) = response.results.into_iter().find(|result| result.id > 0) else {
            return Ok(None);
        };
        let id = result.id;
        let Some(mut found) = tmdb_result_to_match(result, &self.metadata.tmdb_image_base_url)
        else {
            return Ok(None);
        };

        let detail = self
            .fetch_tmdb_detail(input, token, search_kind, id)
            .await?;
        apply_tmdb_detail(
            &mut found,
            detail,
            &self.metadata.tmdb_image_base_url,
            input.country.as_deref(),
            search_kind,
        );

        Ok(Some(found))
    }

    async fn fetch_tmdb_detail(
        &self,
        input: &MetadataLookup,
        token: &str,
        search_kind: TmdbSearchKind,
        id: i64,
    ) -> Result<TmdbDetailResponse, MetadataProviderError> {
        let mut query = vec![("append_to_response", tmdb_detail_appends(search_kind))];
        if let Some(language) = input.language.as_deref().and_then(normalize_language) {
            query.push(("language", language));
        }

        self.client
            .get(tmdb_detail_url(
                &self.metadata.tmdb_api_base_url,
                search_kind,
                id,
            ))
            .bearer_auth(token)
            .query(&query)
            .send()
            .await
            .map_err(MetadataProviderError::Http)?
            .error_for_status()
            .map_err(MetadataProviderError::Http)?
            .json::<TmdbDetailResponse>()
            .await
            .map_err(MetadataProviderError::Http)
    }
}

impl MetadataProviderAttempt {
    fn matched(provider: String, external_id: String) -> Self {
        Self {
            provider,
            status: MetadataProviderAttemptStatus::Matched,
            message: None,
            external_id: Some(external_id),
        }
    }

    fn not_matched(provider: String, message: impl Into<String>) -> Self {
        Self {
            provider,
            status: MetadataProviderAttemptStatus::NotMatched,
            message: Some(message.into()),
            external_id: None,
        }
    }

    fn skipped(provider: String, message: impl Into<String>) -> Self {
        Self {
            provider,
            status: MetadataProviderAttemptStatus::Skipped,
            message: Some(message.into()),
            external_id: None,
        }
    }

    fn failed(provider: String, message: impl Into<String>) -> Self {
        Self {
            provider,
            status: MetadataProviderAttemptStatus::Failed,
            message: Some(message.into()),
            external_id: None,
        }
    }
}

fn normalized_providers(providers: &[String]) -> Vec<String> {
    providers
        .iter()
        .map(|provider| provider.trim().to_ascii_lowercase())
        .filter(|provider| !provider.is_empty())
        .collect()
}

fn tmdb_search_url(base_url: &str, search_kind: TmdbSearchKind) -> String {
    format!(
        "{}/search/{}",
        base_url.trim_end_matches('/'),
        search_kind.as_path()
    )
}

fn tmdb_detail_url(base_url: &str, search_kind: TmdbSearchKind, id: i64) -> String {
    format!(
        "{}/{}/{}",
        base_url.trim_end_matches('/'),
        search_kind.as_path(),
        id
    )
}

fn tmdb_detail_appends(search_kind: TmdbSearchKind) -> String {
    match search_kind {
        TmdbSearchKind::Movie => "credits,release_dates",
        TmdbSearchKind::Tv => "credits,content_ratings",
    }
    .to_owned()
}

fn tmdb_search_kind(item_type: &str) -> Option<TmdbSearchKind> {
    match item_type {
        "movie" => Some(TmdbSearchKind::Movie),
        "series" | "season" | "episode" => Some(TmdbSearchKind::Tv),
        _ => None,
    }
}

fn tmdb_year_param(search_kind: TmdbSearchKind) -> &'static str {
    match search_kind {
        TmdbSearchKind::Movie => "year",
        TmdbSearchKind::Tv => "first_air_date_year",
    }
}

fn tmdb_result_to_match(result: TmdbSearchResult, image_base_url: &str) -> Option<MetadataMatch> {
    let title = result
        .title
        .or(result.name)
        .map(|title| title.trim().to_owned())
        .filter(|title| !title.is_empty())?;
    let original_title = result
        .original_title
        .or(result.original_name)
        .map(|title| title.trim().to_owned())
        .filter(|title| !title.is_empty());
    let premiere_date = result
        .release_date
        .or(result.first_air_date)
        .and_then(normalize_tmdb_date);
    let mut artwork = Vec::new();
    if let Some(remote_url) = tmdb_image_url(image_base_url, result.poster_path.as_deref()) {
        artwork.push(MetadataArtwork {
            artwork_type: "poster".to_owned(),
            remote_url,
            is_primary: true,
        });
    }
    if let Some(remote_url) = tmdb_image_url(image_base_url, result.backdrop_path.as_deref()) {
        artwork.push(MetadataArtwork {
            artwork_type: "backdrop".to_owned(),
            remote_url,
            is_primary: true,
        });
    }

    Some(MetadataMatch {
        provider: "tmdb".to_owned(),
        external_id: result.id.to_string(),
        title,
        original_title,
        overview: result
            .overview
            .map(|overview| overview.trim().to_owned())
            .filter(|overview| !overview.is_empty()),
        production_year: premiere_date
            .as_deref()
            .and_then(|date| date.get(..4))
            .and_then(|year| year.parse::<i32>().ok()),
        premiere_date,
        official_rating: None,
        community_rating: result.vote_average.map(|rating| rating.clamp(0.0, 10.0)),
        artwork,
        genres: Vec::new(),
        studios: Vec::new(),
        people: Vec::new(),
    })
}

fn apply_tmdb_detail(
    found: &mut MetadataMatch,
    detail: TmdbDetailResponse,
    image_base_url: &str,
    country: Option<&str>,
    search_kind: TmdbSearchKind,
) {
    let official_rating = tmdb_official_rating(&detail, country, search_kind);

    if let Some(title) = detail.title.or(detail.name).and_then(normalize_text_title) {
        found.title = title;
    }
    found.original_title = detail
        .original_title
        .or(detail.original_name)
        .and_then(normalize_text_title)
        .or_else(|| found.original_title.clone());
    found.overview = detail
        .overview
        .and_then(normalize_overview)
        .or_else(|| found.overview.clone());

    if let Some(premiere_date) = detail
        .release_date
        .or(detail.first_air_date)
        .and_then(normalize_tmdb_date)
    {
        found.production_year = premiere_date
            .get(..4)
            .and_then(|year| year.parse::<i32>().ok());
        found.premiere_date = Some(premiere_date);
    }
    if let Some(rating) = detail.vote_average {
        found.community_rating = Some(rating.clamp(0.0, 10.0));
    }
    found.official_rating = official_rating;

    let detail_artwork = tmdb_artwork(
        image_base_url,
        detail.poster_path.as_deref(),
        detail.backdrop_path.as_deref(),
    );
    if !detail_artwork.is_empty() {
        found.artwork = detail_artwork;
    }

    found.genres = tmdb_genres(detail.genres);
    found.studios = tmdb_studios(detail.production_companies);
    found.people = tmdb_people(detail.credits);
}

fn tmdb_artwork(
    image_base_url: &str,
    poster_path: Option<&str>,
    backdrop_path: Option<&str>,
) -> Vec<MetadataArtwork> {
    let mut artwork = Vec::new();
    if let Some(remote_url) = tmdb_image_url(image_base_url, poster_path) {
        artwork.push(MetadataArtwork {
            artwork_type: "poster".to_owned(),
            remote_url,
            is_primary: true,
        });
    }
    if let Some(remote_url) = tmdb_image_url(image_base_url, backdrop_path) {
        artwork.push(MetadataArtwork {
            artwork_type: "backdrop".to_owned(),
            remote_url,
            is_primary: true,
        });
    }
    artwork
}

fn tmdb_image_url(base_url: &str, path: Option<&str>) -> Option<String> {
    let path = path?.trim();
    if path.is_empty()
        || !path.starts_with('/')
        || path.contains(char::is_whitespace)
        || path.contains("..")
    {
        return None;
    }
    Some(format!(
        "{}/original{}",
        base_url.trim_end_matches('/'),
        path
    ))
}

fn tmdb_genres(values: Vec<TmdbGenre>) -> Vec<MetadataNamedValue> {
    tmdb_named_values(values.into_iter().map(|value| value.name))
}

fn tmdb_studios(values: Vec<TmdbProductionCompany>) -> Vec<MetadataNamedValue> {
    tmdb_named_values(values.into_iter().map(|value| value.name))
}

fn tmdb_named_values(values: impl Iterator<Item = Option<String>>) -> Vec<MetadataNamedValue> {
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

fn tmdb_official_rating(
    detail: &TmdbDetailResponse,
    country: Option<&str>,
    search_kind: TmdbSearchKind,
) -> Option<String> {
    let preferred_country = country.and_then(normalize_country);
    match search_kind {
        TmdbSearchKind::Movie => detail
            .release_dates
            .as_ref()
            .and_then(|ratings| movie_certification(ratings, preferred_country.as_deref())),
        TmdbSearchKind::Tv => detail
            .content_ratings
            .as_ref()
            .and_then(|ratings| tv_content_rating(ratings, preferred_country.as_deref())),
    }
}

fn movie_certification(
    release_dates: &TmdbMovieReleaseDates,
    preferred_country: Option<&str>,
) -> Option<String> {
    let preferred = preferred_country
        .and_then(|country| movie_certification_for_country(release_dates, country))
        .or_else(|| movie_certification_for_country(release_dates, "US"));
    preferred.or_else(|| {
        release_dates.results.iter().find_map(|country| {
            country
                .release_dates
                .iter()
                .filter_map(|release| {
                    normalize_optional_bounded_text(
                        release.certification.as_deref(),
                        MAX_METADATA_CLASSIFICATION_NAME_LEN,
                    )
                    .filter(|value| !value.is_empty())
                    .map(|value| (movie_release_type_rank(release.release_type), value))
                })
                .min_by_key(|(rank, _)| *rank)
                .map(|(_, value)| value)
        })
    })
}

fn movie_certification_for_country(
    release_dates: &TmdbMovieReleaseDates,
    country: &str,
) -> Option<String> {
    release_dates
        .results
        .iter()
        .find(|entry| entry.iso_3166_1.as_deref() == Some(country))
        .and_then(|entry| {
            entry
                .release_dates
                .iter()
                .filter_map(|release| {
                    normalize_optional_bounded_text(
                        release.certification.as_deref(),
                        MAX_METADATA_CLASSIFICATION_NAME_LEN,
                    )
                    .filter(|value| !value.is_empty())
                    .map(|value| (movie_release_type_rank(release.release_type), value))
                })
                .min_by_key(|(rank, _)| *rank)
                .map(|(_, value)| value)
        })
}

fn movie_release_type_rank(release_type: Option<i32>) -> i32 {
    match release_type {
        Some(3) => 0,
        Some(2) => 1,
        Some(1) => 2,
        Some(4) => 3,
        Some(5) => 4,
        Some(6) => 5,
        _ => 10,
    }
}

fn tv_content_rating(
    content_ratings: &TmdbTvContentRatings,
    preferred_country: Option<&str>,
) -> Option<String> {
    preferred_country
        .and_then(|country| tv_content_rating_for_country(content_ratings, country))
        .or_else(|| tv_content_rating_for_country(content_ratings, "US"))
        .or_else(|| {
            content_ratings.results.iter().find_map(|entry| {
                normalize_optional_bounded_text(
                    entry.rating.as_deref(),
                    MAX_METADATA_CLASSIFICATION_NAME_LEN,
                )
                .filter(|value| !value.is_empty())
            })
        })
}

fn tv_content_rating_for_country(
    content_ratings: &TmdbTvContentRatings,
    country: &str,
) -> Option<String> {
    content_ratings
        .results
        .iter()
        .find(|entry| entry.iso_3166_1.as_deref() == Some(country))
        .and_then(|entry| {
            normalize_optional_bounded_text(
                entry.rating.as_deref(),
                MAX_METADATA_CLASSIFICATION_NAME_LEN,
            )
            .filter(|value| !value.is_empty())
        })
}

fn tmdb_people(credits: Option<TmdbCredits>) -> Vec<MetadataPerson> {
    let Some(credits) = credits else {
        return Vec::new();
    };

    let mut seen = BTreeSet::new();
    let mut people = Vec::new();
    for (index, cast) in credits.cast.into_iter().enumerate() {
        if people.len() >= MAX_METADATA_PEOPLE_ITEMS {
            return people;
        }
        let sort_order = cast
            .order
            .filter(|order| (0..=MAX_METADATA_PERSON_SORT_ORDER).contains(order))
            .unwrap_or_else(|| bounded_sort_order(index));
        push_tmdb_person(
            &mut people,
            &mut seen,
            cast.name.as_deref(),
            "actor",
            cast.character.as_deref(),
            sort_order,
        );
    }

    let crew_base = people.len();
    for (index, crew) in credits.crew.into_iter().enumerate() {
        if people.len() >= MAX_METADATA_PEOPLE_ITEMS {
            return people;
        }
        let Some(role_type) = tmdb_crew_role_type(crew.job.as_deref(), crew.department.as_deref())
        else {
            continue;
        };
        push_tmdb_person(
            &mut people,
            &mut seen,
            crew.name.as_deref(),
            role_type,
            crew.job.as_deref(),
            bounded_sort_order(crew_base + index),
        );
    }

    people
}

fn push_tmdb_person(
    people: &mut Vec<MetadataPerson>,
    seen: &mut BTreeSet<(String, String, String)>,
    name: Option<&str>,
    role_type: &str,
    role_name: Option<&str>,
    sort_order: i32,
) {
    let Some(name) = normalize_bounded_text(name, MAX_METADATA_PERSON_NAME_LEN) else {
        return;
    };
    let role_name = normalize_optional_bounded_text(role_name, MAX_METADATA_PERSON_ROLE_NAME_LEN)
        .unwrap_or_default();
    let name_normalized = normalize_metadata_name(&name);
    let key = (
        name_normalized.clone(),
        role_type.to_owned(),
        role_name.to_lowercase(),
    );
    if !seen.insert(key) {
        return;
    }

    people.push(MetadataPerson {
        name,
        name_normalized,
        role_type: role_type.to_owned(),
        role_name,
        sort_order,
    });
}

fn tmdb_crew_role_type(job: Option<&str>, department: Option<&str>) -> Option<&'static str> {
    let job = job.unwrap_or_default().trim().to_ascii_lowercase();
    let department = department.unwrap_or_default().trim().to_ascii_lowercase();
    match job.as_str() {
        "director" => Some("director"),
        "writer" | "screenplay" | "story" | "teleplay" | "creator" => Some("writer"),
        "producer" | "executive producer" => Some("producer"),
        "composer" | "original music composer" | "music" => Some("composer"),
        _ if department == "directing" => Some("director"),
        _ if department == "writing" => Some("writer"),
        _ if department == "production" => Some("producer"),
        _ if department == "sound" && job.contains("composer") => Some("composer"),
        _ => None,
    }
}

fn normalize_text_title(value: String) -> Option<String> {
    normalize_bounded_text(Some(value.as_str()), 512)
}

fn normalize_overview(value: String) -> Option<String> {
    normalize_bounded_text(Some(value.as_str()), 20_000)
}

fn normalize_bounded_text(value: Option<&str>, max_len: usize) -> Option<String> {
    let value = value?.trim();
    (!value.is_empty() && value.len() <= max_len).then(|| value.to_owned())
}

fn normalize_optional_bounded_text(value: Option<&str>, max_len: usize) -> Option<String> {
    let value = value.unwrap_or_default().trim();
    (value.len() <= max_len).then(|| value.to_owned())
}

fn normalize_metadata_name(value: &str) -> String {
    value.trim().to_lowercase()
}

fn bounded_sort_order(index: usize) -> i32 {
    i32::try_from(index)
        .unwrap_or(MAX_METADATA_PERSON_SORT_ORDER)
        .min(MAX_METADATA_PERSON_SORT_ORDER)
}

fn normalize_language(value: &str) -> Option<String> {
    let value = value.trim().replace('_', "-");
    (!value.is_empty()
        && value.len() <= 16
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphabetic() || byte == b'-'))
    .then_some(value)
}

fn normalize_country(value: &str) -> Option<String> {
    let value = value.trim().to_ascii_uppercase();
    (value.len() == 2 && value.bytes().all(|byte| byte.is_ascii_uppercase())).then_some(value)
}

fn normalize_tmdb_date(value: String) -> Option<String> {
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

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum TmdbSearchKind {
    Movie,
    Tv,
}

impl TmdbSearchKind {
    fn as_path(self) -> &'static str {
        match self {
            Self::Movie => "movie",
            Self::Tv => "tv",
        }
    }
}

#[derive(Debug, Deserialize)]
struct TmdbSearchResponse {
    #[serde(default)]
    results: Vec<TmdbSearchResult>,
}

#[derive(Debug, Deserialize)]
struct TmdbSearchResult {
    id: i64,
    title: Option<String>,
    name: Option<String>,
    original_title: Option<String>,
    original_name: Option<String>,
    overview: Option<String>,
    release_date: Option<String>,
    first_air_date: Option<String>,
    vote_average: Option<f32>,
    poster_path: Option<String>,
    backdrop_path: Option<String>,
}

#[derive(Debug, Deserialize)]
struct TmdbDetailResponse {
    title: Option<String>,
    name: Option<String>,
    original_title: Option<String>,
    original_name: Option<String>,
    overview: Option<String>,
    release_date: Option<String>,
    first_air_date: Option<String>,
    vote_average: Option<f32>,
    poster_path: Option<String>,
    backdrop_path: Option<String>,
    #[serde(default)]
    genres: Vec<TmdbGenre>,
    #[serde(default)]
    production_companies: Vec<TmdbProductionCompany>,
    credits: Option<TmdbCredits>,
    release_dates: Option<TmdbMovieReleaseDates>,
    content_ratings: Option<TmdbTvContentRatings>,
}

#[derive(Debug, Deserialize)]
struct TmdbGenre {
    name: Option<String>,
}

#[derive(Debug, Deserialize)]
struct TmdbProductionCompany {
    name: Option<String>,
}

#[derive(Debug, Deserialize)]
struct TmdbCredits {
    #[serde(default)]
    cast: Vec<TmdbCastCredit>,
    #[serde(default)]
    crew: Vec<TmdbCrewCredit>,
}

#[derive(Debug, Deserialize)]
struct TmdbCastCredit {
    name: Option<String>,
    character: Option<String>,
    order: Option<i32>,
}

#[derive(Debug, Deserialize)]
struct TmdbCrewCredit {
    name: Option<String>,
    job: Option<String>,
    department: Option<String>,
}

#[derive(Debug, Deserialize)]
struct TmdbMovieReleaseDates {
    #[serde(default)]
    results: Vec<TmdbMovieReleaseCountry>,
}

#[derive(Debug, Deserialize)]
struct TmdbMovieReleaseCountry {
    iso_3166_1: Option<String>,
    #[serde(default)]
    release_dates: Vec<TmdbMovieReleaseDate>,
}

#[derive(Debug, Deserialize)]
struct TmdbMovieReleaseDate {
    certification: Option<String>,
    #[serde(rename = "type")]
    release_type: Option<i32>,
}

#[derive(Debug, Deserialize)]
struct TmdbTvContentRatings {
    #[serde(default)]
    results: Vec<TmdbTvContentRating>,
}

#[derive(Debug, Deserialize)]
struct TmdbTvContentRating {
    iso_3166_1: Option<String>,
    rating: Option<String>,
}

impl Display for MetadataProviderError {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Client(err) => write!(f, "metadata provider client error: {err}"),
            Self::Http(err) => write!(f, "metadata provider http error: {err}"),
        }
    }
}

impl Error for MetadataProviderError {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tmdb_url_uses_configurable_base_url() {
        assert_eq!(
            tmdb_search_url("https://tmdb.example.test/3/", TmdbSearchKind::Movie),
            "https://tmdb.example.test/3/search/movie"
        );
        assert_eq!(
            tmdb_search_url("https://tmdb.example.test/3", TmdbSearchKind::Tv),
            "https://tmdb.example.test/3/search/tv"
        );
        assert_eq!(
            tmdb_detail_url("https://tmdb.example.test/3/", TmdbSearchKind::Movie, 42),
            "https://tmdb.example.test/3/movie/42"
        );
        assert_eq!(
            tmdb_detail_url("https://tmdb.example.test/3", TmdbSearchKind::Tv, 77),
            "https://tmdb.example.test/3/tv/77"
        );
        assert_eq!(
            tmdb_detail_appends(TmdbSearchKind::Movie),
            "credits,release_dates"
        );
        assert_eq!(
            tmdb_detail_appends(TmdbSearchKind::Tv),
            "credits,content_ratings"
        );
    }

    #[test]
    fn tmdb_result_maps_movie_metadata() {
        let mapped = tmdb_result_to_match(
            TmdbSearchResult {
                id: 42,
                title: Some("Movie".to_owned()),
                name: None,
                original_title: Some("Original Movie".to_owned()),
                original_name: None,
                overview: Some("Overview".to_owned()),
                release_date: Some("2026-06-19".to_owned()),
                first_air_date: None,
                vote_average: Some(12.0),
                poster_path: Some("/poster.jpg".to_owned()),
                backdrop_path: Some("/backdrop.jpg".to_owned()),
            },
            "https://image.example.test/t/p/",
        )
        .unwrap();

        assert_eq!(mapped.provider, "tmdb");
        assert_eq!(mapped.external_id, "42");
        assert_eq!(mapped.title, "Movie");
        assert_eq!(mapped.production_year, Some(2026));
        assert_eq!(mapped.premiere_date.as_deref(), Some("2026-06-19"));
        assert_eq!(mapped.community_rating, Some(10.0));
        assert_eq!(mapped.artwork.len(), 2);
        assert_eq!(mapped.artwork[0].artwork_type, "poster");
        assert_eq!(
            mapped.artwork[0].remote_url,
            "https://image.example.test/t/p/original/poster.jpg"
        );
        assert_eq!(mapped.artwork[1].artwork_type, "backdrop");
        assert_eq!(
            mapped.artwork[1].remote_url,
            "https://image.example.test/t/p/original/backdrop.jpg"
        );
        assert!(mapped.genres.is_empty());
        assert!(mapped.people.is_empty());
    }

    #[test]
    fn tmdb_detail_enriches_genres_people_and_artwork() {
        let mut mapped = MetadataMatch {
            provider: "tmdb".to_owned(),
            external_id: "42".to_owned(),
            title: "Search Title".to_owned(),
            original_title: None,
            overview: None,
            production_year: None,
            premiere_date: None,
            official_rating: None,
            community_rating: None,
            artwork: Vec::new(),
            genres: Vec::new(),
            studios: Vec::new(),
            people: Vec::new(),
        };

        apply_tmdb_detail(
            &mut mapped,
            TmdbDetailResponse {
                title: Some(" Detail Title ".to_owned()),
                name: None,
                original_title: Some("Original Detail".to_owned()),
                original_name: None,
                overview: Some(" Overview ".to_owned()),
                release_date: Some("2026-06-21".to_owned()),
                first_air_date: None,
                vote_average: Some(8.4),
                poster_path: Some("/detail-poster.jpg".to_owned()),
                backdrop_path: Some("/detail-backdrop.jpg".to_owned()),
                release_dates: Some(TmdbMovieReleaseDates {
                    results: vec![
                        TmdbMovieReleaseCountry {
                            iso_3166_1: Some("US".to_owned()),
                            release_dates: vec![TmdbMovieReleaseDate {
                                certification: Some("PG-13".to_owned()),
                                release_type: Some(3),
                            }],
                        },
                        TmdbMovieReleaseCountry {
                            iso_3166_1: Some("CN".to_owned()),
                            release_dates: vec![TmdbMovieReleaseDate {
                                certification: Some(" 13+ ".to_owned()),
                                release_type: Some(2),
                            }],
                        },
                    ],
                }),
                content_ratings: None,
                genres: vec![
                    TmdbGenre {
                        name: Some("Drama".to_owned()),
                    },
                    TmdbGenre {
                        name: Some(" drama ".to_owned()),
                    },
                    TmdbGenre {
                        name: Some("科幻".to_owned()),
                    },
                ],
                production_companies: vec![
                    TmdbProductionCompany {
                        name: Some("Studio A".to_owned()),
                    },
                    TmdbProductionCompany {
                        name: Some(" studio a ".to_owned()),
                    },
                    TmdbProductionCompany {
                        name: Some("Studio B".to_owned()),
                    },
                ],
                credits: Some(TmdbCredits {
                    cast: vec![
                        TmdbCastCredit {
                            name: Some("Actor One".to_owned()),
                            character: Some("Hero".to_owned()),
                            order: Some(0),
                        },
                        TmdbCastCredit {
                            name: Some(" Actor One ".to_owned()),
                            character: Some("Hero".to_owned()),
                            order: Some(1),
                        },
                    ],
                    crew: vec![
                        TmdbCrewCredit {
                            name: Some("Director One".to_owned()),
                            job: Some("Director".to_owned()),
                            department: Some("Directing".to_owned()),
                        },
                        TmdbCrewCredit {
                            name: Some("Writer One".to_owned()),
                            job: Some("Screenplay".to_owned()),
                            department: Some("Writing".to_owned()),
                        },
                        TmdbCrewCredit {
                            name: Some("Ignored One".to_owned()),
                            job: Some("Costume Design".to_owned()),
                            department: Some("Costume & Make-Up".to_owned()),
                        },
                    ],
                }),
            },
            "https://image.example.test/t/p",
            Some("cn"),
            TmdbSearchKind::Movie,
        );

        assert_eq!(mapped.title, "Detail Title");
        assert_eq!(mapped.original_title.as_deref(), Some("Original Detail"));
        assert_eq!(mapped.overview.as_deref(), Some("Overview"));
        assert_eq!(mapped.production_year, Some(2026));
        assert_eq!(mapped.premiere_date.as_deref(), Some("2026-06-21"));
        assert_eq!(mapped.official_rating.as_deref(), Some("13+"));
        assert_eq!(mapped.community_rating, Some(8.4));
        assert_eq!(mapped.artwork.len(), 2);
        assert_eq!(
            mapped.artwork[0].remote_url,
            "https://image.example.test/t/p/original/detail-poster.jpg"
        );
        assert_eq!(mapped.genres.len(), 2);
        assert_eq!(mapped.genres[0].name, "Drama");
        assert_eq!(mapped.genres[1].name, "科幻");
        assert_eq!(mapped.studios.len(), 2);
        assert_eq!(mapped.studios[0].name, "Studio A");
        assert_eq!(mapped.studios[1].name, "Studio B");
        assert_eq!(mapped.people.len(), 3);
        assert_eq!(mapped.people[0].role_type, "actor");
        assert_eq!(mapped.people[0].role_name, "Hero");
        assert_eq!(mapped.people[1].role_type, "director");
        assert_eq!(mapped.people[1].role_name, "Director");
        assert_eq!(mapped.people[2].role_type, "writer");
    }

    #[test]
    fn tmdb_tv_content_rating_prefers_requested_country_then_us() {
        let ratings = TmdbTvContentRatings {
            results: vec![
                TmdbTvContentRating {
                    iso_3166_1: Some("US".to_owned()),
                    rating: Some("TV-MA".to_owned()),
                },
                TmdbTvContentRating {
                    iso_3166_1: Some("GB".to_owned()),
                    rating: Some(" 15 ".to_owned()),
                },
            ],
        };

        assert_eq!(
            tv_content_rating(&ratings, Some("GB")).as_deref(),
            Some("15")
        );
        assert_eq!(
            tv_content_rating(&ratings, Some("CN")).as_deref(),
            Some("TV-MA")
        );
    }

    #[test]
    fn tmdb_image_url_rejects_unsafe_paths() {
        assert_eq!(
            tmdb_image_url("https://image.example.test/t/p", Some("/poster.jpg")).as_deref(),
            Some("https://image.example.test/t/p/original/poster.jpg")
        );
        assert_eq!(
            tmdb_image_url("https://image.example.test/t/p", Some("poster.jpg")),
            None
        );
        assert_eq!(
            tmdb_image_url("https://image.example.test/t/p", Some("/../poster.jpg")),
            None
        );
        assert_eq!(
            tmdb_image_url("https://image.example.test/t/p", Some("/has space.jpg")),
            None
        );
    }

    #[test]
    fn provider_and_locale_values_are_normalized() {
        assert_eq!(
            normalized_providers(&[" TMDB ".to_owned(), "fanart".to_owned()]),
            ["tmdb", "fanart"]
        );
        assert_eq!(normalize_language("zh_CN").as_deref(), Some("zh-CN"));
        assert_eq!(normalize_country("cn").as_deref(), Some("CN"));
        assert_eq!(normalize_country("china"), None);
    }

    #[tokio::test]
    async fn lookup_report_records_skipped_provider_boundaries() {
        let client = MetadataProviderClient::from_config(
            metadata_config(&[" TMDB ", "tvdb", "fanart", "unknown"]),
            proxy_config(),
        )
        .unwrap();

        let report = client
            .match_item_with_report(&MetadataLookup {
                item_type: "movie".to_owned(),
                title: "Movie".to_owned(),
                production_year: Some(2026),
                language: Some("zh-CN".to_owned()),
                country: Some("CN".to_owned()),
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
}
