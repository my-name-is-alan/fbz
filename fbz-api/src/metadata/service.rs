use std::{
    collections::BTreeSet,
    error::Error,
    fmt::{Display, Formatter},
    path::PathBuf,
    sync::{Arc, RwLock},
    time::Duration,
};

use image::ImageReader;
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use sqlx::{Row, postgres::PgRow};
use tracing::warn;

use crate::{
    config::{MetadataConfig, ProxyConfig},
    db::DbPool,
    jobs::{ExpiredJobMessages, expire_stale_running_jobs, mark_job_failed},
    metadata::provider::shared::safe_remote_image_url,
    metadata::provider::{
        MetadataLookup, MetadataLookupReport, MetadataMatch, MetadataProviderAttempt,
        MetadataProviderError, MetadataProviderRegistry, ProviderProxyOverride,
    },
    metadata::settings::{
        MetadataGlobalSettings, MetadataSettingsError, MetadataSettingsRepository,
        ResolvedMetadataSettings, resolve_metadata_config,
    },
    metadata::write::{
        replace_item_genres, replace_item_networks, replace_item_people, replace_item_studios,
        replace_item_videos,
    },
    notifications::secrets::SecretCipher,
    plugins::hooks::{PluginHookDispatcher, PluginHookEvent},
};

const METADATA_WORKER_ID: &str = "fbz-api-metadata";
pub const METADATA_REFRESH_JOB_TYPE: &str = "metadata.refresh";
/// 查询响应缓存 TTL：同一逻辑条目在此时长内重复刷新跳过联网。
const METADATA_CACHE_TTL: Duration = Duration::from_secs(6 * 60 * 60);
const METADATA_REFRESH_JOB_LEASE_SECONDS: i64 = 10 * 60;
const METADATA_REFRESH_LEASE_EXPIRED_RETRY: &str = "metadata refresh lease expired; will retry";
const METADATA_REFRESH_LEASE_EXPIRED_FINAL: &str =
    "metadata refresh lease expired; max attempts reached";
const METADATA_REFRESH_COMPLETED_EVENT: &str = "metadata.refresh.completed";
const METADATA_REFRESH_FAILED_EVENT: &str = "metadata.refresh.failed";
const MAX_ARTWORK_DOWNLOAD_BYTES: usize = 16 * 1024 * 1024;
const ARTWORK_DOWNLOAD_TIMEOUT: Duration = Duration::from_secs(20);
const METADATA_CLAIM_JOB_SQL: &str = r#"
            with requested_job as (
                select case
                    when $1::text is null then null::uuid
                    when $1::text ~* '^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$'
                    then $1::uuid
                    else null::uuid
                end as public_id
            ),
            candidate as (
                select jobs.id
                from jobs
                cross join requested_job
                where ($1::text is null or jobs.public_id = requested_job.public_id)
                  and job_type = $2
                  and status in ('queued', 'failed')
                  and attempts < max_attempts
                  and run_at <= now()
                order by priority desc, run_at asc, jobs.id asc
                limit 1
                for update of jobs skip locked
            )
            update jobs j
            set status = 'running',
                locked_by = $3,
                locked_until = now() + ($4::bigint * interval '1 second'),
                attempts = attempts + 1,
                updated_at = now()
            from candidate
            where j.id = candidate.id
            returning
                j.id,
                j.public_id::text as public_id,
                j.payload
            "#;
const METADATA_LOAD_TARGET_SQL: &str = r#"
            select mi.id,
                   mi.public_id::text as public_id,
                   mi.item_type,
                   mi.title,
                   mi.original_title,
                   mi.production_year,
                   mi.season_number,
                   mi.episode_number,
                   l.preferred_metadata_language,
                   l.preferred_metadata_country,
                   l.preferred_image_language,
                   l.preferred_image_prefer_original,
                   l.preferred_image_fallback_languages,
                   (select external_id from media_external_ids
                      where media_item_id = mi.id and provider = 'tmdb') as tmdb_id,
                   (select external_id from media_external_ids
                      where media_item_id = mi.id and provider = 'imdb') as imdb_id,
                   (select external_id from media_external_ids
                      where media_item_id = mi.id and provider = 'tvdb') as tvdb_id
            from media_items mi
            join libraries l
              on l.id = mi.library_id
            where mi.public_id = case
                when $1::text ~* '^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$'
                then $1::uuid
                else null::uuid
            end
              and mi.is_deleted = false
            "#;

#[derive(Clone)]
pub struct MetadataService {
    pool: DbPool,
    provider: ProviderSource,
    worker_id: String,
    artwork_cache_dir: PathBuf,
    /// 查询响应缓存（TTL）：相同逻辑条目重复刷新时跳过联网。
    cache: Arc<crate::metadata::cache::MetadataCache>,
}

/// Where the service gets its provider registry from.
#[derive(Clone)]
enum ProviderSource {
    /// Production: env baseline + optional secret cipher, resolved against the
    /// DB on each job and cached until the effective config changes.
    Resolved {
        base_metadata: MetadataConfig,
        proxy: ProxyConfig,
        cipher: Option<SecretCipher>,
        cache: Arc<RwLock<Option<CachedRegistry>>>,
    },
    /// Test/override: a pre-built registry used verbatim.
    Fixed(MetadataProviderRegistry),
}

