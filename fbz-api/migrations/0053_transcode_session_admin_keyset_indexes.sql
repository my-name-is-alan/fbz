create index if not exists idx_transcoding_sessions_admin_recent_keyset
    on transcoding_sessions (
        (case status when 'running' then 0 when 'queued' then 1 when 'failed' then 2 else 3 end),
        created_at desc,
        id desc
    );

create index if not exists idx_transcoding_sessions_admin_status_recent_keyset
    on transcoding_sessions (status, created_at desc, id desc);

create index if not exists idx_transcoding_sessions_admin_hardware_recent_keyset
    on transcoding_sessions (
        hardware_acceleration,
        (case status when 'running' then 0 when 'queued' then 1 when 'failed' then 2 else 3 end),
        created_at desc,
        id desc
    )
    where hardware_acceleration is not null;

create index if not exists idx_transcoding_sessions_admin_status_hardware_recent_keyset
    on transcoding_sessions (status, hardware_acceleration, created_at desc, id desc)
    where hardware_acceleration is not null;
