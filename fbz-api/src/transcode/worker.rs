use std::{path::PathBuf, time::Duration};

use serde_json::{Value, json};
use tokio::{
    process::Command,
    sync::broadcast,
    task::JoinHandle,
    time::{MissedTickBehavior, interval},
};
use tracing::{info, warn};

use crate::{
    admin::repository::AdminRepository,
    config::{TranscodeConfig, TranscodeWorkerConfig},
    db::DbPool,
    media::tools::MediaToolDiagnostics,
    plugins::hooks::{PluginHookDispatcher, PluginHookEvent},
    transcode::{
        cleanup::cleanup_session_output_dir_best_effort,
        planner::{FfmpegPlan, TranscodePlanError, TranscodeTuning, build_ffmpeg_plan_with_tuning},
        repository::{TranscodeClaimOutcome, TranscodeClaimRecord, TranscodeRepository},
        service::TranscodeQueueService,
    },
};

const TRANSCODE_STARTED_EVENT: &str = "transcode.started";
const TRANSCODE_COMPLETED_EVENT: &str = "transcode.completed";
const TRANSCODE_FAILED_EVENT: &str = "transcode.failed";

pub fn spawn_transcode_worker(
    pool: DbPool,
    transcode_config: TranscodeConfig,
    worker_config: TranscodeWorkerConfig,
    transcode_cache_dir: PathBuf,
    media_tools: MediaToolDiagnostics,
    mut shutdown: broadcast::Receiver<()>,
) -> JoinHandle<()> {
    tokio::spawn(async move {
        let worker_id = crate::transcode::service::transcode_worker_id("transcode");
        let service = TranscodeQueueService::new(pool.clone(), transcode_config.clone());
        let repository = TranscodeRepository::new(pool.clone());
        let admin_repository = AdminRepository::new(pool.clone());
        let hook_dispatcher = PluginHookDispatcher::new(pool);
        let mut tick = interval(Duration::from_secs(worker_config.interval_seconds));
        tick.set_missed_tick_behavior(MissedTickBehavior::Delay);

        info!(
            worker_id = %worker_id,
            interval_seconds = worker_config.interval_seconds,
            max_concurrent = transcode_config.max_concurrent,
            lease_seconds = transcode_config.lease_seconds,
            "transcode worker started"
        );

        loop {
            tokio::select! {
                result = shutdown.recv() => {
                    match result {
                        Ok(()) | Err(broadcast::error::RecvError::Closed) => {
                            info!("transcode worker shutdown received");
                            break;
                        }
                        Err(broadcast::error::RecvError::Lagged(skipped)) => {
                            warn!(skipped, "transcode worker shutdown receiver lagged");
                            break;
                        }
                    }
                }
                _ = tick.tick() => {
                    run_once(&service, &repository, &admin_repository, &hook_dispatcher, &transcode_config, &transcode_cache_dir, &media_tools, &worker_id).await;
                }
            }
        }

        info!("transcode worker stopped");
    })
}

#[allow(clippy::too_many_arguments)]
async fn run_once(
    service: &TranscodeQueueService,
    repository: &TranscodeRepository,
    admin_repository: &AdminRepository,
    hook_dispatcher: &PluginHookDispatcher,
    transcode_config: &TranscodeConfig,
    transcode_cache_dir: &PathBuf,
    media_tools: &MediaToolDiagnostics,
    worker_id: &str,
) {
    match service.claim_next(worker_id).await {
        Ok(TranscodeClaimOutcome::Claimed(session)) => {
            // 管理端「转码设置」按会话读取（单行表，读取成本可忽略），改配置即时生效。
            let tuning = match admin_repository.get_transcode_settings().await {
                Ok(Some(settings)) => TranscodeTuning::from_settings(
                    &settings.hardware_acceleration,
                    &settings.preferred_encoder,
                    &settings.max_resolution,
                    settings.segment_duration,
                    settings.throttle,
                ),
                Ok(None) => TranscodeTuning::default(),
                Err(err) => {
                    warn!(error = %err, "failed to load transcode settings; using defaults");
                    TranscodeTuning::default()
                }
            };
            dispatch_transcode_hook(
                hook_dispatcher,
                transcode_hook_event(TRANSCODE_STARTED_EVENT, "running", &session, None),
            )
            .await;
            if let Err(err) = execute_claimed_session(
                repository,
                transcode_config,
                media_tools,
                &tuning,
                session.clone(),
            )
            .await
            {
                let cleanup_reason = transcode_cleanup_reason(&err);
                let message = err.to_string();
                warn!(
                    session_id = %session.id,
                    user_id = %session.user_id,
                    item_id = %session.item_id,
                    media_file_id = ?session.media_file_id,
                    worker_id = %session.worker_id,
                    attempts = session.attempts,
                    max_attempts = session.max_attempts,
                    hardware_acceleration = ?session.hardware_acceleration,
                    video_codec = ?session.video_codec,
                    audio_codec = ?session.audio_codec,
                    container = ?session.container,
                    bitrate = ?session.bitrate,
                    error = %message,
                    "transcode session failed"
                );
                cleanup_session_output_dir_best_effort(
                    transcode_cache_dir,
                    &session.id,
                    session.output_path.as_deref(),
                    cleanup_reason,
                )
                .await;
                match repository.mark_failed(&session.id, &message).await {
                    Ok(true) => {
                        dispatch_transcode_hook(
                            hook_dispatcher,
                            transcode_hook_event(
                                TRANSCODE_FAILED_EVENT,
                                "failed",
                                &session,
                                Some(&message),
                            ),
                        )
                        .await;
                    }
                    Ok(false) => {
                        warn!(
                            session_id = %session.id,
                            "transcode session was no longer running when marking as failed"
                        );
                    }
                    Err(update_err) => {
                        warn!(
                            session_id = %session.id,
                            error = %update_err,
                            "failed to mark transcode session as failed"
                        );
                    }
                }
            } else {
                dispatch_transcode_hook(
                    hook_dispatcher,
                    transcode_hook_event(TRANSCODE_COMPLETED_EVENT, "succeeded", &session, None),
                )
                .await;
            }
        }
        Ok(TranscodeClaimOutcome::AtCapacity) => {
            info!("transcode worker at configured capacity");
        }
        Ok(TranscodeClaimOutcome::NoQueuedSession) => {}
        Err(err) => {
            warn!(error = %err, "transcode worker failed to claim session");
        }
    }
}

