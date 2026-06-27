use axum::{
    Json,
    extract::{Path, Query, State},
    http::{HeaderMap, StatusCode, Uri},
};
use serde::{Deserialize, Serialize};

use crate::{error::AppError, state::AppState};

use super::access::authenticate_request_user;

const MAX_COLLECTION_IDS: usize = 256;
const MAX_COLLECTION_ID_LEN: usize = 128;
const MAX_COLLECTION_NAME_LEN: usize = 256;

#[derive(Clone, Debug, Default, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "PascalCase")]
pub struct CreateCollectionQuery {
    #[serde(alias = "isLocked", alias = "is_locked")]
    pub is_locked: Option<bool>,
    #[serde(alias = "name")]
    pub name: Option<String>,
    #[serde(alias = "parentId", alias = "parent_id")]
    pub parent_id: Option<String>,
    #[serde(alias = "ids")]
    pub ids: Option<String>,
}

#[derive(Clone, Debug, Default, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "PascalCase")]
pub struct CollectionItemsQuery {
    #[serde(alias = "ids")]
    pub ids: Option<String>,
}

#[derive(Clone, Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "PascalCase")]
pub struct CollectionCreationResultDto {
    pub id: String,
    pub name: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct CreateCollectionInput {
    name: String,
    parent_id: Option<String>,
    ids: Vec<String>,
    is_locked: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct CollectionItemsInput {
    collection_id: String,
    ids: Vec<String>,
}

pub async fn create_collection(
    State(state): State<AppState>,
    Query(query): Query<CreateCollectionQuery>,
    headers: HeaderMap,
    uri: Uri,
) -> Result<Json<CollectionCreationResultDto>, AppError> {
    authenticate_request_user(&state, &headers, &uri).await?;
    let input = create_collection_input(query)?;
    let _ = (input.name, input.parent_id, input.ids, input.is_locked);

    Err(collection_write_disabled_error())
}

pub async fn add_collection_items(
    State(state): State<AppState>,
    Path(collection_id): Path<String>,
    Query(query): Query<CollectionItemsQuery>,
    headers: HeaderMap,
    uri: Uri,
) -> Result<StatusCode, AppError> {
    authenticate_request_user(&state, &headers, &uri).await?;
    let input = collection_items_input(&collection_id, query.ids.as_deref())?;
    let _ = (input.collection_id, input.ids);

    Err(collection_write_disabled_error())
}

pub async fn remove_collection_items(
    State(state): State<AppState>,
    Path(collection_id): Path<String>,
    Query(query): Query<CollectionItemsQuery>,
    headers: HeaderMap,
    uri: Uri,
) -> Result<StatusCode, AppError> {
    authenticate_request_user(&state, &headers, &uri).await?;
    let input = collection_items_input(&collection_id, query.ids.as_deref())?;
    let _ = (input.collection_id, input.ids);

    Err(collection_write_disabled_error())
}

fn create_collection_input(
    query: CreateCollectionQuery,
) -> Result<CreateCollectionInput, AppError> {
    Ok(CreateCollectionInput {
        name: normalize_required_collection_name(query.name.as_deref())?,
        parent_id: normalize_optional_collection_id(query.parent_id.as_deref(), "ParentId")?,
        ids: normalize_collection_ids(query.ids.as_deref(), false)?,
        is_locked: query.is_locked.unwrap_or(false),
    })
}

fn collection_items_input(
    collection_id: &str,
    ids: Option<&str>,
) -> Result<CollectionItemsInput, AppError> {
    Ok(CollectionItemsInput {
        collection_id: normalize_required_collection_id(collection_id, "Id")?,
        ids: normalize_collection_ids(ids, true)?,
    })
}

fn normalize_collection_ids(value: Option<&str>, required: bool) -> Result<Vec<String>, AppError> {
    let raw = value.unwrap_or_default();
    let mut ids = Vec::new();

    for part in raw.split(',') {
        let value = part.trim();
        if value.is_empty() {
            continue;
        }
        let value = normalize_required_collection_id(value, "Ids")?;
        if ids.iter().any(|existing| existing == &value) {
            continue;
        }
        ids.push(value);
        if ids.len() > MAX_COLLECTION_IDS {
            return Err(AppError::unprocessable("too many collection item ids"));
        }
    }

    if required && ids.is_empty() {
        return Err(AppError::unprocessable("Ids are required"));
    }

    Ok(ids)
}

fn normalize_required_collection_name(value: Option<&str>) -> Result<String, AppError> {
    let Some(value) = value.map(str::trim).filter(|value| !value.is_empty()) else {
        return Err(AppError::unprocessable("Name is required"));
    };
    if value.len() > MAX_COLLECTION_NAME_LEN || value.chars().any(char::is_control) {
        return Err(AppError::unprocessable("Name is invalid"));
    }

    Ok(value.to_owned())
}

fn normalize_optional_collection_id(
    value: Option<&str>,
    field: &'static str,
) -> Result<Option<String>, AppError> {
    let Some(value) = value.map(str::trim).filter(|value| !value.is_empty()) else {
        return Ok(None);
    };

    Ok(Some(normalize_required_collection_id(value, field)?))
}

fn normalize_required_collection_id(value: &str, field: &'static str) -> Result<String, AppError> {
    let value = value.trim();
    if value.is_empty() {
        return Err(AppError::unprocessable(format!("{field} is required")));
    }
    if value.len() > MAX_COLLECTION_ID_LEN
        || !value
            .chars()
            .all(|ch| ch.is_ascii_alphanumeric() || matches!(ch, '-' | '_' | '.'))
    {
        return Err(AppError::unprocessable(format!("{field} is invalid")));
    }

    Ok(value.to_owned())
}

fn collection_write_disabled_error() -> AppError {
    AppError::conflict("Emby collection writes are disabled; use FBZ collection management APIs")
}

#[cfg(test)]
mod tests {
    use axum::extract::Query;
    use http::Uri;
    use serde_json::json;

    use super::*;

    #[test]
    fn collection_creation_result_serializes_pascal_case() {
        let value = serde_json::to_value(CollectionCreationResultDto {
            id: "collection-1".to_owned(),
            name: "Favorites".to_owned(),
        })
        .expect("collection creation result should serialize");

        assert_eq!(
            value,
            json!({
                "Id": "collection-1",
                "Name": "Favorites"
            })
        );
    }

    #[test]
    fn create_collection_query_normalizes_official_parameters() {
        let input = create_collection_input(CreateCollectionQuery {
            is_locked: Some(true),
            name: Some(" Favorites ".to_owned()),
            parent_id: Some(" parent-1 ".to_owned()),
            ids: Some(" item-1,item-2,item-1 ".to_owned()),
        })
        .expect("official collection query should normalize");

        assert_eq!(input.name, "Favorites");
        assert_eq!(input.parent_id.as_deref(), Some("parent-1"));
        assert_eq!(input.ids, ["item-1", "item-2"]);
        assert!(input.is_locked);
    }

    #[test]
    fn collection_queries_accept_lower_camel_client_fields() {
        let uri =
            "/Collections?isLocked=true&name=Favorites&parentId=parent-1&ids=item-1,item-2,item-1"
                .parse::<Uri>()
                .unwrap();
        let Query(query) = Query::<CreateCollectionQuery>::try_from_uri(&uri).unwrap();
        let input = create_collection_input(query).unwrap();

        assert_eq!(input.name, "Favorites");
        assert_eq!(input.parent_id.as_deref(), Some("parent-1"));
        assert_eq!(input.ids, ["item-1", "item-2"]);
        assert!(input.is_locked);

        let uri = "/Collections/collection-1/Items?ids=item-1,item-2,item-1"
            .parse::<Uri>()
            .unwrap();
        let Query(query) = Query::<CollectionItemsQuery>::try_from_uri(&uri).unwrap();
        let input = collection_items_input("collection-1", query.ids.as_deref()).unwrap();

        assert_eq!(input.collection_id, "collection-1");
        assert_eq!(input.ids, ["item-1", "item-2"]);
    }

    #[test]
    fn collection_items_query_requires_ids_and_safe_path_id() {
        let input = collection_items_input(" collection-1 ", Some("item-1,item-2,item-1"))
            .expect("collection item ids should normalize");

        assert_eq!(input.collection_id, "collection-1");
        assert_eq!(input.ids, ["item-1", "item-2"]);

        assert_eq!(
            collection_items_input("collection-1", None)
                .unwrap_err()
                .status_code(),
            StatusCode::UNPROCESSABLE_ENTITY
        );
        assert_eq!(
            collection_items_input("../collection", Some("item-1"))
                .unwrap_err()
                .status_code(),
            StatusCode::UNPROCESSABLE_ENTITY
        );
        assert_eq!(
            collection_items_input("collection-1", Some("item/1"))
                .unwrap_err()
                .status_code(),
            StatusCode::UNPROCESSABLE_ENTITY
        );
    }

    #[test]
    fn create_collection_query_rejects_missing_or_unsafe_values() {
        assert_eq!(
            create_collection_input(CreateCollectionQuery {
                name: Some(" ".to_owned()),
                ..CreateCollectionQuery::default()
            })
            .unwrap_err()
            .status_code(),
            StatusCode::UNPROCESSABLE_ENTITY
        );
        assert_eq!(
            create_collection_input(CreateCollectionQuery {
                name: Some("Favorites".to_owned()),
                parent_id: Some("bad/parent".to_owned()),
                ..CreateCollectionQuery::default()
            })
            .unwrap_err()
            .status_code(),
            StatusCode::UNPROCESSABLE_ENTITY
        );
    }

    #[test]
    fn collection_write_disabled_error_is_conflict() {
        let error = collection_write_disabled_error();

        assert_eq!(error.status_code(), StatusCode::CONFLICT);
        assert_eq!(error.code(), "conflict");
    }
}
