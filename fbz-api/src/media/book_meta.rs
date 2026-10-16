//! 电子书元数据 + 封面提取（扫描期调用，阻塞 IO，调用方放 spawn_blocking）。
//!
//! - epub：OPF `<metadata>`（dc:title/creator/description/date/language）+ 封面
//!   （`<meta name="cover">` 指向的 manifest 项，或 `properties="cover-image"`，
//!   再退回首个 id/href 含 "cover" 的图片）。
//! - cbz：`ComicInfo.xml`（Title/Series/Writer/Summary/Year/LanguageISO）+ 自然序首页作封面。
//! - pdf：首页渲染为封面（`pdftoppm`，macOS 退回 `sips`）；文本元数据不解析。
//! 封面统一解码→缩放（长边 ≤ 600）→ JPEG，写 `ARTWORK_CACHE_DIR/book-covers/{id}.jpg`。

use std::{
    io::{Cursor, Read},
    path::{Path, PathBuf},
    sync::OnceLock,
};

use image::{ImageReader, imageops::FilterType};
use quick_xml::events::Event;

pub const COVER_MAX_EDGE: u32 = 600;
static ARTWORK_DIR: OnceLock<PathBuf> = OnceLock::new();

/// 启动时设置封面根目录（= `ARTWORK_CACHE_DIR`）。未设置时用 `./var/artwork`。
pub fn set_artwork_dir(dir: PathBuf) {
    let _ = ARTWORK_DIR.set(dir);
}

pub fn artwork_dir() -> PathBuf {
    ARTWORK_DIR.get().cloned().unwrap_or_else(|| PathBuf::from("./var/artwork"))
}

pub fn cover_storage_key(media_item_id: i64) -> String {
    format!("book-covers/{media_item_id}.jpg")
}

#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct BookMeta {
    pub title: Option<String>,
    pub authors: Vec<String>,
    pub overview: Option<String>,
    pub year: Option<i32>,
    pub language: Option<String>,
    pub series: Option<String>,
    /// ISBN-10/13（epub `dc:identifier`、ComicInfo `GTIN`），只保留数字/X。
    pub isbn: Option<String>,
    /// 原始封面字节（未缩放）。
    pub cover: Option<Vec<u8>>,
}

pub fn extract(path: &Path) -> BookMeta {
    // mobi/azw3/cbr：先转换为 epub/cbz 缓存副本，再按目标格式提取。
    if super::book_convert::target_format(path).is_some() {
        return match super::book_convert::readable_path(path) {
            Ok(converted) => extract(&converted),
            Err(err) => {
                tracing::warn!(path = %path.display(), error = %err, "book conversion failed");
                BookMeta::default()
            }
        };
    }
    let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("").to_ascii_lowercase();
    let result = match ext.as_str() {
        "epub" => extract_epub(path),
        "cbz" => extract_cbz(path),
        "pdf" => Ok(BookMeta { cover: render_pdf_first_page(path), ..BookMeta::default() }),
        _ => Ok(BookMeta::default()),
    };
    result.unwrap_or_default()
}

fn zip_entry(zip: &mut zip::ZipArchive<std::fs::File>, name: &str) -> Option<Vec<u8>> {
    let mut entry = zip.by_name(name).ok()?;
    let mut buf = Vec::new();
    entry.read_to_end(&mut buf).ok()?;
    Some(buf)
}

fn extract_epub(path: &Path) -> Result<BookMeta, String> {
    let file = std::fs::File::open(path).map_err(|e| e.to_string())?;
    let mut zip = zip::ZipArchive::new(file).map_err(|e| e.to_string())?;
    let container = zip_entry(&mut zip, "META-INF/container.xml").ok_or("no container")?;
    let opf_path = rootfile(&String::from_utf8_lossy(&container)).ok_or("no rootfile")?;
    let opf = zip_entry(&mut zip, &opf_path).ok_or("no opf")?;
    let (mut meta, cover_href) = parse_opf_metadata(&opf_path, &String::from_utf8_lossy(&opf));
    if let Some(href) = cover_href {
        meta.cover = zip_entry(&mut zip, &href);
    }
    Ok(meta)
}

