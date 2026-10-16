use std::path::{Path, PathBuf};

use axum::{
    Json,
    body::Bytes,
    extract::{Path as AxumPath, Query, State},
    http::{HeaderMap, StatusCode, Uri},
};
use serde::Deserialize;

use crate::{
    auth::service::AuthenticatedUser,
    compat::emby::dto::{LyricDto, LyricLineDto, LyricMetadataDto, RemoteLyricInfoDto},
    error::AppError,
    library::repository::LibraryRepository,
    media::repository::{MediaRepository, PlaybackMediaSourceRecord},
    state::AppState,
};

use super::access::authenticate_request_user;

const MAX_LYRICS_SIDECAR_BYTES: u64 = 1024 * 1024;

#[derive(Clone, Debug, Default, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "PascalCase")]
pub struct LyricsQuery {
    #[serde(alias = "userId", alias = "user_id")]
    pub user_id: Option<String>,
}

#[derive(Clone, Debug, Default, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "PascalCase")]
pub struct RemoteLyricsSearchQuery {
    #[serde(alias = "userId", alias = "user_id")]
    pub user_id: Option<String>,
    #[serde(alias = "mediaSourceId", alias = "media_source_id")]
    pub media_source_id: Option<String>,
    #[serde(alias = "providerName", alias = "provider_name")]
    pub provider_name: Option<String>,
    #[serde(alias = "searchTerm", alias = "search_term")]
    pub search_term: Option<String>,
}

pub async fn item_lyrics(
    State(state): State<AppState>,
    AxumPath(item_id): AxumPath<String>,
    Query(query): Query<LyricsQuery>,
    headers: HeaderMap,
    uri: Uri,
) -> Result<Json<LyricDto>, AppError> {
    let user = ensure_lyrics_item_visible(&state, &item_id, &query, &headers, &uri).await?;

    let Some(database) = state.database() else {
        return Err(AppError::internal("database is not configured"));
    };

    let Some(source) = MediaRepository::new(database.clone())
        .find_playback_media_source(user.id, &item_id, None)
        .await
        .map_err(|err| AppError::internal(format!("failed to get lyrics source: {err}")))?
    else {
        return Ok(Json(LyricDto::empty()));
    };

    Ok(Json(
        load_sidecar_lyrics(&source)
            .await?
            .unwrap_or_else(LyricDto::empty),
    ))
}

pub async fn remote_lyrics_search(
    State(state): State<AppState>,
    AxumPath(item_id): AxumPath<String>,
    Query(query): Query<RemoteLyricsSearchQuery>,
    headers: HeaderMap,
    uri: Uri,
) -> Result<Json<Vec<RemoteLyricInfoDto>>, AppError> {
    let query = remote_lyrics_search_input(query)?;
    let user = ensure_lyrics_item_visible(
        &state,
        &item_id,
        &LyricsQuery {
            user_id: query.user_id.clone(),
        },
        &headers,
        &uri,
    )
    .await?;

    // 插件优先：已启用 `lyrics.provider.query` 订阅者（如 fbz.lyrics.lrclib）。
    if let Some(found) = super::remote_lyrics::search(&state, &item_id, query.search_term.as_deref()).await? {
        let wanted = query.provider_name.as_deref();
        return Ok(Json(
            found
                .into_iter()
                .filter(|l| wanted.is_none_or(|w| l.provider_name.as_deref().is_some_and(|p| p.eq_ignore_ascii_case(w)) || l.id.starts_with(w)))
                .map(provider_lyric_to_remote_info)
                .collect(),
        ));
    }

    // 无歌词插件：回退 core 内置 LRCLIB。Provider 过滤：显式指定且不是 LrcLib → 空。
    if let Some(provider) = query.provider_name.as_deref()
        && !provider.eq_ignore_ascii_case("lrclib")
    {
        return Ok(Json(Vec::new()));
    }
    let Some(database) = state.database() else {
        return Err(AppError::internal("database is not configured"));
    };

    // 曲目上下文（标题/艺人/专辑）：用于 LRCLIB 结构化搜索与结果元数据规范化。
    let visible = LibraryRepository::new(database.clone())
        .find_user_item_by_id(user.id, &item_id)
        .await
        .map_err(|err| AppError::internal(format!("failed to get lyrics item: {err}")))?;
    if visible.is_none() {
        return Ok(Json(Vec::new()));
    }
    let ctx = super::remote_lyrics::track_context(database, &item_id).await?;
    let known = LrclibContext {
        title: ctx.get("title").and_then(|v| v.as_str()).map(str::to_owned),
        artist: ctx.get("artist").and_then(|v| v.as_str()).map(str::to_owned),
    };
    // 搜索词：客户端 SearchTerm 优先，缺省用曲目标题。
    let search_term = query.search_term.clone().or_else(|| known.title.clone());
    let Some(search_term) = search_term.filter(|term| !term.trim().is_empty()) else {
        return Ok(Json(Vec::new()));
    };

    Ok(Json(search_lrclib_lyrics(&search_term, &known).await?))
}

