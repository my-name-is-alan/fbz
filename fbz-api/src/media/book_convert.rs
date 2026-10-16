//! 服务端格式转换：让阅读器统一只处理 epub / cbz / pdf。
//!
//! - mobi / azw / azw3 / prc → epub：纯 Rust（`mobi` crate）解出 HTML 与图片记录，
//!   按 `<mbp:pagebreak>`（无则按标题/体积）切章，`recindex` / `kindle:embed` 图片引用改写为
//!   epub 内路径，首张图片标为封面。DRM 加密的文件拒绝（返回错误，不尝试破解）。
//! - cbr → cbz：调用系统 `bsdtar`（libarchive，支持 RAR4/RAR5 读取）或 `unar` 解包后重新打包。
//!
//! 产物缓存于 `ARTWORK_CACHE_DIR/book-cache/{sha256(path|size|mtime)}.{epub|cbz}`，源文件变化即失效；
//! 先写临时文件再 rename，避免并发读到半成品。

use std::{
    io::Write,
    path::{Path, PathBuf},
};

use sha2::{Digest, Sha256};

/// 需要转换的源扩展名 → 目标格式。
pub fn target_format(path: &Path) -> Option<&'static str> {
    match path.extension().and_then(|e| e.to_str()).unwrap_or("").to_ascii_lowercase().as_str() {
        "mobi" | "azw" | "azw3" | "prc" => Some("epub"),
        "cbr" => Some("cbz"),
        _ => None,
    }
}

fn cache_path(path: &Path, ext: &str) -> Result<PathBuf, String> {
    let meta = std::fs::metadata(path).map_err(|e| format!("book file missing: {e}"))?;
    let mtime = meta.modified().ok().and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok()).map_or(0, |d| d.as_secs());
    let mut hasher = Sha256::new();
    hasher.update(path.to_string_lossy().as_bytes());
    hasher.update(format!("|{}|{mtime}|v1", meta.len()).as_bytes());
    let digest = hasher.finalize();
    let name: String = digest.iter().take(16).map(|b| format!("{b:02x}")).collect();
    Ok(super::book_meta::artwork_dir().join("book-cache").join(format!("{name}.{ext}")))
}

/// 返回可直接阅读的文件：原生格式原样返回；mobi/azw3/cbr 返回（必要时生成的）缓存副本。
pub fn readable_path(path: &Path) -> Result<PathBuf, String> {
    let Some(target) = target_format(path) else {
        return Ok(path.to_path_buf());
    };
    let out = cache_path(path, target)?;
    if out.is_file() {
        return Ok(out);
    }
    std::fs::create_dir_all(out.parent().ok_or("bad cache path")?).map_err(|e| e.to_string())?;
    let tmp = out.with_extension(format!("{target}.tmp{}", std::process::id()));
    let result = if target == "epub" { mobi_to_epub(path, &tmp) } else { cbr_to_cbz(path, &tmp) };
    match result {
        Ok(()) => {
            std::fs::rename(&tmp, &out).map_err(|e| e.to_string())?;
            Ok(out)
        }
        Err(err) => {
            let _ = std::fs::remove_file(&tmp);
            Err(err)
        }
    }
}

fn image_ext(bytes: &[u8]) -> Option<&'static str> {
    if bytes.starts_with(&[0xFF, 0xD8, 0xFF]) {
        Some("jpg")
    } else if bytes.starts_with(b"\x89PNG") {
        Some("png")
    } else if bytes.starts_with(b"GIF8") {
        Some("gif")
    } else if bytes.starts_with(b"BM") {
        Some("bmp")
    } else {
        None
    }
}

fn xml_escape(s: &str) -> String {
    s.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;").replace('"', "&quot;")
}

/// KF8 `kindle:embed:XXXX` 的 base32（0-9A-V）序号 → 1 起的图片序号。
fn kindle_embed_index(code: &str) -> Option<usize> {
    let mut value = 0usize;
    for ch in code.chars() {
        let digit = ch.to_digit(32)? as usize;
        value = value.checked_mul(32)?.checked_add(digit)?;
    }
    Some(value)
}

