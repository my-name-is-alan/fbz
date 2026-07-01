//! 图片元数据提取（家庭库 `item_type='photo'`）。
//!
//! 从图片字节中解析尺寸（[`image`]）与 EXIF（[`exif`]）：拍摄时间、相机/镜头、
//! 曝光参数、GPS。核心是**纯函数**（字节/原始值 → 结构体），无文件系统副作用，
//! 便于穷举单测。落盘/缩略图生成/入库由 worker 层（后续增量）调用本模块。
//!
//! 设计要点：
//! - 一切尽力而为，任一字段缺失或损坏返回 `None`，**绝不 panic**（坏图不可阻断扫描）。
//! - 数值换算（曝光分数、GPS 度分秒）抽成纯函数，独立可测。
//! - HEIC/HEIF：`image` 默认不解码（需 libheif），但 EXIF 仍可能从容器读出；
//!   尺寸缺失时退化为 `None`，不报错。

use std::io::{BufReader, Cursor};
use std::{path::PathBuf, time::Duration};

use exif::{Exif, In, Reader, Tag, Value};
use image::{ImageReader, imageops::FilterType};
use serde_json::{Value as JsonValue, json};
use sqlx::{Row, postgres::PgRow};
use tokio::{
    sync::broadcast,
    task::JoinHandle,
    time::{MissedTickBehavior, interval},
};
use tracing::{info, warn};

use crate::{
    config::PhotoWorkerConfig,
    db::DbPool,
    jobs::{ExpiredJobMessages, expire_stale_running_jobs, mark_job_failed},
};

/// 图片元数据提取 job 类型（与 `media.probe` 并列的扫描后处理）。
pub const MEDIA_PHOTO_JOB_TYPE: &str = "media.photo";
const PHOTO_WORKER_ID: &str = "fbz-api-photo";
const PHOTO_JOB_LEASE_SECONDS: i64 = 10 * 60;
const PHOTO_JOB_LEASE_EXPIRED_RETRY: &str = "media photo lease expired; will retry";
const PHOTO_JOB_LEASE_EXPIRED_FINAL: &str = "media photo lease expired; max attempts reached";
/// 缩略图最长边像素（等比缩放）。
const PHOTO_THUMBNAIL_MAX_EDGE: u32 = 480;
const PHOTO_CLAIM_JOB_SQL: &str = r#"
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

/// 提取出的图片元数据，字段对应 `media_photo_metadata` 表。
#[derive(Clone, Debug, Default, PartialEq)]
pub struct PhotoMetadata {
    pub width: Option<u32>,
    pub height: Option<u32>,
    /// EXIF DateTimeOriginal，归一化为 `YYYY-MM-DDTHH:MM:SS`（无时区，EXIF 通常不带）。
    pub captured_at: Option<String>,
    pub camera_make: Option<String>,
    pub camera_model: Option<String>,
    pub lens_model: Option<String>,
    /// EXIF Orientation，标准值 1..=8。
    pub orientation: Option<u16>,
    pub iso: Option<u32>,
    pub f_number: Option<f64>,
    /// 快门速度，保真为分数文本，如 `1/250`。
    pub exposure_time: Option<String>,
    pub focal_length: Option<f64>,
    pub gps_latitude: Option<f64>,
    pub gps_longitude: Option<f64>,
    pub gps_altitude: Option<f64>,
}

impl PhotoMetadata {
    /// 从完整图片字节提取尺寸 + EXIF。任何子项失败都降级为 `None`。
    pub fn extract_from_bytes(bytes: &[u8]) -> PhotoMetadata {
        let mut metadata = PhotoMetadata {
            ..Default::default()
        };

        if let Some((width, height)) = decode_dimensions(bytes) {
            metadata.width = Some(width);
            metadata.height = Some(height);
        }

        if let Some(exif) = read_exif(bytes) {
            metadata.apply_exif(&exif);
        }

        metadata
    }

