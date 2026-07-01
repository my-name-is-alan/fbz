use std::{
    error::Error,
    fmt::{Display, Formatter},
    path::{Path, PathBuf},
    time::Duration,
};

use serde::Deserialize;
use serde_json::{Value, json};
use sqlx::{Row, postgres::PgRow};
use tokio::{
    process::Command,
    sync::broadcast,
    task::JoinHandle,
    time::{MissedTickBehavior, interval},
};
use tracing::{info, warn};

use crate::{
    config::ProbeWorkerConfig,
    db::DbPool,
    jobs::{ExpiredJobMessages, expire_stale_running_jobs, mark_job_failed},
    media::tools::MediaToolDiagnostics,
};

pub const MEDIA_PROBE_JOB_TYPE: &str = "media.probe";
const PROBE_WORKER_ID: &str = "fbz-api-probe";
const PROBE_JOB_LEASE_SECONDS: i64 = 10 * 60;
const PROBE_JOB_LEASE_EXPIRED_RETRY: &str = "media probe lease expired; will retry";
const PROBE_JOB_LEASE_EXPIRED_FINAL: &str = "media probe lease expired; max attempts reached";
const PROBE_CLAIM_JOB_SQL: &str = r#"
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
const SIDECAR_SUBTITLE_CODECS: &[(&str, &str)] = &[
    ("srt", "srt"),
    ("vtt", "vtt"),
    ("webvtt", "vtt"),
    ("ass", "ass"),
    ("ssa", "ssa"),
    ("sub", "sub"),
];

pub fn spawn_probe_worker(
    pool: DbPool,
    config: ProbeWorkerConfig,
    media_tools: MediaToolDiagnostics,
    mut shutdown: broadcast::Receiver<()>,
) -> JoinHandle<()> {
    tokio::spawn(async move {
        let service = ProbeService::new(pool, media_tools);
        let mut tick = interval(Duration::from_secs(config.interval_seconds));
        tick.set_missed_tick_behavior(MissedTickBehavior::Delay);

        info!(
            interval_seconds = config.interval_seconds,
            "probe worker started"
        );

        loop {
            tokio::select! {
                result = shutdown.recv() => {
                    match result {
                        Ok(()) | Err(broadcast::error::RecvError::Closed) => {
                            info!("probe worker shutdown received");
                            break;
                        }
                        Err(broadcast::error::RecvError::Lagged(skipped)) => {
                            warn!(skipped, "probe worker shutdown receiver lagged");
                            break;
                        }
                    }
                }
                _ = tick.tick() => {
                    run_available_jobs(&service).await;
                }
            }
        }

        info!("probe worker stopped");
    })
}

async fn run_available_jobs(service: &ProbeService) {
    loop {
        match service.run_next_probe_job().await {
            Ok(Some(summary)) => {
                info!(
                    job_id = %summary.job_id,
                    media_file_id = summary.media_file_id,
                    status = %summary.status,
                    stream_count = summary.stream_count,
                    "media probe job completed by background worker"
                );
            }
            Ok(None) => break,
            Err(err) => {
                warn!(error = %err, "probe worker failed to run job");
                break;
            }
        }
    }
}