/// LRCLIB 公共搜索（无需 API key）。失败降级为空列表（歌词是增值功能，
/// 外部服务抖动不该打断客户端）。
async fn search_lrclib_lyrics(search_term: &str, known: &LrclibContext) -> Result<Vec<RemoteLyricInfoDto>, AppError> {
    #[derive(Deserialize)]
    struct LrclibRecord {
        id: i64,
        #[serde(rename = "trackName")]
        track_name: Option<String>,
        #[serde(rename = "artistName")]
        artist_name: Option<String>,
        #[serde(rename = "albumName")]
        album_name: Option<String>,
        #[serde(rename = "syncedLyrics")]
        synced_lyrics: Option<String>,
        #[serde(rename = "plainLyrics")]
        plain_lyrics: Option<String>,
    }

    let client = match reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(15))
        .user_agent("FBZ/0.1 (https://github.com/fbz)")
        .build()
    {
        Ok(client) => client,
        Err(_) => return Ok(Vec::new()),
    };
    // 已知艺人且用默认搜索词 → 结构化 track_name+artist_name（JF LrcLib 同法），否则 q。
    let mut request = client.get("https://lrclib.net/api/search");
    request = match known.artist.as_deref() {
        Some(artist) if known.title.as_deref() == Some(search_term) => {
            request.query(&[("track_name", search_term), ("artist_name", artist)])
        }
        _ => request.query(&[("q", search_term)]),
    };
    let response = match request.send().await {
        Ok(response) if response.status().is_success() => response,
        _ => return Ok(Vec::new()),
    };
    let records: Vec<LrclibRecord> = match response.json().await {
        Ok(records) => records,
        Err(_) => return Ok(Vec::new()),
    };

    Ok(records
        .into_iter()
        .take(10)
        .filter_map(|record| {
            let mut dto = if let Some(synced) = record
                .synced_lyrics
                .as_deref()
                .filter(|value| !value.trim().is_empty())
            {
                parse_lrc_lyrics(synced)
            } else if let Some(plain) = record
                .plain_lyrics
                .as_deref()
                .filter(|value| !value.trim().is_empty())
            {
                parse_plain_text_lyrics(plain)
            } else {
                return None;
            };
            let (title, artist, album) =
                normalize_lrclib_metadata(record.track_name, record.artist_name, record.album_name, known);
            if dto.metadata.title.is_none() {
                dto.metadata.title = title;
            }
            if dto.metadata.artist.is_none() {
                dto.metadata.artist = artist;
            }
            if dto.metadata.album.is_none() {
                dto.metadata.album = album;
            }

            Some(RemoteLyricInfoDto {
                id: format!("lrclib-{}", record.id),
                provider_name: "LrcLib".to_owned(),
                lyrics: Some(dto),
            })
        })
        .collect())
}

/// 曲目已知信息（规范化 LRCLIB 记录用）。
#[derive(Clone, Debug, Default)]
pub struct LrclibContext {
    pub title: Option<String>,
    pub artist: Option<String>,
}

