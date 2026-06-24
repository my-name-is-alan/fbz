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

pub async fn live_tv_info() -> Json<LiveTvInfoDto> {
    Json(LiveTvInfoDto::disabled())
}

pub async fn live_tv_folder() -> Json<BaseItemDto> {
    Json(disabled_live_tv_folder())
}

pub async fn empty_query(Query(query): Query<LiveTvListQuery>) -> Json<QueryResultDto<Value>> {
    let start_index = query.start_index.unwrap_or_default();
    let _limit = query.limit.unwrap_or(MAX_LIVE_TV_EMPTY_LIMIT);
    Json(QueryResultDto::new(Vec::new(), 0, start_index))
}

pub async fn listing_provider_default() -> Json<ListingsProviderInfoDto> {
    Json(default_listing_provider())
}

pub async fn timer_defaults(Query(query): Query<TimerDefaultsQuery>) -> Json<SeriesTimerInfoDto> {
    Json(default_timer_info(query.program_id))
}

pub async fn empty_item(Path(_id): Path<String>) -> Response {
    (StatusCode::NOT_FOUND, "").into_response()
}

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
    async fn mutation_not_configured_is_conflict() {
        let err = mutation_not_configured().await.unwrap_err();

        assert_eq!(err.status_code(), StatusCode::CONFLICT);
        assert_eq!(err.code(), "conflict");
    }
}
