use std::time::Duration;

use tokio::{
    sync::broadcast,
    task::JoinHandle,
    time::{MissedTickBehavior, interval},
};
use tracing::{info, warn};

use crate::{
    config::{MetadataConfig, MetadataWorkerConfig, ProxyConfig},
    db::DbPool,
    metadata::service::MetadataService,
};

pub fn spawn_metadata_worker(
    pool: DbPool,
    metadata: MetadataConfig,
    proxy: ProxyConfig,
    config: MetadataWorkerConfig,
    mut shutdown: broadcast::Receiver<()>,
) -> JoinHandle<()> {
    tokio::spawn(async move {
        let service = match MetadataService::new(pool, metadata, proxy) {
            Ok(service) => service,
            Err(err) => {
                warn!(error = %err, "metadata worker not started");
                return;
            }
        };
        let mut tick = interval(Duration::from_secs(config.interval_seconds));
        tick.set_missed_tick_behavior(MissedTickBehavior::Delay);

        info!(
            interval_seconds = config.interval_seconds,
            "metadata worker started"
        );

        loop {
            tokio::select! {
                result = shutdown.recv() => {
                    match result {
                        Ok(()) | Err(broadcast::error::RecvError::Closed) => {
                            info!("metadata worker shutdown received");
                            break;
                        }
                        Err(broadcast::error::RecvError::Lagged(skipped)) => {
                            warn!(skipped, "metadata worker shutdown receiver lagged");
                            break;
                        }
                    }
                }
                _ = tick.tick() => {
                    run_available_jobs(&service).await;
                }
            }
        }

        info!("metadata worker stopped");
    })
}

async fn run_available_jobs(service: &MetadataService) {
    loop {
        match service.run_next_refresh_job().await {
            Ok(Some(summary)) => {
                info!(
                    job_id = %summary.job_id,
                    item_id = %summary.item_id,
                    status = %summary.status,
                    matched = summary.matched,
                    provider = summary.provider.as_deref(),
                    provider_attempts = summary.provider_attempts.len(),
                    "metadata refresh job completed by background worker"
                );
            }
            Ok(None) => break,
            Err(err) => {
                warn!(error = %err, "metadata worker failed to run job");
                break;
            }
        }
    }
}
