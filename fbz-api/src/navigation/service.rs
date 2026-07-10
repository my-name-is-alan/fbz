//! 导航 BFF 的聚合服务。
//!
//! 一次性拉齐首屏所需数据：可见库列表、最新入库、继续观看，并把后端 record 映射为对齐
//! 前端的 `camelCase` DTO。业务逻辑/SQL 仍下沉在 [`LibraryRepository`]，本层只做编排与映射。

use std::collections::HashMap;

use crate::{
    auth::service::AuthenticatedUser,
    db::DbPool,
    library::repository::{
        BrowseItemsInput, ItemQualityTags, ItemQueryOptions, ItemSortField, ItemTypeFilter,
        LibraryRepository, MediaItemBrowseRecord, MediaQueryInput, SortDirection,
        UserLibraryViewRecord,
    },
    media_types::LibraryType,
    navigation::dto::{
        FeaturedItemDto, MediaItemDto, NavigationDto, NavigationLibraryDto, NavigationSectionDto,
        NavigationUserDto,
    },
};

/// 首页各行默认拉取的条目数。
const SECTION_LIMIT: i64 = 16;
/// hero 主打项数量（取自最新入库前几条）。
const FEATURED_LIMIT: usize = 5;
/// 「最新入库」与 hero 只展示影片/剧集（内部小写词汇，对齐 `media_items.item_type`）。
const FEATURED_ITEM_TYPES: [&str; 2] = ["movie", "series"];

/// 聚合当前用户的首屏导航数据。
pub async fn load_navigation(
    database: DbPool,
    user: &AuthenticatedUser,
) -> Result<NavigationDto, sqlx::Error> {
    let repository = LibraryRepository::new(database);

    let views = repository.list_user_views(user.id).await?;
    let view_counts = repository.list_user_view_counts(user.id).await?;
    let counts_by_library: HashMap<String, u32> = view_counts
        .into_iter()
        .map(|record| (record.library_id, record.item_count.max(0) as u32))
        .collect();
    // 「最新入库」走浏览路径而非 list_latest_items：后者把 image_tags 硬编码为空，
    // 浏览路径支持 include_image_tags=true，从 item_images 真实聚合，海报才能渲染。
    let latest = repository
        .list_user_items(latest_browse_query(user.id))
        .await?;
    let resume = repository
        .list_resume_items(media_query(
            user.id,
            SECTION_LIMIT,
            ItemSortField::DateCreated,
        ))
        .await?;

    let mut sections = Vec::new();
    if !resume.items.is_empty() {
        sections.push(NavigationSectionDto {
            key: "continue".to_owned(),
            title: "继续观看".to_owned(),
            layout: "wide".to_owned(),
            to: None,
            items: resume.items.into_iter().map(media_item_to_dto).collect(),
        });
    }

    // hero 主打项取自最新入库前几条；overview 与画质标签不在浏览记录里，按 id 批量补查。
    let featured_records: Vec<MediaItemBrowseRecord> =
        latest.items.iter().take(FEATURED_LIMIT).cloned().collect();
    let featured_ids: Vec<String> = featured_records
        .iter()
        .map(|record| record.id.clone())
        .collect();
    let mut overviews = repository.fetch_item_overviews(&featured_ids).await?;
    let mut quality_tags = repository.fetch_item_quality_tags(&featured_ids).await?;
    let featured = featured_records
        .into_iter()
        .map(|record| {
            let overview = overviews.remove(&record.id);
            let tags = quality_tags
                .remove(&record.id)
                .map(|tags| quality_tags_to_labels(&tags))
                .unwrap_or_default();
            media_item_to_featured(record, overview, tags)
        })
        .collect::<Vec<_>>();

    if !latest.items.is_empty() {
        sections.push(NavigationSectionDto {
            key: "latest".to_owned(),
            title: "最新入库".to_owned(),
            layout: "poster".to_owned(),
            to: None,
            items: latest.items.into_iter().map(media_item_to_dto).collect(),
        });
    }

    Ok(NavigationDto {
        user: user_to_dto(user),
        libraries: views
            .into_iter()
            .map(|view| library_view_to_dto(view, &counts_by_library))
            .collect(),
        sections,
        featured,
    })
}

/// 构造一个按指定字段降序、默认过滤的媒体查询输入。
fn media_query(user_id: i64, limit: i64, sort_field: ItemSortField) -> MediaQueryInput {
    MediaQueryInput {
        user_id,
        parent_id: None,
        start_index: 0,
        limit,
        options: ItemQueryOptions {
            sort_field,
            sort_direction: SortDirection::Desc,
            ..ItemQueryOptions::default()
        },
    }
}

