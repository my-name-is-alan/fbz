use crate::{auth::password::PasswordService, config::BootstrapAdminConfig, db::DbPool};

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum BootstrapAdminOutcome {
    NotConfigured,
    AlreadyExists,
    Created,
}

pub async fn ensure_bootstrap_admin(
    pool: &DbPool,
    config: &BootstrapAdminConfig,
) -> Result<BootstrapAdminOutcome, sqlx::Error> {
    let (Some(username), Some(password)) = (&config.username, &config.password) else {
        return Ok(BootstrapAdminOutcome::NotConfigured);
    };

    let username_normalized = normalize_username(username);
    let exists = sqlx::query_scalar::<_, bool>(
        r#"
        select exists(
            select 1
            from users
            where username_normalized = $1
        )
        "#,
    )
    .bind(&username_normalized)
    .fetch_one(pool)
    .await?;

    if exists {
        return Ok(BootstrapAdminOutcome::AlreadyExists);
    }

    let mut tx = pool.begin().await?;
    insert_owner_admin(&mut tx, username, password).await?;
    tx.commit().await?;

    Ok(BootstrapAdminOutcome::Created)
}

/// 在给定事务内 upsert `Owner` 角色并插入一个拥有该角色的管理员用户。
///
/// env bootstrap（[`ensure_bootstrap_admin`]）与 HTTP `POST /api/setup` 共用此函数，
/// 保证两条建首个管理员的通道行为一致。调用方负责事务的开启/提交与并发锁定语义
/// （setup 通道需在同一事务内先确认"用户数为 0"）。`username_normalized` 的唯一约束
/// 兜底并发，命中冲突时由调用方转成 409。
pub async fn insert_owner_admin(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    username: &str,
    password: &str,
) -> Result<(), sqlx::Error> {
    let username_normalized = normalize_username(username);
    let role_id = sqlx::query_scalar::<_, i64>(
        r#"
        insert into roles (name, name_normalized, description, is_builtin)
        values ('Owner', 'owner', 'Full server owner', true)
        on conflict (name_normalized) do update
            set is_builtin = true,
                updated_at = now()
        returning id
        "#,
    )
    .fetch_one(&mut **tx)
    .await?;
    let password_hash = PasswordService.hash_password(password);

    sqlx::query(
        r#"
        insert into users (
            username,
            username_normalized,
            password_hash,
            display_name,
            role_id,
            allow_download,
            allow_transcode,
            allow_new_device_login
        )
        values ($1, $2, $3, $4, $5, true, true, true)
        "#,
    )
    .bind(username.trim())
    .bind(username_normalized)
    .bind(password_hash)
    .bind(username.trim())
    .bind(role_id)
    .execute(&mut **tx)
    .await?;

    Ok(())
}

pub fn normalize_username(username: &str) -> String {
    username.trim().to_ascii_lowercase()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unconfigured_bootstrap_admin_is_noop_shape() {
        let config = BootstrapAdminConfig {
            username: None,
            password: None,
        };

        assert_eq!(config.username, None);
        assert_eq!(config.password, None);
    }

    #[test]
    fn username_normalization_is_stable() {
        assert_eq!(normalize_username(" Admin "), "admin");
    }
}
