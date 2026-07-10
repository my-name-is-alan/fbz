use std::collections::BTreeMap;

use axum::{
    Json,
    body::Bytes,
    extract::{Path, Query, State},
    http::{HeaderMap, StatusCode, Uri},
    response::Response,
};
use serde::{Deserialize, Serialize};

use crate::{
    admin::repository::{AdminRepository, QueueMetadataRefreshInput},
    auth::service::AuthenticatedUser,
    compat::emby::payload::parse_emby_body,
    error::AppError,
    media::repository::MediaRepository,
    metadata::remote_search::{RemoteSearchCandidate, RemoteSearchRequest, RemoteSearchService},
    state::AppState,
};

use super::access::authenticate_request_user;
use super::images::{ensure_public_remote_image_url, remote_image_passthrough_response};

const MAX_LOOKUP_BODY_BYTES: usize = 64 * 1024;
const MAX_LOOKUP_TEXT_LEN: usize = 256;
const MAX_LOOKUP_PROVIDER_LEN: usize = 128;
const MAX_LOOKUP_ITEM_IDS: usize = 100;
const MAX_LOOKUP_URL_LEN: usize = 2048;

#[derive(Clone, Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "PascalCase")]
pub struct ExternalIdInfoDto {
    pub name: String,
    pub key: String,
    pub website: Option<String>,
    pub url_format_string: Option<String>,
    pub is_supported_as_identifier: bool,
}

#[derive(Clone, Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "PascalCase")]
pub struct MetadataEditorInfoDto {
    pub parental_rating_options: Vec<MetadataEditorParentalRatingDto>,
    pub countries: Vec<MetadataEditorCountryInfoDto>,
    pub cultures: Vec<MetadataEditorCultureDto>,
    pub external_id_infos: Vec<MetadataEditorExternalIdInfoDto>,
}

#[derive(Clone, Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "PascalCase")]
pub struct MetadataEditorParentalRatingDto {
    pub name: String,
    pub value: i32,
}

#[derive(Clone, Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "PascalCase")]
pub struct MetadataEditorCountryInfoDto {
    pub name: String,
    pub display_name: String,
    #[serde(rename = "TwoLetterISORegionName")]
    pub two_letter_iso_region_name: String,
    #[serde(rename = "ThreeLetterISORegionName")]
    pub three_letter_iso_region_name: String,
}

#[derive(Clone, Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "PascalCase")]
pub struct MetadataEditorCultureDto {
    pub name: String,
    pub display_name: String,
    #[serde(rename = "TwoLetterISOLanguageName")]
    pub two_letter_iso_language_name: String,
    #[serde(rename = "ThreeLetterISOLanguageName")]
    pub three_letter_iso_language_name: String,
    #[serde(rename = "ThreeLetterISOLanguageNames")]
    pub three_letter_iso_language_names: Vec<String>,
}

#[derive(Clone, Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "PascalCase")]
pub struct MetadataEditorExternalIdInfoDto {
    pub name: String,
    pub key: String,
    pub url_format_string: Option<String>,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "PascalCase")]
pub struct RemoteSearchResultDto {
    #[serde(alias = "name")]
    pub name: Option<String>,
    #[serde(alias = "originalTitle", alias = "original_title")]
    pub original_title: Option<String>,
    #[serde(alias = "providerIds", alias = "provider_ids")]
    pub provider_ids: Option<BTreeMap<String, String>>,
    #[serde(alias = "productionYear", alias = "production_year")]
    pub production_year: Option<i32>,
    #[serde(alias = "indexNumber", alias = "index_number")]
    pub index_number: Option<i32>,
    #[serde(alias = "indexNumberEnd", alias = "index_number_end")]
    pub index_number_end: Option<i32>,
    #[serde(alias = "parentIndexNumber", alias = "parent_index_number")]
    pub parent_index_number: Option<i32>,
    #[serde(alias = "sortIndexNumber", alias = "sort_index_number")]
    pub sort_index_number: Option<i32>,
    #[serde(alias = "sortParentIndexNumber", alias = "sort_parent_index_number")]
    pub sort_parent_index_number: Option<i32>,
    #[serde(alias = "premiereDate", alias = "premiere_date")]
    pub premiere_date: Option<String>,
    #[serde(alias = "startDate", alias = "start_date")]
    pub start_date: Option<String>,
    #[serde(alias = "endDate", alias = "end_date")]
    pub end_date: Option<String>,
    #[serde(alias = "imageUrl", alias = "image_url")]
    pub image_url: Option<String>,
    #[serde(alias = "searchProviderName", alias = "search_provider_name")]
    pub search_provider_name: Option<String>,
    #[serde(alias = "gameSystem", alias = "game_system")]
    pub game_system: Option<String>,
    #[serde(alias = "overview")]
    pub overview: Option<String>,
    #[serde(alias = "disambiguationComment", alias = "disambiguation_comment")]
    pub disambiguation_comment: Option<String>,
    #[serde(alias = "albumArtist", alias = "album_artist")]
    pub album_artist: Option<String>,
    #[serde(alias = "artists")]
    pub artists: Option<Vec<String>>,
}

