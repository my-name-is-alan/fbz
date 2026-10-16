//! 库级刮削源链 admin API（§6.4 / §10 P0-D2，`/api/admin/libraries/{id}/metadata/sources`）。
//!
//! - `GET`  读当前链（无行则返回空数组；创建库时已预填默认）。
//! - `PUT`  全量替换开关/优先级/Provider 引用（配置面；不接 UI）。
//!
//! 每个 handler 走 `authenticate_admin`。本地 NFO 进 Match 仅回显
//! [`crate::library::metadata_sources::NFO_MATCH_INTEGRATION`]。

use axum::{
    Json,
    extract::{Path, State},
    http::{HeaderMap, Uri},
};
use serde::{Deserialize, Serialize};

use crate::{
    admin::access::authenticate_admin,
    error::AppError,
    library::metadata_sources::{
        LibraryMetadataSourceInput, LibraryMetadataSourceRecord, LibraryMetadataSourcesRepository,
        NFO_MATCH_INTEGRATION, validate_source_chain, validate_source_input,
    },
    state::AppState,
};

#[derive(Clone, Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct LibraryMetadataSourceDto {
    pub id: String,
    pub kind: String,
    pub key: String,
    /// Provider 引用时与 `key` 相同；NFO 槽为 null（避免把 NFO 误当 provider id）。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub provider_id: Option<String>,
    pub enabled: bool,
    pub priority: i32,
}

impl From<LibraryMetadataSourceRecord> for LibraryMetadataSourceDto {
    fn from(r: LibraryMetadataSourceRecord) -> Self {
        let provider_id = if r.source_kind == "provider" {
            Some(r.source_key.clone())
        } else {
            None
        };
        Self {
            id: r.id,
            kind: r.source_kind,
            key: r.source_key,
            provider_id,
            enabled: r.enabled,
            priority: r.priority,
        }
    }
}

#[derive(Clone, Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct LibraryMetadataSourcesResponseDto {
    pub library_id: String,
    pub sources: Vec<LibraryMetadataSourceDto>,
    /// `"local"`：本地 sidecar NFO 已接入 Match；云盘路径跳过（见 storage importer）。
    pub nfo_match_integration: &'static str,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct LibraryMetadataSourceWriteDto {
    pub kind: String,
    pub key: String,
    pub enabled: bool,
    pub priority: i32,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ReplaceLibraryMetadataSourcesRequestDto {
    pub sources: Vec<LibraryMetadataSourceWriteDto>,
}

/// `GET /api/admin/libraries/{library_id}/metadata/sources`
pub async fn get_library_metadata_sources(
    State(state): State<AppState>,
    Path(library_id): Path<String>,
    headers: HeaderMap,
    uri: Uri,
) -> Result<Json<LibraryMetadataSourcesResponseDto>, AppError> {
    authenticate_admin(&state, &headers, &uri).await?;
    let library_id = validate_library_public_id(&library_id)?;
    let database = require_database(&state)?;

    let Some(records) = LibraryMetadataSourcesRepository::new(database.clone())
        .list_for_library(library_id)
        .await
        .map_err(|err| {
            AppError::internal(format!("failed to list library metadata sources: {err}"))
        })?
    else {
        return Err(AppError::not_found("library not found"));
    };

    Ok(Json(LibraryMetadataSourcesResponseDto {
        library_id: library_id.to_owned(),
        sources: records
            .into_iter()
            .map(LibraryMetadataSourceDto::from)
            .collect(),
        nfo_match_integration: NFO_MATCH_INTEGRATION,
    }))
}

/// `PUT /api/admin/libraries/{library_id}/metadata/sources` — 全量替换源链。
pub async fn replace_library_metadata_sources(
    State(state): State<AppState>,
    Path(library_id): Path<String>,
    headers: HeaderMap,
    uri: Uri,
    Json(payload): Json<ReplaceLibraryMetadataSourcesRequestDto>,
) -> Result<Json<LibraryMetadataSourcesResponseDto>, AppError> {
    authenticate_admin(&state, &headers, &uri).await?;
    let library_id = validate_library_public_id(&library_id)?;
    let database = require_database(&state)?;

    if payload.sources.len() > 64 {
        return Err(AppError::unprocessable(
            "source chain may contain at most 64 entries",
        ));
    }

    let mut entries: Vec<LibraryMetadataSourceInput> = Vec::with_capacity(payload.sources.len());
    for item in &payload.sources {
        let entry = validate_source_input(&item.kind, &item.key, item.enabled, item.priority)
            .map_err(|err| AppError::unprocessable(err.to_string()))?;
        entries.push(entry);
    }
    validate_source_chain(&entries).map_err(|err| AppError::unprocessable(err.to_string()))?;

    let Some(records) = LibraryMetadataSourcesRepository::new(database.clone())
        .replace_for_library(library_id, &entries)
        .await
        .map_err(|err| {
            AppError::internal(format!("failed to replace library metadata sources: {err}"))
        })?
    else {
        return Err(AppError::not_found("library not found"));
    };

    Ok(Json(LibraryMetadataSourcesResponseDto {
        library_id: library_id.to_owned(),
        sources: records
            .into_iter()
            .map(LibraryMetadataSourceDto::from)
            .collect(),
        nfo_match_integration: NFO_MATCH_INTEGRATION,
    }))
}

fn require_database(state: &AppState) -> Result<&crate::db::DbPool, AppError> {
    state
        .database()
        .ok_or_else(|| AppError::internal("database is not configured"))
}

fn validate_library_public_id(raw: &str) -> Result<&str, AppError> {
    let trimmed = raw.trim();
    let ok = trimmed.len() == 36
        && trimmed.as_bytes().iter().enumerate().all(|(i, b)| match i {
            8 | 13 | 18 | 23 => *b == b'-',
            _ => b.is_ascii_hexdigit(),
        });
    if ok {
        Ok(trimmed)
    } else {
        Err(AppError::unprocessable("libraryId must be a uuid"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dto_maps_provider_id_only_for_provider_kind() {
        let nfo = LibraryMetadataSourceDto::from(LibraryMetadataSourceRecord {
            id: "a".into(),
            library_id: "lib".into(),
            source_kind: "nfo".into(),
            source_key: "nfo".into(),
            enabled: true,
            priority: 10,
        });
        assert!(nfo.provider_id.is_none());

        let provider = LibraryMetadataSourceDto::from(LibraryMetadataSourceRecord {
            id: "b".into(),
            library_id: "lib".into(),
            source_kind: "provider".into(),
            source_key: "tmdb".into(),
            enabled: true,
            priority: 20,
        });
        assert_eq!(provider.provider_id.as_deref(), Some("tmdb"));
    }

    #[test]
    fn library_public_id_validation() {
        assert!(validate_library_public_id("550e8400-e29b-41d4-a716-446655440000").is_ok());
        assert!(validate_library_public_id("not-a-uuid").is_err());
    }
}
