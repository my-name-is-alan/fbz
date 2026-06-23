use std::time::Duration;

use tokio::{
    sync::broadcast,
    task::JoinHandle,
    time::{MissedTickBehavior, interval},
};
use tracing::{info, warn};

use crate::{config::ScanWorkerConfig, db::DbPool, scan::service::ScanService};

pub fn spawn_scan_worker(
    pool: DbPool,
    config: ScanWorkerConfig,
    mut shutdown: broadcast::Receiver<()>,
) -> JoinHandle<()> {
    tokio::spawn(async move {
        let service = ScanService::new(pool);
        let mut tick = interval(Duration::from_secs(config.interval_seconds));
        tick.set_missed_tick_behavior(MissedTickBehavior::Delay);

        info!(
            interval_seconds = config.interval_seconds,
            "scan worker started"
        );

        loop {
            tokio::select! {
                result = shutdown.recv() => {
                    match result {
                        Ok(()) | Err(broadcast::error::RecvError::Closed) => {
                            info!("scan worker shutdown received");
                            break;
                        }
                        Err(broadcast::error::RecvError::Lagged(skipped)) => {
                            warn!(skipped, "scan worker shutdown receiver lagged");
                            break;
                        }
                    }
                }
                _ = tick.tick() => {
                    run_available_jobs(&service).await;
                }
            }
        }

        info!("scan worker stopped");
    })
}

async fn run_available_jobs(service: &ScanService) {
    loop {
        match service.run_next_scan_job().await {
            Ok(Some(summary)) => {
                info!(
                    job_id = %summary.job_id,
                    scanned_files = summary.scanned_files,
                    created_items = summary.created_items,
                    updated_files = summary.updated_files,
                    metadata_refresh_jobs = summary.metadata_refresh_jobs,
                    probe_jobs = summary.probe_jobs,
                    missing_items = summary.missing_items,
                    missing_mark_skipped = summary.missing_mark_skipped,
                    has_more = summary.has_more,
                    continuation_job_id = summary.continuation_job_id.as_deref(),
                    "scan job completed by background worker"
                );
            }
            Ok(None) => break,
            Err(err) => {
                warn!(error = %err, "scan worker failed to run job");
                break;
            }
        }
    }
}
