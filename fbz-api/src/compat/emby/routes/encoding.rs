use axum::{
    Json,
    body::Bytes,
    extract::{Query, State},
    http::{HeaderMap, StatusCode, Uri},
};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

use crate::{
    auth::service::AuthenticatedUser, error::AppError,
    settings::repository::SettingsRepository, state::AppState,
};

use super::access::authenticate_request_user;

const MAX_ENCODING_TEXT_LEN: usize = 128;
const MAX_ENCODING_WRITE_BODY_BYTES: usize = 64 * 1024;
const FULL_TONE_MAP_SETTING_KEY: &str = "emby.encoding.tonemap.full";
const PUBLIC_TONE_MAP_SETTING_KEY: &str = "emby.encoding.tonemap.public";
const SUBTITLE_OPTIONS_SETTING_KEY: &str = "emby.encoding.subtitles";

#[derive(Clone, Debug, Default, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "PascalCase")]
pub struct CodecParameterQuery {
    #[serde(alias = "codecId", alias = "codec_id")]
    pub codec_id: Option<String>,
    #[serde(alias = "parameterContext", alias = "parameter_context")]
    pub parameter_context: Option<String>,
}

#[derive(Clone, Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "PascalCase")]
pub struct CodecConfigurationDto {
    pub is_enabled: bool,
    pub priority: i32,
    pub codec_id: String,
}

#[derive(Clone, Debug, Serialize, PartialEq)]
#[serde(rename_all = "PascalCase")]
pub struct EditObjectContainerDto {
    pub object: Value,
    pub default_object: Value,
    pub type_name: String,
    pub editor_root: Vec<Value>,
}

#[derive(Clone, Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "PascalCase")]
pub struct ToneMapOptionsVisibilityDto {
    pub show_advanced: bool,
    pub is_software_tone_mapping_available: bool,
    pub is_any_hardware_tone_mapping_available: bool,
    pub show_nvidia_options: bool,
    pub show_quick_sync_options: bool,
    pub show_vaapi_options: bool,
    pub is_open_cl_available: bool,
    pub is_open_cl_super_t_available: bool,
    pub is_vaapi_native_available: bool,
    pub is_quick_sync_native_available: bool,
    pub operating_system: OperatingSystemDto,
}

