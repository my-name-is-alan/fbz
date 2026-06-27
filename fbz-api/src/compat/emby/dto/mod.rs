use std::collections::BTreeMap;

use serde::{Deserialize, Deserializer, Serialize, de};
use serde_json::Value;

pub(crate) fn deserialize_string_list<'de, D>(deserializer: D) -> Result<Vec<String>, D::Error>
where
    D: Deserializer<'de>,
{
    struct StringListVisitor;

    impl<'de> de::Visitor<'de> for StringListVisitor {
        type Value = Vec<String>;

        fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            formatter.write_str("a string or list of strings")
        }

        fn visit_str<E>(self, value: &str) -> Result<Self::Value, E>
        where
            E: de::Error,
        {
            Ok(split_string_list(value))
        }

        fn visit_string<E>(self, value: String) -> Result<Self::Value, E>
        where
            E: de::Error,
        {
            Ok(split_string_list(&value))
        }

        fn visit_seq<A>(self, mut sequence: A) -> Result<Self::Value, A::Error>
        where
            A: de::SeqAccess<'de>,
        {
            let mut values = Vec::new();
            while let Some(value) = sequence.next_element::<String>()? {
                values.extend(split_string_list(&value));
            }
            Ok(values)
        }
    }

    deserializer.deserialize_any(StringListVisitor)
}

fn split_string_list(value: &str) -> Vec<String> {
    value
        .split(',')
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_owned)
        .collect()
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ServerInfoSource {
    pub id: String,
    pub server_name: String,
    pub version: String,
    pub local_address: String,
    pub operating_system: String,
}

#[derive(Clone, Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "PascalCase")]
pub struct SystemInfoDto {
    pub id: String,
    pub server_name: String,
    pub version: String,
    pub local_address: String,
    pub operating_system: String,
    pub supports_library_monitor: bool,
}

impl From<ServerInfoSource> for SystemInfoDto {
    fn from(source: ServerInfoSource) -> Self {
        Self {
            id: source.id,
            server_name: source.server_name,
            version: source.version,
            local_address: source.local_address,
            operating_system: source.operating_system,
            supports_library_monitor: false,
        }
    }
}

#[derive(Clone, Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "PascalCase")]
pub struct PublicSystemInfoDto {
    pub id: String,
    pub server_name: String,
    pub version: String,
    pub local_address: String,
    pub local_addresses: Vec<String>,
    pub wan_address: String,
    pub remote_addresses: Vec<String>,
}

impl From<ServerInfoSource> for PublicSystemInfoDto {
    fn from(source: ServerInfoSource) -> Self {
        let public_address = source.local_address;
        Self {
            id: source.id,
            server_name: source.server_name,
            version: source.version,
            local_address: public_address.clone(),
            local_addresses: vec![public_address.clone()],
            wan_address: public_address.clone(),
            remote_addresses: vec![public_address],
        }
    }
}

#[derive(Clone, Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "PascalCase")]
pub struct EndpointInfoDto {
    pub is_local: bool,
    pub is_in_network: bool,
}

impl EndpointInfoDto {
    pub fn conservative_default() -> Self {
        Self {
            is_local: false,
            is_in_network: false,
        }
    }
}

#[derive(Clone, Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "PascalCase")]
pub struct WakeOnLanInfoDto {
    pub mac_address: String,
    pub broadcast_address: String,
    pub port: i32,
}

#[derive(Clone, Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "PascalCase")]
pub struct LiveTvInfoDto {
    pub is_enabled: bool,
    pub enabled_users: Vec<String>,
}

impl LiveTvInfoDto {
    pub fn disabled() -> Self {
        Self {
            is_enabled: false,
            enabled_users: Vec::new(),
        }
    }
}

#[derive(Clone, Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "PascalCase")]
pub struct NameIdPairDto {
    pub name: String,
    pub id: String,
}

