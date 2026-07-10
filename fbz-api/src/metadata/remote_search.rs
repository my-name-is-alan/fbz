//! Emby `Items/RemoteSearch/*` 兼容层的多候选元数据搜索。
//!
//! 与刮削管线（单一最佳匹配 + 富化）不同，RemoteSearch 面向管理端"手动识别"：
//! 一次搜索返回多个候选，由管理员挑选后经 `RemoteSearch/Apply/{id}` 写回外部 id
//! 并触发刮削。当前实现 TMDB（movie/series/person/boxset 四类），TMDB 未配置
//! key 或类型不支持时返回空候选（不报错，保持 Emby 客户端兼容）。
//!
//! 设置解析与刮削管线同源：env 基线 ← DB 管理端覆盖（`resolve_metadata_config`），
//! 代理沿用 provider 级 override。

use serde::Deserialize;

use crate::{
    config::{MetadataConfig, ProxyConfig, SecretConfig},
    db::DbPool,
    metadata::provider::{ProviderProxyOverride, build_single_client},
    metadata::settings::{MetadataSettingsRepository, resolve_metadata_config},
    notifications::secrets::SecretCipher,
};

const REMOTE_SEARCH_RESULT_LIMIT: usize = 10;

/// 一次远程搜索的输入（route 层已归一化）。
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct RemoteSearchRequest {
    /// FBZ 内部条目类型：movie / series / person / collection。
    pub item_type: String,
    pub name: Option<String>,
    pub year: Option<i32>,
    pub language: Option<String>,
    pub country: Option<String>,
    /// 显式 TMDB id（SearchInfo.ProviderIds.Tmdb），命中时直查详情。
    pub tmdb_id: Option<String>,
}

/// 单个候选。`provider_key` 是 Emby ProviderIds 字典键（如 "Tmdb"）。
#[derive(Clone, Debug, PartialEq)]
pub struct RemoteSearchCandidate {
    pub provider: String,
    pub provider_key: String,
    pub external_id: String,
    pub name: String,
    pub original_title: Option<String>,
    pub production_year: Option<i32>,
    pub premiere_date: Option<String>,
    pub overview: Option<String>,
    pub image_url: Option<String>,
}

#[derive(Debug)]
pub enum RemoteSearchError {
    Settings(String),
    Http(String),
}

impl std::fmt::Display for RemoteSearchError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Settings(message) => write!(f, "metadata settings error: {message}"),
            Self::Http(message) => write!(f, "remote search request failed: {message}"),
        }
    }
}

pub struct RemoteSearchService {
    pool: DbPool,
    base_metadata: MetadataConfig,
    proxy: ProxyConfig,
    secrets: SecretConfig,
}

impl RemoteSearchService {
    pub fn new(
        pool: DbPool,
        base_metadata: MetadataConfig,
        proxy: ProxyConfig,
        secrets: SecretConfig,
    ) -> Self {
        Self {
            pool,
            base_metadata,
            proxy,
            secrets,
        }
    }