/// LRCLIB 用户上传的记录常把三栏都填成「歌名 - 歌手」。规范化：
/// - 三栏（非空者）相同且含 ` - ` → 拆成标题/艺人（按已知艺人判断顺序，未知时取「标题 - 艺人」），专辑置空；
/// - 艺人栏等于标题栏 → 艺人取已知艺人（无则置空）；专辑等于标题 → 置空。
pub fn normalize_lrclib_metadata(
    track: Option<String>,
    artist: Option<String>,
    album: Option<String>,
    known: &LrclibContext,
) -> (Option<String>, Option<String>, Option<String>) {
    let clean = |v: Option<String>| v.map(|s| s.trim().to_owned()).filter(|s| !s.is_empty());
    let (mut track, mut artist, mut album) = (clean(track), clean(artist), clean(album));
    let same = |a: &Option<String>, b: &Option<String>| match (a, b) {
        (Some(a), Some(b)) => a.eq_ignore_ascii_case(b),
        _ => false,
    };
    let eq = |a: &str, b: &str| a.trim().eq_ignore_ascii_case(b.trim());
    if let Some(combined) = track.clone().filter(|t| t.contains(" - ") && (same(&track, &artist) || artist.is_none())) {
        if let Some((left, right)) = combined.split_once(" - ") {
            let (left, right) = (left.trim().to_owned(), right.trim().to_owned());
            let artist_first = known.artist.as_deref().is_some_and(|a| eq(a, &left))
                || known.title.as_deref().is_some_and(|t| eq(t, &right));
            let (t, a) = if artist_first { (right, left) } else { (left, right) };
            if same(&album, &Some(combined.clone())) {
                album = None;
            }
            track = Some(t);
            artist = Some(a);
        }
    }
    if same(&artist, &track) {
        artist = known.artist.clone();
    }
    if same(&album, &track) {
        album = None;
    }
    (track, artist, album)
}

fn provider_lyric_to_dto(lyric: &super::remote_lyrics::ProviderLyric) -> LyricDto {
    let mut dto = match lyric.text() {
        Some((text, "lrc")) => parse_lrc_lyrics(text),
        Some((text, _)) => parse_plain_text_lyrics(text),
        None => LyricDto::empty(),
    };
    if dto.metadata.title.is_none() {
        dto.metadata.title = lyric.track_name.clone();
    }
    if dto.metadata.artist.is_none() {
        dto.metadata.artist = lyric.artist_name.clone();
    }
    if dto.metadata.album.is_none() {
        dto.metadata.album = lyric.album_name.clone();
    }
    dto
}

fn provider_lyric_to_remote_info(lyric: super::remote_lyrics::ProviderLyric) -> RemoteLyricInfoDto {
    let lyrics = Some(provider_lyric_to_dto(&lyric));
    RemoteLyricInfoDto {
        id: lyric.id,
        provider_name: lyric.provider_name.unwrap_or_default(),
        lyrics,
    }
}

/// 按 LyricId 取歌词原文：插件 id（`{pluginId}~x`）走插件；`lrclib-{n}` 走内置 LRCLIB `/api/get/{n}`。
async fn fetch_remote_lyric(state: &AppState, lyric_id: &str) -> Result<super::remote_lyrics::ProviderLyric, AppError> {
    if super::remote_lyrics::split_lyric_id(lyric_id).is_some() {
        return super::remote_lyrics::fetch(state, lyric_id).await;
    }
    let Some(n) = lyric_id.strip_prefix("lrclib-").filter(|n| !n.is_empty() && n.chars().all(|c| c.is_ascii_digit())) else {
        return Err(AppError::unprocessable("LyricId is invalid"));
    };
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(15))
        .user_agent("FBZ/0.1 (https://github.com/fbz)")
        .build()
        .map_err(|err| AppError::internal(err.to_string()))?;
    let response = client
        .get(format!("https://lrclib.net/api/get/{n}"))
        .send()
        .await
        .map_err(|err| AppError::conflict(format!("lrclib request failed: {err}")))?;
    if !response.status().is_success() {
        return Err(AppError::not_found("lyrics not found"));
    }
    let mut lyric: super::remote_lyrics::ProviderLyric = response.json().await.map_err(|err| AppError::conflict(format!("lrclib response invalid: {err}")))?;
    lyric.id = lyric_id.to_owned();
    lyric.provider_name = Some("LrcLib".to_owned());
    let (title, artist, album) = normalize_lrclib_metadata(
        lyric.track_name.take(),
        lyric.artist_name.take(),
        lyric.album_name.take(),
        &LrclibContext::default(),
    );
    (lyric.track_name, lyric.artist_name, lyric.album_name) = (title, artist, album);
    if lyric.text().is_none() {
        return Err(AppError::not_found("lyrics not found"));
    }
    Ok(lyric)
}

