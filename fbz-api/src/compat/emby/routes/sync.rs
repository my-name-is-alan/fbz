use axum::{
    Json,
    extract::{Path, Query, State},
    http::{HeaderMap, StatusCode, Uri},
};
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::{compat::emby::dto::QueryResultDto, error::AppError, state::AppState};

use super::access::{authenticate_query_user, authenticate_request_user};

const DEFAULT_SYNC_LIMIT: u32 = 100;
const MAX_SYNC_LIMIT: u32 = 200;
const MAX_SYNC_ID_LEN: usize = 256;
const MAX_SYNC_ITEM_IDS_LEN: usize = 4096;
const MAX_SYNC_CATEGORY_LEN: usize = 64;
const MAX_SYNC_FILE_NAME_LEN: usize = 512;

#[derive(Clone, Debug, Default, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "PascalCase")]
pub struct SyncOptionsQuery {
    #[serde(alias = "userId", alias = "user_id")]
    pub user_id: Option<String>,
    #[serde(alias = "itemIds", alias = "item_ids")]
    pub item_ids: Option<String>,
    #[serde(alias = "parentId", alias = "parent_id")]
    pub parent_id: Option<String>,
    #[serde(alias = "targetId", alias = "target_id")]
    pub target_id: Option<String>,
    #[serde(alias = "category")]
    pub category: Option<String>,
}

#[derive(Clone, Debug, Default, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "PascalCase")]
pub struct SyncUserQuery {
    #[serde(alias = "userId", alias = "user_id")]
    pub user_id: Option<String>,
}

#[derive(Clone, Debug, Default, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "PascalCase")]
pub struct SyncListQuery {
    #[serde(alias = "userId", alias = "user_id")]
    pub user_id: Option<String>,
    #[serde(alias = "targetId", alias = "target_id")]
    pub target_id: Option<String>,
    #[serde(alias = "startIndex", alias = "start_index")]
    pub start_index: Option<u32>,
    #[serde(alias = "limit")]
    pub limit: Option<u32>,
}

#[derive(Clone, Debug, Default, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "PascalCase")]
pub struct SyncCancelItemsQuery {
    #[serde(alias = "itemIds", alias = "item_ids")]
    pub item_ids: Option<String>,
}

#[derive(Clone, Debug, Default, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "PascalCase")]
pub struct SyncTargetQuery {
    #[serde(alias = "targetId", alias = "target_id")]
    pub target_id: Option<String>,
}

#[derive(Clone, Debug, Default, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "PascalCase")]
pub struct SyncAdditionalFileQuery {
    #[serde(alias = "name")]
    pub name: Option<String>,
}

#[derive(Clone, Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "PascalCase")]
pub struct SyncDialogOptionsDto {
    pub targets: Vec<SyncTargetDto>,
    pub options: Vec<SyncJobOptionDto>,
    pub quality_options: Vec<SyncQualityOptionDto>,
    pub profile_options: Vec<SyncProfileOptionDto>,
}

impl SyncDialogOptionsDto {
    fn empty() -> Self {
        Self {
            targets: Vec::new(),
            options: Vec::new(),
            quality_options: Vec::new(),
            profile_options: Vec::new(),
        }
    }
}

#[derive(Clone, Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "PascalCase")]
pub struct SyncTargetDto {
    pub name: String,
    pub id: String,
}

#[derive(Clone, Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "PascalCase")]
pub struct SyncJobOptionDto {}

#[derive(Clone, Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "PascalCase")]
pub struct SyncQualityOptionDto {
    pub name: String,
    pub description: String,
    pub id: String,
    pub is_default: bool,
    pub is_original_quality: bool,
}