#[derive(Clone)]
pub struct ProbeService {
    pool: DbPool,
    media_tools: MediaToolDiagnostics,
    worker_id: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ProbeSummary {
    pub job_id: String,
    pub media_file_id: i64,
    pub status: String,
    pub container: Option<String>,
    pub duration_ticks: Option<i64>,
    pub bitrate: Option<i32>,
    pub stream_count: usize,
}

#[derive(Clone, Debug)]
struct ClaimedProbeJob {
    id: i64,
    public_id: String,
    payload: Value,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct ProbeJobRequest {
    media_file_id: i64,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct ProbeTarget {
    media_file_id: i64,
    path: String,
    is_strm: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct ProbeResult {
    container: Option<String>,
    duration_ticks: Option<i64>,
    bitrate: Option<i32>,
    streams: Vec<ProbeStream>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct ProbeStream {
    stream_index: i32,
    stream_type: String,
    codec: Option<String>,
    codec_tag: Option<String>,
    language: Option<String>,
    title: Option<String>,
    profile: Option<String>,
    level: Option<i32>,
    width: Option<i32>,
    height: Option<i32>,
    channels: Option<i32>,
    sample_rate: Option<i32>,
    bit_depth: Option<i32>,
    bitrate: Option<i32>,
    is_default: bool,
    is_forced: bool,
    is_external: bool,
    extra: Value,
}

#[derive(Debug)]
pub enum ProbeError {
    JobNotFound,
    MissingMediaFileId,
    InvalidMediaFileId(String),
    MediaFileNotFound(i64),
    Database(sqlx::Error),
    Io(std::io::Error),
    Json(serde_json::Error),
    FfprobeFailed(String),
}

impl ProbeService {
    pub fn new(pool: DbPool, media_tools: MediaToolDiagnostics) -> Self {
        Self {
            pool,
            media_tools,
            worker_id: PROBE_WORKER_ID.to_owned(),
        }
    }

    pub async fn run_next_probe_job(&self) -> Result<Option<ProbeSummary>, ProbeError> {
        let Some(job) = self.claim_probe_job(None).await? else {
            return Ok(None);
        };

        self.run_claimed_probe_job(job).await.map(Some)
    }

    async fn run_claimed_probe_job(
        &self,
        job: ClaimedProbeJob,
    ) -> Result<ProbeSummary, ProbeError> {
        let request = ProbeJobRequest::from_payload(&job.payload)?;
        let run_id = self.start_job_run(job.id).await?;
        self.record_job_event(
            job.id,
            Some(run_id),
            "media.probe.started",
            "info",
            "media probe started",
            json!({ "mediaFileId": request.media_file_id }),
        )
        .await?;

        let result = self.probe_media_file(request.media_file_id).await;
        match result {
            Ok(summary) => {
                let completed = ProbeSummary {
                    job_id: job.public_id,
                    ..summary
                };
                self.finish_job_success(job.id, run_id, &completed).await?;
                Ok(completed)
            }
            Err(err) => {
                let message = err.to_string();
                if let Err(event_err) = self
                    .record_job_event(
                        job.id,
                        Some(run_id),
                        "media.probe.failed",
                        "error",
                        &message,
                        json!({ "mediaFileId": request.media_file_id }),
                    )
                    .await
                {
                    warn!(error = %event_err, "failed to record media probe failure event");
                }
                self.finish_job_failure(&job.public_id, job.id, run_id, &message)
                    .await?;
                Err(err)
            }
        }
    }

    async fn claim_probe_job(
        &self,
        job_id: Option<&str>,
    ) -> Result<Option<ClaimedProbeJob>, ProbeError> {
        let mut tx = self.pool.begin().await.map_err(ProbeError::Database)?;
        expire_stale_running_jobs(
            &mut tx,
            MEDIA_PROBE_JOB_TYPE,
            ExpiredJobMessages {
                retry: PROBE_JOB_LEASE_EXPIRED_RETRY,
                final_failure: PROBE_JOB_LEASE_EXPIRED_FINAL,
            },
        )
        .await
        .map_err(ProbeError::Database)?;

        let job = sqlx::query(PROBE_CLAIM_JOB_SQL)
            .bind(job_id)
            .bind(MEDIA_PROBE_JOB_TYPE)
            .bind(&self.worker_id)
            .bind(PROBE_JOB_LEASE_SECONDS)
            .fetch_optional(&mut *tx)
            .await
            .map_err(ProbeError::Database)?
            .map(ClaimedProbeJob::from_row)
            .transpose()
            .map_err(ProbeError::Database)?;

        tx.commit().await.map_err(ProbeError::Database)?;
        Ok(job)
    }

    async fn probe_media_file(&self, media_file_id: i64) -> Result<ProbeSummary, ProbeError> {
        let target = self.load_target(media_file_id).await?;
        if target.is_strm {
            return Ok(ProbeSummary {
                job_id: String::new(),
                media_file_id,
                status: "skipped_strm".to_owned(),
                container: None,
                duration_ticks: None,
                bitrate: None,
                stream_count: 0,
            });
        }

        if tokio::fs::metadata(&target.path).await.is_err() {
            return Ok(ProbeSummary {
                job_id: String::new(),
                media_file_id,
                status: "skipped_unavailable".to_owned(),
                container: None,
                duration_ticks: None,
                bitrate: None,
                stream_count: 0,
            });
        }

        let mut result = run_ffprobe(&self.media_tools.ffprobe.path, &target.path).await?;
        append_sidecar_subtitles(&target.path, &mut result.streams);
        self.apply_probe_result(media_file_id, &result).await?;
        Ok(ProbeSummary {
            job_id: String::new(),
            media_file_id,
            status: "probed".to_owned(),
            container: result.container,
            duration_ticks: result.duration_ticks,
            bitrate: result.bitrate,
            stream_count: result.streams.len(),
        })
    }

    async fn load_target(&self, media_file_id: i64) -> Result<ProbeTarget, ProbeError> {
        let Some(row) = sqlx::query(
            r#"
            select mf.id as media_file_id,
                   mf.path,
                   mf.is_strm
            from media_files mf
            join media_items mi on mi.id = mf.media_item_id
            where mf.id = $1
              and mi.is_deleted = false
              and mi.scan_status <> 'missing'
            "#,
        )
        .bind(media_file_id)
        .fetch_optional(&self.pool)
        .await
        .map_err(ProbeError::Database)?
        else {
            return Err(ProbeError::MediaFileNotFound(media_file_id));
        };

        ProbeTarget::from_row(row).map_err(ProbeError::Database)
    }

    async fn apply_probe_result(
        &self,
        media_file_id: i64,
        result: &ProbeResult,
    ) -> Result<(), ProbeError> {
        let mut tx = self.pool.begin().await.map_err(ProbeError::Database)?;
        sqlx::query(
            r#"
            update media_files
            set container = $2,
                duration_ticks = $3,
                bitrate = $4,
                updated_at = now()
            where id = $1
            "#,
        )
        .bind(media_file_id)
        .bind(result.container.as_deref())
        .bind(result.duration_ticks)
        .bind(result.bitrate)
        .execute(&mut *tx)
        .await
        .map_err(ProbeError::Database)?;

        sqlx::query("delete from media_streams where media_file_id = $1")
            .bind(media_file_id)
            .execute(&mut *tx)
            .await
            .map_err(ProbeError::Database)?;

        for stream in &result.streams {
            sqlx::query(
                r#"
                insert into media_streams (
                    media_file_id,
                    stream_index,
                    stream_type,
                    codec,
                    codec_tag,
                    language,
                    title,
                    profile,
                    level,
                    width,
                    height,
                    channels,
                    sample_rate,
                    bit_depth,
                    bitrate,
                    is_default,
                    is_forced,
                    is_external,
                    extra
                )
                values (
                    $1, $2, $3, $4, $5, $6, $7, $8, $9,
                    $10, $11, $12, $13, $14, $15, $16, $17, $18, $19
                )
                "#,
            )
            .bind(media_file_id)
            .bind(stream.stream_index)
            .bind(&stream.stream_type)
            .bind(stream.codec.as_deref())
            .bind(stream.codec_tag.as_deref())
            .bind(stream.language.as_deref())
            .bind(stream.title.as_deref())
            .bind(stream.profile.as_deref())
            .bind(stream.level)
            .bind(stream.width)
            .bind(stream.height)
            .bind(stream.channels)
            .bind(stream.sample_rate)
            .bind(stream.bit_depth)
            .bind(stream.bitrate)
            .bind(stream.is_default)
            .bind(stream.is_forced)
            .bind(stream.is_external)
            .bind(&stream.extra)
            .execute(&mut *tx)
            .await
            .map_err(ProbeError::Database)?;
        }

        // 阶段 5（recognition design §10）：用 ffprobe 实测的 video stream 校正文件名识别的
        // 画质标签（实测优先）。取首个 video stream 的 height → 标准分辨率、codec → 归一编码。
        // 实测缺失时保留文件名标签（coalesce），不清空。
        let video = result.streams.iter().find(|s| s.stream_type == "video");
        if let Some(video) = video {
            let measured_resolution = video
                .height
                .and_then(crate::media_types::resolution_from_height);
            let measured_codec = video.codec.as_deref().and_then(normalize_probe_video_codec);
            sqlx::query(
                r#"
                update media_files
                set resolution = coalesce($2, resolution),
                    video_codec = coalesce($3, video_codec),
                    updated_at = now()
                where id = $1
                "#,
            )
            .bind(media_file_id)
            .bind(measured_resolution)
            .bind(measured_codec)
            .execute(&mut *tx)
            .await
            .map_err(ProbeError::Database)?;
        }

        tx.commit().await.map_err(ProbeError::Database)
    }

    async fn start_job_run(&self, job_id: i64) -> Result<i64, ProbeError> {
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
        .map_err(ProbeError::Database)
    }

    async fn finish_job_success(
        &self,
        job_id: i64,
        run_id: i64,
        summary: &ProbeSummary,
    ) -> Result<(), ProbeError> {
        let metrics = json!({
            "mediaFileId": summary.media_file_id,
            "status": summary.status,
            "container": summary.container,
            "durationTicks": summary.duration_ticks,
            "bitrate": summary.bitrate,
            "streamCount": summary.stream_count,
        });

        let mut tx = self.pool.begin().await.map_err(ProbeError::Database)?;
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
        .map_err(ProbeError::Database)?;

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
        .map_err(ProbeError::Database)?;

        tx.commit().await.map_err(ProbeError::Database)
    }

    async fn finish_job_failure(
        &self,
        job_public_id: &str,
        job_id: i64,
        run_id: i64,
        message: &str,
    ) -> Result<(), ProbeError> {
        let mut tx = self.pool.begin().await.map_err(ProbeError::Database)?;
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
        .map_err(ProbeError::Database)?;

        mark_job_failed(
            &mut tx,
            MEDIA_PROBE_JOB_TYPE,
            job_public_id,
            job_id,
            message,
        )
        .await
        .map_err(ProbeError::Database)?;

        tx.commit().await.map_err(ProbeError::Database)
    }

    async fn record_job_event(
        &self,
        job_id: i64,
        run_id: Option<i64>,
        event_type: &str,
        event_level: &str,
        message: &str,
        payload: Value,
    ) -> Result<(), ProbeError> {
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
        .map_err(ProbeError::Database)?;

        Ok(())
    }
}

async fn run_ffprobe(path: &PathBuf, input_path: &str) -> Result<ProbeResult, ProbeError> {
    let output = Command::new(path)
        .args([
            "-v",
            "error",
            "-print_format",
            "json",
            "-show_format",
            "-show_streams",
            input_path,
        ])
        .output()
        .await
        .map_err(ProbeError::Io)?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        let stdout = String::from_utf8_lossy(&output.stdout);
        let message = stderr
            .lines()
            .chain(stdout.lines())
            .map(str::trim)
            .find(|line| !line.is_empty())
            .unwrap_or("ffprobe exited without diagnostic output")
            .to_owned();
        return Err(ProbeError::FfprobeFailed(message));
    }

    parse_ffprobe_json(&output.stdout)
}

fn parse_ffprobe_json(input: &[u8]) -> Result<ProbeResult, ProbeError> {
    let output: FfprobeOutput = serde_json::from_slice(input).map_err(ProbeError::Json)?;
    Ok(ProbeResult {
        container: output
            .format
            .as_ref()
            .and_then(|format| clean_optional(format.format_name.as_deref())),
        duration_ticks: output
            .format
            .as_ref()
            .and_then(|format| parse_duration_ticks(format.duration.as_deref())),
        bitrate: output
            .format
            .as_ref()
            .and_then(|format| parse_i32_string(format.bit_rate.as_deref())),
        streams: output
            .streams
            .into_iter()
            .filter_map(ffprobe_stream_to_probe_stream)
            .collect(),
    })
}

fn ffprobe_stream_to_probe_stream(stream: FfprobeStream) -> Option<ProbeStream> {
    let stream_type = stream_type_from_ffprobe(stream.codec_type.as_deref())?;
    let extra = serde_json::to_value(&stream).unwrap_or_else(|_| json!({}));
    Some(ProbeStream {
        stream_index: stream.index?,
        stream_type: stream_type.to_owned(),
        codec: clean_optional(stream.codec_name.as_deref()),
        codec_tag: clean_optional(stream.codec_tag_string.as_deref()),
        language: stream
            .tags
            .as_ref()
            .and_then(|tags| clean_optional(tags.language.as_deref())),
        title: stream
            .tags
            .as_ref()
            .and_then(|tags| clean_optional(tags.title.as_deref())),
        profile: clean_optional(stream.profile.as_deref()),
        level: stream.level,
        width: stream.width,
        height: stream.height,
        channels: stream.channels,
        sample_rate: parse_i32_string(stream.sample_rate.as_deref()),
        bit_depth: parse_i32_string(stream.bits_per_raw_sample.as_deref()),
        bitrate: parse_i32_string(stream.bit_rate.as_deref()),
        is_default: stream
            .disposition
            .as_ref()
            .and_then(|disposition| disposition.default)
            .unwrap_or(0)
            == 1,
        is_forced: stream
            .disposition
            .as_ref()
            .and_then(|disposition| disposition.forced)
            .unwrap_or(0)
            == 1,
        is_external: false,
        extra,
    })
}

fn append_sidecar_subtitles(media_path: &str, streams: &mut Vec<ProbeStream>) {
    let media_path = Path::new(media_path.trim());
    let Some(media_parent) = media_path.parent() else {
        return;
    };
    let Some(media_stem) = media_path.file_stem().and_then(|value| value.to_str()) else {
        return;
    };

    let Ok(entries) = std::fs::read_dir(media_parent) else {
        return;
    };
    let mut sidecars = entries
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .filter(|path| path.is_file())
        .filter_map(|path| sidecar_subtitle_stream(media_parent, media_stem, &path))
        .collect::<Vec<_>>();
    sidecars.sort_by(|left, right| {
        left.extra
            .get("path")
            .and_then(Value::as_str)
            .cmp(&right.extra.get("path").and_then(Value::as_str))
    });

    let mut next_index = streams
        .iter()
        .map(|stream| stream.stream_index)
        .max()
        .unwrap_or(-1)
        .saturating_add(1);
    for mut stream in sidecars {
        stream.stream_index = next_index;
        next_index = next_index.saturating_add(1);
        streams.push(stream);
    }
}

fn sidecar_subtitle_stream(
    media_parent: &Path,
    media_stem: &str,
    path: &Path,
) -> Option<ProbeStream> {
    let extension = path.extension().and_then(|value| value.to_str())?;
    let codec = normalize_sidecar_subtitle_extension(extension)?;
    let stem = path.file_stem().and_then(|value| value.to_str())?;
    if !sidecar_stem_matches_media(stem, media_stem) {
        return None;
    }
    let file_name = path.file_name()?.to_str()?;
    let relative_path = path
        .strip_prefix(media_parent)
        .ok()
        .and_then(|path| path.to_str())
        .unwrap_or(file_name)
        .replace('\\', "/");

    Some(ProbeStream {
        stream_index: 0,
        stream_type: "subtitle".to_owned(),
        codec: Some(codec.to_owned()),
        codec_tag: None,
        language: sidecar_subtitle_language(stem, media_stem),
        title: Some(file_name.to_owned()),
        profile: None,
        level: None,
        width: None,
        height: None,
        channels: None,
        sample_rate: None,
        bit_depth: None,
        bitrate: None,
        is_default: false,
        is_forced: sidecar_subtitle_is_forced(stem, media_stem),
        is_external: true,
        extra: json!({
            "source": "sidecar",
            "path": relative_path,
        }),
    })
}

fn normalize_sidecar_subtitle_extension(value: &str) -> Option<&'static str> {
    let normalized = value.trim().trim_start_matches('.').to_ascii_lowercase();
    SIDECAR_SUBTITLE_CODECS
        .iter()
        .find_map(|(extension, codec)| (*extension == normalized).then_some(*codec))
}

fn sidecar_stem_matches_media(stem: &str, media_stem: &str) -> bool {
    sidecar_suffix(stem, media_stem).is_some()
}

fn sidecar_subtitle_language(stem: &str, media_stem: &str) -> Option<String> {
    let suffix = sidecar_suffix(stem, media_stem)?;
    if suffix.is_empty() {
        return None;
    }
    suffix
        .split('.')
        .find(|token| {
            let normalized = token.trim().to_ascii_lowercase();
            !normalized.is_empty()
                && !matches!(
                    normalized.as_str(),
                    "forced" | "default" | "sdh" | "cc" | "hi"
                )
                && normalized.len() <= 32
                && normalized
                    .bytes()
                    .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_'))
        })
        .map(|token| token.trim().to_ascii_lowercase())
}

fn sidecar_subtitle_is_forced(stem: &str, media_stem: &str) -> bool {
    sidecar_suffix(stem, media_stem).is_some_and(|suffix| {
        !suffix.is_empty()
            && suffix
                .split('.')
                .any(|token| token.trim().eq_ignore_ascii_case("forced"))
    })
}

fn sidecar_suffix<'a>(stem: &'a str, media_stem: &str) -> Option<&'a str> {
    if stem.eq_ignore_ascii_case(media_stem) {
        return Some("");
    }

