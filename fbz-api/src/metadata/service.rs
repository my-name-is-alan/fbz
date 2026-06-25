use std::{
    collections::BTreeSet,
    error::Error,
    fmt::{Display, Formatter},
};

use serde_json::{Value, json};
use sqlx::{Row, postgres::PgRow};
use tracing::warn;

use crate::{
    config::{MetadataConfig, ProxyConfig},
    db::DbPool,
    jobs::{ExpiredJobMessages, expire_stale_running_jobs, mark_job_failed},
    metadata::provider::{
        MetadataLookup, MetadataMatch, MetadataProviderAttempt, MetadataProviderClient,
        MetadataProviderError,
    },
    metadata::write::{replace_item_genres, replace_item_people, replace_item_studios},
    plugins::hooks::{PluginHookDispatcher, PluginHookEvent},
};

const METADATA_WORKER_ID: &str = "fbz-api-metadata";
pub const METADATA_REFRESH_JOB_TYPE: &str = "metadata.refresh";
const METADATA_REFRESH_JOB_LEASE_SECONDS: i64 = 10 * 60;
const METADATA_REFRESH_LEASE_EXPIRED_RETRY: &str = "metadata refresh lease expired; will retry";
const METADATA_REFRESH_LEASE_EXPIRED_FINAL: &str =
    "metadata refresh lease expired; max attempts reached";
const METADATA_REFRESH_COMPLETED_EVENT: &str = "metadata.refresh.completed";
const METADATA_REFRESH_FAILED_EVENT: &str = "metadata.refresh.failed";
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
                   mi.production_year,
                   l.preferred_metadata_language,
                   l.preferred_metadata_country
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
    provider: MetadataProviderClient,
    worker_id: String,
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
    production_year: Option<i32>,
    language: Option<String>,
    country: Option<String>,
}

#[derive(Debug)]
pub enum MetadataError {
    JobNotFound,
    MissingItemId,
    ItemNotFound(String),
    Database(sqlx::Error),
    Provider(MetadataProviderError),
}

impl MetadataService {
    pub fn new(
        pool: DbPool,
        metadata: MetadataConfig,
        proxy: ProxyConfig,
    ) -> Result<Self, MetadataError> {
        Ok(Self {
            pool,
            provider: MetadataProviderClient::from_config(metadata, proxy)
                .map_err(MetadataError::Provider)?,
            worker_id: METADATA_WORKER_ID.to_owned(),
        })
    }

    pub fn with_provider(
        pool: DbPool,
        provider: MetadataProviderClient,
        worker_id: String,
    ) -> Self {
        Self {
            pool,
            provider,
            worker_id,
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
        let lookup = MetadataLookup {
            item_type: target.item_type.clone(),
            title: target.title.clone(),
            production_year: target.production_year,
            language: target.language.clone(),
            country: target.country.clone(),
        };

        let report = self
            .provider
            .match_item_with_report(&lookup)
            .await
            .map_err(MetadataError::Provider)?;

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
        let mut tx = self.pool.begin().await.map_err(MetadataError::Database)?;
        let provider_fingerprint = format!("{}:{}", found.provider, found.external_id);
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

            for image in &found.artwork {
                let source = metadata_artwork_source(found, image.source.as_deref());
                sqlx::query(
                    r#"
                    insert into artwork (
                        media_item_id,
                        artwork_type,
                        source,
                        remote_url,
                        is_primary
                    )
                    values ($1, $2, $3, $4, $5)
                    "#,
                )
                .bind(media_item_id)
                .bind(image.artwork_type.trim())
                .bind(source)
                .bind(image.remote_url.trim())
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
            production_year: row.try_get("production_year")?,
            language: row.try_get("preferred_metadata_language")?,
            country: row.try_get("preferred_metadata_country")?,
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

impl Display for MetadataError {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::JobNotFound => f.write_str("metadata refresh job not found or not runnable"),
            Self::MissingItemId => f.write_str("metadata refresh job payload is missing itemId"),
            Self::ItemNotFound(item_id) => write!(f, "media item `{item_id}` not found"),
            Self::Database(err) => write!(f, "database error: {err}"),
            Self::Provider(err) => write!(f, "{err}"),
        }
    }
}

impl Error for MetadataError {}

#[cfg(test)]
mod tests {
    use crate::metadata::provider::{
        MetadataArtwork, MetadataExternalId, MetadataProviderAttemptStatus,
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
            people: Vec::new(),
        };

        assert_eq!(
            metadata_artwork_sources_for_match(&found),
            vec!["fanart".to_owned(), "tmdb".to_owned()]
        );
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