#[derive(Clone, Debug, Serialize, PartialEq, Eq)]
#[allow(dead_code)]
pub enum OperatingSystemDto {
    Windows,
    Linux,
    OSX,
    BSD,
    Android,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct CodecParameterInput {
    codec_id: String,
    parameter_context: String,
}

pub async fn codec_configuration_defaults(
    State(state): State<AppState>,
    headers: HeaderMap,
    uri: Uri,
) -> Result<Json<Vec<CodecConfigurationDto>>, AppError> {
    authenticate_admin_user(&state, &headers, &uri).await?;

    Ok(Json(default_codec_configurations()))
}

pub async fn video_codec_information(
    State(state): State<AppState>,
    headers: HeaderMap,
    uri: Uri,
) -> Result<Json<Vec<Value>>, AppError> {
    authenticate_admin_user(&state, &headers, &uri).await?;

    Ok(Json(Vec::new()))
}

pub async fn tone_map_options_visibility(
    State(state): State<AppState>,
    headers: HeaderMap,
    uri: Uri,
) -> Result<Json<ToneMapOptionsVisibilityDto>, AppError> {
    authenticate_admin_user(&state, &headers, &uri).await?;

    Ok(Json(default_tone_map_visibility()))
}

pub async fn full_tone_map_options(
    State(state): State<AppState>,
    headers: HeaderMap,
    uri: Uri,
) -> Result<Json<EditObjectContainerDto>, AppError> {
    authenticate_request_user(&state, &headers, &uri).await?;

    Ok(Json(
        with_stored_object(
            &state,
            FULL_TONE_MAP_SETTING_KEY,
            tone_map_edit_container("FullToneMapOptions"),
        )
        .await?,
    ))
}

pub async fn update_full_tone_map_options(
    State(state): State<AppState>,
    headers: HeaderMap,
    uri: Uri,
    body: Bytes,
) -> Result<StatusCode, AppError> {
    let user = authenticate_admin_user(&state, &headers, &uri).await?;
    ensure_write_body_within_limit(&body)?;
    let value = parse_encoding_object(&body)?;
    store_encoding_setting(&state, FULL_TONE_MAP_SETTING_KEY, value, &user).await?;

    Ok(StatusCode::NO_CONTENT)
}

pub async fn public_tone_map_options(
    State(state): State<AppState>,
    headers: HeaderMap,
    uri: Uri,
) -> Result<Json<EditObjectContainerDto>, AppError> {
    authenticate_request_user(&state, &headers, &uri).await?;

    Ok(Json(
        with_stored_object(
            &state,
            PUBLIC_TONE_MAP_SETTING_KEY,
            tone_map_edit_container("PublicToneMapOptions"),
        )
        .await?,
    ))
}

pub async fn update_public_tone_map_options(
    State(state): State<AppState>,
    headers: HeaderMap,
    uri: Uri,
    body: Bytes,
) -> Result<StatusCode, AppError> {
    let user = authenticate_admin_user(&state, &headers, &uri).await?;
    ensure_write_body_within_limit(&body)?;
    let value = parse_encoding_object(&body)?;
    store_encoding_setting(&state, PUBLIC_TONE_MAP_SETTING_KEY, value, &user).await?;

    Ok(StatusCode::NO_CONTENT)
}

pub async fn codec_parameters(
    State(state): State<AppState>,
    Query(query): Query<CodecParameterQuery>,
    headers: HeaderMap,
    uri: Uri,
) -> Result<Json<EditObjectContainerDto>, AppError> {
    authenticate_request_user(&state, &headers, &uri).await?;
    let input = codec_parameter_input(&query)?;

    Ok(Json(
        with_stored_object(
            &state,
            &codec_parameter_setting_key(&input),
            codec_parameter_edit_container(&input),
        )
        .await?,
    ))
}

pub async fn update_codec_parameters(
    State(state): State<AppState>,
    Query(query): Query<CodecParameterQuery>,
    headers: HeaderMap,
    uri: Uri,
    body: Bytes,
) -> Result<StatusCode, AppError> {
    let user = authenticate_admin_user(&state, &headers, &uri).await?;
    let input = codec_parameter_input(&query)?;
    ensure_write_body_within_limit(&body)?;
    let value = parse_encoding_object(&body)?;
    store_encoding_setting(&state, &codec_parameter_setting_key(&input), value, &user).await?;

    Ok(StatusCode::NO_CONTENT)
}

pub async fn subtitle_options(
    State(state): State<AppState>,
    headers: HeaderMap,
    uri: Uri,
) -> Result<Json<EditObjectContainerDto>, AppError> {
    authenticate_request_user(&state, &headers, &uri).await?;

    Ok(Json(
        with_stored_object(
            &state,
            SUBTITLE_OPTIONS_SETTING_KEY,
            subtitle_options_edit_container(),
        )
        .await?,
    ))
}

pub async fn update_subtitle_options(
    State(state): State<AppState>,
    headers: HeaderMap,
    uri: Uri,
    body: Bytes,
) -> Result<StatusCode, AppError> {
    let user = authenticate_admin_user(&state, &headers, &uri).await?;
    ensure_write_body_within_limit(&body)?;
    let value = parse_encoding_object(&body)?;
    store_encoding_setting(&state, SUBTITLE_OPTIONS_SETTING_KEY, value, &user).await?;

    Ok(StatusCode::NO_CONTENT)
}

async fn authenticate_admin_user(
    state: &AppState,
    headers: &HeaderMap,
    uri: &Uri,
) -> Result<AuthenticatedUser, AppError> {
    let user = authenticate_request_user(state, headers, uri).await?;
    if !user.can_manage_server() {
        return Err(AppError::forbidden("server management permission required"));
    }

    Ok(user)
}

/// codec 参数按 (codecId, context) 独立成键。两个字段已限长且拒绝控制字符。
fn codec_parameter_setting_key(input: &CodecParameterInput) -> String {
    format!(
        "emby.encoding.codec.{}.{}",
        input.codec_id.to_ascii_lowercase(),
        input.parameter_context.to_ascii_lowercase()
    )
}

fn parse_encoding_object(body: &Bytes) -> Result<Value, AppError> {
    let value: Value = serde_json::from_slice(body)
        .map_err(|err| AppError::unprocessable(format!("invalid JSON request body: {err}")))?;
    if !value.is_object() {
        return Err(AppError::unprocessable(
            "encoding options body must be a JSON object",
        ));
    }

    Ok(value)
}

/// GET 容器：存储对象存在时盖过默认 Object（DefaultObject 保留出厂值）。
async fn with_stored_object(
    state: &AppState,
    setting_key: &str,
    mut container: EditObjectContainerDto,
) -> Result<EditObjectContainerDto, AppError> {
    let Some(database) = state.database() else {
        return Err(AppError::internal("database is not configured"));
    };

    if let Some(stored) = SettingsRepository::new(database.clone())
        .get(setting_key)
        .await
        .map_err(|err| AppError::internal(format!("failed to load encoding options: {err}")))?
    {
        container.object = stored.value;
    }

    Ok(container)
}

async fn store_encoding_setting(
    state: &AppState,
    setting_key: &str,
    value: Value,
    user: &AuthenticatedUser,
) -> Result<(), AppError> {
    let Some(database) = state.database() else {
        return Err(AppError::internal("database is not configured"));
    };

    SettingsRepository::new(database.clone())
        .update_admin_setting(
            setting_key,
            value,
            &user.username,
            Some("emby encoding options update"),
        )
        .await
        .map_err(|err| AppError::internal(format!("failed to store encoding options: {err}")))?;

    Ok(())
}

fn default_codec_configurations() -> Vec<CodecConfigurationDto> {
    Vec::new()
}

fn default_tone_map_visibility() -> ToneMapOptionsVisibilityDto {
    ToneMapOptionsVisibilityDto {
        show_advanced: false,
        is_software_tone_mapping_available: true,
        is_any_hardware_tone_mapping_available: false,
        show_nvidia_options: false,
        show_quick_sync_options: false,
        show_vaapi_options: false,
        is_open_cl_available: false,
        is_open_cl_super_t_available: false,
        is_vaapi_native_available: false,
        is_quick_sync_native_available: false,
        operating_system: current_operating_system(),
    }
}

fn tone_map_edit_container(type_name: &str) -> EditObjectContainerDto {
    empty_edit_container(
        type_name,
        json!({
            "EnableToneMapping": false,
            "ToneMappingAlgorithm": "none"
        }),
    )
}

fn codec_parameter_edit_container(input: &CodecParameterInput) -> EditObjectContainerDto {
    empty_edit_container(
        "CodecParameters",
        json!({
            "CodecId": input.codec_id,
            "ParameterContext": input.parameter_context
        }),
    )
}

fn subtitle_options_edit_container() -> EditObjectContainerDto {
    empty_edit_container(
        "SubtitleOptions",
        json!({
            "EnableSubtitleExtraction": true
        }),
    )
}

fn empty_edit_container(type_name: &str, object: Value) -> EditObjectContainerDto {
    EditObjectContainerDto {
        object: object.clone(),
        default_object: object,
        type_name: type_name.to_owned(),
        editor_root: Vec::new(),
    }
}

fn codec_parameter_input(query: &CodecParameterQuery) -> Result<CodecParameterInput, AppError> {
    Ok(CodecParameterInput {
        codec_id: normalize_required_encoding_text(query.codec_id.as_deref(), "CodecId")?,
        parameter_context: normalize_required_encoding_text(
            query.parameter_context.as_deref(),
            "ParameterContext",
        )?,
    })
}

fn normalize_required_encoding_text(value: Option<&str>, field: &str) -> Result<String, AppError> {
    let value = value.unwrap_or_default();
    if value.chars().any(char::is_control) {
        return Err(AppError::unprocessable(format!(
            "{field} contains invalid characters"
        )));
    }

    let value = value.trim();
    if value.is_empty() {
        return Err(AppError::unprocessable(format!("{field} is required")));
    }

    if value.chars().count() > MAX_ENCODING_TEXT_LEN {
        return Err(AppError::unprocessable(format!(
            "{field} must be at most {MAX_ENCODING_TEXT_LEN} characters"
        )));
    }

    Ok(value.to_owned())
}

fn ensure_write_body_within_limit(body: &Bytes) -> Result<(), AppError> {
    if body.len() > MAX_ENCODING_WRITE_BODY_BYTES {
        return Err(AppError::unprocessable(format!(
            "encoding options payload must be at most {MAX_ENCODING_WRITE_BODY_BYTES} bytes"
        )));
    }

    Ok(())
}

fn current_operating_system() -> OperatingSystemDto {
    #[cfg(target_os = "windows")]
    {
        OperatingSystemDto::Windows
    }
    #[cfg(target_os = "linux")]
    {
        OperatingSystemDto::Linux
    }
    #[cfg(target_os = "macos")]
    {
        OperatingSystemDto::OSX
    }
    #[cfg(target_os = "android")]
    {
        OperatingSystemDto::Android
    }
    #[cfg(all(
        not(target_os = "windows"),
        not(target_os = "linux"),
        not(target_os = "macos"),
        not(target_os = "android")
    ))]
    {
        OperatingSystemDto::BSD
    }
}