    let prefix = format!("{}.", media_stem.to_ascii_lowercase());
    let stem_lower = stem.to_ascii_lowercase();
    stem_lower
        .starts_with(&prefix)
        .then(|| stem.get(prefix.len()..))
        .flatten()
}

fn stream_type_from_ffprobe(value: Option<&str>) -> Option<&'static str> {
    match value?.trim().to_ascii_lowercase().as_str() {
        "video" => Some("video"),
        "audio" => Some("audio"),
        "subtitle" => Some("subtitle"),
        "attachment" => Some("attachment"),
        "data" => Some("data"),
        _ => None,
    }
}

fn parse_duration_ticks(value: Option<&str>) -> Option<i64> {
    let seconds = value?.trim().parse::<f64>().ok()?;
    if !seconds.is_finite() || seconds < 0.0 {
        return None;
    }
    let ticks = seconds * 10_000_000.0;
    (ticks <= i64::MAX as f64).then_some(ticks.round() as i64)
}

fn parse_i32_string(value: Option<&str>) -> Option<i32> {
    value?.trim().parse::<i64>().ok()?.try_into().ok()
}

fn clean_optional(value: Option<&str>) -> Option<String> {
    value
        .map(str::trim)
        .filter(|value| !value.is_empty() && *value != "N/A")
        .map(str::to_owned)
}

