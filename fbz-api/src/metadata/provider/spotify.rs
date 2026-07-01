//! Spotify metadata provider（默认音乐查询源）。
//!
//! Base-match provider，支持 `track` / `album` / `artist`。用 Spotify Web API 的
//! Client Credentials OAuth 流程（client_id + client_secret → bearer token，进程内缓存）
//! 拿 token 后调 `/v1/search`，把结果映射为 `MetadataMatch`（标题/艺术家/专辑/封面/年份）。
//!
//! 缺少凭据时跳过（退回文件自带 ID3 标签或其他 provider）。是音乐元数据「默认 Spotify 查询」
//! 的落地；未来插件 provider 可声明 `media.read` + `metadata.provider.query` 提供其他音乐源。

use std::sync::{Arc, RwLock};
use std::time::{Duration, Instant};

use async_trait::async_trait;
use serde::Deserialize;

use super::shared::*;
use super::{MetadataProvider, ProviderContext, ProviderRole};

/// Spotify 搜索类型（与 item_type 映射）。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum SpotifySearchKind {
    Track,
    Album,
    Artist,
}

impl SpotifySearchKind {
    fn as_query_value(self) -> &'static str {
        match self {
            Self::Track => "track",
            Self::Album => "album",
            Self::Artist => "artist",
        }
    }
}

/// item_type → Spotify 搜索类型。非音乐类型返回 None。
fn spotify_search_kind(item_type: &str) -> Option<SpotifySearchKind> {
    match item_type {
        "track" => Some(SpotifySearchKind::Track),
        "album" => Some(SpotifySearchKind::Album),
        "artist" => Some(SpotifySearchKind::Artist),
        _ => None,
    }
}

#[derive(Clone, Debug)]
struct CachedSpotifyToken {
    /// 缓存键：client_id（凭据变了就失效重取）。
    client_id: String,
    token: String,
    expires_at: Instant,
}

/// Spotify base-match provider。进程内共享 token 缓存。
#[derive(Clone, Default)]
pub struct SpotifyProvider {
    token: Arc<RwLock<Option<CachedSpotifyToken>>>,
}

impl SpotifyProvider {
    pub fn new() -> Self {
        Self::default()
    }

    /// 取（缓存或新拉）Client Credentials bearer token。无凭据返回 None（provider 跳过）。
    async fn access_token(
        &self,
        ctx: &ProviderContext,
    ) -> Result<Option<String>, MetadataProviderError> {
        let client_id = match ctx.metadata.spotify_client_id.as_deref() {
            Some(id) if !id.trim().is_empty() => id.trim().to_owned(),
            _ => return Ok(None),
        };
        let client_secret = match ctx.metadata.spotify_client_secret.as_deref() {
            Some(s) if !s.trim().is_empty() => s.trim().to_owned(),
            _ => return Ok(None),
        };

        // 缓存命中（同 client_id 且未到期，留 60s 余量）。
        if let Ok(guard) = self.token.read()
            && let Some(cached) = guard.as_ref()
            && cached.client_id == client_id
            && cached.expires_at > Instant::now() + Duration::from_secs(60)
        {
            return Ok(Some(cached.token.clone()));
        }

        // Client Credentials 流程：POST auth_url，Basic(client_id:client_secret) + grant_type。
        let response = ctx
            .client("spotify")
            .post(&ctx.metadata.spotify_auth_url)
            .basic_auth(&client_id, Some(&client_secret))
            .form(&[("grant_type", "client_credentials")])
            .send()
            .await
            .map_err(MetadataProviderError::Http)?
            .error_for_status()
            .map_err(MetadataProviderError::Http)?
            .json::<SpotifyTokenResponse>()
            .await
            .map_err(MetadataProviderError::Http)?;

        let token = response.access_token;
        let expires_at =
            Instant::now() + Duration::from_secs(response.expires_in.unwrap_or(3600).max(1));
        if let Ok(mut guard) = self.token.write() {
            *guard = Some(CachedSpotifyToken {
                client_id,
                token: token.clone(),
                expires_at,
            });
        }
        Ok(Some(token))
    }