fn transcode_cleanup_reason(error: &TranscodeWorkerError) -> &'static str {
    match error {
        TranscodeWorkerError::SessionUpdateLost(_) => "transcode_update_lost",
        _ => "transcode_failed",
    }
}

async fn dispatch_transcode_hook(dispatcher: &PluginHookDispatcher, event: PluginHookEvent) {
    let event_key = event.event_key.clone();
    let session_id = event.aggregate_id.clone();
    if let Err(err) = dispatcher.dispatch(event).await {
        warn!(
            error = %err,
            event_key = %event_key,
            session_id = %session_id,
            "failed to dispatch plugin transcode hooks"
        );
    }
}

fn transcode_hook_event(
    event_key: &'static str,
    status: &'static str,
    session: &TranscodeClaimRecord,
    error_message: Option<&str>,
) -> PluginHookEvent {
    PluginHookEvent {
        event_key: event_key.to_owned(),
        aggregate_type: "transcoding_session".to_owned(),
        aggregate_id: session.id.clone(),
        payload: transcode_hook_payload(status, session, error_message),
    }
}

fn transcode_hook_payload(
    status: &'static str,
    session: &TranscodeClaimRecord,
    error_message: Option<&str>,
) -> Value {
    let mut payload = json!({
        "sessionId": &session.id,
        "userId": &session.user_id,
        "itemId": &session.item_id,
        "mediaSourceId": session.media_file_id.map(|id| id.to_string()),
        "status": status,
        "workerId": &session.worker_id,
        "attempts": session.attempts,
        "maxAttempts": session.max_attempts,
        "hardwareAcceleration": session.hardware_acceleration.as_deref(),
        "videoCodec": session.video_codec.as_deref(),
        "audioCodec": session.audio_codec.as_deref(),
        "container": session.container.as_deref(),
        "bitrate": session.bitrate,
    });
    if let Some(message) = error_message {
        payload["error"] = json!(message);
    }
    payload
}

async fn execute_claimed_session(
    repository: &TranscodeRepository,
    transcode_config: &TranscodeConfig,
    media_tools: &MediaToolDiagnostics,
    tuning: &TranscodeTuning,
    session: TranscodeClaimRecord,
) -> Result<(), TranscodeWorkerError> {
    let plan = build_ffmpeg_plan_with_tuning(transcode_config, media_tools, &session, tuning)?;
    run_ffmpeg(&plan).await?;
    let updated = repository.mark_succeeded(&session.id).await?;
    if !updated {
        return Err(TranscodeWorkerError::SessionUpdateLost(session.id));
    }

    info!(
        session_id = %session.id,
        manifest_path = %plan.manifest_path.display(),
        "transcode session succeeded"
    );
    Ok(())
}

async fn run_ffmpeg(plan: &FfmpegPlan) -> Result<(), TranscodeWorkerError> {
    tokio::fs::create_dir_all(&plan.output_dir).await?;
    let output = Command::new(&plan.program)
        .args(&plan.args)
        .output()
        .await?;
    if output.status.success() {
        return Ok(());
    }

    let stderr = String::from_utf8_lossy(&output.stderr);
    let stdout = String::from_utf8_lossy(&output.stdout);
    let message = stderr
        .lines()
        .chain(stdout.lines())
        .map(str::trim)
        .find(|line| !line.is_empty())
        .unwrap_or("ffmpeg exited without diagnostic output")
        .to_owned();
    Err(TranscodeWorkerError::FfmpegFailed(message))
}

#[derive(Debug)]
enum TranscodeWorkerError {
    Plan(TranscodePlanError),
    Io(std::io::Error),
    Database(sqlx::Error),
    FfmpegFailed(String),
    SessionUpdateLost(String),
}