/// 把 ffprobe 原始视频 codec 名归一到与文件名识别词汇一致的标签（x265/x264/AV1）。
/// 无法识别返回 None（保留文件名标签，不覆盖）。
fn normalize_probe_video_codec(codec: &str) -> Option<&'static str> {
    match codec.to_ascii_lowercase().as_str() {
        "hevc" | "h265" | "h.265" | "x265" => Some("x265"),
        "h264" | "h.264" | "avc" | "x264" => Some("x264"),
        "av1" => Some("AV1"),
        _ => None,
    }
}

impl ProbeJobRequest {
    fn from_payload(payload: &Value) -> Result<Self, ProbeError> {
        let Some(value) = payload.get("mediaFileId") else {
            return Err(ProbeError::MissingMediaFileId);
        };
        let media_file_id = value
            .as_i64()
            .or_else(|| value.as_str()?.trim().parse::<i64>().ok())
            .filter(|value| *value > 0)
            .ok_or_else(|| ProbeError::InvalidMediaFileId(value.to_string()))?;

        Ok(Self { media_file_id })
    }
}

impl ClaimedProbeJob {
    fn from_row(row: PgRow) -> Result<Self, sqlx::Error> {
        Ok(Self {
            id: row.try_get("id")?,
            public_id: row.try_get("public_id")?,
            payload: row.try_get("payload")?,
        })
    }
}