/// A registry cached alongside the effective config it was built from. Rebuilt
/// only when the resolved config or proxy overrides change, so the per-provider
/// state (e.g. the TVDB token cache) survives across jobs.
#[derive(Clone)]
struct CachedRegistry {
    config: MetadataConfig,
    overrides: std::collections::HashMap<String, ProviderProxyOverride>,
    /// 令牌池来源（resolved.keys）：纳入缓存比较，加/删 key 时触发 registry 重建。
    keys: std::collections::HashMap<String, Vec<String>>,
    registry: MetadataProviderRegistry,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MetadataRefreshSummary {
    pub job_id: String,
    pub item_id: String,
    pub status: String,
    pub matched: bool,
    pub provider: Option<String>,
    pub external_id: Option<String>,
    pub provider_attempts: Vec<MetadataProviderAttempt>,
}

#[derive(Clone, Debug)]
struct ClaimedMetadataJob {
    id: i64,
    public_id: String,
    payload: Value,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct MetadataTarget {
    id: i64,
    public_id: String,
    item_type: String,
    title: String,
    original_title: Option<String>,
    production_year: Option<i32>,
    season_number: Option<i32>,
    episode_number: Option<i32>,
    language: Option<String>,
    country: Option<String>,
    image_language: Option<String>,
    image_prefer_original: Option<bool>,
    image_fallback_languages: Option<Vec<String>>,
    /// 显式外部 provider id（来自 media_external_ids，扫描时由识别层 `{tmdb-XXX}` 写入）。
    /// 有值时 provider 直接按 id 拉详情、跳过模糊搜索。
    tmdb_id: Option<String>,
    imdb_id: Option<String>,
    tvdb_id: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct CachedArtwork {
    storage_key: String,
    width: Option<i32>,
    height: Option<i32>,
}

#[derive(Debug)]
pub enum MetadataError {
    JobNotFound,
    MissingItemId,
    ItemNotFound(String),
    Database(sqlx::Error),
    Provider(MetadataProviderError),
    Settings(MetadataSettingsError),
}

impl MetadataService {
    /// Production constructor: resolves provider config against the DB on each
    /// job (env baseline ← DB overrides), decrypting keys with `cipher` when
    /// available.
    pub fn new(
        pool: DbPool,
        metadata: MetadataConfig,
        proxy: ProxyConfig,
        cipher: Option<SecretCipher>,
        artwork_cache_dir: PathBuf,
    ) -> Result<Self, MetadataError> {
        Ok(Self {
            pool,
            provider: ProviderSource::Resolved {
                base_metadata: metadata,
                proxy,
                cipher,
                cache: Arc::new(RwLock::new(None)),
            },
            worker_id: METADATA_WORKER_ID.to_owned(),
            artwork_cache_dir,
            cache: Arc::new(crate::metadata::cache::MetadataCache::new(
                METADATA_CACHE_TTL,
            )),
        })
    }

    pub fn with_provider(
        pool: DbPool,
        provider: MetadataProviderRegistry,
        worker_id: String,
    ) -> Self {
        Self {
            pool,
            provider: ProviderSource::Fixed(provider),
            worker_id,
            artwork_cache_dir: std::env::temp_dir().join("fbz-api-test-artwork"),
            cache: Arc::new(crate::metadata::cache::MetadataCache::new(
                METADATA_CACHE_TTL,
            )),
        }
    }

    /// Returns the provider registry to use for a lookup plus the resolved
    /// global defaults (for folding into the lookup). For the resolved source
    /// this reads the DB-backed settings, folds them over the env baseline, and
    /// reuses the cached registry when the effective config is unchanged
    /// (preserving per-provider caches).
    async fn registry(
        &self,
    ) -> Result<(MetadataProviderRegistry, Option<MetadataGlobalSettings>), MetadataError> {
        match &self.provider {
            ProviderSource::Fixed(registry) => Ok((registry.clone(), None)),
            ProviderSource::Resolved {
                base_metadata,
                proxy,
                cipher,
                cache,
            } => {
                let resolved = MetadataSettingsRepository::new(self.pool.clone())
                    .resolve(cipher.as_ref())
                    .await
                    .map_err(MetadataError::Settings)?;
                let effective = resolve_metadata_config(base_metadata, &resolved);
                let overrides = proxy_overrides(&resolved);
                let global = resolved.global.clone();

                if let Ok(guard) = cache.read() {
                    if let Some(cached) = guard.as_ref() {
                        // 缓存比较纳入完整 key 集：管理员加 key 时即使首个 token 不变，
                        // 也要重建 registry 以刷新令牌池（否则多 key 不生效）。
                        if cached.config == effective
                            && cached.overrides == overrides
                            && cached.keys == resolved.keys
                        {
                            return Ok((cached.registry.clone(), global));
                        }
                    }
                }

                let registry = MetadataProviderRegistry::from_config_with_overrides(
                    effective.clone(),
                    proxy.clone(),
                    &overrides,
                )
                .map_err(MetadataError::Provider)?
                .with_token_pools(&resolved.keys);
                if let Ok(mut guard) = cache.write() {
                    *guard = Some(CachedRegistry {
                        config: effective,
                        overrides,
                        keys: resolved.keys.clone(),
                        registry: registry.clone(),
                    });
                }
                Ok((registry, global))
            }
        }
    }

    pub async fn run_metadata_job(
        &self,
        job_id: &str,
    ) -> Result<MetadataRefreshSummary, MetadataError> {
        let Some(job) = self.claim_metadata_job(Some(job_id)).await? else {
            return Err(MetadataError::JobNotFound);
        };

        self.run_claimed_metadata_job(job).await
    }

    pub async fn run_next_refresh_job(
        &self,
    ) -> Result<Option<MetadataRefreshSummary>, MetadataError> {
        let Some(job) = self.claim_metadata_job(None).await? else {
            return Ok(None);
        };

        self.run_claimed_metadata_job(job).await.map(Some)
    }

    async fn run_claimed_metadata_job(
        &self,
        job: ClaimedMetadataJob,
    ) -> Result<MetadataRefreshSummary, MetadataError> {
        let item_id = job
            .payload
            .get("itemId")
            .and_then(Value::as_str)
            .ok_or(MetadataError::MissingItemId)?
            .to_owned();

        let run_id = self.start_job_run(job.id).await?;
        self.record_job_event(
            job.id,
            Some(run_id),
            "metadata.refresh.started",
            "info",
            "metadata refresh started",
            json!({ "itemId": item_id }),
        )
        .await?;

        let result = self.refresh_item(&item_id).await;
        match result {
            Ok(summary) => {
                let completed = MetadataRefreshSummary {
                    job_id: job.public_id.clone(),
                    ..summary
                };
                self.finish_job_success(job.id, run_id, &completed).await?;
                self.dispatch_metadata_hook(metadata_refresh_completed_event(&completed))
                    .await;
                Ok(completed)
            }
            Err(err) => {
                let message = err.to_string();
                if let Err(event_err) = self
                    .record_job_event(
                        job.id,
                        Some(run_id),
                        "metadata.refresh.failed",
                        "error",
                        &message,
                        json!({ "itemId": item_id }),
                    )
                    .await
                {
                    warn!(error = %event_err, "failed to record metadata refresh failure event");
                }
                self.finish_job_failure(&job.public_id, job.id, run_id, &message)
                    .await?;
                self.dispatch_metadata_hook(metadata_refresh_failed_event(
                    &job.public_id,
                    &item_id,
                    &message,
                ))
                .await;
                Err(err)
            }
        }
    }

    async fn claim_metadata_job(
        &self,
        job_id: Option<&str>,
    ) -> Result<Option<ClaimedMetadataJob>, MetadataError> {
        let mut tx = self.pool.begin().await.map_err(MetadataError::Database)?;
        expire_stale_running_jobs(
            &mut tx,
            METADATA_REFRESH_JOB_TYPE,
            ExpiredJobMessages {
                retry: METADATA_REFRESH_LEASE_EXPIRED_RETRY,
                final_failure: METADATA_REFRESH_LEASE_EXPIRED_FINAL,
            },
        )
        .await
        .map_err(MetadataError::Database)?;

        let job = sqlx::query(METADATA_CLAIM_JOB_SQL)
            .bind(job_id)
            .bind(METADATA_REFRESH_JOB_TYPE)
            .bind(&self.worker_id)
            .bind(METADATA_REFRESH_JOB_LEASE_SECONDS)
            .fetch_optional(&mut *tx)
            .await
            .map_err(MetadataError::Database)?
            .map(ClaimedMetadataJob::from_row)
            .transpose()
            .map_err(MetadataError::Database)?;

        tx.commit().await.map_err(MetadataError::Database)?;
        Ok(job)
    }

    async fn refresh_item(&self, item_id: &str) -> Result<MetadataRefreshSummary, MetadataError> {
        let target = self.load_target(item_id).await?;
        let (registry, global) = self.registry().await?;
        let lookup = build_lookup(&target, global.as_ref());

        // 响应缓存：相同逻辑条目（标题/年份/季集/语言）在 TTL 内复用上次结果，跳过联网。
        let cache_key = crate::metadata::cache::cache_key(&lookup);
        let report = if let Some(cached) = self.cache.get(&cache_key) {
            // 命中：用缓存结果（正/负都缓存），不联网；attempts 标记为缓存命中。
            MetadataLookupReport {
                matched: cached.matched,
                attempts: vec![MetadataProviderAttempt::skipped(
                    "cache",
                    "served from metadata response cache",
                )],
            }
        } else {
            let fresh = registry
                .match_item_with_report(&lookup)
                .await
                .map_err(MetadataError::Provider)?;
            // 写缓存（正/负结果都存）。
            self.cache.put(cache_key, fresh.matched.clone());
            fresh
        };

        match report.matched {
            Some(found) => {
                let provider = found.provider.clone();
                let external_id = found.external_id.clone();
                self.apply_match(target.id, &found).await?;
                Ok(MetadataRefreshSummary {
                    job_id: String::new(),
                    item_id: target.public_id,
                    status: "matched".to_owned(),
                    matched: true,
                    provider: Some(provider),
                    external_id: Some(external_id),
                    provider_attempts: report.attempts,
                })
            }
            None => {
                self.mark_item_failed(target.id).await?;
                Ok(MetadataRefreshSummary {
                    job_id: String::new(),
                    item_id: target.public_id,
                    status: "no_match".to_owned(),
                    matched: false,
                    provider: None,
                    external_id: None,
                    provider_attempts: report.attempts,
                })
            }
        }
    }

    async fn load_target(&self, item_id: &str) -> Result<MetadataTarget, MetadataError> {
        let Some(row) = sqlx::query(METADATA_LOAD_TARGET_SQL)
            .bind(item_id.trim())
            .fetch_optional(&self.pool)
            .await
            .map_err(MetadataError::Database)?
        else {
            return Err(MetadataError::ItemNotFound(item_id.to_owned()));
        };

        MetadataTarget::from_row(row).map_err(MetadataError::Database)
    }

    async fn apply_match(
        &self,
        media_item_id: i64,
        found: &MetadataMatch,
    ) -> Result<(), MetadataError> {
        let provider_fingerprint = format!("{}:{}", found.provider, found.external_id);
        let title_pinyin = crate::text::pinyin::pinyin_keys(found.title.trim());
        let cached_artwork = self.cache_match_artwork(media_item_id, found).await;

        let mut tx = self.pool.begin().await.map_err(MetadataError::Database)?;
        sqlx::query(
            r#"
            update media_items
            set title = $2,
                original_title = $3,
                overview = $4,
                production_year = coalesce($5, production_year),
                premiere_date = $6::date,
                community_rating = $7,
                official_rating = coalesce($8, official_rating),
                provider_fingerprint = $9,
                pinyin_full = $10,
                pinyin_initials = $11,
                metadata_status = 'matched',
                updated_at = now()
            where id = $1
            "#,
        )
        .bind(media_item_id)
        .bind(found.title.trim())
        .bind(found.original_title.as_deref())
        .bind(found.overview.as_deref())
        .bind(found.production_year)
        .bind(found.premiere_date.as_deref())
        .bind(found.community_rating)
        .bind(found.official_rating.as_deref())
        .bind(&provider_fingerprint)
        .bind(title_pinyin.as_ref().map(|keys| keys.full.as_str()))
        .bind(title_pinyin.as_ref().map(|keys| keys.initials.as_str()))
        .execute(&mut *tx)
        .await
        .map_err(MetadataError::Database)?;

        for external_id in metadata_external_ids_for_match(found) {
            sqlx::query(
                r#"
                insert into media_external_ids (
                    media_item_id,
                    provider,
                    external_id
                )
                values ($1, $2, $3)
                on conflict (media_item_id, provider) do update
                    set external_id = excluded.external_id
                "#,
            )
            .bind(media_item_id)
            .bind(&external_id.0)
            .bind(&external_id.1)
            .execute(&mut *tx)
            .await
            .map_err(MetadataError::Database)?;
        }

        if !found.artwork.is_empty() {
            for source in metadata_artwork_sources_for_match(found) {
                sqlx::query(
                    r#"
                    delete from artwork
                    where media_item_id = $1
                      and source = $2
                    "#,
                )
                .bind(media_item_id)
                .bind(source)
                .execute(&mut *tx)
                .await
                .map_err(MetadataError::Database)?;
            }

            for (image, cached) in found.artwork.iter().zip(cached_artwork.iter()) {
                let source = metadata_artwork_source(found, image.source.as_deref());
                sqlx::query(
                    r#"
                    insert into artwork (
                        media_item_id,
                        artwork_type,
                        source,
                        storage_key,
                        remote_url,
                        width,
                        height,
                        is_primary
                    )
                    values ($1, $2, $3, $4, $5, $6, $7, $8)
                    "#,
                )
                .bind(media_item_id)
                .bind(image.artwork_type.trim())
                .bind(source)
                .bind(cached.as_ref().map(|image| image.storage_key.as_str()))
                .bind(image.remote_url.trim())
                .bind(cached.as_ref().and_then(|image| image.width))
                .bind(cached.as_ref().and_then(|image| image.height))
                .bind(image.is_primary)
                .execute(&mut *tx)
                .await
                .map_err(MetadataError::Database)?;
            }
        }

        if !found.genres.is_empty() {
            replace_item_genres(&mut tx, media_item_id, &found.genres)
                .await
                .map_err(MetadataError::Database)?;
        }

        if !found.studios.is_empty() {
            replace_item_studios(&mut tx, media_item_id, &found.studios)
                .await
                .map_err(MetadataError::Database)?;
        }

        if !found.people.is_empty() {
            replace_item_people(&mut tx, media_item_id, &found.people)
                .await
                .map_err(MetadataError::Database)?;
        }

        if !found.networks.is_empty() {
            replace_item_networks(&mut tx, media_item_id, &found.networks)
                .await
                .map_err(MetadataError::Database)?;
        }

        if !found.videos.is_empty() {
            replace_item_videos(&mut tx, media_item_id, &found.videos)
                .await
                .map_err(MetadataError::Database)?;
        }

        // 电影所属系列（TMDB belongs_to_collection）：find-or-create collection（库内按
        // name_normalized 去重），再关联 collection_items。collection 与 media_item 同库。
        if let Some(collection) = found.collection.as_ref() {
            let collection_pinyin = crate::text::pinyin::pinyin_keys(collection.name.trim());
            let collection_id = sqlx::query_scalar::<_, i64>(
                r#"
                insert into collections (library_id, name, name_normalized, overview, pinyin_full, pinyin_initials)
                select mi.library_id, $2, $3, $4, $5, $6
                from media_items mi where mi.id = $1
                on conflict (library_id, name_normalized) do update
                    set name = excluded.name,
                        overview = coalesce(excluded.overview, collections.overview),
                        pinyin_full = coalesce(excluded.pinyin_full, collections.pinyin_full),
                        pinyin_initials = coalesce(excluded.pinyin_initials, collections.pinyin_initials),
                        updated_at = now()
                returning id
                "#,
            )
            .bind(media_item_id)
            .bind(collection.name.trim())
            .bind(collection.name_normalized.trim())
            .bind(collection.overview.as_deref())
            .bind(collection_pinyin.as_ref().map(|keys| keys.full.as_str()))
            .bind(collection_pinyin.as_ref().map(|keys| keys.initials.as_str()))
            .fetch_one(&mut *tx)
            .await
            .map_err(MetadataError::Database)?;

            sqlx::query(
                r#"
                insert into collection_items (collection_id, media_item_id)
                values ($1, $2)
                on conflict do nothing
                "#,
            )
            .bind(collection_id)
            .bind(media_item_id)
            .execute(&mut *tx)
            .await
            .map_err(MetadataError::Database)?;
        }

        // series_title 回写（Emby 模型）：episode 的 `title` 是单集名，剧名要落到它所属的
        // series 容器。episode 的 parent 可能是 season（再往上是 series）或直接是 series。
        // 沿 parent_id 向上找最近的 series 容器，把仍是占位（is_virtual）的容器名更新为真实剧名。
        if let Some(series_title) = found.series_title.as_deref()
            && !series_title.trim().is_empty()
        {
            let series_pinyin = crate::text::pinyin::pinyin_keys(series_title.trim());
            sqlx::query(
                r#"
                with recursive ancestry as (
                    select id, parent_id, item_type, 0 as depth
                    from media_items where id = $1
                    union all
                    select mi.id, mi.parent_id, mi.item_type, a.depth + 1
                    from media_items mi
                    join ancestry a on mi.id = a.parent_id
                    where a.depth < 4
                )
                update media_items
                set title = $2,
                    sort_title = $2,
                    pinyin_full = $3,
                    pinyin_initials = $4,
                    metadata_status = 'matched',
                    updated_at = now()
                where id = (
                    select id from ancestry where item_type = 'series' order by depth desc limit 1
                )
                "#,
            )
            .bind(media_item_id)
            .bind(series_title.trim())
            .bind(series_pinyin.as_ref().map(|keys| keys.full.as_str()))
            .bind(series_pinyin.as_ref().map(|keys| keys.initials.as_str()))
            .execute(&mut *tx)
            .await
            .map_err(MetadataError::Database)?;
        }

        tx.commit().await.map_err(MetadataError::Database)
    }

    async fn mark_item_failed(&self, media_item_id: i64) -> Result<(), MetadataError> {
        sqlx::query(
            r#"
            update media_items
            set metadata_status = 'failed',
                updated_at = now()
            where id = $1
            "#,
        )
        .bind(media_item_id)
        .execute(&self.pool)
        .await
        .map_err(MetadataError::Database)?;

        Ok(())
    }

    async fn cache_match_artwork(
        &self,
        media_item_id: i64,
        found: &MetadataMatch,
    ) -> Vec<Option<CachedArtwork>> {
        let mut cached = Vec::with_capacity(found.artwork.len());
        for image in &found.artwork {
            let source = metadata_artwork_source(found, image.source.as_deref());
            cached.push(
                self.cache_artwork(
                    media_item_id,
                    &source,
                    image.artwork_type.trim(),
                    image.remote_url.trim(),
                )
                .await,
            );
        }
        cached
    }

    async fn cache_artwork(
        &self,
        media_item_id: i64,
        source: &str,
        artwork_type: &str,
        remote_url: &str,
    ) -> Option<CachedArtwork> {
        let remote_url = match safe_remote_image_url(remote_url) {
            Some(url) => url,
            None => {
                warn!(media_item_id, remote_url, "metadata artwork URL rejected");
                return None;
            }
        };
        let bytes = match download_artwork_bytes(&remote_url).await {
            Ok(bytes) => bytes,
            Err(err) => {
                warn!(
                    media_item_id,
                    source,
                    artwork_type,
                    remote_url,
                    error = %err,
                    "metadata artwork download failed"
                );
                return None;
            }
        };
        let decoded = match decode_artwork(&bytes) {
            Ok(decoded) => decoded,
            Err(err) => {
                warn!(
                    media_item_id,
                    source,
                    artwork_type,
                    remote_url,
                    error = %err,
                    "metadata artwork decode failed"
                );
                return None;
            }
        };
        let storage_key = artwork_storage_key(
            media_item_id,
            source,
            artwork_type,
            &remote_url,
            decoded.extension,
        );
        let output_path = self.artwork_cache_dir.join(&storage_key);
        if let Some(parent) = output_path.parent()
            && let Err(err) = tokio::fs::create_dir_all(parent).await
        {
            warn!(
                media_item_id,
                source,
                artwork_type,
                path = %parent.display(),
                error = %err,
                "metadata artwork cache directory creation failed"
            );
            return None;
        }
        if let Err(err) = tokio::fs::write(&output_path, &bytes).await {
            warn!(
                media_item_id,
                source,
                artwork_type,
                path = %output_path.display(),
                error = %err,
                "metadata artwork cache write failed"
            );
            return None;
        }

        Some(CachedArtwork {
            storage_key,
            width: decoded.width,
            height: decoded.height,
        })
    }

    async fn start_job_run(&self, job_id: i64) -> Result<i64, MetadataError> {
        sqlx::query_scalar::<_, i64>(
            r#"
            insert into job_runs (job_id, worker_id, status)
            values ($1, $2, 'running')
            returning id
            "#,
        )
        .bind(job_id)
        .bind(&self.worker_id)
        .fetch_one(&self.pool)
        .await
        .map_err(MetadataError::Database)
    }

    async fn finish_job_success(
        &self,
        job_id: i64,
        run_id: i64,
        summary: &MetadataRefreshSummary,
    ) -> Result<(), MetadataError> {
        let metrics = json!({
            "itemId": summary.item_id,
            "matched": summary.matched,
            "provider": summary.provider,
            "externalId": summary.external_id,
            "providerAttempts": summary.provider_attempts,
        });

        let mut tx = self.pool.begin().await.map_err(MetadataError::Database)?;
        sqlx::query(
            r#"
            update job_runs
            set status = 'succeeded',
                finished_at = now(),
                metrics = $2
            where id = $1
            "#,
        )
        .bind(run_id)
        .bind(&metrics)
        .execute(&mut *tx)
        .await
        .map_err(MetadataError::Database)?;

        sqlx::query(
            r#"
            update jobs
            set status = 'succeeded',
                locked_by = null,
                locked_until = null,
                updated_at = now(),
                finished_at = now()
            where id = $1
            "#,
        )
        .bind(job_id)
        .execute(&mut *tx)
        .await
        .map_err(MetadataError::Database)?;

        tx.commit().await.map_err(MetadataError::Database)
    }

    async fn finish_job_failure(
        &self,
        job_public_id: &str,
        job_id: i64,
        run_id: i64,
        message: &str,
    ) -> Result<(), MetadataError> {
        let mut tx = self.pool.begin().await.map_err(MetadataError::Database)?;
        sqlx::query(
            r#"
            update job_runs
            set status = 'failed',
                finished_at = now(),
                error_message = $2
            where id = $1
            "#,
        )
        .bind(run_id)
        .bind(message)
        .execute(&mut *tx)
        .await
        .map_err(MetadataError::Database)?;

        mark_job_failed(
            &mut tx,
            METADATA_REFRESH_JOB_TYPE,
            job_public_id,
            job_id,
            message,
        )
        .await
        .map_err(MetadataError::Database)?;

        tx.commit().await.map_err(MetadataError::Database)
    }

    async fn record_job_event(
        &self,
        job_id: i64,
        run_id: Option<i64>,
        event_type: &str,
        event_level: &str,
        message: &str,
        payload: Value,
    ) -> Result<(), MetadataError> {
        sqlx::query(
            r#"
            insert into job_events (
                job_id,
                job_run_id,
                event_type,
                event_level,
                message,
                payload
            )
            values ($1, $2, $3, $4, $5, $6)
            "#,
        )
        .bind(job_id)
        .bind(run_id)
        .bind(event_type)
        .bind(event_level)
        .bind(message)
        .bind(payload)
        .execute(&self.pool)
        .await
        .map_err(MetadataError::Database)?;

        Ok(())
    }

    async fn dispatch_metadata_hook(&self, event: PluginHookEvent) {
        let event_key = event.event_key.clone();
        let item_id = event.aggregate_id.clone();
        if let Err(err) = PluginHookDispatcher::new(self.pool.clone())
            .dispatch(event)
            .await
        {
            warn!(
                error = %err,
                event_key = %event_key,
                item_id = %item_id,
                "failed to dispatch plugin metadata hooks"
            );
        }
    }
}

/// Builds the provider lookup, layering library-level fields over the global
/// defaults (§6 text language/region, §7 image-language policy). Library values
/// win; absent ones fall back to the global row; absent there, the provider's
/// own built-in defaults apply downstream.
fn build_lookup(
    target: &MetadataTarget,
    global: Option<&MetadataGlobalSettings>,
) -> MetadataLookup {
    let language = target
        .language
        .clone()
        .or_else(|| global.and_then(|g| g.default_language.clone()));
    let country = target
        .country
        .clone()
        .or_else(|| global.and_then(|g| g.default_country.clone()));
    // Image-language policy: library override wins, else the global default,
    // else the provider built-in (handled downstream).
    let image_language = target
        .image_language
        .clone()
        .or_else(|| global.and_then(|g| g.image_language.clone()));
    let image_prefer_original = target
        .image_prefer_original
        .or_else(|| global.map(|g| g.image_prefer_original))
        .unwrap_or(false);
    let image_fallback_languages = target
        .image_fallback_languages
        .clone()
        .filter(|langs| !langs.is_empty())
        .or_else(|| global.map(|g| g.image_fallback_languages.clone()))
        .unwrap_or_default();

    MetadataLookup {
        item_type: target.item_type.clone(),
        title: target.title.clone(),
        original_title: target.original_title.clone(),
        production_year: target.production_year,
        season: target.season_number,
        episode: target.episode_number,
        tmdb_id: target.tmdb_id.clone(),
        imdb_id: target.imdb_id.clone(),
        tvdb_id: target.tvdb_id.clone(),
        language,
        country,
        image_language,
        image_prefer_original,
        image_fallback_languages,
    }
}

/// Builds the per-provider proxy override map from resolved DB settings.
fn proxy_overrides(
    resolved: &ResolvedMetadataSettings,
) -> std::collections::HashMap<String, ProviderProxyOverride> {
    resolved
        .providers
        .iter()
        .map(|(id, settings)| {
            (
                id.clone(),
                ProviderProxyOverride {
                    mode: settings.proxy_mode.clone(),
                    url: settings.proxy_url.clone(),
                },
            )
        })
        .collect()
}

fn metadata_refresh_completed_event(summary: &MetadataRefreshSummary) -> PluginHookEvent {
    PluginHookEvent {
        event_key: METADATA_REFRESH_COMPLETED_EVENT.to_owned(),
        aggregate_type: "media_item".to_owned(),
        aggregate_id: summary.item_id.clone(),
        payload: json!({
            "jobId": &summary.job_id,
            "itemId": &summary.item_id,
            "status": &summary.status,
            "matched": summary.matched,
            "provider": summary.provider.as_deref(),
            "externalId": summary.external_id.as_deref(),
            "providerAttempts": &summary.provider_attempts,
        }),
    }
}

fn metadata_refresh_failed_event(job_id: &str, item_id: &str, message: &str) -> PluginHookEvent {
    PluginHookEvent {
        event_key: METADATA_REFRESH_FAILED_EVENT.to_owned(),
        aggregate_type: "media_item".to_owned(),
        aggregate_id: item_id.to_owned(),
        payload: json!({
            "jobId": job_id,
            "itemId": item_id,
            "status": "failed",
            "matched": false,
            "error": message,
        }),
    }
}

impl ClaimedMetadataJob {
    fn from_row(row: PgRow) -> Result<Self, sqlx::Error> {
        Ok(Self {
            id: row.try_get("id")?,
            public_id: row.try_get("public_id")?,
            payload: row.try_get("payload")?,
        })
    }
}

impl MetadataTarget {
    fn from_row(row: PgRow) -> Result<Self, sqlx::Error> {
        Ok(Self {
            id: row.try_get("id")?,
            public_id: row.try_get("public_id")?,
            item_type: row.try_get("item_type")?,
            title: row.try_get("title")?,
            original_title: row.try_get("original_title")?,
            production_year: row.try_get("production_year")?,
            season_number: row.try_get("season_number")?,
            episode_number: row.try_get("episode_number")?,
            language: row.try_get("preferred_metadata_language")?,
            country: row.try_get("preferred_metadata_country")?,
            image_language: row.try_get("preferred_image_language")?,
            image_prefer_original: row.try_get("preferred_image_prefer_original")?,
            image_fallback_languages: row.try_get("preferred_image_fallback_languages")?,
            tmdb_id: row.try_get("tmdb_id")?,
            imdb_id: row.try_get("imdb_id")?,
            tvdb_id: row.try_get("tvdb_id")?,
        })
    }
}

fn metadata_external_ids_for_match(found: &MetadataMatch) -> Vec<(String, String)> {
    let mut seen_providers = BTreeSet::new();
    let mut external_ids = Vec::new();

    push_metadata_external_id(
        &mut external_ids,
        &mut seen_providers,
        found.provider.as_str(),
        found.external_id.as_str(),
    );
    for external_id in &found.external_ids {
        push_metadata_external_id(
            &mut external_ids,
            &mut seen_providers,
            external_id.provider.as_str(),
            external_id.external_id.as_str(),
        );
    }

    external_ids
}

fn push_metadata_external_id(
    external_ids: &mut Vec<(String, String)>,
    seen_providers: &mut BTreeSet<String>,
    provider: &str,
    external_id: &str,
) {
    let provider = provider.trim().to_ascii_lowercase();
    let external_id = external_id.trim();
    if provider.is_empty() || external_id.is_empty() || !seen_providers.insert(provider.clone()) {
        return;
    }

    external_ids.push((provider, external_id.to_owned()));
}

fn metadata_artwork_sources_for_match(found: &MetadataMatch) -> Vec<String> {
    let mut sources = BTreeSet::new();
    for image in &found.artwork {
        sources.insert(metadata_artwork_source(found, image.source.as_deref()));
    }

    sources.into_iter().collect()
}

fn metadata_artwork_source(found: &MetadataMatch, source: Option<&str>) -> String {
    source
        .and_then(|source| {
            let source = source.trim().to_ascii_lowercase();
            (!source.is_empty()).then_some(source)
        })
        .unwrap_or_else(|| found.provider.trim().to_ascii_lowercase())
}

async fn download_artwork_bytes(remote_url: &str) -> Result<Vec<u8>, String> {
    let response = reqwest::Client::builder()
        .timeout(ARTWORK_DOWNLOAD_TIMEOUT)
        .build()
        .map_err(|err| format!("failed to build HTTP client: {err}"))?
        .get(remote_url)
        .send()
        .await
        .map_err(|err| format!("request failed: {err}"))?;
    let status = response.status();
    if !status.is_success() {
        return Err(format!("remote returned status {status}"));
    }
    let bytes = response
        .bytes()
        .await
        .map_err(|err| format!("read body failed: {err}"))?;
    if bytes.len() > MAX_ARTWORK_DOWNLOAD_BYTES {
        return Err(format!(
            "image is too large: {} bytes (max {})",
            bytes.len(),
            MAX_ARTWORK_DOWNLOAD_BYTES
        ));
    }
    Ok(bytes.to_vec())
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct DecodedArtwork {
    extension: &'static str,
    width: Option<i32>,
    height: Option<i32>,
}

fn decode_artwork(bytes: &[u8]) -> Result<DecodedArtwork, String> {
    if bytes.is_empty() {
        return Err("image is empty".to_owned());
    }
    if bytes.len() > MAX_ARTWORK_DOWNLOAD_BYTES {
        return Err(format!(
            "image is too large: {} bytes (max {})",
            bytes.len(),
            MAX_ARTWORK_DOWNLOAD_BYTES
        ));
    }
    let reader = ImageReader::new(std::io::Cursor::new(bytes))
        .with_guessed_format()
        .map_err(|err| format!("image format detection failed: {err}"))?;
    let format = reader
        .format()
        .ok_or_else(|| "image format is unknown".to_owned())?;
    let extension = match format {
        image::ImageFormat::Jpeg => "jpg",
        image::ImageFormat::Png => "png",
        image::ImageFormat::WebP => "webp",
        image::ImageFormat::Gif => "gif",
        _ => return Err("image format is not supported for artwork cache".to_owned()),
    };
    let (width, height) = reader
        .into_dimensions()
        .map_err(|err| format!("image dimensions failed: {err}"))?;
    Ok(DecodedArtwork {
        extension,
        width: i32::try_from(width).ok().filter(|value| *value > 0),
        height: i32::try_from(height).ok().filter(|value| *value > 0),
    })
}

fn artwork_storage_key(
    media_item_id: i64,
    source: &str,
    artwork_type: &str,
    remote_url: &str,
    extension: &str,
) -> String {
    let source = safe_artwork_path_segment(source, "metadata");
    let artwork_type = safe_artwork_path_segment(artwork_type, "image");
    let extension = safe_artwork_extension(extension);
    let mut hasher = Sha256::new();
    hasher.update(remote_url.as_bytes());
    let digest = hasher.finalize();
    let hash = digest
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect::<String>();
    format!("metadata/{media_item_id}/{source}/{artwork_type}-{hash}.{extension}",)
}

fn safe_artwork_path_segment(value: &str, fallback: &str) -> String {
    let normalized = value
        .trim()
        .chars()
        .filter_map(|ch| {
            let ch = ch.to_ascii_lowercase();
            if ch.is_ascii_alphanumeric() || ch == '-' || ch == '_' {
                Some(ch)
            } else {
                None
            }
        })
        .collect::<String>();
    if normalized.is_empty() {
        fallback.to_owned()
    } else {
        normalized
    }
}

fn safe_artwork_extension(value: &str) -> &'static str {
    match value.trim().to_ascii_lowercase().as_str() {
        "jpg" | "jpeg" => "jpg",
        "png" => "png",
        "webp" => "webp",
        "gif" => "gif",
        _ => "jpg",
    }
}

impl Display for MetadataError {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::JobNotFound => f.write_str("metadata refresh job not found or not runnable"),
            Self::MissingItemId => f.write_str("metadata refresh job payload is missing itemId"),
            Self::ItemNotFound(item_id) => write!(f, "media item `{item_id}` not found"),
            Self::Database(err) => write!(f, "database error: {err}"),
            Self::Provider(err) => write!(f, "{err}"),
            Self::Settings(err) => write!(f, "{err}"),
        }
    }
}

impl Error for MetadataError {}

#[cfg(test)]
mod tests {
    use crate::metadata::provider::{
        MetadataArtwork, MetadataExternalId, MetadataProviderAttemptStatus,
        MetadataProviderRegistry,
    };

