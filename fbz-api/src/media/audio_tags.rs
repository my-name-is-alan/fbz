//! 音乐文件自带标签（ID3 / Vorbis / MP4 等）读取。
//!
//! 用 [`lofty`] 读音频文件内嵌的标签：标题、艺术家、专辑、专辑艺术家、音轨号、碟号、
//! 年份、流派。这是「读取音乐自带信息」的基础——扫描音乐时先用文件自带标签填充，
//! 再由 provider（Spotify 等）联网富化、或管理员手动修改覆盖。
//!
//! 核心是 [`AudioTags::from_path`]：路径 → 结构化标签。任何字段缺失/损坏返回 `None`，
//! 绝不 panic（坏文件不可阻断扫描）。读取本身有 IO，但字段映射逻辑纯、可单测。

use std::path::Path;

use lofty::file::TaggedFileExt;
use lofty::prelude::{Accessor, ItemKey};
use lofty::probe::Probe;
use lofty::tag::Tag;

/// 从音频文件读出的自带标签。所有字段尽力而为；无标签或读取失败全 None。
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct AudioTags {
    pub title: Option<String>,
    pub artist: Option<String>,
    pub album: Option<String>,
    pub album_artist: Option<String>,
    pub track_number: Option<u32>,
    pub disc_number: Option<u32>,
    pub year: Option<i32>,
    pub genre: Option<String>,
}

impl AudioTags {
    /// 读取音频文件的自带标签。无标签/损坏/非音频返回全 None 的默认值（不报错，不 panic）。
    pub fn from_path(path: &Path) -> AudioTags {
        let Ok(probe) = Probe::open(path) else {
            return AudioTags::default();
        };
        let Ok(tagged) = probe.read() else {
            return AudioTags::default();
        };
        // 优先主标签，无则取第一个标签；都没有返回空。
        let tag = tagged.primary_tag().or_else(|| tagged.first_tag());
        match tag {
            Some(tag) => AudioTags::from_tag(tag),
            None => AudioTags::default(),
        }
    }

    /// 从已解析的 lofty `Tag` 映射字段（纯逻辑，便于测试用内存构造的 Tag 覆盖）。
    pub fn from_tag(tag: &Tag) -> AudioTags {
        let clean = |s: Option<&str>| {
            s.map(str::trim)
                .filter(|v| !v.is_empty())
                .map(str::to_owned)
        };
        AudioTags {
            title: clean(tag.title().as_deref()),
            artist: clean(tag.artist().as_deref()),
            album: clean(tag.album().as_deref()),
            album_artist: clean(tag.get_string(ItemKey::AlbumArtist)),
            track_number: tag.track(),
            disc_number: tag.disk(),
            // 年份：lofty 0.24 规范用 RecordingDate（date() 也优先读它），Year 作兜底。
            year: clean(tag.get_string(ItemKey::RecordingDate))
                .or_else(|| clean(tag.get_string(ItemKey::Year)))
                .and_then(|s| s.get(..4).and_then(|y| y.parse::<i32>().ok())),
            genre: clean(tag.genre().as_deref()),
        }
    }

    /// 是否完全没读到任何字段（用于判断是否退化为文件名识别）。
    pub fn is_empty(&self) -> bool {
        self.title.is_none()
            && self.artist.is_none()
            && self.album.is_none()
            && self.album_artist.is_none()
            && self.track_number.is_none()
            && self.disc_number.is_none()
            && self.year.is_none()
            && self.genre.is_none()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use lofty::tag::{Tag, TagType};

    #[test]
    fn from_tag_maps_common_fields() {
        let mut tag = Tag::new(TagType::Id3v2);
        tag.set_title("Bohemian Rhapsody".to_owned());
        tag.set_artist("Queen".to_owned());
        tag.set_album("A Night at the Opera".to_owned());
        tag.set_track(11);
        tag.insert_text(ItemKey::RecordingDate, "1975".to_owned());
        tag.set_genre("Rock".to_owned());

        let tags = AudioTags::from_tag(&tag);
        assert_eq!(tags.title.as_deref(), Some("Bohemian Rhapsody"));
        assert_eq!(tags.artist.as_deref(), Some("Queen"));
        assert_eq!(tags.album.as_deref(), Some("A Night at the Opera"));
        assert_eq!(tags.track_number, Some(11));
        assert_eq!(tags.year, Some(1975));
        assert_eq!(tags.genre.as_deref(), Some("Rock"));
        assert!(!tags.is_empty());
    }

    #[test]
    fn empty_tag_yields_empty_struct() {
        let tag = Tag::new(TagType::Id3v2);
        let tags = AudioTags::from_tag(&tag);
        assert!(tags.is_empty());
        assert_eq!(tags, AudioTags::default());
    }

    #[test]
    fn blank_strings_are_filtered() {
        let mut tag = Tag::new(TagType::Id3v2);
        tag.set_title("   ".to_owned());
        tag.set_artist("Real Artist".to_owned());
        let tags = AudioTags::from_tag(&tag);
        assert_eq!(tags.title, None, "whitespace-only title filtered to None");
        assert_eq!(tags.artist.as_deref(), Some("Real Artist"));
    }

    #[test]
    fn missing_file_returns_empty_without_panic() {
        let tags = AudioTags::from_path(Path::new("/nonexistent/file.mp3"));
        assert!(tags.is_empty());
    }
}