    fn apply_exif(&mut self, exif: &Exif) {
        self.captured_at = exif_datetime(exif, Tag::DateTimeOriginal)
            .or_else(|| exif_datetime(exif, Tag::DateTime));
        self.camera_make = exif_ascii(exif, Tag::Make);
        self.camera_model = exif_ascii(exif, Tag::Model);
        self.lens_model = exif_ascii(exif, Tag::LensModel);
        self.orientation = exif_u32(exif, Tag::Orientation)
            .and_then(|value| u16::try_from(value).ok())
            .filter(|value| (1..=8).contains(value));
        self.iso =
            exif_u32(exif, Tag::PhotographicSensitivity).or_else(|| exif_u32(exif, Tag::ISOSpeed));
        self.f_number = exif_rational_f64(exif, Tag::FNumber);
        self.exposure_time = exif_first_rational(exif, Tag::ExposureTime)
            .map(|(num, denom)| format_exposure(num, denom));
        self.focal_length = exif_rational_f64(exif, Tag::FocalLength);
        self.gps_latitude = exif_gps_coordinate(exif, Tag::GPSLatitude, Tag::GPSLatitudeRef);
        self.gps_longitude = exif_gps_coordinate(exif, Tag::GPSLongitude, Tag::GPSLongitudeRef);
        self.gps_altitude = exif_gps_altitude(exif);
    }
}

/// 只读图片头部得到尺寸，不全量解码（省内存）。无法识别返回 `None`。
fn decode_dimensions(bytes: &[u8]) -> Option<(u32, u32)> {
    ImageReader::new(Cursor::new(bytes))
        .with_guessed_format()
        .ok()?
        .into_dimensions()
        .ok()
}

/// 从容器读 EXIF；无 EXIF（如纯 PNG）或损坏返回 `None`。
fn read_exif(bytes: &[u8]) -> Option<Exif> {
    Reader::new()
        .read_from_container(&mut BufReader::new(Cursor::new(bytes)))
        .ok()
}

fn exif_ascii(exif: &Exif, tag: Tag) -> Option<String> {
    let field = exif.get_field(tag, In::PRIMARY)?;
    match &field.value {
        Value::Ascii(values) => {
            let text = values
                .iter()
                .flat_map(|chunk| chunk.iter().copied())
                .collect::<Vec<u8>>();
            let text = String::from_utf8_lossy(&text).trim().to_owned();
            if text.is_empty() { None } else { Some(text) }
        }
        _ => None,
    }
}

fn exif_u32(exif: &Exif, tag: Tag) -> Option<u32> {
    let field = exif.get_field(tag, In::PRIMARY)?;
    field.value.get_uint(0)
}

fn exif_first_rational(exif: &Exif, tag: Tag) -> Option<(u32, u32)> {
    let field = exif.get_field(tag, In::PRIMARY)?;
    match &field.value {
        Value::Rational(values) => values.first().map(|r| (r.num, r.denom)),
        _ => None,
    }
}

fn exif_rational_f64(exif: &Exif, tag: Tag) -> Option<f64> {
    exif_first_rational(exif, tag).and_then(|(num, denom)| {
        if denom == 0 {
            None
        } else {
            Some(f64::from(num) / f64::from(denom))
        }
    })
}

/// EXIF 日期 `YYYY:MM:DD HH:MM:SS` → 归一化 `YYYY-MM-DDTHH:MM:SS`。
fn exif_datetime(exif: &Exif, tag: Tag) -> Option<String> {
    let field = exif.get_field(tag, In::PRIMARY)?;
    match &field.value {
        Value::Ascii(values) => {
            let raw = values.first()?;
            let datetime = exif::DateTime::from_ascii(raw).ok()?;
            Some(format!(
                "{:04}-{:02}-{:02}T{:02}:{:02}:{:02}",
                datetime.year,
                datetime.month,
                datetime.day,
                datetime.hour,
                datetime.minute,
                datetime.second,
            ))
        }
        _ => None,
    }
}

