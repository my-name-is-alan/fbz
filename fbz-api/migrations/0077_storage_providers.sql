create table storage_accounts (
    id uuid primary key default gen_random_uuid(),
    provider text not null check (provider = 'guangya'),
    name text not null,
    device_id text not null,
    cloud_user_id text,
    status text not null default 'pending',
    secret_nonce bytea,
    secret_ciphertext bytea,
    version bigint not null default 0,
    login_epoch bigint not null default 0,
    qps integer not null default 1 check (qps between 1 and 5),
    next_request_at timestamptz not null default now(),
    created_at timestamptz not null default now(),
    updated_at timestamptz not null default now()
);
create table storage_login_attempts (
    id uuid primary key default gen_random_uuid(),
    account_id uuid not null references storage_accounts(id),
    nonce bytea not null,
    ciphertext bytea not null,
    interval_seconds integer not null default 5,
    login_epoch bigint not null,
    next_poll_at timestamptz not null default now(),
    expires_at timestamptz not null,
    finished boolean not null default false
);
create table storage_mounts (
    id uuid primary key default gen_random_uuid(),
    account_id uuid not null references storage_accounts(id),
    library_id bigint not null unique references libraries(id),
    root_id text not null,
    display_path text not null,
    status text not null default 'idle',
    generation bigint not null default 0,
    frontier jsonb not null default '[]',
    import_cursor text not null default '',
    scanned bigint not null default 0,
    imported bigint not null default 0,
    last_error text,
    updated_at timestamptz not null default now()
);
create table storage_entries (
    mount_id uuid not null references storage_mounts(id),
    remote_id text not null,
    parent_id text not null,
    name text not null,
    entry jsonb not null,
    generation bigint not null,
    media_item_id bigint references media_items(id),
    media_file_id bigint references media_files(id),
    imported_version text,
    primary key(mount_id, remote_id)
);
create index storage_entries_parent on storage_entries(mount_id, parent_id);
create index storage_entries_media_file on storage_entries(media_file_id) where media_file_id is not null;
create table storage_subtitles (
    media_file_id bigint not null references media_files(id),
    stream_index integer not null,
    storage_key text not null,
    codec text not null,
    primary key(media_file_id, stream_index)
);
create table storage_groups (
    mount_id uuid not null references storage_mounts(id),
    group_key text not null,
    media_item_id bigint not null references media_items(id),
    primary key(mount_id, group_key)
);
