//! 远程歌词 provider 接线（插件优先）：`lyrics.provider.query` 同步能力。
//!
//! - 搜索：`{action:"search", itemId, title, artist, album, durationSeconds, searchTerm}` →
//!   插件回 `{results:[{id, providerName, trackName, artistName, albumName, syncedLyrics, plainLyrics}]}`。
//! - 取词：`{action:"download", lyricId}` → `{syncedLyrics?, plainLyrics?, trackName?, artistName?, albumName?}`。
//!
//! 对外 LyricId = `{pluginId}~{providerLyricId}`。无已启用插件时，调用方回退到 core 内置 LRCLIB。
use serde::Deserialize;
use serde_json::{Value, json};

use crate::{
    db::DbPool,
    error::AppError,
    plugins::invoke::{PluginSyncInvoker, SyncAuditMode},
    state::AppState,
};

pub const LYRICS_PROVIDER_HOOK: &str = "lyrics.provider.query";
const ID_SEPARATOR: char = '~';

fn unwrap_envelope(value: &Value) -> &Value {
    value.get("result").filter(|_| value.get("ok").and_then(Value::as_bool) == Some(true)).unwrap_or(value)
}

/// 插件返回的一条歌词（原文 + 元数据）。
#[derive(Clone, Debug, Default, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", default)]
pub struct ProviderLyric {
    /// LRCLIB `/api/get/{n}` 返回数字 id；插件返回字符串，两者都接受。
    #[serde(deserialize_with = "string_or_number")]
    pub id: String,
    pub provider_name: Option<String>,
    pub track_name: Option<String>,
    pub artist_name: Option<String>,
    pub album_name: Option<String>,
    pub synced_lyrics: Option<String>,
    pub plain_lyrics: Option<String>,
}

fn string_or_number<'de, D: serde::Deserializer<'de>>(d: D) -> Result<String, D::Error> {
    Ok(match serde_json::Value::deserialize(d)? {
        serde_json::Value::String(v) => v,
        serde_json::Value::Number(n) => n.to_string(),
        _ => String::new(),
    })
}

impl ProviderLyric {
    /// (正文, 扩展名)：同步歌词优先写 `.lrc`，否则纯文本 `.txt`。
    pub fn text(&self) -> Option<(&str, &'static str)> {
        fn pick(v: &Option<String>) -> Option<&str> {
            v.as_deref().filter(|t| !t.trim().is_empty())
        }
        pick(&self.synced_lyrics).map(|t| (t, "lrc")).or_else(|| pick(&self.plain_lyrics).map(|t| (t, "txt")))
    }
}

pub fn map_search_response(plugin_id: &str, value: &Value) -> Vec<ProviderLyric> {
    let Some(items) = unwrap_envelope(value).get("results").and_then(Value::as_array) else {
        return Vec::new();
    };
    items
        .iter()
        .filter_map(|raw| serde_json::from_value::<ProviderLyric>(raw.clone()).ok())
        .filter(|r| !r.id.trim().is_empty() && !r.id.contains(['/', '\\']) && r.text().is_some())
        .map(|mut r| {
            r.id = format!("{plugin_id}{ID_SEPARATOR}{}", r.id.trim());
            r.provider_name.get_or_insert_with(|| plugin_id.to_owned());
            r
        })
        .collect()
}

pub fn split_lyric_id(id: &str) -> Option<(&str, &str)> {
    let (plugin, inner) = id.split_once(ID_SEPARATOR)?;
    (!plugin.is_empty() && !inner.is_empty()).then_some((plugin, inner))
}

pub(super) async fn track_context(pool: &DbPool, item_id: &str) -> Result<Value, AppError> {
    use sqlx::Row;
    let row = sqlx::query(
        r#"
        select t.title, t.runtime_ticks,
               case when a.item_type = 'album' then a.title end as album,
               coalesce(
                   case when ar.item_type = 'artist' then ar.title end,
                   (select p.name from media_item_people x join people p on p.id = x.person_id
                     where x.media_item_id in (t.id, a.id) and x.role_type in ('artist', 'composer') order by x.sort_order limit 1)
               ) as artist
        from media_items t
        left join media_items a on a.id = t.parent_id
        left join media_items ar on ar.id = a.parent_id
        where t.public_id::text = $1 and t.is_deleted = false
        "#,
    )
    .bind(item_id)
    .fetch_optional(pool)
    .await
    .map_err(|err| AppError::internal(format!("failed to load track for lyrics search: {err}")))?
    .ok_or_else(|| AppError::not_found("item not found"))?;
    let ticks: Option<i64> = row.try_get("runtime_ticks").ok().flatten();
    Ok(json!({
        "title": row.try_get::<String, _>("title").ok(),
        "album": row.try_get::<Option<String>, _>("album").ok().flatten(),
        "artist": row.try_get::<Option<String>, _>("artist").ok().flatten(),
        "durationSeconds": ticks.map(|t| t / 10_000_000),
    }))
}

