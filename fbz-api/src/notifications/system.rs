//! 服务器侧（非插件）通知入队：与插件 Host API 同一条投递链
//! （`plugin_notification_requests` + `event_outbox` → 投递 worker → 通知目标），
//! 但 plugin_id 固定为 `fbz-core`，用于 Emby 兼容层 `Notifications/Admin`、
//! 通知服务测试等系统来源。0089 已解除审计表对插件安装行的外键，系统来源合法。

use serde_json::{Value, json};
use sqlx::Row;

use crate::{db::DbPool, notifications::delivery::NOTIFICATION_REQUESTED_EVENT};

const SYSTEM_PLUGIN_ID: &str = "fbz-core";
const SYSTEM_PACKAGE_ID: &str = "system";

/// 系统通知入参。level 必须是 info/success/warning/error（表 check 约束）。
#[derive(Clone, Debug, PartialEq)]
pub struct SystemNotificationInput {
    pub title: String,
    pub message: String,
    pub level: String,
    pub metadata: Value,
}

/// 入队一条系统通知，返回请求 public_id。单事务写请求行 + outbox 事件。
pub async fn enqueue_system_notification(
    pool: &DbPool,
    input: SystemNotificationInput,
) -> Result<String, sqlx::Error> {
    let mut tx = pool.begin().await?;

    let request_row = sqlx::query(
        r#"
        insert into plugin_notification_requests (
            plugin_id,
            package_id,
            title,
            message,
            level,
            channel,
            metadata
        )
        values ($1, $2, $3, $4, $5, null, $6)
        returning id, public_id::text as public_id
        "#,
    )
    .bind(SYSTEM_PLUGIN_ID)
    .bind(SYSTEM_PACKAGE_ID)
    .bind(&input.title)
    .bind(&input.message)
    .bind(&input.level)
    .bind(&input.metadata)
    .fetch_one(&mut *tx)
    .await?;
    let request_id = request_row.try_get::<i64, _>("id")?;
    let public_id = request_row.try_get::<String, _>("public_id")?;

    let payload = json!({
        "requestId": public_id,
        "pluginId": SYSTEM_PLUGIN_ID,
        "packageId": SYSTEM_PACKAGE_ID,
        "title": input.title,
        "message": input.message,
        "level": input.level,
        "channel": Value::Null,
        "metadata": input.metadata,
    });
    let outbox_row = sqlx::query(
        r#"
        insert into event_outbox (
            event_type,
            aggregate_type,
            aggregate_id,
            payload
        )
        values ($1, 'plugin_notification', $2, $3)
        returning id
        "#,
    )
    .bind(NOTIFICATION_REQUESTED_EVENT)
    .bind(&public_id)
    .bind(payload)
    .fetch_one(&mut *tx)
    .await?;
    let outbox_id = outbox_row.try_get::<i64, _>("id")?;

    sqlx::query(
        r#"
        update plugin_notification_requests
        set outbox_event_id = $2,
            updated_at = now()
        where id = $1
        "#,
    )
    .bind(request_id)
    .bind(outbox_id)
    .execute(&mut *tx)
    .await?;

    tx.commit().await?;

    Ok(public_id)
}

/// Emby level 文案 → 表约束允许的 level。未知值一律 info。
pub fn normalize_notification_level(value: Option<&str>) -> &'static str {
    match value
        .map(str::trim)
        .map(str::to_ascii_lowercase)
        .as_deref()
    {
        Some("warn" | "warning") => "warning",
        Some("error" | "fatal") => "error",
        Some("success") => "success",
        _ => "info",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn level_normalization_maps_emby_vocabulary() {
        assert_eq!(normalize_notification_level(Some("Warn")), "warning");
        assert_eq!(normalize_notification_level(Some("Warning")), "warning");
        assert_eq!(normalize_notification_level(Some("Error")), "error");
        assert_eq!(normalize_notification_level(Some("fatal")), "error");
        assert_eq!(normalize_notification_level(Some("Success")), "success");
        assert_eq!(normalize_notification_level(Some("Info")), "info");
        assert_eq!(normalize_notification_level(Some("weird")), "info");
        assert_eq!(normalize_notification_level(None), "info");
    }
}
