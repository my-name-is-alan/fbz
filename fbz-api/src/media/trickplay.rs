//! Trickplay（进度条缩略图）生成与读取，形状对齐 Jellyfin：
//! `{cache}/trickplay/{item_id}/{width}/{index}.jpg`（10×10 拼图，index 从 0）+ `manifest.json`
//! （`TrickplayInfo`：Width/Height/TileWidth/TileHeight/ThumbnailCount/Interval/Bandwidth）。
//! 生成走 ffmpeg：`fps=1/interval,scale,tile`，先写临时目录再原子改名。

use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::time::Duration;

use serde::{Deserialize, Serialize};

pub const DEFAULT_WIDTH: u32 = 320;
pub const DEFAULT_INTERVAL_MS: u32 = 10_000;
pub const TILE_COLUMNS: u32 = 10;
pub const TILE_ROWS: u32 = 10;
pub const ALLOWED_WIDTHS: [u32; 3] = [160, 320, 480];

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "PascalCase")]
pub struct TrickplayInfo {
    pub width: u32,
    pub height: u32,
    pub tile_width: u32,
    pub tile_height: u32,
    pub thumbnail_count: u32,
    pub interval: u32,
    pub bandwidth: u32,
}

impl TrickplayInfo {
    pub fn tiles_per_image(&self) -> u32 {
        (self.tile_width * self.tile_height).max(1)
    }

    pub fn image_count(&self) -> u32 {
        self.thumbnail_count.div_ceil(self.tiles_per_image())
    }
}

/// 根据视频分辨率与时长规划生成参数。
pub fn plan(width: u32, source_width: Option<i32>, source_height: Option<i32>, runtime_ticks: Option<i64>) -> Option<TrickplayInfo> {
    let runtime_ms = runtime_ticks.filter(|t| *t > 0)? / 10_000;
    let height = match (source_width, source_height) {
        (Some(w), Some(h)) if w > 0 && h > 0 => {
            let raw = (f64::from(width) * f64::from(h) / f64::from(w)).round() as u32;
            (raw + (raw & 1)).max(2)
        }
        _ => width * 9 / 16 + (width * 9 / 16) % 2,
    };
    let count = u32::try_from(runtime_ms.div_euclid(i64::from(DEFAULT_INTERVAL_MS)) + 1).ok()?;
    Some(TrickplayInfo {
        width,
        height,
        tile_width: TILE_COLUMNS,
        tile_height: TILE_ROWS,
        thumbnail_count: count,
        interval: DEFAULT_INTERVAL_MS,
        // 粗估码率（bit/s）：每缩略图约 width*height/10 字节。
        bandwidth: width * height * 8 / 10 / (DEFAULT_INTERVAL_MS / 1000),
    })
}

pub fn root_dir(cache_root: &Path, item_id: &str) -> PathBuf {
    cache_root.join("trickplay").join(sanitize(item_id))
}

pub fn width_dir(cache_root: &Path, item_id: &str, width: u32) -> PathBuf {
    root_dir(cache_root, item_id).join(width.to_string())
}

fn sanitize(value: &str) -> String {
    value.chars().filter(|c| c.is_ascii_alphanumeric() || *c == '-' || *c == '_').collect()
}

pub fn ffmpeg_args(media_path: &Path, info: &TrickplayInfo, out_dir: &Path) -> Vec<OsString> {
    let filter = format!(
        "fps=1000/{}:start_time=0:round=up,scale={}:{},tile={}x{},format=yuvj420p",
        info.interval, info.width, info.height, info.tile_width, info.tile_height
    );
    let mut args: Vec<OsString> = ["-hide_banner", "-nostdin", "-y", "-loglevel", "error", "-i"]
        .iter()
        .map(OsString::from)
        .collect();
    args.push(media_path.as_os_str().to_owned());
    for a in ["-an", "-sn", "-dn", "-vf"] {
        args.push(a.into());
    }
    args.push(filter.into());
    for a in ["-q:v", "5", "-start_number", "0"] {
        args.push(a.into());
    }
    args.push(out_dir.join("%d.jpg").into_os_string());
    args
}