    /// 搜索候选。TMDB 未配置或类型不支持 → Ok(空)。
    pub async fn search(
        &self,
        request: &RemoteSearchRequest,
    ) -> Result<Vec<RemoteSearchCandidate>, RemoteSearchError> {
        let Some(kind) = TmdbSearchTarget::from_item_type(&request.item_type) else {
            return Ok(Vec::new());
        };

        let cipher = SecretCipher::from_config(&self.secrets).ok();
        let resolved = MetadataSettingsRepository::new(self.pool.clone())
            .resolve(cipher.as_ref())
            .await
            .map_err(|err| RemoteSearchError::Settings(err.to_string()))?;
        let effective = resolve_metadata_config(&self.base_metadata, &resolved);
        let Some(token) = effective.tmdb_access_token.clone() else {
            return Ok(Vec::new());
        };
        let proxy_override = resolved
            .providers
            .get("tmdb")
            .map(|settings| ProviderProxyOverride {
                mode: settings.proxy_mode.clone(),
                url: settings.proxy_url.clone(),
            });
        let client = build_single_client(&self.proxy, proxy_override.as_ref())
            .map_err(|err| RemoteSearchError::Http(err.to_string()))?;

        // 显式 TMDB id：直查详情，返回单候选。
        if let Some(tmdb_id) = request
            .tmdb_id
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
        {
            if !tmdb_id.chars().all(|ch| ch.is_ascii_digit()) {
                return Ok(Vec::new());
            }
            let url = format!(
                "{}/{}/{tmdb_id}",
                effective.tmdb_api_base_url.trim_end_matches('/'),
                kind.detail_segment()
            );
            let mut query: Vec<(&str, String)> = Vec::new();
            if let Some(language) = normalized_bcp47(request.language.as_deref()) {
                query.push(("language", language));
            }
            let detail = client
                .get(url)
                .bearer_auth(&token)
                .query(&query)
                .send()
                .await
                .map_err(|err| RemoteSearchError::Http(err.to_string()))?;
            if !detail.status().is_success() {
                return Ok(Vec::new());
            }
            let result = detail
                .json::<TmdbSearchResult>()
                .await
                .map_err(|err| RemoteSearchError::Http(err.to_string()))?;
            return Ok(candidate_from_result(result, kind, &effective.tmdb_image_base_url)
                .into_iter()
                .collect());
        }

        let Some(name) = request
            .name
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
        else {
            return Ok(Vec::new());
        };

        let url = format!(
            "{}/search/{}",
            effective.tmdb_api_base_url.trim_end_matches('/'),
            kind.search_segment()
        );
        let mut query: Vec<(&str, String)> = vec![
            ("query", name.to_owned()),
            ("include_adult", "false".to_owned()),
        ];
        if let Some(year) = request.year {
            query.push((kind.year_param(), year.to_string()));
        }
        if let Some(language) = normalized_bcp47(request.language.as_deref()) {
            query.push(("language", language));
        }
        if let Some(country) = request
            .country
            .as_deref()
            .map(str::trim)
            .filter(|value| value.len() == 2 && value.chars().all(|ch| ch.is_ascii_alphabetic()))
        {
            query.push(("region", country.to_ascii_uppercase()));
        }

        let response = client
            .get(url)
            .bearer_auth(&token)
            .query(&query)
            .send()
            .await
            .map_err(|err| RemoteSearchError::Http(err.to_string()))?;
        if !response.status().is_success() {
            return Err(RemoteSearchError::Http(format!(
                "TMDB returned status {}",
                response.status()
            )));
        }
        let payload = response
            .json::<TmdbSearchResponse>()
            .await
            .map_err(|err| RemoteSearchError::Http(err.to_string()))?;

        Ok(payload
            .results
            .into_iter()
            .filter(|result| result.id > 0)
            .take(REMOTE_SEARCH_RESULT_LIMIT)
            .filter_map(|result| {
                candidate_from_result(result, kind, &effective.tmdb_image_base_url)
            })
            .collect())
    }
}

/// TMDB /images 的单张候选图（`/Items/{id}/RemoteImages` 用）。
#[derive(Clone, Debug, PartialEq)]
pub struct RemoteImageCandidate {
    /// Emby 图片类型：Primary / Backdrop / Logo。
    pub image_type: String,
    pub url: String,
    pub thumbnail_url: Option<String>,
    pub width: Option<u32>,
    pub height: Option<u32>,
    pub language: Option<String>,
    pub community_rating: Option<f32>,
    pub vote_count: Option<u32>,
}

