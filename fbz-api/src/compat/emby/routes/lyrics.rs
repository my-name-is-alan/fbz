use std::path::{Path, PathBuf};

use axum::{
    Json,
    extract::{Path as AxumPath, Query, State},
    http::{HeaderMap, Uri},
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
    ensure_lyrics_item_visible(
        &state,
        &item_id,
        &LyricsQuery {
            user_id: query.user_id,
        },
        &headers,
        &uri,
    )
    .await?;

    Ok(Json(Vec::new()))
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
