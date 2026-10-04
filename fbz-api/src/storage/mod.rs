pub mod importer;
pub mod nfo;
pub mod scan;

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
        return Err(AppError::unauthorized("请先扫码连接光鸭"));
    }
    let wait: f64 = row.get("wait");
    if wait > 2.0 {
        return Err(AppError::unprocessable("光鸭限流冷却中，请稍后重试"));
    }
    tokio::time::sleep(Duration::from_secs_f64(wait.max(0.0))).await;
    input["accountId"] = json!(account_id);
    input["deviceId"] = json!(row.get::<String, _>("device_id"));
    input["credentialVersion"] = json!(row.get::<i64, _>("version"));
    input["qps"] = json!(row.get::<i32, _>("qps"));
    input["credentials"] = credentials;
    let endpoint = std::env::var("FBZ_GUANGYA_PLUGIN_URL")
        .unwrap_or_else(|_| "http://127.0.0.1:8098/rpc".into());
    let key = std::env::var("FBZ_STORAGE_PLUGIN_KEY")
        .map_err(|_| AppError::unprocessable("请配置 FBZ_STORAGE_PLUGIN_KEY 并启动光鸭插件"))?;
    if key.len() < 32 {
        return Err(AppError::unprocessable("插件通信密钥至少需要 32 字符"));
    }
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(55))
        .redirect(reqwest::redirect::Policy::none())
        .build()
        .map_err(|_| AppError::internal("plugin client unavailable"))?;
    let mut response = client
        .post(endpoint)
        .bearer_auth(key)
        .json(&input)
        .send()
        .await
        .map_err(|_| AppError::internal("光鸭插件连接失败"))?;
    if !response.status().is_success() {
        return Err(AppError::internal("光鸭插件拒绝请求，请检查通信密钥"));
    }
    let mut bytes = Vec::new();
    while let Some(chunk) = response
        .chunk()
        .await
        .map_err(|_| AppError::internal("插件响应读取失败"))?
    {
        if bytes.len() + chunk.len() > 45 * 1024 * 1024 {
            return Err(AppError::unprocessable("插件响应超出上限"));
        }
        bytes.extend_from_slice(&chunk);
    }
    let result: Value =
        serde_json::from_slice(&bytes).map_err(|_| AppError::internal("插件响应无效"))?;
    if result["data"]["authenticated"] == true {
        let cloud_id: Option<String> = row.get("cloud_user_id");
        if cloud_id
            .as_deref()
            .is_some_and(|v| Some(v) != result["data"]["userId"].as_str())
        {
            return Err(AppError::unprocessable(
                "扫码账号与原绑定账号不同，请添加新账号",
            ));
        }
    }
    if result["credentials"].is_object() {
        let encrypted = c
            .encrypt_scoped(
                "storage-account",
                account_id,
                "credentials",
                &result["credentials"].to_string(),
            )
            .map_err(|_| AppError::internal("凭据加密失败"))?;
        sqlx::query("update storage_accounts set secret_nonce=$2,secret_ciphertext=$3,version=version+1,status='ready',updated_at=now(),cloud_user_id=coalesce($4,cloud_user_id) where id::text=$1")
            .bind(account_id).bind(encrypted.nonce).bind(encrypted.ciphertext).bind(result["data"]["userId"].as_str()).execute(&mut *tx).await.map_err(sql_error)?;
    }
    let cooldown = result["error"]["retryAfter"]
        .as_f64()
        .unwrap_or(0.0)
        .clamp(0.0, 300.0)
        .max(1.0 / row.get::<i32, _>("qps") as f64);
    sqlx::query("update storage_accounts set next_request_at=clock_timestamp()+$2*interval '1 second' where id::text=$1").bind(account_id).bind(cooldown).execute(&mut *tx).await.map_err(sql_error)?;
    if result["error"]["code"] == "auth_expired" {
        sqlx::query("update storage_accounts set status='expired' where id::text=$1")
            .bind(account_id)
            .execute(&mut *tx)
            .await
            .map_err(sql_error)?;
    }
    tx.commit().await.map_err(sql_error)?;
    if let Some(code) = result["error"]["code"].as_str() {
        return Err(AppError::unprocessable(match code {
            "auth_expired" => "光鸭登录已失效，请重新扫码",
            "rate_limited" => "光鸭限流，请稍后重试",
            "unsafe_url" => "云端文件地址被安全策略拒绝",
            _ => "光鸭操作失败，请稍后重试或检查插件配置",
        }));
    }
    Ok(result["data"].clone())
}

