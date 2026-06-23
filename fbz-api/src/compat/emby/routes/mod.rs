use axum::{
    Router,
    routing::{get, post},
};

use crate::state::AppState;

mod access;
mod artists;
mod branding;
mod devices;
mod display_preferences;
mod genres;
mod images;
mod items;
mod media_folders;
mod persons;
mod playback;
mod scheduled_tasks;
mod sessions;
mod shows;
mod streaming;
mod subtitles;
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
        .route(
            "/emby/System/Configuration",
            get(system::system_configuration),
        )
        .route("/System/Configuration", get(system::system_configuration))
        .route("/emby/System/WakeOnLanInfo", get(system::wake_on_lan_info))
        .route("/System/WakeOnLanInfo", get(system::wake_on_lan_info))
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
        .route(
            "/emby/DisplayPreferences/{item_id}",
            get(display_preferences::display_preferences),
        )
        .route(
            "/DisplayPreferences/{item_id}",
            get(display_preferences::display_preferences),
        )
        .route("/emby/Users/Public", get(users::public_users))
        .route("/Users/Public", get(users::public_users))
        .route("/emby/Users/Me", get(users::current_user))
        .route("/Users/Me", get(users::current_user))
        .route("/emby/Users/{user_id}", get(users::user_by_id))
        .route("/Users/{user_id}", get(users::user_by_id))
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
        .route("/emby/Sessions", get(sessions::list_sessions))
        .route("/Sessions", get(sessions::list_sessions))
        .route("/emby/Sessions/Logout", post(sessions::logout))
        .route("/Sessions/Logout", post(sessions::logout))
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
        .route("/emby/Sessions/Playing", post(playback::playing))
        .route("/Sessions/Playing", post(playback::playing))
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
            "/emby/videos/{item_id}/{file_name}",
            get(transcoding::hls_file),
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
            "/emby/Audio/{item_id}/{stream_file_name}",
            get(streaming::audio_stream),
        )
        .route(
            "/Audio/{item_id}/{stream_file_name}",
            get(streaming::audio_stream),
        )
        .route("/videos/{item_id}/{file_name}", get(transcoding::hls_file))
        .route("/Videos/{item_id}/{file_name}", get(transcoding::hls_file))
        .route("/emby/Shows/NextUp", get(shows::next_up))
        .route("/Shows/NextUp", get(shows::next_up))
        .route("/emby/Shows/{series_id}/Seasons", get(shows::seasons))
        .route("/Shows/{series_id}/Seasons", get(shows::seasons))
        .route("/emby/Shows/{series_id}/Episodes", get(shows::episodes))
        .route("/Shows/{series_id}/Episodes", get(shows::episodes))
        .route("/emby/Users/{user_id}/Views", get(views::user_views))
        .route("/Users/{user_id}/Views", get(views::user_views))
        .route("/emby/Genres", get(genres::genres))
        .route("/Genres", get(genres::genres))
        .route("/emby/Genres/{name}", get(genres::genre_by_name))
        .route("/Genres/{name}", get(genres::genre_by_name))
        .route("/emby/MusicGenres", get(genres::music_genres))
        .route("/MusicGenres", get(genres::music_genres))
        .route("/emby/MusicGenres/{name}", get(genres::music_genre_by_name))
        .route("/MusicGenres/{name}", get(genres::music_genre_by_name))
        .route("/emby/Persons", get(persons::persons))
        .route("/Persons", get(persons::persons))
        .route("/emby/Persons/{name}", get(persons::person_by_name))
        .route("/Persons/{name}", get(persons::person_by_name))
        .route("/emby/Artists", get(artists::artists))
        .route("/Artists", get(artists::artists))
        .route("/emby/Artists/AlbumArtists", get(artists::album_artists))
        .route("/Artists/AlbumArtists", get(artists::album_artists))
        .route("/emby/Artists/{item_id}/Similar", get(items::similar_items))
        .route("/Artists/{item_id}/Similar", get(items::similar_items))
        .route("/emby/Artists/{name}", get(artists::artist_by_name))
        .route("/Artists/{name}", get(artists::artist_by_name))
        .route("/emby/Albums/{item_id}/Similar", get(items::similar_items))
        .route("/Albums/{item_id}/Similar", get(items::similar_items))
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
        .route("/emby/Users/{user_id}/Items", get(items::user_items))
        .route("/Users/{user_id}/Items", get(items::user_items))
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
        .route(
            "/emby/Users/{user_id}/Items/{item_id}",
            get(items::user_item_by_id),
        )
        .route(
            "/Users/{user_id}/Items/{item_id}",
            get(items::user_item_by_id),
        )
        .route("/emby/Items/Counts", get(items::item_counts))
        .route("/Items/Counts", get(items::item_counts))
        .route(
            "/emby/Items/{item_id}/Images/{image_type}",
            get(images::item_image),
        )
        .route(
            "/Items/{item_id}/Images/{image_type}",
            get(images::item_image),
        )
        .route(
            "/emby/Items/{item_id}/Images/{image_type}/{index}",
            get(images::item_image_index),
        )
        .route(
            "/Items/{item_id}/Images/{image_type}/{index}",
            get(images::item_image_index),
        )
        .route(
            "/emby/Items/{item_id}/Ancestors",
            get(items::item_ancestors),
        )
        .route("/Items/{item_id}/Ancestors", get(items::item_ancestors))
        .route("/emby/Items/{item_id}/Similar", get(items::similar_items))
        .route("/Items/{item_id}/Similar", get(items::similar_items))
        .route(
            "/emby/Items/{item_id}/RemoteSearch/Subtitles/{language}",
            get(subtitles::remote_subtitle_search),
        )
        .route(
            "/Items/{item_id}/RemoteSearch/Subtitles/{language}",
            get(subtitles::remote_subtitle_search),
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
            "/emby/Items/{item_id}/Download",
            get(streaming::item_download),
        )
        .route("/Items/{item_id}/Download", get(streaming::item_download))
        .route(
            "/emby/Users/{user_id}/Items/{item_id}/PlaybackInfo",
            get(playback::user_playback_info).post(playback::post_user_playback_info),
        )
        .route(
            "/Users/{user_id}/Items/{item_id}/PlaybackInfo",
            get(playback::user_playback_info).post(playback::post_user_playback_info),
        )
}
