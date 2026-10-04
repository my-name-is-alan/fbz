use super::*;

pub fn router() -> Router<AppState> {
    Router::new()
        .route(
            "/api/admin/storage/accounts/{id}/settings",
            post(account_settings),
        )
        .route(
            "/api/admin/storage/mounts/{id}/settings",
            post(mount_settings),
        )
        .route(
            "/api/admin/storage/mounts/{id}/library",
            post(create_library),
        )
}
pub fn validate_interval(value: i32) -> Result<(), AppError> {
    if value != 0 && !(5..=10080).contains(&value) {
        return Err(AppError::unprocessable(
            "刷新周期应为 5–10080 分钟，0 表示手动",
        ));
    }
    Ok(())
}
pub fn validate_mount_path(value: &str) -> Result<(), AppError> {
    if !value.starts_with("/cloud/")
        || value.len() > 240
        || value[7..].is_empty()
        || value.split('/').skip(2).any(|s| {
            s.is_empty()
                || s == "."
                || s == ".."
                || s.chars()
                    .any(|c| c.is_control() || c == '\\' || c == ':' || c == '?' || c == '#')
        })
    {
        return Err(AppError::unprocessable(
            "挂载地址必须是 /cloud/ 下的有效目录",
        ));
    }
    Ok(())
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Settings {
    nfo_source: String,
    image_cache: String,
    refresh_minutes: i32,
}
fn validate_settings(s: &Settings) -> Result<(), AppError> {
    validate_interval(s.refresh_minutes)?;
    if !["cloud", "filename"].contains(&s.nfo_source.as_str())
        || !["prefetch", "on_demand"].contains(&s.image_cache.as_str())
    {
        return Err(AppError::unprocessable("资料来源或图片缓存策略无效"));
    }
    Ok(())
}
async fn account_settings(
    State(state): State<AppState>,
    Path(id): Path<String>,
    headers: HeaderMap,
    uri: Uri,
    Json(input): Json<AccountInput>,
) -> Result<StatusCode, AppError> {
    authenticate_admin(&state, &headers, &uri).await?;
    if input.name.trim().is_empty()
        || input.name.len() > 120
        || !(1..=20).contains(&input.qps.unwrap_or(10))
    {
        return Err(AppError::unprocessable("账号名称或 QPS 无效"));
    }
    let result = sqlx::query(
        "update storage_accounts set name=$2,qps=$3,updated_at=now() where id::text=$1",
    )
    .bind(id)
    .bind(input.name.trim())
    .bind(input.qps.unwrap_or(10))
    .execute(db(&state)?)
    .await
    .map_err(sql_error)?;
    if result.rows_affected() == 0 {
        return Err(AppError::not_found("账号不存在"));
    }
    Ok(StatusCode::NO_CONTENT)
}
async fn mount_settings(
    State(state): State<AppState>,
    Path(id): Path<String>,
    headers: HeaderMap,
    uri: Uri,
    Json(input): Json<Settings>,
) -> Result<StatusCode, AppError> {
    authenticate_admin(&state, &headers, &uri).await?;
    validate_settings(&input)?;
    let result=sqlx::query("update storage_mounts set nfo_source=$2,image_cache=$3,refresh_minutes=$4,next_refresh_at=now()+greatest($4,5)*interval '1 minute',updated_at=now() where id::text=$1 and status not in ('scanning','importing')").bind(id).bind(input.nfo_source).bind(input.image_cache).bind(input.refresh_minutes).execute(db(&state)?).await.map_err(sql_error)?;
    if result.rows_affected() == 0 {
        return Err(AppError::conflict("挂载不存在或正在扫描，请稍后修改"));
    }
    Ok(StatusCode::NO_CONTENT)
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct LibraryInput {
    name: String,
    library_type: String,
    #[serde(flatten)]
    settings: Settings,
}
async fn create_library(
    State(state): State<AppState>,
    Path(id): Path<String>,
    headers: HeaderMap,
    uri: Uri,
    Json(input): Json<LibraryInput>,
) -> Result<Json<Value>, AppError> {
    let admin = authenticate_admin(&state, &headers, &uri).await?;
    validate_settings(&input.settings)?;
    if input.name.trim().is_empty()
        || input.name.len() > 120
        || !["movies", "tv"].contains(&input.library_type.as_str())
    {
        return Err(AppError::unprocessable("媒体库名称或类型无效"));
    }
    let mut tx = db(&state)?.begin().await.map_err(sql_error)?;
    let mount = sqlx::query("select library_id from storage_mounts where id::text=$1 for update")
        .bind(&id)
        .fetch_optional(&mut *tx)
        .await
        .map_err(sql_error)?
        .ok_or_else(|| AppError::not_found("挂载不存在"))?;
    if mount.get::<Option<i64>, _>("library_id").is_some() {
        return Err(AppError::conflict("此挂载已关联媒体库，请选择其他挂载"));
    }
    let lib: i64 =
        sqlx::query_scalar("insert into libraries(name,library_type) values($1,$2) returning id")
            .bind(input.name.trim())
            .bind(input.library_type)
            .fetch_one(&mut *tx)
            .await
            .map_err(sql_error)?;
    sqlx::query("insert into library_permissions(library_id,user_id,can_view,can_download,can_transcode) values($1,$2,true,true,false)").bind(lib).bind(admin.id).execute(&mut *tx).await.map_err(sql_error)?;
    sqlx::query("update storage_mounts set library_id=$2,nfo_source=$3,image_cache=$4,refresh_minutes=$5,next_refresh_at=now()+greatest($5,5)*interval '1 minute',updated_at=now() where id::text=$1").bind(id).bind(lib).bind(input.settings.nfo_source).bind(input.settings.image_cache).bind(input.settings.refresh_minutes).execute(&mut *tx).await.map_err(sql_error)?;
    let public_id: String = sqlx::query_scalar("select public_id::text from libraries where id=$1")
        .bind(lib)
        .fetch_one(&mut *tx)
        .await
        .map_err(sql_error)?;
    tx.commit().await.map_err(sql_error)?;
    Ok(Json(json!({"id":public_id})))
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn mount_paths_are_virtual_and_bounded() {
        for s in [
            "/media/a",
            "/cloud/",
            "/cloud/../a",
            "/cloud/a//b",
            "/cloud/a\\b",
        ] {
            assert!(validate_mount_path(s).is_err());
        }
        assert!(validate_mount_path("/cloud/电影").is_ok());
    }
    #[test]
    fn refresh_can_be_disabled_but_not_spin() {
        assert!(validate_interval(0).is_ok());
        assert!(validate_interval(5).is_ok());
        assert!(validate_interval(1).is_err());
        assert!(validate_interval(-1).is_err());
    }
}
