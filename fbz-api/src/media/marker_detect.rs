//! 片头 / 片尾（intro / credits）探测作业与 worker。
//!
//! 职责划分（架构 §6.7）：
//! - Core：入队 `media.marker.detect`、用本机 FFmpeg/ffprobe 收集章节与头尾
//!   静音/黑场摘要（`probeSummary`，**不含**裸存储路径）、同步调用订阅插件、
//!   校验并写入 `media_markers`。
//! - 插件：根据 `probeSummary` 做章节名 / 静音启发式，回传候选区间。
//!
//! 无订阅插件时，core 使用与官方包相同的保守启发式作为回退，保证作业可验收。

use std::{
    path::{Path, PathBuf},
    process::Stdio,
    time::Duration,
};

use regex::Regex;
use serde::Deserialize;
use serde_json::{Value, json};
use sqlx::Row;
use tokio::{
    process::Command,
    sync::broadcast,
    task::JoinHandle,
    time::{MissedTickBehavior, interval},
};
use tracing::{info, warn};

use crate::{
    config::{MarkerDetectWorkerConfig, PluginConfig},
    db::DbPool,
    jobs::{ExpiredJobMessages, expire_stale_running_jobs, mark_job_failed},
    media::tools::MediaToolDiagnostics,
    plugins::invoke::{PluginSyncInvoker, SyncAuditMode},
};

/// 片头片尾探测 job 类型（与 draft / 架构 §6.7 对齐）。
pub const MEDIA_MARKER_DETECT_JOB_TYPE: &str = "media.marker.detect";
pub const MEDIA_MARKER_DETECT_HOOK: &str = "media.marker.detect";

const MARKER_WORKER_ID: &str = "fbz-api-marker-detect";
const MARKER_JOB_LEASE_SECONDS: i64 = 15 * 60;
const MARKER_JOB_LEASE_EXPIRED_RETRY: &str = "media marker detect lease expired; will retry";
const MARKER_JOB_LEASE_EXPIRED_FINAL: &str =
    "media marker detect lease expired; max attempts reached";

/// Emby / FBZ ticks：每秒 10_000_000。
const TICKS_PER_SECOND: i64 = 10_000_000;

/// 头/尾分析窗口（秒）——保守上限，避免全片扫。
const ANALYSIS_WINDOW_SECS: f64 = 180.0;
/// 片尾窗口：从时长末尾往前取。
const TAIL_WINDOW_SECS: f64 = 240.0;
/// ffmpeg 单次分析超时。
const FFMPEG_ANALYSIS_TIMEOUT: Duration = Duration::from_secs(90);
const FFPROBE_TIMEOUT: Duration = Duration::from_secs(30);

const MAX_MARKERS_PER_JOB: usize = 32;
const CORE_FALLBACK_DETECTOR_VERSION: &str = "core-heuristic-0.1.0";
const CORE_FALLBACK_SOURCE: &str = "core:marker-detect";

const SUPPORTED_MARKER_TYPES: &[&str] = &[
    "intro_start",
    "intro_end",
    "credits_start",
    "credits_end",
    "commercial",
    "chapter",
];

const MARKER_CLAIM_JOB_SQL: &str = r#"
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

#[derive(Clone, Debug, PartialEq)]
pub struct MarkerDetectJobOutcome {
    pub job_id: String,
    pub media_item_id: String,
    pub marker_count: usize,
    pub detector_version: String,
    pub status: String,
}

