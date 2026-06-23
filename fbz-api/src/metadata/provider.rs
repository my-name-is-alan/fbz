use std::{
    collections::BTreeSet,
    error::Error,
    fmt::{Display, Formatter},
    sync::Arc,
    time::Duration,
    time::Instant,
};

use reqwest::{Client, Proxy, Url};
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::config::{MetadataConfig, ProxyConfig};

const HTTP_TIMEOUT_SECONDS: u64 = 10;
const MAX_METADATA_CLASSIFICATION_ITEMS: usize = 128;
const MAX_METADATA_CLASSIFICATION_NAME_LEN: usize = 128;
const MAX_METADATA_PEOPLE_ITEMS: usize = 512;
const MAX_METADATA_PERSON_NAME_LEN: usize = 256;
const MAX_METADATA_PERSON_ROLE_NAME_LEN: usize = 128;
const MAX_METADATA_PERSON_SORT_ORDER: i32 = 1_000_000;
const MAX_METADATA_EXTERNAL_ID_LEN: usize = 128;
const TVDB_TOKEN_CACHE_SECONDS: u64 = 25 * 24 * 60 * 60;
const MAX_FANART_IMAGES: usize = 64;
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

#[derive(Clone)]
pub struct MetadataProviderClient {
    client: Client,
    metadata: MetadataConfig,
    tvdb_token: Arc<std::sync::RwLock<Option<CachedTvdbToken>>>,
}

#[derive(Clone, Debug)]
struct CachedTvdbToken {
    api_key: String,
    token: String,
    expires_at: Instant,
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
    pub external_ids: Vec<MetadataExternalId>,
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

        Ok(Self {
            client,
            metadata,
            tvdb_token: Arc::new(std::sync::RwLock::new(None)),
        })
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
        let mut matched = None;

