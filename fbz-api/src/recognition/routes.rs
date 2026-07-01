//! 识别词管理 admin API（design §7.4，`/api/admin/recognition/*`，server-admin 门控）。
//!
//! - `GET  /api/admin/recognition/words`         列出规则（全局 + 按库过滤）。
//! - `POST /api/admin/recognition/words`         新增/校验一条规则（服务端解析录入语法）。
//! - `DELETE /api/admin/recognition/words/{id}`  删除规则。
//! - `POST /api/admin/recognition/test`          核心调试：样例文件名 → RecognizedMedia 全字段。
//!
//! 每个 handler 走 `authenticate_admin`，被 `every_admin_route_handler_enforces_server_admin`
//! 守卫自动覆盖。

use axum::{
    Json,
    extract::{Path, Query, State},
    http::{HeaderMap, Uri},
};
use serde::{Deserialize, Serialize};

use crate::{
    admin::access::authenticate_admin,
    error::AppError,
    media_types::LibraryType,
    recognition::{
        recognize,
        repository::{CreateRecognitionWordInput, RecognitionRepository, RecognitionWordRecord},
        rules::{ParsedRule, RuleSet, parse_rule_line},
        types::RecognitionInput,
    },
    state::AppState,
};

const MAX_RULE_LINE_LEN: usize = 500;
const MAX_TEST_FILENAME_LEN: usize = 1000;

#[derive(Clone, Debug, Default, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct RecognitionWordQueryDto {
    /// 按库 public_id 过滤；缺省列出全部（全局 + 各库）。
    pub library_id: Option<String>,
}

/// 识别词规则对外形态（`RecognitionWordRecord` 的 camelCase 映射）。
#[derive(Clone, Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct RecognitionWordDto {
    pub id: String,
    pub kind: String,
    pub pattern: String,
    pub replacement: Option<String>,
    pub anchor_after: Option<String>,
    pub offset_expr: Option<String>,
    pub is_regex: bool,
    pub enabled: bool,
    pub library_id: Option<String>,
    pub priority: i32,
    pub note: Option<String>,
}

impl From<RecognitionWordRecord> for RecognitionWordDto {
    fn from(r: RecognitionWordRecord) -> Self {
        Self {
            id: r.id,
            kind: r.kind,
            pattern: r.pattern,
            replacement: r.replacement,
            anchor_after: r.anchor_after,
            offset_expr: r.offset_expr,
            is_regex: r.is_regex,
            enabled: r.enabled,
            library_id: r.library_id,
            priority: r.priority,
            note: r.note,
        }
    }
}

/// 新增识别词请求体。`line` 是 design §7.1 的录入语法行，服务端解析为结构化列。
#[derive(Clone, Debug, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct CreateRecognitionWordDto {
    /// 录入语法行，如 `斗破苍穹年番 => 斗破苍穹` 或 `SP <> 结束 >> EP-12`。
    pub line: String,
    #[serde(default)]
    pub is_regex: bool,
    #[serde(default = "default_enabled")]
    pub enabled: bool,
    pub library_id: Option<String>,
    #[serde(default = "default_priority")]
    pub priority: i32,
    pub note: Option<String>,
}

fn default_enabled() -> bool {
    true
}

fn default_priority() -> i32 {
    100
}

pub async fn list_recognition_words(
    State(state): State<AppState>,
    Query(query): Query<RecognitionWordQueryDto>,
    headers: HeaderMap,
    uri: Uri,
) -> Result<Json<Vec<RecognitionWordDto>>, AppError> {
    authenticate_admin(&state, &headers, &uri).await?;
    let Some(database) = state.database() else {
        return Err(AppError::internal("database is not configured"));
    };
    let words = RecognitionRepository::new(database.clone())
        .list_words(query.library_id.as_deref())
        .await
        .map_err(|err| AppError::internal(format!("failed to list recognition words: {err}")))?;
    Ok(Json(
        words.into_iter().map(RecognitionWordDto::from).collect(),
    ))
}

