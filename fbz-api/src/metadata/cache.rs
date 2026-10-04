//! metadata 查询响应缓存（provider 联网编排收尾）。
//!
//! `metadata.refresh` 每次都联网查 provider；同一逻辑条目（相同标题/年份/季集/语言）
//! 重复刷新会重复打 TMDB/TVDB。本模块在 service 层加一个**内存 TTL 缓存**：相同查询
//! 在 TTL 内复用上次结果，命中即跳过联网。正/负结果都缓存（负缓存避免反复查无果的条目）。
//!
//! 缓存键是 [`MetadataLookup`] 关键字段的纯函数（可穷举单测）；TTL 过期判断用可注入
//! 时钟测。进程内缓存——多节点各自持有，重启即失效（provider 结果幂等，重查无副作用）。

use std::collections::HashMap;
use std::sync::Mutex;
use std::time::{Duration, Instant};

use super::provider::shared::{MetadataLookup, MetadataMatch};

/// 缓存键：把查询的语义关键字段拼成稳定字符串。纯函数。
/// 图片语言策略不入键——它只影响 artwork 选择，不影响「匹配到哪个条目」，
/// 且把它纳入会大幅降低命中率。语言/区域入键（影响 provider 返回的本地化标题）。
pub fn cache_key(lookup: &MetadataLookup) -> String {
    // 用不可能出现在字段值里的分隔符；None 用空串占位。
    format!(
        "{}\u{1f}{}\u{1f}{}\u{1f}{}\u{1f}{}\u{1f}{}\u{1f}{}",
        lookup.item_type,
        lookup.title.trim().to_lowercase(),
        lookup
            .production_year
            .map(|y| y.to_string())
            .unwrap_or_default(),
        lookup.season.map(|s| s.to_string()).unwrap_or_default(),
        lookup.episode.map(|e| e.to_string()).unwrap_or_default(),
        lookup.language.as_deref().unwrap_or(""),
        lookup.country.as_deref().unwrap_or(""),
    )
}

/// 缓存的查询结果：匹配到的元数据（`None` = 负缓存，查无结果）。
#[derive(Clone, Debug)]
pub struct CachedLookup {
    pub matched: Option<MetadataMatch>,
}

struct Entry {
    matched: Option<MetadataMatch>,
    expires_at: Instant,
}

/// 内存 TTL 响应缓存。线程安全，service 持 `Arc` 共享。
pub struct MetadataCache {
    ttl: Duration,
    entries: Mutex<HashMap<String, Entry>>,
}

impl MetadataCache {
    pub fn new(ttl: Duration) -> Self {
        Self {
            ttl,
            entries: Mutex::new(HashMap::new()),
        }
    }

    /// 取缓存（未过期才返回）。可注入时钟版本。
    pub fn get_at(&self, key: &str, now: Instant) -> Option<CachedLookup> {
        let entries = self.entries.lock().expect("metadata cache mutex poisoned");
        let entry = entries.get(key)?;
        if entry.expires_at <= now {
            return None; // 过期：当作未命中（惰性，下次 put 覆盖）。
        }
        Some(CachedLookup {
            matched: entry.matched.clone(),
        })
    }

    /// 写缓存（正/负结果都存），过期时刻 = now + ttl。可注入时钟版本。
    pub fn put_at(&self, key: String, matched: Option<MetadataMatch>, now: Instant) {
        let mut entries = self.entries.lock().expect("metadata cache mutex poisoned");
        entries.insert(
            key,
            Entry {
                matched,
                expires_at: now + self.ttl,
            },
        );
    }

    /// 生产用：当前时钟取缓存。
    pub fn get(&self, key: &str) -> Option<CachedLookup> {
        self.get_at(key, Instant::now())
    }

    /// 生产用：当前时钟写缓存。
    pub fn put(&self, key: String, matched: Option<MetadataMatch>) {
        self.put_at(key, matched, Instant::now());
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn lookup(item_type: &str, title: &str) -> MetadataLookup {
        MetadataLookup {
            item_type: item_type.to_owned(),
            title: title.to_owned(),
            original_title: None,
            production_year: Some(2008),
            season: None,
            episode: None,
            tmdb_id: None,
            imdb_id: None,
            tvdb_id: None,
            language: Some("zh-CN".to_owned()),
            country: None,
            image_language: None,
            image_prefer_original: false,
            image_fallback_languages: Vec::new(),
        }
    }

    #[test]
    fn cache_key_is_stable_and_distinguishes_fields() {
        let a = lookup("movie", "Inception");
        // 同字段（标题大小写无关）→ 同键。
        let mut a2 = lookup("movie", "  inception ");
        a2.image_prefer_original = true; // 图片策略不入键。
        assert_eq!(cache_key(&a), cache_key(&a2));

        // 年份不同 → 不同键。
        let mut b = lookup("movie", "Inception");
        b.production_year = Some(2010);
        assert_ne!(cache_key(&a), cache_key(&b));

        // 季集不同 → 不同键（剧集去重靠这个）。
        let mut ep1 = lookup("episode", "Breaking Bad");
        ep1.season = Some(1);
        ep1.episode = Some(1);
        let mut ep2 = lookup("episode", "Breaking Bad");
        ep2.season = Some(1);
        ep2.episode = Some(2);
        assert_ne!(cache_key(&ep1), cache_key(&ep2));

        // 语言不同 → 不同键（本地化标题不同）。
        let mut en = lookup("movie", "Inception");
        en.language = Some("en-US".to_owned());
        assert_ne!(cache_key(&a), cache_key(&en));
    }

    #[test]
    fn put_then_get_within_ttl_hits() {
        let cache = MetadataCache::new(Duration::from_secs(60));
        let t0 = Instant::now();
        cache.put_at("k".to_owned(), None, t0);
        // TTL 内命中（负缓存：matched None 也算命中）。
        let hit = cache.get_at("k", t0 + Duration::from_secs(30));
        assert!(hit.is_some());
        assert!(hit.unwrap().matched.is_none());
    }

    #[test]
    fn get_after_ttl_misses() {
        let cache = MetadataCache::new(Duration::from_secs(60));
        let t0 = Instant::now();
        cache.put_at("k".to_owned(), None, t0);
        // 超过 TTL → 未命中。
        assert!(cache.get_at("k", t0 + Duration::from_secs(61)).is_none());
    }

    #[test]
    fn missing_key_misses() {
        let cache = MetadataCache::new(Duration::from_secs(60));
        assert!(cache.get_at("absent", Instant::now()).is_none());
    }
}