#[derive(Clone, Debug)]
struct ClaimedMarkerJob {
    id: i64,
    public_id: String,
    payload: Value,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct MarkerJobRequest {
    media_item_public_id: String,
    force: bool,
}

#[derive(Clone, Debug)]
struct MarkerTarget {
    media_item_internal_id: i64,
    media_item_public_id: String,
    media_file_id: i64,
    path: String,
    duration_ticks: Option<i64>,
    content_fingerprint: Option<String>,
    is_strm: bool,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct TimeInterval {
    pub start_ms: i64,
    pub end_ms: Option<i64>,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct ChapterHint {
    pub title: String,
    pub start_ms: i64,
    pub end_ms: Option<i64>,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct ProbeSummary {
    pub chapters: Vec<ChapterHint>,
    pub silence: Vec<TimeInterval>,
    pub black: Vec<TimeInterval>,
    pub analysis_window_ms: i64,
    pub tail_window_ms: i64,
    pub ffmpeg_available: bool,
    pub notes: Vec<String>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct DetectedMarker {
    pub marker_type: String,
    pub start_ticks: i64,
    pub end_ticks: Option<i64>,
    pub confidence: Option<f64>,
    pub title: Option<String>,
}

#[derive(Debug)]
pub enum MarkerDetectError {
    JobNotFound,
    MissingMediaItemId,
    InvalidMediaItemId(String),
    MediaItemNotFound(String),
    NoMediaFile(String),
    Database(sqlx::Error),
    Io(std::io::Error),
    Ffmpeg(String),
    Internal(String),
}

impl std::fmt::Display for MarkerDetectError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::JobNotFound => f.write_str("marker detect job not found or not runnable"),
            Self::MissingMediaItemId => {
                f.write_str("marker detect job payload is missing mediaItemId")
            }
            Self::InvalidMediaItemId(value) => {
                write!(f, "invalid marker detect mediaItemId `{value}`")
            }
            Self::MediaItemNotFound(id) => write!(f, "media item `{id}` not found"),
            Self::NoMediaFile(id) => write!(f, "media item `{id}` has no local media file"),
            Self::Database(err) => write!(f, "database error: {err}"),
            Self::Io(err) => write!(f, "io error: {err}"),
            Self::Ffmpeg(message) => write!(f, "ffmpeg/ffprobe error: {message}"),
            Self::Internal(message) => f.write_str(message),
        }
    }
}

impl std::error::Error for MarkerDetectError {}

impl From<sqlx::Error> for MarkerDetectError {
    fn from(value: sqlx::Error) -> Self {
        Self::Database(value)
    }
}

/// 启动片头片尾探测后台 worker。
pub fn spawn_marker_detect_worker(
    pool: DbPool,
    config: MarkerDetectWorkerConfig,
    media_tools: MediaToolDiagnostics,
    plugins: PluginConfig,
    mut shutdown: broadcast::Receiver<()>,
) -> JoinHandle<()> {
    tokio::spawn(async move {
        let invoker = PluginSyncInvoker::new(pool.clone(), plugins);
        let service = MarkerDetectService::new(pool, media_tools, invoker, config.max_concurrent);
        let mut tick = interval(Duration::from_secs(config.interval_seconds));
        tick.set_missed_tick_behavior(MissedTickBehavior::Delay);

        info!(
            interval_seconds = config.interval_seconds,
            max_concurrent = config.max_concurrent,
            "marker detect worker started"
        );

        loop {
            tokio::select! {
                result = shutdown.recv() => {
                    match result {
                        Ok(()) | Err(broadcast::error::RecvError::Closed) => {
                            info!("marker detect worker shutdown received");
                            break;
                        }
                        Err(broadcast::error::RecvError::Lagged(skipped)) => {
                            warn!(skipped, "marker detect worker shutdown receiver lagged");
                            break;
                        }
                    }
                }
                _ = tick.tick() => {
                    run_available_jobs(&service).await;
                }
            }
        }

        info!("marker detect worker stopped");
    })
}

async fn run_available_jobs(service: &MarkerDetectService) {
    if let Ok(mut tx) = service.pool.begin().await {
        let _ = expire_stale_running_jobs(
            &mut tx,
            MEDIA_MARKER_DETECT_JOB_TYPE,
            ExpiredJobMessages {
                retry: MARKER_JOB_LEASE_EXPIRED_RETRY,
                final_failure: MARKER_JOB_LEASE_EXPIRED_FINAL,
            },
        )
        .await;
        let _ = tx.commit().await;
    }

    let mut ran = 0u32;
    let cap = service.max_concurrent.max(1);
    while ran < cap {
        match service.run_next_job().await {
            Ok(Some(summary)) => {
                info!(
                    job_id = %summary.job_id,
                    media_item_id = %summary.media_item_id,
                    marker_count = summary.marker_count,
                    detector_version = %summary.detector_version,
                    status = %summary.status,
                    "media marker detect job completed by background worker"
                );
                ran += 1;
            }
            Ok(None) => break,
            Err(err) => {
                warn!(error = %err, "marker detect worker failed to run job");
                break;
            }
        }
    }
}

#[derive(Clone)]
pub struct MarkerDetectService {
    pool: DbPool,
    media_tools: MediaToolDiagnostics,
    invoker: PluginSyncInvoker,
    max_concurrent: u32,
    worker_id: String,
}

impl MarkerDetectService {
    pub fn new(
        pool: DbPool,
        media_tools: MediaToolDiagnostics,
        invoker: PluginSyncInvoker,
        max_concurrent: u32,
    ) -> Self {
        Self {
            pool,
            media_tools,
            invoker,
            max_concurrent,
            worker_id: MARKER_WORKER_ID.to_owned(),
        }
    }

    pub async fn run_next_job(&self) -> Result<Option<MarkerDetectJobOutcome>, MarkerDetectError> {
        let Some(job) = self.claim_job(None).await? else {
            return Ok(None);
        };
        self.run_claimed_job(job).await.map(Some)
    }

    async fn claim_job(
        &self,
        public_id: Option<&str>,
    ) -> Result<Option<ClaimedMarkerJob>, MarkerDetectError> {
        let row = sqlx::query(MARKER_CLAIM_JOB_SQL)
            .bind(public_id)
            .bind(MEDIA_MARKER_DETECT_JOB_TYPE)
            .bind(&self.worker_id)
            .bind(MARKER_JOB_LEASE_SECONDS)
            .fetch_optional(&self.pool)
            .await?;
        Ok(row.map(|row| ClaimedMarkerJob {
            id: row.get("id"),
            public_id: row.get("public_id"),
            payload: row.get("payload"),
        }))
    }

    async fn run_claimed_job(
        &self,
        job: ClaimedMarkerJob,
    ) -> Result<MarkerDetectJobOutcome, MarkerDetectError> {
        let request = MarkerJobRequest::from_payload(&job.payload)?;
        let run_id = self.start_job_run(job.id).await?;
        self.record_job_event(
            job.id,
            Some(run_id),
            "media.marker.detect.started",
            "info",
            "media marker detect started",
            json!({ "mediaItemId": request.media_item_public_id }),
        )
        .await?;

        match self.detect_for_item(&request).await {
            Ok(mut outcome) => {
                outcome.job_id = job.public_id.clone();
                self.finish_job_success(job.id, run_id, &outcome).await?;
                Ok(outcome)
            }
            Err(err) => {
                let message = err.to_string();
                if let Err(event_err) = self
                    .record_job_event(
                        job.id,
                        Some(run_id),
                        "media.marker.detect.failed",
                        "error",
                        &message,
                        json!({ "mediaItemId": request.media_item_public_id }),
                    )
                    .await
                {
                    warn!(error = %event_err, "failed to record marker detect failure event");
                }
                if let Err(fail_err) = self
                    .finish_job_failure(&job.public_id, job.id, run_id, &message)
                    .await
                {
                    warn!(error = %fail_err, "failed to mark marker detect job failed");
                }
                Err(err)
            }
        }
    }

