use axum::{
    Json,
    body::Bytes,
    extract::{Path, State},
    http::{HeaderMap, StatusCode, Uri},
};
use serde::{Deserialize, Serialize};

use crate::{compat::emby::payload::parse_emby_body, error::AppError, state::AppState};

use super::access::authenticate_request_user;

const DEFAULT_DLNA_PROFILE_ID: &str = "fbz-default";
const MAX_DLNA_PROFILE_ID_LEN: usize = 128;
const MAX_DLNA_PROFILE_TEXT_LEN: usize = 256;

#[derive(Clone, Debug, Default, Deserialize, Serialize, PartialEq, Eq)]
#[serde(default, rename_all = "PascalCase")]
pub struct DlnaProfileDto {
    #[serde(rename = "Type")]
    pub profile_type: DeviceProfileTypeDto,
    pub path: Option<String>,
    pub user_id: Option<String>,
    pub album_art_pn: Option<String>,
    pub max_album_art_width: i32,
    pub max_album_art_height: i32,
    pub max_icon_width: i32,
    pub max_icon_height: i32,
    pub friendly_name: Option<String>,
    pub manufacturer: Option<String>,
    pub manufacturer_url: Option<String>,
    pub model_name: Option<String>,
    pub model_description: Option<String>,
    pub model_number: Option<String>,
    pub model_url: Option<String>,
    pub serial_number: Option<String>,
    pub enable_album_art_in_didl: bool,
    pub enable_single_album_art_limit: bool,
    pub enable_single_subtitle_limit: bool,
    pub protocol_info: Option<String>,
    pub timeline_offset_seconds: i32,
    pub requires_plain_video_items: bool,
    pub requires_plain_folders: bool,
    pub ignore_transcode_byte_range_requests: bool,
    pub supports_samsung_bookmark: bool,
    pub identification: Vec<DeviceIdentificationDto>,
    pub protocol_info_detection: ProtocolInfoDetectionDto,
    pub name: String,
    pub id: String,
    pub supported_media_types: String,
    pub max_streaming_bitrate: i64,
    pub music_streaming_transcoding_bitrate: i32,
    pub max_static_music_bitrate: i32,
    pub declared_features: Vec<String>,
    pub direct_play_profiles: Vec<DirectPlayProfileDto>,
    pub transcoding_profiles: Vec<TranscodingProfileDto>,
    pub container_profiles: Vec<ProfileConditionGroupDto>,
    pub codec_profiles: Vec<ProfileConditionGroupDto>,
    pub response_profiles: Vec<ResponseProfileDto>,
    pub subtitle_profiles: Vec<SubtitleProfileDto>,
}

#[derive(Clone, Copy, Debug, Default, Deserialize, Serialize, PartialEq, Eq)]
pub enum DeviceProfileTypeDto {
    #[default]
    System,
    User,
}

#[derive(Clone, Debug, Default, Deserialize, Serialize, PartialEq, Eq)]
#[serde(default, rename_all = "PascalCase")]
pub struct DeviceIdentificationDto {
    pub friendly_name: Option<String>,
    pub model_number: Option<String>,
    pub serial_number: Option<String>,
    pub model_name: Option<String>,
    pub model_description: Option<String>,
    pub device_description: Option<String>,
    pub model_url: Option<String>,
    pub manufacturer: Option<String>,
    pub manufacturer_url: Option<String>,
    pub headers: Vec<HttpHeaderInfoDto>,
}

#[derive(Clone, Debug, Default, Deserialize, Serialize, PartialEq, Eq)]
#[serde(default, rename_all = "PascalCase")]
pub struct HttpHeaderInfoDto {
    pub name: String,
    pub value: String,
    #[serde(rename = "Match")]
    pub header_match: HeaderMatchTypeDto,
}

#[derive(Clone, Copy, Debug, Default, Deserialize, Serialize, PartialEq, Eq)]
pub enum HeaderMatchTypeDto {
    #[default]
    Equals,
    Regex,
    Substring,
}

#[derive(Clone, Debug, Default, Deserialize, Serialize, PartialEq, Eq)]
#[serde(default, rename_all = "PascalCase")]
pub struct ProtocolInfoDetectionDto {
    pub enabled_for_video: bool,
    pub enabled_for_audio: bool,
    pub enabled_for_photos: bool,
}