/// JF 兼容 `tiles.m3u8`（EXT-X-IMAGES-ONLY）。`query` 原样拼到每张图（携带 api_key）。
pub fn m3u8(info: &TrickplayInfo, query: &str) -> String {
    let per = info.tiles_per_image();
    let interval_s = f64::from(info.interval) / 1000.0;
    let mut out = format!(
        "#EXTM3U\n#EXT-X-TARGETDURATION:{}\n#EXT-X-VERSION:7\n#EXT-X-MEDIA-SEQUENCE:1\n#EXT-X-PLAYLIST-TYPE:VOD\n#EXT-X-IMAGES-ONLY\n",
        (interval_s * f64::from(per)).ceil() as u64
    );
    let suffix = if query.is_empty() { String::new() } else { format!("?{query}") };
    for i in 0..info.image_count() {
        let tiles = (info.thumbnail_count - i * per).min(per);
        let cols = info.tile_width.min(tiles);
        let rows = tiles.div_ceil(info.tile_width);
        out.push_str(&format!(
            "\n#EXTINF:{:.3},\n#EXT-X-TILES:RESOLUTION={}x{},LAYOUT={}x{},DURATION={:.3}\n{i}.jpg{suffix}\n",
            interval_s * f64::from(tiles),
            info.width,
            info.height,
            cols,
            rows,
            interval_s
        ));
    }
    out.push_str("\n#EXT-X-ENDLIST\n");
    out
}

pub async fn read_manifest(dir: &Path) -> Option<TrickplayInfo> {
    let bytes = tokio::fs::read(dir.join("manifest.json")).await.ok()?;
    serde_json::from_slice(&bytes).ok()
}

/// 列出已生成的宽度 → 信息（item DTO `Trickplay` 字段用）。
pub async fn available(cache_root: &Path, item_id: &str) -> Vec<TrickplayInfo> {
    let mut out = Vec::new();
    for w in ALLOWED_WIDTHS {
        if let Some(info) = read_manifest(&width_dir(cache_root, item_id, w)).await {
            out.push(info);
        }
    }
    out
}

/// 运行 ffmpeg 生成（超时按时长放宽，至少 10 分钟）。
pub async fn generate(ffmpeg: &Path, media_path: &Path, info: &TrickplayInfo, cache_root: &Path, item_id: &str) -> Result<(), String> {
    let final_dir = width_dir(cache_root, item_id, info.width);
    let tmp_dir = final_dir.with_extension("tmp");
    let _ = tokio::fs::remove_dir_all(&tmp_dir).await;
    tokio::fs::create_dir_all(&tmp_dir).await.map_err(|e| format!("mkdir: {e}"))?;
    let mut command = tokio::process::Command::new(ffmpeg);
    command
        .args(ffmpeg_args(media_path, info, &tmp_dir))
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .kill_on_drop(true);
    let mut child = command.spawn().map_err(|e| format!("spawn ffmpeg: {e}"))?;
    let budget = Duration::from_secs(600 + u64::from(info.thumbnail_count) / 2);
    let status = tokio::time::timeout(budget, child.wait())
        .await
        .map_err(|_| "ffmpeg timed out".to_owned())?
        .map_err(|e| format!("ffmpeg wait: {e}"))?;
    if !status.success() {
        let _ = tokio::fs::remove_dir_all(&tmp_dir).await;
        return Err(format!("ffmpeg exited with {status}"));
    }
    // 短片 / 异常时间戳下 ffmpeg 可能成功退出却不产出图片：视为失败而不是写空 manifest。
    if !tokio::fs::try_exists(tmp_dir.join("0.jpg")).await.unwrap_or(false) {
        let _ = tokio::fs::remove_dir_all(&tmp_dir).await;
        return Err("ffmpeg produced no trickplay images".to_owned());
    }
    let manifest = serde_json::to_vec(info).map_err(|e| e.to_string())?;
    tokio::fs::write(tmp_dir.join("manifest.json"), manifest).await.map_err(|e| format!("write manifest: {e}"))?;
    let _ = tokio::fs::remove_dir_all(&final_dir).await;
    tokio::fs::rename(&tmp_dir, &final_dir).await.map_err(|e| format!("rename: {e}"))?;
    Ok(())
}

// ---------------------------------------------------------------------------
// 运行时配置（main 启动时 install）、进度状态、计划任务批量生成。
// ---------------------------------------------------------------------------

use std::collections::HashMap;
use std::sync::{Mutex, OnceLock};

