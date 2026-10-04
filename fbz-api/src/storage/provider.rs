use crate::{
    error::AppError,
    plugins::{
        invoke::{PluginSyncInvoker, PluginSyncSubscriber, SyncAuditMode},
        manifest::PluginManifest,
    },
    state::AppState,
};
use serde_json::Value;
use sqlx::Row;

pub async fn target(state: &AppState, provider_id: &str) -> Result<PluginSyncSubscriber, AppError> {
    let database = state
        .database()
        .ok_or_else(|| AppError::internal("database unavailable"))?;
    let row=sqlx::query("select pkg.manifest,pkg.public_id::text as package_id,pkg.entrypoint from plugin_installations pi join plugin_packages pkg on pkg.id=pi.active_package_id where pi.plugin_id=$1 and pi.enabled=true and pi.approval_status='approved' and pkg.package_status='approved'")
 .bind(provider_id).fetch_optional(database).await.map_err(|_|AppError::internal("plugin lookup failed"))?.ok_or_else(||AppError::unprocessable("请先安装并启用存储插件"))?;
    let manifest: PluginManifest = serde_json::from_value(row.get("manifest"))
        .map_err(|_| AppError::internal("plugin manifest invalid"))?;
    let capability = manifest
        .storage_provider
        .as_ref()
        .ok_or_else(|| AppError::unprocessable("此插件未声明存储能力"))?;
    if manifest.id != provider_id
        || manifest.runtime != "http"
        || !manifest
            .permissions
            .iter()
            .any(|p| p.key == "storage.provider")
    {
        return Err(AppError::unprocessable("存储插件权限无效"));
    }
    Ok(PluginSyncSubscriber {
        plugin_id: manifest.id,
        package_id: row.get("package_id"),
        hook_id: 0,
        handler: capability.handler.clone(),
        entrypoint: crate::plugins::invoke::resolved_http_entrypoint(
            provider_id,
            &row.get::<String, _>("entrypoint"),
        ),
        runtime: manifest.runtime,
    })
}
pub async fn invoke(state: &AppState, provider_id: &str, input: &Value) -> Result<Value, AppError> {
    let subscriber = target(state, provider_id).await?;
    let invoker = PluginSyncInvoker::new(
        state
            .database()
            .ok_or_else(|| AppError::internal("database unavailable"))?
            .clone(),
        state.config().plugins.clone(),
    );
    let outcome = invoker
        .invoke_with_response_limit(
            &subscriber,
            "storage.provider.request",
            input,
            SyncAuditMode::FailuresOnly,
            45 * 1024 * 1024,
            55_000,
        )
        .await;
    outcome
        .result
        .map_err(|_| AppError::unprocessable("存储插件请求失败或超时"))
}