    use super::*;

    #[test]
    fn metadata_error_messages_are_client_safe() {
        assert_eq!(
            MetadataError::MissingItemId.to_string(),
            "metadata refresh job payload is missing itemId"
        );
        assert!(
            MetadataError::ItemNotFound("item-1".to_owned())
                .to_string()
                .contains("item-1")
        );
    }

    #[test]
    fn metadata_completed_hook_payload_preserves_provider_attempts() {
        let summary = MetadataRefreshSummary {
            job_id: "job-1".to_owned(),
            item_id: "item-1".to_owned(),
            status: "matched".to_owned(),
            matched: true,
            provider: Some("tmdb".to_owned()),
            external_id: Some("123".to_owned()),
            provider_attempts: vec![MetadataProviderAttempt {
                provider: "tmdb".to_owned(),
                status: MetadataProviderAttemptStatus::Matched,
                message: None,
                external_id: Some("123".to_owned()),
            }],
        };

        let event = metadata_refresh_completed_event(&summary);

        assert_eq!(event.event_key, METADATA_REFRESH_COMPLETED_EVENT);
        assert_eq!(event.aggregate_type, "media_item");
        assert_eq!(event.aggregate_id, "item-1");
        assert_eq!(event.payload["jobId"], "job-1");
        assert_eq!(event.payload["itemId"], "item-1");
        assert_eq!(event.payload["status"], "matched");
        assert_eq!(event.payload["matched"], true);
        assert_eq!(event.payload["provider"], "tmdb");
        assert_eq!(event.payload["externalId"], "123");
        assert_eq!(event.payload["providerAttempts"][0]["provider"], "tmdb");
        assert_eq!(event.payload["providerAttempts"][0]["status"], "matched");
        assert!(event.payload.get("mediaItemId").is_none());
    }