#[cfg(test)]
mod tests {
    use axum::extract::Query;
    use http::Uri;

    use super::*;

    #[test]
    fn tone_map_visibility_serializes_pascal_case_with_official_os_enum() {
        let value = serde_json::to_value(default_tone_map_visibility()).unwrap();

        assert_eq!(value["ShowAdvanced"], false);
        assert_eq!(value["IsSoftwareToneMappingAvailable"], true);
        assert_eq!(value["IsAnyHardwareToneMappingAvailable"], false);
        assert!(
            ["Windows", "Linux", "OSX", "BSD", "Android"]
                .contains(&value["OperatingSystem"].as_str().unwrap())
        );
    }

    #[test]
    fn edit_container_serializes_official_pascal_shape() {
        let value = serde_json::to_value(tone_map_edit_container("FullToneMapOptions")).unwrap();

        assert_eq!(value["TypeName"], "FullToneMapOptions");
        assert_eq!(value["Object"]["EnableToneMapping"], false);
        assert_eq!(value["DefaultObject"]["ToneMappingAlgorithm"], "none");
        assert_eq!(value["EditorRoot"], json!([]));
    }

    #[test]
    fn codec_parameter_query_requires_bounded_safe_fields() {
        let input = codec_parameter_input(&CodecParameterQuery {
            codec_id: Some(" h264 ".to_owned()),
            parameter_context: Some(" Encoding ".to_owned()),
        })
        .expect("safe codec parameter query should normalize");

        assert_eq!(input.codec_id, "h264");
        assert_eq!(input.parameter_context, "Encoding");

        assert!(
            codec_parameter_input(&CodecParameterQuery {
                parameter_context: Some("Encoding".to_owned()),
                ..CodecParameterQuery::default()
            })
            .is_err()
        );
        assert!(
            codec_parameter_input(&CodecParameterQuery {
                codec_id: Some("h264".to_owned()),
                parameter_context: Some("Encoding\n".to_owned()),
            })
            .is_err()
        );
        assert!(
            codec_parameter_input(&CodecParameterQuery {
                codec_id: Some("x".repeat(MAX_ENCODING_TEXT_LEN + 1)),
                parameter_context: Some("Encoding".to_owned()),
            })
            .is_err()
        );
    }