impl ProbeTarget {
    fn from_row(row: PgRow) -> Result<Self, sqlx::Error> {
        Ok(Self {
            media_file_id: row.try_get("media_file_id")?,
            path: row.try_get("path")?,
            is_strm: row.try_get("is_strm")?,
        })
    }
}

impl Display for ProbeError {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::JobNotFound => f.write_str("media probe job not found or not runnable"),
            Self::MissingMediaFileId => {
                f.write_str("media probe job payload is missing mediaFileId")
            }
            Self::InvalidMediaFileId(value) => {
                write!(f, "invalid media probe mediaFileId `{value}`")
            }
            Self::MediaFileNotFound(id) => write!(f, "media file `{id}` not found"),
            Self::Database(err) => write!(f, "database error: {err}"),
            Self::Io(err) => write!(f, "io error: {err}"),
            Self::Json(err) => write!(f, "ffprobe json parse error: {err}"),
            Self::FfprobeFailed(message) => write!(f, "ffprobe failed: {message}"),
        }
    }
}

impl Error for ProbeError {}

#[derive(Debug, Deserialize, serde::Serialize)]
struct FfprobeOutput {
    #[serde(default)]
    streams: Vec<FfprobeStream>,
    format: Option<FfprobeFormat>,
}

#[derive(Debug, Deserialize, serde::Serialize)]
struct FfprobeFormat {
    format_name: Option<String>,
    duration: Option<String>,
    bit_rate: Option<String>,
}

