//! 音乐浏览 BFF 路由：`/api/music/*`。
//!
//! 瘦控制器——只做鉴权、调 service、映射错误。编排与映射在 [`super::service`]，SQL 在
//! [`crate::library::repository`]。鉴权复用 navigation 的 [`authenticate_user`]（任意已登录用户）。

use axum::{
    Json, Router,
    extract::{Path, Query, State},
    http::{HeaderMap, Uri},
    routing::get,
};
use serde::Deserialize;

use crate::{
    error::AppError,
    music::dto::{AlbumDetailDto, ArtistDetailDto, ArtistListDto},
    navigation::access::authenticate_user,
    state::AppState,
};

use super::service::{album_detail, artist_detail, list_artists};

/// 挂载音乐浏览路由。
pub fn router() -> Router<AppState> {
    Router::new()
        .route("/api/music/artists", get(artists))
        .route("/api/music/artists/{id}", get(artist))
        .route("/api/music/albums/{id}", get(album))
}

/// `GET /api/music/artists?libraryId=`：列出某音乐库下的艺术家。
#[derive(Clone, Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ArtistsQuery {
    /// 音乐库对外标识（`public_id`）。
    library_id: String,
}

async fn artists(
    State(state): State<AppState>,
    headers: HeaderMap,
    uri: Uri,
    Query(query): Query<ArtistsQuery>,
) -> Result<Json<ArtistListDto>, AppError> {
    let database = require_database(&state)?;
    let user = authenticate_user(&state, &headers, &uri).await?;
    let result = list_artists(database, &user, query.library_id)
        .await
        .map_err(|err| AppError::internal(format!("failed to list artists: {err}")))?;
    Ok(Json(result))
}

/// `GET /api/music/artists/:id`：艺术家详情 + 名下专辑。
async fn artist(
    State(state): State<AppState>,
    headers: HeaderMap,
    uri: Uri,
    Path(id): Path<String>,
) -> Result<Json<ArtistDetailDto>, AppError> {
    let database = require_database(&state)?;
    let user = authenticate_user(&state, &headers, &uri).await?;
    artist_detail(database, &user, id)
        .await
        .map_err(|err| AppError::internal(format!("failed to load artist: {err}")))?
        .map(Json)
        .ok_or_else(|| AppError::not_found("artist not found"))
}

/// `GET /api/music/albums/:id`：专辑详情 + 内含曲目。
async fn album(
    State(state): State<AppState>,
    headers: HeaderMap,
    uri: Uri,
    Path(id): Path<String>,
) -> Result<Json<AlbumDetailDto>, AppError> {
    let database = require_database(&state)?;
    let user = authenticate_user(&state, &headers, &uri).await?;
    album_detail(database, &user, id)
        .await
        .map_err(|err| AppError::internal(format!("failed to load album: {err}")))?
        .map(Json)
        .ok_or_else(|| AppError::not_found("album not found"))
}

fn require_database(state: &AppState) -> Result<crate::db::DbPool, AppError> {
    state
        .database()
        .cloned()
        .ok_or_else(|| AppError::internal("database is not configured"))
}