    #[test]
    fn metadata_external_ids_preserve_primary_and_first_additional_provider() {
        let found = MetadataMatch {
            provider: " TMDB ".to_owned(),
            external_id: " 42 ".to_owned(),
            external_ids: vec![
                MetadataExternalId {
                    provider: "imdb".to_owned(),
                    external_id: " tt1234567 ".to_owned(),
                },
                MetadataExternalId {
                    provider: "IMDB".to_owned(),
                    external_id: "tt7654321".to_owned(),
                },
                MetadataExternalId {
                    provider: "tvdb".to_owned(),
                    external_id: "121361".to_owned(),
                },
            ],
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
            metadata_external_ids_for_match(&found),
            vec![
                ("tmdb".to_owned(), "42".to_owned()),
                ("imdb".to_owned(), "tt1234567".to_owned()),
                ("tvdb".to_owned(), "121361".to_owned()),
            ]
        );
    }

    #[test]
    fn metadata_artwork_sources_fallback_to_provider_and_keep_explicit_sources() {
        let found = MetadataMatch {
            provider: " TMDB ".to_owned(),
            external_id: "42".to_owned(),
            external_ids: Vec::new(),
            title: "Title".to_owned(),
            series_title: None,
            original_title: None,
            overview: None,
            production_year: None,
            premiere_date: None,
            official_rating: None,
            community_rating: None,
            artwork: vec![
                MetadataArtwork {
                    artwork_type: "poster".to_owned(),
                    source: None,
                    remote_url: "https://image.example/poster.jpg".to_owned(),
                    is_primary: true,
                },
                MetadataArtwork {
                    artwork_type: "backdrop".to_owned(),
                    source: Some(" Fanart ".to_owned()),
                    remote_url: "https://image.example/backdrop.jpg".to_owned(),
                    is_primary: true,
                },
            ],
            genres: Vec::new(),
            studios: Vec::new(),
            networks: Vec::new(),
            videos: Vec::new(),
            collection: None,
            people: Vec::new(),
        };

        assert_eq!(
            metadata_artwork_sources_for_match(&found),
            vec!["fanart".to_owned(), "tmdb".to_owned()]
        );
    }

