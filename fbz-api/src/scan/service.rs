use std::{
    error::Error,
    fmt::{Display, Formatter},
    fs,
    path::{Path, PathBuf},
    time::{SystemTime, UNIX_EPOCH},
};

use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use sqlx::{Row, postgres::PgRow};
use tracing::warn;

use crate::{
    db::DbPool,
    jobs::{ExpiredJobMessages, expire_stale_running_jobs, mark_job_failed},
    media::probe::MEDIA_PROBE_JOB_TYPE,
    metadata::service::METADATA_REFRESH_JOB_TYPE,
    plugins::hooks::{PluginHookDispatcher, PluginHookEvent},
};

const SCAN_WORKER_ID: &str = "fbz-api-inline-scan";
const LIBRARY_SCAN_JOB_TYPE: &str = "library.scan";
const SCAN_JOB_LEASE_SECONDS: i64 = 15 * 60;
const MAX_SCAN_FILES: usize = 10_000;
const MAX_SCAN_METADATA_QUEUE_ITEMS: i64 = 10_000;
const SCAN_CURSOR_PAYLOAD_KEY: &str = "cursor";
const SCAN_ID_PAYLOAD_KEY: &str = "scanId";
const SCAN_JOB_LEASE_EXPIRED_RETRY: &str = "scan job lease expired; will retry";
const SCAN_JOB_LEASE_EXPIRED_FINAL: &str = "scan job lease expired; max attempts reached";
const LIBRARY_SCAN_STARTED_EVENT: &str = "library.scan.started";
const LIBRARY_SCAN_COMPLETED_EVENT: &str = "library.scan.completed";
const LIBRARY_SCAN_FAILED_EVENT: &str = "library.scan.failed";
const SCAN_CLAIM_JOB_SQL: &str = r#"
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
                  and job_type = $3
                  and status in ('queued', 'failed')
                  and attempts < max_attempts
                  and run_at <= now()
                order by priority desc, run_at asc, jobs.id asc
                limit 1
                for update of jobs skip locked
            )
            update jobs j
            set status = 'running',
                locked_by = $2,
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
const SCAN_LOAD_LIBRARY_TARGET_SQL: &str = r#"
            select
                id,
                public_id::text as public_id,
                library_type
            from libraries
            where public_id = case
                when $1::text ~* '^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$'
                then $1::uuid
                else null::uuid
            end
              and is_hidden = false
            "#;

