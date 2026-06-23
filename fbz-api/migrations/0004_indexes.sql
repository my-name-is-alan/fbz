create index if not exists idx_users_role_id
    on users (role_id);

create index if not exists idx_api_keys_user_active
    on api_keys (user_id, revoked_at, expires_at);

create index if not exists idx_devices_user_last_seen
    on devices (user_id, last_seen_at desc);

create index if not exists idx_sessions_user_active
    on sessions (user_id, revoked_at, expires_at);

create index if not exists idx_library_permissions_user_library
    on library_permissions (user_id, library_id)
    include (can_view, can_download, can_transcode);

create index if not exists idx_library_paths_path_hash
    on library_paths (path_hash);

create index if not exists idx_media_items_library_type_parent
    on media_items (library_id, item_type, parent_id)
    where is_deleted = false;

create index if not exists idx_media_items_parent_sort
    on media_items (parent_id, sort_title, id)
    where is_deleted = false;

create index if not exists idx_media_items_created_at_desc
    on media_items (created_at desc)
    where is_deleted = false;

create index if not exists idx_media_items_library_latest
    on media_items (library_id, created_at desc, id desc)
    where is_deleted = false;

create index if not exists idx_media_items_search_vector
    on media_items using gin (search_vector);

create index if not exists idx_media_external_ids_item
    on media_external_ids (media_item_id);

create index if not exists idx_media_files_item_primary
    on media_files (media_item_id, is_primary);

create index if not exists idx_media_files_scan_dedupe
    on media_files (library_path_id, path_hash, file_size, modified_at);

create index if not exists idx_media_streams_file_type
    on media_streams (media_file_id, stream_type);

create index if not exists idx_media_markers_item_type
    on media_markers (media_item_id, marker_type, start_ticks);

create index if not exists idx_people_name_normalized
    on people (name_normalized);

create index if not exists idx_media_item_people_person
    on media_item_people (person_id, role_type);

create index if not exists idx_media_item_genres_genre
    on media_item_genres (genre_id, media_item_id);

create index if not exists idx_media_item_tags_tag
    on media_item_tags (tag_id, media_item_id);

create index if not exists idx_artwork_media_item_type
    on artwork (media_item_id, artwork_type, is_primary)
    where media_item_id is not null;

create index if not exists idx_artwork_person_type
    on artwork (person_id, artwork_type, is_primary)
    where person_id is not null;

create index if not exists idx_collection_items_item
    on collection_items (media_item_id, collection_id);

create index if not exists idx_jobs_status_run_at
    on jobs (status, run_at, priority desc, id)
    where status in ('queued', 'failed');

create index if not exists idx_jobs_locked_until
    on jobs (locked_until)
    where locked_until is not null;

create index if not exists idx_job_runs_job_started
    on job_runs (job_id, started_at desc);

create index if not exists idx_job_events_job_created
    on job_events (job_id, created_at desc);

create index if not exists idx_job_events_created_at
    on job_events (created_at desc);

create index if not exists idx_event_outbox_status_available
    on event_outbox (status, available_at, id)
    where status in ('pending', 'failed');

create index if not exists idx_event_outbox_aggregate
    on event_outbox (aggregate_type, aggregate_id, created_at desc);

create index if not exists idx_scheduled_tasks_enabled_next_run
    on scheduled_tasks (enabled, next_run_at)
    where enabled = true;

create index if not exists idx_webhook_subscriptions_enabled
    on webhook_subscriptions (is_enabled)
    where is_enabled = true;

create index if not exists idx_notification_targets_enabled
    on notification_targets (is_enabled, target_type)
    where is_enabled = true;

create index if not exists idx_playback_sessions_user_active
    on playback_sessions (user_id, stopped_at, last_progress_at desc);

create index if not exists idx_playback_sessions_item_active
    on playback_sessions (media_item_id, stopped_at, last_progress_at desc);

create index if not exists idx_user_playstates_continue
    on user_playstates (user_id, updated_at desc)
    where played = false and position_ticks > 0;

create index if not exists idx_transcoding_sessions_status_created
    on transcoding_sessions (status, created_at)
    where status in ('queued', 'running');

create index if not exists idx_transcoding_sessions_user_created
    on transcoding_sessions (user_id, created_at desc);