#[derive(Clone, Debug, Default, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "PascalCase")]
pub struct RemoteSearchImageQuery {
    #[serde(alias = "imageUrl", alias = "image_url")]
    pub image_url: Option<String>,
    #[serde(alias = "providerName", alias = "provider_name")]
    pub provider_name: Option<String>,
}

#[derive(Clone, Debug, Default, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "PascalCase")]
pub struct MetadataResetQuery {
    #[serde(alias = "itemIds", alias = "item_ids")]
    pub item_ids: Option<String>,
}

#[derive(Clone, Debug, Default, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "PascalCase")]
pub struct RemoteSearchApplyQuery {
    #[serde(alias = "replaceAllImages", alias = "replace_all_images")]
    pub replace_all_images: Option<bool>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum RemoteSearchKind {
    Book,
    BoxSet,
    Game,
    Movie,
    MusicAlbum,
    MusicArtist,
    MusicVideo,
    Person,
    Series,
    Trailer,
}

#[derive(Clone, Debug, Default, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "PascalCase")]
struct RemoteSearchQueryDto {
    #[serde(alias = "searchInfo", alias = "search_info")]
    pub search_info: Option<RemoteSearchInfoDto>,
    #[serde(alias = "itemId", alias = "item_id")]
    pub item_id: Option<FlexibleTextValue>,
    #[serde(alias = "searchProviderName", alias = "search_provider_name")]
    pub search_provider_name: Option<String>,
    #[serde(alias = "providers")]
    pub providers: Option<Vec<String>>,
    #[serde(
        alias = "includeDisabledProviders",
        alias = "include_disabled_providers"
    )]
    pub include_disabled_providers: Option<bool>,
}

#[derive(Clone, Debug, Default, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "PascalCase")]
struct RemoteSearchInfoDto {
    #[serde(alias = "name")]
    pub name: Option<String>,
    #[serde(alias = "path")]
    pub path: Option<String>,
    #[serde(alias = "metadataLanguage", alias = "metadata_language")]
    pub metadata_language: Option<String>,
    #[serde(alias = "metadataCountryCode", alias = "metadata_country_code")]
    pub metadata_country_code: Option<String>,
    #[serde(alias = "providerIds", alias = "provider_ids")]
    pub provider_ids: Option<BTreeMap<String, String>>,
    #[serde(alias = "year")]
    pub year: Option<i32>,
    #[serde(alias = "indexNumber", alias = "index_number")]
    pub index_number: Option<i32>,
    #[serde(alias = "parentIndexNumber", alias = "parent_index_number")]
    pub parent_index_number: Option<i32>,
    #[serde(alias = "premiereDate", alias = "premiere_date")]
    pub premiere_date: Option<String>,
    #[serde(alias = "isAutomated", alias = "is_automated")]
    pub is_automated: Option<bool>,
    #[serde(alias = "enableAdultMetadata", alias = "enable_adult_metadata")]
    pub enable_adult_metadata: Option<bool>,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Eq)]
