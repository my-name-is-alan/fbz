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

#[derive(Clone, Debug, PartialEq)]
pub struct UserItemDataUpdateInput {
    pub user_id: i64,
    pub item_id: String,
    pub playback_position_ticks: Option<i64>,
    pub play_count: Option<i32>,
    pub is_favorite: Option<bool>,
    pub rating: Option<f64>,
    pub played: Option<bool>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ArtworkRecord {
    pub artwork_type: String,
    pub storage_key: Option<String>,
    pub remote_url: Option<String>,
    pub width: Option<i32>,
    pub height: Option<i32>,
}

/// 图片写入入参（上传/远程下载共用）。`make_primary` 为 true 时同类型旧主图降级。
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct InsertArtworkInput {
    pub item_id: String,
    pub artwork_type: String,
    pub source: String,
    pub storage_key: Option<String>,
    pub remote_url: Option<String>,
    pub width: Option<i32>,
    pub height: Option<i32>,
    pub make_primary: bool,
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

    /// 列出条目全部可播放文件 id（多版本），主文件优先。带库权限过滤，上限 32 个版本。
    pub async fn list_playback_media_file_ids(
        &self,
        user_id: i64,
        item_id: &str,
    ) -> Result<Vec<i64>, sqlx::Error> {
        let rows = sqlx::query(
            r#"
            select mf.id
            from media_items mi
            join libraries l on l.id = mi.library_id
            join library_permissions lp on lp.library_id = mi.library_id
            join media_files mf on mf.media_item_id = mi.id
            where mi.public_id = case
                when $2 ~* '^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$'
                then $2::uuid
                else null::uuid
            end
              and lp.user_id = $1
              and lp.can_view = true
              and mi.is_deleted = false
              and l.is_hidden = false
            order by mf.is_primary desc, mf.id
            limit 32
            "#,
        )
        .bind(user_id)
        .bind(item_id)
        .fetch_all(&self.pool)
        .await?;

        rows.into_iter().map(|row| row.try_get::<i64, _>("id")).collect()
    }

    /// 取内嵌附件流（字体等，stream_type='attachment'），带库权限过滤。
    /// 复用 [`SubtitleStreamRecord`] 形状（media_path + stream_index 足够抽取）。
    pub async fn find_attachment_stream(
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
              and ms.stream_type = 'attachment'
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

    pub async fn find_user_item_data(
        &self,
        user_id: i64,
        item_id: &str,
    ) -> Result<Option<UserItemDataRecord>, sqlx::Error> {
        let Some(target) = self.find_playback_target(user_id, item_id, None).await? else {
            return Ok(None);
        };

        self.find_user_item_data_by_media_id(user_id, target.media_item_id)
            .await
    }

    pub async fn update_user_item_data(
        &self,
        input: UserItemDataUpdateInput,
    ) -> Result<Option<UserItemDataRecord>, sqlx::Error> {
        let Some(target) = self
            .find_playback_target(input.user_id, &input.item_id, None)
            .await?
        else {
            return Ok(None);
        };

        sqlx::query(
            r#"
            insert into user_playstates (
                user_id,
                media_item_id,
                played,
                position_ticks,
                play_count,
                is_favorite,
                rating,
                last_played_at,
                updated_at
            )
            values (
                $1,
                $2,
                coalesce($3::boolean, false),
                coalesce($4::bigint, 0),
                coalesce($5::integer, 0),
                coalesce($6::boolean, false),
                case
                    when $7::boolean then $8::double precision::numeric(4, 2)
                    else null
                end,
                case when coalesce($3::boolean, false) then now() else null end,
                now()
            )
            on conflict (user_id, media_item_id) do update
            set played = coalesce($3::boolean, user_playstates.played),
                position_ticks = coalesce($4::bigint, user_playstates.position_ticks),
                play_count = coalesce($5::integer, user_playstates.play_count),
                is_favorite = coalesce($6::boolean, user_playstates.is_favorite),
                rating = case
                    when $7::boolean then $8::double precision::numeric(4, 2)
                    else user_playstates.rating
                end,
                last_played_at = case
                    when $3::boolean is true then now()
                    when $3::boolean is false then null
                    else user_playstates.last_played_at
                end,
                updated_at = now()
            "#,
        )
        .bind(input.user_id)
        .bind(target.media_item_id)
        .bind(input.played)
        .bind(input.playback_position_ticks)
        .bind(input.play_count)
        .bind(input.is_favorite)
        .bind(input.rating.is_some())
        .bind(input.rating)
        .execute(&self.pool)
        .await?;

        self.find_user_item_data_by_media_id(input.user_id, target.media_item_id)
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

    /// 解析一个对用户可见的图片条目 public_id → 内部 bigint id（用于拼缩略图文件名）。
    /// 走与 artwork 查询同样的 can_view 权限过滤；只匹配未删除、非隐藏库的 photo 条目。
    /// 返回整数 id（无法目录穿越），None 时调用方退回 404。
    pub async fn find_photo_thumbnail_item_id(
        &self,
        user_id: i64,
        item_id: &str,
    ) -> Result<Option<i64>, sqlx::Error> {
        sqlx::query_scalar::<_, i64>(
            r#"
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
              and mi.item_type = 'photo'
              and mi.is_deleted = false
              and l.is_hidden = false
            limit 1
            "#,
        )
        .bind(user_id)
        .bind(item_id)
        .fetch_optional(&self.pool)
        .await
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
                            a.sort_order asc,
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

    /// 按人物名取其头像（primary 等）artwork。可见性同 `find_user_person_detail_by_name`：
    /// 该人参演的条目里存在用户可见、非隐藏库的项。返回复用 [`ArtworkRecord`]。
    /// 只存 TMDB CDN 的 remote_url（元数据管线不下载字节），serving 端直通远端 URL。
    pub async fn find_person_artwork_by_name(
        &self,
        user_id: i64,
        name: &str,
        artwork_types: &[String],
        index: i64,
    ) -> Result<Option<ArtworkRecord>, sqlx::Error> {
        if artwork_types.is_empty() || index < 0 {
            return Ok(None);
        }

        sqlx::query(
            r#"
            select
                a.artwork_type,
                a.storage_key,
                a.remote_url,
                a.width,
                a.height
            from artwork a
            join people p on p.id = a.person_id
            where lower(p.name) = lower($2)
              and a.artwork_type = any($3::text[])
              and exists (
                    select 1
                    from media_item_people mip
                    join media_items mi on mi.id = mip.media_item_id
                    join libraries l on l.id = mi.library_id
                    join library_permissions lp on lp.library_id = mi.library_id
                    where mip.person_id = p.id
                      and lp.user_id = $1
                      and lp.can_view = true
                      and mi.is_deleted = false
                      and l.is_hidden = false
              )
            order by
                array_position($3::text[], a.artwork_type) asc,
                a.is_primary desc,
                a.sort_order asc,
                a.id asc
            offset $4
            limit 1
            "#,
        )
        .bind(user_id)
        .bind(name.trim())
        .bind(artwork_types.to_vec())
        .bind(index)
        .fetch_optional(&self.pool)
        .await?
        .map(ArtworkRecord::from_row)
        .transpose()
    }

    pub async fn list_item_artwork(
        &self,
        user_id: i64,
        item_id: &str,
    ) -> Result<Vec<ArtworkRecord>, sqlx::Error> {
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
            )
            select
                a.artwork_type,
                a.storage_key,
                a.remote_url,
                a.width,
                a.height
            from artwork a
            join target_item target on target.id = a.media_item_id
            order by a.artwork_type, a.is_primary desc, a.sort_order, a.id
            "#,
        )
        .bind(user_id)
        .bind(item_id)
        .fetch_all(&self.pool)
        .await?
        .into_iter()
        .map(ArtworkRecord::from_row)
        .collect()
    }

    /// 写入一张条目图片（上传/远程下载落盘后调用）。`make_primary` 时同事务把同
    /// 类型旧主图降级。调用方已完成 admin + 条目可见性校验。返回 false = 条目不存在。
    pub async fn insert_item_artwork(
        &self,
        input: InsertArtworkInput,
    ) -> Result<bool, sqlx::Error> {
        let mut tx = self.pool.begin().await?;

        let Some(internal_id) = resolve_item_internal_id(&mut tx, &input.item_id).await? else {
            return Ok(false);
        };

        if input.make_primary {
            sqlx::query(
                r#"
                update artwork
                set is_primary = false
                where media_item_id = $1
                  and artwork_type = $2
                  and is_primary = true
                "#,
            )
            .bind(internal_id)
            .bind(&input.artwork_type)
            .execute(&mut *tx)
            .await?;
        }

        sqlx::query(
            r#"
            insert into artwork (
                media_item_id,
                artwork_type,
                source,
                storage_key,
                remote_url,
                width,
                height,
                is_primary,
                sort_order
            )
            values (
                $1, $2, $3, $4, $5, $6, $7, $8,
                coalesce(
                    (
                        select max(sort_order) + 1
                        from artwork
                        where media_item_id = $1
                          and artwork_type = $2
                    ),
                    0
                )
            )
            "#,
        )
        .bind(internal_id)
        .bind(&input.artwork_type)
        .bind(&input.source)
        .bind(&input.storage_key)
        .bind(&input.remote_url)
        .bind(input.width)
        .bind(input.height)
        .bind(input.make_primary)
        .execute(&mut *tx)
        .await?;

        tx.commit().await?;

        Ok(true)
    }

    /// 删除条目在 Emby 类型序（同 `find_item_artwork` 的 rank 规则）中第 `index`
    /// 张图。返回 `Ok(None)` = 条目或该序号的图不存在；`Ok(Some(storage_key))` =
    /// 已删除，storage_key 交调用方清理磁盘缓存。
    pub async fn delete_item_artwork_at_index(
        &self,
        item_id: &str,
        artwork_types: &[String],
        index: i64,
    ) -> Result<Option<Option<String>>, sqlx::Error> {
        if artwork_types.is_empty() || index < 0 {
            return Ok(None);
        }

        let mut tx = self.pool.begin().await?;

        let Some(internal_id) = resolve_item_internal_id(&mut tx, item_id).await? else {
            return Ok(None);
        };

        let row = sqlx::query(
            r#"
            with ranked_artwork as (
                select
                    a.id,
                    row_number() over (
                        order by
                            array_position($2::text[], a.artwork_type) asc,
                            a.is_primary desc,
                            a.sort_order asc,
                            a.id asc
                    ) - 1 as image_index
                from artwork a
                where a.media_item_id = $1
                  and a.artwork_type = any($2::text[])
            )
            delete from artwork a
            using ranked_artwork ranked
            where a.id = ranked.id
              and ranked.image_index = $3
            returning a.storage_key
            "#,
        )
        .bind(internal_id)
        .bind(artwork_types.to_vec())
        .bind(index)
        .fetch_optional(&mut *tx)
        .await?;

        let Some(row) = row else {
            return Ok(None);
        };
        let storage_key = row.try_get::<Option<String>, _>("storage_key")?;

        tx.commit().await?;

        Ok(Some(storage_key))
    }

    /// 把类型序中第 `index` 张图移动到 `new_index`（0 基，越界钳制到末尾），事务内
    /// 对该类型全量重排 sort_order，并把新首位设为主图。返回 false = 目标不存在。
    pub async fn reindex_item_artwork(
        &self,
        item_id: &str,
        artwork_types: &[String],
        index: i64,
        new_index: i64,
    ) -> Result<bool, sqlx::Error> {
        if artwork_types.is_empty() || index < 0 || new_index < 0 {
            return Ok(false);
        }

        let mut tx = self.pool.begin().await?;

        let Some(internal_id) = resolve_item_internal_id(&mut tx, item_id).await? else {
            return Ok(false);
        };

        let rows = sqlx::query(
            r#"
            select a.id
            from artwork a
            where a.media_item_id = $1
              and a.artwork_type = any($2::text[])
            order by
                array_position($2::text[], a.artwork_type) asc,
                a.is_primary desc,
                a.sort_order asc,
                a.id asc
            "#,
        )
        .bind(internal_id)
        .bind(artwork_types.to_vec())
        .fetch_all(&mut *tx)
        .await?;
        let mut ordered = rows
            .into_iter()
            .map(|row| row.try_get::<i64, _>("id"))
            .collect::<Result<Vec<_>, _>>()?;

        let current_index = index as usize;
        if current_index >= ordered.len() {
            return Ok(false);
        }
        let target_index = (new_index as usize).min(ordered.len().saturating_sub(1));
        if target_index != current_index {
            let moved = ordered.remove(current_index);
            ordered.insert(target_index, moved);
        }

        let orders = (0..ordered.len() as i32).collect::<Vec<_>>();
        sqlx::query(
            r#"
            update artwork a
            set sort_order = updates.sort_order,
                is_primary = (updates.sort_order = 0)
            from (
                select unnest($2::bigint[]) as id,
                       unnest($3::integer[]) as sort_order
            ) updates
            where a.media_item_id = $1
              and a.id = updates.id
            "#,
        )
        .bind(internal_id)
        .bind(&ordered)
        .bind(&orders)
        .execute(&mut *tx)
        .await?;

        tx.commit().await?;

        Ok(true)
    }

    /// 手动识别（RemoteSearch/Apply）：写入外部 provider id 并把元数据状态置回
    /// pending（调用方随后入队 metadata.refresh）。同一 (provider, external_id)
    /// 已被其他条目占用时返回该 unique 冲突的数据库错误，由路由映射 409。
    /// 返回 false = 条目不存在。
    pub async fn apply_item_external_ids(
        &self,
        item_id: &str,
        external_ids: &[(String, String)],
    ) -> Result<bool, sqlx::Error> {
        let mut tx = self.pool.begin().await?;

        let Some(internal_id) = resolve_item_internal_id(&mut tx, item_id).await? else {
            return Ok(false);
        };

        for (provider, external_id) in external_ids {
            sqlx::query(
                r#"
                insert into media_external_ids (media_item_id, provider, external_id)
                values ($1, $2, $3)
                on conflict (media_item_id, provider)
                do update set external_id = excluded.external_id
                "#,
            )
            .bind(internal_id)
            .bind(provider)
            .bind(external_id)
            .execute(&mut *tx)
            .await?;
        }

        sqlx::query(
            r#"
            update media_items
            set metadata_status = 'pending',
                updated_at = now()
            where id = $1
            "#,
        )
        .bind(internal_id)
        .execute(&mut *tx)
        .await?;

        tx.commit().await?;

        Ok(true)
    }

    /// 多版本合并（Emby `Videos/MergeVersions`）：把其余条目的媒体文件全部挂到
    /// 首个条目名下（成为该条目的备选版本），其余条目软删除、播放状态并入。
    /// 返回 None = 可解析条目不足两个。
    pub async fn merge_video_versions(&self, item_ids: &[String]) -> Result<Option<u64>, sqlx::Error> {
        let mut tx = self.pool.begin().await?;

        let mut internal_ids = Vec::new();
        for item_id in item_ids {
            if let Some(internal_id) = resolve_item_internal_id(&mut tx, item_id).await?
                && !internal_ids.contains(&internal_id)
            {
                internal_ids.push(internal_id);
            }
        }
        if internal_ids.len() < 2 {
            return Ok(None);
        }
        let target_id = internal_ids[0];
        let source_ids = internal_ids[1..].to_vec();

        let moved_files = sqlx::query(
            r#"
            update media_files
            set is_primary = false,
                media_item_id = $1,
                updated_at = now()
            where media_item_id = any($2::bigint[])
            "#,
        )
        .bind(target_id)
        .bind(&source_ids)
        .execute(&mut *tx)
        .await?
        .rows_affected();

        // 保证合并后有且仅有一个主文件（目标原主文件优先，其次最早的文件）。
        sqlx::query(
            r#"
            update media_files
            set is_primary = (media_files.id = keeper.id)
            from (
                select id
                from media_files
                where media_item_id = $1
                order by is_primary desc, id asc
                limit 1
            ) keeper
            where media_files.media_item_id = $1
            "#,
        )
        .bind(target_id)
        .execute(&mut *tx)
        .await?;

        // 播放状态并入：目标无该用户状态时改挂，冲突行丢弃（保留目标侧进度）。
        sqlx::query(
            r#"
            update user_playstates up
            set media_item_id = $1,
                updated_at = now()
            where up.media_item_id = any($2::bigint[])
              and not exists (
                    select 1
                    from user_playstates target_state
                    where target_state.user_id = up.user_id
                      and target_state.media_item_id = $1
              )
            "#,
        )
        .bind(target_id)
        .bind(&source_ids)
        .execute(&mut *tx)
        .await?;
        sqlx::query("delete from user_playstates where media_item_id = any($1::bigint[])")
            .bind(&source_ids)
            .execute(&mut *tx)
            .await?;

        sqlx::query(
            r#"
            update media_items
            set is_deleted = true,
                updated_at = now()
            where id = any($1::bigint[])
            "#,
        )
        .bind(&source_ids)
        .execute(&mut *tx)
        .await?;

        tx.commit().await?;

        Ok(Some(moved_files))
    }

    /// 拆分多版本（Emby `DELETE Videos/{Id}/AlternateSources`）：条目的每个非主
    /// 文件独立成新条目（克隆标题/归属/元数据字段，metadata_status 置 pending 待重刮），
    /// 文件改挂新条目并成为其主文件。返回 None = 条目不存在；Some(n) = 拆出的版本数。
    pub async fn split_video_alternate_sources(
        &self,
        item_id: &str,
    ) -> Result<Option<u64>, sqlx::Error> {
        let mut tx = self.pool.begin().await?;

        let Some(internal_id) = resolve_item_internal_id(&mut tx, item_id).await? else {
            return Ok(None);
        };

        let rows = sqlx::query(
            r#"
            select id
            from media_files
            where media_item_id = $1
              and is_primary = false
            order by id asc
            "#,
        )
        .bind(internal_id)
        .fetch_all(&mut *tx)
        .await?;
        let file_ids = rows
            .into_iter()
            .map(|row| row.try_get::<i64, _>("id"))
            .collect::<Result<Vec<_>, _>>()?;

        let mut split_count = 0u64;
        for file_id in file_ids {
            let new_item_id = sqlx::query_scalar::<_, i64>(
                r#"
                insert into media_items (
                    library_id,
                    parent_id,
                    item_type,
                    title,
                    original_title,
                    sort_title,
                    overview,
                    production_year,
                    premiere_date,
                    community_rating,
                    critic_rating,
                    runtime_ticks,
                    index_number,
                    parent_index_number,
                    season_number,
                    episode_number,
                    metadata_status,
                    scan_status,
                    pinyin_full,
                    pinyin_initials
                )
                select
                    library_id,
                    parent_id,
                    item_type,
                    title,
                    original_title,
                    sort_title,
                    overview,
                    production_year,
                    premiere_date,
                    community_rating,
                    critic_rating,
                    runtime_ticks,
                    index_number,
                    parent_index_number,
                    season_number,
                    episode_number,
                    'pending',
                    scan_status,
                    pinyin_full,
                    pinyin_initials
                from media_items
                where id = $1
                returning id
                "#,
            )
            .bind(internal_id)
            .fetch_one(&mut *tx)
            .await?;

            sqlx::query(
                r#"
                update media_files
                set media_item_id = $1,
                    is_primary = true,
                    updated_at = now()
                where id = $2
                "#,
            )
            .bind(new_item_id)
            .bind(file_id)
            .execute(&mut *tx)
            .await?;
            split_count += 1;
        }

        tx.commit().await?;

        Ok(Some(split_count))
    }

    /// 取条目在某 provider 的外部 id（RemoteImages 反查 TMDB id 用）。
    pub async fn find_item_external_id(
        &self,
        item_id: &str,
        provider: &str,
    ) -> Result<Option<String>, sqlx::Error> {
        let row = sqlx::query(
            r#"
            select mei.external_id
            from media_external_ids mei
            join media_items mi on mi.id = mei.media_item_id
            where mi.public_id = case
                when $1 ~* '^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$'
                then $1::uuid
                else null::uuid
            end
              and mei.provider = $2
              and mi.is_deleted = false
            "#,
        )
        .bind(item_id)
        .bind(provider)
        .fetch_optional(&self.pool)
        .await?;

        row.map(|row| row.try_get::<String, _>("external_id"))
            .transpose()
    }

    /// 元数据重置：清空外部 id 关联并把 metadata_status 置回 pending
    /// （调用方随后入队 metadata.refresh 重刮）。返回 false = 条目不存在。
    pub async fn reset_item_metadata(&self, item_id: &str) -> Result<bool, sqlx::Error> {
        let mut tx = self.pool.begin().await?;

        let Some(internal_id) = resolve_item_internal_id(&mut tx, item_id).await? else {
            return Ok(false);
        };

        sqlx::query("delete from media_external_ids where media_item_id = $1")
            .bind(internal_id)
            .execute(&mut *tx)
            .await?;
        sqlx::query(
            r#"
            update media_items
            set metadata_status = 'pending',
                updated_at = now()
            where id = $1
            "#,
        )
        .bind(internal_id)
        .execute(&mut *tx)
        .await?;

        tx.commit().await?;

        Ok(true)
    }

    /// 更新类型序中第 `index` 张图的远端 URL；该序号无图时插入一张新的远端图。
    /// 返回 false = 条目不存在。
    pub async fn update_item_artwork_url(
        &self,
        item_id: &str,
        artwork_types: &[String],
        index: i64,
        remote_url: &str,
    ) -> Result<bool, sqlx::Error> {
        if artwork_types.is_empty() || index < 0 {
            return Ok(false);
        }

        let mut tx = self.pool.begin().await?;

        let Some(internal_id) = resolve_item_internal_id(&mut tx, item_id).await? else {
            return Ok(false);
        };

        let updated = sqlx::query(
            r#"
            with ranked_artwork as (
                select
                    a.id,
                    row_number() over (
                        order by
                            array_position($2::text[], a.artwork_type) asc,
                            a.is_primary desc,
                            a.sort_order asc,
                            a.id asc
                    ) - 1 as image_index
                from artwork a
                where a.media_item_id = $1
                  and a.artwork_type = any($2::text[])
            )
            update artwork a
            set remote_url = $4
            from ranked_artwork ranked
            where a.id = ranked.id
              and ranked.image_index = $3
            "#,
        )
        .bind(internal_id)
        .bind(artwork_types.to_vec())
        .bind(index)
        .bind(remote_url)
        .execute(&mut *tx)
        .await?
        .rows_affected();

        if updated == 0 {
            let artwork_type = artwork_types
                .first()
                .map(String::as_str)
                .unwrap_or("primary");
            sqlx::query(
                r#"
                insert into artwork (
                    media_item_id,
                    artwork_type,
                    source,
                    remote_url,
                    is_primary,
                    sort_order
                )
                values (
                    $1, $2, 'manual', $3,
                    not exists (
                        select 1
                        from artwork
                        where media_item_id = $1
                          and artwork_type = $2
                          and is_primary = true
                    ),
                    coalesce(
                        (
                            select max(sort_order) + 1
                            from artwork
                            where media_item_id = $1
                              and artwork_type = $2
                        ),
                        0
                    )
                )
                "#,
            )
            .bind(internal_id)
            .bind(artwork_type)
            .bind(remote_url)
            .execute(&mut *tx)
            .await?;
        }

        tx.commit().await?;

        Ok(true)
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

/// 图片写侧定位条目内部 id（UUID 守卫，不做库权限过滤——写入口已由路由层做
/// admin + 可见性校验）。行锁串行化同一条目的并发图片写。
async fn resolve_item_internal_id(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    item_id: &str,
) -> Result<Option<i64>, sqlx::Error> {
    let row = sqlx::query(
        r#"
        select mi.id
        from media_items mi
        where mi.public_id = case
            when $1 ~* '^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$'
            then $1::uuid
            else null::uuid
        end
          and mi.is_deleted = false
        for update
        "#,
    )
    .bind(item_id)
    .fetch_optional(&mut **tx)
    .await?;

    row.map(|row| row.try_get::<i64, _>("id")).transpose()
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
