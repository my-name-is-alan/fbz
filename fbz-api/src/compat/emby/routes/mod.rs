use axum::{
    Router,
    routing::{delete, get, post},
};

use crate::state::AppState;

mod access;
mod activity_log;
mod artists;
mod bif;
mod branding;
mod channels;
mod classifications;
mod collections;
mod content;
mod devices;
mod display_preferences;
mod dlna;
mod encoding;
mod environment;
mod features;
mod genres;
mod images;
mod instant_mix;
mod item_lookup;
mod item_refresh;
mod items;
mod live_tv;
mod localization;
mod lyrics;
mod media_folders;
mod notifications;
mod packages;
mod persons;
mod playback;
mod playlists;
mod plugins;
mod prefixes;
mod scheduled_tasks;
mod sessions;
mod shows;
mod streaming;
mod studios;
mod subtitles;
mod sync;
mod system;
mod theme_media;
mod transcoding;
mod user_data;
mod users;
mod views;

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/emby/System/Info", get(system::system_info))
        .route("/System/Info", get(system::system_info))
        .route("/emby/System/Info/Public", get(system::public_system_info))
        .route("/System/Info/Public", get(system::public_system_info))
        .route("/emby/System/Endpoint", get(system::system_endpoint))
        .route("/System/Endpoint", get(system::system_endpoint))
        .route("/emby/System/ReleaseNotes", get(system::release_notes))
        .route("/System/ReleaseNotes", get(system::release_notes))
        .route(
            "/emby/System/ReleaseNotes/Versions",
            get(system::release_note_versions),
        )
        .route(
            "/System/ReleaseNotes/Versions",
            get(system::release_note_versions),
        )
        .route(
            "/emby/System/Configuration",
            get(system::system_configuration).post(system::update_system_configuration),
        )
        .route(
            "/System/Configuration",
            get(system::system_configuration).post(system::update_system_configuration),
        )
        .route(
            "/emby/System/Configuration/Partial",
            post(system::update_system_configuration_partial),
        )
        .route(
            "/System/Configuration/Partial",
            post(system::update_system_configuration_partial),
        )
        .route(
            "/emby/System/Configuration/{config_key}",
            get(system::system_configuration_by_key)
                .post(system::update_system_configuration_by_key),
        )
        .route(
            "/System/Configuration/{config_key}",
            get(system::system_configuration_by_key)
                .post(system::update_system_configuration_by_key),
        )
        .route("/emby/System/WakeOnLanInfo", get(system::wake_on_lan_info))
        .route("/System/WakeOnLanInfo", get(system::wake_on_lan_info))
        .route(
            "/emby/System/ActivityLog/Entries",
            get(activity_log::activity_log_entries),
        )
        .route(
            "/System/ActivityLog/Entries",
            get(activity_log::activity_log_entries),
        )
        .route("/emby/Features", get(features::features))
        .route("/Features", get(features::features))
        .route("/emby/Plugins", get(plugins::plugins))
        .route("/Plugins", get(plugins::plugins))
        .route(
            "/emby/Plugins/{plugin_id}/Configuration",
            get(plugins::plugin_configuration).post(plugins::update_plugin_configuration),
        )
        .route(
            "/Plugins/{plugin_id}/Configuration",
            get(plugins::plugin_configuration).post(plugins::update_plugin_configuration),
        )
        .route(
            "/emby/Plugins/{plugin_id}/Thumb",
            get(plugins::plugin_thumb),
        )
        .route("/Plugins/{plugin_id}/Thumb", get(plugins::plugin_thumb))
        .route(
            "/emby/Plugins/{plugin_id}",
            delete(plugins::delete_plugin),
        )
        .route("/Plugins/{plugin_id}", delete(plugins::delete_plugin))
        .route(
            "/emby/Plugins/{plugin_id}/Delete",
            post(plugins::delete_plugin),
        )
        .route("/Plugins/{plugin_id}/Delete", post(plugins::delete_plugin))
        .route("/emby/Packages", get(packages::packages))
        .route("/Packages", get(packages::packages))
        .route("/emby/Packages/Updates", get(packages::package_updates))
        .route("/Packages/Updates", get(packages::package_updates))
        .route(
            "/emby/Packages/Installed/{package_name}",
            post(packages::install_package),
        )
        .route(
            "/Packages/Installed/{package_name}",
            post(packages::install_package),
        )
        .route(
            "/emby/Packages/Installing/{installation_id}",
            delete(packages::cancel_package_installation),
        )
        .route(
            "/Packages/Installing/{installation_id}",
            delete(packages::cancel_package_installation),
        )
        .route(
            "/emby/Packages/Installing/{installation_id}/Delete",
            post(packages::cancel_package_installation),
        )
        .route(
            "/Packages/Installing/{installation_id}/Delete",
            post(packages::cancel_package_installation),
        )
        .route(
            "/emby/Packages/{package_name}",
            get(packages::package_by_name),
        )
        .route("/Packages/{package_name}", get(packages::package_by_name))
        .route("/emby/Dlna/ProfileInfos", get(dlna::profile_infos))
        .route("/Dlna/ProfileInfos", get(dlna::profile_infos))
        .route("/emby/Dlna/Profiles/Default", get(dlna::default_profile))
        .route("/Dlna/Profiles/Default", get(dlna::default_profile))
        .route("/emby/Dlna/Profiles", post(dlna::create_profile))
        .route("/Dlna/Profiles", post(dlna::create_profile))
        .route(
            "/emby/Dlna/Profiles/{profile_id}",
            get(dlna::profile_by_id)
                .post(dlna::update_profile)
                .delete(dlna::delete_profile),
        )
        .route(
            "/Dlna/Profiles/{profile_id}",
            get(dlna::profile_by_id)
                .post(dlna::update_profile)
                .delete(dlna::delete_profile),
        )
        .route(
            "/emby/Environment/DefaultDirectoryBrowser",
            get(environment::default_directory_browser),
        )
        .route(
            "/Environment/DefaultDirectoryBrowser",
            get(environment::default_directory_browser),
        )
        .route("/emby/Environment/Drives", get(environment::drives))
        .route("/Environment/Drives", get(environment::drives))
        .route(
            "/emby/Environment/DirectoryContents",
            get(environment::directory_contents).post(environment::post_directory_contents),
        )
        .route(
            "/Environment/DirectoryContents",
            get(environment::directory_contents).post(environment::post_directory_contents),
        )
        .route(
            "/emby/Environment/ParentPath",
            get(environment::parent_path),
        )
        .route("/Environment/ParentPath", get(environment::parent_path))
        .route(
            "/emby/Environment/NetworkDevices",
            get(environment::network_devices),
        )
        .route(
            "/Environment/NetworkDevices",
            get(environment::network_devices),
        )
        .route(
            "/emby/Environment/NetworkShares",
            get(environment::network_shares),
        )
        .route(
            "/Environment/NetworkShares",
            get(environment::network_shares),
        )
        .route(
            "/emby/Environment/ValidatePath",
            post(environment::validate_path),
        )
        .route("/Environment/ValidatePath", post(environment::validate_path))
        .route(
            "/emby/Encoding/CodecConfiguration/Defaults",
            get(encoding::codec_configuration_defaults),
        )
        .route(
            "/Encoding/CodecConfiguration/Defaults",
            get(encoding::codec_configuration_defaults),
        )
        .route(
            "/emby/Encoding/CodecInformation/Video",
            get(encoding::video_codec_information),
        )
        .route(
            "/Encoding/CodecInformation/Video",
            get(encoding::video_codec_information),
        )
        .route(
            "/emby/Encoding/ToneMapOptions",
            get(encoding::tone_map_options_visibility),
        )
        .route(
            "/Encoding/ToneMapOptions",
            get(encoding::tone_map_options_visibility),
        )
        .route(
            "/emby/Encoding/FullToneMapOptions",
            get(encoding::full_tone_map_options).post(encoding::update_full_tone_map_options),
        )
        .route(
            "/Encoding/FullToneMapOptions",
            get(encoding::full_tone_map_options).post(encoding::update_full_tone_map_options),
        )
        .route(
            "/emby/Encoding/PublicToneMapOptions",
            get(encoding::public_tone_map_options).post(encoding::update_public_tone_map_options),
        )
        .route(
            "/Encoding/PublicToneMapOptions",
            get(encoding::public_tone_map_options).post(encoding::update_public_tone_map_options),
        )
        .route(
            "/emby/Encoding/CodecParameters",
            get(encoding::codec_parameters).post(encoding::update_codec_parameters),
        )
        .route(
            "/Encoding/CodecParameters",
            get(encoding::codec_parameters).post(encoding::update_codec_parameters),
        )
        .route(
            "/emby/Encoding/SubtitleOptions",
            get(encoding::subtitle_options).post(encoding::update_subtitle_options),
        )
        .route(
            "/Encoding/SubtitleOptions",
            get(encoding::subtitle_options).post(encoding::update_subtitle_options),
        )
        .route(
            "/emby/Notifications/Types",
            get(notifications::notification_types),
        )
        .route(
            "/Notifications/Types",
            get(notifications::notification_types),
        )
        .route(
            "/emby/Notifications/Admin",
            post(notifications::admin_notification),
        )
        .route(
            "/Notifications/Admin",
            post(notifications::admin_notification),
        )
        .route(
            "/emby/Notifications/Services/Defaults",
            get(notifications::service_defaults),
        )
        .route(
            "/Notifications/Services/Defaults",
            get(notifications::service_defaults),
        )
        .route(
            "/emby/Notifications/Services/Test",
            post(notifications::service_test),
        )
        .route(
            "/Notifications/Services/Test",
            post(notifications::service_test),
        )
        .route(
            "/emby/Localization/Countries",
            get(localization::countries),
        )
        .route("/Localization/Countries", get(localization::countries))
        .route(
            "/emby/Localization/Cultures",
            get(localization::cultures),
        )
        .route("/Localization/Cultures", get(localization::cultures))
        .route(
            "/emby/Localization/Options",
            get(localization::options),
        )
        .route("/Localization/Options", get(localization::options))
        .route(
            "/emby/Localization/ParentalRatings",
            get(localization::parental_ratings),
        )
        .route(
            "/Localization/ParentalRatings",
            get(localization::parental_ratings),
        )
        .route("/emby/LiveTv/Info", get(live_tv::live_tv_info))
        .route("/LiveTv/Info", get(live_tv::live_tv_info))
        .route("/emby/LiveTv/GuideInfo", get(live_tv::live_tv_info))
        .route("/LiveTv/GuideInfo", get(live_tv::live_tv_info))
        .route("/emby/LiveTv/Folder", get(live_tv::live_tv_folder))
        .route("/LiveTv/Folder", get(live_tv::live_tv_folder))
        .route("/emby/LiveTv/EPG", get(live_tv::empty_query))
        .route("/LiveTv/EPG", get(live_tv::empty_query))
        .route(
            "/emby/LiveTv/AvailableRecordingOptions",
            get(live_tv::empty_query),
        )
        .route(
            "/LiveTv/AvailableRecordingOptions",
            get(live_tv::empty_query),
        )
        .route(
            "/emby/LiveTv/ChannelMappingOptions",
            get(live_tv::empty_query)
                .post(live_tv::mutation_not_configured)
                .put(live_tv::mutation_not_configured)
                .delete(live_tv::mutation_not_configured),
        )
        .route(
            "/LiveTv/ChannelMappingOptions",
            get(live_tv::empty_query)
                .post(live_tv::mutation_not_configured)
                .put(live_tv::mutation_not_configured)
                .delete(live_tv::mutation_not_configured),
        )
        .route(
            "/emby/LiveTv/ChannelMappings",
            get(live_tv::empty_query)
                .post(live_tv::mutation_not_configured)
                .put(live_tv::mutation_not_configured)
                .delete(live_tv::mutation_not_configured),
        )
        .route(
            "/LiveTv/ChannelMappings",
            get(live_tv::empty_query)
                .post(live_tv::mutation_not_configured)
                .put(live_tv::mutation_not_configured)
                .delete(live_tv::mutation_not_configured),
        )
        .route("/emby/LiveTv/Channels", get(live_tv::empty_query))
        .route("/LiveTv/Channels", get(live_tv::empty_query))
        .route("/emby/LiveTv/Channels/{id}", get(live_tv::empty_item))
        .route("/LiveTv/Channels/{id}", get(live_tv::empty_item))
        .route(
            "/emby/LiveTv/ChannelTags/Prefixes",
            get(live_tv::empty_query),
        )
        .route("/LiveTv/ChannelTags/Prefixes", get(live_tv::empty_query))
        .route("/emby/LiveTv/ChannelTags", get(live_tv::empty_query))
        .route("/LiveTv/ChannelTags", get(live_tv::empty_query))
        .route(
            "/emby/LiveTv/Programs",
            get(live_tv::empty_query).post(live_tv::empty_query),
        )
        .route(
            "/LiveTv/Programs",
            get(live_tv::empty_query).post(live_tv::empty_query),
        )
        .route(
            "/emby/LiveTv/Programs/Recommended",
            get(live_tv::empty_query),
        )
        .route("/LiveTv/Programs/Recommended", get(live_tv::empty_query))
        .route("/emby/LiveTv/Programs/{id}", get(live_tv::empty_item))
        .route("/LiveTv/Programs/{id}", get(live_tv::empty_item))
        .route(
            "/emby/LiveTv/RecommendedPrograms",
            get(live_tv::empty_query),
        )
        .route("/LiveTv/RecommendedPrograms", get(live_tv::empty_query))
        .route("/emby/LiveTv/UpcomingPrograms", get(live_tv::empty_query))
        .route("/LiveTv/UpcomingPrograms", get(live_tv::empty_query))
        .route(
            "/emby/LiveTv/ListingProviders/Available",
            get(live_tv::empty_query),
        )
        .route(
            "/LiveTv/ListingProviders/Available",
            get(live_tv::empty_query),
        )
        .route(
            "/emby/LiveTv/ListingProviders/Default",
            get(live_tv::listing_provider_default),
        )
        .route(
            "/LiveTv/ListingProviders/Default",
            get(live_tv::listing_provider_default),
        )
        .route(
            "/emby/LiveTv/ListingProviders/Lineups",
            get(live_tv::empty_query),
        )
        .route("/LiveTv/ListingProviders/Lineups", get(live_tv::empty_query))
        .route(
            "/emby/LiveTv/ListingProviders",
            get(live_tv::empty_query)
                .post(live_tv::mutation_not_configured)
                .delete(live_tv::mutation_not_configured),
        )
        .route(
            "/LiveTv/ListingProviders",
            get(live_tv::empty_query)
                .post(live_tv::mutation_not_configured)
                .delete(live_tv::mutation_not_configured),
        )
        .route(
            "/emby/LiveTv/ListingProviders/Delete",
            post(live_tv::mutation_not_configured),
        )
        .route(
            "/LiveTv/ListingProviders/Delete",
            post(live_tv::mutation_not_configured),
        )
        .route(
            "/emby/LiveTv/Manage/Channels",
            get(live_tv::empty_query),
        )
        .route("/LiveTv/Manage/Channels", get(live_tv::empty_query))
        .route(
            "/emby/LiveTv/Manage/Channels/{id}/Disabled",
            post(live_tv::mutation_not_configured),
        )
        .route(
            "/LiveTv/Manage/Channels/{id}/Disabled",
            post(live_tv::mutation_not_configured),
        )
        .route(
            "/emby/LiveTv/Manage/Channels/{id}/SortIndex",
            post(live_tv::mutation_not_configured),
        )
        .route(
            "/LiveTv/Manage/Channels/{id}/SortIndex",
            post(live_tv::mutation_not_configured),
        )
        .route("/emby/LiveTv/Recordings", get(live_tv::empty_query))
        .route("/LiveTv/Recordings", get(live_tv::empty_query))
        .route(
            "/emby/LiveTv/Recordings/Folders",
            get(live_tv::empty_query),
        )
        .route("/LiveTv/Recordings/Folders", get(live_tv::empty_query))
        .route(
            "/emby/LiveTv/Recordings/Groups",
            get(live_tv::empty_query),
        )
        .route("/LiveTv/Recordings/Groups", get(live_tv::empty_query))
        .route(
            "/emby/LiveTv/Recordings/Series",
            get(live_tv::empty_query),
        )
        .route("/LiveTv/Recordings/Series", get(live_tv::empty_query))
        .route(
            "/emby/LiveTv/Recordings/{id}",
            get(live_tv::empty_item).delete(live_tv::mutation_not_configured),
        )
        .route(
            "/LiveTv/Recordings/{id}",
            get(live_tv::empty_item).delete(live_tv::mutation_not_configured),
        )
        .route(
            "/emby/LiveTv/Recordings/{id}/Delete",
            post(live_tv::mutation_not_configured),
        )
        .route(
            "/LiveTv/Recordings/{id}/Delete",
            post(live_tv::mutation_not_configured),
        )
        .route(
            "/emby/LiveTv/Timers",
            get(live_tv::empty_query).post(live_tv::mutation_not_configured),
        )
        .route(
            "/LiveTv/Timers",
            get(live_tv::empty_query).post(live_tv::mutation_not_configured),
        )
        .route(
            "/emby/LiveTv/Timers/Defaults",
            get(live_tv::timer_defaults),
        )
        .route("/LiveTv/Timers/Defaults", get(live_tv::timer_defaults))
        .route(
            "/emby/LiveTv/SeriesTimers",
            get(live_tv::empty_query).post(live_tv::mutation_not_configured),
        )
        .route(
            "/LiveTv/SeriesTimers",
            get(live_tv::empty_query).post(live_tv::mutation_not_configured),
        )
        .route(
            "/emby/LiveTv/Timers/{id}",
            get(live_tv::empty_item)
                .post(live_tv::mutation_not_configured)
                .delete(live_tv::mutation_not_configured),
        )
        .route(
            "/LiveTv/Timers/{id}",
            get(live_tv::empty_item)
                .post(live_tv::mutation_not_configured)
                .delete(live_tv::mutation_not_configured),
        )
        .route(
            "/emby/LiveTv/Timers/{id}/Delete",
            post(live_tv::mutation_not_configured),
        )
        .route(
            "/LiveTv/Timers/{id}/Delete",
            post(live_tv::mutation_not_configured),
        )
        .route(
            "/emby/LiveTv/TunerHosts/Default/{tuner_type}",
            get(live_tv::empty_query),
        )
        .route(
            "/LiveTv/TunerHosts/Default/{tuner_type}",
            get(live_tv::empty_query),
        )
        .route("/emby/LiveTv/TunerHosts/Types", get(live_tv::empty_query))
        .route("/LiveTv/TunerHosts/Types", get(live_tv::empty_query))
        .route(
            "/emby/LiveTv/TunerHosts",
            get(live_tv::empty_query)
                .post(live_tv::mutation_not_configured)
                .delete(live_tv::mutation_not_configured),
        )
        .route(
            "/LiveTv/TunerHosts",
            get(live_tv::empty_query)
                .post(live_tv::mutation_not_configured)
                .delete(live_tv::mutation_not_configured),
        )
        .route(
            "/emby/LiveTv/TunerHosts/Delete",
            post(live_tv::mutation_not_configured),
        )
        .route(
            "/LiveTv/TunerHosts/Delete",
            post(live_tv::mutation_not_configured),
        )
        .route("/emby/LiveTv/Tuners/Discover", get(live_tv::empty_query))
        .route("/LiveTv/Tuners/Discover", get(live_tv::empty_query))
        .route("/emby/LiveTv/Tuners/Discvover", get(live_tv::empty_query))
        .route("/LiveTv/Tuners/Discvover", get(live_tv::empty_query))
        .route(
            "/emby/LiveTv/SeriesTimers/{id}",
            get(live_tv::empty_item)
                .post(live_tv::mutation_not_configured)
                .delete(live_tv::mutation_not_configured),
        )
        .route(
            "/LiveTv/SeriesTimers/{id}",
            get(live_tv::empty_item)
                .post(live_tv::mutation_not_configured)
                .delete(live_tv::mutation_not_configured),
        )
        .route(
            "/emby/LiveTv/SeriesTimers/{id}/Delete",
            post(live_tv::mutation_not_configured),
        )
        .route(
            "/LiveTv/SeriesTimers/{id}/Delete",
            post(live_tv::mutation_not_configured),
        )
        .route(
            "/emby/LiveTv/Tuners/{id}/Reset",
            post(live_tv::mutation_not_configured),
        )
        .route(
            "/LiveTv/Tuners/{id}/Reset",
            post(live_tv::mutation_not_configured),
        )
        .route(
            "/emby/System/Ping",
            get(system::system_ping).post(system::system_ping),
        )
        .route(
            "/System/Ping",
            get(system::system_ping).post(system::system_ping),
        )
        .route(
            "/emby/Branding/Configuration",
            get(branding::branding_configuration),
        )
        .route(
            "/Branding/Configuration",
            get(branding::branding_configuration),
        )
        .route("/emby/Branding/Css", get(branding::branding_css))
        .route("/Branding/Css", get(branding::branding_css))
        .route("/emby/Branding/Css.css", get(branding::branding_css))
        .route("/Branding/Css.css", get(branding::branding_css))
        .route("/emby/Channels", get(channels::channels))
        .route("/Channels", get(channels::channels))
        .route(
            "/emby/Users/{user_id}/HomeSections",
            get(content::home_sections),
        )
        .route("/Users/{user_id}/HomeSections", get(content::home_sections))
        .route(
            "/emby/Users/{user_id}/Sections/{section_id}/Items",
            get(content::section_items),
        )
        .route(
            "/Users/{user_id}/Sections/{section_id}/Items",
            get(content::section_items),
        )
        .route(
            "/emby/DisplayPreferences/{item_id}",
            get(display_preferences::display_preferences)
                .post(display_preferences::update_display_preferences),
        )
        .route(
            "/DisplayPreferences/{item_id}",
            get(display_preferences::display_preferences)
                .post(display_preferences::update_display_preferences),
        )
        .route(
            "/emby/UserSettings/{user_id}",
            get(display_preferences::user_settings).post(display_preferences::update_user_settings),
        )
        .route(
            "/UserSettings/{user_id}",
            get(display_preferences::user_settings).post(display_preferences::update_user_settings),
        )
        .route(
            "/emby/UserSettings/{user_id}/Partial",
            post(display_preferences::update_user_settings_partial),
        )
        .route(
            "/UserSettings/{user_id}/Partial",
            post(display_preferences::update_user_settings_partial),
        )
        .route(
            "/emby/Users/{user_id}/TypedSettings/{key}",
            get(display_preferences::typed_setting).post(display_preferences::update_typed_setting),
        )
        .route(
            "/Users/{user_id}/TypedSettings/{key}",
            get(display_preferences::typed_setting).post(display_preferences::update_typed_setting),
        )
        .route(
            "/emby/Users/{user_id}/TrackSelections/{track_type}",
            delete(display_preferences::clear_track_selection),
        )
        .route(
            "/Users/{user_id}/TrackSelections/{track_type}",
            delete(display_preferences::clear_track_selection),
        )
        .route(
            "/emby/Users/{user_id}/TrackSelections/{track_type}/Delete",
            post(display_preferences::clear_track_selection),
        )
        .route(
            "/Users/{user_id}/TrackSelections/{track_type}/Delete",
            post(display_preferences::clear_track_selection),
        )
        .route("/emby/Users/Public", get(users::public_users))
        .route("/Users/Public", get(users::public_users))
        .route("/emby/Users/Query", get(users::users_query))
        .route("/Users/Query", get(users::users_query))
        .route("/emby/Users/ItemAccess", get(users::users_query))
        .route("/Users/ItemAccess", get(users::users_query))
        .route("/emby/Users/Prefixes", get(users::user_prefixes))
        .route("/Users/Prefixes", get(users::user_prefixes))
        .route("/emby/Users/Me", get(users::current_user))
        .route("/Users/Me", get(users::current_user))
        .route("/emby/Users/ForgotPassword", post(users::forgot_password))
        .route("/Users/ForgotPassword", post(users::forgot_password))
        .route(
            "/emby/Users/ForgotPassword/Pin",
            post(users::forgot_password_pin),
        )
        .route(
            "/Users/ForgotPassword/Pin",
            post(users::forgot_password_pin),
        )
        .route("/emby/Users/New", post(users::create_user))
        .route("/Users/New", post(users::create_user))
        .route(
            "/emby/Users/{user_id}",
            get(users::user_by_id)
                .post(users::update_user)
                .delete(users::delete_user),
        )
        .route(
            "/Users/{user_id}",
            get(users::user_by_id)
                .post(users::update_user)
                .delete(users::delete_user),
        )
        .route("/emby/Users/{user_id}/Delete", post(users::delete_user))
        .route("/Users/{user_id}/Delete", post(users::delete_user))
        .route(
            "/emby/Users/{user_id}/Configuration",
            post(users::update_user_configuration),
        )
        .route(
            "/Users/{user_id}/Configuration",
            post(users::update_user_configuration),
        )
        .route(
            "/emby/Users/{user_id}/Configuration/Partial",
            post(users::update_user_configuration_partial),
        )
        .route(
            "/Users/{user_id}/Configuration/Partial",
            post(users::update_user_configuration_partial),
        )
        .route(
            "/emby/Users/{user_id}/Policy",
            post(users::update_user_policy),
        )
        .route("/Users/{user_id}/Policy", post(users::update_user_policy))
        .route(
            "/emby/Users/{user_id}/Password",
            post(users::update_user_password),
        )
        .route(
            "/Users/{user_id}/Password",
            post(users::update_user_password),
        )
        .route(
            "/emby/Users/{user_id}/Authenticate",
            post(users::authenticate_by_user_id),
        )
        .route(
            "/Users/{user_id}/Authenticate",
            post(users::authenticate_by_user_id),
        )
        .route(
            "/emby/Users/AuthenticateByName",
            post(users::authenticate_by_name),
        )
        .route(
            "/Users/AuthenticateByName",
            post(users::authenticate_by_name),
        )
        .route("/emby/Auth/Providers", get(sessions::auth_providers))
        .route("/Auth/Providers", get(sessions::auth_providers))
        .route(
            "/emby/Auth/Keys",
            get(sessions::auth_keys).post(sessions::create_auth_key),
        )
        .route(
            "/Auth/Keys",
            get(sessions::auth_keys).post(sessions::create_auth_key),
        )
        .route(
            "/emby/Auth/Keys/{key}",
            delete(sessions::delete_auth_key),
        )
        .route("/Auth/Keys/{key}", delete(sessions::delete_auth_key))
        .route(
            "/emby/Auth/Keys/{key}/Delete",
            post(sessions::delete_auth_key),
        )
        .route(
            "/Auth/Keys/{key}/Delete",
            post(sessions::delete_auth_key),
        )
        .route("/emby/Sessions", get(sessions::list_sessions))
        .route("/Sessions", get(sessions::list_sessions))
        .route("/emby/Sessions/Logout", post(sessions::logout))
        .route("/Sessions/Logout", post(sessions::logout))
        .route("/emby/Sessions/PlayQueue", get(sessions::play_queue))
        .route("/Sessions/PlayQueue", get(sessions::play_queue))
        .route(
            "/emby/Devices",
            get(devices::list_devices).delete(devices::delete_device),
        )
        .route(
            "/Devices",
            get(devices::list_devices).delete(devices::delete_device),
        )
        .route("/emby/Devices/Info", get(devices::device_info))
        .route("/Devices/Info", get(devices::device_info))
        .route(
            "/emby/Devices/CameraUploads",
            get(devices::camera_upload_history).post(devices::camera_upload_disabled),
        )
        .route(
            "/Devices/CameraUploads",
            get(devices::camera_upload_history).post(devices::camera_upload_disabled),
        )
        .route("/emby/Devices/Delete", post(devices::delete_device))
        .route("/Devices/Delete", post(devices::delete_device))
        .route(
            "/emby/Devices/Options",
            get(devices::device_options).post(devices::update_device_options),
        )
        .route(
            "/Devices/Options",
            get(devices::device_options).post(devices::update_device_options),
        )
        .route("/emby/Sync/Options", get(sync::sync_options))
        .route("/Sync/Options", get(sync::sync_options))
        .route("/emby/Sync/Targets", get(sync::sync_targets))
        .route("/Sync/Targets", get(sync::sync_targets))
        .route(
            "/emby/Sync/Jobs",
            get(sync::sync_jobs).post(sync::create_sync_job),
        )
        .route(
            "/Sync/Jobs",
            get(sync::sync_jobs).post(sync::create_sync_job),
        )
        .route(
            "/emby/Sync/Jobs/{id}",
            get(sync::sync_job_by_id)
                .post(sync::update_sync_job)
                .delete(sync::cancel_sync_job),
        )
        .route(
            "/Sync/Jobs/{id}",
            get(sync::sync_job_by_id)
                .post(sync::update_sync_job)
                .delete(sync::cancel_sync_job),
        )
        .route(
            "/emby/Sync/Jobs/{id}/Delete",
            post(sync::cancel_sync_job),
        )
        .route("/Sync/Jobs/{id}/Delete", post(sync::cancel_sync_job))
        .route("/emby/Sync/JobItems", get(sync::sync_job_items))
        .route("/Sync/JobItems", get(sync::sync_job_items))
        .route(
            "/emby/Sync/JobItems/{id}",
            delete(sync::cancel_sync_job_item),
        )
        .route("/Sync/JobItems/{id}", delete(sync::cancel_sync_job_item))
        .route(
            "/emby/Sync/JobItems/{id}/Delete",
            post(sync::cancel_sync_job_item),
        )
        .route(
            "/Sync/JobItems/{id}/Delete",
            post(sync::cancel_sync_job_item),
        )
        .route(
            "/emby/Sync/JobItems/{id}/Enable",
            post(sync::enable_sync_job_item),
        )
        .route(
            "/Sync/JobItems/{id}/Enable",
            post(sync::enable_sync_job_item),
        )
        .route(
            "/emby/Sync/JobItems/{id}/MarkForRemoval",
            post(sync::mark_sync_job_item_for_removal),
        )
        .route(
            "/Sync/JobItems/{id}/MarkForRemoval",
            post(sync::mark_sync_job_item_for_removal),
        )
        .route(
            "/emby/Sync/JobItems/{id}/UnmarkForRemoval",
            post(sync::unmark_sync_job_item_for_removal),
        )
        .route(
            "/Sync/JobItems/{id}/UnmarkForRemoval",
            post(sync::unmark_sync_job_item_for_removal),
        )
        .route(
            "/emby/Sync/JobItems/{id}/Transferred",
            post(sync::transferred_sync_job_item),
        )
        .route(
            "/Sync/JobItems/{id}/Transferred",
            post(sync::transferred_sync_job_item),
        )
        .route(
            "/emby/Sync/JobItems/{id}/AdditionalFiles",
            get(sync::sync_job_item_additional_file),
        )
        .route(
            "/Sync/JobItems/{id}/AdditionalFiles",
            get(sync::sync_job_item_additional_file),
        )
        .route(
            "/emby/Sync/JobItems/{id}/File",
            get(sync::sync_job_item_file).head(sync::sync_job_item_file),
        )
        .route(
            "/Sync/JobItems/{id}/File",
            get(sync::sync_job_item_file).head(sync::sync_job_item_file),
        )
        .route("/emby/Sync/Items/Ready", get(sync::ready_sync_items))
        .route("/Sync/Items/Ready", get(sync::ready_sync_items))
        .route("/emby/Sync/Items/Cancel", post(sync::cancel_items))
        .route("/Sync/Items/Cancel", post(sync::cancel_items))
        .route("/emby/Sync/Data", post(sync::sync_data))
        .route("/Sync/Data", post(sync::sync_data))
        .route("/emby/Sync/OfflineActions", post(sync::offline_actions))
        .route("/Sync/OfflineActions", post(sync::offline_actions))
        .route(
            "/emby/Sync/{target_id}/Items",
            delete(sync::cancel_target_items),
        )
        .route("/Sync/{target_id}/Items", delete(sync::cancel_target_items))
        .route(
            "/emby/Sync/{target_id}/Items/Delete",
            post(sync::cancel_target_items),
        )
        .route(
            "/Sync/{target_id}/Items/Delete",
            post(sync::cancel_target_items),
        )
        .route(
            "/emby/Sync/{item_id}/Status",
            post(sync::item_sync_status),
        )
        .route("/Sync/{item_id}/Status", post(sync::item_sync_status))
        .route(
            "/emby/Sessions/Capabilities",
            post(sessions::update_capabilities),
        )
        .route(
            "/Sessions/Capabilities",
            post(sessions::update_capabilities),
        )
        .route(
            "/emby/Sessions/Capabilities/Full",
            post(sessions::update_capabilities_full),
        )
        .route(
            "/Sessions/Capabilities/Full",
            post(sessions::update_capabilities_full),
        )
        .route("/emby/Sessions/{session_id}", get(sessions::session_by_id))
        .route("/Sessions/{session_id}", get(sessions::session_by_id))
        .route(
            "/emby/Sessions/{session_id}/Playing",
            post(sessions::remote_play),
        )
        .route("/Sessions/{session_id}/Playing", post(sessions::remote_play))
        .route(
            "/emby/Sessions/{session_id}/Playing/{command}",
            post(sessions::remote_playstate_command),
        )
        .route(
            "/Sessions/{session_id}/Playing/{command}",
            post(sessions::remote_playstate_command),
        )
        .route(
            "/emby/Sessions/{session_id}/Command",
            post(sessions::remote_general_command),
        )
        .route(
            "/Sessions/{session_id}/Command",
            post(sessions::remote_general_command),
        )
        .route(
            "/emby/Sessions/Command",
            post(sessions::remote_general_command_without_session),
        )
        .route(
            "/Sessions/Command",
            post(sessions::remote_general_command_without_session),
        )
        .route(
            "/emby/Sessions/{session_id}/Command/{command}",
            post(sessions::remote_general_command_by_name),
        )
        .route(
            "/Sessions/{session_id}/Command/{command}",
            post(sessions::remote_general_command_by_name),
        )
        .route(
            "/emby/Sessions/Command/{command}",
            post(sessions::remote_general_command_by_name_without_session),
        )
        .route(
            "/Sessions/Command/{command}",
            post(sessions::remote_general_command_by_name_without_session),
        )
        .route(
            "/emby/Sessions/{session_id}/System/{command}",
            post(sessions::remote_system_command),
        )
        .route(
            "/Sessions/{session_id}/System/{command}",
            post(sessions::remote_system_command),
        )
        .route(
            "/emby/Sessions/{session_id}/Message",
            post(sessions::remote_message),
        )
        .route(
            "/Sessions/{session_id}/Message",
            post(sessions::remote_message),
        )
        .route(
            "/emby/Sessions/{session_id}/Viewing",
            post(sessions::remote_viewing),
        )
        .route(
            "/Sessions/{session_id}/Viewing",
            post(sessions::remote_viewing),
        )
        .route(
            "/emby/Sessions/{session_id}/Users/{user_id}",
            post(sessions::remote_add_session_user).delete(sessions::remote_remove_session_user),
        )
        .route(
            "/Sessions/{session_id}/Users/{user_id}",
            post(sessions::remote_add_session_user).delete(sessions::remote_remove_session_user),
        )
        .route(
            "/emby/Sessions/{session_id}/Users/{user_id}/Delete",
            post(sessions::remote_remove_session_user),
        )
        .route(
            "/Sessions/{session_id}/Users/{user_id}/Delete",
            post(sessions::remote_remove_session_user),
        )
        .route(
            "/emby/ScheduledTasks",
            get(scheduled_tasks::list_scheduled_tasks),
        )
        .route(
            "/ScheduledTasks",
            get(scheduled_tasks::list_scheduled_tasks),
        )
        .route(
            "/emby/ScheduledTasks/{task_id}",
            get(scheduled_tasks::scheduled_task_by_id),
        )
        .route(
            "/ScheduledTasks/{task_id}",
            get(scheduled_tasks::scheduled_task_by_id),
        )
        .route(
            "/emby/ScheduledTasks/Running/{task_id}",
            post(scheduled_tasks::run_scheduled_task).delete(scheduled_tasks::stop_scheduled_task),
        )
        .route(
            "/ScheduledTasks/Running/{task_id}",
            post(scheduled_tasks::run_scheduled_task).delete(scheduled_tasks::stop_scheduled_task),
        )
        .route(
            "/emby/ScheduledTasks/Running/{task_id}/Delete",
            post(scheduled_tasks::stop_scheduled_task),
        )
        .route(
            "/ScheduledTasks/Running/{task_id}/Delete",
            post(scheduled_tasks::stop_scheduled_task),
        )
        .route(
            "/emby/Library/MediaFolders",
            get(media_folders::media_folders),
        )
        .route("/Library/MediaFolders", get(media_folders::media_folders))
        .route(
            "/emby/Library/SelectableMediaFolders",
            get(media_folders::selectable_media_folders),
        )
        .route(
            "/Library/SelectableMediaFolders",
            get(media_folders::selectable_media_folders),
        )
        .route(
            "/emby/Library/VirtualFolders",
            get(media_folders::virtual_folders),
        )
        .route("/Library/VirtualFolders", get(media_folders::virtual_folders))
        .route(
            "/emby/Library/VirtualFolders/Query",
            get(media_folders::virtual_folders_query),
        )
        .route(
            "/Library/VirtualFolders/Query",
            get(media_folders::virtual_folders_query),
        )
        .route(
            "/emby/Library/PhysicalPaths",
            get(media_folders::physical_paths),
        )
        .route(
            "/Library/PhysicalPaths",
            get(media_folders::physical_paths),
        )
        .route(
            "/emby/Libraries/AvailableOptions",
            get(media_folders::available_options),
        )
        .route(
            "/Libraries/AvailableOptions",
            get(media_folders::available_options),
        )
        .route(
            "/emby/Library/Refresh",
            post(media_folders::refresh_library),
        )
        .route("/Library/Refresh", post(media_folders::refresh_library))
        .route(
            "/emby/Playback/BitrateTest",
            get(playback::bitrate_test),
        )
        .route("/Playback/BitrateTest", get(playback::bitrate_test))
        .route("/emby/LiveStreams/Open", post(playback::live_stream_open))
        .route("/LiveStreams/Open", post(playback::live_stream_open))
        .route(
            "/emby/LiveStreams/MediaInfo",
            post(playback::live_stream_media_info),
        )
        .route(
            "/LiveStreams/MediaInfo",
            post(playback::live_stream_media_info),
        )
        .route(
            "/emby/LiveStreams/Close",
            post(playback::live_stream_close),
        )
        .route("/LiveStreams/Close", post(playback::live_stream_close))
        .route("/emby/Sessions/Playing", post(playback::playing))
        .route("/Sessions/Playing", post(playback::playing))
        .route(
            "/emby/Sessions/Playing/Ping",
            post(playback::playing_ping),
        )
        .route("/Sessions/Playing/Ping", post(playback::playing_ping))
        .route(
            "/emby/Sessions/Playing/Progress",
            post(playback::playing_progress),
        )
        .route(
            "/Sessions/Playing/Progress",
            post(playback::playing_progress),
        )
        .route(
            "/emby/Sessions/Playing/Stopped",
            post(playback::playing_stopped),
        )
        .route("/Sessions/Playing/Stopped", post(playback::playing_stopped))
        .route(
            "/emby/Users/{user_id}/PlayingItems/{item_id}",
            post(playback::user_playing_item).delete(playback::user_playing_item_stopped),
        )
        .route(
            "/Users/{user_id}/PlayingItems/{item_id}",
            post(playback::user_playing_item).delete(playback::user_playing_item_stopped),
        )
        .route(
            "/emby/Users/{user_id}/PlayingItems/{item_id}/Delete",
            post(playback::user_playing_item_stopped),
        )
        .route(
            "/Users/{user_id}/PlayingItems/{item_id}/Delete",
            post(playback::user_playing_item_stopped),
        )
        .route(
            "/emby/Users/{user_id}/PlayingItems/{item_id}/Progress",
            post(playback::user_playing_item_progress),
        )
        .route(
            "/Users/{user_id}/PlayingItems/{item_id}/Progress",
            post(playback::user_playing_item_progress),
        )
        .route(
            "/emby/Videos/ActiveEncodings",
            delete(transcoding::delete_active_encodings),
        )
        .route(
            "/Videos/ActiveEncodings",
            delete(transcoding::delete_active_encodings),
        )
        .route(
            "/emby/Videos/{item_id}/hls1/{playlist_id}/{segment_file_name}",
            get(transcoding::hls_segment),
        )
        .route(
            "/Videos/{item_id}/hls1/{playlist_id}/{segment_file_name}",
            get(transcoding::hls_segment),
        )
        .route(
            "/emby/Videos/{item_id}/subtitles.m3u8",
            get(subtitles::hls_subtitle_playlist),
        )
        .route(
            "/Videos/{item_id}/subtitles.m3u8",
            get(subtitles::hls_subtitle_playlist),
        )
        .route(
            "/emby/Videos/{item_id}/live_subtitles.m3u8",
            get(subtitles::hls_live_subtitle_playlist),
        )
        .route(
            "/Videos/{item_id}/live_subtitles.m3u8",
            get(subtitles::hls_live_subtitle_playlist),
        )
        .route(
            "/emby/Videos/{item_id}/master.m3u8",
            get(transcoding::hls_master_manifest),
        )
        .route(
            "/Videos/{item_id}/master.m3u8",
            get(transcoding::hls_master_manifest),
        )
        .route(
            "/emby/Videos/{item_id}/main.m3u8",
            get(transcoding::hls_main_manifest),
        )
        .route(
            "/Videos/{item_id}/main.m3u8",
            get(transcoding::hls_main_manifest),
        )
        .route(
            "/emby/Videos/{item_id}/live.m3u8",
            get(transcoding::hls_live_manifest),
        )
        .route(
            "/Videos/{item_id}/live.m3u8",
            get(transcoding::hls_live_manifest),
        )
        .route(
            "/emby/Videos/{item_id}/index.bif",
            get(bif::video_index_bif),
        )
        .route("/Videos/{item_id}/index.bif", get(bif::video_index_bif))
        .route(
            "/emby/videos/{item_id}/{file_name}",
            get(transcoding::video_file),
        )
        .route(
            "/emby/Videos/{item_id}/{file_name}",
            get(transcoding::video_file),
        )
        .route(
            "/emby/Videos/{item_id}/stream",
            get(streaming::video_stream),
        )
        .route("/Videos/{item_id}/stream", get(streaming::video_stream))
        .route(
            "/emby/Videos/{item_id}/stream.{container}",
            get(streaming::video_stream_container),
        )
        .route(
            "/Videos/{item_id}/stream.{container}",
            get(streaming::video_stream_container),
        )
        .route(
            "/emby/Videos/{item_id}/AdditionalParts",
            get(items::additional_video_parts),
        )
        .route(
            "/Videos/{item_id}/AdditionalParts",
            get(items::additional_video_parts),
        )
        .route(
            "/emby/Videos/{item_id}/{media_source_id}/Subtitles/{index}/Stream.{format}",
            get(subtitles::subtitle_stream),
        )
        .route(
            "/Videos/{item_id}/{media_source_id}/Subtitles/{index}/Stream.{format}",
            get(subtitles::subtitle_stream),
        )
        .route(
            "/emby/Videos/{item_id}/{media_source_id}/Subtitles/{index}/{start_position_ticks}/Stream.{format}",
            get(subtitles::subtitle_stream_with_start_position),
        )
        .route(
            "/Videos/{item_id}/{media_source_id}/Subtitles/{index}/{start_position_ticks}/Stream.{format}",
            get(subtitles::subtitle_stream_with_start_position),
        )
        .route(
            "/emby/Audio/{item_id}/hls1/{playlist_id}/{segment_file_name}",
            get(transcoding::audio_hls_segment),
        )
        .route(
            "/Audio/{item_id}/hls1/{playlist_id}/{segment_file_name}",
            get(transcoding::audio_hls_segment),
        )
        .route(
            "/emby/Audio/{item_id}/master.m3u8",
            get(transcoding::audio_hls_master_manifest),
        )
        .route(
            "/Audio/{item_id}/master.m3u8",
            get(transcoding::audio_hls_master_manifest),
        )
        .route(
            "/emby/Audio/{item_id}/main.m3u8",
            get(transcoding::audio_hls_main_manifest),
        )
        .route(
            "/Audio/{item_id}/main.m3u8",
            get(transcoding::audio_hls_main_manifest),
        )
        .route(
            "/emby/Audio/{item_id}/live.m3u8",
            get(transcoding::audio_hls_live_manifest),
        )
        .route(
            "/Audio/{item_id}/live.m3u8",
            get(transcoding::audio_hls_live_manifest),
        )
        .route(
            "/emby/Audio/{item_id}/universal",
            get(streaming::universal_audio_stream),
        )
        .route(
            "/Audio/{item_id}/universal",
            get(streaming::universal_audio_stream),
        )
        .route(
            "/emby/Audio/{item_id}/Lyrics",
            get(lyrics::item_lyrics),
        )
        .route("/Audio/{item_id}/Lyrics", get(lyrics::item_lyrics))
        .route(
            "/emby/Audio/{item_id}/RemoteSearch/Lyrics",
            get(lyrics::remote_lyrics_search),
        )
        .route(
            "/Audio/{item_id}/RemoteSearch/Lyrics",
            get(lyrics::remote_lyrics_search),
        )
        .route(
            "/emby/Audio/{item_id}/{stream_file_name}",
            get(streaming::audio_stream),
        )
        .route(
            "/Audio/{item_id}/{stream_file_name}",
            get(streaming::audio_stream),
        )
        .route("/videos/{item_id}/{file_name}", get(transcoding::video_file))
        .route("/Videos/{item_id}/{file_name}", get(transcoding::video_file))
        .route("/emby/Shows/NextUp", get(shows::next_up))
        .route("/Shows/NextUp", get(shows::next_up))
        .route("/emby/Shows/{series_id}/Seasons", get(shows::seasons))
        .route("/Shows/{series_id}/Seasons", get(shows::seasons))
        .route("/emby/Shows/{series_id}/Episodes", get(shows::episodes))
        .route("/Shows/{series_id}/Episodes", get(shows::episodes))
        .route("/emby/Users/{user_id}/Views", get(views::user_views))
        .route("/Users/{user_id}/Views", get(views::user_views))
        .route("/emby/UserViews", get(views::user_views_by_query))
        .route("/UserViews", get(views::user_views_by_query))
        .route("/emby/Search/Hints", get(items::search_hints))
        .route("/Search/Hints", get(items::search_hints))
        .route("/emby/Genres", get(genres::genres))
        .route("/Genres", get(genres::genres))
        .route("/emby/Genres/{name}", get(genres::genre_by_name))
        .route("/Genres/{name}", get(genres::genre_by_name))
        .route("/emby/Tags", get(classifications::tags))
        .route("/Tags", get(classifications::tags))
        .route(
            "/emby/OfficialRatings",
            get(classifications::official_ratings),
        )
        .route(
            "/OfficialRatings",
            get(classifications::official_ratings),
        )
        .route("/emby/Years", get(classifications::years))
        .route("/Years", get(classifications::years))
        .route("/emby/Containers", get(classifications::containers))
        .route("/Containers", get(classifications::containers))
        .route("/emby/AudioCodecs", get(classifications::audio_codecs))
        .route("/AudioCodecs", get(classifications::audio_codecs))
        .route("/emby/VideoCodecs", get(classifications::video_codecs))
        .route("/VideoCodecs", get(classifications::video_codecs))
        .route(
            "/emby/SubtitleCodecs",
            get(classifications::subtitle_codecs),
        )
        .route("/SubtitleCodecs", get(classifications::subtitle_codecs))
        .route(
            "/emby/StreamLanguages",
            get(classifications::stream_languages),
        )
        .route("/StreamLanguages", get(classifications::stream_languages))
        .route("/emby/MusicGenres", get(genres::music_genres))
        .route("/MusicGenres", get(genres::music_genres))
        .route(
            "/emby/MusicGenres/InstantMix",
            get(instant_mix::empty_instant_mix),
        )
        .route(
            "/MusicGenres/InstantMix",
            get(instant_mix::empty_instant_mix),
        )
        .route("/emby/MusicGenres/{name}", get(genres::music_genre_by_name))
        .route("/MusicGenres/{name}", get(genres::music_genre_by_name))
        .route(
            "/emby/MusicGenres/{name}/InstantMix",
            get(instant_mix::empty_instant_mix),
        )
        .route(
            "/MusicGenres/{name}/InstantMix",
            get(instant_mix::empty_instant_mix),
        )
        .route("/emby/Persons", get(persons::persons))
        .route("/Persons", get(persons::persons))
        .route("/emby/Persons/{name}", get(persons::person_by_name))
        .route("/Persons/{name}", get(persons::person_by_name))
        .route("/emby/Studios", get(studios::studios))
        .route("/Studios", get(studios::studios))
        .route("/emby/Studios/{name}", get(studios::studio_by_name))
        .route("/Studios/{name}", get(studios::studio_by_name))
        .route("/emby/Artists", get(artists::artists))
        .route("/Artists", get(artists::artists))
        .route("/emby/Artists/AlbumArtists", get(artists::album_artists))
        .route("/Artists/AlbumArtists", get(artists::album_artists))
        .route("/emby/Artists/Prefixes", get(prefixes::artist_prefixes))
        .route("/Artists/Prefixes", get(prefixes::artist_prefixes))
        .route(
            "/emby/Artists/InstantMix",
            get(instant_mix::empty_instant_mix),
        )
        .route("/Artists/InstantMix", get(instant_mix::empty_instant_mix))
        .route("/emby/Artists/{item_id}/Similar", get(items::similar_items))
        .route("/Artists/{item_id}/Similar", get(items::similar_items))
        .route("/emby/Artists/{name}", get(artists::artist_by_name))
        .route("/Artists/{name}", get(artists::artist_by_name))
        .route("/emby/Albums", get(items::albums))
        .route("/Albums", get(items::albums))
        .route("/emby/Albums/{item_id}/InstantMix", get(items::similar_items))
        .route("/Albums/{item_id}/InstantMix", get(items::similar_items))
        .route("/emby/Albums/{item_id}/Similar", get(items::similar_items))
        .route("/Albums/{item_id}/Similar", get(items::similar_items))
        .route(
            "/emby/Collections",
            post(collections::create_collection),
        )
        .route("/Collections", post(collections::create_collection))
        .route(
            "/emby/Collections/{collection_id}/Items",
            post(collections::add_collection_items).delete(collections::remove_collection_items),
        )
        .route(
            "/Collections/{collection_id}/Items",
            post(collections::add_collection_items).delete(collections::remove_collection_items),
        )
        .route(
            "/emby/Collections/{collection_id}/Items/Delete",
            post(collections::remove_collection_items),
        )
        .route(
            "/Collections/{collection_id}/Items/Delete",
            post(collections::remove_collection_items),
        )
        .route(
            "/emby/Playlists",
            get(playlists::playlists).post(playlists::create_playlist),
        )
        .route(
            "/Playlists",
            get(playlists::playlists).post(playlists::create_playlist),
        )
        .route(
            "/emby/Playlists/{playlist_id}/AddToPlaylistInfo",
            get(playlists::add_to_playlist_info),
        )
        .route(
            "/Playlists/{playlist_id}/AddToPlaylistInfo",
            get(playlists::add_to_playlist_info),
        )
        .route(
            "/emby/Playlists/{playlist_id}/Items",
            get(playlists::playlist_items)
                .post(playlists::add_playlist_items)
                .delete(playlists::remove_playlist_items),
        )
        .route(
            "/Playlists/{playlist_id}/Items",
            get(playlists::playlist_items)
                .post(playlists::add_playlist_items)
                .delete(playlists::remove_playlist_items),
        )
        .route(
            "/emby/Playlists/{playlist_id}/Items/Delete",
            post(playlists::remove_playlist_items),
        )
        .route(
            "/Playlists/{playlist_id}/Items/Delete",
            post(playlists::remove_playlist_items),
        )
        .route(
            "/emby/Playlists/{playlist_id}/Items/{item_id}/Move/{new_index}",
            post(playlists::move_playlist_item),
        )
        .route(
            "/Playlists/{playlist_id}/Items/{item_id}/Move/{new_index}",
            post(playlists::move_playlist_item),
        )
        .route(
            "/emby/Playlists/{playlist_id}/InstantMix",
            get(playlists::playlist_items),
        )
        .route(
            "/Playlists/{playlist_id}/InstantMix",
            get(playlists::playlist_items),
        )
        .route("/emby/Songs", get(items::songs))
        .route("/Songs", get(items::songs))
        .route("/emby/Songs/{item_id}/InstantMix", get(items::similar_items))
        .route("/Songs/{item_id}/InstantMix", get(items::similar_items))
        .route("/emby/Items/{item_id}/InstantMix", get(items::similar_items))
        .route("/Items/{item_id}/InstantMix", get(items::similar_items))
        .route("/emby/Items/Prefixes", get(prefixes::item_prefixes))
        .route("/Items/Prefixes", get(prefixes::item_prefixes))
        .route(
            "/emby/Users/{user_id}/PlayedItems/{item_id}",
            post(user_data::mark_played).delete(user_data::mark_unplayed),
        )
        .route(
            "/Users/{user_id}/PlayedItems/{item_id}",
            post(user_data::mark_played).delete(user_data::mark_unplayed),
        )
        .route(
            "/emby/Users/{user_id}/PlayedItems/{item_id}/Delete",
            post(user_data::mark_unplayed),
        )
        .route(
            "/Users/{user_id}/PlayedItems/{item_id}/Delete",
            post(user_data::mark_unplayed),
        )
        .route(
            "/emby/Users/{user_id}/FavoriteItems/{item_id}",
            post(user_data::mark_favorite).delete(user_data::unmark_favorite),
        )
        .route(
            "/Users/{user_id}/FavoriteItems/{item_id}",
            post(user_data::mark_favorite).delete(user_data::unmark_favorite),
        )
        .route(
            "/emby/Users/{user_id}/FavoriteItems/{item_id}/Delete",
            post(user_data::unmark_favorite),
        )
        .route(
            "/Users/{user_id}/FavoriteItems/{item_id}/Delete",
            post(user_data::unmark_favorite),
        )
        .route(
            "/emby/Users/{user_id}/Items/{item_id}/Rating",
            post(user_data::set_rating).delete(user_data::delete_rating),
        )
        .route(
            "/Users/{user_id}/Items/{item_id}/Rating",
            post(user_data::set_rating).delete(user_data::delete_rating),
        )
        .route(
            "/emby/Users/{user_id}/Items/{item_id}/Rating/Delete",
            post(user_data::delete_rating),
        )
        .route(
            "/Users/{user_id}/Items/{item_id}/Rating/Delete",
            post(user_data::delete_rating),
        )
        .route(
            "/emby/Users/{user_id}/Items/{item_id}/HideFromResume",
            post(user_data::hide_from_resume),
        )
        .route(
            "/Users/{user_id}/Items/{item_id}/HideFromResume",
            post(user_data::hide_from_resume),
        )
        .route("/emby/Users/{user_id}/Items", get(items::user_items))
        .route("/Users/{user_id}/Items", get(items::user_items))
        .route(
            "/emby/Users/{user_id}/Items/{item_id}/UserData",
            get(user_data::item_user_data).post(user_data::update_item_user_data),
        )
        .route(
            "/Users/{user_id}/Items/{item_id}/UserData",
            get(user_data::item_user_data).post(user_data::update_item_user_data),
        )
        .route(
            "/emby/Users/{user_id}/Items/{item_id}/SpecialFeatures",
            get(items::user_item_special_features),
        )
        .route(
            "/Users/{user_id}/Items/{item_id}/SpecialFeatures",
            get(items::user_item_special_features),
        )
        .route(
            "/emby/Users/{user_id}/Items/{item_id}/Intros",
            get(items::user_item_intros),
        )
        .route(
            "/Users/{user_id}/Items/{item_id}/Intros",
            get(items::user_item_intros),
        )
        .route(
            "/emby/Users/{user_id}/Items/{item_id}/LocalTrailers",
            get(items::user_item_local_trailers),
        )
        .route(
            "/Users/{user_id}/Items/{item_id}/LocalTrailers",
            get(items::user_item_local_trailers),
        )
        .route(
            "/emby/Users/{user_id}/Items/Resume",
            get(items::resume_items),
        )
        .route("/Users/{user_id}/Items/Resume", get(items::resume_items))
        .route(
            "/emby/Users/{user_id}/Items/Latest",
            get(items::latest_items),
        )
        .route("/Users/{user_id}/Items/Latest", get(items::latest_items))
        .route(
            "/emby/Users/{user_id}/Suggestions",
            get(items::suggested_items),
        )
        .route(
            "/Users/{user_id}/Suggestions",
            get(items::suggested_items),
        )
        .route(
            "/emby/Movies/Recommendations",
            get(items::movie_recommendations),
        )
        .route(
            "/Movies/Recommendations",
            get(items::movie_recommendations),
        )
        .route(
            "/emby/Movies/{item_id}/Similar",
            get(items::similar_items),
        )
        .route(
            "/Movies/{item_id}/Similar",
            get(items::similar_items),
        )
        .route(
            "/emby/Users/{user_id}/Items/Counts",
            get(items::user_item_counts),
        )
        .route(
            "/Users/{user_id}/Items/Counts",
            get(items::user_item_counts),
        )
        .route(
            "/emby/Users/{user_id}/Items/Root",
            get(items::user_items_root),
        )
        .route("/Users/{user_id}/Items/Root", get(items::user_items_root))
        .route("/emby/Trailers", get(items::trailers))
        .route("/Trailers", get(items::trailers))
        .route(
            "/emby/Users/{user_id}/Items/{item_id}",
            get(items::user_item_by_id),
        )
        .route(
            "/Users/{user_id}/Items/{item_id}",
            get(items::user_item_by_id),
        )
        .route(
            "/emby/Items/{item_id}/ThumbnailSet",
            get(bif::thumbnail_set),
        )
        .route("/Items/{item_id}/ThumbnailSet", get(bif::thumbnail_set))
        .route("/emby/Items/Counts", get(items::item_counts))
        .route("/Items/Counts", get(items::item_counts))
        .route("/emby/Items/Filters", get(classifications::items_filters))
        .route("/Items/Filters", get(classifications::items_filters))
        .route("/emby/Items/Access", post(user_data::update_item_access))
        .route("/Items/Access", post(user_data::update_item_access))
        .route(
            "/emby/Items/Shared/Leave",
            post(user_data::leave_shared_items),
        )
        .route(
            "/Items/Shared/Leave",
            post(user_data::leave_shared_items),
        )
        .route(
            "/emby/Items/{item_id}/MakePrivate",
            post(user_data::make_item_private),
        )
        .route(
            "/Items/{item_id}/MakePrivate",
            post(user_data::make_item_private),
        )
        .route(
            "/emby/Items/{item_id}/MakePublic",
            post(user_data::make_item_public),
        )
        .route(
            "/Items/{item_id}/MakePublic",
            post(user_data::make_item_public),
        )
        .route("/emby/Images/Remote", get(images::remote_image_proxy))
        .route("/Images/Remote", get(images::remote_image_proxy))
        .route(
            "/emby/Items/{item_id}/RemoteImages",
            get(images::remote_images),
        )
        .route(
            "/Items/{item_id}/RemoteImages",
            get(images::remote_images),
        )
        .route(
            "/emby/Items/{item_id}/RemoteImages/Providers",
            get(images::remote_image_providers),
        )
        .route(
            "/Items/{item_id}/RemoteImages/Providers",
            get(images::remote_image_providers),
        )
        .route(
            "/emby/Items/{item_id}/RemoteImages/Download",
            post(images::download_remote_image),
        )
        .route(
            "/Items/{item_id}/RemoteImages/Download",
            post(images::download_remote_image),
        )
        .route("/emby/Items/{item_id}/Images", get(images::item_images))
        .route("/Items/{item_id}/Images", get(images::item_images))
        .route(
            "/emby/Items/{item_id}/ExternalIdInfos",
            get(item_lookup::external_id_infos),
        )
        .route(
            "/Items/{item_id}/ExternalIdInfos",
            get(item_lookup::external_id_infos),
        )
        .route(
            "/emby/Items/RemoteSearch/Image",
            get(item_lookup::remote_search_image),
        )
        .route(
            "/Items/RemoteSearch/Image",
            get(item_lookup::remote_search_image),
        )
        .route(
            "/emby/Items/Metadata/Reset",
            post(item_lookup::reset_metadata),
        )
        .route(
            "/Items/Metadata/Reset",
            post(item_lookup::reset_metadata),
        )
        .route(
            "/emby/Items/RemoteSearch/Apply/{item_id}",
            post(item_lookup::apply_remote_search),
        )
        .route(
            "/Items/RemoteSearch/Apply/{item_id}",
            post(item_lookup::apply_remote_search),
        )
        .route(
            "/emby/Items/RemoteSearch/Book",
            post(item_lookup::remote_search_book),
        )
        .route(
            "/Items/RemoteSearch/Book",
            post(item_lookup::remote_search_book),
        )
        .route(
            "/emby/Items/RemoteSearch/BoxSet",
            post(item_lookup::remote_search_box_set),
        )
        .route(
            "/Items/RemoteSearch/BoxSet",
            post(item_lookup::remote_search_box_set),
        )
        .route(
            "/emby/Items/RemoteSearch/Game",
            post(item_lookup::remote_search_game),
        )
        .route(
            "/Items/RemoteSearch/Game",
            post(item_lookup::remote_search_game),
        )
        .route(
            "/emby/Items/RemoteSearch/Movie",
            post(item_lookup::remote_search_movie),
        )
        .route(
            "/Items/RemoteSearch/Movie",
            post(item_lookup::remote_search_movie),
        )
        .route(
            "/emby/Items/RemoteSearch/MusicAlbum",
            post(item_lookup::remote_search_music_album),
        )
        .route(
            "/Items/RemoteSearch/MusicAlbum",
            post(item_lookup::remote_search_music_album),
        )
        .route(
            "/emby/Items/RemoteSearch/MusicArtist",
            post(item_lookup::remote_search_music_artist),
        )
        .route(
            "/Items/RemoteSearch/MusicArtist",
            post(item_lookup::remote_search_music_artist),
        )
        .route(
            "/emby/Items/RemoteSearch/MusicVideo",
            post(item_lookup::remote_search_music_video),
        )
        .route(
            "/Items/RemoteSearch/MusicVideo",
            post(item_lookup::remote_search_music_video),
        )
        .route(
            "/emby/Items/RemoteSearch/Person",
            post(item_lookup::remote_search_person),
        )
        .route(
            "/Items/RemoteSearch/Person",
            post(item_lookup::remote_search_person),
        )
        .route(
            "/emby/Items/RemoteSearch/Series",
            post(item_lookup::remote_search_series),
        )
        .route(
            "/Items/RemoteSearch/Series",
            post(item_lookup::remote_search_series),
        )
        .route(
            "/emby/Items/RemoteSearch/Trailer",
            post(item_lookup::remote_search_trailer),
        )
        .route(
            "/Items/RemoteSearch/Trailer",
            post(item_lookup::remote_search_trailer),
        )
        .route(
            "/emby/Items/{item_id}/Images/{image_type}",
            get(images::item_image)
                .post(images::upload_item_image)
                .delete(images::delete_item_image),
        )
        .route(
            "/Items/{item_id}/Images/{image_type}",
            get(images::item_image)
                .post(images::upload_item_image)
                .delete(images::delete_item_image),
        )
        .route(
            "/emby/Items/{item_id}/Images/{image_type}/Delete",
            post(images::delete_item_image),
        )
        .route(
            "/Items/{item_id}/Images/{image_type}/Delete",
            post(images::delete_item_image),
        )
        .route(
            "/emby/Items/{item_id}/Images/{image_type}/{index}/Delete",
            post(images::delete_item_image_index),
        )
        .route(
            "/Items/{item_id}/Images/{image_type}/{index}/Delete",
            post(images::delete_item_image_index),
        )
        .route(
            "/emby/Items/{item_id}/Images/{image_type}/{index}/Index",
            post(images::reindex_item_image),
        )
        .route(
            "/Items/{item_id}/Images/{image_type}/{index}/Index",
            post(images::reindex_item_image),
        )
        .route(
            "/emby/Items/{item_id}/Images/{image_type}/{index}/Url",
            post(images::update_item_image_url),
        )
        .route(
            "/Items/{item_id}/Images/{image_type}/{index}/Url",
            post(images::update_item_image_url),
        )
        .route(
            "/emby/Items/{item_id}/Images/{image_type}/{index}",
            get(images::item_image_index)
                .post(images::upload_item_image_index)
                .delete(images::delete_item_image_index),
        )
        .route(
            "/Items/{item_id}/Images/{image_type}/{index}",
            get(images::item_image_index)
                .post(images::upload_item_image_index)
                .delete(images::delete_item_image_index),
        )
        .route(
            "/emby/Items/{item_id}/Ancestors",
            get(items::item_ancestors),
        )
        .route("/Items/{item_id}/Ancestors", get(items::item_ancestors))
        .route("/emby/Items/{item_id}/Similar", get(items::similar_items))
        .route("/Items/{item_id}/Similar", get(items::similar_items))
        .route(
            "/emby/Items/{item_id}/DeleteInfo",
            get(items::item_delete_info),
        )
        .route(
            "/Items/{item_id}/DeleteInfo",
            get(items::item_delete_info),
        )
        .route(
            "/emby/Items/{item_id}/CriticReviews",
            get(items::item_critic_reviews),
        )
        .route(
            "/Items/{item_id}/CriticReviews",
            get(items::item_critic_reviews),
        )
        .route(
            "/emby/Items/{item_id}/SpecialFeatures",
            get(items::item_special_features),
        )
        .route(
            "/Items/{item_id}/SpecialFeatures",
            get(items::item_special_features),
        )
        .route("/emby/Items/{item_id}/Intros", get(items::item_intros))
        .route("/Items/{item_id}/Intros", get(items::item_intros))
        .route(
            "/emby/Items/{item_id}/LocalTrailers",
            get(items::item_local_trailers),
        )
        .route(
            "/Items/{item_id}/LocalTrailers",
            get(items::item_local_trailers),
        )
        .route(
            "/emby/Items/{item_id}/Lyrics",
            get(lyrics::item_lyrics),
        )
        .route("/Items/{item_id}/Lyrics", get(lyrics::item_lyrics))
        .route(
            "/emby/Items/{item_id}/RemoteSearch/Subtitles/{language}",
            get(subtitles::remote_subtitle_search).post(subtitles::download_remote_subtitle),
        )
        .route(
            "/Items/{item_id}/RemoteSearch/Subtitles/{language}",
            get(subtitles::remote_subtitle_search).post(subtitles::download_remote_subtitle),
        )
        .route(
            "/emby/Providers/Subtitles/Subtitles/{subtitle_id}",
            get(subtitles::provider_subtitle_download),
        )
        .route(
            "/Providers/Subtitles/Subtitles/{subtitle_id}",
            get(subtitles::provider_subtitle_download),
        )
        .route(
            "/emby/Items/{item_id}/Subtitles/{index}",
            delete(subtitles::delete_item_subtitle),
        )
        .route(
            "/Items/{item_id}/Subtitles/{index}",
            delete(subtitles::delete_item_subtitle),
        )
        .route(
            "/emby/Items/{item_id}/Subtitles/{index}/Delete",
            post(subtitles::delete_item_subtitle),
        )
        .route(
            "/Items/{item_id}/Subtitles/{index}/Delete",
            post(subtitles::delete_item_subtitle),
        )
        .route(
            "/emby/Items/{item_id}/{media_source_id}/Subtitles/{index}/Stream.{format}",
            get(subtitles::subtitle_stream),
        )
        .route(
            "/Items/{item_id}/{media_source_id}/Subtitles/{index}/Stream.{format}",
            get(subtitles::subtitle_stream),
        )
        .route(
            "/emby/Items/{item_id}/{media_source_id}/Subtitles/{index}/{start_position_ticks}/Stream.{format}",
            get(subtitles::subtitle_stream_with_start_position),
        )
        .route(
            "/Items/{item_id}/{media_source_id}/Subtitles/{index}/{start_position_ticks}/Stream.{format}",
            get(subtitles::subtitle_stream_with_start_position),
        )
        .route(
            "/emby/Videos/{item_id}/{media_source_id}/Attachments/{index}/Stream",
            get(subtitles::video_attachment_stream),
        )
        .route(
            "/Videos/{item_id}/{media_source_id}/Attachments/{index}/Stream",
            get(subtitles::video_attachment_stream),
        )
        .route(
            "/emby/Videos/{item_id}/Subtitles/{index}",
            delete(subtitles::delete_video_subtitle),
        )
        .route(
            "/Videos/{item_id}/Subtitles/{index}",
            delete(subtitles::delete_video_subtitle),
        )
        .route(
            "/emby/Videos/{item_id}/Subtitles/{index}/Delete",
            post(subtitles::delete_video_subtitle),
        )
        .route(
            "/Videos/{item_id}/Subtitles/{index}/Delete",
            post(subtitles::delete_video_subtitle),
        )
        .route(
            "/emby/Items/{item_id}/ThemeMedia",
            get(theme_media::theme_media),
        )
        .route("/Items/{item_id}/ThemeMedia", get(theme_media::theme_media))
        .route(
            "/emby/Items/{item_id}/ThemeSongs",
            get(theme_media::theme_songs),
        )
        .route("/Items/{item_id}/ThemeSongs", get(theme_media::theme_songs))
        .route(
            "/emby/Items/{item_id}/ThemeVideos",
            get(theme_media::theme_videos),
        )
        .route(
            "/Items/{item_id}/ThemeVideos",
            get(theme_media::theme_videos),
        )
        .route("/emby/Items/{item_id}", get(items::item_by_id))
        .route("/Items/{item_id}", get(items::item_by_id))
        .route(
            "/emby/Items/{item_id}/PlaybackInfo",
            get(playback::playback_info).post(playback::post_playback_info),
        )
        .route(
            "/Items/{item_id}/PlaybackInfo",
            get(playback::playback_info).post(playback::post_playback_info),
        )
        .route(
            "/emby/Items/{item_id}/Refresh",
            post(item_refresh::refresh_item),
        )
        .route(
            "/Items/{item_id}/Refresh",
            post(item_refresh::refresh_item),
        )
        .route(
            "/emby/Items/{item_id}/Download",
            get(streaming::item_download),
        )
        .route("/Items/{item_id}/Download", get(streaming::item_download))
        .route(
            "/emby/Items/{item_id}/File",
            get(streaming::item_download),
        )
        .route("/Items/{item_id}/File", get(streaming::item_download))
        .route(
            "/emby/Users/{user_id}/Items/{item_id}/PlaybackInfo",
            get(playback::user_playback_info).post(playback::post_user_playback_info),
        )
        .route(
            "/Users/{user_id}/Items/{item_id}/PlaybackInfo",
            get(playback::user_playback_info).post(playback::post_user_playback_info),
        )
}

