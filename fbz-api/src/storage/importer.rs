use super::{nfo, read_small, sql_error};
use crate::{error::AppError, state::AppState};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use sqlx::{Postgres, Row, Transaction};

pub fn is_video(name: &str) -> bool {
    matches!(
        name.rsplit('.')
            .next()
            .unwrap_or("")
            .to_ascii_lowercase()
            .as_str(),
        "mp4" | "mkv" | "m4v" | "mov" | "avi" | "ts" | "m2ts" | "webm"
    )
}
fn stem(name: &str) -> &str {
    name.rsplit_once('.').map(|p| p.0).unwrap_or(name)
}
async fn siblings(
    tx: &mut Transaction<'_, Postgres>,
    mount: &str,
    parent: &str,
    generation: i64,
) -> Result<Vec<Value>, AppError> {
    sqlx::query_scalar("select entry from storage_entries where mount_id::text=$1 and parent_id=$2 and generation=$3 order by remote_id").bind(mount).bind(parent).bind(generation).fetch_all(&mut **tx).await.map_err(sql_error)
}
pub async fn cached(
    state: &AppState,
    account: &str,
    entry: &Value,
    max: usize,
) -> Result<(String, Vec<u8>), AppError> {
    let name = entry["name"].as_str().unwrap_or("");
    let extension = name
        .rsplit('.')
        .next()
        .unwrap_or("bin")
        .to_ascii_lowercase();
    if ![
        "nfo", "jpg", "jpeg", "png", "webp", "srt", "ass", "ssa", "vtt",
    ]
    .contains(&extension.as_str())
    {
        return Err(AppError::unprocessable("不支持的小文件类型"));
    }
    let hash = format!(
        "{:x}",
        Sha256::digest(format!("{account}:{}:{}", entry["id"], entry["version"]))
    );
    let key = format!("storage/{hash}.{extension}");
    let target = state.config().storage.artwork_cache_dir.join(&key);
    if let Ok(bytes) = tokio::fs::read(&target).await {
        if bytes.len() <= max {
            return Ok((key, bytes));
        }
    }
    let bytes = read_small(state, account, entry["id"].as_str().unwrap_or(""), max).await?;
    tokio::fs::create_dir_all(target.parent().unwrap())
        .await
        .map_err(|_| AppError::internal("无法创建云端资料缓存"))?;
    let temp = target.with_extension(format!(
        "{}.tmp",
        crate::auth::token::issue_access_token().token
    ));
    tokio::fs::write(&temp, &bytes)
        .await
        .map_err(|_| AppError::internal("缓存写入失败"))?;
    if tokio::fs::rename(&temp, &target).await.is_err() {
        let _ = tokio::fs::remove_file(&temp).await;
        if !target.exists() {
            return Err(AppError::internal("缓存保存失败"));
        }
    }
    Ok((key, bytes))
}
async fn metadata(
    state: &AppState,
    account: &str,
    entries: &[Value],
    file_name: &str,
    tv: bool,
) -> Result<nfo::Nfo, AppError> {
    let exact = format!("{}.nfo", stem(file_name)).to_lowercase();
    let entry = entries
        .iter()
        .find(|e| {
            e["name"]
                .as_str()
                .unwrap_or("")
                .eq_ignore_ascii_case(&exact)
        })
        .or_else(|| {
            if tv {
                None
            } else {
                entries.iter().find(|e| {
                    e["name"]
                        .as_str()
                        .unwrap_or("")
                        .eq_ignore_ascii_case("movie.nfo")
                })
            }
        });
    if let Some(entry) = entry {
        let (_, bytes) = cached(state, account, entry, 2 * 1024 * 1024).await?;
        nfo::parse(&bytes)
    } else {
        Ok(nfo::Nfo::default())
    }
}
async fn group(
    tx: &mut Transaction<'_, Postgres>,
    mount: &str,
    library: i64,
    key: &str,
    title: &str,
    kind: &str,
    parent: Option<i64>,
    number: Option<i32>,
) -> Result<i64, AppError> {
    if let Some(id) = sqlx::query_scalar::<_, i64>(
        "select media_item_id from storage_groups where mount_id::text=$1 and group_key=$2",
    )
    .bind(mount)
    .bind(key)
    .fetch_optional(&mut **tx)
    .await
    .map_err(sql_error)?
    {
        sqlx::query(
            "update media_items set title=$2,is_deleted=false,scan_status='scanned' where id=$1",
        )
        .bind(id)
        .bind(title)
        .execute(&mut **tx)
        .await
        .map_err(sql_error)?;
        return Ok(id);
    }
    let id:i64=sqlx::query_scalar("insert into media_items(library_id,title,item_type,parent_id,index_number,season_number,metadata_status,scan_status,is_virtual) values($1,$2,$3,$4,$5,$5,'manual','scanned',true) returning id")
        .bind(library).bind(title).bind(kind).bind(parent).bind(number).fetch_one(&mut **tx).await.map_err(sql_error)?;
    sqlx::query(
        "insert into storage_groups(mount_id,group_key,media_item_id) values($1::uuid,$2,$3)",
    )
    .bind(mount)
    .bind(key)
    .bind(id)
    .execute(&mut **tx)
    .await
    .map_err(sql_error)?;
    Ok(id)
}
async fn save_artwork(
    state: &AppState,
    tx: &mut Transaction<'_, Postgres>,
    account: &str,
    item: i64,
    entries: &[Value],
    media_name: &str,
    prefetch: bool,
) -> Result<(), AppError> {
    for (kind, names) in [
        (
            "poster",
            vec!["poster".to_owned(), format!("{}-poster", stem(media_name))],
        ),
        (
            "backdrop",
            vec!["fanart".to_owned(), format!("{}-fanart", stem(media_name))],
        ),
    ] {
        if let Some(entry) = entries.iter().find(|e| {
            let n = e["name"].as_str().unwrap_or("").to_lowercase();
            names.iter().any(|base| {
                ["jpg", "jpeg", "png", "webp"]
                    .iter()
                    .any(|ext| n == format!("{base}.{ext}"))
            })
        }) {
            let extension = entry["name"]
                .as_str()
                .unwrap_or("")
                .rsplit('.')
                .next()
                .unwrap_or("jpg")
                .to_ascii_lowercase();
            let hash = format!(
                "{:x}",
                Sha256::digest(format!("{account}:{}:{}", entry["id"], entry["version"]))
            );
            let key = format!("storage/{hash}.{extension}");
            sqlx::query("insert into storage_artwork_sources(storage_key,account_id,entry) values($1,$2::uuid,$3) on conflict(storage_key) do update set entry=excluded.entry").bind(&key).bind(account).bind(entry).execute(&mut **tx).await.map_err(sql_error)?;
            if prefetch {
                cached(state, account, entry, 10 * 1024 * 1024).await?;
            }
            sqlx::query("delete from artwork where media_item_id=$1 and source='storage' and artwork_type=$2").bind(item).bind(kind).execute(&mut **tx).await.map_err(sql_error)?;
            sqlx::query("insert into artwork(media_item_id,artwork_type,source,storage_key,is_primary) values($1,$2,'storage',$3,true)").bind(item).bind(kind).bind(key).execute(&mut **tx).await.map_err(sql_error)?;
        }
    }
    Ok(())
}