    #[test]
    fn codec_parameter_query_accepts_lower_camel_client_fields() {
        let uri = "/Encoding/CodecParameters?codecId=h264&parameterContext=Encoding"
            .parse::<Uri>()
            .unwrap();
        let Query(query) = Query::<CodecParameterQuery>::try_from_uri(&uri).unwrap();
        let input = codec_parameter_input(&query).unwrap();

        assert_eq!(input.codec_id, "h264");
        assert_eq!(input.parameter_context, "Encoding");
    }

    #[test]
    fn codec_parameter_container_echoes_normalized_scope() {
        let input = CodecParameterInput {
            codec_id: "h264".to_owned(),
            parameter_context: "Encoding".to_owned(),
        };
        let value = serde_json::to_value(codec_parameter_edit_container(&input)).unwrap();

        assert_eq!(value["TypeName"], "CodecParameters");
        assert_eq!(value["Object"]["CodecId"], "h264");
        assert_eq!(value["Object"]["ParameterContext"], "Encoding");
    }

    #[test]
    fn subtitle_options_container_uses_safe_defaults() {
        let value = serde_json::to_value(subtitle_options_edit_container()).unwrap();

        assert_eq!(value["TypeName"], "SubtitleOptions");
        assert_eq!(value["Object"]["EnableSubtitleExtraction"], true);
        assert_eq!(value["EditorRoot"], json!([]));
    }

    #[test]
    fn encoding_options_write_body_is_bounded() {
        assert!(ensure_write_body_within_limit(&Bytes::from_static(b"{}")).is_ok());
        assert!(
            ensure_write_body_within_limit(&Bytes::from(vec![
                b'x';
                MAX_ENCODING_WRITE_BODY_BYTES + 1
            ]))
            .is_err()
        );
    }
}
