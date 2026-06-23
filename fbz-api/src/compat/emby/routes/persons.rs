use axum::{
    Json,
    extract::{Path, Query, State},
    http::{HeaderMap, Uri},
};
use serde::Deserialize;

use crate::{
    compat::emby::dto::{BaseItemDto, BaseItemSource, QueryResultDto},
    error::AppError,
    library::repository::{
        LibraryRepository, PersonListInput, PersonRecord, PersonRoleFilter, SortDirection,
    },
    state::AppState,
};

use super::{access::authenticate_query_user, items::normalized_parent_id};

const DEFAULT_PERSONS_LIMIT: u32 = 100;
const MAX_PERSONS_LIMIT: u32 = 200;

#[derive(Clone, Debug, Default, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "PascalCase")]
pub struct PersonsQuery {
    pub user_id: Option<String>,
    pub parent_id: Option<String>,
    pub recursive: Option<bool>,
    pub start_index: Option<u32>,
    pub limit: Option<u32>,
    pub search_term: Option<String>,
    pub sort_order: Option<String>,
    pub person_types: Option<String>,
    pub name_starts_with: Option<String>,
    pub name_starts_with_or_greater: Option<String>,
    pub fields: Option<String>,
    pub enable_images: Option<bool>,
}

#[derive(Clone, Debug, Default, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "PascalCase")]
pub struct PersonByNameQuery {
    pub user_id: Option<String>,
}

pub async fn persons(
    State(state): State<AppState>,
    Query(query): Query<PersonsQuery>,
    headers: HeaderMap,
    uri: Uri,
) -> Result<Json<QueryResultDto<BaseItemDto>>, AppError> {
    let user = authenticate_query_user(&state, query.user_id.as_deref(), &headers, &uri).await?;
    let Some(database) = state.database() else {
        return Err(AppError::internal("database is not configured"));
    };

    let window = PersonWindow::from_query(&query);
    let _requested_fields = query.fields.as_deref().unwrap_or_default();
    let _enable_images = query.enable_images.unwrap_or(false);
    let result = LibraryRepository::new(database.clone())
        .list_user_persons(PersonListInput {
            user_id: user.id,
            parent_id: normalized_parent_id(query.parent_id),
            start_index: window.start_index,
            limit: window.limit,
            recursive: query.recursive.unwrap_or(true),
            role_filter: role_filter_from_query(query.person_types.as_deref()),
            search_term: normalized_text_filter(query.search_term),
            name_starts_with: normalized_text_filter(query.name_starts_with),
            name_starts_with_or_greater: normalized_text_filter(query.name_starts_with_or_greater),
            sort_direction: sort_direction_from_query(query.sort_order.as_deref()),
        })
        .await
        .map_err(|err| AppError::internal(format!("failed to list persons: {err}")))?;

    let items = result.items.into_iter().map(person_to_base_item).collect();
    Ok(Json(QueryResultDto::new(
        items,
        result.total_record_count,
        window.start_index as u32,
    )))
}