/// 把 MOBI 图片引用改写为 `images/{n}.{ext}`：`recindex="00012"`（MOBI6）与
/// `kindle:embed:000C?mime=…`（KF8）。`names[n-1]` 为第 n 张图片的文件名。
pub fn rewrite_image_refs(html: &str, names: &[String]) -> String {
    let mut out = String::with_capacity(html.len());
    let mut rest = html;
    loop {
        let rec = rest.find("recindex=");
        let kin = rest.find("kindle:embed:");
        let (at, is_rec) = match (rec, kin) {
            (Some(a), Some(b)) if a <= b => (a, true),
            (Some(a), None) => (a, true),
            (_, Some(b)) => (b, false),
            (None, None) => break,
        };
        out.push_str(&rest[..at]);
        if is_rec {
            let after = &rest[at + "recindex=".len()..];
            let quote = after.chars().next().filter(|c| *c == '"' || *c == '\'');
            let body = quote.map_or(after, |q| &after[q.len_utf8()..]);
            let digits: String = body.chars().take_while(char::is_ascii_digit).collect();
            let consumed = "recindex=".len() + quote.map_or(0, |q| q.len_utf8() * 2) + digits.len();
            match digits.parse::<usize>().ok().and_then(|n| names.get(n.wrapping_sub(1))) {
                Some(name) => out.push_str(&format!("src=\"images/{name}\"")),
                None => out.push_str(&rest[at..at + consumed.min(rest.len() - at)]),
            }
            rest = &rest[(at + consumed).min(rest.len())..];
        } else {
            let after = &rest[at + "kindle:embed:".len()..];
            let code: String = after.chars().take_while(char::is_ascii_alphanumeric).collect();
            let mut consumed = "kindle:embed:".len() + code.len();
            // 去掉 `?mime=image/jpg` 之类查询串（到引号为止）。
            if rest[at + consumed..].starts_with('?') {
                consumed += rest[at + consumed..].find(['"', '\'', ')']).unwrap_or(0);
            }
            match kindle_embed_index(&code).and_then(|n| names.get(n.wrapping_sub(1))) {
                Some(name) => out.push_str(&format!("images/{name}")),
                None => out.push_str(&rest[at..at + consumed]),
            }
            rest = &rest[at + consumed..];
        }
    }
    out.push_str(rest);
    out
}

/// 切章：优先 `<mbp:pagebreak`；只有一段且很长时按 `<h1`/`<h2` 切；空章丢弃。
pub fn split_chapters(html: &str) -> Vec<String> {
    let body = {
        let lower = html.to_ascii_lowercase();
        let start = lower.find("<body").and_then(|i| lower[i..].find('>').map(|j| i + j + 1)).unwrap_or(0);
        let end = lower.rfind("</body>").filter(|e| *e >= start).unwrap_or(html.len());
        &html[start..end]
    };
    let lower = body.to_ascii_lowercase();
    let mut cuts: Vec<usize> = lower.match_indices("<mbp:pagebreak").map(|(i, _)| i).collect();
    if cuts.is_empty() && body.len() > 100_000 {
        cuts = lower.match_indices("<h1").chain(lower.match_indices("<h2")).map(|(i, _)| i).filter(|i| *i > 0).collect();
        cuts.sort_unstable();
    }
    let mut chapters = Vec::new();
    let mut prev = 0;
    for cut in cuts.into_iter().chain(std::iter::once(body.len())) {
        let mut piece = &body[prev..cut];
        if let Some(end) = piece.find('>').filter(|_| piece.to_ascii_lowercase().starts_with("<mbp:pagebreak")) {
            piece = &piece[end + 1..];
        }
        let text = super::book_meta::strip_tags(piece);
        if !text.trim().is_empty() || piece.to_ascii_lowercase().contains("<img") {
            chapters.push(piece.to_owned());
        }
        prev = cut;
    }
    if chapters.is_empty() {
        chapters.push(body.to_owned());
    }
    chapters
}

/// 修正 MOBI HTML 中 XHTML 不认的写法（`<br>` 等空元素未闭合），保证 epub 阅读端 DOMParser 可解析。
fn xhtml_fix(html: &str) -> String {
    let mut out = html.to_owned();
    for tag in ["br", "hr"] {
        out = out.replace(&format!("<{tag}>"), &format!("<{tag}/>")).replace(&format!("<{}>", tag.to_uppercase()), &format!("<{tag}/>"));
    }
    // `<img ...>` → `<img .../>`
    let mut fixed = String::with_capacity(out.len() + 16);
    let mut rest = out.as_str();
    while let Some(pos) = rest.to_ascii_lowercase().find("<img") {
        let Some(end) = rest[pos..].find('>').map(|e| pos + e) else { break };
        fixed.push_str(&rest[..end]);
        if !rest[..end].ends_with('/') {
            fixed.push('/');
        }
        fixed.push('>');
        rest = &rest[end + 1..];
    }
    fixed.push_str(rest);
    fixed
}

