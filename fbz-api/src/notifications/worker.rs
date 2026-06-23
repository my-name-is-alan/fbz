use std::time::Duration;

use tokio::{
    sync::broadcast,
    task::JoinHandle,
    time::{MissedTickBehavior, interval},
};
use tracing::{info, warn};

use crate::{
    config::{NotificationWorkerConfig, SecretConfig},
    db::DbPool,
    notifications::delivery::NotificationDeliveryService,
};

pub fn spawn_notification_worker(
    pool: DbPool,
    worker_config: NotificationWorkerConfig,
    secret_config: SecretConfig,
    mut shutdown: broadcast::Receiver<()>,
) -> JoinHandle<()> {
    tokio::spawn(async move {
        let service = NotificationDeliveryService::new(pool, worker_config.clone(), secret_config);
        let mut tick = interval(Duration::from_secs(worker_config.interval_seconds));
        tick.set_missed_tick_behavior(MissedTickBehavior::Delay);

        info!(
            interval_seconds = worker_config.interval_seconds,
            delivery_timeout_ms = worker_config.delivery_timeout_ms,
            "notification worker started"
        );

        loop {
            tokio::select! {
                result = shutdown.recv() => {
                    match result {
                        Ok(()) | Err(broadcast::error::RecvError::Closed) => {
                            info!("notification worker shutdown received");
                            break;
                        }
                        Err(broadcast::error::RecvError::Lagged(skipped)) => {
                            warn!(skipped, "notification worker shutdown receiver lagged");
                            break;
                        }
                    }
                }
                _ = tick.tick() => {
                    run_available_deliveries(&service).await;
                }
            }
        }

        info!("notification worker stopped");
    })
}

async fn run_available_deliveries(service: &NotificationDeliveryService) {
    loop {
        match service.run_next_delivery().await {
            Ok(Some(summary)) => {
                info!(
                    outbox_event_id = %summary.outbox_event_id,
                    request_id = %summary.request_id,
                    target_count = summary.target_count,
                    delivered_targets = summary.delivered_targets,
                    failed_targets = summary.failed_targets,
                    outbox_status = %summary.outbox_status,
                    error = ?summary.error_message,
                    "notification delivery processed"
                );
            }
            Ok(None) => break,
            Err(err) => {
                warn!(error = %err, "notification worker failed to process delivery");
                break;
            }
        }
    }
}