#[derive(Clone, Debug, Default, Deserialize, Serialize, PartialEq, Eq)]
#[serde(default, rename_all = "PascalCase")]
pub struct DirectPlayProfileDto {
    pub container: String,
    pub audio_codec: Option<String>,
    pub video_codec: Option<String>,
    #[serde(rename = "Type")]
    pub profile_type: DlnaProfileTypeDto,
}

#[derive(Clone, Copy, Debug, Default, Deserialize, Serialize, PartialEq, Eq)]
pub enum DlnaProfileTypeDto {
    Audio,
    Video,
    #[default]
    Photo,
}

#[derive(Clone, Debug, Default, Deserialize, Serialize, PartialEq, Eq)]
#[serde(default, rename_all = "PascalCase")]
pub struct TranscodingProfileDto {
    pub container: String,
    #[serde(rename = "Type")]
    pub profile_type: DlnaProfileTypeDto,
    pub video_codec: Option<String>,
    pub audio_codec: Option<String>,
    pub protocol: String,
    pub estimate_content_length: bool,
    pub enable_mpegts_m2_ts_mode: bool,
    pub transcode_seek_info: TranscodeSeekInfoDto,
    pub copy_timestamps: bool,
    pub context: EncodingContextDto,
    pub max_audio_channels: Option<String>,
    pub min_segments: i32,
    pub segment_length: i32,
    pub break_on_non_key_frames: bool,
    pub allow_interlaced_video_stream_copy: bool,
    pub manifest_subtitles: Option<String>,
    pub max_manifest_subtitles: i32,
    pub max_width: Option<i32>,
    pub max_height: Option<i32>,
    pub fill_empty_subtitle_segments: bool,
}

#[derive(Clone, Copy, Debug, Default, Deserialize, Serialize, PartialEq, Eq)]
pub enum TranscodeSeekInfoDto {
    #[default]
    Auto,
    Bytes,
}

#[derive(Clone, Copy, Debug, Default, Deserialize, Serialize, PartialEq, Eq)]
pub enum EncodingContextDto {
    #[default]
    Streaming,
    Static,
}

#[derive(Clone, Debug, Default, Deserialize, Serialize, PartialEq, Eq)]
#[serde(default, rename_all = "PascalCase")]
pub struct ProfileConditionGroupDto {
    #[serde(rename = "Type")]
    pub profile_type: Option<String>,
    pub conditions: Vec<ProfileConditionDto>,
    pub apply_conditions: Vec<ProfileConditionDto>,
    pub container: Option<String>,
    pub codec: Option<String>,
}

#[derive(Clone, Debug, Default, Deserialize, Serialize, PartialEq, Eq)]
#[serde(default, rename_all = "PascalCase")]
pub struct ProfileConditionDto {
    pub condition: ProfileConditionTypeDto,
    pub property: String,
    pub value: String,
    pub is_required: bool,
}

#[derive(Clone, Copy, Debug, Default, Deserialize, Serialize, PartialEq, Eq)]
pub enum ProfileConditionTypeDto {
    #[default]
    Equals,
    NotEquals,
    LessThanEqual,
    GreaterThanEqual,
    EqualsAny,
}

#[derive(Clone, Debug, Default, Deserialize, Serialize, PartialEq, Eq)]
#[serde(default, rename_all = "PascalCase")]
pub struct ResponseProfileDto {
    pub container: Option<String>,
    pub audio_codec: Option<String>,
    pub video_codec: Option<String>,
    #[serde(rename = "Type")]
    pub profile_type: DlnaProfileTypeDto,
    pub org_pn: Option<String>,
    pub mime_type: Option<String>,
    pub conditions: Vec<ProfileConditionDto>,
}

#[derive(Clone, Debug, Default, Deserialize, Serialize, PartialEq, Eq)]
#[serde(default, rename_all = "PascalCase")]
pub struct SubtitleProfileDto {
    pub format: String,
    pub method: SubtitleDeliveryMethodDto,
    pub didl_mode: Option<String>,
    pub language: Option<String>,
    pub container: Option<String>,
    pub allow_chunked_response: bool,
    pub protocol: Option<String>,
}