/// mobi 0.8 的 `RawRecords::range` 对排他上界多减 1，会丢掉最后一个正文记录（单记录书正文全空），
/// 且不剥离 trailing entries；这里自行按 PalmDOC 头取正文记录、剥尾、解压。Huff/CDIC 仍交给 crate。
fn mobi_text(book: &mobi::Mobi, raw: &[u8]) -> String {
    // 注意：crate 的 `Mobi.content` 前段（record0）被零填充，必须用原始文件字节。
    let offsets: Vec<usize> = book.metadata.records.records.iter().map(|r| r.offset as usize).collect();
    let record = |i: usize| -> &[u8] {
        let start = offsets.get(i).copied().unwrap_or(raw.len()).min(raw.len());
        let end = offsets.get(i + 1).copied().unwrap_or(raw.len()).clamp(start, raw.len());
        &raw[start..end]
    };
    let rec0 = record(0);
    let compression = u16::from_be_bytes([rec0.first().copied().unwrap_or(0), rec0.get(1).copied().unwrap_or(0)]);
    if compression != 1 && compression != 2 {
        return book.content_as_string_lossy();
    }
    let text_count = rec0.get(8..10).map_or(0, |b| u16::from_be_bytes([b[0], b[1]]) as usize);
    let header_len = rec0.get(20..24).map_or(0, |b| u32::from_be_bytes([b[0], b[1], b[2], b[3]]));
    let flags = if header_len >= 0xE4 { rec0.get(240..244).map_or(0, |b| u32::from_be_bytes([b[0], b[1], b[2], b[3]])) } else { 0 };
    let mut bytes = Vec::new();
    for i in 1..=text_count.min(offsets.len().saturating_sub(1)) {
        let data = strip_trailing_entries(record(i), flags);
        if compression == 2 { bytes.extend(palmdoc_decompress(data)) } else { bytes.extend_from_slice(data) }
    }
    if matches!(book.text_encoding(), mobi::headers::TextEncoding::CP1252) {
        bytes.iter().map(|b| if *b < 0x80 { *b as char } else { char::from_u32(*b as u32).unwrap_or('?') }).collect()
    } else {
        String::from_utf8_lossy(&bytes).into_owned()
    }
}

fn strip_trailing_entries(data: &[u8], flags: u32) -> &[u8] {
    let mut size = data.len();
    let mut f = flags >> 1;
    while f != 0 {
        if f & 1 == 1 {
            let (mut bitpos, mut result, mut p) = (0u32, 0usize, size);
            while p > 0 {
                let v = data[p - 1];
                result |= ((v & 0x7F) as usize) << bitpos;
                bitpos += 7;
                p -= 1;
                if v & 0x80 != 0 || bitpos >= 28 { break }
            }
            size = size.saturating_sub(result);
        }
        f >>= 1;
    }
    if flags & 1 == 1 && size > 0 {
        size = size.saturating_sub(((data[size - 1] & 3) + 1) as usize);
    }
    &data[..size]
}

fn palmdoc_decompress(data: &[u8]) -> Vec<u8> {
    let mut out: Vec<u8> = Vec::with_capacity(data.len() * 2);
    let mut i = 0;
    while i < data.len() {
        let c = data[i];
        i += 1;
        match c {
            1..=8 => {
                let end = (i + c as usize).min(data.len());
                out.extend_from_slice(&data[i..end]);
                i = end;
            }
            0x80..=0xBF => {
                let Some(&c2) = data.get(i) else { break };
                i += 1;
                let m = (((c as usize) << 8) | c2 as usize) & 0x3FFF;
                let (dist, n) = (m >> 3, (m & 7) + 3);
                if dist == 0 || dist > out.len() { continue }
                for _ in 0..n { out.push(out[out.len() - dist]) }
            }
            0xC0..=0xFF => { out.push(b' '); out.push(c ^ 0x80) }
            _ => out.push(c),
        }
    }
    out
}