impl RemoteSearchService {
    /// 列出条目在 TMDB 的候选图片（posters→Primary、backdrops→Backdrop、logos→Logo）。
    /// TMDB 未配置 key、类型不支持或 id 非数字 → Ok(空)。
    pub async fn list_images(
        &self,
        item_type: &str,
        tmdb_id: &str,
    ) -> Result<Vec<RemoteImageCandidate>, RemoteSearchError> {
        let Some(kind) = TmdbSearchTarget::from_item_type(item_type) else {
            return Ok(Vec::new());
        };
        let tmdb_id = tmdb_id.trim();
        if tmdb_id.is_empty() || !tmdb_id.chars().all(|ch| ch.is_ascii_digit()) {
            return Ok(Vec::new());
        }

        let cipher = SecretCipher::from_config(&self.secrets).ok();
        let resolved = MetadataSettingsRepository::new(self.pool.clone())
            .resolve(cipher.as_ref())
            .await
            .map_err(|err| RemoteSearchError::Settings(err.to_string()))?;
        let effective = resolve_metadata_config(&self.base_metadata, &resolved);
        let Some(token) = effective.tmdb_access_token.clone() else {
            return Ok(Vec::new());
        };
        let proxy_override = resolved
            .providers
            .get("tmdb")
            .map(|settings| ProviderProxyOverride {
                mode: settings.proxy_mode.clone(),
                url: settings.proxy_url.clone(),
            });
        let client = build_single_client(&self.proxy, proxy_override.as_ref())
            .map_err(|err| RemoteSearchError::Http(err.to_string()))?;

        let url = format!(
            "{}/{}/{tmdb_id}/images",
            effective.tmdb_api_base_url.trim_end_matches('/'),
            kind.detail_segment()
        );
        let response = client
            .get(url)
            .bearer_auth(&token)
            .send()
            .await
            .map_err(|err| RemoteSearchError::Http(err.to_string()))?;
        if !response.status().is_success() {
            return Err(RemoteSearchError::Http(format!(
                "TMDB returned status {}",
                response.status()
            )));
        }
        let payload = response
            .json::<TmdbImagesResponse>()
            .await
            .map_err(|err| RemoteSearchError::Http(err.to_string()))?;

        let image_base = effective.tmdb_image_base_url.clone();
        let mut candidates = Vec::new();
        for (bucket, image_type) in [
            (payload.posters, "Primary"),
            (payload.backdrops, "Backdrop"),
            (payload.logos, "Logo"),
        ] {
            for image in bucket {
                let Some(file_path) = image
                    .file_path
                    .as_deref()
                    .map(str::trim)
                    .filter(|value| !value.is_empty())
                else {
                    continue;
                };
                let base = image_base.trim_end_matches('/');
                let path = ensure_leading_slash(file_path);
                candidates.push(RemoteImageCandidate {
                    image_type: image_type.to_owned(),
                    url: format!("{base}/original{path}"),
                    thumbnail_url: Some(format!("{base}/w342{path}")),
                    width: image.width,
                    height: image.height,
                    language: image
                        .iso_639_1
                        .as_deref()
                        .map(str::trim)
                        .filter(|value| !value.is_empty())
                        .map(str::to_owned),
                    community_rating: image.vote_average,
                    vote_count: image.vote_count,
                });
            }
        }

        Ok(candidates)
    }
}

#[derive(Debug, Default, Deserialize)]
struct TmdbImagesResponse {
    #[serde(default)]
    posters: Vec<TmdbImageEntry>,
    #[serde(default)]
    backdrops: Vec<TmdbImageEntry>,
    #[serde(default)]
    logos: Vec<TmdbImageEntry>,
}

#[derive(Debug, Default, Deserialize)]
struct TmdbImageEntry {
    file_path: Option<String>,
    width: Option<u32>,
    height: Option<u32>,
    iso_639_1: Option<String>,
    vote_average: Option<f32>,
    vote_count: Option<u32>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum TmdbSearchTarget {
    Movie,
    Series,
    Person,
    Collection,
}

impl TmdbSearchTarget {
    fn from_item_type(item_type: &str) -> Option<Self> {
        match item_type.trim().to_ascii_lowercase().as_str() {
            "movie" | "trailer" | "musicvideo" => Some(Self::Movie),
            "series" => Some(Self::Series),
            "person" => Some(Self::Person),
            "collection" | "boxset" => Some(Self::Collection),
            _ => None,
        }
    }

    fn search_segment(self) -> &'static str {
        match self {
            Self::Movie => "movie",
            Self::Series => "tv",
            Self::Person => "person",
            Self::Collection => "collection",
        }
    }

    fn detail_segment(self) -> &'static str {
        match self {
            Self::Movie => "movie",
            Self::Series => "tv",
            Self::Person => "person",
            Self::Collection => "collection",
        }
    }

    fn year_param(self) -> &'static str {
        match self {
            Self::Series => "first_air_date_year",
            _ => "year",
        }
    }
}

#[derive(Debug, Default, Deserialize)]
struct TmdbSearchResponse {
    #[serde(default)]
    results: Vec<TmdbSearchResult>,
}

#[derive(Debug, Default, Deserialize)]
struct TmdbSearchResult {
    #[serde(default)]
    id: i64,
    title: Option<String>,
    name: Option<String>,
    original_title: Option<String>,
    original_name: Option<String>,
    release_date: Option<String>,
    first_air_date: Option<String>,
    overview: Option<String>,
    poster_path: Option<String>,
    profile_path: Option<String>,
}

