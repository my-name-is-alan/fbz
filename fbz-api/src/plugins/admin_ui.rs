use crate::{
    admin::access::authenticate_admin,
    error::AppError,
    plugins::{
        invoke::{PluginSyncInvoker, PluginSyncSubscriber, SyncAuditMode},
        manifest::PluginManifest,
    },
    state::AppState,
};
use axum::{
    Json, Router,
    extract::{DefaultBodyLimit, Path, State},
    http::{HeaderMap, StatusCode, Uri},
    routing::{get, post},
};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sqlx::Row;
use std::{
    io::Read,
    path::{Component, Path as FsPath, PathBuf},
};

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/api/admin/plugins/{plugin_id}/ui", get(get_ui))
        .route(
            "/api/admin/plugins/{plugin_id}/ui/actions/{action}",
            post(invoke_action).layer(DefaultBodyLimit::max(32768)),
        )
}
struct ActiveUi {
    manifest: PluginManifest,
    package_path: String,
    entrypoint: String,
    package_id: String,
}
async fn active_ui(state: &AppState, plugin_id: &str) -> Result<ActiveUi, AppError> {
    if plugin_id.len() > 128
        || !plugin_id.bytes().all(|b| {
            b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'.' || b == b'-' || b == b'_'
        })
    {
        return Err(AppError::unprocessable("invalid plugin id"));
    }
    let database = state
        .database()
        .ok_or_else(|| AppError::internal("database unavailable"))?;
    let row=sqlx::query("select pkg.manifest,pkg.package_path,pkg.entrypoint,pkg.public_id::text as package_id from plugin_installations pi join plugin_packages pkg on pkg.id=pi.active_package_id where pi.plugin_id=$1 and pi.enabled=true and pi.approval_status='approved' and pkg.package_status='approved'")
 .bind(plugin_id).fetch_optional(database).await.map_err(|_|AppError::internal("plugin database query failed"))?.ok_or_else(||AppError::not_found("plugin is not active"))?;
    let manifest: PluginManifest = serde_json::from_value(row.get("manifest"))
        .map_err(|_| AppError::internal("plugin manifest is invalid"))?;
    if manifest.id != plugin_id
        || manifest.admin_ui.is_none()
        || !manifest.permissions.iter().any(|p| p.key == "admin.menu")
    {
        return Err(AppError::not_found("plugin page is unavailable"));
    }
    Ok(ActiveUi {
        manifest,
        package_path: row.get("package_path"),
        entrypoint: crate::plugins::invoke::resolved_http_entrypoint(
            plugin_id,
            &row.get::<String, _>("entrypoint"),
        ),
        package_id: row.get("package_id"),
    })
}
fn resolve_package(package_dir: &FsPath, relative: &str) -> Result<PathBuf, AppError> {
    let mut path = package_dir.to_path_buf();
    for part in FsPath::new(relative).components() {
        match part {
            Component::Normal(s) => path.push(s),
            Component::CurDir => {}
            _ => return Err(AppError::unprocessable("invalid plugin package path")),
        }
    }
    Ok(path)
}
fn read_ui_html(package_dir: &FsPath, package: &ActiveUi) -> Result<String, AppError> {
    let ui = package
        .manifest
        .admin_ui
        .as_ref()
        .ok_or_else(|| AppError::not_found("plugin UI unavailable"))?;
    let archive_file = std::fs::File::open(resolve_package(package_dir, &package.package_path)?)
        .map_err(|_| AppError::not_found("plugin package missing"))?;
    let mut archive = zip::ZipArchive::new(archive_file)
        .map_err(|_| AppError::internal("plugin package is invalid"))?;
    let entry = archive
        .by_name(&ui.path)
        .map_err(|_| AppError::not_found("plugin UI file missing"))?;
    if entry.size() > 512 * 1024 {
        return Err(AppError::unprocessable("plugin UI exceeds 512 KiB"));
    }
    let mut buf = Vec::new();
    entry
        .take(512 * 1024 + 1)
        .read_to_end(&mut buf)
        .map_err(|_| AppError::internal("plugin UI read failed"))?;
    if buf.len() > 512 * 1024 {
        return Err(AppError::unprocessable("plugin UI exceeds 512 KiB"));
    }
    String::from_utf8(buf).map_err(|_| AppError::unprocessable("plugin UI must be UTF-8"))
}
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct PluginUiDto {
    html: String,
    actions: Vec<String>,
    storage_provider: bool,
}
async fn get_ui(
    State(state): State<AppState>,
    Path(plugin_id): Path<String>,
    headers: HeaderMap,
    uri: Uri,
) -> Result<Json<PluginUiDto>, AppError> {
    authenticate_admin(&state, &headers, &uri).await?;
    let pkg = active_ui(&state, &plugin_id).await?;
    let actions = pkg
        .manifest
        .admin_ui
        .as_ref()
        .unwrap()
        .actions
        .iter()
        .map(|a| a.key.clone())
        .collect();
    let storage_provider = pkg.manifest.storage_provider.is_some();
    let html = tokio::task::spawn_blocking({
        let dir = state.config().plugins.package_dir.clone();
        move || read_ui_html(&dir, &pkg)
    })
    .await
    .map_err(|_| AppError::internal("plugin UI task failed"))??;
    Ok(Json(PluginUiDto {
        html,
        actions,
        storage_provider,
    }))
}
#[derive(Deserialize)]
struct ActionInput {
    #[serde(default)]
    data: Value,
}
async fn invoke_action(
    State(state): State<AppState>,
    Path((plugin_id, action)): Path<(String, String)>,
    headers: HeaderMap,
    uri: Uri,
    Json(input): Json<ActionInput>,
) -> Result<(StatusCode, Json<Value>), AppError> {
    authenticate_admin(&state, &headers, &uri).await?;
    let pkg = active_ui(&state, &plugin_id).await?;
    let action_spec = pkg
        .manifest
        .admin_ui
        .as_ref()
        .unwrap()
        .actions
        .iter()
        .find(|a| a.key == action)
        .ok_or_else(|| AppError::not_found("plugin action unavailable"))?;
    if !input.data.is_object() && !input.data.is_null() {
        return Err(AppError::unprocessable(
            "plugin action data must be an object",
        ));
    }
    if action_spec.handler == "storage.accounts" {
        if pkg.manifest.storage_provider.is_none() {
            return Err(AppError::forbidden("plugin has no storage provider"));
        }
        let accounts:Vec<Value>=sqlx::query_scalar("select jsonb_build_object('id',id,'name',name,'status',status,'qps',qps) from storage_accounts where provider=$1 order by created_at")
            .bind(&plugin_id).fetch_all(state.database().ok_or_else(||AppError::internal("database unavailable"))?).await.map_err(|_|AppError::internal("account query failed"))?;
        return Ok((StatusCode::OK, Json(json!({"accounts":accounts}))));
    }
    if action_spec.handler == "storage.createAccount" {
        crate::storage::cipher(&state)?;
        if pkg.manifest.storage_provider.is_none() {
            return Err(AppError::forbidden("plugin has no storage provider"));
        }
        let name = input.data["name"]
            .as_str()
            .ok_or_else(|| AppError::unprocessable("name is required"))?
            .trim();
        let qps = input.data["qps"].as_i64().unwrap_or(10);
        if name.is_empty() || name.len() > 120 || !(1..=20).contains(&qps) {
            return Err(AppError::unprocessable("invalid account settings"));
        }
        let id:String=sqlx::query_scalar("insert into storage_accounts(provider,name,device_id,qps) values($1,$2,gen_random_uuid()::text,$3) returning id::text")
            .bind(&plugin_id).bind(name).bind(qps as i32).fetch_one(state.database().ok_or_else(||AppError::internal("database unavailable"))?).await.map_err(|_|AppError::internal("account creation failed"))?;
        return Ok((StatusCode::OK, Json(json!({"id":id}))));
    }
    if action_spec.handler == "storage.connect" {
        if pkg.manifest.storage_provider.is_none() {
            return Err(AppError::forbidden("plugin has no storage provider"));
        }
        let account_id = input.data["accountId"]
            .as_str()
            .ok_or_else(|| AppError::unprocessable("accountId is required"))?;
        let owner: Option<String> =
            sqlx::query_scalar("select provider from storage_accounts where id::text=$1")
                .bind(account_id)
                .fetch_optional(
                    state
                        .database()
                        .ok_or_else(|| AppError::internal("database unavailable"))?,
                )
                .await
                .map_err(|_| AppError::internal("account lookup failed"))?;
        if owner.as_deref() != Some(plugin_id.as_str()) {
            return Err(AppError::forbidden(
                "account does not belong to this plugin",
            ));
        }
        let result = crate::storage::rpc(
            &state,
            account_id,
            json!({"op":"auth.connect","fields":input.data["fields"]}),
        )
        .await?;
        return Ok((StatusCode::OK, Json(result)));
    }
    let subscriber = PluginSyncSubscriber {
        plugin_id: pkg.manifest.id,
        package_id: pkg.package_id,
        hook_id: 0,
        handler: action_spec.handler.clone(),
        entrypoint: pkg.entrypoint,
        runtime: pkg.manifest.runtime,
    };
    let invoker = PluginSyncInvoker::new(
        state
            .database()
            .ok_or_else(|| AppError::internal("database unavailable"))?
            .clone(),
        state.config().plugins.clone(),
    );
    let outcome = invoker
        .invoke(
            &subscriber,
            "admin.ui",
            &json!({"action":action,"data":input.data}),
            SyncAuditMode::All,
        )
        .await;
    let result = outcome
        .result
        .map_err(|_| AppError::unprocessable("plugin action failed"))?;
    Ok((StatusCode::OK, Json(result)))
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn path_does_not_escape() {
        let base = FsPath::new("/tmp/plugins");
        assert!(resolve_package(base, "uploads/p.zip").is_ok());
        assert!(resolve_package(base, "../keys.json").is_err());
    }
}

