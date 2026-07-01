//! 音乐浏览 BFF 的聚合服务。
//!
//! 三级下钻，全部复用 [`LibraryRepository`] 既有查询，本层零新 SQL：
//! - `list_artists`：某音乐库下艺术家 → `list_user_artists`（parent = 库 public_id）。
//! - `artist_detail`：艺术家名下专辑 → `list_user_items`（parent = artist，type=album）。
//! - `album_detail`：专辑内曲目 → `list_user_items`（parent = album，type=track，按音轨号）。
//!
//! 艺术家头部信息（名字）来自 `list_user_artists` 的记录；专辑/曲目头部信息用 include_ids
//! 快路径取单条目，避免新增「按 id 取条目」的查询。

use crate::{
    auth::service::AuthenticatedUser,
    db::DbPool,
    library::repository::{
        ArtistListInput, BrowseItemsInput, ItemQueryOptions, ItemSortField, ItemTypeFilter,
        LibraryRepository, MediaItemBrowseRecord, SortDirection, StringListFilter,
    },
    music::dto::{AlbumDetailDto, AlbumDto, ArtistDetailDto, ArtistDto, ArtistListDto, TrackDto},
};

/// Emby tick = 100ns，1 秒 = 10_000_000 ticks。
const TICKS_PER_SECOND: i64 = 10_000_000;
/// 单库浏览上限（防御性，足够覆盖常规音乐库一页）。
const BROWSE_LIMIT: i64 = 500;

/// 列出某音乐库下的艺术家。
pub async fn list_artists(
    database: DbPool,
    user: &AuthenticatedUser,
    library_id: String,
) -> Result<ArtistListDto, sqlx::Error> {
    let repository = LibraryRepository::new(database);
    let result = repository
        .list_user_artists(artist_input(user.id, library_id))
        .await?;
    Ok(ArtistListDto {
        items: result
            .items
            .into_iter()
            .map(|record| ArtistDto {
                id: record.id,
                name: record.name,
            })
            .collect(),
        total: result.total_record_count,
    })
}

/// 艺术家详情：名字（取自艺术家列表记录）+ 名下专辑。
pub async fn artist_detail(
    database: DbPool,
    user: &AuthenticatedUser,
    artist_id: String,
) -> Result<Option<ArtistDetailDto>, sqlx::Error> {
    let repository = LibraryRepository::new(database);

    // 艺术家名：以 artist 自身为 parent 跑一次艺术家查询，命中即取其名（容器 public_id 一致）。
    let artists = repository
        .list_user_artists(artist_input(user.id, artist_id.clone()))
        .await?;
    let Some(name) = artists
        .items
        .into_iter()
        .find(|a| a.id == artist_id)
        .map(|a| a.name)
    else {
        return Ok(None);
    };

    // 名下专辑：parent = artist，type = album，按发行年/名升序。
    let albums = repository
        .list_user_items(child_input(
            user.id,
            artist_id.clone(),
            "album",
            ItemSortField::ProductionYear,
        ))
        .await?;

    Ok(Some(ArtistDetailDto {
        id: artist_id,
        name,
        albums: albums.items.into_iter().map(album_to_dto).collect(),
    }))
}

/// 专辑详情：专辑头部信息 + 专辑内曲目（按音轨号）。
pub async fn album_detail(
    database: DbPool,
    user: &AuthenticatedUser,
    album_id: String,
) -> Result<Option<AlbumDetailDto>, sqlx::Error> {
    let repository = LibraryRepository::new(database);

    // 专辑头部：include_ids 快路径取单条目。
    let Some(album) = fetch_single_item(&repository, user.id, album_id.clone()).await? else {
        return Ok(None);
    };

    // 曲目：parent = album，type = track，按音轨号（index_number）升序。
    let tracks = repository
        .list_user_items(child_input(
            user.id,
            album_id.clone(),
            "track",
            ItemSortField::IndexNumber,
        ))
        .await?;

    Ok(Some(AlbumDetailDto {
        id: album.id.clone(),
        title: album.name.clone(),
        year: album.production_year,
        poster: primary_image_path(&album),
        tracks: tracks.items.into_iter().map(track_to_dto).collect(),
    }))
}