    async fn detect_for_item(
        &self,
        request: &MarkerJobRequest,
    ) -> Result<MarkerDetectJobOutcome, MarkerDetectError> {
        let target = self.load_target(&request.media_item_public_id).await?;

        if !request.force
            && self
                .has_recent_plugin_markers(target.media_item_internal_id)
                .await?
        {
            return Ok(MarkerDetectJobOutcome {
                job_id: String::new(),
                media_item_id: target.media_item_public_id,
                marker_count: 0,
                detector_version: "skipped".to_owned(),
                status: "skipped_existing".to_owned(),
            });
        }

        let duration_ms = target
            .duration_ticks
            .map(|ticks| ticks / (TICKS_PER_SECOND / 1000))
            .unwrap_or(0);

        let probe_summary = if target.is_strm || target.path.starts_with("fbz-storage://") {
            ProbeSummary {
                notes: vec!["skipped ffmpeg: strm or storage provider path".to_owned()],
                ..ProbeSummary::default()
            }
        } else {
            gather_probe_summary(
                &self.media_tools,
                Path::new(&target.path),
                duration_ms,
            )
            .await
            .unwrap_or_else(|err| ProbeSummary {
                notes: vec![format!("probe summary degraded: {err}")],
                ffmpeg_available: false,
                ..ProbeSummary::default()
            })
        };

        let request_payload = json!({
            "mediaItemId": target.media_item_public_id,
            "mediaFileId": target.media_file_id.to_string(),
            "durationMs": duration_ms,
            "contentFingerprint": target.content_fingerprint,
            "probeSummary": probe_summary_to_json(&probe_summary),
            "locale": "zh-CN",
        });

        let (markers, detector_version, source) = self
            .invoke_detectors(&request_payload, &probe_summary, duration_ms)
            .await;

        let written = self
            .replace_markers(
                target.media_item_internal_id,
                &source,
                &markers,
            )
            .await?;

        Ok(MarkerDetectJobOutcome {
            job_id: String::new(),
            media_item_id: target.media_item_public_id,
            marker_count: written,
            detector_version,
            status: "completed".to_owned(),
        })
    }

    async fn invoke_detectors(
        &self,
        payload: &Value,
        probe_summary: &ProbeSummary,
        duration_ms: i64,
    ) -> (Vec<DetectedMarker>, String, String) {
        let outcomes = match self
            .invoker
            .invoke_all(MEDIA_MARKER_DETECT_HOOK, payload, SyncAuditMode::FailuresOnly)
            .await
        {
            Ok(outcomes) => outcomes,
            Err(err) => {
                warn!(error = %err, "marker detect plugin discovery failed; using core fallback");
                let markers = heuristic_detect(probe_summary, duration_ms);
                return (
                    markers,
                    CORE_FALLBACK_DETECTOR_VERSION.to_owned(),
                    CORE_FALLBACK_SOURCE.to_owned(),
                );
            }
        };

        if outcomes.is_empty() {
            let markers = heuristic_detect(probe_summary, duration_ms);
            return (
                markers,
                CORE_FALLBACK_DETECTOR_VERSION.to_owned(),
                CORE_FALLBACK_SOURCE.to_owned(),
            );
        }

        let mut merged = Vec::new();
        let mut version = CORE_FALLBACK_DETECTOR_VERSION.to_owned();
        let mut source = CORE_FALLBACK_SOURCE.to_owned();

        for outcome in outcomes {
            match outcome.result {
                Ok(value) => {
                    if let Some((markers, detector_version)) = parse_detect_response(&value) {
                        if !markers.is_empty() {
                            version = detector_version;
                            source = format!("plugin:{}", outcome.plugin_id);
                            merged = markers;
                            break;
                        }
                        version = detector_version;
                        source = format!("plugin:{}", outcome.plugin_id);
                    }
                }
                Err(err) => {
                    warn!(
                        plugin_id = %outcome.plugin_id,
                        error = %err,
                        "marker detect plugin invocation failed"
                    );
                }
            }
        }

        if merged.is_empty() && source.starts_with("plugin:") {
            // 插件已应答但无候选：尊重空结果，不强制回退覆盖。
            return (merged, version, source);
        }
        if merged.is_empty() {
            merged = heuristic_detect(probe_summary, duration_ms);
            version = CORE_FALLBACK_DETECTOR_VERSION.to_owned();
            source = CORE_FALLBACK_SOURCE.to_owned();
        }

        (merged, version, source)
    }

    async fn load_target(
        &self,
        media_item_public_id: &str,
    ) -> Result<MarkerTarget, MarkerDetectError> {
        let row = sqlx::query(
            r#"
            select
                mi.id as media_item_internal_id,
                mi.public_id::text as media_item_public_id,
                mf.id as media_file_id,
                mf.path,
                coalesce(mf.duration_ticks, mi.runtime_ticks) as duration_ticks,
                encode(mf.content_hash, 'hex') as content_fingerprint,
                mf.is_strm
            from media_items mi
            join media_files mf on mf.media_item_id = mi.id
            where mi.public_id = case
                when $1::text ~* '^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$'
                then $1::uuid
                else null::uuid
            end
              and mi.is_deleted = false
            order by mf.is_primary desc nulls last, mf.id asc
            limit 1
            "#,
        )
        .bind(media_item_public_id)
        .fetch_optional(&self.pool)
        .await?;

        let Some(row) = row else {
            // Distinguish missing item vs missing file.
            let exists: Option<i64> = sqlx::query_scalar(
                r#"
                select mi.id
                from media_items mi
                where mi.public_id = case
                    when $1::text ~* '^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$'
                    then $1::uuid
                    else null::uuid
                end
                  and mi.is_deleted = false
                limit 1
                "#,
            )
            .bind(media_item_public_id)
            .fetch_optional(&self.pool)
            .await?;
            return if exists.is_some() {
                Err(MarkerDetectError::NoMediaFile(
                    media_item_public_id.to_owned(),
                ))
            } else {
                Err(MarkerDetectError::MediaItemNotFound(
                    media_item_public_id.to_owned(),
                ))
            };
        };

        Ok(MarkerTarget {
            media_item_internal_id: row.try_get("media_item_internal_id")?,
            media_item_public_id: row.try_get("media_item_public_id")?,
            media_file_id: row.try_get("media_file_id")?,
            path: row.try_get("path")?,
            duration_ticks: row.try_get("duration_ticks")?,
            content_fingerprint: row.try_get("content_fingerprint")?,
            is_strm: row.try_get("is_strm")?,
        })
    }