pub async fn create_recognition_word(
    State(state): State<AppState>,
    headers: HeaderMap,
    uri: Uri,
    Json(payload): Json<CreateRecognitionWordDto>,
) -> Result<(axum::http::StatusCode, Json<RecognitionWordDto>), AppError> {
    authenticate_admin(&state, &headers, &uri).await?;
    if payload.line.trim().is_empty() || payload.line.len() > MAX_RULE_LINE_LEN {
        return Err(AppError::unprocessable(
            "line must be non-empty and within length limit",
        ));
    }
    // 服务端解析录入语法（design §7.1），语法错误回报 422。
    let parsed: ParsedRule = parse_rule_line(&payload.line, payload.is_regex)
        .map_err(|err| AppError::unprocessable(format!("invalid rule syntax: {err}")))?;

    let Some(database) = state.database() else {
        return Err(AppError::internal("database is not configured"));
    };
    let created = RecognitionRepository::new(database.clone())
        .create_word(CreateRecognitionWordInput {
            kind: parsed.kind,
            pattern: parsed.pattern,
            replacement: parsed.replacement,
            anchor_after: parsed.anchor_after,
            offset_expr: parsed.offset_expr,
            is_regex: parsed.is_regex,
            enabled: payload.enabled,
            library_public_id: payload.library_id,
            priority: payload.priority,
            note: payload.note,
        })
        .await
        .map_err(|err| AppError::internal(format!("failed to create recognition word: {err}")))?
        .ok_or_else(|| AppError::not_found("library not found"))?;

    Ok((axum::http::StatusCode::CREATED, Json(created.into())))
}

pub async fn delete_recognition_word(
    State(state): State<AppState>,
    Path(word_id): Path<String>,
    headers: HeaderMap,
    uri: Uri,
) -> Result<axum::http::StatusCode, AppError> {
    authenticate_admin(&state, &headers, &uri).await?;
    let Some(database) = state.database() else {
        return Err(AppError::internal("database is not configured"));
    };
    let removed = RecognitionRepository::new(database.clone())
        .delete_word(&word_id)
        .await
        .map_err(|err| AppError::internal(format!("failed to delete recognition word: {err}")))?;
    if removed {
        Ok(axum::http::StatusCode::NO_CONTENT)
    } else {
        Err(AppError::not_found("recognition word not found"))
    }
}

/// `/recognition/test` 请求：样例文件名 + 可选库类型 + 可选库（加载该库规则）。
#[derive(Clone, Debug, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct RecognitionTestDto {
    /// 样例文件名（可含路径，用 `/` 或 `\` 分隔，末段为文件名）。
    pub filename: String,
    /// 库类型，缺省 `mixed`。
    pub library_type: Option<String>,
    /// 加载该库的识别词规则（连同全局规则）；缺省只用全局规则。
    pub library_id: Option<String>,
}

/// `/recognition/test` 响应：`RecognizedMedia` 全字段 + 命中规则（design §7.4 核心调试）。
#[derive(Clone, Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct RecognitionTestResultDto {
    pub kind: String,
    pub title: String,
    pub original_title: Option<String>,
    pub year: Option<i32>,
    pub season: Option<i32>,
    pub episodes: Vec<i32>,
    pub edition: Option<String>,
    pub release_group: Option<String>,
    pub resolution: Option<String>,
    pub source: Option<String>,
    pub video_codec: Option<String>,
    pub audio_codec: Option<String>,
    pub hdr: Option<String>,
    pub part: Option<i32>,
    pub content_hint: String,
    pub confidence: String,
    pub matched_rules: Vec<String>,
}

