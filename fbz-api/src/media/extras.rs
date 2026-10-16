//! Extra (trailer / theme / featurette / …) minimal set — P1 draft.
//!
//! Types + scan include/ignore heuristics only. No download, no DB migration,
//! no `ItemType` CHECK change (keeps taxonomy / StorageProvider PRs clear).
//!
//! See `docs/plans/scan-lifecycle-mount-cache-extras-draft.md` and architecture §6.8.

use std::path::{Component, Path};

use serde::{Deserialize, Serialize};

/// Logical extra kind attached to a primary movie/episode/series item.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ExtraType {
    Trailer,
    ThemeSong,
    ThemeVideo,
    BehindTheScenes,
    Interview,
    Scene,
    Sample,
    Short,
    Clip,
    Featurette,
    DeletedScene,
    Other,
}

impl ExtraType {
    pub const ALL: [ExtraType; 12] = [
        ExtraType::Trailer,
        ExtraType::ThemeSong,
        ExtraType::ThemeVideo,
        ExtraType::BehindTheScenes,
        ExtraType::Interview,
        ExtraType::Scene,
        ExtraType::Sample,
        ExtraType::Short,
        ExtraType::Clip,
        ExtraType::Featurette,
        ExtraType::DeletedScene,
        ExtraType::Other,
    ];

    pub const fn as_str(self) -> &'static str {
        match self {
            ExtraType::Trailer => "trailer",
            ExtraType::ThemeSong => "theme_song",
            ExtraType::ThemeVideo => "theme_video",
            ExtraType::BehindTheScenes => "behind_the_scenes",
            ExtraType::Interview => "interview",
            ExtraType::Scene => "scene",
            ExtraType::Sample => "sample",
            ExtraType::Short => "short",
            ExtraType::Clip => "clip",
            ExtraType::Featurette => "featurette",
            ExtraType::DeletedScene => "deleted_scene",
            ExtraType::Other => "other",
        }
    }
}

/// What to do when a path looks like a sample.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum SampleDisposition {
    /// Skip discover / do not upsert as primary or extra.
    #[default]
    Ignore,
    /// Record as [`ExtraType::Sample`] linked to a primary when possible.
    AsExtra,
}

/// Directory names (lowercased) that mark their children as extras, not primaries.
pub const EXTRA_DIRECTORY_NAMES: &[&str] = &[
    "extras",
    "trailers",
    "featurettes",
    "samples",
    "sample",
    "theme-music",
    "theme",
    "themes",
    "interviews",
    "behind the scenes",
    "behindthescenes",
    "deleted scenes",
    "deletedscenes",
    "clips",
    "shorts",
    "scenes",
];

/// Filename tokens (matched as path-segment or `-token` / `.token.` style).
const EXTRA_FILENAME_HINTS: &[(&str, ExtraType)] = &[
    ("trailer", ExtraType::Trailer),
    ("theme-video", ExtraType::ThemeVideo),
    ("theme_video", ExtraType::ThemeVideo),
    ("themesong", ExtraType::ThemeSong),
    ("theme-song", ExtraType::ThemeSong),
    ("theme_song", ExtraType::ThemeSong),
    ("behindthescenes", ExtraType::BehindTheScenes),
    ("behind-the-scenes", ExtraType::BehindTheScenes),
    ("interview", ExtraType::Interview),
    ("featurette", ExtraType::Featurette),
    ("deletedscene", ExtraType::DeletedScene),
    ("deleted-scene", ExtraType::DeletedScene),
    ("sample", ExtraType::Sample),
    ("short", ExtraType::Short),
    ("clip", ExtraType::Clip),
    ("scene", ExtraType::Scene),
];

const THEME_AUDIO_HINTS: &[&str] = &["theme", "theme-music", "themesong"];

/// Classify a relative media path for Extra handling during scan.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ScanPathClass {
    /// Treat as normal primary media (movie/episode/…).
    Primary,
    /// Link as extra of the given type; do not classify as standalone movie.
    Extra(ExtraType),
    /// Drop from primary ingest (and from extras unless disposition says otherwise).
    Ignore,
}

