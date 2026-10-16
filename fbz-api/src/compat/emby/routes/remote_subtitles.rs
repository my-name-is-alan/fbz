//! 远程字幕 provider 接线（插件优先）：`subtitle.provider.query` 同步能力。
//!
//! - 搜索：`{action:"search", itemId, language, title, year, itemType, seriesName, season, episode,
//!   imdbId, tmdbId, fileName, isForced, isHearingImpaired, isPerfectMatch}` →
//!   插件回 `{results:[{id,name,format,language,author,comment,dateCreated,communityRating,
//!   downloadCount,isHashMatch,isForced,isHearingImpaired,providerName}]}`。
//! - 下载：`{action:"download", subtitleId, itemId}` → `{format, language, dataBase64, isForced, isHearingImpaired}`；
//!   core 写 sidecar 并入队 probe（与上传同一路径）。
//!
//! 对外 SubtitleId = `{pluginId}~{providerSubtitleId}`（插件侧 id 需 URL-safe、不含 `/`）。
//! 无已启用插件订阅时：搜索空列表、下载 409（与此前一致，不造假命中）。

use serde::Deserialize;
use serde_json::{Value, json};

use crate::{
    compat::emby::dto::RemoteSubtitleInfoDto,
    db::DbPool,
    error::AppError,
    plugins::invoke::{PluginSyncInvoker, SyncAuditMode},
    state::AppState,
};

pub const SUBTITLE_PROVIDER_HOOK: &str = "subtitle.provider.query";
const ID_SEPARATOR: char = '~';

fn unwrap_envelope(value: &Value) -> &Value {
    value
        .get("result")
        .filter(|_| value.get("ok").and_then(Value::as_bool) == Some(true))
        .unwrap_or(value)
}

#[derive(Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase", default)]
struct ResultDto {
    id: String,
    name: String,
    format: Option<String>,
    language: Option<String>,
    three_letter_iso_language_name: Option<String>,
    author: Option<String>,
    comment: Option<String>,
    date_created: Option<String>,
    community_rating: Option<f32>,
    download_count: Option<i64>,
    is_hash_match: bool,
    is_forced: bool,
    is_hearing_impaired: bool,
    provider_name: Option<String>,
}

/// 把插件搜索响应映射为 JF `RemoteSubtitleInfo`（丢弃空/含 `/` 的 id）。
pub fn map_search_response(plugin_id: &str, value: &Value, language: &str) -> Vec<RemoteSubtitleInfoDto> {
    let body = unwrap_envelope(value);
    let Some(items) = body.get("results").and_then(Value::as_array) else {
        return Vec::new();
    };
    items
        .iter()
        .filter_map(|raw| serde_json::from_value::<ResultDto>(raw.clone()).ok())
        .filter(|r| !r.id.trim().is_empty() && !r.id.contains(['/', '\\']))
        .map(|r| {
            let three = r.three_letter_iso_language_name.clone().unwrap_or_else(|| language.to_owned());
            RemoteSubtitleInfoDto {
                three_letter_iso_language_name: three.clone(),
                id: format!("{plugin_id}{ID_SEPARATOR}{}", r.id.trim()),
                provider_name: r.provider_name.unwrap_or_else(|| plugin_id.to_owned()),
                name: r.name,
                format: r.format.unwrap_or_else(|| "srt".to_owned()),
                author: r.author.unwrap_or_default(),
                comment: r.comment.unwrap_or_default(),
                date_created: r.date_created,
                community_rating: r.community_rating,
                download_count: r.download_count.unwrap_or(0).clamp(0, i32::MAX as i64) as i32,
                is_hash_match: r.is_hash_match,
                is_forced: r.is_forced,
                is_hearing_impaired: r.is_hearing_impaired,
                language: r.language.unwrap_or(three),
            }
        })
        .collect()
}

pub fn split_subtitle_id(id: &str) -> Option<(&str, &str)> {
    let (plugin, inner) = id.split_once(ID_SEPARATOR)?;
    (!plugin.is_empty() && !inner.is_empty()).then_some((plugin, inner))
}

#[derive(Debug, PartialEq, Eq)]
pub struct DownloadedSubtitle {
    pub format: String,
    pub language: String,
    pub data: Vec<u8>,
    pub is_forced: bool,
    pub is_hearing_impaired: bool,
}