/// 「最新入库」浏览查询：递归全库、按入库时间倒序、只取影片/剧集、带图片标签。
/// 经浏览路径而非 list_latest_items，是为了拿到真实 image_tags（海报）。
fn latest_browse_query(user_id: i64) -> BrowseItemsInput {
    BrowseItemsInput {
        user_id,
        parent_id: None,
        start_index: 0,
        limit: SECTION_LIMIT,
        recursive: true,
        include_image_tags: true,
        options: ItemQueryOptions {
            type_filter: ItemTypeFilter::enabled(
                FEATURED_ITEM_TYPES
                    .iter()
                    .map(|t| (*t).to_owned())
                    .collect(),
            ),
            sort_field: ItemSortField::DateCreated,
            sort_direction: SortDirection::Desc,
            ..ItemQueryOptions::default()
        },
    }
}

fn user_to_dto(user: &AuthenticatedUser) -> NavigationUserDto {
    NavigationUserDto {
        id: user.public_id.clone(),
        name: user.username.clone(),
        is_admin: user.can_manage_server(),
    }
}

fn library_view_to_dto(
    record: UserLibraryViewRecord,
    counts_by_library: &HashMap<String, u32>,
) -> NavigationLibraryDto {
    let collection_type = LibraryType::parse(&record.library_type)
        .map(|kind| kind.collection_type().to_owned())
        .unwrap_or_else(|| record.library_type.clone());
    let count = counts_by_library.get(&record.id).copied().unwrap_or(0);
    NavigationLibraryDto {
        id: record.id,
        name: record.name,
        kind: library_type_to_frontend_kind(&record.library_type),
        collection_type,
        count,
    }
}

/// 把后端 `library_type` 映射为前端展示用的 `kind`（best-effort）。
///
/// 前端的 `anime` / `documentary` 是用户语义，后端无法区分（它们本质是 `tvshows` /
/// `movies` 库），因此只给出可从库类型确定推断的值，未知值原样返回作防御。
fn library_type_to_frontend_kind(library_type: &str) -> String {
    match LibraryType::parse(library_type) {
        Some(LibraryType::Movies) => "movie",
        Some(LibraryType::TvShows) => "series",
        Some(LibraryType::Music) => "music",
        Some(LibraryType::HomeVideos | LibraryType::Mixed) => "mixed",
        Some(LibraryType::LiveTv) => "livetv",
        None => return library_type.to_owned(),
    }
    .to_owned()
}

/// 把 `item_type`（内部小写词汇）映射为详情路由类型 `movie` / `tv`。
fn item_type_to_detail_type(item_type: &str) -> Option<String> {
    match item_type.to_ascii_lowercase().as_str() {
        "movie" => Some("movie".to_owned()),
        "series" | "season" | "episode" => Some("tv".to_owned()),
        _ => None,
    }
}

/// `image_tags` 项形如 `artwork_type=id`（见 repository 的 artwork 聚合）。
/// 判断是否含某类图（大小写不敏感，匹配 `<type>=` 前缀）。
fn has_image_type(record: &MediaItemBrowseRecord, artwork_type: &str) -> bool {
    let prefix = format!("{artwork_type}=");
    record
        .image_tags
        .iter()
        .any(|tag| tag.to_ascii_lowercase().starts_with(&prefix))
}

/// 海报地址：条目带 primary/poster 图时返回服务器根路径，否则交前端渲染占位块。
fn primary_image_path(record: &MediaItemBrowseRecord) -> Option<String> {
    if !has_image_type(record, "primary") && !has_image_type(record, "poster") {
        return None;
    }
    Some(format!("/Items/{}/Images/Primary", record.id))
}

/// 剧照地址：条目带 backdrop 图时返回 Backdrop 端点，否则 None（前端占位兜底）。
fn backdrop_image_path(record: &MediaItemBrowseRecord) -> Option<String> {
    if !has_image_type(record, "backdrop") {
        return None;
    }
    Some(format!("/Items/{}/Images/Backdrop", record.id))
}

/// 观看进度百分比 0–100：仅在有有效时长与已观看位置时给出。
fn progress_percent(record: &MediaItemBrowseRecord) -> Option<f64> {
    let runtime = record.run_time_ticks?;
    if runtime <= 0 || record.playback_position_ticks <= 0 {
        return None;
    }
    let percent = (record.playback_position_ticks as f64 / runtime as f64) * 100.0;
    Some(percent.clamp(0.0, 100.0))
}

fn media_item_to_dto(record: MediaItemBrowseRecord) -> MediaItemDto {
    let detail_type = item_type_to_detail_type(&record.item_type);
    let poster = primary_image_path(&record);
    let progress = progress_percent(&record);
    MediaItemDto {
        id: record.id,
        library_id: None,
        title: record.name,
        meta: record
            .production_year
            .map(|year| year.to_string())
            .unwrap_or_default(),
        detail_type,
        poster,
        year: record.production_year,
        rating: record.rating,
        progress,
    }
}