pub fn mobi_to_epub(src: &Path, dst: &Path) -> Result<(), String> {
    let book = mobi::Mobi::from_path(src).map_err(|e| format!("mobi: {e}"))?;
    if !matches!(book.encryption(), mobi::headers::Encryption::No) {
        return Err("mobi: DRM-protected file cannot be converted".to_owned());
    }
    let raw = std::fs::read(src).map_err(|e| e.to_string())?;
    let html = mobi_text(&book, &raw);
    let mut images = Vec::new();
    let mut names = Vec::new();
    for (i, record) in book.image_records().iter().enumerate() {
        let ext = image_ext(record.content).unwrap_or("bin");
        names.push(format!("{:05}.{ext}", i + 1));
        images.push(record.content.to_vec());
    }
    let html = rewrite_image_refs(&html, &names);
    let chapters = split_chapters(&html);

    let file = std::fs::File::create(dst).map_err(|e| e.to_string())?;
    let mut zip = zip::ZipWriter::new(file);
    let stored = zip::write::SimpleFileOptions::default().compression_method(zip::CompressionMethod::Stored);
    let deflated = zip::write::SimpleFileOptions::default();
    let w = |zip: &mut zip::ZipWriter<std::fs::File>, name: &str, data: &[u8], opts| -> Result<(), String> {
        zip.start_file(name, opts).map_err(|e| e.to_string())?;
        zip.write_all(data).map_err(|e| e.to_string())
    };
    w(&mut zip, "mimetype", b"application/epub+zip", stored)?;
    w(&mut zip, "META-INF/container.xml", br#"<?xml version="1.0"?><container version="1.0" xmlns="urn:oasis:names:tc:opendocument:xmlns:container"><rootfiles><rootfile full-path="OEBPS/content.opf" media-type="application/oebps-package+xml"/></rootfiles></container>"#, deflated)?;
    let mut manifest = String::new();
    let mut spine = String::new();
    for (i, chapter) in chapters.iter().enumerate() {
        let name = format!("ch{:04}.xhtml", i + 1);
        let doc = format!("<?xml version=\"1.0\" encoding=\"utf-8\"?><html xmlns=\"http://www.w3.org/1999/xhtml\"><head><title>{}</title></head><body>{}</body></html>", i + 1, xhtml_fix(chapter));
        w(&mut zip, &format!("OEBPS/{name}"), doc.as_bytes(), deflated)?;
        manifest.push_str(&format!("<item id=\"c{i}\" href=\"{name}\" media-type=\"application/xhtml+xml\"/>"));
        spine.push_str(&format!("<itemref idref=\"c{i}\"/>"));
    }
    for (i, (name, data)) in names.iter().zip(&images).enumerate() {
        w(&mut zip, &format!("OEBPS/images/{name}"), data, stored)?;
        let mime = match name.rsplit('.').next() {
            Some("jpg") => "image/jpeg",
            Some("png") => "image/png",
            Some("gif") => "image/gif",
            Some("bmp") => "image/bmp",
            _ => "application/octet-stream",
        };
        let props = if i == 0 { " properties=\"cover-image\"" } else { "" };
        manifest.push_str(&format!("<item id=\"img{i}\" href=\"images/{name}\" media-type=\"{mime}\"{props}/>"));
    }
    let mut meta = format!("<dc:title>{}</dc:title>", xml_escape(book.title().trim()));
    if let Some(author) = book.author().filter(|a| !a.trim().is_empty()) {
        for name in author.split(['&', ';']).map(str::trim).filter(|s| !s.is_empty()) {
            meta.push_str(&format!("<dc:creator>{}</dc:creator>", xml_escape(name)));
        }
    }
    if let Some(d) = book.description().filter(|d| !d.trim().is_empty()) {
        meta.push_str(&format!("<dc:description>{}</dc:description>", xml_escape(&d)));
    }
    if let Some(d) = book.publish_date().filter(|d| !d.trim().is_empty()) {
        meta.push_str(&format!("<dc:date>{}</dc:date>", xml_escape(&d)));
    }
    if let Some(isbn) = book.isbn().filter(|d| !d.trim().is_empty()) {
        meta.push_str(&format!("<dc:identifier>urn:isbn:{}</dc:identifier>", xml_escape(&isbn)));
    }
    if !names.is_empty() {
        meta.push_str("<meta name=\"cover\" content=\"img0\"/>");
    }
    let opf = format!("<?xml version=\"1.0\" encoding=\"utf-8\"?><package xmlns=\"http://www.idpf.org/2007/opf\" version=\"2.0\"><metadata xmlns:dc=\"http://purl.org/dc/elements/1.1/\">{meta}</metadata><manifest>{manifest}</manifest><spine>{spine}</spine></package>");
    w(&mut zip, "OEBPS/content.opf", opf.as_bytes(), deflated)?;
    zip.finish().map_err(|e| e.to_string())?;
    Ok(())
}

pub fn cbr_to_cbz(src: &Path, dst: &Path) -> Result<(), String> {
    let dir = std::env::temp_dir().join(format!(
        "fbz-cbr-{}-{}",
        std::process::id(),
        std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_nanos())
    ));
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    let quiet = || (std::process::Stdio::null(), std::process::Stdio::null());
    let (o1, e1) = quiet();
    let ok = std::process::Command::new("bsdtar").arg("-xf").arg(src).arg("-C").arg(&dir).stdout(o1).stderr(e1).status().is_ok_and(|s| s.success())
        || {
            let (o2, e2) = quiet();
            std::process::Command::new("unar").args(["-q", "-f", "-o"]).arg(&dir).arg(src).stdout(o2).stderr(e2).status().is_ok_and(|s| s.success())
        };
    let result = if ok { zip_dir_images(&dir, dst) } else { Err("cbr: no extractor (bsdtar/unar) could read this archive".to_owned()) };
    let _ = std::fs::remove_dir_all(&dir);
    result
}