    async fn has_recent_plugin_markers(
        &self,
        media_item_internal_id: i64,
    ) -> Result<bool, MarkerDetectError> {
        let count: i64 = sqlx::query_scalar(
            r#"
            select count(*)::bigint
            from media_markers
            where media_item_id = $1
              and (
                    source like 'plugin:%'
                 or source = $2
              )
              and marker_type in ('intro_start', 'intro_end', 'credits_start', 'credits_end')
            "#,
        )
        .bind(media_item_internal_id)
        .bind(CORE_FALLBACK_SOURCE)
        .fetch_one(&self.pool)
        .await?;
        Ok(count > 0)
    }

    async fn replace_markers(
        &self,
        media_item_internal_id: i64,
        source: &str,
        markers: &[DetectedMarker],
    ) -> Result<usize, MarkerDetectError> {
        let mut tx = self.pool.begin().await?;
        sqlx::query(
            r#"
            delete from media_markers
            where media_item_id = $1
              and source = $2
            "#,
        )
        .bind(media_item_internal_id)
        .bind(source)
        .execute(&mut *tx)
        .await?;

        let mut written = 0usize;
        for marker in markers.iter().take(MAX_MARKERS_PER_JOB) {
            if !SUPPORTED_MARKER_TYPES.contains(&marker.marker_type.as_str()) {
                continue;
            }
            if marker.start_ticks < 0 {
                continue;
            }
            if let Some(end) = marker.end_ticks {
                if end < marker.start_ticks {
                    continue;
                }
            }
            let confidence = marker.confidence.filter(|value| (0.0..=1.0).contains(value));
            sqlx::query(
                r#"
                insert into media_markers (
                    media_item_id,
                    marker_type,
                    start_ticks,
                    end_ticks,
                    source,
                    confidence
                )
                values ($1, $2, $3, $4, $5, $6)
                on conflict (media_item_id, marker_type, start_ticks, source) do update
                set end_ticks = excluded.end_ticks,
                    confidence = excluded.confidence
                "#,
            )
            .bind(media_item_internal_id)
            .bind(&marker.marker_type)
            .bind(marker.start_ticks)
            .bind(marker.end_ticks)
            .bind(source)
            .bind(confidence)
            .execute(&mut *tx)
            .await?;
            written += 1;
        }

        tx.commit().await?;
        Ok(written)
    }

    async fn start_job_run(&self, job_id: i64) -> Result<i64, MarkerDetectError> {
        let run_id: i64 = sqlx::query_scalar(
            r#"
            insert into job_runs (job_id, worker_id, status)
            values ($1, $2, 'running')
            returning id
            "#,
        )
        .bind(job_id)
        .bind(&self.worker_id)
        .fetch_one(&self.pool)
        .await?;
        Ok(run_id)
    }

    async fn finish_job_success(
        &self,
        job_id: i64,
        run_id: i64,
        outcome: &MarkerDetectJobOutcome,
    ) -> Result<(), MarkerDetectError> {
        let metrics = json!({
            "mediaItemId": outcome.media_item_id,
            "markerCount": outcome.marker_count,
            "detectorVersion": outcome.detector_version,
            "status": outcome.status,
        });
        let mut tx = self.pool.begin().await?;
        sqlx::query(
            r#"
            update job_runs
            set status = 'succeeded', finished_at = now(), metrics = $2
            where id = $1
            "#,
        )
        .bind(run_id)
        .bind(&metrics)
        .execute(&mut *tx)
        .await?;
        sqlx::query(
            r#"
            update jobs
            set status = 'succeeded',
                locked_by = null,
                locked_until = null,
                last_error = null,
                updated_at = now(),
                finished_at = now()
            where id = $1
            "#,
        )
        .bind(job_id)
        .execute(&mut *tx)
        .await?;
        tx.commit().await?;
        Ok(())
    }

    async fn finish_job_failure(
        &self,
        job_public_id: &str,
        job_id: i64,
        run_id: i64,
        message: &str,
    ) -> Result<(), MarkerDetectError> {
        let mut tx = self.pool.begin().await?;
        sqlx::query(
            r#"
            update job_runs
            set status = 'failed', finished_at = now(), error_message = $2
            where id = $1
            "#,
        )
        .bind(run_id)
        .bind(message)
        .execute(&mut *tx)
        .await?;
        mark_job_failed(
            &mut tx,
            MEDIA_MARKER_DETECT_JOB_TYPE,
            job_public_id,
            job_id,
            message,
        )
        .await?;
        tx.commit().await?;
        Ok(())
    }

