use crate::error::AppError;
use quick_xml::{Reader, events::Event};
use serde::Deserialize;

#[derive(Debug, Default, Deserialize)]
pub struct Nfo {
    #[serde(default)]
    pub title: String,
    #[serde(default)]
    pub originaltitle: String,
    #[serde(default)]
    pub plot: String,
    pub year: Option<i32>,
    pub season: Option<i32>,
    pub episode: Option<i32>,
    pub runtime: Option<f64>,
    #[serde(default)]
    pub uniqueid: Vec<UniqueId>,
    #[serde(default)]
    pub genre: Vec<String>,
}
#[derive(Debug, Deserialize)]
pub struct UniqueId {
    #[serde(rename = "@type")]
    pub kind: String,
    #[serde(rename = "$text")]
    pub value: String,
}
/// Fall back to the conventional SxxExx filename only when episode NFO omits it.
pub fn episode_numbers(name: &str) -> Option<(i32, i32)> {
    let bytes = name.as_bytes();
    for start in 0..bytes.len() {
        if !bytes[start].eq_ignore_ascii_case(&b's') {
            continue;
        }
        let mut middle = start + 1;
        while middle < bytes.len() && bytes[middle].is_ascii_digit() {
            middle += 1;
        }
        if middle == start + 1
            || middle >= bytes.len()
            || !bytes[middle].eq_ignore_ascii_case(&b'e')
        {
            continue;
        }
        let mut end = middle + 1;
        while end < bytes.len() && bytes[end].is_ascii_digit() {
            end += 1;
        }
        if end == middle + 1 {
            continue;
        }
        let season: i32 = std::str::from_utf8(&bytes[start + 1..middle])
            .ok()?
            .parse()
            .ok()?;
        let episode: i32 = std::str::from_utf8(&bytes[middle + 1..end])
            .ok()?
            .parse()
            .ok()?;
        if season <= 999 && episode <= 9999 {
            return Some((season, episode));
        }
    }
    None
}
pub fn parse(bytes: &[u8]) -> Result<Nfo, AppError> {
    if bytes.len() > 2 * 1024 * 1024 {
        return Err(AppError::unprocessable("NFO 超过 2 MiB"));
    }
    let mut reader = Reader::from_reader(bytes);
    let mut depth = 0;
    loop {
        match reader.read_event() {
            Ok(Event::DocType(_)) => {
                return Err(AppError::unprocessable("NFO 不允许 DTD 或外部实体"));
            }
            Ok(Event::Start(_)) => {
                depth += 1;
                if depth > 32 {
                    return Err(AppError::unprocessable("NFO 嵌套过深"));
                }
            }
            Ok(Event::End(_)) => depth -= 1,
            Ok(Event::Eof) => break,
            Err(_) => return Err(AppError::unprocessable("NFO XML 格式错误")),
            _ => {}
        }
    }
    let nfo: Nfo = quick_xml::de::from_reader(bytes)
        .map_err(|_| AppError::unprocessable("NFO 字段格式错误"))?;
    if nfo.title.len() > 2048
        || nfo.plot.len() > 512 * 1024
        || nfo
            .runtime
            .is_some_and(|v| !v.is_finite() || !(0.0..=100000.0).contains(&v))
    {
        return Err(AppError::unprocessable("NFO 字段超出限制"));
    }
    Ok(nfo)
}
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_unicode_episode_file_names_without_byte_boundary_panics() {
        assert_eq!(
            episode_numbers("틈만나면,.2024.S01E02.2160p.mkv"),
            Some((1, 2))
        );
        assert_eq!(episode_numbers("剧名.s03e105.mp4"), Some((3, 105)));
        assert_eq!(episode_numbers("Movie.2024.2160p.mkv"), None);
    }
    #[test]
    fn reads_scraped_movie_and_episode() {
        let n=parse(br#"<episodedetails><title>Episode</title><season>2</season><episode>3</episode><runtime>42</runtime><uniqueid type="tmdb">123</uniqueid></episodedetails>"#).unwrap();
        assert_eq!(n.season, Some(2));
        assert_eq!(n.episode, Some(3));
        assert_eq!(n.uniqueid[0].value, "123");
    }
    #[test]
    fn rejects_external_entities() {
        assert!(parse(br#"<!DOCTYPE movie SYSTEM "file:///etc/passwd"><movie/>"#).is_err());
    }
    #[test]
    fn handles_escaped_titles() {
        assert_eq!(
            parse(b"<movie><title>A &amp; B</title></movie>")
                .unwrap()
                .title,
            "A & B"
        );
    }
}