    async fn search(
        &self,
        ctx: &ProviderContext,
        input: &MetadataLookup,
        token: &str,
        kind: SpotifySearchKind,
    ) -> Result<Option<MetadataMatch>, MetadataProviderError> {
        let base = ctx.metadata.spotify_api_base_url.trim_end_matches('/');
        let response = ctx
            .client("spotify")
            .get(format!("{base}/search"))
            .bearer_auth(token)
            .query(&[
                ("q", input.title.as_str()),
                ("type", kind.as_query_value()),
                ("limit", "1"),
            ])
            .send()
            .await
            .map_err(MetadataProviderError::Http)?
            .error_for_status()
            .map_err(MetadataProviderError::Http)?
            .json::<SpotifySearchResponse>()
            .await
            .map_err(MetadataProviderError::Http)?;

        Ok(spotify_response_to_match(response, kind))
    }
}

#[async_trait]
impl MetadataProvider for SpotifyProvider {
    fn id(&self) -> &str {
        "spotify"
    }

    fn role(&self) -> ProviderRole {
        ProviderRole::BaseMatch
    }

    fn supports(&self, item_type: &str) -> bool {
        spotify_search_kind(item_type).is_some()
    }

    async fn match_item(
        &self,
        ctx: &ProviderContext,
        input: &MetadataLookup,
    ) -> Result<ProviderMatchOutcome, MetadataProviderError> {
        let Some(kind) = spotify_search_kind(&input.item_type) else {
            return Ok(ProviderMatchOutcome::Skipped(format!(
                "spotify does not support `{}`",
                input.item_type
            )));
        };
        let Some(token) = self.access_token(ctx).await? else {
            return Ok(ProviderMatchOutcome::Skipped(
                "missing Spotify client credentials".to_owned(),
            ));
        };
        match self.search(ctx, input, &token, kind).await? {
            Some(found) => Ok(ProviderMatchOutcome::Matched(Box::new(found))),
            None => Ok(ProviderMatchOutcome::NotMatched(
                "no Spotify search result".to_owned(),
            )),
        }
    }
}

/// 把 Spotify 搜索响应映射为 MetadataMatch。无结果返回 None。纯函数。
fn spotify_response_to_match(
    response: SpotifySearchResponse,
    kind: SpotifySearchKind,
) -> Option<MetadataMatch> {
    match kind {
        SpotifySearchKind::Track => {
            let item = response.tracks?.items.into_iter().next()?;
            let artist_names = join_artists(&item.artists);
            let mut found = base_match(item.id?, item.name?, item.external_urls);
            // track：artist 存 series_title 字段不合适，放进 studios（厂牌/艺人聚合）+ people。
            if let Some(album) = item.album.as_ref() {
                found.premiere_date = album.release_date.clone().and_then(normalize_release_date);
                found.production_year = found
                    .premiere_date
                    .as_deref()
                    .and_then(|d| d.get(..4))
                    .and_then(|y| y.parse::<i32>().ok());
                found.artwork = album_artwork(album);
            }
            found.people = artist_people(&item.artists);
            let _ = artist_names;
            Some(found)
        }
        SpotifySearchKind::Album => {
            let item = response.albums?.items.into_iter().next()?;
            let mut found = base_match(item.id?, item.name?, item.external_urls);
            found.premiere_date = item.release_date.clone().and_then(normalize_release_date);
            found.production_year = found
                .premiere_date
                .as_deref()
                .and_then(|d| d.get(..4))
                .and_then(|y| y.parse::<i32>().ok());
            found.artwork = album_artwork_from_images(&item.images);
            found.people = artist_people(&item.artists);
            Some(found)
        }
        SpotifySearchKind::Artist => {
            let item = response.artists?.items.into_iter().next()?;
            let mut found = base_match(item.id?, item.name?, item.external_urls);
            found.genres = item
                .genres
                .into_iter()
                .map(|g| {
                    let name_normalized = g.trim().to_lowercase();
                    MetadataNamedValue {
                        name: g.trim().to_owned(),
                        name_normalized,
                    }
                })
                .filter(|g| !g.name.is_empty())
                .collect();
            found.artwork = album_artwork_from_images(&item.images);
            Some(found)
        }
    }
}