#[derive(Clone)]
pub struct ScanService {
    pool: DbPool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ScanRunSummary {
    pub job_id: String,
    pub status: String,
    pub scanned_files: usize,
    pub created_items: usize,
    pub updated_files: usize,
    pub metadata_refresh_jobs: i64,
    pub probe_jobs: i64,
    pub missing_items: i64,
    pub missing_mark_skipped: bool,
    pub has_more: bool,
    pub continuation_job_id: Option<String>,
}

#[derive(Clone, Debug)]
struct ClaimedScanJob {
    id: i64,
    public_id: String,
    payload: Value,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct ScanJobRequest {
    library_id: String,
    requested_by_user_id: Option<String>,
    reason: Option<String>,
    scan_id: Option<String>,
    cursor: Option<ScanCursor>,
}

#[derive(Clone, Debug)]
struct LibraryScanTarget {
    library_id: i64,
    library_public_id: String,
    library_type: String,
    paths: Vec<LibraryPathTarget>,
}

#[derive(Clone, Debug)]
struct LibraryPathTarget {
    id: i64,
    path: PathBuf,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct ScanFile {
    path: PathBuf,
    file_size: Option<i64>,
    modified_at_epoch_ms: Option<i64>,
    is_strm: bool,
    strm_target: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct ExistingMediaFileObservation {
    media_item_id: i64,
    file_size: Option<i64>,
    modified_at_epoch_ms: Option<i64>,
    is_strm: bool,
    strm_target: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
struct ScanCursor {
    path_index: usize,
    pending_paths: Vec<String>,
    #[serde(default)]
    unavailable_roots: usize,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct ScanPage {
    files: Vec<ScanFile>,
    next_cursor: Option<ScanCursor>,
    unavailable_roots: usize,
}

#[derive(Debug)]
pub enum ScanError {
    JobNotFound,
    MissingLibraryId,
    InvalidCursor(String),
    LibraryNotFound(String),
    Database(sqlx::Error),
    Io(std::io::Error),
    Join(tokio::task::JoinError),
}

impl ScanService {
    pub fn new(pool: DbPool) -> Self {
        Self { pool }
    }

    pub async fn run_scan_job(&self, job_id: &str) -> Result<ScanRunSummary, ScanError> {
        let Some(job) = self.claim_scan_job(Some(job_id)).await? else {
            return Err(ScanError::JobNotFound);
        };

        self.run_claimed_scan_job(job).await
    }

    pub async fn run_next_scan_job(&self) -> Result<Option<ScanRunSummary>, ScanError> {
        let Some(job) = self.claim_scan_job(None).await? else {
            return Ok(None);
        };

        self.run_claimed_scan_job(job).await.map(Some)
    }

    async fn run_claimed_scan_job(&self, job: ClaimedScanJob) -> Result<ScanRunSummary, ScanError> {
        let request = ScanJobRequest::from_payload(&job.payload)?;
        let scan_id = request.effective_scan_id(&job.public_id);

        let run_id = self.start_job_run(job.id).await?;
        self.record_job_event(
            job.id,
            Some(run_id),
            LIBRARY_SCAN_STARTED_EVENT,
            "info",
            "scan started",
        )
        .await?;
        self.dispatch_scan_hook(scan_started_event(&job.public_id, &request.library_id))
            .await;

        let result = self.scan_library(&job.public_id, &scan_id, &request).await;
        match result {
            Ok(summary) => {
                self.record_job_event(
                    job.id,
                    Some(run_id),
                    "library.scan.summary",
                    "info",
                    &format!("scan completed: {} files", summary.scanned_files),
                )
                .await?;
                self.finish_job_success(job.id, run_id, &summary).await?;
                self.dispatch_scan_hook(scan_completed_event(
                    &job.public_id,
                    &request.library_id,
                    &summary,
                ))
                .await;
                Ok(ScanRunSummary {
                    job_id: job.public_id,
                    status: if summary.has_more {
                        "continuing".to_owned()
                    } else {
                        "succeeded".to_owned()
                    },
                    scanned_files: summary.scanned_files,
                    created_items: summary.created_items,
                    updated_files: summary.updated_files,
                    metadata_refresh_jobs: summary.metadata_refresh_jobs,
                    probe_jobs: summary.probe_jobs,
                    missing_items: summary.missing_items,
                    missing_mark_skipped: summary.missing_mark_skipped,
                    has_more: summary.has_more,
                    continuation_job_id: summary.continuation_job_id,
                })
            }
            Err(err) => {
                let message = err.to_string();
                self.finish_job_failure(&job.public_id, job.id, run_id, &message)
                    .await?;
                self.dispatch_scan_hook(scan_failed_event(
                    &job.public_id,
                    &request.library_id,
                    &message,
                ))
                .await;
                Err(err)
            }
        }
    }

    async fn dispatch_scan_hook(&self, event: PluginHookEvent) {
        let event_key = event.event_key.clone();
        let library_id = event.aggregate_id.clone();
        if let Err(err) = PluginHookDispatcher::new(self.pool.clone())
            .dispatch(event)
            .await
        {
            warn!(
                error = %err,
                event_key = %event_key,
                library_id = %library_id,
                "failed to dispatch plugin scan hooks"
            );
        }
    }

    async fn claim_scan_job(
        &self,
        job_id: Option<&str>,
    ) -> Result<Option<ClaimedScanJob>, ScanError> {
        let mut tx = self.pool.begin().await.map_err(ScanError::Database)?;
        expire_stale_running_jobs(
            &mut tx,
            LIBRARY_SCAN_JOB_TYPE,
            ExpiredJobMessages {
                retry: SCAN_JOB_LEASE_EXPIRED_RETRY,
                final_failure: SCAN_JOB_LEASE_EXPIRED_FINAL,
            },
        )
        .await
        .map_err(ScanError::Database)?;

        let job = sqlx::query(SCAN_CLAIM_JOB_SQL)
            .bind(job_id)
            .bind(SCAN_WORKER_ID)
            .bind(LIBRARY_SCAN_JOB_TYPE)
            .bind(SCAN_JOB_LEASE_SECONDS)
            .fetch_optional(&mut *tx)
            .await
            .map_err(ScanError::Database)?
            .map(ClaimedScanJob::from_row)
            .transpose()
            .map_err(ScanError::Database)?;

        tx.commit().await.map_err(ScanError::Database)?;
        Ok(job)
    }

    async fn start_job_run(&self, job_id: i64) -> Result<i64, ScanError> {
        sqlx::query_scalar::<_, i64>(
            r#"
            insert into job_runs (job_id, worker_id, status)
            values ($1, $2, 'running')
            returning id
            "#,
        )
        .bind(job_id)
        .bind(SCAN_WORKER_ID)
        .fetch_one(&self.pool)
        .await
        .map_err(ScanError::Database)
    }

    async fn scan_library(
        &self,
        job_public_id: &str,
        scan_id: &str,
        request: &ScanJobRequest,
    ) -> Result<PartialScanSummary, ScanError> {
        let target = self.load_library_target(&request.library_id).await?;
        let page =
            discover_media_files(target.paths.clone(), request.cursor.clone(), MAX_SCAN_FILES)
                .await?;
        let mut summary = PartialScanSummary::default();
        let mut touched_media_item_ids = Vec::new();
        let mut probe_media_file_ids = Vec::new();

        for file in page.files {
            if !is_supported_media_file(&file.path) {
                continue;
            }

            let item_type = item_type_for_file(&target.library_type, &file.path);
            let title = title_from_path(&file.path);
            let path_string = file.path.to_string_lossy().into_owned();
            let normalized_path = normalize_path(&path_string);
            let path_hash = sha256(normalized_path.as_bytes());
            let library_path_id = target
                .paths
                .iter()
                .find(|library_path| file.path.starts_with(&library_path.path))
                .map(|library_path| library_path.id);

            let mut tx = self.pool.begin().await.map_err(ScanError::Database)?;
            let existing_file = sqlx::query(
                r#"
                select media_item_id,
                       file_size,
                       case
                           when modified_at is null then null
                           else floor(extract(epoch from modified_at) * 1000)::bigint
                       end as modified_at_epoch_ms,
                       is_strm,
                       strm_target
                from media_files
                where path_hash = $1
                "#,
            )
            .bind(&path_hash)
            .fetch_optional(&mut *tx)
            .await
            .map_err(ScanError::Database)?
            .map(ExistingMediaFileObservation::from_row)
            .transpose()
            .map_err(ScanError::Database)?;

            let media_item_id = if let Some(existing) = existing_file.as_ref() {
                existing.media_item_id
            } else {
                summary.created_items += 1;
                sqlx::query_scalar::<_, i64>(
                    r#"
                    insert into media_items (
                        library_id,
                        item_type,
                        title,
                        sort_title,
                        metadata_status,
                        scan_status
                    )
                    values ($1, $2, $3, $3, 'pending', 'scanned')
                    returning id
                    "#,
                )
                .bind(target.library_id)
                .bind(item_type)
                .bind(&title)
                .fetch_one(&mut *tx)
                .await
                .map_err(ScanError::Database)?
            };

            if existing_file
                .as_ref()
                .is_some_and(|existing| file_observation_unchanged(existing, &file))
            {
                self.mark_existing_file_seen(&mut tx, &path_hash, media_item_id, scan_id)
                    .await?;
                tx.commit().await.map_err(ScanError::Database)?;
                summary.scanned_files += 1;
                continue;
            }

            sqlx::query(
                r#"
                update media_items
                set scan_status = 'scanned',
                    updated_at = now()
                where id = $1
                "#,
            )
            .bind(media_item_id)
            .execute(&mut *tx)
            .await
            .map_err(ScanError::Database)?;

            let media_file_id = sqlx::query_scalar::<_, i64>(
                r#"
                insert into media_files (
                    media_item_id,
                    library_path_id,
                    path,
                    normalized_path,
                    path_hash,
                    file_size,
                    modified_at,
                    last_seen_scan_id,
                    last_seen_at,
                    is_primary,
                    is_strm,
                    strm_target
                )
                values (
                    $1,
                    $2,
                    $3,
                    $4,
                    $5,
                    $6,
                    case
                        when $7::bigint is null then null
                        else to_timestamp(($7::bigint)::double precision / 1000.0)
                    end,
                    $8,
                    now(),
                    true,
                    $9,
                    $10
                )
                on conflict (path_hash) do update
                    set media_item_id = excluded.media_item_id,
                        library_path_id = excluded.library_path_id,
                        path = excluded.path,
                        normalized_path = excluded.normalized_path,
                        file_size = excluded.file_size,
                        modified_at = excluded.modified_at,
                        last_seen_scan_id = excluded.last_seen_scan_id,
                        last_seen_at = excluded.last_seen_at,
                        is_strm = excluded.is_strm,
                        strm_target = excluded.strm_target,
                        updated_at = now()
                returning id
                "#,
            )
            .bind(media_item_id)
            .bind(library_path_id)
            .bind(&path_string)
            .bind(normalized_path)
            .bind(path_hash)
            .bind(file.file_size)
            .bind(file.modified_at_epoch_ms)
            .bind(scan_id)
            .bind(file.is_strm)
            .bind(file.strm_target)
            .fetch_one(&mut *tx)
            .await
            .map_err(ScanError::Database)?;

            tx.commit().await.map_err(ScanError::Database)?;
            touched_media_item_ids.push(media_item_id);
            if !file.is_strm {
                probe_media_file_ids.push(media_file_id);
            }
            summary.scanned_files += 1;
            summary.updated_files += 1;
        }

        summary.metadata_refresh_jobs = self
            .queue_metadata_refresh_for_items(
                &touched_media_item_ids,
                &format!("scan completed for library {}", request.library_id),
            )
            .await?;
        summary.probe_jobs = self
            .queue_probe_for_files(
                &probe_media_file_ids,
                &format!("scan updated files for library {}", request.library_id),
            )
            .await?;

        if let Some(cursor) = page.next_cursor {
            summary.has_more = true;
            summary.continuation_job_id = self
                .queue_continuation_scan_job(job_public_id, scan_id, request, cursor)
                .await?;
        } else if page.unavailable_roots == 0 {
            summary.missing_items = self.mark_missing_items(target.library_id, scan_id).await?;
        } else {
            summary.missing_mark_skipped = true;
        }

        let _ = target.library_public_id;

        Ok(summary)
    }

    async fn load_library_target(&self, library_id: &str) -> Result<LibraryScanTarget, ScanError> {
        let Some(library_row) = sqlx::query(SCAN_LOAD_LIBRARY_TARGET_SQL)
            .bind(library_id)
            .fetch_optional(&self.pool)
            .await
            .map_err(ScanError::Database)?
        else {
            return Err(ScanError::LibraryNotFound(library_id.to_owned()));
        };

        let library_row_id = library_row
            .try_get::<i64, _>("id")
            .map_err(ScanError::Database)?;
        let path_rows = sqlx::query(
            r#"
            select id,
                   path
            from library_paths
            where library_id = $1
              and is_enabled = true
            order by id
            "#,
        )
        .bind(library_row_id)
        .fetch_all(&self.pool)
        .await
        .map_err(ScanError::Database)?;

        let paths = path_rows
            .into_iter()
            .map(|row| {
                Ok(LibraryPathTarget {
                    id: row.try_get("id")?,
                    path: PathBuf::from(row.try_get::<String, _>("path")?),
                })
            })
            .collect::<Result<Vec<_>, sqlx::Error>>()
            .map_err(ScanError::Database)?;

        Ok(LibraryScanTarget {
            library_id: library_row_id,
            library_public_id: library_row
                .try_get("public_id")
                .map_err(ScanError::Database)?,
            library_type: library_row
                .try_get("library_type")
                .map_err(ScanError::Database)?,
            paths,
        })
    }

    async fn finish_job_success(
        &self,
        job_id: i64,
        run_id: i64,
        summary: &PartialScanSummary,
    ) -> Result<(), ScanError> {
        let metrics = serde_json::json!({
            "scannedFiles": summary.scanned_files,
            "createdItems": summary.created_items,
            "updatedFiles": summary.updated_files,
            "metadataRefreshJobs": summary.metadata_refresh_jobs,
            "probeJobs": summary.probe_jobs,
            "missingItems": summary.missing_items,
            "missingMarkSkipped": summary.missing_mark_skipped,
            "hasMore": summary.has_more,
            "continuationJobId": summary.continuation_job_id,
        });

        let mut tx = self.pool.begin().await.map_err(ScanError::Database)?;
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
        .map_err(ScanError::Database)?;
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
        .map_err(ScanError::Database)?;
        tx.commit().await.map_err(ScanError::Database)
    }

    async fn queue_metadata_refresh_for_items(
        &self,
        item_ids: &[i64],
        reason: &str,
    ) -> Result<i64, ScanError> {
        if item_ids.is_empty() {
            return Ok(0);
        }

        let payload_reason = json!(reason);
        sqlx::query_scalar::<_, i64>(
            r#"
            with touched_items as (
                select distinct unnest($1::bigint[]) as id
            ),
            eligible_items as (
                select mi.public_id::text as item_public_id
                from media_items mi
                join touched_items ti
                  on ti.id = mi.id
                where mi.is_deleted = false
                  and mi.metadata_status in ('pending', 'failed')
                  and mi.item_type in ('movie', 'series', 'episode')
                  and not exists (
                      select 1
                      from jobs j
                      where j.job_type = $2
                        and j.status in ('queued', 'running', 'failed')
                        and j.attempts < j.max_attempts
                        and j.payload->>'itemId' = mi.public_id::text
                  )
                order by mi.updated_at asc, mi.id asc
                limit $3
            ),
            inserted as (
                insert into jobs (
                    job_type,
                    status,
                    queue_name,
                    priority,
                    payload
                )
                select
                    $2,
                    'queued',
                    'metadata',
                    -5,
                    jsonb_build_object(
                        'itemId', eligible_items.item_public_id,
                        'requestedByUserId', null,
                        'reason', $4::jsonb
                    )
                from eligible_items
                on conflict do nothing
                returning id
            )
            select count(*)::bigint from inserted
            "#,
        )
        .bind(item_ids)
        .bind(METADATA_REFRESH_JOB_TYPE)
        .bind(MAX_SCAN_METADATA_QUEUE_ITEMS)
        .bind(payload_reason)
        .fetch_one(&self.pool)
        .await
        .map_err(ScanError::Database)
    }

    async fn queue_probe_for_files(
        &self,
        media_file_ids: &[i64],
        reason: &str,
    ) -> Result<i64, ScanError> {
        if media_file_ids.is_empty() {
            return Ok(0);
        }

        let payload_reason = json!(reason);
        sqlx::query_scalar::<_, i64>(
            r#"
            with target_files as (
                select distinct unnest($1::bigint[]) as media_file_id
            ),
            eligible_files as (
                select mf.id as media_file_id
                from media_files mf
                join media_items mi on mi.id = mf.media_item_id
                join target_files tf on tf.media_file_id = mf.id
                where mi.is_deleted = false
                  and mi.scan_status <> 'missing'
                  and mf.is_strm = false
                  and not exists (
                      select 1
                      from jobs j
                      where j.job_type = $2
                        and j.status in ('queued', 'running', 'failed')
                        and j.attempts < j.max_attempts
                        and j.payload->>'mediaFileId' = mf.id::text
                  )
            ),
            inserted as (
                insert into jobs (
                    job_type,
                    status,
                    queue_name,
                    priority,
                    payload
                )
                select
                    $2,
                    'queued',
                    'probe',
                    -3,
                    jsonb_build_object(
                        'mediaFileId', eligible_files.media_file_id,
                        'reason', $3::jsonb
                    )
                from eligible_files
                on conflict do nothing
                returning id
            )
            select count(*)::bigint from inserted
            "#,
        )
        .bind(media_file_ids)
        .bind(MEDIA_PROBE_JOB_TYPE)
        .bind(payload_reason)
        .fetch_one(&self.pool)
        .await
        .map_err(ScanError::Database)
    }

    async fn mark_existing_file_seen(
        &self,
        tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
        path_hash: &[u8],
        media_item_id: i64,
        scan_id: &str,
    ) -> Result<(), ScanError> {
        sqlx::query(
            r#"
            update media_files
            set last_seen_scan_id = $2,
                last_seen_at = now()
            where path_hash = $1
              and (
                  last_seen_scan_id is distinct from $2
                  or last_seen_at is null
              )
            "#,
        )
        .bind(path_hash)
        .bind(scan_id)
        .execute(&mut **tx)
        .await
        .map_err(ScanError::Database)?;

        sqlx::query(
            r#"
            update media_items
            set scan_status = 'scanned',
                updated_at = now()
            where id = $1
              and scan_status = 'missing'
            "#,
        )
        .bind(media_item_id)
        .execute(&mut **tx)
        .await
        .map_err(ScanError::Database)?;

        Ok(())
    }

    async fn mark_missing_items(&self, library_id: i64, scan_id: &str) -> Result<i64, ScanError> {
        sqlx::query_scalar::<_, i64>(
            r#"
            with missing_items as (
                update media_items mi
                set scan_status = 'missing',
                    updated_at = now()
                where mi.library_id = $1
                  and mi.is_deleted = false
                  and mi.scan_status <> 'missing'
                  and exists (
                      select 1
                      from media_files mf
                      where mf.media_item_id = mi.id
                  )
                  and not exists (
                      select 1
                      from media_files mf
                      where mf.media_item_id = mi.id
                        and mf.last_seen_scan_id = $2
                  )
                returning mi.id
            )
            select count(*)::bigint
            from missing_items
            "#,
        )
        .bind(library_id)
        .bind(scan_id)
        .fetch_one(&self.pool)
        .await
        .map_err(ScanError::Database)
    }

    async fn queue_continuation_scan_job(
        &self,
        parent_job_id: &str,
        scan_id: &str,
        request: &ScanJobRequest,
        cursor: ScanCursor,
    ) -> Result<Option<String>, ScanError> {
        let cursor_value = serde_json::to_value(&cursor)
            .map_err(|err| ScanError::InvalidCursor(err.to_string()))?;
        let payload = json!({
            "libraryId": request.library_id,
            "requestedByUserId": request.requested_by_user_id,
            "reason": request.reason,
            "continuationOfJobId": parent_job_id,
            SCAN_ID_PAYLOAD_KEY: scan_id,
            SCAN_CURSOR_PAYLOAD_KEY: cursor_value,
        });
        let dedupe_key = continuation_dedupe_key(parent_job_id, &cursor)?;

        sqlx::query_scalar::<_, String>(
            r#"
            insert into jobs (
                job_type,
                status,
                queue_name,
                priority,
                payload,
                dedupe_key
            )
            values ($1, 'queued', 'scan', 0, $2, $3)
            on conflict (dedupe_key) do update
                set updated_at = jobs.updated_at
            returning public_id::text
            "#,
        )
        .bind(LIBRARY_SCAN_JOB_TYPE)
        .bind(payload)
        .bind(dedupe_key)
        .fetch_optional(&self.pool)
        .await
        .map_err(ScanError::Database)
    }

    async fn finish_job_failure(
        &self,
        job_public_id: &str,
        job_id: i64,
        run_id: i64,
        message: &str,
    ) -> Result<(), ScanError> {
        let mut tx = self.pool.begin().await.map_err(ScanError::Database)?;
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
        .map_err(ScanError::Database)?;
        mark_job_failed(
            &mut tx,
            LIBRARY_SCAN_JOB_TYPE,
            job_public_id,
            job_id,
            message,
        )
        .await
        .map_err(ScanError::Database)?;
        tx.commit().await.map_err(ScanError::Database)
    }

    async fn record_job_event(
        &self,
        job_id: i64,
        run_id: Option<i64>,
        event_type: &str,
        event_level: &str,
        message: &str,
    ) -> Result<(), ScanError> {
        if job_id == 0 {
            return Ok(());
        }

        sqlx::query(
            r#"
            insert into job_events (
                job_id,
                job_run_id,
                event_type,
                event_level,
                message
            )
            values ($1, $2, $3, $4, $5)
            "#,
        )
        .bind(job_id)
        .bind(run_id)
        .bind(event_type)
        .bind(event_level)
        .bind(message)
        .execute(&self.pool)
        .await
        .map_err(ScanError::Database)?;

        Ok(())
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
struct PartialScanSummary {
    scanned_files: usize,
    created_items: usize,
    updated_files: usize,
    metadata_refresh_jobs: i64,
    probe_jobs: i64,
    missing_items: i64,
    missing_mark_skipped: bool,
    has_more: bool,
    continuation_job_id: Option<String>,
}

fn scan_started_event(job_public_id: &str, library_id: &str) -> PluginHookEvent {
    PluginHookEvent {
        event_key: LIBRARY_SCAN_STARTED_EVENT.to_owned(),
        aggregate_type: "library".to_owned(),
        aggregate_id: library_id.to_owned(),
        payload: json!({
            "jobId": job_public_id,
            "libraryId": library_id,
            "status": "running",
        }),
    }
}

fn scan_completed_event(
    job_public_id: &str,
    library_id: &str,
    summary: &PartialScanSummary,
) -> PluginHookEvent {
    PluginHookEvent {
        event_key: LIBRARY_SCAN_COMPLETED_EVENT.to_owned(),
        aggregate_type: "library".to_owned(),
        aggregate_id: library_id.to_owned(),
        payload: json!({
            "jobId": job_public_id,
            "libraryId": library_id,
            "status": "succeeded",
            "scannedFiles": summary.scanned_files,
            "createdItems": summary.created_items,
            "updatedFiles": summary.updated_files,
            "metadataRefreshJobs": summary.metadata_refresh_jobs,
            "probeJobs": summary.probe_jobs,
            "missingItems": summary.missing_items,
            "missingMarkSkipped": summary.missing_mark_skipped,
            "hasMore": summary.has_more,
            "continuationJobId": summary.continuation_job_id,
        }),
    }
}

fn scan_failed_event(job_public_id: &str, library_id: &str, message: &str) -> PluginHookEvent {
    PluginHookEvent {
        event_key: LIBRARY_SCAN_FAILED_EVENT.to_owned(),
        aggregate_type: "library".to_owned(),
        aggregate_id: library_id.to_owned(),
        payload: json!({
            "jobId": job_public_id,
            "libraryId": library_id,
            "status": "failed",
            "error": message,
        }),
    }
}

impl ClaimedScanJob {
    fn from_row(row: PgRow) -> Result<Self, sqlx::Error> {
        Ok(Self {
            id: row.try_get("id")?,
            public_id: row.try_get("public_id")?,
            payload: row.try_get("payload")?,
        })
    }
}

impl ExistingMediaFileObservation {
    fn from_row(row: PgRow) -> Result<Self, sqlx::Error> {
        Ok(Self {
            media_item_id: row.try_get("media_item_id")?,
            file_size: row.try_get("file_size")?,
            modified_at_epoch_ms: row.try_get("modified_at_epoch_ms")?,
            is_strm: row.try_get("is_strm")?,
            strm_target: row.try_get("strm_target")?,
        })
    }
}

impl ScanJobRequest {
    fn from_payload(payload: &Value) -> Result<Self, ScanError> {
        let library_id = payload
            .get("libraryId")
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .ok_or(ScanError::MissingLibraryId)?
            .to_owned();
        let requested_by_user_id = payload
            .get("requestedByUserId")
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(str::to_owned);
        let reason = payload
            .get("reason")
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(str::to_owned);
        let scan_id = payload
            .get(SCAN_ID_PAYLOAD_KEY)
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(str::to_owned);
        let cursor = payload
            .get(SCAN_CURSOR_PAYLOAD_KEY)
            .map(|value| {
                serde_json::from_value::<ScanCursor>(value.clone())
                    .map_err(|err| ScanError::InvalidCursor(err.to_string()))
                    .and_then(validate_scan_cursor)
            })
            .transpose()?;

        Ok(Self {
            library_id,
            requested_by_user_id,
            reason,
            scan_id,
            cursor,
        })
    }

    fn effective_scan_id(&self, current_job_id: &str) -> String {
        self.scan_id
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .unwrap_or_else(|| current_job_id.trim())
            .to_owned()
    }
}

async fn discover_media_files(
    paths: Vec<LibraryPathTarget>,
    cursor: Option<ScanCursor>,
    max_files: usize,
) -> Result<ScanPage, ScanError> {
    tokio::task::spawn_blocking(move || {
        let mut traversal = ScanTraversal::new(paths, cursor)?;
        let mut files = Vec::new();
        while files.len() < max_files {
            let Some(path) = traversal.next_path()? else {
                break;
            };
            visit_scan_path(&path, &mut traversal, &mut files)?;
        }

        Ok(ScanPage {
            files,
            next_cursor: traversal.next_cursor(),
            unavailable_roots: traversal.unavailable_roots,
        })
    })
    .await
    .map_err(ScanError::Join)?
}

#[derive(Debug)]
struct ScanTraversal {
    roots: Vec<LibraryPathTarget>,
    path_index: usize,
    pending_paths: Vec<PathBuf>,
    unavailable_roots: usize,
}

impl ScanTraversal {
    fn new(roots: Vec<LibraryPathTarget>, cursor: Option<ScanCursor>) -> Result<Self, ScanError> {
        let Some(cursor) = cursor else {
            return Ok(Self {
                roots,
                path_index: 0,
                pending_paths: Vec::new(),
                unavailable_roots: 0,
            });
        };

        if cursor.path_index > roots.len() {
            return Err(ScanError::InvalidCursor(
                "cursor path index is out of range".to_owned(),
            ));
        }

        Ok(Self {
            roots,
            path_index: cursor.path_index,
            pending_paths: cursor
                .pending_paths
                .into_iter()
                .map(PathBuf::from)
                .collect(),
            unavailable_roots: cursor.unavailable_roots,
        })
    }

    fn next_path(&mut self) -> Result<Option<PathBuf>, ScanError> {
        loop {
            if let Some(path) = self.pending_paths.pop() {
                return Ok(Some(path));
            }
            if self.path_index >= self.roots.len() {
                return Ok(None);
            }
            let root = self.roots.get(self.path_index).ok_or_else(|| {
                ScanError::InvalidCursor("cursor path index is out of range".to_owned())
            })?;
            self.pending_paths.push(root.path.clone());
            self.path_index += 1;
        }
    }

    fn push_children(&mut self, mut children: Vec<PathBuf>) {
        children.sort_by(|left, right| {
            normalized_path_for_sort(right).cmp(&normalized_path_for_sort(left))
        });
        self.pending_paths.extend(children);
    }

    fn next_cursor(&self) -> Option<ScanCursor> {
        if self.pending_paths.is_empty() && self.path_index >= self.roots.len() {
            return None;
        }

        Some(ScanCursor {
            path_index: self.path_index,
            pending_paths: self
                .pending_paths
                .iter()
                .map(|path| path.to_string_lossy().into_owned())
                .collect(),
            unavailable_roots: self.unavailable_roots,
        })
    }

    fn mark_unavailable_if_root(&mut self, path: &Path) {
        if self.roots.iter().any(|root| root.path == path) {
            self.unavailable_roots += 1;
        }
    }
}

fn visit_scan_path(
    path: &Path,
    traversal: &mut ScanTraversal,
    files: &mut Vec<ScanFile>,
) -> Result<(), ScanError> {
    let metadata = match fs::metadata(path) {
        Ok(metadata) => metadata,
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => {
            traversal.mark_unavailable_if_root(path);
            return Ok(());
        }
        Err(err) => return Err(ScanError::Io(err)),
    };

    if metadata.is_file() {
        push_scan_file(path, metadata, files)?;
        return Ok(());
    }

    if !metadata.is_dir() {
        return Ok(());
    }

    let mut children = Vec::new();
    for entry in fs::read_dir(path).map_err(ScanError::Io)? {
        let entry = entry.map_err(ScanError::Io)?;
        children.push(entry.path());
    }
    traversal.push_children(children);

    Ok(())
}

fn push_scan_file(
    path: &Path,
    metadata: fs::Metadata,
    files: &mut Vec<ScanFile>,
) -> Result<(), ScanError> {
    if !is_supported_media_file(path) {
        return Ok(());
    }

    let is_strm = path
        .extension()
        .and_then(|ext| ext.to_str())
        .is_some_and(|ext| ext.eq_ignore_ascii_case("strm"));
    let strm_target = if is_strm {
        fs::read_to_string(path)
            .ok()
            .and_then(|content| content.lines().next().map(str::trim).map(str::to_owned))
            .filter(|value| !value.is_empty())
    } else {
        None
    };

    files.push(ScanFile {
        path: path.to_owned(),
        file_size: Some(metadata.len().min(i64::MAX as u64) as i64),
        modified_at_epoch_ms: metadata.modified().ok().and_then(system_time_epoch_millis),
        is_strm,
        strm_target,
    });
    Ok(())
}

fn is_supported_media_file(path: &Path) -> bool {
    path.extension()
        .and_then(|ext| ext.to_str())
        .map(|ext| {
            matches!(
                ext.to_ascii_lowercase().as_str(),
                "mkv"
                    | "mp4"
                    | "avi"
                    | "mov"
                    | "m4v"
                    | "ts"
                    | "strm"
                    | "mp3"
                    | "flac"
                    | "m4a"
                    | "wav"
                    | "ogg"
            )
        })
        .unwrap_or(false)
}

fn item_type_for_file(library_type: &str, path: &Path) -> &'static str {
    match library_type {
        "music" => "track",
        _ if is_audio_file(path) => "track",
        _ => "movie",
    }
}

fn is_audio_file(path: &Path) -> bool {
    path.extension()
        .and_then(|ext| ext.to_str())
        .map(|ext| {
            matches!(
                ext.to_ascii_lowercase().as_str(),
                "mp3" | "flac" | "m4a" | "wav" | "ogg"
            )
        })
        .unwrap_or(false)
}

fn title_from_path(path: &Path) -> String {
    path.file_stem()
        .and_then(|stem| stem.to_str())
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .unwrap_or("Untitled")
        .to_owned()
}

fn normalize_path(path: &str) -> String {
    path.trim().replace('\\', "/").to_ascii_lowercase()
}

fn file_observation_unchanged(existing: &ExistingMediaFileObservation, file: &ScanFile) -> bool {
    existing.file_size == file.file_size
        && existing.modified_at_epoch_ms == file.modified_at_epoch_ms
        && existing.is_strm == file.is_strm
        && existing.strm_target == file.strm_target
}

fn system_time_epoch_millis(value: SystemTime) -> Option<i64> {
    let millis = value.duration_since(UNIX_EPOCH).ok()?.as_millis();
    i64::try_from(millis).ok()
}

fn sha256(input: &[u8]) -> Vec<u8> {
    Sha256::digest(input).to_vec()
}

fn normalized_path_for_sort(path: &Path) -> String {
    normalize_path(&path.to_string_lossy())
}

fn validate_scan_cursor(cursor: ScanCursor) -> Result<ScanCursor, ScanError> {
    if cursor
        .pending_paths
        .iter()
        .any(|path| path.trim().is_empty())
    {
        return Err(ScanError::InvalidCursor(
            "cursor pending paths must not be empty".to_owned(),
        ));
    }
    Ok(cursor)
}

fn continuation_dedupe_key(parent_job_id: &str, cursor: &ScanCursor) -> Result<String, ScanError> {
    let cursor_json =
        serde_json::to_vec(cursor).map_err(|err| ScanError::InvalidCursor(err.to_string()))?;
    Ok(format!(
        "library.scan.continuation:{}:{}",
        parent_job_id.trim(),
        hex_lower(&sha256(&cursor_json))
    ))
}

fn hex_lower(bytes: &[u8]) -> String {
    const TABLE: &[u8; 16] = b"0123456789abcdef";
    let mut output = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        output.push(TABLE[(byte >> 4) as usize] as char);
        output.push(TABLE[(byte & 0x0f) as usize] as char);
    }
    output
}

impl Display for ScanError {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::JobNotFound => f.write_str("scan job not found or not runnable"),
            Self::MissingLibraryId => f.write_str("scan job payload is missing libraryId"),
            Self::InvalidCursor(message) => write!(f, "invalid scan cursor: {message}"),
            Self::LibraryNotFound(library_id) => write!(f, "library `{library_id}` not found"),
            Self::Database(err) => write!(f, "database error: {err}"),
            Self::Io(err) => write!(f, "filesystem scan error: {err}"),
            Self::Join(err) => write!(f, "scan worker join error: {err}"),
        }
    }
}

impl Error for ScanError {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn supported_media_extensions_are_detected() {
        assert!(is_supported_media_file(Path::new("movie.mkv")));
        assert!(is_supported_media_file(Path::new("song.flac")));
        assert!(is_supported_media_file(Path::new("remote.strm")));
        assert!(!is_supported_media_file(Path::new("cover.jpg")));
    }

    #[test]
    fn media_title_comes_from_file_stem() {
        assert_eq!(
            title_from_path(Path::new("D:/Movies/Inception.mkv")),
            "Inception"
        );
    }

    #[test]
    fn item_type_uses_library_and_extension() {
        assert_eq!(
            item_type_for_file("movies", Path::new("movie.mkv")),
            "movie"
        );
        assert_eq!(item_type_for_file("music", Path::new("song.mp3")), "track");
        assert_eq!(item_type_for_file("mixed", Path::new("song.flac")), "track");
    }

    #[test]
    fn scan_started_hook_payload_exposes_public_boundary() {
        let event = scan_started_event("job-1", "library-1");

        assert_eq!(event.event_key, LIBRARY_SCAN_STARTED_EVENT);
        assert_eq!(event.aggregate_type, "library");
        assert_eq!(event.aggregate_id, "library-1");
        assert_eq!(event.payload["jobId"], "job-1");
        assert_eq!(event.payload["libraryId"], "library-1");
        assert_eq!(event.payload["status"], "running");
        assert!(event.payload.get("libraryInternalId").is_none());
        assert!(event.payload.get("paths").is_none());
    }

    #[test]
    fn scan_completed_hook_payload_includes_counts() {
        let summary = PartialScanSummary {
            scanned_files: 10,
            created_items: 3,
            updated_files: 2,
            metadata_refresh_jobs: 5,
            probe_jobs: 6,
            missing_items: 4,
            missing_mark_skipped: false,
            has_more: true,
            continuation_job_id: Some("job-2".to_owned()),
        };

        let event = scan_completed_event("job-1", "library-1", &summary);

        assert_eq!(event.event_key, LIBRARY_SCAN_COMPLETED_EVENT);
        assert_eq!(event.aggregate_type, "library");
        assert_eq!(event.aggregate_id, "library-1");
        assert_eq!(event.payload["jobId"], "job-1");
        assert_eq!(event.payload["libraryId"], "library-1");
        assert_eq!(event.payload["status"], "succeeded");
        assert_eq!(event.payload["scannedFiles"], 10);
        assert_eq!(event.payload["createdItems"], 3);
        assert_eq!(event.payload["updatedFiles"], 2);
        assert_eq!(event.payload["metadataRefreshJobs"], 5);
        assert_eq!(event.payload["probeJobs"], 6);
        assert_eq!(event.payload["missingItems"], 4);
        assert_eq!(event.payload["missingMarkSkipped"], false);
        assert_eq!(event.payload["hasMore"], true);
        assert_eq!(event.payload["continuationJobId"], "job-2");
    }

    #[test]
    fn scan_failed_hook_payload_exposes_public_failure_boundary() {
        let event = scan_failed_event("job-1", "library-1", "filesystem scan error");

        assert_eq!(event.event_key, LIBRARY_SCAN_FAILED_EVENT);
        assert_eq!(event.aggregate_type, "library");
        assert_eq!(event.aggregate_id, "library-1");
        assert_eq!(event.payload["jobId"], "job-1");
        assert_eq!(event.payload["libraryId"], "library-1");
        assert_eq!(event.payload["status"], "failed");
        assert_eq!(event.payload["error"], "filesystem scan error");
        assert!(event.payload.get("jobInternalId").is_none());
        assert!(event.payload.get("libraryInternalId").is_none());
    }

    #[test]
    fn scan_job_lease_policy_is_bounded_and_retryable() {
        assert_eq!(LIBRARY_SCAN_JOB_TYPE, "library.scan");
        assert_eq!(SCAN_JOB_LEASE_SECONDS, 900);
        assert_ne!(SCAN_JOB_LEASE_EXPIRED_RETRY, SCAN_JOB_LEASE_EXPIRED_FINAL);
        assert!(SCAN_JOB_LEASE_EXPIRED_RETRY.contains("retry"));
        assert!(SCAN_JOB_LEASE_EXPIRED_FINAL.contains("max attempts"));
    }

    #[test]
    fn scan_public_id_inputs_use_uuid_comparisons() {
        assert!(SCAN_CLAIM_JOB_SQL.contains("with requested_job as"));
        assert!(SCAN_CLAIM_JOB_SQL.contains("$1::uuid"));
        assert!(SCAN_CLAIM_JOB_SQL.contains("jobs.public_id = requested_job.public_id"));
        assert!(!SCAN_CLAIM_JOB_SQL.contains("public_id::text = $1"));

        assert!(SCAN_LOAD_LIBRARY_TARGET_SQL.contains("where public_id = case"));
        assert!(SCAN_LOAD_LIBRARY_TARGET_SQL.contains("$1::uuid"));
        assert!(!SCAN_LOAD_LIBRARY_TARGET_SQL.contains("public_id::text = $1"));
    }

    #[test]
    fn scan_job_request_parses_optional_cursor_payload() {
        let payload = json!({
            "libraryId": "library-1",
            "requestedByUserId": "user-1",
            "reason": "manual scan",
            "cursor": {
                "pathIndex": 2,
                "pendingPaths": ["D:/Media/Movies"],
                "unavailableRoots": 1
            },
            "scanId": "scan-1"
        });

        let request = ScanJobRequest::from_payload(&payload).unwrap();

        assert_eq!(request.library_id, "library-1");
        assert_eq!(request.requested_by_user_id.as_deref(), Some("user-1"));
        assert_eq!(request.reason.as_deref(), Some("manual scan"));
        assert_eq!(request.scan_id.as_deref(), Some("scan-1"));
        assert_eq!(request.effective_scan_id("job-1"), "scan-1");
        assert_eq!(
            request.cursor,
            Some(ScanCursor {
                path_index: 2,
                pending_paths: vec!["D:/Media/Movies".to_owned()],
                unavailable_roots: 1,
            })
        );
    }

    #[test]
    fn continuation_dedupe_key_is_stable_and_parent_scoped() {
        let cursor = ScanCursor {
            path_index: 1,
            pending_paths: vec!["D:/Media/Movies".to_owned()],
            unavailable_roots: 0,
        };

        let first = continuation_dedupe_key("job-1", &cursor).unwrap();
        let second = continuation_dedupe_key("job-1", &cursor).unwrap();
        let other_parent = continuation_dedupe_key("job-2", &cursor).unwrap();

        assert_eq!(first, second);
        assert_ne!(first, other_parent);
        assert!(first.starts_with("library.scan.continuation:job-1:"));
    }

    #[tokio::test]
    async fn discover_media_files_returns_cursor_for_next_batch() {
        let root = unique_test_dir("scan-cursor");
        let nested = root.join("Series");
        std::fs::create_dir_all(&nested).unwrap();
        std::fs::write(root.join("Movie.mkv"), b"movie").unwrap();
        std::fs::write(nested.join("Episode.mp4"), b"episode").unwrap();
        std::fs::write(root.join("cover.jpg"), b"ignored").unwrap();

        let target = LibraryPathTarget {
            id: 1,
            path: root.clone(),
        };
        let first = discover_media_files(vec![target.clone()], None, 1)
            .await
            .unwrap();
        assert_eq!(first.files.len(), 1);
        assert!(first.next_cursor.is_some());

        let second = discover_media_files(vec![target], first.next_cursor, 10)
            .await
            .unwrap();
        let mut names = second
            .files
            .iter()
            .filter_map(|file| file.path.file_name().and_then(|name| name.to_str()))
            .map(str::to_owned)
            .collect::<Vec<_>>();
        names.sort();

        assert_eq!(names, ["Episode.mp4"]);
        assert!(second.next_cursor.is_none());

        std::fs::remove_dir_all(root).unwrap();
    }

    #[tokio::test]
    async fn discover_media_files_tracks_unavailable_roots() {
        let missing_root = unique_test_dir("scan-missing-root");
        let target = LibraryPathTarget {
            id: 1,
            path: missing_root,
        };

        let page = discover_media_files(vec![target], None, 10).await.unwrap();

        assert!(page.files.is_empty());
        assert!(page.next_cursor.is_none());
        assert_eq!(page.unavailable_roots, 1);
    }

    #[test]
    fn scan_traversal_rejects_cursor_path_index_past_roots() {
        let err = ScanTraversal::new(
            vec![LibraryPathTarget {
                id: 1,
                path: PathBuf::from("D:/Media"),
            }],
            Some(ScanCursor {
                path_index: 2,
                pending_paths: Vec::new(),
                unavailable_roots: 0,
            }),
        )
        .unwrap_err();

        assert!(matches!(err, ScanError::InvalidCursor(_)));
    }

    #[test]
    fn unchanged_file_observation_requires_same_size_mtime_and_strm_target() {
        let existing = ExistingMediaFileObservation {
            media_item_id: 1,
            file_size: Some(1024),
            modified_at_epoch_ms: Some(1_700_000_000_000),
            is_strm: true,
            strm_target: Some("http://192.168.1.10/movie.mkv".to_owned()),
        };
        let same = ScanFile {
            path: PathBuf::from("D:/Media/movie.strm"),
            file_size: Some(1024),
            modified_at_epoch_ms: Some(1_700_000_000_000),
            is_strm: true,
            strm_target: Some("http://192.168.1.10/movie.mkv".to_owned()),
        };
        let changed_target = ScanFile {
            strm_target: Some("http://192.168.1.11/movie.mkv".to_owned()),
            ..same.clone()
        };
        let changed_mtime = ScanFile {
            modified_at_epoch_ms: Some(1_700_000_000_001),
            ..same.clone()
        };

        assert!(file_observation_unchanged(&existing, &same));
        assert!(!file_observation_unchanged(&existing, &changed_target));
        assert!(!file_observation_unchanged(&existing, &changed_mtime));
    }

    #[test]
    fn system_time_epoch_millis_uses_unix_epoch_boundary() {
        assert_eq!(system_time_epoch_millis(UNIX_EPOCH), Some(0));
        assert_eq!(
            system_time_epoch_millis(UNIX_EPOCH + std::time::Duration::from_millis(42)),
            Some(42)
        );
    }

    fn unique_test_dir(prefix: &str) -> PathBuf {
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        std::env::temp_dir().join(format!("{prefix}-{}-{nanos}", std::process::id()))
    }
}
