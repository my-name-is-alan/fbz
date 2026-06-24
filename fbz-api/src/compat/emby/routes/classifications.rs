use axum::{
    Json,
    extract::{Query, State},
    http::{HeaderMap, Uri},
};
use serde::{Deserialize, Serialize};

use crate::{
    compat::emby::dto::QueryResultDto,
    error::AppError,
    library::repository::{
        ItemsFiltersInput, ItemsFiltersResult, LibraryRepository, OfficialRatingListInput,
        OfficialRatingRecord, SortDirection, TagListInput, TagRecord, TechnicalFacetKind,
        TechnicalFacetListInput, TechnicalFacetRecord, YearListInput, YearRecord,
    },
    state::AppState,
};

use super::{
    access::authenticate_query_user,
    items::{include_item_types_filter, media_type_list_filter, normalized_parent_id},
};

const DEFAULT_CLASSIFICATION_LIMIT: u32 = 100;
const MAX_CLASSIFICATION_LIMIT: u32 = 200;
const ITEMS_FILTERS_LIMIT: i64 = 500;

#[derive(Clone, Debug, Default, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "PascalCase")]
pub struct ClassificationQuery {
    pub user_id: Option<String>,
    pub parent_id: Option<String>,
    pub recursive: Option<bool>,
    pub start_index: Option<u32>,
    pub limit: Option<u32>,
    pub search_term: Option<String>,
    pub sort_order: Option<String>,
    pub name_starts_with: Option<String>,
    pub name_starts_with_or_greater: Option<String>,
    pub fields: Option<String>,
    pub enable_images: Option<bool>,
}

#[derive(Clone, Debug, Default, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "PascalCase")]
pub struct ItemsFiltersQuery {
    pub user_id: Option<String>,
    pub parent_id: Option<String>,
    pub include_item_types: Option<String>,
    pub media_types: Option<String>,
}

#[derive(Clone, Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "PascalCase")]
pub struct TagItemDto {
    pub name: String,
    pub id: String,
}

#[derive(Clone, Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "PascalCase")]
pub struct OfficialRatingItemDto {
    pub name: String,
}

#[derive(Clone, Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "PascalCase")]
pub struct ItemsFiltersDto {
    pub genres: Vec<String>,
    pub tags: Vec<String>,
    pub official_ratings: Vec<String>,
    pub years: Vec<i32>,
}

pub async fn items_filters(
    State(state): State<AppState>,
    Query(query): Query<ItemsFiltersQuery>,
    headers: HeaderMap,
    uri: Uri,
) -> Result<Json<ItemsFiltersDto>, AppError> {
    let user = authenticate_query_user(&state, query.user_id.as_deref(), &headers, &uri).await?;
    let Some(database) = state.database() else {
        return Err(AppError::internal("database is not configured"));
    };

    let repository = LibraryRepository::new(database.clone());

    let result = repository
        .list_user_item_filters(items_filters_input(user.id, query))
        .await
        .map_err(|err| AppError::internal(format!("failed to list item filters: {err}")))?;

    Ok(Json(items_filters_to_dto(result)))
}

fn items_filters_input(user_id: i64, query: ItemsFiltersQuery) -> ItemsFiltersInput {
    ItemsFiltersInput {
        user_id,
        parent_id: normalized_parent_id(query.parent_id),
        recursive: true,
        item_types: include_item_types_filter(query.include_item_types.as_deref()),
        media_types: media_type_list_filter(query.media_types.as_deref()),
        limit: ITEMS_FILTERS_LIMIT,
    }
}

fn items_filters_to_dto(result: ItemsFiltersResult) -> ItemsFiltersDto {
    ItemsFiltersDto {
        genres: result.genres,
        tags: result.tags,
        official_ratings: result.official_ratings,
        years: result.years,
    }
}

pub async fn tags(
    State(state): State<AppState>,
    Query(query): Query<ClassificationQuery>,
    headers: HeaderMap,
    uri: Uri,
) -> Result<Json<QueryResultDto<TagItemDto>>, AppError> {
    let user = authenticate_query_user(&state, query.user_id.as_deref(), &headers, &uri).await?;
    let Some(database) = state.database() else {
        return Err(AppError::internal("database is not configured"));
    };

    let window = ClassificationWindow::from_query(&query);
    let _requested_fields = query.fields.as_deref().unwrap_or_default();
    let _enable_images = query.enable_images.unwrap_or(false);
    let result = LibraryRepository::new(database.clone())
        .list_user_tags(TagListInput {
            user_id: user.id,
            parent_id: normalized_parent_id(query.parent_id),
            start_index: window.start_index,
            limit: window.limit,
            recursive: query.recursive.unwrap_or(true),
            search_term: normalized_text_filter(query.search_term),
            name_starts_with: normalized_text_filter(query.name_starts_with),
            name_starts_with_or_greater: normalized_text_filter(query.name_starts_with_or_greater),
            sort_direction: sort_direction_from_query(query.sort_order.as_deref()),
        })
        .await
        .map_err(|err| AppError::internal(format!("failed to list tags: {err}")))?;

    let items = result.items.into_iter().map(tag_to_dto).collect();
    Ok(Json(QueryResultDto::new(
        items,
        result.total_record_count,
        window.start_index as u32,
    )))
}