fn attr(e: &quick_xml::events::BytesStart<'_>, key: &[u8]) -> Option<String> {
    e.attributes()
        .flatten()
        .find(|a| a.key.local_name().as_ref() == key)
        .and_then(|a| a.unescape_value().ok().map(|v| v.into_owned()))
}

fn rootfile(container_xml: &str) -> Option<String> {
    let mut reader = quick_xml::Reader::from_str(container_xml);
    loop {
        match reader.read_event() {
            Ok(Event::Start(e) | Event::Empty(e)) if e.local_name().as_ref() == b"rootfile" => return attr(&e, b"full-path"),
            Ok(Event::Eof) | Err(_) => return None,
            _ => {}
        }
    }
}

fn join_zip_path(base: &str, href: &str) -> String {
    let href = href.split('#').next().unwrap_or("").replace("%20", " ");
    let mut parts: Vec<&str> = base.split('/').filter(|p| !p.is_empty()).collect();
    for seg in href.split('/') {
        match seg {
            "" | "." => {}
            ".." => {
                parts.pop();
            }
            s => parts.push(s),
        }
    }
    parts.join("/")
}

fn clean_text(raw: &str) -> Option<String> {
    let text = strip_tags(raw);
    let text = text.split_whitespace().collect::<Vec<_>>().join(" ");
    (!text.is_empty()).then_some(text)
}

/// 去掉 HTML 标签（dc:description 常含转义后的 `<p>`），并解常见实体。
pub fn strip_tags(raw: &str) -> String {
    let mut out = String::with_capacity(raw.len());
    let mut in_tag = false;
    for ch in raw.chars() {
        match ch {
            '<' => in_tag = true,
            '>' if in_tag => {
                in_tag = false;
                out.push(' ');
            }
            c if !in_tag => out.push(c),
            _ => {}
        }
    }
    out.replace("&nbsp;", " ").replace("&amp;", "&").replace("&lt;", "<").replace("&gt;", ">").replace("&quot;", "\"").replace("&#39;", "'")
}

/// 从 `urn:isbn:978…` / `ISBN 0-306-40615-2` 等抽出合法长度（10/13）的 ISBN。
pub fn normalize_isbn(raw: &str) -> Option<String> {
    let lower = raw.to_ascii_lowercase();
    if lower.contains("uuid") || lower.contains("calibre") {
        return None;
    }
    let digits: String = raw.chars().filter(|c| c.is_ascii_digit() || *c == 'X' || *c == 'x').collect::<String>().to_ascii_uppercase();
    let ok = (digits.len() == 13 && digits.chars().all(|c| c.is_ascii_digit()) && (digits.starts_with("978") || digits.starts_with("979")))
        || (digits.len() == 10 && digits[..9].chars().all(|c| c.is_ascii_digit()));
    ok.then_some(digits)
}