    #[test]
    fn artwork_storage_key_is_stable_and_safe() {
        let key = artwork_storage_key(
            42,
            " TMDB Provider ",
            "../Poster Image",
            "https://image.example.test/path/poster.jpg?size=original",
            "jpeg",
        );

        assert!(key.starts_with("metadata/42/tmdbprovider/posterimage-"));
        assert!(key.ends_with(".jpg"));
        assert!(!key.contains(".."));
        assert!(!key.contains('?'));

        let same = artwork_storage_key(
            42,
            " TMDB Provider ",
            "../Poster Image",
            "https://image.example.test/path/poster.jpg?size=original",
            "jpeg",
        );
        assert_eq!(key, same);
    }

    #[test]
    fn safe_artwork_path_segment_and_extension_normalize_inputs() {
        assert_eq!(
            safe_artwork_path_segment(" TMDB Provider ", "metadata"),
            "tmdbprovider"
        );
        assert_eq!(safe_artwork_path_segment("../", "metadata"), "metadata");
        assert_eq!(safe_artwork_extension("JPEG"), "jpg");
        assert_eq!(safe_artwork_extension("webp"), "webp");
        assert_eq!(safe_artwork_extension("unknown"), "jpg");
    }

    #[test]
    fn decode_artwork_reads_dimensions_and_rejects_empty_bytes() {
        let mut bytes = std::io::Cursor::new(Vec::new());
        image::RgbImage::new(7, 13)
            .write_to(&mut bytes, image::ImageFormat::Png)
            .expect("test png should encode");

        let decoded = decode_artwork(bytes.get_ref()).expect("test png should decode");
        assert_eq!(decoded.extension, "png");
        assert_eq!(decoded.width, Some(7));
        assert_eq!(decoded.height, Some(13));
        assert!(decode_artwork(&[]).is_err());
    }

