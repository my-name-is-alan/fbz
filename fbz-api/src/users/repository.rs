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

/// 用户头像元数据：content-type + 更新时间的 epoch 秒（供 URL 缓存击穿）。
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AvatarMeta {
    pub content_type: String,
    pub updated_at_epoch: i64,
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

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct UsersQueryFilter {
    pub is_hidden: Option<bool>,
    pub is_disabled: Option<bool>,
    pub start_index: i64,
    pub limit: i64,
    pub name_starts_with_or_greater: Option<String>,
    pub sort_descending: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct UsersQueryPage {
    pub records: Vec<UserDetailRecord>,
    pub total_record_count: i64,
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

    pub async fn list_users_query(
        &self,
        filter: UsersQueryFilter,
    ) -> Result<UsersQueryPage, sqlx::Error> {
        let rows = sqlx::query(
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
            where ($1::boolean is null or $1 = false)
              and ($2::boolean is null or u.is_disabled = $2)
              and (
                  $3::text is null
                  or u.username_normalized >= lower($3)
              )
            order by
                case when $4::boolean then u.username_normalized end desc,
                case when $4::boolean then u.id end desc,
                case when not $4::boolean then u.username_normalized end asc,
                case when not $4::boolean then u.id end asc
            offset $5
            limit $6 + 1
            "#,
        )
        .bind(filter.is_hidden)
        .bind(filter.is_disabled)
        .bind(filter.name_starts_with_or_greater.as_deref())
        .bind(filter.sort_descending)
        .bind(filter.start_index)
        .bind(filter.limit)
        .fetch_all(&self.pool)
        .await?;

        users_query_page_lower_bound_from_rows(rows, filter.start_index, filter.limit)
    }

    /// 读取用户头像元数据（content-type + 更新时间的 epoch 秒）。
    /// 返回 `None` 表示用户不存在；`Some(None)` 表示用户存在但未设置头像。
    pub async fn find_avatar_meta(
        &self,
        user_id: &str,
    ) -> Result<Option<Option<AvatarMeta>>, sqlx::Error> {
        let row = sqlx::query(
            r#"
            select
                avatar_content_type,
                extract(epoch from avatar_updated_at)::bigint as updated_at_epoch
            from users
            where public_id = case
                when $1 ~* '^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$'
                then $1::uuid
                else null::uuid
            end
            "#,
        )
        .bind(user_id)
        .fetch_optional(&self.pool)
        .await?;

        let Some(row) = row else {
            return Ok(None);
        };
        let content_type: Option<String> = row.try_get("avatar_content_type")?;
        let updated_at_epoch: Option<i64> = row.try_get("updated_at_epoch")?;
        Ok(Some(match (content_type, updated_at_epoch) {
            (Some(content_type), updated_at_epoch) => Some(AvatarMeta {
                content_type,
                updated_at_epoch: updated_at_epoch.unwrap_or(0),
            }),
            _ => None,
        }))
    }

    /// 记录用户头像 content-type 并把更新时间设为 now()。文件应已写盘。
    /// 返回受影响行数（0 表示用户不存在）。
    pub async fn set_avatar_meta(
        &self,
        user_id: &str,
        content_type: &str,
    ) -> Result<u64, sqlx::Error> {
        let result = sqlx::query(
            r#"
            update users
            set avatar_content_type = $2,
                avatar_updated_at = now(),
                updated_at = now()
            where public_id = case
                when $1 ~* '^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$'
                then $1::uuid
                else null::uuid
            end
            "#,
        )
        .bind(user_id)
        .bind(content_type)
        .execute(&self.pool)
        .await?;
        Ok(result.rows_affected())
    }

    /// 清除用户头像元数据（删除文件后调用）。返回受影响行数。
    pub async fn clear_avatar_meta(&self, user_id: &str) -> Result<u64, sqlx::Error> {
        let result = sqlx::query(
            r#"
            update users
            set avatar_content_type = null,
                avatar_updated_at = now(),
                updated_at = now()
            where public_id = case
                when $1 ~* '^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$'
                then $1::uuid
                else null::uuid
            end
            "#,
        )
        .bind(user_id)
        .execute(&self.pool)
        .await?;
        Ok(result.rows_affected())
    }
}

fn users_query_page_lower_bound_from_rows(
    rows: Vec<PgRow>,
    start_index: i64,
    limit: i64,
) -> Result<UsersQueryPage, sqlx::Error> {
    let visible_limit = limit.max(0) as usize;
    let has_more = rows.len() > visible_limit;
    let records = rows
        .into_iter()
        .take(visible_limit)
        .map(UserDetailRecord::from_row)
        .collect::<Result<Vec<_>, _>>()?;
    let total_record_count =
        users_query_lower_bound_total_record_count(start_index, records.len(), has_more);

    Ok(UsersQueryPage {
        records,
        total_record_count,
    })
}

fn users_query_lower_bound_total_record_count(
    start_index: i64,
    item_count: usize,
    has_more: bool,
) -> i64 {
    if item_count == 0 {
        0
    } else {
        start_index
            .max(0)
            .saturating_add(item_count as i64)
            .saturating_add(i64::from(has_more))
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

#[cfg(test)]
mod tests {
    use super::*;

    fn repository_source() -> String {
        include_str!("repository.rs").replace("\r\n", "\n")
    }

    fn users_query_source() -> String {
        let repository = repository_source();
        let query_start = repository
            .find("pub async fn list_users_query")
            .expect("users query repository method should exist");
        let query_end = repository[query_start..]
            .find("\nimpl PublicUserRecord")
            .map(|offset| query_start + offset)
            .expect("users query repository method should stay before row mappers");
        repository[query_start..query_end].to_owned()
    }

    #[test]
    fn users_query_uses_lower_bound_pagination_result() {
        let query = users_query_source();

        assert!(
            !query.contains("let total_record_count: i64 = sqlx::query_scalar"),
            "Users/Query should not exact-count the full users table before fetching the page"
        );
        assert!(
            !query.contains("select count(*)::bigint\n            from users u"),
            "Users/Query should not exact-count the full users table before fetching the page"
        );
        assert!(
            query.contains("limit $6 + 1"),
            "Users/Query should fetch one probe row for lower-bound count semantics"
        );
        assert!(
            query.contains(
                "users_query_page_lower_bound_from_rows(rows, filter.start_index, filter.limit)"
            ),
            "Users/Query should drop the probe row before mapping the response"
        );
    }

    #[test]
    fn users_query_lower_bound_count_preserves_start_index_window() {
        let query = users_query_source();

        assert!(
            query.contains("fn users_query_lower_bound_total_record_count"),
            "Users/Query lower-bound counting should be isolated for overflow-safe window math"
        );
        assert!(
            query.contains(".saturating_add(item_count as i64)"),
            "Users/Query lower-bound count should include the visible page after StartIndex"
        );
        assert!(
            query.contains(".saturating_add(i64::from(has_more))"),
            "Users/Query lower-bound count should include a one-row has-more probe"
        );
        assert_eq!(users_query_lower_bound_total_record_count(0, 0, false), 0);
        assert_eq!(users_query_lower_bound_total_record_count(3, 2, false), 5);
        assert_eq!(users_query_lower_bound_total_record_count(3, 2, true), 6);
        assert_eq!(
            users_query_lower_bound_total_record_count(i64::MAX, 100, true),
            i64::MAX
        );
    }

    // Live-DB smoke: validates the Emby Users/Query SQL parses and executes
    // against the real migrated schema. The query is a read-only SELECT, so it
    // does not mutate user records.
    //   cargo test -- --ignored users_query_executes_against_live_schema
    #[tokio::test]
    #[ignore = "requires a running PostgreSQL from ./scripts/dev-deps.ps1"]
    async fn users_query_executes_against_live_schema() {
        use sqlx::postgres::PgPoolOptions;

        let url = std::env::var("DATABASE_URL")
            .unwrap_or_else(|_| "postgres://fbz:fbz@127.0.0.1:5432/fbz".to_owned());
        let pool = PgPoolOptions::new()
            .max_connections(2)
            .connect(&url)
            .await
            .expect("connect to live PostgreSQL");
        crate::db::migrate(&pool).await.expect("run migrations");

        let repository = UsersRepository::new(pool);
        repository
            .list_users_query(UsersQueryFilter {
                is_hidden: Some(false),
                is_disabled: None,
                start_index: 0,
                limit: 5,
                name_starts_with_or_greater: Some("A".to_owned()),
                sort_descending: false,
            })
            .await
            .expect("Users/Query should execute against the live schema");
    }
}