/// GPS 经/纬度：3 个 rational（度/分/秒）+ 参考方向（N/S/E/W）→ 带符号十进制度。
fn exif_gps_coordinate(exif: &Exif, value_tag: Tag, ref_tag: Tag) -> Option<f64> {
    let field = exif.get_field(value_tag, In::PRIMARY)?;
    let parts = match &field.value {
        Value::Rational(values) if values.len() >= 3 => values,
        _ => return None,
    };
    let degrees = rational_to_f64(parts[0].num, parts[0].denom)?;
    let minutes = rational_to_f64(parts[1].num, parts[1].denom)?;
    let seconds = rational_to_f64(parts[2].num, parts[2].denom)?;

    let reference = exif
        .get_field(ref_tag, In::PRIMARY)
        .map(|field| field.display_value().to_string())
        .unwrap_or_default();
    let positive = !matches!(reference.trim().to_ascii_uppercase().as_str(), "S" | "W");

    Some(gps_to_decimal(degrees, minutes, seconds, positive))
}

/// GPS 海拔：rational 高度 + 参考（0=海平面以上，1=以下）。
fn exif_gps_altitude(exif: &Exif) -> Option<f64> {
    let altitude = exif_rational_f64(exif, Tag::GPSAltitude)?;
    let below_sea_level = exif_u32(exif, Tag::GPSAltitudeRef) == Some(1);
    Some(if below_sea_level { -altitude } else { altitude })
}

fn rational_to_f64(num: u32, denom: u32) -> Option<f64> {
    if denom == 0 {
        None
    } else {
        Some(f64::from(num) / f64::from(denom))
    }
}

/// 度/分/秒 → 十进制度，`positive=false`（南纬/西经）取负。纯函数，可独立测试。
pub fn gps_to_decimal(degrees: f64, minutes: f64, seconds: f64, positive: bool) -> f64 {
    let magnitude = degrees + minutes / 60.0 + seconds / 3600.0;
    if positive { magnitude } else { -magnitude }
}

/// 曝光时间 rational → 人类可读分数文本。纯函数，可独立测试。
///
/// - `denom == 0`：返回 `"0"`（防御非法值）。
/// - 整数秒（如 `2/1`）：返回 `"2"`。
/// - 小于 1 秒：返回 `"1/N"` 形式（如 `1/250`）。
pub fn format_exposure(num: u32, denom: u32) -> String {
    if denom == 0 {
        return "0".to_owned();
    }
    if num == 0 {
        return "0".to_owned();
    }
    if num % denom == 0 {
        return (num / denom).to_string();
    }
    // 归一化为 1/N（相机几乎总是以 1/N 记录快门）。
    let reciprocal = (f64::from(denom) / f64::from(num)).round() as u32;
    format!("1/{reciprocal}")
}

/// 启动图片提取后台 worker，仿照 `spawn_probe_worker`。
pub fn spawn_photo_worker(
    pool: DbPool,
    config: PhotoWorkerConfig,
    thumbnail_dir: PathBuf,
    mut shutdown: broadcast::Receiver<()>,
) -> JoinHandle<()> {
    tokio::spawn(async move {
        let service = PhotoService::new(pool, thumbnail_dir);
        let mut tick = interval(Duration::from_secs(config.interval_seconds));
        tick.set_missed_tick_behavior(MissedTickBehavior::Delay);

        info!(
            interval_seconds = config.interval_seconds,
            "photo worker started"
        );

        loop {
            tokio::select! {
                result = shutdown.recv() => {
                    match result {
                        Ok(()) | Err(broadcast::error::RecvError::Closed) => {
                            info!("photo worker shutdown received");
                            break;
                        }
                        Err(broadcast::error::RecvError::Lagged(skipped)) => {
                            warn!(skipped, "photo worker shutdown receiver lagged");
                            break;
                        }
                    }
                }
                _ = tick.tick() => {
                    run_available_jobs(&service).await;
                }
            }
        }

        info!("photo worker stopped");
    })
}

async fn run_available_jobs(service: &PhotoService) {
    loop {
        match service.run_next_photo_job().await {
            Ok(Some(summary)) => {
                info!(
                    job_id = %summary.job_id,
                    media_item_id = summary.media_item_id,
                    status = %summary.status,
                    "media photo job completed by background worker"
                );
            }
            Ok(None) => break,
            Err(err) => {
                warn!(error = %err, "photo worker failed to run job");
                break;
            }
        }
    }
}