pub async fn official_ratings(
    State(state): State<AppState>,
    Query(query): Query<ClassificationQuery>,
    headers: HeaderMap,
    uri: Uri,
) -> Result<Json<QueryResultDto<OfficialRatingItemDto>>, AppError> {
    let user = authenticate_query_user(&state, query.user_id.as_deref(), &headers, &uri).await?;
    let Some(database) = state.database() else {
        return Err(AppError::internal("database is not configured"));
    };

    let window = ClassificationWindow::from_query(&query);
    let _requested_fields = query.fields.as_deref().unwrap_or_default();
    let _enable_images = query.enable_images.unwrap_or(false);
    let result = LibraryRepository::new(database.clone())
        .list_user_official_ratings(OfficialRatingListInput {
            user_id: user.id,
            parent_id: normalized_parent_id(query.parent_id),
            start_index: window.start_index,
            limit: window.limit,
            recursive: query.recursive.unwrap_or(true),
            search_term: normalized_text_filter(query.search_term),
            name_starts_with: normalized_text_filter(query.name_starts_with),
            name_starts_with_or_greater: normalized_text_filter(query.name_starts_with_or_greater),
            sort_direction: sort_direction_from_query(query.sort_order.as_deref()),
        })
        .await
        .map_err(|err| AppError::internal(format!("failed to list official ratings: {err}")))?;

    let items = result
        .items
        .into_iter()
        .map(official_rating_to_dto)
        .collect();
    Ok(Json(QueryResultDto::new(
        items,
        result.total_record_count,
        window.start_index as u32,
    )))
}

pub async fn years(
    State(state): State<AppState>,
    Query(query): Query<ClassificationQuery>,
    headers: HeaderMap,
    uri: Uri,
) -> Result<Json<QueryResultDto<TagItemDto>>, AppError> {
    let user = authenticate_query_user(&state, query.user_id.as_deref(), &headers, &uri).await?;
    let Some(database) = state.database() else {
        return Err(AppError::internal("database is not configured"));
    };

    let window = ClassificationWindow::from_query(&query);
    let _requested_fields = query.fields.as_deref().unwrap_or_default();
    let _enable_images = query.enable_images.unwrap_or(false);
    let result = LibraryRepository::new(database.clone())
        .list_user_years(YearListInput {
            user_id: user.id,
            parent_id: normalized_parent_id(query.parent_id),
            start_index: window.start_index,
            limit: window.limit,
            recursive: query.recursive.unwrap_or(true),
            search_term: normalized_text_filter(query.search_term),
            name_starts_with: normalized_text_filter(query.name_starts_with),
            name_starts_with_or_greater: normalized_text_filter(query.name_starts_with_or_greater),
            sort_direction: sort_direction_from_query(query.sort_order.as_deref()),
        })
        .await
        .map_err(|err| AppError::internal(format!("failed to list years: {err}")))?;

    let items = result.items.into_iter().map(year_to_dto).collect();
    Ok(Json(QueryResultDto::new(
        items,
        result.total_record_count,
        window.start_index as u32,
    )))
}

pub async fn containers(
    State(state): State<AppState>,
    Query(query): Query<ClassificationQuery>,
    headers: HeaderMap,
    uri: Uri,
) -> Result<Json<QueryResultDto<TagItemDto>>, AppError> {
    technical_facets(
        state,
        query,
        headers,
        uri,
        TechnicalFacetKind::Container,
        "containers",
    )
    .await
}

pub async fn audio_codecs(
    State(state): State<AppState>,
    Query(query): Query<ClassificationQuery>,
    headers: HeaderMap,
    uri: Uri,
) -> Result<Json<QueryResultDto<TagItemDto>>, AppError> {
    technical_facets(
        state,
        query,
        headers,
        uri,
        TechnicalFacetKind::AudioCodec,
        "audio codecs",
    )
    .await
}