fn media_item_to_featured(
    record: MediaItemBrowseRecord,
    overview: Option<String>,
    tags: Vec<String>,
) -> FeaturedItemDto {
    let detail_type = item_type_to_detail_type(&record.item_type);
    let backdrop = backdrop_image_path(&record);
    let thumb = primary_image_path(&record);
    let mut meta = Vec::new();
    if let Some(year) = record.production_year {
        meta.push(year.to_string());
    }
    FeaturedItemDto {
        id: record.id,
        title: record.name,
        meta,
        tags,
        overview: overview.unwrap_or_default(),
        thumb,
        backdrop,
        detail_type,
    }
}

/// 把主文件画质（DB 词汇）映射为前端展示标签，顺序：清晰度 → HDR → 音频。
/// 清晰度对齐前端 `resolutionColors` 词汇（2160p→4K、1440p→2K、1080p→1080P…）；
/// HDR / 音频编码直接透传（DB 已是 HDR10+/DV/Atmos 等展示词）。识别不出即略过。
fn quality_tags_to_labels(tags: &ItemQualityTags) -> Vec<String> {
    let mut labels = Vec::new();
    if let Some(resolution) = tags.resolution.as_deref() {
        labels.push(resolution_to_label(resolution).to_owned());
    }
    if let Some(hdr) = tags.hdr.as_deref().filter(|value| !value.is_empty()) {
        labels.push(hdr.to_owned());
    }
    if let Some(audio) = tags
        .audio_codec
        .as_deref()
        .filter(|value| !value.is_empty())
    {
        labels.push(audio.to_owned());
    }
    labels
}

