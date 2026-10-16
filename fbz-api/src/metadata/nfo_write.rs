//! NFO 写回（Jellyfin「Nfo」元数据保存器子集）。
//!
//! - 开关：按库 `libraries.save_local_metadata`（JF LibraryOptions.MetadataSavers=Nfo /
//!   `POST /Library/VirtualFolders/LibraryOptions`），或全局 `NFO_SAVER_ENABLED=true`。
//! - 时机：元数据编辑器保存后、元数据刷新命中后。
//! - 范围：movie / video → `{文件名}.nfo`（同目录）；episode → `{文件名}.nfo`；
//!   series → 剧集目录 `tvshow.nfo`（由首个分集路径推断：`…/Show/Season x/ep.mkv` → `…/Show`）。
//! - 安全：只在目标不存在、或目标是本保存器写出的（含 `FBZ_NFO_MARKER`）时覆盖，
//!   绝不改写用户/其他工具手写的 NFO；先写临时文件再 rename。
use std::path::{Path, PathBuf};

use sqlx::Row;

use crate::db::DbPool;

pub const FBZ_NFO_MARKER: &str = "<!-- written by fbz nfo saver -->";

pub fn enabled() -> bool {
    matches!(
        std::env::var("NFO_SAVER_ENABLED").ok().as_deref().map(str::trim).map(str::to_ascii_lowercase).as_deref(),
        Some("1" | "true" | "yes" | "on")
    )
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct NfoDocument {
    pub root: &'static str,
    pub title: String,
    pub original_title: Option<String>,
    pub sort_title: Option<String>,
    pub overview: Option<String>,
    pub year: Option<i32>,
    pub premiered: Option<String>,
    pub mpaa: Option<String>,
    pub rating: Option<f64>,
    pub season: Option<i32>,
    pub episode: Option<i32>,
    pub genres: Vec<String>,
    pub tags: Vec<String>,
    pub studios: Vec<String>,
    /// (name, role_type, role_name)
    pub people: Vec<(String, String, String)>,
    /// (provider, id)
    pub unique_ids: Vec<(String, String)>,
}

fn esc(value: &str) -> String {
    value
        .chars()
        .filter(|c| !matches!(*c as u32, 0..=8 | 11 | 12 | 14..=31))
        .collect::<String>()
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

pub fn render(doc: &NfoDocument) -> String {
    let mut out = String::from("<?xml version=\"1.0\" encoding=\"utf-8\" standalone=\"yes\"?>\n");
    out.push_str(FBZ_NFO_MARKER);
    out.push('\n');
    out.push_str(&format!("<{}>\n", doc.root));
    let mut tag = |name: &str, value: &str| out.push_str(&format!("  <{name}>{}</{name}>\n", esc(value)));
    tag("title", &doc.title);
    if let Some(v) = &doc.original_title { tag("originaltitle", v) }
    if let Some(v) = &doc.sort_title { tag("sorttitle", v) }
    if let Some(v) = &doc.overview { tag("plot", v) }
    if let Some(v) = doc.year { tag("year", &v.to_string()) }
    if let Some(v) = &doc.premiered { tag(if doc.root == "episodedetails" { "aired" } else { "premiered" }, v) }
    if let Some(v) = &doc.mpaa { tag("mpaa", v) }
    if let Some(v) = doc.rating { tag("rating", &format!("{v:.1}")) }
    if let Some(v) = doc.season { tag(if doc.root == "season" { "seasonnumber" } else { "season" }, &v.to_string()) }
    if let Some(v) = doc.episode { tag("episode", &v.to_string()) }
    for g in &doc.genres { tag("genre", g) }
    for t in &doc.tags { tag("tag", t) }
    for s in &doc.studios { tag("studio", s) }
    for (provider, id) in &doc.unique_ids {
        out.push_str(&format!("  <uniqueid type=\"{}\">{}</uniqueid>\n", esc(provider), esc(id)));
        if provider == "imdb" || provider == "tmdb" || provider == "tvdb" {
            out.push_str(&format!("  <{provider}id>{}</{provider}id>\n", esc(id)));
        }
    }
    for (name, role_type, role_name) in &doc.people {
        match role_type.as_str() {
            "director" => out.push_str(&format!("  <director>{}</director>\n", esc(name))),
            "writer" => out.push_str(&format!("  <credits>{}</credits>\n", esc(name))),
            _ => {
                out.push_str(&format!("  <actor>\n    <name>{}</name>\n", esc(name)));
                if !role_name.is_empty() {
                    out.push_str(&format!("    <role>{}</role>\n", esc(role_name)));
                }
                if role_type != "actor" {
                    out.push_str(&format!("    <type>{}</type>\n", esc(role_type)));
                }
                out.push_str("  </actor>\n");
            }
        }
    }
    out.push_str(&format!("</{}>\n", doc.root));
    out
}

/// NFO 目标路径：视频同名 `.nfo`；剧集 `tvshow.nfo`。
pub fn target_path(item_type: &str, media_path: &Path) -> Option<PathBuf> {
    match item_type {
        "movie" | "video" | "episode" => Some(media_path.with_extension("nfo")),
        "series" => {
            let dir = media_path.parent()?;
            let show = if is_season_dir(dir) { dir.parent()? } else { dir };
            Some(show.join("tvshow.nfo"))
        }
        // JF：季目录下 `season.nfo`；分集直接放在剧目录（无季目录）时不写。
        "season" => {
            let dir = media_path.parent()?;
            is_season_dir(dir).then(|| dir.join("season.nfo"))
        }
        _ => None,
    }
}

fn is_season_dir(dir: &Path) -> bool {
    let Some(name) = dir.file_name().map(|n| n.to_string_lossy().to_ascii_lowercase()) else { return false };
    name.starts_with("season") || name.starts_with("specials") || (name.len() > 1 && name.starts_with('s') && name[1..].chars().all(|c| c.is_ascii_digit()))
}

pub fn root_for(item_type: &str) -> Option<&'static str> {
    match item_type {
        "movie" | "video" => Some("movie"),
        "episode" => Some("episodedetails"),
        "series" => Some("tvshow"),
        "season" => Some("season"),
        _ => None,
    }
}

/// 写文件：目标存在且不是本保存器产物 → 跳过（返回 Ok(false)）。
pub fn write_nfo(path: &Path, content: &str) -> std::io::Result<bool> {
    if let Ok(existing) = std::fs::read_to_string(path) {
        if !existing.contains(FBZ_NFO_MARKER) {
            return Ok(false);
        }
    } else if path.exists() {
        return Ok(false);
    }
    let tmp = path.with_extension("nfo.fbz-tmp");
    std::fs::write(&tmp, content)?;
    std::fs::rename(&tmp, path)?;
    Ok(true)
}

/// 从库里读出条目并写 NFO。返回写出的路径（未启用/不支持类型/被保护时为 None）。
/// 分集写完后顺带写所属季的 `season.nfo`（JF 刷新分集/剧也会落季 NFO）。
pub async fn save_item_nfo(pool: &DbPool, public_id: &str) -> Result<Option<PathBuf>, String> {
    let written = save_single_nfo(pool, public_id).await?;
    let season: Option<String> = sqlx::query_scalar(
        "select s.public_id::text from media_items e join media_items s on s.id = e.parent_id
         where e.public_id::text = $1 and e.item_type = 'episode' and s.item_type = 'season'",
    )
    .bind(public_id)
    .fetch_optional(pool)
    .await
    .map_err(|e| e.to_string())?;
    if let Some(season) = season {
        save_single_nfo(pool, &season).await?;
    }
    Ok(written)
}

async fn save_single_nfo(pool: &DbPool, public_id: &str) -> Result<Option<PathBuf>, String> {
    let row = sqlx::query(
        r#"select id, item_type, title, original_title, sort_title, overview, production_year,
                  premiere_date::text as premiere_date, official_rating, community_rating::float8 as community_rating,
                  coalesce(parent_index_number, season_number) as season, coalesce(index_number, episode_number) as episode,
                  coalesce(season_number, index_number) as season_self,
                  (select l.save_local_metadata from libraries l where l.id = media_items.library_id) as library_saver
           from media_items where public_id::text = $1 and is_deleted = false"#,
    )
    .bind(public_id)
    .fetch_optional(pool)
    .await
    .map_err(|e| e.to_string())?;
    let Some(row) = row else { return Ok(None) };
    let library_saver: Option<bool> = row.try_get("library_saver").ok().flatten();
    if !(enabled() || library_saver.unwrap_or(false)) {
        return Ok(None);
    }
    let id: i64 = row.try_get("id").map_err(|e| e.to_string())?;
    let item_type: String = row.try_get("item_type").map_err(|e| e.to_string())?;
    let Some(root) = root_for(&item_type) else { return Ok(None) };
    let media_path: Option<String> = if item_type == "season" {
        sqlx::query_scalar(
            "select f.path from media_items e join media_files f on f.media_item_id = e.id
             where e.parent_id = $1 and e.item_type = 'episode' and e.is_deleted = false and f.is_strm = false
             order by f.path limit 1",
        )
        .bind(id)
        .fetch_optional(pool)
        .await
        .map_err(|e| e.to_string())?
    } else if item_type == "series" {
        sqlx::query_scalar(
            "select f.path from media_items e join media_files f on f.media_item_id = e.id
             left join media_items s on s.id = e.parent_id
             where (e.parent_id = $1 or s.parent_id = $1) and e.item_type = 'episode' and e.is_deleted = false and f.is_strm = false
             order by f.path limit 1",
        )
        .bind(id)
        .fetch_optional(pool)
        .await
        .map_err(|e| e.to_string())?
    } else {
        sqlx::query_scalar("select path from media_files where media_item_id = $1 and is_strm = false order by is_primary desc, id limit 1")
            .bind(id)
            .fetch_optional(pool)
            .await
            .map_err(|e| e.to_string())?
    };
    let Some(media_path) = media_path else { return Ok(None) };
    let Some(target) = target_path(&item_type, Path::new(&media_path)) else { return Ok(None) };
    let names = |sql: &'static str| async move {
        sqlx::query_scalar::<_, String>(sql).bind(id).fetch_all(pool).await.map_err(|e| e.to_string())
    };
    let genres = names("select g.name from media_item_genres x join genres g on g.id = x.genre_id where x.media_item_id = $1 order by g.name").await?;
    let tags = names("select t.name from media_item_tags x join tags t on t.id = x.tag_id where x.media_item_id = $1 order by t.name").await?;
    let studios = names("select s.name from media_item_studios x join studios s on s.id = x.studio_id where x.media_item_id = $1 order by s.name").await?;
    let people = sqlx::query("select p.name, x.role_type, x.role_name from media_item_people x join people p on p.id = x.person_id where x.media_item_id = $1 order by x.sort_order, x.id")
        .bind(id)
        .fetch_all(pool)
        .await
        .map_err(|e| e.to_string())?
        .into_iter()
        .filter_map(|r| Some((r.try_get("name").ok()?, r.try_get("role_type").ok()?, r.try_get("role_name").ok()?)))
        .collect();
    let unique_ids = sqlx::query("select provider, external_id from media_external_ids where media_item_id = $1 order by provider")
        .bind(id)
        .fetch_all(pool)
        .await
        .map_err(|e| e.to_string())?
        .into_iter()
        .filter_map(|r| Some((r.try_get("provider").ok()?, r.try_get("external_id").ok()?)))
        .collect();
    let doc = NfoDocument {
        root,
        title: row.try_get("title").map_err(|e| e.to_string())?,
        original_title: row.try_get("original_title").ok().flatten(),
        sort_title: row.try_get("sort_title").ok().flatten(),
        overview: row.try_get("overview").ok().flatten(),
        year: row.try_get("production_year").ok().flatten(),
        premiered: row.try_get("premiere_date").ok().flatten(),
        mpaa: row.try_get("official_rating").ok().flatten(),
        rating: row.try_get("community_rating").ok().flatten(),
        season: match root {
            "episodedetails" => row.try_get("season").ok().flatten(),
            "season" => row.try_get("season_self").ok().flatten(),
            _ => None,
        },
        episode: if root == "episodedetails" { row.try_get("episode").ok().flatten() } else { None },
        genres,
        tags,
        studios,
        people,
        unique_ids,
    };
    let content = render(&doc);
    let written = tokio::task::spawn_blocking({
        let target = target.clone();
        move || write_nfo(&target, &content)
    })
    .await
    .map_err(|e| e.to_string())?
    .map_err(|e| e.to_string())?;
    Ok(written.then_some(target))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn renders_season_nfo_with_seasonnumber() {
        let doc = NfoDocument { root: "season", title: "Season 2".into(), season: Some(2), ..Default::default() };
        let xml = render(&doc);
        assert!(xml.contains("<season>\n") && xml.contains("<seasonnumber>2</seasonnumber>"));
        assert!(!xml.contains("<season>2</season>"));
    }

    #[test]
    fn renders_kodi_style_movie_nfo() {
        let doc = NfoDocument {
            root: "movie",
            title: "A & B".into(),
            overview: Some("<plot>".into()),
            year: Some(2001),
            rating: Some(7.25),
            genres: vec!["Drama".into()],
            people: vec![("Dir".into(), "director".into(), String::new()), ("Act".into(), "actor".into(), "Hero".into()), ("W".into(), "writer".into(), String::new())],
            unique_ids: vec![("imdb".into(), "tt0000001".into())],
            ..Default::default()
        };
        let xml = render(&doc);
        assert!(xml.contains(FBZ_NFO_MARKER));
        assert!(xml.contains("<title>A &amp; B</title>"));
        assert!(xml.contains("<plot>&lt;plot&gt;</plot>"));
        assert!(xml.contains("<rating>7.2</rating>") || xml.contains("<rating>7.3</rating>"));
        assert!(xml.contains("<director>Dir</director>") && xml.contains("<credits>W</credits>"));
        assert!(xml.contains("<name>Act</name>\n    <role>Hero</role>"));
        assert!(xml.contains("<uniqueid type=\"imdb\">tt0000001</uniqueid>") && xml.contains("<imdbid>tt0000001</imdbid>"));
        // 写出的 NFO 能被本地 NFO 读入解析回标题。
        assert!(xml.trim_end().ends_with("</movie>"));
    }

    #[test]
    fn target_paths_and_protection() {
        assert_eq!(target_path("movie", Path::new("/m/Film (2001)/Film.mkv")), Some(PathBuf::from("/m/Film (2001)/Film.nfo")));
        assert_eq!(target_path("series", Path::new("/tv/Show/Season 01/e1.mkv")), Some(PathBuf::from("/tv/Show/tvshow.nfo")));
        assert_eq!(target_path("series", Path::new("/tv/Show/e1.mkv")), Some(PathBuf::from("/tv/Show/tvshow.nfo")));
        assert_eq!(target_path("season", Path::new("/tv/Show/Season 02/e1.mkv")), Some(PathBuf::from("/tv/Show/Season 02/season.nfo")));
        assert_eq!(target_path("season", Path::new("/tv/Show/e1.mkv")), None);
        assert_eq!(root_for("season"), Some("season"));
        assert_eq!(target_path("book", Path::new("/b/x.epub")), None);
        let dir = std::env::temp_dir().join(format!("fbz-nfo-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let p = dir.join("x.nfo");
        std::fs::write(&p, "<movie><title>hand made</title></movie>").unwrap();
        assert!(!write_nfo(&p, "new").unwrap());
        assert!(std::fs::read_to_string(&p).unwrap().contains("hand made"));
        std::fs::remove_file(&p).unwrap();
        assert!(write_nfo(&p, &format!("{FBZ_NFO_MARKER}\n<movie/>")).unwrap());
        assert!(write_nfo(&p, &format!("{FBZ_NFO_MARKER}\n<movie>2</movie>")).unwrap());
        assert!(std::fs::read_to_string(&p).unwrap().contains("<movie>2</movie>"));
        let _ = std::fs::remove_dir_all(&dir);
    }
}