    async fn record_job_event(
        &self,
        job_id: i64,
        run_id: Option<i64>,
        event_type: &str,
        level: &str,
        message: &str,
        payload: Value,
    ) -> Result<(), MarkerDetectError> {
        sqlx::query(
            r#"
            insert into job_events (job_id, job_run_id, event_type, event_level, message, payload)
            values ($1, $2, $3, $4, $5, $6)
            "#,
        )
        .bind(job_id)
        .bind(run_id)
        .bind(event_type)
        .bind(level)
        .bind(message)
        .bind(payload)
        .execute(&self.pool)
        .await?;
        Ok(())
    }
}

impl MarkerJobRequest {
    fn from_payload(payload: &Value) -> Result<Self, MarkerDetectError> {
        let media_item_public_id = payload
            .get("mediaItemId")
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .ok_or(MarkerDetectError::MissingMediaItemId)?
            .to_owned();
        if !is_uuid_like(&media_item_public_id) {
            return Err(MarkerDetectError::InvalidMediaItemId(
                media_item_public_id,
            ));
        }
        let force = payload
            .get("force")
            .and_then(Value::as_bool)
            .unwrap_or(false);
        Ok(Self {
            media_item_public_id,
            force,
        })
    }
}

fn is_uuid_like(value: &str) -> bool {
    let re = Regex::new(
        r"(?i)^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$",
    )
    .expect("uuid regex");
    re.is_match(value)
}

fn ms_to_ticks(ms: i64) -> i64 {
    ms.saturating_mul(TICKS_PER_SECOND / 1000)
}

fn probe_summary_to_json(summary: &ProbeSummary) -> Value {
    json!({
        "chapters": summary.chapters.iter().map(|chapter| json!({
            "title": chapter.title,
            "startMs": chapter.start_ms,
            "endMs": chapter.end_ms,
        })).collect::<Vec<_>>(),
        "silence": summary.silence.iter().map(|interval| json!({
            "startMs": interval.start_ms,
            "endMs": interval.end_ms,
        })).collect::<Vec<_>>(),
        "black": summary.black.iter().map(|interval| json!({
            "startMs": interval.start_ms,
            "endMs": interval.end_ms,
        })).collect::<Vec<_>>(),
        "analysisWindowMs": summary.analysis_window_ms,
        "tailWindowMs": summary.tail_window_ms,
        "ffmpegAvailable": summary.ffmpeg_available,
        "notes": summary.notes,
    })
}

/// 从插件响应解析 markers（兼容 SDK `{ok,result}` 信封）。
pub fn parse_detect_response(value: &Value) -> Option<(Vec<DetectedMarker>, String)> {
    let body = value
        .get("result")
        .filter(|_result| value.get("ok").and_then(Value::as_bool) == Some(true))
        .unwrap_or(value);

    #[derive(Debug, Deserialize)]
    #[serde(rename_all = "camelCase")]
    struct Envelope {
        #[serde(default)]
        markers: Vec<MarkerDto>,
        #[serde(default)]
        detector_version: Option<String>,
        #[serde(default)]
        skipped: Option<bool>,
    }

    #[derive(Debug, Deserialize)]
    #[serde(rename_all = "camelCase")]
    struct MarkerDto {
        marker_type: String,
        start_ticks: i64,
        #[serde(default)]
        end_ticks: Option<i64>,
        #[serde(default)]
        confidence: Option<f64>,
        #[serde(default)]
        title: Option<String>,
    }

    let parsed: Envelope = serde_json::from_value(body.clone()).ok()?;
    let version = parsed
        .detector_version
        .unwrap_or_else(|| "unknown".to_owned());
    if parsed.skipped == Some(true) && parsed.markers.is_empty() {
        return Some((Vec::new(), version));
    }

    let mut markers = Vec::new();
    let mut seen = std::collections::BTreeSet::new();
    for marker in parsed.markers {
        let marker_type = marker.marker_type.trim().to_ascii_lowercase();
        if !SUPPORTED_MARKER_TYPES.contains(&marker_type.as_str()) {
            continue;
        }
        if marker.start_ticks < 0 {
            continue;
        }
        if let Some(end) = marker.end_ticks {
            if end < marker.start_ticks {
                continue;
            }
        }
        if let Some(confidence) = marker.confidence {
            if !(0.0..=1.0).contains(&confidence) {
                continue;
            }
        }
        let key = (marker_type.clone(), marker.start_ticks);
        if !seen.insert(key) {
            continue;
        }
        markers.push(DetectedMarker {
            marker_type,
            start_ticks: marker.start_ticks,
            end_ticks: marker.end_ticks,
            confidence: marker.confidence,
            title: marker.title,
        });
        if markers.len() >= MAX_MARKERS_PER_JOB {
            break;
        }
    }
    Some((markers, version))
}

/// 章节名 + 头尾静音/黑场的保守启发式（core 回退与官方插件共用语义）。
pub fn heuristic_detect(summary: &ProbeSummary, duration_ms: i64) -> Vec<DetectedMarker> {
    let mut markers = Vec::new();

    // 1) 章节标题关键词 → Intro / Credits
    for chapter in &summary.chapters {
        let title = chapter.title.to_ascii_lowercase();
        let kind = classify_chapter_title(&title);
        if kind == ChapterKind::None {
            // 仍可作为普通 chapter 写入（低置信）
            markers.push(DetectedMarker {
                marker_type: "chapter".to_owned(),
                start_ticks: ms_to_ticks(chapter.start_ms),
                end_ticks: chapter.end_ms.map(ms_to_ticks),
                confidence: Some(0.55),
                title: Some(chapter.title.clone()),
            });
            continue;
        }

        let start_ticks = ms_to_ticks(chapter.start_ms);
        let end_ticks = chapter
            .end_ms
            .map(ms_to_ticks)
            .or_else(|| {
                // 无结束时给一个短默认窗（90s），避免只有 start。
                Some(start_ticks.saturating_add(90 * TICKS_PER_SECOND))
            });

        match kind {
            ChapterKind::Intro => {
                markers.push(DetectedMarker {
                    marker_type: "intro_start".to_owned(),
                    start_ticks,
                    end_ticks: None,
                    confidence: Some(0.85),
                    title: Some(chapter.title.clone()),
                });
                if let Some(end) = end_ticks {
                    markers.push(DetectedMarker {
                        marker_type: "intro_end".to_owned(),
                        start_ticks: end,
                        end_ticks: None,
                        confidence: Some(0.85),
                        title: Some(chapter.title.clone()),
                    });
                }
            }
            ChapterKind::Credits | ChapterKind::Outro => {
                markers.push(DetectedMarker {
                    marker_type: "credits_start".to_owned(),
                    start_ticks,
                    end_ticks: None,
                    confidence: Some(0.85),
                    title: Some(chapter.title.clone()),
                });
                if let Some(end) = end_ticks {
                    markers.push(DetectedMarker {
                        marker_type: "credits_end".to_owned(),
                        start_ticks: end,
                        end_ticks: None,
                        confidence: Some(0.8),
                        title: Some(chapter.title.clone()),
                    });
                }
            }
            ChapterKind::None => {}
        }
    }

    // 2) 无章节命中 Intro 时：用片头窗口内最长静音/黑场交汇估 intro
    let has_intro = markers
        .iter()
        .any(|marker| marker.marker_type == "intro_start");
    if !has_intro {
        if let Some((start_ms, end_ms)) =
            guess_intro_from_silence_or_black(&summary.silence, &summary.black, summary.analysis_window_ms)
        {
            markers.push(DetectedMarker {
                marker_type: "intro_start".to_owned(),
                start_ticks: ms_to_ticks(start_ms),
                end_ticks: None,
                confidence: Some(0.55),
                title: None,
            });
            markers.push(DetectedMarker {
                marker_type: "intro_end".to_owned(),
                start_ticks: ms_to_ticks(end_ms),
                end_ticks: None,
                confidence: Some(0.55),
                title: None,
            });
        }
    }

    // 3) 无章节命中 Credits 时：用片尾窗口静音/黑场估 credits
    let has_credits = markers
        .iter()
        .any(|marker| marker.marker_type == "credits_start");
    if !has_credits && duration_ms > 0 {
        let tail_origin = duration_ms.saturating_sub(summary.tail_window_ms.max(1));
        if let Some((start_ms, end_ms)) = guess_credits_from_silence_or_black(
            &summary.silence,
            &summary.black,
            tail_origin,
            duration_ms,
        ) {
            markers.push(DetectedMarker {
                marker_type: "credits_start".to_owned(),
                start_ticks: ms_to_ticks(start_ms),
                end_ticks: None,
                confidence: Some(0.5),
                title: None,
            });
            markers.push(DetectedMarker {
                marker_type: "credits_end".to_owned(),
                start_ticks: ms_to_ticks(end_ms.min(duration_ms)),
                end_ticks: None,
                confidence: Some(0.5),
                title: None,
            });
        }
    }

    dedupe_markers(markers)
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ChapterKind {
    Intro,
    Outro,
    Credits,
    None,
}

fn classify_chapter_title(title: &str) -> ChapterKind {
    let t = title.trim().to_ascii_lowercase();
    if t.is_empty() {
        return ChapterKind::None;
    }
    // 中英文常见片头/片尾命名（保守，避免误伤 "Introduction" 长解说——仍匹配 intro）。
    const INTRO: &[&str] = &[
        "intro",
        "opening",
        "op",
        "cold open",
        "title sequence",
        "片头",
        "開場",
        "开场",
        "op曲",
    ];
    const CREDITS: &[&str] = &[
        "credit",
        "credits",
        "ending credits",
        "end credits",
        "closing credits",
        "字幕",
        "演职员",
        "演職人員",
        "制作人员",
    ];
    const OUTRO: &[&str] = &[
        "outro",
        "ending",
        "ed",
        "epilogue",
        "片尾",
        "結尾",
        "结尾",
        "ed曲",
    ];

    if INTRO.iter().any(|key| t.contains(key)) {
        return ChapterKind::Intro;
    }
    if CREDITS.iter().any(|key| t.contains(key)) {
        return ChapterKind::Credits;
    }
    if OUTRO.iter().any(|key| t.contains(key)) {
        return ChapterKind::Outro;
    }
    ChapterKind::None
}

fn guess_intro_from_silence_or_black(
    silence: &[TimeInterval],
    black: &[TimeInterval],
    window_ms: i64,
) -> Option<(i64, i64)> {
    let window_ms = window_ms.max(1);
    // 优先：片头窗口内时长 ≥ 1.5s 的静音段，取其结束作为 intro_end。
    let candidate = silence
        .iter()
        .chain(black.iter())
        .filter(|interval| interval.start_ms < window_ms)
        .filter_map(|interval| {
            let end = interval.end_ms.unwrap_or(interval.start_ms);
            let duration = end.saturating_sub(interval.start_ms);
            if duration < 1500 || end > window_ms {
                return None;
            }
            // intro 通常在开场后不久结束；要求 end 在 5s..window
            if end < 5_000 {
                return None;
            }
            Some((0_i64, end, duration))
        })
        .max_by_key(|(_, _, duration)| *duration)?;
    Some((candidate.0, candidate.1))
}

fn guess_credits_from_silence_or_black(
    silence: &[TimeInterval],
    black: &[TimeInterval],
    tail_origin_ms: i64,
    duration_ms: i64,
) -> Option<(i64, i64)> {
    let candidate = silence
        .iter()
        .chain(black.iter())
        .filter(|interval| interval.start_ms >= tail_origin_ms)
        .filter_map(|interval| {
            let end = interval
                .end_ms
                .unwrap_or(duration_ms)
                .min(duration_ms);
            let start = interval.start_ms.max(tail_origin_ms);
            let duration = end.saturating_sub(start);
            if duration < 2_000 {
                return None;
            }
            Some((start, end, duration))
        })
        .max_by_key(|(_, _, duration)| *duration)?;
    Some((candidate.0, candidate.1))
}

fn dedupe_markers(markers: Vec<DetectedMarker>) -> Vec<DetectedMarker> {
    let mut seen = std::collections::BTreeSet::new();
    let mut out = Vec::new();
    for marker in markers {
        let key = (marker.marker_type.clone(), marker.start_ticks);
        if seen.insert(key) {
            out.push(marker);
        }
        if out.len() >= MAX_MARKERS_PER_JOB {
            break;
        }
    }
    out
}

async fn gather_probe_summary(
    tools: &MediaToolDiagnostics,
    path: &Path,
    duration_ms: i64,
) -> Result<ProbeSummary, MarkerDetectError> {
    let mut summary = ProbeSummary {
        analysis_window_ms: (ANALYSIS_WINDOW_SECS * 1000.0) as i64,
        tail_window_ms: (TAIL_WINDOW_SECS * 1000.0) as i64,
        ffmpeg_available: true,
        ..ProbeSummary::default()
    };

    match run_ffprobe_chapters(&tools.ffprobe.path, path).await {
        Ok(chapters) => summary.chapters = chapters,
        Err(err) => summary.notes.push(format!("ffprobe chapters: {err}")),
    }

    let head_limit = ANALYSIS_WINDOW_SECS.min(duration_ms.max(1) as f64 / 1000.0);
    match run_ffmpeg_silence_black(&tools.ffmpeg.path, path, 0.0, head_limit, 0).await {
        Ok((silence, black)) => {
            summary.silence.extend(silence);
            summary.black.extend(black);
        }
        Err(err) => summary.notes.push(format!("head analysis: {err}")),
    }

    if duration_ms > 60_000 {
        let duration_secs = duration_ms as f64 / 1000.0;
        let tail_len = TAIL_WINDOW_SECS.min(duration_secs / 2.0);
        let start_secs = (duration_secs - tail_len).max(0.0);
        let offset_ms = (start_secs * 1000.0) as i64;
        match run_ffmpeg_silence_black(
            &tools.ffmpeg.path,
            path,
            start_secs,
            tail_len,
            offset_ms,
        )
        .await
        {
            Ok((silence, black)) => {
                summary.silence.extend(silence);
                summary.black.extend(black);
            }
            Err(err) => summary.notes.push(format!("tail analysis: {err}")),
        }
    }

    Ok(summary)
}

async fn run_ffprobe_chapters(
    ffprobe: &PathBuf,
    input_path: &Path,
) -> Result<Vec<ChapterHint>, MarkerDetectError> {
    let output = tokio::time::timeout(
        FFPROBE_TIMEOUT,
        Command::new(ffprobe)
            .args([
                "-v",
                "error",
                "-print_format",
                "json",
                "-show_chapters",
                input_path.to_str().unwrap_or_default(),
            ])
            .output(),
    )
    .await
    .map_err(|_| MarkerDetectError::Ffmpeg("ffprobe chapters timed out".to_owned()))?
    .map_err(MarkerDetectError::Io)?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(MarkerDetectError::Ffmpeg(
            stderr.lines().next().unwrap_or("ffprobe failed").to_owned(),
        ));
    }

    #[derive(Deserialize)]
    struct ChaptersFile {
        #[serde(default)]
        chapters: Vec<FfprobeChapter>,
    }
    #[derive(Deserialize)]
    struct FfprobeChapter {
        #[serde(default)]
        start_time: Option<String>,
        #[serde(default)]
        end_time: Option<String>,
        #[serde(default)]
        tags: Option<FfprobeChapterTags>,
    }
    #[derive(Deserialize)]
    struct FfprobeChapterTags {
        #[serde(default)]
        title: Option<String>,
    }

    let parsed: ChaptersFile =
        serde_json::from_slice(&output.stdout).map_err(|err| MarkerDetectError::Ffmpeg(err.to_string()))?;
    let mut chapters = Vec::new();
    for chapter in parsed.chapters {
        let start_ms = parse_seconds_to_ms(chapter.start_time.as_deref()).unwrap_or(0);
        let end_ms = parse_seconds_to_ms(chapter.end_time.as_deref());
        let title = chapter
            .tags
            .and_then(|tags| tags.title)
            .unwrap_or_default();
        chapters.push(ChapterHint {
            title,
            start_ms,
            end_ms,
        });
    }
    Ok(chapters)
}

async fn run_ffmpeg_silence_black(
    ffmpeg: &PathBuf,
    input_path: &Path,
    start_secs: f64,
    duration_secs: f64,
    timeline_offset_ms: i64,
) -> Result<(Vec<TimeInterval>, Vec<TimeInterval>), MarkerDetectError> {
    if duration_secs <= 0.1 {
        return Ok((Vec::new(), Vec::new()));
    }

    let mut command = Command::new(ffmpeg);
    command
        .args(["-hide_banner", "-nostats", "-vn"])
        .arg("-ss")
        .arg(format!("{start_secs:.3}"))
        .arg("-t")
        .arg(format!("{duration_secs:.3}"))
        .arg("-i")
        .arg(input_path)
        // 仅音频静音检测；黑场需要视频，另跑一条轻量视频滤镜（可失败）。
        .args([
            "-af",
            "silencedetect=noise=-30dB:d=0.5",
            "-f",
            "null",
            "-",
        ])
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::piped());

