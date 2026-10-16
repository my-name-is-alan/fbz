//! 本地 NFO → Match（Jellyfin 对齐：库级源链 NFO 槽可读 sidecar）。
//!
//! - 仅处理本地路径（非 `fbz-storage://`）；云盘 NFO 仍走 `storage/importer`。
//! - 候选：`{stem}.nfo`、同目录 `movie.nfo`；剧集还可试 `tvshow.nfo`（仅补外部 id/剧名弱匹配）。
//! - 安全：复用 `storage::nfo::parse`（大小/嵌套/DTD 限制）。

use std::path::{Path, PathBuf};

use crate::metadata::provider::shared::{
    MetadataExternalId, MetadataLookup, MetadataMatch, MetadataNamedValue, normalize_text_title,
};
use crate::storage::nfo::{self, Nfo};

const MAX_NFO_READ_BYTES: usize = 2 * 1024 * 1024;

/// 尝试从 lookup.primary_path 旁路读取 NFO 并转为 MetadataMatch。
/// `Ok(None)` = 无可用 NFO（调用方记 skipped）；`Err` 仅用于不可恢复 I/O（当前一律 Ok）。
pub fn try_match_local_nfo(lookup: &MetadataLookup) -> Result<Option<MetadataMatch>, String> {
    let Some(path) = lookup
        .primary_path
        .as_deref()
        .map(str::trim)
        .filter(|p| !p.is_empty())
    else {
        return Ok(None);
    };
    if path.starts_with("fbz-storage://") {
        return Ok(None);
    }
    let media = Path::new(path);
    let Some(bytes) = read_first_nfo(media, &lookup.item_type) else {
        return Ok(None);
    };
    let parsed = nfo::parse(&bytes).map_err(|err| format!("{err:?}"))?;
    Ok(Some(nfo_to_match(lookup, &parsed)))
}

fn read_first_nfo(media: &Path, item_type: &str) -> Option<Vec<u8>> {
    for candidate in nfo_candidates(media, item_type) {
        if let Ok(bytes) = std::fs::read(&candidate) {
            if bytes.is_empty() || bytes.len() > MAX_NFO_READ_BYTES {
                continue;
            }
            return Some(bytes);
        }
    }
    None
}

fn nfo_candidates(media: &Path, item_type: &str) -> Vec<PathBuf> {
    let mut out = Vec::new();
    let parent = media.parent().unwrap_or_else(|| Path::new("."));
    if let Some(stem) = media.file_stem().and_then(|s| s.to_str()) {
        out.push(parent.join(format!("{stem}.nfo")));
    }
    let kind = item_type.trim().to_ascii_lowercase();
    if matches!(kind.as_str(), "movie" | "video") {
        out.push(parent.join("movie.nfo"));
    }
    if matches!(kind.as_str(), "episode" | "season" | "series" | "tvshow") {
        out.push(parent.join("tvshow.nfo"));
    }
    out
}

fn nfo_to_match(lookup: &MetadataLookup, nfo: &Nfo) -> MetadataMatch {
    let title = if nfo.title.trim().is_empty() {
        lookup.title.clone()
    } else {
        nfo.title.trim().to_owned()
    };
    let mut external_ids = Vec::new();
    for uid in &nfo.uniqueid {
        let provider = uid.kind.trim().to_ascii_lowercase();
        let value = uid.value.trim();
        if provider.is_empty() || value.is_empty() {
            continue;
        }
        external_ids.push(MetadataExternalId {
            provider: provider.clone(),
            external_id: value.to_owned(),
        });
    }
    let external_id = external_ids
        .first()
        .map(|e| e.external_id.clone())
        .unwrap_or_else(|| format!("nfo:{}", title.to_ascii_lowercase()));

    let genres = nfo
        .genre
        .iter()
        .filter_map(|g| {
            let name = g.trim();
            if name.is_empty() {
                return None;
            }
            Some(MetadataNamedValue {
                name: name.to_owned(),
                name_normalized: normalize_text_title(name.to_owned()).unwrap_or_else(|| name.to_ascii_lowercase()),
            })
        })
        .collect();

    MetadataMatch {
        provider: "nfo".to_owned(),
        external_id,
        external_ids,
        title,
        series_title: None,
        original_title: {
            let t = nfo.originaltitle.trim();
            if t.is_empty() {
                None
            } else {
                Some(t.to_owned())
            }
        },
        overview: {
            let p = nfo.plot.trim();
            if p.is_empty() {
                None
            } else {
                Some(p.to_owned())
            }
        },
        production_year: nfo.year.or(lookup.production_year),
        premiere_date: None,
        official_rating: None,
        community_rating: None,
        artwork: Vec::new(),
        genres,
        studios: Vec::new(),
        networks: Vec::new(),
        videos: Vec::new(),
        collection: None,
        people: Vec::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    fn lookup_with_path(path: &str) -> MetadataLookup {
        MetadataLookup {
            item_type: "movie".to_owned(),
            title: "Fallback".to_owned(),
            original_title: None,
            production_year: Some(2010),
            season: None,
            episode: None,
            tmdb_id: None,
            imdb_id: None,
            tvdb_id: None,
            language: None,
            country: None,
            image_language: None,
            image_prefer_original: false,
            image_fallback_languages: Vec::new(),
            primary_path: Some(path.to_owned()),
        }
    }

    #[test]
    fn skips_when_no_path() {
        let mut lookup = lookup_with_path("");
        lookup.primary_path = None;
        assert!(try_match_local_nfo(&lookup).unwrap().is_none());
    }

    #[test]
    fn skips_storage_uri() {
        let lookup = lookup_with_path("fbz-storage://acct/file");
        assert!(try_match_local_nfo(&lookup).unwrap().is_none());
    }

    #[test]
    fn matches_sidecar_nfo() {
        let dir = std::env::temp_dir().join(format!("fbz-nfo-test-{}", std::process::id()));
        let _ = std::fs::create_dir_all(&dir);
        let media = dir.join("Sintel.mkv");
        std::fs::write(&media, b"fake").unwrap();
        let nfo_path = dir.join("Sintel.nfo");
        let _ = std::fs::remove_file(&nfo_path);
        let mut f = std::fs::File::create(&nfo_path).unwrap();
        write!(
            f,
            r#"<?xml version="1.0"?><movie><title>Sintel</title><year>2010</year><uniqueid type="imdb">tt1727587</uniqueid><genre>Animation</genre><plot>Open movie.</plot></movie>"#
        )
        .unwrap();

        let matched = try_match_local_nfo(&lookup_with_path(media.to_str().unwrap()))
            .unwrap()
            .expect("nfo match");
        assert_eq!(matched.provider, "nfo");
        assert_eq!(matched.title, "Sintel");
        assert_eq!(matched.production_year, Some(2010));
        assert!(
            matched
                .external_ids
                .iter()
                .any(|e| e.provider == "imdb" && e.external_id == "tt1727587")
        );
        assert_eq!(matched.genres[0].name, "Animation");
        let _ = std::fs::remove_dir_all(&dir);
    }
}
