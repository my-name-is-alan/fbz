//! 书籍阅读端点（fbz 扩展，Jellyfin 无标准阅读 API；JF Web 前端直接下载文件后本地渲染）。
//!
//! - `GET /Books/{id}/Manifest`：格式 + epub 章节 / cbz 页数。
//! - `GET /Books/{id}/Chapters/{index}`：epub spine 第 n 章（去 `<script>` 的 body HTML + 章节内路径）。
//! - `GET /Books/{id}/Resource?Path=`：epub 内资源（图片/CSS）。
//! - `GET /Books/{id}/Pages/{index}`：cbz 第 n 页图片。
//! - `GET /Books/{id}/File`：原文件 inline（PDF 交浏览器内置阅读器）。
//!
//! 访问控制沿用播放源查询（库可见性/家长控制），与流媒体一致。

use std::{
    io::Read,
    path::{Path, PathBuf},
};

use axum::{
    Json,
    extract::{Path as AxumPath, Query, State},
    http::{HeaderMap, HeaderValue, Uri, header::CONTENT_TYPE},
    response::{IntoResponse, Response},
};
use serde::Deserialize;
use serde_json::{Value, json};

use crate::{error::AppError, media::repository::MediaRepository, state::AppState};

use super::access::authenticate_request_user;

const IMAGE_EXTS: &[&str] = &["jpg", "jpeg", "png", "gif", "webp", "bmp"];

/// 阅读用路径：mobi/azw3 → epub、cbr → cbz 的服务端转换缓存（首次访问时生成）。
async fn book_path(state: &AppState, headers: &HeaderMap, uri: &Uri, item_id: &str) -> Result<PathBuf, AppError> {
    let source = book_source_path(state, headers, uri, item_id).await?;
    if crate::media::book_convert::target_format(&source).is_none() {
        return Ok(source);
    }
    blocking(move || crate::media::book_convert::readable_path(&source)).await
}

async fn book_source_path(state: &AppState, headers: &HeaderMap, uri: &Uri, item_id: &str) -> Result<PathBuf, AppError> {
    let user = authenticate_request_user(state, headers, uri).await?;
    let Some(database) = state.database() else {
        return Err(AppError::internal("database is not configured"));
    };
    let source = MediaRepository::new(database.clone())
        .find_playback_media_source(user.id, item_id, None)
        .await
        .map_err(|err| AppError::internal(format!("failed to get book source: {err}")))?
        .ok_or_else(|| AppError::not_found("book not found"))?;
    if source.is_strm || source.path.starts_with("fbz-storage://") {
        return Err(AppError::unprocessable("book reader supports local files only"));
    }
    Ok(PathBuf::from(source.path.trim()))
}

pub fn book_format(path: &Path) -> String {
    path.extension().and_then(|e| e.to_str()).unwrap_or("").to_ascii_lowercase()
}

fn mime_for(name: &str) -> &'static str {
    match name.rsplit('.').next().unwrap_or("").to_ascii_lowercase().as_str() {
        "jpg" | "jpeg" => "image/jpeg",
        "png" => "image/png",
        "gif" => "image/gif",
        "webp" => "image/webp",
        "bmp" => "image/bmp",
        "svg" => "image/svg+xml",
        "css" => "text/css; charset=utf-8",
        "pdf" => "application/pdf",
        "epub" => "application/epub+zip",
        "cbz" => "application/vnd.comicbook+zip",
        "xhtml" | "html" | "htm" => "text/html; charset=utf-8",
        _ => "application/octet-stream",
    }
}

fn blocking<T: Send + 'static>(f: impl FnOnce() -> Result<T, String> + Send + 'static) -> impl std::future::Future<Output = Result<T, AppError>> {
    async move {
        tokio::task::spawn_blocking(f)
            .await
            .map_err(|e| AppError::internal(e.to_string()))?
            .map_err(AppError::unprocessable)
    }
}

fn read_entry(path: &Path, name: &str) -> Result<Vec<u8>, String> {
    let file = std::fs::File::open(path).map_err(|e| e.to_string())?;
    let mut zip = zip::ZipArchive::new(file).map_err(|e| e.to_string())?;
    let mut entry = zip.by_name(name).map_err(|_| format!("entry not found: {name}"))?;
    let mut buf = Vec::new();
    entry.read_to_end(&mut buf).map_err(|e| e.to_string())?;
    Ok(buf)
}

