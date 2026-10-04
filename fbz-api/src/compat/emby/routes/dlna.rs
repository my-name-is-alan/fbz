use axum::{
    Json,
    body::Bytes,
    extract::{Path, State},
    http::{HeaderMap, StatusCode, Uri},
};
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::{
    auth::service::AuthenticatedUser, compat::emby::payload::parse_emby_body, error::AppError,
    settings::repository::SettingsRepository, state::AppState,
};

use super::access::authenticate_request_user;

const DEFAULT_DLNA_PROFILE_ID: &str = "fbz-default";
const MAX_DLNA_PROFILE_ID_LEN: usize = 128;
const MAX_DLNA_PROFILE_TEXT_LEN: usize = 256;
/// 自定义 DLNA profile（JSON 数组）在 server_settings 中的键。
const DLNA_PROFILES_SETTING_KEY: &str = "emby.dlna.profiles";
const MAX_CUSTOM_DLNA_PROFILES: usize = 64;

#[derive(Clone, Debug, Default, Deserialize, Serialize, PartialEq, Eq)]
#[serde(default, rename_all = "PascalCase")]
pub struct DlnaProfileDto {
    #[serde(rename = "Type")]
    pub profile_type: DeviceProfileTypeDto,
    #[serde(alias = "path")]
    pub path: Option<String>,
    #[serde(alias = "userId", alias = "user_id")]
    pub user_id: Option<String>,
    #[serde(alias = "albumArtPn", alias = "album_art_pn")]
    pub album_art_pn: Option<String>,
    #[serde(alias = "maxAlbumArtWidth", alias = "max_album_art_width")]
    pub max_album_art_width: i32,
    #[serde(alias = "maxAlbumArtHeight", alias = "max_album_art_height")]
    pub max_album_art_height: i32,
    #[serde(alias = "maxIconWidth", alias = "max_icon_width")]
    pub max_icon_width: i32,
    #[serde(alias = "maxIconHeight", alias = "max_icon_height")]
    pub max_icon_height: i32,
    #[serde(alias = "friendlyName", alias = "friendly_name")]
    pub friendly_name: Option<String>,
    #[serde(alias = "manufacturer")]
    pub manufacturer: Option<String>,
    #[serde(alias = "manufacturerUrl", alias = "manufacturer_url")]
    pub manufacturer_url: Option<String>,
    #[serde(alias = "modelName", alias = "model_name")]
    pub model_name: Option<String>,
    #[serde(alias = "modelDescription", alias = "model_description")]
    pub model_description: Option<String>,
    #[serde(alias = "modelNumber", alias = "model_number")]
    pub model_number: Option<String>,
    #[serde(alias = "modelUrl", alias = "model_url")]
    pub model_url: Option<String>,
    #[serde(alias = "serialNumber", alias = "serial_number")]
    pub serial_number: Option<String>,
    #[serde(alias = "enableAlbumArtInDidl", alias = "enable_album_art_in_didl")]
    pub enable_album_art_in_didl: bool,
    #[serde(
        alias = "enableSingleAlbumArtLimit",
        alias = "enable_single_album_art_limit"
    )]
    pub enable_single_album_art_limit: bool,
    #[serde(
        alias = "enableSingleSubtitleLimit",
        alias = "enable_single_subtitle_limit"
    )]
    pub enable_single_subtitle_limit: bool,
    #[serde(alias = "protocolInfo", alias = "protocol_info")]
    pub protocol_info: Option<String>,
    #[serde(alias = "timelineOffsetSeconds", alias = "timeline_offset_seconds")]
    pub timeline_offset_seconds: i32,
    #[serde(
        alias = "requiresPlainVideoItems",
        alias = "requires_plain_video_items"
    )]
    pub requires_plain_video_items: bool,
    #[serde(alias = "requiresPlainFolders", alias = "requires_plain_folders")]
    pub requires_plain_folders: bool,
    #[serde(
        alias = "ignoreTranscodeByteRangeRequests",
        alias = "ignore_transcode_byte_range_requests"
    )]
    pub ignore_transcode_byte_range_requests: bool,
    #[serde(alias = "supportsSamsungBookmark", alias = "supports_samsung_bookmark")]
    pub supports_samsung_bookmark: bool,
    #[serde(alias = "identification")]
    pub identification: Vec<DeviceIdentificationDto>,
    #[serde(alias = "protocolInfoDetection", alias = "protocol_info_detection")]
    pub protocol_info_detection: ProtocolInfoDetectionDto,
    #[serde(alias = "name")]
    pub name: String,
    #[serde(alias = "id")]
    pub id: String,
    #[serde(alias = "supportedMediaTypes", alias = "supported_media_types")]
    pub supported_media_types: String,
    #[serde(alias = "maxStreamingBitrate", alias = "max_streaming_bitrate")]
    pub max_streaming_bitrate: i64,
    #[serde(
        alias = "musicStreamingTranscodingBitrate",
        alias = "music_streaming_transcoding_bitrate"
    )]
    pub music_streaming_transcoding_bitrate: i32,
    #[serde(alias = "maxStaticMusicBitrate", alias = "max_static_music_bitrate")]
    pub max_static_music_bitrate: i32,
    #[serde(alias = "declaredFeatures", alias = "declared_features")]
    pub declared_features: Vec<String>,
    #[serde(alias = "directPlayProfiles", alias = "direct_play_profiles")]
    pub direct_play_profiles: Vec<DirectPlayProfileDto>,
    #[serde(alias = "transcodingProfiles", alias = "transcoding_profiles")]
    pub transcoding_profiles: Vec<TranscodingProfileDto>,
    #[serde(alias = "containerProfiles", alias = "container_profiles")]
    pub container_profiles: Vec<ProfileConditionGroupDto>,
    #[serde(alias = "codecProfiles", alias = "codec_profiles")]
    pub codec_profiles: Vec<ProfileConditionGroupDto>,
    #[serde(alias = "responseProfiles", alias = "response_profiles")]
    pub response_profiles: Vec<ResponseProfileDto>,
    #[serde(alias = "subtitleProfiles", alias = "subtitle_profiles")]
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
    #[serde(alias = "friendlyName", alias = "friendly_name")]
    pub friendly_name: Option<String>,
    #[serde(alias = "modelNumber", alias = "model_number")]
    pub model_number: Option<String>,
    #[serde(alias = "serialNumber", alias = "serial_number")]
    pub serial_number: Option<String>,
    #[serde(alias = "modelName", alias = "model_name")]
    pub model_name: Option<String>,
    #[serde(alias = "modelDescription", alias = "model_description")]
    pub model_description: Option<String>,
    #[serde(alias = "deviceDescription", alias = "device_description")]
    pub device_description: Option<String>,
    #[serde(alias = "modelUrl", alias = "model_url")]
    pub model_url: Option<String>,
    #[serde(alias = "manufacturer")]
    pub manufacturer: Option<String>,
    #[serde(alias = "manufacturerUrl", alias = "manufacturer_url")]
    pub manufacturer_url: Option<String>,
    #[serde(alias = "headers")]
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
    #[serde(alias = "enabledForVideo", alias = "enabled_for_video")]
    pub enabled_for_video: bool,
    #[serde(alias = "enabledForAudio", alias = "enabled_for_audio")]
    pub enabled_for_audio: bool,
    #[serde(alias = "enabledForPhotos", alias = "enabled_for_photos")]
    pub enabled_for_photos: bool,
}