/// `GET /Providers/Lyrics/{lyricId}`（JF）：取远程歌词但不保存。
pub async fn remote_lyric_by_id(
    State(state): State<AppState>,
    AxumPath(lyric_id): AxumPath<String>,
    headers: HeaderMap,
    uri: Uri,
) -> Result<Json<LyricDto>, AppError> {
    authenticate_request_user(&state, &headers, &uri).await?;
    let lyric = fetch_remote_lyric(&state, lyric_id.trim()).await?;
    Ok(Json(provider_lyric_to_dto(&lyric)))
}

/// `POST /Audio/{id}/RemoteSearch/Lyrics/{lyricId}`（JF）：下载远程歌词并写 sidecar（同步 → .lrc，纯文本 → .txt），返回解析结果。
pub async fn download_remote_lyrics(
    State(state): State<AppState>,
    AxumPath((item_id, lyric_id)): AxumPath<(String, String)>,
    headers: HeaderMap,
    uri: Uri,
) -> Result<Json<LyricDto>, AppError> {
    let source = writable_lyrics_source(&state, &item_id, &headers, &uri).await?;
    let lyric = fetch_remote_lyric(&state, lyric_id.trim()).await?;
    let Some((text, extension)) = lyric.text() else {
        return Err(AppError::not_found("lyrics not found"));
    };
    for candidate in lyric_sidecar_candidates(Path::new(source.path.trim())) {
        let _ = tokio::fs::remove_file(&candidate).await;
    }
    let mut target = PathBuf::from(source.path.trim());
    target.set_extension(extension);
    tokio::fs::write(&target, text.as_bytes())
        .await
        .map_err(|err| AppError::internal(format!("failed to write lyrics sidecar: {err}")))?;
    Ok(Json(provider_lyric_to_dto(&lyric)))
}

#[derive(Clone, Debug, Default, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "PascalCase")]
pub struct UploadLyricsQuery {
    #[serde(default, alias = "fileName", alias = "filename")]
    pub file_name: Option<String>,
}

/// 上传目标扩展名：仅 lrc / elrc / txt，默认 lrc。
fn upload_extension(file_name: Option<&str>) -> &'static str {
    let ext = file_name
        .and_then(|n| n.rsplit_once('.').map(|(_, e)| e.trim().to_ascii_lowercase()))
        .unwrap_or_default();
    match ext.as_str() {
        "txt" => "txt",
        "elrc" => "elrc",
        _ => "lrc",
    }
}

async fn writable_lyrics_source(
    state: &AppState,
    item_id: &str,
    headers: &HeaderMap,
    uri: &Uri,
) -> Result<PlaybackMediaSourceRecord, AppError> {
    let user = ensure_lyrics_item_visible(state, item_id, &LyricsQuery::default(), headers, uri).await?;
    if !user.can_manage_server() {
        return Err(AppError::forbidden("lyric management permission required"));
    }
    let Some(database) = state.database() else {
        return Err(AppError::internal("database is not configured"));
    };
    let source = MediaRepository::new(database.clone())
        .find_playback_media_source(user.id, item_id, None)
        .await
        .map_err(|err| AppError::internal(format!("failed to get lyrics source: {err}")))?
        .ok_or_else(|| AppError::not_found("item not found"))?;
    if source.is_strm {
        return Err(AppError::unprocessable("lyrics sidecar is not supported for STRM items"));
    }
    Ok(source)
}

/// `POST /Audio/{id}/Lyrics?fileName=` — 正文为歌词文本，写入同目录 sidecar 并返回解析结果（JF 10.9）。
pub async fn upload_lyrics(
    State(state): State<AppState>,
    AxumPath(item_id): AxumPath<String>,
    Query(query): Query<UploadLyricsQuery>,
    headers: HeaderMap,
    uri: Uri,
    body: Bytes,
) -> Result<Json<LyricDto>, AppError> {
    if body.is_empty() {
        return Err(AppError::unprocessable("lyrics body is empty"));
    }
    if body.len() as u64 > MAX_LYRICS_SIDECAR_BYTES {
        return Err(AppError::unprocessable("lyrics body is too large"));
    }
    let source = writable_lyrics_source(&state, &item_id, &headers, &uri).await?;
    let extension = upload_extension(query.file_name.as_deref());
    let contents = String::from_utf8_lossy(&body).into_owned();
    // 先清掉其它扩展名的旧 sidecar，避免读取优先级让新上传不生效。
    for candidate in lyric_sidecar_candidates(Path::new(source.path.trim())) {
        let _ = tokio::fs::remove_file(&candidate).await;
    }
    let mut target = PathBuf::from(source.path.trim());
    target.set_extension(extension);
    tokio::fs::write(&target, contents.as_bytes())
        .await
        .map_err(|err| AppError::internal(format!("failed to write lyrics sidecar: {err}")))?;
    Ok(Json(parse_lyrics_sidecar(&contents, extension)))
}

