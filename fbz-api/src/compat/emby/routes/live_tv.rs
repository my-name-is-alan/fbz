//! Live TV **Emby 兼容空桩**（非目标 / 非明早必验）。
//!
//! 客户端探测路由一律受控响应，**禁止 500**：
//! - 列表类 → 空 `QueryResult`（`Items=[]`, `TotalRecordCount=0`）
//! - Info / GuideInfo → `IsEnabled=false`
//! - Folder → 固定 CollectionFolder 形状
//! - 单条 GET → 404（空 body）
//! - 写/删/配置 mutation → 409 Conflict（明确 stub 文案）
//!
//! **不**实现真 tuner / IPTV 源 / 录制流水线。见
//! `docs/ACCEPTANCE_2026-10-16.md` 附录 C、`docs/JELLYFIN_PARITY_MATRIX.md`。

use axum::{
    Json,
    extract::{Path, Query},
    response::{IntoResponse, Response},
};
use http::StatusCode;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::BTreeMap;

use crate::{
    compat::emby::dto::{
        BaseItemDto, BaseItemSource, LiveTvInfoDto, NameValuePairDto, QueryResultDto,
    },
    error::AppError,
};

const MAX_LIVE_TV_EMPTY_LIMIT: u32 = 200;
const MAX_LIVE_TV_EMPTY_START_INDEX: u32 = 10_000;
const MAX_LIVE_TV_PROGRAM_ID_LEN: usize = 128;

#[derive(Clone, Debug, Default, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "PascalCase")]
pub struct LiveTvListQuery {
    #[serde(alias = "startIndex", alias = "start_index")]
    pub start_index: Option<u32>,
    #[serde(alias = "limit")]
    pub limit: Option<u32>,
}

#[derive(Clone, Debug, Default, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "PascalCase")]
pub struct TimerDefaultsQuery {
    #[serde(alias = "programId", alias = "program_id")]
    pub program_id: Option<String>,
}

#[derive(Clone, Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "PascalCase")]
pub struct ListingsProviderInfoDto {
    pub name: String,
    pub setup_url: String,
    pub id: String,
    #[serde(rename = "Type")]
    pub provider_type: String,
    pub username: String,
    pub password: String,
    pub listings_id: String,
    pub zip_code: String,
    pub country: String,
    pub path: String,
    pub enabled_tuners: Vec<String>,
    pub enable_all_tuners: bool,
    pub news_categories: Vec<String>,
    pub sports_categories: Vec<String>,
    pub kids_categories: Vec<String>,
    pub movie_categories: Vec<String>,
    pub channel_mappings: Vec<NameValuePairDto>,
    pub tvg_shift_ticks: i64,
    pub movie_prefix: String,
    pub preferred_language: String,
    pub user_agent: String,
    pub data_version: String,
}

#[derive(Clone, Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "PascalCase")]
pub struct SeriesTimerInfoDto {
    pub record_any_time: bool,
    pub skip_episodes_in_library: bool,
    pub match_existing_items_with_any_library: bool,
    pub record_any_channel: bool,
    pub keep_up_to: i32,
    pub max_recording_seconds: i32,
    pub record_new_only: bool,
    pub channel_ids: Vec<String>,
    pub days: Vec<String>,
    pub image_tags: BTreeMap<String, String>,
    pub series_id: Option<String>,
    #[serde(rename = "TimerType")]
    pub timer_type: String,
    pub id: String,
    #[serde(rename = "Type")]
    pub item_type: String,
    pub server_id: String,
    pub channel_id: Option<String>,
    pub program_id: Option<String>,
    pub name: String,
    pub overview: String,
    pub parent_folder_id: Option<String>,
    pub start_date: Option<String>,
    pub end_date: Option<String>,
    pub priority: i32,
    pub pre_padding_seconds: i32,
    pub post_padding_seconds: i32,
    pub is_pre_padding_required: bool,
    pub is_post_padding_required: bool,
    pub keep_until: String,
}

/// `GET …/LiveTv/Info|GuideInfo` — disabled stub（200，非 500）。
pub async fn live_tv_info() -> Json<LiveTvInfoDto> {
    Json(LiveTvInfoDto::disabled())
}

/// `GET …/LiveTv/Folder` — 固定空库文件夹形状（200）。
pub async fn live_tv_folder() -> Json<BaseItemDto> {
    Json(disabled_live_tv_folder())
}