#[cfg(test)]
mod tests {
    #[test]
    fn music_collection_routes_are_registered_with_prefixed_and_plain_paths() {
        let routes = include_str!("mod.rs");

        assert!(routes.contains(".route(\"/emby/Albums\", get(items::albums))"));
        assert!(routes.contains(".route(\"/Albums\", get(items::albums))"));
        assert!(routes.contains(".route(\"/emby/Songs\", get(items::songs))"));
        assert!(routes.contains(".route(\"/Songs\", get(items::songs))"));
        assert!(routes.contains("\"/emby/Audio/{item_id}/universal\""));
        assert!(routes.contains("\"/Audio/{item_id}/universal\""));
        assert!(routes.contains("\"/emby/Audio/{item_id}/RemoteSearch/Lyrics\""));
        assert!(routes.contains("\"/Audio/{item_id}/RemoteSearch/Lyrics\""));
    }

    #[test]
    fn dynamic_hls_routes_are_registered_with_official_prefixed_and_plain_paths() {
        let routes = include_str!("mod.rs");

        for route in [
            "\"/emby/Videos/ActiveEncodings\"",
            "\"/Videos/ActiveEncodings\"",
            "\"/emby/Videos/{item_id}/master.m3u8\"",
            "\"/emby/Videos/{item_id}/main.m3u8\"",
            "\"/emby/Videos/{item_id}/live.m3u8\"",
            "\"/emby/Audio/{item_id}/master.m3u8\"",
            "\"/emby/Audio/{item_id}/main.m3u8\"",
            "\"/emby/Audio/{item_id}/live.m3u8\"",
            "\"/Audio/{item_id}/master.m3u8\"",
            "\"/emby/Videos/{item_id}/hls1/{playlist_id}/{segment_file_name}\"",
            "\"/emby/Audio/{item_id}/hls1/{playlist_id}/{segment_file_name}\"",
            "\"/emby/Videos/{item_id}/subtitles.m3u8\"",
            "\"/emby/Videos/{item_id}/live_subtitles.m3u8\"",
        ] {
            assert!(routes.contains(route), "missing route {route}");
        }
    }
}
