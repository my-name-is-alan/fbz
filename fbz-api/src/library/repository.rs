use sqlx::{Row, postgres::PgRow};

use crate::db::DbPool;

const USER_VIEW_LIMIT: i64 = 1_000;

#[derive(Clone)]
pub struct LibraryRepository {
    pool: DbPool,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ItemTypeFilter {
    pub enabled: bool,
    pub item_types: Vec<String>,
}

impl ItemTypeFilter {
    pub fn enabled(item_types: Vec<String>) -> Self {
        Self {
            enabled: true,
            item_types,
        }
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct StringListFilter {
    pub enabled: bool,
    pub values: Vec<String>,
}

impl StringListFilter {
    pub fn enabled(values: Vec<String>) -> Self {
        Self {
            enabled: true,
            values,
        }
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct IntListFilter {
    pub enabled: bool,
    pub values: Vec<i32>,
}

impl IntListFilter {
    pub fn enabled(values: Vec<i32>) -> Self {
        Self {
            enabled: true,
            values,
        }
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ItemScalarFilter {
    pub include_ids: StringListFilter,
    pub exclude_ids: StringListFilter,
    pub years: IntListFilter,
    pub search_term: Option<String>,
    pub name_starts_with: Option<String>,
    pub name_starts_with_or_greater: Option<String>,
    pub name_less_than: Option<String>,
}

impl ItemScalarFilter {
    pub fn has_any_filter(&self) -> bool {
        self.include_ids.enabled
            || self.exclude_ids.enabled
            || self.years.enabled
            || self.search_term.is_some()
            || self.name_starts_with.is_some()
            || self.name_starts_with_or_greater.is_some()
            || self.name_less_than.is_some()
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ItemUserDataFilter {
    pub is_played: Option<bool>,
    pub is_favorite: Option<bool>,
    pub require_played: bool,
    pub require_unplayed: bool,
    pub require_favorite: bool,
    pub require_resumable: bool,
    pub require_likes: bool,
    pub require_dislikes: bool,
}

impl ItemUserDataFilter {
    pub fn has_any_filter(&self) -> bool {
        self.is_played.is_some()
            || self.is_favorite.is_some()
            || self.require_played
            || self.require_unplayed
            || self.require_favorite
            || self.require_resumable
            || self.require_likes
            || self.require_dislikes
    }

    fn can_use_positive_playstate_fast_path(&self) -> bool {
        let has_positive_filter = self.is_played == Some(true)
            || self.is_favorite == Some(true)
            || self.require_played
            || self.require_favorite
            || self.require_resumable
            || self.require_likes
            || self.require_dislikes;
        let requires_missing_playstate_semantics = self.is_played == Some(false)
            || self.is_favorite == Some(false)
            || self.require_unplayed;

        has_positive_filter && !requires_missing_playstate_semantics
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ItemStructureFilter {
    pub is_folder: Option<bool>,
    pub is_movie: Option<bool>,
    pub is_series: Option<bool>,
    pub require_folder: bool,
    pub require_not_folder: bool,
}

impl ItemStructureFilter {
    pub fn has_any_filter(&self) -> bool {
        self.is_folder.is_some()
            || self.is_movie.is_some()
            || self.is_series.is_some()
            || self.require_folder
            || self.require_not_folder
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ItemMediaFilter {
    pub media_types: StringListFilter,
    pub containers: StringListFilter,
    pub audio_codecs: StringListFilter,
    pub video_codecs: StringListFilter,
    pub subtitle_codecs: StringListFilter,
}

impl ItemMediaFilter {
    pub fn has_any_filter(&self) -> bool {
        self.media_types.enabled
            || self.containers.enabled
            || self.audio_codecs.enabled
            || self.video_codecs.enabled
            || self.subtitle_codecs.enabled
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ItemProviderFilter {
    pub any_provider_id_equals: StringListFilter,
}

impl ItemProviderFilter {
    pub fn has_any_filter(&self) -> bool {
        self.any_provider_id_equals.enabled
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ItemImageFilter {
    pub image_types: StringListFilter,
}

impl ItemImageFilter {
    pub fn has_any_filter(&self) -> bool {
        self.image_types.enabled
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ItemAssociationFilter {
    pub genre_names: StringListFilter,
    pub genre_ids: StringListFilter,
    pub person_names: StringListFilter,
    pub person_ids: StringListFilter,
    pub person_role_types: PersonRoleFilter,
    pub artist_names: StringListFilter,
    pub artist_ids: StringListFilter,
    pub album_names: StringListFilter,
    pub album_ids: StringListFilter,
    pub studio_names: StringListFilter,
    pub studio_ids: StringListFilter,
    pub tag_names: StringListFilter,
    pub exclude_tag_names: StringListFilter,
    pub official_ratings: StringListFilter,
}

impl ItemAssociationFilter {
    pub fn has_any_filter(&self) -> bool {
        self.genre_names.enabled
            || self.genre_ids.enabled
            || self.person_names.enabled
            || self.person_ids.enabled
            || self.person_role_types.enabled
            || self.artist_names.enabled
            || self.artist_ids.enabled
            || self.album_names.enabled
            || self.album_ids.enabled
            || self.studio_names.enabled
            || self.studio_ids.enabled
            || self.tag_names.enabled
            || self.exclude_tag_names.enabled
            || self.official_ratings.enabled
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ItemSortField {
    SortName,
    DateCreated,
    Runtime,
    ProductionYear,
    IndexNumber,
}

impl ItemSortField {
    pub fn as_sql_key(self) -> &'static str {
        match self {
            Self::SortName => "sort_name",
            Self::DateCreated => "date_created",
            Self::Runtime => "runtime",
            Self::ProductionYear => "production_year",
            Self::IndexNumber => "index_number",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SortDirection {
    Asc,
    Desc,
}

impl SortDirection {
    pub fn as_sql_key(self) -> &'static str {
        match self {
            Self::Asc => "asc",
            Self::Desc => "desc",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ItemQueryOptions {
    pub type_filter: ItemTypeFilter,
    pub scalar_filter: ItemScalarFilter,
    pub user_data_filter: ItemUserDataFilter,
    pub structure_filter: ItemStructureFilter,
    pub media_filter: ItemMediaFilter,
    pub provider_filter: ItemProviderFilter,
    pub image_filter: ItemImageFilter,
    pub association_filter: ItemAssociationFilter,
    pub sort_field: ItemSortField,
    pub sort_direction: SortDirection,
}

impl Default for ItemQueryOptions {
    fn default() -> Self {
        Self {
            type_filter: ItemTypeFilter::default(),
            scalar_filter: ItemScalarFilter::default(),
            user_data_filter: ItemUserDataFilter::default(),
            structure_filter: ItemStructureFilter::default(),
            media_filter: ItemMediaFilter::default(),
            provider_filter: ItemProviderFilter::default(),
            image_filter: ItemImageFilter::default(),
            association_filter: ItemAssociationFilter::default(),
            sort_field: ItemSortField::SortName,
            sort_direction: SortDirection::Asc,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct UserLibraryViewRecord {
    pub id: String,
    pub name: String,
    pub library_type: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct UserMediaFolderRecord {
    pub id: String,
    pub name: String,
    pub library_type: String,
    pub subfolders: Vec<UserMediaSubFolderRecord>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct UserMediaSubFolderRecord {
    pub id: String,
    pub path: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BrowseItemsInput {
    pub user_id: i64,
    pub parent_id: Option<String>,
    pub start_index: i64,
    pub limit: i64,
    pub recursive: bool,
    pub include_image_tags: bool,
    pub options: ItemQueryOptions,
}

impl BrowseItemsInput {
    fn can_use_positive_playstate_fast_path(&self) -> bool {
        self.options
            .user_data_filter
            .can_use_positive_playstate_fast_path()
            && !self.options.scalar_filter.has_any_filter()
            && !self.options.structure_filter.has_any_filter()
            && !self.options.media_filter.has_any_filter()
            && !self.options.provider_filter.has_any_filter()
            && !self.options.image_filter.has_any_filter()
            && !self.options.association_filter.has_any_filter()
    }

    fn can_use_include_ids_fast_path(&self) -> bool {
        self.options.scalar_filter.include_ids.enabled
            && !self.options.scalar_filter.include_ids.values.is_empty()
            && self
                .options
                .scalar_filter
                .include_ids
                .values
                .iter()
                .all(|value| is_uuid_text(value))
            && !self.options.scalar_filter.exclude_ids.enabled
            && !self.options.scalar_filter.years.enabled
            && self.options.scalar_filter.search_term.is_none()
            && self.options.scalar_filter.name_starts_with.is_none()
            && self
                .options
                .scalar_filter
                .name_starts_with_or_greater
                .is_none()
            && self.options.scalar_filter.name_less_than.is_none()
            && !self.options.user_data_filter.has_any_filter()
            && !self.options.structure_filter.has_any_filter()
            && !self.options.media_filter.has_any_filter()
            && !self.options.provider_filter.has_any_filter()
            && !self.options.image_filter.has_any_filter()
            && !self.options.association_filter.has_any_filter()
    }

    fn can_use_provider_id_fast_path(&self) -> bool {
        self.options.provider_filter.any_provider_id_equals.enabled
            && !self
                .options
                .provider_filter
                .any_provider_id_equals
                .values
                .is_empty()
            && !self.options.scalar_filter.has_any_filter()
            && !self.options.user_data_filter.has_any_filter()
            && !self.options.structure_filter.has_any_filter()
            && !self.options.media_filter.has_any_filter()
            && !self.options.image_filter.has_any_filter()
            && !self.options.association_filter.has_any_filter()
    }
}

fn is_uuid_text(value: &str) -> bool {
    let value = value.trim();
    if value.len() != 36 {
        return false;
    }

    value.bytes().enumerate().all(|(index, byte)| {
        if matches!(index, 8 | 13 | 18 | 23) {
            byte == b'-'
        } else {
            byte.is_ascii_hexdigit()
        }
    })
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MediaQueryInput {
    pub user_id: i64,
    pub parent_id: Option<String>,
    pub start_index: i64,
    pub limit: i64,
    pub options: ItemQueryOptions,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SimilarItemsInput {
    pub user_id: i64,
    pub item_id: String,
    pub start_index: i64,
    pub limit: i64,
    pub options: ItemQueryOptions,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PlaylistListInput {
    pub user_id: i64,
    pub parent_id: Option<String>,
    pub start_index: i64,
    pub limit: i64,
    pub search_term: Option<String>,
    pub sort_direction: SortDirection,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PlaylistItemsInput {
    pub user_id: i64,
    pub playlist_id: String,
    pub start_index: i64,
    pub limit: i64,
    pub include_image_tags: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ItemPrefixesInput {
    pub user_id: i64,
    pub parent_id: Option<String>,
    pub start_index: i64,
    pub limit: i64,
    pub recursive: bool,
    pub type_filter: ItemTypeFilter,
    pub search_term: Option<String>,
    pub name_starts_with: Option<String>,
    pub name_starts_with_or_greater: Option<String>,
    pub name_less_than: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ItemsFiltersInput {
    pub user_id: i64,
    pub parent_id: Option<String>,
    pub recursive: bool,
    pub item_types: ItemTypeFilter,
    pub media_types: StringListFilter,
    pub limit: i64,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct GenreListInput {
    pub user_id: i64,
    pub parent_id: Option<String>,
    pub start_index: i64,
    pub limit: i64,
    pub recursive: bool,
    pub music_only: bool,
    pub search_term: Option<String>,
    pub name_starts_with: Option<String>,
    pub name_starts_with_or_greater: Option<String>,
    pub sort_direction: SortDirection,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ArtistListInput {
    pub user_id: i64,
    pub parent_id: Option<String>,
    pub start_index: i64,
    pub limit: i64,
    pub recursive: bool,
    pub album_artists_only: bool,
    pub search_term: Option<String>,
    pub name_starts_with: Option<String>,
    pub name_starts_with_or_greater: Option<String>,
    pub album_names: StringListFilter,
    pub album_ids: StringListFilter,
    pub sort_direction: SortDirection,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct PersonRoleFilter {
    pub enabled: bool,
    pub role_types: Vec<String>,
}

impl PersonRoleFilter {
    pub fn enabled(role_types: Vec<String>) -> Self {
        Self {
            enabled: true,
            role_types,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PersonListInput {
    pub user_id: i64,
    pub parent_id: Option<String>,
    pub start_index: i64,
    pub limit: i64,
    pub recursive: bool,
    pub role_filter: PersonRoleFilter,
    pub search_term: Option<String>,
    pub name_starts_with: Option<String>,
    pub name_starts_with_or_greater: Option<String>,
    pub sort_direction: SortDirection,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct StudioListInput {
    pub user_id: i64,
    pub parent_id: Option<String>,
    pub start_index: i64,
    pub limit: i64,
    pub recursive: bool,
    pub search_term: Option<String>,
    pub name_starts_with: Option<String>,
    pub name_starts_with_or_greater: Option<String>,
    pub sort_direction: SortDirection,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TagListInput {
    pub user_id: i64,
    pub parent_id: Option<String>,
    pub start_index: i64,
    pub limit: i64,
    pub recursive: bool,
    pub search_term: Option<String>,
    pub name_starts_with: Option<String>,
    pub name_starts_with_or_greater: Option<String>,
    pub sort_direction: SortDirection,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct OfficialRatingListInput {
    pub user_id: i64,
    pub parent_id: Option<String>,
    pub start_index: i64,
    pub limit: i64,
    pub recursive: bool,
    pub search_term: Option<String>,
    pub name_starts_with: Option<String>,
    pub name_starts_with_or_greater: Option<String>,
    pub sort_direction: SortDirection,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct YearListInput {
    pub user_id: i64,
    pub parent_id: Option<String>,
    pub start_index: i64,
    pub limit: i64,
    pub recursive: bool,
    pub search_term: Option<String>,
    pub name_starts_with: Option<String>,
    pub name_starts_with_or_greater: Option<String>,
    pub sort_direction: SortDirection,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TechnicalFacetKind {
    Container,
    AudioCodec,
    VideoCodec,
    SubtitleCodec,
    StreamLanguage,
}

impl TechnicalFacetKind {
    pub fn as_sql_key(self) -> &'static str {
        match self {
            Self::Container => "container",
            Self::AudioCodec => "audio_codec",
            Self::VideoCodec => "video_codec",
            Self::SubtitleCodec => "subtitle_codec",
            Self::StreamLanguage => "stream_language",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TechnicalFacetListInput {
    pub user_id: i64,
    pub parent_id: Option<String>,
    pub start_index: i64,
    pub limit: i64,
    pub recursive: bool,
    pub search_term: Option<String>,
    pub name_starts_with: Option<String>,
    pub name_starts_with_or_greater: Option<String>,
    pub sort_direction: SortDirection,
    pub kind: TechnicalFacetKind,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ShowItemsInput {
    pub user_id: i64,
    pub series_id: String,
    pub season_id: Option<String>,
    pub start_index: i64,
    pub limit: i64,
    pub options: ItemQueryOptions,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NextUpInput {
    pub user_id: i64,
    pub series_id: Option<String>,
    pub parent_id: Option<String>,
    pub start_index: i64,
    pub limit: i64,
    pub options: ItemQueryOptions,
}

#[derive(Clone, Debug, PartialEq)]
pub struct BrowseItemsResult {
    pub items: Vec<MediaItemBrowseRecord>,
    pub total_record_count: u32,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct GenreListResult {
    pub items: Vec<GenreRecord>,
    pub total_record_count: u32,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct GenreRecord {
    pub id: String,
    pub name: String,
    pub total_record_count: i64,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ItemsFiltersResult {
    pub genres: Vec<String>,
    pub tags: Vec<String>,
    pub official_ratings: Vec<String>,
    pub years: Vec<i32>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ArtistListResult {
    pub items: Vec<ArtistRecord>,
    pub total_record_count: u32,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ArtistRecord {
    pub id: String,
    pub name: String,
    pub total_record_count: i64,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PersonListResult {
    pub items: Vec<PersonRecord>,
    pub total_record_count: u32,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct StudioListResult {
    pub items: Vec<StudioRecord>,
    pub total_record_count: u32,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TagListResult {
    pub items: Vec<TagRecord>,
    pub total_record_count: u32,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct OfficialRatingListResult {
    pub items: Vec<OfficialRatingRecord>,
    pub total_record_count: u32,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct YearListResult {
    pub items: Vec<YearRecord>,
    pub total_record_count: u32,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TechnicalFacetListResult {
    pub items: Vec<TechnicalFacetRecord>,
    pub total_record_count: u32,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PlaylistListResult {
    pub items: Vec<PlaylistRecord>,
    pub total_record_count: u32,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PersonRecord {
    pub id: String,
    pub name: String,
    pub total_record_count: i64,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct StudioRecord {
    pub id: String,
    pub name: String,
    pub total_record_count: i64,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TagRecord {
    pub id: String,
    pub name: String,
    pub total_record_count: i64,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct OfficialRatingRecord {
    pub name: String,
    pub total_record_count: i64,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct YearRecord {
    pub year: i32,
    pub total_record_count: i64,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TechnicalFacetRecord {
    pub id: String,
    pub name: String,
    pub total_record_count: i64,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PlaylistRecord {
    pub id: String,
    pub name: String,
    pub total_record_count: i64,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ItemPrefixRecord {
    pub name: String,
    pub value: String,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ItemCountsRecord {
    pub movie_count: u32,
    pub series_count: u32,
    pub episode_count: u32,
    pub artist_count: u32,
    pub song_count: u32,
    pub album_count: u32,
    pub box_set_count: u32,
    pub item_count: u32,
}

#[derive(Clone, Debug, PartialEq)]
pub struct MediaItemBrowseRecord {
    pub id: String,
    pub name: String,
    pub item_type: String,
    pub parent_id: Option<String>,
    pub run_time_ticks: Option<i64>,
    pub media_file_id: Option<i64>,
    pub media_file_size: Option<i64>,
    pub media_file_container: Option<String>,
    pub media_file_bitrate: Option<i32>,
    pub media_file_is_strm: Option<bool>,
    pub supports_transcoding: bool,
    pub production_year: Option<i32>,
    pub playback_position_ticks: i64,
    pub play_count: i32,
    pub is_favorite: bool,
    pub rating: Option<f64>,
    pub played: bool,
    pub image_tags: Vec<String>,
    pub total_record_count: i64,
}

#[derive(Clone, Debug, PartialEq)]
pub enum UserItemAncestorRecord {
    Library(UserLibraryViewRecord),
    Media(MediaItemBrowseRecord),
}

impl LibraryRepository {
    pub fn new(pool: DbPool) -> Self {
        Self { pool }
    }

    pub async fn list_user_views(
        &self,
        user_id: i64,
    ) -> Result<Vec<UserLibraryViewRecord>, sqlx::Error> {
        let rows = sqlx::query(
            r#"
            select
                l.public_id::text as id,
                l.name,
                l.library_type
            from libraries l
            join library_permissions lp on lp.library_id = l.id
            where lp.user_id = $1
              and lp.can_view = true
              and l.is_hidden = false
            order by l.name, l.id
            limit $2
            "#,
        )
        .bind(user_id)
        .bind(USER_VIEW_LIMIT)
        .fetch_all(&self.pool)
        .await?;

        rows.into_iter()
            .map(UserLibraryViewRecord::from_row)
            .collect()
    }

    pub async fn list_user_media_folders(
        &self,
        user_id: i64,
    ) -> Result<Vec<UserMediaFolderRecord>, sqlx::Error> {
        let rows = sqlx::query(
            r#"
            with allowed_libraries as (
                select
                    l.id,
                    l.public_id,
                    l.name,
                    l.library_type
                from libraries l
                join library_permissions perm on perm.library_id = l.id
                where perm.user_id = $1
                  and perm.can_view = true
                  and l.is_hidden = false
                order by l.name, l.id
                limit $2
            )
            select
                l.public_id::text as library_id,
                l.name,
                l.library_type,
                lp.id::text as subfolder_id,
                lp.path as subfolder_path
            from allowed_libraries l
            left join library_paths lp on lp.library_id = l.id
                and lp.is_enabled = true
            order by l.name, l.id, lp.id
            "#,
        )
        .bind(user_id)
        .bind(USER_VIEW_LIMIT)
        .fetch_all(&self.pool)
        .await?;

        media_folders_from_rows(rows)
    }

    pub async fn list_admin_physical_paths(&self) -> Result<Vec<String>, sqlx::Error> {
        let rows = sqlx::query(admin_physical_paths_query())
            .fetch_all(&self.pool)
            .await?;

        rows.into_iter()
            .map(|row| row.try_get::<String, _>("path"))
            .collect()
    }

    pub async fn find_user_view_by_id(
        &self,
        user_id: i64,
        view_id: &str,
    ) -> Result<Option<UserLibraryViewRecord>, sqlx::Error> {
        sqlx::query(
            r#"
            select
                l.public_id::text as id,
                l.name,
                l.library_type
            from libraries l
            join library_permissions lp on lp.library_id = l.id
            where lp.user_id = $1
              and l.public_id = case
                  when $2 ~* '^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$'
                  then $2::uuid
                  else null::uuid
              end
              and lp.can_view = true
              and l.is_hidden = false
            "#,
        )
        .bind(user_id)
        .bind(view_id)
        .fetch_optional(&self.pool)
        .await?
        .map(UserLibraryViewRecord::from_row)
        .transpose()
    }

    pub async fn count_user_items(&self, user_id: i64) -> Result<ItemCountsRecord, sqlx::Error> {
        sqlx::query(
            r#"
            select
                count(*) filter (where mi.item_type = 'movie') as movie_count,
                count(*) filter (where mi.item_type = 'series') as series_count,
                count(*) filter (where mi.item_type = 'episode') as episode_count,
                count(*) filter (where mi.item_type = 'artist') as artist_count,
                count(*) filter (where mi.item_type = 'track') as song_count,
                count(*) filter (where mi.item_type = 'album') as album_count,
                count(*) filter (where mi.item_type = 'collection') as box_set_count,
                count(*) as item_count
            from media_items mi
            join libraries l on l.id = mi.library_id
            join library_permissions lp on lp.library_id = mi.library_id
            where lp.user_id = $1
              and lp.can_view = true
              and mi.is_deleted = false
              and l.is_hidden = false
            "#,
        )
        .bind(user_id)
        .fetch_one(&self.pool)
        .await
        .and_then(ItemCountsRecord::from_row)
    }

    pub async fn list_user_playlists(
        &self,
        input: PlaylistListInput,
    ) -> Result<PlaylistListResult, sqlx::Error> {
        let rows = sqlx::query(
            r#"
            with requested_parent as (
                select case
                    when $2::text is null then null::uuid
                    when $2::text ~* '^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$'
                    then $2::uuid
                    else null::uuid
                end as public_id
            ),
            visible_playlists as (
                select
                    c.id as internal_id,
                    c.public_id::text as id,
                    c.name
                from collections c
                left join libraries l on l.id = c.library_id
                left join library_permissions lp on lp.library_id = c.library_id
                    and lp.user_id = $1
                cross join requested_parent rp
                where (
                        $2::text is null
                     or (
                            c.library_id is not null
                        and l.public_id = rp.public_id
                     )
                  )
                  and (
                        c.library_id is null
                     or (
                            lp.can_view = true
                        and l.is_hidden = false
                     )
                  )
                  and (
                        c.library_id is not null
                     or exists (
                            select 1
                            from collection_items ci_visible
                            join media_items mi_visible
                              on mi_visible.id = ci_visible.media_item_id
                            join libraries l_visible
                              on l_visible.id = mi_visible.library_id
                            join library_permissions lp_visible
                              on lp_visible.library_id = mi_visible.library_id
                            where ci_visible.collection_id = c.id
                              and lp_visible.user_id = $1
                              and lp_visible.can_view = true
                              and mi_visible.is_deleted = false
                              and l_visible.is_hidden = false
                     )
                  )
                  and (
                        $5::text is null
                     or lower(c.name) like '%' || lower($5::text) || '%'
                  )
            )
            select
                id,
                name,
                count(*) over() as total_record_count
            from visible_playlists
            order by
                case when $6 = 'asc' then lower(name) end asc nulls last,
                case when $6 = 'desc' then lower(name) end desc nulls last,
                internal_id asc
            limit $3
            offset $4
            "#,
        )
        .bind(input.user_id)
        .bind(input.parent_id)
        .bind(input.limit)
        .bind(input.start_index)
        .bind(input.search_term)
        .bind(input.sort_direction.as_sql_key())
        .fetch_all(&self.pool)
        .await?;

        playlist_result_from_rows(rows)
    }

    pub async fn list_user_playlist_items(
        &self,
        input: PlaylistItemsInput,
    ) -> Result<BrowseItemsResult, sqlx::Error> {
        let rows = sqlx::query(
            r#"
            with requested_playlist as (
                select case
                    when $2::text ~* '^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$'
                    then $2::uuid
                    else null::uuid
                end as public_id
            ),
            playlist_scope as (
                select c.id
                from collections c
                left join libraries playlist_library
                  on playlist_library.id = c.library_id
                left join library_permissions playlist_permission
                  on playlist_permission.library_id = c.library_id
                 and playlist_permission.user_id = $1
                cross join requested_playlist requested
                where c.public_id = requested.public_id
                  and (
                        c.library_id is null
                     or (
                            playlist_permission.can_view = true
                        and playlist_library.is_hidden = false
                     )
                  )
            )
            select
                mi.public_id::text as id,
                mi.title as name,
                mi.item_type,
                parent.public_id::text as parent_id,
                coalesce(mi.runtime_ticks, primary_file.duration_ticks) as runtime_ticks,
                primary_file.media_file_id,
                primary_file.file_size as media_file_size,
                primary_file.container as media_file_container,
                primary_file.bitrate as media_file_bitrate,
                primary_file.is_strm as media_file_is_strm,
                (u.allow_transcode and lp.can_transcode) as supports_transcoding,
                mi.production_year,
                coalesce(up.position_ticks, 0) as playback_position_ticks,
                coalesce(up.play_count, 0) as play_count,
                coalesce(up.is_favorite, false) as is_favorite,
                up.rating::double precision as rating,
                coalesce(up.played, false) as played,
                case
                    when $5::boolean = true then coalesce(item_images.image_tags, array[]::text[])
                    else array[]::text[]
                end as image_tags,
                count(*) over() as total_record_count
            from playlist_scope scope
            join collection_items ci on ci.collection_id = scope.id
            join media_items mi on mi.id = ci.media_item_id
            join libraries l on l.id = mi.library_id
            join library_permissions lp on lp.library_id = mi.library_id
            join users u on u.id = lp.user_id
            left join media_items parent on parent.id = mi.parent_id
            left join lateral (
                select mf.id as media_file_id,
                       mf.file_size,
                       mf.container,
                       mf.duration_ticks,
                       mf.bitrate,
                       mf.is_strm
                from media_files mf
                where mf.media_item_id = mi.id
                order by mf.is_primary desc, mf.id
                limit 1
            ) primary_file on true
            left join user_playstates up on up.user_id = $1
                and up.media_item_id = mi.id
            left join lateral (
                select array_agg(a.artwork_type || '=' || a.id::text order by a.artwork_type, a.is_primary desc, a.id) as image_tags
                from artwork a
                where a.media_item_id = mi.id
            ) item_images on $5::boolean = true
            where lp.user_id = $1
              and lp.can_view = true
              and mi.is_deleted = false
              and l.is_hidden = false
            order by ci.sort_order asc,
                     coalesce(nullif(mi.sort_title, ''), mi.title) asc,
                     mi.id asc
            limit $3
            offset $4
            "#,
        )
        .bind(input.user_id)
        .bind(input.playlist_id)
        .bind(input.limit)
        .bind(input.start_index)
        .bind(input.include_image_tags)
        .fetch_all(&self.pool)
        .await?;

        browse_result_from_rows(rows)
    }

    pub async fn list_user_item_filters(
        &self,
        input: ItemsFiltersInput,
    ) -> Result<ItemsFiltersResult, sqlx::Error> {
        let row = sqlx::query(
            r#"
            with recursive requested_parent as (
                select case
                    when $2::text ~* '^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$'
                    then $2::uuid
                    else null::uuid
                end as public_id
            ),
            allowed_libraries as (
                select l.id,
                       l.public_id
                from libraries l
                join library_permissions lp on lp.library_id = l.id
                where lp.user_id = $1
                  and lp.can_view = true
                  and l.is_hidden = false
            ),
            parent_library as (
                select al.id as library_id
                from allowed_libraries al
                join requested_parent rp on rp.public_id = al.public_id
            ),
            parent_item as (
                select mi.id as item_id,
                       mi.library_id
                from media_items mi
                join allowed_libraries al on al.id = mi.library_id
                join requested_parent rp on rp.public_id = mi.public_id
                where mi.is_deleted = false
            ),
            parent_descendants as (
                select pi.item_id
                from parent_item pi
                union all
                select child.id
                from media_items child
                join parent_descendants parent on parent.item_id = child.parent_id
                where child.is_deleted = false
            ),
            visible_items as (
                select
                    mi.id,
                    mi.item_type,
                    btrim(mi.official_rating) as official_rating,
                    mi.production_year
                from media_items mi
                join allowed_libraries al on al.id = mi.library_id
                left join parent_library pl on pl.library_id = mi.library_id
                left join parent_item pi on pi.library_id = mi.library_id
                where mi.is_deleted = false
                  and (
                        $2::text is null
                     or pl.library_id is not null
                     or (
                            pi.item_id is not null
                        and (
                                mi.id = pi.item_id
                             or ($7::boolean = false and mi.parent_id = pi.item_id)
                             or ($7::boolean = true and mi.id in (
                                    select item_id from parent_descendants
                                ))
                        )
                     )
                  )
                  and (
                        $3::boolean = false
                     or mi.item_type = any($4::text[])
                  )
                  and (
                        $5::boolean = false
                     or case
                            when mi.item_type in ('movie', 'series', 'season', 'episode') then 'video'
                            when mi.item_type in ('artist', 'album', 'track') then 'audio'
                            else null
                        end = any($6::text[])
                  )
            ),
            genre_candidates as (
                select distinct
                    g.name
                from media_item_genres mig
                join visible_items vi on vi.id = mig.media_item_id
                join genres g on g.id = mig.genre_id
            ),
            tag_candidates as (
                select distinct
                    t.name
                from media_item_tags mit
                join visible_items vi on vi.id = mit.media_item_id
                join tags t on t.id = mit.tag_id
            ),
            rating_candidates as (
                select distinct
                    vi.official_rating as name
                from visible_items vi
                where vi.official_rating is not null
                  and length(vi.official_rating) > 0
            ),
            year_candidates as (
                select distinct
                    vi.production_year as year
                from visible_items vi
                where vi.production_year is not null
            )
            select
                coalesce((
                    select array_agg(name order by lower(name), name)
                    from (
                        select name
                        from genre_candidates
                        order by lower(name), name
                        limit $8
                    ) selected_genres
                ), array[]::text[]) as genres,
                coalesce((
                    select array_agg(name order by lower(name), name)
                    from (
                        select name
                        from tag_candidates
                        order by lower(name), name
                        limit $8
                    ) selected_tags
                ), array[]::text[]) as tags,
                coalesce((
                    select array_agg(name order by lower(name), name)
                    from (
                        select name
                        from rating_candidates
                        order by lower(name), name
                        limit $8
                    ) selected_ratings
                ), array[]::text[]) as official_ratings,
                coalesce((
                    select array_agg(year order by year desc)
                    from (
                        select year
                        from year_candidates
                        order by year desc
                        limit $8
                    ) selected_years
                ), array[]::int[]) as years
            "#,
        )
        .bind(input.user_id)
        .bind(input.parent_id)
        .bind(input.item_types.enabled)
        .bind(&input.item_types.item_types)
        .bind(input.media_types.enabled)
        .bind(&input.media_types.values)
        .bind(input.recursive)
        .bind(input.limit)
        .fetch_one(&self.pool)
        .await?;

        items_filters_result_from_row(row)
    }

    pub async fn list_user_genres(
        &self,
        input: GenreListInput,
    ) -> Result<GenreListResult, sqlx::Error> {
        let rows = sqlx::query(
            r#"
            with recursive requested_parent as (
                select case
                    when $2::text ~* '^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$'
                    then $2::uuid
                    else null::uuid
                end as public_id
            ),
            allowed_libraries as (
                select l.id,
                       l.public_id,
                       l.library_type
                from libraries l
                join library_permissions lp on lp.library_id = l.id
                where lp.user_id = $1
                  and lp.can_view = true
                  and l.is_hidden = false
            ),
            parent_library as (
                select al.id as library_id
                from allowed_libraries al
                join requested_parent rp on rp.public_id = al.public_id
            ),
            parent_item as (
                select mi.id as item_id,
                       mi.library_id
                from media_items mi
                join allowed_libraries al on al.id = mi.library_id
                join requested_parent rp on rp.public_id = mi.public_id
                where mi.is_deleted = false
            ),
            parent_descendants as (
                select pi.item_id
                from parent_item pi
                union all
                select child.id
                from media_items child
                join parent_descendants parent on parent.item_id = child.parent_id
                where child.is_deleted = false
            ),
            genre_candidates as (
                select distinct
                    g.id::text as id,
                    g.name
                from media_item_genres mig
                join genres g on g.id = mig.genre_id
                join media_items mi on mi.id = mig.media_item_id
                join allowed_libraries al on al.id = mi.library_id
                left join parent_library pl on pl.library_id = mi.library_id
                left join parent_item pi on pi.library_id = mi.library_id
                where mi.is_deleted = false
                  and (
                        $6::boolean = false
                     or al.library_type = 'music'
                     or mi.item_type in ('artist', 'album', 'track')
                  )
                  and (
                        $2::text is null
                     or pl.library_id is not null
                     or (
                            pi.item_id is not null
                        and (
                                mi.id = pi.item_id
                             or ($5::boolean = false and mi.parent_id = pi.item_id)
                             or ($5::boolean = true and mi.id in (
                                    select item_id from parent_descendants
                                ))
                        )
                     )
                  )
                  and (
                        $7::text is null
                     or lower(g.name) like '%' || lower($7::text) || '%'
                  )
                  and (
                        $8::text is null
                     or lower(g.name) like lower($8::text) || '%'
                  )
                  and (
                        $9::text is null
                     or lower(g.name) >= lower($9::text)
                  )
            )
            select
                id,
                name,
                count(*) over() as total_record_count
            from genre_candidates
            order by
                case when $10 = 'asc' then lower(name) end asc nulls last,
                case when $10 = 'desc' then lower(name) end desc nulls last,
                id asc
            limit $3
            offset $4
            "#,
        )
        .bind(input.user_id)
        .bind(input.parent_id)
        .bind(input.limit)
        .bind(input.start_index)
        .bind(input.recursive)
        .bind(input.music_only)
        .bind(input.search_term)
        .bind(input.name_starts_with)
        .bind(input.name_starts_with_or_greater)
        .bind(input.sort_direction.as_sql_key())
        .fetch_all(&self.pool)
        .await?;

        genre_result_from_rows(rows)
    }

    pub async fn find_user_genre_by_name(
        &self,
        user_id: i64,
        name: &str,
        music_only: bool,
    ) -> Result<Option<GenreRecord>, sqlx::Error> {
        sqlx::query(
            r#"
            select
                g.id::text as id,
                g.name,
                1::bigint as total_record_count
            from genres g
            where lower(g.name) = lower($2)
              and exists (
                    select 1
                    from media_item_genres mig
                    join media_items mi on mi.id = mig.media_item_id
                    join libraries l on l.id = mi.library_id
                    join library_permissions lp on lp.library_id = mi.library_id
                    where mig.genre_id = g.id
                      and lp.user_id = $1
                      and lp.can_view = true
                      and mi.is_deleted = false
                      and l.is_hidden = false
                      and (
                            $3::boolean = false
                         or l.library_type = 'music'
                         or mi.item_type in ('artist', 'album', 'track')
                      )
              )
            order by g.id
            limit 1
            "#,
        )
        .bind(user_id)
        .bind(name.trim())
        .bind(music_only)
        .fetch_optional(&self.pool)
        .await?
        .map(GenreRecord::from_row)
        .transpose()
    }

    pub async fn list_user_studios(
        &self,
        input: StudioListInput,
    ) -> Result<StudioListResult, sqlx::Error> {
        let rows = sqlx::query(
            r#"
            with recursive requested_parent as (
                select case
                    when $2::text ~* '^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$'
                    then $2::uuid
                    else null::uuid
                end as public_id
            ),
            allowed_libraries as (
                select l.id,
                       l.public_id
                from libraries l
                join library_permissions lp on lp.library_id = l.id
                where lp.user_id = $1
                  and lp.can_view = true
                  and l.is_hidden = false
            ),
            parent_library as (
                select al.id as library_id
                from allowed_libraries al
                join requested_parent rp on rp.public_id = al.public_id
            ),
            parent_item as (
                select mi.id as item_id,
                       mi.library_id
                from media_items mi
                join allowed_libraries al on al.id = mi.library_id
                join requested_parent rp on rp.public_id = mi.public_id
                where mi.is_deleted = false
            ),
            parent_descendants as (
                select pi.item_id
                from parent_item pi
                union all
                select child.id
                from media_items child
                join parent_descendants parent on parent.item_id = child.parent_id
                where child.is_deleted = false
            ),
            studio_candidates as (
                select distinct
                    s.public_id::text as id,
                    s.name
                from media_item_studios mis
                join studios s on s.id = mis.studio_id
                join media_items mi on mi.id = mis.media_item_id
                join allowed_libraries al on al.id = mi.library_id
                left join parent_library pl on pl.library_id = mi.library_id
                left join parent_item pi on pi.library_id = mi.library_id
                where mi.is_deleted = false
                  and (
                        $2::text is null
                     or pl.library_id is not null
                     or (
                            pi.item_id is not null
                        and (
                                mi.id = pi.item_id
                             or ($5::boolean = false and mi.parent_id = pi.item_id)
                             or ($5::boolean = true and mi.id in (
                                    select item_id from parent_descendants
                                ))
                        )
                     )
                  )
                  and (
                        $6::text is null
                     or lower(s.name) like '%' || lower($6::text) || '%'
                  )
                  and (
                        $7::text is null
                     or lower(s.name) like lower($7::text) || '%'
                  )
                  and (
                        $8::text is null
                     or lower(s.name) >= lower($8::text)
                  )
            )
            select
                id,
                name,
                count(*) over() as total_record_count
            from studio_candidates
            order by
                case when $9 = 'asc' then lower(name) end asc nulls last,
                case when $9 = 'desc' then lower(name) end desc nulls last,
                id asc
            limit $3
            offset $4
            "#,
        )
        .bind(input.user_id)
        .bind(input.parent_id)
        .bind(input.limit)
        .bind(input.start_index)
        .bind(input.recursive)
        .bind(input.search_term)
        .bind(input.name_starts_with)
        .bind(input.name_starts_with_or_greater)
        .bind(input.sort_direction.as_sql_key())
        .fetch_all(&self.pool)
        .await?;

        studio_result_from_rows(rows)
    }

    pub async fn find_user_studio_by_name(
        &self,
        user_id: i64,
        name: &str,
    ) -> Result<Option<StudioRecord>, sqlx::Error> {
        sqlx::query(
            r#"
            select
                s.public_id::text as id,
                s.name,
                1::bigint as total_record_count
            from studios s
            where s.name_normalized = lower($2)
              and exists (
                    select 1
                    from media_item_studios mis
                    join media_items mi on mi.id = mis.media_item_id
                    join libraries l on l.id = mi.library_id
                    join library_permissions lp on lp.library_id = mi.library_id
                    where mis.studio_id = s.id
                      and lp.user_id = $1
                      and lp.can_view = true
                      and mi.is_deleted = false
                      and l.is_hidden = false
              )
            order by s.id
            limit 1
            "#,
        )
        .bind(user_id)
        .bind(name.trim().to_lowercase())
        .fetch_optional(&self.pool)
        .await?
        .map(StudioRecord::from_row)
        .transpose()
    }

    pub async fn list_user_tags(&self, input: TagListInput) -> Result<TagListResult, sqlx::Error> {
        let rows = sqlx::query(
            r#"
            with recursive requested_parent as (
                select case
                    when $2::text ~* '^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$'
                    then $2::uuid
                    else null::uuid
                end as public_id
            ),
            allowed_libraries as (
                select l.id,
                       l.public_id
                from libraries l
                join library_permissions lp on lp.library_id = l.id
                where lp.user_id = $1
                  and lp.can_view = true
                  and l.is_hidden = false
            ),
            parent_library as (
                select al.id as library_id
                from allowed_libraries al
                join requested_parent rp on rp.public_id = al.public_id
            ),
            parent_item as (
                select mi.id as item_id,
                       mi.library_id
                from media_items mi
                join allowed_libraries al on al.id = mi.library_id
                join requested_parent rp on rp.public_id = mi.public_id
                where mi.is_deleted = false
            ),
            parent_descendants as (
                select pi.item_id
                from parent_item pi
                union all
                select child.id
                from media_items child
                join parent_descendants parent on parent.item_id = child.parent_id
                where child.is_deleted = false
            ),
            tag_candidates as (
                select distinct
                    t.id::text as id,
                    t.name
                from media_item_tags mit
                join tags t on t.id = mit.tag_id
                join media_items mi on mi.id = mit.media_item_id
                join allowed_libraries al on al.id = mi.library_id
                left join parent_library pl on pl.library_id = mi.library_id
                left join parent_item pi on pi.library_id = mi.library_id
                where mi.is_deleted = false
                  and (
                        $2::text is null
                     or pl.library_id is not null
                     or (
                            pi.item_id is not null
                        and (
                                mi.id = pi.item_id
                             or ($5::boolean = false and mi.parent_id = pi.item_id)
                             or ($5::boolean = true and mi.id in (
                                    select item_id from parent_descendants
                                ))
                        )
                     )
                  )
                  and (
                        $6::text is null
                     or lower(t.name) like '%' || lower($6::text) || '%'
                  )
                  and (
                        $7::text is null
                     or lower(t.name) like lower($7::text) || '%'
                  )
                  and (
                        $8::text is null
                     or lower(t.name) >= lower($8::text)
                  )
            )
            select
                id,
                name,
                count(*) over() as total_record_count
            from tag_candidates
            order by
                case when $9 = 'asc' then lower(name) end asc nulls last,
                case when $9 = 'desc' then lower(name) end desc nulls last,
                id asc
            limit $3
            offset $4
            "#,
        )
        .bind(input.user_id)
        .bind(input.parent_id)
        .bind(input.limit)
        .bind(input.start_index)
        .bind(input.recursive)
        .bind(input.search_term)
        .bind(input.name_starts_with)
        .bind(input.name_starts_with_or_greater)
        .bind(input.sort_direction.as_sql_key())
        .fetch_all(&self.pool)
        .await?;

        tag_result_from_rows(rows)
    }

    pub async fn list_user_official_ratings(
        &self,
        input: OfficialRatingListInput,
    ) -> Result<OfficialRatingListResult, sqlx::Error> {
        let rows = sqlx::query(
            r#"
            with recursive requested_parent as (
                select case
                    when $2::text ~* '^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$'
                    then $2::uuid
                    else null::uuid
                end as public_id
            ),
            allowed_libraries as (
                select l.id,
                       l.public_id
                from libraries l
                join library_permissions lp on lp.library_id = l.id
                where lp.user_id = $1
                  and lp.can_view = true
                  and l.is_hidden = false
            ),
            parent_library as (
                select al.id as library_id
                from allowed_libraries al
                join requested_parent rp on rp.public_id = al.public_id
            ),
            parent_item as (
                select mi.id as item_id,
                       mi.library_id
                from media_items mi
                join allowed_libraries al on al.id = mi.library_id
                join requested_parent rp on rp.public_id = mi.public_id
                where mi.is_deleted = false
            ),
            parent_descendants as (
                select pi.item_id
                from parent_item pi
                union all
                select child.id
                from media_items child
                join parent_descendants parent on parent.item_id = child.parent_id
                where child.is_deleted = false
            ),
            rating_candidates as (
                select distinct
                    btrim(mi.official_rating) as name
                from media_items mi
                join allowed_libraries al on al.id = mi.library_id
                left join parent_library pl on pl.library_id = mi.library_id
                left join parent_item pi on pi.library_id = mi.library_id
                where mi.is_deleted = false
                  and mi.official_rating is not null
                  and length(btrim(mi.official_rating)) > 0
                  and (
                        $2::text is null
                     or pl.library_id is not null
                     or (
                            pi.item_id is not null
                        and (
                                mi.id = pi.item_id
                             or ($5::boolean = false and mi.parent_id = pi.item_id)
                             or ($5::boolean = true and mi.id in (
                                    select item_id from parent_descendants
                                ))
                        )
                     )
                  )
                  and (
                        $6::text is null
                     or lower(mi.official_rating) like '%' || lower($6::text) || '%'
                  )
                  and (
                        $7::text is null
                     or lower(mi.official_rating) like lower($7::text) || '%'
                  )
                  and (
                        $8::text is null
                     or lower(mi.official_rating) >= lower($8::text)
                  )
            )
            select
                name,
                count(*) over() as total_record_count
            from rating_candidates
            order by
                case when $9 = 'asc' then lower(name) end asc nulls last,
                case when $9 = 'desc' then lower(name) end desc nulls last,
                name asc
            limit $3
            offset $4
            "#,
        )
        .bind(input.user_id)
        .bind(input.parent_id)
        .bind(input.limit)
        .bind(input.start_index)
        .bind(input.recursive)
        .bind(input.search_term)
        .bind(input.name_starts_with)
        .bind(input.name_starts_with_or_greater)
        .bind(input.sort_direction.as_sql_key())
        .fetch_all(&self.pool)
        .await?;

        official_rating_result_from_rows(rows)
    }

    pub async fn list_user_years(
        &self,
        input: YearListInput,
    ) -> Result<YearListResult, sqlx::Error> {
        let rows = sqlx::query(
            r#"
            with recursive requested_parent as (
                select case
                    when $2::text ~* '^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$'
                    then $2::uuid
                    else null::uuid
                end as public_id
            ),
            allowed_libraries as (
                select l.id,
                       l.public_id
                from libraries l
                join library_permissions lp on lp.library_id = l.id
                where lp.user_id = $1
                  and lp.can_view = true
                  and l.is_hidden = false
            ),
            parent_library as (
                select al.id as library_id
                from allowed_libraries al
                join requested_parent rp on rp.public_id = al.public_id
            ),
            parent_item as (
                select mi.id as item_id,
                       mi.library_id
                from media_items mi
                join allowed_libraries al on al.id = mi.library_id
                join requested_parent rp on rp.public_id = mi.public_id
                where mi.is_deleted = false
            ),
            parent_descendants as (
                select pi.item_id
                from parent_item pi
                union all
                select child.id
                from media_items child
                join parent_descendants parent on parent.item_id = child.parent_id
                where child.is_deleted = false
            ),
            year_candidates as (
                select distinct
                    mi.production_year as year
                from media_items mi
                join allowed_libraries al on al.id = mi.library_id
                left join parent_library pl on pl.library_id = mi.library_id
                left join parent_item pi on pi.library_id = mi.library_id
                where mi.is_deleted = false
                  and mi.production_year is not null
                  and (
                        $2::text is null
                     or pl.library_id is not null
                     or (
                            pi.item_id is not null
                        and (
                                mi.id = pi.item_id
                             or ($5::boolean = false and mi.parent_id = pi.item_id)
                             or ($5::boolean = true and mi.id in (
                                    select item_id from parent_descendants
                                ))
                        )
                     )
                  )
                  and (
                        $6::text is null
                     or mi.production_year::text like '%' || $6::text || '%'
                  )
                  and (
                        $7::text is null
                     or mi.production_year::text like $7::text || '%'
                  )
                  and (
                        $8::text is null
                     or mi.production_year::text >= $8::text
                  )
            )
            select
                year,
                count(*) over() as total_record_count
            from year_candidates
            order by
                case when $9 = 'asc' then year end asc nulls last,
                case when $9 = 'desc' then year end desc nulls last,
                year asc
            limit $3
            offset $4
            "#,
        )
        .bind(input.user_id)
        .bind(input.parent_id)
        .bind(input.limit)
        .bind(input.start_index)
        .bind(input.recursive)
        .bind(input.search_term)
        .bind(input.name_starts_with)
        .bind(input.name_starts_with_or_greater)
        .bind(input.sort_direction.as_sql_key())
        .fetch_all(&self.pool)
        .await?;

        year_result_from_rows(rows)
    }

    pub async fn list_user_technical_facets(
        &self,
        input: TechnicalFacetListInput,
    ) -> Result<TechnicalFacetListResult, sqlx::Error> {
        let rows = sqlx::query(
            r#"
            with recursive requested_parent as (
                select case
                    when $2::text ~* '^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$'
                    then $2::uuid
                    else null::uuid
                end as public_id
            ),
            allowed_libraries as (
                select l.id,
                       l.public_id
                from libraries l
                join library_permissions lp on lp.library_id = l.id
                where lp.user_id = $1
                  and lp.can_view = true
                  and l.is_hidden = false
            ),
            parent_library as (
                select al.id as library_id
                from allowed_libraries al
                join requested_parent rp on rp.public_id = al.public_id
            ),
            parent_item as (
                select mi.id as item_id,
                       mi.library_id
                from media_items mi
                join allowed_libraries al on al.id = mi.library_id
                join requested_parent rp on rp.public_id = mi.public_id
                where mi.is_deleted = false
            ),
            parent_descendants as (
                select pi.item_id
                from parent_item pi
                union all
                select child.id
                from media_items child
                join parent_descendants parent on parent.item_id = child.parent_id
                where child.is_deleted = false
            ),
            item_scope as (
                select mi.id
                from media_items mi
                join allowed_libraries al on al.id = mi.library_id
                left join parent_library pl on pl.library_id = mi.library_id
                left join parent_item pi on pi.library_id = mi.library_id
                where mi.is_deleted = false
                  and (
                        $2::text is null
                     or pl.library_id is not null
                     or (
                            pi.item_id is not null
                        and (
                                mi.id = pi.item_id
                             or ($5::boolean = false and mi.parent_id = pi.item_id)
                             or ($5::boolean = true and mi.id in (
                                    select item_id from parent_descendants
                                ))
                        )
                     )
                  )
            ),
            facet_candidates as (
                select distinct
                    btrim(mf.container) as name
                from item_scope mi
                join media_files mf on mf.media_item_id = mi.id
                where $9 = 'container'
                  and mf.container is not null
                  and length(btrim(mf.container)) > 0

                union all

                select distinct
                    btrim(ms.codec) as name
                from item_scope mi
                join media_files mf on mf.media_item_id = mi.id
                join media_streams ms on ms.media_file_id = mf.id
                where $9 = 'audio_codec'
                  and ms.stream_type = 'audio'
                  and ms.codec is not null
                  and length(btrim(ms.codec)) > 0

                union all

                select distinct
                    btrim(ms.codec) as name
                from item_scope mi
                join media_files mf on mf.media_item_id = mi.id
                join media_streams ms on ms.media_file_id = mf.id
                where $9 = 'video_codec'
                  and ms.stream_type = 'video'
                  and ms.codec is not null
                  and length(btrim(ms.codec)) > 0

                union all

                select distinct
                    btrim(ms.codec) as name
                from item_scope mi
                join media_files mf on mf.media_item_id = mi.id
                join media_streams ms on ms.media_file_id = mf.id
                where $9 = 'subtitle_codec'
                  and ms.stream_type = 'subtitle'
                  and ms.codec is not null
                  and length(btrim(ms.codec)) > 0

                union all

                select distinct
                    btrim(ms.language) as name
                from item_scope mi
                join media_files mf on mf.media_item_id = mi.id
                join media_streams ms on ms.media_file_id = mf.id
                where $9 = 'stream_language'
                  and ms.language is not null
                  and length(btrim(ms.language)) > 0
            ),
            filtered_facets as (
                select name
                from facet_candidates
                where (
                        $6::text is null
                     or lower(name) like '%' || lower($6::text) || '%'
                  )
                  and (
                        $7::text is null
                     or lower(name) like lower($7::text) || '%'
                  )
                  and (
                        $8::text is null
                     or lower(name) >= lower($8::text)
                  )
            )
            select
                name as id,
                name,
                count(*) over() as total_record_count
            from filtered_facets
            order by
                case when $10 = 'asc' then lower(name) end asc nulls last,
                case when $10 = 'desc' then lower(name) end desc nulls last,
                name asc
            limit $3
            offset $4
            "#,
        )
        .bind(input.user_id)
        .bind(input.parent_id)
        .bind(input.limit)
        .bind(input.start_index)
        .bind(input.recursive)
        .bind(input.search_term)
        .bind(input.name_starts_with)
        .bind(input.name_starts_with_or_greater)
        .bind(input.kind.as_sql_key())
        .bind(input.sort_direction.as_sql_key())
        .fetch_all(&self.pool)
        .await?;

        technical_facet_result_from_rows(rows)
    }

    pub async fn list_user_artists(
        &self,
        input: ArtistListInput,
    ) -> Result<ArtistListResult, sqlx::Error> {
        let rows = sqlx::query(
            r#"
            with recursive requested_parent as (
                select case
                    when $2::text ~* '^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$'
                    then $2::uuid
                    else null::uuid
                end as public_id
            ),
            requested_album_ids as (
                select distinct album_id::uuid as public_id
                from unnest($14::text[]) as album_id
                where album_id ~* '^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$'
            ),
            allowed_libraries as (
                select l.id,
                       l.public_id,
                       l.library_type
                from libraries l
                join library_permissions lp on lp.library_id = l.id
                where lp.user_id = $1
                  and lp.can_view = true
                  and l.is_hidden = false
            ),
            parent_library as (
                select al.id as library_id
                from allowed_libraries al
                join requested_parent rp on rp.public_id = al.public_id
            ),
            parent_item as (
                select mi.id as item_id,
                       mi.library_id
                from media_items mi
                join allowed_libraries al on al.id = mi.library_id
                join requested_parent rp on rp.public_id = mi.public_id
                where mi.is_deleted = false
            ),
            parent_descendants as (
                select pi.item_id
                from parent_item pi
                union all
                select child.id
                from media_items child
                join parent_descendants parent on parent.item_id = child.parent_id
                where child.is_deleted = false
            ),
            visible_music_items as (
                select
                    mi.id,
                    mi.public_id,
                    mi.parent_id,
                    mi.title,
                    mi.item_type
                from media_items mi
                join allowed_libraries al on al.id = mi.library_id
                left join parent_library pl on pl.library_id = mi.library_id
                left join parent_item pi on pi.library_id = mi.library_id
                where mi.is_deleted = false
                  and (
                        al.library_type = 'music'
                     or mi.item_type in ('artist', 'album', 'track')
                  )
                  and (
                        $2::text is null
                     or pl.library_id is not null
                     or (
                            pi.item_id is not null
                        and (
                                mi.id = pi.item_id
                             or ($5::boolean = false and mi.parent_id = pi.item_id)
                             or ($5::boolean = true and mi.id in (
                                    select item_id from parent_descendants
                                ))
                        )
                     )
                  )
            ),
            raw_artists as (
                select
                    p.public_id::text as id,
                    p.name,
                    1 as source_priority
                from visible_music_items mi
                left join media_items album_parent
                  on album_parent.id = mi.parent_id
                 and album_parent.item_type = 'album'
                 and album_parent.is_deleted = false
                join media_item_people mip on mip.media_item_id = mi.id
                join people p on p.id = mip.person_id
                where mip.role_type = 'artist'
                  and (
                        $6::boolean = false
                     or mi.item_type in ('artist', 'album', 'track')
                  )
                  and (
                        ($11::boolean = false and $13::boolean = false)
                     or (
                            mi.item_type = 'album'
                        and $11::boolean = true
                        and lower(mi.title) = any($12::text[])
                     )
                     or (
                            album_parent.id is not null
                        and $11::boolean = true
                        and lower(album_parent.title) = any($12::text[])
                     )
                     or (
                            mi.item_type = 'album'
                        and $13::boolean = true
                        and exists (
                            select 1
                            from requested_album_ids requested
                            where requested.public_id = mi.public_id
                        )
                     )
                     or (
                            album_parent.id is not null
                        and $13::boolean = true
                        and exists (
                            select 1
                            from requested_album_ids requested
                            where requested.public_id = album_parent.public_id
                        )
                     )
                  )
                union
                select
                    mi.public_id::text as id,
                    mi.title as name,
                    0 as source_priority
                from visible_music_items mi
                where mi.item_type = 'artist'
                  and $11::boolean = false
                  and $13::boolean = false
            ),
            artist_candidates as (
                select distinct on (lower(name)) id, name
                from raw_artists
                where (
                        $7::text is null
                     or lower(name) like '%' || lower($7::text) || '%'
                  )
                  and (
                        $8::text is null
                     or lower(name) like lower($8::text) || '%'
                  )
                  and (
                        $9::text is null
                     or lower(name) >= lower($9::text)
                  )
                order by lower(name), source_priority, id
            )
            select
                id,
                name,
                count(*) over() as total_record_count
            from artist_candidates
            order by
                case when $10 = 'asc' then lower(name) end asc nulls last,
                case when $10 = 'desc' then lower(name) end desc nulls last,
                id asc
            limit $3
            offset $4
            "#,
        )
        .bind(input.user_id)
        .bind(input.parent_id)
        .bind(input.limit)
        .bind(input.start_index)
        .bind(input.recursive)
        .bind(input.album_artists_only)
        .bind(input.search_term)
        .bind(input.name_starts_with)
        .bind(input.name_starts_with_or_greater)
        .bind(input.sort_direction.as_sql_key())
        .bind(input.album_names.enabled)
        .bind(&input.album_names.values)
        .bind(input.album_ids.enabled)
        .bind(&input.album_ids.values)
        .fetch_all(&self.pool)
        .await?;

        artist_result_from_rows(rows)
    }

    pub async fn find_user_artist_by_name(
        &self,
        user_id: i64,
        name: &str,
        album_artists_only: bool,
    ) -> Result<Option<ArtistRecord>, sqlx::Error> {
        sqlx::query(
            r#"
            with allowed_libraries as (
                select l.id,
                       l.library_type
                from libraries l
                join library_permissions lp on lp.library_id = l.id
                where lp.user_id = $1
                  and lp.can_view = true
                  and l.is_hidden = false
            ),
            visible_music_items as (
                select
                    mi.id,
                    mi.public_id,
                    mi.title,
                    mi.item_type
                from media_items mi
                join allowed_libraries al on al.id = mi.library_id
                where mi.is_deleted = false
                  and (
                        al.library_type = 'music'
                     or mi.item_type in ('artist', 'album', 'track')
                  )
            ),
            raw_artists as (
                select
                    p.public_id::text as id,
                    p.name,
                    1 as source_priority
                from visible_music_items mi
                join media_item_people mip on mip.media_item_id = mi.id
                join people p on p.id = mip.person_id
                where mip.role_type = 'artist'
                  and (
                        $3::boolean = false
                     or mi.item_type in ('artist', 'album', 'track')
                  )
                union
                select
                    mi.public_id::text as id,
                    mi.title as name,
                    0 as source_priority
                from visible_music_items mi
                where mi.item_type = 'artist'
            )
            select
                id,
                name,
                1::bigint as total_record_count
            from raw_artists
            where lower(name) = lower($2)
            order by source_priority, id
            limit 1
            "#,
        )
        .bind(user_id)
        .bind(name.trim())
        .bind(album_artists_only)
        .fetch_optional(&self.pool)
        .await?
        .map(ArtistRecord::from_row)
        .transpose()
    }

    pub async fn list_user_persons(
        &self,
        input: PersonListInput,
    ) -> Result<PersonListResult, sqlx::Error> {
        let rows = sqlx::query(
            r#"
            with recursive requested_parent as (
                select case
                    when $2::text ~* '^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$'
                    then $2::uuid
                    else null::uuid
                end as public_id
            ),
            allowed_libraries as (
                select l.id,
                       l.public_id
                from libraries l
                join library_permissions lp on lp.library_id = l.id
                where lp.user_id = $1
                  and lp.can_view = true
                  and l.is_hidden = false
            ),
            parent_library as (
                select al.id as library_id
                from allowed_libraries al
                join requested_parent rp on rp.public_id = al.public_id
            ),
            parent_item as (
                select mi.id as item_id,
                       mi.library_id
                from media_items mi
                join allowed_libraries al on al.id = mi.library_id
                join requested_parent rp on rp.public_id = mi.public_id
                where mi.is_deleted = false
            ),
            parent_descendants as (
                select pi.item_id
                from parent_item pi
                union all
                select child.id
                from media_items child
                join parent_descendants parent on parent.item_id = child.parent_id
                where child.is_deleted = false
            ),
            person_candidates as (
                select distinct
                    p.public_id::text as id,
                    p.name
                from media_item_people mip
                join people p on p.id = mip.person_id
                join media_items mi on mi.id = mip.media_item_id
                join allowed_libraries al on al.id = mi.library_id
                left join parent_library pl on pl.library_id = mi.library_id
                left join parent_item pi on pi.library_id = mi.library_id
                where mi.is_deleted = false
                  and (
                        $6::boolean = false
                     or mip.role_type = any($7::text[])
                  )
                  and (
                        $2::text is null
                     or pl.library_id is not null
                     or (
                            pi.item_id is not null
                        and (
                                mi.id = pi.item_id
                             or ($5::boolean = false and mi.parent_id = pi.item_id)
                             or ($5::boolean = true and mi.id in (
                                    select item_id from parent_descendants
                                ))
                        )
                     )
                  )
                  and (
                        $8::text is null
                     or lower(p.name) like '%' || lower($8::text) || '%'
                  )
                  and (
                        $9::text is null
                     or lower(p.name) like lower($9::text) || '%'
                  )
                  and (
                        $10::text is null
                     or lower(p.name) >= lower($10::text)
                  )
            )
            select
                id,
                name,
                count(*) over() as total_record_count
            from person_candidates
            order by
                case when $11 = 'asc' then lower(name) end asc nulls last,
                case when $11 = 'desc' then lower(name) end desc nulls last,
                id asc
            limit $3
            offset $4
            "#,
        )
        .bind(input.user_id)
        .bind(input.parent_id)
        .bind(input.limit)
        .bind(input.start_index)
        .bind(input.recursive)
        .bind(input.role_filter.enabled)
        .bind(&input.role_filter.role_types)
        .bind(input.search_term)
        .bind(input.name_starts_with)
        .bind(input.name_starts_with_or_greater)
        .bind(input.sort_direction.as_sql_key())
        .fetch_all(&self.pool)
        .await?;

        person_result_from_rows(rows)
    }

    pub async fn find_user_person_by_name(
        &self,
        user_id: i64,
        name: &str,
    ) -> Result<Option<PersonRecord>, sqlx::Error> {
        sqlx::query(
            r#"
            select
                p.public_id::text as id,
                p.name,
                1::bigint as total_record_count
            from people p
            where lower(p.name) = lower($2)
              and exists (
                    select 1
                    from media_item_people mip
                    join media_items mi on mi.id = mip.media_item_id
                    join libraries l on l.id = mi.library_id
                    join library_permissions lp on lp.library_id = mi.library_id
                    where mip.person_id = p.id
                      and lp.user_id = $1
                      and lp.can_view = true
                      and mi.is_deleted = false
                      and l.is_hidden = false
              )
            order by p.id
            limit 1
            "#,
        )
        .bind(user_id)
        .bind(name.trim())
        .fetch_optional(&self.pool)
        .await?
        .map(PersonRecord::from_row)
        .transpose()
    }

    pub async fn list_user_items(
        &self,
        input: BrowseItemsInput,
    ) -> Result<BrowseItemsResult, sqlx::Error> {
        if input.can_use_positive_playstate_fast_path() {
            return self.list_user_items_from_playstates(&input).await;
        }
        if input.can_use_include_ids_fast_path() {
            return self.list_user_items_by_include_ids(&input).await;
        }
        if input.can_use_provider_id_fast_path() {
            return self.list_user_items_by_provider_ids(&input).await;
        }

        let rows = sqlx::query(
            r#"
            with requested_parent as (
                select case
                    when $2::text is null then null::uuid
                    when $2::text ~* '^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$'
                    then $2::uuid
                    else null::uuid
                end as public_id
            ),
            requested_include_ids as (
                select distinct item_id::uuid as public_id
                from unnest($10::text[]) as item_id
                where item_id ~* '^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$'
            ),
            requested_exclude_ids as (
                select distinct item_id::uuid as public_id
                from unnest($12::text[]) as item_id
                where item_id ~* '^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$'
            ),
            requested_genre_ids as (
                select distinct genre_id::bigint as id
                from unnest($22::text[]) as genre_id
                where genre_id ~ '^[0-9]{1,18}$'
            ),
            requested_person_ids as (
                select distinct person_id::uuid as public_id
                from unnest($26::text[]) as person_id
                where person_id ~* '^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$'
            ),
            requested_artist_ids as (
                select distinct artist_id::uuid as public_id
                from unnest($32::text[]) as artist_id
                where artist_id ~* '^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$'
            ),
            requested_album_ids as (
                select distinct album_id::uuid as public_id
                from unnest($75::text[]) as album_id
                where album_id ~* '^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$'
            ),
            requested_studio_ids as (
                select distinct studio_id::uuid as public_id
                from unnest($65::text[]) as studio_id
                where studio_id ~* '^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$'
            ),
            parent_library as (
                select l.id as library_id,
                       (u.allow_transcode and lp.can_transcode) as supports_transcoding
                from libraries l
                join library_permissions lp on lp.library_id = l.id
                join users u on u.id = lp.user_id
                cross join requested_parent rp
                where ($2::text is null or l.public_id = rp.public_id)
                  and lp.user_id = $1
                  and lp.can_view = true
                  and l.is_hidden = false
            ),
            parent_item as (
                select mi.id as item_id,
                       mi.library_id,
                       (u.allow_transcode and lp.can_transcode) as supports_transcoding
                from media_items mi
                join libraries l on l.id = mi.library_id
                join library_permissions lp on lp.library_id = mi.library_id
                join users u on u.id = lp.user_id
                cross join requested_parent rp
                where $2::text is not null
                  and mi.public_id = rp.public_id
                  and lp.user_id = $1
                  and lp.can_view = true
                  and mi.is_deleted = false
                  and l.is_hidden = false
            ),
            browse_scope as (
                select library_id,
                       null::bigint as parent_item_id,
                       true as is_library,
                       supports_transcoding
                from parent_library
                union all
                select library_id,
                       item_id as parent_item_id,
                       false as is_library,
                       supports_transcoding
                from parent_item
            )
            select
                mi.public_id::text as id,
                mi.title as name,
                mi.item_type,
                parent.public_id::text as parent_id,
                coalesce(mi.runtime_ticks, primary_file.duration_ticks) as runtime_ticks,
                primary_file.media_file_id,
                primary_file.file_size as media_file_size,
                primary_file.container as media_file_container,
                primary_file.bitrate as media_file_bitrate,
                primary_file.is_strm as media_file_is_strm,
                coalesce(scope.supports_transcoding, false) as supports_transcoding,
                mi.production_year,
                coalesce(up.position_ticks, 0) as playback_position_ticks,
                coalesce(up.play_count, 0) as play_count,
                coalesce(up.is_favorite, false) as is_favorite,
                up.rating::double precision as rating,
                coalesce(up.played, false) as played,
                case
                    when $6::boolean = true then coalesce(item_images.image_tags, array[]::text[])
                    else array[]::text[]
                end as image_tags,
                count(*) over() as total_record_count
            from media_items mi
            join browse_scope scope on scope.library_id = mi.library_id
            left join media_items parent on parent.id = mi.parent_id
            left join lateral (
                select mf.id as media_file_id,
                       mf.file_size,
                       mf.container,
                       mf.duration_ticks,
                       mf.bitrate,
                       mf.is_strm
                from media_files mf
                where mf.media_item_id = mi.id
                order by mf.is_primary desc, mf.id
                limit 1
            ) primary_file on true
            left join user_playstates up on up.user_id = $1
                and up.media_item_id = mi.id
            left join lateral (
                select array_agg(a.artwork_type || '=' || a.id::text order by a.artwork_type, a.is_primary desc, a.id) as image_tags
                from artwork a
                where a.media_item_id = mi.id
            ) item_images on $6::boolean = true
            where mi.is_deleted = false
              and ($7::boolean = false or mi.item_type = any($8::text[]))
              and (
                    $9::boolean = false
                 or exists (
                        select 1
                        from requested_include_ids requested
                        where requested.public_id = mi.public_id
                 )
              )
              and (
                    $11::boolean = false
                 or not exists (
                        select 1
                        from requested_exclude_ids requested
                        where requested.public_id = mi.public_id
                 )
              )
              and ($13::boolean = false or mi.production_year = any($14::integer[]))
              and (
                    $15::text is null
                 or lower(mi.title) like '%' || lower($15::text) || '%'
                 or lower(mi.original_title) like '%' || lower($15::text) || '%'
                 or lower(mi.sort_title) like '%' || lower($15::text) || '%'
              )
              and (
                    $16::text is null
                 or lower(coalesce(nullif(mi.sort_title, ''), mi.title)) like lower($16::text) || '%'
              )
              and (
                    $17::text is null
                 or lower(coalesce(nullif(mi.sort_title, ''), mi.title)) >= lower($17::text)
              )
              and (
                    $18::text is null
                 or lower(coalesce(nullif(mi.sort_title, ''), mi.title)) < lower($18::text)
              )
              and (
                    ($19::boolean = false and $21::boolean = false)
                 or exists (
                        select 1
                        from media_item_genres mig
                        join genres g on g.id = mig.genre_id
                        where mig.media_item_id = mi.id
                          and ($19::boolean = false or lower(g.name) = any($20::text[]))
                          and (
                              $21::boolean = false
                              or exists (
                                  select 1
                                  from requested_genre_ids requested
                                  where requested.id = mig.genre_id
                              )
                          )
                 )
              )
              and (
                    ($23::boolean = false and $25::boolean = false and $27::boolean = false)
                 or exists (
                        select 1
                        from media_item_people mip
                        join people p on p.id = mip.person_id
                        where mip.media_item_id = mi.id
                          and ($23::boolean = false or lower(p.name) = any($24::text[]))
                          and (
                              $25::boolean = false
                              or exists (
                                  select 1
                                  from requested_person_ids requested
                                  where requested.public_id = p.public_id
                              )
                          )
                          and ($27::boolean = false or mip.role_type = any($28::text[]))
                 )
              )
              and (
                    $29::boolean = false
                 or (
                        mi.item_type = 'artist'
                    and lower(mi.title) = any($30::text[])
                 )
                 or exists (
                        select 1
                        from media_item_people mip
                        join people p on p.id = mip.person_id
                        where mip.media_item_id = mi.id
                          and mip.role_type = 'artist'
                          and lower(p.name) = any($30::text[])
                 )
              )
              and (
                    $31::boolean = false
                 or (
                        mi.item_type = 'artist'
                    and exists (
                        select 1
                        from requested_artist_ids requested
                        where requested.public_id = mi.public_id
                    )
                 )
                 or exists (
                        select 1
                        from media_item_people mip
                        join people p on p.id = mip.person_id
                        where mip.media_item_id = mi.id
                          and mip.role_type = 'artist'
                          and (
                                exists (
                                    select 1
                                    from requested_artist_ids requested
                                    where requested.public_id = p.public_id
                                )
                             or exists (
                                    select 1
                                    from media_items artist_item
                                    join requested_artist_ids requested
                                      on requested.public_id = artist_item.public_id
                                    join libraries artist_library on artist_library.id = artist_item.library_id
                                    join library_permissions artist_lp on artist_lp.library_id = artist_item.library_id
                                    where artist_item.item_type = 'artist'
                                      and artist_lp.user_id = $1
                                      and artist_lp.can_view = true
                                      and artist_item.is_deleted = false
                                      and artist_library.is_hidden = false
                                      and lower(artist_item.title) = lower(p.name)
                             )
                          )
                 )
              )
              and (
                    ($72::boolean = false and $74::boolean = false)
                 or (
                        mi.item_type = 'album'
                    and $72::boolean = true
                    and lower(mi.title) = any($73::text[])
                 )
                 or (
                        parent.item_type = 'album'
                    and $72::boolean = true
                    and lower(parent.title) = any($73::text[])
                 )
                 or (
                        mi.item_type = 'album'
                    and $74::boolean = true
                    and exists (
                        select 1
                        from requested_album_ids requested
                        where requested.public_id = mi.public_id
                    )
                 )
                 or (
                        parent.item_type = 'album'
                    and $74::boolean = true
                    and exists (
                        select 1
                        from requested_album_ids requested
                        where requested.public_id = parent.public_id
                    )
                 )
              )
              and ($33::boolean is null or coalesce(up.played, false) = $33::boolean)
              and ($34::boolean is null or coalesce(up.is_favorite, false) = $34::boolean)
              and ($35::boolean = false or coalesce(up.played, false) = true)
              and ($36::boolean = false or coalesce(up.played, false) = false)
              and ($37::boolean = false or coalesce(up.is_favorite, false) = true)
              and (
                    $38::boolean = false
                 or (
                        coalesce(up.position_ticks, 0) > 0
                    and coalesce(up.played, false) = false
                 )
              )
              and ($39::boolean = false or up.rating >= 5)
              and ($40::boolean = false or up.rating < 5)
              and (
                    $41::boolean is null
                 or (
                        mi.item_type in ('folder', 'series', 'season', 'artist', 'album', 'collection')
                    ) = $41::boolean
              )
              and ($42::boolean is null or (mi.item_type = 'movie') = $42::boolean)
              and ($43::boolean is null or (mi.item_type = 'series') = $43::boolean)
              and (
                    $44::boolean = false
                 or mi.item_type in ('folder', 'series', 'season', 'artist', 'album', 'collection')
              )
              and (
                    $45::boolean = false
                 or mi.item_type not in ('folder', 'series', 'season', 'artist', 'album', 'collection')
              )
              and (
                    $46::boolean = false
                 or (
                        ('video' = any($47::text[]) and mi.item_type in ('movie', 'series', 'season', 'episode'))
                     or ('audio' = any($47::text[]) and mi.item_type in ('artist', 'album', 'track'))
                 )
              )
              and (
                    $48::boolean = false
                 or exists (
                        select 1
                        from media_files mf_container
                        where mf_container.media_item_id = mi.id
                          and mf_container.container is not null
                          and (
                                lower(mf_container.container) = any($49::text[])
                             or exists (
                                    select 1
                                    from unnest(string_to_array(lower(mf_container.container), ',')) as container_token(value)
                                    where btrim(container_token.value) = any($49::text[])
                             )
                          )
                 )
              )
              and (
                    $50::boolean = false
                 or exists (
                        select 1
                        from media_files mf_audio
                        join media_streams ms_audio on ms_audio.media_file_id = mf_audio.id
                        where mf_audio.media_item_id = mi.id
                          and ms_audio.stream_type = 'audio'
                          and lower(ms_audio.codec) = any($51::text[])
                 )
              )
              and (
                    $52::boolean = false
                 or exists (
                        select 1
                        from media_files mf_video
                        join media_streams ms_video on ms_video.media_file_id = mf_video.id
                        where mf_video.media_item_id = mi.id
                          and ms_video.stream_type = 'video'
                          and lower(ms_video.codec) = any($53::text[])
                 )
              )
              and (
                    $54::boolean = false
                 or exists (
                        select 1
                        from media_files mf_subtitle
                        join media_streams ms_subtitle on ms_subtitle.media_file_id = mf_subtitle.id
                        where mf_subtitle.media_item_id = mi.id
                          and ms_subtitle.stream_type = 'subtitle'
                          and lower(ms_subtitle.codec) = any($55::text[])
                 )
              )
              and (
                    $56::boolean = false
                 or lower(mi.official_rating) = any($57::text[])
              )
              and (
                    $58::boolean = false
                 or exists (
                        select 1
                        from media_item_tags mit
                        join tags t on t.id = mit.tag_id
                        where mit.media_item_id = mi.id
                          and t.name_normalized = any($59::text[])
                 )
              )
              and (
                    $60::boolean = false
                 or not exists (
                        select 1
                        from media_item_tags exclude_mit
                        join tags exclude_t on exclude_t.id = exclude_mit.tag_id
                        where exclude_mit.media_item_id = mi.id
                          and exclude_t.name_normalized = any($61::text[])
                 )
              )
              and (
                    ($62::boolean = false and $64::boolean = false)
                 or exists (
                        select 1
                        from media_item_studios mis
                        join studios s on s.id = mis.studio_id
                        where mis.media_item_id = mi.id
                          and ($62::boolean = false or s.name_normalized = any($63::text[]))
                          and (
                              $64::boolean = false
                              or exists (
                                  select 1
                                  from requested_studio_ids requested
                                  where requested.public_id = s.public_id
                              )
                          )
                 )
              )
              and (
                    $66::boolean = false
                 or exists (
                        select 1
                        from media_external_ids mei
                        where mei.media_item_id = mi.id
                          and lower(mei.provider || '.' || mei.external_id) = any($67::text[])
                 )
              )
              and (
                    $68::boolean = false
                 or exists (
                        select 1
                        from artwork filter_artwork
                        where filter_artwork.media_item_id = mi.id
                          and filter_artwork.artwork_type = any($69::text[])
                 )
              )
              and (
                    (scope.is_library = true and (($5 = true) or mi.parent_id is null))
                 or (scope.is_library = false and mi.parent_id = scope.parent_item_id)
              )
            order by
                case when $70 = 'sort_name' and $71 = 'asc' then coalesce(nullif(mi.sort_title, ''), mi.title) end asc nulls last,
                case when $70 = 'sort_name' and $71 = 'desc' then coalesce(nullif(mi.sort_title, ''), mi.title) end desc nulls last,
                case when $70 = 'date_created' and $71 = 'asc' then mi.created_at end asc nulls last,
                case when $70 = 'date_created' and $71 = 'desc' then mi.created_at end desc nulls last,
                case when $70 = 'runtime' and $71 = 'asc' then coalesce(mi.runtime_ticks, primary_file.duration_ticks) end asc nulls last,
                case when $70 = 'runtime' and $71 = 'desc' then coalesce(mi.runtime_ticks, primary_file.duration_ticks) end desc nulls last,
                case when $70 = 'production_year' and $71 = 'asc' then mi.production_year end asc nulls last,
                case when $70 = 'production_year' and $71 = 'desc' then mi.production_year end desc nulls last,
                case when $70 = 'index_number' and $71 = 'asc' then mi.parent_index_number end asc nulls last,
                case when $70 = 'index_number' and $71 = 'asc' then mi.index_number end asc nulls last,
                case when $70 = 'index_number' and $71 = 'desc' then mi.parent_index_number end desc nulls last,
                case when $70 = 'index_number' and $71 = 'desc' then mi.index_number end desc nulls last,
                case when $71 = 'asc' then mi.id end asc,
                case when $71 = 'desc' then mi.id end desc,
                mi.id asc
            limit $3
            offset $4
            "#,
        )
        .bind(input.user_id)
        .bind(input.parent_id)
        .bind(input.limit)
        .bind(input.start_index)
        .bind(input.recursive)
        .bind(input.include_image_tags)
        .bind(input.options.type_filter.enabled)
        .bind(&input.options.type_filter.item_types)
        .bind(input.options.scalar_filter.include_ids.enabled)
        .bind(&input.options.scalar_filter.include_ids.values)
        .bind(input.options.scalar_filter.exclude_ids.enabled)
        .bind(&input.options.scalar_filter.exclude_ids.values)
        .bind(input.options.scalar_filter.years.enabled)
        .bind(&input.options.scalar_filter.years.values)
        .bind(input.options.scalar_filter.search_term)
        .bind(input.options.scalar_filter.name_starts_with)
        .bind(input.options.scalar_filter.name_starts_with_or_greater)
        .bind(input.options.scalar_filter.name_less_than)
        .bind(input.options.association_filter.genre_names.enabled)
        .bind(&input.options.association_filter.genre_names.values)
        .bind(input.options.association_filter.genre_ids.enabled)
        .bind(&input.options.association_filter.genre_ids.values)
        .bind(input.options.association_filter.person_names.enabled)
        .bind(&input.options.association_filter.person_names.values)
        .bind(input.options.association_filter.person_ids.enabled)
        .bind(&input.options.association_filter.person_ids.values)
        .bind(input.options.association_filter.person_role_types.enabled)
        .bind(&input.options.association_filter.person_role_types.role_types)
        .bind(input.options.association_filter.artist_names.enabled)
        .bind(&input.options.association_filter.artist_names.values)
        .bind(input.options.association_filter.artist_ids.enabled)
        .bind(&input.options.association_filter.artist_ids.values)
        .bind(input.options.user_data_filter.is_played)
        .bind(input.options.user_data_filter.is_favorite)
        .bind(input.options.user_data_filter.require_played)
        .bind(input.options.user_data_filter.require_unplayed)
        .bind(input.options.user_data_filter.require_favorite)
        .bind(input.options.user_data_filter.require_resumable)
        .bind(input.options.user_data_filter.require_likes)
        .bind(input.options.user_data_filter.require_dislikes)
        .bind(input.options.structure_filter.is_folder)
        .bind(input.options.structure_filter.is_movie)
        .bind(input.options.structure_filter.is_series)
        .bind(input.options.structure_filter.require_folder)
        .bind(input.options.structure_filter.require_not_folder)
        .bind(input.options.media_filter.media_types.enabled)
        .bind(&input.options.media_filter.media_types.values)
        .bind(input.options.media_filter.containers.enabled)
        .bind(&input.options.media_filter.containers.values)
        .bind(input.options.media_filter.audio_codecs.enabled)
        .bind(&input.options.media_filter.audio_codecs.values)
        .bind(input.options.media_filter.video_codecs.enabled)
        .bind(&input.options.media_filter.video_codecs.values)
        .bind(input.options.media_filter.subtitle_codecs.enabled)
        .bind(&input.options.media_filter.subtitle_codecs.values)
        .bind(input.options.association_filter.official_ratings.enabled)
        .bind(&input.options.association_filter.official_ratings.values)
        .bind(input.options.association_filter.tag_names.enabled)
        .bind(&input.options.association_filter.tag_names.values)
        .bind(input.options.association_filter.exclude_tag_names.enabled)
        .bind(&input.options.association_filter.exclude_tag_names.values)
        .bind(input.options.association_filter.studio_names.enabled)
        .bind(&input.options.association_filter.studio_names.values)
        .bind(input.options.association_filter.studio_ids.enabled)
        .bind(&input.options.association_filter.studio_ids.values)
        .bind(input.options.provider_filter.any_provider_id_equals.enabled)
        .bind(&input.options.provider_filter.any_provider_id_equals.values)
        .bind(input.options.image_filter.image_types.enabled)
        .bind(&input.options.image_filter.image_types.values)
        .bind(input.options.sort_field.as_sql_key())
        .bind(input.options.sort_direction.as_sql_key())
        .bind(input.options.association_filter.album_names.enabled)
        .bind(&input.options.association_filter.album_names.values)
        .bind(input.options.association_filter.album_ids.enabled)
        .bind(&input.options.association_filter.album_ids.values)
        .fetch_all(&self.pool)
        .await?;

        let total_record_count = rows
            .first()
            .map(|row| row.try_get::<i64, _>("total_record_count"))
            .transpose()?
            .unwrap_or(0)
            .try_into()
            .unwrap_or(u32::MAX);
        let items = rows
            .into_iter()
            .map(MediaItemBrowseRecord::from_row)
            .collect::<Result<Vec<_>, _>>()?;

        Ok(BrowseItemsResult {
            items,
            total_record_count,
        })
    }

    async fn list_user_items_from_playstates(
        &self,
        input: &BrowseItemsInput,
    ) -> Result<BrowseItemsResult, sqlx::Error> {
        let rows = sqlx::query(
            r#"
            with requested_parent as (
                select case
                    when $2::text is null then null::uuid
                    when $2::text ~* '^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$'
                    then $2::uuid
                    else null::uuid
                end as public_id
            ),
            parent_library as (
                select l.id as library_id,
                       (u.allow_transcode and lp.can_transcode) as supports_transcoding
                from libraries l
                join library_permissions lp on lp.library_id = l.id
                join users u on u.id = lp.user_id
                cross join requested_parent rp
                where ($2::text is null or l.public_id = rp.public_id)
                  and lp.user_id = $1
                  and lp.can_view = true
                  and l.is_hidden = false
            ),
            parent_item as (
                select mi.id as item_id,
                       mi.library_id,
                       (u.allow_transcode and lp.can_transcode) as supports_transcoding
                from media_items mi
                join libraries l on l.id = mi.library_id
                join library_permissions lp on lp.library_id = mi.library_id
                join users u on u.id = lp.user_id
                cross join requested_parent rp
                where $2::text is not null
                  and mi.public_id = rp.public_id
                  and lp.user_id = $1
                  and lp.can_view = true
                  and mi.is_deleted = false
                  and l.is_hidden = false
            ),
            browse_scope as (
                select library_id,
                       null::bigint as parent_item_id,
                       true as is_library,
                       supports_transcoding
                from parent_library
                union all
                select library_id,
                       item_id as parent_item_id,
                       false as is_library,
                       supports_transcoding
                from parent_item
            ),
            state_items as (
                select up.media_item_id,
                       up.position_ticks,
                       up.play_count,
                       up.is_favorite,
                       up.rating,
                       up.played
                from user_playstates up
                where up.user_id = $1
                  and ($9::boolean = false or up.played = true)
                  and ($10::boolean = false or up.is_favorite = true)
                  and ($11::boolean = false or up.rating >= 5)
                  and ($12::boolean = false or up.rating < 5)
                  and ($13::boolean = false or (up.position_ticks > 0 and up.played = false))
            )
            select
                mi.public_id::text as id,
                mi.title as name,
                mi.item_type,
                parent.public_id::text as parent_id,
                coalesce(mi.runtime_ticks, primary_file.duration_ticks) as runtime_ticks,
                primary_file.media_file_id,
                primary_file.file_size as media_file_size,
                primary_file.container as media_file_container,
                primary_file.bitrate as media_file_bitrate,
                primary_file.is_strm as media_file_is_strm,
                coalesce(scope.supports_transcoding, false) as supports_transcoding,
                mi.production_year,
                up.position_ticks as playback_position_ticks,
                up.play_count,
                up.is_favorite,
                up.rating::double precision as rating,
                up.played,
                case
                    when $6::boolean = true then coalesce(item_images.image_tags, array[]::text[])
                    else array[]::text[]
                end as image_tags,
                count(*) over() as total_record_count
            from state_items up
            join media_items mi on mi.id = up.media_item_id
            join browse_scope scope on scope.library_id = mi.library_id
            left join media_items parent on parent.id = mi.parent_id
            left join lateral (
                select mf.id as media_file_id,
                       mf.file_size,
                       mf.container,
                       mf.duration_ticks,
                       mf.bitrate,
                       mf.is_strm
                from media_files mf
                where mf.media_item_id = mi.id
                order by mf.is_primary desc, mf.id
                limit 1
            ) primary_file on true
            left join lateral (
                select array_agg(a.artwork_type || '=' || a.id::text order by a.artwork_type, a.is_primary desc, a.id) as image_tags
                from artwork a
                where a.media_item_id = mi.id
            ) item_images on $6::boolean = true
            where mi.is_deleted = false
              and ($7::boolean = false or mi.item_type = any($8::text[]))
              and (
                    (scope.is_library = true and (($5 = true) or mi.parent_id is null))
                 or (scope.is_library = false and mi.parent_id = scope.parent_item_id)
              )
            order by
                case when $14 = 'sort_name' and $15 = 'asc' then coalesce(nullif(mi.sort_title, ''), mi.title) end asc nulls last,
                case when $14 = 'sort_name' and $15 = 'desc' then coalesce(nullif(mi.sort_title, ''), mi.title) end desc nulls last,
                case when $14 = 'date_created' and $15 = 'asc' then mi.created_at end asc nulls last,
                case when $14 = 'date_created' and $15 = 'desc' then mi.created_at end desc nulls last,
                case when $14 = 'runtime' and $15 = 'asc' then coalesce(mi.runtime_ticks, primary_file.duration_ticks) end asc nulls last,
                case when $14 = 'runtime' and $15 = 'desc' then coalesce(mi.runtime_ticks, primary_file.duration_ticks) end desc nulls last,
                case when $14 = 'production_year' and $15 = 'asc' then mi.production_year end asc nulls last,
                case when $14 = 'production_year' and $15 = 'desc' then mi.production_year end desc nulls last,
                case when $14 = 'index_number' and $15 = 'asc' then mi.parent_index_number end asc nulls last,
                case when $14 = 'index_number' and $15 = 'asc' then mi.index_number end asc nulls last,
                case when $14 = 'index_number' and $15 = 'desc' then mi.parent_index_number end desc nulls last,
                case when $14 = 'index_number' and $15 = 'desc' then mi.index_number end desc nulls last,
                case when $15 = 'asc' then mi.id end asc,
                case when $15 = 'desc' then mi.id end desc,
                mi.id asc
            limit $3
            offset $4
            "#,
        )
        .bind(input.user_id)
        .bind(&input.parent_id)
        .bind(input.limit)
        .bind(input.start_index)
        .bind(input.recursive)
        .bind(input.include_image_tags)
        .bind(input.options.type_filter.enabled)
        .bind(&input.options.type_filter.item_types)
        .bind(
            input.options.user_data_filter.is_played == Some(true)
                || input.options.user_data_filter.require_played,
        )
        .bind(
            input.options.user_data_filter.is_favorite == Some(true)
                || input.options.user_data_filter.require_favorite,
        )
        .bind(input.options.user_data_filter.require_likes)
        .bind(input.options.user_data_filter.require_dislikes)
        .bind(input.options.user_data_filter.require_resumable)
        .bind(input.options.sort_field.as_sql_key())
        .bind(input.options.sort_direction.as_sql_key())
        .fetch_all(&self.pool)
        .await?;

        browse_result_from_rows(rows)
    }

    pub async fn list_user_item_prefixes(
        &self,
        input: ItemPrefixesInput,
    ) -> Result<Vec<ItemPrefixRecord>, sqlx::Error> {
        let rows = sqlx::query(
            r#"
            with requested_parent as (
                select case
                    when $2::text is null then null::uuid
                    when $2::text ~* '^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$'
                    then $2::uuid
                    else null::uuid
                end as public_id
            ),
            parent_library as (
                select l.id as library_id
                from libraries l
                join library_permissions lp on lp.library_id = l.id
                cross join requested_parent rp
                where ($2::text is null or l.public_id = rp.public_id)
                  and lp.user_id = $1
                  and lp.can_view = true
                  and l.is_hidden = false
            ),
            parent_item as (
                select mi.id as item_id,
                       mi.library_id
                from media_items mi
                join libraries l on l.id = mi.library_id
                join library_permissions lp on lp.library_id = mi.library_id
                cross join requested_parent rp
                where $2::text is not null
                  and mi.public_id = rp.public_id
                  and lp.user_id = $1
                  and lp.can_view = true
                  and mi.is_deleted = false
                  and l.is_hidden = false
            ),
            browse_scope as (
                select library_id,
                       null::bigint as parent_item_id,
                       true as is_library
                from parent_library
                union all
                select library_id,
                       item_id as parent_item_id,
                       false as is_library
                from parent_item
            ),
            prefix_candidates as (
                select distinct
                    upper(left(trim(coalesce(nullif(mi.sort_title, ''), mi.title)), 1)) as prefix
                from media_items mi
                join browse_scope scope on scope.library_id = mi.library_id
                where mi.is_deleted = false
                  and ($6::boolean = false or mi.item_type = any($7::text[]))
                  and (
                        $8::text is null
                     or lower(mi.title) like '%' || lower($8::text) || '%'
                     or lower(mi.original_title) like '%' || lower($8::text) || '%'
                     or lower(mi.sort_title) like '%' || lower($8::text) || '%'
                  )
                  and (
                        $9::text is null
                     or lower(coalesce(nullif(mi.sort_title, ''), mi.title)) like lower($9::text) || '%'
                  )
                  and (
                        $10::text is null
                     or lower(coalesce(nullif(mi.sort_title, ''), mi.title)) >= lower($10::text)
                  )
                  and (
                        $11::text is null
                     or lower(coalesce(nullif(mi.sort_title, ''), mi.title)) < lower($11::text)
                  )
                  and (
                        (scope.is_library = true and (($5 = true) or mi.parent_id is null))
                     or (scope.is_library = false and mi.parent_id = scope.parent_item_id)
                  )
            )
            select prefix as name,
                   prefix as value
            from prefix_candidates
            where prefix <> ''
            order by lower(prefix), prefix
            limit $3
            offset $4
            "#,
        )
        .bind(input.user_id)
        .bind(input.parent_id)
        .bind(input.limit)
        .bind(input.start_index)
        .bind(input.recursive)
        .bind(input.type_filter.enabled)
        .bind(&input.type_filter.item_types)
        .bind(input.search_term)
        .bind(input.name_starts_with)
        .bind(input.name_starts_with_or_greater)
        .bind(input.name_less_than)
        .fetch_all(&self.pool)
        .await?;

        rows.into_iter().map(ItemPrefixRecord::from_row).collect()
    }

    async fn list_user_items_by_include_ids(
        &self,
        input: &BrowseItemsInput,
    ) -> Result<BrowseItemsResult, sqlx::Error> {
        let rows = sqlx::query(
            r#"
            with requested_parent as (
                select case
                    when $2::text is null then null::uuid
                    when $2::text ~* '^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$'
                    then $2::uuid
                    else null::uuid
                end as public_id
            ),
            parent_library as (
                select l.id as library_id,
                       (u.allow_transcode and lp.can_transcode) as supports_transcoding
                from libraries l
                join library_permissions lp on lp.library_id = l.id
                join users u on u.id = lp.user_id
                cross join requested_parent rp
                where ($2::text is null or l.public_id = rp.public_id)
                  and lp.user_id = $1
                  and lp.can_view = true
                  and l.is_hidden = false
            ),
            parent_item as (
                select mi.id as item_id,
                       mi.library_id,
                       (u.allow_transcode and lp.can_transcode) as supports_transcoding
                from media_items mi
                join libraries l on l.id = mi.library_id
                join library_permissions lp on lp.library_id = mi.library_id
                join users u on u.id = lp.user_id
                cross join requested_parent rp
                where $2::text is not null
                  and mi.public_id = rp.public_id
                  and lp.user_id = $1
                  and lp.can_view = true
                  and mi.is_deleted = false
                  and l.is_hidden = false
            ),
            browse_scope as (
                select library_id,
                       null::bigint as parent_item_id,
                       true as is_library,
                       supports_transcoding
                from parent_library
                union all
                select library_id,
                       item_id as parent_item_id,
                       false as is_library,
                       supports_transcoding
                from parent_item
            ),
            requested_ids as (
                select distinct item_id::uuid as public_id
                from unnest($9::text[]) as item_id
            )
            select
                mi.public_id::text as id,
                mi.title as name,
                mi.item_type,
                parent.public_id::text as parent_id,
                coalesce(mi.runtime_ticks, primary_file.duration_ticks) as runtime_ticks,
                primary_file.media_file_id,
                primary_file.file_size as media_file_size,
                primary_file.container as media_file_container,
                primary_file.bitrate as media_file_bitrate,
                primary_file.is_strm as media_file_is_strm,
                coalesce(scope.supports_transcoding, false) as supports_transcoding,
                mi.production_year,
                coalesce(up.position_ticks, 0) as playback_position_ticks,
                coalesce(up.play_count, 0) as play_count,
                coalesce(up.is_favorite, false) as is_favorite,
                up.rating::double precision as rating,
                coalesce(up.played, false) as played,
                case
                    when $6::boolean = true then coalesce(item_images.image_tags, array[]::text[])
                    else array[]::text[]
                end as image_tags,
                count(*) over() as total_record_count
            from requested_ids requested
            join media_items mi on mi.public_id = requested.public_id
            join browse_scope scope on scope.library_id = mi.library_id
            left join media_items parent on parent.id = mi.parent_id
            left join lateral (
                select mf.id as media_file_id,
                       mf.file_size,
                       mf.container,
                       mf.duration_ticks,
                       mf.bitrate,
                       mf.is_strm
                from media_files mf
                where mf.media_item_id = mi.id
                order by mf.is_primary desc, mf.id
                limit 1
            ) primary_file on true
            left join user_playstates up on up.user_id = $1
                and up.media_item_id = mi.id
            left join lateral (
                select array_agg(a.artwork_type || '=' || a.id::text order by a.artwork_type, a.is_primary desc, a.id) as image_tags
                from artwork a
                where a.media_item_id = mi.id
            ) item_images on $6::boolean = true
            where mi.is_deleted = false
              and ($7::boolean = false or mi.item_type = any($8::text[]))
              and (
                    (scope.is_library = true and (($5 = true) or mi.parent_id is null))
                 or (scope.is_library = false and mi.parent_id = scope.parent_item_id)
              )
            order by
                case when $10 = 'sort_name' and $11 = 'asc' then coalesce(nullif(mi.sort_title, ''), mi.title) end asc nulls last,
                case when $10 = 'sort_name' and $11 = 'desc' then coalesce(nullif(mi.sort_title, ''), mi.title) end desc nulls last,
                case when $10 = 'date_created' and $11 = 'asc' then mi.created_at end asc nulls last,
                case when $10 = 'date_created' and $11 = 'desc' then mi.created_at end desc nulls last,
                case when $10 = 'runtime' and $11 = 'asc' then coalesce(mi.runtime_ticks, primary_file.duration_ticks) end asc nulls last,
                case when $10 = 'runtime' and $11 = 'desc' then coalesce(mi.runtime_ticks, primary_file.duration_ticks) end desc nulls last,
                case when $10 = 'production_year' and $11 = 'asc' then mi.production_year end asc nulls last,
                case when $10 = 'production_year' and $11 = 'desc' then mi.production_year end desc nulls last,
                case when $10 = 'index_number' and $11 = 'asc' then mi.parent_index_number end asc nulls last,
                case when $10 = 'index_number' and $11 = 'asc' then mi.index_number end asc nulls last,
                case when $10 = 'index_number' and $11 = 'desc' then mi.parent_index_number end desc nulls last,
                case when $10 = 'index_number' and $11 = 'desc' then mi.index_number end desc nulls last,
                case when $11 = 'asc' then mi.id end asc,
                case when $11 = 'desc' then mi.id end desc,
                mi.id asc
            limit $3
            offset $4
            "#,
        )
        .bind(input.user_id)
        .bind(&input.parent_id)
        .bind(input.limit)
        .bind(input.start_index)
        .bind(input.recursive)
        .bind(input.include_image_tags)
        .bind(input.options.type_filter.enabled)
        .bind(&input.options.type_filter.item_types)
        .bind(&input.options.scalar_filter.include_ids.values)
        .bind(input.options.sort_field.as_sql_key())
        .bind(input.options.sort_direction.as_sql_key())
        .fetch_all(&self.pool)
        .await?;

        browse_result_from_rows(rows)
    }

    async fn list_user_items_by_provider_ids(
        &self,
        input: &BrowseItemsInput,
    ) -> Result<BrowseItemsResult, sqlx::Error> {
        let rows = sqlx::query(
            r#"
            with requested_parent as (
                select case
                    when $2::text is null then null::uuid
                    when $2::text ~* '^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$'
                    then $2::uuid
                    else null::uuid
                end as public_id
            ),
            parent_library as (
                select l.id as library_id,
                       (u.allow_transcode and lp.can_transcode) as supports_transcoding
                from libraries l
                join library_permissions lp on lp.library_id = l.id
                join users u on u.id = lp.user_id
                cross join requested_parent rp
                where ($2::text is null or l.public_id = rp.public_id)
                  and lp.user_id = $1
                  and lp.can_view = true
                  and l.is_hidden = false
            ),
            parent_item as (
                select mi.id as item_id,
                       mi.library_id,
                       (u.allow_transcode and lp.can_transcode) as supports_transcoding
                from media_items mi
                join libraries l on l.id = mi.library_id
                join library_permissions lp on lp.library_id = mi.library_id
                join users u on u.id = lp.user_id
                cross join requested_parent rp
                where $2::text is not null
                  and mi.public_id = rp.public_id
                  and lp.user_id = $1
                  and lp.can_view = true
                  and mi.is_deleted = false
                  and l.is_hidden = false
            ),
            browse_scope as (
                select library_id,
                       null::bigint as parent_item_id,
                       true as is_library,
                       supports_transcoding
                from parent_library
                union all
                select library_id,
                       item_id as parent_item_id,
                       false as is_library,
                       supports_transcoding
                from parent_item
            ),
            provider_items as (
                select distinct mei.media_item_id
                from media_external_ids mei
                where lower(mei.provider || '.' || mei.external_id) = any($9::text[])
            )
            select
                mi.public_id::text as id,
                mi.title as name,
                mi.item_type,
                parent.public_id::text as parent_id,
                coalesce(mi.runtime_ticks, primary_file.duration_ticks) as runtime_ticks,
                primary_file.media_file_id,
                primary_file.file_size as media_file_size,
                primary_file.container as media_file_container,
                primary_file.bitrate as media_file_bitrate,
                primary_file.is_strm as media_file_is_strm,
                coalesce(scope.supports_transcoding, false) as supports_transcoding,
                mi.production_year,
                coalesce(up.position_ticks, 0) as playback_position_ticks,
                coalesce(up.play_count, 0) as play_count,
                coalesce(up.is_favorite, false) as is_favorite,
                up.rating::double precision as rating,
                coalesce(up.played, false) as played,
                case
                    when $6::boolean = true then coalesce(item_images.image_tags, array[]::text[])
                    else array[]::text[]
                end as image_tags,
                count(*) over() as total_record_count
            from provider_items provider_match
            join media_items mi on mi.id = provider_match.media_item_id
            join browse_scope scope on scope.library_id = mi.library_id
            left join media_items parent on parent.id = mi.parent_id
            left join lateral (
                select mf.id as media_file_id,
                       mf.file_size,
                       mf.container,
                       mf.duration_ticks,
                       mf.bitrate,
                       mf.is_strm
                from media_files mf
                where mf.media_item_id = mi.id
                order by mf.is_primary desc, mf.id
                limit 1
            ) primary_file on true
            left join user_playstates up on up.user_id = $1
                and up.media_item_id = mi.id
            left join lateral (
                select array_agg(a.artwork_type || '=' || a.id::text order by a.artwork_type, a.is_primary desc, a.id) as image_tags
                from artwork a
                where a.media_item_id = mi.id
            ) item_images on $6::boolean = true
            where mi.is_deleted = false
              and ($7::boolean = false or mi.item_type = any($8::text[]))
              and (
                    (scope.is_library = true and (($5 = true) or mi.parent_id is null))
                 or (scope.is_library = false and mi.parent_id = scope.parent_item_id)
              )
            order by
                case when $10 = 'sort_name' and $11 = 'asc' then coalesce(nullif(mi.sort_title, ''), mi.title) end asc nulls last,
                case when $10 = 'sort_name' and $11 = 'desc' then coalesce(nullif(mi.sort_title, ''), mi.title) end desc nulls last,
                case when $10 = 'date_created' and $11 = 'asc' then mi.created_at end asc nulls last,
                case when $10 = 'date_created' and $11 = 'desc' then mi.created_at end desc nulls last,
                case when $10 = 'runtime' and $11 = 'asc' then coalesce(mi.runtime_ticks, primary_file.duration_ticks) end asc nulls last,
                case when $10 = 'runtime' and $11 = 'desc' then coalesce(mi.runtime_ticks, primary_file.duration_ticks) end desc nulls last,
                case when $10 = 'production_year' and $11 = 'asc' then mi.production_year end asc nulls last,
                case when $10 = 'production_year' and $11 = 'desc' then mi.production_year end desc nulls last,
                case when $10 = 'index_number' and $11 = 'asc' then mi.parent_index_number end asc nulls last,
                case when $10 = 'index_number' and $11 = 'asc' then mi.index_number end asc nulls last,
                case when $10 = 'index_number' and $11 = 'desc' then mi.parent_index_number end desc nulls last,
                case when $10 = 'index_number' and $11 = 'desc' then mi.index_number end desc nulls last,
                case when $11 = 'asc' then mi.id end asc,
                case when $11 = 'desc' then mi.id end desc,
                mi.id asc
            limit $3
            offset $4
            "#,
        )
        .bind(input.user_id)
        .bind(&input.parent_id)
        .bind(input.limit)
        .bind(input.start_index)
        .bind(input.recursive)
        .bind(input.include_image_tags)
        .bind(input.options.type_filter.enabled)
        .bind(&input.options.type_filter.item_types)
        .bind(&input.options.provider_filter.any_provider_id_equals.values)
        .bind(input.options.sort_field.as_sql_key())
        .bind(input.options.sort_direction.as_sql_key())
        .fetch_all(&self.pool)
        .await?;

        browse_result_from_rows(rows)
    }

    pub async fn list_resume_items(
        &self,
        input: MediaQueryInput,
    ) -> Result<BrowseItemsResult, sqlx::Error> {
        let fetch_limit = input.limit.saturating_add(1);
        let rows = sqlx::query(
            r#"
            with requested_parent as (
                select case
                    when $4::text is null then null::uuid
                    when $4::text ~* '^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$'
                    then $4::uuid
                    else null::uuid
                end as public_id
            )
            select
                mi.public_id::text as id,
                mi.title as name,
                mi.item_type,
                parent.public_id::text as parent_id,
                coalesce(mi.runtime_ticks, primary_file.duration_ticks) as runtime_ticks,
                primary_file.media_file_id,
                primary_file.file_size as media_file_size,
                primary_file.container as media_file_container,
                primary_file.bitrate as media_file_bitrate,
                primary_file.is_strm as media_file_is_strm,
                coalesce((u.allow_transcode and lp.can_transcode), false) as supports_transcoding,
                mi.production_year,
                up.position_ticks as playback_position_ticks,
                up.play_count,
                up.is_favorite,
                up.rating::double precision as rating,
                up.played,
                array[]::text[] as image_tags,
                0::bigint as total_record_count
            from user_playstates up
            join media_items mi on mi.id = up.media_item_id
            join libraries l on l.id = mi.library_id
            join library_permissions lp on lp.library_id = mi.library_id
            join users u on u.id = lp.user_id
            cross join requested_parent rp
            left join media_items parent on parent.id = mi.parent_id
            left join lateral (
                select mf.id as media_file_id,
                       mf.file_size,
                       mf.container,
                       mf.duration_ticks,
                       mf.bitrate,
                       mf.is_strm
                from media_files mf
                where mf.media_item_id = mi.id
                order by mf.is_primary desc, mf.id
                limit 1
            ) primary_file on true
            where up.user_id = $1
              and lp.user_id = $1
              and lp.can_view = true
              and up.played = false
              and up.position_ticks > 0
              and mi.is_deleted = false
              and l.is_hidden = false
              and ($5::boolean = false or mi.item_type = any($6::text[]))
              and (
                    $4::text is null
                 or l.public_id = rp.public_id
                 or parent.public_id = rp.public_id
              )
            order by up.updated_at desc, mi.id desc
            limit $2
            offset $3
            "#,
        )
        .bind(input.user_id)
        .bind(fetch_limit)
        .bind(input.start_index)
        .bind(input.parent_id)
        .bind(input.options.type_filter.enabled)
        .bind(&input.options.type_filter.item_types)
        .fetch_all(&self.pool)
        .await?;

        browse_result_lower_bound_from_rows(rows, input.start_index, input.limit)
    }

    pub async fn list_latest_items(
        &self,
        input: MediaQueryInput,
    ) -> Result<BrowseItemsResult, sqlx::Error> {
        let rows = sqlx::query(
            r#"
            with requested_parent as (
                select case
                    when $4::text is null then null::uuid
                    when $4::text ~* '^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$'
                    then $4::uuid
                    else null::uuid
                end as public_id
            )
            select
                mi.public_id::text as id,
                mi.title as name,
                mi.item_type,
                parent.public_id::text as parent_id,
                coalesce(mi.runtime_ticks, primary_file.duration_ticks) as runtime_ticks,
                primary_file.media_file_id,
                primary_file.file_size as media_file_size,
                primary_file.container as media_file_container,
                primary_file.bitrate as media_file_bitrate,
                primary_file.is_strm as media_file_is_strm,
                coalesce((u.allow_transcode and lp.can_transcode), false) as supports_transcoding,
                mi.production_year,
                coalesce(up.position_ticks, 0) as playback_position_ticks,
                coalesce(up.play_count, 0) as play_count,
                coalesce(up.is_favorite, false) as is_favorite,
                up.rating::double precision as rating,
                coalesce(up.played, false) as played,
                array[]::text[] as image_tags,
                0::bigint as total_record_count
            from media_items mi
            join libraries l on l.id = mi.library_id
            join library_permissions lp on lp.library_id = mi.library_id
            join users u on u.id = lp.user_id
            cross join requested_parent rp
            left join media_items parent on parent.id = mi.parent_id
            left join lateral (
                select mf.id as media_file_id,
                       mf.file_size,
                       mf.container,
                       mf.duration_ticks,
                       mf.bitrate,
                       mf.is_strm
                from media_files mf
                where mf.media_item_id = mi.id
                order by mf.is_primary desc, mf.id
                limit 1
            ) primary_file on true
            left join user_playstates up on up.user_id = $1
                and up.media_item_id = mi.id
            where lp.user_id = $1
              and lp.can_view = true
              and mi.is_deleted = false
              and l.is_hidden = false
              and mi.item_type in ('movie', 'series', 'episode', 'track')
              and ($5::boolean = false or mi.item_type = any($6::text[]))
              and (
                    $4::text is null
                 or l.public_id = rp.public_id
                 or parent.public_id = rp.public_id
              )
            order by
                case when $7 = 'sort_name' and $8 = 'asc' then coalesce(nullif(mi.sort_title, ''), mi.title) end asc nulls last,
                case when $7 = 'sort_name' and $8 = 'desc' then coalesce(nullif(mi.sort_title, ''), mi.title) end desc nulls last,
                case when $7 = 'date_created' and $8 = 'asc' then mi.created_at end asc nulls last,
                case when $7 = 'date_created' and $8 = 'desc' then mi.created_at end desc nulls last,
                case when $7 = 'runtime' and $8 = 'asc' then coalesce(mi.runtime_ticks, primary_file.duration_ticks) end asc nulls last,
                case when $7 = 'runtime' and $8 = 'desc' then coalesce(mi.runtime_ticks, primary_file.duration_ticks) end desc nulls last,
                case when $7 = 'production_year' and $8 = 'asc' then mi.production_year end asc nulls last,
                case when $7 = 'production_year' and $8 = 'desc' then mi.production_year end desc nulls last,
                case when $7 = 'index_number' and $8 = 'asc' then mi.parent_index_number end asc nulls last,
                case when $7 = 'index_number' and $8 = 'asc' then mi.index_number end asc nulls last,
                case when $7 = 'index_number' and $8 = 'desc' then mi.parent_index_number end desc nulls last,
                case when $7 = 'index_number' and $8 = 'desc' then mi.index_number end desc nulls last,
                case when $8 = 'asc' then mi.id end asc,
                case when $8 = 'desc' then mi.id end desc,
                mi.id asc
            limit $2
            offset $3
            "#,
        )
        .bind(input.user_id)
        .bind(input.limit)
        .bind(input.start_index)
        .bind(input.parent_id)
        .bind(input.options.type_filter.enabled)
        .bind(&input.options.type_filter.item_types)
        .bind(input.options.sort_field.as_sql_key())
        .bind(input.options.sort_direction.as_sql_key())
        .fetch_all(&self.pool)
        .await?;

        browse_result_from_rows(rows)
    }

    pub async fn list_similar_items(
        &self,
        input: SimilarItemsInput,
    ) -> Result<Option<BrowseItemsResult>, sqlx::Error> {
        let fetch_limit = input.limit.saturating_add(1);
        let rows = sqlx::query(
            r#"
            with target_item as (
                select mi.id,
                       mi.library_id,
                       mi.item_type,
                       mi.production_year
                from media_items mi
                join libraries l on l.id = mi.library_id
                join library_permissions lp on lp.library_id = mi.library_id
                where mi.public_id = case
                    when $2 ~* '^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$'
                    then $2::uuid
                    else null::uuid
                end
                  and lp.user_id = $1
                  and lp.can_view = true
                  and mi.is_deleted = false
                  and l.is_hidden = false
            ),
            candidates as (
                select
                    mi.public_id::text as id,
                    mi.title as name,
                    mi.item_type,
                    parent.public_id::text as parent_id,
                    coalesce(mi.runtime_ticks, primary_file.duration_ticks) as runtime_ticks,
                    primary_file.media_file_id,
                    primary_file.file_size as media_file_size,
                    primary_file.container as media_file_container,
                    primary_file.bitrate as media_file_bitrate,
                    primary_file.is_strm as media_file_is_strm,
                    coalesce((u.allow_transcode and lp.can_transcode), false) as supports_transcoding,
                    mi.production_year,
                    coalesce(up.position_ticks, 0) as playback_position_ticks,
                    coalesce(up.play_count, 0) as play_count,
                    coalesce(up.is_favorite, false) as is_favorite,
                    up.rating::double precision as rating,
                    coalesce(up.played, false) as played,
                    mi.created_at as sort_created_at,
                    case
                        when target.production_year is null or mi.production_year is null then null
                    else abs(mi.production_year - target.production_year)
                    end as year_distance,
                    array[]::text[] as image_tags,
                    0::bigint as total_record_count
                from target_item target
                join media_items mi on mi.library_id = target.library_id
                    and mi.item_type = target.item_type
                    and mi.id <> target.id
                join libraries l on l.id = mi.library_id
                join library_permissions lp on lp.library_id = mi.library_id
                join users u on u.id = lp.user_id
                left join media_items parent on parent.id = mi.parent_id
                left join lateral (
                    select mf.id as media_file_id,
                           mf.file_size,
                           mf.container,
                           mf.duration_ticks,
                           mf.bitrate,
                           mf.is_strm
                    from media_files mf
                    where mf.media_item_id = mi.id
                    order by mf.is_primary desc, mf.id
                    limit 1
                ) primary_file on true
                left join user_playstates up on up.user_id = $1
                    and up.media_item_id = mi.id
                where lp.user_id = $1
                  and lp.can_view = true
                  and mi.is_deleted = false
                  and l.is_hidden = false
                  and ($5::boolean = false or mi.item_type = any($6::text[]))
            )
            select *
            from candidates
            order by
                year_distance asc nulls last,
                case when $7 = 'sort_name' and $8 = 'asc' then name end asc nulls last,
                case when $7 = 'sort_name' and $8 = 'desc' then name end desc nulls last,
                case when $7 = 'date_created' and $8 = 'asc' then sort_created_at end asc nulls last,
                case when $7 = 'date_created' and $8 = 'desc' then sort_created_at end desc nulls last,
                id asc
            limit $3
            offset $4
            "#,
        )
        .bind(input.user_id)
        .bind(&input.item_id)
        .bind(fetch_limit)
        .bind(input.start_index)
        .bind(input.options.type_filter.enabled)
        .bind(&input.options.type_filter.item_types)
        .bind(input.options.sort_field.as_sql_key())
        .bind(input.options.sort_direction.as_sql_key())
        .fetch_all(&self.pool)
        .await?;

        let target_exists = self
            .find_user_item_by_id(input.user_id, &input.item_id)
            .await?
            .is_some();
        if !target_exists {
            return Ok(None);
        }

        browse_result_lower_bound_from_rows(rows, input.start_index, input.limit).map(Some)
    }

    pub async fn list_series_seasons(
        &self,
        input: ShowItemsInput,
    ) -> Result<BrowseItemsResult, sqlx::Error> {
        let rows = sqlx::query(
            r#"
            select
                season.public_id::text as id,
                season.title as name,
                season.item_type,
                series.public_id::text as parent_id,
                coalesce(season.runtime_ticks, primary_file.duration_ticks) as runtime_ticks,
                primary_file.media_file_id,
                primary_file.file_size as media_file_size,
                primary_file.container as media_file_container,
                primary_file.bitrate as media_file_bitrate,
                primary_file.is_strm as media_file_is_strm,
                coalesce((u.allow_transcode and lp.can_transcode), false) as supports_transcoding,
                season.production_year,
                coalesce(up.position_ticks, 0) as playback_position_ticks,
                coalesce(up.play_count, 0) as play_count,
                coalesce(up.is_favorite, false) as is_favorite,
                up.rating::double precision as rating,
                coalesce(up.played, false) as played,
                array[]::text[] as image_tags,
                count(*) over() as total_record_count
            from media_items series
            join libraries l on l.id = series.library_id
            join library_permissions lp on lp.library_id = series.library_id
            join users u on u.id = lp.user_id
            join media_items season on season.parent_id = series.id
            left join lateral (
                select mf.id as media_file_id,
                       mf.file_size,
                       mf.container,
                       mf.duration_ticks,
                       mf.bitrate,
                       mf.is_strm
                from media_files mf
                where mf.media_item_id = season.id
                order by mf.is_primary desc, mf.id
                limit 1
            ) primary_file on true
            left join user_playstates up on up.user_id = $1
                and up.media_item_id = season.id
            where series.public_id = case
                  when $2::text ~* '^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$'
                  then $2::uuid
                  else null::uuid
              end
              and series.item_type = 'series'
              and season.item_type = 'season'
              and lp.user_id = $1
              and lp.can_view = true
              and series.is_deleted = false
              and season.is_deleted = false
              and l.is_hidden = false
              and ($5::boolean = false or season.item_type = any($6::text[]))
            order by season.index_number nulls last,
                     season.sort_title,
                     season.id
            limit $3
            offset $4
            "#,
        )
        .bind(input.user_id)
        .bind(input.series_id)
        .bind(input.limit)
        .bind(input.start_index)
        .bind(input.options.type_filter.enabled)
        .bind(&input.options.type_filter.item_types)
        .fetch_all(&self.pool)
        .await?;

        browse_result_from_rows(rows)
    }

    pub async fn list_series_episodes(
        &self,
        input: ShowItemsInput,
    ) -> Result<BrowseItemsResult, sqlx::Error> {
        let rows = sqlx::query(
            r#"
            with series_scope as (
                select series.id,
                       (u.allow_transcode and lp.can_transcode) as supports_transcoding
                from media_items series
                join libraries l on l.id = series.library_id
                join library_permissions lp on lp.library_id = series.library_id
                join users u on u.id = lp.user_id
                where series.public_id = case
                      when $2::text ~* '^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$'
                      then $2::uuid
                      else null::uuid
                  end
                  and series.item_type = 'series'
                  and lp.user_id = $1
                  and lp.can_view = true
                  and series.is_deleted = false
                  and l.is_hidden = false
            ),
            selected_seasons as (
                select season.id
                from media_items season
                join series_scope series on series.id = season.parent_id
                where season.item_type = 'season'
                  and season.is_deleted = false
                  and (
                      $5::text is null
                      or season.public_id = case
                          when $5::text ~* '^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$'
                          then $5::uuid
                          else null::uuid
                      end
                  )
            ),
            episode_parent_scope as (
                select id
                from selected_seasons
                union all
                select id
                from series_scope
                where $5::text is null
            )
            select
                episode.public_id::text as id,
                episode.title as name,
                episode.item_type,
                parent.public_id::text as parent_id,
                coalesce(episode.runtime_ticks, primary_file.duration_ticks) as runtime_ticks,
                primary_file.media_file_id,
                primary_file.file_size as media_file_size,
                primary_file.container as media_file_container,
                primary_file.bitrate as media_file_bitrate,
                primary_file.is_strm as media_file_is_strm,
                coalesce(access.supports_transcoding, false) as supports_transcoding,
                episode.production_year,
                coalesce(up.position_ticks, 0) as playback_position_ticks,
                coalesce(up.play_count, 0) as play_count,
                coalesce(up.is_favorite, false) as is_favorite,
                up.rating::double precision as rating,
                coalesce(up.played, false) as played,
                array[]::text[] as image_tags,
                count(*) over() as total_record_count
            from episode_parent_scope episode_parent
            join media_items episode on episode.parent_id = episode_parent.id
            join media_items parent on parent.id = episode.parent_id
            cross join series_scope access
            left join lateral (
                select mf.id as media_file_id,
                       mf.file_size,
                       mf.container,
                       mf.duration_ticks,
                       mf.bitrate,
                       mf.is_strm
                from media_files mf
                where mf.media_item_id = episode.id
                order by mf.is_primary desc, mf.id
                limit 1
            ) primary_file on true
            left join user_playstates up on up.user_id = $1
                and up.media_item_id = episode.id
            where episode.item_type = 'episode'
              and episode.is_deleted = false
              and ($6::boolean = false or episode.item_type = any($7::text[]))
            order by parent.index_number nulls last,
                     episode.index_number nulls last,
                     episode.sort_title,
                     episode.id
            limit $3
            offset $4
            "#,
        )
        .bind(input.user_id)
        .bind(input.series_id)
        .bind(input.limit)
        .bind(input.start_index)
        .bind(input.season_id)
        .bind(input.options.type_filter.enabled)
        .bind(&input.options.type_filter.item_types)
        .fetch_all(&self.pool)
        .await?;

        browse_result_from_rows(rows)
    }

    pub async fn list_next_up_items(
        &self,
        input: NextUpInput,
    ) -> Result<BrowseItemsResult, sqlx::Error> {
        let fetch_limit = input.limit.saturating_add(1);
        let rows = sqlx::query(
            r#"
            with accessible_series as (
                select series.id,
                       series.library_id,
                       (u.allow_transcode and lp.can_transcode) as supports_transcoding
                from media_items series
                join libraries l on l.id = series.library_id
                join library_permissions lp on lp.library_id = series.library_id
                join users u on u.id = lp.user_id
                where series.item_type = 'series'
                  and lp.user_id = $1
                  and lp.can_view = true
                  and series.is_deleted = false
                  and l.is_hidden = false
                  and (
                      $4::text is null
                      or series.public_id = case
                          when $4::text ~* '^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$'
                          then $4::uuid
                          else null::uuid
                      end
                  )
                  and (
                      $5::text is null
                      or l.public_id = case
                          when $5::text ~* '^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$'
                          then $5::uuid
                          else null::uuid
                      end
                  )
            ),
            candidate_episodes as (
                select
                    episode.id,
                    episode.public_id::text as id_text,
                    episode.title as name,
                    episode.item_type,
                    parent.public_id::text as parent_id,
                    coalesce(episode.runtime_ticks, primary_file.duration_ticks) as runtime_ticks,
                    primary_file.media_file_id,
                    primary_file.file_size as media_file_size,
                    primary_file.container as media_file_container,
                    primary_file.bitrate as media_file_bitrate,
                    primary_file.is_strm as media_file_is_strm,
                    coalesce(series.supports_transcoding, false) as supports_transcoding,
                    episode.production_year,
                    coalesce(up.position_ticks, 0) as playback_position_ticks,
                    coalesce(up.play_count, 0) as play_count,
                    coalesce(up.is_favorite, false) as is_favorite,
                    up.rating::double precision as rating,
                    coalesce(up.played, false) as played,
                    series.id as series_id,
                    parent.index_number as parent_sort_index,
                    episode.index_number as episode_sort_index,
                    episode.sort_title
                from accessible_series series
                join media_items season on season.parent_id = series.id
                    and season.item_type = 'season'
                    and season.is_deleted = false
                join media_items episode on episode.parent_id = season.id
                    and episode.item_type = 'episode'
                    and episode.is_deleted = false
                join media_items parent on parent.id = episode.parent_id
                left join lateral (
                    select mf.id as media_file_id,
                           mf.file_size,
                           mf.container,
                           mf.duration_ticks,
                           mf.bitrate,
                           mf.is_strm
                    from media_files mf
                    where mf.media_item_id = episode.id
                    order by mf.is_primary desc, mf.id
                    limit 1
                ) primary_file on true
                left join user_playstates up on up.user_id = $1
                    and up.media_item_id = episode.id
                where coalesce(up.played, false) = false
                  and ($6::boolean = false or episode.item_type = any($7::text[]))
                union all
                select
                    episode.id,
                    episode.public_id::text as id_text,
                    episode.title as name,
                    episode.item_type,
                    parent.public_id::text as parent_id,
                    coalesce(episode.runtime_ticks, primary_file.duration_ticks) as runtime_ticks,
                    primary_file.media_file_id,
                    primary_file.file_size as media_file_size,
                    primary_file.container as media_file_container,
                    primary_file.bitrate as media_file_bitrate,
                    primary_file.is_strm as media_file_is_strm,
                    coalesce(series.supports_transcoding, false) as supports_transcoding,
                    episode.production_year,
                    coalesce(up.position_ticks, 0) as playback_position_ticks,
                    coalesce(up.play_count, 0) as play_count,
                    coalesce(up.is_favorite, false) as is_favorite,
                    up.rating::double precision as rating,
                    coalesce(up.played, false) as played,
                    series.id as series_id,
                    episode.parent_index_number as parent_sort_index,
                    episode.index_number as episode_sort_index,
                    episode.sort_title
                from accessible_series series
                join media_items episode on episode.parent_id = series.id
                    and episode.item_type = 'episode'
                    and episode.is_deleted = false
                join media_items parent on parent.id = episode.parent_id
                left join lateral (
                    select mf.id as media_file_id,
                           mf.file_size,
                           mf.container,
                           mf.duration_ticks,
                           mf.bitrate,
                           mf.is_strm
                    from media_files mf
                    where mf.media_item_id = episode.id
                    order by mf.is_primary desc, mf.id
                    limit 1
                ) primary_file on true
                left join user_playstates up on up.user_id = $1
                    and up.media_item_id = episode.id
                where coalesce(up.played, false) = false
                  and ($6::boolean = false or episode.item_type = any($7::text[]))
            ),
            ranked as (
                select *,
                       row_number() over (
                           partition by series_id
                           order by parent_sort_index nulls last,
                                    episode_sort_index nulls last,
                                    sort_title,
                                    id
                       ) as series_rank
                from candidate_episodes
            )
            select
                id_text as id,
                name,
                item_type,
                parent_id,
                runtime_ticks,
                media_file_id,
                media_file_size,
                media_file_container,
                media_file_bitrate,
                media_file_is_strm,
                supports_transcoding,
                production_year,
                playback_position_ticks,
                play_count,
                is_favorite,
                rating,
                played,
                array[]::text[] as image_tags,
                0::bigint as total_record_count
            from ranked
            where series_rank = 1
            order by sort_title, id
            limit $2
            offset $3
            "#,
        )
        .bind(input.user_id)
        .bind(fetch_limit)
        .bind(input.start_index)
        .bind(input.series_id)
        .bind(input.parent_id)
        .bind(input.options.type_filter.enabled)
        .bind(&input.options.type_filter.item_types)
        .fetch_all(&self.pool)
        .await?;

        browse_result_lower_bound_from_rows(rows, input.start_index, input.limit)
    }

    pub async fn find_user_item_by_id(
        &self,
        user_id: i64,
        item_id: &str,
    ) -> Result<Option<MediaItemBrowseRecord>, sqlx::Error> {
        sqlx::query(
            r#"
            select
                mi.public_id::text as id,
                mi.title as name,
                mi.item_type,
                parent.public_id::text as parent_id,
                coalesce(mi.runtime_ticks, primary_file.duration_ticks) as runtime_ticks,
                primary_file.media_file_id,
                primary_file.file_size as media_file_size,
                primary_file.container as media_file_container,
                primary_file.bitrate as media_file_bitrate,
                primary_file.is_strm as media_file_is_strm,
                coalesce((u.allow_transcode and lp.can_transcode), false) as supports_transcoding,
                mi.production_year,
                coalesce(up.position_ticks, 0) as playback_position_ticks,
                coalesce(up.play_count, 0) as play_count,
                coalesce(up.is_favorite, false) as is_favorite,
                up.rating::double precision as rating,
                coalesce(up.played, false) as played,
                array[]::text[] as image_tags,
                1::bigint as total_record_count
            from media_items mi
            join libraries l on l.id = mi.library_id
            join library_permissions lp on lp.library_id = mi.library_id
            join users u on u.id = lp.user_id
            left join media_items parent on parent.id = mi.parent_id
            left join lateral (
                select mf.id as media_file_id,
                       mf.file_size,
                       mf.container,
                       mf.duration_ticks,
                       mf.bitrate,
                       mf.is_strm
                from media_files mf
                where mf.media_item_id = mi.id
                order by mf.is_primary desc, mf.id
                limit 1
            ) primary_file on true
            left join user_playstates up on up.user_id = $1
                and up.media_item_id = mi.id
            where mi.public_id = case
                when $2 ~* '^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$'
                then $2::uuid
                else null::uuid
            end
              and lp.user_id = $1
              and lp.can_view = true
              and mi.is_deleted = false
              and l.is_hidden = false
            "#,
        )
        .bind(user_id)
        .bind(item_id)
        .fetch_optional(&self.pool)
        .await?
        .map(MediaItemBrowseRecord::from_row)
        .transpose()
    }

    pub async fn list_user_item_ancestors(
        &self,
        user_id: i64,
        item_id: &str,
    ) -> Result<Vec<UserItemAncestorRecord>, sqlx::Error> {
        let media_rows = sqlx::query(
            r#"
            with recursive target_item as (
                select mi.id,
                       mi.parent_id,
                       mi.library_id
                from media_items mi
                join libraries l on l.id = mi.library_id
                join library_permissions lp on lp.library_id = mi.library_id
                where mi.public_id = case
                    when $2 ~* '^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$'
                    then $2::uuid
                    else null::uuid
                end
                  and lp.user_id = $1
                  and lp.can_view = true
                  and mi.is_deleted = false
                  and l.is_hidden = false
            ),
            ancestor_items as (
                select parent.id,
                       parent.parent_id,
                       parent.library_id,
                       1::integer as depth
                from target_item target
                join lateral (
                    select parent.id,
                           parent.parent_id,
                           parent.library_id
                    from media_items parent
                    where parent.id = target.parent_id
                      and parent.library_id = target.library_id
                      and parent.is_deleted = false
                    limit 1
                ) parent on true
                union all
                select parent.id,
                       parent.parent_id,
                       parent.library_id,
                       ancestor.depth + 1 as depth
                from ancestor_items ancestor
                join lateral (
                    select parent.id,
                           parent.parent_id,
                           parent.library_id
                    from media_items parent
                    where parent.id = ancestor.parent_id
                      and parent.library_id = ancestor.library_id
                      and parent.is_deleted = false
                    limit 1
                ) parent on true
                where ancestor.depth < 32
            )
            select
                mi.public_id::text as id,
                mi.title as name,
                mi.item_type,
                parent.public_id::text as parent_id,
                coalesce(mi.runtime_ticks, primary_file.duration_ticks) as runtime_ticks,
                primary_file.media_file_id,
                primary_file.file_size as media_file_size,
                primary_file.container as media_file_container,
                primary_file.bitrate as media_file_bitrate,
                primary_file.is_strm as media_file_is_strm,
                coalesce((u.allow_transcode and lp.can_transcode), false) as supports_transcoding,
                mi.production_year,
                coalesce(up.position_ticks, 0) as playback_position_ticks,
                coalesce(up.play_count, 0) as play_count,
                coalesce(up.is_favorite, false) as is_favorite,
                up.rating::double precision as rating,
                coalesce(up.played, false) as played,
                array[]::text[] as image_tags,
                1::bigint as total_record_count
            from ancestor_items ancestor
            join lateral (
                select *
                from media_items mi
                where mi.id = ancestor.id
                limit 1
            ) mi on true
            join libraries l on l.id = mi.library_id
            join library_permissions lp on lp.library_id = mi.library_id
            join users u on u.id = lp.user_id
            left join media_items parent on parent.id = mi.parent_id
            left join lateral (
                select mf.id as media_file_id,
                       mf.file_size,
                       mf.container,
                       mf.duration_ticks,
                       mf.bitrate,
                       mf.is_strm
                from media_files mf
                where mf.media_item_id = mi.id
                order by mf.is_primary desc, mf.id
                limit 1
            ) primary_file on true
            left join user_playstates up on up.user_id = $1
                and up.media_item_id = mi.id
            where lp.user_id = $1
              and lp.can_view = true
              and l.is_hidden = false
            order by ancestor.depth desc
            "#,
        )
        .bind(user_id)
        .bind(item_id)
        .fetch_all(&self.pool)
        .await?;

        let mut ancestors = Vec::new();
        if let Some(library) = self.find_user_item_library(user_id, item_id).await? {
            ancestors.push(UserItemAncestorRecord::Library(library));
        }

        let media_ancestors = media_rows
            .into_iter()
            .map(MediaItemBrowseRecord::from_row)
            .collect::<Result<Vec<_>, _>>()?
            .into_iter()
            .map(UserItemAncestorRecord::Media);
        ancestors.extend(media_ancestors);

        Ok(ancestors)
    }

    async fn find_user_item_library(
        &self,
        user_id: i64,
        item_id: &str,
    ) -> Result<Option<UserLibraryViewRecord>, sqlx::Error> {
        sqlx::query(
            r#"
            select
                l.public_id::text as id,
                l.name,
                l.library_type
            from media_items mi
            join libraries l on l.id = mi.library_id
            join library_permissions lp on lp.library_id = mi.library_id
            where mi.public_id = case
                when $2 ~* '^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$'
                then $2::uuid
                else null::uuid
            end
              and lp.user_id = $1
              and lp.can_view = true
              and mi.is_deleted = false
              and l.is_hidden = false
            "#,
        )
        .bind(user_id)
        .bind(item_id)
        .fetch_optional(&self.pool)
        .await?
        .map(UserLibraryViewRecord::from_row)
        .transpose()
    }
}

fn browse_result_from_rows(rows: Vec<PgRow>) -> Result<BrowseItemsResult, sqlx::Error> {
    let total_record_count = rows
        .first()
        .map(|row| row.try_get::<i64, _>("total_record_count"))
        .transpose()?
        .unwrap_or(0)
        .try_into()
        .unwrap_or(u32::MAX);
    let items = rows
        .into_iter()
        .map(MediaItemBrowseRecord::from_row)
        .collect::<Result<Vec<_>, _>>()?;

    Ok(BrowseItemsResult {
        items,
        total_record_count,
    })
}

fn browse_result_lower_bound_from_rows(
    rows: Vec<PgRow>,
    start_index: i64,
    limit: i64,
) -> Result<BrowseItemsResult, sqlx::Error> {
    let visible_limit = limit.max(0) as usize;
    let has_more = rows.len() > visible_limit;
    let mut items = rows
        .into_iter()
        .take(visible_limit)
        .map(MediaItemBrowseRecord::from_row)
        .collect::<Result<Vec<_>, _>>()?;
    let total_record_count = if items.is_empty() {
        0
    } else {
        start_index
            .max(0)
            .saturating_add(items.len() as i64)
            .saturating_add(i64::from(has_more))
            .try_into()
            .unwrap_or(u32::MAX)
    };
    for item in &mut items {
        item.total_record_count = i64::from(total_record_count);
    }

    Ok(BrowseItemsResult {
        items,
        total_record_count,
    })
}

fn items_filters_result_from_row(row: PgRow) -> Result<ItemsFiltersResult, sqlx::Error> {
    Ok(ItemsFiltersResult {
        genres: row.try_get::<Vec<String>, _>("genres")?,
        tags: row.try_get::<Vec<String>, _>("tags")?,
        official_ratings: row.try_get::<Vec<String>, _>("official_ratings")?,
        years: row.try_get::<Vec<i32>, _>("years")?,
    })
}

fn genre_result_from_rows(rows: Vec<PgRow>) -> Result<GenreListResult, sqlx::Error> {
    let total_record_count = rows
        .first()
        .map(|row| row.try_get::<i64, _>("total_record_count"))
        .transpose()?
        .unwrap_or(0)
        .try_into()
        .unwrap_or(u32::MAX);
    let items = rows
        .into_iter()
        .map(GenreRecord::from_row)
        .collect::<Result<Vec<_>, _>>()?;

    Ok(GenreListResult {
        items,
        total_record_count,
    })
}

fn artist_result_from_rows(rows: Vec<PgRow>) -> Result<ArtistListResult, sqlx::Error> {
    let total_record_count = rows
        .first()
        .map(|row| row.try_get::<i64, _>("total_record_count"))
        .transpose()?
        .unwrap_or(0)
        .try_into()
        .unwrap_or(u32::MAX);
    let items = rows
        .into_iter()
        .map(ArtistRecord::from_row)
        .collect::<Result<Vec<_>, _>>()?;

    Ok(ArtistListResult {
        items,
        total_record_count,
    })
}

fn person_result_from_rows(rows: Vec<PgRow>) -> Result<PersonListResult, sqlx::Error> {
    let total_record_count = rows
        .first()
        .map(|row| row.try_get::<i64, _>("total_record_count"))
        .transpose()?
        .unwrap_or(0)
        .try_into()
        .unwrap_or(u32::MAX);
    let items = rows
        .into_iter()
        .map(PersonRecord::from_row)
        .collect::<Result<Vec<_>, _>>()?;

    Ok(PersonListResult {
        items,
        total_record_count,
    })
}

fn studio_result_from_rows(rows: Vec<PgRow>) -> Result<StudioListResult, sqlx::Error> {
    let total_record_count = rows
        .first()
        .map(|row| row.try_get::<i64, _>("total_record_count"))
        .transpose()?
        .unwrap_or(0)
        .try_into()
        .unwrap_or(u32::MAX);
    let items = rows
        .into_iter()
        .map(StudioRecord::from_row)
        .collect::<Result<Vec<_>, _>>()?;

    Ok(StudioListResult {
        items,
        total_record_count,
    })
}

fn tag_result_from_rows(rows: Vec<PgRow>) -> Result<TagListResult, sqlx::Error> {
    let total_record_count = rows
        .first()
        .map(|row| row.try_get::<i64, _>("total_record_count"))
        .transpose()?
        .unwrap_or(0)
        .try_into()
        .unwrap_or(u32::MAX);
    let items = rows
        .into_iter()
        .map(TagRecord::from_row)
        .collect::<Result<Vec<_>, _>>()?;

    Ok(TagListResult {
        items,
        total_record_count,
    })
}

fn official_rating_result_from_rows(
    rows: Vec<PgRow>,
) -> Result<OfficialRatingListResult, sqlx::Error> {
    let total_record_count = rows
        .first()
        .map(|row| row.try_get::<i64, _>("total_record_count"))
        .transpose()?
        .unwrap_or(0)
        .try_into()
        .unwrap_or(u32::MAX);
    let items = rows
        .into_iter()
        .map(OfficialRatingRecord::from_row)
        .collect::<Result<Vec<_>, _>>()?;

    Ok(OfficialRatingListResult {
        items,
        total_record_count,
    })
}

fn year_result_from_rows(rows: Vec<PgRow>) -> Result<YearListResult, sqlx::Error> {
    let total_record_count = rows
        .first()
        .map(|row| row.try_get::<i64, _>("total_record_count"))
        .transpose()?
        .unwrap_or(0)
        .try_into()
        .unwrap_or(u32::MAX);
    let items = rows
        .into_iter()
        .map(YearRecord::from_row)
        .collect::<Result<Vec<_>, _>>()?;

    Ok(YearListResult {
        items,
        total_record_count,
    })
}

fn technical_facet_result_from_rows(
    rows: Vec<PgRow>,
) -> Result<TechnicalFacetListResult, sqlx::Error> {
    let total_record_count = rows
        .first()
        .map(|row| row.try_get::<i64, _>("total_record_count"))
        .transpose()?
        .unwrap_or(0)
        .try_into()
        .unwrap_or(u32::MAX);
    let items = rows
        .into_iter()
        .map(TechnicalFacetRecord::from_row)
        .collect::<Result<Vec<_>, _>>()?;

    Ok(TechnicalFacetListResult {
        items,
        total_record_count,
    })
}

fn playlist_result_from_rows(rows: Vec<PgRow>) -> Result<PlaylistListResult, sqlx::Error> {
    let total_record_count = rows
        .first()
        .map(|row| row.try_get::<i64, _>("total_record_count"))
        .transpose()?
        .unwrap_or(0)
        .try_into()
        .unwrap_or(u32::MAX);
    let items = rows
        .into_iter()
        .map(PlaylistRecord::from_row)
        .collect::<Result<Vec<_>, _>>()?;

    Ok(PlaylistListResult {
        items,
        total_record_count,
    })
}

fn admin_physical_paths_query() -> &'static str {
    r#"
            select
                nullif(trim(lp.path), '') as path
            from library_paths lp
            join libraries l on l.id = lp.library_id
            where lp.is_enabled = true
              and nullif(trim(lp.path), '') is not null
            order by l.name, lp.path, lp.id
            "#
}

fn media_folders_from_rows(rows: Vec<PgRow>) -> Result<Vec<UserMediaFolderRecord>, sqlx::Error> {
    let mut folders = Vec::<UserMediaFolderRecord>::new();

    for row in rows {
        let folder_id = row.try_get::<String, _>("library_id")?;
        let folder_index =
            if let Some(index) = folders.iter().position(|folder| folder.id == folder_id) {
                index
            } else {
                folders.push(UserMediaFolderRecord {
                    id: folder_id,
                    name: row.try_get("name")?,
                    library_type: row.try_get("library_type")?,
                    subfolders: Vec::new(),
                });
                folders.len() - 1
            };

        if let Some(subfolder_id) = row.try_get::<Option<String>, _>("subfolder_id")? {
            if let Some(path) = row.try_get::<Option<String>, _>("subfolder_path")? {
                folders[folder_index]
                    .subfolders
                    .push(UserMediaSubFolderRecord {
                        id: subfolder_id,
                        path,
                    });
            }
        }
    }

    Ok(folders)
}

impl UserLibraryViewRecord {
    fn from_row(row: PgRow) -> Result<Self, sqlx::Error> {
        Ok(Self {
            id: row.try_get("id")?,
            name: row.try_get("name")?,
            library_type: row.try_get("library_type")?,
        })
    }
}

impl ItemCountsRecord {
    fn from_row(row: PgRow) -> Result<Self, sqlx::Error> {
        Ok(Self {
            movie_count: count_column(&row, "movie_count")?,
            series_count: count_column(&row, "series_count")?,
            episode_count: count_column(&row, "episode_count")?,
            artist_count: count_column(&row, "artist_count")?,
            song_count: count_column(&row, "song_count")?,
            album_count: count_column(&row, "album_count")?,
            box_set_count: count_column(&row, "box_set_count")?,
            item_count: count_column(&row, "item_count")?,
        })
    }
}

impl GenreRecord {
    fn from_row(row: PgRow) -> Result<Self, sqlx::Error> {
        Ok(Self {
            id: row.try_get("id")?,
            name: row.try_get("name")?,
            total_record_count: row.try_get("total_record_count")?,
        })
    }
}

impl ArtistRecord {
    fn from_row(row: PgRow) -> Result<Self, sqlx::Error> {
        Ok(Self {
            id: row.try_get("id")?,
            name: row.try_get("name")?,
            total_record_count: row.try_get("total_record_count")?,
        })
    }
}

impl PersonRecord {
    fn from_row(row: PgRow) -> Result<Self, sqlx::Error> {
        Ok(Self {
            id: row.try_get("id")?,
            name: row.try_get("name")?,
            total_record_count: row.try_get("total_record_count")?,
        })
    }
}

impl StudioRecord {
    fn from_row(row: PgRow) -> Result<Self, sqlx::Error> {
        Ok(Self {
            id: row.try_get("id")?,
            name: row.try_get("name")?,
            total_record_count: row.try_get("total_record_count")?,
        })
    }
}

impl TagRecord {
    fn from_row(row: PgRow) -> Result<Self, sqlx::Error> {
        Ok(Self {
            id: row.try_get("id")?,
            name: row.try_get("name")?,
            total_record_count: row.try_get("total_record_count")?,
        })
    }
}

impl OfficialRatingRecord {
    fn from_row(row: PgRow) -> Result<Self, sqlx::Error> {
        Ok(Self {
            name: row.try_get("name")?,
            total_record_count: row.try_get("total_record_count")?,
        })
    }
}

impl YearRecord {
    fn from_row(row: PgRow) -> Result<Self, sqlx::Error> {
        Ok(Self {
            year: row.try_get("year")?,
            total_record_count: row.try_get("total_record_count")?,
        })
    }
}

impl TechnicalFacetRecord {
    fn from_row(row: PgRow) -> Result<Self, sqlx::Error> {
        Ok(Self {
            id: row.try_get("id")?,
            name: row.try_get("name")?,
            total_record_count: row.try_get("total_record_count")?,
        })
    }
}

impl PlaylistRecord {
    fn from_row(row: PgRow) -> Result<Self, sqlx::Error> {
        Ok(Self {
            id: row.try_get("id")?,
            name: row.try_get("name")?,
            total_record_count: row.try_get("total_record_count")?,
        })
    }
}

impl ItemPrefixRecord {
    fn from_row(row: PgRow) -> Result<Self, sqlx::Error> {
        Ok(Self {
            name: row.try_get("name")?,
            value: row.try_get("value")?,
        })
    }
}

impl MediaItemBrowseRecord {
    fn from_row(row: PgRow) -> Result<Self, sqlx::Error> {
        Ok(Self {
            id: row.try_get("id")?,
            name: row.try_get("name")?,
            item_type: row.try_get("item_type")?,
            parent_id: row.try_get("parent_id")?,
            run_time_ticks: row.try_get("runtime_ticks")?,
            media_file_id: row.try_get("media_file_id")?,
            media_file_size: row.try_get("media_file_size")?,
            media_file_container: row.try_get("media_file_container")?,
            media_file_bitrate: row.try_get("media_file_bitrate")?,
            media_file_is_strm: row.try_get("media_file_is_strm")?,
            supports_transcoding: row.try_get("supports_transcoding")?,
            production_year: row.try_get("production_year")?,
            playback_position_ticks: row.try_get("playback_position_ticks")?,
            play_count: row.try_get("play_count")?,
            is_favorite: row.try_get("is_favorite")?,
            rating: row.try_get("rating")?,
            played: row.try_get("played")?,
            image_tags: row.try_get("image_tags")?,
            total_record_count: row.try_get("total_record_count")?,
        })
    }
}

fn count_column(row: &PgRow, column: &str) -> Result<u32, sqlx::Error> {
    Ok(row
        .try_get::<i64, _>(column)?
        .try_into()
        .unwrap_or(u32::MAX))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn primary_media_file_index_matches_hot_lateral_queries() {
        let migration = include_str!("../../migrations/0034_media_file_primary_covering_index.sql");
        let repository = include_str!("repository.rs");

        assert!(migration.contains("idx_media_files_item_primary_covering"));
        assert!(migration.contains("media_item_id, is_primary desc, id"));
        for included_column in [
            "file_size",
            "container",
            "duration_ticks",
            "bitrate",
            "is_strm",
        ] {
            assert!(migration.contains(included_column));
        }
        assert!(repository.contains("from media_files mf"));
        assert!(repository.contains("order by mf.is_primary desc, mf.id"));
    }

    #[test]
    fn resume_playstate_index_matches_continue_watching_query() {
        let migration = include_str!("../../migrations/0035_resume_playstate_covering_index.sql");
        let repository = include_str!("repository.rs");

        assert!(migration.contains("idx_user_playstates_continue_covering"));
        assert!(migration.contains("user_id, updated_at desc, media_item_id desc"));
        assert!(migration.contains("where played = false and position_ticks > 0"));
        for included_column in [
            "position_ticks",
            "play_count",
            "is_favorite",
            "rating",
            "played",
        ] {
            assert!(migration.contains(included_column));
        }
        assert!(repository.contains("from user_playstates up"));
        assert!(repository.contains("up.played = false"));
        assert!(repository.contains("up.position_ticks > 0"));
        assert!(repository.contains("order by up.updated_at desc, mi.id desc"));
    }

    #[test]
    fn user_playstate_filter_indexes_match_positive_user_data_filters() {
        let migration =
            include_str!("../../migrations/0040_user_playstate_filter_covering_indexes.sql");
        let repository = include_str!("repository.rs");

        assert!(migration.contains("idx_user_playstates_favorites_covering"));
        assert!(migration.contains("idx_user_playstates_played_covering"));
        assert!(migration.contains("idx_user_playstates_rating_covering"));
        assert!(migration.contains("where is_favorite = true"));
        assert!(migration.contains("where played = true"));
        assert!(migration.contains("where rating is not null"));
        assert!(migration.contains("media_item_id desc"));
        for included_column in [
            "position_ticks",
            "play_count",
            "is_favorite",
            "rating",
            "played",
        ] {
            assert!(migration.contains(included_column));
        }
        assert!(repository.contains("coalesce(up.is_favorite, false) = true"));
        assert!(repository.contains("coalesce(up.played, false) = true"));
        assert!(repository.contains("up.rating >= 5"));
        assert!(repository.contains("up.rating < 5"));
    }

    #[test]
    fn positive_playstate_fast_path_only_accepts_narrow_user_state_filters() {
        let mut positive = BrowseItemsInput {
            user_id: 1,
            parent_id: None,
            start_index: 0,
            limit: 20,
            recursive: true,
            include_image_tags: false,
            options: ItemQueryOptions::default(),
        };
        positive.options.user_data_filter.require_favorite = true;
        assert!(positive.can_use_positive_playstate_fast_path());

        let mut resumable = positive.clone();
        resumable.options.user_data_filter.require_favorite = false;
        resumable.options.user_data_filter.require_resumable = true;
        assert!(resumable.can_use_positive_playstate_fast_path());

        let mut unplayed = positive.clone();
        unplayed.options.user_data_filter.require_unplayed = true;
        assert!(!unplayed.can_use_positive_playstate_fast_path());

        let mut complex = positive.clone();
        complex.options.scalar_filter.search_term = Some("movie".to_owned());
        assert!(!complex.can_use_positive_playstate_fast_path());
    }

    #[test]
    fn positive_playstate_fast_path_query_starts_from_playstates() {
        let repository = include_str!("repository.rs");
        let fast_query = repository
            .split("async fn list_user_items_from_playstates")
            .nth(1)
            .unwrap()
            .split("async fn list_user_items_by_include_ids")
            .next()
            .unwrap();

        assert!(fast_query.contains("from user_playstates up"));
        assert!(fast_query.contains("from state_items up"));
        assert!(fast_query.contains("join media_items mi on mi.id = up.media_item_id"));
        assert!(fast_query.contains("up.played = true"));
        assert!(fast_query.contains("up.is_favorite = true"));
        assert!(fast_query.contains("up.rating >= 5"));
        assert!(fast_query.contains("up.rating < 5"));
        assert!(fast_query.contains("up.position_ticks > 0 and up.played = false"));
        assert!(!fast_query.contains("media_item_people"));
        assert!(!fast_query.contains("media_streams"));
        assert!(!fast_query.contains("media_external_ids"));
    }

    #[test]
    fn browse_parent_scope_queries_use_uuid_public_id_comparisons() {
        let repository = include_str!("repository.rs");
        let query_ranges = [
            (
                "pub async fn list_user_items",
                "async fn list_user_items_from_playstates",
            ),
            (
                "async fn list_user_items_from_playstates",
                "async fn list_user_items_by_include_ids",
            ),
            (
                "async fn list_user_items_by_include_ids",
                "async fn list_user_items_by_provider_ids",
            ),
            (
                "async fn list_user_items_by_provider_ids",
                "pub async fn list_resume_items",
            ),
        ];

        for (start, end) in query_ranges {
            let query = repository
                .split(start)
                .nth(1)
                .unwrap_or_else(|| panic!("missing query start `{start}`"))
                .split(end)
                .next()
                .unwrap_or_else(|| panic!("missing query end `{end}`"));

            assert!(query.contains("with requested_parent as"));
            assert!(query.contains("when $2::text ~*"));
            assert!(query.contains("then $2::uuid"));
            assert!(query.contains("cross join requested_parent rp"));
            assert!(query.contains("l.public_id = rp.public_id"));
            assert!(query.contains("mi.public_id = rp.public_id"));
            assert!(!query.contains("l.public_id::text = $2"));
            assert!(!query.contains("mi.public_id::text = $2"));
        }
    }

    #[test]
    fn main_browse_include_exclude_id_filters_use_uuid_sets() {
        let repository = include_str!("repository.rs");
        let main_query = repository
            .split("pub async fn list_user_items")
            .nth(1)
            .unwrap()
            .split("async fn list_user_items_from_playstates")
            .next()
            .unwrap();
        let bad_include_filter = format!("{}{}", "mi.public_id::text = any(", "$10");
        let bad_exclude_filter = format!("{}{}", "mi.public_id::text = any(", "$12");

        assert!(main_query.contains("requested_include_ids as"));
        assert!(main_query.contains("requested_exclude_ids as"));
        assert!(main_query.contains("from unnest($10::text[]) as item_id"));
        assert!(main_query.contains("from unnest($12::text[]) as item_id"));
        assert!(main_query.contains("select distinct item_id::uuid as public_id"));
        assert!(main_query.contains("from requested_include_ids requested"));
        assert!(main_query.contains("from requested_exclude_ids requested"));
        assert!(main_query.contains("where requested.public_id = mi.public_id"));
        assert!(!main_query.contains(&bad_include_filter));
        assert!(!main_query.contains(&bad_exclude_filter));
    }

    #[test]
    fn main_browse_association_id_filters_use_uuid_sets() {
        let repository = include_str!("repository.rs");
        let main_query = repository
            .split("pub async fn list_user_items")
            .nth(1)
            .unwrap()
            .split("async fn list_user_items_from_playstates")
            .next()
            .unwrap();
        let bad_genre_filter = format!("{}{}", "g.id::text = any(", "$22");
        let bad_person_filter = format!("{}{}", "p.public_id::text = any(", "$26");
        let bad_artist_filter = format!("{}{}", "p.public_id::text = any(", "$32");
        let bad_artist_item_filter = format!("{}{}", "artist_item.public_id::text = any(", "$32");
        let bad_studio_filter = format!("{}{}", "s.public_id::text = any(", "$65");
        let bad_album_filter = format!("{}{}", "mi.public_id::text = any(", "$75");
        let bad_parent_album_filter = format!("{}{}", "parent.public_id::text = any(", "$75");

        assert!(main_query.contains("requested_genre_ids as"));
        assert!(main_query.contains("requested_person_ids as"));
        assert!(main_query.contains("requested_artist_ids as"));
        assert!(main_query.contains("requested_album_ids as"));
        assert!(main_query.contains("requested_studio_ids as"));
        assert!(main_query.contains("from unnest($22::text[]) as genre_id"));
        assert!(main_query.contains("where genre_id ~ '^[0-9]{1,18}$'"));
        assert!(main_query.contains("select distinct genre_id::bigint as id"));
        assert!(main_query.contains("from unnest($26::text[]) as person_id"));
        assert!(main_query.contains("from unnest($32::text[]) as artist_id"));
        assert!(main_query.contains("from unnest($75::text[]) as album_id"));
        assert!(main_query.contains("from unnest($65::text[]) as studio_id"));
        assert!(main_query.contains("from requested_genre_ids requested"));
        assert!(main_query.contains("where requested.id = mig.genre_id"));
        assert!(main_query.contains("from requested_person_ids requested"));
        assert!(main_query.contains("from requested_artist_ids requested"));
        assert!(main_query.contains("from requested_album_ids requested"));
        assert!(main_query.contains("from requested_studio_ids requested"));
        assert!(main_query.contains("where requested.public_id = p.public_id"));
        assert!(main_query.contains("where requested.public_id = mi.public_id"));
        assert!(main_query.contains("where requested.public_id = parent.public_id"));
        assert!(main_query.contains("on requested.public_id = artist_item.public_id"));
        assert!(main_query.contains("where requested.public_id = s.public_id"));
        assert!(!main_query.contains(&bad_genre_filter));
        assert!(!main_query.contains(&bad_person_filter));
        assert!(!main_query.contains(&bad_artist_filter));
        assert!(!main_query.contains(&bad_artist_item_filter));
        assert!(!main_query.contains(&bad_album_filter));
        assert!(!main_query.contains(&bad_parent_album_filter));
        assert!(!main_query.contains(&bad_studio_filter));
    }

    #[test]
    fn uuid_text_validation_matches_public_id_shape() {
        assert!(is_uuid_text("bbbbbbbb-0000-0000-0000-000000000001"));
        assert!(is_uuid_text("BBBBBBBB-0000-0000-0000-000000000001"));
        assert!(!is_uuid_text("item-1"));
        assert!(!is_uuid_text("bbbbbbbb000000000000000000000001"));
        assert!(!is_uuid_text("bbbbbbbb-0000-0000-0000-00000000000x"));
    }

    #[test]
    fn include_ids_fast_path_only_accepts_uuid_only_scalar_filters() {
        let mut ids = BrowseItemsInput {
            user_id: 1,
            parent_id: None,
            start_index: 0,
            limit: 20,
            recursive: true,
            include_image_tags: true,
            options: ItemQueryOptions::default(),
        };
        ids.options.scalar_filter.include_ids = StringListFilter::enabled(vec![
            "bbbbbbbb-0000-0000-0000-000000000001".to_owned(),
            "bbbbbbbb-0000-0000-0000-000000000002".to_owned(),
        ]);
        assert!(ids.can_use_include_ids_fast_path());

        let mut typed = ids.clone();
        typed.options.type_filter = ItemTypeFilter::enabled(vec!["movie".to_owned()]);
        assert!(typed.can_use_include_ids_fast_path());

        let mut invalid = ids.clone();
        invalid.options.scalar_filter.include_ids =
            StringListFilter::enabled(vec!["item-2".to_owned()]);
        assert!(!invalid.can_use_include_ids_fast_path());

        let mut excluded = ids.clone();
        excluded.options.scalar_filter.exclude_ids =
            StringListFilter::enabled(vec!["bbbbbbbb-0000-0000-0000-000000000003".to_owned()]);
        assert!(!excluded.can_use_include_ids_fast_path());

        let mut search = ids.clone();
        search.options.scalar_filter.search_term = Some("movie".to_owned());
        assert!(!search.can_use_include_ids_fast_path());

        let mut provider = ids.clone();
        provider.options.provider_filter.any_provider_id_equals =
            StringListFilter::enabled(vec!["tmdb.123".to_owned()]);
        assert!(!provider.can_use_include_ids_fast_path());
    }

    #[test]
    fn include_ids_fast_path_query_uses_public_id_uuid_index_shape() {
        let migration = include_str!("../../migrations/0002_core_media.sql");
        let media_items_table = migration
            .split("create table if not exists media_items")
            .nth(1)
            .unwrap()
            .split("create table if not exists media_files")
            .next()
            .unwrap();
        let repository = include_str!("repository.rs");
        let fast_query = repository
            .split("async fn list_user_items_by_include_ids")
            .nth(1)
            .unwrap()
            .split("async fn list_user_items_by_provider_ids")
            .next()
            .unwrap();

        assert!(media_items_table.contains("public_id uuid not null default gen_random_uuid()"));
        assert!(media_items_table.contains("unique (public_id)"));
        assert!(fast_query.contains("requested_ids as"));
        assert!(fast_query.contains("select distinct item_id::uuid as public_id"));
        assert!(fast_query.contains("from unnest($9::text[]) as item_id"));
        assert!(fast_query.contains("join media_items mi on mi.public_id = requested.public_id"));
        assert!(!fast_query.contains("mi.public_id::text = any"));
    }

    #[test]
    fn include_ids_fast_path_query_starts_from_requested_ids() {
        let repository = include_str!("repository.rs");
        let fast_query = repository
            .split("async fn list_user_items_by_include_ids")
            .nth(1)
            .unwrap()
            .split("async fn list_user_items_by_provider_ids")
            .next()
            .unwrap();

        assert!(fast_query.contains("from requested_ids requested"));
        assert!(fast_query.contains("join browse_scope scope on scope.library_id = mi.library_id"));
        assert!(fast_query.contains("left join user_playstates up"));
        assert!(fast_query.contains("count(*) over() as total_record_count"));
        assert!(!fast_query.contains("media_item_people"));
        assert!(!fast_query.contains("media_streams"));
        assert!(!fast_query.contains("media_external_ids"));
    }

    #[test]
    fn playlist_queries_keep_uuid_and_permission_boundaries() {
        let repository = include_str!("repository.rs");
        let list_query = repository
            .split("pub async fn list_user_playlists")
            .nth(1)
            .unwrap()
            .split("pub async fn list_user_playlist_items")
            .next()
            .unwrap();
        let items_query = repository
            .split("pub async fn list_user_playlist_items")
            .nth(1)
            .unwrap()
            .split("pub async fn list_user_genres")
            .next()
            .unwrap();

        assert!(list_query.contains("with requested_parent as"));
        assert!(list_query.contains("then $2::uuid"));
        assert!(list_query.contains("left join library_permissions lp"));
        assert!(list_query.contains("lp.user_id = $1"));
        assert!(list_query.contains("lp.can_view = true"));
        assert!(list_query.contains("l.is_hidden = false"));
        assert!(list_query.contains("exists ("));
        assert!(!list_query.contains("c.public_id::text = $2"));

        assert!(items_query.contains("with requested_playlist as"));
        assert!(items_query.contains("then $2::uuid"));
        assert!(items_query.contains("join collection_items ci on ci.collection_id = scope.id"));
        assert!(
            items_query.contains("join library_permissions lp on lp.library_id = mi.library_id")
        );
        assert!(items_query.contains("lp.user_id = $1"));
        assert!(items_query.contains("lp.can_view = true"));
        assert!(items_query.contains("l.is_hidden = false"));
        assert!(items_query.contains("order by ci.sort_order asc"));
        assert!(!items_query.contains("c.public_id::text = $2"));
    }

    #[test]
    fn studio_queries_keep_permission_and_uuid_parent_boundaries() {
        let repository = include_str!("repository.rs");
        let list_query = repository
            .split("pub async fn list_user_studios")
            .nth(1)
            .unwrap()
            .split("pub async fn find_user_studio_by_name")
            .next()
            .unwrap();
        let by_name_query = repository
            .split("pub async fn find_user_studio_by_name")
            .nth(1)
            .unwrap()
            .split("pub async fn list_user_artists")
            .next()
            .unwrap();

        assert!(list_query.contains("with recursive requested_parent as"));
        assert!(list_query.contains("then $2::uuid"));
        assert!(list_query.contains("join library_permissions lp"));
        assert!(list_query.contains("lp.user_id = $1"));
        assert!(list_query.contains("lp.can_view = true"));
        assert!(list_query.contains("l.is_hidden = false"));
        assert!(list_query.contains("from media_item_studios mis"));
        assert!(list_query.contains("join studios s on s.id = mis.studio_id"));
        assert!(!list_query.contains("mi.public_id::text = $2"));

        assert!(by_name_query.contains("s.name_normalized = lower($2)"));
        assert!(by_name_query.contains("exists ("));
        assert!(by_name_query.contains("lp.user_id = $1"));
        assert!(by_name_query.contains("lp.can_view = true"));
        assert!(by_name_query.contains("l.is_hidden = false"));
    }

    #[test]
    fn classification_queries_keep_permission_and_uuid_parent_boundaries() {
        let repository = include_str!("repository.rs");
        let item_filters_query = repository
            .split("pub async fn list_user_item_filters")
            .nth(1)
            .unwrap()
            .split("pub async fn list_user_genres")
            .next()
            .unwrap();
        let tags_query = repository
            .split("pub async fn list_user_tags")
            .nth(1)
            .unwrap()
            .split("pub async fn list_user_official_ratings")
            .next()
            .unwrap();
        let ratings_query = repository
            .split("pub async fn list_user_official_ratings")
            .nth(1)
            .unwrap()
            .split("pub async fn list_user_years")
            .next()
            .unwrap();
        let years_query = repository
            .split("pub async fn list_user_years")
            .nth(1)
            .unwrap()
            .split("pub async fn list_user_technical_facets")
            .next()
            .unwrap();
        let technical_query = repository
            .split("pub async fn list_user_technical_facets")
            .nth(1)
            .unwrap()
            .split("pub async fn list_user_artists")
            .next()
            .unwrap();

        assert!(item_filters_query.contains("with recursive requested_parent as"));
        assert!(item_filters_query.contains("then $2::uuid"));
        assert!(item_filters_query.contains("join library_permissions lp"));
        assert!(item_filters_query.contains("lp.user_id = $1"));
        assert!(item_filters_query.contains("lp.can_view = true"));
        assert!(item_filters_query.contains("l.is_hidden = false"));
        assert!(item_filters_query.contains("mi.is_deleted = false"));
        assert!(item_filters_query.contains("from media_item_genres mig"));
        assert!(item_filters_query.contains("from media_item_tags mit"));
        assert!(item_filters_query.contains("mi.item_type = any($4::text[])"));
        assert!(item_filters_query.contains(
            "when mi.item_type in ('movie', 'series', 'season', 'episode') then 'video'"
        ));
        assert!(item_filters_query.contains("= any($6::text[])"));
        assert!(!item_filters_query.contains("mi.public_id::text = $2"));

        assert!(tags_query.contains("with recursive requested_parent as"));
        assert!(tags_query.contains("then $2::uuid"));
        assert!(tags_query.contains("join library_permissions lp"));
        assert!(tags_query.contains("lp.user_id = $1"));
        assert!(tags_query.contains("lp.can_view = true"));
        assert!(tags_query.contains("l.is_hidden = false"));
        assert!(tags_query.contains("from media_item_tags mit"));
        assert!(tags_query.contains("join tags t on t.id = mit.tag_id"));
        assert!(!tags_query.contains("mi.public_id::text = $2"));

        assert!(ratings_query.contains("with recursive requested_parent as"));
        assert!(ratings_query.contains("then $2::uuid"));
        assert!(ratings_query.contains("join library_permissions lp"));
        assert!(ratings_query.contains("lp.user_id = $1"));
        assert!(ratings_query.contains("lp.can_view = true"));
        assert!(ratings_query.contains("l.is_hidden = false"));
        assert!(ratings_query.contains("mi.official_rating is not null"));
        assert!(!ratings_query.contains("mi.public_id::text = $2"));

        assert!(years_query.contains("with recursive requested_parent as"));
        assert!(years_query.contains("then $2::uuid"));
        assert!(years_query.contains("join library_permissions lp"));
        assert!(years_query.contains("lp.user_id = $1"));
        assert!(years_query.contains("lp.can_view = true"));
        assert!(years_query.contains("l.is_hidden = false"));
        assert!(years_query.contains("mi.production_year is not null"));
        assert!(!years_query.contains("mi.public_id::text = $2"));

        assert!(technical_query.contains("with recursive requested_parent as"));
        assert!(technical_query.contains("then $2::uuid"));
        assert!(technical_query.contains("join library_permissions lp"));
        assert!(technical_query.contains("lp.user_id = $1"));
        assert!(technical_query.contains("lp.can_view = true"));
        assert!(technical_query.contains("l.is_hidden = false"));
        assert!(technical_query.contains("mi.is_deleted = false"));
        assert!(technical_query.contains("join media_files mf"));
        assert!(technical_query.contains("join media_streams ms"));
        assert!(technical_query.contains("ms.stream_type = 'audio'"));
        assert!(technical_query.contains("ms.stream_type = 'video'"));
        assert!(technical_query.contains("ms.stream_type = 'subtitle'"));
        assert!(!technical_query.contains("mi.public_id::text = $2"));
    }

    #[test]
    fn resume_and_latest_parent_scope_queries_use_uuid_public_id_comparisons() {
        let repository = include_str!("repository.rs");
        let query_ranges = [
            (
                "pub async fn list_resume_items",
                "pub async fn list_latest_items",
            ),
            (
                "pub async fn list_latest_items",
                "pub async fn list_similar_items",
            ),
        ];

        for (start, end) in query_ranges {
            let query = repository
                .split(start)
                .nth(1)
                .unwrap_or_else(|| panic!("missing query start `{start}`"))
                .split(end)
                .next()
                .unwrap_or_else(|| panic!("missing query end `{end}`"));

            assert!(query.contains("with requested_parent as"));
            assert!(query.contains("when $4::text ~*"));
            assert!(query.contains("then $4::uuid"));
            assert!(query.contains("cross join requested_parent rp"));
            assert!(query.contains("l.public_id = rp.public_id"));
            assert!(query.contains("parent.public_id = rp.public_id"));
            assert!(!query.contains("l.public_id::text = $4"));
            assert!(!query.contains("parent.public_id::text = $4"));
        }
    }

    #[test]
    fn provider_id_fast_path_only_accepts_narrow_provider_filters() {
        let mut provider = BrowseItemsInput {
            user_id: 1,
            parent_id: None,
            start_index: 0,
            limit: 20,
            recursive: true,
            include_image_tags: true,
            options: ItemQueryOptions::default(),
        };
        provider.options.provider_filter.any_provider_id_equals =
            StringListFilter::enabled(vec!["tmdb.123".to_owned()]);
        assert!(provider.can_use_provider_id_fast_path());

        let mut typed = provider.clone();
        typed.options.type_filter = ItemTypeFilter::enabled(vec!["movie".to_owned()]);
        assert!(typed.can_use_provider_id_fast_path());

        let mut empty = provider.clone();
        empty.options.provider_filter.any_provider_id_equals =
            StringListFilter::enabled(Vec::new());
        assert!(!empty.can_use_provider_id_fast_path());

        let mut search = provider.clone();
        search.options.scalar_filter.search_term = Some("movie".to_owned());
        assert!(!search.can_use_provider_id_fast_path());

        let mut user_state = provider.clone();
        user_state.options.user_data_filter.require_favorite = true;
        assert!(!user_state.can_use_provider_id_fast_path());

        let mut media = provider.clone();
        media.options.media_filter.containers = StringListFilter::enabled(vec!["mkv".to_owned()]);
        assert!(!media.can_use_provider_id_fast_path());
    }

    #[test]
    fn provider_id_index_matches_fast_path_lookup() {
        let migration = include_str!("../../migrations/0027_provider_id_filter_indexes.sql");
        let repository = include_str!("repository.rs");
        let fast_query = repository
            .split("async fn list_user_items_by_provider_ids")
            .nth(1)
            .unwrap()
            .split("pub async fn list_resume_items")
            .next()
            .unwrap();

        assert!(migration.contains("idx_media_external_ids_provider_external_lower_item"));
        assert!(migration.contains("lower(provider || '.' || external_id)"));
        assert!(migration.contains("media_item_id"));
        assert!(fast_query.contains("from media_external_ids mei"));
        assert!(fast_query.contains("select distinct mei.media_item_id"));
        assert!(fast_query.contains("lower(mei.provider || '.' || mei.external_id) = any"));
    }

    #[test]
    fn provider_id_fast_path_query_starts_from_external_ids() {
        let repository = include_str!("repository.rs");
        let fast_query = repository
            .split("async fn list_user_items_by_provider_ids")
            .nth(1)
            .unwrap()
            .split("pub async fn list_resume_items")
            .next()
            .unwrap();

        assert!(fast_query.contains("provider_items as"));
        assert!(fast_query.contains("from provider_items provider_match"));
        assert!(fast_query.contains("join media_items mi on mi.id = provider_match.media_item_id"));
        assert!(fast_query.contains("join browse_scope scope on scope.library_id = mi.library_id"));
        assert!(fast_query.contains("left join user_playstates up"));
        assert!(fast_query.contains("count(*) over() as total_record_count"));
        assert!(!fast_query.contains("media_item_people"));
        assert!(!fast_query.contains("media_streams"));
        assert!(!fast_query.contains("media_item_tags"));
    }

    #[test]
    fn item_prefixes_query_uses_uuid_parent_and_type_allowlist() {
        let migration = include_str!("../../migrations/0042_media_directory_name_indexes.sql");
        let repository = include_str!("repository.rs");
        let prefix_query = repository
            .split("pub async fn list_user_item_prefixes")
            .nth(1)
            .unwrap()
            .split("async fn list_user_items_from_playstates")
            .next()
            .unwrap();

        assert!(migration.contains("idx_media_items_visible_sort_key_lower_pattern"));
        assert!(migration.contains("lower(coalesce(nullif(sort_title, ''), title))"));
        assert!(prefix_query.contains("requested_parent as"));
        assert!(prefix_query.contains("then $2::uuid"));
        assert!(prefix_query.contains("l.public_id = rp.public_id"));
        assert!(prefix_query.contains("mi.public_id = rp.public_id"));
        assert!(prefix_query.contains("mi.item_type = any($7::text[])"));
        assert!(
            prefix_query
                .contains("upper(left(trim(coalesce(nullif(mi.sort_title, ''), mi.title)), 1))")
        );
        assert!(!prefix_query.contains("public_id::text = $2"));
        assert!(!prefix_query.contains("item_type = any($7)"));
    }

    #[test]
    fn show_child_index_matches_season_episode_ordering() {
        let migration = include_str!("../../migrations/0036_media_item_parent_type_index_sort.sql");
        let repository = include_str!("repository.rs");
        let episodes_query = repository
            .split("pub async fn list_series_episodes")
            .nth(1)
            .unwrap()
            .split("pub async fn list_next_up_items")
            .next()
            .unwrap();

        assert!(migration.contains("idx_media_items_parent_type_index_sort"));
        assert!(migration.contains("parent_id, item_type, index_number, sort_title, id"));
        assert!(migration.contains("where is_deleted = false"));
        for included_column in [
            "public_id",
            "title",
            "runtime_ticks",
            "production_year",
            "parent_index_number",
        ] {
            assert!(migration.contains(included_column));
        }
        assert!(repository.contains("join media_items season on season.parent_id = series.id"));
        assert!(repository.contains("join media_items episode on episode.parent_id = season.id"));
        assert!(repository.contains("season.item_type = 'season'"));
        assert!(repository.contains("episode.item_type = 'episode'"));
        assert!(repository.contains("order by season.index_number nulls last"));
        assert!(repository.contains("episode.index_number nulls last"));
        assert!(episodes_query.contains("episode_parent_scope as"));
        assert!(episodes_query.contains("from episode_parent_scope episode_parent"));
        assert!(
            episodes_query
                .contains("join media_items episode on episode.parent_id = episode_parent.id")
        );
        assert!(!episodes_query.contains("episode.parent_id in (select id from selected_seasons)"));
        assert!(!episodes_query.contains("episode.parent_id in (select id from series_scope)"));
    }

    #[test]
    fn show_and_next_up_public_id_filters_use_uuid_comparisons() {
        let repository = include_str!("repository.rs");
        let seasons_query = repository
            .split("pub async fn list_series_seasons")
            .nth(1)
            .unwrap()
            .split("pub async fn list_series_episodes")
            .next()
            .unwrap();
        let episodes_query = repository
            .split("pub async fn list_series_episodes")
            .nth(1)
            .unwrap()
            .split("pub async fn list_next_up_items")
            .next()
            .unwrap();
        let next_up_query = repository
            .split("pub async fn list_next_up_items")
            .nth(1)
            .unwrap()
            .split("pub async fn find_user_item_by_id")
            .next()
            .unwrap();
        let bad_series_filter = format!("{}{}", "series.public_id::text = ", "$");
        let bad_season_filter = format!("{}{}", "season.public_id::text = ", "$");
        let bad_library_filter = format!("{}{}", "l.public_id::text = ", "$");

        assert!(seasons_query.contains("where series.public_id = case"));
        assert!(seasons_query.contains("then $2::uuid"));
        assert!(!seasons_query.contains(&bad_series_filter));

        assert!(episodes_query.contains("where series.public_id = case"));
        assert!(episodes_query.contains("then $2::uuid"));
        assert!(episodes_query.contains("or season.public_id = case"));
        assert!(episodes_query.contains("then $5::uuid"));
        assert!(!episodes_query.contains(&bad_series_filter));
        assert!(!episodes_query.contains(&bad_season_filter));

        assert!(next_up_query.contains("or series.public_id = case"));
        assert!(next_up_query.contains("then $4::uuid"));
        assert!(next_up_query.contains("or l.public_id = case"));
        assert!(next_up_query.contains("then $5::uuid"));
        assert!(!next_up_query.contains(&bad_series_filter));
        assert!(!next_up_query.contains(&bad_library_filter));
    }

    #[test]
    fn admin_physical_paths_query_returns_enabled_paths_without_dynamic_filters() {
        let query = admin_physical_paths_query();

        assert!(query.contains("from library_paths lp"));
        assert!(query.contains("join libraries l on l.id = lp.library_id"));
        assert!(query.contains("lp.is_enabled = true"));
        assert!(query.contains("nullif(trim(lp.path), '')"));
        assert!(query.contains("order by l.name, lp.path, lp.id"));
        assert!(!query.contains("format!("));
        assert!(!query.contains("public_id::text"));
    }

    #[test]
    fn artwork_image_tag_index_matches_browse_aggregation_order() {
        let migration = include_str!("../../migrations/0037_artwork_media_item_tag_order.sql");
        let repository = include_str!("repository.rs");

        assert!(migration.contains("idx_artwork_media_item_type_primary_id"));
        assert!(migration.contains("media_item_id, artwork_type, is_primary desc, id"));
        assert!(migration.contains("where media_item_id is not null"));
        assert!(repository.contains("from artwork a"));
        assert!(
            repository.contains(
                "array_agg(a.artwork_type || '=' || a.id::text order by a.artwork_type, a.is_primary desc, a.id)"
            )
        );
        assert!(repository.contains("item_images on $6::boolean = true"));
    }

    #[test]
    fn media_directory_name_indexes_match_directory_queries() {
        let migration = include_str!("../../migrations/0042_media_directory_name_indexes.sql");
        let repository = include_str!("repository.rs");
        let main_query = repository
            .split("pub async fn list_user_items")
            .nth(1)
            .unwrap()
            .split("async fn list_user_items_from_playstates")
            .next()
            .unwrap();
        let genre_query = repository
            .split("pub async fn list_user_genres")
            .nth(1)
            .unwrap()
            .split("pub async fn find_user_genre_by_name")
            .next()
            .unwrap();
        let artist_query = repository
            .split("pub async fn list_user_artists")
            .nth(1)
            .unwrap()
            .split("pub async fn find_user_artist_by_name")
            .next()
            .unwrap();
        let person_query = repository
            .split("pub async fn list_user_persons")
            .nth(1)
            .unwrap()
            .split("pub async fn find_user_person_by_name")
            .next()
            .unwrap();

        assert!(migration.contains("create extension if not exists pg_trgm"));
        for index in [
            "idx_genres_lower_name_pattern",
            "idx_genres_lower_name_trgm",
            "idx_people_lower_name_pattern",
            "idx_people_lower_name_trgm",
            "idx_media_items_artist_lower_title_pattern",
            "idx_media_items_artist_lower_title_trgm",
            "idx_media_items_visible_title_lower_trgm",
            "idx_media_items_visible_original_title_lower_trgm",
            "idx_media_items_visible_sort_title_lower_trgm",
            "idx_media_items_visible_sort_key_lower_pattern",
            "idx_media_item_people_item_role_person",
        ] {
            assert!(migration.contains(index));
        }
        assert!(migration.contains("lower(name) text_pattern_ops"));
        assert!(migration.contains("lower(name) gin_trgm_ops"));
        assert!(migration.contains("lower(title) text_pattern_ops"));
        assert!(migration.contains("lower(title) gin_trgm_ops"));
        assert!(migration.contains("lower(original_title) gin_trgm_ops"));
        assert!(migration.contains("lower(sort_title) gin_trgm_ops"));
        assert!(
            migration.contains("lower(coalesce(nullif(sort_title, ''), title)) text_pattern_ops")
        );
        assert!(migration.contains("media_item_people (media_item_id, role_type, person_id)"));

        assert!(main_query.contains("lower(mi.title) like '%' || lower($15::text) || '%'"));
        assert!(
            main_query.contains("lower(mi.original_title) like '%' || lower($15::text) || '%'")
        );
        assert!(main_query.contains("lower(mi.sort_title) like '%' || lower($15::text) || '%'"));
        assert!(main_query.contains(
            "lower(coalesce(nullif(mi.sort_title, ''), mi.title)) like lower($16::text) || '%'"
        ));

        assert!(genre_query.contains("lower(g.name) like '%' || lower($7::text) || '%'"));
        assert!(genre_query.contains("lower(g.name) like lower($8::text) || '%'"));
        assert!(genre_query.contains("lower(g.name) >= lower($9::text)"));
        assert!(genre_query.contains("case when $10 = 'asc' then lower(name)"));

        assert!(artist_query.contains("from visible_music_items mi"));
        assert!(artist_query.contains("mi.item_type = 'artist'"));
        assert!(artist_query.contains("lower(name) like '%' || lower($7::text) || '%'"));
        assert!(artist_query.contains("lower(name) like lower($8::text) || '%'"));
        assert!(artist_query.contains("lower(name) >= lower($9::text)"));

        assert!(person_query.contains("lower(p.name) like '%' || lower($8::text) || '%'"));
        assert!(person_query.contains("lower(p.name) like lower($9::text) || '%'"));
        assert!(person_query.contains("lower(p.name) >= lower($10::text)"));
        assert!(person_query.contains("case when $11 = 'asc' then lower(name)"));
    }

    #[test]
    fn latest_items_query_does_not_force_exact_total_count() {
        let repository = include_str!("repository.rs");
        let latest_query = repository
            .split("pub async fn list_latest_items")
            .nth(1)
            .unwrap()
            .split("pub async fn list_similar_items")
            .next()
            .unwrap();

        assert!(!latest_query.contains("count(*) over()"));
        assert!(latest_query.contains("0::bigint as total_record_count"));
        assert!(latest_query.contains("mi.item_type in ('movie', 'series', 'episode', 'track')"));
    }

    #[test]
    fn artist_album_filters_use_uuid_sets_and_album_title_index() {
        let migration = include_str!("../../migrations/0059_artist_album_filter_indexes.sql");
        let repository = include_str!("repository.rs");
        let artist_query = repository
            .split("pub async fn list_user_artists")
            .nth(1)
            .unwrap()
            .split("pub async fn find_user_artist_by_name")
            .next()
            .unwrap();

        assert!(migration.contains("idx_media_items_album_lower_title_public"));
        assert!(migration.contains("on media_items (lower(title), public_id)"));
        assert!(migration.contains("where item_type = 'album'"));

        assert!(artist_query.contains("requested_album_ids as"));
        assert!(artist_query.contains("from unnest($14::text[]) as album_id"));
        assert!(artist_query.contains("left join media_items album_parent"));
        assert!(artist_query.contains("lower(mi.title) = any($12::text[])"));
        assert!(artist_query.contains("lower(album_parent.title) = any($12::text[])"));
        assert!(artist_query.contains("from requested_album_ids requested"));
        assert!(artist_query.contains("where requested.public_id = mi.public_id"));
        assert!(artist_query.contains("where requested.public_id = album_parent.public_id"));
        assert!(artist_query.contains("where mi.item_type = 'artist'"));
        assert!(artist_query.contains("and $11::boolean = false"));
        assert!(artist_query.contains("and $13::boolean = false"));
        assert!(!artist_query.contains("mi.public_id::text = any($14"));
        assert!(!artist_query.contains("album_parent.public_id::text = any($14"));
    }

    #[test]
    fn resume_items_query_uses_limit_probe_without_exact_total_count() {
        let repository = include_str!("repository.rs");
        let resume_query = repository
            .split("pub async fn list_resume_items")
            .nth(1)
            .unwrap()
            .split("pub async fn list_latest_items")
            .next()
            .unwrap();

        assert!(resume_query.contains("let fetch_limit = input.limit.saturating_add(1);"));
        assert!(!resume_query.contains("count(*) over()"));
        assert!(resume_query.contains("0::bigint as total_record_count"));
        assert!(resume_query.contains(".bind(fetch_limit)"));
        assert!(
            resume_query.contains(
                "browse_result_lower_bound_from_rows(rows, input.start_index, input.limit)"
            )
        );
    }

    #[test]
    fn similar_items_query_uses_limit_probe_without_exact_total_count() {
        let repository = include_str!("repository.rs");
        let similar_query = repository
            .split("pub async fn list_similar_items")
            .nth(1)
            .unwrap()
            .split("pub async fn list_series_seasons")
            .next()
            .unwrap();

        assert!(similar_query.contains("let fetch_limit = input.limit.saturating_add(1);"));
        assert!(!similar_query.contains("count(*) over()"));
        assert!(similar_query.contains("0::bigint as total_record_count"));
        assert!(similar_query.contains(".bind(fetch_limit)"));
        assert!(
            similar_query.contains(
                "browse_result_lower_bound_from_rows(rows, input.start_index, input.limit)"
            )
        );
    }

    #[test]
    fn next_up_query_uses_limit_probe_without_exact_total_count() {
        let repository = include_str!("repository.rs");
        let next_up_query = repository
            .split("pub async fn list_next_up_items")
            .nth(1)
            .unwrap()
            .split("pub async fn find_user_item_by_id")
            .next()
            .unwrap();

        assert!(next_up_query.contains("let fetch_limit = input.limit.saturating_add(1);"));
        assert!(!next_up_query.contains("count(*) over()"));
        assert!(next_up_query.contains("0::bigint as total_record_count"));
        assert!(next_up_query.contains(".bind(fetch_limit)"));
        assert!(
            next_up_query.contains(
                "browse_result_lower_bound_from_rows(rows, input.start_index, input.limit)"
            )
        );
    }
}