#[serde(untagged)]
enum FlexibleTextValue {
    Text(String),
    I64(i64),
    U64(u64),
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct RemoteSearchInput {
    kind: RemoteSearchKind,
    item_id: Option<String>,
    search_provider_name: Option<String>,
    providers: Vec<String>,
    include_disabled_providers: bool,
    search_name: Option<String>,
    search_path: Option<String>,
    metadata_language: Option<String>,
    metadata_country_code: Option<String>,
    provider_ids: BTreeMap<String, String>,
    year: Option<i32>,
    index_number: Option<i32>,
    parent_index_number: Option<i32>,
    premiere_date: Option<String>,
    is_automated: bool,
    enable_adult_metadata: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct RemoteSearchImageInput {
    image_url: String,
    provider_name: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct MetadataResetInput {
    item_ids: Vec<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct RemoteSearchApplyInput {
    item_id: String,
    replace_all_images: bool,
    provider_ids: BTreeMap<String, String>,
    search_provider_name: Option<String>,
}

pub async fn external_id_infos(
    State(state): State<AppState>,
    Path(item_id): Path<String>,
    headers: HeaderMap,
    uri: Uri,
) -> Result<Json<Vec<ExternalIdInfoDto>>, AppError> {
    let user = authenticate_request_user(&state, &headers, &uri).await?;
    ensure_lookup_admin(&user)?;
    let _item_id = normalize_required_lookup_text("Id", &item_id, MAX_LOOKUP_TEXT_LEN)?;

    Ok(Json(external_id_info_items()))
}

pub async fn metadata_editor_info(
    State(state): State<AppState>,
    Path(item_id): Path<String>,
    headers: HeaderMap,
    uri: Uri,
) -> Result<Json<MetadataEditorInfoDto>, AppError> {
    let user = authenticate_request_user(&state, &headers, &uri).await?;
    ensure_lookup_admin(&user)?;
    let _item_id = normalize_required_lookup_text("ItemId", &item_id, MAX_LOOKUP_TEXT_LEN)?;

    Ok(Json(metadata_editor_info_dto()))
}

pub async fn remote_search_image(
    State(state): State<AppState>,
    Query(query): Query<RemoteSearchImageQuery>,
    headers: HeaderMap,
    uri: Uri,
) -> Result<Response, AppError> {
    let user = authenticate_request_user(&state, &headers, &uri).await?;
    ensure_lookup_admin(&user)?;
    let input = remote_search_image_input(query)?;
    ensure_public_remote_image_url(&input.image_url)?;

    remote_image_passthrough_response(&input.image_url).await
}

pub async fn reset_metadata(
    State(state): State<AppState>,
    Query(query): Query<MetadataResetQuery>,
    headers: HeaderMap,
    uri: Uri,
) -> Result<StatusCode, AppError> {
    let user = authenticate_request_user(&state, &headers, &uri).await?;
    ensure_lookup_admin(&user)?;
    let input = metadata_reset_input(query)?;
    let Some(database) = state.database() else {
        return Err(AppError::internal("database is not configured"));
    };

    let media = MediaRepository::new(database.clone());
    let admin = AdminRepository::new(database.clone());
    let mut reset_count = 0usize;
    for item_id in &input.item_ids {
        let reset = media
            .reset_item_metadata(item_id)
            .await
            .map_err(|err| AppError::internal(format!("failed to reset metadata: {err}")))?;
        if !reset {
            continue;
        }
        reset_count += 1;
        admin
            .queue_metadata_refresh_for_item(
                item_id,
                QueueMetadataRefreshInput {
                    requested_by_user_id: user.id,
                    reason: Some("emby metadata reset".to_owned()),
                },
            )
            .await
            .map_err(|err| {
                AppError::internal(format!("failed to queue metadata refresh: {err}"))
            })?;
    }

    if reset_count == 0 {
        return Err(AppError::not_found("no matching items"));
    }

    Ok(StatusCode::NO_CONTENT)
}

pub async fn apply_remote_search(
    State(state): State<AppState>,
    Path(item_id): Path<String>,
    Query(query): Query<RemoteSearchApplyQuery>,
    headers: HeaderMap,
    uri: Uri,
    body: Bytes,
) -> Result<StatusCode, AppError> {
    let user = authenticate_request_user(&state, &headers, &uri).await?;
    ensure_lookup_admin(&user)?;
    ensure_lookup_body_size(&body)?;
    let result: RemoteSearchResultDto = parse_emby_body(&headers, &body)?;
    let input = remote_search_apply_input(&item_id, query, result)?;
    let Some(database) = state.database() else {
        return Err(AppError::internal("database is not configured"));
    };

    let external_ids = storage_external_ids(&input.provider_ids);
    if external_ids.is_empty() {
        return Err(AppError::unprocessable("ProviderIds is required"));
    }

    let applied = MediaRepository::new(database.clone())
        .apply_item_external_ids(&input.item_id, &external_ids)
        .await
        .map_err(|err| {
            if is_unique_violation(&err) {
                AppError::conflict("provider id is already linked to another item")
            } else {
                AppError::internal(format!("failed to apply remote search: {err}"))
            }
        })?;
    if !applied {
        return Err(AppError::not_found("item not found"));
    }

    AdminRepository::new(database.clone())
        .queue_metadata_refresh_for_item(
            &input.item_id,
            QueueMetadataRefreshInput {
                requested_by_user_id: user.id,
                reason: Some("emby remote search apply".to_owned()),
            },
        )
        .await
        .map_err(|err| AppError::internal(format!("failed to queue metadata refresh: {err}")))?;

    Ok(StatusCode::NO_CONTENT)
}

/// Emby ProviderIds 字典键 → media_external_ids.provider 存储键。
/// 只放行已知 provider，防任意键膨胀 provider 命名空间。
fn storage_external_ids(provider_ids: &BTreeMap<String, String>) -> Vec<(String, String)> {
    provider_ids
        .iter()
        .filter_map(|(key, value)| {
            let provider = match key.trim().to_ascii_lowercase().as_str() {
                "tmdb" | "themoviedb" => "tmdb",
                "imdb" => "imdb",
                "tvdb" | "thetvdb" => "tvdb",
                _ => return None,
            };
            let value = value.trim();
            (!value.is_empty()).then(|| (provider.to_owned(), value.to_owned()))
        })
        .collect()
}

fn is_unique_violation(error: &sqlx::Error) -> bool {
    error
        .as_database_error()
        .and_then(|db| db.code())
        .is_some_and(|code| code == "23505")
}

pub async fn remote_search_book(
    State(state): State<AppState>,
    headers: HeaderMap,
    uri: Uri,
    body: Bytes,
) -> Result<Json<Vec<RemoteSearchResultDto>>, AppError> {
    remote_search(state, RemoteSearchKind::Book, headers, uri, body).await
}

pub async fn remote_search_box_set(
    State(state): State<AppState>,
    headers: HeaderMap,
    uri: Uri,
    body: Bytes,
) -> Result<Json<Vec<RemoteSearchResultDto>>, AppError> {
    remote_search(state, RemoteSearchKind::BoxSet, headers, uri, body).await
}

pub async fn remote_search_game(
    State(state): State<AppState>,
    headers: HeaderMap,
    uri: Uri,
    body: Bytes,
) -> Result<Json<Vec<RemoteSearchResultDto>>, AppError> {
    remote_search(state, RemoteSearchKind::Game, headers, uri, body).await
}

pub async fn remote_search_movie(
    State(state): State<AppState>,
    headers: HeaderMap,
    uri: Uri,
    body: Bytes,
) -> Result<Json<Vec<RemoteSearchResultDto>>, AppError> {
    remote_search(state, RemoteSearchKind::Movie, headers, uri, body).await
}

pub async fn remote_search_music_album(
    State(state): State<AppState>,
    headers: HeaderMap,
    uri: Uri,
    body: Bytes,
) -> Result<Json<Vec<RemoteSearchResultDto>>, AppError> {
    remote_search(state, RemoteSearchKind::MusicAlbum, headers, uri, body).await
}

pub async fn remote_search_music_artist(
    State(state): State<AppState>,
    headers: HeaderMap,
    uri: Uri,
    body: Bytes,
) -> Result<Json<Vec<RemoteSearchResultDto>>, AppError> {
    remote_search(state, RemoteSearchKind::MusicArtist, headers, uri, body).await
}

pub async fn remote_search_music_video(
    State(state): State<AppState>,
    headers: HeaderMap,
    uri: Uri,
    body: Bytes,
) -> Result<Json<Vec<RemoteSearchResultDto>>, AppError> {
    remote_search(state, RemoteSearchKind::MusicVideo, headers, uri, body).await
}

pub async fn remote_search_person(
    State(state): State<AppState>,
    headers: HeaderMap,
    uri: Uri,
    body: Bytes,
) -> Result<Json<Vec<RemoteSearchResultDto>>, AppError> {
    remote_search(state, RemoteSearchKind::Person, headers, uri, body).await
}

pub async fn remote_search_series(
    State(state): State<AppState>,
    headers: HeaderMap,
    uri: Uri,
    body: Bytes,
) -> Result<Json<Vec<RemoteSearchResultDto>>, AppError> {
    remote_search(state, RemoteSearchKind::Series, headers, uri, body).await
}

pub async fn remote_search_trailer(
    State(state): State<AppState>,
    headers: HeaderMap,
    uri: Uri,
    body: Bytes,
) -> Result<Json<Vec<RemoteSearchResultDto>>, AppError> {
    remote_search(state, RemoteSearchKind::Trailer, headers, uri, body).await
}

async fn remote_search(
    state: AppState,
    kind: RemoteSearchKind,
    headers: HeaderMap,
    uri: Uri,
    body: Bytes,
) -> Result<Json<Vec<RemoteSearchResultDto>>, AppError> {
    let _user = authenticate_request_user(&state, &headers, &uri).await?;
    ensure_lookup_body_size(&body)?;
    let query: RemoteSearchQueryDto = parse_emby_body(&headers, &body)?;
    let input = remote_search_input(kind, query)?;
    let Some(database) = state.database() else {
        return Err(AppError::internal("database is not configured"));
    };

    // Providers 过滤：显式指定 provider 且不含 TMDB 时不做任何联网（当前只实现 TMDB）。
    if !input.providers.is_empty()
        && !input.providers.iter().any(|provider| {
            matches!(
                provider.trim().to_ascii_lowercase().as_str(),
                "tmdb" | "themoviedb"
            )
        })
    {
        return Ok(Json(Vec::new()));
    }

    let Some(item_type) = remote_search_kind_item_type(input.kind) else {
        return Ok(Json(Vec::new()));
    };
    let tmdb_id = input
        .provider_ids
        .iter()
        .find(|(key, _)| {
            matches!(
                key.trim().to_ascii_lowercase().as_str(),
                "tmdb" | "themoviedb"
            )
        })
        .map(|(_, value)| value.clone());

    let config = state.config();
    let candidates = RemoteSearchService::new(
        database.clone(),
        config.metadata.clone(),
        config.proxy.clone(),
        config.secrets.clone(),
    )
    .search(&RemoteSearchRequest {
        item_type: item_type.to_owned(),
        name: input.search_name.clone(),
        year: input.year,
        language: input.metadata_language.clone(),
        country: input.metadata_country_code.clone(),
        tmdb_id,
    })
    .await
    .map_err(|err| AppError::internal(format!("remote search failed: {err}")))?;

    Ok(Json(
        candidates
            .into_iter()
            .map(remote_search_candidate_to_dto)
            .collect(),
    ))
}

/// RemoteSearch 路径类型 → FBZ 内部条目类型；当前未接 provider 的类型返回 None（空结果）。
fn remote_search_kind_item_type(kind: RemoteSearchKind) -> Option<&'static str> {
    match kind {
        RemoteSearchKind::Movie => Some("movie"),
        RemoteSearchKind::Series => Some("series"),
        RemoteSearchKind::Person => Some("person"),
        RemoteSearchKind::BoxSet => Some("collection"),
        RemoteSearchKind::Trailer | RemoteSearchKind::MusicVideo => Some("movie"),
        RemoteSearchKind::Book
        | RemoteSearchKind::Game
        | RemoteSearchKind::MusicAlbum
        | RemoteSearchKind::MusicArtist => None,
    }
}

fn remote_search_candidate_to_dto(candidate: RemoteSearchCandidate) -> RemoteSearchResultDto {
    let mut provider_ids = BTreeMap::new();
    provider_ids.insert(candidate.provider_key.clone(), candidate.external_id.clone());

    RemoteSearchResultDto {
        name: Some(candidate.name),
        original_title: candidate.original_title,
        provider_ids: Some(provider_ids),
        production_year: candidate.production_year,
        premiere_date: candidate.premiere_date,
        image_url: candidate.image_url,
        search_provider_name: Some("TheMovieDb".to_owned()),
        overview: candidate.overview,
        ..RemoteSearchResultDto::default()
    }
}

fn external_id_info_items() -> Vec<ExternalIdInfoDto> {
    vec![
        ExternalIdInfoDto {
            name: "TheMovieDb".to_owned(),
            key: "Tmdb".to_owned(),
            website: Some("https://www.themoviedb.org/".to_owned()),
            url_format_string: Some("https://www.themoviedb.org/movie/{0}".to_owned()),
            is_supported_as_identifier: true,
        },
        ExternalIdInfoDto {
            name: "TheTVDB".to_owned(),
            key: "Tvdb".to_owned(),
            website: Some("https://thetvdb.com/".to_owned()),
            url_format_string: Some("https://thetvdb.com/dereferrer/series/{0}".to_owned()),
            is_supported_as_identifier: true,
        },
        ExternalIdInfoDto {
            name: "IMDb".to_owned(),
            key: "Imdb".to_owned(),
            website: Some("https://www.imdb.com/".to_owned()),
            url_format_string: Some("https://www.imdb.com/title/{0}".to_owned()),
            is_supported_as_identifier: true,
        },
    ]
}

fn metadata_editor_info_dto() -> MetadataEditorInfoDto {
    MetadataEditorInfoDto {
        parental_rating_options: vec![
            MetadataEditorParentalRatingDto {
                name: "G".to_owned(),
                value: 1,
            },
            MetadataEditorParentalRatingDto {
                name: "PG".to_owned(),
                value: 5,
            },
            MetadataEditorParentalRatingDto {
                name: "PG-13".to_owned(),
                value: 8,
            },
            MetadataEditorParentalRatingDto {
                name: "R".to_owned(),
                value: 9,
            },
            MetadataEditorParentalRatingDto {
                name: "NC-17".to_owned(),
                value: 10,
            },
        ],
        countries: vec![
            MetadataEditorCountryInfoDto {
                name: "US".to_owned(),
                display_name: "United States".to_owned(),
                two_letter_iso_region_name: "US".to_owned(),
                three_letter_iso_region_name: "USA".to_owned(),
            },
            MetadataEditorCountryInfoDto {
                name: "CN".to_owned(),
                display_name: "China".to_owned(),
                two_letter_iso_region_name: "CN".to_owned(),
                three_letter_iso_region_name: "CHN".to_owned(),
            },
        ],
        cultures: vec![
            MetadataEditorCultureDto {
                name: "en-US".to_owned(),
                display_name: "English (United States)".to_owned(),
                two_letter_iso_language_name: "en".to_owned(),
                three_letter_iso_language_name: "eng".to_owned(),
                three_letter_iso_language_names: vec!["eng".to_owned()],
            },
            MetadataEditorCultureDto {
                name: "zh-CN".to_owned(),
                display_name: "Chinese (Simplified, China)".to_owned(),
                two_letter_iso_language_name: "zh".to_owned(),
                three_letter_iso_language_name: "zho".to_owned(),
                three_letter_iso_language_names: vec!["zho".to_owned(), "chi".to_owned()],
            },
        ],
        external_id_infos: external_id_info_items()
            .into_iter()
            .map(|info| MetadataEditorExternalIdInfoDto {
                name: info.name,
                key: info.key,
                url_format_string: info.url_format_string,
            })
            .collect(),
    }
}

fn remote_search_input(
    kind: RemoteSearchKind,
    query: RemoteSearchQueryDto,
) -> Result<RemoteSearchInput, AppError> {
    let search_info = query.search_info.unwrap_or_default();
    let provider_ids = normalize_provider_id_dictionary(search_info.provider_ids)?;

    Ok(RemoteSearchInput {
        kind,
        item_id: normalize_optional_flexible_text(query.item_id, "ItemId", MAX_LOOKUP_TEXT_LEN)?,
        search_provider_name: normalize_optional_lookup_text(
            "SearchProviderName",
            query.search_provider_name.as_deref(),
            MAX_LOOKUP_PROVIDER_LEN,
        )?,
        providers: normalize_lookup_text_list(
            "Providers",
            query.providers.unwrap_or_default(),
            MAX_LOOKUP_PROVIDER_LEN,
            16,
        )?,
        include_disabled_providers: query.include_disabled_providers.unwrap_or(false),
        search_name: normalize_optional_lookup_text(
            "SearchInfo.Name",
            search_info.name.as_deref(),
            MAX_LOOKUP_TEXT_LEN,
        )?,
        search_path: normalize_optional_lookup_text(
            "SearchInfo.Path",
            search_info.path.as_deref(),
            MAX_LOOKUP_URL_LEN,
        )?,
        metadata_language: normalize_optional_lookup_text(
            "SearchInfo.MetadataLanguage",
            search_info.metadata_language.as_deref(),
            32,
        )?,
        metadata_country_code: normalize_optional_lookup_text(
            "SearchInfo.MetadataCountryCode",
            search_info.metadata_country_code.as_deref(),
            16,
        )?,
        provider_ids,
        year: search_info.year,
        index_number: search_info.index_number,
        parent_index_number: search_info.parent_index_number,
        premiere_date: normalize_optional_lookup_text(
            "SearchInfo.PremiereDate",
            search_info.premiere_date.as_deref(),
            64,
        )?,
        is_automated: search_info.is_automated.unwrap_or(false),
        enable_adult_metadata: search_info.enable_adult_metadata.unwrap_or(false),
    })
}

fn remote_search_image_input(
    query: RemoteSearchImageQuery,
) -> Result<RemoteSearchImageInput, AppError> {
    Ok(RemoteSearchImageInput {
        image_url: normalize_required_remote_image_url(query.image_url.as_deref())?,
        provider_name: normalize_required_lookup_text(
            "ProviderName",
            query.provider_name.as_deref().unwrap_or_default(),
            MAX_LOOKUP_PROVIDER_LEN,
        )?,
    })
}

fn metadata_reset_input(query: MetadataResetQuery) -> Result<MetadataResetInput, AppError> {
    let item_ids = query
        .item_ids
        .as_deref()
        .ok_or_else(|| AppError::unprocessable("ItemIds is required"))?
        .split(',')
        .map(|value| normalize_required_lookup_text("ItemIds", value, MAX_LOOKUP_TEXT_LEN))
        .collect::<Result<Vec<_>, _>>()?;

    if item_ids.is_empty() || item_ids.len() > MAX_LOOKUP_ITEM_IDS {
        return Err(AppError::unprocessable("ItemIds is invalid"));
    }

    Ok(MetadataResetInput { item_ids })
}

fn remote_search_apply_input(
    item_id: &str,
    query: RemoteSearchApplyQuery,
    result: RemoteSearchResultDto,
) -> Result<RemoteSearchApplyInput, AppError> {
    Ok(RemoteSearchApplyInput {
        item_id: normalize_required_lookup_text("Id", item_id, MAX_LOOKUP_TEXT_LEN)?,
        replace_all_images: query.replace_all_images.unwrap_or(false),
        provider_ids: normalize_provider_id_dictionary(result.provider_ids)?,
        search_provider_name: normalize_optional_lookup_text(
            "SearchProviderName",
            result.search_provider_name.as_deref(),
            MAX_LOOKUP_PROVIDER_LEN,
        )?,
    })
}

fn normalize_provider_id_dictionary(
    values: Option<BTreeMap<String, String>>,
) -> Result<BTreeMap<String, String>, AppError> {
    let mut normalized = BTreeMap::new();
    for (key, value) in values.unwrap_or_default() {
        let key = normalize_required_lookup_text("ProviderIds key", &key, 64)?;
        let value = normalize_required_lookup_text("ProviderIds value", &value, 256)?;
        normalized.insert(key, value);
    }
    if normalized.len() > 32 {
        return Err(AppError::unprocessable("ProviderIds is invalid"));
    }

    Ok(normalized)
}

fn normalize_lookup_text_list(
    field: &'static str,
    values: Vec<String>,
    max_len: usize,
    max_items: usize,
) -> Result<Vec<String>, AppError> {
    if values.len() > max_items {
        return Err(AppError::unprocessable(format!("{field} is invalid")));
    }

    values
        .into_iter()
        .filter_map(
            |value| match normalize_optional_lookup_text(field, Some(&value), max_len) {
                Ok(Some(value)) => Some(Ok(value)),
                Ok(None) => None,
                Err(err) => Some(Err(err)),
            },
        )
        .collect()
}

fn normalize_optional_flexible_text(
    value: Option<FlexibleTextValue>,
    field: &'static str,
    max_len: usize,
) -> Result<Option<String>, AppError> {
    let Some(value) = value else {
        return Ok(None);
    };
    let value = match value {
        FlexibleTextValue::Text(value) => value,
        FlexibleTextValue::I64(value) => value.to_string(),
        FlexibleTextValue::U64(value) => value.to_string(),
    };

    normalize_optional_lookup_text(field, Some(&value), max_len)
}

fn normalize_optional_lookup_text(
    field: &'static str,
    value: Option<&str>,
    max_len: usize,
) -> Result<Option<String>, AppError> {
    let Some(value) = value else {
        return Ok(None);
    };
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return Ok(None);
    }
    if trimmed.len() > max_len || trimmed.chars().any(char::is_control) {
        return Err(AppError::unprocessable(format!("{field} is invalid")));
    }

    Ok(Some(trimmed.to_owned()))
}

fn normalize_required_lookup_text(
    field: &'static str,
    value: &str,
    max_len: usize,
) -> Result<String, AppError> {
    let Some(value) = normalize_optional_lookup_text(field, Some(value), max_len)? else {
        return Err(AppError::unprocessable(format!("{field} is required")));
    };
    if value.chars().any(|ch| matches!(ch, '/' | '\\')) {
        return Err(AppError::unprocessable(format!("{field} is invalid")));
    }

    Ok(value)
}

fn normalize_required_remote_image_url(value: Option<&str>) -> Result<String, AppError> {
    let Some(value) = value else {
        return Err(AppError::unprocessable("ImageUrl is required"));
    };
    let value = value.trim();
    if value.is_empty() {
        return Err(AppError::unprocessable("ImageUrl is required"));
    }
    if value.len() > MAX_LOOKUP_URL_LEN
        || value
            .chars()
            .any(|ch| ch.is_control() || ch.is_whitespace())
    {
        return Err(AppError::unprocessable("ImageUrl is invalid"));
    }

    let uri = value
        .parse::<Uri>()
        .map_err(|_| AppError::unprocessable("ImageUrl is invalid"))?;
    if !matches!(uri.scheme_str(), Some("http" | "https")) || uri.host().is_none() {
        return Err(AppError::unprocessable("ImageUrl is invalid"));
    }

    Ok(value.to_owned())
}

fn ensure_lookup_body_size(body: &Bytes) -> Result<(), AppError> {
    if body.len() > MAX_LOOKUP_BODY_BYTES {
        return Err(AppError::unprocessable("request body is too large"));
    }

    Ok(())
}

fn ensure_lookup_admin(user: &AuthenticatedUser) -> Result<(), AppError> {
    if user.can_manage_server() {
        return Ok(());
    }

    Err(AppError::forbidden("server management permission required"))
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    #[test]
    fn external_id_infos_serialize_pascal_case() {
        let value = serde_json::to_value(external_id_info_items().remove(0))
            .expect("external id info should serialize");

        assert_eq!(value["Name"], "TheMovieDb");
        assert_eq!(value["Key"], "Tmdb");
        assert_eq!(value["Website"], "https://www.themoviedb.org/");
        assert_eq!(
            value["UrlFormatString"],
            "https://www.themoviedb.org/movie/{0}"
        );
        assert_eq!(value["IsSupportedAsIdentifier"], true);
    }

    #[test]
    fn remote_search_query_normalizes_official_fields() {
        let query: RemoteSearchQueryDto = serde_json::from_value(json!({
            "SearchInfo": {
                "Name": " A Movie ",
                "MetadataLanguage": " en ",
                "MetadataCountryCode": " US ",
                "ProviderIds": {
                    "Tmdb": " 42 "
                },
                "Year": 2024,
                "IsAutomated": true,
                "EnableAdultMetadata": false
            },
            "ItemId": 42,
            "SearchProviderName": " TheMovieDb ",
            "Providers": [" TheMovieDb "],
            "IncludeDisabledProviders": true
        }))
        .expect("official remote search query should parse");

        let input = remote_search_input(RemoteSearchKind::Movie, query)
            .expect("official remote search query should normalize");

        assert_eq!(input.kind, RemoteSearchKind::Movie);
        assert_eq!(input.item_id.as_deref(), Some("42"));
        assert_eq!(input.search_name.as_deref(), Some("A Movie"));
        assert_eq!(input.metadata_language.as_deref(), Some("en"));
        assert_eq!(input.metadata_country_code.as_deref(), Some("US"));
        assert_eq!(input.search_provider_name.as_deref(), Some("TheMovieDb"));
        assert_eq!(input.providers, ["TheMovieDb"]);
        assert_eq!(
            input.provider_ids.get("Tmdb").map(String::as_str),
            Some("42")
        );
        assert_eq!(input.year, Some(2024));
        assert!(input.include_disabled_providers);
        assert!(input.is_automated);
    }

    #[test]
    fn remote_search_body_accepts_lower_camel_client_fields() {
        let query: RemoteSearchQueryDto = serde_json::from_value(json!({
            "searchInfo": {
                "name": " A Movie ",
                "path": "/media/movie.mkv",
                "metadataLanguage": " en ",
                "metadataCountryCode": " US ",
                "providerIds": {
                    "Tmdb": " 42 "
                },
                "year": 2024,
                "indexNumber": 1,
                "parentIndexNumber": 2,
                "premiereDate": "2024-01-01",
                "isAutomated": true,
                "enableAdultMetadata": false
            },
            "itemId": 42,
            "searchProviderName": " TheMovieDb ",
            "providers": [" TheMovieDb "],
            "includeDisabledProviders": true
        }))
        .expect("lower-camel remote search query should parse");

        let input = remote_search_input(RemoteSearchKind::Movie, query)
            .expect("lower-camel remote search query should normalize");

        assert_eq!(input.kind, RemoteSearchKind::Movie);
        assert_eq!(input.item_id.as_deref(), Some("42"));
        assert_eq!(input.search_name.as_deref(), Some("A Movie"));
        assert_eq!(input.search_path.as_deref(), Some("/media/movie.mkv"));
        assert_eq!(input.metadata_language.as_deref(), Some("en"));
        assert_eq!(input.metadata_country_code.as_deref(), Some("US"));
        assert_eq!(input.search_provider_name.as_deref(), Some("TheMovieDb"));
        assert_eq!(input.providers, ["TheMovieDb"]);
        assert_eq!(
            input.provider_ids.get("Tmdb").map(String::as_str),
            Some("42")
        );
        assert_eq!(input.year, Some(2024));
        assert_eq!(input.index_number, Some(1));
        assert_eq!(input.parent_index_number, Some(2));
        assert_eq!(input.premiere_date.as_deref(), Some("2024-01-01"));
        assert!(input.include_disabled_providers);
        assert!(input.is_automated);
        assert!(!input.enable_adult_metadata);
    }

    #[test]
    fn remote_search_apply_body_accepts_lower_camel_client_fields() {
        let result: RemoteSearchResultDto = serde_json::from_value(json!({
            "name": "A Movie",
            "originalTitle": "Original Movie",
            "providerIds": {
                "Tmdb": "42"
            },
            "productionYear": 2024,
            "indexNumber": 1,
            "parentIndexNumber": 2,
            "premiereDate": "2024-01-01",
            "imageUrl": "https://image.example.test/poster.jpg",
            "searchProviderName": "TheMovieDb",
            "albumArtist": "Artist",
            "artists": ["Artist"]
        }))
        .expect("lower-camel remote search result should parse");
        let input = remote_search_apply_input(
            "item-1",
            RemoteSearchApplyQuery {
                replace_all_images: Some(true),
            },
            result,
        )
        .expect("lower-camel remote search apply should normalize");

        assert_eq!(input.item_id, "item-1");
        assert!(input.replace_all_images);
        assert_eq!(
            input.provider_ids.get("Tmdb").map(String::as_str),
            Some("42")
        );
        assert_eq!(input.search_provider_name.as_deref(), Some("TheMovieDb"));
    }

    #[test]
    fn metadata_editor_info_uses_official_option_shape() {
        let info = metadata_editor_info_dto();
        let value = serde_json::to_value(info).unwrap();

        assert_eq!(value["ParentalRatingOptions"][0]["Name"], "G");
        assert_eq!(value["ParentalRatingOptions"][0]["Value"], 1);
        assert_eq!(value["Countries"][0]["TwoLetterISORegionName"], "US");
        assert_eq!(value["Countries"][0]["ThreeLetterISORegionName"], "USA");
        assert_eq!(value["Cultures"][0]["TwoLetterISOLanguageName"], "en");
        assert_eq!(
            value["Cultures"][0]["ThreeLetterISOLanguageNames"][0],
            "eng"
        );
        assert_eq!(value["ExternalIdInfos"][0]["Key"], "Tmdb");
        assert_eq!(
            value["ExternalIdInfos"][0]["UrlFormatString"],
            "https://www.themoviedb.org/movie/{0}"
        );
    }

    #[test]
    fn metadata_reset_query_splits_item_ids() {
        let input = metadata_reset_input(MetadataResetQuery {
            item_ids: Some(" item-1, item-2 ".to_owned()),
        })
        .expect("item ids should normalize");

        assert_eq!(input.item_ids, ["item-1", "item-2"]);
    }

    #[test]
    fn remote_search_image_rejects_non_http_urls() {
        assert!(
            remote_search_image_input(RemoteSearchImageQuery {
                image_url: Some("file:///tmp/poster.jpg".to_owned()),
                provider_name: Some("TheMovieDb".to_owned()),
            })
            .is_err()
        );
        assert!(
            remote_search_image_input(RemoteSearchImageQuery {
                image_url: Some("https://image.example.test/poster.jpg".to_owned()),
                provider_name: Some("TheMovieDb".to_owned()),
            })
            .is_ok()
        );
    }

    #[test]
    fn remote_search_apply_input_rejects_path_like_ids() {
        let err = remote_search_apply_input(
            "../item",
            RemoteSearchApplyQuery::default(),
            RemoteSearchResultDto::default(),
        )
        .expect_err("path-like ids should be rejected");

        assert_eq!(err.status_code(), StatusCode::UNPROCESSABLE_ENTITY);
    }
}
