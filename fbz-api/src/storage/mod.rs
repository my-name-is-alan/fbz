pub mod importer;
pub mod nfo;
pub mod provider;
pub mod scan;
mod settings;

use crate::{
    admin::access::authenticate_admin, db::DbPool, error::AppError,
    notifications::secrets::SecretCipher, state::AppState,
};
use axum::{
    Json, Router,
    extract::{Path, Query, State},
    http::{HeaderMap, StatusCode, Uri, header},
    response::{IntoResponse, Response},
    routing::{get, post},
};
use serde::Deserialize;
use serde_json::{Value, json};
use sqlx::Row;
use std::time::Duration;

pub fn db(state: &AppState) -> Result<&DbPool, AppError> {
    state
        .database()
        .ok_or_else(|| AppError::internal("database unavailable"))
}
pub fn sql_error(_: sqlx::Error) -> AppError {
    AppError::internal("storage database operation failed")
}
fn cipher(state: &AppState) -> Result<SecretCipher, AppError> {
    SecretCipher::from_config(&state.config().secrets)
        .map_err(|_| AppError::unprocessable("请先配置 FBZ_SECRET_KEY，再连接光鸭账号"))
}
pub fn router() -> Router<AppState> {
    Router::new()
        .merge(settings::router())
        .route("/api/admin/storage", get(overview))
        .route("/api/admin/storage/accounts", post(create_account))
        .route("/api/admin/storage/accounts/{id}/login", post(start_login))
        .route("/api/admin/storage/accounts/{id}/poll", post(poll_login))
        .route(
            "/api/admin/storage/accounts/{id}/disconnect",
            post(disconnect),
        )
        .route(
            "/api/admin/storage/accounts/{id}/directories",
            get(directories),
        )
        .route("/api/admin/storage/mounts", post(create_mount))
        .route("/api/admin/storage/mounts/{id}/scan", post(queue_scan))
}