/// 列表类 Live TV 探测 — 空列表（200）；忽略多余 query 字段。
pub async fn empty_query(Query(query): Query<LiveTvListQuery>) -> Json<QueryResultDto<Value>> {
    let start_index = query
        .start_index
        .unwrap_or_default()
        .min(MAX_LIVE_TV_EMPTY_START_INDEX);
    let _limit = query.limit.unwrap_or(MAX_LIVE_TV_EMPTY_LIMIT);
    Json(QueryResultDto::new(Vec::new(), 0, start_index))
}

/// `GET …/ListingProviders/Default` — Type=None 空配置（200）。
pub async fn listing_provider_default() -> Json<ListingsProviderInfoDto> {
    Json(default_listing_provider())
}

/// `GET …/Timers/Defaults` — 未启用录制的默认 Timer 形状（200）。
pub async fn timer_defaults(Query(query): Query<TimerDefaultsQuery>) -> Json<SeriesTimerInfoDto> {
    Json(default_timer_info(query.program_id))
}

/// 单条 Channel/Program/Recording/Timer GET — 明确 404（非 500）。
pub async fn empty_item(Path(_id): Path<String>) -> Response {
    (StatusCode::NOT_FOUND, "").into_response()
}

/// Live TV 写/删/配置 — 明确 409 Conflict stub（非 500 / 非真 tuner）。
pub async fn mutation_not_configured() -> Result<Response, AppError> {
    Err(AppError::conflict(
        "Emby Live TV mutations are disabled; configure Live TV sources through FBZ APIs",
    ))
}

fn disabled_live_tv_folder() -> BaseItemDto {
    let mut item = BaseItemDto::from(BaseItemSource {
        id: "livetv".to_owned(),
        name: "Live TV".to_owned(),
        item_type: "CollectionFolder".to_owned(),
        media_type: None,
        parent_id: None,
        is_folder: true,
        run_time_ticks: None,
        production_year: None,
    });
    item.collection_type = Some("livetv".to_owned());
    item
}

fn default_listing_provider() -> ListingsProviderInfoDto {
    ListingsProviderInfoDto {
        name: String::new(),
        setup_url: String::new(),
        id: String::new(),
        provider_type: "None".to_owned(),
        username: String::new(),
        password: String::new(),
        listings_id: String::new(),
        zip_code: String::new(),
        country: String::new(),
        path: String::new(),
        enabled_tuners: Vec::new(),
        enable_all_tuners: false,
        news_categories: Vec::new(),
        sports_categories: Vec::new(),
        kids_categories: Vec::new(),
        movie_categories: Vec::new(),
        channel_mappings: Vec::new(),
        tvg_shift_ticks: 0,
        movie_prefix: String::new(),
        preferred_language: String::new(),
        user_agent: String::new(),
        data_version: String::new(),
    }
}

fn default_timer_info(program_id: Option<String>) -> SeriesTimerInfoDto {
    SeriesTimerInfoDto {
        record_any_time: false,
        skip_episodes_in_library: false,
        match_existing_items_with_any_library: false,
        record_any_channel: false,
        keep_up_to: 0,
        max_recording_seconds: 0,
        record_new_only: false,
        channel_ids: Vec::new(),
        days: Vec::new(),
        image_tags: BTreeMap::new(),
        series_id: None,
        timer_type: "Program".to_owned(),
        id: String::new(),
        item_type: "SeriesTimer".to_owned(),
        server_id: String::new(),
        channel_id: None,
        program_id: normalize_optional_program_id(program_id),
        name: String::new(),
        overview: String::new(),
        parent_folder_id: None,
        start_date: None,
        end_date: None,
        priority: 0,
        pre_padding_seconds: 0,
        post_padding_seconds: 0,
        is_pre_padding_required: false,
        is_post_padding_required: false,
        keep_until: "UntilDeleted".to_owned(),
    }
}

fn normalize_optional_program_id(value: Option<String>) -> Option<String> {
    value
        .map(|value| value.trim().to_owned())
        .filter(|value| !value.is_empty())
        .filter(|value| {
            value.len() <= MAX_LIVE_TV_PROGRAM_ID_LEN && !value.chars().any(char::is_control)
        })
}

#[cfg(test)]
mod tests {
    use axum::extract::Query;
    use http::Uri;
    use serde_json::json;

    use super::*;

    #[test]
    fn disabled_info_matches_emby_live_tv_probe_shape() {
        let value = serde_json::to_value(LiveTvInfoDto::disabled()).unwrap();

        assert_eq!(value["IsEnabled"], json!(false));
        assert_eq!(value["EnabledUsers"], json!([]));
    }