/// 元数据落库（扫描与「刷新元数据」共用）：标题/简介/年份、作者（people role=writer）、
/// ISBN（media_external_ids provider=isbn）、封面（artwork primary，`book-covers/{id}.jpg`）。
/// `replace` 为 true 时覆盖已有标题/简介/年份，否则只补空。返回是否写入了任何内容。
pub async fn apply_to_db(pool: &crate::db::DbPool, media_item_id: i64, meta: BookMeta, cover_source: &str, replace: bool) -> Result<bool, sqlx::Error> {
    let mut applied = false;
    let title = meta.title.as_deref().map(str::trim).filter(|t| !t.is_empty());
    let sql = if replace {
        "update media_items set title = coalesce($2, title), sort_title = coalesce($2, sort_title), overview = coalesce($3, overview), production_year = coalesce($4, production_year), updated_at = now() where id = $1"
    } else {
        "update media_items set overview = coalesce(overview, $3), production_year = coalesce(production_year, $4), updated_at = now() where id = $1 and ($2::text is null or true)"
    };
    if title.is_some() || meta.overview.is_some() || meta.year.is_some() {
        sqlx::query(sql).bind(media_item_id).bind(title).bind(meta.overview.as_deref()).bind(meta.year).execute(pool).await?;
        applied = true;
    }
    // 只补空模式：已有作者时不再追加（避免在线来源把别的作者混进来）。
    let has_writers = !replace
        && sqlx::query_scalar::<_, bool>("select exists(select 1 from media_item_people where media_item_id = $1 and role_type = 'writer')")
            .bind(media_item_id)
            .fetch_one(pool)
            .await?;
    let cast_locked = sqlx::query_scalar::<_, bool>("select lock_data or 'Cast' = any(locked_fields) from media_items where id = $1")
        .bind(media_item_id)
        .fetch_optional(pool)
        .await?
        .unwrap_or(false);
    let authors: &[String] = if has_writers || cast_locked { &[] } else { &meta.authors };
    for author in authors.iter().take(5) {
        let normalized = author.trim().to_lowercase();
        if normalized.is_empty() {
            continue;
        }
        let pinyin = crate::text::pinyin::pinyin_keys(author);
        let person_id = sqlx::query_scalar::<_, i64>(
            r#"
            insert into people (name, name_normalized, pinyin_full, pinyin_initials)
            values ($1, $2, $3, $4)
            on conflict (name_normalized) do update set name = people.name, updated_at = now()
            returning id
            "#,
        )
        .bind(author.trim())
        .bind(&normalized)
        .bind(pinyin.as_ref().map(|keys| keys.full.as_str()))
        .bind(pinyin.as_ref().map(|keys| keys.initials.as_str()))
        .fetch_one(pool)
        .await?;
        sqlx::query("insert into media_item_people (media_item_id, person_id, role_type) values ($1, $2, 'writer') on conflict do nothing")
            .bind(media_item_id)
            .bind(person_id)
            .execute(pool)
            .await?;
        applied = true;
    }
    if let Some(isbn) = meta.isbn.as_deref() {
        sqlx::query("insert into media_external_ids (media_item_id, provider, external_id) values ($1, 'isbn', $2) on conflict do nothing")
            .bind(media_item_id)
            .bind(isbn)
            .execute(pool)
            .await?;
        applied = true;
    }
    if let Some(cover) = meta.cover {
        let written = tokio::task::spawn_blocking(move || write_cover(media_item_id, &cover)).await.ok().flatten();
        if let Some((key, width, height)) = written {
            sqlx::query("delete from artwork where media_item_id = $1 and storage_key = $2")
                .bind(media_item_id)
                .bind(&key)
                .execute(pool)
                .await?;
            sqlx::query("update artwork set is_primary = false where media_item_id = $1 and artwork_type = 'primary'")
                .bind(media_item_id)
                .execute(pool)
                .await?;
            sqlx::query(
                "insert into artwork (media_item_id, artwork_type, source, storage_key, width, height, is_primary) values ($1, 'primary', $2, $3, $4, $5, true)",
            )
            .bind(media_item_id)
            .bind(cover_source)
            .bind(&key)
            .bind(width as i32)
            .bind(height as i32)
            .execute(pool)
            .await?;
            applied = true;
        }
    }
    Ok(applied)
}

fn year_from(text: &str) -> Option<i32> {
    let digits: String = text.trim().chars().take(4).collect();
    digits.parse::<i32>().ok().filter(|y| (1000..=2999).contains(y))
}