    #[test]
    fn metadata_failed_hook_payload_exposes_public_failure_boundary() {
        let event = metadata_refresh_failed_event("job-1", "item-1", "provider timeout");

        assert_eq!(event.event_key, METADATA_REFRESH_FAILED_EVENT);
        assert_eq!(event.aggregate_type, "media_item");
        assert_eq!(event.aggregate_id, "item-1");
        assert_eq!(event.payload["jobId"], "job-1");
        assert_eq!(event.payload["itemId"], "item-1");
        assert_eq!(event.payload["status"], "failed");
        assert_eq!(event.payload["matched"], false);
        assert_eq!(event.payload["error"], "provider timeout");
        assert!(event.payload.get("jobInternalId").is_none());
        assert!(event.payload.get("mediaItemId").is_none());
    }

    #[test]
    fn metadata_refresh_job_lease_policy_is_bounded_and_retryable() {
        assert_eq!(METADATA_REFRESH_JOB_TYPE, "metadata.refresh");
        assert_eq!(METADATA_REFRESH_JOB_LEASE_SECONDS, 600);
        assert_ne!(
            METADATA_REFRESH_LEASE_EXPIRED_RETRY,
            METADATA_REFRESH_LEASE_EXPIRED_FINAL
        );
        assert!(METADATA_REFRESH_LEASE_EXPIRED_RETRY.contains("retry"));
        assert!(METADATA_REFRESH_LEASE_EXPIRED_FINAL.contains("max attempts"));
    }

