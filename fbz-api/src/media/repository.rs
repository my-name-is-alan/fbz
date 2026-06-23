use sqlx::{Row, postgres::PgRow};

use crate::db::DbPool;

#[derive(Clone)]
pub struct MediaRepository {
    pool: DbPool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PlaybackMediaSourceRecord {
    pub media_item_id: i64,
    pub item_id: String,
    pub item_type: String,
    pub media_file_id: i64,
    pub path: String,
    pub file_size: Option<i64>,
    pub is_strm: bool,
    pub strm_target: Option<String>,
    pub container: Option<String>,
    pub runtime_ticks: Option<i64>,
    pub bitrate: Option<i32>,
    pub supports_transcoding: bool,
    pub streams: Vec<PlaybackMediaStreamRecord>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PlaybackMediaStreamRecord {
    pub stream_index: i32,
    pub stream_type: String,
    pub codec: Option<String>,
    pub codec_tag: Option<String>,
    pub language: Option<String>,
    pub title: Option<String>,
    pub profile: Option<String>,
    pub level: Option<i32>,
    pub width: Option<i32>,
    pub height: Option<i32>,
    pub channels: Option<i32>,
    pub sample_rate: Option<i32>,
    pub bit_depth: Option<i32>,
    pub bitrate: Option<i32>,
    pub is_default: bool,
    pub is_forced: bool,
}

#[derive(Clone, Debug, PartialEq)]
pub struct SubtitleStreamRecord {
    pub media_item_id: i64,
    pub item_id: String,
    pub media_file_id: i64,
    pub media_path: String,
    pub stream_index: i32,
    pub codec: Option<String>,
    pub language: Option<String>,
    pub is_external: bool,
    pub extra: serde_json::Value,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PlaybackReportInput {
    pub user_id: i64,
    pub item_id: String,
    pub media_file_id: Option<i64>,
    pub client_session_id: Option<String>,
    pub position_ticks: i64,
    pub is_paused: bool,
    pub play_method: String,
}

#[derive(Clone, Debug, PartialEq)]
pub struct UserItemDataRecord {
    pub item_id: String,
    pub playback_position_ticks: i64,
    pub play_count: i32,
    pub is_favorite: bool,
    pub rating: Option<f64>,
    pub played: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ArtworkRecord {
    pub artwork_type: String,
    pub storage_key: Option<String>,
    pub remote_url: Option<String>,
    pub width: Option<i32>,
    pub height: Option<i32>,
}

impl MediaRepository {
    pub fn new(pool: DbPool) -> Self {
        Self { pool }
    }

    pub async fn find_playback_media_source(
        &self,
        user_id: i64,
        item_id: &str,
        media_file_id: Option<i64>,
    ) -> Result<Option<PlaybackMediaSourceRecord>, sqlx::Error> {
        self.find_media_source(user_id, item_id, media_file_id, false)
            .await
    }

    pub async fn find_download_media_source(
        &self,
        user_id: i64,
        item_id: &str,
        media_file_id: Option<i64>,
    ) -> Result<Option<PlaybackMediaSourceRecord>, sqlx::Error> {
        self.find_media_source(user_id, item_id, media_file_id, true)
            .await
    }

    pub async fn find_subtitle_stream(
        &self,
        user_id: i64,
        item_id: &str,
        media_file_id: i64,
        stream_index: i32,
    ) -> Result<Option<SubtitleStreamRecord>, sqlx::Error> {
        let row = sqlx::query(
            r#"
            select
                mi.id as media_item_id,
                mi.public_id::text as item_id,
                mf.id as media_file_id,
                mf.path as media_path,
                ms.stream_index,
                ms.codec,
                ms.language,
                ms.is_external,
                ms.extra
            from media_items mi
            join libraries l on l.id = mi.library_id
            join library_permissions lp on lp.library_id = mi.library_id
            join media_files mf on mf.media_item_id = mi.id
            join media_streams ms on ms.media_file_id = mf.id
            where mi.public_id = case
                when $2 ~* '^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$'
                then $2::uuid
                else null::uuid
            end
              and mf.id = $3
              and ms.stream_index = $4
              and ms.stream_type = 'subtitle'
              and lp.user_id = $1
              and lp.can_view = true
              and mi.is_deleted = false
              and l.is_hidden = false
            limit 1
            "#,
        )
        .bind(user_id)
        .bind(item_id)
        .bind(media_file_id)
        .bind(stream_index)
        .fetch_optional(&self.pool)
        .await?;

        row.map(SubtitleStreamRecord::from_row).transpose()
    }

    async fn find_media_source(
        &self,
        user_id: i64,
        item_id: &str,
        media_file_id: Option<i64>,
        require_download: bool,
    ) -> Result<Option<PlaybackMediaSourceRecord>, sqlx::Error> {
        let Some(source_row) = sqlx::query(
            r#"
            select
                mi.id as media_item_id,
                mi.public_id::text as item_id,
                mi.item_type,
                mf.id as media_file_id,
                mf.path,
                mf.file_size,
                mf.is_strm,
                mf.strm_target,
                mf.container,
                coalesce(mi.runtime_ticks, mf.duration_ticks) as runtime_ticks,
                mf.bitrate,
                (u.allow_transcode and lp.can_transcode) as supports_transcoding
            from media_items mi
            join libraries l on l.id = mi.library_id
            join library_permissions lp on lp.library_id = mi.library_id
            join users u on u.id = lp.user_id
            join media_files mf on mf.media_item_id = mi.id
            where mi.public_id = case
                when $2 ~* '^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$'
                then $2::uuid
                else null::uuid
            end
              and ($3::bigint is null or mf.id = $3)
              and lp.user_id = $1
              and lp.can_view = true
              and ($4::boolean = false or (u.allow_download = true and lp.can_download = true))
              and mi.is_deleted = false
              and l.is_hidden = false
            order by mf.is_primary desc, mf.id
            limit 1
            "#,
        )
        .bind(user_id)
        .bind(item_id)
        .bind(media_file_id)
        .bind(require_download)
        .fetch_optional(&self.pool)
        .await?
        else {
            return Ok(None);
        };

        let media_file_id = source_row.try_get::<i64, _>("media_file_id")?;
        let stream_rows = sqlx::query(
            r#"
            select
                stream_index,
                stream_type,
                codec,
                codec_tag,
                language,
                title,
                profile,
                level,
                width,
                height,
                channels,
                sample_rate,
                bit_depth,
                bitrate,
                is_default,
                is_forced
            from media_streams
            where media_file_id = $1
            order by stream_index
            "#,
        )
        .bind(media_file_id)
        .fetch_all(&self.pool)
        .await?;

        let streams = stream_rows
            .into_iter()
            .map(PlaybackMediaStreamRecord::from_row)
            .collect::<Result<Vec<_>, _>>()?;

        Ok(Some(PlaybackMediaSourceRecord {
            media_item_id: source_row.try_get("media_item_id")?,
            item_id: source_row.try_get("item_id")?,
            item_type: source_row.try_get("item_type")?,
            media_file_id,
            path: source_row.try_get("path")?,
            file_size: source_row.try_get("file_size")?,
            is_strm: source_row.try_get("is_strm")?,
            strm_target: source_row.try_get("strm_target")?,
            container: source_row.try_get("container")?,
            runtime_ticks: source_row.try_get("runtime_ticks")?,
            bitrate: source_row.try_get("bitrate")?,
            supports_transcoding: source_row.try_get("supports_transcoding")?,
            streams,
        }))
    }

    pub async fn start_playback(
        &self,
        input: PlaybackReportInput,
    ) -> Result<Option<String>, sqlx::Error> {
        let Some(target) = self
            .find_playback_target(input.user_id, &input.item_id, input.media_file_id)
            .await?
        else {
            return Ok(None);
        };

        let mut tx = self.pool.begin().await?;
        let session = sqlx::query(
            r#"
            insert into playback_sessions (
                user_id,
                media_item_id,
                media_file_id,
                play_method,
                position_ticks,
                is_paused,
                client_session_id
            )
            values ($1, $2, $3, $4, $5, $6, $7)
            returning public_id::text as public_id
            "#,
        )
        .bind(input.user_id)
        .bind(target.media_item_id)
        .bind(target.media_file_id)
        .bind(input.play_method)
        .bind(input.position_ticks)
        .bind(input.is_paused)
        .bind(input.client_session_id)
        .fetch_one(&mut *tx)
        .await?;

        upsert_user_playstate(
            &mut tx,
            input.user_id,
            target.media_item_id,
            input.position_ticks,
            false,
        )
        .await?;

        tx.commit().await?;

        Ok(Some(session.try_get("public_id")?))
    }

    pub async fn update_playback_progress(
        &self,
        input: PlaybackReportInput,
    ) -> Result<bool, sqlx::Error> {
        let Some(target) = self
            .find_playback_target(input.user_id, &input.item_id, input.media_file_id)
            .await?
        else {
            return Ok(false);
        };

        let mut tx = self.pool.begin().await?;
        let updated = update_recent_playback_session(
            &mut tx,
            input.user_id,
            target.media_item_id,
            input.client_session_id.as_deref(),
            input.position_ticks,
            input.is_paused,
            false,
        )
        .await?;

        if !updated {
            insert_playback_session(&mut tx, &input, &target, false).await?;
        }

        upsert_user_playstate(
            &mut tx,
            input.user_id,
            target.media_item_id,
            input.position_ticks,
            false,
        )
        .await?;

        tx.commit().await?;

        Ok(true)
    }

    pub async fn stop_playback(&self, input: PlaybackReportInput) -> Result<bool, sqlx::Error> {
        let Some(target) = self
            .find_playback_target(input.user_id, &input.item_id, input.media_file_id)
            .await?
        else {
            return Ok(false);
        };

        let played = target
            .runtime_ticks
            .filter(|runtime| *runtime > 0)
            .is_some_and(|runtime| {
                input.position_ticks.saturating_mul(10) >= runtime.saturating_mul(9)
            });
        let stored_position = if played { 0 } else { input.position_ticks };

        let mut tx = self.pool.begin().await?;
        let updated = update_recent_playback_session(
            &mut tx,
            input.user_id,
            target.media_item_id,
            input.client_session_id.as_deref(),
            input.position_ticks,
            input.is_paused,
            true,
        )
        .await?;

        if !updated {
            insert_playback_session(&mut tx, &input, &target, true).await?;
        }

        upsert_user_playstate(
            &mut tx,
            input.user_id,
            target.media_item_id,
            stored_position,
            played,
        )
        .await?;

        tx.commit().await?;

        Ok(true)
    }

    pub async fn set_item_played(
        &self,
        user_id: i64,
        item_id: &str,
        played: bool,
    ) -> Result<Option<UserItemDataRecord>, sqlx::Error> {
        let Some(target) = self.find_playback_target(user_id, item_id, None).await? else {
            return Ok(None);
        };

        sqlx::query(
            r#"
            insert into user_playstates (
                user_id,
                media_item_id,
                played,
                play_count,
                position_ticks,
                last_played_at,
                updated_at
            )
            values (
                $1,
                $2,
                $3,
                case when $3 then 1 else 0 end,
                0,
                case when $3 then now() else null end,
                now()
            )
            on conflict (user_id, media_item_id) do update
            set played = excluded.played,
                position_ticks = 0,
                play_count = case
                    when excluded.played and user_playstates.played then greatest(user_playstates.play_count, 1)
                    when excluded.played then user_playstates.play_count + 1
                    else 0
                end,
                last_played_at = case
                    when excluded.played then now()
                    else null
                end,
                updated_at = now()
            "#,
        )
        .bind(user_id)
        .bind(target.media_item_id)
        .bind(played)
        .execute(&self.pool)
        .await?;

        self.find_user_item_data_by_media_id(user_id, target.media_item_id)
            .await
    }

    pub async fn set_item_favorite(
        &self,
        user_id: i64,
        item_id: &str,
        is_favorite: bool,
    ) -> Result<Option<UserItemDataRecord>, sqlx::Error> {
        let Some(target) = self.find_playback_target(user_id, item_id, None).await? else {
            return Ok(None);
        };

        sqlx::query(
            r#"
            insert into user_playstates (
                user_id,
                media_item_id,
                is_favorite,
                updated_at
            )
            values ($1, $2, $3, now())
            on conflict (user_id, media_item_id) do update
            set is_favorite = excluded.is_favorite,
                updated_at = now()
            "#,
        )
        .bind(user_id)
        .bind(target.media_item_id)
        .bind(is_favorite)
        .execute(&self.pool)
        .await?;

        self.find_user_item_data_by_media_id(user_id, target.media_item_id)
            .await
    }

    pub async fn set_item_rating(
        &self,
        user_id: i64,
        item_id: &str,
        rating: Option<f64>,
    ) -> Result<Option<UserItemDataRecord>, sqlx::Error> {
        let Some(target) = self.find_playback_target(user_id, item_id, None).await? else {
            return Ok(None);
        };

        sqlx::query(
            r#"
            insert into user_playstates (
                user_id,
                media_item_id,
                rating,
                updated_at
            )
            values ($1, $2, $3::double precision::numeric(4, 2), now())
            on conflict (user_id, media_item_id) do update
            set rating = excluded.rating,
                updated_at = now()
            "#,
        )
        .bind(user_id)
        .bind(target.media_item_id)
        .bind(rating)
        .execute(&self.pool)
        .await?;

        self.find_user_item_data_by_media_id(user_id, target.media_item_id)
            .await
    }

    async fn find_user_item_data_by_media_id(
        &self,
        user_id: i64,
        media_item_id: i64,
    ) -> Result<Option<UserItemDataRecord>, sqlx::Error> {
        sqlx::query(
            r#"
            select
                mi.public_id::text as item_id,
                coalesce(up.position_ticks, 0) as playback_position_ticks,
                coalesce(up.play_count, 0) as play_count,
                coalesce(up.is_favorite, false) as is_favorite,
                up.rating::double precision as rating,
                coalesce(up.played, false) as played
            from media_items mi
            left join user_playstates up on up.media_item_id = mi.id
                and up.user_id = $1
            where mi.id = $2
            "#,
        )
        .bind(user_id)
        .bind(media_item_id)
        .fetch_optional(&self.pool)
        .await?
        .map(UserItemDataRecord::from_row)
        .transpose()
    }

    pub async fn find_item_artwork(
        &self,
        user_id: i64,
        item_id: &str,
        artwork_types: &[String],
        index: i64,
    ) -> Result<Option<ArtworkRecord>, sqlx::Error> {
        if artwork_types.is_empty() || index < 0 {
            return Ok(None);
        }

        sqlx::query(
            r#"
            with target_item as (
                select mi.id
                from media_items mi
                join libraries l on l.id = mi.library_id
                join library_permissions lp on lp.library_id = mi.library_id
                where mi.public_id = case
                    when $2 ~* '^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$'
                    then $2::uuid
                    else null::uuid
                end
                  and lp.user_id = $1
                  and lp.can_view = true
                  and mi.is_deleted = false
                  and l.is_hidden = false
            ),
            ranked_artwork as (
                select
                    a.artwork_type,
                    a.storage_key,
                    a.remote_url,
                    a.width,
                    a.height,
                    row_number() over (
                        order by
                            array_position($3::text[], a.artwork_type) asc,
                            a.is_primary desc,
                            a.id asc
                    ) - 1 as image_index
                from artwork a
                join target_item target on target.id = a.media_item_id
                where a.artwork_type = any($3::text[])
            )
            select
                artwork_type,
                storage_key,
                remote_url,
                width,
                height
            from ranked_artwork
            where image_index = $4
            "#,
        )
        .bind(user_id)
        .bind(item_id)
        .bind(artwork_types.to_vec())
        .bind(index)
        .fetch_optional(&self.pool)
        .await?
        .map(ArtworkRecord::from_row)
        .transpose()
    }

    async fn find_playback_target(
        &self,
        user_id: i64,
        item_id: &str,
        media_file_id: Option<i64>,
    ) -> Result<Option<PlaybackTargetRecord>, sqlx::Error> {
        sqlx::query(
            r#"
            select
                mi.id as media_item_id,
                coalesce(mi.runtime_ticks, mf.duration_ticks) as runtime_ticks,
                mf.id as media_file_id
            from media_items mi
            join libraries l on l.id = mi.library_id
            join library_permissions lp on lp.library_id = mi.library_id
            left join lateral (
                select id
                from media_files mf
                where mf.media_item_id = mi.id
                  and ($3::bigint is null or mf.id = $3)
                order by mf.is_primary desc, mf.id
                limit 1
            ) mf on true
            where mi.public_id = case
                when $2 ~* '^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$'
                then $2::uuid
                else null::uuid
            end
              and lp.user_id = $1
              and lp.can_view = true
              and mi.is_deleted = false
              and l.is_hidden = false
            "#,
        )
        .bind(user_id)
        .bind(item_id)
        .bind(media_file_id)
        .fetch_optional(&self.pool)
        .await?
        .map(PlaybackTargetRecord::from_row)
        .transpose()
    }
}

impl PlaybackMediaStreamRecord {
    fn from_row(row: PgRow) -> Result<Self, sqlx::Error> {
        Ok(Self {
            stream_index: row.try_get("stream_index")?,
            stream_type: row.try_get("stream_type")?,
            codec: row.try_get("codec")?,
            codec_tag: row.try_get("codec_tag")?,
            language: row.try_get("language")?,
            title: row.try_get("title")?,
            profile: row.try_get("profile")?,
            level: row.try_get("level")?,
            width: row.try_get("width")?,
            height: row.try_get("height")?,
            channels: row.try_get("channels")?,
            sample_rate: row.try_get("sample_rate")?,
            bit_depth: row.try_get("bit_depth")?,
            bitrate: row.try_get("bitrate")?,
            is_default: row.try_get("is_default")?,
            is_forced: row.try_get("is_forced")?,
        })
    }
}

impl SubtitleStreamRecord {
    fn from_row(row: PgRow) -> Result<Self, sqlx::Error> {
        Ok(Self {
            media_item_id: row.try_get("media_item_id")?,
            item_id: row.try_get("item_id")?,
            media_file_id: row.try_get("media_file_id")?,
            media_path: row.try_get("media_path")?,
            stream_index: row.try_get("stream_index")?,
            codec: row.try_get("codec")?,
            language: row.try_get("language")?,
            is_external: row.try_get("is_external")?,
            extra: row.try_get("extra")?,
        })
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct PlaybackTargetRecord {
    media_item_id: i64,
    media_file_id: Option<i64>,
    runtime_ticks: Option<i64>,
}

impl PlaybackTargetRecord {
    fn from_row(row: PgRow) -> Result<Self, sqlx::Error> {
        Ok(Self {
            media_item_id: row.try_get("media_item_id")?,
            media_file_id: row.try_get("media_file_id")?,
            runtime_ticks: row.try_get("runtime_ticks")?,
        })
    }
}

impl UserItemDataRecord {
    fn from_row(row: PgRow) -> Result<Self, sqlx::Error> {
        Ok(Self {
            item_id: row.try_get("item_id")?,
            playback_position_ticks: row.try_get("playback_position_ticks")?,
            play_count: row.try_get("play_count")?,
            is_favorite: row.try_get("is_favorite")?,
            rating: row.try_get("rating")?,
            played: row.try_get("played")?,
        })
    }
}

impl ArtworkRecord {
    fn from_row(row: PgRow) -> Result<Self, sqlx::Error> {
        Ok(Self {
            artwork_type: row.try_get("artwork_type")?,
            storage_key: row.try_get("storage_key")?,
            remote_url: row.try_get("remote_url")?,
            width: row.try_get("width")?,
            height: row.try_get("height")?,
        })
    }
}

async fn insert_playback_session(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    input: &PlaybackReportInput,
    target: &PlaybackTargetRecord,
    stopped: bool,
) -> Result<(), sqlx::Error> {
    sqlx::query(
        r#"
        insert into playback_sessions (
            user_id,
            media_item_id,
            media_file_id,
            play_method,
            position_ticks,
            is_paused,
            client_session_id,
            stopped_at
        )
        values ($1, $2, $3, $4, $5, $6, $7, case when $8 then now() else null end)
        "#,
    )
    .bind(input.user_id)
    .bind(target.media_item_id)
    .bind(target.media_file_id)
    .bind(&input.play_method)
    .bind(input.position_ticks)
    .bind(input.is_paused)
    .bind(&input.client_session_id)
    .bind(stopped)
    .execute(&mut **tx)
    .await?;

    Ok(())
}

async fn update_recent_playback_session(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    user_id: i64,
    media_item_id: i64,
    client_session_id: Option<&str>,
    position_ticks: i64,
    is_paused: bool,
    stopped: bool,
) -> Result<bool, sqlx::Error> {
    let result = sqlx::query(
        r#"
        with candidate as (
            select id
            from playback_sessions
            where user_id = $1
              and media_item_id = $2
              and stopped_at is null
              and ($3::text is null or client_session_id = $3)
            order by last_progress_at desc, id desc
            limit 1
        )
        update playback_sessions ps
        set position_ticks = $4,
            is_paused = $5,
            last_progress_at = now(),
            client_session_id = coalesce(ps.client_session_id, $3),
            stopped_at = case when $6 then now() else ps.stopped_at end
        from candidate
        where ps.id = candidate.id
        "#,
    )
    .bind(user_id)
    .bind(media_item_id)
    .bind(client_session_id)
    .bind(position_ticks)
    .bind(is_paused)
    .bind(stopped)
    .execute(&mut **tx)
    .await?;

    Ok(result.rows_affected() > 0)
}

async fn upsert_user_playstate(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    user_id: i64,
    media_item_id: i64,
    position_ticks: i64,
    played: bool,
) -> Result<(), sqlx::Error> {
    sqlx::query(
        r#"
        insert into user_playstates (
            user_id,
            media_item_id,
            position_ticks,
            played,
            play_count,
            last_played_at,
            updated_at
        )
        values (
            $1,
            $2,
            $3,
            $4,
            case when $4 then 1 else 0 end,
            case when $4 then now() else null end,
            now()
        )
        on conflict (user_id, media_item_id) do update
        set position_ticks = excluded.position_ticks,
            played = excluded.played,
            play_count = user_playstates.play_count + case when excluded.played then 1 else 0 end,
            last_played_at = case
                when excluded.played then now()
                else user_playstates.last_played_at
            end,
            updated_at = now()
        "#,
    )
    .bind(user_id)
    .bind(media_item_id)
    .bind(position_ticks)
    .bind(played)
    .execute(&mut **tx)
    .await?;

    Ok(())
}