/// 解析 OPF 元数据；返回 (元数据, 封面在 zip 内的完整路径)。
pub fn parse_opf_metadata(opf_path: &str, opf: &str) -> (BookMeta, Option<String>) {
    let base = opf_path.rsplit_once('/').map(|(d, _)| format!("{d}/")).unwrap_or_default();
    let mut meta = BookMeta::default();
    let mut cover_id: Option<String> = None;
    let mut cover_prop: Option<String> = None;
    let mut cover_guess: Option<String> = None;
    let mut items = std::collections::HashMap::<String, String>::new();
    let mut current: Option<Vec<u8>> = None;
    let mut buffer = String::new();
    let mut reader = quick_xml::Reader::from_str(opf);
    loop {
        match reader.read_event() {
            Ok(Event::Start(e)) => {
                let name = e.local_name().as_ref().to_vec();
                if matches!(name.as_slice(), b"title" | b"creator" | b"description" | b"date" | b"language" | b"identifier") {
                    current = Some(name);
                    buffer.clear();
                } else {
                    handle_opf_empty(&e, &mut items, &mut cover_id, &mut cover_prop, &mut cover_guess, &mut meta);
                }
            }
            Ok(Event::Empty(e)) => handle_opf_empty(&e, &mut items, &mut cover_id, &mut cover_prop, &mut cover_guess, &mut meta),
            Ok(Event::Text(t)) if current.is_some() => {
                let decoded = t.decode().map(|c| c.into_owned()).unwrap_or_else(|_| String::from_utf8_lossy(t.as_ref()).into_owned());
                buffer.push_str(&quick_xml::escape::unescape(&decoded).map(|c| c.into_owned()).unwrap_or(decoded));
            }
            Ok(Event::GeneralRef(r)) if current.is_some() => {
                let name = String::from_utf8_lossy(r.as_ref()).into_owned();
                buffer.push_str(&quick_xml::escape::unescape(&format!("&{name};")).map(|c| c.into_owned()).unwrap_or_default());
            }
            Ok(Event::CData(t)) if current.is_some() => buffer.push_str(&String::from_utf8_lossy(t.as_ref())),
            Ok(Event::End(e)) => {
                if current.as_deref() == Some(e.local_name().as_ref()) {
                    let value = buffer.trim().to_owned();
                    match current.take().as_deref() {
                        Some(b"title") if meta.title.is_none() && !value.is_empty() => meta.title = Some(value),
                        Some(b"creator") if !value.is_empty() && !meta.authors.contains(&value) => meta.authors.push(value),
                        Some(b"description") if meta.overview.is_none() => meta.overview = clean_text(&value),
                        Some(b"date") if meta.year.is_none() => meta.year = year_from(&value),
                        Some(b"language") if meta.language.is_none() && !value.is_empty() => meta.language = Some(value),
                        Some(b"identifier") if meta.isbn.is_none() => meta.isbn = normalize_isbn(&value),
                        _ => {}
                    }
                }
            }
            Ok(Event::Eof) | Err(_) => break,
            _ => {}
        }
    }
    let href = cover_prop
        .or_else(|| cover_id.and_then(|id| items.get(&id).cloned()))
        .or(cover_guess)
        .map(|h| join_zip_path(&base, &h));
    (meta, href)
}

fn handle_opf_empty(
    e: &quick_xml::events::BytesStart<'_>,
    items: &mut std::collections::HashMap<String, String>,
    cover_id: &mut Option<String>,
    cover_prop: &mut Option<String>,
    cover_guess: &mut Option<String>,
    meta: &mut BookMeta,
) {
    match e.local_name().as_ref() {
        b"meta" => {
            let name = attr(e, b"name").unwrap_or_default();
            if name == "cover" {
                *cover_id = attr(e, b"content");
            } else if name == "calibre:series" {
                meta.series = attr(e, b"content").filter(|s| !s.trim().is_empty());
            }
        }
        b"item" => {
            let (Some(id), Some(href)) = (attr(e, b"id"), attr(e, b"href")) else {
                return;
            };
            let media_type = attr(e, b"media-type").unwrap_or_default();
            if media_type.starts_with("image/") {
                if attr(e, b"properties").is_some_and(|p| p.split_whitespace().any(|x| x == "cover-image")) {
                    *cover_prop = Some(href.clone());
                }
                if cover_guess.is_none() && (id.to_ascii_lowercase().contains("cover") || href.to_ascii_lowercase().contains("cover")) {
                    *cover_guess = Some(href.clone());
                }
            }
            items.insert(id, href);
        }
        _ => {}
    }
}

const IMAGE_EXTS: [&str; 6] = ["jpg", "jpeg", "png", "gif", "webp", "bmp"];

