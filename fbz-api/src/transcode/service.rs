use crate::{
    config::TranscodeConfig,
    db::DbPool,
    transcode::repository::{TranscodeClaimOutcome, TranscodeRepository},
};

#[derive(Clone)]
pub struct TranscodeQueueService {
    repository: TranscodeRepository,
    config: TranscodeConfig,
}

impl TranscodeQueueService {
    pub fn new(pool: DbPool, config: TranscodeConfig) -> Self {
        Self {
            repository: TranscodeRepository::new(pool),
            config,
        }
    }

    pub async fn claim_next(&self, worker_id: &str) -> Result<TranscodeClaimOutcome, sqlx::Error> {
        self.repository
            .claim_next(
                self.config.max_concurrent,
                self.config.lease_seconds,
                worker_id,
            )
            .await
    }
}

pub fn transcode_worker_id(prefix: &str) -> String {
    format!("{}-{}", prefix.trim(), std::process::id())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn transcode_worker_id_includes_process_id() {
        let worker_id = transcode_worker_id("transcode");

        assert!(worker_id.starts_with("transcode-"));
        assert!(worker_id.ends_with(&std::process::id().to_string()));
    }
}