    let output = tokio::time::timeout(FFMPEG_ANALYSIS_TIMEOUT, command.output())
        .await
        .map_err(|_| MarkerDetectError::Ffmpeg("ffmpeg silence timed out".to_owned()))?
        .map_err(MarkerDetectError::Io)?;

    let stderr = String::from_utf8_lossy(&output.stderr);
    let silence = parse_silence_intervals(&stderr, timeline_offset_ms);

    // 黑场：单独短跑（可能无视频流则跳过）
    let mut black = Vec::new();
    let mut black_cmd = Command::new(ffmpeg);
    black_cmd
        .args(["-hide_banner", "-nostats", "-an"])
        .arg("-ss")
        .arg(format!("{start_secs:.3}"))
        .arg("-t")
        .arg(format!("{duration_secs:.3}"))
        .arg("-i")
        .arg(input_path)
        .args([
            "-vf",
            "blackdetect=d=0.5:pix_th=0.10",
            "-f",
            "null",
            "-",
        ])
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::piped());

    if let Ok(Ok(black_output)) =
        tokio::time::timeout(FFMPEG_ANALYSIS_TIMEOUT, black_cmd.output()).await
    {
        let black_stderr = String::from_utf8_lossy(&black_output.stderr);
        black = parse_black_intervals(&black_stderr, timeline_offset_ms);
    }

