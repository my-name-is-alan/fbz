//! TMDB metadata provider.
//!
//! Base-match provider for movies and TV series. Searches TMDB, then enriches
//! the top result with detail data (genres, studios, people, certifications,
//! external ids, artwork). Migrated from the legacy `provider.rs` with no
//! behavioral change.

use async_trait::async_trait;
use serde::Deserialize;

use super::shared::*;
use super::{MetadataProvider, ProviderContext, ProviderRole};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TmdbSearchKind {
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

/// TMDB base-match provider. Stateless: all config flows through the context.
#[derive(Clone, Default)]
pub struct TmdbProvider;

impl TmdbProvider {
    pub fn new() -> Self {
        Self
    }

    async fn search_tmdb(
        &self,
        ctx: &ProviderContext,
        input: &MetadataLookup,
        token: &str,
        search_kind: TmdbSearchKind,
    ) -> Result<Option<MetadataMatch>, MetadataProviderError> {
        let metadata = &ctx.metadata;
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

        let response = ctx
            .client("tmdb")
            .get(tmdb_search_url(&metadata.tmdb_api_base_url, search_kind))
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
        let Some(found) = tmdb_result_to_match(result, &metadata.tmdb_image_base_url) else {
            return Ok(None);
        };
        // 富化（detail + 本地化 artwork + episode 下钻）与 direct-by-id 共用。
        let found = self
            .enrich_from_id(ctx, input, token, search_kind, id, found)
            .await?;
        Ok(Some(found))
    }

    /// 从已知 TMDB id 富化一个 match：detail（标题/简介/年份/外部 id/演职员）+ 本地化 artwork
    /// + episode 下钻。`base` 是初始 match（search 给的部分结果，或 direct-by-id 的空壳）。
    /// search 与「显式 `{tmdb-XXX}` 直查」共用此路径。
    async fn enrich_from_id(
        &self,
        ctx: &ProviderContext,
        input: &MetadataLookup,
        token: &str,
        search_kind: TmdbSearchKind,
        id: i64,
        mut found: MetadataMatch,
    ) -> Result<MetadataMatch, MetadataProviderError> {
        let metadata = &ctx.metadata;
        let detail = self
            .fetch_tmdb_detail(ctx, input, token, search_kind, id)
            .await?;
        let original_language = detail.original_language.clone();
        apply_tmdb_detail(
            &mut found,
            detail,
            &metadata.tmdb_image_base_url,
            input.country.as_deref(),
            search_kind,
        );

        // Localized artwork selection (§7): fetch /images and pick poster/
        // backdrop by the image-language policy, independent of text language.
        let policy = image_policy(input, original_language.as_deref());
        if let Ok(images) = self
            .fetch_tmdb_images(ctx, token, search_kind, id, &policy)
            .await
        {
            apply_tmdb_localized_artwork(
                &mut found,
                images,
                &metadata.tmdb_image_base_url,
                &policy,
            );
        }

        // Episode 下钻（§6.3）：识别层填了 season/episode 时，搜到的是 series 级元数据，
        // 再调单集端点拿真正的集标题/简介/播出日期/剧照覆盖。下钻失败（如该集未收录）
        // 不致命——保留 series 级 match，best-effort。
        if input.item_type == "episode"
            && let (Some(season), Some(episode)) = (input.season, input.episode)
            && let Ok(ep) = self
                .fetch_tmdb_episode(ctx, input, token, id, season, episode)
                .await
        {
            apply_tmdb_episode(&mut found, ep, &metadata.tmdb_image_base_url);
        }

        // Season 下钻：season 容器富化，调 `/tv/{id}/season/{n}` 拿季标题/简介/海报覆盖。
        // 同 episode：series_title 存剧名，title 用季名。best-effort。
        if input.item_type == "season"
            && let Some(season) = input.season
            && let Ok(se) = self.fetch_tmdb_season(ctx, input, token, id, season).await
        {
            apply_tmdb_season(&mut found, se, &metadata.tmdb_image_base_url);
        }

        Ok(found)
    }

    /// 直接按显式 TMDB id 拉 match（跳过搜索）。id 非法/非数字返回 None（退回搜索）。
    async fn fetch_by_tmdb_id(
        &self,
        ctx: &ProviderContext,
        input: &MetadataLookup,
        token: &str,
        search_kind: TmdbSearchKind,
        tmdb_id: &str,
    ) -> Result<Option<MetadataMatch>, MetadataProviderError> {
        let Ok(id) = tmdb_id.trim().parse::<i64>() else {
            return Ok(None); // 非数字 id（不该发生）：退回搜索。
        };
        if id <= 0 {
            return Ok(None);
        }
        // 初始空壳 match：detail 富化会填齐标题/年份等。external_id 先置为该 id。
        let base = MetadataMatch {
            provider: "tmdb".to_owned(),
            external_id: id.to_string(),
            external_ids: Vec::new(),
            title: String::new(),
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
        let found = self
            .enrich_from_id(ctx, input, token, search_kind, id, base)
            .await?;
        // detail 没填出标题 = id 无效/被删：当未匹配处理。
        if found.title.trim().is_empty() {
            return Ok(None);
        }
        Ok(Some(found))
    }

    async fn fetch_tmdb_images(
        &self,
        ctx: &ProviderContext,
        token: &str,
        search_kind: TmdbSearchKind,
        id: i64,
        policy: &ImageLanguagePolicy,
    ) -> Result<TmdbImagesResponse, MetadataProviderError> {
        let mut query = Vec::new();
        let include = tmdb_include_image_language(policy);
        if !include.is_empty() {
            query.push(("include_image_language", include));
        }

        ctx.client("tmdb")
            .get(tmdb_images_url(
                &ctx.metadata.tmdb_api_base_url,
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
            .json::<TmdbImagesResponse>()
            .await
            .map_err(MetadataProviderError::Http)
    }

    async fn fetch_tmdb_detail(
        &self,
        ctx: &ProviderContext,
        input: &MetadataLookup,
        token: &str,
        search_kind: TmdbSearchKind,
        id: i64,
    ) -> Result<TmdbDetailResponse, MetadataProviderError> {
        let mut query = vec![("append_to_response", tmdb_detail_appends(search_kind))];
        if let Some(language) = input.language.as_deref().and_then(normalize_language) {
            query.push(("language", language));
        }

        ctx.client("tmdb")
            .get(tmdb_detail_url(
                &ctx.metadata.tmdb_api_base_url,
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

    /// 下钻到 TMDB 单集详情（`/tv/{series_id}/season/{s}/episode/{e}`）。
    /// 用于 episode 类型在搜到 series id 后取真正的单集元数据。
    async fn fetch_tmdb_episode(
        &self,
        ctx: &ProviderContext,
        input: &MetadataLookup,
        token: &str,
        series_id: i64,
        season: i32,
        episode: i32,
    ) -> Result<TmdbEpisodeResponse, MetadataProviderError> {
        let mut query: Vec<(&str, String)> = Vec::new();
        if let Some(language) = input.language.as_deref().and_then(normalize_language) {
            query.push(("language", language));
        }

        ctx.client("tmdb")
            .get(tmdb_episode_url(
                &ctx.metadata.tmdb_api_base_url,
                series_id,
                season,
                episode,
            ))
            .bearer_auth(token)
            .query(&query)
            .send()
            .await
            .map_err(MetadataProviderError::Http)?
            .error_for_status()
            .map_err(MetadataProviderError::Http)?
            .json::<TmdbEpisodeResponse>()
            .await
            .map_err(MetadataProviderError::Http)
    }

    /// 下钻到 TMDB 季详情（`/tv/{series_id}/season/{s}`）。用于 season 容器富化。
    async fn fetch_tmdb_season(
        &self,
        ctx: &ProviderContext,
        input: &MetadataLookup,
        token: &str,
        series_id: i64,
        season: i32,
    ) -> Result<TmdbSeasonResponse, MetadataProviderError> {
        let mut query: Vec<(&str, String)> = Vec::new();
        if let Some(language) = input.language.as_deref().and_then(normalize_language) {
            query.push(("language", language));
        }

        ctx.client("tmdb")
            .get(tmdb_season_url(
                &ctx.metadata.tmdb_api_base_url,
                series_id,
                season,
            ))
            .bearer_auth(token)
            .query(&query)
            .send()
            .await
            .map_err(MetadataProviderError::Http)?
            .error_for_status()
            .map_err(MetadataProviderError::Http)?
            .json::<TmdbSeasonResponse>()
            .await
            .map_err(MetadataProviderError::Http)
    }
}

#[async_trait]
impl MetadataProvider for TmdbProvider {
    fn id(&self) -> &str {
        "tmdb"
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
        // 令牌池优先（多 key 轮转）；无池时回退 metadata 的单 token。
        let lease = ctx.token_pool("tmdb").and_then(|pool| pool.acquire());
        let token = match lease.as_ref() {
            Some(lease) => lease.token.as_str(),
            None => {
                let Some(token) = ctx.metadata.tmdb_access_token.as_deref() else {
                    return Ok(ProviderMatchOutcome::Skipped(
                        "missing TMDB access token".to_owned(),
                    ));
                };
                token
            }
        };
        let Some(search_kind) = tmdb_search_kind(&input.item_type) else {
            return Ok(ProviderMatchOutcome::Skipped(format!(
                "unsupported item type `{}`",
                input.item_type
            )));
        };

        // 显式 `{tmdb-XXX}` id 优先：直接按 id 拉详情，零歧义、跳过模糊搜索（Emby 刮削首选）。
        // 直查无果（id 无效/被删）才退回标题搜索。
        let lookup_result = match input.tmdb_id.as_deref() {
            Some(tmdb_id) if !tmdb_id.trim().is_empty() => {
                match self
                    .fetch_by_tmdb_id(ctx, input, token, search_kind, tmdb_id)
                    .await
                {
                    Ok(Some(found)) => Ok(Some(found)),
                    Ok(None) => self.search_tmdb(ctx, input, token, search_kind).await,
                    Err(err) => Err(err),
                }
            }
            _ => self.search_tmdb(ctx, input, token, search_kind).await,
        };

        match lookup_result {
            Ok(Some(found)) => Ok(ProviderMatchOutcome::Matched(Box::new(found))),
            Ok(None) => Ok(ProviderMatchOutcome::NotMatched(
                "no TMDB search result".to_owned(),
            )),
            Err(err) => {
                // 429：标记该 token 冷却，让 registry 的 retry 重入时轮转到下一个 key。
                if let (Some(lease), Some(pool)) = (lease.as_ref(), ctx.token_pool("tmdb"))
                    && is_rate_limited(&err)
                {
                    pool.mark_rate_limited(lease.index, TMDB_RATE_LIMIT_COOLDOWN);
                }
                Err(err)
            }
        }
    }
}

/// TMDB 限流（429）后单个 key 的冷却时长。
const TMDB_RATE_LIMIT_COOLDOWN: std::time::Duration = std::time::Duration::from_secs(10);

/// 错误是否为限流（429），用于令牌池冷却标记。
fn is_rate_limited(err: &MetadataProviderError) -> bool {
    matches!(
        err,
        MetadataProviderError::Http(http)
            if http.status().map(|s| s.as_u16()) == Some(429)
    )
}

// ---------------------------------------------------------------------------
// TMDB-specific helpers (migrated verbatim from provider.rs).
// ---------------------------------------------------------------------------

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
        TmdbSearchKind::Movie => "credits,release_dates,external_ids,videos",
        TmdbSearchKind::Tv => "credits,content_ratings,external_ids,videos",
    }
    .to_owned()
}

/// TMDB 单集详情端点 URL：`/tv/{series_id}/season/{s}/episode/{e}`。纯函数，可单测。
fn tmdb_episode_url(base_url: &str, series_id: i64, season: i32, episode: i32) -> String {
    format!(
        "{}/tv/{}/season/{}/episode/{}",
        base_url.trim_end_matches('/'),
        series_id,
        season,
        episode
    )
}

/// TMDB 季详情端点 URL：`/tv/{series_id}/season/{s}`。纯函数，可单测。
fn tmdb_season_url(base_url: &str, series_id: i64, season: i32) -> String {
    format!(
        "{}/tv/{}/season/{}",
        base_url.trim_end_matches('/'),
        series_id,
        season
    )
}

pub fn tmdb_search_kind(item_type: &str) -> Option<TmdbSearchKind> {
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
        series_title: None,
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
        networks: Vec::new(),
        videos: Vec::new(),
        collection: None,
        people: Vec::new(),
    })
}

/// 把 TMDB 单集详情覆盖到剧集级 match：**剧名先存入 `series_title`**，再用集标题覆盖 `title`
/// （Emby episode 需要 `Name`=单集名 + `SeriesName`=剧名两者）；单集简介/播出日期/评分覆盖，
/// 剧照（still）置为 primary artwork（追加，不删除剧集级海报/背景）。纯函数，可单测。
/// 单集字段缺失时保留原值（不清空），与 `apply_tmdb_detail` 的「尽力覆盖」语义一致。
fn apply_tmdb_episode(
    found: &mut MetadataMatch,
    episode: TmdbEpisodeResponse,
    image_base_url: &str,
) {
    if let Some(title) = episode.name.and_then(normalize_text_title) {
        // 覆盖前把当前 title（剧名，来自 tv detail）存为 series_title，避免剧名被单集名吞掉。
        if found.series_title.is_none() && !found.title.trim().is_empty() {
            found.series_title = Some(found.title.clone());
        }
        found.title = title;
    }
    if let Some(overview) = episode.overview.and_then(normalize_overview) {
        found.overview = Some(overview);
    }
    if let Some(air_date) = episode.air_date.and_then(normalize_tmdb_date) {
        found.production_year = air_date.get(..4).and_then(|y| y.parse::<i32>().ok());
        found.premiere_date = Some(air_date);
    }
    if let Some(rating) = episode.vote_average {
        found.community_rating = Some(rating.clamp(0.0, 10.0));
    }
    // 单集剧照作为 primary 海报（追加在前，标 primary；原剧集级 artwork 保留为非 primary）。
    if let Some(remote_url) = tmdb_image_url(image_base_url, episode.still_path.as_deref()) {
        for art in found.artwork.iter_mut() {
            art.is_primary = false;
        }
        found.artwork.insert(
            0,
            MetadataArtwork {
                artwork_type: "primary".to_owned(),
                source: None,
                remote_url,
                is_primary: true,
            },
        );
    }
}

/// 把 TMDB 季详情覆盖到 season 容器 match：**剧名先存入 `series_title`**，季名覆盖 `title`
/// （Emby season 需要 `Name`=季名 + `SeriesName`=剧名）；季简介/播出日期覆盖，季海报置 primary。
/// 季字段缺失时保留原值。纯函数，可单测。
fn apply_tmdb_season(found: &mut MetadataMatch, season: TmdbSeasonResponse, image_base_url: &str) {
    if let Some(title) = season.name.and_then(normalize_text_title) {
        if found.series_title.is_none() && !found.title.trim().is_empty() {
            found.series_title = Some(found.title.clone());
        }
        found.title = title;
    }
    if let Some(overview) = season.overview.and_then(normalize_overview) {
        found.overview = Some(overview);
    }
    if let Some(air_date) = season.air_date.and_then(normalize_tmdb_date) {
        found.production_year = air_date.get(..4).and_then(|y| y.parse::<i32>().ok());
        found.premiere_date = Some(air_date);
    }
    if let Some(remote_url) = tmdb_image_url(image_base_url, season.poster_path.as_deref()) {
        for art in found.artwork.iter_mut() {
            art.is_primary = false;
        }
        found.artwork.insert(
            0,
            MetadataArtwork {
                artwork_type: "poster".to_owned(),
                source: None,
                remote_url,
                is_primary: true,
            },
        );
    }
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
    found.networks = tmdb_networks(detail.networks);
    found.videos = tmdb_videos(detail.videos);
    found.collection = tmdb_collection(detail.belongs_to_collection);
    found.people = tmdb_people(detail.credits, image_base_url);
}

/// TMDB belongs_to_collection → MetadataCollection（电影所属系列）。无名返回 None。
fn tmdb_collection(collection: Option<TmdbBelongsToCollection>) -> Option<MetadataCollection> {
    let name = collection?.name?.trim().to_owned();
    if name.is_empty() {
        return None;
    }
    let name_normalized = name.to_lowercase();
    Some(MetadataCollection {
        name,
        name_normalized,
        overview: None,
    })
}

/// TMDB networks → MetadataNamedValue（播出平台：Netflix / 爱奇艺 / Disney+ 等）。
fn tmdb_networks(networks: Vec<TmdbNetwork>) -> Vec<MetadataNamedValue> {
    networks
        .into_iter()
        .filter_map(|n| n.name)
        .filter_map(|name| {
            let name = name.trim().to_owned();
            (!name.is_empty()).then(|| {
                let name_normalized = name.to_lowercase();
                MetadataNamedValue {
                    name,
                    name_normalized,
                }
            })
        })
        .collect()
}

/// TMDB videos → MetadataVideo（主题曲 / 宣传片 / 预告）。映射 TMDB type 到内部 video_type，
/// 拼出可打开的完整 URL（YouTube/Vimeo），去掉无 key 的条目。
fn tmdb_videos(videos: Option<TmdbVideos>) -> Vec<MetadataVideo> {
    let Some(videos) = videos else {
        return Vec::new();
    };
    videos
        .results
        .into_iter()
        .enumerate()
        .filter_map(|(idx, v)| {
            let site = v
                .site
                .map(|s| s.trim().to_owned())
                .filter(|s| !s.is_empty());
            let key = v
                .key
                .map(|k| k.trim().to_owned())
                .filter(|k| !k.is_empty())?;
            let video_type = normalize_tmdb_video_type(v.video_type.as_deref());
            let url = tmdb_video_url(site.as_deref(), &key);
            Some(MetadataVideo {
                video_type,
                name: v
                    .name
                    .map(|n| n.trim().to_owned())
                    .filter(|n| !n.is_empty()),
                site,
                site_key: Some(key),
                url,
                is_official: v.official,
                sort_order: idx as i32,
            })
        })
        .collect()
}

/// TMDB video type 字符串 → 内部 video_type（media_videos CHECK allowlist）。未知归 clip。
fn normalize_tmdb_video_type(value: Option<&str>) -> String {
    match value.unwrap_or("").to_ascii_lowercase().as_str() {
        "trailer" => "trailer",
        "teaser" => "teaser",
        "featurette" => "featurette",
        "behind the scenes" => "behind_the_scenes",
        "clip" => "clip",
        "opening credits" => "opening_theme",
        _ => "clip",
    }
    .to_owned()
}

/// 由 site + key 拼可打开的视频 URL（YouTube / Vimeo / Bilibili），未知站点返回 None。
fn tmdb_video_url(site: Option<&str>, key: &str) -> Option<String> {
    match site.unwrap_or("").to_ascii_lowercase().as_str() {
        "youtube" => Some(format!("https://www.youtube.com/watch?v={key}")),
        "vimeo" => Some(format!("https://vimeo.com/{key}")),
        "bilibili" => Some(format!("https://www.bilibili.com/video/{key}")),
        _ => None,
    }
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

/// TMDB 人物头像 URL：与 `tmdb_image_url` 同样的校验，但使用 profile
/// 专用尺寸 `w185`（海报/背景走 `original`，头像走 `w185`）。
fn tmdb_profile_image_url(base_url: &str, path: Option<&str>) -> Option<String> {
    let path = path?.trim();
    if path.is_empty()
        || !path.starts_with('/')
        || path.contains(char::is_whitespace)
        || path.contains("..")
    {
        return None;
    }
    Some(format!("{}/w185{}", base_url.trim_end_matches('/'), path))
}

fn tmdb_images_url(base_url: &str, search_kind: TmdbSearchKind, id: i64) -> String {
    format!(
        "{}/{}/{}/images",
        base_url.trim_end_matches('/'),
        search_kind.as_path(),
        id
    )
}

/// Builds the image-language policy from the lookup + the item's original
/// language (from TMDB detail).
fn image_policy(input: &MetadataLookup, original_language: Option<&str>) -> ImageLanguagePolicy {
    ImageLanguagePolicy {
        original_language: original_language.map(str::to_owned),
        image_language: input.effective_image_language().map(str::to_owned),
        prefer_original: input.image_prefer_original,
        fallback_languages: input.image_fallback_languages.clone(),
    }
}

/// The `include_image_language` value: every language the policy cares about,
/// plus `null` for textless artwork. Comma-joined primary subtags.
fn tmdb_include_image_language(policy: &ImageLanguagePolicy) -> String {
    let mut parts: Vec<String> = Vec::new();
    if policy.prefer_original {
        if let Some(original) = policy
            .original_language
            .as_deref()
            .and_then(image_language_primary_subtag)
        {
            push_unique(&mut parts, original);
        }
    }
    if let Some(language) = policy
        .image_language
        .as_deref()
        .and_then(image_language_primary_subtag)
    {
        push_unique(&mut parts, language);
    }
    let mut wants_textless = false;
    for fallback in &policy.fallback_languages {
        if is_textless_token(fallback) {
            wants_textless = true;
        } else if let Some(language) = image_language_primary_subtag(fallback) {
            push_unique(&mut parts, language);
        }
    }
    if wants_textless || parts.is_empty() {
        // `null` selects textless artwork; also request it when nothing else is
        // specified so the call still returns the default images.
        push_unique(&mut parts, "null".to_owned());
    }
    parts.join(",")
}

fn push_unique(parts: &mut Vec<String>, value: String) {
    if !parts.contains(&value) {
        parts.push(value);
    }
}

/// Replaces the match's primary poster/backdrop with the best localized choices
/// from `/images`, keeping the detail artwork as fallback when none rank.
fn apply_tmdb_localized_artwork(
    found: &mut MetadataMatch,
    images: TmdbImagesResponse,
    image_base_url: &str,
    policy: &ImageLanguagePolicy,
) {
    let poster = pick_localized_tmdb_image(&images.posters, policy);
    let backdrop = pick_localized_tmdb_image(&images.backdrops, policy);

    let poster_url =
        poster.and_then(|image| tmdb_image_url(image_base_url, image.file_path.as_deref()));
    let backdrop_url =
        backdrop.and_then(|image| tmdb_image_url(image_base_url, image.file_path.as_deref()));

    if poster_url.is_none() && backdrop_url.is_none() {
        return;
    }

    let mut artwork = Vec::new();
    if let Some(remote_url) = poster_url {
        artwork.push(MetadataArtwork {
            artwork_type: "poster".to_owned(),
            source: None,
            remote_url,
            is_primary: true,
        });
    }
    if let Some(remote_url) = backdrop_url {
        artwork.push(MetadataArtwork {
            artwork_type: "backdrop".to_owned(),
            source: None,
            remote_url,
            is_primary: true,
        });
    }
    found.artwork = artwork;
}

/// Picks the best-ranked image: lowest language rank, then highest vote average.
fn pick_localized_tmdb_image<'a>(
    images: &'a [TmdbImage],
    policy: &ImageLanguagePolicy,
) -> Option<&'a TmdbImage> {
    images
        .iter()
        .filter(|image| {
            image
                .file_path
                .as_deref()
                .is_some_and(|path| path.trim().starts_with('/'))
        })
        .min_by(|left, right| {
            let left_rank = image_language_rank(policy, left.iso_639_1.as_deref());
            let right_rank = image_language_rank(policy, right.iso_639_1.as_deref());
            left_rank.cmp(&right_rank).then_with(|| {
                right
                    .vote_average
                    .unwrap_or(0.0)
                    .total_cmp(&left.vote_average.unwrap_or(0.0))
            })
        })
}

fn tmdb_genres(values: Vec<TmdbGenre>) -> Vec<MetadataNamedValue> {
    dedupe_named_values(values.into_iter().map(|value| value.name))
}

fn tmdb_studios(values: Vec<TmdbProductionCompany>) -> Vec<MetadataNamedValue> {
    dedupe_named_values(values.into_iter().map(|value| value.name))
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

fn tmdb_people(credits: Option<TmdbCredits>, image_base_url: &str) -> Vec<MetadataPerson> {
    let Some(credits) = credits else {
        return Vec::new();
    };

    let mut seen = std::collections::BTreeSet::new();
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
            tmdb_profile_image_url(image_base_url, cast.profile_path.as_deref()),
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
            tmdb_profile_image_url(image_base_url, crew.profile_path.as_deref()),
        );
    }

    people
}

fn push_tmdb_person(
    people: &mut Vec<MetadataPerson>,
    seen: &mut std::collections::BTreeSet<(String, String, String)>,
    name: Option<&str>,
    role_type: &str,
    role_name: Option<&str>,
    sort_order: i32,
    profile_image_url: Option<String>,
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
        profile_image_url,
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

// ---------------------------------------------------------------------------
// TMDB request/response structs.
// ---------------------------------------------------------------------------

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
    original_language: Option<String>,
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
    #[serde(default)]
    networks: Vec<TmdbNetwork>,
    credits: Option<TmdbCredits>,
    release_dates: Option<TmdbMovieReleaseDates>,
    content_ratings: Option<TmdbTvContentRatings>,
    external_ids: Option<TmdbExternalIds>,
    videos: Option<TmdbVideos>,
    belongs_to_collection: Option<TmdbBelongsToCollection>,
}

/// TMDB 单集详情响应（`/tv/{id}/season/{s}/episode/{e}`）。只取覆盖单集所需字段。
#[derive(Debug, Deserialize)]
struct TmdbEpisodeResponse {
    name: Option<String>,
    overview: Option<String>,
    air_date: Option<String>,
    vote_average: Option<f32>,
    still_path: Option<String>,
}

/// TMDB 季详情响应（`/tv/{id}/season/{s}`）。只取覆盖季所需字段。
#[derive(Debug, Deserialize)]
struct TmdbSeasonResponse {
    name: Option<String>,
    overview: Option<String>,
    air_date: Option<String>,
    poster_path: Option<String>,
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
struct TmdbNetwork {
    name: Option<String>,
}

/// TMDB movie `belongs_to_collection`：所属系列（如「变形金刚系列」）。
#[derive(Debug, Deserialize)]
struct TmdbBelongsToCollection {
    name: Option<String>,
}

#[derive(Debug, Deserialize)]
struct TmdbVideos {
    #[serde(default)]
    results: Vec<TmdbVideo>,
}

#[derive(Debug, Deserialize)]
struct TmdbVideo {
    name: Option<String>,
    site: Option<String>,
    key: Option<String>,
    /// TMDB video type: Trailer / Teaser / Clip / Featurette / Behind the Scenes / Opening Credits 等。
    #[serde(rename = "type")]
    video_type: Option<String>,
    #[serde(default)]
    official: bool,
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
    profile_path: Option<String>,
}

#[derive(Debug, Deserialize)]
struct TmdbCrewCredit {
    name: Option<String>,
    job: Option<String>,
    department: Option<String>,
    profile_path: Option<String>,
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

#[derive(Debug, Deserialize, Default)]
struct TmdbImagesResponse {
    #[serde(default)]
    posters: Vec<TmdbImage>,
    #[serde(default)]
    backdrops: Vec<TmdbImage>,
}

#[derive(Debug, Deserialize)]
struct TmdbImage {
    file_path: Option<String>,
    iso_639_1: Option<String>,
    vote_average: Option<f32>,
}

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
            "credits,release_dates,external_ids,videos"
        );
        assert_eq!(
            tmdb_detail_appends(TmdbSearchKind::Tv),
            "credits,content_ratings,external_ids,videos"
        );
    }

    #[test]
    fn tmdb_networks_map_to_named_values() {
        let networks = vec![
            TmdbNetwork {
                name: Some("Netflix".to_owned()),
            },
            TmdbNetwork {
                name: Some(" 爱奇艺 ".to_owned()),
            },
            TmdbNetwork { name: None },
            TmdbNetwork {
                name: Some("  ".to_owned()),
            },
        ];
        let out = tmdb_networks(networks);
        assert_eq!(out.len(), 2);
        assert_eq!(out[0].name, "Netflix");
        assert_eq!(out[0].name_normalized, "netflix");
        assert_eq!(out[1].name, "爱奇艺");
    }

    #[test]
    fn tmdb_videos_map_type_and_build_url() {
        let videos = Some(TmdbVideos {
            results: vec![
                TmdbVideo {
                    name: Some("Official Trailer".to_owned()),
                    site: Some("YouTube".to_owned()),
                    key: Some("abc123".to_owned()),
                    video_type: Some("Trailer".to_owned()),
                    official: true,
                },
                TmdbVideo {
                    name: Some("Opening".to_owned()),
                    site: Some("YouTube".to_owned()),
                    key: Some("opening1".to_owned()),
                    video_type: Some("Opening Credits".to_owned()),
                    official: false,
                },
                // 无 key → 丢弃。
                TmdbVideo {
                    name: Some("No Key".to_owned()),
                    site: Some("YouTube".to_owned()),
                    key: None,
                    video_type: Some("Clip".to_owned()),
                    official: false,
                },
            ],
        });
        let out = tmdb_videos(videos);
        assert_eq!(out.len(), 2);
        assert_eq!(out[0].video_type, "trailer");
        assert_eq!(
            out[0].url.as_deref(),
            Some("https://www.youtube.com/watch?v=abc123")
        );
        assert!(out[0].is_official);
        assert_eq!(out[1].video_type, "opening_theme");
        // None videos → 空。
        assert!(tmdb_videos(None).is_empty());
    }

    #[test]
    fn tmdb_season_url_builds_path() {
        assert_eq!(
            tmdb_season_url("https://tmdb.example.test/3/", 77, 2),
            "https://tmdb.example.test/3/tv/77/season/2"
        );
    }

    #[test]
    fn apply_tmdb_season_overlays_season_over_series() {
        let mut found = MetadataMatch {
            provider: "tmdb".to_owned(),
            external_id: "77".to_owned(),
            external_ids: Vec::new(),
            title: "Breaking Bad".to_owned(),
            series_title: None,
            original_title: None,
            overview: Some("series overview".to_owned()),
            production_year: Some(2008),
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
        let season = TmdbSeasonResponse {
            name: Some("Season 2".to_owned()),
            overview: Some("season 2 overview".to_owned()),
            air_date: Some("2009-03-08".to_owned()),
            poster_path: Some("/s2.jpg".to_owned()),
        };
        apply_tmdb_season(&mut found, season, "https://img");
        // 剧名存 series_title，季名覆盖 title。
        assert_eq!(found.series_title.as_deref(), Some("Breaking Bad"));
        assert_eq!(found.title, "Season 2");
        assert_eq!(found.overview.as_deref(), Some("season 2 overview"));
        assert_eq!(found.production_year, Some(2009));
        // 季海报作 primary。
        assert_eq!(found.artwork.len(), 1);
        assert_eq!(found.artwork[0].artwork_type, "poster");
        assert!(found.artwork[0].is_primary);
    }

    #[test]
    fn tmdb_episode_url_builds_season_episode_path() {
        assert_eq!(
            tmdb_episode_url("https://tmdb.example.test/3/", 77, 2, 8),
            "https://tmdb.example.test/3/tv/77/season/2/episode/8"
        );
        // specials（season 0）也成立。
        assert_eq!(
            tmdb_episode_url("https://tmdb.example.test/3", 100, 0, 1),
            "https://tmdb.example.test/3/tv/100/season/0/episode/1"
        );
    }

    #[test]
    fn apply_tmdb_episode_overlays_episode_fields_over_series() {
        // series 级 match（搜到剧后 detail 填的形态）。
        let mut found = MetadataMatch {
            provider: "tmdb".to_owned(),
            external_id: "77".to_owned(),
            external_ids: Vec::new(),
            title: "Breaking Bad".to_owned(),
            series_title: None,
            original_title: None,
            overview: Some("series overview".to_owned()),
            production_year: Some(2008),
            premiere_date: Some("2008-01-20".to_owned()),
            official_rating: None,
            community_rating: Some(9.5),
            artwork: vec![MetadataArtwork {
                artwork_type: "poster".to_owned(),
                source: None,
                remote_url: "https://img/series-poster.jpg".to_owned(),
                is_primary: true,
            }],
            genres: Vec::new(),
            studios: Vec::new(),
            networks: Vec::new(),
            videos: Vec::new(),
            collection: None,
            people: Vec::new(),
        };
        let episode = TmdbEpisodeResponse {
            name: Some("Pilot".to_owned()),
            overview: Some("episode overview".to_owned()),
            air_date: Some("2008-01-20".to_owned()),
            vote_average: Some(8.2),
            still_path: Some("/still.jpg".to_owned()),
        };
        apply_tmdb_episode(&mut found, episode, "https://img");

        // 集标题覆盖剧名、单集简介/评分覆盖。
        assert_eq!(found.title, "Pilot");
        assert_eq!(found.overview.as_deref(), Some("episode overview"));
        assert_eq!(found.community_rating, Some(8.2));
        assert_eq!(found.production_year, Some(2008));
        // 剧照作 primary 插在最前，原剧集海报降为非 primary。
        assert_eq!(found.artwork.len(), 2);
        assert_eq!(found.artwork[0].artwork_type, "primary");
        assert!(found.artwork[0].is_primary);
        assert_eq!(
            found.artwork[0].remote_url,
            "https://img/original/still.jpg"
        );
        assert!(!found.artwork[1].is_primary);
    }

    #[test]
    fn apply_tmdb_episode_preserves_series_fields_when_episode_empty() {
        let mut found = MetadataMatch {
            provider: "tmdb".to_owned(),
            external_id: "77".to_owned(),
            external_ids: Vec::new(),
            title: "Breaking Bad".to_owned(),
            series_title: None,
            original_title: None,
            overview: Some("series overview".to_owned()),
            production_year: Some(2008),
            premiere_date: Some("2008-01-20".to_owned()),
            official_rating: None,
            community_rating: Some(9.5),
            artwork: Vec::new(),
            genres: Vec::new(),
            studios: Vec::new(),
            networks: Vec::new(),
            videos: Vec::new(),
            collection: None,
            people: Vec::new(),
        };
        let empty = TmdbEpisodeResponse {
            name: None,
            overview: None,
            air_date: None,
            vote_average: None,
            still_path: None,
        };
        apply_tmdb_episode(&mut found, empty, "https://img");
        // 单集字段全空：保留 series 级值，不清空。
        assert_eq!(found.title, "Breaking Bad");
        assert_eq!(found.overview.as_deref(), Some("series overview"));
        assert_eq!(found.community_rating, Some(9.5));
        assert!(found.artwork.is_empty());
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
            external_ids: Vec::new(),
            title: "Search Title".to_owned(),
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

        apply_tmdb_detail(
            &mut mapped,
            TmdbDetailResponse {
                title: Some(" Detail Title ".to_owned()),
                name: None,
                original_title: Some("Original Detail".to_owned()),
                original_name: None,
                original_language: Some("en".to_owned()),
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
                videos: None,
                belongs_to_collection: None,
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
                networks: Vec::new(),
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

    fn image(path: &str, lang: Option<&str>, votes: f32) -> TmdbImage {
        TmdbImage {
            file_path: Some(path.to_owned()),
            iso_639_1: lang.map(str::to_owned),
            vote_average: Some(votes),
        }
    }

    #[test]
    fn prefer_original_picks_original_language_poster() {
        let policy = ImageLanguagePolicy {
            original_language: Some("ja".to_owned()),
            image_language: Some("zh".to_owned()),
            prefer_original: true,
            fallback_languages: vec!["none".to_owned()],
        };
        let posters = vec![
            image("/zh.jpg", Some("zh"), 9.0),
            image("/ja.jpg", Some("ja"), 1.0),
            image("/none.jpg", None, 10.0),
        ];
        // prefer_original wins over higher-voted zh / textless.
        let picked = pick_localized_tmdb_image(&posters, &policy).unwrap();
        assert_eq!(picked.file_path.as_deref(), Some("/ja.jpg"));
    }

    #[test]
    fn image_language_wins_when_not_preferring_original() {
        let policy = ImageLanguagePolicy {
            original_language: Some("ja".to_owned()),
            image_language: Some("zh".to_owned()),
            prefer_original: false,
            fallback_languages: vec!["en".to_owned()],
        };
        let posters = vec![
            image("/en.jpg", Some("en"), 10.0),
            image("/zh-a.jpg", Some("zh"), 4.0),
            image("/zh-b.jpg", Some("zh"), 8.0),
        ];
        // zh preferred; among zh, higher vote average wins.
        let picked = pick_localized_tmdb_image(&posters, &policy).unwrap();
        assert_eq!(picked.file_path.as_deref(), Some("/zh-b.jpg"));
    }

    #[test]
    fn textless_fallback_selected_when_no_language_match() {
        let policy = ImageLanguagePolicy {
            original_language: Some("ja".to_owned()),
            image_language: Some("zh".to_owned()),
            prefer_original: false,
            fallback_languages: vec!["none".to_owned()],
        };
        let posters = vec![
            image("/fr.jpg", Some("fr"), 9.0),
            image("/none.jpg", None, 1.0),
        ];
        let picked = pick_localized_tmdb_image(&posters, &policy).unwrap();
        assert_eq!(picked.file_path.as_deref(), Some("/none.jpg"));
    }

    #[test]
    fn include_image_language_lists_policy_languages_and_null() {
        let policy = ImageLanguagePolicy {
            original_language: Some("ja".to_owned()),
            image_language: Some("zh-CN".to_owned()),
            prefer_original: true,
            fallback_languages: vec!["en".to_owned(), "none".to_owned()],
        };
        let include = tmdb_include_image_language(&policy);
        assert_eq!(include, "ja,zh,en,null");
    }
}