/// 搜索：`None` = 没有已启用的歌词插件（调用方回退内置 LRCLIB）。
pub async fn search(state: &AppState, item_id: &str, search_term: Option<&str>) -> Result<Option<Vec<ProviderLyric>>, AppError> {
    let Some(pool) = state.database() else { return Ok(None) };
    let invoker = PluginSyncInvoker::new(pool.clone(), state.config().plugins.clone());
    let subscribers = match invoker.list_subscribers(LYRICS_PROVIDER_HOOK).await {
        Ok(s) if !s.is_empty() => s,
        Ok(_) => return Ok(None),
        Err(err) => {
            tracing::warn!(error = ?err, "lyrics provider discovery failed");
            return Ok(None);
        }
    };
    let mut payload = track_context(pool, item_id).await?;
    payload["action"] = json!("search");
    payload["itemId"] = json!(item_id);
    payload["searchTerm"] = json!(search_term);
    let mut out = Vec::new();
    for sub in &subscribers {
        let outcome = invoker.invoke(sub, LYRICS_PROVIDER_HOOK, &payload, SyncAuditMode::FailuresOnly).await;
        match outcome.result {
            Ok(value) => out.extend(map_search_response(&outcome.plugin_id, &value)),
            Err(err) => tracing::warn!(plugin_id = %outcome.plugin_id, error = ?err, "lyrics provider search failed"),
        }
    }
    Ok(Some(out))
}

/// 取词：插件 id 形如 `{pluginId}~{inner}`；插件未启用 → 409。
pub async fn fetch(state: &AppState, lyric_id: &str) -> Result<ProviderLyric, AppError> {
    let (plugin_id, inner) = split_lyric_id(lyric_id).ok_or_else(|| AppError::unprocessable("LyricId is invalid"))?;
    let Some(pool) = state.database() else {
        return Err(AppError::internal("database is not configured"));
    };
    let invoker = PluginSyncInvoker::new(pool.clone(), state.config().plugins.clone());
    let subscribers = invoker
        .list_subscribers(LYRICS_PROVIDER_HOOK)
        .await
        .map_err(|err| AppError::internal(format!("lyrics provider discovery failed: {err:?}")))?;
    let sub = subscribers
        .iter()
        .find(|s| s.plugin_id == plugin_id)
        .ok_or_else(|| AppError::conflict("remote lyrics provider is not enabled"))?;
    let outcome = invoker
        .invoke(sub, LYRICS_PROVIDER_HOOK, &json!({"action": "download", "lyricId": inner}), SyncAuditMode::FailuresOnly)
        .await;
    let value = outcome.result.map_err(|err| AppError::conflict(format!("lyrics provider download failed: {err:?}")))?;
    let mut lyric: ProviderLyric = serde_json::from_value(unwrap_envelope(&value).clone()).unwrap_or_default();
    lyric.id = lyric_id.to_owned();
    if lyric.text().is_none() {
        return Err(AppError::not_found("lyrics provider returned no lyrics"));
    }
    Ok(lyric)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accepts_numeric_lrclib_ids() {
        let lyric: ProviderLyric = serde_json::from_value(json!({"id": 30585262, "plainLyrics": "x"})).unwrap();
        assert_eq!(lyric.id, "30585262");
        assert_eq!(lyric.text(), Some(("x", "txt")));
    }

    #[test]
    fn maps_results_and_prefers_synced_text() {
        let value = json!({"ok": true, "result": {"results": [
            {"id": "123", "providerName": "LrcLib", "trackName": "T", "syncedLyrics": "[00:01.00]hi", "plainLyrics": "hi"},
            {"id": "456", "plainLyrics": "plain only"},
            {"id": "789"},
            {"id": "a/b", "plainLyrics": "x"}
        ]}});
        let out = map_search_response("fbz.lyrics.lrclib", &value);
        assert_eq!(out.len(), 2);
        assert_eq!(out[0].id, "fbz.lyrics.lrclib~123");
        assert_eq!(out[0].text(), Some(("[00:01.00]hi", "lrc")));
        assert_eq!(out[1].text(), Some(("plain only", "txt")));
        assert_eq!(out[1].provider_name.as_deref(), Some("fbz.lyrics.lrclib"));
        assert_eq!(split_lyric_id(&out[0].id), Some(("fbz.lyrics.lrclib", "123")));
        assert_eq!(split_lyric_id("lrclib-1"), None);
    }
}