    Ok((silence, black))
}

fn parse_seconds_to_ms(value: Option<&str>) -> Option<i64> {
    let raw = value?.trim();
    let seconds: f64 = raw.parse().ok()?;
    if !seconds.is_finite() || seconds < 0.0 {
        return None;
    }
    Some((seconds * 1000.0).round() as i64)
}

fn parse_silence_intervals(stderr: &str, offset_ms: i64) -> Vec<TimeInterval> {
    let start_re =
        Regex::new(r"silence_start:\s*([0-9.]+)").expect("silence_start regex");
    let end_re = Regex::new(r"silence_end:\s*([0-9.]+)").expect("silence_end regex");
    let mut starts = Vec::new();
    let mut intervals = Vec::new();
    for line in stderr.lines() {
        if let Some(caps) = start_re.captures(line) {
            if let Ok(secs) = caps[1].parse::<f64>() {
                starts.push(((secs * 1000.0).round() as i64).saturating_add(offset_ms));
            }
        }
        if let Some(caps) = end_re.captures(line) {
            if let Ok(secs) = caps[1].parse::<f64>() {
                let end = ((secs * 1000.0).round() as i64).saturating_add(offset_ms);
                let start = starts.pop().unwrap_or(end);
                intervals.push(TimeInterval {
                    start_ms: start,
                    end_ms: Some(end),
                });
            }
        }
    }
    // 未闭合的 silence_start
    for start in starts {
        intervals.push(TimeInterval {
            start_ms: start,
            end_ms: None,
        });
    }
    intervals
}