/// 图片提取服务：认领 `media.photo` job，读文件、提取元数据、生成缩略图、落库。
#[derive(Clone)]
pub struct PhotoService {
    pool: DbPool,
    thumbnail_dir: PathBuf,
    worker_id: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PhotoJobOutcome {
    pub job_id: String,
    pub media_item_id: i64,
    pub status: String,
}

#[derive(Clone, Debug)]
struct ClaimedPhotoJob {
    id: i64,
    public_id: String,
    payload: JsonValue,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct PhotoJobRequest {
    media_item_id: i64,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct PhotoTarget {
    media_item_id: i64,
    path: String,
}

#[derive(Debug)]
pub enum PhotoError {
    JobNotFound,
    MissingMediaItemId,
    InvalidMediaItemId(String),
    MediaItemNotFound(i64),
    Database(sqlx::Error),
}

impl std::fmt::Display for PhotoError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            PhotoError::JobNotFound => write!(f, "photo job not found"),
            PhotoError::MissingMediaItemId => write!(f, "photo job payload missing mediaItemId"),
            PhotoError::InvalidMediaItemId(value) => {
                write!(f, "photo job payload has invalid mediaItemId: {value}")
            }
            PhotoError::MediaItemNotFound(id) => write!(f, "photo media item {id} not found"),
            PhotoError::Database(err) => write!(f, "database error: {err}"),
        }
    }
}

impl std::error::Error for PhotoError {}

impl PhotoService {
    pub fn new(pool: DbPool, thumbnail_dir: PathBuf) -> Self {
        Self {
            pool,
            thumbnail_dir,
            worker_id: PHOTO_WORKER_ID.to_owned(),
        }
    }

    pub async fn run_next_photo_job(&self) -> Result<Option<PhotoJobOutcome>, PhotoError> {
        let Some(job) = self.claim_photo_job(None).await? else {
            return Ok(None);
        };
        self.run_claimed_photo_job(job).await.map(Some)
    }

    async fn run_claimed_photo_job(
        &self,
        job: ClaimedPhotoJob,
    ) -> Result<PhotoJobOutcome, PhotoError> {
        let request = PhotoJobRequest::from_payload(&job.payload)?;
        let run_id = self.start_job_run(job.id).await?;
        self.record_job_event(
            job.id,
            Some(run_id),
            "media.photo.started",
            "info",
            "media photo extraction started",
            json!({ "mediaItemId": request.media_item_id }),
        )
        .await?;

        match self.extract_for_item(request.media_item_id).await {
            Ok(status) => {
                let outcome = PhotoJobOutcome {
                    job_id: job.public_id,
                    media_item_id: request.media_item_id,
                    status,
                };
                self.finish_job_success(job.id, run_id, &outcome).await?;
                Ok(outcome)
            }
            Err(err) => {
                let message = err.to_string();
                if let Err(event_err) = self
                    .record_job_event(
                        job.id,
                        Some(run_id),
                        "media.photo.failed",
                        "error",
                        &message,
                        json!({ "mediaItemId": request.media_item_id }),
                    )
                    .await
                {
                    warn!(error = %event_err, "failed to record photo failure event");
                }
                self.finish_job_failure(&job.public_id, job.id, run_id, &message)
                    .await?;
                Err(err)
            }
        }
    }