/// `DELETE /Audio/{id}/Lyrics` — 删除 sidecar 歌词，204。
pub async fn delete_lyrics(
    State(state): State<AppState>,
    AxumPath(item_id): AxumPath<String>,
    headers: HeaderMap,
    uri: Uri,
) -> Result<StatusCode, AppError> {
    let source = writable_lyrics_source(&state, &item_id, &headers, &uri).await?;
    let mut removed = false;
    for candidate in lyric_sidecar_candidates(Path::new(source.path.trim())) {
        if tokio::fs::remove_file(&candidate).await.is_ok() {
            removed = true;
        }
    }
    if removed { Ok(StatusCode::NO_CONTENT) } else { Err(AppError::not_found("lyrics not found")) }
}

async fn ensure_lyrics_item_visible(
    state: &AppState,
    item_id: &str,
    query: &LyricsQuery,
    headers: &HeaderMap,
    uri: &Uri,
) -> Result<AuthenticatedUser, AppError> {
    let user = authenticate_request_user(state, headers, uri).await?;
    if let Some(query_user_id) = query.user_id.as_deref()
        && query_user_id != user.public_id
    {
        return Err(AppError::forbidden(
            "authenticated user does not match query user",
        ));
    }

    let Some(database) = state.database() else {
        return Err(AppError::internal("database is not configured"));
    };

    let exists = LibraryRepository::new(database.clone())
        .find_user_item_by_id(user.id, item_id)
        .await
        .map_err(|err| AppError::internal(format!("failed to get lyrics item: {err}")))?
        .is_some();
    if !exists {
        return Err(AppError::not_found("item not found"));
    }

    Ok(user)
}

fn remote_lyrics_search_input(
    query: RemoteLyricsSearchQuery,
) -> Result<RemoteLyricsSearchQuery, AppError> {
    Ok(RemoteLyricsSearchQuery {
        user_id: normalize_optional_query_value(query.user_id, "UserId", 256)?,
        media_source_id: normalize_optional_query_value(
            query.media_source_id,
            "MediaSourceId",
            256,
        )?,
        provider_name: normalize_optional_query_value(query.provider_name, "ProviderName", 128)?,
        search_term: normalize_optional_query_value(query.search_term, "SearchTerm", 512)?,
    })
}

fn normalize_optional_query_value(
    value: Option<String>,
    name: &'static str,
    max_len: usize,
) -> Result<Option<String>, AppError> {
    let Some(value) = value else {
        return Ok(None);
    };
    let value = value.trim();
    if value.is_empty() {
        return Ok(None);
    }
    if value.len() > max_len {
        return Err(AppError::unprocessable(format!("{name} is too long")));
    }
    Ok(Some(value.to_owned()))
}

async fn load_sidecar_lyrics(
    source: &PlaybackMediaSourceRecord,
) -> Result<Option<LyricDto>, AppError> {
    if source.is_strm {
        return Ok(None);
    }

    let media_path = Path::new(source.path.trim());
    for candidate in lyric_sidecar_candidates(media_path) {
        let metadata = match tokio::fs::metadata(&candidate).await {
            Ok(metadata) => metadata,
            Err(err) if err.kind() == std::io::ErrorKind::NotFound => continue,
            Err(err) => {
                return Err(AppError::internal(format!(
                    "failed to stat lyrics sidecar: {err}"
                )));
            }
        };
        if !metadata.is_file() {
            continue;
        }
        if metadata.len() > MAX_LYRICS_SIDECAR_BYTES {
            return Err(AppError::unprocessable("lyrics sidecar is too large"));
        }

        let bytes = tokio::fs::read(&candidate)
            .await
            .map_err(|err| AppError::internal(format!("failed to read lyrics sidecar: {err}")))?;
        let contents = String::from_utf8_lossy(&bytes);
        let extension = candidate
            .extension()
            .and_then(|value| value.to_str())
            .unwrap_or_default();

        return Ok(Some(parse_lyrics_sidecar(&contents, extension)));
    }

    Ok(None)
}