#[allow(clippy::too_many_arguments)]
pub async fn import_video(
    state: &AppState,
    tx: &mut Transaction<'_, Postgres>,
    mount: &str,
    account: &str,
    library: i64,
    library_type: &str,
    parent_dir: &str,
    entry: &Value,
    generation: i64,
) -> Result<(), AppError> {
    let file_id = entry["id"]
        .as_str()
        .ok_or_else(|| AppError::unprocessable("missing file ID"))?;
    let name = entry["name"].as_str().unwrap_or("Untitled");
    let entries = siblings(tx, mount, parent_dir, generation).await?;
    let options =
        sqlx::query("select nfo_source,image_cache from storage_mounts where id::text=$1")
            .bind(mount)
            .fetch_one(&mut **tx)
            .await
            .map_err(sql_error)?;
    let use_nfo = options.get::<String, _>("nfo_source") == "cloud";
    let prefetch = options.get::<String, _>("image_cache") == "prefetch";
    // Invalid NFO is a per-item warning; a network error must not advance the checkpoint.
    let mut meta = match if use_nfo {
        metadata(state, account, &entries, name, library_type == "tv").await
    } else {
        Ok(nfo::Nfo::default())
    } {
        Ok(meta) => meta,
        Err(AppError::UnprocessableEntity { message }) if message.starts_with("NFO") => {
            sqlx::query("update storage_entries set entry=entry||$3 where mount_id::text=$1 and remote_id=$2").bind(mount).bind(file_id).bind(json!({"warning":message})).execute(&mut **tx).await.map_err(sql_error)?;
            nfo::Nfo::default()
        }
        Err(error) => return Err(error),
    };
    if library_type == "tv" {
        if let Some((season, episode)) = nfo::episode_numbers(name) {
            meta.season = meta.season.or(Some(season));
            meta.episode = meta.episode.or(Some(episode));
        }
    }
    let title = if meta.title.trim().is_empty() {
        stem(name)
    } else {
        meta.title.trim()
    };
    let mut parent_item = None;
    if library_type == "tv" {
        let mut dir = parent_dir.to_owned();
        let mut series_meta = None;
        let mut series_entries = Vec::new();
        for _ in 0..32 {
            let siblings = siblings(tx, mount, &dir, generation).await?;
            if let Some(nfo) = siblings.iter().filter(|_| use_nfo).find(|e| {
                e["name"]
                    .as_str()
                    .unwrap_or("")
                    .eq_ignore_ascii_case("tvshow.nfo")
            }) {
                let (_, bytes) = cached(state, account, nfo, 2 * 1024 * 1024).await?;
                series_meta = Some(nfo::parse(&bytes)?);
                series_entries = siblings;
                break;
            }
            if !use_nfo {
                series_entries = siblings;
                break;
            }
            let next: Option<String> = sqlx::query_scalar(
                "select parent_id from storage_entries where mount_id::text=$1 and remote_id=$2",
            )
            .bind(mount)
            .bind(&dir)
            .fetch_optional(&mut **tx)
            .await
            .map_err(sql_error)?;
            if let Some(next) = next {
                if next == dir {
                    break;
                }
                dir = next;
            } else {
                break;
            }
        }
        let fallback: Option<String> = sqlx::query_scalar(
            "select name from storage_entries where mount_id::text=$1 and remote_id=$2",
        )
        .bind(mount)
        .bind(&dir)
        .fetch_optional(&mut **tx)
        .await
        .map_err(sql_error)?;
        let series_title = series_meta
            .as_ref()
            .map(|m| m.title.as_str())
            .filter(|s| !s.trim().is_empty())
            .unwrap_or(fallback.as_deref().unwrap_or("未命名剧集"));
        let series = group(
            tx,
            mount,
            library,
            &format!("series:{dir}"),
            series_title,
            "series",
            None,
            None,
        )
        .await?;
        if let Some(series_meta) = series_meta.as_ref() {
            sqlx::query("update media_items set title=$2,overview=$3,production_year=$4,metadata_status='manual',is_deleted=false where id=$1")
                .bind(series).bind(series_title).bind(&series_meta.plot).bind(series_meta.year).execute(&mut **tx).await.map_err(sql_error)?;
        }
        save_artwork(
            state,
            tx,
            account,
            series,
            &series_entries,
            "tvshow",
            prefetch,
        )
        .await?;
        let season = meta.season.unwrap_or(1).max(0);
        parent_item = Some(
            group(
                tx,
                mount,
                library,
                &format!("season:{dir}:{season}"),
                &format!("第 {season} 季"),
                "season",
                Some(series),
                Some(season),
            )
            .await?,
        );
    }
    let existing: Option<i64> = sqlx::query_scalar(
        "select media_item_id from storage_entries where mount_id::text=$1 and remote_id=$2",
    )
    .bind(mount)
    .bind(file_id)
    .fetch_one(&mut **tx)
    .await
    .map_err(sql_error)?;
    let item = if let Some(id) = existing {
        id
    } else {
        sqlx::query_scalar("insert into media_items(library_id,title,item_type,metadata_status,scan_status) values($1,$2,$3,'manual','scanned') returning id").bind(library).bind(title).bind(if library_type=="tv"{"episode"}else{"movie"}).fetch_one(&mut **tx).await.map_err(sql_error)?
    };
    let runtime = meta.runtime.map(|v| (v * 60.0 * 10_000_000.0) as i64);
    sqlx::query("update media_items set title=$2,original_title=$3,overview=$4,production_year=$5,runtime_ticks=$6,parent_id=$7,season_number=$8,episode_number=$9,index_number=$9,parent_index_number=$8,metadata_status='manual',scan_status='scanned',is_deleted=false,updated_at=now() where id=$1")
        .bind(item).bind(title).bind(&meta.originaltitle).bind(&meta.plot).bind(meta.year).bind(runtime).bind(parent_item).bind(meta.season).bind(meta.episode).execute(&mut **tx).await.map_err(sql_error)?;
    let path = format!("fbz-storage://{mount}/{file_id}");
    let hash = Sha256::digest(path.as_bytes()).to_vec();
    let container = name.rsplit('.').next().unwrap_or("mp4").to_lowercase();
    let media_file:i64=sqlx::query_scalar("insert into media_files(media_item_id,path,normalized_path,path_hash,file_size,container,duration_ticks) values($1,$2,$2,$3,$4,$5,$6) on conflict(path_hash) do update set file_size=excluded.file_size,container=excluded.container,duration_ticks=excluded.duration_ticks returning id")
        .bind(item).bind(&path).bind(hash).bind(entry["size"].as_i64()).bind(container).bind(runtime).fetch_one(&mut **tx).await.map_err(sql_error)?;
    sqlx::query("update storage_entries set media_item_id=$3,media_file_id=$4,imported_version=$5 where mount_id::text=$1 and remote_id=$2").bind(mount).bind(file_id).bind(item).bind(media_file).bind(entry["version"].as_str()).execute(&mut **tx).await.map_err(sql_error)?;
    for id in &meta.uniqueid {
        if ["tmdb", "imdb", "tvdb"].contains(&id.kind.as_str()) && !id.value.trim().is_empty() {
            sqlx::query("insert into media_external_ids(media_item_id,provider,external_id) values($1,$2,$3) on conflict do nothing").bind(item).bind(&id.kind).bind(id.value.trim()).execute(&mut **tx).await.map_err(sql_error)?;
        }
    }
    // Preserve cloud NFO genres using the same normalized domain tables as providers.
    sqlx::query("delete from media_item_genres where media_item_id=$1")
        .bind(item)
        .execute(&mut **tx)
        .await
        .map_err(sql_error)?;
    for genre in &meta.genre {
        if genre.trim().is_empty() {
            continue;
        }
        let genre_id:i64=sqlx::query_scalar("insert into genres(name,name_normalized) values($1,$2) on conflict(name_normalized) do update set name=excluded.name returning id").bind(genre.trim()).bind(genre.trim().to_lowercase()).fetch_one(&mut **tx).await.map_err(sql_error)?;
        sqlx::query("insert into media_item_genres(media_item_id,genre_id) values($1,$2) on conflict do nothing").bind(item).bind(genre_id).execute(&mut **tx).await.map_err(sql_error)?;
    }
    save_artwork(state, tx, account, item, &entries, name, prefetch).await?;
    sqlx::query("delete from media_streams where media_file_id=$1 and stream_type='subtitle'")
        .bind(media_file)
        .execute(&mut **tx)
        .await
        .map_err(sql_error)?;
    sqlx::query("delete from storage_subtitles where media_file_id=$1")
        .bind(media_file)
        .execute(&mut **tx)
        .await
        .map_err(sql_error)?;
    let mut index = 1000;
    for subtitle in &entries {
        let n = subtitle["name"].as_str().unwrap_or("");
        let ext = n.rsplit('.').next().unwrap_or("").to_lowercase();
        if !["srt", "ass", "ssa", "vtt"].contains(&ext.as_str())
            || !(stem(n) == stem(name) || stem(n).starts_with(&format!("{}.", stem(name))))
        {
            continue;
        }
        let (key, _) = cached(state, account, subtitle, 2 * 1024 * 1024).await?;
        sqlx::query("insert into media_streams(media_file_id,stream_index,stream_type,codec,title,is_external) values($1,$2,'subtitle',$3,$4,true)").bind(media_file).bind(index).bind(&ext).bind(n).execute(&mut **tx).await.map_err(sql_error)?;
        sqlx::query("insert into storage_subtitles(media_file_id,stream_index,storage_key,codec) values($1,$2,$3,$4)").bind(media_file).bind(index).bind(key).bind(ext).execute(&mut **tx).await.map_err(sql_error)?;
        index += 1;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn accepts_video_not_metadata() {
        assert!(is_video("Film.MKV"));
        assert!(!is_video("film.nfo"));
        assert!(!is_video("folder"));
    }
}

/// Called only after the artwork endpoint has checked the library ACL.
pub async fn ensure_artwork(state: &AppState, key: &str) -> Result<(), AppError> {
    if !key.starts_with("storage/") || key.contains("..") || key.contains('\\') {
        return Ok(());
    }
    if state.config().storage.artwork_cache_dir.join(key).is_file() {
        return Ok(());
    }
    let row=sqlx::query("select account_id::text as account,entry from storage_artwork_sources where storage_key=$1").bind(key).fetch_optional(super::db(state)?).await.map_err(sql_error)?;
    if let Some(row) = row {
        cached(
            state,
            &row.get::<String, _>("account"),
            &row.get::<Value, _>("entry"),
            10 * 1024 * 1024,
        )
        .await?;
    }
    Ok(())
}