/// 计划任务 key / type（`ScheduledTasks` 可见，可 Run-now）。
pub const TRICKPLAY_TASK_KEY: &str = "core.trickplay.generate";
pub const TRICKPLAY_TASK_TYPE: &str = "trickplay.generate";

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Runtime {
    pub cache_root: PathBuf,
    pub ffmpeg: PathBuf,
    /// `TRICKPLAY_AUTO_GENERATE`（默认 false）：计划任务是否启用。
    pub auto_generate: bool,
    /// `TRICKPLAY_SCHEDULE`（默认 `24h`，格式同 `SCHEDULE_*`）。
    pub schedule: String,
    /// `TRICKPLAY_MAX_PER_RUN`（默认 10）：每次任务最多生成条目数。
    pub max_per_run: i64,
    /// `TRICKPLAY_WIDTH`（默认 320，限 160/320/480）。
    pub width: u32,
    /// `TRICKPLAY_AFTER_SCAN`（默认随 `TRICKPLAY_AUTO_GENERATE`）：扫描后的 probe 批次排空即增量生成。
    pub after_scan: bool,
}

impl Runtime {
    pub fn from_lookup(cache_root: PathBuf, ffmpeg: PathBuf, get: impl Fn(&str) -> Option<String>) -> Self {
        let flag = |k: &str| get(k).is_some_and(|v| matches!(v.trim().to_ascii_lowercase().as_str(), "1" | "true" | "yes" | "on"));
        let width = get("TRICKPLAY_WIDTH")
            .and_then(|v| v.trim().parse::<u32>().ok())
            .filter(|w| ALLOWED_WIDTHS.contains(w))
            .unwrap_or(DEFAULT_WIDTH);
        Self {
            cache_root,
            ffmpeg,
            auto_generate: flag("TRICKPLAY_AUTO_GENERATE"),
            schedule: get("TRICKPLAY_SCHEDULE").filter(|v| !v.trim().is_empty()).unwrap_or_else(|| "24h".to_owned()),
            max_per_run: get("TRICKPLAY_MAX_PER_RUN").and_then(|v| v.trim().parse().ok()).filter(|n: &i64| *n > 0).unwrap_or(10),
            width,
            after_scan: match get("TRICKPLAY_AFTER_SCAN") {
                Some(_) => flag("TRICKPLAY_AFTER_SCAN"),
                None => flag("TRICKPLAY_AUTO_GENERATE"),
            },
        }
    }
}

static AFTER_SCAN_RUNNING: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

/// 扫描后入队：在后台跑一轮增量生成（与计划任务同一选择逻辑，单飞：已有一轮在跑则跳过）。
/// 返回是否真正启动了一轮。
pub fn trigger_after_scan(pool: crate::db::DbPool) -> bool {
    use std::sync::atomic::Ordering;
    let Some(rt) = runtime() else { return false };
    if !rt.after_scan {
        return false;
    }
    if AFTER_SCAN_RUNNING.swap(true, Ordering::SeqCst) {
        return false;
    }
    tokio::spawn(async move {
        match run_scheduled(&pool, rt).await {
            Ok(n) => tracing::info!(generated = n, "trickplay after-scan generation finished"),
            Err(err) => tracing::warn!(error = %err, "trickplay after-scan generation failed"),
        }
        AFTER_SCAN_RUNNING.store(false, Ordering::SeqCst);
    });
    true
}

static RUNTIME: OnceLock<Runtime> = OnceLock::new();

pub fn install(runtime: Runtime) {
    let _ = RUNTIME.set(runtime);
}

pub fn runtime() -> Option<&'static Runtime> {
    RUNTIME.get()
}

#[derive(Clone, Debug, Serialize, serde::Deserialize, PartialEq, Eq)]
#[serde(rename_all = "PascalCase")]
pub struct JobStatus {
    /// Queued / Running / Completed / Failed
    pub state: String,
    pub width: u32,
    pub error: Option<String>,
    pub updated_at_ms: u64,
}

fn statuses() -> &'static Mutex<HashMap<String, JobStatus>> {
    static STATUS: OnceLock<Mutex<HashMap<String, JobStatus>>> = OnceLock::new();
    STATUS.get_or_init(Default::default)
}