#[derive(Debug, Deserialize, serde::Serialize)]
struct FfprobeStream {
    index: Option<i32>,
    codec_type: Option<String>,
    codec_name: Option<String>,
    codec_tag_string: Option<String>,
    profile: Option<String>,
    level: Option<i32>,
    width: Option<i32>,
    height: Option<i32>,
    channels: Option<i32>,
    sample_rate: Option<String>,
    bits_per_raw_sample: Option<String>,
    bit_rate: Option<String>,
    disposition: Option<FfprobeDisposition>,
    tags: Option<FfprobeTags>,
}

#[derive(Debug, Deserialize, serde::Serialize)]
struct FfprobeDisposition {
    default: Option<i32>,
    forced: Option<i32>,
}

#[derive(Debug, Deserialize, serde::Serialize)]
struct FfprobeTags {
    language: Option<String>,
    title: Option<String>,
}

#[cfg(test)]
mod tests {
    use std::{
        fs as std_fs, process,
        time::{SystemTime, UNIX_EPOCH},
    };

    use super::*;

    #[test]
    fn normalize_probe_video_codec_aligns_with_filename_vocab() {
        assert_eq!(normalize_probe_video_codec("hevc"), Some("x265"));
        assert_eq!(normalize_probe_video_codec("H265"), Some("x265"));
        assert_eq!(normalize_probe_video_codec("h264"), Some("x264"));
        assert_eq!(normalize_probe_video_codec("avc"), Some("x264"));
        assert_eq!(normalize_probe_video_codec("av1"), Some("AV1"));
        assert_eq!(normalize_probe_video_codec("mpeg2video"), None);
    }