/// cbz：按文件名（自然序：数字段按数值）排序的图片条目。
pub fn cbz_pages(path: &Path) -> Result<Vec<String>, String> {
    let file = std::fs::File::open(path).map_err(|e| e.to_string())?;
    let zip = zip::ZipArchive::new(file).map_err(|e| e.to_string())?;
    let mut pages: Vec<String> = zip
        .file_names()
        .filter(|n| !n.ends_with('/') && !n.starts_with("__MACOSX"))
        .filter(|n| IMAGE_EXTS.contains(&n.rsplit('.').next().unwrap_or("").to_ascii_lowercase().as_str()))
        .map(str::to_owned)
        .collect();
    pages.sort_by_key(|n| natural_key(n));
    Ok(pages)
}

fn natural_key(name: &str) -> Vec<(u8, u64, String)> {
    let mut out = Vec::new();
    let mut digits = String::new();
    let mut text = String::new();
    for c in name.to_ascii_lowercase().chars() {
        if c.is_ascii_digit() {
            if !text.is_empty() {
                out.push((1, 0, std::mem::take(&mut text)));
            }
            digits.push(c);
        } else {
            if !digits.is_empty() {
                out.push((0, digits.parse().unwrap_or(u64::MAX), String::new()));
                digits.clear();
            }
            text.push(c);
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

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EpubChapter {
    pub href: String,
    pub title: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EpubInfo {
    pub title: Option<String>,
    pub chapters: Vec<EpubChapter>,
}

fn attr(e: &quick_xml::events::BytesStart<'_>, key: &[u8]) -> Option<String> {
    e.attributes().flatten().find(|a| a.key.local_name().as_ref() == key).and_then(|a| a.unescape_value().ok().map(|v| v.into_owned()))
}

/// 解析 container.xml 拿 OPF 路径。
pub fn epub_rootfile(container_xml: &str) -> Option<String> {
    let mut reader = quick_xml::Reader::from_str(container_xml);
    loop {
        match reader.read_event() {
            Ok(quick_xml::events::Event::Start(e) | quick_xml::events::Event::Empty(e)) if e.local_name().as_ref() == b"rootfile" => {
                return attr(&e, b"full-path");
            }
            Ok(quick_xml::events::Event::Eof) | Err(_) => return None,
            _ => {}
        }
    }
}

/// 解析 OPF：标题 + spine 顺序的章节（href 已拼接为 zip 内完整路径）。
pub fn parse_opf(opf_path: &str, opf: &str) -> EpubInfo {
    use quick_xml::events::Event;
    let base = opf_path.rsplit_once('/').map(|(d, _)| format!("{d}/")).unwrap_or_default();
    let mut manifest = std::collections::HashMap::new();
    let mut spine = Vec::new();
    let mut title = None;
    let mut in_title = false;
    let mut reader = quick_xml::Reader::from_str(opf);
    loop {
        match reader.read_event() {
            Ok(Event::Start(e)) if e.local_name().as_ref() == b"title" => in_title = title.is_none(),
            Ok(Event::Text(t)) if in_title => {
                let text = String::from_utf8_lossy(t.as_ref()).trim().to_owned();
                if !text.is_empty() {
                    title = Some(text);
                }
                in_title = false;
            }
            Ok(Event::Start(e) | Event::Empty(e)) => match e.local_name().as_ref() {
                b"item" => {
                    if let (Some(id), Some(href)) = (attr(&e, b"id"), attr(&e, b"href")) {
                        manifest.insert(id, href);
                    }
                }
                b"itemref" => {
                    if attr(&e, b"linear").as_deref() != Some("no") {
                        if let Some(idref) = attr(&e, b"idref") {
                            spine.push(idref);
                        }
                    }
                }
                _ => {}
            },
            Ok(Event::Eof) | Err(_) => break,
            _ => {}
        }
    }
    let chapters = spine
        .into_iter()
        .filter_map(|id| manifest.get(&id))
        .map(|href| EpubChapter { href: resolve_path(&base, href), title: None })
        .collect();
    EpubInfo { title, chapters }
}

/// 把相对 href 拼到目录 base（处理 `../`、`./`、去 `#fragment`、百分号解码空格）。
pub fn resolve_path(base: &str, href: &str) -> String {
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

fn epub_info(path: &Path) -> Result<EpubInfo, String> {
    let container = String::from_utf8_lossy(&read_entry(path, "META-INF/container.xml")?).into_owned();
    let opf_path = epub_rootfile(&container).ok_or("epub: rootfile missing")?;
    let opf = String::from_utf8_lossy(&read_entry(path, &opf_path)?).into_owned();
    Ok(parse_opf(&opf_path, &opf))
}

/// 取 `<body>` 内部并去掉 `<script>…</script>`（大小写不敏感）。
pub fn sanitize_chapter(html: &str) -> String {
    let lower = html.to_ascii_lowercase();
    let body = match (lower.find("<body"), lower.rfind("</body>")) {
        (Some(start), Some(end)) if end > start => {
            let open_end = lower[start..].find('>').map(|i| start + i + 1).unwrap_or(start);
            &html[open_end..end]
        }
        _ => html,
    };
    let mut out = String::with_capacity(body.len());
    let mut rest = body;
    loop {
        let l = rest.to_ascii_lowercase();
        match l.find("<script") {
            Some(i) => {
                out.push_str(&rest[..i]);
                match l[i..].find("</script>") {
                    Some(j) => rest = &rest[i + j + "</script>".len()..],
                    None => break,
                }
            }
            None => {
                out.push_str(rest);
                break;
            }
        }
    }
    out
}

pub async fn manifest(
    State(state): State<AppState>,
    AxumPath(item_id): AxumPath<String>,
    headers: HeaderMap,
    uri: Uri,
) -> Result<Json<Value>, AppError> {
    let source = book_source_path(&state, &headers, &uri, &item_id).await?;
    let source_format = book_format(&source);
    let path = book_path(&state, &headers, &uri, &item_id).await?;
    let format = book_format(&path);
    let mut body = match format.as_str() {
        "epub" => {
            let info = blocking(move || epub_info(&path)).await?;
            json!({
                "Id": item_id, "Format": "epub", "Readable": true, "Title": info.title,
                "Chapters": info.chapters.iter().enumerate().map(|(i, c)| json!({"Index": i, "Path": c.href, "Title": c.title})).collect::<Vec<_>>(),
            })
        }
        "cbz" => {
            let pages = blocking(move || cbz_pages(&path)).await?;
            json!({"Id": item_id, "Format": "cbz", "Readable": true, "PageCount": pages.len()})
        }
        "pdf" => json!({"Id": item_id, "Format": "pdf", "Readable": true}),
        other => json!({"Id": item_id, "Format": other, "Readable": false}),
    };
    body["SourceFormat"] = json!(source_format);
    body["Converted"] = json!(source_format != format);
    Ok(Json(body))
}

pub async fn chapter(
    State(state): State<AppState>,
    AxumPath((item_id, index)): AxumPath<(String, usize)>,
    headers: HeaderMap,
    uri: Uri,
) -> Result<Json<Value>, AppError> {
    let path = book_path(&state, &headers, &uri, &item_id).await?;
    if book_format(&path) != "epub" {
        return Err(AppError::unprocessable("not an epub"));
    }
    let (href, html, count) = blocking(move || {
        let info = epub_info(&path)?;
        let Some(ch) = info.chapters.get(index) else { return Ok(None) };
        let raw = String::from_utf8_lossy(&read_entry(&path, &ch.href)?).into_owned();
        Ok(Some((ch.href.clone(), sanitize_chapter(&raw), info.chapters.len())))
    })
    .await?
    .ok_or_else(|| AppError::not_found("chapter not found"))?;
    Ok(Json(json!({"Index": index, "Path": href, "ChapterCount": count, "Html": html})))
}

#[derive(Deserialize)]
pub struct ResourceQuery {
    #[serde(alias = "path")]
    #[serde(rename = "Path")]
    pub path: String,
}

pub async fn resource(
    State(state): State<AppState>,
    AxumPath(item_id): AxumPath<String>,
    Query(query): Query<ResourceQuery>,
    headers: HeaderMap,
    uri: Uri,
) -> Result<Response, AppError> {
    let path = book_path(&state, &headers, &uri, &item_id).await?;
    let name = resolve_path("", &query.path);
    let mime = mime_for(&name);
    if mime.starts_with("text/html") {
        return Err(AppError::unprocessable("use Chapters for documents"));
    }
    let bytes = blocking(move || read_entry(&path, &name)).await.map_err(|_| AppError::not_found("resource not found"))?;
    Ok(([(CONTENT_TYPE, HeaderValue::from_static(mime))], bytes).into_response())
}

pub async fn page(
    State(state): State<AppState>,
    AxumPath((item_id, index)): AxumPath<(String, usize)>,
    headers: HeaderMap,
    uri: Uri,
) -> Result<Response, AppError> {
    let path = book_path(&state, &headers, &uri, &item_id).await?;
    if book_format(&path) != "cbz" {
        return Err(AppError::unprocessable("not a cbz"));
    }
    let result = blocking(move || {
        let pages = cbz_pages(&path)?;
        let Some(name) = pages.get(index) else { return Ok(None) };
        Ok(Some((mime_for(name), read_entry(&path, name)?)))
    })
    .await?;
    let (mime, bytes) = result.ok_or_else(|| AppError::not_found("page not found"))?;
    Ok(([(CONTENT_TYPE, HeaderValue::from_static(mime))], bytes).into_response())
}

pub async fn file(
    State(state): State<AppState>,
    AxumPath(item_id): AxumPath<String>,
    headers: HeaderMap,
    uri: Uri,
) -> Result<Response, AppError> {
    // 下载/内嵌原文件（不是转换副本）。
    let path = book_source_path(&state, &headers, &uri, &item_id).await?;
    let mime = mime_for(&path.to_string_lossy());
    let bytes = tokio::fs::read(&path).await.map_err(|_| AppError::not_found("book file missing"))?;
    Ok((
        [
            (CONTENT_TYPE, HeaderValue::from_static(mime)),
            (axum::http::header::CONTENT_DISPOSITION, HeaderValue::from_static("inline")),
        ],
        bytes,
    )
        .into_response())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    fn write_zip(path: &Path, entries: &[(&str, &str)]) {
        let file = std::fs::File::create(path).unwrap();
        let mut zip = zip::ZipWriter::new(file);
        for (name, body) in entries {
            zip.start_file(*name, zip::write::SimpleFileOptions::default()).unwrap();
            zip.write_all(body.as_bytes()).unwrap();
        }
        zip.finish().unwrap();
    }

    #[test]
    fn resolve_path_handles_relative_segments() {
        assert_eq!(resolve_path("OEBPS/", "Text/ch1.xhtml#p2"), "OEBPS/Text/ch1.xhtml");
        assert_eq!(resolve_path("OEBPS/Text/", "../Images/a%20b.png"), "OEBPS/Images/a b.png");
        assert_eq!(resolve_path("", "./x.css"), "x.css");
    }

    #[test]
    fn sanitize_strips_scripts_and_keeps_body() {
        let html = "<html><head><title>t</title></head><body class=\"x\"><p>Hi</p><SCRIPT>alert(1)</SCRIPT><p>there</p></body></html>";
        assert_eq!(sanitize_chapter(html), "<p>Hi</p><p>there</p>");
    }

    #[test]
    fn natural_order_for_comic_pages() {
        let mut v = vec!["p10.jpg", "p2.jpg", "p1.jpg"];
        v.sort_by_key(|n| natural_key(n));
        assert_eq!(v, ["p1.jpg", "p2.jpg", "p10.jpg"]);
    }

    #[test]
    fn epub_and_cbz_parse_from_real_zip() {
        let dir = std::env::temp_dir().join(format!("fbz-books-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let epub = dir.join("b.epub");
        write_zip(&epub, &[
            ("mimetype", "application/epub+zip"),
            ("META-INF/container.xml", r#"<?xml version="1.0"?><container xmlns="urn:oasis:names:tc:opendocument:xmlns:container"><rootfiles><rootfile full-path="OEBPS/content.opf" media-type="application/oebps-package+xml"/></rootfiles></container>"#),
            ("OEBPS/content.opf", r#"<package xmlns="http://www.idpf.org/2007/opf" xmlns:dc="http://purl.org/dc/elements/1.1/"><metadata><dc:title>Smoke Book</dc:title></metadata><manifest><item id="c1" href="ch1.xhtml" media-type="application/xhtml+xml"/><item id="c2" href="Text/ch2.xhtml" media-type="application/xhtml+xml"/><item id="nav" href="nav.xhtml" media-type="application/xhtml+xml"/></manifest><spine><itemref idref="c1"/><itemref idref="nav" linear="no"/><itemref idref="c2"/></spine></package>"#),
            ("OEBPS/ch1.xhtml", "<html><body><p>one</p></body></html>"),
            ("OEBPS/Text/ch2.xhtml", "<html><body><p>two</p></body></html>"),
        ]);
        let info = epub_info(&epub).unwrap();
        assert_eq!(info.title.as_deref(), Some("Smoke Book"));
        let hrefs: Vec<_> = info.chapters.iter().map(|c| c.href.as_str()).collect();
        assert_eq!(hrefs, ["OEBPS/ch1.xhtml", "OEBPS/Text/ch2.xhtml"]);

        let cbz = dir.join("c.cbz");
        write_zip(&cbz, &[("10.png", "x"), ("2.png", "x"), ("notes.txt", "x"), ("__MACOSX/1.png", "x")]);
        assert_eq!(cbz_pages(&cbz).unwrap(), ["2.png", "10.png"]);
        let _ = std::fs::remove_dir_all(dir);
    }
}
