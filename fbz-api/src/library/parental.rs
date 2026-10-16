//! Emby/JF 家长控制：MaxParentalRating / BlockUnratedItems。
//!
//! 等级表与 `/Localization/ParentalRatings` 的电影档对齐，并补常见 TV 标签；
//! 未识别的 official_rating 视为未分级（走 BlockUnratedItems）。

/// 将官方分级名映射为与 Localization ParentalRatings 一致的数值。
pub fn official_rating_level(name: &str) -> Option<i32> {
    let key = name.trim().to_ascii_uppercase().replace('_', "-");
    let key = key.trim();
    if key.is_empty() {
        return None;
    }
    match key {
        "G" | "TV-Y" | "TV-G" => Some(1),
        "TV-Y7" | "TV-Y7-FV" => Some(3),
        "PG" | "TV-PG" => Some(5),
        "PG-13" | "TV-14" => Some(8),
        "R" | "TV-MA" => Some(9),
        "NC-17" => Some(10),
        _ => None,
    }
}

/// 将 Policy.BlockUnratedItems（Emby UnratedItem 名）展开为内部 `item_type`。
pub fn normalize_block_unrated_items(raw: &[String]) -> Vec<String> {
    let mut out = Vec::new();
    for entry in raw {
        let key = entry.trim().to_ascii_lowercase();
        if key.is_empty() {
            continue;
        }
        match key.as_str() {
            "movie" => out.push("movie".to_owned()),
            "trailer" => {
                out.push("trailer".to_owned());
                out.push("video".to_owned());
            }
            "series" => {
                out.push("series".to_owned());
                out.push("season".to_owned());
                out.push("episode".to_owned());
            }
            "music" => {
                out.push("artist".to_owned());
                out.push("album".to_owned());
                out.push("track".to_owned());
            }
            "book" => out.push("book".to_owned()),
            "game" => out.push("game".to_owned()),
            "livetvchannel" | "livetvprogram" | "channelcontent" => {
                out.push("livetv".to_owned());
                out.push("channel".to_owned());
            }
            "other" => out.push("other".to_owned()),
            other => out.push(other.to_owned()),
        }
    }
    out.sort_unstable();
    out.dedup();
    out
}

/// Items SQL 用的 official_rating → 等级 CASE（与 [`official_rating_level`] 同步）。
pub const OFFICIAL_RATING_LEVEL_SQL: &str = r#"
case lower(btrim(coalesce(mi.official_rating, '')))
    when 'g' then 1
    when 'tv-y' then 1
    when 'tv-g' then 1
    when 'tv-y7' then 3
    when 'tv-y7-fv' then 3
    when 'pg' then 5
    when 'tv-pg' then 5
    when 'pg-13' then 8
    when 'tv-14' then 8
    when 'r' then 9
    when 'tv-ma' then 9
    when 'nc-17' then 10
    else null
end
"#;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn official_rating_level_matches_localization_movie_ladder() {
        assert_eq!(official_rating_level("G"), Some(1));
        assert_eq!(official_rating_level("PG"), Some(5));
        assert_eq!(official_rating_level("PG-13"), Some(8));
        assert_eq!(official_rating_level("R"), Some(9));
        assert_eq!(official_rating_level("NC-17"), Some(10));
        assert_eq!(official_rating_level("  pg-13 "), Some(8));
        assert_eq!(official_rating_level(""), None);
        assert_eq!(official_rating_level("NotARating"), None);
    }

    #[test]
    fn block_unrated_series_expands_to_episode_tree() {
        let types = normalize_block_unrated_items(&[
            "Movie".to_owned(),
            "Series".to_owned(),
            "Music".to_owned(),
        ]);
        assert!(types.contains(&"movie".to_owned()));
        assert!(types.contains(&"series".to_owned()));
        assert!(types.contains(&"episode".to_owned()));
        assert!(types.contains(&"track".to_owned()));
    }

    #[test]
    fn sql_case_covers_same_keys_as_rust_mapper() {
        let sql = OFFICIAL_RATING_LEVEL_SQL.to_ascii_lowercase();
        for name in ["g", "pg", "pg-13", "r", "nc-17", "tv-ma", "tv-14"] {
            assert!(
                sql.contains(&format!("when '{name}'")),
                "SQL missing rating {name}"
            );
        }
    }
}