#[derive(Clone, Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "PascalCase")]
pub struct SyncProfileOptionDto {
    pub name: String,
    pub description: String,
    pub id: String,
    pub is_default: bool,
    pub enable_quality_options: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct SyncOptionsInput {
    user_id: Option<String>,
    item_ids: Option<String>,
    parent_id: Option<String>,
    target_id: Option<String>,
    category: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct SyncListInput {
    user_id: Option<String>,
    target_id: Option<String>,
    start_index: u32,
    limit: u32,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct SyncCancelItemsInput {
    item_ids: Option<String>,
}

pub async fn sync_options(
    State(state): State<AppState>,
    Query(query): Query<SyncOptionsQuery>,
    headers: HeaderMap,
    uri: Uri,
) -> Result<Json<SyncDialogOptionsDto>, AppError> {
    let input = sync_options_input(&query)?;
    authenticate_query_user(&state, input.user_id.as_deref(), &headers, &uri).await?;
    let _requested_scope = (
        input.item_ids,
        input.parent_id,
        input.target_id,
        input.category,
    );

    Ok(Json(SyncDialogOptionsDto::empty()))
}

pub async fn sync_targets(
    State(state): State<AppState>,
    Query(query): Query<SyncUserQuery>,
    headers: HeaderMap,
    uri: Uri,
) -> Result<Json<Vec<SyncTargetDto>>, AppError> {
    let user_id = normalize_optional_text(query.user_id.as_deref(), MAX_SYNC_ID_LEN, "UserId")?;
    authenticate_query_user(&state, user_id.as_deref(), &headers, &uri).await?;

    Ok(Json(Vec::new()))
}

pub async fn cancel_target_items(
    State(state): State<AppState>,
    Path(target_id): Path<String>,
    Query(query): Query<SyncCancelItemsQuery>,
    headers: HeaderMap,
    uri: Uri,
) -> Result<StatusCode, AppError> {
    let _user = authenticate_request_user(&state, &headers, &uri).await?;
    let _target_id = normalize_required_text(Some(&target_id), MAX_SYNC_ID_LEN, "TargetId")?;
    let _input = sync_cancel_items_input(&query)?;

    Ok(StatusCode::OK)
}

pub async fn cancel_items(
    State(state): State<AppState>,
    Query(query): Query<SyncCancelItemsQuery>,
    headers: HeaderMap,
    uri: Uri,
) -> Result<StatusCode, AppError> {
    let _user = authenticate_request_user(&state, &headers, &uri).await?;
    let _input = sync_cancel_items_input(&query)?;

    Ok(StatusCode::OK)
}

pub async fn cancel_sync_job(
    State(state): State<AppState>,
    Path(id): Path<String>,
    headers: HeaderMap,
    uri: Uri,
) -> Result<StatusCode, AppError> {
    let _user = authenticate_request_user(&state, &headers, &uri).await?;
    let _id = normalize_required_text(Some(&id), MAX_SYNC_ID_LEN, "Id")?;

    Ok(StatusCode::OK)
}

pub async fn cancel_sync_job_item(
    State(state): State<AppState>,
    Path(id): Path<String>,
    headers: HeaderMap,
    uri: Uri,
) -> Result<StatusCode, AppError> {
    let _user = authenticate_request_user(&state, &headers, &uri).await?;
    let _id = normalize_required_text(Some(&id), MAX_SYNC_ID_LEN, "Id")?;

    Ok(StatusCode::OK)
}

pub async fn enable_sync_job_item(
    State(state): State<AppState>,
    Path(id): Path<String>,
    headers: HeaderMap,
    uri: Uri,
) -> Result<StatusCode, AppError> {
    unsupported_sync_job_item_action(&state, &id, &headers, &uri).await
}

pub async fn mark_sync_job_item_for_removal(
    State(state): State<AppState>,
    Path(id): Path<String>,
    headers: HeaderMap,
    uri: Uri,
) -> Result<StatusCode, AppError> {
    unsupported_sync_job_item_action(&state, &id, &headers, &uri).await
}

pub async fn unmark_sync_job_item_for_removal(
    State(state): State<AppState>,
    Path(id): Path<String>,
    headers: HeaderMap,
    uri: Uri,
) -> Result<StatusCode, AppError> {
    unsupported_sync_job_item_action(&state, &id, &headers, &uri).await
}

pub async fn transferred_sync_job_item(
    State(state): State<AppState>,
    Path(id): Path<String>,
    headers: HeaderMap,
    uri: Uri,
) -> Result<StatusCode, AppError> {
    unsupported_sync_job_item_action(&state, &id, &headers, &uri).await
}

pub async fn item_sync_status(
    State(state): State<AppState>,
    Path(item_id): Path<String>,
    headers: HeaderMap,
    uri: Uri,
) -> Result<StatusCode, AppError> {
    let _user = authenticate_request_user(&state, &headers, &uri).await?;
    let _item_id = normalize_required_text(Some(&item_id), MAX_SYNC_ID_LEN, "ItemId")?;

    Err(sync_mutation_not_configured())
}

pub async fn sync_data(
    State(state): State<AppState>,
    Query(query): Query<SyncTargetQuery>,
    headers: HeaderMap,
    uri: Uri,
) -> Result<StatusCode, AppError> {
    let _user = authenticate_request_user(&state, &headers, &uri).await?;
    let _target_id =
        normalize_required_text(query.target_id.as_deref(), MAX_SYNC_ID_LEN, "TargetId")?;

    Err(sync_mutation_not_configured())
}

pub async fn offline_actions(
    State(state): State<AppState>,
    headers: HeaderMap,
    uri: Uri,
) -> Result<StatusCode, AppError> {
    let _user = authenticate_request_user(&state, &headers, &uri).await?;

    Err(sync_mutation_not_configured())
}

pub async fn create_sync_job(
    State(state): State<AppState>,
    headers: HeaderMap,
    uri: Uri,
) -> Result<StatusCode, AppError> {
    let _user = authenticate_request_user(&state, &headers, &uri).await?;

    Err(sync_mutation_not_configured())
}

pub async fn update_sync_job(
    State(state): State<AppState>,
    Path(id): Path<String>,
    headers: HeaderMap,
    uri: Uri,
) -> Result<StatusCode, AppError> {
    let _user = authenticate_request_user(&state, &headers, &uri).await?;
    let _id = normalize_required_text(Some(&id), MAX_SYNC_ID_LEN, "Id")?;

    Err(sync_mutation_not_configured())
}

pub async fn sync_jobs(
    State(state): State<AppState>,
    Query(query): Query<SyncListQuery>,
    headers: HeaderMap,
    uri: Uri,
) -> Result<Json<QueryResultDto<Value>>, AppError> {
    let input = sync_list_input(&query, false)?;
    authenticate_query_user(&state, input.user_id.as_deref(), &headers, &uri).await?;
    let _requested_window = input.limit;

    Ok(Json(empty_sync_query_result(input.start_index)))
}

pub async fn sync_job_by_id(
    State(state): State<AppState>,
    Path(id): Path<String>,
    headers: HeaderMap,
    uri: Uri,
) -> Result<Json<Value>, AppError> {
    let _user = authenticate_request_user(&state, &headers, &uri).await?;
    let _id = normalize_required_text(Some(&id), MAX_SYNC_ID_LEN, "Id")?;

    Err(sync_job_not_found())
}

pub async fn sync_job_items(
    State(state): State<AppState>,
    Query(query): Query<SyncListQuery>,
    headers: HeaderMap,
    uri: Uri,
) -> Result<Json<QueryResultDto<Value>>, AppError> {
    let input = sync_list_input(&query, true)?;
    authenticate_query_user(&state, input.user_id.as_deref(), &headers, &uri).await?;
    let _requested_target = input.target_id;
    let _requested_window = input.limit;

    Ok(Json(empty_sync_query_result(input.start_index)))
}

pub async fn ready_sync_items(
    State(state): State<AppState>,
    Query(query): Query<SyncListQuery>,
    headers: HeaderMap,
    uri: Uri,
) -> Result<Json<Vec<Value>>, AppError> {
    let input = sync_list_input(&query, true)?;
    authenticate_query_user(&state, input.user_id.as_deref(), &headers, &uri).await?;
    let _requested_target = input.target_id;

    Ok(Json(Vec::new()))
}

pub async fn sync_job_item_additional_file(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Query(query): Query<SyncAdditionalFileQuery>,
    headers: HeaderMap,
    uri: Uri,
) -> Result<StatusCode, AppError> {
    let _user = authenticate_request_user(&state, &headers, &uri).await?;
    let _id = normalize_required_text(Some(&id), MAX_SYNC_ID_LEN, "Id")?;
    let _name = sync_additional_file_name(&query)?;

    Err(sync_file_not_configured())
}

pub async fn sync_job_item_file(
    State(state): State<AppState>,
    Path(id): Path<String>,
    headers: HeaderMap,
    uri: Uri,
) -> Result<StatusCode, AppError> {
    let _user = authenticate_request_user(&state, &headers, &uri).await?;
    let _id = normalize_required_text(Some(&id), MAX_SYNC_ID_LEN, "Id")?;

    Err(sync_file_not_configured())
}

fn sync_options_input(query: &SyncOptionsQuery) -> Result<SyncOptionsInput, AppError> {
    Ok(SyncOptionsInput {
        user_id: normalize_optional_text(query.user_id.as_deref(), MAX_SYNC_ID_LEN, "UserId")?,
        item_ids: normalize_optional_text(
            query.item_ids.as_deref(),
            MAX_SYNC_ITEM_IDS_LEN,
            "ItemIds",
        )?,
        parent_id: normalize_optional_text(
            query.parent_id.as_deref(),
            MAX_SYNC_ID_LEN,
            "ParentId",
        )?,
        target_id: normalize_optional_text(
            query.target_id.as_deref(),
            MAX_SYNC_ID_LEN,
            "TargetId",
        )?,
        category: normalize_optional_text(
            query.category.as_deref(),
            MAX_SYNC_CATEGORY_LEN,
            "Category",
        )?,
    })
}

fn sync_list_input(query: &SyncListQuery, require_target: bool) -> Result<SyncListInput, AppError> {
    let target_id =
        normalize_optional_text(query.target_id.as_deref(), MAX_SYNC_ID_LEN, "TargetId")?;
    if require_target && target_id.is_none() {
        return Err(AppError::unprocessable("TargetId is required"));
    }

    Ok(SyncListInput {
        user_id: normalize_optional_text(query.user_id.as_deref(), MAX_SYNC_ID_LEN, "UserId")?,
        target_id,
        start_index: query.start_index.unwrap_or_default(),
        limit: query
            .limit
            .unwrap_or(DEFAULT_SYNC_LIMIT)
            .min(MAX_SYNC_LIMIT),
    })
}

fn sync_cancel_items_input(query: &SyncCancelItemsQuery) -> Result<SyncCancelItemsInput, AppError> {
    Ok(SyncCancelItemsInput {
        item_ids: normalize_optional_text(
            query.item_ids.as_deref(),
            MAX_SYNC_ITEM_IDS_LEN,
            "ItemIds",
        )?,
    })
}

fn sync_additional_file_name(query: &SyncAdditionalFileQuery) -> Result<String, AppError> {
    normalize_required_text(query.name.as_deref(), MAX_SYNC_FILE_NAME_LEN, "Name")
}

fn empty_sync_query_result(start_index: u32) -> QueryResultDto<Value> {
    QueryResultDto::new(Vec::new(), 0, start_index)
}

async fn unsupported_sync_job_item_action(
    state: &AppState,
    id: &str,
    headers: &HeaderMap,
    uri: &Uri,
) -> Result<StatusCode, AppError> {
    let _user = authenticate_request_user(state, headers, uri).await?;
    let _id = normalize_required_text(Some(id), MAX_SYNC_ID_LEN, "Id")?;

    Err(sync_mutation_not_configured())
}

fn sync_mutation_not_configured() -> AppError {
    AppError::conflict("offline sync job mutations are not configured")
}

fn sync_file_not_configured() -> AppError {
    AppError::conflict("offline sync files are not configured")
}

fn sync_job_not_found() -> AppError {
    AppError::not_found("sync job not found")
}

fn normalize_required_text(
    value: Option<&str>,
    max_len: usize,
    field: &'static str,
) -> Result<String, AppError> {
    normalize_optional_text(value, max_len, field)?
        .ok_or_else(|| AppError::unprocessable(format!("{field} is required")))
}

fn normalize_optional_text(
    value: Option<&str>,
    max_len: usize,
    field: &'static str,
) -> Result<Option<String>, AppError> {
    let Some(value) = value.map(str::trim).filter(|value| !value.is_empty()) else {
        return Ok(None);
    };
    if value.len() > max_len {
        return Err(AppError::unprocessable(format!("{field} is too long")));
    }

    Ok(Some(value.to_owned()))
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    #[test]
    fn empty_sync_options_serializes_with_emby_dialog_option_keys() {
        let value = serde_json::to_value(SyncDialogOptionsDto::empty()).unwrap();

        assert_eq!(value["Targets"], json!([]));
        assert_eq!(value["Options"], json!([]));
        assert_eq!(value["QualityOptions"], json!([]));
        assert_eq!(value["ProfileOptions"], json!([]));
    }

    #[test]
    fn sync_options_input_normalizes_optional_scope() {
        let input = sync_options_input(&SyncOptionsQuery {
            user_id: Some(" user-1 ".to_owned()),
            item_ids: Some(" item-1,item-2 ".to_owned()),
            parent_id: Some(" parent-1 ".to_owned()),
            target_id: Some(" device-1 ".to_owned()),
            category: Some(" Latest ".to_owned()),
        })
        .unwrap();

        assert_eq!(input.user_id.as_deref(), Some("user-1"));
        assert_eq!(input.item_ids.as_deref(), Some("item-1,item-2"));
        assert_eq!(input.parent_id.as_deref(), Some("parent-1"));
        assert_eq!(input.target_id.as_deref(), Some("device-1"));
        assert_eq!(input.category.as_deref(), Some("Latest"));
    }

    #[test]
    fn sync_list_input_preserves_window_and_requires_target_when_needed() {
        let input = sync_list_input(
            &SyncListQuery {
                user_id: Some(" user-1 ".to_owned()),
                target_id: Some(" device-1 ".to_owned()),
                start_index: Some(25),
                limit: Some(999),
            },
            true,
        )
        .unwrap();

        assert_eq!(input.user_id.as_deref(), Some("user-1"));
        assert_eq!(input.target_id.as_deref(), Some("device-1"));
        assert_eq!(input.start_index, 25);
        assert_eq!(input.limit, MAX_SYNC_LIMIT);

        assert!(sync_list_input(&SyncListQuery::default(), true).is_err());
    }

    #[test]
    fn sync_cancel_items_input_normalizes_item_ids() {
        let input = sync_cancel_items_input(&SyncCancelItemsQuery {
            item_ids: Some(" item-1,item-2 ".to_owned()),
        })
        .unwrap();

        assert_eq!(input.item_ids.as_deref(), Some("item-1,item-2"));
    }

    #[test]
    fn sync_required_text_rejects_blank_and_oversized_values() {
        assert!(normalize_required_text(Some(" "), MAX_SYNC_ID_LEN, "Id").is_err());

        let oversized = "x".repeat(MAX_SYNC_ID_LEN + 1);
        assert!(normalize_required_text(Some(&oversized), MAX_SYNC_ID_LEN, "Id").is_err());
    }

    #[test]
    fn sync_mutation_not_configured_is_conflict() {
        let error = sync_mutation_not_configured();

        assert_eq!(error.status_code(), StatusCode::CONFLICT);
        assert_eq!(error.code(), "conflict");
    }

    #[test]
    fn sync_file_probe_errors_use_controlled_status_codes() {
        let file_error = sync_file_not_configured();
        let job_error = sync_job_not_found();

        assert_eq!(file_error.status_code(), StatusCode::CONFLICT);
        assert_eq!(file_error.code(), "conflict");
        assert_eq!(job_error.status_code(), StatusCode::NOT_FOUND);
        assert_eq!(job_error.code(), "not_found");
    }

    #[test]
    fn sync_additional_file_name_is_required_and_bounded() {
        let input = sync_additional_file_name(&SyncAdditionalFileQuery {
            name: Some(" poster.jpg ".to_owned()),
        })
        .unwrap();

        assert_eq!(input, "poster.jpg");
        assert!(sync_additional_file_name(&SyncAdditionalFileQuery::default()).is_err());

        let oversized = "x".repeat(MAX_SYNC_FILE_NAME_LEN + 1);
        assert!(
            sync_additional_file_name(&SyncAdditionalFileQuery {
                name: Some(oversized)
            })
            .is_err()
        );
    }

    #[test]
    fn empty_sync_query_result_preserves_requested_start_index() {
        let result = empty_sync_query_result(15);

        assert_eq!(result.start_index, 15);
        assert_eq!(result.total_record_count, 0);
        assert!(result.items.is_empty());
    }

    #[test]
    fn sync_text_normalizer_rejects_oversized_values() {
        let oversized = "x".repeat(MAX_SYNC_ID_LEN + 1);

        assert!(normalize_optional_text(Some(&oversized), MAX_SYNC_ID_LEN, "TargetId").is_err());
    }
}