#[derive(Clone, Debug, Default, Deserialize, Serialize, PartialEq, Eq)]
#[serde(default, rename_all = "PascalCase")]
pub struct DirectPlayProfileDto {
    #[serde(alias = "container")]
    pub container: String,
    #[serde(alias = "audioCodec", alias = "audio_codec")]
    pub audio_codec: Option<String>,
    #[serde(alias = "videoCodec", alias = "video_codec")]
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
    #[serde(alias = "container")]
    pub container: String,
    #[serde(rename = "Type")]
    pub profile_type: DlnaProfileTypeDto,
    #[serde(alias = "videoCodec", alias = "video_codec")]
    pub video_codec: Option<String>,
    #[serde(alias = "audioCodec", alias = "audio_codec")]
    pub audio_codec: Option<String>,
    #[serde(alias = "protocol")]
    pub protocol: String,
    #[serde(alias = "estimateContentLength", alias = "estimate_content_length")]
    pub estimate_content_length: bool,
    #[serde(alias = "enableMpegtsM2TsMode", alias = "enable_mpegts_m2_ts_mode")]
    pub enable_mpegts_m2_ts_mode: bool,
    #[serde(alias = "transcodeSeekInfo", alias = "transcode_seek_info")]
    pub transcode_seek_info: TranscodeSeekInfoDto,
    #[serde(alias = "copyTimestamps", alias = "copy_timestamps")]
    pub copy_timestamps: bool,
    #[serde(alias = "context")]
    pub context: EncodingContextDto,
    #[serde(alias = "maxAudioChannels", alias = "max_audio_channels")]
    pub max_audio_channels: Option<String>,
    #[serde(alias = "minSegments", alias = "min_segments")]
    pub min_segments: i32,
    #[serde(alias = "segmentLength", alias = "segment_length")]
    pub segment_length: i32,
    #[serde(alias = "breakOnNonKeyFrames", alias = "break_on_non_key_frames")]
    pub break_on_non_key_frames: bool,
    #[serde(
        alias = "allowInterlacedVideoStreamCopy",
        alias = "allow_interlaced_video_stream_copy"
    )]
    pub allow_interlaced_video_stream_copy: bool,
    #[serde(alias = "manifestSubtitles", alias = "manifest_subtitles")]
    pub manifest_subtitles: Option<String>,
    #[serde(alias = "maxManifestSubtitles", alias = "max_manifest_subtitles")]
    pub max_manifest_subtitles: i32,
    #[serde(alias = "maxWidth", alias = "max_width")]
    pub max_width: Option<i32>,
    #[serde(alias = "maxHeight", alias = "max_height")]
    pub max_height: Option<i32>,
    #[serde(
        alias = "fillEmptySubtitleSegments",
        alias = "fill_empty_subtitle_segments"
    )]
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
    #[serde(alias = "conditions")]
    pub conditions: Vec<ProfileConditionDto>,
    #[serde(alias = "applyConditions", alias = "apply_conditions")]
    pub apply_conditions: Vec<ProfileConditionDto>,
    #[serde(alias = "container")]
    pub container: Option<String>,
    #[serde(alias = "codec")]
    pub codec: Option<String>,
}