pub fn map_download_response(value: &Value) -> Option<DownloadedSubtitle> {
    let body = unwrap_envelope(value);
    let data = body.get("dataBase64").and_then(Value::as_str)?;
    let data = super::subtitle_upload::decode_base64(data)?;
    if data.is_empty() {
        return None;
    }
    Some(DownloadedSubtitle {
        format: body.get("format").and_then(Value::as_str).unwrap_or("srt").to_owned(),
        language: body.get("language").and_then(Value::as_str).unwrap_or("und").to_owned(),
        data,
        is_forced: body.get("isForced").and_then(Value::as_bool).unwrap_or(false),
        is_hearing_impaired: body.get("isHearingImpaired").and_then(Value::as_bool).unwrap_or(false),
    })
}

/// 搜索上下文（标题/年份/外部 id/季集/文件名）。
async fn item_context(pool: &DbPool, item_id: &str) -> Result<Value, AppError> {
    use sqlx::Row;
    let row = sqlx::query(
        r#"
        select mi.item_type, mi.title, mi.original_title, mi.production_year,
               coalesce(mi.season_number, mi.parent_index_number) as season,
               coalesce(mi.episode_number, mi.index_number) as episode,
               series.title as series_name,
               (select mf.path from media_files mf where mf.media_item_id = mi.id order by mf.is_primary desc, mf.id limit 1) as path,
               (select e.external_id from media_external_ids e where e.media_item_id = coalesce(series.id, mi.id) and e.provider = 'imdb' limit 1) as series_imdb,
               (select e.external_id from media_external_ids e where e.media_item_id = mi.id and e.provider = 'imdb' limit 1) as imdb,
               (select e.external_id from media_external_ids e where e.media_item_id = mi.id and e.provider = 'tmdb' limit 1) as tmdb
        from media_items mi
        left join media_items season on season.id = mi.parent_id and mi.item_type = 'episode'
        left join media_items series on series.id = coalesce(season.parent_id, mi.parent_id) and series.item_type = 'series' and mi.item_type = 'episode'
        where mi.public_id::text = $1 and mi.is_deleted = false
        "#,
    )
    .bind(item_id)
    .fetch_optional(pool)
    .await
    .map_err(|err| AppError::internal(format!("failed to load item for subtitle search: {err}")))?
    .ok_or_else(|| AppError::not_found("item not found"))?;
    let path: Option<String> = row.try_get("path").ok().flatten();
    // OpenSubtitles moviehash：仅本地可读文件（strm/远端存储读不到 → 不给，插件退回文本搜索）。
    let hash = match path.clone() {
        Some(p) => tokio::task::spawn_blocking(move || opensubtitles_hash(std::path::Path::new(p.trim()))).await.ok().flatten(),
        None => None,
    };
    let file_name = path
        .as_deref()
        .and_then(|p| std::path::Path::new(p.trim()).file_name())
        .and_then(|n| n.to_str())
        .map(str::to_owned);
    Ok(json!({
        "itemType": row.try_get::<String, _>("item_type").ok(),
        "title": row.try_get::<String, _>("title").ok(),
        "originalTitle": row.try_get::<Option<String>, _>("original_title").ok().flatten(),
        "year": row.try_get::<Option<i32>, _>("production_year").ok().flatten(),
        "season": row.try_get::<Option<i32>, _>("season").ok().flatten(),
        "episode": row.try_get::<Option<i32>, _>("episode").ok().flatten(),
        "seriesName": row.try_get::<Option<String>, _>("series_name").ok().flatten(),
        "imdbId": row.try_get::<Option<String>, _>("imdb").ok().flatten().or_else(|| row.try_get::<Option<String>, _>("series_imdb").ok().flatten()),
        "tmdbId": row.try_get::<Option<String>, _>("tmdb").ok().flatten(),
        "fileName": file_name,
        "movieHash": hash.as_ref().map(|(h, _)| h.clone()),
        "movieByteSize": hash.as_ref().map(|(_, size)| *size),
    }))
}

/// OpenSubtitles moviehash：文件大小 + 首/尾各 64KiB 按 u64 小端求和（回绕），16 位十六进制。
/// 文件不足 64KiB 时不算（与官方实现一致，过小文件无意义）。
pub fn opensubtitles_hash(path: &std::path::Path) -> Option<(String, u64)> {
    use std::io::{Read, Seek, SeekFrom};
    const CHUNK: u64 = 65536;
    let mut file = std::fs::File::open(path).ok()?;
    let size = file.metadata().ok()?.len();
    if size < CHUNK {
        return None;
    }
    let mut hash = size;
    let mut buf = vec![0u8; CHUNK as usize];
    for offset in [0, size - CHUNK] {
        file.seek(SeekFrom::Start(offset)).ok()?;
        file.read_exact(&mut buf).ok()?;
        for word in buf.chunks_exact(8) {
            hash = hash.wrapping_add(u64::from_le_bytes(word.try_into().ok()?));
        }
    }
    Some((format!("{hash:016x}"), size))
}

