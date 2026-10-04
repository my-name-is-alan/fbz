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

/// 显示偏好行（sort 键 + CustomPrefs jsonb）。
#[derive(Clone, Debug, PartialEq)]
pub struct DisplayPreferencesRecord {
    pub sort_by: Option<String>,
    pub sort_order: Option<String>,
    pub custom_prefs: serde_json::Value,
}

/// 用户设置行（key → jsonb 值）。
#[derive(Clone, Debug, PartialEq)]
pub struct UserSettingRecord {
    pub key: String,
    pub value: serde_json::Value,
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

    /// 读取显示偏好（(user, client, key) 一行）。缺省返回 None（路由回默认 DTO）。
    pub async fn find_display_preferences(
        &self,
        user_internal_id: i64,
        client: &str,
        preferences_key: &str,
    ) -> Result<Option<DisplayPreferencesRecord>, sqlx::Error> {
        sqlx::query(
            r#"
            select sort_by, sort_order, custom_prefs
            from user_display_preferences
            where user_id = $1
              and client = $2
              and preferences_key = $3
            "#,
        )
        .bind(user_internal_id)
        .bind(client)
        .bind(preferences_key)
        .fetch_optional(&self.pool)
        .await?
        .map(DisplayPreferencesRecord::from_row)
        .transpose()
    }

    /// upsert 显示偏好（整行替换语义：sort/custom_prefs 一起写入）。
    pub async fn upsert_display_preferences(
        &self,
        user_internal_id: i64,
        client: &str,
        preferences_key: &str,
        sort_by: Option<&str>,
        sort_order: Option<&str>,
        custom_prefs: &serde_json::Value,
    ) -> Result<(), sqlx::Error> {
        sqlx::query(
            r#"
            insert into user_display_preferences
                (user_id, client, preferences_key, sort_by, sort_order, custom_prefs)
            values ($1, $2, $3, $4, $5, $6)
            on conflict (user_id, client, preferences_key)
            do update set
                sort_by = excluded.sort_by,
                sort_order = excluded.sort_order,
                custom_prefs = excluded.custom_prefs,
                updated_at = now()
            "#,
        )
        .bind(user_internal_id)
        .bind(client)
        .bind(preferences_key)
        .bind(sort_by)
        .bind(sort_order)
        .bind(custom_prefs)
        .execute(&self.pool)
        .await?;

        Ok(())
    }

    /// 列出用户全部设置行（key → jsonb 值）。
    pub async fn list_user_settings(
        &self,
        user_internal_id: i64,
    ) -> Result<Vec<UserSettingRecord>, sqlx::Error> {
        let rows = sqlx::query(
            r#"
            select setting_key, setting_value
            from user_settings
            where user_id = $1
            order by setting_key
            "#,
        )
        .bind(user_internal_id)
        .fetch_all(&self.pool)
        .await?;

        rows.into_iter().map(UserSettingRecord::from_row).collect()
    }

    /// 读取单个设置（typed setting）。
    pub async fn find_user_setting(
        &self,
        user_internal_id: i64,
        setting_key: &str,
    ) -> Result<Option<serde_json::Value>, sqlx::Error> {
        let row = sqlx::query(
            r#"
            select setting_value
            from user_settings
            where user_id = $1
              and setting_key = $2
            "#,
        )
        .bind(user_internal_id)
        .bind(setting_key)
        .fetch_optional(&self.pool)
        .await?;

        row.map(|row| row.try_get::<serde_json::Value, _>("setting_value"))
            .transpose()
    }

    /// upsert 单个设置。
    pub async fn upsert_user_setting(
        &self,
        user_internal_id: i64,
        setting_key: &str,
        setting_value: &serde_json::Value,
    ) -> Result<(), sqlx::Error> {
        sqlx::query(
            r#"
            insert into user_settings (user_id, setting_key, setting_value)
            values ($1, $2, $3)
            on conflict (user_id, setting_key)
            do update set
                setting_value = excluded.setting_value,
                updated_at = now()
            "#,
        )
        .bind(user_internal_id)
        .bind(setting_key)
        .bind(setting_value)
        .execute(&self.pool)
        .await?;

        Ok(())
    }