pub async fn test_recognition(
    State(state): State<AppState>,
    headers: HeaderMap,
    uri: Uri,
    Json(payload): Json<RecognitionTestDto>,
) -> Result<Json<RecognitionTestResultDto>, AppError> {
    authenticate_admin(&state, &headers, &uri).await?;
    if payload.filename.trim().is_empty() || payload.filename.len() > MAX_TEST_FILENAME_LEN {
        return Err(AppError::unprocessable(
            "filename must be non-empty and within length limit",
        ));
    }
    let library_type = payload
        .library_type
        .as_deref()
        .and_then(LibraryType::parse)
        .unwrap_or(LibraryType::Mixed);

    // 加载该库（+全局）规则；无库则只全局。后端不可达时用空规则集（仍能跑内置解析）。
    let ruleset = match state.database() {
        Some(database) => RecognitionRepository::new(database.clone())
            .load_ruleset_for_library(payload.library_id.as_deref())
            .await
            .map(|(rs, _skipped)| rs)
            .unwrap_or_else(|_| RuleSet::compile(Vec::new()).0),
        None => RuleSet::compile(Vec::new()).0,
    };

    // 把文件名拆成 stem + 祖先目录链（近→远）。
    let (stem, extension, ancestors) = split_filename(&payload.filename);
    let ancestor_refs: Vec<&str> = ancestors.iter().map(String::as_str).collect();
    let input = RecognitionInput {
        file_stem: &stem,
        extension: extension.as_deref(),
        ancestors: &ancestor_refs,
    };
    let result = recognize(&input, library_type, &ruleset);

    Ok(Json(RecognitionTestResultDto {
        kind: format!("{:?}", result.kind),
        title: result.title,
        original_title: result.original_title,
        year: result.year,
        season: result.season,
        episodes: result.episodes,
        edition: result.edition,
        release_group: result.release_group,
        resolution: result.quality.resolution,
        source: result.quality.source,
        video_codec: result.quality.video_codec,
        audio_codec: result.quality.audio_codec,
        hdr: result.quality.hdr,
        part: result.part,
        content_hint: format!("{:?}", result.content_hint),
        confidence: format!("{:?}", result.confidence),
        matched_rules: result.matched_rules,
    }))
}

/// 把一条路径串拆为（文件主名, 扩展名, 祖先目录链近→远）。纯解析，无 IO。
fn split_filename(raw: &str) -> (String, Option<String>, Vec<String>) {
    let normalized = raw.replace('\\', "/");
    let parts: Vec<&str> = normalized.split('/').filter(|p| !p.is_empty()).collect();
    let Some((file, dirs)) = parts.split_last() else {
        return (raw.to_owned(), None, Vec::new());
    };
    let (stem, extension) = match file.rsplit_once('.') {
        Some((s, ext)) if !s.is_empty() => (s.to_owned(), Some(ext.to_ascii_lowercase())),
        _ => ((*file).to_owned(), None),
    };
    // 祖先链近→远。
    let ancestors: Vec<String> = dirs.iter().rev().map(|d| (*d).to_owned()).collect();
    (stem, extension, ancestors)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn split_filename_extracts_stem_ext_and_ancestors() {
        let (stem, ext, ancestors) =
            split_filename("/media/tv/Friends/Season 02/friends.s02e08.mkv");
        assert_eq!(stem, "friends.s02e08");
        assert_eq!(ext.as_deref(), Some("mkv"));
        // 近→远。
        assert_eq!(ancestors, vec!["Season 02", "Friends", "tv", "media"]);
    }

    #[test]
    fn split_filename_handles_bare_name() {
        let (stem, ext, ancestors) = split_filename("Inception.2010.mkv");
        assert_eq!(stem, "Inception.2010");
        assert_eq!(ext.as_deref(), Some("mkv"));
        assert!(ancestors.is_empty());
    }

    #[test]
    fn split_filename_handles_windows_paths() {
        let (stem, _ext, ancestors) = split_filename(r"D:\Media\Show\ep.mkv");
        assert_eq!(stem, "ep");
        assert_eq!(ancestors, vec!["Show", "Media", "D:"]);
    }
}