fn lyric_sidecar_candidates(media_path: &Path) -> Vec<PathBuf> {
    let Some(stem) = media_path.file_stem() else {
        return Vec::new();
    };
    let parent = media_path.parent().unwrap_or_else(|| Path::new(""));

    ["lrc", "elrc", "txt"]
        .into_iter()
        .map(|extension| {
            let mut candidate = parent.join(stem);
            candidate.set_extension(extension);
            candidate
        })
        .collect()
}

fn parse_lyrics_sidecar(contents: &str, extension: &str) -> LyricDto {
    match extension
        .trim()
        .trim_start_matches('.')
        .to_ascii_lowercase()
        .as_str()
    {
        "lrc" | "elrc" => parse_lrc_lyrics(contents),
        "txt" => parse_plain_text_lyrics(contents),
        _ => LyricDto::empty(),
    }
}

fn parse_plain_text_lyrics(contents: &str) -> LyricDto {
    LyricDto {
        metadata: LyricMetadataDto {
            is_synced: false,
            ..LyricMetadataDto::default()
        },
        lyrics: contents
            .lines()
            .map(str::trim)
            .filter(|line| !line.is_empty())
            .map(|line| LyricLineDto {
                text: line.to_owned(),
                start: None,
                cues: None,
            })
            .collect(),
    }
}

fn parse_lrc_lyrics(contents: &str) -> LyricDto {
    let mut metadata = LyricMetadataDto::default();
    let mut lyrics = Vec::new();
    let mut unsynced_lines = Vec::new();

    for raw_line in contents.lines() {
        let line = raw_line.trim().trim_start_matches('\u{feff}');
        if line.is_empty() {
            continue;
        }

        if let Some(tag) = line
            .strip_prefix('[')
            .and_then(|value| value.strip_suffix(']'))
            && !tag.contains("][")
            && let Some((key, value)) = tag.split_once(':')
            && parse_lrc_timestamp(tag).is_none()
        {
            apply_lrc_metadata(&mut metadata, key, value);
            continue;
        }

        let mut rest = line;
        let mut starts = Vec::new();
        while let Some(after_open) = rest.strip_prefix('[') {
            let Some((tag, after_close)) = after_open.split_once(']') else {
                break;
            };
            let Some(start) = parse_lrc_timestamp(tag) else {
                break;
            };
            starts.push(start);
            rest = after_close;
        }

        let text = rest.trim();
        if text.is_empty() {
            continue;
        }

        if starts.is_empty() {
            unsynced_lines.push(LyricLineDto {
                text: text.to_owned(),
                start: None,
                cues: None,
            });
            continue;
        }

        for start in starts {
            lyrics.push(LyricLineDto {
                text: text.to_owned(),
                start: Some(start),
                cues: None,
            });
        }
    }

    lyrics.sort_by_key(|line| line.start.unwrap_or_default());
    metadata.is_synced = !lyrics.is_empty();
    if lyrics.is_empty() {
        lyrics = unsynced_lines;
    }

    LyricDto { metadata, lyrics }
}

fn apply_lrc_metadata(metadata: &mut LyricMetadataDto, key: &str, value: &str) {
    let value = value.trim();
    if value.is_empty() {
        return;
    }

    match key.trim().to_ascii_lowercase().as_str() {
        "ar" | "artist" => metadata.artist = Some(value.to_owned()),
        "ti" | "title" => metadata.title = Some(value.to_owned()),
        "al" | "album" => metadata.album = Some(value.to_owned()),
        "au" | "author" => metadata.author = Some(value.to_owned()),
        "by" => metadata.by = Some(value.to_owned()),
        "length" => metadata.length = parse_lrc_timestamp(value),
        "offset" => {
            metadata.offset = value
                .parse::<i64>()
                .ok()
                .map(|milliseconds| milliseconds * 10_000);
        }
        "re" | "creator" => metadata.creator = Some(value.to_owned()),
        "ve" | "version" => metadata.version = Some(value.to_owned()),
        _ => {}
    }
}

