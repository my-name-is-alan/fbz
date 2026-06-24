alter table transcoding_sessions
    add column if not exists play_session_id text,
    add column if not exists device_id text;

create index if not exists idx_transcoding_sessions_active_encoding_cancel
    on transcoding_sessions (user_id, play_session_id, device_id)
    where status in ('queued', 'running')
      and play_session_id is not null;
