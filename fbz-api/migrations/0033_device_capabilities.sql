alter table devices
    add column if not exists playable_media_types text[] not null default '{}',
    add column if not exists supported_commands text[] not null default '{}',
    add column if not exists supports_media_control boolean not null default false,
    add column if not exists supports_sync boolean not null default false,
    add column if not exists push_token text,
    add column if not exists push_token_type text,
    add column if not exists icon_url text,
    add column if not exists app_id text,
    add column if not exists device_profile jsonb,
    add column if not exists capabilities_updated_at timestamptz;