    #[test]
    fn probe_job_request_accepts_numeric_or_string_media_file_ids() {
        assert_eq!(
            ProbeJobRequest::from_payload(&json!({"mediaFileId": 42})).unwrap(),
            ProbeJobRequest { media_file_id: 42 }
        );
        assert_eq!(
            ProbeJobRequest::from_payload(&json!({"mediaFileId": "43"})).unwrap(),
            ProbeJobRequest { media_file_id: 43 }
        );
        assert!(matches!(
            ProbeJobRequest::from_payload(&json!({"mediaFileId": 0})),
            Err(ProbeError::InvalidMediaFileId(_))
        ));
    }

    #[test]
    fn probe_job_public_id_input_uses_uuid_comparison() {
        assert!(PROBE_CLAIM_JOB_SQL.contains("with requested_job as"));
        assert!(PROBE_CLAIM_JOB_SQL.contains("$1::uuid"));
        assert!(PROBE_CLAIM_JOB_SQL.contains("jobs.public_id = requested_job.public_id"));
        assert!(!PROBE_CLAIM_JOB_SQL.contains("public_id::text = $1"));
    }

    #[test]
    fn parse_ffprobe_json_maps_format_and_streams() {
        let input = br#"
        {
          "streams": [
            {
              "index": 0,
              "codec_type": "video",
              "codec_name": "h264",
              "codec_tag_string": "avc1",
              "profile": "High",
              "level": 41,
              "width": 1920,
              "height": 1080,
              "bit_rate": "4000000",
              "disposition": {"default": 1, "forced": 0},
              "tags": {"language": "und", "title": "Main"}
            },
            {
              "index": 1,
              "codec_type": "audio",
              "codec_name": "aac",
              "channels": 2,
              "sample_rate": "48000",
              "bit_rate": "192000",
              "disposition": {"default": 1, "forced": 0},
              "tags": {"language": "eng"}
            }
          ],
          "format": {
            "format_name": "mov,mp4,m4a,3gp,3g2,mj2",
            "duration": "12.345678",
            "bit_rate": "4200000"
          }
        }
        "#;

        let result = parse_ffprobe_json(input).unwrap();

        assert_eq!(result.container.as_deref(), Some("mov,mp4,m4a,3gp,3g2,mj2"));
        assert_eq!(result.duration_ticks, Some(123_456_780));
        assert_eq!(result.bitrate, Some(4_200_000));
        assert_eq!(result.streams.len(), 2);
        assert_eq!(result.streams[0].stream_type, "video");
        assert_eq!(result.streams[0].codec.as_deref(), Some("h264"));
        assert_eq!(result.streams[0].width, Some(1920));
        assert!(result.streams[0].is_default);
        assert_eq!(result.streams[1].stream_type, "audio");
        assert_eq!(result.streams[1].channels, Some(2));
        assert_eq!(result.streams[1].sample_rate, Some(48_000));
    }