/// 构造艺术家查询输入：以库或艺术家 public_id 为 parent，列其下艺术家。
fn artist_input(user_id: i64, parent_id: String) -> ArtistListInput {
    ArtistListInput {
        user_id,
        parent_id: Some(parent_id),
        start_index: 0,
        limit: BROWSE_LIMIT,
        recursive: true,
        album_artists_only: false,
        search_term: None,
        name_starts_with: None,
        name_starts_with_or_greater: None,
        album_names: StringListFilter::default(),
        album_ids: StringListFilter::default(),
        sort_direction: SortDirection::Asc,
    }
}

/// 构造「某 parent 下指定 item_type 子条目」的浏览输入。
fn child_input(
    user_id: i64,
    parent_id: String,
    item_type: &str,
    sort_field: ItemSortField,
) -> BrowseItemsInput {
    BrowseItemsInput {
        user_id,
        parent_id: Some(parent_id),
        start_index: 0,
        limit: BROWSE_LIMIT,
        recursive: false,
        include_image_tags: true,
        options: ItemQueryOptions {
            type_filter: ItemTypeFilter::enabled(vec![item_type.to_owned()]),
            sort_field,
            sort_direction: SortDirection::Asc,
            ..ItemQueryOptions::default()
        },
    }
}

/// 用 include_ids 快路径取单条目（无需新增「按 id 取条目」查询）。
async fn fetch_single_item(
    repository: &LibraryRepository,
    user_id: i64,
    id: String,
) -> Result<Option<MediaItemBrowseRecord>, sqlx::Error> {
    let mut options = ItemQueryOptions::default();
    options.scalar_filter.include_ids = StringListFilter::enabled(vec![id]);
    let result = repository
        .list_user_items(BrowseItemsInput {
            user_id,
            parent_id: None,
            start_index: 0,
            limit: 1,
            recursive: true,
            include_image_tags: true,
            options,
        })
        .await?;
    Ok(result.items.into_iter().next())
}

fn album_to_dto(record: MediaItemBrowseRecord) -> AlbumDto {
    AlbumDto {
        poster: primary_image_path(&record),
        id: record.id,
        title: record.name,
        year: record.production_year,
    }
}

fn track_to_dto(record: MediaItemBrowseRecord) -> TrackDto {
    TrackDto {
        duration: ticks_to_seconds(record.run_time_ticks),
        id: record.id,
        title: record.name,
    }
}

/// 海报地址：仅当条目带图片标签时返回服务器根路径，否则交前端渲染占位块（同 navigation）。
fn primary_image_path(record: &MediaItemBrowseRecord) -> Option<String> {
    if record.image_tags.is_empty() {
        return None;
    }
    Some(format!("/Items/{}/Images/Primary", record.id))
}

/// run_time_ticks（100ns）→ 秒；缺省或非正值返回 None。
fn ticks_to_seconds(ticks: Option<i64>) -> Option<i64> {
    ticks.filter(|&t| t > 0).map(|t| t / TICKS_PER_SECOND)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ticks_convert_to_seconds() {
        // 3 分 30 秒 = 210s。
        assert_eq!(ticks_to_seconds(Some(210 * TICKS_PER_SECOND)), Some(210));
        // 缺省/非正值不给时长。
        assert_eq!(ticks_to_seconds(None), None);
        assert_eq!(ticks_to_seconds(Some(0)), None);
        assert_eq!(ticks_to_seconds(Some(-5)), None);
    }

    fn record(id: &str, image_tags: Vec<String>) -> MediaItemBrowseRecord {
        MediaItemBrowseRecord {
            id: id.to_owned(),
            name: "Example".to_owned(),
            item_type: "album".to_owned(),
            parent_id: None,
            run_time_ticks: None,
            media_file_id: None,
            media_file_size: None,
            media_file_container: None,
            media_file_bitrate: None,
            media_file_is_strm: None,
            supports_transcoding: false,
            production_year: Some(1975),
            playback_position_ticks: 0,
            play_count: 0,
            is_favorite: false,
            rating: None,
            played: false,
            image_tags,
            total_record_count: 0,
        }
    }

    #[test]
    fn poster_present_only_with_image_tags() {
        assert_eq!(primary_image_path(&record("abc", Vec::new())), None);
        assert_eq!(
            primary_image_path(&record("abc", vec!["tag".to_owned()])).as_deref(),
            Some("/Items/abc/Images/Primary"),
        );
    }

    #[test]
    fn album_dto_maps_year_and_poster() {
        let dto = album_to_dto(record("abc", vec!["tag".to_owned()]));
        assert_eq!(dto.year, Some(1975));
        assert_eq!(dto.poster.as_deref(), Some("/Items/abc/Images/Primary"));
    }
}