/// Draft ignore / include rules for scanners (local + cloud import).
pub fn classify_scan_path(path: &Path, sample: SampleDisposition) -> ScanPathClass {
    let file_name = path
        .file_name()
        .and_then(|s| s.to_str())
        .unwrap_or("")
        .to_ascii_lowercase();
    let stem = Path::new(&file_name)
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or(&file_name)
        .to_ascii_lowercase();

    if let Some(extra) = classify_from_ancestors(path) {
        return finalize_sample(extra, sample);
    }

    if let Some(extra) = classify_from_stem(&stem) {
        return finalize_sample(extra, sample);
    }

    // Loose "theme.*" audio in a normal folder → theme song.
    if is_likely_audio(&file_name) && THEME_AUDIO_HINTS.iter().any(|h| stem == *h || stem.starts_with(&format!("{h}.")))
    {
        return ScanPathClass::Extra(ExtraType::ThemeSong);
    }

    ScanPathClass::Primary
}

fn finalize_sample(extra: ExtraType, sample: SampleDisposition) -> ScanPathClass {
    if extra == ExtraType::Sample {
        match sample {
            SampleDisposition::Ignore => ScanPathClass::Ignore,
            SampleDisposition::AsExtra => ScanPathClass::Extra(ExtraType::Sample),
        }
    } else {
        ScanPathClass::Extra(extra)
    }
}

fn classify_from_ancestors(path: &Path) -> Option<ExtraType> {
    for component in path.components() {
        let Component::Normal(os) = component else {
            continue;
        };
        let name = os.to_str()?.to_ascii_lowercase();
        if !EXTRA_DIRECTORY_NAMES.iter().any(|d| *d == name) {
            continue;
        }
        return Some(match name.as_str() {
            "trailers" => ExtraType::Trailer,
            "featurettes" | "extras" => ExtraType::Featurette,
            "samples" | "sample" => ExtraType::Sample,
            "theme-music" | "theme" | "themes" => ExtraType::ThemeSong,
            "interviews" => ExtraType::Interview,
            "behind the scenes" | "behindthescenes" => ExtraType::BehindTheScenes,
            "deleted scenes" | "deletedscenes" => ExtraType::DeletedScene,
            "clips" => ExtraType::Clip,
            "shorts" => ExtraType::Short,
            "scenes" => ExtraType::Scene,
            _ => ExtraType::Other,
        });
    }
    None
}

fn classify_from_stem(stem: &str) -> Option<ExtraType> {
    // Prefer longer / more specific hints first (slice is ordered that way-ish;
    // still scan all and keep the most specific by matching token boundaries).
    let mut best: Option<(usize, ExtraType)> = None;
    for (hint, kind) in EXTRA_FILENAME_HINTS {
        if !stem_has_hint(stem, hint) {
            continue;
        }
        let score = hint.len();
        if best.is_none_or(|(best_score, _)| score > best_score) {
            best = Some((score, *kind));
        }
    }
    best.map(|(_, kind)| kind)
}

fn stem_has_hint(stem: &str, hint: &str) -> bool {
    if stem == hint {
        return true;
    }
    // "Movie-trailer", "Movie.trailer", "Movie_trailer"
    let bordered = |sep: char| {
        stem.contains(&format!("{sep}{hint}"))
            || stem.contains(&format!("{hint}{sep}"))
            || stem.ends_with(&format!("{sep}{hint}"))
            || stem.starts_with(&format!("{hint}{sep}"))
    };
    bordered('-') || bordered('.') || bordered('_')
}

fn is_likely_audio(file_name: &str) -> bool {
    matches!(
        Path::new(file_name)
            .extension()
            .and_then(|e| e.to_str())
            .map(|e| e.to_ascii_lowercase())
            .as_deref(),
        Some("mp3" | "flac" | "m4a" | "aac" | "wav" | "ogg" | "opus" | "wma")
    )
}


/// Whether this extra belongs on ThemeMedia / ThemeSongs / ThemeVideos surfaces.
pub fn is_theme_media(extra: ExtraType) -> bool {
    matches!(extra, ExtraType::ThemeSong | ExtraType::ThemeVideo)
}