fn parse_lrc_timestamp(value: &str) -> Option<i64> {
    let parts = value.trim().split(':').collect::<Vec<_>>();
    let (hours, minutes, seconds) = match parts.as_slice() {
        [minutes, seconds] => (0_i64, parse_i64(minutes)?, *seconds),
        [hours, minutes, seconds] => (parse_i64(hours)?, parse_i64(minutes)?, *seconds),
        _ => return None,
    };

    let (whole_seconds, fraction) = seconds.split_once('.').unwrap_or((seconds, ""));
    let seconds = parse_i64(whole_seconds)?;
    if hours < 0 || minutes < 0 || seconds < 0 || !(0..60).contains(&seconds) {
        return None;
    }

    let total_seconds = hours
        .checked_mul(3600)?
        .checked_add(minutes.checked_mul(60)?)?
        .checked_add(seconds)?;
    let fraction_ticks = lrc_fraction_to_ticks(fraction)?;

    total_seconds
        .checked_mul(10_000_000)?
        .checked_add(fraction_ticks)
}

fn parse_i64(value: &str) -> Option<i64> {
    value.trim().parse::<i64>().ok()
}

fn lrc_fraction_to_ticks(value: &str) -> Option<i64> {
    if value.is_empty() {
        return Some(0);
    }
    if !value.chars().all(|ch| ch.is_ascii_digit()) {
        return None;
    }

    let mut fraction = value.chars().take(7).collect::<String>();
    while fraction.len() < 7 {
        fraction.push('0');
    }

    fraction.parse::<i64>().ok()
}

#[cfg(test)]
mod tests {
    use std::path::{Path, PathBuf};

    use axum::{extract::Query, http::Uri};
    use serde_json::json;

    use super::*;

    #[test]
    fn lyrics_query_accepts_common_emby_parameters() {
        let query = serde_json::from_value::<LyricsQuery>(json!({
            "UserId": "user-1"
        }))
        .unwrap();

        assert_eq!(query.user_id.as_deref(), Some("user-1"));
    }

    #[test]
    fn lrclib_metadata_is_normalized() {
        let s = |v: &str| Some(v.to_owned());
        let combined = s("Bohemian Rhapsody - Queen");
        let none = LrclibContext::default();
        assert_eq!(
            normalize_lrclib_metadata(combined.clone(), combined.clone(), combined.clone(), &none),
            (s("Bohemian Rhapsody"), s("Queen"), None)
        );
        let known = LrclibContext { title: s("Bohemian Rhapsody"), artist: s("Queen") };
        let reversed = s("Queen - Bohemian Rhapsody");
        assert_eq!(
            normalize_lrclib_metadata(reversed.clone(), reversed.clone(), reversed, &known),
            (s("Bohemian Rhapsody"), s("Queen"), None)
        );
        assert_eq!(
            normalize_lrclib_metadata(s("Song"), s("Song"), s("Song"), &known),
            (s("Song"), s("Queen"), None)
        );
        assert_eq!(
            normalize_lrclib_metadata(s("Song"), s("Artist"), s("Album"), &known),
            (s("Song"), s("Artist"), s("Album"))
        );
        assert_eq!(
            normalize_lrclib_metadata(s("AC - DC Song"), s("Real Artist"), None, &none),
            (s("AC - DC Song"), s("Real Artist"), None)
        );
    }

    #[test]
    fn remote_lyrics_search_query_accepts_common_parameters() {
        let query = serde_json::from_value::<RemoteLyricsSearchQuery>(json!({
            "UserId": "user-1",
            "MediaSourceId": "42",
            "ProviderName": "LrcLib",
            "SearchTerm": "Signal Alice"
        }))
        .unwrap();

        assert_eq!(query.user_id.as_deref(), Some("user-1"));
        assert_eq!(query.media_source_id.as_deref(), Some("42"));
        assert_eq!(query.provider_name.as_deref(), Some("LrcLib"));
        assert_eq!(query.search_term.as_deref(), Some("Signal Alice"));
    }