fn parse_black_intervals(stderr: &str, offset_ms: i64) -> Vec<TimeInterval> {
    // black_start:0.5 black_end:2.1 black_duration:1.6
    let re = Regex::new(
        r"black_start:([0-9.]+)\s+black_end:([0-9.]+)",
    )
    .expect("blackdetect regex");
    let mut intervals = Vec::new();
    for caps in re.captures_iter(stderr) {
        let start_secs: f64 = caps[1].parse().unwrap_or(0.0);
        let end_secs: f64 = caps[2].parse().unwrap_or(start_secs);
        intervals.push(TimeInterval {
            start_ms: ((start_secs * 1000.0).round() as i64).saturating_add(offset_ms),
            end_ms: Some(((end_secs * 1000.0).round() as i64).saturating_add(offset_ms)),
        });
    }
    intervals
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn chapter_title_classifier_matches_zh_en() {
        assert_eq!(classify_chapter_title("Opening / Intro"), ChapterKind::Intro);
        assert_eq!(classify_chapter_title("片头曲"), ChapterKind::Intro);
        assert_eq!(classify_chapter_title("End Credits"), ChapterKind::Credits);
        assert_eq!(classify_chapter_title("片尾"), ChapterKind::Outro);
        assert_eq!(classify_chapter_title("Scene 3"), ChapterKind::None);
    }

    #[test]
    fn heuristic_detect_from_intro_chapter() {
        let summary = ProbeSummary {
            chapters: vec![ChapterHint {
                title: "Intro".to_owned(),
                start_ms: 0,
                end_ms: Some(85_000),
            }],
            analysis_window_ms: 180_000,
            tail_window_ms: 240_000,
            ..ProbeSummary::default()
        };
        let markers = heuristic_detect(&summary, 1_800_000);
        assert!(
            markers
                .iter()
                .any(|marker| marker.marker_type == "intro_start")
        );
        assert!(
            markers
                .iter()
                .any(|marker| marker.marker_type == "intro_end")
        );
    }

    #[test]
    fn heuristic_detect_from_silence_window() {
        let summary = ProbeSummary {
            silence: vec![TimeInterval {
                start_ms: 70_000,
                end_ms: Some(72_500),
            }],
            analysis_window_ms: 180_000,
            tail_window_ms: 240_000,
            ..ProbeSummary::default()
        };
        let markers = heuristic_detect(&summary, 2_000_000);
        let intro_end = markers
            .iter()
            .find(|marker| marker.marker_type == "intro_end")
            .expect("intro_end");
        assert_eq!(intro_end.start_ticks, ms_to_ticks(72_500));
    }

    #[test]
    fn parse_detect_response_unwraps_sdk_envelope() {
        let value = json!({
            "ok": true,
            "result": {
                "markers": [{
                    "markerType": "intro_start",
                    "startTicks": 0,
                    "confidence": 0.9
                }],
                "detectorVersion": "marker-detect-0.2.0"
            }
        });
        let (markers, version) = parse_detect_response(&value).expect("parsed");
        assert_eq!(version, "marker-detect-0.2.0");
        assert_eq!(markers.len(), 1);
        assert_eq!(markers[0].marker_type, "intro_start");
    }

    #[test]
    fn parse_silence_and_black_from_ffmpeg_stderr() {
        let silence_log = "\
[silencedetect @ 0x] silence_start: 1.5
[silencedetect @ 0x] silence_end: 3.25 | silence_duration: 1.75
";
        let intervals = parse_silence_intervals(silence_log, 0);
        assert_eq!(intervals.len(), 1);
        assert_eq!(intervals[0].start_ms, 1500);
        assert_eq!(intervals[0].end_ms, Some(3250));

        let black_log =
            "black_start:0.5 black_end:2.0 black_duration:1.5\nblack_start:10 black_end:12.5\n";
        let blacks = parse_black_intervals(black_log, 1000);
        assert_eq!(blacks.len(), 2);
        assert_eq!(blacks[0].start_ms, 1500);
        assert_eq!(blacks[0].end_ms, Some(3000));
    }
}