async fn overview(
    State(state): State<AppState>,
    headers: HeaderMap,
    uri: Uri,
) -> Result<Json<Value>, AppError> {
    authenticate_admin(&state, &headers, &uri).await?;
    let accounts: Vec<Value> = sqlx::query_scalar("select jsonb_build_object('id',id,'name',name,'status',status,'cloudUserId',cloud_user_id,'qps',qps) from storage_accounts order by created_at").fetch_all(db(&state)?).await.map_err(sql_error)?;
    let mounts: Vec<Value> = sqlx::query_scalar("select jsonb_build_object('id',m.id,'accountId',m.account_id,'name',l.name,'libraryId',l.public_id,'rootId',root_id,'path',display_path,'status',status,'scanned',scanned,'imported',imported,'lastError',last_error) from storage_mounts m join libraries l on l.id=m.library_id order by m.updated_at desc").fetch_all(db(&state)?).await.map_err(sql_error)?;
    Ok(Json(
        json!({"accounts":accounts,"mounts":mounts,"configured":cipher(&state).is_ok() && std::env::var("FBZ_STORAGE_PLUGIN_KEY").is_ok()}),
    ))
}
#[derive(Deserialize)]
struct AccountInput {
    name: String,
    qps: Option<i32>,
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
    let id:String=sqlx::query_scalar("insert into storage_accounts(provider,name,device_id,qps) values('guangya',$1,gen_random_uuid()::text,$2) returning id::text").bind(input.name.trim()).bind(input.qps.unwrap_or(1).clamp(1,5)).fetch_one(db(&state)?).await.map_err(sql_error)?;
    Ok(Json(json!({"id":id})))
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
        .ok_or_else(|| AppError::internal("光鸭未返回授权码"))?;
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
    library_type: String,
}
async fn create_mount(
    State(state): State<AppState>,
    headers: HeaderMap,
    uri: Uri,
    Json(input): Json<MountInput>,
) -> Result<Json<Value>, AppError> {
    let admin = authenticate_admin(&state, &headers, &uri).await?;
    if !["movies", "tv"].contains(&input.library_type.as_str())
        || input.name.trim().is_empty()
        || input.name.len() > 120
        || input.root_id.len() > 256
        || input.display_path.len() > 2048
    {
        return Err(AppError::unprocessable("媒体库参数无效"));
    }
    rpc(
        &state,
        &input.account_id,
        json!({"op":"list","parentId":input.root_id,"page":0}),
    )
    .await?;
    let mut tx = db(&state)?.begin().await.map_err(sql_error)?;
    let lib: i64 =
        sqlx::query_scalar("insert into libraries(name,library_type) values($1,$2) returning id")
            .bind(input.name.trim())
            .bind(&input.library_type)
            .fetch_one(&mut *tx)
            .await
            .map_err(sql_error)?;
    sqlx::query("insert into library_permissions(library_id,user_id,can_view,can_download,can_transcode) values($1,$2,true,true,false)").bind(lib).bind(admin.id).execute(&mut *tx).await.map_err(sql_error)?;
    let id:String=sqlx::query_scalar("insert into storage_mounts(account_id,library_id,root_id,display_path) values($1::uuid,$2,$3,$4) returning id::text").bind(&input.account_id).bind(lib).bind(&input.root_id).bind(&input.display_path).fetch_one(&mut *tx).await.map_err(sql_error)?;
    tx.commit().await.map_err(sql_error)?;
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
    let row=sqlx::query("select status,root_id,frontier,import_cursor from storage_mounts where id::text=$1 for update").bind(&id).fetch_optional(&mut *tx).await.map_err(sql_error)?.ok_or_else(||AppError::not_found("挂载不存在"))?;
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
        .ok_or_else(|| AppError::internal("光鸭未返回播放地址"))?;
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