    fn lookup_target() -> MetadataTarget {
        MetadataTarget {
            id: 1,
            public_id: "00000000-0000-0000-0000-000000000001".to_owned(),
            item_type: "movie".to_owned(),
            title: "Movie".to_owned(),
            original_title: None,
            production_year: Some(2026),
            season_number: None,
            episode_number: None,
            language: None,
            country: None,
            image_language: None,
            image_prefer_original: None,
            image_fallback_languages: None,
            tmdb_id: None,
            imdb_id: None,
            tvdb_id: None,
        }
    }

    #[test]
    fn build_lookup_library_image_language_overrides_global() {
        let global = MetadataGlobalSettings {
            provider_order: Vec::new(),
            default_language: Some("en".to_owned()),
            default_country: Some("US".to_owned()),
            image_language: Some("en".to_owned()),
            image_prefer_original: false,
            image_fallback_languages: vec!["en".to_owned()],
        };
        let mut target = lookup_target();
        target.image_language = Some("ja".to_owned());
        target.image_prefer_original = Some(true);
        target.image_fallback_languages = Some(vec!["none".to_owned()]);

        let lookup = build_lookup(&target, Some(&global));

        // Library image policy wins over the global defaults.
        assert_eq!(lookup.image_language.as_deref(), Some("ja"));
        assert!(lookup.image_prefer_original);
        assert_eq!(lookup.image_fallback_languages, vec!["none".to_owned()]);
        // Text language still falls back to global (library left it unset).
        assert_eq!(lookup.language.as_deref(), Some("en"));
    }