#[derive(Clone, Debug, Default, Deserialize, Serialize, PartialEq, Eq)]
#[serde(default, rename_all = "PascalCase")]
pub struct ProfileConditionDto {
    #[serde(alias = "condition")]
    pub condition: ProfileConditionTypeDto,
    #[serde(alias = "property")]
    pub property: String,
    #[serde(alias = "value")]
    pub value: String,
    #[serde(alias = "isRequired", alias = "is_required")]
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
    #[serde(alias = "container")]
    pub container: Option<String>,
    #[serde(alias = "audioCodec", alias = "audio_codec")]
    pub audio_codec: Option<String>,
    #[serde(alias = "videoCodec", alias = "video_codec")]
    pub video_codec: Option<String>,
    #[serde(rename = "Type")]
    pub profile_type: DlnaProfileTypeDto,
    #[serde(alias = "orgPn", alias = "org_pn")]
    pub org_pn: Option<String>,
    #[serde(alias = "mimeType", alias = "mime_type")]
    pub mime_type: Option<String>,
    #[serde(alias = "conditions")]
    pub conditions: Vec<ProfileConditionDto>,
}

#[derive(Clone, Debug, Default, Deserialize, Serialize, PartialEq, Eq)]
#[serde(default, rename_all = "PascalCase")]
pub struct SubtitleProfileDto {
    #[serde(alias = "format")]
    pub format: String,
    #[serde(alias = "method")]
    pub method: SubtitleDeliveryMethodDto,
    #[serde(alias = "didlMode", alias = "didl_mode")]
    pub didl_mode: Option<String>,
    #[serde(alias = "language")]
    pub language: Option<String>,
    #[serde(alias = "container")]
    pub container: Option<String>,
    #[serde(alias = "allowChunkedResponse", alias = "allow_chunked_response")]
    pub allow_chunked_response: bool,
    #[serde(alias = "protocol")]
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
) -> Result<Json<Vec<Value>>, AppError> {
    authenticate_admin_compatible(&state, &headers, &uri).await?;

    let mut profiles = vec![
        serde_json::to_value(default_dlna_profile())
            .map_err(|err| AppError::internal(format!("failed to serialize profile: {err}")))?,
    ];
    profiles.extend(load_custom_profiles(&state).await?);

    Ok(Json(profiles))
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
) -> Result<Json<Value>, AppError> {
    authenticate_admin_compatible(&state, &headers, &uri).await?;
    let profile_id = normalize_profile_id(&profile_id)?;
    if profile_id == DEFAULT_DLNA_PROFILE_ID {
        return Ok(Json(serde_json::to_value(default_dlna_profile()).map_err(
            |err| AppError::internal(format!("failed to serialize profile: {err}")),
        )?));
    }

    let profiles = load_custom_profiles(&state).await?;
    profiles
        .into_iter()
        .find(|profile| stored_profile_id(profile) == Some(profile_id.as_str()))
        .map(Json)
        .ok_or_else(|| AppError::not_found("DLNA profile not found"))
}