fn natural_key(name: &str) -> Vec<(u8, u64, String)> {
    let mut out = Vec::new();
    let mut digits = String::new();
    let mut text = String::new();
    for ch in name.to_ascii_lowercase().chars() {
        if ch.is_ascii_digit() {
            if !text.is_empty() {
                out.push((1, 0, std::mem::take(&mut text)));
            }
            digits.push(ch);
        } else {
            if !digits.is_empty() {
                out.push((0, digits.parse().unwrap_or(u64::MAX), String::new()));
                digits.clear();
            }
            text.push(ch);
        }
    }
    if !digits.is_empty() {
        out.push((0, digits.parse().unwrap_or(u64::MAX), String::new()));
    }
    if !text.is_empty() {
        out.push((1, 0, text));
    }
    out
}

fn extract_cbz(path: &Path) -> Result<BookMeta, String> {
    let file = std::fs::File::open(path).map_err(|e| e.to_string())?;
    let mut zip = zip::ZipArchive::new(file).map_err(|e| e.to_string())?;
    let names: Vec<String> = zip.file_names().map(str::to_owned).collect();
    let mut meta = names
        .iter()
        .find(|n| n.rsplit('/').next().is_some_and(|f| f.eq_ignore_ascii_case("ComicInfo.xml")))
        .cloned()
        .and_then(|n| zip_entry(&mut zip, &n))
        .map(|xml| parse_comic_info(&String::from_utf8_lossy(&xml)))
        .unwrap_or_default();
    let mut pages: Vec<&String> = names
        .iter()
        .filter(|n| !n.ends_with('/') && !n.starts_with("__MACOSX"))
        .filter(|n| IMAGE_EXTS.contains(&n.rsplit('.').next().unwrap_or("").to_ascii_lowercase().as_str()))
        .collect();
    pages.sort_by_key(|n| natural_key(n));
    if let Some(first) = pages.first().map(|s| (*s).clone()) {
        meta.cover = zip_entry(&mut zip, &first);
    }
    Ok(meta)
}

/// ComicInfo.xml（ComicRack 约定）。
pub fn parse_comic_info(xml: &str) -> BookMeta {
    let mut meta = BookMeta::default();
    let mut current: Option<Vec<u8>> = None;
    let mut buffer = String::new();
    let mut reader = quick_xml::Reader::from_str(xml);
    loop {
        match reader.read_event() {
            Ok(Event::Start(e)) => {
                current = Some(e.local_name().as_ref().to_vec());
                buffer.clear();
            }
            Ok(Event::Text(t)) => {
                let raw = String::from_utf8_lossy(t.as_ref()).into_owned();
                buffer.push_str(&quick_xml::escape::unescape(&raw).map(|c| c.into_owned()).unwrap_or(raw));
            }
            Ok(Event::GeneralRef(r)) => {
                let name = String::from_utf8_lossy(r.as_ref()).into_owned();
                buffer.push_str(&quick_xml::escape::unescape(&format!("&{name};")).map(|c| c.into_owned()).unwrap_or_default());
            }
            Ok(Event::End(_)) => {
                let value = buffer.trim().to_owned();
                buffer.clear();
                if value.is_empty() {
                    current = None;
                    continue;
                }
                match current.take().as_deref() {
                    Some(b"Title") => meta.title = Some(value),
                    Some(b"Series") => meta.series = Some(value),
                    Some(b"Writer") => meta.authors.extend(value.split(',').map(|s| s.trim().to_owned()).filter(|s| !s.is_empty())),
                    Some(b"Summary") => meta.overview = clean_text(&value),
                    Some(b"Year") => meta.year = year_from(&value),
                    Some(b"LanguageISO") => meta.language = Some(value),
                    Some(b"GTIN") => meta.isbn = normalize_isbn(&value),
                    _ => {}
                }
            }
            Ok(Event::Eof) | Err(_) => break,
            _ => {}
        }
    }
    if meta.title.is_none() {
        meta.title = meta.series.clone();
    }
    meta
}

