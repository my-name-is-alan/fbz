use serde_json::{Value, json};
use sqlx::Row;

use crate::db::DbPool;

pub const PLUGIN_HOOK_DISPATCH_EVENT: &str = "plugin.hook.dispatch";

#[derive(Clone)]
pub struct PluginHookDispatcher {
    pool: DbPool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PluginHookEvent {
    pub event_key: String,
    pub aggregate_type: String,
    pub aggregate_id: String,
    pub payload: Value,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct HookDispatchSummary {
    pub matched_hooks: usize,
    pub queued_dispatches: usize,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct EnabledHookTarget {
    plugin_id: String,
    package_id: String,
    hook_id: i64,
    handler: String,
}

impl PluginHookDispatcher {
    pub fn new(pool: DbPool) -> Self {
        Self { pool }
    }

    pub async fn dispatch(
        &self,
        event: PluginHookEvent,
    ) -> Result<HookDispatchSummary, sqlx::Error> {
        let hooks = self.enabled_hooks_for_event(&event.event_key).await?;
        if hooks.is_empty() {
            return Ok(HookDispatchSummary {
                matched_hooks: 0,
                queued_dispatches: 0,
            });
        }

        let mut tx = self.pool.begin().await?;
        for hook in &hooks {
            sqlx::query(
                r#"
                insert into event_outbox (
                    event_type,
                    aggregate_type,
                    aggregate_id,
                    payload
                )
                values ($1, 'plugin', $2, $3)
                "#,
            )
            .bind(PLUGIN_HOOK_DISPATCH_EVENT)
            .bind(&hook.plugin_id)
            .bind(dispatch_payload(&event, hook))
            .execute(&mut *tx)
            .await?;
        }
        tx.commit().await?;

        Ok(HookDispatchSummary {
            matched_hooks: hooks.len(),
            queued_dispatches: hooks.len(),
        })
    }

    async fn enabled_hooks_for_event(
        &self,
        event_key: &str,
    ) -> Result<Vec<EnabledHookTarget>, sqlx::Error> {
        let rows = sqlx::query(
            r#"
            select
                pi.plugin_id,
                pkg.public_id::text as package_id,
                h.id as hook_id,
                h.handler
            from plugin_hooks h
            join plugin_packages pkg on pkg.id = h.package_id
            join plugin_installations pi on pi.active_package_id = pkg.id
            where h.event_key = $1
              and h.enabled = true
              and pi.enabled = true
              and pi.approval_status = 'approved'
              and pkg.package_status = 'approved'
            order by h.priority desc, h.id asc
            "#,
        )
        .bind(event_key)
        .fetch_all(&self.pool)
        .await?;

        rows.into_iter()
            .map(|row| {
                Ok(EnabledHookTarget {
                    plugin_id: row.try_get("plugin_id")?,
                    package_id: row.try_get("package_id")?,
                    hook_id: row.try_get("hook_id")?,
                    handler: row.try_get("handler")?,
                })
            })
            .collect()
    }
}

fn dispatch_payload(event: &PluginHookEvent, hook: &EnabledHookTarget) -> Value {
    json!({
        "pluginId": hook.plugin_id,
        "packageId": hook.package_id,
        "hookId": hook.hook_id,
        "handler": hook.handler,
        "hookEvent": event.event_key,
        "source": {
            "aggregateType": event.aggregate_type,
            "aggregateId": event.aggregate_id,
            "payload": event.payload.clone(),
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dispatch_payload_preserves_source_event_boundary() {
        let event = PluginHookEvent {
            event_key: "library.scan.completed".to_owned(),
            aggregate_type: "library".to_owned(),
            aggregate_id: "library-1".to_owned(),
            payload: json!({ "scannedFiles": 2 }),
        };
        let hook = EnabledHookTarget {
            plugin_id: "dev.fbz.notify".to_owned(),
            package_id: "package-1".to_owned(),
            hook_id: 7,
            handler: "hooks.onScanCompleted".to_owned(),
        };

        let payload = dispatch_payload(&event, &hook);

        assert_eq!(payload["pluginId"], "dev.fbz.notify");
        assert_eq!(payload["hookEvent"], "library.scan.completed");
        assert_eq!(payload["source"]["aggregateType"], "library");
        assert_eq!(payload["source"]["payload"]["scannedFiles"], 2);
    }
}
