use sqlx::{Row, postgres::PgRow};

use crate::db::DbPool;

const PUBLIC_USER_LIMIT: i64 = 1_000;

#[derive(Clone)]
pub struct UsersRepository {
    pool: DbPool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PublicUserRecord {
    pub id: String,
    pub name: String,
    pub has_password: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct UserDetailRecord {
    pub id: String,
    pub name: String,
    pub has_password: bool,
    pub is_administrator: bool,
    pub is_disabled: bool,
    pub allow_download: bool,
    pub allow_transcode: bool,
    pub allow_new_device_login: bool,
    pub enable_content_downloading: bool,
    pub enable_playback_transcoding: bool,
    pub enable_all_folders: bool,
    pub enabled_folders: Vec<String>,
}

impl UsersRepository {
    pub fn new(pool: DbPool) -> Self {
        Self { pool }
    }

    pub async fn list_public_users(&self) -> Result<Vec<PublicUserRecord>, sqlx::Error> {
        let rows = sqlx::query(
            r#"
            select
                public_id::text as id,
                username as name,
                password_hash is not null as has_password
            from users
            where is_disabled = false
            order by username_normalized, id
            limit $1
            "#,
        )
        .bind(PUBLIC_USER_LIMIT)
        .fetch_all(&self.pool)
        .await?;

        rows.into_iter().map(PublicUserRecord::from_row).collect()
    }

    pub async fn find_user_by_public_id(
        &self,
        user_id: &str,
    ) -> Result<Option<UserDetailRecord>, sqlx::Error> {
        sqlx::query(
            r#"
            select
                u.public_id::text as id,
                u.username as name,
                u.password_hash is not null as has_password,
                r.name_normalized in ('owner', 'admin', 'administrator') as is_administrator,
                u.is_disabled,
                u.allow_download,
                u.allow_transcode,
                u.allow_new_device_login,
                (
                    u.allow_download
                    and coalesce(accessible_libraries.has_downloadable_library, false)
                ) as enable_content_downloading,
                (
                    u.allow_transcode
                    and coalesce(accessible_libraries.has_transcodable_library, false)
                ) as enable_playback_transcoding,
                (
                    coalesce(visible_libraries.visible_count, 0) = 0
                    or coalesce(accessible_libraries.accessible_count, 0)
                       = coalesce(visible_libraries.visible_count, 0)
                ) as enable_all_folders,
                coalesce(accessible_libraries.enabled_folders, array[]::text[]) as enabled_folders
            from users u
            join roles r on r.id = u.role_id
            left join lateral (
                select count(*)::bigint as visible_count
                from libraries l
                where l.is_hidden = false
            ) visible_libraries on true
            left join lateral (
                select count(*)::bigint as accessible_count,
                       array_agg(l.public_id::text order by l.name asc, l.id asc) as enabled_folders,
                       bool_or(lp.can_download) as has_downloadable_library,
                       bool_or(lp.can_transcode) as has_transcodable_library
                from libraries l
                join library_permissions lp
                  on lp.library_id = l.id
                 and lp.user_id = u.id
                 and lp.can_view = true
                where l.is_hidden = false
            ) accessible_libraries on true
            where u.public_id = case
                when $1 ~* '^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$'
                then $1::uuid
                else null::uuid
            end
            "#,
        )
        .bind(user_id)
        .fetch_optional(&self.pool)
        .await?
        .map(UserDetailRecord::from_row)
        .transpose()
    }
}

impl PublicUserRecord {
    fn from_row(row: PgRow) -> Result<Self, sqlx::Error> {
        Ok(Self {
            id: row.try_get("id")?,
            name: row.try_get("name")?,
            has_password: row.try_get("has_password")?,
        })
    }
}

impl UserDetailRecord {
    fn from_row(row: PgRow) -> Result<Self, sqlx::Error> {
        Ok(Self {
            id: row.try_get("id")?,
            name: row.try_get("name")?,
            has_password: row.try_get("has_password")?,
            is_administrator: row.try_get("is_administrator")?,
            is_disabled: row.try_get("is_disabled")?,
            allow_download: row.try_get("allow_download")?,
            allow_transcode: row.try_get("allow_transcode")?,
            allow_new_device_login: row.try_get("allow_new_device_login")?,
            enable_content_downloading: row.try_get("enable_content_downloading")?,
            enable_playback_transcoding: row.try_get("enable_playback_transcoding")?,
            enable_all_folders: row.try_get("enable_all_folders")?,
            enabled_folders: row.try_get("enabled_folders")?,
        })
    }
}