/// pdf 首页渲染：优先 poppler `pdftoppm`，macOS 退回 `sips`。都不可用 → None。
pub fn render_pdf_first_page(path: &Path) -> Option<Vec<u8>> {
    let dir = std::env::temp_dir().join(format!("fbz-pdfcover-{}-{}", std::process::id(), std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_nanos()).unwrap_or(0)));
    std::fs::create_dir_all(&dir).ok()?;
    let out_base = dir.join("cover");
    let png = dir.join("cover.png");
    let ok = std::process::Command::new("pdftoppm")
        .args(["-png", "-f", "1", "-l", "1", "-singlefile", "-scale-to", &COVER_MAX_EDGE.to_string()])
        .arg(path)
        .arg(&out_base)
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .is_ok_and(|s| s.success())
        || (cfg!(target_os = "macos")
            && std::process::Command::new("sips")
                .args(["-s", "format", "png", "-Z", &COVER_MAX_EDGE.to_string()])
                .arg(path)
                .arg("--out")
                .arg(&png)
                .stdout(std::process::Stdio::null())
                .stderr(std::process::Stdio::null())
                .status()
                .is_ok_and(|s| s.success()));
    let bytes = if ok { std::fs::read(&png).ok() } else { None };
    let _ = std::fs::remove_dir_all(&dir);
    bytes
}

/// 解码→缩放→写 JPEG。返回 (storage_key, 宽, 高)；无法解码返回 None。
pub fn write_cover(media_item_id: i64, bytes: &[u8]) -> Option<(String, u32, u32)> {
    let image = ImageReader::new(Cursor::new(bytes)).with_guessed_format().ok()?.decode().ok()?;
    let image = if image.width() > COVER_MAX_EDGE || image.height() > COVER_MAX_EDGE {
        image.resize(COVER_MAX_EDGE, COVER_MAX_EDGE, FilterType::Lanczos3)
    } else {
        image
    };
    let key = cover_storage_key(media_item_id);
    let path = artwork_dir().join(&key);
    std::fs::create_dir_all(path.parent()?).ok()?;
    flatten_on_white(&image).save_with_format(&path, image::ImageFormat::Jpeg).ok()?;
    Some((key, image.width(), image.height()))
}