#[cfg(test)]
mod lifecycle_tests {
    use super::*;
    use crate::plugins::repository::{InstallPluginPackageInput, PluginRepository};
    #[tokio::test]
    #[ignore = "requires explicit disposable FBZ_PLUGIN_TEST_URL and packaged Guangya ZIP"]
    async fn package_lifecycle_exposes_page_and_storage_only_while_enabled() {
        let url = std::env::var("FBZ_PLUGIN_TEST_URL").expect("disposable DB URL required");
        assert!(
            url.contains("fbz_plugin_test"),
            "must use disposable merge test DB"
        );
        let pool = sqlx::PgPool::connect(&url).await.unwrap();
        crate::db::migrate(&pool).await.unwrap();
        let repository = PluginRepository::new(pool.clone());
        let manifest: PluginManifest =
            serde_json::from_str(include_str!("../../plugins/guangya-storage/manifest.json"))
                .unwrap();
        let validated = manifest.clone().validate().unwrap();
        let package_path = "org.fbz.guangya-0.1.0.zip";
        let packaged = FsPath::new(env!("CARGO_MANIFEST_DIR"))
            .join("var/plugin-packages")
            .join(package_path);
        assert!(packaged.is_file(), "build the package first");
        let installed = repository
            .install_package(InstallPluginPackageInput {
                package_path: package_path.into(),
                checksum_sha256: None,
                signature: None,
                validated_manifest: validated,
            })
            .await
            .unwrap();
        let owner: i64 = sqlx::query_scalar("select id from users where username='storage-check'")
            .fetch_one(&pool)
            .await
            .unwrap();
        repository
            .approve_package(&installed.package_id, owner)
            .await
            .unwrap();
        repository.enable_plugin("org.fbz.guangya").await.unwrap();
        let rows = repository.list_active_menu_items().await.unwrap();
        assert!(rows.iter().any(|item| item.plugin_id == "org.fbz.guangya"));
        let legacy_before: i64 =
            sqlx::query_scalar("select count(*) from storage_accounts where provider='guangya'")
                .fetch_one(&pool)
                .await
                .unwrap();
        let plugin_account:String=sqlx::query_scalar("insert into storage_accounts(provider,name,device_id,qps) values('org.fbz.guangya','Plugin fixture','fixture-device',10) returning id::text").fetch_one(&pool).await.unwrap();
        assert_eq!(
            sqlx::query_scalar::<_, i64>(
                "select count(*) from storage_accounts where provider='guangya'"
            )
            .fetch_one(&pool)
            .await
            .unwrap(),
            legacy_before
        );
        let ui = ActiveUi {
            manifest,
            package_path: package_path.into(),
            entrypoint: "http://127.0.0.1:8098/fbz-plugin".into(),
            package_id: installed.package_id,
        };
        let html = read_ui_html(
            &FsPath::new(env!("CARGO_MANIFEST_DIR")).join("var/plugin-packages"),
            &ui,
        )
        .unwrap();
        assert!(html.contains("光鸭网盘"));
        repository.disable_plugin("org.fbz.guangya").await.unwrap();
        let rows = repository.list_active_menu_items().await.unwrap();
        assert!(!rows.iter().any(|item| item.plugin_id == "org.fbz.guangya"));
        let preserved: Option<String> =
            sqlx::query_scalar("select provider from storage_accounts where id::text=$1")
                .bind(&plugin_account)
                .fetch_optional(&pool)
                .await
                .unwrap();
        assert_eq!(preserved.as_deref(), Some("org.fbz.guangya"));
    }
}