pub fn set_status(item_id: &str, width: u32, state: &str, error: Option<String>) {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or_default();
    let job = JobStatus { state: state.to_owned(), width, error, updated_at_ms: now };
    if let Some(rt) = RUNTIME.get() {
        persist_status(&rt.cache_root, item_id, &job);
    }
    statuses().lock().expect("trickplay status lock").insert(item_id.to_owned(), job);
}

/// 状态落盘：`{cache_root}/{item}/status.json`（tmp+rename），重启后仍可查询。
pub fn persist_status(cache_root: &Path, item_id: &str, job: &JobStatus) {
    let dir = root_dir(cache_root, item_id);
    if std::fs::create_dir_all(&dir).is_err() {
        return;
    }
    let Ok(bytes) = serde_json::to_vec(job) else { return };
    let tmp = dir.join("status.json.tmp");
    if std::fs::write(&tmp, bytes).is_ok() {
        let _ = std::fs::rename(&tmp, dir.join("status.json"));
    }
}

/// 读落盘状态；进程重启前处于 Queued/Running 的任务已中断，标为 `Interrupted`（计划任务会重试）。
pub fn load_persisted_status(cache_root: &Path, item_id: &str) -> Option<JobStatus> {
    let bytes = std::fs::read(root_dir(cache_root, item_id).join("status.json")).ok()?;
    let mut job: JobStatus = serde_json::from_slice(&bytes).ok()?;
    if job.state == "Running" || job.state == "Queued" {
        job.state = "Interrupted".to_owned();
    }
    Some(job)
}

pub fn status(item_id: &str) -> Option<JobStatus> {
    if let Some(job) = statuses().lock().expect("trickplay status lock").get(item_id).cloned() {
        return Some(job);
    }
    let job = load_persisted_status(&RUNTIME.get()?.cache_root, item_id)?;
    statuses().lock().expect("trickplay status lock").insert(item_id.to_owned(), job.clone());
    Some(job)
}

/// 生成并记录进度状态。
pub async fn generate_tracked(ffmpeg: &Path, media_path: &Path, info: &TrickplayInfo, cache_root: &Path, item_id: &str) -> Result<(), String> {
    set_status(item_id, info.width, "Running", None);
    let result = generate(ffmpeg, media_path, info, cache_root, item_id).await;
    match &result {
        Ok(()) => set_status(item_id, info.width, "Completed", None),
        Err(err) => set_status(item_id, info.width, "Failed", Some(err.clone())),
    }
    result
}

/// 计划任务：挑选尚无该宽度 trickplay 的视频（电影/剧集，非 STRM，时长已知）批量生成。
/// 返回成功生成条数。
pub async fn run_scheduled(pool: &crate::db::DbPool, rt: &Runtime) -> Result<i64, String> {
    use sqlx::Row;
    let rows = sqlx::query(
        r#"
        select mi.public_id::text as item_id,
               mf.path,
               coalesce(mi.runtime_ticks, mf.duration_ticks) as runtime_ticks,
               vs.width,
               vs.height
        from media_items mi
        join media_files mf on mf.media_item_id = mi.id and mf.is_strm = false
        left join lateral (
            select ms.width, ms.height from media_streams ms
            where ms.media_file_id = mf.id and ms.stream_type = 'video'
            order by ms.stream_index limit 1
        ) vs on true
        where mi.is_deleted = false
          and mi.item_type in ('movie', 'episode', 'video', 'music_video')
          and coalesce(mi.runtime_ticks, mf.duration_ticks, 0) > 0
        order by mi.id desc
        limit 2000
        "#,
    )
    .fetch_all(pool)
    .await
    .map_err(|e| format!("trickplay candidates: {e}"))?;

    let mut done = 0i64;
    for row in rows {
        if done >= rt.max_per_run {
            break;
        }
        let item_id: String = row.try_get("item_id").map_err(|e| e.to_string())?;
        if read_manifest(&width_dir(&rt.cache_root, &item_id, rt.width)).await.is_some() {
            continue;
        }
        if status(&item_id).is_some_and(|s| s.state == "Failed" || s.state == "Running") {
            continue; // 失败的不在每轮重试；运行中的不重复跑
        }
        let path: String = row.try_get("path").map_err(|e| e.to_string())?;
        let runtime_ticks: Option<i64> = row.try_get("runtime_ticks").ok();
        let (w, h): (Option<i32>, Option<i32>) = (row.try_get("width").ok().flatten(), row.try_get("height").ok().flatten());
        let Some(info) = plan(rt.width, w, h, runtime_ticks) else { continue };
        set_status(&item_id, rt.width, "Queued", None);
        match generate_tracked(&rt.ffmpeg, Path::new(path.trim()), &info, &rt.cache_root, &item_id).await {
            Ok(()) => done += 1,
            Err(err) => tracing::warn!(item_id = %item_id, error = %err, "scheduled trickplay generation failed"),
        }
    }
    Ok(done)
}

