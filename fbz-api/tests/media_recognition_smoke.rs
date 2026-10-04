//! 临时端到端识别 + TMDB 联网冒烟（C:\Media 真实文件名）。测完删除。
use fbz_api::config::{MetadataConfig, ProxyConfig};
use fbz_api::media_types::LibraryType;
use fbz_api::metadata::provider::{MetadataLookup, MetadataProviderRegistry};
use fbz_api::recognition::{recognize, rules::RuleSet, types::RecognitionInput};

fn rec(stem: &str, ancestors: &[&str], lib: LibraryType) {
    let (rules, _) = RuleSet::compile(Vec::new());
    let input = RecognitionInput {
        file_stem: stem,
        extension: Some("mkv"),
        ancestors,
    };
    let r = recognize(&input, lib, &rules);
    println!("STEM: {stem}");
    println!(
        "  kind={:?} title={:?} orig={:?} year={:?} season={:?} eps={:?}",
        r.kind, r.title, r.original_title, r.year, r.season, r.episodes
    );
    println!(
        "  group={:?} res={:?} source={:?} vcodec={:?} conf={:?}",
        r.release_group,
        r.quality.resolution,
        r.quality.source,
        r.quality.video_codec,
        r.confidence
    );
}

#[test]
fn media_dir_recognition() {
    println!("\n===== MOVIE =====");
    rec("变形金刚", &["变形金刚"], LibraryType::Movies);
    println!("\n===== TV EPISODES =====");
    rec(
        "Perfect Crown.2026.S01E01.1080p.WEB-DL.AVC.AAC 2.0-DSNP@HiveWeb",
        &["Season 1", "21世纪大君夫人 (2026) {tmdb-278573}"],
        LibraryType::TvShows,
    );
    rec(
        "Perfect Crown.2026.S01E02.1080p.WEB-DL.AVC.AAC 2.0-DSNP@HiveWeb",
        &["Season 1", "21世纪大君夫人 (2026) {tmdb-278573}"],
        LibraryType::TvShows,
    );
}

fn tmdb_config(token: &str) -> MetadataConfig {
    MetadataConfig {
        providers: vec!["tmdb".to_owned()],
        tmdb_access_token: Some(token.to_owned()),
        tmdb_api_base_url: "https://api.themoviedb.org/3".to_owned(),
        tmdb_image_base_url: "https://image.tmdb.org/t/p".to_owned(),
        tvdb_api_key: None,
        tvdb_api_base_url: "https://api4.thetvdb.com/v4".to_owned(),
        fanart_api_key: None,
        fanart_api_base_url: "https://webservice.fanart.tv/v3".to_owned(),
        spotify_client_id: None,
        spotify_client_secret: None,
        spotify_api_base_url: "https://api.spotify.com/v1".to_owned(),
        spotify_auth_url: "https://accounts.spotify.com/api/token".to_owned(),
    }
}

fn proxy() -> ProxyConfig {
    ProxyConfig {
        http_proxy: None,
        https_proxy: None,
        no_proxy: Vec::new(),
        policy: "system".to_owned(),
    }
}

