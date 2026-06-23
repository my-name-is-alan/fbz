use std::time::Duration;

use tokio::{
    sync::broadcast,
    task::{JoinError, JoinHandle, JoinSet},
    time::{MissedTickBehavior, interval},
};
use tracing::{info, warn};

use crate::{
    config::{PluginConfig, PluginWorkerConfig},
    db::DbPool,
    plugins::execution::{PluginExecutionError, PluginExecutionService, PluginExecutionSummary},
};

pub fn spawn_plugin_worker(
    pool: DbPool,
    plugin_config: PluginConfig,
    host_base_url: String,
    worker_config: PluginWorkerConfig,
    mut shutdown: broadcast::Receiver<()>,
) -> JoinHandle<()> {
    tokio::spawn(async move {
        let max_concurrency = plugin_config.max_concurrency;
        let service = PluginExecutionService::new(pool, plugin_config, host_base_url);
        let mut tick = interval(Duration::from_secs(worker_config.interval_seconds));
        tick.set_missed_tick_behavior(MissedTickBehavior::Delay);

        info!(
            interval_seconds = worker_config.interval_seconds,
            max_concurrency, "plugin worker started"
        );

        loop {
            tokio::select! {
                result = shutdown.recv() => {
                    match result {
                        Ok(()) | Err(broadcast::error::RecvError::Closed) => {
                            info!("plugin worker shutdown received");
                            break;
                        }
                        Err(broadcast::error::RecvError::Lagged(skipped)) => {
                            warn!(skipped, "plugin worker shutdown receiver lagged");
                            break;
                        }
                    }
                }
                _ = tick.tick() => {
                    run_available_dispatches(&service, max_concurrency).await;
                }
            }
        }

        info!("plugin worker stopped");
    })
}

async fn run_available_dispatches(service: &PluginExecutionService, max_concurrency: u16) {
    match service.recover_stale_execution_runs().await {
        Ok(summary) if summary.recovered_anything() => {
            info!(
                expired_runs = summary.expired_runs,
                revoked_tokens = summary.revoked_tokens,
                "recovered stale plugin execution runs"
            );
        }
        Ok(_) => {}
        Err(err) => {
            log_dispatch_error(&err);
            return;
        }
    }

    loop {
        let batch = run_dispatch_batch(service, max_concurrency).await;
        if !batch.should_continue(dispatch_batch_size(max_concurrency)) {
            break;
        }
    }
}

async fn run_dispatch_batch(
    service: &PluginExecutionService,
    max_concurrency: u16,
) -> PluginDispatchBatchOutcome {
    let batch_size = dispatch_batch_size(max_concurrency);
    let mut tasks = JoinSet::new();

    for _ in 0..batch_size {
        let service = service.clone();
        tasks.spawn(async move { service.run_next_dispatch().await });
    }

    let mut outcome = PluginDispatchBatchOutcome::default();
    while let Some(result) = tasks.join_next().await {
        match result {
            Ok(Ok(Some(summary))) => {
                outcome.processed += 1;
                log_dispatch_summary(&summary);
            }
            Ok(Ok(None)) => {
                outcome.empty += 1;
            }
            Ok(Err(err)) => {
                outcome.failed += 1;
                log_dispatch_error(&err);
            }
            Err(err) => {
                outcome.failed += 1;
                log_join_error(&err);
            }
        }
    }

    outcome
}

fn log_dispatch_summary(summary: &PluginExecutionSummary) {
    info!(
        outbox_event_id = %summary.outbox_event_id,
        plugin_id = %summary.plugin_id,
        handler = %summary.handler,
        outbox_status = %summary.outbox_status,
        error = ?summary.error_message,
        "plugin dispatch processed"
    );
}

fn log_dispatch_error(err: &PluginExecutionError) {
    warn!(error = %err, "plugin worker failed to process dispatch");
}

fn log_join_error(err: &JoinError) {
    warn!(error = %err, "plugin worker task join failed");
}

fn dispatch_batch_size(max_concurrency: u16) -> usize {
    usize::from(max_concurrency.max(1))
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
struct PluginDispatchBatchOutcome {
    processed: usize,
    empty: usize,
    failed: usize,
}

impl PluginDispatchBatchOutcome {
    fn should_continue(&self, batch_size: usize) -> bool {
        self.failed == 0 && self.processed == batch_size
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dispatch_batch_size_never_returns_zero() {
        assert_eq!(dispatch_batch_size(0), 1);
        assert_eq!(dispatch_batch_size(4), 4);
    }

    #[test]
    fn dispatch_batch_continues_only_when_full_batch_processed_without_errors() {
        assert!(
            PluginDispatchBatchOutcome {
                processed: 4,
                empty: 0,
                failed: 0,
            }
            .should_continue(4)
        );
        assert!(
            !PluginDispatchBatchOutcome {
                processed: 3,
                empty: 1,
                failed: 0,
            }
            .should_continue(4)
        );
        assert!(
            !PluginDispatchBatchOutcome {
                processed: 4,
                empty: 0,
                failed: 1,
            }
            .should_continue(4)
        );
    }
}
