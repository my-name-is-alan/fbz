//! 初始化判定与锁定式建首个管理员。SQL 与事务编排集中在此，路由只做鉴权/映射。

use crate::{
    auth::bootstrap::{insert_owner_admin, normalize_username},
    config::admin_password_meets_policy,
    db::DbPool,
};

/// 用户名长度上限（与展示/存储约束一致，避免超长输入）。
const MAX_USERNAME_LEN: usize = 128;

/// `pg_advisory_xact_lock` 的固定 key，用于串行化首次初始化（取自 "fbz-setup" 的稳定常量）。
const SETUP_ADVISORY_LOCK_KEY: i64 = 0x_FB2_5E_70;

/// `POST /api/setup` 的失败原因，由路由映射为 HTTP 状态码。
#[derive(Debug)]
pub enum SetupError {
    /// 系统已初始化（用户数 > 0）。→ 409
    AlreadyInitialized,
    /// 用户名为空或超长。→ 422
    InvalidUsername,
    /// 密码不满足强度策略。→ 422
    WeakPassword,
    /// 底层数据库错误。→ 500
    Database(sqlx::Error),
}

impl From<sqlx::Error> for SetupError {
    fn from(err: sqlx::Error) -> Self {
        Self::Database(err)
    }
}

/// 系统是否已存在任意用户（= 初始化已完成）。
pub async fn has_any_user(pool: &DbPool) -> Result<bool, sqlx::Error> {
    sqlx::query_scalar::<_, bool>("select exists(select 1 from users)")
        .fetch_one(pool)
        .await
}

/// 锁定式建首个管理员。
///
/// 并发安全靠 `pg_advisory_xact_lock`：在单个事务内先取一个进程级建议锁，串行化所有 setup
/// 尝试，再确认"用户数为 0"，最后建 Owner。两个并发请求即便用不同用户名也只会成功一个——
/// 后到者拿到锁时看到用户数 >0，返回 [`SetupError::AlreadyInitialized`]。
pub async fn complete_setup(
    pool: &DbPool,
    username: &str,
    password: &str,
) -> Result<(), SetupError> {
    validate_setup_credentials(username, password)?;

    let mut tx = pool.begin().await?;

    // 建议锁：同一事务内串行化 setup，提交/回滚时自动释放。常量 key 任意但需全局唯一约定。
    sqlx::query("select pg_advisory_xact_lock($1)")
        .bind(SETUP_ADVISORY_LOCK_KEY)
        .execute(&mut *tx)
        .await?;

    let already = sqlx::query_scalar::<_, bool>("select exists(select 1 from users)")
        .fetch_one(&mut *tx)
        .await?;
    if already {
        return Err(SetupError::AlreadyInitialized);
    }

    insert_owner_admin(&mut tx, username, password).await?;
    tx.commit().await?;

    Ok(())
}

/// 校验用户名与密码（纯逻辑，不触 DB），供 [`complete_setup`] 与单测共用。
fn validate_setup_credentials(username: &str, password: &str) -> Result<(), SetupError> {
    let username_normalized = normalize_username(username);
    if username_normalized.is_empty() || username_normalized.len() > MAX_USERNAME_LEN {
        return Err(SetupError::InvalidUsername);
    }
    if !admin_password_meets_policy(password) {
        return Err(SetupError::WeakPassword);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_empty_username() {
        assert!(matches!(
            validate_setup_credentials("   ", "longenoughpassword"),
            Err(SetupError::InvalidUsername)
        ));
    }

    #[test]
    fn rejects_password_below_min_length() {
        // 5 位 → 不足 6 位下限。
        assert!(matches!(
            validate_setup_credentials("admin", "01234"),
            Err(SetupError::WeakPassword)
        ));
    }

    #[test]
    fn accepts_valid_credentials() {
        // 恰好 6 位边界通过。
        assert!(validate_setup_credentials("admin", "012345").is_ok());
    }

    #[tokio::test]
    #[ignore = "requires a running PostgreSQL from ./scripts/dev-deps.ps1"]
    async fn complete_setup_on_initialized_db_is_locked() {
        use sqlx::postgres::PgPoolOptions;
        use std::time::{SystemTime, UNIX_EPOCH};

        let url = std::env::var("DATABASE_URL")
            .unwrap_or_else(|_| "postgres://fbz:fbz@127.0.0.1:5432/fbz".to_owned());
        let pool = PgPoolOptions::new()
            .max_connections(2)
            .connect(&url)
            .await
            .expect("connect to live PostgreSQL");
        crate::db::migrate(&pool).await.expect("run migrations");

        // 保证库非空：插入一个唯一后缀的探针用户（带 Owner 角色）。
        let suffix = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("clock after epoch")
            .as_nanos();
        let probe = format!("0000_setup_lock_probe_{suffix}");
        let mut tx = pool.begin().await.expect("begin");
        crate::auth::bootstrap::insert_owner_admin(&mut tx, &probe, "012345678901")
            .await
            .expect("seed probe user");
        tx.commit().await.expect("commit probe");

        // 库已非空 → setup 必须 409（锁定）。
        let result = complete_setup(&pool, "another_admin", "012345678901").await;
        assert!(matches!(result, Err(SetupError::AlreadyInitialized)));

        // has_any_user 在非空库返回 true。
        assert!(has_any_user(&pool).await.expect("has_any_user"));

        // 清理探针用户。
        sqlx::query("delete from users where username_normalized = $1")
            .bind(&probe)
            .execute(&pool)
            .await
            .expect("cleanup probe user");
    }
}
