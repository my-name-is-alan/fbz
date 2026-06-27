use std::{error::Error, fmt::Display};

use serde_json::Value;
use sqlx::{Row, postgres::PgRow};

use crate::{auth::token::hash_token, db::DbPool};

#[derive(Clone)]
pub struct AuthRepository {
    pool: DbPool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AuthUserRecord {
    pub id: i64,
    pub public_id: String,
    pub username: String,
    pub password_hash: Option<String>,
    pub is_disabled: bool,
    pub allow_new_device_login: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ClientDevice {
    pub device_id: String,
    pub device_name: Option<String>,
    pub client_name: Option<String>,
    pub client_version: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CreatedSession {
    pub public_id: String,
}

#[derive(Debug)]
pub enum CreateSessionError {
    NewDeviceLoginDisabled,
    DeviceRevoked,
    Database(sqlx::Error),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AuthenticatedUserRecord {
    pub id: i64,
    pub public_id: String,
    pub username: String,
    pub role_name: String,
    pub role_name_normalized: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SessionInfoRecord {
    pub id: String,
    pub user_id: String,
    pub user_name: String,
    pub client: Option<String>,
    pub device_id: Option<String>,
    pub device_name: Option<String>,
    pub application_version: Option<String>,
    pub is_active: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DeviceInfoRecord {
    pub internal_id: i64,
    pub public_id: String,
    pub reported_device_id: String,
    pub name: Option<String>,
    pub last_user_name: String,
    pub app_name: Option<String>,
    pub app_version: Option<String>,
    pub last_user_id: String,
    pub date_last_activity: Option<String>,
    pub icon_url: Option<String>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct SessionCapabilitiesInput {
    pub user_id: i64,
    pub session_id: String,
    pub playable_media_types: Vec<String>,
    pub supported_commands: Vec<String>,
    pub supports_media_control: bool,
    pub supports_sync: bool,
    pub push_token: Option<String>,
    pub push_token_type: Option<String>,
    pub icon_url: Option<String>,
    pub app_id: Option<String>,
    pub device_profile: Option<Value>,
}

impl AuthRepository {
    pub fn new(pool: DbPool) -> Self {
        Self { pool }
    }

    pub async fn find_user_by_name(
        &self,
        username: &str,
    ) -> Result<Option<AuthUserRecord>, sqlx::Error> {
        let username_normalized = normalize_username(username);

        sqlx::query(
            r#"
            select
                id,
                public_id::text as public_id,
                username,
                password_hash,
                is_disabled,
                allow_new_device_login
            from users
            where username_normalized = $1
            "#,
        )
        .bind(username_normalized)
        .fetch_optional(&self.pool)
        .await?
        .map(AuthUserRecord::from_row)
        .transpose()
    }

    pub async fn find_user_by_public_id(
        &self,
        user_id: &str,
    ) -> Result<Option<AuthUserRecord>, sqlx::Error> {
        sqlx::query(
            r#"
            select
                id,
                public_id::text as public_id,
                username,
                password_hash,
                is_disabled,
                allow_new_device_login
            from users
            where public_id = case
                when $1 ~* '^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$'
                then $1::uuid
                else null::uuid
            end
            "#,
        )
        .bind(user_id.trim())
        .fetch_optional(&self.pool)
        .await?
        .map(AuthUserRecord::from_row)
        .transpose()
    }

    pub async fn create_session(
        &self,
        user: &AuthUserRecord,
        device: &ClientDevice,
        access_token_hash: Vec<u8>,
        expires_in_days: i64,
    ) -> Result<CreatedSession, CreateSessionError> {
        let mut tx = self.pool.begin().await?;
        let device_id = device.device_id.trim();

        let existing_device = sqlx::query(
            r#"
            select id,
                   revoked_at is not null as is_revoked
            from devices
            where user_id = $1
              and device_id = $2
            for update
            "#,
        )
        .bind(user.id)
        .bind(device_id)
        .fetch_optional(&mut *tx)
        .await?;

        let device_row_id = if let Some(row) = existing_device {
            if row.try_get::<bool, _>("is_revoked")? {
                return Err(CreateSessionError::DeviceRevoked);
            }

            sqlx::query_scalar::<_, i64>(
                r#"
                update devices
                set device_name = $2,
                    client_name = $3,
                    client_version = $4,
                    last_seen_at = now()
                where id = $1
                returning id
                "#,
            )
            .bind(row.try_get::<i64, _>("id")?)
            .bind(&device.device_name)
            .bind(&device.client_name)
            .bind(&device.client_version)
            .fetch_one(&mut *tx)
            .await?
        } else {
            if !user.allow_new_device_login {
                return Err(CreateSessionError::NewDeviceLoginDisabled);
            }

            sqlx::query_scalar::<_, i64>(
                r#"
                insert into devices (
                    user_id,
                    device_id,
                    device_name,
                    client_name,
                    client_version,
                    last_seen_at
                )
                values ($1, $2, $3, $4, $5, now())
                returning id
                "#,
            )
            .bind(user.id)
            .bind(device_id)
            .bind(&device.device_name)
            .bind(&device.client_name)
            .bind(&device.client_version)
            .fetch_one(&mut *tx)
            .await?
        };

        let session = sqlx::query(
            r#"
            insert into sessions (
                user_id,
                device_id,
                access_token_hash,
                expires_at,
                last_seen_at
            )
            values ($1, $2, $3, now() + make_interval(days => $4::int), now())
            returning public_id::text as public_id
            "#,
        )
        .bind(user.id)
        .bind(device_row_id)
        .bind(access_token_hash)
        .bind(expires_in_days)
        .fetch_one(&mut *tx)
        .await?;

        sqlx::query(
            r#"
            update users
            set last_login_at = now(),
                updated_at = now()
            where id = $1
            "#,
        )
        .bind(user.id)
        .execute(&mut *tx)
        .await?;

        tx.commit().await?;

        Ok(CreatedSession {
            public_id: session.try_get("public_id")?,
        })
    }

    pub async fn revoke_session_by_token(&self, token: &str) -> Result<bool, sqlx::Error> {
        let result = sqlx::query(
            r#"
            update sessions
            set revoked_at = now(),
                last_seen_at = now()
            where access_token_hash = $1
              and revoked_at is null
            "#,
        )
        .bind(hash_token(token))
        .execute(&self.pool)
        .await?;

        Ok(result.rows_affected() > 0)
    }

    pub async fn find_active_user_by_token(
        &self,
        token: &str,
    ) -> Result<Option<AuthenticatedUserRecord>, sqlx::Error> {
        sqlx::query(
            r#"
            update sessions s
            set last_seen_at = now()
            from users u
            join roles r on r.id = u.role_id
            where s.user_id = u.id
              and s.access_token_hash = $1
              and s.revoked_at is null
              and s.expires_at > now()
              and not exists (
                  select 1
                  from devices d
                  where d.id = s.device_id
                    and d.revoked_at is not null
              )
              and u.is_disabled = false
            returning
                u.id,
                u.public_id::text as public_id,
                u.username,
                r.name as role_name,
                r.name_normalized as role_name_normalized
            "#,
        )
        .bind(hash_token(token))
        .fetch_optional(&self.pool)
        .await?
        .map(AuthenticatedUserRecord::from_row)
        .transpose()
    }

    pub async fn list_active_sessions_for_user(
        &self,
        user_id: i64,
        limit: i64,
        device_id: Option<&str>,
    ) -> Result<Vec<SessionInfoRecord>, sqlx::Error> {
        let rows = sqlx::query(
            r#"
            select
                s.public_id::text as id,
                u.public_id::text as user_id,
                u.username as user_name,
                d.client_name as client,
                d.device_id,
                d.device_name,
                d.client_version as application_version,
                true as is_active
            from sessions s
            join users u on u.id = s.user_id
            left join devices d on d.id = s.device_id
            where s.user_id = $1
              and s.revoked_at is null
              and s.expires_at > now()
              and ($3::text is null or d.device_id = $3)
              and not exists (
                  select 1
                  from devices revoked
                  where revoked.id = s.device_id
                    and revoked.revoked_at is not null
              )
            order by s.last_seen_at desc nulls last, s.created_at desc, s.id desc
            limit $2
            "#,
        )
        .bind(user_id)
        .bind(limit.clamp(1, 100))
        .bind(device_id.map(str::trim))
        .fetch_all(&self.pool)
        .await?;

        rows.into_iter().map(SessionInfoRecord::from_row).collect()
    }

    pub async fn find_active_session_for_user(
        &self,
        user_id: i64,
        session_id: &str,
    ) -> Result<Option<SessionInfoRecord>, sqlx::Error> {
        let row = sqlx::query(
            r#"
            select
                s.public_id::text as id,
                u.public_id::text as user_id,
                u.username as user_name,
                d.client_name as client,
                d.device_id,
                d.device_name,
                d.client_version as application_version,
                true as is_active
            from sessions s
            join users u on u.id = s.user_id
            left join devices d on d.id = s.device_id
            where s.user_id = $1
              and s.public_id = case
                  when $2 ~* '^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$'
                  then $2::uuid
                  else null::uuid
              end
              and s.revoked_at is null
              and s.expires_at > now()
              and not exists (
                  select 1
                  from devices revoked
                  where revoked.id = s.device_id
                    and revoked.revoked_at is not null
              )
            limit 1
            "#,
        )
        .bind(user_id)
        .bind(session_id.trim())
        .fetch_optional(&self.pool)
        .await?;

        row.map(SessionInfoRecord::from_row).transpose()
    }

    pub async fn list_devices(
        &self,
        sort_descending: bool,
    ) -> Result<Vec<DeviceInfoRecord>, sqlx::Error> {
        let sql = if sort_descending {
            r#"
            select
                d.id as internal_id,
                d.public_id::text as public_id,
                d.device_id as reported_device_id,
                d.device_name as name,
                u.username as last_user_name,
                d.client_name as app_name,
                d.client_version as app_version,
                u.public_id::text as last_user_id,
                coalesce(d.last_seen_at, d.created_at)::text as date_last_activity,
                d.icon_url
            from devices d
            join users u on u.id = d.user_id
            where d.revoked_at is null
            order by coalesce(d.last_seen_at, d.created_at) desc, d.id desc
            "#
        } else {
            r#"
            select
                d.id as internal_id,
                d.public_id::text as public_id,
                d.device_id as reported_device_id,
                d.device_name as name,
                u.username as last_user_name,
                d.client_name as app_name,
                d.client_version as app_version,
                u.public_id::text as last_user_id,
                coalesce(d.last_seen_at, d.created_at)::text as date_last_activity,
                d.icon_url
            from devices d
            join users u on u.id = d.user_id
            where d.revoked_at is null
            order by coalesce(d.last_seen_at, d.created_at) asc, d.id asc
            "#
        };

        let rows = sqlx::query(sql).fetch_all(&self.pool).await?;

        rows.into_iter().map(DeviceInfoRecord::from_row).collect()
    }

    pub async fn find_device_info(
        &self,
        device_id: &str,
    ) -> Result<Option<DeviceInfoRecord>, sqlx::Error> {
        let row = sqlx::query(
            r#"
            with requested_device as (
                select
                    case
                        when $1::text ~* '^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$'
                        then $1::uuid
                        else null::uuid
                    end as public_id,
                    $1::text as reported_device_id
            ),
            selected_device as (
                select
                    d.id,
                    0 as match_priority,
                    coalesce(d.last_seen_at, d.created_at) as last_activity
                from requested_device
                join devices d on d.public_id = requested_device.public_id
                where d.revoked_at is null

                union all

                select
                    d.id,
                    1 as match_priority,
                    coalesce(d.last_seen_at, d.created_at) as last_activity
                from requested_device
                join devices d on d.device_id = requested_device.reported_device_id
                where d.revoked_at is null
            )
            select
                d.id as internal_id,
                d.public_id::text as public_id,
                d.device_id as reported_device_id,
                d.device_name as name,
                u.username as last_user_name,
                d.client_name as app_name,
                d.client_version as app_version,
                u.public_id::text as last_user_id,
                coalesce(d.last_seen_at, d.created_at)::text as date_last_activity,
                d.icon_url
            from selected_device
            join devices d on d.id = selected_device.id
            join users u on u.id = d.user_id
            order by selected_device.match_priority,
                     selected_device.last_activity desc,
                     selected_device.id desc
            limit 1
            "#,
        )
        .bind(device_id.trim())
        .fetch_optional(&self.pool)
        .await?;

        row.map(DeviceInfoRecord::from_row).transpose()
    }

    pub async fn update_device_custom_name(
        &self,
        device_id: &str,
        custom_name: Option<&str>,
    ) -> Result<bool, sqlx::Error> {
        let result = sqlx::query(
            r#"
            with requested_device as (
                select
                    case
                        when $1::text ~* '^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$'
                        then $1::uuid
                        else null::uuid
                    end as public_id,
                    $1::text as reported_device_id
            ),
            selected_device as (
                select id
                from (
                    select
                        d.id,
                        0 as match_priority,
                        coalesce(d.last_seen_at, d.created_at) as last_activity
                    from requested_device
                    join devices d on d.public_id = requested_device.public_id
                    where d.revoked_at is null

                    union all

                    select
                        d.id,
                        1 as match_priority,
                        coalesce(d.last_seen_at, d.created_at) as last_activity
                    from requested_device
                    join devices d on d.device_id = requested_device.reported_device_id
                    where d.revoked_at is null
                ) candidates
                order by match_priority, last_activity desc, id desc
                limit 1
            )
            update devices d
            set device_name = $2
            from selected_device
            where d.id = selected_device.id
            "#,
        )
        .bind(device_id.trim())
        .bind(custom_name)
        .execute(&self.pool)
        .await?;

        Ok(result.rows_affected() > 0)
    }

    pub async fn revoke_device(&self, device_id: &str) -> Result<bool, sqlx::Error> {
        let mut tx = self.pool.begin().await?;

        let revoked_device_id = sqlx::query_scalar::<_, i64>(
            r#"
            with requested_device as (
                select
                    case
                        when $1::text ~* '^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$'
                        then $1::uuid
                        else null::uuid
                    end as public_id,
                    $1::text as reported_device_id
            ),
            selected_device as (
                select id
                from (
                    select
                        d.id,
                        0 as match_priority,
                        coalesce(d.last_seen_at, d.created_at) as last_activity
                    from requested_device
                    join devices d on d.public_id = requested_device.public_id
                    where d.revoked_at is null

                    union all

                    select
                        d.id,
                        1 as match_priority,
                        coalesce(d.last_seen_at, d.created_at) as last_activity
                    from requested_device
                    join devices d on d.device_id = requested_device.reported_device_id
                    where d.revoked_at is null
                ) candidates
                order by match_priority, last_activity desc, id desc
                limit 1
            )
            update devices d
            set revoked_at = now(),
                last_seen_at = coalesce(d.last_seen_at, now())
            from selected_device
            where d.id = selected_device.id
            returning d.id
            "#,
        )
        .bind(device_id.trim())
        .fetch_optional(&mut *tx)
        .await?;

        let Some(revoked_device_id) = revoked_device_id else {
            tx.commit().await?;
            return Ok(false);
        };

        sqlx::query(
            r#"
            update sessions
            set revoked_at = now(),
                last_seen_at = now()
            where device_id = $1
              and revoked_at is null
            "#,
        )
        .bind(revoked_device_id)
        .execute(&mut *tx)
        .await?;

        tx.commit().await?;

        Ok(true)
    }

    pub async fn update_session_capabilities(
        &self,
        input: SessionCapabilitiesInput,
    ) -> Result<bool, sqlx::Error> {
        let result = sqlx::query(
            r#"
            update devices d
            set playable_media_types = $3,
                supported_commands = $4,
                supports_media_control = $5,
                supports_sync = $6,
                push_token = $7,
                push_token_type = $8,
                icon_url = $9,
                app_id = $10,
                device_profile = $11,
                capabilities_updated_at = now(),
                last_seen_at = now()
            from sessions s
            where s.device_id = d.id
              and s.user_id = $1
              and s.public_id = $2::uuid
              and s.revoked_at is null
              and s.expires_at > now()
              and d.revoked_at is null
            "#,
        )
        .bind(input.user_id)
        .bind(input.session_id)
        .bind(input.playable_media_types)
        .bind(input.supported_commands)
        .bind(input.supports_media_control)
        .bind(input.supports_sync)
        .bind(input.push_token)
        .bind(input.push_token_type)
        .bind(input.icon_url)
        .bind(input.app_id)
        .bind(input.device_profile)
        .execute(&self.pool)
        .await?;

        Ok(result.rows_affected() > 0)
    }
}

impl AuthUserRecord {
    fn from_row(row: PgRow) -> Result<Self, sqlx::Error> {
        Ok(Self {
            id: row.try_get("id")?,
            public_id: row.try_get("public_id")?,
            username: row.try_get("username")?,
            password_hash: row.try_get("password_hash")?,
            is_disabled: row.try_get("is_disabled")?,
            allow_new_device_login: row.try_get("allow_new_device_login")?,
        })
    }
}

impl AuthenticatedUserRecord {
    fn from_row(row: PgRow) -> Result<Self, sqlx::Error> {
        Ok(Self {
            id: row.try_get("id")?,
            public_id: row.try_get("public_id")?,
            username: row.try_get("username")?,
            role_name: row.try_get("role_name")?,
            role_name_normalized: row.try_get("role_name_normalized")?,
        })
    }
}

impl SessionInfoRecord {
    fn from_row(row: PgRow) -> Result<Self, sqlx::Error> {
        Ok(Self {
            id: row.try_get("id")?,
            user_id: row.try_get("user_id")?,
            user_name: row.try_get("user_name")?,
            client: row.try_get("client")?,
            device_id: row.try_get("device_id")?,
            device_name: row.try_get("device_name")?,
            application_version: row.try_get("application_version")?,
            is_active: row.try_get("is_active")?,
        })
    }
}

impl DeviceInfoRecord {
    fn from_row(row: PgRow) -> Result<Self, sqlx::Error> {
        Ok(Self {
            internal_id: row.try_get("internal_id")?,
            public_id: row.try_get("public_id")?,
            reported_device_id: row.try_get("reported_device_id")?,
            name: row.try_get("name")?,
            last_user_name: row.try_get("last_user_name")?,
            app_name: row.try_get("app_name")?,
            app_version: row.try_get("app_version")?,
            last_user_id: row.try_get("last_user_id")?,
            date_last_activity: row.try_get("date_last_activity")?,
            icon_url: row.try_get("icon_url")?,
        })
    }
}

fn normalize_username(username: &str) -> String {
    username.trim().to_ascii_lowercase()
}

impl Display for CreateSessionError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::NewDeviceLoginDisabled => {
                f.write_str("user is not allowed to login from new devices")
            }
            Self::DeviceRevoked => f.write_str("device is revoked"),
            Self::Database(err) => write!(f, "session repository error: {err}"),
        }
    }
}

impl Error for CreateSessionError {}

impl From<sqlx::Error> for CreateSessionError {
    fn from(error: sqlx::Error) -> Self {
        Self::Database(error)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn username_normalization_is_stable() {
        assert_eq!(normalize_username(" Admin "), "admin");
    }

    #[test]
    fn session_capabilities_input_keeps_client_capability_boundary() {
        let input = SessionCapabilitiesInput {
            user_id: 1,
            session_id: "00000000-0000-0000-0000-000000000001".to_owned(),
            playable_media_types: vec!["Audio".to_owned(), "Video".to_owned()],
            supported_commands: vec!["Play".to_owned()],
            supports_media_control: true,
            supports_sync: false,
            push_token: Some("push-token".to_owned()),
            push_token_type: Some("apns".to_owned()),
            icon_url: Some("https://example.test/icon.png".to_owned()),
            app_id: Some("client.app".to_owned()),
            device_profile: Some(serde_json::json!({"Name": "Client"})),
        };

        assert_eq!(input.user_id, 1);
        assert_eq!(input.playable_media_types, ["Audio", "Video"]);
        assert!(input.supports_media_control);
        assert_eq!(input.device_profile.unwrap()["Name"], "Client");
    }

    #[test]
    fn device_public_id_entrypoints_keep_uuid_index_shape() {
        let repository = include_str!("repository.rs");
        let bad_or_filter = format!("{}{}", "public_id::text = ", "$1 or device_id = $1");
        let bad_order_filter = format!("{}{}", "public_id::text = ", "$1 then 0");

        assert!(repository.contains("with requested_device as"));
        assert!(repository.contains("then $1::uuid"));
        assert!(repository.contains("join devices d on d.public_id = requested_device.public_id"));
        assert!(
            repository
                .contains("join devices d on d.device_id = requested_device.reported_device_id")
        );
        assert!(repository.contains("match_priority"));
        assert!(!repository.contains(&bad_or_filter));
        assert!(!repository.contains(&bad_order_filter));
    }

    #[test]
    fn auth_user_public_id_lookup_keeps_uuid_index_shape() {
        let repository = include_str!("repository.rs");
        let bad_filter = format!("{}{}", "where public_id::text = ", "$1");

        assert!(repository.contains("find_user_by_public_id"));
        assert!(repository.contains("where public_id = case"));
        assert!(repository.contains("then $1::uuid"));
        assert!(!repository.contains(&bad_filter));
    }

    #[test]
    fn active_session_list_filters_optional_device_id_in_sql() {
        let repository = include_str!("repository.rs");
        let signature = ["device_id: Option<&str", ">"].join("");
        let predicate = ["and ($3::text is null or d.device_id = ", "$3)"].join("");
        let bind = [".bind(device_id.map(str::", "trim))"].join("");

        assert!(repository.contains(&signature));
        assert!(repository.contains(&predicate));
        assert!(repository.contains(&bind));
    }

    #[test]
    fn device_id_recent_index_matches_compat_lookup() {
        let migration = include_str!("../../migrations/0041_device_lookup_indexes.sql");

        assert!(migration.contains("idx_devices_device_id_recent_active"));
        assert!(migration.contains(
            "on devices (device_id, (coalesce(last_seen_at, created_at)) desc, id desc)"
        ));
        assert!(migration.contains("where revoked_at is null"));
    }
}