fn candidate_from_result(
    result: TmdbSearchResult,
    kind: TmdbSearchTarget,
    image_base_url: &str,
) -> Option<RemoteSearchCandidate> {
    if result.id <= 0 {
        return None;
    }
    let name = result
        .title
        .or(result.name)
        .map(|value| value.trim().to_owned())
        .filter(|value| !value.is_empty())?;
    let premiere_date = result
        .release_date
        .or(result.first_air_date)
        .map(|value| value.trim().to_owned())
        .filter(|value| !value.is_empty());
    let production_year = premiere_date
        .as_deref()
        .and_then(|date| date.get(0..4))
        .and_then(|year| year.parse::<i32>().ok());
    let image_path = result.poster_path.or(result.profile_path);
    let image_url = image_path
        .map(|path| {
            format!(
                "{}/w342{}",
                image_base_url.trim_end_matches('/'),
                ensure_leading_slash(path.trim())
            )
        })
        .filter(|_| !image_base_url.trim().is_empty());
    let _ = kind;

    Some(RemoteSearchCandidate {
        provider: "tmdb".to_owned(),
        provider_key: "Tmdb".to_owned(),
        external_id: result.id.to_string(),
        name,
        original_title: result
            .original_title
            .or(result.original_name)
            .map(|value| value.trim().to_owned())
            .filter(|value| !value.is_empty()),
        production_year,
        premiere_date,
        overview: result
            .overview
            .map(|value| value.trim().to_owned())
            .filter(|value| !value.is_empty()),
        image_url,
    })
}

fn ensure_leading_slash(path: &str) -> String {
    if path.starts_with('/') {
        path.to_owned()
    } else {
        format!("/{path}")
    }
}

/// language 归一化：只放行 `xx` / `xx-YY` 形状，防查询串注入垃圾。
fn normalized_bcp47(value: Option<&str>) -> Option<String> {
    let value = value?.trim();
    let valid = match value.len() {
        2 => value.chars().all(|ch| ch.is_ascii_alphabetic()),
        5 => {
            let bytes = value.as_bytes();
            bytes[2] == b'-'
                && value[0..2].chars().all(|ch| ch.is_ascii_alphabetic())
                && value[3..5].chars().all(|ch| ch.is_ascii_alphabetic())
        }
        _ => false,
    };

    valid.then(|| {
        if value.len() == 2 {
            value.to_ascii_lowercase()
        } else {
            format!(
                "{}-{}",
                value[0..2].to_ascii_lowercase(),
                value[3..5].to_ascii_uppercase()
            )
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn search_target_maps_emby_kinds() {
        assert_eq!(
            TmdbSearchTarget::from_item_type("movie"),
            Some(TmdbSearchTarget::Movie)
        );
        assert_eq!(
            TmdbSearchTarget::from_item_type("Series"),
            Some(TmdbSearchTarget::Series)
        );
        assert_eq!(
            TmdbSearchTarget::from_item_type("person"),
            Some(TmdbSearchTarget::Person)
        );
        assert_eq!(
            TmdbSearchTarget::from_item_type("boxset"),
            Some(TmdbSearchTarget::Collection)
        );
        assert_eq!(TmdbSearchTarget::from_item_type("musicalbum"), None);
    }

    #[test]
    fn candidate_maps_movie_result_shape() {
        let candidate = candidate_from_result(
            TmdbSearchResult {
                id: 603,
                title: Some("The Matrix".to_owned()),
                original_title: Some("The Matrix".to_owned()),
                release_date: Some("1999-03-30".to_owned()),
                overview: Some("A hacker...".to_owned()),
                poster_path: Some("/poster.jpg".to_owned()),
                ..TmdbSearchResult::default()
            },
            TmdbSearchTarget::Movie,
            "https://image.tmdb.org/t/p",
        )
        .expect("candidate should map");

        assert_eq!(candidate.provider, "tmdb");
        assert_eq!(candidate.provider_key, "Tmdb");
        assert_eq!(candidate.external_id, "603");
        assert_eq!(candidate.name, "The Matrix");
        assert_eq!(candidate.production_year, Some(1999));
        assert_eq!(
            candidate.image_url.as_deref(),
            Some("https://image.tmdb.org/t/p/w342/poster.jpg")
        );
    }

    #[test]
    fn candidate_requires_id_and_name() {
        assert!(
            candidate_from_result(
                TmdbSearchResult::default(),
                TmdbSearchTarget::Movie,
                "https://image.tmdb.org/t/p",
            )
            .is_none()
        );
    }

    #[test]
    fn bcp47_normalization_is_shape_guarded() {
        assert_eq!(normalized_bcp47(Some("zh-cn")).as_deref(), Some("zh-CN"));
        assert_eq!(normalized_bcp47(Some("EN")).as_deref(), Some("en"));
        assert_eq!(normalized_bcp47(Some("english")), None);
        assert_eq!(normalized_bcp47(Some("zh_CN")), None);
        assert_eq!(normalized_bcp47(None), None);
    }
}