    /// 取用户内部 id（Emby 用户写接口定位目标用）。
    pub async fn find_internal_id_by_public_id(
        &self,
        user_id: &str,
    ) -> Result<Option<i64>, sqlx::Error> {
        let row = sqlx::query(
            r#"
            select id
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

        row.map(|row| row.try_get::<i64, _>("id")).transpose()
    }

    /// 取用户当前密码哈希（自助改密时校验旧密码）。`Ok(None)` = 用户不存在；
    /// `Ok(Some(None))` = 用户无密码。
    pub async fn find_password_hash_by_public_id(
        &self,
        user_id: &str,
    ) -> Result<Option<Option<String>>, sqlx::Error> {
        let row = sqlx::query(
            r#"
            select password_hash
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

        row.map(|row| row.try_get::<Option<String>, _>("password_hash"))
            .transpose()
    }

    /// 取用户当前显示名（policy 更新时保留现值用）。`Ok(None)` = 用户不存在。
    pub async fn find_display_name_by_public_id(
        &self,
        user_id: &str,
    ) -> Result<Option<Option<String>>, sqlx::Error> {
        let row = sqlx::query(
            r#"
            select display_name
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

        row.map(|row| row.try_get::<Option<String>, _>("display_name"))
            .transpose()
    }

    /// 设置/重置用户密码哈希（None = 清空密码）。返回受影响行数。
    pub async fn set_user_password_hash(
        &self,
        user_id: &str,
        password_hash: Option<&str>,
    ) -> Result<u64, sqlx::Error> {
        let result = sqlx::query(
            r#"
            update users
            set password_hash = $2,
                updated_at = now()
            where public_id = case
                when $1 ~* '^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$'
                then $1::uuid
                else null::uuid
            end
            "#,
        )
        .bind(user_id)
        .bind(password_hash)
        .execute(&self.pool)
        .await?;

        Ok(result.rows_affected())
    }

    /// 更新用户显示名（Emby `POST /Users/{Id}` 的 Name 映射到 display_name，
    /// 不动登录用 username）。返回受影响行数。
    pub async fn set_display_name(
        &self,
        user_id: &str,
        display_name: Option<&str>,
    ) -> Result<u64, sqlx::Error> {
        let result = sqlx::query(
            r#"
            update users
            set display_name = $2,
                updated_at = now()
            where public_id = case
                when $1 ~* '^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$'
                then $1::uuid
                else null::uuid
            end
            "#,
        )
        .bind(user_id)
        .bind(display_name)
        .execute(&self.pool)
        .await?;

        Ok(result.rows_affected())
    }

    /// 删除 key 前缀命中的设置（清除音轨/字幕选择记忆用）。返回删除行数。
    pub async fn delete_user_settings_by_prefix(
        &self,
        user_internal_id: i64,
        key_prefix: &str,
    ) -> Result<u64, sqlx::Error> {
        let result = sqlx::query(
            r#"
            delete from user_settings
            where user_id = $1
              and setting_key like $2 || '%'
            "#,
        )
        .bind(user_internal_id)
        .bind(key_prefix)
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

impl DisplayPreferencesRecord {
    fn from_row(row: PgRow) -> Result<Self, sqlx::Error> {
        Ok(Self {
            sort_by: row.try_get("sort_by")?,
            sort_order: row.try_get("sort_order")?,
            custom_prefs: row.try_get("custom_prefs")?,
        })
    }
}

impl UserSettingRecord {
    fn from_row(row: PgRow) -> Result<Self, sqlx::Error> {
        Ok(Self {
            key: row.try_get("setting_key")?,
            value: row.try_get("setting_value")?,
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