pub async fn create_profile(
    State(state): State<AppState>,
    headers: HeaderMap,
    uri: Uri,
    body: Bytes,
) -> Result<StatusCode, AppError> {
    let user = authenticate_admin_compatible(&state, &headers, &uri).await?;
    let profile: DlnaProfileDto = parse_emby_body(&headers, &body)?;
    validate_profile_payload(&profile)?;
    let mut raw: Value = parse_emby_body(&headers, &body)?;

    let mut profiles = load_custom_profiles(&state).await?;
    if profiles.len() >= MAX_CUSTOM_DLNA_PROFILES {
        return Err(AppError::unprocessable("too many custom DLNA profiles"));
    }

    // 生成稳定自定义 id；客户端提供的 Id 一律覆盖（防与默认/既有冲突）。
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|duration| duration.as_nanos())
        .unwrap_or_default();
    let profile_id = format!("custom-{nanos}");
    if let Value::Object(object) = &mut raw {
        object.insert("Id".to_owned(), Value::String(profile_id));
        object.insert("Type".to_owned(), Value::String("User".to_owned()));
    }
    profiles.push(raw);
    store_custom_profiles(&state, profiles, &user).await?;

    Ok(StatusCode::NO_CONTENT)
}

pub async fn update_profile(
    State(state): State<AppState>,
    Path(profile_id): Path<String>,
    headers: HeaderMap,
    uri: Uri,
    body: Bytes,
) -> Result<StatusCode, AppError> {
    let user = authenticate_admin_compatible(&state, &headers, &uri).await?;
    let profile_id = normalize_profile_id(&profile_id)?;
    let profile: DlnaProfileDto = parse_emby_body(&headers, &body)?;
    validate_profile_payload(&profile)?;
    if profile.id.trim() != profile_id {
        return Err(AppError::unprocessable(
            "profile Id does not match route Id",
        ));
    }
    if profile_id == DEFAULT_DLNA_PROFILE_ID {
        return Err(AppError::conflict(
            "the built-in DLNA profile cannot be modified",
        ));
    }
    let raw: Value = parse_emby_body(&headers, &body)?;

    let mut profiles = load_custom_profiles(&state).await?;
    let Some(slot) = profiles
        .iter_mut()
        .find(|stored| stored_profile_id(stored) == Some(profile_id.as_str()))
    else {
        return Err(AppError::not_found("DLNA profile not found"));
    };
    *slot = raw;
    store_custom_profiles(&state, profiles, &user).await?;

    Ok(StatusCode::NO_CONTENT)
}