pub async fn person_by_name(
    State(state): State<AppState>,
    Path(name): Path<String>,
    Query(query): Query<PersonByNameQuery>,
    headers: HeaderMap,
    uri: Uri,
) -> Result<Json<BaseItemDto>, AppError> {
    let user = authenticate_query_user(&state, query.user_id.as_deref(), &headers, &uri).await?;
    let Some(database) = state.database() else {
        return Err(AppError::internal("database is not configured"));
    };

    let Some(record) = LibraryRepository::new(database.clone())
        .find_user_person_by_name(user.id, &name)
        .await
        .map_err(|err| AppError::internal(format!("failed to get person: {err}")))?
    else {
        return Err(AppError::not_found("person not found"));
    };

    Ok(Json(person_to_base_item(record)))
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct PersonWindow {
    start_index: i64,
    limit: i64,
}

impl PersonWindow {
    fn from_query(query: &PersonsQuery) -> Self {
        Self {
            start_index: i64::from(query.start_index.unwrap_or(0)),
            limit: i64::from(
                query
                    .limit
                    .unwrap_or(DEFAULT_PERSONS_LIMIT)
                    .clamp(1, MAX_PERSONS_LIMIT),
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

fn role_filter_from_query(value: Option<&str>) -> PersonRoleFilter {
    let Some(value) = value else {
        return PersonRoleFilter::default();
    };

    let mut saw_non_empty_type = false;
    let role_types = value
        .split(',')
        .filter_map(|value| {
            saw_non_empty_type |= !value.trim().is_empty();
            emby_person_type_to_role(value)
        })
        .collect();
    if !saw_non_empty_type {
        return PersonRoleFilter::default();
    }

    PersonRoleFilter::enabled(role_types)
}

fn emby_person_type_to_role(value: &str) -> Option<String> {
    let normalized = value
        .trim()
        .chars()
        .filter(|character| !matches!(character, ' ' | '_' | '-'))
        .flat_map(char::to_lowercase)
        .collect::<String>();

    match normalized.as_str() {
        "actor" => Some("actor".to_owned()),
        "director" => Some("director".to_owned()),
        "writer" => Some("writer".to_owned()),
        "producer" => Some("producer".to_owned()),
        "composer" => Some("composer".to_owned()),
        "artist" => Some("artist".to_owned()),
        "gueststar" => Some("guest_star".to_owned()),
        _ => None,
    }
}

fn person_to_base_item(record: PersonRecord) -> BaseItemDto {
    BaseItemDto::from(BaseItemSource {
        id: record.id,
        name: record.name,
        item_type: "Person".to_owned(),
        media_type: None,
        parent_id: None,
        is_folder: false,
        run_time_ticks: None,
        production_year: None,
    })
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    #[test]
    fn persons_query_accepts_common_emby_parameters() {
        let query = serde_json::from_value::<PersonsQuery>(json!({
            "UserId": "user-1",
            "ParentId": "library-1",
            "Recursive": true,
            "StartIndex": 10,
            "Limit": 500,
            "SearchTerm": "tom",
            "SortOrder": "Descending",
            "PersonTypes": "Actor,Director,Guest Star",
            "NameStartsWith": "T",
            "NameStartsWithOrGreater": "M",
            "Fields": "PrimaryImageAspectRatio",
            "EnableImages": true
        }))
        .unwrap();

        let window = PersonWindow::from_query(&query);
        let role_filter = role_filter_from_query(query.person_types.as_deref());
        assert_eq!(query.user_id.as_deref(), Some("user-1"));
        assert_eq!(query.parent_id.as_deref(), Some("library-1"));
        assert_eq!(query.recursive, Some(true));
        assert_eq!(window.start_index, 10);
        assert_eq!(window.limit, i64::from(MAX_PERSONS_LIMIT));
        assert_eq!(query.search_term.as_deref(), Some("tom"));
        assert_eq!(query.name_starts_with.as_deref(), Some("T"));
        assert_eq!(
            sort_direction_from_query(query.sort_order.as_deref()),
            SortDirection::Desc
        );
        assert!(role_filter.enabled);
        assert_eq!(role_filter.role_types, ["actor", "director", "guest_star"]);
    }

    #[test]
    fn unknown_person_type_becomes_empty_enabled_filter() {
        let role_filter = role_filter_from_query(Some("Conductor"));

        assert!(role_filter.enabled);
        assert!(role_filter.role_types.is_empty());
    }

    #[test]
    fn empty_person_type_does_not_enable_filter() {
        let role_filter = role_filter_from_query(Some(" , "));

        assert!(!role_filter.enabled);
        assert!(role_filter.role_types.is_empty());
    }

    #[test]
    fn person_mapping_uses_person_shape() {
        let item = person_to_base_item(PersonRecord {
            id: "person-1".to_owned(),
            name: "Person".to_owned(),
            total_record_count: 1,
        });

        assert_eq!(item.item_type, "Person");
        assert_eq!(item.media_type, None);
        assert!(!item.is_folder);
    }
}
