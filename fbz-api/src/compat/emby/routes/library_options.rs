//! Jellyfin `POST /Library/VirtualFolders/LibraryOptions`（库选项保存）子集：
//! `SaveLocalMetadata` / `MetadataSavers`（含 "Nfo" 即开启 NFO 写回）/ `EnableRealtimeMonitor`。
//! VirtualFolders 输出回显 `SaveLocalMetadata` 与 `MetadataSavers`。
use axum::{
    body::Bytes,
    extract::State,
    http::{HeaderMap, StatusCode, Uri},
};
use serde::Deserialize;

use crate::{
    compat::emby::{dto::VirtualFolderInfoDto, payload::parse_emby_body},
    db::DbPool,
    error::AppError,
    state::AppState,
};

use super::access::authenticate_request_user;

#[derive(Debug, Default, Deserialize)]
#[serde(rename_all = "PascalCase", default)]
pub struct UpdateLibraryOptionsDto {
    pub id: Option<String>,
    pub library_options: Option<LibraryOptionsPatch>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(rename_all = "PascalCase", default)]
pub struct LibraryOptionsPatch {
    pub save_local_metadata: Option<bool>,
    pub metadata_savers: Option<Vec<String>>,
    pub enable_realtime_monitor: Option<bool>,
}

/// NFO 保存器开关：显式 MetadataSavers 优先（含 "Nfo"），否则看 SaveLocalMetadata。
pub fn nfo_saver_flag(patch: &LibraryOptionsPatch) -> Option<bool> {
    match &patch.metadata_savers {
        Some(savers) => Some(savers.iter().any(|s| s.trim().eq_ignore_ascii_case("nfo"))),
        None => patch.save_local_metadata,
    }
}

pub async fn update_library_options(
    State(state): State<AppState>,
    headers: HeaderMap,
    uri: Uri,
    body: Bytes,
) -> Result<StatusCode, AppError> {
    let user = authenticate_request_user(&state, &headers, &uri).await?;
    if !user.can_manage_server() {
        return Err(AppError::forbidden("server management permission required"));
    }
    let dto: UpdateLibraryOptionsDto = parse_emby_body(&headers, &body)?;
    let id = dto.id.map(|v| v.trim().to_owned()).filter(|v| !v.is_empty()).ok_or_else(|| AppError::unprocessable("Id is required"))?;
    let patch = dto.library_options.unwrap_or_default();
    let Some(pool) = state.database() else {
        return Err(AppError::internal("database is not configured"));
    };
    let updated = sqlx::query(
        "update libraries set save_local_metadata = coalesce($2, save_local_metadata),
                enable_realtime_monitor = coalesce($3, enable_realtime_monitor), updated_at = now()
         where replace(public_id::text, '-', '') = replace(lower($1), '-', '')",
    )
    .bind(&id)
    .bind(nfo_saver_flag(&patch))
    .bind(patch.enable_realtime_monitor)
    .execute(pool)
    .await
    .map_err(|err| AppError::internal(format!("failed to update library options: {err}")))?
    .rows_affected();
    if updated == 0 {
        return Err(AppError::not_found("library not found"));
    }
    Ok(StatusCode::NO_CONTENT)
}

/// VirtualFolders 回显保存器开关。
pub async fn attach_saver_flags(pool: Option<&DbPool>, folders: &mut [VirtualFolderInfoDto]) {
    let Some(pool) = pool else { return };
    let ids: Vec<String> = folders.iter().map(|f| f.item_id.replace('-', "").to_ascii_lowercase()).collect();
    let rows: Vec<(String, bool)> = sqlx::query_as("select replace(public_id::text, '-', ''), save_local_metadata from libraries where replace(public_id::text, '-', '') = any($1)")
        .bind(&ids)
        .fetch_all(pool)
        .await
        .unwrap_or_default();
    for folder in folders.iter_mut() {
        let key = folder.item_id.replace('-', "").to_ascii_lowercase();
        if let Some((_, on)) = rows.iter().find(|(id, _)| *id == key) {
            folder.library_options.save_local_metadata = *on;
            folder.library_options.metadata_savers = if *on { vec!["Nfo".to_owned()] } else { Vec::new() };
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn nfo_flag_prefers_metadata_savers() {
        let p = |savers: Option<Vec<&str>>, local: Option<bool>| LibraryOptionsPatch {
            save_local_metadata: local,
            metadata_savers: savers.map(|v| v.into_iter().map(str::to_owned).collect()),
            enable_realtime_monitor: None,
        };
        assert_eq!(nfo_saver_flag(&p(Some(vec!["Nfo"]), Some(false))), Some(true));
        assert_eq!(nfo_saver_flag(&p(Some(vec![]), Some(true))), Some(false));
        assert_eq!(nfo_saver_flag(&p(None, Some(true))), Some(true));
        assert_eq!(nfo_saver_flag(&p(None, None)), None);
    }
}