/// Whether this extra belongs on LocalTrailers / Trailers surfaces.
pub fn is_trailer_extra(extra: ExtraType) -> bool {
    matches!(extra, ExtraType::Trailer)
}

/// Emby `BaseItem.Type` override for a classified local extra (scan item_type stays movie/video/track).
pub fn emby_extra_item_type(extra: ExtraType) -> &'static str {
    match extra {
        ExtraType::Trailer => "Trailer",
        ExtraType::ThemeSong => "Audio",
        ExtraType::ThemeVideo => "Video",
        ExtraType::Sample => "Video",
        ExtraType::Featurette
        | ExtraType::BehindTheScenes
        | ExtraType::Interview
        | ExtraType::Scene
        | ExtraType::Short
        | ExtraType::Clip
        | ExtraType::DeletedScene
        | ExtraType::Other => "Video",
    }
}

/// Whether discover should skip creating a **primary** item for this path.
pub fn should_skip_as_primary(path: &Path, sample: SampleDisposition) -> bool {
    !matches!(classify_scan_path(path, sample), ScanPathClass::Primary)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn extra_type_strings_are_stable() {
        assert_eq!(ExtraType::Trailer.as_str(), "trailer");
        assert_eq!(ExtraType::ThemeSong.as_str(), "theme_song");
        assert_eq!(ExtraType::ALL.len(), 12);
    }

    #[test]
    fn trailer_filename_is_extra_not_primary() {
        let path = PathBuf::from("Movies/Foo (2020)/Foo-trailer.mkv");
        assert_eq!(
            classify_scan_path(&path, SampleDisposition::Ignore),
            ScanPathClass::Extra(ExtraType::Trailer)
        );
        assert!(should_skip_as_primary(&path, SampleDisposition::Ignore));
    }

    #[test]
    fn trailers_directory_marks_children() {
        let path = PathBuf::from("Series/Bar/trailers/teaser.mp4");
        assert_eq!(
            classify_scan_path(&path, SampleDisposition::Ignore),
            ScanPathClass::Extra(ExtraType::Trailer)
        );
    }

    #[test]
    fn sample_can_be_ignored_or_kept_as_extra() {
        let path = PathBuf::from("Movies/Foo/Foo-sample.mkv");
        assert_eq!(
            classify_scan_path(&path, SampleDisposition::Ignore),
            ScanPathClass::Ignore
        );
        assert_eq!(
            classify_scan_path(&path, SampleDisposition::AsExtra),
            ScanPathClass::Extra(ExtraType::Sample)
        );
    }

    #[test]
    fn normal_movie_stays_primary() {
        let path = PathBuf::from("Movies/Foo (2020)/Foo (2020).mkv");
        assert_eq!(
            classify_scan_path(&path, SampleDisposition::Ignore),
            ScanPathClass::Primary
        );
    }

    #[test]
    fn theme_audio_in_place_is_theme_song() {
        let path = PathBuf::from("Movies/Foo/theme.mp3");
        assert_eq!(
            classify_scan_path(&path, SampleDisposition::Ignore),
            ScanPathClass::Extra(ExtraType::ThemeSong)
        );
    }

    #[test]
    fn theme_and_trailer_predicates() {
        assert!(is_trailer_extra(ExtraType::Trailer));
        assert!(is_theme_media(ExtraType::ThemeSong));
        assert!(is_theme_media(ExtraType::ThemeVideo));
        assert!(!is_theme_media(ExtraType::Trailer));
        assert_eq!(emby_extra_item_type(ExtraType::Trailer), "Trailer");
        assert_eq!(emby_extra_item_type(ExtraType::ThemeSong), "Audio");
    }

    #[test]
    fn featurette_directory_maps_to_featurette() {
        let path = PathBuf::from("Movies/Foo/extras/making-of.mkv");
        assert_eq!(
            classify_scan_path(&path, SampleDisposition::Ignore),
            ScanPathClass::Extra(ExtraType::Featurette)
        );
    }
}