#[derive(Clone, Copy, Debug, Default, Deserialize, Serialize, PartialEq, Eq)]
pub enum SubtitleDeliveryMethodDto {
    Encode,
    Embed,
    #[default]
    External,
    Hls,
    VideoSideData,
}

pub async fn profile_infos(
    State(state): State<AppState>,
    headers: HeaderMap,
    uri: Uri,
) -> Result<Json<Vec<DlnaProfileDto>>, AppError> {
    authenticate_admin_compatible(&state, &headers, &uri).await?;

    Ok(Json(vec![default_dlna_profile()]))
}

pub async fn default_profile(
    State(state): State<AppState>,
    headers: HeaderMap,
    uri: Uri,
) -> Result<Json<DlnaProfileDto>, AppError> {
    authenticate_admin_compatible(&state, &headers, &uri).await?;

    Ok(Json(default_dlna_profile()))
}

pub async fn profile_by_id(
    State(state): State<AppState>,
    Path(profile_id): Path<String>,
    headers: HeaderMap,
    uri: Uri,
) -> Result<Json<DlnaProfileDto>, AppError> {
    authenticate_admin_compatible(&state, &headers, &uri).await?;
    let profile_id = normalize_profile_id(&profile_id)?;
    if profile_id == DEFAULT_DLNA_PROFILE_ID {
        return Ok(Json(default_dlna_profile()));
    }

    Err(AppError::not_found("DLNA profile not found"))
}

pub async fn create_profile(
    State(state): State<AppState>,
    headers: HeaderMap,
    uri: Uri,
    body: Bytes,
) -> Result<StatusCode, AppError> {
    authenticate_admin_compatible(&state, &headers, &uri).await?;
    let profile: DlnaProfileDto = parse_emby_body(&headers, &body)?;
    validate_profile_payload(&profile)?;

    Err(dlna_profile_write_disabled_error())
}

pub async fn update_profile(
    State(state): State<AppState>,
    Path(profile_id): Path<String>,
    headers: HeaderMap,
    uri: Uri,
    body: Bytes,
) -> Result<StatusCode, AppError> {
    authenticate_admin_compatible(&state, &headers, &uri).await?;
    let profile_id = normalize_profile_id(&profile_id)?;
    let profile: DlnaProfileDto = parse_emby_body(&headers, &body)?;
    validate_profile_payload(&profile)?;
    if profile.id.trim() != profile_id {
        return Err(AppError::unprocessable(
            "profile Id does not match route Id",
        ));
    }

    Err(dlna_profile_write_disabled_error())
}

pub async fn delete_profile(
    State(state): State<AppState>,
    Path(profile_id): Path<String>,
    headers: HeaderMap,
    uri: Uri,
) -> Result<StatusCode, AppError> {
    authenticate_admin_compatible(&state, &headers, &uri).await?;
    let _profile_id = normalize_profile_id(&profile_id)?;

    Err(dlna_profile_write_disabled_error())
}

async fn authenticate_admin_compatible(
    state: &AppState,
    headers: &HeaderMap,
    uri: &Uri,
) -> Result<(), AppError> {
    let user = authenticate_request_user(state, headers, uri).await?;
    if !user.can_manage_server() {
        return Err(AppError::forbidden("server management permission required"));
    }

    Ok(())
}