pub async fn delete_profile(
    State(state): State<AppState>,
    Path(profile_id): Path<String>,
    headers: HeaderMap,
    uri: Uri,
) -> Result<StatusCode, AppError> {
    let user = authenticate_admin_compatible(&state, &headers, &uri).await?;
    let profile_id = normalize_profile_id(&profile_id)?;
    if profile_id == DEFAULT_DLNA_PROFILE_ID {
        return Err(AppError::conflict(
            "the built-in DLNA profile cannot be deleted",
        ));
    }

    let mut profiles = load_custom_profiles(&state).await?;
    let before = profiles.len();
    profiles.retain(|stored| stored_profile_id(stored) != Some(profile_id.as_str()));
    if profiles.len() == before {
        return Err(AppError::not_found("DLNA profile not found"));
    }
    store_custom_profiles(&state, profiles, &user).await?;

    Ok(StatusCode::NO_CONTENT)
}

fn stored_profile_id(profile: &Value) -> Option<&str> {
    profile.get("Id").and_then(Value::as_str)
}

async fn load_custom_profiles(state: &AppState) -> Result<Vec<Value>, AppError> {
    let Some(database) = state.database() else {
        return Err(AppError::internal("database is not configured"));
    };

    Ok(SettingsRepository::new(database.clone())
        .get(DLNA_PROFILES_SETTING_KEY)
        .await
        .map_err(|err| AppError::internal(format!("failed to load DLNA profiles: {err}")))?
        .and_then(|setting| setting.value.as_array().cloned())
        .unwrap_or_default())
}

async fn store_custom_profiles(
    state: &AppState,
    profiles: Vec<Value>,
    user: &AuthenticatedUser,
) -> Result<(), AppError> {
    let Some(database) = state.database() else {
        return Err(AppError::internal("database is not configured"));
    };

    SettingsRepository::new(database.clone())
        .update_admin_setting(
            DLNA_PROFILES_SETTING_KEY,
            Value::Array(profiles),
            &user.username,
            Some("emby dlna profile update"),
        )
        .await
        .map_err(|err| AppError::internal(format!("failed to store DLNA profiles: {err}")))?;

    Ok(())
}