pub async fn rpc(state: &AppState, account_id: &str, mut input: Value) -> Result<Value, AppError> {
    use provider::{
        StorageProviderContext, global_registry, load_plugin_key, user_message_for_provider_error,
    };

    let c = cipher(state)?;
    let mut tx = db(state)?.begin().await.map_err(sql_error)?;
    // This row lock serializes credentials and quotas across all FBZ processes.
    let row = sqlx::query("select *, greatest(0, extract(epoch from next_request_at-clock_timestamp()))::float8 as wait from storage_accounts where id::text=$1 for update")
        .bind(account_id).fetch_optional(&mut *tx).await.map_err(sql_error)?.ok_or_else(|| AppError::not_found("云盘账号不存在"))?;
    if input["loginEpoch"]
        .as_i64()
        .is_some_and(|v| v != row.get::<i64, _>("login_epoch"))
    {
        return Err(AppError::conflict("授权任务已取消"));
    }
    let op = input["op"].as_str().unwrap_or("").to_owned();
    let provider_id: String = row.get("provider");
    let storage_provider = global_registry().get(&provider_id).ok_or_else(|| {
        AppError::unprocessable(format!("未知存储源 `{provider_id}`，请检查扩展点注册表"))
    })?;
    let nonce: Option<Vec<u8>> = row.get("secret_nonce");
    let ciphertext: Option<Vec<u8>> = row.get("secret_ciphertext");
    let credentials: Value = if let (Some(nonce), Some(ciphertext)) = (nonce, ciphertext) {
        let raw = c
            .decrypt_scoped(
                "storage-account",
                account_id,
                "credentials",
                &nonce,
                &ciphertext,
            )
            .map_err(|_| AppError::internal("云盘凭据解密失败，请检查密钥"))?;
        serde_json::from_str(&raw).map_err(|_| AppError::internal("invalid stored credential"))?
    } else {
        json!({})
    };
    if !op.starts_with("auth.") && credentials["access_token"].as_str().is_none() {
        return Err(AppError::unauthorized("请先扫码连接云盘账号"));
    }
    let wait: f64 = row.get("wait");
    if wait > 2.0 {
        return Err(AppError::unprocessable("云盘限流冷却中，请稍后重试"));
    }
    tokio::time::sleep(Duration::from_secs_f64(wait.max(0.0))).await;

    let plugin_key = load_plugin_key().map_err(|e| AppError::unprocessable(e.message))?;
    let ctx = StorageProviderContext {
        account_id: account_id.to_owned(),
        device_id: row.get::<String, _>("device_id"),
        credential_version: row.get::<i64, _>("version"),
        qps: row.get::<i32, _>("qps"),
        credentials,
        plugin_key,
    };

    // Drop op from the mutable bag; adapter re-inserts it with account fields.
    if let Some(obj) = input.as_object_mut() {
        obj.remove("op");
    }

    let outcome = storage_provider
        .invoke(&ctx, &op, input)
        .await
        .map_err(|e| {
            if e.code == "transport" || e.code == "invalid_request" {
                AppError::unprocessable(e.message)
            } else {
                AppError::internal(e.message)
            }
        })?;

    if outcome.data["authenticated"] == true {
        let cloud_id: Option<String> = row.get("cloud_user_id");
        if cloud_id
            .as_deref()
            .is_some_and(|v| Some(v) != outcome.data["userId"].as_str())
        {
            return Err(AppError::unprocessable(
                "扫码账号与原绑定账号不同，请添加新账号",
            ));
        }
    }
    if let Some(ref refreshed) = outcome.credentials {
        let encrypted = c
            .encrypt_scoped(
                "storage-account",
                account_id,
                "credentials",
                &refreshed.to_string(),
            )
            .map_err(|_| AppError::internal("凭据加密失败"))?;
        sqlx::query("update storage_accounts set secret_nonce=$2,secret_ciphertext=$3,version=version+1,status='ready',updated_at=now(),cloud_user_id=coalesce($4,cloud_user_id) where id::text=$1")
            .bind(account_id).bind(encrypted.nonce).bind(encrypted.ciphertext).bind(outcome.data["userId"].as_str()).execute(&mut *tx).await.map_err(sql_error)?;
    }
    let cooldown = outcome
        .retry_after
        .clamp(0.0, 300.0)
        .max(1.0 / row.get::<i32, _>("qps") as f64);
    sqlx::query("update storage_accounts set next_request_at=clock_timestamp()+$2*interval '1 second' where id::text=$1").bind(account_id).bind(cooldown).execute(&mut *tx).await.map_err(sql_error)?;
    if outcome.error_code.as_deref() == Some("auth_expired") {
        sqlx::query("update storage_accounts set status='expired' where id::text=$1")
            .bind(account_id)
            .execute(&mut *tx)
            .await
            .map_err(sql_error)?;
    }
    tx.commit().await.map_err(sql_error)?;
    if let Some(code) = outcome.error_code.as_deref() {
        return Err(AppError::unprocessable(user_message_for_provider_error(
            &provider_id,
            code,
        )));
    }
    Ok(outcome.data)
}

