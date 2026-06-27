use sqlx::{Postgres, QueryBuilder, Row, postgres::PgRow};
use tracing::warn;

use crate::db::DbPool;

#[derive(Clone)]
pub struct TranscodeRepository {
    pool: DbPool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TranscodeSessionRecord {
    pub id: String,
    pub status: String,
    pub hardware_acceleration: Option<String>,
    pub input_path: Option<String>,
    pub output_path: Option<String>,
    pub manifest_path: Option<String>,
    pub video_codec: Option<String>,
    pub audio_codec: Option<String>,
    pub container: Option<String>,
    pub bitrate: Option<i32>,
    pub worker_id: Option<String>,
    pub lease_expires_at: Option<String>,
    pub attempts: i32,
    pub max_attempts: i32,
    pub error_message: Option<String>,
    pub created_at: String,
    pub updated_at: String,
    pub started_at: Option<String>,
    pub finished_at: Option<String>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct TranscodeSessionFilter {
    pub status: Option<String>,
    pub hardware_acceleration: Option<String>,
    pub cursor: Option<String>,
    pub limit: i64,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TranscodeSessionPage {
    pub records: Vec<TranscodeSessionRecord>,
    pub next_cursor: Option<String>,
    pub has_more: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TranscodeClaimRecord {
    pub id: String,
    pub status: String,
    pub user_id: String,
    pub item_id: String,
    pub media_file_id: Option<i64>,
    pub hardware_acceleration: Option<String>,
    pub input_path: Option<String>,
    pub output_path: Option<String>,
    pub manifest_path: Option<String>,
    pub video_codec: Option<String>,
    pub audio_codec: Option<String>,
    pub container: Option<String>,
    pub bitrate: Option<i32>,
    pub worker_id: String,
    pub lease_expires_at: String,
    pub attempts: i32,
    pub max_attempts: i32,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct HlsTranscodeSessionRecord {
    pub id: String,
    pub status: String,
    pub item_id: String,
    pub media_file_id: Option<i64>,
    pub output_path: Option<String>,
    pub manifest_path: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum TranscodeClaimOutcome {
    Claimed(TranscodeClaimRecord),
    AtCapacity,
    NoQueuedSession,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct TranscodeExpiredLeaseSummary {
    pub expired_sessions: u64,
    pub retryable_sessions: u64,
    pub terminal_sessions: u64,
}

impl TranscodeExpiredLeaseSummary {
    pub fn has_work(&self) -> bool {
        self.expired_sessions > 0
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CreateTranscodeSessionInput {
    pub user_id: i64,
    pub media_item_id: i64,
    pub media_file_id: Option<i64>,
    pub input_path: String,
    pub output_base_path: String,
    pub play_session_id: Option<String>,
    pub device_id: Option<String>,
    pub video_codec: Option<String>,
    pub audio_codec: Option<String>,
    pub container: Option<String>,
    pub bitrate: Option<i32>,
}

const TRANSCODE_CANCEL_SESSION_SQL: &str = r#"
            update transcoding_sessions
            set status = 'cancelled',
                worker_id = null,
                lease_expires_at = null,
                finished_at = coalesce(finished_at, now()),
                updated_at = now()
            where public_id = case
                when $1::text ~* '^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$'
                then $1::uuid
                else null::uuid
            end
              and status in ('queued', 'running')
            returning public_id::text as id,
                      status,
                      hardware_acceleration,
                      input_path,
                      output_path,
                      manifest_path,
                      video_codec,
                      audio_codec,
                      container,
                      bitrate,
                      worker_id,
                      lease_expires_at::text as lease_expires_at,
                      attempts,
                      max_attempts,
                      error_message,
                      created_at::text as created_at,
                      updated_at::text as updated_at,
                      started_at::text as started_at,
                      finished_at::text as finished_at
            "#;

const TRANSCODE_SESSION_EXISTS_SQL: &str = r#"
            select exists (
                select 1
                from transcoding_sessions
                where public_id = case
                    when $1::text ~* '^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$'
                    then $1::uuid
                    else null::uuid
                end
            )
            "#;

const TRANSCODE_CANCEL_ACTIVE_ENCODING_SQL: &str = r#"
            update transcoding_sessions
            set status = 'cancelled',
                worker_id = null,
                lease_expires_at = null,
                finished_at = coalesce(finished_at, now()),
                updated_at = now()
            where user_id = $1
              and play_session_id = $2
              and ($3::text is null or device_id = $3)
              and status in ('queued', 'running')
            returning public_id::text as id,
                      status,
                      hardware_acceleration,
                      input_path,
                      output_path,
                      manifest_path,
                      video_codec,
                      audio_codec,
                      container,
                      bitrate,
                      worker_id,
                      lease_expires_at::text as lease_expires_at,
                      attempts,
                      max_attempts,
                      error_message,
                      created_at::text as created_at,
                      updated_at::text as updated_at,
                      started_at::text as started_at,
                      finished_at::text as finished_at
            "#;

const TRANSCODE_FIND_HLS_SESSION_SQL: &str = r#"
            with requested as (
                select case
                    when $2::text ~* '^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$'
                           then $2::uuid
                           else null::uuid
                       end as item_public_id,
                       case
                           when $3::text ~* '^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$'
                           then $3::uuid
                           else null::uuid
                       end as session_public_id
            )
            select
                ts.public_id::text as id,
                ts.status,
                mi.public_id::text as item_id,
                ts.media_file_id,
                ts.output_path,
                ts.manifest_path
            from requested
            join transcoding_sessions ts on ts.public_id = requested.session_public_id
            join media_items mi on mi.id = ts.media_item_id
                               and mi.public_id = requested.item_public_id
            join libraries l on l.id = mi.library_id
            join library_permissions lp on lp.library_id = mi.library_id
            where ts.user_id = $1
              and lp.user_id = $1
              and lp.can_view = true
              and mi.is_deleted = false
              and l.is_hidden = false
              and ($4::bigint is null or ts.media_file_id = $4)
            limit 1
            "#;

const EXPIRE_STALE_TRANSCODE_LEASES_SQL: &str = r#"
        with stale_session_candidates as (
            select id
            from transcoding_sessions
            where status = 'running'
              and lease_expires_at <= now()
            order by lease_expires_at asc, id asc
            limit 1000
            for update skip locked
        ),
        expired_sessions as (
            update transcoding_sessions as sessions
            set status = case
                    when attempts >= max_attempts then 'failed'
                    else 'queued'
                end,
                worker_id = null,
                lease_expires_at = null,
                error_message = case
                    when attempts >= max_attempts then coalesce(error_message, 'transcode lease expired')
                    else 'transcode lease expired; requeued'
                end,
                finished_at = case
                    when attempts >= max_attempts then coalesce(finished_at, now())
                    else finished_at
                end,
                updated_at = now()
            from stale_session_candidates candidates
            where sessions.id = candidates.id
            returning sessions.id,
                      sessions.attempts < sessions.max_attempts as retryable
        )
        select count(*)::bigint as expired_sessions,
               count(*) filter (where retryable)::bigint as retryable_sessions,
               count(*) filter (where not retryable)::bigint as terminal_sessions
        from expired_sessions
        "#;

const TRANSCODE_UPDATE_TERMINAL_STATUS_SQL: &str = r#"
        update transcoding_sessions
        set status = $2,
            worker_id = null,
            lease_expires_at = null,
            error_message = $3,
            finished_at = coalesce(finished_at, now()),
            updated_at = now()
        where public_id = case
            when $1::text ~* '^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$'
            then $1::uuid
            else null::uuid
        end
          and status = 'running'
        "#;

impl TranscodeRepository {
    pub fn new(pool: DbPool) -> Self {
        Self { pool }
    }

    pub async fn create_session(
        &self,
        input: CreateTranscodeSessionInput,
    ) -> Result<TranscodeSessionRecord, sqlx::Error> {
        let row = sqlx::query(
            r#"
            with prepared as (
                select gen_random_uuid() as public_id
            ),
            inserted as (
                insert into transcoding_sessions (
                    public_id,
                    user_id,
                    media_item_id,
                    media_file_id,
                    status,
                    input_path,
                    output_path,
                    manifest_path,
                    video_codec,
                    audio_codec,
                    container,
                    bitrate,
                    play_session_id,
                    device_id
                )
                select
                    prepared.public_id,
                    $1,
                    $2,
                    $3,
                    'queued',
                    $4,
                    concat($5::text, '/', prepared.public_id::text),
                    concat($5::text, '/', prepared.public_id::text, '/master.m3u8'),
                    $6,
                    $7,
                    $8,
                    $9,
                    $10,
                    $11
                from prepared
                returning public_id::text as id,
                          status,
                          hardware_acceleration,
                          input_path,
                          output_path,
                          manifest_path,
                          video_codec,
                          audio_codec,
                          container,
                          bitrate,
                          worker_id,
                          lease_expires_at::text as lease_expires_at,
                          attempts,
                          max_attempts,
                          error_message,
                          created_at::text as created_at,
                          updated_at::text as updated_at,
                          started_at::text as started_at,
                          finished_at::text as finished_at
            )
            select *
            from inserted
            "#,
        )
        .bind(input.user_id)
        .bind(input.media_item_id)
        .bind(input.media_file_id)
        .bind(input.input_path.trim())
        .bind(normalize_output_base_path(&input.output_base_path))
        .bind(input.video_codec)
        .bind(input.audio_codec)
        .bind(input.container)
        .bind(input.bitrate)
        .bind(normalize_optional_client_id(input.play_session_id))
        .bind(normalize_optional_client_id(input.device_id))
        .fetch_one(&self.pool)
        .await?;

        TranscodeSessionRecord::from_row(row)
    }

    pub async fn list_sessions(
        &self,
        limit: i64,
    ) -> Result<Vec<TranscodeSessionRecord>, sqlx::Error> {
        self.list_sessions_page(TranscodeSessionFilter {
            status: None,
            hardware_acceleration: None,
            cursor: None,
            limit,
        })
        .await
        .map(|page| page.records)
    }

    pub async fn list_sessions_page(
        &self,
        filter: TranscodeSessionFilter,
    ) -> Result<TranscodeSessionPage, sqlx::Error> {
        let page_limit = filter.limit.max(1);
        let fetch_limit = page_limit.saturating_add(1);
        let mut query = QueryBuilder::<Postgres>::new(
            r#"
            select sessions.public_id::text as id,
                   sessions.status,
                   sessions.hardware_acceleration,
                   sessions.input_path,
                   sessions.output_path,
                   sessions.manifest_path,
                   sessions.video_codec,
                   sessions.audio_codec,
                   sessions.container,
                   sessions.bitrate,
                   sessions.worker_id,
                   sessions.lease_expires_at::text as lease_expires_at,
                   sessions.attempts,
                   sessions.max_attempts,
                   sessions.error_message,
                   sessions.created_at::text as created_at,
                   sessions.updated_at::text as updated_at,
                   sessions.started_at::text as started_at,
                   sessions.finished_at::text as finished_at
            from transcoding_sessions sessions
            "#,
        );

        if let Some(cursor) = filter.cursor.as_deref() {
            query.push(
                r#"
                join transcoding_sessions cursor_session
                  on cursor_session.public_id = case
                      when
                "#,
            );
            query.push_bind(cursor);
            query.push(
                r#"::text ~* '^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$'
                      then
                "#,
            );
            query.push_bind(cursor);
            query.push(
                r#"::uuid
                      else null::uuid
                  end
                "#,
            );
        }

        query.push(" where true");

        if let Some(status) = filter.status.as_deref() {
            query.push(" and sessions.status = ");
            query.push_bind(status);
        }

        if let Some(hardware_acceleration) = filter.hardware_acceleration.as_deref() {
            query.push(" and sessions.hardware_acceleration = ");
            query.push_bind(hardware_acceleration);
        }

        if filter.cursor.is_some() {
            query.push(
                r#"
                and (
                    case sessions.status when 'running' then 0 when 'queued' then 1 when 'failed' then 2 else 3 end
                    >
                    case cursor_session.status when 'running' then 0 when 'queued' then 1 when 'failed' then 2 else 3 end
                    or (
                        case sessions.status when 'running' then 0 when 'queued' then 1 when 'failed' then 2 else 3 end
                        =
                        case cursor_session.status when 'running' then 0 when 'queued' then 1 when 'failed' then 2 else 3 end
                        and (sessions.created_at, sessions.id) < (cursor_session.created_at, cursor_session.id)
                    )
                )
                "#,
            );
        }

        query.push(
            r#"
            order by case sessions.status
                         when 'running' then 0
                         when 'queued' then 1
                         when 'failed' then 2
                         else 3
                     end,
                     sessions.created_at desc,
                     sessions.id desc
            limit
            "#,
        );
        query.push_bind(fetch_limit);

        let rows = query.build().fetch_all(&self.pool).await?;

        let has_more = rows.len() as i64 > page_limit;
        let mut records = rows
            .into_iter()
            .map(TranscodeSessionRecord::from_row)
            .collect::<Result<Vec<_>, _>>()?;
        if has_more {
            records.truncate(page_limit as usize);
        }
        let next_cursor = has_more
            .then(|| records.last().map(|record| record.id.clone()))
            .flatten();

        Ok(TranscodeSessionPage {
            records,
            next_cursor,
            has_more,
        })
    }

    pub async fn cancel_session(
        &self,
        session_id: &str,
    ) -> Result<Option<TranscodeSessionRecord>, sqlx::Error> {
        let row = sqlx::query(TRANSCODE_CANCEL_SESSION_SQL)
            .bind(session_id.trim())
            .fetch_optional(&self.pool)
            .await?;

        row.map(TranscodeSessionRecord::from_row).transpose()
    }

    pub async fn session_exists(&self, session_id: &str) -> Result<bool, sqlx::Error> {
        sqlx::query_scalar::<_, bool>(TRANSCODE_SESSION_EXISTS_SQL)
            .bind(session_id.trim())
            .fetch_one(&self.pool)
            .await
    }

    pub async fn cancel_active_encoding(
        &self,
        user_id: i64,
        play_session_id: &str,
        device_id: Option<&str>,
    ) -> Result<Option<TranscodeSessionRecord>, sqlx::Error> {
        let row = sqlx::query(TRANSCODE_CANCEL_ACTIVE_ENCODING_SQL)
            .bind(user_id)
            .bind(play_session_id.trim())
            .bind(device_id.and_then(normalize_client_id))
            .fetch_optional(&self.pool)
            .await?;

        row.map(TranscodeSessionRecord::from_row).transpose()
    }

    pub async fn find_hls_session(
        &self,
        user_id: i64,
        item_id: &str,
        session_id: &str,
        media_file_id: Option<i64>,
    ) -> Result<Option<HlsTranscodeSessionRecord>, sqlx::Error> {
        sqlx::query(TRANSCODE_FIND_HLS_SESSION_SQL)
            .bind(user_id)
            .bind(item_id.trim())
            .bind(session_id.trim())
            .bind(media_file_id)
            .fetch_optional(&self.pool)
            .await?
            .map(HlsTranscodeSessionRecord::from_row)
            .transpose()
    }

    pub async fn claim_next(
        &self,
        max_concurrent: u16,
        lease_seconds: u64,
        worker_id: &str,
    ) -> Result<TranscodeClaimOutcome, sqlx::Error> {
        let mut tx = self.pool.begin().await?;
        expire_stale_leases(&mut tx).await?;

        let active_count = sqlx::query_scalar::<_, i64>(
            r#"
            select count(*)::bigint
            from (
                select 1
                from transcoding_sessions
                where status = 'running'
                  and lease_expires_at > now()
                order by lease_expires_at asc, id asc
                limit $1
            ) active_sessions
            "#,
        )
        .bind(i64::from(max_concurrent))
        .fetch_one(&mut *tx)
        .await?;

        if active_count >= i64::from(max_concurrent) {
            tx.commit().await?;
            return Ok(TranscodeClaimOutcome::AtCapacity);
        }

        let Some(row) = sqlx::query(
            r#"
            select id
            from transcoding_sessions
            where status = 'queued'
            order by created_at asc, id asc
            limit 1
            for update skip locked
            "#,
        )
        .fetch_optional(&mut *tx)
        .await?
        else {
            tx.commit().await?;
            return Ok(TranscodeClaimOutcome::NoQueuedSession);
        };

        let session_id = row.try_get::<i64, _>("id")?;
        let row = sqlx::query(
            r#"
            with updated as (
            update transcoding_sessions
            set status = 'running',
                worker_id = $2,
                lease_expires_at = now() + ($3::bigint * interval '1 second'),
                attempts = attempts + 1,
                started_at = coalesce(started_at, now()),
                error_message = null,
                updated_at = now()
            where id = $1
            returning id as internal_id,
                      public_id::text as id,
                      user_id,
                      media_item_id,
                      media_file_id,
                      status,
                      hardware_acceleration,
                      input_path,
                      output_path,
                      manifest_path,
                      video_codec,
                      audio_codec,
                      container,
                      bitrate,
                      worker_id,
                      lease_expires_at::text as lease_expires_at,
                      attempts,
                      max_attempts
            )
            select updated.id,
                   u.public_id::text as user_id,
                   mi.public_id::text as item_id,
                   updated.media_file_id,
                   updated.status,
                   updated.hardware_acceleration,
                   updated.input_path,
                   updated.output_path,
                   updated.manifest_path,
                   updated.video_codec,
                   updated.audio_codec,
                   updated.container,
                   updated.bitrate,
                   updated.worker_id,
                   updated.lease_expires_at,
                   updated.attempts,
                   updated.max_attempts
            from updated
            join users u on u.id = updated.user_id
            join media_items mi on mi.id = updated.media_item_id
            "#,
        )
        .bind(session_id)
        .bind(worker_id.trim())
        .bind(lease_seconds as i64)
        .fetch_one(&mut *tx)
        .await?;

        tx.commit().await?;
        TranscodeClaimRecord::from_row(row).map(TranscodeClaimOutcome::Claimed)
    }

    pub async fn mark_succeeded(&self, session_id: &str) -> Result<bool, sqlx::Error> {
        update_terminal_status(&self.pool, session_id, "succeeded", None).await
    }

    pub async fn mark_failed(&self, session_id: &str, message: &str) -> Result<bool, sqlx::Error> {
        update_terminal_status(&self.pool, session_id, "failed", Some(message)).await
    }
}

async fn expire_stale_leases(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
) -> Result<TranscodeExpiredLeaseSummary, sqlx::Error> {
    let row = sqlx::query(EXPIRE_STALE_TRANSCODE_LEASES_SQL)
        .fetch_one(&mut **tx)
        .await?;
    let summary = TranscodeExpiredLeaseSummary {
        expired_sessions: row.try_get::<i64, _>("expired_sessions")?.max(0) as u64,
        retryable_sessions: row.try_get::<i64, _>("retryable_sessions")?.max(0) as u64,
        terminal_sessions: row.try_get::<i64, _>("terminal_sessions")?.max(0) as u64,
    };

    log_expired_transcode_lease_summary(summary);

    Ok(summary)
}

fn log_expired_transcode_lease_summary(summary: TranscodeExpiredLeaseSummary) {
    if summary.has_work() {
        warn!(
            expired_sessions = summary.expired_sessions,
            retryable_sessions = summary.retryable_sessions,
            terminal_sessions = summary.terminal_sessions,
            "recovered stale transcode sessions"
        );
    }
}

async fn update_terminal_status(
    pool: &DbPool,
    session_id: &str,
    status: &str,
    error_message: Option<&str>,
) -> Result<bool, sqlx::Error> {
    let result = sqlx::query(TRANSCODE_UPDATE_TERMINAL_STATUS_SQL)
        .bind(session_id.trim())
        .bind(status)
        .bind(error_message)
        .execute(pool)
        .await?;

    Ok(result.rows_affected() > 0)
}

impl TranscodeSessionRecord {
    fn from_row(row: PgRow) -> Result<Self, sqlx::Error> {
        Ok(Self {
            id: row.try_get("id")?,
            status: row.try_get("status")?,
            hardware_acceleration: row.try_get("hardware_acceleration")?,
            input_path: row.try_get("input_path")?,
            output_path: row.try_get("output_path")?,
            manifest_path: row.try_get("manifest_path")?,
            video_codec: row.try_get("video_codec")?,
            audio_codec: row.try_get("audio_codec")?,
            container: row.try_get("container")?,
            bitrate: row.try_get("bitrate")?,
            worker_id: row.try_get("worker_id")?,
            lease_expires_at: row.try_get("lease_expires_at")?,
            attempts: row.try_get("attempts")?,
            max_attempts: row.try_get("max_attempts")?,
            error_message: row.try_get("error_message")?,
            created_at: row.try_get("created_at")?,
            updated_at: row.try_get("updated_at")?,
            started_at: row.try_get("started_at")?,
            finished_at: row.try_get("finished_at")?,
        })
    }
}

impl TranscodeClaimRecord {
    fn from_row(row: PgRow) -> Result<Self, sqlx::Error> {
        Ok(Self {
            id: row.try_get("id")?,
            status: row.try_get("status")?,
            user_id: row.try_get("user_id")?,
            item_id: row.try_get("item_id")?,
            media_file_id: row.try_get("media_file_id")?,
            hardware_acceleration: row.try_get("hardware_acceleration")?,
            input_path: row.try_get("input_path")?,
            output_path: row.try_get("output_path")?,
            manifest_path: row.try_get("manifest_path")?,
            video_codec: row.try_get("video_codec")?,
            audio_codec: row.try_get("audio_codec")?,
            container: row.try_get("container")?,
            bitrate: row.try_get("bitrate")?,
            worker_id: row.try_get("worker_id")?,
            lease_expires_at: row.try_get("lease_expires_at")?,
            attempts: row.try_get("attempts")?,
            max_attempts: row.try_get("max_attempts")?,
        })
    }
}

impl HlsTranscodeSessionRecord {
    fn from_row(row: PgRow) -> Result<Self, sqlx::Error> {
        Ok(Self {
            id: row.try_get("id")?,
            status: row.try_get("status")?,
            item_id: row.try_get("item_id")?,
            media_file_id: row.try_get("media_file_id")?,
            output_path: row.try_get("output_path")?,
            manifest_path: row.try_get("manifest_path")?,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const REPOSITORY_SOURCE: &str = include_str!("repository.rs");
    const TRANSCODE_ADMIN_KEYSET_INDEX_MIGRATION: &str =
        include_str!("../../migrations/0053_transcode_session_admin_keyset_indexes.sql");
    const TRANSCODE_LEASE_MIGRATION: &str =
        include_str!("../../migrations/0013_transcode_queue_leases.sql");

    #[test]
    fn output_base_path_trims_trailing_separators() {
        assert_eq!(
            normalize_output_base_path("./var/transcode/"),
            "./var/transcode"
        );
        assert_eq!(
            normalize_output_base_path("C:\\fbz\\transcode\\"),
            "C:/fbz/transcode"
        );
    }

    #[test]
    fn claim_outcome_shapes_are_distinct() {
        assert_eq!(
            TranscodeClaimOutcome::AtCapacity,
            TranscodeClaimOutcome::AtCapacity
        );
        assert_ne!(
            TranscodeClaimOutcome::AtCapacity,
            TranscodeClaimOutcome::NoQueuedSession
        );
    }

    #[test]
    fn transcode_public_id_filters_use_uuid_comparisons() {
        for sql in [
            TRANSCODE_CANCEL_SESSION_SQL,
            TRANSCODE_SESSION_EXISTS_SQL,
            TRANSCODE_FIND_HLS_SESSION_SQL,
            TRANSCODE_UPDATE_TERMINAL_STATUS_SQL,
        ] {
            assert!(sql.contains("::uuid"));
            assert!(!sql.contains("public_id::text = $"));
        }

        assert!(TRANSCODE_CANCEL_SESSION_SQL.contains("where public_id = case"));
        assert!(TRANSCODE_SESSION_EXISTS_SQL.contains("where public_id = case"));
        assert!(TRANSCODE_UPDATE_TERMINAL_STATUS_SQL.contains("where public_id = case"));
        assert!(TRANSCODE_FIND_HLS_SESSION_SQL.contains(
            "join transcoding_sessions ts on ts.public_id = requested.session_public_id"
        ));
        assert!(
            TRANSCODE_FIND_HLS_SESSION_SQL.contains("and mi.public_id = requested.item_public_id")
        );
    }

    #[test]
    fn transcode_session_admin_list_uses_keyset_shape() {
        let offset_token = format!(" {} ", "offset");

        assert!(REPOSITORY_SOURCE.contains("QueryBuilder::<Postgres>"));
        assert!(REPOSITORY_SOURCE.contains("join transcoding_sessions cursor_session"));
        assert!(REPOSITORY_SOURCE.contains("cursor_session.public_id = case"));
        assert!(REPOSITORY_SOURCE.contains("case sessions.status"));
        assert!(REPOSITORY_SOURCE.contains("case cursor_session.status"));
        assert!(REPOSITORY_SOURCE.contains(
            "(sessions.created_at, sessions.id) < (cursor_session.created_at, cursor_session.id)"
        ));
        assert!(REPOSITORY_SOURCE.contains("sessions.status = "));
        assert!(REPOSITORY_SOURCE.contains("sessions.hardware_acceleration = "));
        assert!(REPOSITORY_SOURCE.contains("sessions.created_at desc"));
        assert!(REPOSITORY_SOURCE.contains("sessions.id desc"));
        assert!(
            !REPOSITORY_SOURCE
                .to_ascii_lowercase()
                .contains(&offset_token)
        );

        assert!(
            TRANSCODE_ADMIN_KEYSET_INDEX_MIGRATION
                .contains("idx_transcoding_sessions_admin_recent_keyset")
        );
        assert!(
            TRANSCODE_ADMIN_KEYSET_INDEX_MIGRATION
                .contains("idx_transcoding_sessions_admin_status_recent_keyset")
        );
        assert!(
            TRANSCODE_ADMIN_KEYSET_INDEX_MIGRATION
                .contains("idx_transcoding_sessions_admin_hardware_recent_keyset")
        );
        assert!(
            TRANSCODE_ADMIN_KEYSET_INDEX_MIGRATION
                .contains("idx_transcoding_sessions_admin_status_hardware_recent_keyset")
        );
        assert!(TRANSCODE_ADMIN_KEYSET_INDEX_MIGRATION.contains("created_at desc"));
        assert!(
            !TRANSCODE_ADMIN_KEYSET_INDEX_MIGRATION
                .to_ascii_lowercase()
                .contains(&offset_token)
        );
    }

    #[test]
    fn active_encoding_cancel_uses_user_scoped_client_session_ids() {
        let migration =
            include_str!("../../migrations/0061_transcoding_sessions_active_encoding_ids.sql");

        assert!(REPOSITORY_SOURCE.contains("pub play_session_id: Option<String>"));
        assert!(REPOSITORY_SOURCE.contains("pub device_id: Option<String>"));
        assert!(REPOSITORY_SOURCE.contains("play_session_id"));
        assert!(REPOSITORY_SOURCE.contains("device_id"));
        assert!(REPOSITORY_SOURCE.contains("TRANSCODE_CANCEL_ACTIVE_ENCODING_SQL"));
        assert!(REPOSITORY_SOURCE.contains("where user_id = $1"));
        assert!(REPOSITORY_SOURCE.contains("and play_session_id = $2"));
        assert!(REPOSITORY_SOURCE.contains("and ($3::text is null or device_id = $3)"));
        assert!(REPOSITORY_SOURCE.contains("and status in ('queued', 'running')"));
        assert!(REPOSITORY_SOURCE.contains("cancel_active_encoding"));

        assert!(migration.contains("add column if not exists play_session_id text"));
        assert!(migration.contains("add column if not exists device_id text"));
        assert!(migration.contains("idx_transcoding_sessions_active_encoding_cancel"));
        assert!(migration.contains("where status in ('queued', 'running')"));
    }

    #[test]
    fn stale_transcode_lease_recovery_counts_retryable_and_terminal_sessions() {
        let summary = TranscodeExpiredLeaseSummary {
            expired_sessions: 3,
            retryable_sessions: 2,
            terminal_sessions: 1,
        };

        assert!(summary.has_work());
        assert_eq!(
            summary.expired_sessions,
            summary.retryable_sessions + summary.terminal_sessions
        );
        assert!(EXPIRE_STALE_TRANSCODE_LEASES_SQL.contains("retryable_sessions"));
        assert!(EXPIRE_STALE_TRANSCODE_LEASES_SQL.contains("terminal_sessions"));
        assert!(EXPIRE_STALE_TRANSCODE_LEASES_SQL.contains("status = 'running'"));
        assert!(EXPIRE_STALE_TRANSCODE_LEASES_SQL.contains("lease_expires_at <= now()"));
    }

    #[test]
    fn stale_transcode_lease_recovery_uses_bounded_locked_candidate_batch() {
        let normalized = EXPIRE_STALE_TRANSCODE_LEASES_SQL
            .split_whitespace()
            .collect::<Vec<_>>()
            .join(" ");

        assert!(normalized.contains("with stale_session_candidates as"));
        assert!(normalized.contains("from transcoding_sessions"));
        assert!(normalized.contains("where status = 'running'"));
        assert!(normalized.contains("lease_expires_at <= now()"));
        assert!(normalized.contains("order by lease_expires_at asc, id asc"));
        assert!(normalized.contains("limit 1000"));
        assert!(normalized.contains("for update skip locked"));
        assert!(normalized.contains("from stale_session_candidates candidates"));
        assert!(normalized.contains("sessions.id = candidates.id"));
        assert!(
            !normalized.contains("), with expired_sessions as"),
            "stale transcode recovery should use one WITH clause with comma-separated CTEs"
        );
        assert!(
            !normalized.contains("update transcoding_sessions set status = case when attempts"),
            "stale transcode recovery should not update every expired running session directly"
        );
    }

    #[test]
    fn stale_transcode_lease_recovery_index_matches_candidate_batch_shape() {
        assert!(TRANSCODE_LEASE_MIGRATION.contains("idx_transcoding_sessions_running_lease"));
        assert!(
            TRANSCODE_LEASE_MIGRATION.contains("on transcoding_sessions (lease_expires_at, id)")
        );
        assert!(TRANSCODE_LEASE_MIGRATION.contains("where status = 'running'"));
    }

    #[test]
    fn stale_transcode_lease_recovery_reports_structured_summary_fields() {
        let production_source = REPOSITORY_SOURCE
            .split("#[cfg(test)]")
            .next()
            .expect("repository source should include production section");

        assert!(production_source.contains("TranscodeExpiredLeaseSummary"));
        assert!(production_source.contains("retryable_sessions"));
        assert!(production_source.contains("terminal_sessions"));
        assert!(production_source.contains("expired_sessions = summary.expired_sessions"));
        assert!(production_source.contains("retryable_sessions = summary.retryable_sessions"));
        assert!(production_source.contains("terminal_sessions = summary.terminal_sessions"));
        assert!(production_source.contains("recovered stale transcode sessions"));
    }

    #[test]
    fn claim_next_uses_bounded_capacity_probe() {
        let repository_source = REPOSITORY_SOURCE.replace("\r\n", "\n");
        let claim_start = repository_source
            .find("pub async fn claim_next")
            .expect("claim_next should exist");
        let claim_end = repository_source[claim_start..]
            .find("pub async fn mark_succeeded")
            .map(|offset| claim_start + offset)
            .expect("claim_next should be followed by terminal update methods");
        let claim_source = &repository_source[claim_start..claim_end];

        assert!(
            !claim_source
                .contains("select count(*)::bigint\n            from transcoding_sessions"),
            "claim_next should not exact-count every active transcode session"
        );
        assert!(
            claim_source.contains(
                "from (\n                select 1\n                from transcoding_sessions"
            ),
            "claim_next should count only a bounded running-session probe"
        );
        assert!(
            claim_source.contains("limit $1"),
            "claim_next capacity probe should be bounded by max_concurrent"
        );
    }

    // Live-DB smoke: validates stale transcode lease recovery parses and plans
    // against the migrated schema. Plain EXPLAIN does not execute the UPDATE,
    // so this does not mutate any transcode sessions.
    //   cargo test -- --ignored stale_transcode_lease_recovery_sql_plans_against_live_schema
    #[tokio::test]
    #[ignore = "requires a running PostgreSQL from ./scripts/dev-deps.ps1"]
    async fn stale_transcode_lease_recovery_sql_plans_against_live_schema() {
        use sqlx::postgres::PgPoolOptions;

        let url = std::env::var("DATABASE_URL")
            .unwrap_or_else(|_| "postgres://fbz:fbz@127.0.0.1:5432/fbz".to_owned());
        let pool = PgPoolOptions::new()
            .max_connections(2)
            .connect(&url)
            .await
            .expect("connect to live PostgreSQL");
        crate::db::migrate(&pool).await.expect("run migrations");

        let plan_rows = sqlx::query(&format!("explain {EXPIRE_STALE_TRANSCODE_LEASES_SQL}"))
            .fetch_all(&pool)
            .await
            .expect("stale transcode lease recovery SQL should parse and plan");
        assert!(
            !plan_rows.is_empty(),
            "EXPLAIN should return a query plan for stale transcode lease recovery"
        );
    }

    // Live-DB smoke: validates the bounded capacity probe used before claiming
    // transcode work. This is a read-only SELECT and does not mutate sessions.
    //   cargo test -- --ignored transcode_claim_capacity_probe_executes_against_live_schema
    #[tokio::test]
    #[ignore = "requires a running PostgreSQL from ./scripts/dev-deps.ps1"]
    async fn transcode_claim_capacity_probe_executes_against_live_schema() {
        use sqlx::postgres::PgPoolOptions;

        let url = std::env::var("DATABASE_URL")
            .unwrap_or_else(|_| "postgres://fbz:fbz@127.0.0.1:5432/fbz".to_owned());
        let pool = PgPoolOptions::new()
            .max_connections(2)
            .connect(&url)
            .await
            .expect("connect to live PostgreSQL");
        crate::db::migrate(&pool).await.expect("run migrations");

        let active_count = sqlx::query_scalar::<_, i64>(
            r#"
            select count(*)::bigint
            from (
                select 1
                from transcoding_sessions
                where status = 'running'
                  and lease_expires_at > now()
                order by lease_expires_at asc, id asc
                limit $1
            ) active_sessions
            "#,
        )
        .bind(3_i64)
        .fetch_one(&pool)
        .await
        .expect("transcode claim capacity probe should execute against live schema");

        assert!(active_count <= 3);
    }
}

fn normalize_output_base_path(path: &str) -> String {
    let normalized = path.trim().replace('\\', "/");
    let trimmed = normalized.trim_end_matches('/');
    if trimmed.is_empty() {
        ".".to_owned()
    } else {
        trimmed.to_owned()
    }
}

fn normalize_optional_client_id(value: Option<String>) -> Option<String> {
    value.and_then(|value| normalize_client_id(&value))
}

fn normalize_client_id(value: &str) -> Option<String> {
    let value = value.trim();
    if value.is_empty()
        || value.len() > 128
        || !value
            .chars()
            .all(|ch| ch.is_ascii_alphanumeric() || matches!(ch, '-' | '_' | '.' | ':'))
    {
        return None;
    }

    Some(value.to_owned())
}