/// JF item 字段 `Trickplay`: `{ mediaSourceId: { "width": TrickplayInfo } }`。
pub fn item_field(media_source_ids: &[String], infos: &[TrickplayInfo]) -> Option<serde_json::Value> {
    if infos.is_empty() || media_source_ids.is_empty() {
        return None;
    }
    let per_width: serde_json::Map<String, serde_json::Value> = infos
        .iter()
        .map(|i| (i.width.to_string(), serde_json::to_value(i).unwrap_or_default()))
        .collect();
    Some(serde_json::Value::Object(
        media_source_ids.iter().map(|id| (id.clone(), serde_json::Value::Object(per_width.clone()))).collect(),
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn plan_computes_even_height_and_count() {
        let info = plan(320, Some(1920), Some(800), Some(3_600 * 10_000_000)).unwrap();
        assert_eq!(info.height, 134);
        assert_eq!(info.thumbnail_count, 361);
        assert_eq!(info.image_count(), 4);
        assert_eq!(plan(320, None, None, Some(10_000_000)).unwrap().height, 180);
        assert!(plan(320, None, None, None).is_none());
    }

    #[test]
    fn m3u8_lists_each_tile_image_with_layout() {
        let info = TrickplayInfo { width: 320, height: 180, tile_width: 10, tile_height: 10, thumbnail_count: 105, interval: 10_000, bandwidth: 1 };
        let text = m3u8(&info, "api_key=t");
        assert!(text.starts_with("#EXTM3U\n"));
        assert!(text.contains("#EXT-X-IMAGES-ONLY"));
        assert!(text.contains("LAYOUT=10x10,DURATION=10.000\n0.jpg?api_key=t"));
        assert!(text.contains("LAYOUT=5x1,DURATION=10.000\n1.jpg?api_key=t"));
        assert!(text.trim_end().ends_with("#EXT-X-ENDLIST"));
    }

    #[test]
    fn ffmpeg_args_use_tile_filter_and_zero_index() {
        let info = plan(320, Some(1280), Some(720), Some(600_000_000)).unwrap();
        let args: Vec<String> = ffmpeg_args(Path::new("/m/a.mkv"), &info, Path::new("/o"))
            .into_iter()
            .map(|a| a.into_string().unwrap())
            .collect();
        assert!(args.contains(&"fps=1000/10000:start_time=0:round=up,scale=320:180,tile=10x10,format=yuvj420p".to_owned()));
        assert!(args.windows(2).any(|w| w == ["-start_number", "0"]));
        assert_eq!(args.last().unwrap(), "/o/%d.jpg");
    }

    #[test]
    fn paths_are_sanitized() {
        let p = width_dir(Path::new("/c"), "../ab-1", 320);
        assert_eq!(p, PathBuf::from("/c/trickplay/ab-1/320"));
    }

    #[tokio::test]
    async fn manifest_roundtrip_and_available() {
        let root = std::env::temp_dir().join(format!("fbz-tp-{}", std::process::id()));
        let dir = width_dir(&root, "item", 320);
        tokio::fs::create_dir_all(&dir).await.unwrap();
        let info = plan(320, None, None, Some(1_000_000_000)).unwrap();
        tokio::fs::write(dir.join("manifest.json"), serde_json::to_vec(&info).unwrap()).await.unwrap();
        assert_eq!(available(&root, "item").await, vec![info]);
        let _ = tokio::fs::remove_dir_all(&root).await;
    }

    /// 本机有 ffmpeg 时真跑一次生成（无 ffmpeg 则跳过）。
    #[tokio::test]
    async fn generate_with_real_ffmpeg_when_available() {
        let ffmpeg = Path::new("ffmpeg");
        if std::process::Command::new(ffmpeg).arg("-version").output().is_err() {
            return;
        }
        let root = std::env::temp_dir().join(format!("fbz-tpgen-{}", std::process::id()));
        tokio::fs::create_dir_all(&root).await.unwrap();
        let src = root.join("src.mp4");
        let ok = std::process::Command::new(ffmpeg)
            .args(["-hide_banner", "-loglevel", "error", "-y", "-f", "lavfi", "-i", "testsrc=size=640x360:rate=5:duration=25"])
            .arg(&src)
            .status()
            .unwrap()
            .success();
        assert!(ok);
        let info = plan(160, Some(640), Some(360), Some(25 * 10_000_000)).unwrap();
        generate(ffmpeg, &src, &info, &root, "vid").await.unwrap();
        let dir = width_dir(&root, "vid", 160);
        assert!(dir.join("0.jpg").is_file());

        // 2 秒短片（实测：无 start_time/round=up 时 ffmpeg 不出图）
        let short = root.join("short.mp4");
        assert!(std::process::Command::new(ffmpeg)
            .args(["-hide_banner", "-loglevel", "error", "-y", "-f", "lavfi", "-i", "testsrc=size=640x360:rate=24:duration=2"])
            .arg(&short)
            .status()
            .unwrap()
            .success());
        let info = plan(160, Some(640), Some(360), Some(2 * 10_000_000)).unwrap();
        generate(ffmpeg, &short, &info, &root, "short").await.unwrap();
        assert!(width_dir(&root, "short", 160).join("0.jpg").is_file());
        assert_eq!(read_manifest(&dir).await.unwrap().height, 90);
        let _ = tokio::fs::remove_dir_all(&root).await;
    }

    #[test]
    fn runtime_reads_env_style_lookup() {
        let env: HashMap<&str, &str> = [("TRICKPLAY_AUTO_GENERATE", "true"), ("TRICKPLAY_WIDTH", "480"), ("TRICKPLAY_MAX_PER_RUN", "3")].into();
        let rt = Runtime::from_lookup("/c".into(), "ffmpeg".into(), |k| env.get(k).map(|v| v.to_string()));
        assert!(rt.auto_generate);
        assert_eq!((rt.width, rt.max_per_run, rt.schedule.as_str()), (480, 3, "24h"));
        assert!(rt.after_scan, "after_scan defaults to auto_generate");
        let rt2 = Runtime::from_lookup("/c".into(), "ffmpeg".into(), |k| match k {
            "TRICKPLAY_AUTO_GENERATE" => Some("true".to_owned()),
            "TRICKPLAY_AFTER_SCAN" => Some("false".to_owned()),
            _ => None,
        });
        assert!(rt2.auto_generate && !rt2.after_scan);
        let rt = Runtime::from_lookup("/c".into(), "ffmpeg".into(), |k| (k == "TRICKPLAY_WIDTH").then(|| "999".to_owned()));
        assert!(!rt.auto_generate);
        assert_eq!(rt.width, 320);
    }

    #[test]
    fn persisted_status_survives_restart_and_marks_interrupted() {
        let root = std::env::temp_dir().join(format!("fbz-tp-status-{}", std::process::id()));
        let job = |state: &str| JobStatus { state: state.into(), width: 320, error: None, updated_at_ms: 1 };
        persist_status(&root, "item-a", &job("Completed"));
        assert_eq!(load_persisted_status(&root, "item-a").unwrap().state, "Completed");
        persist_status(&root, "item-a", &job("Running"));
        assert_eq!(load_persisted_status(&root, "item-a").unwrap().state, "Interrupted");
        assert!(load_persisted_status(&root, "missing").is_none());
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn status_tracking_and_item_field_shape() {
        set_status("st-1", 320, "Running", None);
        assert_eq!(status("st-1").unwrap().state, "Running");
        set_status("st-1", 320, "Failed", Some("boom".into()));
        assert_eq!(status("st-1").unwrap().error.as_deref(), Some("boom"));
        assert!(status("nope").is_none());

        let info = plan(320, None, None, Some(600_000_000)).unwrap();
        let v = item_field(&["ms1".into()], &[info]).unwrap();
        assert_eq!(v["ms1"]["320"]["Width"], 320);
        assert_eq!(v["ms1"]["320"]["TileWidth"], 10);
        assert!(item_field(&["ms1".into()], &[]).is_none());
    }
}