async fn authenticate_admin_compatible(
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

    #[test]
    fn dlna_profile_body_accepts_lower_camel_and_snake_case_fields() {
        let lower_camel = serde_json::from_value::<DlnaProfileDto>(serde_json::json!({
            "Type": "User",
            "Name": "Custom Profile",
            "Id": "custom-profile",
            "friendlyName": "Living Room",
            "manufacturerUrl": "https://example.test",
            "modelName": "FBZ Client",
            "modelDescription": "Test profile",
            "modelNumber": "1",
            "enableAlbumArtInDidl": true,
            "timelineOffsetSeconds": 7,
            "supportedMediaTypes": "Audio,Video",
            "maxStreamingBitrate": 1000000,
            "musicStreamingTranscodingBitrate": 128000,
            "protocolInfoDetection": {
                "enabledForVideo": true,
                "enabledForAudio": true,
                "enabledForPhotos": false
            },
            "identification": [{
                "friendlyName": "FBZ Client",
                "modelName": "Model",
                "manufacturerUrl": "https://example.test",
                "headers": [{
                    "Name": "User-Agent",
                    "Value": "Client",
                    "Match": "Substring"
                }]
            }],
            "directPlayProfiles": [{
                "container": "mp4",
                "audioCodec": "aac",
                "videoCodec": "h264",
                "Type": "Video"
            }],
            "transcodingProfiles": [{
                "container": "ts",
                "Type": "Video",
                "audioCodec": "aac",
                "videoCodec": "h264",
                "estimateContentLength": true,
                "copyTimestamps": true,
                "maxAudioChannels": "2",
                "minSegments": 2,
                "segmentLength": 6,
                "manifestSubtitles": "vtt",
                "maxManifestSubtitles": 1,
                "fillEmptySubtitleSegments": true
            }],
            "responseProfiles": [{
                "container": "mp4",
                "audioCodec": "aac",
                "videoCodec": "h264",
                "orgPn": "AVC_MP4",
                "mimeType": "video/mp4"
            }],
            "subtitleProfiles": [{
                "format": "srt",
                "method": "External",
                "didlMode": "CaptionInfoEx",
                "allowChunkedResponse": true
            }]
        }))
        .expect("lower-camel DLNA profile should deserialize");

        assert_eq!(lower_camel.profile_type, DeviceProfileTypeDto::User);
        assert_eq!(lower_camel.friendly_name.as_deref(), Some("Living Room"));
        assert_eq!(
            lower_camel.manufacturer_url.as_deref(),
            Some("https://example.test")
        );
        assert_eq!(lower_camel.model_name.as_deref(), Some("FBZ Client"));
        assert!(lower_camel.enable_album_art_in_didl);
        assert_eq!(lower_camel.timeline_offset_seconds, 7);
        assert_eq!(lower_camel.supported_media_types, "Audio,Video");
        assert_eq!(lower_camel.max_streaming_bitrate, 1_000_000);
        assert_eq!(lower_camel.music_streaming_transcoding_bitrate, 128_000);
        assert!(lower_camel.protocol_info_detection.enabled_for_video);
        assert!(lower_camel.protocol_info_detection.enabled_for_audio);
        assert!(!lower_camel.protocol_info_detection.enabled_for_photos);
        assert_eq!(
            lower_camel.identification[0].friendly_name.as_deref(),
            Some("FBZ Client")
        );
        assert_eq!(
            lower_camel.direct_play_profiles[0].audio_codec.as_deref(),
            Some("aac")
        );
        assert!(lower_camel.transcoding_profiles[0].estimate_content_length);
        assert!(lower_camel.transcoding_profiles[0].copy_timestamps);
        assert_eq!(
            lower_camel.response_profiles[0].mime_type.as_deref(),
            Some("video/mp4")
        );
        assert!(lower_camel.subtitle_profiles[0].allow_chunked_response);

        let snake_case = serde_json::from_value::<DlnaProfileDto>(serde_json::json!({
            "Type": "User",
            "Name": "Snake Profile",
            "Id": "snake-profile",
            "friendly_name": "Den",
            "manufacturer_url": "https://example.test",
            "model_name": "Snake Client",
            "supported_media_types": "Audio",
            "max_streaming_bitrate": 640000,
            "protocol_info_detection": {
                "enabled_for_video": false,
                "enabled_for_audio": true,
                "enabled_for_photos": false
            },
            "direct_play_profiles": [{
                "container": "mp3",
                "audio_codec": "mp3",
                "Type": "Audio"
            }]
        }))
        .expect("snake-case DLNA profile should deserialize");

        assert_eq!(snake_case.friendly_name.as_deref(), Some("Den"));
        assert_eq!(snake_case.model_name.as_deref(), Some("Snake Client"));
        assert_eq!(snake_case.supported_media_types, "Audio");
        assert_eq!(snake_case.max_streaming_bitrate, 640_000);
        assert!(!snake_case.protocol_info_detection.enabled_for_video);
        assert!(snake_case.protocol_info_detection.enabled_for_audio);
        assert_eq!(
            snake_case.direct_play_profiles[0].audio_codec.as_deref(),
            Some("mp3")
        );
    }
}