    #[test]
    fn stream_type_mapping_rejects_unknown_types() {
        assert_eq!(stream_type_from_ffprobe(Some("video")), Some("video"));
        assert_eq!(stream_type_from_ffprobe(Some("unknown")), None);
    }

    #[test]
    fn sidecar_subtitle_detection_appends_external_streams() {
        let base_dir = unique_test_dir("fbz-sidecar-subtitles-test");
        std_fs::create_dir_all(&base_dir).unwrap();
        let media_path = base_dir.join("Movie.mkv");
        std_fs::write(&media_path, b"movie").unwrap();
        std_fs::write(base_dir.join("Movie.en.srt"), b"1\n").unwrap();
        std_fs::write(
            base_dir.join("Movie.zh-Hans.forced.ass"),
            b"[Script Info]\n",
        )
        .unwrap();
        std_fs::write(base_dir.join("movie.vtt"), b"WEBVTT\n").unwrap();
        std_fs::write(base_dir.join("Movie Extras.srt"), b"ignored").unwrap();
        std_fs::write(base_dir.join("Other.srt"), b"ignored").unwrap();

        let mut streams = vec![probe_stream(1, "video")];
        append_sidecar_subtitles(&media_path.to_string_lossy(), &mut streams);

        assert_eq!(streams.len(), 4);
        assert_eq!(streams[0].stream_index, 1);
        assert!(!streams[0].is_external);

        let external = streams
            .iter()
            .filter(|stream| stream.is_external)
            .collect::<Vec<_>>();
        assert_eq!(external.len(), 3);
        assert_eq!(external[0].stream_index, 2);
        assert_eq!(external[0].stream_type, "subtitle");
        assert_eq!(external[0].codec.as_deref(), Some("srt"));
        assert_eq!(external[0].language.as_deref(), Some("en"));
        assert_eq!(external[0].extra["path"], "Movie.en.srt");
        assert_eq!(external[1].stream_index, 3);
        assert_eq!(external[1].codec.as_deref(), Some("ass"));
        assert_eq!(external[1].language.as_deref(), Some("zh-hans"));
        assert!(external[1].is_forced);
        assert_eq!(external[2].stream_index, 4);
        assert_eq!(external[2].codec.as_deref(), Some("vtt"));
        assert_eq!(external[2].language, None);

        std_fs::remove_dir_all(base_dir).unwrap();
    }

    #[test]
    fn sidecar_subtitle_helpers_are_strict() {
        assert!(sidecar_stem_matches_media("Movie", "movie"));
        assert!(sidecar_stem_matches_media("Movie.zh-Hans", "movie"));
        assert!(!sidecar_stem_matches_media("Movie Extras", "movie"));
        assert_eq!(normalize_sidecar_subtitle_extension("WEBVTT"), Some("vtt"));
        assert_eq!(normalize_sidecar_subtitle_extension("txt"), None);
        assert_eq!(
            sidecar_subtitle_language("Movie.sdh.zh", "Movie").as_deref(),
            Some("zh")
        );
        assert!(sidecar_subtitle_is_forced("Movie.en.Forced", "Movie"));
    }

    fn probe_stream(stream_index: i32, stream_type: &str) -> ProbeStream {
        ProbeStream {
            stream_index,
            stream_type: stream_type.to_owned(),
            codec: None,
            codec_tag: None,
            language: None,
            title: None,
            profile: None,
            level: None,
            width: None,
            height: None,
            channels: None,
            sample_rate: None,
            bit_depth: None,
            bitrate: None,
            is_default: false,
            is_forced: false,
            is_external: false,
            extra: json!({}),
        }
    }

    fn unique_test_dir(prefix: &str) -> PathBuf {
        std::env::temp_dir().join(format!(
            "{}-{}-{}",
            prefix,
            process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ))
    }
}
