use std::{
    error::Error,
    fmt::{Display, Formatter},
    time::Duration,
};

use tokio::{
    sync::broadcast,
    task::JoinHandle,
    time::{MissedTickBehavior, interval},
};
use tracing::{info, warn};

use crate::{
    cache::RedisConnection,
    config::RedisConfig,
    db::DbPool,
    events::{
        repository::{ClaimedOutboxEvent, EventOutboxMirrorRepository},
        stream::publish_outbox_event,
    },
};

pub fn spawn_event_stream_mirror_worker(
    pool: DbPool,
    redis: RedisConnection,
    config: RedisConfig,
    mut shutdown: broadcast::Receiver<()>,
) -> JoinHandle<()> {
    tokio::spawn(async move {
        let worker_id = format!(
            "event-stream-mirror-{}-{}",
            std::process::id(),
            rand::random::<u64>()
        );
        let mut service = EventStreamMirrorService::new(pool, redis, config.clone(), worker_id);
        let mut tick = interval(Duration::from_secs(config.event_stream_interval_seconds));
        tick.set_missed_tick_behavior(MissedTickBehavior::Delay);

        info!(
            stream_key = %config.event_stream_key,
            batch_size = config.event_stream_batch_size,
            interval_seconds = config.event_stream_interval_seconds,
            lease_seconds = config.event_stream_lease_seconds,
            "event stream mirror worker started"
        );

        loop {
            tokio::select! {
                result = shutdown.recv() => {
                    match result {
                        Ok(()) | Err(broadcast::error::RecvError::Closed) => {
                            info!("event stream mirror worker shutdown received");
                            break;
                        }
                        Err(broadcast::error::RecvError::Lagged(skipped)) => {
                            warn!(skipped, "event stream mirror worker shutdown receiver lagged");
                            break;
                        }
                    }
                }
                _ = tick.tick() => {
                    run_available_mirrors(&mut service).await;
                }
            }
        }

        info!("event stream mirror worker stopped");
    })
}

async fn run_available_mirrors(service: &mut EventStreamMirrorService) {
    loop {
        match service.mirror_next_batch().await {
            Ok(summary) if summary.claimed == 0 => break,
            Ok(summary) => {
                info!(
                    claimed = summary.claimed,
                    published = summary.published,
                    "event outbox batch mirrored to redis stream"
                );
                if summary.claimed < service.config.event_stream_batch_size as usize {
                    break;
                }
            }
            Err(err) => {
                warn!(error = %err, "event stream mirror worker failed");
                break;
            }
        }
    }
}

struct EventStreamMirrorService {
    repository: EventOutboxMirrorRepository,
    redis: RedisConnection,
    config: RedisConfig,
    worker_id: String,
}

impl EventStreamMirrorService {
    fn new(pool: DbPool, redis: RedisConnection, config: RedisConfig, worker_id: String) -> Self {
        Self {
            repository: EventOutboxMirrorRepository::new(pool),
            redis,
            config,
            worker_id,
        }
    }

    async fn mirror_next_batch(
        &mut self,
    ) -> Result<EventStreamMirrorSummary, EventStreamMirrorError> {
        let events = self
            .repository
            .claim_batch(
                self.config.event_stream_batch_size,
                &self.worker_id,
                self.config.event_stream_lease_seconds,
            )
            .await?;
        let claimed = events.len();
        if claimed == 0 {
            return Ok(EventStreamMirrorSummary {
                claimed: 0,
                published: 0,
            });
        }

        let mut published = 0;
        for event in events {
            self.publish_one(&event).await?;
            published += 1;
        }

        Ok(EventStreamMirrorSummary { claimed, published })
    }

    async fn publish_one(
        &mut self,
        event: &ClaimedOutboxEvent,
    ) -> Result<(), EventStreamMirrorError> {
        let stream_id = match publish_outbox_event(&mut self.redis, &self.config, event).await {
            Ok(stream_id) => stream_id,
            Err(err) => {
                let message = err.to_string();
                let retry_delay_seconds = stream_mirror_retry_delay_seconds(
                    event.stream_mirror_attempts,
                    self.config.event_stream_retry_base_seconds,
                    self.config.event_stream_retry_max_seconds,
                );
                self.repository
                    .mark_failed(event.id, &self.worker_id, &message, retry_delay_seconds)
                    .await?;
                return Err(EventStreamMirrorError::Redis(err));
            }
        };

        self.repository
            .mark_mirrored(event.id, &self.worker_id, &stream_id)
            .await?;
        Ok(())
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct EventStreamMirrorSummary {
    claimed: usize,
    published: usize,
}

#[derive(Debug)]
enum EventStreamMirrorError {
    Database(sqlx::Error),
    Redis(redis::RedisError),
}

impl Display for EventStreamMirrorError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Database(err) => write!(formatter, "database error: {err}"),
            Self::Redis(err) => write!(formatter, "redis error: {err}"),
        }
    }
}

impl Error for EventStreamMirrorError {}

impl From<sqlx::Error> for EventStreamMirrorError {
    fn from(value: sqlx::Error) -> Self {
        Self::Database(value)
    }
}

impl From<redis::RedisError> for EventStreamMirrorError {
    fn from(value: redis::RedisError) -> Self {
        Self::Redis(value)
    }
}

fn stream_mirror_retry_delay_seconds(attempts: i32, base_seconds: u64, max_seconds: u64) -> u64 {
    let retry_step = u32::try_from(attempts.saturating_sub(1))
        .unwrap_or_default()
        .min(20);
    let multiplier = 1_u64.checked_shl(retry_step).unwrap_or(u64::MAX);

    base_seconds.saturating_mul(multiplier).min(max_seconds)
}

#[cfg(test)]
mod tests {
    use super::stream_mirror_retry_delay_seconds;

    #[test]
    fn stream_mirror_retry_delay_is_bounded_exponential_backoff() {
        assert_eq!(stream_mirror_retry_delay_seconds(0, 5, 300), 5);
        assert_eq!(stream_mirror_retry_delay_seconds(1, 5, 300), 5);
        assert_eq!(stream_mirror_retry_delay_seconds(2, 5, 300), 10);
        assert_eq!(stream_mirror_retry_delay_seconds(3, 5, 300), 20);
        assert_eq!(stream_mirror_retry_delay_seconds(20, 5, 300), 300);
        assert_eq!(stream_mirror_retry_delay_seconds(i32::MAX, 5, 300), 300);
    }
}