/// DB 清晰度词汇 → 前端徽章词汇（对齐 `tmdb.ts` 的 resolutionColors）；未知值原样保留。
fn resolution_to_label(resolution: &str) -> &str {
    match resolution.to_ascii_lowercase().as_str() {
        "2160p" | "4k" | "uhd" => "4K",
        "1440p" | "2k" => "2K",
        "1080p" => "1080P",
        "720p" => "720P",
        "480p" => "480P",
        _ => resolution,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn record(item_type: &str, image_tags: Vec<String>) -> MediaItemBrowseRecord {
        MediaItemBrowseRecord {
            id: "abc".to_owned(),
            name: "Example".to_owned(),
            item_type: item_type.to_owned(),
            parent_id: None,
            run_time_ticks: Some(1_000),
            media_file_id: None,
            media_file_size: None,
            media_file_container: None,
            media_file_bitrate: None,
            media_file_is_strm: None,
            supports_transcoding: false,
            production_year: Some(2025),
            index_number: None,
            parent_index_number: None,
            premiere_date: None,
            playback_position_ticks: 0,
            play_count: 0,
            is_favorite: false,
            rating: Some(8.1),
            played: false,
            playlist_item_id: None,
            image_tags,
            total_record_count: 0,
        }
    }

    #[test]
    fn frontend_kind_maps_emby_collection_vocabulary() {
        assert_eq!(library_type_to_frontend_kind("movies"), "movie");
        assert_eq!(library_type_to_frontend_kind("tvshows"), "series");
        assert_eq!(library_type_to_frontend_kind("music"), "music");
        assert_eq!(library_type_to_frontend_kind("homevideos"), "mixed");
        // 未知值原样保留作防御。
        assert_eq!(library_type_to_frontend_kind("custom"), "custom");
    }

    #[test]
    fn library_dto_uses_matched_count_else_zero() {
        let view = UserLibraryViewRecord {
            id: "lib-1".to_owned(),
            name: "电影".to_owned(),
            library_type: "movies".to_owned(),
        };
        let mut counts = HashMap::new();
        counts.insert("lib-1".to_owned(), 42u32);

        let dto = library_view_to_dto(view.clone(), &counts);
        assert_eq!(dto.count, 42);
        assert_eq!(dto.kind, "movie");
        assert_eq!(dto.collection_type, "movies");

        // 无计数条目（如空库或未统计）兜底为 0，不 panic。
        let dto_zero = library_view_to_dto(view, &HashMap::new());
        assert_eq!(dto_zero.count, 0);
    }

    #[test]
    fn detail_type_routes_series_family_to_tv() {
        assert_eq!(item_type_to_detail_type("Movie").as_deref(), Some("movie"));
        assert_eq!(item_type_to_detail_type("episode").as_deref(), Some("tv"));
        assert_eq!(item_type_to_detail_type("Series").as_deref(), Some("tv"));
        assert_eq!(item_type_to_detail_type("track"), None);
    }

    #[test]
    fn poster_matches_primary_or_poster_tag_only() {
        // 无图标签 → 无海报。
        assert_eq!(primary_image_path(&record("movie", Vec::new())), None);
        // 只有 backdrop → 不当作海报（收紧后不再误给 Primary URL）。
        assert_eq!(
            primary_image_path(&record("movie", vec!["backdrop=9".to_owned()])),
            None
        );
        // 有 primary → 给 Primary 端点。
        assert_eq!(
            primary_image_path(&record("movie", vec!["primary=123".to_owned()])).as_deref(),
            Some("/Items/abc/Images/Primary"),
        );
        // poster 也算海报。
        assert_eq!(
            primary_image_path(&record("movie", vec!["poster=7".to_owned()])).as_deref(),
            Some("/Items/abc/Images/Primary"),
        );
    }

    #[test]
    fn backdrop_matches_backdrop_tag_only() {
        assert_eq!(
            backdrop_image_path(&record("movie", vec!["primary=1".to_owned()])),
            None
        );
        assert_eq!(
            backdrop_image_path(&record("movie", vec!["backdrop=456".to_owned()])).as_deref(),
            Some("/Items/abc/Images/Backdrop"),
        );
    }

    #[test]
    fn featured_uses_backdrop_thumb_and_overview() {
        let rec = record(
            "movie",
            vec!["primary=1".to_owned(), "backdrop=2".to_owned()],
        );
        let dto = media_item_to_featured(rec, Some("一段简介".to_owned()), vec!["4K".to_owned()]);
        assert_eq!(dto.backdrop.as_deref(), Some("/Items/abc/Images/Backdrop"));
        assert_eq!(dto.thumb.as_deref(), Some("/Items/abc/Images/Primary"));
        assert_eq!(dto.overview, "一段简介");
        assert_eq!(dto.tags, vec!["4K".to_owned()]);
        assert_eq!(dto.meta, vec!["2025".to_owned()]);
        assert_eq!(dto.detail_type.as_deref(), Some("movie"));

        // 无简介/无标签 → 空占位，不 panic。
        let rec2 = record("movie", Vec::new());
        let dto2 = media_item_to_featured(rec2, None, Vec::new());
        assert_eq!(dto2.overview, "");
        assert_eq!(dto2.backdrop, None);
        assert_eq!(dto2.thumb, None);
        assert!(dto2.tags.is_empty());
    }

    #[test]
    fn quality_labels_order_resolution_hdr_audio() {
        let tags = ItemQualityTags {
            resolution: Some("2160p".to_owned()),
            hdr: Some("HDR10+".to_owned()),
            audio_codec: Some("Atmos".to_owned()),
        };
        assert_eq!(quality_tags_to_labels(&tags), vec!["4K", "HDR10+", "Atmos"]);

        // 只有清晰度；空串 HDR/音频被跳过。
        let partial = ItemQualityTags {
            resolution: Some("1080p".to_owned()),
            hdr: Some("".to_owned()),
            audio_codec: None,
        };
        assert_eq!(quality_tags_to_labels(&partial), vec!["1080P"]);

        // 全空 → 空标签。
        assert!(quality_tags_to_labels(&ItemQualityTags::default()).is_empty());
    }

    #[test]
    fn resolution_label_maps_db_vocabulary() {
        assert_eq!(resolution_to_label("2160p"), "4K");
        assert_eq!(resolution_to_label("1440p"), "2K");
        assert_eq!(resolution_to_label("1080p"), "1080P");
        assert_eq!(resolution_to_label("720p"), "720P");
        // 未知值原样保留作防御。
        assert_eq!(resolution_to_label("8K"), "8K");
    }

    #[test]
    fn progress_requires_runtime_and_position() {
        // 无观看位置 → 不给进度。
        assert_eq!(progress_percent(&record("movie", Vec::new())), None);

        let mut watched = record("movie", Vec::new());
        watched.playback_position_ticks = 250;
        assert_eq!(progress_percent(&watched), Some(25.0));

        // 进度溢出钳到 100。
        watched.playback_position_ticks = 5_000;
        assert_eq!(progress_percent(&watched), Some(100.0));
    }

    #[test]
    fn media_item_dto_carries_progress_for_resumed_item() {
        let mut watched = record("movie", vec!["primary=1".to_owned()]);
        watched.playback_position_ticks = 500;
        let dto = media_item_to_dto(watched);
        assert_eq!(dto.detail_type.as_deref(), Some("movie"));
        assert_eq!(dto.meta, "2025");
        assert_eq!(dto.progress, Some(50.0));
        assert_eq!(dto.poster.as_deref(), Some("/Items/abc/Images/Primary"));
    }
}