pub async fn video_codecs(
    State(state): State<AppState>,
    Query(query): Query<ClassificationQuery>,
    headers: HeaderMap,
    uri: Uri,
) -> Result<Json<QueryResultDto<TagItemDto>>, AppError> {
    technical_facets(
        state,
        query,
        headers,
        uri,
        TechnicalFacetKind::VideoCodec,
        "video codecs",
    )
    .await
}

pub async fn subtitle_codecs(
    State(state): State<AppState>,
    Query(query): Query<ClassificationQuery>,
    headers: HeaderMap,
    uri: Uri,
) -> Result<Json<QueryResultDto<TagItemDto>>, AppError> {
    technical_facets(
        state,
        query,
        headers,
        uri,
        TechnicalFacetKind::SubtitleCodec,
        "subtitle codecs",
    )
    .await
}

pub async fn stream_languages(
    State(state): State<AppState>,
    Query(query): Query<ClassificationQuery>,
    headers: HeaderMap,
    uri: Uri,
) -> Result<Json<QueryResultDto<TagItemDto>>, AppError> {
    technical_facets(
        state,
        query,
        headers,
        uri,
        TechnicalFacetKind::StreamLanguage,
        "stream languages",
    )
    .await
}

async fn technical_facets(
    state: AppState,
    query: ClassificationQuery,
    headers: HeaderMap,
    uri: Uri,
    kind: TechnicalFacetKind,
    label: &'static str,
) -> Result<Json<QueryResultDto<TagItemDto>>, AppError> {
    let user = authenticate_query_user(&state, query.user_id.as_deref(), &headers, &uri).await?;
    let Some(database) = state.database() else {
        return Err(AppError::internal("database is not configured"));
    };

    let window = ClassificationWindow::from_query(&query);
    let _requested_fields = query.fields.as_deref().unwrap_or_default();
    let _enable_images = query.enable_images.unwrap_or(false);
    let result = LibraryRepository::new(database.clone())
        .list_user_technical_facets(TechnicalFacetListInput {
            user_id: user.id,
            parent_id: normalized_parent_id(query.parent_id),
            start_index: window.start_index,
            limit: window.limit,
            recursive: query.recursive.unwrap_or(true),
            search_term: normalized_text_filter(query.search_term),
            name_starts_with: normalized_text_filter(query.name_starts_with),
            name_starts_with_or_greater: normalized_text_filter(query.name_starts_with_or_greater),
            sort_direction: sort_direction_from_query(query.sort_order.as_deref()),
            kind,
        })
        .await
        .map_err(|err| AppError::internal(format!("failed to list {label}: {err}")))?;

    let items = result
        .items
        .into_iter()
        .map(technical_facet_to_dto)
        .collect();
    Ok(Json(QueryResultDto::new(
        items,
        result.total_record_count,
        window.start_index as u32,
    )))
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct ClassificationWindow {
    start_index: i64,
    limit: i64,
}

impl ClassificationWindow {
    fn from_query(query: &ClassificationQuery) -> Self {
        Self {
            start_index: i64::from(query.start_index.unwrap_or(0)),
            limit: i64::from(
                query
                    .limit
                    .unwrap_or(DEFAULT_CLASSIFICATION_LIMIT)
                    .clamp(1, MAX_CLASSIFICATION_LIMIT),
            ),
        }
    }
}

fn normalized_text_filter(value: Option<String>) -> Option<String> {
    value.and_then(|value| {
        let trimmed = value.trim();
        (!trimmed.is_empty()).then(|| trimmed.to_owned())
    })
}

fn sort_direction_from_query(value: Option<&str>) -> SortDirection {
    match value.map(str::trim) {
        Some(value) if value.eq_ignore_ascii_case("Descending") => SortDirection::Desc,
        Some(value) if value.eq_ignore_ascii_case("Desc") => SortDirection::Desc,
        _ => SortDirection::Asc,
    }
}

fn tag_to_dto(record: TagRecord) -> TagItemDto {
    TagItemDto {
        name: record.name,
        id: record.id,
    }
}

fn official_rating_to_dto(record: OfficialRatingRecord) -> OfficialRatingItemDto {
    OfficialRatingItemDto { name: record.name }
}

fn year_to_dto(record: YearRecord) -> TagItemDto {
    let year = record.year.to_string();
    TagItemDto {
        name: year.clone(),
        id: year,
    }
}

fn technical_facet_to_dto(record: TechnicalFacetRecord) -> TagItemDto {
    TagItemDto {
        name: record.name,
        id: record.id,
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;
    use crate::library::repository::{
        OfficialRatingRecord, SortDirection, TagRecord, TechnicalFacetRecord, YearRecord,
    };

    #[test]
    fn classification_query_accepts_common_emby_parameters() {
        let query = serde_json::from_value::<ClassificationQuery>(json!({
            "UserId": "user-1",
            "ParentId": "library-1",
            "Recursive": true,
            "StartIndex": 10,
            "Limit": 500,
            "SearchTerm": "hdr",
            "SortOrder": "Descending",
            "NameStartsWith": "H",
            "NameStartsWithOrGreater": "G",
            "Fields": "PrimaryImageAspectRatio",
            "EnableImages": true
        }))
        .unwrap();

        let window = ClassificationWindow::from_query(&query);
        assert_eq!(query.user_id.as_deref(), Some("user-1"));
        assert_eq!(query.parent_id.as_deref(), Some("library-1"));
        assert_eq!(query.recursive, Some(true));
        assert_eq!(window.start_index, 10);
        assert_eq!(window.limit, i64::from(MAX_CLASSIFICATION_LIMIT));
        assert_eq!(query.search_term.as_deref(), Some("hdr"));
        assert_eq!(
            sort_direction_from_query(query.sort_order.as_deref()),
            SortDirection::Desc
        );
    }

    #[test]
    fn items_filters_query_accepts_official_filter_context_parameters() {
        let query = serde_json::from_value::<ItemsFiltersQuery>(json!({
            "UserId": "user-1",
            "ParentId": "library-1",
            "IncludeItemTypes": "Movie,Series",
            "MediaTypes": "Video"
        }))
        .unwrap();

        assert_eq!(query.user_id.as_deref(), Some("user-1"));
        assert_eq!(query.parent_id.as_deref(), Some("library-1"));
        assert_eq!(query.include_item_types.as_deref(), Some("Movie,Series"));
        assert_eq!(query.media_types.as_deref(), Some("Video"));
    }

    #[test]
    fn items_filters_response_uses_legacy_shape() {
        let filters = ItemsFiltersDto {
            genres: vec!["Action".to_owned()],
            tags: vec!["HDR".to_owned()],
            official_ratings: vec!["PG-13".to_owned()],
            years: vec![2024],
        };

        assert_eq!(
            serde_json::to_value(filters).unwrap(),
            json!({
                "Genres": ["Action"],
                "Tags": ["HDR"],
                "OfficialRatings": ["PG-13"],
                "Years": [2024]
            })
        );
    }

    #[test]
    fn items_filters_query_maps_context_fields_to_repository_input() {
        let query = ItemsFiltersQuery {
            parent_id: Some("aaaaaaaa-0000-0000-0000-000000000001".to_owned()),
            include_item_types: Some("Movie,Series,Audio".to_owned()),
            media_types: Some("Video".to_owned()),
            ..ItemsFiltersQuery::default()
        };

        let input = items_filters_input(42, query);

        assert_eq!(
            input.parent_id.as_deref(),
            Some("aaaaaaaa-0000-0000-0000-000000000001")
        );
        assert!(input.recursive);
        assert_eq!(input.limit, ITEMS_FILTERS_LIMIT);
        assert!(input.item_types.enabled);
        assert_eq!(
            input.item_types.item_types,
            vec!["movie".to_owned(), "series".to_owned(), "track".to_owned()]
        );
        assert!(input.media_types.enabled);
        assert_eq!(input.media_types.values, vec!["video".to_owned()]);
    }

    #[test]
    fn classification_mapping_uses_official_response_shapes() {
        let tag = tag_to_dto(TagRecord {
            id: "42".to_owned(),
            name: "HDR".to_owned(),
            total_record_count: 1,
        });
        let rating = official_rating_to_dto(OfficialRatingRecord {
            name: "PG-13".to_owned(),
            total_record_count: 1,
        });
        let year = year_to_dto(YearRecord {
            year: 2024,
            total_record_count: 1,
        });
        let technical_facet = technical_facet_to_dto(TechnicalFacetRecord {
            id: "mkv".to_owned(),
            name: "mkv".to_owned(),
            total_record_count: 1,
        });

        assert_eq!(tag.id, "42");
        assert_eq!(tag.name, "HDR");
        assert_eq!(rating.name, "PG-13");
        assert_eq!(year.id, "2024");
        assert_eq!(year.name, "2024");
        assert_eq!(technical_facet.id, "mkv");
        assert_eq!(technical_facet.name, "mkv");
    }
}