const AD_MARKERS: [&str; 10] = [
    "opensubtitles",
    "osdb.link",
    "become vip member",
    "advertise your product",
    "watch online movies",
    "rate this subtitle",
    "help other users to choose the best subtitles",
    "subtitles by explosiveskull",
    "sync & corrections by",
    "subtitles downloaded from",
];

fn is_ad_line(line: &str) -> bool {
    let lower = line.to_lowercase();
    AD_MARKERS.iter().any(|m| lower.contains(m))
}

/// 去掉字幕源注入的广告：srt 丢弃含广告文本的整条 cue 并重编号；其它格式（vtt/ass/ssa…）丢弃广告行。
/// 非 UTF-8 文本原样返回（避免破坏编码）。
pub fn strip_subtitle_ads(data: &[u8], format: &str) -> Vec<u8> {
    let Ok(text) = std::str::from_utf8(data) else {
        return data.to_vec();
    };
    let (bom, text) = text.strip_prefix('\u{feff}').map_or(("", text), |t| ("\u{feff}", t));
    let newline = if text.contains("\r\n") { "\r\n" } else { "\n" };
    let normalized = text.replace("\r\n", "\n");
    if !format.eq_ignore_ascii_case("srt") {
        let kept: Vec<&str> = normalized.lines().filter(|l| !is_ad_line(l)).collect();
        return format!("{bom}{}{newline}", kept.join(newline)).into_bytes();
    }
    let mut cues = Vec::new();
    for block in normalized.split("\n\n") {
        let lines: Vec<&str> = block.lines().filter(|l| !l.trim().is_empty()).collect();
        let Some(timing_at) = lines.iter().position(|l| l.contains("-->")) else {
            continue;
        };
        let body = &lines[timing_at + 1..];
        if body.is_empty() || body.iter().any(|l| is_ad_line(l)) {
            continue;
        }
        cues.push((lines[timing_at], body.join(newline)));
    }
    let mut out = String::from(bom);
    for (i, (timing, body)) in cues.iter().enumerate() {
        out.push_str(&format!("{}{newline}{timing}{newline}{body}{newline}{newline}", i + 1));
    }
    out.into_bytes()
}

pub struct SearchFlags {
    pub is_perfect_match: bool,
    pub is_forced: bool,
    pub is_hearing_impaired: bool,
}

pub async fn search(state: &AppState, item_id: &str, language: &str, flags: SearchFlags) -> Result<Vec<RemoteSubtitleInfoDto>, AppError> {
    let Some(pool) = state.database() else {
        return Ok(Vec::new());
    };
    let invoker = PluginSyncInvoker::new(pool.clone(), state.config().plugins.clone());
    let subscribers = match invoker.list_subscribers(SUBTITLE_PROVIDER_HOOK).await {
        Ok(s) if !s.is_empty() => s,
        Ok(_) => return Ok(Vec::new()),
        Err(err) => {
            tracing::warn!(error = ?err, "subtitle provider discovery failed");
            return Ok(Vec::new());
        }
    };
    let mut payload = item_context(pool, item_id).await?;
    payload["action"] = json!("search");
    payload["itemId"] = json!(item_id);
    payload["language"] = json!(language);
    payload["isPerfectMatch"] = json!(flags.is_perfect_match);
    payload["isForced"] = json!(flags.is_forced);
    payload["isHearingImpaired"] = json!(flags.is_hearing_impaired);
    let mut out = Vec::new();
    for sub in &subscribers {
        let outcome = invoker.invoke(sub, SUBTITLE_PROVIDER_HOOK, &payload, SyncAuditMode::FailuresOnly).await;
        match outcome.result {
            Ok(value) => out.extend(map_search_response(&outcome.plugin_id, &value, language)),
            Err(err) => tracing::warn!(plugin_id = %outcome.plugin_id, error = ?err, "subtitle provider search failed"),
        }
    }
    Ok(out)
}

