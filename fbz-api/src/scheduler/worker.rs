use std::{path::PathBuf, time::Duration};

use tokio::{
    sync::broadcast,
    task::JoinHandle,
    time::{MissedTickBehavior, interval},
};
use tracing::{info, warn};

use crate::{config::SchedulerWorkerConfig, db::DbPool, scheduler::service::SchedulerService};

pub fn spawn_scheduler_worker(
    pool: DbPool,
    config: SchedulerWorkerConfig,
    transcode_cache_dir: PathBuf,
    mut shutdown: broadcast::Receiver<()>,
) -> JoinHandle<()> {
    tokio::spawn(async move {
        let service = SchedulerService::new(pool, transcode_cache_dir);
        let mut tick = interval(Duration::from_secs(config.interval_seconds));
        tick.set_missed_tick_behavior(MissedTickBehavior::Delay);

        info!(
            interval_seconds = config.interval_seconds,
            "scheduler worker started"
        );

        loop {
            tokio::select! {
                result = shutdown.recv() => {
                    match result {
                        Ok(()) | Err(broadcast::error::RecvError::Closed) => {
                            info!("scheduler worker shutdown received");
                            break;
                        }
                        Err(broadcast::error::RecvError::Lagged(skipped)) => {
                            warn!(skipped, "scheduler worker shutdown receiver lagged");
                            break;
                        }
                    }
                }
                _ = tick.tick() => {
                    run_due_tasks(&service).await;
                }
            }
        }

        info!("scheduler worker stopped");
    })
}

async fn run_due_tasks(service: &SchedulerService) {
    loop {
        match service.run_next_due_task().await {
            Ok(Some(summary)) => {
                info!(
                    task_key = %summary.task_key,
                    task_type = %summary.task_type,
                    queued_jobs = summary.queued_jobs,
                    "scheduled task dispatched"
                );
            }
            Ok(None) => break,
            Err(err) => {
                warn!(error = %err, "scheduler worker failed to dispatch task");
                break;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn scheduler_worker_receives_transcode_cache_dir() {
        let source = include_str!("worker.rs");

        assert!(source.contains("transcode_cache_dir"));
        assert!(source.contains("SchedulerService::new(pool, transcode_cache_dir)"));
    }
}