    #[test]
    fn disabled_folder_uses_emby_collection_folder_shape() {
        let value = serde_json::to_value(disabled_live_tv_folder()).unwrap();

        assert_eq!(value["Id"], json!("livetv"));
        assert_eq!(value["Name"], json!("Live TV"));
        assert_eq!(value["Type"], json!("CollectionFolder"));
        assert_eq!(value["CollectionType"], json!("livetv"));
        assert_eq!(value["IsFolder"], json!(true));
    }

    #[test]
    fn default_listing_provider_is_empty_and_disabled() {
        let value = serde_json::to_value(default_listing_provider()).unwrap();

        assert_eq!(value["Type"], json!("None"));
        assert_eq!(value["EnableAllTuners"], json!(false));
        assert_eq!(value["EnabledTuners"], json!([]));
        assert_eq!(value["ChannelMappings"], json!([]));
    }

    #[test]
    fn timer_defaults_keep_program_id_without_enabling_recording() {
        let value =
            serde_json::to_value(default_timer_info(Some(" program-1 ".to_owned()))).unwrap();

        assert_eq!(value["ProgramId"], json!("program-1"));
        assert_eq!(value["TimerType"], json!("Program"));
        assert_eq!(value["RecordAnyTime"], json!(false));
        assert_eq!(value["RecordAnyChannel"], json!(false));
        assert_eq!(value["ChannelIds"], json!([]));
    }

    #[test]
    fn timer_defaults_query_accepts_lower_camel_client_fields() {
        let uri = "/LiveTv/Timers/Defaults?programId=program-1"
            .parse::<Uri>()
            .unwrap();
        let Query(query) = Query::<TimerDefaultsQuery>::try_from_uri(&uri).unwrap();
        let value = serde_json::to_value(default_timer_info(query.program_id)).unwrap();

        assert_eq!(value["ProgramId"], json!("program-1"));
        assert_eq!(value["TimerType"], json!("Program"));
    }

    #[tokio::test]
    async fn empty_query_preserves_requested_start_index() {
        let query = LiveTvListQuery {
            start_index: Some(25),
            limit: Some(10),
        };

        let Json(result) = empty_query(Query(query)).await;

        assert_eq!(result.start_index, 25);
        assert_eq!(result.total_record_count, 0);
        assert!(result.items.is_empty());
    }

    #[tokio::test]
    async fn empty_query_clamps_pathologically_large_start_index() {
        let query = LiveTvListQuery {
            start_index: Some(500_000),
            limit: Some(10),
        };

        let Json(result) = empty_query(Query(query)).await;

        assert_eq!(result.start_index, 10_000);
        assert_eq!(result.total_record_count, 0);
        assert!(result.items.is_empty());
    }

    #[tokio::test]
    async fn mutation_not_configured_is_conflict() {
        let err = mutation_not_configured().await.unwrap_err();

        assert_eq!(err.status_code(), StatusCode::CONFLICT);
        assert_eq!(err.code(), "conflict");
        assert_ne!(err.status_code(), StatusCode::INTERNAL_SERVER_ERROR);
    }

    #[tokio::test]
    async fn live_tv_info_and_folder_handlers_are_ok_not_500() {
        let Json(info) = live_tv_info().await;
        assert!(!info.is_enabled);
        assert!(info.enabled_users.is_empty());

        let Json(folder) = live_tv_folder().await;
        assert_eq!(folder.id, "livetv");
        assert_eq!(folder.collection_type.as_deref(), Some("livetv"));
    }

    #[tokio::test]
    async fn empty_item_is_not_found_not_server_error() {
        let response = empty_item(Path("channel-1".to_owned())).await;
        assert_eq!(response.status(), StatusCode::NOT_FOUND);
        assert_ne!(response.status(), StatusCode::INTERNAL_SERVER_ERROR);
    }

    #[tokio::test]
    async fn listing_provider_and_timer_defaults_handlers_are_ok() {
        let Json(provider) = listing_provider_default().await;
        assert_eq!(provider.provider_type, "None");
        assert!(!provider.enable_all_tuners);

        let Json(timer) = timer_defaults(Query(TimerDefaultsQuery {
            program_id: Some("p1".to_owned()),
        }))
        .await;
        assert_eq!(timer.program_id.as_deref(), Some("p1"));
        assert!(!timer.record_any_time);
    }

    #[test]
    fn stub_contract_documents_no_real_tuner() {
        // Contract for morning matrix: Live TV is non-target; responses are
        // empty / 404 / 409 stubs — never an unimplemented panic/500 path.
        assert!(MAX_LIVE_TV_EMPTY_LIMIT > 0);
        assert_eq!(default_listing_provider().provider_type, "None");
        assert!(!LiveTvInfoDto::disabled().is_enabled);
    }
}
