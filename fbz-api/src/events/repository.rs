use serde_json::Value;
use sqlx::Row;

use crate::db::DbPool;

const MARK_STREAM_MIRROR_FAILED_SQL: &str = r#"
            update event_outbox
            set stream_mirror_locked_by = null,
                stream_mirror_locked_until = now() + ($4::bigint * interval '1 second'),
                stream_mirror_last_error = $3
            where id = $1
              and stream_mirror_locked_by = $2
              and stream_mirrored_at is null
            "#;

#[derive(Clone)]
pub struct EventOutboxMirrorRepository {
    pool: DbPool,
}

impl EventOutboxMirrorRepository {
    pub fn new(pool: DbPool) -> Self {
        Self { pool }
    }

    pub async fn claim_batch(
        &self,
        batch_size: u16,
        worker_id: &str,
        lease_seconds: u64,
    ) -> Result<Vec<ClaimedOutboxEvent>, sqlx::Error> {
        let rows = sqlx::query(
            r#"
            with claimable as (
                select id,
                       (
                           stream_mirror_locked_by is not null
                           and stream_mirror_locked_until <= now()
                       ) as stale_mirror_lease
                from event_outbox
                where stream_mirrored_at is null
                  and (
                      stream_mirror_locked_until is null
                      or stream_mirror_locked_until <= now()
                  )
                order by id asc
                for update skip locked
                limit $1
            )
            update event_outbox outbox
            set stream_mirror_attempts = outbox.stream_mirror_attempts + 1,
                stream_mirror_locked_by = $2,
                stream_mirror_locked_until = now() + ($3::bigint * interval '1 second'),
                stream_mirror_last_error = null
            from claimable
            where outbox.id = claimable.id
            returning
                outbox.id,
                outbox.public_id::text as public_id,
                outbox.event_type,
                outbox.aggregate_type,
                outbox.aggregate_id,
                outbox.payload,
                outbox.status,
                outbox.attempts,
                outbox.max_attempts,
                outbox.available_at::text as available_at,
                outbox.created_at::text as created_at,
                outbox.stream_mirror_attempts,
                claimable.stale_mirror_lease
            "#,
        )
        .bind(i64::from(batch_size))
        .bind(worker_id)
        .bind(i64::try_from(lease_seconds).unwrap_or(i64::MAX))
        .fetch_all(&self.pool)
        .await?;

        let mut events = rows
            .into_iter()
            .map(|row| -> Result<ClaimedOutboxEvent, sqlx::Error> {
                Ok(ClaimedOutboxEvent {
                    id: row.try_get("id")?,
                    public_id: row.try_get("public_id")?,
                    event_type: row.try_get("event_type")?,
                    aggregate_type: row.try_get("aggregate_type")?,
                    aggregate_id: row.try_get("aggregate_id")?,
                    payload: row.try_get("payload")?,
                    status: row.try_get("status")?,
                    attempts: row.try_get("attempts")?,
                    max_attempts: row.try_get("max_attempts")?,
                    available_at: row.try_get("available_at")?,
                    created_at: row.try_get("created_at")?,
                    stream_mirror_attempts: row.try_get("stream_mirror_attempts")?,
                    stale_mirror_lease: row.try_get("stale_mirror_lease")?,
                })
            })
            .collect::<Result<Vec<_>, _>>()?;
        events.sort_by_key(|event| event.id);
        Ok(events)
    }

    pub async fn mark_mirrored(
        &self,
        event_id: i64,
        worker_id: &str,
        stream_id: &str,
    ) -> Result<u64, sqlx::Error> {
        let result = sqlx::query(
            r#"
            update event_outbox
            set stream_mirrored_at = now(),
                stream_mirror_locked_by = null,
                stream_mirror_locked_until = null,
                stream_mirror_last_error = null,
                stream_mirror_last_stream_id = $3
            where id = $1
              and stream_mirror_locked_by = $2
              and stream_mirrored_at is null
            "#,
        )
        .bind(event_id)
        .bind(worker_id)
        .bind(stream_id)
        .execute(&self.pool)
        .await?;

        Ok(result.rows_affected())
    }

    pub async fn mark_failed(
        &self,
        event_id: i64,
        worker_id: &str,
        error: &str,
        retry_delay_seconds: u64,
    ) -> Result<u64, sqlx::Error> {
        let result = sqlx::query(MARK_STREAM_MIRROR_FAILED_SQL)
            .bind(event_id)
            .bind(worker_id)
            .bind(truncate_error(error))
            .bind(i64::try_from(retry_delay_seconds).unwrap_or(i64::MAX))
            .execute(&self.pool)
            .await?;

        Ok(result.rows_affected())
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct ClaimedOutboxEvent {
    pub id: i64,
    pub public_id: String,
    pub event_type: String,
    pub aggregate_type: String,
    pub aggregate_id: String,
    pub payload: Value,
    pub status: String,
    pub attempts: i32,
    pub max_attempts: i32,
    pub available_at: String,
    pub created_at: String,
    pub stream_mirror_attempts: i32,
    pub stale_mirror_lease: bool,
}

fn truncate_error(error: &str) -> String {
    const MAX_ERROR_LEN: usize = 2_000;
    error.chars().take(MAX_ERROR_LEN).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    const REPOSITORY_SOURCE: &str = include_str!("repository.rs");

    #[test]
    fn truncate_error_keeps_database_error_values_bounded() {
        let error = "x".repeat(2_500);

        let truncated = truncate_error(&error);

        assert_eq!(truncated.len(), 2_000);
    }

    #[test]
    fn stream_mirror_failure_defers_retry_with_locked_until() {
        let normalized = MARK_STREAM_MIRROR_FAILED_SQL
            .split_whitespace()
            .collect::<Vec<_>>()
            .join(" ");

        assert!(normalized.contains("stream_mirror_locked_by = null"));
        assert!(
            normalized.contains(
                "stream_mirror_locked_until = now() + ($4::bigint * interval '1 second')"
            )
        );
        assert!(!normalized.contains("stream_mirror_locked_until = null"));
    }

    #[test]
    fn stream_mirror_claim_marks_expired_leases_before_update() {
        let production_source = REPOSITORY_SOURCE
            .split("#[cfg(test)]")
            .next()
            .expect("repository source should include production section");

        assert!(production_source.contains("stale_mirror_lease"));
        assert!(production_source.contains("stream_mirror_locked_by is not null"));
        assert!(production_source.contains("stream_mirror_locked_until <= now()"));
        assert!(production_source.contains("claimable.stale_mirror_lease"));
        assert!(production_source.contains("stale_mirror_lease: row.try_get"));
    }
}