async fn lookup(registry: &MetadataProviderRegistry, lookup: MetadataLookup) {
    println!(
        "LOOKUP: type={} title={:?} year={:?} s={:?} e={:?}",
        lookup.item_type, lookup.title, lookup.production_year, lookup.season, lookup.episode
    );
    match registry.match_item_with_report(&lookup).await {
        Ok(report) => {
            for a in &report.attempts {
                println!("  attempt: {:?}", a);
            }
            match report.matched {
                Some(m) => {
                    println!(
                        "  MATCHED provider={} ext_id={} title(单集名)={:?} series_title(剧名)={:?} year={:?} premiere={:?} rating={:?}",
                        m.provider,
                        m.external_id,
                        m.title,
                        m.series_title,
                        m.production_year,
                        m.premiere_date,
                        m.community_rating
                    );
                    println!(
                        "    overview={:?}",
                        m.overview
                            .as_deref()
                            .map(|s| s.chars().take(50).collect::<String>())
                    );
                    println!(
                        "    artwork={} genres={} studios={} people={}",
                        m.artwork.len(),
                        m.genres.len(),
                        m.studios.len(),
                        m.people.len()
                    );
                    println!(
                        "    networks(播出平台)={:?}",
                        m.networks
                            .iter()
                            .map(|n| n.name.clone())
                            .collect::<Vec<_>>()
                    );
                    println!(
                        "    collection(系列)={:?}",
                        m.collection.as_ref().map(|c| c.name.clone())
                    );
                    println!("    videos(主题曲/宣传片)={}", m.videos.len());
                    for v in m.videos.iter().take(5) {
                        println!(
                            "      [{}] official={} {:?} {:?}",
                            v.video_type, v.is_official, v.name, v.url
                        );
                    }
                    for art in m.artwork.iter().take(3) {
                        println!(
                            "    art[{}] primary={} {}",
                            art.artwork_type, art.is_primary, art.remote_url
                        );
                    }
                }
                None => println!("  NO MATCH"),
            }
        }
        Err(e) => println!("  ERROR: {e}"),
    }
}

// 真实 TMDB 联网（需 TMDB_ACCESS_TOKEN 环境变量）。
//   cargo test --test media_recognition_smoke -- --ignored --nocapture tmdb_live
#[tokio::test]
#[ignore = "requires real TMDB_ACCESS_TOKEN and network"]
async fn tmdb_live_metadata_for_media_dir() {
    let token = std::env::var("TMDB_ACCESS_TOKEN").unwrap_or_default();
    if token.is_empty() {
        println!("TMDB_ACCESS_TOKEN not set; skipping live lookup");
        return;
    }
    let registry =
        MetadataProviderRegistry::from_config(tmdb_config(&token), proxy()).expect("registry");

    println!("\n===== MOVIE 变形金刚 (中文标题搜索) =====");
    lookup(
        &registry,
        MetadataLookup {
            item_type: "movie".to_owned(),
            title: "变形金刚".to_owned(),
            original_title: None,
            production_year: None,
            season: None,
            episode: None,
            tmdb_id: None,
            imdb_id: None,
            tvdb_id: None,
            language: Some("zh-CN".to_owned()),
            country: Some("CN".to_owned()),
            image_language: None,
            image_prefer_original: false,
            image_fallback_languages: Vec::new(),
        },
    )
    .await;

    println!("\n===== TV EPISODE Perfect Crown S01E01 (标题搜索 + episode 下钻) =====");
    lookup(
        &registry,
        MetadataLookup {
            item_type: "episode".to_owned(),
            title: "Perfect Crown".to_owned(),
            original_title: None,
            production_year: Some(2026),
            season: Some(1),
            episode: Some(1),
            tmdb_id: None,
            imdb_id: None,
            tvdb_id: None,
            language: Some("zh-CN".to_owned()),
            country: Some("CN".to_owned()),
            image_language: None,
            image_prefer_original: false,
            image_fallback_languages: Vec::new(),
        },
    )
    .await;

    println!("\n===== TV EPISODE 显式 {{tmdb-278573}} 直查 (跳过搜索) =====");
    lookup(
        &registry,
        MetadataLookup {
            item_type: "episode".to_owned(),
            // 标题故意填错，证明直查只认 id、不用标题。
            title: "WRONG TITLE SHOULD BE IGNORED".to_owned(),
            original_title: None,
            production_year: None,
            season: Some(1),
            episode: Some(1),
            tmdb_id: Some("278573".to_owned()),
            imdb_id: None,
            tvdb_id: None,
            language: Some("zh-CN".to_owned()),
            country: Some("CN".to_owned()),
            image_language: None,
            image_prefer_original: false,
            image_fallback_languages: Vec::new(),
        },
    )
    .await;
}