fn zip_dir_images(dir: &Path, dst: &Path) -> Result<(), String> {
    let mut files = Vec::new();
    let mut stack = vec![dir.to_path_buf()];
    while let Some(d) = stack.pop() {
        for entry in std::fs::read_dir(&d).map_err(|e| e.to_string())?.flatten() {
            let p = entry.path();
            // 不跟随符号链接，防止压缩包内链接指向包外文件。
            let Ok(ft) = entry.file_type() else { continue };
            if ft.is_dir() {
                stack.push(p);
            } else if ft.is_file() {
                files.push(p);
            }
        }
    }
    let file = std::fs::File::create(dst).map_err(|e| e.to_string())?;
    let mut zip = zip::ZipWriter::new(file);
    let opts = zip::write::SimpleFileOptions::default().compression_method(zip::CompressionMethod::Stored);
    let mut count = 0;
    for p in files {
        let rel = p.strip_prefix(dir).map_err(|e| e.to_string())?.to_string_lossy().replace('\\', "/");
        let ext = rel.rsplit('.').next().unwrap_or("").to_ascii_lowercase();
        if !["jpg", "jpeg", "png", "gif", "webp", "bmp"].contains(&ext.as_str()) && !rel.eq_ignore_ascii_case("ComicInfo.xml") {
            continue;
        }
        zip.start_file(rel, opts).map_err(|e| e.to_string())?;
        zip.write_all(&std::fs::read(&p).map_err(|e| e.to_string())?).map_err(|e| e.to_string())?;
        count += 1;
    }
    zip.finish().map_err(|e| e.to_string())?;
    if count == 0 {
        return Err("cbr: archive contains no images".to_owned());
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rewrites_mobi6_and_kf8_image_refs() {
        let names = vec!["00001.jpg".to_owned(), "00002.png".to_owned()];
        assert_eq!(rewrite_image_refs(r#"<img recindex="00002" />"#, &names), r#"<img src="images/00002.png" />"#);
        assert_eq!(rewrite_image_refs(r#"<img src="kindle:embed:0001?mime=image/jpg"/>"#, &names), r#"<img src="images/00001.jpg"/>"#);
        // 越界保持原样。
        assert_eq!(rewrite_image_refs(r#"<img recindex="00009">"#, &names), r#"<img recindex="00009">"#);
        assert_eq!(kindle_embed_index("000V"), Some(31));
    }

    #[test]
    fn splits_on_pagebreaks_and_drops_empty() {
        let html = "<html><body><p>One</p><mbp:pagebreak/><p>Two</p><mbp:pagebreak /><mbp:pagebreak/></body></html>";
        assert_eq!(split_chapters(html), vec!["<p>One</p>".to_owned(), "<p>Two</p>".to_owned()]);
        assert_eq!(split_chapters("<p>Only</p>"), vec!["<p>Only</p>".to_owned()]);
    }

    #[test]
    fn mobi_converts_to_readable_epub_with_metadata_and_cover() {
        let dir = std::env::temp_dir().join(format!("fbz-mobitest-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let src = dir.join("s.mobi");
        std::fs::write(&src, include_bytes!("testdata/smoke.mobi")).unwrap();
        let out = dir.join("s.epub");
        mobi_to_epub(&src, &out).unwrap();
        let meta = super::super::book_meta::extract(&out);
        assert_eq!(meta.title.as_deref(), Some("Smoke Mobi"));
        assert_eq!(meta.authors, vec!["Mo Bi Author"]);
        assert_eq!(meta.year, Some(2015));
        assert_eq!(meta.isbn.as_deref(), Some("9780306406157"));
        assert!(meta.overview.as_deref().is_some_and(|o| o.contains("tiny MOBI")));
        assert!(meta.cover.as_ref().is_some_and(|c| c.starts_with(b"\x89PNG")));
        let mut zip = zip::ZipArchive::new(std::fs::File::open(&out).unwrap()).unwrap();
        let mut ch1 = String::new();
        std::io::Read::read_to_string(&mut zip.by_name("OEBPS/ch0001.xhtml").unwrap(), &mut ch1).unwrap();
        assert!(ch1.contains("Part One"), "{ch1}");
        assert!(ch1.contains(r#"src="images/00001.png""#), "{ch1}");
        let mut ch2 = String::new();
        std::io::Read::read_to_string(&mut zip.by_name("OEBPS/ch0002.xhtml").unwrap(), &mut ch2).unwrap();
        assert!(ch2.contains("Part Two") && ch2.contains("<br/>"));
        assert!(ch1.contains(r#"<img src="images/00001.png"/>"#), "{ch1}");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn palmdoc_and_trailing_entries() {
        // 'ab' + 后向引用 dist=2 len=3 -> "ababa"；0xC1 -> " A"
        let m: u16 = 0x8000 | (2 << 3) | 0;
        let data = [b'a', b'b', (m >> 8) as u8, m as u8, 0xC1];
        assert_eq!(palmdoc_decompress(&data), b"ababa A");
        // flags=0b11：一个 2 字节 trailing entry（大小编码 0x82），再加 multibyte 1+1 字节
        let rec = [b'x', b'y', b'z', 0x00, b'q', 0x82];
        assert_eq!(strip_trailing_entries(&rec, 0b11), b"xyz");
    }

    #[test]
    fn target_formats() {
        assert_eq!(target_format(Path::new("a.AZW3")), Some("epub"));
        assert_eq!(target_format(Path::new("a.cbr")), Some("cbz"));
        assert_eq!(target_format(Path::new("a.epub")), None);
    }

    #[test]
    fn cbr_via_bsdtar_roundtrip_when_available() {
        // 用 bsdtar 造一个 zip 格式的“cbr”（很多 cbr 实际是 zip）验证解包重打包路径。
        if std::process::Command::new("bsdtar").arg("--version").output().is_err() {
            return;
        }
        let dir = std::env::temp_dir().join(format!("fbz-cbrtest-{}", std::process::id()));
        std::fs::create_dir_all(dir.join("in")).unwrap();
        std::fs::write(dir.join("in/p1.png"), b"\x89PNGfake").unwrap();
        std::fs::write(dir.join("in/notes.txt"), b"x").unwrap();
        let cbr = dir.join("c.cbr");
        let status = std::process::Command::new("bsdtar").arg("-a").arg("-cf").arg(&cbr).arg("-C").arg(dir.join("in")).arg("p1.png").arg("notes.txt").status().unwrap();
        assert!(status.success());
        let out = dir.join("c.cbz");
        cbr_to_cbz(&cbr, &out).unwrap();
        let zip = zip::ZipArchive::new(std::fs::File::open(&out).unwrap()).unwrap();
        assert_eq!(zip.file_names().collect::<Vec<_>>(), vec!["p1.png"]);
        let _ = std::fs::remove_dir_all(&dir);
    }
}