fn default_dlna_profile() -> DlnaProfileDto {
    DlnaProfileDto {
        profile_type: DeviceProfileTypeDto::System,
        path: None,
        user_id: None,
        album_art_pn: Some("JPEG_TN".to_owned()),
        max_album_art_width: 1024,
        max_album_art_height: 1024,
        max_icon_width: 48,
        max_icon_height: 48,
        friendly_name: Some("FBZ DLNA".to_owned()),
        manufacturer: Some("FBZ".to_owned()),
        manufacturer_url: None,
        model_name: Some("FBZ API Server".to_owned()),
        model_description: Some("FBZ Emby-compatible DLNA profile".to_owned()),
        model_number: Some(env!("CARGO_PKG_VERSION").to_owned()),
        model_url: None,
        serial_number: None,
        enable_album_art_in_didl: true,
        enable_single_album_art_limit: false,
        enable_single_subtitle_limit: true,
        protocol_info: None,
        timeline_offset_seconds: 0,
        requires_plain_video_items: false,
        requires_plain_folders: false,
        ignore_transcode_byte_range_requests: false,
        supports_samsung_bookmark: false,
        identification: vec![DeviceIdentificationDto {
            friendly_name: Some("FBZ DLNA".to_owned()),
            manufacturer: Some("FBZ".to_owned()),
            model_name: Some("FBZ API Server".to_owned()),
            ..DeviceIdentificationDto::default()
        }],
        protocol_info_detection: ProtocolInfoDetectionDto {
            enabled_for_video: true,
            enabled_for_audio: true,
            enabled_for_photos: true,
        },
        name: "FBZ Default".to_owned(),
        id: DEFAULT_DLNA_PROFILE_ID.to_owned(),
        supported_media_types: "Audio,Photo,Video".to_owned(),
        max_streaming_bitrate: 120_000_000,
        music_streaming_transcoding_bitrate: 320_000,
        max_static_music_bitrate: 320_000,
        declared_features: Vec::new(),
        direct_play_profiles: vec![
            DirectPlayProfileDto {
                container: "mp3,flac,m4a,wav".to_owned(),
                audio_codec: Some("mp3,flac,aac,pcm".to_owned()),
                video_codec: None,
                profile_type: DlnaProfileTypeDto::Audio,
            },
            DirectPlayProfileDto {
                container: "mp4,mkv,ts".to_owned(),
                audio_codec: Some("aac,ac3,mp3".to_owned()),
                video_codec: Some("h264,hevc,mpeg2video".to_owned()),
                profile_type: DlnaProfileTypeDto::Video,
            },
            DirectPlayProfileDto {
                container: "jpg,jpeg,png".to_owned(),
                audio_codec: None,
                video_codec: None,
                profile_type: DlnaProfileTypeDto::Photo,
            },
        ],
        transcoding_profiles: vec![
            TranscodingProfileDto {
                container: "ts".to_owned(),
                profile_type: DlnaProfileTypeDto::Video,
                video_codec: Some("h264".to_owned()),
                audio_codec: Some("aac".to_owned()),
                protocol: "hls".to_owned(),
                estimate_content_length: false,
                enable_mpegts_m2_ts_mode: false,
                transcode_seek_info: TranscodeSeekInfoDto::Auto,
                copy_timestamps: false,
                context: EncodingContextDto::Streaming,
                max_audio_channels: Some("2".to_owned()),
                min_segments: 1,
                segment_length: 6,
                break_on_non_key_frames: false,
                allow_interlaced_video_stream_copy: false,
                manifest_subtitles: Some("vtt".to_owned()),
                max_manifest_subtitles: 1,
                max_width: None,
                max_height: None,
                fill_empty_subtitle_segments: true,
            },
            TranscodingProfileDto {
                container: "mp3".to_owned(),
                profile_type: DlnaProfileTypeDto::Audio,
                video_codec: None,
                audio_codec: Some("mp3".to_owned()),
                protocol: "http".to_owned(),
                estimate_content_length: true,
                context: EncodingContextDto::Streaming,
                transcode_seek_info: TranscodeSeekInfoDto::Auto,
                max_audio_channels: Some("2".to_owned()),
                ..TranscodingProfileDto::default()
            },
        ],
        container_profiles: Vec::new(),
        codec_profiles: Vec::new(),
        response_profiles: Vec::new(),
        subtitle_profiles: vec![
            SubtitleProfileDto {
                format: "srt".to_owned(),
                method: SubtitleDeliveryMethodDto::External,
                allow_chunked_response: true,
                ..SubtitleProfileDto::default()
            },
            SubtitleProfileDto {
                format: "vtt".to_owned(),
                method: SubtitleDeliveryMethodDto::Hls,
                protocol: Some("hls".to_owned()),
                allow_chunked_response: true,
                ..SubtitleProfileDto::default()
            },
        ],
    }
}