#[derive(Clone, Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "PascalCase")]
pub struct NameValuePairDto {
    pub name: String,
    pub value: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ServerConfigurationSource {
    pub server_name: String,
    pub public_base_url: String,
    pub http_server_port_number: i32,
    pub cache_path: String,
    pub metadata_path: String,
    pub simultaneous_stream_limit: i32,
}

#[derive(Clone, Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "PascalCase")]
pub struct ServerConfigurationDto {
    #[serde(rename = "EnableUPnP")]
    pub enable_upnp: bool,
    pub public_port: i32,
    pub public_https_port: i32,
    pub http_server_port_number: i32,
    pub https_port_number: i32,
    pub enable_https: bool,
    pub certificate_path: String,
    pub certificate_password: Option<String>,
    pub is_port_authorized: bool,
    pub auto_run_web_app: bool,
    pub enable_remote_access: bool,
    pub log_all_query_times: bool,
    pub disable_outgoing_ipv6: bool,
    pub enable_case_sensitive_item_ids: bool,
    pub metadata_path: String,
    pub metadata_network_path: String,
    pub preferred_metadata_language: String,
    pub metadata_country_code: String,
    pub sort_remove_words: Vec<String>,
    pub library_monitor_delay_seconds: i32,
    pub enable_dashboard_response_caching: bool,
    pub dashboard_source_path: String,
    pub image_saving_convention: String,
    pub enable_automatic_restart: bool,
    pub server_name: String,
    pub preferred_detected_remote_address_family: String,
    #[serde(rename = "WanDdns")]
    pub wan_ddns: String,
    #[serde(rename = "UICulture")]
    pub ui_culture: String,
    pub remote_client_bitrate_limit: i32,
    pub local_network_subnets: Vec<String>,
    pub local_network_addresses: Vec<String>,
    pub enable_external_content_in_suggestions: bool,
    pub require_https: bool,
    pub is_behind_proxy: bool,
    #[serde(rename = "RemoteIPFilter")]
    pub remote_ip_filter: Vec<String>,
    #[serde(rename = "IsRemoteIPFilterBlacklist")]
    pub is_remote_ip_filter_blacklist: bool,
    pub image_extraction_timeout_ms: i32,
    pub path_substitutions: Vec<PathSubstitutionDto>,
    pub uninstalled_plugins: Vec<String>,
    pub collapse_video_folders: bool,
    pub enable_original_track_titles: bool,
    pub vacuum_database_on_startup: bool,
    pub simultaneous_stream_limit: i32,
    pub database_cache_size_mb: i32,
    #[serde(rename = "EnableSqLiteMmio")]
    pub enable_sqlite_mmio: bool,
    #[serde(rename = "PlaylistsUpgradedToM3U")]
    pub playlists_upgraded_to_m3u: bool,
    pub image_extractor_upgraded1: bool,
    pub enable_people_letter_sub_folders: bool,
    pub optimize_database_on_shutdown: bool,
    pub database_analysis_limit: i32,
    pub max_library_database_connections: i32,
    pub max_auth_db_connections: i32,
    pub max_other_db_connections: i32,
    pub disable_async_io: bool,
    pub migrated_to_user_item_shares8: bool,
    pub migrated_library_options_to_db: bool,
    pub allow_legacy_local_network_password: bool,
    pub enable_saved_metadata_for_people: bool,
    pub tv_channels_refreshed: bool,
    pub proxy_header_mode: String,
    pub is_in_maintenance_mode: bool,
    pub maintenance_mode_message: String,
    pub enable_debug_level_logging: bool,
    pub revert_debug_logging: String,
    pub enable_auto_update: bool,
    pub log_file_retention_days: i32,
    pub run_at_startup: bool,
    pub is_startup_wizard_completed: bool,
    pub cache_path: String,
}

impl From<ServerConfigurationSource> for ServerConfigurationDto {
    fn from(source: ServerConfigurationSource) -> Self {
        let enable_https = source.public_base_url.starts_with("https://");

        Self {
            enable_upnp: false,
            public_port: source.http_server_port_number,
            public_https_port: if enable_https {
                source.http_server_port_number
            } else {
                0
            },
            http_server_port_number: source.http_server_port_number,
            https_port_number: if enable_https {
                source.http_server_port_number
            } else {
                0
            },
            enable_https,
            certificate_path: String::new(),
            certificate_password: None,
            is_port_authorized: true,
            auto_run_web_app: false,
            enable_remote_access: true,
            log_all_query_times: false,
            disable_outgoing_ipv6: false,
            enable_case_sensitive_item_ids: false,
            metadata_path: source.metadata_path,
            metadata_network_path: String::new(),
            preferred_metadata_language: "zh-CN".to_owned(),
            metadata_country_code: "CN".to_owned(),
            sort_remove_words: vec![
                "the".to_owned(),
                "a".to_owned(),
                "an".to_owned(),
                "das".to_owned(),
                "der".to_owned(),
                "die".to_owned(),
            ],
            library_monitor_delay_seconds: 60,
            enable_dashboard_response_caching: true,
            dashboard_source_path: String::new(),
            image_saving_convention: "Compatible".to_owned(),
            enable_automatic_restart: false,
            server_name: source.server_name,
            preferred_detected_remote_address_family: "InterNetwork".to_owned(),
            wan_ddns: source.public_base_url,
            ui_culture: "zh-CN".to_owned(),
            remote_client_bitrate_limit: 0,
            local_network_subnets: Vec::new(),
            local_network_addresses: Vec::new(),
            enable_external_content_in_suggestions: true,
            require_https: false,
            is_behind_proxy: false,
            remote_ip_filter: Vec::new(),
            is_remote_ip_filter_blacklist: false,
            image_extraction_timeout_ms: 0,
            path_substitutions: Vec::new(),
            uninstalled_plugins: Vec::new(),
            collapse_video_folders: false,
            enable_original_track_titles: false,
            vacuum_database_on_startup: false,
            simultaneous_stream_limit: source.simultaneous_stream_limit,
            database_cache_size_mb: 0,
            enable_sqlite_mmio: false,
            playlists_upgraded_to_m3u: true,
            image_extractor_upgraded1: true,
            enable_people_letter_sub_folders: false,
            optimize_database_on_shutdown: false,
            database_analysis_limit: 0,
            max_library_database_connections: 0,
            max_auth_db_connections: 0,
            max_other_db_connections: 0,
            disable_async_io: false,
            migrated_to_user_item_shares8: true,
            migrated_library_options_to_db: true,
            allow_legacy_local_network_password: false,
            enable_saved_metadata_for_people: true,
            tv_channels_refreshed: false,
            proxy_header_mode: "None".to_owned(),
            is_in_maintenance_mode: false,
            maintenance_mode_message: String::new(),
            enable_debug_level_logging: false,
            revert_debug_logging: String::new(),
            enable_auto_update: false,
            log_file_retention_days: 3,
            run_at_startup: false,
            is_startup_wizard_completed: true,
            cache_path: source.cache_path,
        }
    }
}

#[derive(Clone, Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "PascalCase")]
pub struct PathSubstitutionDto {
    pub from: String,
    pub to: String,
}

#[derive(Clone, Debug, Default, Serialize, PartialEq, Eq)]
#[serde(rename_all = "PascalCase")]
pub struct BrandingOptionsDto {
    pub login_disclaimer: String,
    pub custom_css: String,
}

#[derive(Clone, Debug, PartialEq)]
pub struct ScheduledTaskInfoSource {
    pub id: String,
    pub key: String,
    pub name: String,
    pub description: String,
    pub category: String,
    pub state: String,
    pub current_progress_percentage: Option<f64>,
    pub last_execution_result: Option<ScheduledTaskResultSource>,
    pub triggers: Vec<ScheduledTaskTriggerSource>,
    pub is_hidden: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ScheduledTaskResultSource {
    pub start_time_utc: String,
    pub end_time_utc: Option<String>,
    pub status: String,
    pub name: String,
    pub key: String,
    pub id: String,
    pub error_message: Option<String>,
    pub long_error_message: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ScheduledTaskTriggerSource {
    pub trigger_type: String,
    pub time_of_day_ticks: Option<i64>,
    pub interval_ticks: Option<i64>,
    pub system_event: Option<String>,
    pub day_of_week: Option<String>,
    pub max_runtime_ticks: Option<i64>,
}

#[derive(Clone, Debug, Serialize, PartialEq)]
#[serde(rename_all = "PascalCase")]
pub struct ScheduledTaskInfoDto {
    pub name: String,
    pub state: String,
    pub current_progress_percentage: Option<f64>,
    pub id: String,
    pub last_execution_result: Option<ScheduledTaskResultDto>,
    pub triggers: Vec<ScheduledTaskTriggerInfoDto>,
    pub description: String,
    pub category: String,
    pub is_hidden: bool,
    pub key: String,
}

impl From<ScheduledTaskInfoSource> for ScheduledTaskInfoDto {
    fn from(source: ScheduledTaskInfoSource) -> Self {
        Self {
            name: source.name,
            state: source.state,
            current_progress_percentage: source.current_progress_percentage,
            id: source.id,
            last_execution_result: source.last_execution_result.map(Into::into),
            triggers: source.triggers.into_iter().map(Into::into).collect(),
            description: source.description,
            category: source.category,
            is_hidden: source.is_hidden,
            key: source.key,
        }
    }
}

#[derive(Clone, Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "PascalCase")]
pub struct ScheduledTaskResultDto {
    pub start_time_utc: String,
    pub end_time_utc: Option<String>,
    pub status: String,
    pub name: String,
    pub key: String,
    pub id: String,
    pub error_message: Option<String>,
    pub long_error_message: Option<String>,
}

impl From<ScheduledTaskResultSource> for ScheduledTaskResultDto {
    fn from(source: ScheduledTaskResultSource) -> Self {
        Self {
            start_time_utc: source.start_time_utc,
            end_time_utc: source.end_time_utc,
            status: source.status,
            name: source.name,
            key: source.key,
            id: source.id,
            error_message: source.error_message,
            long_error_message: source.long_error_message,
        }
    }
}

#[derive(Clone, Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "PascalCase")]
pub struct ScheduledTaskTriggerInfoDto {
    #[serde(rename = "Type")]
    pub trigger_type: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub time_of_day_ticks: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub interval_ticks: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub system_event: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub day_of_week: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_runtime_ticks: Option<i64>,
}

impl From<ScheduledTaskTriggerSource> for ScheduledTaskTriggerInfoDto {
    fn from(source: ScheduledTaskTriggerSource) -> Self {
        Self {
            trigger_type: source.trigger_type,
            time_of_day_ticks: source.time_of_day_ticks,
            interval_ticks: source.interval_ticks,
            system_event: source.system_event,
            day_of_week: source.day_of_week,
            max_runtime_ticks: source.max_runtime_ticks,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DisplayPreferencesSource {
    pub id: String,
    pub client: String,
}

#[derive(Clone, Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "PascalCase")]
pub struct DisplayPreferencesDto {
    pub id: String,
    pub sort_by: String,
    pub custom_prefs: BTreeMap<String, String>,
    pub sort_order: String,
    pub client: String,
}

impl From<DisplayPreferencesSource> for DisplayPreferencesDto {
    fn from(source: DisplayPreferencesSource) -> Self {
        Self {
            id: source.id,
            sort_by: "SortName".to_owned(),
            custom_prefs: BTreeMap::new(),
            sort_order: "Ascending".to_owned(),
            client: source.client,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct UserSource {
    pub id: String,
    pub name: String,
    pub has_password: bool,
}

#[derive(Clone, Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "PascalCase")]
pub struct PublicUserDto {
    pub id: String,
    pub name: String,
    pub has_password: bool,
}

impl From<UserSource> for PublicUserDto {
    fn from(source: UserSource) -> Self {
        Self {
            id: source.id,
            name: source.name,
            has_password: source.has_password,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct UserDetailSource {
    pub id: String,
    pub name: String,
    pub has_password: bool,
    pub is_administrator: bool,
    pub is_disabled: bool,
    pub allow_download: bool,
    pub allow_transcode: bool,
    pub allow_new_device_login: bool,
    pub enable_content_downloading: bool,
    pub enable_playback_transcoding: bool,
    pub enable_all_folders: bool,
    pub enabled_folders: Vec<String>,
}

#[derive(Clone, Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "PascalCase")]
pub struct UserDto {
    pub id: String,
    pub name: String,
    pub server_id: String,
    pub server_name: String,
    pub has_password: bool,
    pub has_configured_password: bool,
    pub has_configured_easy_password: bool,
    pub enable_auto_login: bool,
    pub connect_user_name: Option<String>,
    pub connect_link_type: String,
    pub primary_image_tag: Option<String>,
    pub primary_image_aspect_ratio: Option<String>,
    pub last_login_date: Option<String>,
    pub last_activity_date: Option<String>,
    pub date_created: Option<String>,
    pub user_item_share_level: String,
    pub prefix: Option<String>,
    pub policy: UserPolicyDto,
    pub configuration: UserConfigurationDto,
}

impl From<UserDetailSource> for UserDto {
    fn from(source: UserDetailSource) -> Self {
        Self {
            id: source.id,
            name: source.name,
            server_id: "fbz-api".to_owned(),
            server_name: "FBZ API".to_owned(),
            has_password: source.has_password,
            has_configured_password: source.has_password,
            has_configured_easy_password: false,
            enable_auto_login: false,
            connect_user_name: None,
            connect_link_type: "LinkedUser".to_owned(),
            primary_image_tag: None,
            primary_image_aspect_ratio: None,
            last_login_date: None,
            last_activity_date: None,
            date_created: None,
            user_item_share_level: "None".to_owned(),
            prefix: None,
            policy: UserPolicyDto {
                is_administrator: source.is_administrator,
                is_disabled: source.is_disabled,
                is_hidden: false,
                is_hidden_remotely: false,
                is_hidden_from_unused_devices: false,
                locked_out_date: 0,
                enable_media_playback: true,
                enable_audio_playback_transcoding: source.enable_playback_transcoding,
                enable_video_playback_transcoding: source.enable_playback_transcoding,
                enable_playback_remuxing: source.enable_playback_transcoding,
                force_remote_source_transcoding: source.enable_playback_transcoding,
                enable_content_deletion: false,
                enable_content_deletion_from_folders: Vec::new(),
                enable_content_downloading: source.enable_content_downloading,
                enable_subtitle_downloading: false,
                enable_subtitle_management: false,
                max_parental_rating: None,
                allow_tag_or_rating: false,
                is_tag_blocking_mode_inclusive: false,
                include_tags: Vec::new(),
                access_schedules: Vec::new(),
                block_unrated_items: Vec::new(),
                enable_sync_transcoding: source.enable_playback_transcoding,
                enable_media_conversion: source.enable_playback_transcoding,
                enable_remote_control_of_other_users: source.is_administrator,
                enable_shared_device_control: false,
                enable_live_tv_management: false,
                enable_live_tv_access: false,
                enable_all_channels: false,
                enable_all_folders: source.enable_all_folders,
                enable_public_sharing: false,
                enable_user_preference_access: true,
                enable_remote_access: true,
                enable_next_episode_auto_play: true,
                enable_all_devices: source.allow_new_device_login,
                enabled_devices: Vec::new(),
                enabled_channels: Vec::new(),
                blocked_channels: Vec::new(),
                enabled_folders: source.enabled_folders,
                blocked_media_folders: Vec::new(),
                blocked_tags: Vec::new(),
                allowed_tags: Vec::new(),
                invalid_login_attempt_count: 0,
                login_attempts_before_lockout: -1,
                remote_client_bitrate_limit: 0,
                simultaneous_stream_limit: 0,
                max_active_sessions: 0,
                auto_remote_quality: 0,
                restricted_features: Vec::new(),
                authentication_provider_id: None,
                excluded_sub_folders: Vec::new(),
                allow_camera_upload: false,
                allow_sharing_personal_items: false,
            },
            configuration: UserConfigurationDto::default(),
        }
    }
}

#[derive(Clone, Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "PascalCase")]
pub struct UserPolicyDto {
    pub is_administrator: bool,
    pub is_disabled: bool,
    pub is_hidden: bool,
    pub is_hidden_remotely: bool,
    pub is_hidden_from_unused_devices: bool,
    pub locked_out_date: i64,
    pub enable_media_playback: bool,
    pub enable_audio_playback_transcoding: bool,
    pub enable_video_playback_transcoding: bool,
    pub enable_playback_remuxing: bool,
    pub force_remote_source_transcoding: bool,
    pub enable_content_deletion: bool,
    pub enable_content_deletion_from_folders: Vec<String>,
    pub enable_content_downloading: bool,
    pub enable_subtitle_downloading: bool,
    pub enable_subtitle_management: bool,
    pub max_parental_rating: Option<i32>,
    pub allow_tag_or_rating: bool,
    pub is_tag_blocking_mode_inclusive: bool,
    pub include_tags: Vec<String>,
    pub access_schedules: Vec<Value>,
    pub block_unrated_items: Vec<String>,
    pub enable_sync_transcoding: bool,
    pub enable_media_conversion: bool,
    pub enable_remote_control_of_other_users: bool,
    pub enable_shared_device_control: bool,
    pub enable_live_tv_management: bool,
    pub enable_live_tv_access: bool,
    pub enable_all_channels: bool,
    pub enable_all_folders: bool,
    pub enable_public_sharing: bool,
    pub enable_user_preference_access: bool,
    pub enable_remote_access: bool,
    pub enable_next_episode_auto_play: bool,
    pub enable_all_devices: bool,
    pub enabled_devices: Vec<String>,
    pub enabled_channels: Vec<String>,
    pub blocked_channels: Vec<String>,
    pub enabled_folders: Vec<String>,
    pub blocked_media_folders: Vec<String>,
    pub blocked_tags: Vec<String>,
    pub allowed_tags: Vec<String>,
    pub invalid_login_attempt_count: i32,
    pub login_attempts_before_lockout: i32,
    pub remote_client_bitrate_limit: i32,
    pub simultaneous_stream_limit: i32,
    pub max_active_sessions: i32,
    pub auto_remote_quality: i32,
    pub restricted_features: Vec<String>,
    pub authentication_provider_id: Option<String>,
    pub excluded_sub_folders: Vec<String>,
    pub allow_camera_upload: bool,
    pub allow_sharing_personal_items: bool,
}

#[derive(Clone, Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "PascalCase")]
pub struct UserConfigurationDto {
    pub audio_language_preference: Option<String>,
    pub play_default_audio_track: bool,
    pub subtitle_language_preference: Option<String>,
    pub display_missing_episodes: bool,
    pub subtitle_mode: String,
    pub enable_local_password: bool,
    pub ordered_views: Vec<String>,
    pub latest_items_excludes: Vec<String>,
    pub my_media_excludes: Vec<String>,
    pub hide_played_in_latest: bool,
    pub hide_played_in_more_like_this: bool,
    pub hide_played_in_suggestions: bool,
    pub intro_skip_mode: String,
    pub profile_pin: Option<String>,
    pub remember_audio_selections: bool,
    pub remember_subtitle_selections: bool,
    pub resume_rewind_seconds: i32,
    pub enable_next_episode_auto_play: bool,
}

impl Default for UserConfigurationDto {
    fn default() -> Self {
        Self {
            audio_language_preference: None,
            play_default_audio_track: true,
            subtitle_language_preference: None,
            display_missing_episodes: false,
            subtitle_mode: "Default".to_owned(),
            enable_local_password: false,
            ordered_views: Vec::new(),
            latest_items_excludes: Vec::new(),
            my_media_excludes: Vec::new(),
            hide_played_in_latest: false,
            hide_played_in_more_like_this: false,
            hide_played_in_suggestions: false,
            intro_skip_mode: "None".to_owned(),
            profile_pin: None,
            remember_audio_selections: true,
            remember_subtitle_selections: true,
            resume_rewind_seconds: 0,
            enable_next_episode_auto_play: true,
        }
    }
}

#[derive(Clone, Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "PascalCase")]
pub struct AuthenticationResultDto {
    pub user: PublicUserDto,
    pub session_info: SessionInfoDto,
    pub access_token: String,
    pub server_id: String,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "PascalCase")]
pub struct AuthenticateByNameRequestDto {
    pub username: String,
    #[serde(rename = "Pw")]
    pub pw: Option<String>,
    pub password: Option<String>,
}

impl AuthenticateByNameRequestDto {
    pub fn password(&self) -> Option<&str> {
        self.pw.as_deref().or(self.password.as_deref())
    }
}

#[derive(Clone, Debug, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "PascalCase")]
pub struct AuthenticateUserRequestDto {
    #[serde(rename = "Pw")]
    pub pw: Option<String>,
    pub password: Option<String>,
}

impl AuthenticateUserRequestDto {
    pub fn password(&self) -> Option<&str> {
        self.pw.as_deref().or(self.password.as_deref())
    }
}

#[derive(Clone, Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "PascalCase")]
pub struct SessionInfoDto {
    pub id: String,
    pub user_id: String,
    pub user_name: String,
    pub client: Option<String>,
    pub device_id: Option<String>,
    pub device_name: Option<String>,
    pub application_version: Option<String>,
    pub is_active: bool,
}

#[derive(Clone, Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "PascalCase")]
pub struct DeviceInfoDto {
    pub name: String,
    pub id: String,
    pub internal_id: i64,
    pub reported_device_id: String,
    pub last_user_name: String,
    pub app_name: String,
    pub app_version: String,
    pub last_user_id: String,
    pub date_last_activity: Option<String>,
    pub icon_url: Option<String>,
    pub ip_address: Option<String>,
}

#[derive(Clone, Debug, Default, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "PascalCase")]
pub struct DeviceOptionsDto {
    pub custom_name: Option<String>,
}

#[derive(Clone, Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "PascalCase")]
pub struct ContentUploadHistoryDto {
    pub device_id: String,
    pub files_uploaded: Vec<LocalFileInfoDto>,
}

#[derive(Clone, Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "PascalCase")]
pub struct LocalFileInfoDto {
    pub name: String,
    pub id: String,
    pub album: String,
    pub mime_type: String,
    pub date_created: Option<String>,
}

#[derive(Clone, Debug, Serialize, PartialEq)]
#[serde(rename_all = "PascalCase")]
pub struct RemoteSubtitleInfoDto {
    #[serde(rename = "ThreeLetterISOLanguageName")]
    pub three_letter_iso_language_name: String,
    pub id: String,
    pub provider_name: String,
    pub name: String,
    pub format: String,
    pub author: String,
    pub comment: String,
    pub date_created: Option<String>,
    pub community_rating: Option<f32>,
    pub download_count: i32,
    pub is_hash_match: bool,
    pub is_forced: bool,
    pub is_hearing_impaired: bool,
    pub language: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LibraryViewSource {
    pub id: String,
    pub name: String,
    pub collection_type: String,
}

#[derive(Clone, Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "PascalCase")]
pub struct UserViewDto {
    pub id: String,
    pub name: String,
    pub collection_type: String,
    #[serde(rename = "Type")]
    pub item_type: String,
    pub is_folder: bool,
}

impl From<LibraryViewSource> for UserViewDto {
    fn from(source: LibraryViewSource) -> Self {
        Self {
            id: source.id,
            name: source.name,
            collection_type: source.collection_type,
            item_type: "CollectionFolder".to_owned(),
            is_folder: true,
        }
    }
}

#[derive(Clone, Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "PascalCase")]
pub struct MediaFolderDto {
    pub name: String,
    pub id: String,
    pub guid: String,
    pub sub_folders: Vec<MediaSubFolderDto>,
    pub is_user_access_configurable: bool,
}

#[derive(Clone, Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "PascalCase")]
pub struct MediaSubFolderDto {
    pub name: String,
    pub id: String,
    pub path: String,
    pub is_user_access_configurable: bool,
}

#[derive(Clone, Debug, Serialize, PartialEq)]
#[serde(rename_all = "PascalCase")]
pub struct VirtualFolderInfoDto {
    pub name: String,
    pub locations: Vec<String>,
    pub collection_type: String,
    pub library_options: LibraryDefaultOptionsDto,
    pub item_id: String,
    pub id: String,
    pub guid: String,
    pub primary_image_item_id: Option<String>,
    pub primary_image_tag: Option<String>,
    pub refresh_progress: Option<f64>,
    pub refresh_status: String,
}

#[derive(Clone, Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "PascalCase")]
pub struct LibraryOptionsResultDto {
    pub metadata_savers: Vec<LibraryOptionInfoDto>,
    pub metadata_readers: Vec<LibraryOptionInfoDto>,
    pub subtitle_fetchers: Vec<LibraryOptionInfoDto>,
    pub lyrics_fetchers: Vec<LibraryOptionInfoDto>,
    pub type_options: Vec<LibraryTypeOptionsDto>,
    pub default_library_options: Vec<LibraryDefaultOptionsDto>,
}

impl LibraryOptionsResultDto {
    pub fn fbz_default() -> Self {
        let metadata_reader = LibraryOptionInfoDto::new("FBZ Metadata", true);
        let image_fetcher = LibraryOptionInfoDto::new("FBZ Artwork", true);
        let supported_image_types = vec![
            "Primary".to_owned(),
            "Backdrop".to_owned(),
            "Logo".to_owned(),
            "Thumb".to_owned(),
        ];
        let default_image_options = vec![
            LibraryImageOptionDto::new("Primary", 1),
            LibraryImageOptionDto::new("Backdrop", 3),
            LibraryImageOptionDto::new("Logo", 1),
            LibraryImageOptionDto::new("Thumb", 1),
        ];
        let content_types = ["movies", "tvshows", "music", "mixed"];

        Self {
            metadata_savers: Vec::new(),
            metadata_readers: vec![metadata_reader.clone()],
            subtitle_fetchers: Vec::new(),
            lyrics_fetchers: Vec::new(),
            type_options: content_types
                .iter()
                .map(|content_type| LibraryTypeOptionsDto {
                    type_name: (*content_type).to_owned(),
                    metadata_fetchers: vec![metadata_reader.clone()],
                    image_fetchers: vec![image_fetcher.clone()],
                    supported_image_types: supported_image_types.clone(),
                    default_image_options: default_image_options.clone(),
                })
                .collect(),
            default_library_options: content_types
                .iter()
                .map(|content_type| LibraryDefaultOptionsDto::for_content_type(content_type))
                .collect(),
        }
    }
}

#[derive(Clone, Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "PascalCase")]
pub struct LibraryOptionInfoDto {
    pub name: String,
    pub setup_url: Option<String>,
    pub default_enabled: bool,
    pub features: Vec<String>,
}

impl LibraryOptionInfoDto {
    fn new(name: &str, default_enabled: bool) -> Self {
        Self {
            name: name.to_owned(),
            setup_url: None,
            default_enabled,
            features: Vec::new(),
        }
    }
}

#[derive(Clone, Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "PascalCase")]
pub struct LibraryTypeOptionsDto {
    #[serde(rename = "Type")]
    pub type_name: String,
    pub metadata_fetchers: Vec<LibraryOptionInfoDto>,
    pub image_fetchers: Vec<LibraryOptionInfoDto>,
    pub supported_image_types: Vec<String>,
    pub default_image_options: Vec<LibraryImageOptionDto>,
}

#[derive(Clone, Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "PascalCase")]
pub struct LibraryImageOptionDto {
    #[serde(rename = "Type")]
    pub image_type: String,
    pub limit: i32,
    pub min_width: i32,
}

impl LibraryImageOptionDto {
    fn new(image_type: &str, limit: i32) -> Self {
        Self {
            image_type: image_type.to_owned(),
            limit,
            min_width: 0,
        }
    }
}

#[derive(Clone, Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "PascalCase")]
pub struct LibraryDefaultOptionsDto {
    pub enable_archive_media_files: bool,
    pub enable_photos: bool,
    pub enable_realtime_monitor: bool,
    pub enable_marker_detection: bool,
    pub enable_marker_detection_during_library_scan: bool,
    pub intro_detection_fingerprint_length: i32,
    pub enable_chapter_image_extraction: bool,
    pub extract_chapter_images_during_library_scan: bool,
    pub download_images_in_advance: bool,
    pub cache_images: bool,
    pub exclude_from_search: bool,
    pub enable_plex_ignore: bool,
    pub path_infos: Vec<LibraryMediaPathInfoDto>,
    pub ignore_hidden_files: bool,
    pub ignore_file_extensions: Vec<String>,
    pub save_local_metadata: bool,
    pub save_metadata_hidden: bool,
    pub save_local_thumbnail_sets: bool,
    pub import_playlists: bool,
    pub enable_automatic_series_grouping: bool,
    pub share_embedded_music_album_images: bool,
    pub enable_embedded_titles: bool,
    pub enable_audio_resume: bool,
    pub auto_generate_chapters: bool,
    pub merge_top_level_folders: bool,
    pub auto_generate_chapter_interval_minutes: i32,
    pub automatic_refresh_interval_days: i32,
    pub placeholder_metadata_refresh_interval_days: i32,
    pub preferred_metadata_language: String,
    pub preferred_image_language: String,
    pub content_type: String,
    pub metadata_country_code: String,
    pub metadata_savers: Vec<String>,
    pub disabled_local_metadata_readers: Vec<String>,
    pub local_metadata_reader_order: Vec<String>,
    pub disabled_lyrics_fetchers: Vec<String>,
    pub save_lyrics_with_media: bool,
    pub lyrics_download_max_age_days: i32,
    pub lyrics_fetcher_order: Vec<String>,
    pub lyrics_download_languages: Vec<String>,
    pub disabled_subtitle_fetchers: Vec<String>,
    pub subtitle_fetcher_order: Vec<String>,
    pub skip_subtitles_if_embedded_subtitles_present: bool,
    pub skip_subtitles_if_audio_track_matches: bool,
    pub subtitle_download_languages: Vec<String>,
    pub subtitle_download_max_age_days: i32,
    pub require_perfect_subtitle_match: bool,
    pub save_subtitles_with_media: bool,
    pub forced_subtitles_only: bool,
    pub hearing_impaired_subtitles_only: bool,
    pub collapse_single_item_folders: bool,
    pub force_collapse_single_item_folders: bool,
    pub enable_adult_metadata: bool,
    pub import_collections: bool,
    pub enable_multi_version_by_files: bool,
    pub enable_multi_version_by_metadata: bool,
    pub enable_multi_part_items: bool,
    pub min_collection_items: i32,
    pub music_folder_structure: String,
    pub min_resume_pct: i32,
    pub max_resume_pct: i32,
    pub min_resume_duration_seconds: i32,
    pub thumbnail_images_interval_seconds: i32,
    pub sample_ignore_size: i32,
}

impl LibraryDefaultOptionsDto {
    pub(crate) fn for_content_type(content_type: &str) -> Self {
        Self {
            enable_archive_media_files: false,
            enable_photos: true,
            enable_realtime_monitor: false,
            enable_marker_detection: false,
            enable_marker_detection_during_library_scan: false,
            intro_detection_fingerprint_length: 10,
            enable_chapter_image_extraction: false,
            extract_chapter_images_during_library_scan: false,
            download_images_in_advance: false,
            cache_images: true,
            exclude_from_search: false,
            enable_plex_ignore: true,
            path_infos: Vec::new(),
            ignore_hidden_files: true,
            ignore_file_extensions: Vec::new(),
            save_local_metadata: false,
            save_metadata_hidden: false,
            save_local_thumbnail_sets: false,
            import_playlists: true,
            enable_automatic_series_grouping: true,
            share_embedded_music_album_images: true,
            enable_embedded_titles: false,
            enable_audio_resume: true,
            auto_generate_chapters: false,
            merge_top_level_folders: false,
            auto_generate_chapter_interval_minutes: 5,
            automatic_refresh_interval_days: 0,
            placeholder_metadata_refresh_interval_days: 0,
            preferred_metadata_language: "en".to_owned(),
            preferred_image_language: "en".to_owned(),
            content_type: content_type.to_owned(),
            metadata_country_code: "US".to_owned(),
            metadata_savers: Vec::new(),
            disabled_local_metadata_readers: Vec::new(),
            local_metadata_reader_order: Vec::new(),
            disabled_lyrics_fetchers: Vec::new(),
            save_lyrics_with_media: false,
            lyrics_download_max_age_days: 180,
            lyrics_fetcher_order: Vec::new(),
            lyrics_download_languages: Vec::new(),
            disabled_subtitle_fetchers: Vec::new(),
            subtitle_fetcher_order: Vec::new(),
            skip_subtitles_if_embedded_subtitles_present: false,
            skip_subtitles_if_audio_track_matches: false,
            subtitle_download_languages: Vec::new(),
            subtitle_download_max_age_days: 180,
            require_perfect_subtitle_match: false,
            save_subtitles_with_media: false,
            forced_subtitles_only: false,
            hearing_impaired_subtitles_only: false,
            collapse_single_item_folders: false,
            force_collapse_single_item_folders: false,
            enable_adult_metadata: false,
            import_collections: true,
            enable_multi_version_by_files: true,
            enable_multi_version_by_metadata: false,
            enable_multi_part_items: true,
            min_collection_items: 2,
            music_folder_structure: "Other".to_owned(),
            min_resume_pct: 5,
            max_resume_pct: 90,
            min_resume_duration_seconds: 300,
            thumbnail_images_interval_seconds: 10,
            sample_ignore_size: 1024,
        }
    }
}

#[derive(Clone, Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "PascalCase")]
pub struct LibraryMediaPathInfoDto {
    pub path: String,
    pub network_path: Option<String>,
    pub username: Option<String>,
    pub password: Option<String>,
}

#[derive(Clone, Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "PascalCase")]
pub struct QueryResultDto<T> {
    pub items: Vec<T>,
    pub total_record_count: u32,
    pub start_index: u32,
}

impl<T> QueryResultDto<T> {
    pub fn new(items: Vec<T>, total_record_count: u32, start_index: u32) -> Self {
        Self {
            items,
            total_record_count,
            start_index,
        }
    }
}

#[derive(Clone, Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "PascalCase")]
pub struct DeleteInfoDto {
    pub paths: Vec<String>,
}

impl DeleteInfoDto {
    pub fn empty() -> Self {
        Self { paths: Vec::new() }
    }
}

#[derive(Clone, Debug, Serialize, PartialEq)]
#[serde(rename_all = "PascalCase")]
pub struct RecommendationDto {
    pub items: Vec<BaseItemDto>,
    pub recommendation_type: String,
    pub baseline_item_name: String,
    pub category_id: i64,
}

impl RecommendationDto {
    pub fn recently_added_movies(items: Vec<BaseItemDto>) -> Self {
        Self {
            items,
            recommendation_type: "SimilarToRecentlyPlayed".to_owned(),
            baseline_item_name: "Recently Added Movies".to_owned(),
            category_id: 0,
        }
    }
}

#[derive(Clone, Debug, Serialize, PartialEq)]
#[serde(rename_all = "PascalCase")]
pub struct ThemeMediaResultDto {
    pub owner_id: String,
    pub items: Vec<BaseItemDto>,
    pub total_record_count: u32,
}

impl ThemeMediaResultDto {
    pub fn empty(owner_id: String) -> Self {
        Self {
            owner_id,
            items: Vec::new(),
            total_record_count: 0,
        }
    }
}

#[derive(Clone, Debug, Serialize, PartialEq)]
#[serde(rename_all = "PascalCase")]
pub struct AllThemeMediaResultDto {
    pub theme_videos_result: ThemeMediaResultDto,
    pub theme_songs_result: ThemeMediaResultDto,
    pub soundtrack_songs_result: ThemeMediaResultDto,
}

impl AllThemeMediaResultDto {
    pub fn empty(owner_id: String) -> Self {
        Self {
            theme_videos_result: ThemeMediaResultDto::empty(owner_id.clone()),
            theme_songs_result: ThemeMediaResultDto::empty(owner_id.clone()),
            soundtrack_songs_result: ThemeMediaResultDto::empty(owner_id),
        }
    }
}

#[derive(Clone, Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "PascalCase")]
pub struct LyricDto {
    pub metadata: LyricMetadataDto,
    pub lyrics: Vec<LyricLineDto>,
}

impl LyricDto {
    pub fn empty() -> Self {
        Self {
            metadata: LyricMetadataDto {
                is_synced: false,
                ..LyricMetadataDto::default()
            },
            lyrics: Vec::new(),
        }
    }
}

#[derive(Clone, Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "PascalCase")]
pub struct RemoteLyricInfoDto {
    pub id: String,
    pub provider_name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub lyrics: Option<LyricDto>,
}

#[derive(Clone, Debug, Default, Serialize, PartialEq, Eq)]
#[serde(rename_all = "PascalCase")]
pub struct LyricMetadataDto {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub artist: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub album: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub author: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub length: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub by: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub offset: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub creator: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub version: Option<String>,
    pub is_synced: bool,
}

#[derive(Clone, Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "PascalCase")]
pub struct LyricLineDto {
    pub text: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub start: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cues: Option<Vec<LyricLineCueDto>>,
}

#[derive(Clone, Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "PascalCase")]
pub struct LyricLineCueDto {
    pub position: u32,
    pub end_position: u32,
    pub start: i64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub end: Option<i64>,
}

#[derive(Clone, Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "PascalCase")]
pub struct ItemCountsDto {
    pub movie_count: u32,
    pub series_count: u32,
    pub episode_count: u32,
    pub artist_count: u32,
    pub program_count: u32,
    pub trailer_count: u32,
    pub song_count: u32,
    pub album_count: u32,
    pub music_video_count: u32,
    pub box_set_count: u32,
    pub book_count: u32,
    pub item_count: u32,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BaseItemSource {
    pub id: String,
    pub name: String,
    pub item_type: String,
    pub media_type: Option<String>,
    pub parent_id: Option<String>,
    pub is_folder: bool,
    pub run_time_ticks: Option<i64>,
    pub production_year: Option<i32>,
}

#[derive(Clone, Debug, Serialize, PartialEq)]
#[serde(rename_all = "PascalCase")]
pub struct BaseItemDto {
    pub id: String,
    pub name: String,
    #[serde(rename = "Type")]
    pub item_type: String,
    pub media_type: Option<String>,
    pub parent_id: Option<String>,
    pub is_folder: bool,
    pub run_time_ticks: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub size: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub container: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub bitrate: Option<i32>,
    pub production_year: Option<i32>,
    pub image_tags: BTreeMap<String, String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub backdrop_image_tags: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub collection_type: Option<String>,
    pub user_data: Option<UserItemDataDto>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub media_sources: Vec<MediaSourceDto>,
    pub chapters: Vec<ChapterInfoDto>,
}

impl From<BaseItemSource> for BaseItemDto {
    fn from(source: BaseItemSource) -> Self {
        Self {
            id: source.id,
            name: source.name,
            item_type: source.item_type,
            media_type: source.media_type,
            parent_id: source.parent_id,
            is_folder: source.is_folder,
            run_time_ticks: source.run_time_ticks,
            size: None,
            container: None,
            bitrate: None,
            production_year: source.production_year,
            image_tags: BTreeMap::new(),
            backdrop_image_tags: Vec::new(),
            collection_type: None,
            user_data: None,
            media_sources: Vec::new(),
            chapters: Vec::new(),
        }
    }
}

#[derive(Clone, Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "PascalCase")]
pub struct ChapterInfoDto {
    pub start_position_ticks: i64,
    pub name: String,
    pub image_tag: Option<String>,
    #[serde(rename = "MarkerType")]
    pub marker_type: String,
}

#[derive(Clone, Debug, Serialize, PartialEq)]
#[serde(rename_all = "PascalCase")]
pub struct UserItemDataDto {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub rating: Option<f64>,
    pub playback_position_ticks: i64,
    pub play_count: i32,
    pub is_favorite: bool,
    pub played: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub item_id: Option<String>,
}

#[derive(Clone, Debug, Deserialize, PartialEq)]
#[serde(rename_all = "PascalCase")]
pub struct PlaybackInfoRequestDto {
    #[serde(alias = "userId", alias = "user_id")]
    pub user_id: Option<String>,
    #[serde(alias = "maxStreamingBitrate", alias = "max_streaming_bitrate")]
    pub max_streaming_bitrate: Option<i64>,
    #[serde(alias = "startTimeTicks", alias = "start_time_ticks")]
    pub start_time_ticks: Option<i64>,
    #[serde(alias = "mediaSourceId", alias = "media_source_id")]
    pub media_source_id: Option<String>,
    #[serde(alias = "deviceProfile", alias = "device_profile")]
    pub device_profile: Option<Value>,
}

#[derive(Clone, Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "PascalCase")]
pub struct PlaybackInfoResponseDto {
    pub media_sources: Vec<MediaSourceDto>,
    pub play_session_id: String,
    pub error_code: Option<String>,
}

#[derive(Clone, Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "PascalCase")]
pub struct MediaSourceDto {
    pub id: String,
    #[serde(rename = "Type")]
    pub source_type: String,
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub item_id: Option<String>,
    pub path: Option<String>,
    pub protocol: String,
    pub is_remote: bool,
    pub requires_opening: bool,
    pub requires_closing: bool,
    pub supports_probing: bool,
    pub read_at_native_framerate: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub container: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub run_time_ticks: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub size: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub bitrate: Option<i32>,
    pub media_streams: Vec<MediaStreamDto>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub default_audio_stream_index: Option<i32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub default_subtitle_stream_index: Option<i32>,
    pub supports_direct_play: bool,
    pub supports_direct_stream: bool,
    pub supports_transcoding: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub direct_stream_url: Option<String>,
    pub add_api_key_to_direct_stream_url: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub transcoding_url: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub transcoding_sub_protocol: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub transcoding_container: Option<String>,
    pub chapters: Vec<ChapterInfoDto>,
}

#[derive(Clone, Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "PascalCase")]
pub struct MediaStreamDto {
    pub index: i32,
    #[serde(rename = "Type")]
    pub stream_type: String,
    pub codec: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub codec_tag: Option<String>,
    pub language: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub display_title: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub profile: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub level: Option<i32>,
    pub width: Option<i32>,
    pub height: Option<i32>,
    pub channels: Option<i32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sample_rate: Option<i32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub bit_depth: Option<i32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub bit_rate: Option<i32>,
    pub is_default: bool,
    pub is_forced: bool,
}

#[derive(Clone, Debug, PartialEq)]
pub struct PlaybackProgressDto {
    pub item_id: String,
    pub user_id: Option<String>,
    pub session_id: Option<String>,
    pub play_session_id: Option<String>,
    pub media_source_id: Option<String>,
    pub play_method: Option<String>,
    pub queueable_media_types: Vec<String>,
    pub can_seek: Option<bool>,
    pub event_name: Option<String>,
    pub audio_stream_index: Option<i32>,
    pub subtitle_stream_index: Option<i32>,
    pub position_ticks: Option<i64>,
    pub is_paused: Option<bool>,
    pub is_muted: Option<bool>,
    pub volume_level: Option<i32>,
    pub live_stream_id: Option<String>,
    pub playlist_index: Option<i32>,
    pub playlist_length: Option<i32>,
    pub subtitle_offset: Option<f64>,
    pub playback_rate: Option<f64>,
    pub now_playing_queue: Vec<Value>,
    pub playlist_item_id: Option<String>,
    pub playlist_item_ids: Vec<String>,
    pub runtime_ticks: Option<i64>,
    pub playback_start_time_ticks: Option<i64>,
    pub brightness: Option<i32>,
    pub aspect_ratio: Option<String>,
    pub repeat_mode: Option<String>,
    pub sleep_timer_mode: Option<String>,
    pub sleep_timer_end_time: Option<String>,
    pub shuffle: Option<bool>,
}

#[derive(Deserialize)]
#[serde(rename_all = "PascalCase")]
struct PlaybackProgressRawDto {
    #[serde(alias = "itemId", alias = "item_id")]
    item_id: Option<String>,
    #[serde(alias = "item")]
    item: Option<PlaybackProgressItemDto>,
    #[serde(alias = "userId", alias = "user_id")]
    user_id: Option<String>,
    #[serde(alias = "sessionId", alias = "session_id")]
    session_id: Option<String>,
    #[serde(alias = "playSessionId", alias = "play_session_id")]
    play_session_id: Option<String>,
    #[serde(alias = "mediaSourceId", alias = "media_source_id")]
    media_source_id: Option<String>,
    #[serde(alias = "playMethod", alias = "play_method")]
    play_method: Option<String>,
    #[serde(default)]
    #[serde(deserialize_with = "deserialize_string_list")]
    #[serde(alias = "queueableMediaTypes", alias = "queueable_media_types")]
    queueable_media_types: Vec<String>,
    #[serde(alias = "canSeek", alias = "can_seek")]
    can_seek: Option<bool>,
    #[serde(alias = "eventName", alias = "event_name")]
    event_name: Option<String>,
    #[serde(alias = "audioStreamIndex", alias = "audio_stream_index")]
    audio_stream_index: Option<i32>,
    #[serde(alias = "subtitleStreamIndex", alias = "subtitle_stream_index")]
    subtitle_stream_index: Option<i32>,
    #[serde(alias = "positionTicks", alias = "position_ticks")]
    position_ticks: Option<i64>,
    #[serde(alias = "isPaused", alias = "is_paused")]
    is_paused: Option<bool>,
    #[serde(alias = "isMuted", alias = "is_muted")]
    is_muted: Option<bool>,
    #[serde(alias = "volumeLevel", alias = "volume_level")]
    volume_level: Option<i32>,
    #[serde(alias = "liveStreamId", alias = "live_stream_id")]
    live_stream_id: Option<String>,
    #[serde(alias = "playlistIndex", alias = "playlist_index")]
    playlist_index: Option<i32>,
    #[serde(alias = "playlistLength", alias = "playlist_length")]
    playlist_length: Option<i32>,
    #[serde(alias = "subtitleOffset", alias = "subtitle_offset")]
    subtitle_offset: Option<f64>,
    #[serde(alias = "playbackRate", alias = "playback_rate")]
    playback_rate: Option<f64>,
    #[serde(default)]
    #[serde(alias = "nowPlayingQueue", alias = "now_playing_queue")]
    now_playing_queue: Vec<Value>,
    #[serde(alias = "playlistItemId", alias = "playlist_item_id")]
    playlist_item_id: Option<String>,
    #[serde(default)]
    #[serde(deserialize_with = "deserialize_string_list")]
    #[serde(alias = "playlistItemIds", alias = "playlist_item_ids")]
    playlist_item_ids: Vec<String>,
    #[serde(rename = "RunTimeTicks")]
    #[serde(
        alias = "runTimeTicks",
        alias = "runtimeTicks",
        alias = "runtime_ticks"
    )]
    runtime_ticks: Option<i64>,
    #[serde(alias = "playbackStartTimeTicks", alias = "playback_start_time_ticks")]
    playback_start_time_ticks: Option<i64>,
    #[serde(alias = "brightness")]
    brightness: Option<i32>,
    #[serde(alias = "aspectRatio", alias = "aspect_ratio")]
    aspect_ratio: Option<String>,
    #[serde(alias = "repeatMode", alias = "repeat_mode")]
    repeat_mode: Option<String>,
    #[serde(alias = "sleepTimerMode", alias = "sleep_timer_mode")]
    sleep_timer_mode: Option<String>,
    #[serde(alias = "sleepTimerEndTime", alias = "sleep_timer_end_time")]
    sleep_timer_end_time: Option<String>,
    #[serde(alias = "shuffle")]
    shuffle: Option<bool>,
}

#[derive(Deserialize)]
#[serde(rename_all = "PascalCase")]
struct PlaybackProgressItemDto {
    #[serde(alias = "id")]
    id: Option<String>,
}

impl<'de> Deserialize<'de> for PlaybackProgressDto {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let raw = PlaybackProgressRawDto::deserialize(deserializer)?;
        let item_id = raw
            .item_id
            .or_else(|| raw.item.and_then(|item| item.id))
            .ok_or_else(|| de::Error::missing_field("ItemId"))?;

        Ok(Self {
            item_id,
            user_id: raw.user_id,
            session_id: raw.session_id,
            play_session_id: raw.play_session_id,
            media_source_id: raw.media_source_id,
            play_method: raw.play_method,
            queueable_media_types: raw.queueable_media_types,
            can_seek: raw.can_seek,
            event_name: raw.event_name,
            audio_stream_index: raw.audio_stream_index,
            subtitle_stream_index: raw.subtitle_stream_index,
            position_ticks: raw.position_ticks,
            is_paused: raw.is_paused,
            is_muted: raw.is_muted,
            volume_level: raw.volume_level,
            live_stream_id: raw.live_stream_id,
            playlist_index: raw.playlist_index,
            playlist_length: raw.playlist_length,
            subtitle_offset: raw.subtitle_offset,
            playback_rate: raw.playback_rate,
            now_playing_queue: raw.now_playing_queue,
            playlist_item_id: raw.playlist_item_id,
            playlist_item_ids: raw.playlist_item_ids,
            runtime_ticks: raw.runtime_ticks,
            playback_start_time_ticks: raw.playback_start_time_ticks,
            brightness: raw.brightness,
            aspect_ratio: raw.aspect_ratio,
            repeat_mode: raw.repeat_mode,
            sleep_timer_mode: raw.sleep_timer_mode,
            sleep_timer_end_time: raw.sleep_timer_end_time,
            shuffle: raw.shuffle,
        })
    }
}

#[cfg(test)]
mod tests {
    use serde_json::{Value, json};

    use super::*;

    #[test]
    fn system_info_uses_emby_pascal_case_fields() {
        let dto = SystemInfoDto::from(ServerInfoSource {
            id: "server-1".to_owned(),
            server_name: "FBZ".to_owned(),
            version: "0.1.0".to_owned(),
            local_address: "http://127.0.0.1:8080".to_owned(),
            operating_system: "windows".to_owned(),
        });

        let value = serde_json::to_value(dto).unwrap();

        assert_eq!(value["ServerName"], "FBZ");
        assert_eq!(value["LocalAddress"], "http://127.0.0.1:8080");
        assert_eq!(value["SupportsLibraryMonitor"], false);
    }

    #[test]
    fn public_system_info_exposes_emby_network_fields() {
        let dto = PublicSystemInfoDto::from(ServerInfoSource {
            id: "server-1".to_owned(),
            server_name: "FBZ".to_owned(),
            version: "0.1.0".to_owned(),
            local_address: "http://127.0.0.1:8080".to_owned(),
            operating_system: "windows".to_owned(),
        });

        let value = serde_json::to_value(dto).unwrap();

        assert_eq!(value["ServerName"], "FBZ");
        assert_eq!(value["LocalAddress"], "http://127.0.0.1:8080");
        assert_eq!(value["LocalAddresses"], json!(["http://127.0.0.1:8080"]));
        assert_eq!(value["WanAddress"], "http://127.0.0.1:8080");
        assert_eq!(value["RemoteAddresses"], json!(["http://127.0.0.1:8080"]));
    }

    #[test]
    fn branding_options_use_emby_pascal_case_fields() {
        let dto = BrandingOptionsDto {
            login_disclaimer: "Private server".to_owned(),
            custom_css: ".app { color: white; }".to_owned(),
        };

        let value = serde_json::to_value(dto).unwrap();

        assert_eq!(value["LoginDisclaimer"], "Private server");
        assert_eq!(value["CustomCss"], ".app { color: white; }");
    }

    #[test]
    fn endpoint_info_uses_emby_pascal_case_fields() {
        let dto = EndpointInfoDto {
            is_local: true,
            is_in_network: false,
        };

        let value = serde_json::to_value(dto).unwrap();

        assert_eq!(value["IsLocal"], true);
        assert_eq!(value["IsInNetwork"], false);
    }

    #[test]
    fn wake_on_lan_info_uses_emby_pascal_case_fields() {
        let dto = WakeOnLanInfoDto {
            mac_address: "00:11:22:33:44:55".to_owned(),
            broadcast_address: "192.168.1.255".to_owned(),
            port: 9,
        };

        let value = serde_json::to_value(dto).unwrap();

        assert_eq!(value["MacAddress"], "00:11:22:33:44:55");
        assert_eq!(value["BroadcastAddress"], "192.168.1.255");
        assert_eq!(value["Port"], 9);
    }

    #[test]
    fn server_configuration_uses_emby_system_configuration_shape() {
        let dto = ServerConfigurationDto::from(ServerConfigurationSource {
            server_name: "FBZ".to_owned(),
            public_base_url: "https://media.example.test".to_owned(),
            http_server_port_number: 8096,
            cache_path: "./var/artwork".to_owned(),
            metadata_path: "./var/metadata".to_owned(),
            simultaneous_stream_limit: 3,
        });

        let value = serde_json::to_value(dto).unwrap();

        assert_eq!(value["EnableUPnP"], false);
        assert_eq!(value["PublicPort"], 8096);
        assert_eq!(value["PublicHttpsPort"], 8096);
        assert_eq!(value["HttpServerPortNumber"], 8096);
        assert_eq!(value["HttpsPortNumber"], 8096);
        assert_eq!(value["EnableHttps"], true);
        assert_eq!(value["ServerName"], "FBZ");
        assert_eq!(value["WanDdns"], "https://media.example.test");
        assert_eq!(value["UICulture"], "zh-CN");
        assert_eq!(value["MetadataPath"], "./var/metadata");
        assert_eq!(value["PathSubstitutions"], json!([]));
        assert_eq!(value["UninstalledPlugins"], json!([]));
        assert_eq!(value["RemoteIPFilter"], json!([]));
        assert_eq!(value["IsRemoteIPFilterBlacklist"], false);
        assert_eq!(value["SimultaneousStreamLimit"], 3);
        assert_eq!(value["EnableSqLiteMmio"], false);
        assert_eq!(value["PlaylistsUpgradedToM3U"], true);
        assert_eq!(value["IsStartupWizardCompleted"], true);
        assert_eq!(value["CachePath"], "./var/artwork");
    }

    #[test]
    fn scheduled_task_info_uses_emby_task_shape() {
        let dto = ScheduledTaskInfoDto::from(ScheduledTaskInfoSource {
            id: "task-1".to_owned(),
            key: "core.library.incremental_scan".to_owned(),
            name: "Incremental library scan".to_owned(),
            description: "Scans changed library paths.".to_owned(),
            category: "Library".to_owned(),
            state: "Idle".to_owned(),
            current_progress_percentage: None,
            last_execution_result: Some(ScheduledTaskResultSource {
                start_time_utc: "2026-06-22T01:00:00Z".to_owned(),
                end_time_utc: Some("2026-06-22T01:00:30Z".to_owned()),
                status: "Completed".to_owned(),
                name: "Incremental library scan".to_owned(),
                key: "core.library.incremental_scan".to_owned(),
                id: "run-1".to_owned(),
                error_message: None,
                long_error_message: None,
            }),
            triggers: vec![ScheduledTaskTriggerSource {
                trigger_type: "IntervalTrigger".to_owned(),
                time_of_day_ticks: None,
                interval_ticks: Some(900 * 10_000_000),
                system_event: None,
                day_of_week: None,
                max_runtime_ticks: Some(300 * 10_000_000),
            }],
            is_hidden: false,
        });

        let value = serde_json::to_value(dto).unwrap();

        assert_eq!(value["Name"], "Incremental library scan");
        assert_eq!(value["State"], "Idle");
        assert_eq!(value["Id"], "task-1");
        assert_eq!(value["Key"], "core.library.incremental_scan");
        assert_eq!(value["Category"], "Library");
        assert_eq!(value["IsHidden"], false);
        assert_eq!(value["CurrentProgressPercentage"], Value::Null);
        assert_eq!(value["Triggers"][0]["Type"], "IntervalTrigger");
        assert_eq!(value["Triggers"][0]["IntervalTicks"], 9_000_000_000i64);
        assert_eq!(value["LastExecutionResult"]["Status"], "Completed");
    }

    #[test]
    fn display_preferences_use_emby_pascal_case_fields() {
        let dto = DisplayPreferencesDto::from(DisplayPreferencesSource {
            id: "item-1".to_owned(),
            client: "Infuse".to_owned(),
        });

        let value = serde_json::to_value(dto).unwrap();

        assert_eq!(value["Id"], "item-1");
        assert_eq!(value["SortBy"], "SortName");
        assert_eq!(value["SortOrder"], "Ascending");
        assert_eq!(value["Client"], "Infuse");
        assert_eq!(value["CustomPrefs"], json!({}));
    }

    #[test]
    fn base_item_serializes_type_field_for_emby_clients() {
        let dto = BaseItemDto::from(BaseItemSource {
            id: "item-1".to_owned(),
            name: "Movie".to_owned(),
            item_type: "Movie".to_owned(),
            media_type: Some("Video".to_owned()),
            parent_id: None,
            is_folder: false,
            run_time_ticks: Some(60_000_000),
            production_year: Some(2026),
        });

        let value = serde_json::to_value(dto).unwrap();

        assert_eq!(value["Type"], "Movie");
        assert_eq!(value["MediaType"], "Video");
        assert_eq!(value["ImageTags"], json!({}));
        assert_eq!(value["Chapters"], json!([]));
        assert!(value.get("CollectionType").is_none());
        assert!(value.get("MediaSources").is_none());
    }

    #[test]
    fn selectable_media_folder_uses_emby_pascal_case_fields() {
        let dto = MediaFolderDto {
            name: "Movies".to_owned(),
            id: "library-1".to_owned(),
            guid: "library-1".to_owned(),
            sub_folders: vec![MediaSubFolderDto {
                name: "Movies".to_owned(),
                id: "path-1".to_owned(),
                path: "D:/Media/Movies".to_owned(),
                is_user_access_configurable: true,
            }],
            is_user_access_configurable: true,
        };

        let value = serde_json::to_value(dto).unwrap();

        assert_eq!(value["Id"], "library-1");
        assert_eq!(value["Guid"], "library-1");
        assert_eq!(value["SubFolders"][0]["Name"], "Movies");
        assert_eq!(value["SubFolders"][0]["Path"], "D:/Media/Movies");
        assert_eq!(value["IsUserAccessConfigurable"], true);
    }

    #[test]
    fn virtual_folder_info_serializes_emby_shape() {
        let mut library_options = LibraryDefaultOptionsDto::for_content_type("movies");
        library_options.path_infos = vec![LibraryMediaPathInfoDto {
            path: "D:/Media/Movies".to_owned(),
            network_path: None,
            username: None,
            password: None,
        }];
        let dto = VirtualFolderInfoDto {
            name: "Movies".to_owned(),
            locations: vec!["D:/Media/Movies".to_owned()],
            collection_type: "movies".to_owned(),
            library_options,
            item_id: "library-1".to_owned(),
            id: "library-1".to_owned(),
            guid: "library-1".to_owned(),
            primary_image_item_id: None,
            primary_image_tag: None,
            refresh_progress: None,
            refresh_status: "Idle".to_owned(),
        };

        let value = serde_json::to_value(dto).unwrap();

        assert_eq!(value["Name"], "Movies");
        assert_eq!(value["Locations"], json!(["D:/Media/Movies"]));
        assert_eq!(value["CollectionType"], "movies");
        assert_eq!(value["ItemId"], "library-1");
        assert_eq!(value["Id"], "library-1");
        assert_eq!(value["Guid"], "library-1");
        assert_eq!(value["PrimaryImageItemId"], Value::Null);
        assert_eq!(value["LibraryOptions"]["ContentType"], "movies");
        assert_eq!(
            value["LibraryOptions"]["PathInfos"][0]["Path"],
            "D:/Media/Movies"
        );
        assert_eq!(value["RefreshStatus"], "Idle");
    }

    #[test]
    fn library_options_result_uses_emby_pascal_case_defaults() {
        let result = LibraryOptionsResultDto::fbz_default();

        let value = serde_json::to_value(result).unwrap();

        assert!(value["MetadataSavers"].as_array().unwrap().is_empty());
        assert!(value["SubtitleFetchers"].as_array().unwrap().is_empty());
        assert!(value["LyricsFetchers"].as_array().unwrap().is_empty());
        assert!(
            value["MetadataReaders"]
                .as_array()
                .unwrap()
                .iter()
                .any(|reader| reader["Name"] == "FBZ Metadata")
        );
        assert!(
            value["TypeOptions"]
                .as_array()
                .unwrap()
                .iter()
                .any(|option| option["Type"] == "movies")
        );
        assert_eq!(
            value["TypeOptions"][0]["SupportedImageTypes"],
            json!(["Primary", "Backdrop", "Logo", "Thumb"])
        );
        assert_eq!(
            value["TypeOptions"][0]["DefaultImageOptions"][0]["Type"],
            "Primary"
        );
        assert_eq!(value["DefaultLibraryOptions"][0]["ContentType"], "movies");
        assert_eq!(
            value["DefaultLibraryOptions"][0]["PreferredMetadataLanguage"],
            "en"
        );
        assert_eq!(value["DefaultLibraryOptions"][0]["PathInfos"], json!([]));
    }

    #[test]
    fn query_result_wraps_items_for_browse_endpoints() {
        let result = QueryResultDto::new(
            vec![UserViewDto::from(LibraryViewSource {
                id: "library-1".to_owned(),
                name: "Movies".to_owned(),
                collection_type: "movies".to_owned(),
            })],
            1,
            0,
        );

        let value = serde_json::to_value(result).unwrap();

        assert_eq!(value["TotalRecordCount"], 1);
        assert_eq!(value["StartIndex"], 0);
        assert_eq!(value["Items"][0]["Type"], "CollectionFolder");
    }

    #[test]
    fn delete_info_uses_safe_emby_pascal_case_paths() {
        let dto = DeleteInfoDto::empty();

        let value = serde_json::to_value(dto).unwrap();

        assert_eq!(value["Paths"], json!([]));
        assert_eq!(value.as_object().unwrap().len(), 1);
    }

    #[test]
    fn recommendation_dto_uses_emby_pascal_case_fields() {
        let result = RecommendationDto {
            items: vec![BaseItemDto::from(BaseItemSource {
                id: "movie-1".to_owned(),
                name: "Alien".to_owned(),
                item_type: "Movie".to_owned(),
                media_type: Some("Video".to_owned()),
                parent_id: Some("library-1".to_owned()),
                is_folder: false,
                run_time_ticks: Some(7_000_000),
                production_year: Some(1979),
            })],
            recommendation_type: "SimilarToRecentlyPlayed".to_owned(),
            baseline_item_name: "Recently Added Movies".to_owned(),
            category_id: 0,
        };

        let value = serde_json::to_value(result).unwrap();

        assert_eq!(value["RecommendationType"], "SimilarToRecentlyPlayed");
        assert_eq!(value["BaselineItemName"], "Recently Added Movies");
        assert_eq!(value["CategoryId"], 0);
        assert_eq!(value["Items"][0]["Type"], "Movie");
        assert_eq!(value["Items"][0]["MediaType"], "Video");
    }

    #[test]
    fn theme_media_results_serialize_empty_emby_shape() {
        let result = AllThemeMediaResultDto::empty("item-1".to_owned());

        let value = serde_json::to_value(result).unwrap();

        assert_eq!(value["ThemeVideosResult"]["OwnerId"], "item-1");
        assert_eq!(value["ThemeVideosResult"]["Items"], json!([]));
        assert_eq!(value["ThemeVideosResult"]["TotalRecordCount"], 0);
        assert_eq!(value["ThemeSongsResult"]["OwnerId"], "item-1");
        assert_eq!(value["SoundtrackSongsResult"]["OwnerId"], "item-1");
    }

    #[test]
    fn lyric_result_serializes_empty_emby_shape() {
        let result = LyricDto::empty();

        let value = serde_json::to_value(result).unwrap();

        assert_eq!(value["Metadata"]["IsSynced"], false);
        assert_eq!(value["Lyrics"], json!([]));
    }

    #[test]
    fn remote_lyric_info_uses_emby_pascal_case_fields() {
        let result = RemoteLyricInfoDto {
            id: "lyric-1".to_owned(),
            provider_name: "LrcLib".to_owned(),
            lyrics: Some(LyricDto::empty()),
        };

        let value = serde_json::to_value(result).unwrap();

        assert_eq!(value["Id"], "lyric-1");
        assert_eq!(value["ProviderName"], "LrcLib");
        assert_eq!(value["Lyrics"]["Metadata"]["IsSynced"], false);
    }

    #[test]
    fn item_counts_uses_emby_pascal_case_fields() {
        let dto = ItemCountsDto {
            movie_count: 10,
            series_count: 2,
            episode_count: 30,
            artist_count: 4,
            program_count: 0,
            trailer_count: 0,
            song_count: 50,
            album_count: 6,
            music_video_count: 0,
            box_set_count: 1,
            book_count: 0,
            item_count: 103,
        };

        let value = serde_json::to_value(dto).unwrap();

        assert_eq!(value["MovieCount"], 10);
        assert_eq!(value["SeriesCount"], 2);
        assert_eq!(value["SongCount"], 50);
        assert_eq!(value["BoxSetCount"], 1);
        assert_eq!(value["ItemCount"], 103);
    }

    #[test]
    fn user_detail_serializes_policy_for_emby_clients() {
        let dto = UserDto::from(UserDetailSource {
            id: "user-1".to_owned(),
            name: "alice".to_owned(),
            has_password: true,
            is_administrator: false,
            is_disabled: false,
            allow_download: true,
            allow_transcode: false,
            allow_new_device_login: true,
            enable_content_downloading: true,
            enable_playback_transcoding: false,
            enable_all_folders: false,
            enabled_folders: vec!["library-1".to_owned(), "library-2".to_owned()],
        });

        let value = serde_json::to_value(dto).unwrap();

        assert_eq!(value["Id"], "user-1");
        assert_eq!(value["ServerId"], "fbz-api");
        assert_eq!(value["ServerName"], "FBZ API");
        assert_eq!(value["HasConfiguredPassword"], true);
        assert_eq!(value["ConnectUserName"], Value::Null);
        assert_eq!(value["ConnectLinkType"], "LinkedUser");
        assert_eq!(value["PrimaryImageTag"], Value::Null);
        assert_eq!(value["PrimaryImageAspectRatio"], Value::Null);
        assert_eq!(value["LastLoginDate"], Value::Null);
        assert_eq!(value["LastActivityDate"], Value::Null);
        assert_eq!(value["DateCreated"], Value::Null);
        assert_eq!(value["UserItemShareLevel"], "None");
        assert_eq!(value["Prefix"], Value::Null);
        assert_eq!(value["Policy"]["IsAdministrator"], false);
        assert_eq!(value["Policy"]["EnableContentDownloading"], true);
        assert_eq!(value["Policy"]["EnableVideoPlaybackTranscoding"], false);
        assert_eq!(value["Policy"]["EnableAudioPlaybackTranscoding"], false);
        assert_eq!(value["Policy"]["EnablePlaybackRemuxing"], false);
        assert_eq!(value["Policy"]["ForceRemoteSourceTranscoding"], false);
        assert_eq!(value["Policy"]["EnableRemoteControlOfOtherUsers"], false);
        assert_eq!(value["Policy"]["EnableLiveTvManagement"], false);
        assert_eq!(value["Policy"]["EnableLiveTvAccess"], false);
        assert_eq!(value["Policy"]["EnableSharedDeviceControl"], false);
        assert_eq!(value["Policy"]["EnableMediaConversion"], false);
        assert_eq!(value["Policy"]["RemoteClientBitrateLimit"], 0);
        assert_eq!(value["Policy"]["SimultaneousStreamLimit"], 0);
        assert_eq!(value["Policy"]["MaxActiveSessions"], 0);
        assert_eq!(value["Policy"]["EnabledChannels"], json!([]));
        assert_eq!(value["Policy"]["BlockedChannels"], json!([]));
        assert_eq!(value["Policy"]["EnableAllDevices"], true);
        assert_eq!(value["Policy"]["EnableAllFolders"], false);
        assert_eq!(
            value["Policy"]["EnabledFolders"],
            json!(["library-1", "library-2"])
        );
        assert_eq!(value["Policy"]["BlockedMediaFolders"], json!([]));
        assert_eq!(value["Policy"]["AllowedTags"], json!([]));
        assert_eq!(value["Policy"]["BlockedTags"], json!([]));
        assert_eq!(value["Configuration"]["SubtitleMode"], "Default");
        assert_eq!(value["Configuration"]["HidePlayedInMoreLikeThis"], false);
        assert_eq!(value["Configuration"]["HidePlayedInSuggestions"], false);
        assert_eq!(value["Configuration"]["IntroSkipMode"], "None");
        assert_eq!(value["Configuration"]["ProfilePin"], Value::Null);
        assert_eq!(value["Configuration"]["ResumeRewindSeconds"], 0);
    }

    #[test]
    fn user_detail_serializes_policy_restriction_defaults_for_emby_clients() {
        let dto = UserDto::from(UserDetailSource {
            id: "user-1".to_owned(),
            name: "alice".to_owned(),
            has_password: true,
            is_administrator: true,
            is_disabled: false,
            allow_download: true,
            allow_transcode: true,
            allow_new_device_login: true,
            enable_content_downloading: true,
            enable_playback_transcoding: true,
            enable_all_folders: true,
            enabled_folders: Vec::new(),
        });

        let value = serde_json::to_value(dto).unwrap();

        assert_eq!(value["Policy"]["IsHidden"], false);
        assert_eq!(value["Policy"]["IsHiddenRemotely"], false);
        assert_eq!(value["Policy"]["IsHiddenFromUnusedDevices"], false);
        assert_eq!(value["Policy"]["LockedOutDate"], 0);
        assert_eq!(value["Policy"]["MaxParentalRating"], Value::Null);
        assert_eq!(value["Policy"]["AllowTagOrRating"], false);
        assert_eq!(value["Policy"]["IsTagBlockingModeInclusive"], false);
        assert_eq!(value["Policy"]["IncludeTags"], json!([]));
        assert_eq!(value["Policy"]["AccessSchedules"], json!([]));
        assert_eq!(value["Policy"]["BlockUnratedItems"], json!([]));
        assert_eq!(value["Policy"]["AutoRemoteQuality"], 0);
        assert_eq!(value["Policy"]["RestrictedFeatures"], json!([]));
        assert_eq!(value["Policy"]["AuthenticationProviderId"], Value::Null);
        assert_eq!(value["Policy"]["ExcludedSubFolders"], json!([]));
        assert_eq!(value["Policy"]["AllowCameraUpload"], false);
        assert_eq!(value["Policy"]["AllowSharingPersonalItems"], false);
    }

    #[test]
    fn playback_info_request_deserializes_pascal_case_payload() {
        let payload: PlaybackInfoRequestDto = serde_json::from_value(json!({
            "UserId": "user-1",
            "MaxStreamingBitrate": 12000000,
            "StartTimeTicks": 100,
            "MediaSourceId": "source-1",
            "DeviceProfile": { "Name": "client-profile" }
        }))
        .unwrap();

        assert_eq!(payload.user_id.as_deref(), Some("user-1"));
        assert_eq!(payload.max_streaming_bitrate, Some(12_000_000));
        assert_eq!(
            payload.device_profile,
            Some(json!({ "Name": "client-profile" }))
        );
    }

    #[test]
    fn playback_info_request_accepts_lower_camel_payload() {
        let payload: PlaybackInfoRequestDto = serde_json::from_value(json!({
            "userId": "user-1",
            "maxStreamingBitrate": 12000000,
            "startTimeTicks": 42,
            "mediaSourceId": "source-1",
            "deviceProfile": { "name": "client-profile" }
        }))
        .unwrap();

        assert_eq!(payload.user_id.as_deref(), Some("user-1"));
        assert_eq!(payload.max_streaming_bitrate, Some(12_000_000));
        assert_eq!(payload.start_time_ticks, Some(42));
        assert_eq!(payload.media_source_id.as_deref(), Some("source-1"));
        assert_eq!(
            payload.device_profile,
            Some(json!({ "name": "client-profile" }))
        );
    }

    #[test]
    fn authenticate_by_name_accepts_emby_pw_field() {
        let payload: AuthenticateByNameRequestDto = serde_json::from_value(json!({
            "Username": "admin",
            "Pw": "secret"
        }))
        .unwrap();

        assert_eq!(payload.username, "admin");
        assert_eq!(payload.password(), Some("secret"));
    }

    #[test]
    fn authenticate_user_accepts_emby_pw_and_password_fields() {
        let payload: AuthenticateUserRequestDto = serde_json::from_value(json!({
            "Pw": "secret"
        }))
        .unwrap();

        assert_eq!(payload.password(), Some("secret"));

        let payload: AuthenticateUserRequestDto = serde_json::from_value(json!({
            "Password": "fallback-secret"
        }))
        .unwrap();

        assert_eq!(payload.password(), Some("fallback-secret"));
    }

    #[test]
    fn playback_progress_accepts_partial_client_payload() {
        let payload: PlaybackProgressDto = serde_json::from_value(json!({
            "ItemId": "item-1",
            "UserId": "user-1",
            "PlaySessionId": "play-1",
            "MediaSourceId": "source-1",
            "PlayMethod": "DirectPlay",
            "QueueableMediaTypes": ["Audio", "Video"],
            "CanSeek": true,
            "EventName": "TimeUpdate",
            "AudioStreamIndex": 1,
            "SubtitleStreamIndex": -1,
            "PositionTicks": 42,
            "IsPaused": true,
            "IsMuted": false,
            "VolumeLevel": 85,
            "LiveStreamId": "live-1",
            "PlaylistIndex": 2,
            "PlaylistLength": 4,
            "SubtitleOffset": 0,
            "PlaybackRate": 1.25
        }))
        .unwrap();

        assert_eq!(payload.item_id, "item-1");
        assert_eq!(payload.user_id.as_deref(), Some("user-1"));
        assert_eq!(payload.session_id, None);
        assert_eq!(payload.play_session_id.as_deref(), Some("play-1"));
        assert_eq!(payload.media_source_id.as_deref(), Some("source-1"));
        assert_eq!(payload.play_method.as_deref(), Some("DirectPlay"));
        assert_eq!(
            payload.queueable_media_types,
            vec!["Audio".to_owned(), "Video".to_owned()]
        );
        assert_eq!(payload.can_seek, Some(true));
        assert_eq!(payload.event_name.as_deref(), Some("TimeUpdate"));
        assert_eq!(payload.audio_stream_index, Some(1));
        assert_eq!(payload.subtitle_stream_index, Some(-1));
        assert_eq!(payload.position_ticks, Some(42));
        assert_eq!(payload.is_paused, Some(true));
        assert_eq!(payload.is_muted, Some(false));
        assert_eq!(payload.volume_level, Some(85));
        assert_eq!(payload.live_stream_id.as_deref(), Some("live-1"));
        assert_eq!(payload.playlist_index, Some(2));
        assert_eq!(payload.playlist_length, Some(4));
        assert_eq!(payload.subtitle_offset, Some(0.0));
        assert_eq!(payload.playback_rate, Some(1.25));
    }

    #[test]
    fn playback_progress_accepts_lower_camel_client_payload() {
        let payload: PlaybackProgressDto = serde_json::from_value(json!({
            "itemId": "item-1",
            "userId": "user-1",
            "sessionId": "session-1",
            "playSessionId": "play-1",
            "mediaSourceId": "source-1",
            "playMethod": "DirectPlay",
            "queueableMediaTypes": ["Audio", "Video"],
            "canSeek": true,
            "eventName": "TimeUpdate",
            "audioStreamIndex": 1,
            "subtitleStreamIndex": -1,
            "positionTicks": 42,
            "isPaused": true,
            "isMuted": false,
            "volumeLevel": 85,
            "liveStreamId": "live-1",
            "playlistIndex": 2,
            "playlistLength": 4,
            "subtitleOffset": 0,
            "playbackRate": 1.25,
            "nowPlayingQueue": [{ "id": "queue-1" }],
            "playlistItemId": "playlist-item-1",
            "playlistItemIds": ["playlist-item-1", "playlist-item-2"],
            "runTimeTicks": 9000,
            "playbackStartTimeTicks": 100,
            "brightness": 40,
            "aspectRatio": "16:9",
            "repeatMode": "RepeatAll",
            "sleepTimerMode": "EndOfEpisode",
            "sleepTimerEndTime": "2026-06-24T12:00:00Z",
            "shuffle": true
        }))
        .unwrap();

        assert_eq!(payload.item_id, "item-1");
        assert_eq!(payload.user_id.as_deref(), Some("user-1"));
        assert_eq!(payload.session_id.as_deref(), Some("session-1"));
        assert_eq!(payload.play_session_id.as_deref(), Some("play-1"));
        assert_eq!(payload.media_source_id.as_deref(), Some("source-1"));
        assert_eq!(payload.play_method.as_deref(), Some("DirectPlay"));
        assert_eq!(
            payload.queueable_media_types,
            vec!["Audio".to_owned(), "Video".to_owned()]
        );
        assert_eq!(payload.can_seek, Some(true));
        assert_eq!(payload.event_name.as_deref(), Some("TimeUpdate"));
        assert_eq!(payload.audio_stream_index, Some(1));
        assert_eq!(payload.subtitle_stream_index, Some(-1));
        assert_eq!(payload.position_ticks, Some(42));
        assert_eq!(payload.is_paused, Some(true));
        assert_eq!(payload.is_muted, Some(false));
        assert_eq!(payload.volume_level, Some(85));
        assert_eq!(payload.live_stream_id.as_deref(), Some("live-1"));
        assert_eq!(payload.playlist_index, Some(2));
        assert_eq!(payload.playlist_length, Some(4));
        assert_eq!(payload.subtitle_offset, Some(0.0));
        assert_eq!(payload.playback_rate, Some(1.25));
        assert_eq!(payload.now_playing_queue.len(), 1);
        assert_eq!(payload.playlist_item_id.as_deref(), Some("playlist-item-1"));
        assert_eq!(
            payload.playlist_item_ids,
            vec!["playlist-item-1".to_owned(), "playlist-item-2".to_owned()]
        );
        assert_eq!(payload.runtime_ticks, Some(9000));
        assert_eq!(payload.playback_start_time_ticks, Some(100));
        assert_eq!(payload.brightness, Some(40));
        assert_eq!(payload.aspect_ratio.as_deref(), Some("16:9"));
        assert_eq!(payload.repeat_mode.as_deref(), Some("RepeatAll"));
        assert_eq!(payload.sleep_timer_mode.as_deref(), Some("EndOfEpisode"));
        assert_eq!(
            payload.sleep_timer_end_time.as_deref(),
            Some("2026-06-24T12:00:00Z")
        );
        assert_eq!(payload.shuffle, Some(true));
    }

    #[test]
    fn playback_progress_accepts_csv_string_lists() {
        let payload: PlaybackProgressDto = serde_json::from_value(json!({
            "ItemId": "item-1",
            "QueueableMediaTypes": "Audio, Video",
            "PlaylistItemIds": "playlist-item-1,playlist-item-2",
        }))
        .unwrap();

        assert_eq!(
            payload.queueable_media_types,
            vec!["Audio".to_owned(), "Video".to_owned()]
        );
        assert_eq!(
            payload.playlist_item_ids,
            vec!["playlist-item-1".to_owned(), "playlist-item-2".to_owned()]
        );
    }

    #[test]
    fn playback_progress_accepts_item_object_when_item_id_is_missing() {
        let payload: PlaybackProgressDto = serde_json::from_value(json!({
            "Item": {
                "Id": "item-from-object",
                "Name": "External item",
                "MediaType": "Video"
            },
            "PlaySessionId": "play-1",
            "PositionTicks": 42
        }))
        .unwrap();

        assert_eq!(payload.item_id, "item-from-object");
        assert_eq!(payload.play_session_id.as_deref(), Some("play-1"));
        assert_eq!(payload.position_ticks, Some(42));
    }

    #[test]
    fn playback_progress_accepts_lower_camel_item_object_when_item_id_is_missing() {
        let payload: PlaybackProgressDto = serde_json::from_value(json!({
            "item": {
                "id": "item-from-object",
                "name": "External item",
                "mediaType": "Video"
            },
            "playSessionId": "play-1",
            "positionTicks": 42
        }))
        .unwrap();

        assert_eq!(payload.item_id, "item-from-object");
        assert_eq!(payload.play_session_id.as_deref(), Some("play-1"));
        assert_eq!(payload.position_ticks, Some(42));
    }

    #[test]
    fn playback_progress_accepts_queue_and_mode_state() {
        let payload: PlaybackProgressDto = serde_json::from_value(json!({
            "ItemId": "item-1",
            "NowPlayingQueue": [
                { "Id": "queue-1", "PlaylistItemId": "playlist-item-1" },
                { "Id": "queue-2" }
            ],
            "PlaylistItemId": "playlist-item-1",
            "PlaylistItemIds": ["playlist-item-1", "playlist-item-2"],
            "RunTimeTicks": 9000,
            "PlaybackStartTimeTicks": 100,
            "Brightness": 40,
            "AspectRatio": "16:9",
            "RepeatMode": "RepeatAll",
            "SleepTimerMode": "EndOfEpisode",
            "SleepTimerEndTime": "2026-06-24T12:00:00Z",
            "Shuffle": true
        }))
        .unwrap();

        assert_eq!(payload.now_playing_queue.len(), 2);
        assert_eq!(payload.now_playing_queue[0]["Id"], json!("queue-1"));
        assert_eq!(payload.playlist_item_id.as_deref(), Some("playlist-item-1"));
        assert_eq!(
            payload.playlist_item_ids,
            vec!["playlist-item-1".to_owned(), "playlist-item-2".to_owned()]
        );
        assert_eq!(payload.runtime_ticks, Some(9000));
        assert_eq!(payload.playback_start_time_ticks, Some(100));
        assert_eq!(payload.brightness, Some(40));
        assert_eq!(payload.aspect_ratio.as_deref(), Some("16:9"));
        assert_eq!(payload.repeat_mode.as_deref(), Some("RepeatAll"));
        assert_eq!(payload.sleep_timer_mode.as_deref(), Some("EndOfEpisode"));
        assert_eq!(
            payload.sleep_timer_end_time.as_deref(),
            Some("2026-06-24T12:00:00Z")
        );
        assert_eq!(payload.shuffle, Some(true));
    }

    #[test]
    fn playback_info_response_keeps_media_source_boundary() {
        let response = PlaybackInfoResponseDto {
            play_session_id: "play-1".to_owned(),
            error_code: None,
            media_sources: vec![MediaSourceDto {
                id: "source-1".to_owned(),
                source_type: "Default".to_owned(),
                name: "source-1".to_owned(),
                item_id: Some("item-1".to_owned()),
                path: Some("D:/Media/Movie.mkv".to_owned()),
                protocol: "File".to_owned(),
                is_remote: false,
                requires_opening: false,
                requires_closing: false,
                supports_probing: false,
                read_at_native_framerate: false,
                container: Some("mkv".to_owned()),
                run_time_ticks: Some(7_200_000_000),
                size: Some(42_000_000),
                bitrate: Some(12_000_000),
                default_audio_stream_index: None,
                default_subtitle_stream_index: None,
                supports_direct_play: true,
                supports_direct_stream: true,
                supports_transcoding: true,
                direct_stream_url: Some(
                    "/emby/Videos/item-1/stream?MediaSourceId=source-1".to_owned(),
                ),
                add_api_key_to_direct_stream_url: false,
                transcoding_url: Some(
                    "/emby/videos/item-1/master.m3u8?TranscodeSessionId=session-1".to_owned(),
                ),
                transcoding_sub_protocol: Some("hls".to_owned()),
                transcoding_container: Some("ts".to_owned()),
                chapters: Vec::new(),
                media_streams: vec![MediaStreamDto {
                    index: 0,
                    stream_type: "Video".to_owned(),
                    codec: Some("hevc".to_owned()),
                    codec_tag: Some("hvc1".to_owned()),
                    language: None,
                    title: Some("Main".to_owned()),
                    display_title: Some("Main - 2160p HEVC".to_owned()),
                    profile: Some("Main 10".to_owned()),
                    level: Some(153),
                    width: Some(3840),
                    height: Some(2160),
                    channels: None,
                    sample_rate: None,
                    bit_depth: Some(10),
                    bit_rate: Some(12_000_000),
                    is_default: true,
                    is_forced: false,
                }],
            }],
        };

        let value: Value = serde_json::to_value(response).unwrap();

        assert_eq!(value["PlaySessionId"], "play-1");
        assert_eq!(value["MediaSources"][0]["Type"], "Default");
        assert_eq!(value["MediaSources"][0]["Name"], "source-1");
        assert_eq!(value["MediaSources"][0]["IsRemote"], false);
        assert_eq!(value["MediaSources"][0]["RequiresOpening"], false);
        assert_eq!(value["MediaSources"][0]["RequiresClosing"], false);
        assert_eq!(value["MediaSources"][0]["SupportsProbing"], false);
        assert_eq!(value["MediaSources"][0]["ReadAtNativeFramerate"], false);
        assert_eq!(value["MediaSources"][0]["ItemId"], "item-1");
        assert_eq!(value["MediaSources"][0]["RunTimeTicks"], 7_200_000_000i64);
        assert_eq!(value["MediaSources"][0]["Size"], 42_000_000);
        assert_eq!(value["MediaSources"][0]["MediaStreams"][0]["Type"], "Video");
        assert_eq!(value["MediaSources"][0]["Chapters"], json!([]));
        assert_eq!(
            value["MediaSources"][0]["MediaStreams"][0]["DisplayTitle"],
            "Main - 2160p HEVC"
        );
        assert_eq!(value["MediaSources"][0]["TranscodingSubProtocol"], "hls");
    }
}
