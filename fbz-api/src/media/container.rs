//! MediaSource 对外容器名规范化。
//!
//! ffprobe `format_name` 常给出 demuxer 别名（`matroska`、`mpegts`、逗号列表）。
//! Emby 客户端与 DeviceProfile 匹配更习惯扩展名风格（`mkv`、`ts`、`mp4`）。

/// 将探测/存储得到的容器串规范为 MediaSource.`Container` 对外值。
///
/// - 取逗号列表首段（如 `matroska,webm` → `matroska` 再映射）
/// - 常见别名映射到扩展名：`matroska`→`mkv`，`mpegts`→`ts`，…
/// - 无法识别时返回小写原串（已 trim）；空输入返回 `None`
pub fn normalize_media_source_container(raw: Option<&str>) -> Option<String> {
    let first = raw?
        .split([',', '|'])
        .map(str::trim)
        .find(|part| !part.is_empty())?
        .trim_start_matches('.')
        .to_ascii_lowercase();

    let mapped = match first.as_str() {
        "matroska" => "mkv",
        "webm" => "webm",
        "mpegts" | "m2ts" | "mts" => "ts",
        "mp4" | "3gp" | "3g2" | "mj2" => "mp4",
        "mov" => "mov",
        "m4v" => "m4v",
        "m4a" => "m4a",
        "ogg" | "ogv" | "oga" => "ogg",
        "mpeg" | "mpg" | "vob" => "mpg",
        other => other,
    };

    Some(mapped.to_owned())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn matroska_becomes_mkv() {
        assert_eq!(
            normalize_media_source_container(Some("matroska")).as_deref(),
            Some("mkv")
        );
        assert_eq!(
            normalize_media_source_container(Some("matroska,webm")).as_deref(),
            Some("mkv")
        );
    }

    #[test]
    fn mpegts_becomes_ts() {
        assert_eq!(
            normalize_media_source_container(Some("mpegts")).as_deref(),
            Some("ts")
        );
    }

    #[test]
    fn already_mkv_stays() {
        assert_eq!(
            normalize_media_source_container(Some("mkv")).as_deref(),
            Some("mkv")
        );
    }

    #[test]
    fn empty_is_none() {
        assert_eq!(normalize_media_source_container(None), None);
        assert_eq!(normalize_media_source_container(Some("  ")), None);
    }
}