    /// 加载图片主文件 → 提取元数据 → 生成缩略图 → upsert media_photo_metadata。
    /// 文件缺失/坏图等可恢复情况返回 `skipped_*` 状态而非报错（不阻断队列）。
    async fn extract_for_item(&self, media_item_id: i64) -> Result<String, PhotoError> {
        let Some(target) = self.load_target(media_item_id).await? else {
            return Ok("skipped_no_file".to_owned());
        };

        let path = target.path.clone();
        let thumbnail_dir = self.thumbnail_dir.clone();
        // 解码/EXIF/缩放是 CPU 密集且阻塞，丢到 blocking 线程池。
        let extraction = tokio::task::spawn_blocking(move || {
            let Ok(bytes) = std::fs::read(&path) else {
                return None;
            };
            let metadata = PhotoMetadata::extract_from_bytes(&bytes);
            let thumbnail_path = generate_thumbnail(&bytes, &thumbnail_dir, media_item_id)
                .ok()
                .flatten();
            Some((metadata, thumbnail_path))
        })
        .await
        .map_err(|err| PhotoError::Database(sqlx::Error::Io(std::io::Error::other(err))))?;

        let Some((metadata, thumbnail_path)) = extraction else {
            return Ok("skipped_unavailable".to_owned());
        };

        self.upsert_metadata(media_item_id, &metadata, thumbnail_path.as_deref())
            .await?;
        Ok("extracted".to_owned())
    }

    async fn load_target(&self, media_item_id: i64) -> Result<Option<PhotoTarget>, PhotoError> {
        let row = sqlx::query(
            r#"
            select mi.id as media_item_id,
                   mf.path as path
            from media_items mi
            join media_files mf on mf.media_item_id = mi.id and mf.is_primary = true
            where mi.id = $1
              and mi.item_type = 'photo'
              and mi.is_deleted = false
            limit 1
            "#,
        )
        .bind(media_item_id)
        .fetch_optional(&self.pool)
        .await
        .map_err(PhotoError::Database)?;

        Ok(row
            .map(PhotoTarget::from_row)
            .transpose()
            .map_err(PhotoError::Database)?)
    }

    async fn upsert_metadata(
        &self,
        media_item_id: i64,
        metadata: &PhotoMetadata,
        thumbnail_path: Option<&str>,
    ) -> Result<(), PhotoError> {
        // metadata 写入与 media_items 派生字段回写放在同一事务：要么都成功，
        // 要么都回滚，避免缩略图/EXIF 已落但排序日期没写的半成品状态。
        let mut tx = self.pool.begin().await.map_err(PhotoError::Database)?;
        sqlx::query(
            r#"
            insert into media_photo_metadata (
                media_item_id, width, height, captured_at, camera_make, camera_model,
                lens_model, orientation, iso, f_number, exposure_time, focal_length,
                gps_latitude, gps_longitude, gps_altitude, thumbnail_path, extracted_at
            )
            values (
                $1, $2, $3,
                case when $4::text is null then null else $4::timestamptz end,
                $5, $6, $7, $8, $9, $10, $11, $12, $13, $14, $15, $16, now()
            )
            on conflict (media_item_id) do update set
                width = excluded.width,
                height = excluded.height,
                captured_at = excluded.captured_at,
                camera_make = excluded.camera_make,
                camera_model = excluded.camera_model,
                lens_model = excluded.lens_model,
                orientation = excluded.orientation,
                iso = excluded.iso,
                f_number = excluded.f_number,
                exposure_time = excluded.exposure_time,
                focal_length = excluded.focal_length,
                gps_latitude = excluded.gps_latitude,
                gps_longitude = excluded.gps_longitude,
                gps_altitude = excluded.gps_altitude,
                thumbnail_path = excluded.thumbnail_path,
                extracted_at = now()
            "#,
        )
        .bind(media_item_id)
        .bind(metadata.width.map(|v| v as i32))
        .bind(metadata.height.map(|v| v as i32))
        .bind(metadata.captured_at.as_deref())
        .bind(metadata.camera_make.as_deref())
        .bind(metadata.camera_model.as_deref())
        .bind(metadata.lens_model.as_deref())
        .bind(metadata.orientation.map(|v| v as i16))
        .bind(metadata.iso.map(|v| v as i32))
        .bind(metadata.f_number)
        .bind(metadata.exposure_time.as_deref())
        .bind(metadata.focal_length)
        .bind(metadata.gps_latitude)
        .bind(metadata.gps_longitude)
        .bind(metadata.gps_altitude)
        .bind(thumbnail_path)
        .execute(&mut *tx)
        .await
        .map_err(PhotoError::Database)?;

        // EXIF 拍摄时间回写到通用排序字段：让照片在 Emby 通用浏览里也能按
        // premiere_date / production_year 排序。captured_at 为空时保留原值（防御，
        // 不清空已有日期）；有值时拍摄时间是照片最权威的日期来源，直接覆盖。
        sqlx::query(
            r#"
            update media_items
            set premiere_date = case
                    when $2::text is null then premiere_date
                    else ($2::timestamptz)::date
                end,
                production_year = case
                    when $2::text is null then production_year
                    else extract(year from $2::timestamptz)::int
                end,
                updated_at = now()
            where id = $1
            "#,
        )
        .bind(media_item_id)
        .bind(metadata.captured_at.as_deref())
        .execute(&mut *tx)
        .await
        .map_err(PhotoError::Database)?;

        tx.commit().await.map_err(PhotoError::Database)?;
        Ok(())
    }