fn base_match(id: String, name: String, urls: Option<SpotifyExternalUrls>) -> MetadataMatch {
    let mut external_ids = Vec::new();
    external_ids.push(MetadataExternalId {
        provider: "spotify".to_owned(),
        external_id: id.clone(),
    });
    let _ = urls;
    MetadataMatch {
        provider: "spotify".to_owned(),
        external_id: id,
        external_ids,
        title: name,
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

fn join_artists(artists: &[SpotifyArtist]) -> String {
    artists
        .iter()
        .filter_map(|a| a.name.as_deref())
        .collect::<Vec<_>>()
        .join(", ")
}

fn artist_people(artists: &[SpotifyArtist]) -> Vec<MetadataPerson> {
    artists
        .iter()
        .filter_map(|a| a.name.as_deref())
        .enumerate()
        .map(|(idx, name)| MetadataPerson {
            name: name.trim().to_owned(),
            name_normalized: name.trim().to_lowercase(),
            role_type: "artist".to_owned(),
            role_name: String::new(),
            sort_order: idx as i32,
        })
        .filter(|p| !p.name.is_empty())
        .collect()
}

fn album_artwork(album: &SpotifyAlbum) -> Vec<MetadataArtwork> {
    album_artwork_from_images(&album.images)
}

fn album_artwork_from_images(images: &[SpotifyImage]) -> Vec<MetadataArtwork> {
    images
        .iter()
        .filter_map(|img| img.url.as_deref())
        .take(1)
        .map(|url| MetadataArtwork {
            artwork_type: "primary".to_owned(),
            source: Some("spotify".to_owned()),
            remote_url: url.to_owned(),
            is_primary: true,
        })
        .collect()
}

/// Spotify release_date 可能是 `YYYY` / `YYYY-MM` / `YYYY-MM-DD`，归一为 `YYYY-MM-DD`（缺位补 01）。
fn normalize_release_date(raw: String) -> Option<String> {
    let raw = raw.trim();
    let parts: Vec<&str> = raw.split('-').collect();
    let year = parts.first()?;
    if year.len() != 4 || year.parse::<i32>().is_err() {
        return None;
    }
    let month = parts.get(1).copied().unwrap_or("01");
    let day = parts.get(2).copied().unwrap_or("01");
    Some(format!("{year}-{month:0>2}-{day:0>2}"))
}

// ---- Spotify API 响应类型 ----

#[derive(Debug, Deserialize)]
struct SpotifyTokenResponse {
    access_token: String,
    expires_in: Option<u64>,
}

#[derive(Debug, Deserialize)]
struct SpotifySearchResponse {
    tracks: Option<SpotifyPage<SpotifyTrack>>,
    albums: Option<SpotifyPage<SpotifyAlbum>>,
    artists: Option<SpotifyPage<SpotifyArtistFull>>,
}

#[derive(Debug, Deserialize)]
struct SpotifyPage<T> {
    #[serde(default = "Vec::new")]
    items: Vec<T>,
}

#[derive(Debug, Deserialize)]
struct SpotifyTrack {
    id: Option<String>,
    name: Option<String>,
    #[serde(default)]
    artists: Vec<SpotifyArtist>,
    album: Option<SpotifyAlbum>,
    external_urls: Option<SpotifyExternalUrls>,
}

#[derive(Debug, Deserialize)]
struct SpotifyAlbum {
    id: Option<String>,
    name: Option<String>,
    release_date: Option<String>,
    #[serde(default)]
    images: Vec<SpotifyImage>,
    #[serde(default)]
    artists: Vec<SpotifyArtist>,
    external_urls: Option<SpotifyExternalUrls>,
}

#[derive(Debug, Deserialize)]
struct SpotifyArtistFull {
    id: Option<String>,
    name: Option<String>,
    #[serde(default)]
    genres: Vec<String>,
    #[serde(default)]
    images: Vec<SpotifyImage>,
    external_urls: Option<SpotifyExternalUrls>,
}

#[derive(Debug, Deserialize)]
struct SpotifyArtist {
    name: Option<String>,
}

#[derive(Debug, Deserialize)]
struct SpotifyImage {
    url: Option<String>,
}

#[derive(Debug, Deserialize)]
struct SpotifyExternalUrls {
    #[allow(dead_code)]
    spotify: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn search_kind_maps_music_item_types() {
        assert_eq!(spotify_search_kind("track"), Some(SpotifySearchKind::Track));
        assert_eq!(spotify_search_kind("album"), Some(SpotifySearchKind::Album));
        assert_eq!(
            spotify_search_kind("artist"),
            Some(SpotifySearchKind::Artist)
        );
        assert_eq!(spotify_search_kind("movie"), None);
    }

    #[test]
    fn release_date_normalizes_partial_dates() {
        assert_eq!(
            normalize_release_date("1975".to_owned()).as_deref(),
            Some("1975-01-01")
        );
        assert_eq!(
            normalize_release_date("1975-11".to_owned()).as_deref(),
            Some("1975-11-01")
        );
        assert_eq!(
            normalize_release_date("1975-11-21".to_owned()).as_deref(),
            Some("1975-11-21")
        );
        assert_eq!(normalize_release_date("bad".to_owned()), None);
    }

    #[test]
    fn track_response_maps_to_match_with_artwork_and_people() {
        let response = SpotifySearchResponse {
            tracks: Some(SpotifyPage {
                items: vec![SpotifyTrack {
                    id: Some("track42".to_owned()),
                    name: Some("Bohemian Rhapsody".to_owned()),
                    artists: vec![SpotifyArtist {
                        name: Some("Queen".to_owned()),
                    }],
                    album: Some(SpotifyAlbum {
                        id: Some("alb1".to_owned()),
                        name: Some("A Night at the Opera".to_owned()),
                        release_date: Some("1975-11-21".to_owned()),
                        images: vec![SpotifyImage {
                            url: Some("https://i/cover.jpg".to_owned()),
                        }],
                        artists: Vec::new(),
                        external_urls: None,
                    }),
                    external_urls: None,
                }],
            }),
            albums: None,
            artists: None,
        };
        let m = spotify_response_to_match(response, SpotifySearchKind::Track).unwrap();
        assert_eq!(m.provider, "spotify");
        assert_eq!(m.external_id, "track42");
        assert_eq!(m.title, "Bohemian Rhapsody");
        assert_eq!(m.production_year, Some(1975));
        assert_eq!(m.premiere_date.as_deref(), Some("1975-11-21"));
        assert_eq!(m.artwork.len(), 1);
        assert!(m.artwork[0].is_primary);
        assert_eq!(m.people.len(), 1);
        assert_eq!(m.people[0].name, "Queen");
        assert_eq!(m.people[0].role_type, "artist");
        // spotify id 进 external_ids。
        assert!(
            m.external_ids
                .iter()
                .any(|e| e.provider == "spotify" && e.external_id == "track42")
        );
    }

    #[test]
    fn empty_response_yields_no_match() {
        let response = SpotifySearchResponse {
            tracks: Some(SpotifyPage { items: Vec::new() }),
            albums: None,
            artists: None,
        };
        assert!(spotify_response_to_match(response, SpotifySearchKind::Track).is_none());
    }
}