    #[test]
    fn build_lookup_falls_back_to_global_image_policy_when_library_unset() {
        let global = MetadataGlobalSettings {
            provider_order: Vec::new(),
            default_language: Some("zh-CN".to_owned()),
            default_country: None,
            image_language: Some("zh".to_owned()),
            image_prefer_original: true,
            image_fallback_languages: vec!["en".to_owned(), "none".to_owned()],
        };
        let target = lookup_target();

        let lookup = build_lookup(&target, Some(&global));

        // No library override -> inherit the global image policy.
        assert_eq!(lookup.image_language.as_deref(), Some("zh"));
        assert!(lookup.image_prefer_original);
        assert_eq!(
            lookup.image_fallback_languages,
            vec!["en".to_owned(), "none".to_owned()]
        );
        assert_eq!(lookup.language.as_deref(), Some("zh-CN"));
    }

    #[test]
    fn build_lookup_empty_library_fallback_does_not_shadow_global() {
        let global = MetadataGlobalSettings {
            provider_order: Vec::new(),
            default_language: None,
            default_country: None,
            image_language: None,
            image_prefer_original: false,
            image_fallback_languages: vec!["en".to_owned()],
        };
        let mut target = lookup_target();
        // An empty (but Some) library list must not blank out the global fallback.
        target.image_fallback_languages = Some(Vec::new());

        let lookup = build_lookup(&target, Some(&global));

        assert_eq!(lookup.image_fallback_languages, vec!["en".to_owned()]);
    }

    #[test]
    fn metadata_load_target_sql_selects_library_image_policy() {
        assert!(METADATA_LOAD_TARGET_SQL.contains("l.preferred_image_language"));
        assert!(METADATA_LOAD_TARGET_SQL.contains("l.preferred_image_prefer_original"));
        assert!(METADATA_LOAD_TARGET_SQL.contains("l.preferred_image_fallback_languages"));
    }

    // Live-DB smoke: validates migration 0079's library image-language columns
    // and the core metadata.refresh worker `load_target` SQL against the real
    // migrated schema (this SQL was previously only compile- and string-checked).
    // Seeds a library with an image-language override + a media item, loads the
    // target through the production query, asserts the override round-trips, and
    // confirms build_lookup folds it over the global default. Cleans up after.
    //   cargo test -- --ignored metadata_load_target_executes_against_live_schema
    #[tokio::test]
    #[ignore = "requires a running PostgreSQL from ./scripts/dev-deps.ps1"]
    async fn metadata_load_target_executes_against_live_schema() {
        use sqlx::Row;
        use sqlx::postgres::PgPoolOptions;

        let url = std::env::var("DATABASE_URL")
            .unwrap_or_else(|_| "postgres://fbz:fbz@127.0.0.1:5432/fbz".to_owned());
        let pool = PgPoolOptions::new()
            .max_connections(2)
            .connect(&url)
            .await
            .expect("connect to live PostgreSQL");
        crate::db::migrate(&pool).await.expect("run migrations");

        // Seed a distinctly-named library carrying an image-language override.
        let library_id: i64 = sqlx::query_scalar(
            r#"
            insert into libraries (
                name, library_type,
                preferred_metadata_language, preferred_metadata_country,
                preferred_image_language, preferred_image_prefer_original,
                preferred_image_fallback_languages
            )
            values (
                'metadata-load-target-smoke', 'movies',
                'en', 'US', 'ja', true, array['none']::text[]
            )
            returning id
            "#,
        )
        .fetch_one(&pool)
        .await
        .expect("insert smoke library");

        let item_public_id: String = sqlx::query(
            r#"
            insert into media_items (library_id, item_type, title)
            values ($1, 'movie', 'Load Target Smoke')
            returning public_id::text as public_id
            "#,
        )
        .bind(library_id)
        .fetch_one(&pool)
        .await
        .expect("insert smoke media item")
        .try_get("public_id")
        .expect("read public_id");

        // A fixed (empty-config) registry is enough: load_target/build_lookup do
        // not hit the network, and we only exercise the DB-backed load path.
        let registry = MetadataProviderRegistry::from_config(
            MetadataConfig {
                providers: Vec::new(),
                tmdb_access_token: None,
                tmdb_api_base_url: "https://api.themoviedb.org/3".to_owned(),
                tmdb_image_base_url: "https://image.tmdb.org/t/p".to_owned(),
                tvdb_api_key: None,
                tvdb_api_base_url: "https://api4.thetvdb.com/v4".to_owned(),
                fanart_api_key: None,
                fanart_api_base_url: "https://webservice.fanart.tv/v3".to_owned(),
                spotify_client_id: None,
                spotify_client_secret: None,
                spotify_api_base_url: "https://api.spotify.com/v1".to_owned(),
                spotify_auth_url: "https://accounts.spotify.com/api/token".to_owned(),
            },
            ProxyConfig {
                http_proxy: None,
                https_proxy: None,
                no_proxy: Vec::new(),
                policy: "system".to_owned(),
            },
        )
        .expect("build fixed registry");
        let service =
            MetadataService::with_provider(pool.clone(), registry, "smoke-worker".to_owned());

        let target = service
            .load_target(&item_public_id)
            .await
            .expect("load_target should execute against the live schema");

        // The library image-language override round-trips through the worker SQL.
        assert_eq!(target.image_language.as_deref(), Some("ja"));
        assert_eq!(target.image_prefer_original, Some(true));
        assert_eq!(
            target.image_fallback_languages,
            Some(vec!["none".to_owned()])
        );
        assert_eq!(target.language.as_deref(), Some("en"));

        // build_lookup keeps the library override above a conflicting global.
        let global = MetadataGlobalSettings {
            provider_order: Vec::new(),
            default_language: Some("fr".to_owned()),
            default_country: Some("FR".to_owned()),
            image_language: Some("fr".to_owned()),
            image_prefer_original: false,
            image_fallback_languages: vec!["fr".to_owned()],
        };
        let lookup = build_lookup(&target, Some(&global));
        assert_eq!(lookup.image_language.as_deref(), Some("ja"));
        assert!(lookup.image_prefer_original);

        // Cleanup: media_items cascade is not assumed; delete child then library.
        sqlx::query("delete from media_items where library_id = $1")
            .bind(library_id)
            .execute(&pool)
            .await
            .expect("cleanup smoke media item");
        sqlx::query("delete from libraries where id = $1")
            .bind(library_id)
            .execute(&pool)
            .await
            .expect("cleanup smoke library");
    }

    #[test]
    fn metadata_public_id_inputs_use_uuid_comparisons() {
        assert!(METADATA_CLAIM_JOB_SQL.contains("with requested_job as"));
        assert!(METADATA_CLAIM_JOB_SQL.contains("$1::uuid"));
        assert!(METADATA_CLAIM_JOB_SQL.contains("jobs.public_id = requested_job.public_id"));
        assert!(!METADATA_CLAIM_JOB_SQL.contains("public_id::text = $1"));

        assert!(METADATA_LOAD_TARGET_SQL.contains("where mi.public_id = case"));
        assert!(METADATA_LOAD_TARGET_SQL.contains("$1::uuid"));
        assert!(!METADATA_LOAD_TARGET_SQL.contains("mi.public_id::text = $1"));
    }
}