    async fn claim_photo_job(
        &self,
        job_id: Option<&str>,
    ) -> Result<Option<ClaimedPhotoJob>, PhotoError> {
        let mut tx = self.pool.begin().await.map_err(PhotoError::Database)?;
        expire_stale_running_jobs(
            &mut tx,
            MEDIA_PHOTO_JOB_TYPE,
            ExpiredJobMessages {
                retry: PHOTO_JOB_LEASE_EXPIRED_RETRY,
                final_failure: PHOTO_JOB_LEASE_EXPIRED_FINAL,
            },
        )
        .await
        .map_err(PhotoError::Database)?;

        let job = sqlx::query(PHOTO_CLAIM_JOB_SQL)
            .bind(job_id)
            .bind(MEDIA_PHOTO_JOB_TYPE)
            .bind(&self.worker_id)
            .bind(PHOTO_JOB_LEASE_SECONDS)
            .fetch_optional(&mut *tx)
            .await
            .map_err(PhotoError::Database)?
            .map(ClaimedPhotoJob::from_row)
            .transpose()
            .map_err(PhotoError::Database)?;

        tx.commit().await.map_err(PhotoError::Database)?;
        Ok(job)
    }

    async fn start_job_run(&self, job_id: i64) -> Result<i64, PhotoError> {
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
        .map_err(PhotoError::Database)
    }

    async fn finish_job_success(
        &self,
        job_id: i64,
        run_id: i64,
        outcome: &PhotoJobOutcome,
    ) -> Result<(), PhotoError> {
        let metrics = json!({
            "mediaItemId": outcome.media_item_id,
            "status": outcome.status,
        });
        let mut tx = self.pool.begin().await.map_err(PhotoError::Database)?;
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
        .await
        .map_err(PhotoError::Database)?;

        sqlx::query(
            r#"
            update jobs
            set status = 'succeeded', locked_by = null, locked_until = null,
                updated_at = now(), finished_at = now()
            where id = $1
            "#,
        )
        .bind(job_id)
        .execute(&mut *tx)
        .await
        .map_err(PhotoError::Database)?;

        tx.commit().await.map_err(PhotoError::Database)
    }

    async fn finish_job_failure(
        &self,
        job_public_id: &str,
        job_id: i64,
        run_id: i64,
        message: &str,
    ) -> Result<(), PhotoError> {
        let mut tx = self.pool.begin().await.map_err(PhotoError::Database)?;
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
        .await
        .map_err(PhotoError::Database)?;

        mark_job_failed(
            &mut tx,
            MEDIA_PHOTO_JOB_TYPE,
            job_public_id,
            job_id,
            message,
        )
        .await
        .map_err(PhotoError::Database)?;

        tx.commit().await.map_err(PhotoError::Database)
    }

