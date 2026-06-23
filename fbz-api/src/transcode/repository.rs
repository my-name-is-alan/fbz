use sqlx::{Row, postgres::PgRow};

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

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CreateTranscodeSessionInput {
    pub user_id: i64,
    pub media_item_id: i64,
    pub media_file_id: Option<i64>,
    pub input_path: String,
    pub output_base_path: String,
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

const TRANSCODE_UPDATE_TERMINAL_STATUS_SQL: &str = r#"
        update transcoding_sessions
        set status = $2,
            worker_id = null,
            lease_expires_at = null,
            error_message = $3,
            finished_at = coalesce(finished_at, now()),
            updated_at = now()
        where public_id = case
            when $1::text ~* '^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$'
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
                    bitrate
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
                    $9
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
        .fetch_one(&self.pool)
        .await?;

        TranscodeSessionRecord::from_row(row)
    }

    pub async fn list_sessions(
        &self,
        limit: i64,
    ) -> Result<Vec<TranscodeSessionRecord>, sqlx::Error> {
        let rows = sqlx::query(
            r#"
            select public_id::text as id,
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
            from transcoding_sessions
            order by case status
                         when 'running' then 0
                         when 'queued' then 1
                         when 'failed' then 2
                         else 3
                     end,
                     created_at desc,
                     id desc
            limit $1
            "#,
        )
        .bind(limit)
        .fetch_all(&self.pool)
        .await?;

        rows.into_iter()
            .map(TranscodeSessionRecord::from_row)
            .collect()
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
            from transcoding_sessions
            where status = 'running'
              and lease_expires_at > now()
            "#,
        )
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
) -> Result<(), sqlx::Error> {
    sqlx::query(
        r#"
        update transcoding_sessions
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
        where status = 'running'
          and lease_expires_at <= now()
        "#,
    )
    .execute(&mut **tx)
    .await?;

    Ok(())
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