/// 下载：返回字幕字节（调用方负责落盘 + probe）。插件未启用 → 409。
pub async fn download(state: &AppState, item_id: &str, subtitle_id: &str) -> Result<DownloadedSubtitle, AppError> {
    let (plugin_id, inner) = split_subtitle_id(subtitle_id).ok_or_else(|| AppError::unprocessable("SubtitleId is invalid"))?;
    let Some(pool) = state.database() else {
        return Err(AppError::internal("database is not configured"));
    };
    let invoker = PluginSyncInvoker::new(pool.clone(), state.config().plugins.clone());
    let subscribers = invoker
        .list_subscribers(SUBTITLE_PROVIDER_HOOK)
        .await
        .map_err(|err| AppError::internal(format!("subtitle provider discovery failed: {err:?}")))?;
    let sub = subscribers
        .iter()
        .find(|s| s.plugin_id == plugin_id)
        .ok_or_else(|| AppError::conflict("remote subtitle provider is not enabled"))?;
    let payload = json!({"action": "download", "itemId": item_id, "subtitleId": inner});
    let outcome = invoker.invoke(sub, SUBTITLE_PROVIDER_HOOK, &payload, SyncAuditMode::All).await;
    let value = outcome.result.map_err(|err| AppError::conflict(format!("subtitle provider download failed: {err:?}")))?;
    let mut subtitle = map_download_response(&value).ok_or_else(|| AppError::conflict("subtitle provider returned no subtitle data"))?;
    subtitle.data = strip_subtitle_ads(&subtitle.data, &subtitle.format);
    Ok(subtitle)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn maps_search_results_with_plugin_prefixed_ids() {
        let value = json!({"ok": true, "result": {"results": [
            {"id": "abc_1", "name": "Sintel.en.srt", "format": "srt", "language": "en", "threeLetterIsoLanguageName": "eng", "downloadCount": 12, "providerName": "OpenSubtitles"},
            {"id": "bad/id", "name": "x"},
            {"id": "", "name": "y"}
        ]}});
        let out = map_search_response("fbz.subtitle.opensubtitles", &value, "eng");
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].id, "fbz.subtitle.opensubtitles~abc_1");
        assert_eq!(out[0].provider_name, "OpenSubtitles");
        assert_eq!(out[0].download_count, 12);
        assert_eq!(out[0].three_letter_iso_language_name, "eng");
        assert_eq!(split_subtitle_id(&out[0].id), Some(("fbz.subtitle.opensubtitles", "abc_1")));
        assert_eq!(split_subtitle_id("noseparator"), None);
        assert!(map_search_response("p", &json!({"skipped": true}), "eng").is_empty());
    }

    #[test]
    fn moviehash_matches_reference_definition() {
        let dir = std::env::temp_dir().join(format!("fbz-oshash-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let zero = dir.join("zero.bin");
        std::fs::write(&zero, vec![0u8; 200_000]).unwrap();
        assert_eq!(opensubtitles_hash(&zero), Some(("0000000000030d40".to_owned(), 200_000)));
        // 首块第一个 u64 = 1，尾块最后一个 u64 = 2 → size + 3。
        let mut bytes = vec![0u8; 131_072];
        bytes[0] = 1;
        bytes[131_064] = 2;
        let ones = dir.join("ones.bin");
        std::fs::write(&ones, &bytes).unwrap();
        assert_eq!(opensubtitles_hash(&ones).unwrap().0, format!("{:016x}", 131_072u64 + 3));
        std::fs::write(dir.join("small.bin"), b"tiny").unwrap();
        assert!(opensubtitles_hash(&dir.join("small.bin")).is_none());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn strips_ad_cues_and_renumbers() {
        let srt = "1\r\n00:00:01,000 --> 00:00:02,000\r\nWatch Online Movies and Series for FREE\r\nwww.osdb.link/lm\r\n\r\n2\r\n00:00:03,000 --> 00:00:04,000\r\nHello\r\n\r\n3\r\n00:00:05,000 --> 00:00:06,000\r\nSupport us and become VIP member\r\n";
        let out = String::from_utf8(strip_subtitle_ads(srt.as_bytes(), "srt")).unwrap();
        assert_eq!(out, "1\r\n00:00:03,000 --> 00:00:04,000\r\nHello\r\n\r\n");
        let vtt = "WEBVTT\n\n00:01.000 --> 00:02.000\nHi\nAdvertise your product or brand here\n";
        let out = String::from_utf8(strip_subtitle_ads(vtt.as_bytes(), "vtt")).unwrap();
        assert!(!out.contains("Advertise") && out.contains("Hi"));
        let latin1 = [0xE9u8, b'\n'];
        assert_eq!(strip_subtitle_ads(&latin1, "srt"), latin1.to_vec());
    }

    #[test]
    fn maps_download_response() {
        let got = map_download_response(&json!({"format": "srt", "language": "eng", "dataBase64": "aGk="})).unwrap();
        assert_eq!(got.data, b"hi");
        assert_eq!(got.format, "srt");
        assert!(map_download_response(&json!({"format": "srt"})).is_none());
    }
}