    async fn record_job_event(
        &self,
        job_id: i64,
        run_id: Option<i64>,
        event_type: &str,
        event_level: &str,
        message: &str,
        payload: JsonValue,
    ) -> Result<(), PhotoError> {
        sqlx::query(
            r#"
            insert into job_events (
                job_id, job_run_id, event_type, event_level, message, payload
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
        .map_err(PhotoError::Database)?;
        Ok(())
    }
}

impl PhotoJobRequest {
    fn from_payload(payload: &JsonValue) -> Result<Self, PhotoError> {
        let Some(value) = payload.get("mediaItemId") else {
            return Err(PhotoError::MissingMediaItemId);
        };
        let media_item_id = value
            .as_i64()
            .or_else(|| value.as_str()?.trim().parse::<i64>().ok())
            .filter(|value| *value > 0)
            .ok_or_else(|| PhotoError::InvalidMediaItemId(value.to_string()))?;
        Ok(Self { media_item_id })
    }
}

impl ClaimedPhotoJob {
    fn from_row(row: PgRow) -> Result<Self, sqlx::Error> {
        Ok(Self {
            id: row.try_get("id")?,
            public_id: row.try_get("public_id")?,
            payload: row.try_get("payload")?,
        })
    }
}

impl PhotoTarget {
    fn from_row(row: PgRow) -> Result<Self, sqlx::Error> {
        Ok(Self {
            media_item_id: row.try_get("media_item_id")?,
            path: row.try_get("path")?,
        })
    }
}

/// 生成缩略图：等比缩放到最长边 `PHOTO_THUMBNAIL_MAX_EDGE`，落盘为 JPEG。
/// 返回缩略图路径；坏图返回 `Ok(None)`（不阻断元数据提取）。
fn generate_thumbnail(
    bytes: &[u8],
    thumbnail_dir: &std::path::Path,
    media_item_id: i64,
) -> std::io::Result<Option<String>> {
    let Ok(reader) = ImageReader::new(Cursor::new(bytes)).with_guessed_format() else {
        return Ok(None);
    };
    let Ok(image) = reader.decode() else {
        return Ok(None);
    };

    std::fs::create_dir_all(thumbnail_dir)?;
    let thumbnail = image.resize(
        PHOTO_THUMBNAIL_MAX_EDGE,
        PHOTO_THUMBNAIL_MAX_EDGE,
        FilterType::Lanczos3,
    );
    let output_path = thumbnail_dir.join(format!("{media_item_id}.jpg"));
    if thumbnail
        .to_rgb8()
        .save_with_format(&output_path, image::ImageFormat::Jpeg)
        .is_err()
    {
        return Ok(None);
    }
    Ok(Some(output_path.to_string_lossy().into_owned()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn gps_decimal_handles_hemisphere_sign() {
        // 北纬/东经为正。
        let north = gps_to_decimal(40.0, 26.0, 46.0, true);
        assert!((north - 40.446_111).abs() < 1e-5);
        // 南纬/西经取负。
        let south = gps_to_decimal(40.0, 26.0, 46.0, false);
        assert!((south + 40.446_111).abs() < 1e-5);
    }

    #[test]
    fn exposure_formats_common_shutter_speeds() {
        assert_eq!(format_exposure(1, 250), "1/250");
        assert_eq!(format_exposure(2, 1), "2"); // 2 秒长曝光
        assert_eq!(format_exposure(10, 500), "1/50"); // 未约分也归一化
        assert_eq!(format_exposure(0, 250), "0");
        assert_eq!(format_exposure(1, 0), "0"); // 非法分母不 panic
    }

    #[test]
    fn dimensions_extracted_from_encoded_png() {
        // 编码一张已知尺寸的 PNG，验证只读头部能拿到尺寸。
        let image = image::RgbImage::new(7, 13);
        let mut bytes = Cursor::new(Vec::new());
        image
            .write_to(&mut bytes, image::ImageFormat::Png)
            .expect("png encode should succeed");
        let metadata = PhotoMetadata::extract_from_bytes(bytes.get_ref());
        assert_eq!(metadata.width, Some(7));
        assert_eq!(metadata.height, Some(13));
        // 纯 PNG 无 EXIF，相机/拍摄时间应为空，且不 panic。
        assert_eq!(metadata.captured_at, None);
        assert_eq!(metadata.camera_model, None);
    }

    #[test]
    fn garbage_bytes_yield_empty_metadata_without_panic() {
        let metadata = PhotoMetadata::extract_from_bytes(b"not an image at all");
        assert_eq!(metadata, PhotoMetadata::default());
    }

    // Live-DB smoke: validates that upsert_metadata writes media_photo_metadata
    // AND writes the EXIF capture time back to media_items.premiere_date /
    // production_year in one transaction, and that a NULL captured_at preserves
    // any existing date instead of clearing it.
    //   cargo test -- --ignored photo_metadata_upsert_writes_back_premiere_date
    #[tokio::test]
    #[ignore = "requires a running PostgreSQL from ./scripts/dev-deps.ps1"]
    async fn photo_metadata_upsert_writes_back_premiere_date() {
        use sqlx::Row;
        use sqlx::postgres::PgPoolOptions;
        use std::time::{SystemTime, UNIX_EPOCH};

        let url = std::env::var("DATABASE_URL")
            .unwrap_or_else(|_| "postgres://fbz:fbz@127.0.0.1:5432/fbz".to_owned());
        let pool = PgPoolOptions::new()
            .max_connections(2)
            .connect(&url)
            .await
            .expect("connect to live PostgreSQL");
        crate::db::migrate(&pool).await.expect("run migrations");

        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        // Minimal homevideos library to host the photo item.
        let library_name = format!("photo-wb-lib-{nonce}");
        let library_id = sqlx::query_scalar::<_, i64>(
            r#"
            insert into libraries (name, library_type)
            values ($1, 'homevideos')
            returning id
            "#,
        )
        .bind(&library_name)
        .fetch_one(&pool)
        .await
        .expect("create homevideos library");
        let item_id = sqlx::query_scalar::<_, i64>(
            r#"
            insert into media_items (library_id, item_type, title, sort_title)
            values ($1, 'photo', 'wb-photo', 'wb-photo')
            returning id
            "#,
        )
        .bind(library_id)
        .fetch_one(&pool)
        .await
        .expect("insert photo item");

        let service = PhotoService::new(pool.clone(), std::env::temp_dir());

        // Captured time present → premiere_date + production_year written back.
        let with_date = PhotoMetadata {
            width: Some(4000),
            height: Some(3000),
            captured_at: Some("2021-07-15T08:30:00".to_owned()),
            ..Default::default()
        };
        service
            .upsert_metadata(item_id, &with_date, Some("/thumbs/x.jpg"))
            .await
            .expect("upsert with date should execute");
        let row = sqlx::query(
            "select premiere_date::text as d, production_year as y from media_items where id = $1",
        )
        .bind(item_id)
        .fetch_one(&pool)
        .await
        .expect("read back media item");
        assert_eq!(
            row.try_get::<Option<String>, _>("d").unwrap().as_deref(),
            Some("2021-07-15"),
            "capture date must write back to premiere_date"
        );
        assert_eq!(
            row.try_get::<Option<i32>, _>("y").unwrap(),
            Some(2021),
            "capture year must write back to production_year"
        );

        // captured_at NULL → existing premiere_date preserved (not cleared).
        let no_date = PhotoMetadata {
            width: Some(100),
            height: Some(100),
            captured_at: None,
            ..Default::default()
        };
        service
            .upsert_metadata(item_id, &no_date, None)
            .await
            .expect("upsert without date should execute");
        let preserved = sqlx::query_scalar::<_, Option<String>>(
            "select premiere_date::text from media_items where id = $1",
        )
        .bind(item_id)
        .fetch_one(&pool)
        .await
        .expect("read back preserved date");
        assert_eq!(
            preserved.as_deref(),
            Some("2021-07-15"),
            "NULL capture time must preserve existing premiere_date, not clear it"
        );

        // Cleanup (media_items + media_photo_metadata cascade via FK on library delete).
        sqlx::query("delete from libraries where id = $1")
            .bind(library_id)
            .execute(&pool)
            .await
            .expect("cleanup smoke library");
    }
}