        for provider in normalized_providers(&self.metadata.providers) {
            match provider.as_str() {
                "tmdb" => {
                    if matched.is_some() {
                        attempts.push(MetadataProviderAttempt::skipped(
                            provider,
                            "base metadata match already exists",
                        ));
                        continue;
                    }
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
                                provider.clone(),
                                found.external_id.clone(),
                            ));
                            matched = Some(found);
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
                "tvdb" => {
                    if matched.is_some() {
                        attempts.push(MetadataProviderAttempt::skipped(
                            provider,
                            "base metadata match already exists",
                        ));
                        continue;
                    }
                    let Some(api_key) = self.metadata.tvdb_api_key.as_deref() else {
                        attempts.push(MetadataProviderAttempt::skipped(
                            provider,
                            "missing TVDB API key",
                        ));
                        continue;
                    };
                    let Some(search_kind) = tvdb_search_kind(&input.item_type) else {
                        attempts.push(MetadataProviderAttempt::skipped(
                            provider,
                            format!("unsupported item type `{}`", input.item_type),
                        ));
                        continue;
                    };

                    match self.search_tvdb(input, api_key, search_kind).await {
                        Ok(Some(found)) => {
                            attempts.push(MetadataProviderAttempt::matched(
                                provider.clone(),
                                found.external_id.clone(),
                            ));
                            matched = Some(found);
                        }
                        Ok(None) => attempts.push(MetadataProviderAttempt::not_matched(
                            provider,
                            "no TVDB search result",
                        )),
                        Err(err) => {
                            attempts
                                .push(MetadataProviderAttempt::failed(provider, err.to_string()));
                            return Err(err);
                        }
                    }
                }
                "fanart" => {
                    let Some(token) = self.metadata.fanart_api_key.as_deref() else {
                        attempts.push(MetadataProviderAttempt::skipped(
                            provider,
                            "missing Fanart API key",
                        ));
                        continue;
                    };
                    let Some(found) = matched.as_mut() else {
                        attempts.push(MetadataProviderAttempt::skipped(
                            provider,
                            "requires a matched metadata item",
                        ));
                        continue;
                    };

                    match self.search_fanart(input, found, token).await {
                        Ok(FanartSearchOutcome::Matched {
                            external_id,
                            artwork,
                        }) => {
                            attempts.push(MetadataProviderAttempt::matched(provider, external_id));
                            found.artwork.extend(artwork);
                        }
                        Ok(FanartSearchOutcome::NotMatched(message)) => {
                            attempts.push(MetadataProviderAttempt::not_matched(provider, message))
                        }
                        Ok(FanartSearchOutcome::Skipped(message)) => {
                            attempts.push(MetadataProviderAttempt::skipped(provider, message))
                        }
                        Err(err) => {
                            attempts
                                .push(MetadataProviderAttempt::failed(provider, err.to_string()));
                            return Err(err);
                        }
                    }
                }
                _ => attempts.push(MetadataProviderAttempt::skipped(
                    provider,
                    "unsupported metadata provider",
                )),
            }
        }

        Ok(MetadataLookupReport { matched, attempts })
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

    async fn search_tvdb(
        &self,
        input: &MetadataLookup,
        api_key: &str,
        search_kind: TvdbSearchKind,
    ) -> Result<Option<MetadataMatch>, MetadataProviderError> {
        let token = self.tvdb_bearer_token(api_key).await?;
        let mut query = vec![
            ("query", input.title.clone()),
            ("type", search_kind.as_query_value().to_owned()),
            ("limit", "10".to_owned()),
        ];
        if let Some(year) = input.production_year {
            query.push(("year", year.to_string()));
        }

        let response = self
            .client
            .get(tvdb_search_url(&self.metadata.tvdb_api_base_url))
            .bearer_auth(token)
            .query(&query)
            .send()
            .await
            .map_err(MetadataProviderError::Http)?
            .error_for_status()
            .map_err(MetadataProviderError::Http)?
            .json::<TvdbSearchResponse>()
            .await
            .map_err(MetadataProviderError::Http)?;

        Ok(response
            .data
            .into_iter()
            .filter(|result| tvdb_search_result_matches_kind(result, search_kind))
            .find_map(tvdb_result_to_match))
    }

    async fn tvdb_bearer_token(&self, api_key: &str) -> Result<String, MetadataProviderError> {
        let api_key = api_key.trim();
        if api_key.is_empty() {
            return Err(MetadataProviderError::Client(
                "TVDB API key is empty".to_owned(),
            ));
        }

        if let Ok(cache) = self.tvdb_token.read() {
            if let Some(cache) = cache.as_ref() {
                if cache.api_key == api_key && cache.expires_at > Instant::now() {
                    return Ok(cache.token.clone());
                }
            }
        }

        let response = self
            .client
            .post(tvdb_login_url(&self.metadata.tvdb_api_base_url))
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

        if let Ok(mut cache) = self.tvdb_token.write() {
            *cache = Some(CachedTvdbToken {
                api_key: api_key.to_owned(),
                token: token.clone(),
                expires_at: Instant::now() + Duration::from_secs(TVDB_TOKEN_CACHE_SECONDS),
            });
        }

        Ok(token)
    }

    async fn search_fanart(
        &self,
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

        let response = self
            .client
            .get(fanart_url(
                &self.metadata.fanart_api_base_url,
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
        TmdbSearchKind::Movie => "credits,release_dates,external_ids",
        TmdbSearchKind::Tv => "credits,content_ratings,external_ids",
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
            source: None,
            remote_url,
            is_primary: true,
        });
    }
    if let Some(remote_url) = tmdb_image_url(image_base_url, result.backdrop_path.as_deref()) {
        artwork.push(MetadataArtwork {
            artwork_type: "backdrop".to_owned(),
            source: None,
            remote_url,
            is_primary: true,
        });
    }

    Some(MetadataMatch {
        provider: "tmdb".to_owned(),
        external_id: result.id.to_string(),
        external_ids: Vec::new(),
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
    add_tmdb_external_ids(found, detail.external_ids.as_ref(), search_kind);

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

fn add_tmdb_external_ids(
    found: &mut MetadataMatch,
    external_ids: Option<&TmdbExternalIds>,
    search_kind: TmdbSearchKind,
) {
    let Some(external_ids) = external_ids else {
        return;
    };

    if let Some(imdb_id) = external_ids
        .imdb_id
        .as_deref()
        .and_then(|value| normalize_external_id("imdb", value))
    {
        push_metadata_external_id(&mut found.external_ids, "imdb", imdb_id);
    }
    if matches!(search_kind, TmdbSearchKind::Tv) {
        if let Some(tvdb_id) = external_ids.tvdb_id.filter(|id| *id > 0) {
            push_metadata_external_id(&mut found.external_ids, "tvdb", tvdb_id.to_string());
        }
    }
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
            source: None,
            remote_url,
            is_primary: true,
        });
    }
    if let Some(remote_url) = tmdb_image_url(image_base_url, backdrop_path) {
        artwork.push(MetadataArtwork {
            artwork_type: "backdrop".to_owned(),
            source: None,
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

fn tvdb_login_url(base_url: &str) -> String {
    format!("{}/login", base_url.trim_end_matches('/'))
}

fn tvdb_search_url(base_url: &str) -> String {
    format!("{}/search", base_url.trim_end_matches('/'))
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
        original_title: None,
        overview: result.overview.and_then(normalize_overview),
        production_year,
        premiere_date,
        official_rating: None,
        community_rating: None,
        artwork: tvdb_artwork(result.image_url, result.poster, result.thumbnail),
        genres: Vec::new(),
        studios: Vec::new(),
        people: Vec::new(),
    })
}

fn tvdb_artwork(
    image_url: Option<String>,
    poster: Option<String>,
    thumbnail: Option<String>,
) -> Vec<MetadataArtwork> {
    let mut seen_urls = BTreeSet::new();
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

fn safe_remote_image_url(value: &str) -> Option<String> {
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

fn metadata_external_id(found: &MetadataMatch, provider: &str) -> Option<String> {
    if found.provider.trim().eq_ignore_ascii_case(provider) {
        return normalize_external_id(provider, &found.external_id);
    }

    found
        .external_ids
        .iter()
        .find(|external_id| external_id.provider.trim().eq_ignore_ascii_case(provider))
        .and_then(|external_id| normalize_external_id(provider, &external_id.external_id))
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

fn push_metadata_external_id(
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

fn normalize_external_id(_provider: &str, value: &str) -> Option<String> {
    normalize_bounded_text(Some(value), MAX_METADATA_EXTERNAL_ID_LEN)
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

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum TvdbSearchKind {
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

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum FanartKind {
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

#[derive(Clone, Debug, PartialEq, Eq)]
struct FanartCandidate {
    url: String,
    language_rank: i32,
    likes: i32,
}

#[derive(Clone, Debug, PartialEq)]
enum FanartSearchOutcome {
    Matched {
        external_id: String,
        artwork: Vec<MetadataArtwork>,
    },
    NotMatched(String),
    Skipped(String),
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
    external_ids: Option<TmdbExternalIds>,
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

#[derive(Debug, Deserialize)]
struct TmdbExternalIds {
    imdb_id: Option<String>,
    tvdb_id: Option<i64>,
}

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
            "credits,release_dates,external_ids"
        );
        assert_eq!(
            tmdb_detail_appends(TmdbSearchKind::Tv),
            "credits,content_ratings,external_ids"
        );
        assert_eq!(
            tvdb_login_url("https://tvdb.example.test/v4/"),
            "https://tvdb.example.test/v4/login"
        );
        assert_eq!(
            tvdb_search_url("https://tvdb.example.test/v4"),
            "https://tvdb.example.test/v4/search"
        );
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
    fn tmdb_detail_enriches_genres_people_and_artwork() {
        let mut mapped = MetadataMatch {
            provider: "tmdb".to_owned(),
            external_id: "42".to_owned(),
            external_ids: Vec::new(),
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
                external_ids: Some(TmdbExternalIds {
                    imdb_id: Some(" tt1234567 ".to_owned()),
                    tvdb_id: Some(98765),
                }),
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
        assert_eq!(mapped.external_ids.len(), 1);
        assert_eq!(mapped.external_ids[0].provider, "imdb");
        assert_eq!(mapped.external_ids[0].external_id, "tt1234567");
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
    fn tmdb_tv_detail_records_tvdb_external_id() {
        let mut mapped = MetadataMatch {
            provider: "tmdb".to_owned(),
            external_id: "77".to_owned(),
            external_ids: Vec::new(),
            title: "Show".to_owned(),
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

        add_tmdb_external_ids(
            &mut mapped,
            Some(&TmdbExternalIds {
                imdb_id: Some("tt7654321".to_owned()),
                tvdb_id: Some(121361),
            }),
            TmdbSearchKind::Tv,
        );

        assert_eq!(mapped.external_ids.len(), 2);
        assert_eq!(mapped.external_ids[0].provider, "imdb");
        assert_eq!(mapped.external_ids[1].provider, "tvdb");
        assert_eq!(mapped.external_ids[1].external_id, "121361");
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

        assert_eq!(
            fanart_external_id(&matched, FanartKind::Movie).as_deref(),
            Some("42")
        );
        assert_eq!(
            fanart_external_id(&matched, FanartKind::Tv).as_deref(),
            Some("121361")
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
