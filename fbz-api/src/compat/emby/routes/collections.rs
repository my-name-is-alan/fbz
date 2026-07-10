use axum::{
    Json,
    extract::{Path, Query, State},
    http::{HeaderMap, StatusCode, Uri},
};
use serde::{Deserialize, Serialize};

use crate::{
    auth::service::AuthenticatedUser,
    compat::emby::dto::{BaseItemDto, BaseItemSource, QueryResultDto},
    error::AppError,
    library::repository::{CollectionDetailRecord, CollectionItemsListInput, LibraryRepository},
    state::AppState,
};

use super::{
    access::{authenticate_query_user, authenticate_request_user},
    items::media_item_to_base_item,
};

const DEFAULT_COLLECTION_ITEMS_LIMIT: u32 = 100;
const MAX_COLLECTION_ITEMS_LIMIT: u32 = 200;
const MAX_COLLECTION_ITEMS_START_INDEX: u32 = 10_000;

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

/// `GET /Collections/{id}` 与 `GET /Collections/{id}/Items` 的读 query。
#[derive(Clone, Debug, Default, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "PascalCase")]
pub struct CollectionReadQuery {
    #[serde(alias = "userId", alias = "user_id")]
    pub user_id: Option<String>,
    #[serde(alias = "startIndex", alias = "start_index")]
    pub start_index: Option<u32>,
    #[serde(alias = "limit")]
    pub limit: Option<u32>,
    #[serde(alias = "fields")]
    pub fields: Option<String>,
    #[serde(alias = "enableImages", alias = "enable_images")]
    pub enable_images: Option<bool>,
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

/// `GET /Collections/{id}`：系列/合集详情（名称 + 简介）。无可见集返回 404。
pub async fn collection_detail(
    State(state): State<AppState>,
    Path(collection_id): Path<String>,
    Query(query): Query<CollectionReadQuery>,
    headers: HeaderMap,
    uri: Uri,
) -> Result<Json<BaseItemDto>, AppError> {
    let user = authenticate_query_user(&state, query.user_id.as_deref(), &headers, &uri).await?;
    let Some(database) = state.database() else {
        return Err(AppError::internal("database is not configured"));
    };
    let collection_id = normalize_required_collection_id(&collection_id, "Id")?;

    let Some(record) = LibraryRepository::new(database.clone())
        .find_user_collection_detail_by_id(user.id, &collection_id)
        .await
        .map_err(|err| AppError::internal(format!("failed to get collection: {err}")))?
    else {
        return Err(AppError::not_found("collection not found"));
    };

    Ok(Json(collection_detail_to_base_item(record)))
}

/// `GET /Collections/{id}/Items`：系列/合集成员（复用 playlist 成员查询，同表）。
pub async fn collection_items(
    State(state): State<AppState>,
    Path(collection_id): Path<String>,
    Query(query): Query<CollectionReadQuery>,
    headers: HeaderMap,
    uri: Uri,
) -> Result<Json<QueryResultDto<BaseItemDto>>, AppError> {
    let user = authenticate_query_user(&state, query.user_id.as_deref(), &headers, &uri).await?;
    let Some(database) = state.database() else {
        return Err(AppError::internal("database is not configured"));
    };
    let collection_id = normalize_required_collection_id(&collection_id, "Id")?;
    let window = CollectionItemsWindow::from_query(&query);
    let _requested_fields = query.fields.as_deref().unwrap_or_default();

    let result = LibraryRepository::new(database.clone())
        .list_user_collection_items(CollectionItemsListInput {
            user_id: user.id,
            collection_id,
            start_index: window.start_index,
            limit: window.limit,
            include_image_tags: query.enable_images.unwrap_or(false),
        })
        .await
        .map_err(|err| AppError::internal(format!("failed to list collection items: {err}")))?;

    let items = result
        .items
        .into_iter()
        .map(media_item_to_base_item)
        .collect();
    Ok(Json(QueryResultDto::new(
        items,
        result.total_record_count,
        window.start_index as u32,
    )))
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct CollectionItemsWindow {
    start_index: i64,
    limit: i64,
}

impl CollectionItemsWindow {
    fn from_query(query: &CollectionReadQuery) -> Self {
        Self {
            start_index: i64::from(
                query
                    .start_index
                    .unwrap_or(0)
                    .min(MAX_COLLECTION_ITEMS_START_INDEX),
            ),
            limit: i64::from(
                query
                    .limit
                    .unwrap_or(DEFAULT_COLLECTION_ITEMS_LIMIT)
                    .clamp(1, MAX_COLLECTION_ITEMS_LIMIT),
            ),
        }
    }
}

/// 系列/合集详情 → BaseItemDto：BoxSet 形状 + 简介。封面由客户端取首个成员海报派生。
fn collection_detail_to_base_item(record: CollectionDetailRecord) -> BaseItemDto {
    let mut item = BaseItemDto::from(BaseItemSource {
        id: record.id,
        name: record.name,
        item_type: "BoxSet".to_owned(),
        media_type: None,
        parent_id: None,
        is_folder: true,
        run_time_ticks: None,
        production_year: None,
    });
    item.overview = record.overview;
    item.collection_type = Some("boxsets".to_owned());
    item
}

pub async fn create_collection(
    State(state): State<AppState>,
    Query(query): Query<CreateCollectionQuery>,
    headers: HeaderMap,
    uri: Uri,
) -> Result<Json<CollectionCreationResultDto>, AppError> {
    let user = authenticate_request_user(&state, &headers, &uri).await?;
    require_collection_manager(&user)?;
    let Some(database) = state.database() else {
        return Err(AppError::internal("database is not configured"));
    };
    let input = create_collection_input(query)?;
    let _ = (input.parent_id, input.is_locked);

    let created = LibraryRepository::new(database.clone())
        .create_collection(&input.name, &input.ids)
        .await
        .map_err(|err| AppError::internal(format!("failed to create collection: {err}")))?;

    Ok(Json(CollectionCreationResultDto {
        id: created.id,
        name: created.name,
    }))
}

pub async fn add_collection_items(
    State(state): State<AppState>,
    Path(collection_id): Path<String>,
    Query(query): Query<CollectionItemsQuery>,
    headers: HeaderMap,
    uri: Uri,
) -> Result<StatusCode, AppError> {
    let user = authenticate_request_user(&state, &headers, &uri).await?;
    require_collection_manager(&user)?;
    let Some(database) = state.database() else {
        return Err(AppError::internal("database is not configured"));
    };
    let input = collection_items_input(&collection_id, query.ids.as_deref())?;

    let outcome = LibraryRepository::new(database.clone())
        .add_collection_items(&input.collection_id, &input.ids)
        .await
        .map_err(|err| AppError::internal(format!("failed to add collection items: {err}")))?;

    match outcome {
        Some(_) => Ok(StatusCode::NO_CONTENT),
        None => Err(AppError::not_found("collection not found")),
    }
}

pub async fn remove_collection_items(
    State(state): State<AppState>,
    Path(collection_id): Path<String>,
    Query(query): Query<CollectionItemsQuery>,
    headers: HeaderMap,
    uri: Uri,
) -> Result<StatusCode, AppError> {
    let user = authenticate_request_user(&state, &headers, &uri).await?;
    require_collection_manager(&user)?;
    let Some(database) = state.database() else {
        return Err(AppError::internal("database is not configured"));
    };
    let input = collection_items_input(&collection_id, query.ids.as_deref())?;

    let outcome = LibraryRepository::new(database.clone())
        .remove_collection_items(&input.collection_id, &input.ids)
        .await
        .map_err(|err| AppError::internal(format!("failed to remove collection items: {err}")))?;

    match outcome {
        Some(_) => Ok(StatusCode::NO_CONTENT),
        None => Err(AppError::not_found("collection not found")),
    }
}

/// 合集是全局资产（影响所有用户的浏览面），写操作限管理员。
fn require_collection_manager(user: &AuthenticatedUser) -> Result<(), AppError> {
    if user.can_manage_server() {
        Ok(())
    } else {
        Err(AppError::forbidden(
            "collection management requires an administrator",
        ))
    }
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
    fn collection_detail_maps_boxset_shape_with_overview() {
        let item = collection_detail_to_base_item(CollectionDetailRecord {
            id: "collection-1".to_owned(),
            name: "哈利·波特系列".to_owned(),
            overview: Some("魔法世界八部曲。".to_owned()),
        });

        assert_eq!(item.item_type, "BoxSet");
        assert!(item.is_folder);
        assert_eq!(item.collection_type.as_deref(), Some("boxsets"));
        assert_eq!(item.overview.as_deref(), Some("魔法世界八部曲。"));
    }

    #[test]
    fn collection_items_window_clamps_pathological_values() {
        let window = CollectionItemsWindow::from_query(&CollectionReadQuery {
            start_index: Some(500_000),
            limit: Some(50),
            ..CollectionReadQuery::default()
        });

        assert_eq!(window.start_index, 10_000);
        assert_eq!(window.limit, 50);
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
    fn collection_writes_require_administrator_role() {
        let admin = AuthenticatedUser {
            id: 1,
            public_id: "user-1".to_owned(),
            username: "admin".to_owned(),
            role_name: "Admin".to_owned(),
            role_name_normalized: "admin".to_owned(),
        };
        let member = AuthenticatedUser {
            id: 2,
            public_id: "user-2".to_owned(),
            username: "member".to_owned(),
            role_name: "Member".to_owned(),
            role_name_normalized: "member".to_owned(),
        };

        assert!(require_collection_manager(&admin).is_ok());
        assert_eq!(
            require_collection_manager(&member)
                .unwrap_err()
                .status_code(),
            StatusCode::FORBIDDEN
        );
    }
}