impl std::fmt::Display for TranscodeWorkerError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Plan(err) => write!(f, "{err}"),
            Self::Io(err) => write!(f, "io error: {err}"),
            Self::Database(err) => write!(f, "database error: {err}"),
            Self::FfmpegFailed(message) => write!(f, "ffmpeg failed: {message}"),
            Self::SessionUpdateLost(session_id) => write!(
                f,
                "transcode session `{session_id}` was no longer running when updating status"
            ),
        }
    }
}

impl std::error::Error for TranscodeWorkerError {}

impl From<TranscodePlanError> for TranscodeWorkerError {
    fn from(error: TranscodePlanError) -> Self {
        Self::Plan(error)
    }
}

impl From<std::io::Error> for TranscodeWorkerError {
    fn from(error: std::io::Error) -> Self {
        Self::Io(error)
    }
}

impl From<sqlx::Error> for TranscodeWorkerError {
    fn from(error: sqlx::Error) -> Self {
        Self::Database(error)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn transcode_hook_payload_exposes_safe_public_boundary() {
        let session = claim();

        let event = transcode_hook_event(
            TRANSCODE_FAILED_EVENT,
            "failed",
            &session,
            Some("ffmpeg failed"),
        );

        assert_eq!(event.event_key, TRANSCODE_FAILED_EVENT);
        assert_eq!(event.aggregate_type, "transcoding_session");
        assert_eq!(event.aggregate_id, "session-1");
        assert_eq!(event.payload["sessionId"], "session-1");
        assert_eq!(event.payload["userId"], "user-1");
        assert_eq!(event.payload["itemId"], "item-1");
        assert_eq!(event.payload["mediaSourceId"], "2");
        assert_eq!(event.payload["status"], "failed");
        assert_eq!(event.payload["workerId"], "worker-1");
        assert_eq!(event.payload["attempts"], 1);
        assert_eq!(event.payload["maxAttempts"], 3);
        assert_eq!(event.payload["container"], "mkv");
        assert_eq!(event.payload["bitrate"], 10_000_000);
        assert_eq!(event.payload["error"], "ffmpeg failed");
        assert!(event.payload.get("inputPath").is_none());
        assert!(event.payload.get("outputPath").is_none());
        assert!(event.payload.get("manifestPath").is_none());
    }

    #[test]
    fn transcode_completed_hook_omits_error() {
        let session = claim();

        let event = transcode_hook_event(TRANSCODE_COMPLETED_EVENT, "succeeded", &session, None);

        assert_eq!(event.event_key, TRANSCODE_COMPLETED_EVENT);
        assert_eq!(event.payload["status"], "succeeded");
        assert!(event.payload.get("error").is_none());
    }

    #[test]
    fn failed_or_lost_transcodes_attempt_output_cleanup() {
        let source = include_str!("worker.rs");

        assert!(source.contains("cleanup_session_output_dir_best_effort"));
        assert!(source.contains("\"transcode_failed\""));
        assert!(source.contains("\"transcode_update_lost\""));
    }

    #[test]
    fn transcode_failure_logs_structured_session_context() {
        let source = include_str!("worker.rs");
        let production_source = source
            .split("#[cfg(test)]")
            .next()
            .expect("worker source should include production section");

        assert!(production_source.contains("session_id = %session.id"));
        assert!(production_source.contains("user_id = %session.user_id"));
        assert!(production_source.contains("item_id = %session.item_id"));
        assert!(production_source.contains("media_file_id = ?session.media_file_id"));
        assert!(production_source.contains("worker_id = %session.worker_id"));
        assert!(production_source.contains("attempts = session.attempts"));
        assert!(production_source.contains("max_attempts = session.max_attempts"));
        assert!(
            production_source.contains("hardware_acceleration = ?session.hardware_acceleration")
        );
        assert!(production_source.contains("video_codec = ?session.video_codec"));
        assert!(production_source.contains("audio_codec = ?session.audio_codec"));
        assert!(production_source.contains("container = ?session.container"));
        assert!(production_source.contains("bitrate = ?session.bitrate"));
        assert!(production_source.contains("transcode session failed"));
    }

    fn claim() -> TranscodeClaimRecord {
        TranscodeClaimRecord {
            id: "session-1".to_owned(),
            status: "running".to_owned(),
            user_id: "user-1".to_owned(),
            item_id: "item-1".to_owned(),
            media_file_id: Some(2),
            hardware_acceleration: Some("nvidia".to_owned()),
            input_path: Some("D:/Media/Movie.mkv".to_owned()),
            output_path: Some("./var/transcode/session-1".to_owned()),
            manifest_path: Some("./var/transcode/session-1/master.m3u8".to_owned()),
            video_codec: Some("h264".to_owned()),
            audio_codec: Some("aac".to_owned()),
            container: Some("mkv".to_owned()),
            bitrate: Some(10_000_000),
            worker_id: "worker-1".to_owned(),
            lease_expires_at: "now".to_owned(),
            attempts: 1,
            max_attempts: 3,
        }
    }
}