fn validate_profile_payload(profile: &DlnaProfileDto) -> Result<(), AppError> {
    normalize_profile_text(&profile.name, "Name")?;
    normalize_profile_id(&profile.id)?;
    validate_optional_profile_text(profile.friendly_name.as_deref(), "FriendlyName")?;
    validate_optional_profile_text(profile.manufacturer.as_deref(), "Manufacturer")?;
    validate_optional_profile_text(profile.model_name.as_deref(), "ModelName")?;
    Ok(())
}

fn normalize_profile_id(value: &str) -> Result<String, AppError> {
    let value = value.trim();
    if value.is_empty() {
        return Err(AppError::unprocessable("profile Id is required"));
    }
    if value.len() > MAX_DLNA_PROFILE_ID_LEN
        || !value
            .chars()
            .all(|ch| ch.is_ascii_alphanumeric() || matches!(ch, '-' | '_' | '.'))
    {
        return Err(AppError::unprocessable("profile Id is invalid"));
    }

    Ok(value.to_owned())
}

fn normalize_profile_text(value: &str, field: &'static str) -> Result<String, AppError> {
    let value = value.trim();
    if value.is_empty() {
        return Err(AppError::unprocessable(format!("{field} is required")));
    }
    if value.len() > MAX_DLNA_PROFILE_TEXT_LEN || value.chars().any(char::is_control) {
        return Err(AppError::unprocessable(format!("{field} is invalid")));
    }

    Ok(value.to_owned())
}

fn validate_optional_profile_text(
    value: Option<&str>,
    field: &'static str,
) -> Result<(), AppError> {
    if let Some(value) = value {
        normalize_profile_text(value, field)?;
    }
    Ok(())
}

fn dlna_profile_write_disabled_error() -> AppError {
    AppError::conflict("Emby DLNA profile writes are disabled; use FBZ server configuration APIs")
}

#[cfg(test)]
mod tests {
    use axum::http::StatusCode;

    use super::*;

    #[test]
    fn default_dlna_profile_serializes_pascal_case_with_official_enums() {
        let value = serde_json::to_value(default_dlna_profile()).unwrap();

        assert_eq!(value["Type"], "System");
        assert_eq!(value["Name"], "FBZ Default");
        assert_eq!(value["Id"], DEFAULT_DLNA_PROFILE_ID);
        assert_eq!(value["SupportedMediaTypes"], "Audio,Photo,Video");
        assert_eq!(value["ProtocolInfoDetection"]["EnabledForVideo"], true);
        assert_eq!(value["DirectPlayProfiles"][0]["Type"], "Audio");
        assert_eq!(value["TranscodingProfiles"][0]["Type"], "Video");
        assert_eq!(value["SubtitleProfiles"][1]["Method"], "Hls");
    }

    #[test]
    fn profile_lookup_accepts_only_default_profile() {
        assert_eq!(
            normalize_profile_id(" fbz-default ").unwrap(),
            DEFAULT_DLNA_PROFILE_ID
        );
        assert_eq!(
            normalize_profile_id("bad/id").unwrap_err().status_code(),
            StatusCode::UNPROCESSABLE_ENTITY
        );
        assert_eq!(
            normalize_profile_id(&"x".repeat(MAX_DLNA_PROFILE_ID_LEN + 1))
                .unwrap_err()
                .status_code(),
            StatusCode::UNPROCESSABLE_ENTITY
        );
    }

    #[test]
    fn profile_payload_validation_requires_safe_name_and_id() {
        let mut profile = default_dlna_profile();
        profile.id = "custom-profile".to_owned();
        profile.name = " Custom Profile ".to_owned();

        assert!(validate_profile_payload(&profile).is_ok());

        profile.id = "bad/id".to_owned();
        assert_eq!(
            validate_profile_payload(&profile)
                .unwrap_err()
                .status_code(),
            StatusCode::UNPROCESSABLE_ENTITY
        );

        profile.id = "custom-profile".to_owned();
        profile.name = "bad\nname".to_owned();
        assert_eq!(
            validate_profile_payload(&profile)
                .unwrap_err()
                .status_code(),
            StatusCode::UNPROCESSABLE_ENTITY
        );
    }
}