async fn overview(
    State(state): State<AppState>,
    headers: HeaderMap,
    uri: Uri,
) -> Result<Json<Value>, AppError> {
    authenticate_admin(&state, &headers, &uri).await?;
    let accounts: Vec<Value> = sqlx::query_scalar("select jsonb_build_object('id',id,'name',name,'status',status,'cloudUserId',cloud_user_id,'qps',qps) from storage_accounts order by created_at").fetch_all(db(&state)?).await.map_err(sql_error)?;
    let mounts: Vec<Value> = sqlx::query_scalar("select jsonb_build_object('id',m.id,'accountId',m.account_id,'name',m.name,'mountPath',m.mount_path,'refreshMinutes',m.refresh_minutes,'nfoSource',m.nfo_source,'imageCache',m.image_cache,'lastRefreshedAt',m.last_refreshed_at,'nextRefreshAt',m.next_refresh_at,'libraryId',l.public_id,'rootId',root_id,'path',display_path,'status',status,'scanned',scanned,'imported',imported,'lastError',last_error) from storage_mounts m left join libraries l on l.id=m.library_id order by m.updated_at desc").fetch_all(db(&state)?).await.map_err(sql_error)?;
    Ok(Json(json!({
        "accounts": accounts,
        "mounts": mounts,
        "configured": cipher(&state).is_ok() && std::env::var(provider::STORAGE_PLUGIN_KEY_ENV).is_ok(),
        "providers": provider::global_registry().ids(),
    })))
}
#[derive(Deserialize)]
struct AccountInput {
    name: String,
    qps: Option<i32>,
    /// Registered StorageProvider id; defaults to the Guangya transitional adapter.
    provider: Option<String>,
}
async fn create_account(
    State(state): State<AppState>,
    headers: HeaderMap,
    uri: Uri,
    Json(input): Json<AccountInput>,
) -> Result<Json<Value>, AppError> {
    authenticate_admin(&state, &headers, &uri).await?;
    cipher(&state)?;
    if input.name.trim().is_empty() || input.name.len() > 120 {
        return Err(AppError::unprocessable("请填写账号名称"));
    }
    let provider_id = input
        .provider
        .as_deref()
        .map(str::trim)
        .filter(|v| !v.is_empty())
        .unwrap_or(provider::GUANGYA_PROVIDER_ID);
    if !provider::global_registry().contains(provider_id) {
        return Err(AppError::unprocessable(format!(
            "未知存储源 `{provider_id}`，当前已注册: {}",
            provider::global_registry().ids().join(", ")
        )));
    }
    let id: String = sqlx::query_scalar(
        "insert into storage_accounts(provider,name,device_id,qps) values($1,$2,gen_random_uuid()::text,$3) returning id::text",
    )
    .bind(provider_id)
    .bind(input.name.trim())
    .bind(input.qps.unwrap_or(10).clamp(1, 20))
    .fetch_one(db(&state)?)
    .await
    .map_err(sql_error)?;
    Ok(Json(json!({"id": id, "provider": provider_id})))
}
async fn start_login(
    State(state): State<AppState>,
    Path(id): Path<String>,
    headers: HeaderMap,
    uri: Uri,
) -> Result<Json<Value>, AppError> {
    authenticate_admin(&state, &headers, &uri).await?;
    let epoch:i64=sqlx::query_scalar("update storage_accounts set login_epoch=login_epoch+1 where id::text=$1 returning login_epoch").bind(&id).fetch_one(db(&state)?).await.map_err(sql_error)?;
    let data = rpc(&state, &id, json!({"op":"auth.start","loginEpoch":epoch})).await?;
    let code = data["device_code"]
        .as_str()
        .ok_or_else(|| AppError::internal("云盘未返回授权码"))?;
    let encrypted = cipher(&state)?
        .encrypt_scoped("storage-login", &id, "device-code", code)
        .map_err(|_| AppError::internal("授权码加密失败"))?;
    let interval = data["interval"].as_i64().unwrap_or(5).clamp(2, 60) as i32;
    let expires = data["expires_in"].as_i64().unwrap_or(300).clamp(30, 1800) as f64;
    sqlx::query("update storage_login_attempts set finished=true where account_id::text=$1")
        .bind(&id)
        .execute(db(&state)?)
        .await
        .map_err(sql_error)?;
    let attempt:String=sqlx::query_scalar("insert into storage_login_attempts(account_id,nonce,ciphertext,interval_seconds,expires_at,login_epoch) values($1::uuid,$2,$3,$4,now()+$5*interval '1 second',$6) returning id::text").bind(&id).bind(encrypted.nonce).bind(encrypted.ciphertext).bind(interval).bind(expires).bind(epoch).fetch_one(db(&state)?).await.map_err(sql_error)?;
    let url = data["verification_uri_complete"]
        .as_str()
        .or(data["verification_url"].as_str())
        .or(data["verification_uri"].as_str())
        .unwrap_or("");
    if !url.starts_with("https://") {
        return Err(AppError::unprocessable("未获得安全的扫码地址"));
    }
    Ok(Json(
        json!({"attemptId":attempt,"url":url,"userCode":data["user_code"],"interval":interval,"expiresIn":expires}),
    ))
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct PollInput {
    attempt_id: String,
}
async fn poll_login(
    State(state): State<AppState>,
    Path(id): Path<String>,
    headers: HeaderMap,
    uri: Uri,
    Json(input): Json<PollInput>,
) -> Result<Json<Value>, AppError> {
    authenticate_admin(&state, &headers, &uri).await?;
    let row=sqlx::query("update storage_login_attempts set next_poll_at=now()+interval_seconds*interval '1 second' where id::text=$1 and account_id::text=$2 and not finished and expires_at>now() and next_poll_at<=now() returning nonce,ciphertext,login_epoch")
        .bind(&input.attempt_id).bind(&id).fetch_optional(db(&state)?).await.map_err(sql_error)?;
    let Some(row) = row else {
        return Err(AppError::unprocessable("授权任务过期、已完成或轮询过快"));
    };
    let code = cipher(&state)?
        .decrypt_scoped(
            "storage-login",
            &id,
            "device-code",
            &row.get::<Vec<u8>, _>("nonce"),
            &row.get::<Vec<u8>, _>("ciphertext"),
        )
        .map_err(|_| AppError::internal("授权码解密失败"))?;
    let result = rpc(
        &state,
        &id,
        json!({"op":"auth.poll","deviceCode":code,"loginEpoch":row.get::<i64,_>("login_epoch")}),
    )
    .await?;
    if result["authenticated"] == true {
        sqlx::query("update storage_login_attempts set finished=true,ciphertext=''::bytea,nonce=''::bytea where id::text=$1").bind(&input.attempt_id).execute(db(&state)?).await.map_err(sql_error)?;
    }
    if result["slowDown"] == true {
        sqlx::query("update storage_login_attempts set interval_seconds=least(interval_seconds+5,60),next_poll_at=now()+(interval_seconds+5)*interval '1 second' where id::text=$1").bind(&input.attempt_id).execute(db(&state)?).await.map_err(sql_error)?;
    }
    Ok(Json(result))
}
async fn disconnect(
    State(state): State<AppState>,
    Path(id): Path<String>,
    headers: HeaderMap,
    uri: Uri,
) -> Result<StatusCode, AppError> {
    authenticate_admin(&state, &headers, &uri).await?;
    let mut tx = db(&state)?.begin().await.map_err(sql_error)?;
    sqlx::query("update storage_accounts set secret_nonce=null,secret_ciphertext=null,status='disconnected',version=version+1,login_epoch=login_epoch+1 where id::text=$1").bind(&id).execute(&mut *tx).await.map_err(sql_error)?;
    sqlx::query("delete from storage_login_attempts where account_id::text=$1")
        .bind(&id)
        .execute(&mut *tx)
        .await
        .map_err(sql_error)?;
    tx.commit().await.map_err(sql_error)?;
    Ok(StatusCode::NO_CONTENT)
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct DirectoryInput {
    #[serde(default)]
    parent_id: String,
    #[serde(default)]
    page: u32,
}
async fn directories(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Query(input): Query<DirectoryInput>,
    headers: HeaderMap,
    uri: Uri,
) -> Result<Json<Value>, AppError> {
    authenticate_admin(&state, &headers, &uri).await?;
    Ok(Json(
        rpc(
            &state,
            &id,
            json!({"op":"list","parentId":input.parent_id,"page":input.page}),
        )
        .await?,
    ))
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct MountInput {
    account_id: String,
    root_id: String,
    name: String,
    display_path: String,
    mount_path: String,
    refresh_minutes: Option<i32>,
}
async fn create_mount(
    State(state): State<AppState>,
    headers: HeaderMap,
    uri: Uri,
    Json(input): Json<MountInput>,
) -> Result<Json<Value>, AppError> {
    authenticate_admin(&state, &headers, &uri).await?;
    settings::validate_mount_path(&input.mount_path)?;
    let interval = input.refresh_minutes.unwrap_or(60);
    settings::validate_interval(interval)?;
    if input.name.trim().is_empty()
        || input.name.len() > 120
        || input.root_id.len() > 256
        || input.display_path.len() > 2048
    {
        return Err(AppError::unprocessable("挂载参数无效"));
    }
    rpc(
        &state,
        &input.account_id,
        json!({"op":"list","parentId":input.root_id,"page":0}),
    )
    .await?;
    let id:String=sqlx::query_scalar("insert into storage_mounts(account_id,root_id,display_path,name,mount_path,refresh_minutes) values($1::uuid,$2,$3,$4,$5,$6) returning id::text")
      .bind(&input.account_id).bind(&input.root_id).bind(&input.display_path).bind(input.name.trim()).bind(&input.mount_path).bind(interval).fetch_one(db(&state)?).await.map_err(sql_error)?;
    Ok(Json(json!({"id":id})))
}

#[derive(Deserialize)]
struct ScanInput {
    #[serde(default)]
    restart: bool,
}
async fn queue_scan(
    State(state): State<AppState>,
    Path(id): Path<String>,
    headers: HeaderMap,
    uri: Uri,
    Json(input): Json<ScanInput>,
) -> Result<StatusCode, AppError> {
    authenticate_admin(&state, &headers, &uri).await?;
    let mut tx = db(&state)?.begin().await.map_err(sql_error)?;
    let row=sqlx::query("select status,root_id,frontier,import_cursor,library_id from storage_mounts where id::text=$1 for update").bind(&id).fetch_optional(&mut *tx).await.map_err(sql_error)?.ok_or_else(||AppError::not_found("挂载不存在"))?;
    if row.get::<Option<i64>, _>("library_id").is_none() {
        return Err(AppError::unprocessable("请先在媒体库中选择此挂载"));
    }
    if ["scanning", "importing"].contains(&row.get::<String, _>("status").as_str()) {
        return Err(AppError::unprocessable("扫描正在运行"));
    }
    if input.restart || row.get::<String, _>("status") != "failed" {
        sqlx::query("update storage_mounts set status='scanning',generation=generation+1,frontier=$2,import_cursor='',scanned=0,imported=0,last_error=null,updated_at=now() where id::text=$1").bind(&id).bind(json!([{ "id":row.get::<String,_>("root_id"),"page":0 }])).execute(&mut *tx).await.map_err(sql_error)?;
    } else {
        sqlx::query("update storage_mounts set status=case when jsonb_array_length(frontier)>0 then 'scanning' else 'importing' end,last_error=null where id::text=$1").bind(&id).execute(&mut *tx).await.map_err(sql_error)?;
    }
    tx.commit().await.map_err(sql_error)?;
    Ok(StatusCode::ACCEPTED)
}

pub async fn stream(state: &AppState, media_file_id: i64) -> Result<Option<Response>, AppError> {
    let row=sqlx::query("select m.account_id::text,e.remote_id from storage_entries e join storage_mounts m on m.id=e.mount_id where e.media_file_id=$1").bind(media_file_id).fetch_optional(db(state)?).await.map_err(sql_error)?;
    let Some(row) = row else {
        return Ok(None);
    };
    let data = rpc(
        state,
        &row.get::<String, _>("account_id"),
        json!({"op":"resolve","fileId":row.get::<String,_>("remote_id")}),
    )
    .await?;
    let url = data["url"]
        .as_str()
        .ok_or_else(|| AppError::internal("云盘未返回播放地址"))?;
    let parsed = reqwest::Url::parse(url).map_err(|_| AppError::unprocessable("播放地址无效"))?;
    if parsed.scheme() != "https" || !parsed.username().is_empty() || parsed.password().is_some() {
        return Err(AppError::unprocessable("播放地址不安全"));
    }
    let location =
        header::HeaderValue::from_str(url).map_err(|_| AppError::unprocessable("播放地址无效"))?;
    Ok(Some(
        (
            StatusCode::FOUND,
            [
                (header::LOCATION, location),
                (
                    header::CACHE_CONTROL,
                    header::HeaderValue::from_static("no-store"),
                ),
            ],
        )
            .into_response(),
    ))
}

pub async fn read_small(
    state: &AppState,
    account: &str,
    file_id: &str,
    max: usize,
) -> Result<Vec<u8>, AppError> {
    let result = rpc(
        state,
        account,
        json!({"op":"read","fileId":file_id,"maxBytes":max}),
    )
    .await?;
    let bytes: Vec<u8> = serde_json::from_value(result["bytes"].clone())
        .map_err(|_| AppError::internal("小文件响应无效"))?;
    if bytes.len() > max {
        return Err(AppError::unprocessable("小文件超出限制"));
    }
    Ok(bytes)
}

/// Short-lived signed URL cache for byte relays; every request still checks user
/// permissions and this account credential generation before reaching the cache.
pub async fn relay_url(state: &AppState, file_id: i64, force: bool) -> Result<String, AppError> {
    use std::{
        collections::HashMap,
        sync::{Mutex, OnceLock},
        time::{Instant, SystemTime, UNIX_EPOCH},
    };
    static CACHE: OnceLock<Mutex<HashMap<String, (String, Instant)>>> = OnceLock::new();
    let row=sqlx::query("select m.account_id::text,e.remote_id,a.version,a.status from storage_entries e join storage_mounts m on m.id=e.mount_id join storage_accounts a on a.id=m.account_id where e.media_file_id=$1")
        .bind(file_id).fetch_optional(db(state)?).await.map_err(sql_error)?.ok_or_else(||AppError::not_found("cloud source not found"))?;
    if row.get::<String, _>("status") != "ready" {
        return Err(AppError::unauthorized(
            "cloud account is disconnected or expired",
        ));
    }
    let account: String = row.get("account_id");
    let remote: String = row.get("remote_id");
    let key = format!("{}:{}:{}", account, row.get::<i64, _>("version"), file_id);
    let cache = CACHE.get_or_init(|| Mutex::new(HashMap::new()));
    if !force {
        if let Some((url, expires)) = cache
            .lock()
            .map_err(|_| AppError::internal("URL cache unavailable"))?
            .get(&key)
        {
            if *expires > Instant::now() {
                return Ok(url.clone());
            }
        }
    }
    let result = rpc(
        state,
        &account,
        json!({"op":"resolve","fileId":remote,"force":force}),
    )
    .await?;
    let url = result["url"]
        .as_str()
        .ok_or_else(|| AppError::internal("cloud source returned no URL"))?
        .to_owned();
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64;
    let ttl = result["expiresAt"]
        .as_u64()
        .map(|end| end.saturating_sub(now))
        .unwrap_or(20000)
        .min(20000);
    let mut entries = cache
        .lock()
        .map_err(|_| AppError::internal("URL cache unavailable"))?;
    entries.retain(|_, (_, expiry)| *expiry > Instant::now());
    if entries.len() >= 1024 {
        entries.clear();
    }
    if ttl > 0 {
        entries.insert(
            key,
            (url.clone(), Instant::now() + Duration::from_millis(ttl)),
        );
    }
    Ok(url)
}