/// 透明背景铺白再转 RGB：macOS `sips` 渲染的 pdf 页是透明底，直接 `to_rgb8` 会变成全黑封面。
pub fn flatten_on_white(image: &image::DynamicImage) -> image::RgbImage {
    if !image.color().has_alpha() {
        return image.to_rgb8();
    }
    let rgba = image.to_rgba8();
    image::RgbImage::from_fn(rgba.width(), rgba.height(), |x, y| {
        let p = rgba.get_pixel(x, y).0;
        let a = p[3] as u32;
        let mix = |c: u8| ((c as u32 * a + 255 * (255 - a)) / 255) as u8;
        image::Rgb([mix(p[0]), mix(p[1]), mix(p[2])])
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    #[test]
    fn transparent_covers_are_flattened_on_white() {
        let mut img = image::RgbaImage::new(2, 1);
        img.put_pixel(0, 0, image::Rgba([0, 0, 0, 0]));
        img.put_pixel(1, 0, image::Rgba([10, 20, 30, 255]));
        let rgb = flatten_on_white(&image::DynamicImage::ImageRgba8(img));
        assert_eq!(rgb.get_pixel(0, 0).0, [255, 255, 255]);
        assert_eq!(rgb.get_pixel(1, 0).0, [10, 20, 30]);
    }

    #[test]
    fn opf_metadata_and_cover_variants() {
        let opf = r#"<package><metadata xmlns:dc="x">
          <dc:title>The Hobbit</dc:title><dc:creator>J. R. R. Tolkien</dc:creator><dc:creator>Second</dc:creator>
          <dc:description>&lt;p&gt;There &amp;amp; back&lt;/p&gt;</dc:description><dc:date>1937-09-21</dc:date><dc:language>en</dc:language>
          <meta name="cover" content="c1"/><meta name="calibre:series" content="Middle-earth"/></metadata>
          <manifest><item id="c1" href="../img/front.jpg" media-type="image/jpeg"/></manifest></package>"#;
        let (meta, cover) = parse_opf_metadata("OEBPS/content.opf", opf);
        assert_eq!(meta.title.as_deref(), Some("The Hobbit"));
        assert_eq!(meta.authors, vec!["J. R. R. Tolkien", "Second"]);
        assert_eq!(meta.overview.as_deref(), Some("There & back"));
        assert_eq!(meta.year, Some(1937));
        assert_eq!(meta.language.as_deref(), Some("en"));
        assert_eq!(meta.series.as_deref(), Some("Middle-earth"));
        assert_eq!(cover.as_deref(), Some("img/front.jpg"));

        let epub3 = r#"<package><metadata><dc:title>X</dc:title></metadata><manifest>
          <item id="a" href="images/a.png" media-type="image/png"/>
          <item id="b" href="images/b.png" media-type="image/png" properties="cover-image"/></manifest></package>"#;
        assert_eq!(parse_opf_metadata("content.opf", epub3).1.as_deref(), Some("images/b.png"));
        let guess = r#"<package><manifest><item id="cover-img" href="c.jpg" media-type="image/jpeg"/></manifest></package>"#;
        assert_eq!(parse_opf_metadata("content.opf", guess).1.as_deref(), Some("c.jpg"));
    }

    #[test]
    fn comic_info_fields() {
        let meta = parse_comic_info("<ComicInfo><Series>Akira</Series><Writer>Katsuhiro Otomo, Other</Writer><Summary>Neo-Tokyo</Summary><Year>1982</Year><LanguageISO>ja</LanguageISO></ComicInfo>");
        assert_eq!(meta.title.as_deref(), Some("Akira"));
        assert_eq!(meta.authors, vec!["Katsuhiro Otomo", "Other"]);
        assert_eq!(meta.year, Some(1982));
        assert_eq!(meta.overview.as_deref(), Some("Neo-Tokyo"));
    }

    fn png_bytes() -> Vec<u8> {
        let img = image::RgbImage::from_pixel(800, 1200, image::Rgb([200, 10, 10]));
        let mut out = Cursor::new(Vec::new());
        image::DynamicImage::ImageRgb8(img).write_to(&mut out, image::ImageFormat::Png).unwrap();
        out.into_inner()
    }

    #[test]
    fn cbz_cover_is_first_page_and_cover_is_resized() {
        let dir = std::env::temp_dir().join(format!("fbz-bookmeta-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("c.cbz");
        let mut zip = zip::ZipWriter::new(std::fs::File::create(&path).unwrap());
        let opts = zip::write::SimpleFileOptions::default();
        zip.start_file("p10.png", opts).unwrap();
        zip.write_all(b"not an image").unwrap();
        zip.start_file("p2.png", opts).unwrap();
        zip.write_all(&png_bytes()).unwrap();
        zip.start_file("ComicInfo.xml", opts).unwrap();
        zip.write_all(b"<ComicInfo><Title>T</Title></ComicInfo>").unwrap();
        zip.finish().unwrap();
        let meta = extract(&path);
        assert_eq!(meta.title.as_deref(), Some("T"));
        let cover = meta.cover.expect("cover");
        let _ = ARTWORK_DIR.set(dir.join("art"));
        let (key, w, h) = write_cover(42, &cover).expect("written");
        assert_eq!(key, "book-covers/42.jpg");
        assert_eq!((w, h), (400, 600));
        assert!(write_cover(43, b"garbage").is_none());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn isbn_normalization() {
        assert_eq!(normalize_isbn("urn:isbn:978-0-261-10221-7").as_deref(), Some("9780261102217"));
        assert_eq!(normalize_isbn("ISBN 0-306-40615-X").as_deref(), Some("030640615X"));
        assert_eq!(normalize_isbn("urn:uuid:1234567890123"), None);
        assert_eq!(normalize_isbn("12345"), None);
        let (meta, _) = parse_opf_metadata("c.opf", "<package><metadata><dc:identifier>urn:isbn:9780261102217</dc:identifier></metadata></package>");
        assert_eq!(meta.isbn.as_deref(), Some("9780261102217"));
    }

    #[test]
    fn strip_tags_and_year() {
        assert_eq!(strip_tags("<b>a</b>&amp;b").trim(), "a &b");
        assert_eq!(year_from("2001-01-01"), Some(2001));
        assert_eq!(year_from("n/a"), None);
    }
}