    #[test]
    fn lyrics_queries_accept_lower_camel_client_fields() {
        let uri: Uri = "/emby/Audio/item-1/Lyrics?userId=user-1".parse().unwrap();
        let Query(query) = Query::<LyricsQuery>::try_from_uri(&uri).unwrap();

        assert_eq!(query.user_id.as_deref(), Some("user-1"));

        let uri: Uri = concat!(
            "/emby/Audio/item-1/RemoteSearch/Lyrics?",
            "userId=user-1&mediaSourceId=42",
            "&providerName=LrcLib&searchTerm=Signal%20Alice"
        )
        .parse()
        .unwrap();
        let Query(query) = Query::<RemoteLyricsSearchQuery>::try_from_uri(&uri).unwrap();

        assert_eq!(query.user_id.as_deref(), Some("user-1"));
        assert_eq!(query.media_source_id.as_deref(), Some("42"));
        assert_eq!(query.provider_name.as_deref(), Some("LrcLib"));
        assert_eq!(query.search_term.as_deref(), Some("Signal Alice"));
    }

    #[test]
    fn lrc_parser_maps_metadata_and_timestamps_to_emby_ticks() {
        let dto = parse_lrc_lyrics(
            r#"[ar:Alice]
[ti:Signal]
[al:Night Drive]
[by:LRCGet]
[offset:250]
[00:01.50]First line
[00:03.00][00:04.25]Repeated line"#,
        );

        assert_eq!(dto.metadata.artist.as_deref(), Some("Alice"));
        assert_eq!(dto.metadata.title.as_deref(), Some("Signal"));
        assert_eq!(dto.metadata.album.as_deref(), Some("Night Drive"));
        assert_eq!(dto.metadata.by.as_deref(), Some("LRCGet"));
        assert_eq!(dto.metadata.offset, Some(2_500_000));
        assert!(dto.metadata.is_synced);
        assert_eq!(dto.lyrics.len(), 3);
        assert_eq!(dto.lyrics[0].text, "First line");
        assert_eq!(dto.lyrics[0].start, Some(15_000_000));
        assert_eq!(dto.lyrics[1].text, "Repeated line");
        assert_eq!(dto.lyrics[1].start, Some(30_000_000));
        assert_eq!(dto.lyrics[2].start, Some(42_500_000));
    }

    #[test]
    fn plain_text_parser_preserves_unsynced_lyric_lines() {
        let dto = parse_plain_text_lyrics("First line\r\n\r\nSecond line\n  Third line  ");

        assert!(!dto.metadata.is_synced);
        assert_eq!(dto.lyrics.len(), 3);
        assert_eq!(dto.lyrics[0].text, "First line");
        assert_eq!(dto.lyrics[0].start, None);
        assert_eq!(dto.lyrics[1].text, "Second line");
        assert_eq!(dto.lyrics[2].text, "Third line");
    }

    #[test]
    fn lrc_parser_keeps_unsynced_text_when_no_timestamps_exist() {
        let dto = parse_lrc_lyrics(
            r#"[ar:Alice]
First line
Second line"#,
        );

        assert_eq!(dto.metadata.artist.as_deref(), Some("Alice"));
        assert!(!dto.metadata.is_synced);
        assert_eq!(dto.lyrics.len(), 2);
        assert_eq!(dto.lyrics[0].text, "First line");
        assert_eq!(dto.lyrics[0].start, None);
        assert_eq!(dto.lyrics[1].text, "Second line");
    }

    #[test]
    fn lyric_sidecar_candidates_stay_next_to_media_file() {
        let candidates = lyric_sidecar_candidates(Path::new("D:/Music/Artist/Song.flac"));

        assert_eq!(
            candidates,
            vec![
                PathBuf::from("D:/Music/Artist/Song.lrc"),
                PathBuf::from("D:/Music/Artist/Song.elrc"),
                PathBuf::from("D:/Music/Artist/Song.txt"),
            ]
        );
    }
}

#[cfg(test)]
mod upload_tests {
    use super::upload_extension;

    #[test]
    fn upload_extension_defaults_to_lrc() {
        assert_eq!(upload_extension(None), "lrc");
        assert_eq!(upload_extension(Some("a.TXT")), "txt");
        assert_eq!(upload_extension(Some("a.elrc")), "elrc");
        assert_eq!(upload_extension(Some("../../etc/passwd")), "lrc");
    }
}
