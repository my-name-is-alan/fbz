create extension if not exists pgcrypto;

create table if not exists roles (
    id bigserial primary key,
    public_id uuid not null default gen_random_uuid(),
    name text not null,
    name_normalized text not null,
    description text,
    is_builtin boolean not null default false,
    created_at timestamptz not null default now(),
    updated_at timestamptz not null default now(),
    unique (public_id),
    unique (name_normalized),
    check (length(trim(name)) > 0),
    check (length(trim(name_normalized)) > 0)
);

create table if not exists users (
    id bigserial primary key,
    public_id uuid not null default gen_random_uuid(),
    username text not null,
    username_normalized text not null,
    password_hash text,
    display_name text,
    role_id bigint not null references roles(id),
    is_disabled boolean not null default false,
    allow_download boolean not null default false,
    allow_transcode boolean not null default true,
    allow_new_device_login boolean not null default true,
    created_at timestamptz not null default now(),
    updated_at timestamptz not null default now(),
    last_login_at timestamptz,
    unique (public_id),
    unique (username_normalized),
    check (length(trim(username)) > 0),
    check (length(trim(username_normalized)) > 0)
);

create table if not exists api_keys (
    id bigserial primary key,
    public_id uuid not null default gen_random_uuid(),
    user_id bigint references users(id) on delete cascade,
    name text not null,
    token_hash bytea not null,
    token_prefix text not null,
    scopes text[] not null default '{}',
    expires_at timestamptz,
    revoked_at timestamptz,
    created_at timestamptz not null default now(),
    last_used_at timestamptz,
    unique (public_id),
    unique (token_hash),
    check (length(trim(name)) > 0),
    check (length(trim(token_prefix)) > 0)
);

create table if not exists devices (
    id bigserial primary key,
    public_id uuid not null default gen_random_uuid(),
    user_id bigint references users(id) on delete cascade,
    device_id text not null,
    device_name text,
    client_name text,
    client_version text,
    last_seen_at timestamptz,
    revoked_at timestamptz,
    created_at timestamptz not null default now(),
    unique (public_id),
    unique (user_id, device_id),
    check (length(trim(device_id)) > 0)
);

create table if not exists sessions (
    id bigserial primary key,
    public_id uuid not null default gen_random_uuid(),
    user_id bigint not null references users(id) on delete cascade,
    device_id bigint references devices(id) on delete set null,
    access_token_hash bytea not null,
    remote_addr inet,
    user_agent text,
    expires_at timestamptz not null,
    revoked_at timestamptz,
    created_at timestamptz not null default now(),
    last_seen_at timestamptz,
    unique (public_id),
    unique (access_token_hash)
);

create table if not exists libraries (
    id bigserial primary key,
    public_id uuid not null default gen_random_uuid(),
    name text not null,
    library_type text not null,
    preferred_metadata_language text,
    preferred_metadata_country text,
    is_hidden boolean not null default false,
    created_at timestamptz not null default now(),
    updated_at timestamptz not null default now(),
    unique (public_id),
    unique (name),
    check (length(trim(name)) > 0),
    check (library_type in ('movies', 'tvshows', 'music', 'homevideos', 'mixed', 'livetv'))
);

create table if not exists library_paths (
    id bigserial primary key,
    library_id bigint not null references libraries(id) on delete cascade,
    path text not null,
    normalized_path text not null,
    path_hash bytea not null,
    is_enabled boolean not null default true,
    created_at timestamptz not null default now(),
    updated_at timestamptz not null default now(),
    unique (library_id, path_hash),
    check (length(trim(path)) > 0),
    check (length(path_hash) = 32)
);

create table if not exists library_permissions (
    id bigserial primary key,
    library_id bigint not null references libraries(id) on delete cascade,
    user_id bigint not null references users(id) on delete cascade,
    can_view boolean not null default true,
    can_download boolean not null default false,
    can_transcode boolean not null default true,
    created_at timestamptz not null default now(),
    updated_at timestamptz not null default now(),
    unique (library_id, user_id)
);

create table if not exists media_items (
    id bigserial primary key,
    public_id uuid not null default gen_random_uuid(),
    library_id bigint not null references libraries(id) on delete cascade,
    parent_id bigint references media_items(id) on delete cascade,
    item_type text not null,
    title text not null,
    original_title text,
    sort_title text,
    overview text,
    production_year integer,
    premiere_date date,
    community_rating numeric(3, 1),
    critic_rating numeric(5, 2),
    runtime_ticks bigint,
    index_number integer,
    parent_index_number integer,
    season_number integer,
    episode_number integer,
    provider_fingerprint text,
    metadata_status text not null default 'pending',
    scan_status text not null default 'pending',
    is_virtual boolean not null default false,
    is_deleted boolean not null default false,
    created_at timestamptz not null default now(),
    updated_at timestamptz not null default now(),
    search_vector tsvector generated always as (
        to_tsvector(
            'simple',
            coalesce(title, '') || ' ' ||
            coalesce(original_title, '') || ' ' ||
            coalesce(sort_title, '')
        )
    ) stored,
    unique (public_id),
    check (length(trim(title)) > 0),
    check (item_type in ('folder', 'movie', 'series', 'season', 'episode', 'artist', 'album', 'track', 'collection', 'photo', 'video', 'tvchannel', 'program', 'recording')),
    check (metadata_status in ('pending', 'matched', 'manual', 'failed')),
    check (scan_status in ('pending', 'scanned', 'missing', 'failed')),
    check (runtime_ticks is null or runtime_ticks >= 0)
);

create table if not exists media_files (
    id bigserial primary key,
    media_item_id bigint not null references media_items(id) on delete cascade,
    library_path_id bigint references library_paths(id) on delete set null,
    path text not null,
    normalized_path text not null,
    path_hash bytea not null,
    file_size bigint,
    modified_at timestamptz,
    content_hash bytea,
    container text,
    duration_ticks bigint,
    bitrate integer,
    is_primary boolean not null default true,
    is_strm boolean not null default false,
    strm_target text,
    created_at timestamptz not null default now(),
    updated_at timestamptz not null default now(),
    unique (path_hash),
    check (length(trim(path)) > 0),
    check (length(path_hash) = 32),
    check (file_size is null or file_size >= 0),
    check (duration_ticks is null or duration_ticks >= 0),
    check (bitrate is null or bitrate >= 0)
);

create table if not exists media_streams (
    id bigserial primary key,
    media_file_id bigint not null references media_files(id) on delete cascade,
    stream_index integer not null,
    stream_type text not null,
    codec text,
    codec_tag text,
    language text,
    title text,
    profile text,
    level integer,
    width integer,
    height integer,
    channels integer,
    sample_rate integer,
    bit_depth integer,
    bitrate integer,
    is_default boolean not null default false,
    is_forced boolean not null default false,
    is_external boolean not null default false,
    extra jsonb not null default '{}'::jsonb,
    unique (media_file_id, stream_index),
    check (stream_index >= 0),
    check (stream_type in ('video', 'audio', 'subtitle', 'attachment', 'data')),
    check (width is null or width > 0),
    check (height is null or height > 0),
    check (channels is null or channels > 0),
    check (sample_rate is null or sample_rate > 0),
    check (bitrate is null or bitrate >= 0)
);

create table if not exists media_markers (
    id bigserial primary key,
    media_item_id bigint not null references media_items(id) on delete cascade,
    marker_type text not null,
    start_ticks bigint not null,
    end_ticks bigint,
    source text not null default 'manual',
    confidence numeric(5, 4),
    created_at timestamptz not null default now(),
    unique (media_item_id, marker_type, start_ticks, source),
    check (marker_type in ('intro_start', 'intro_end', 'credits_start', 'credits_end', 'commercial', 'chapter')),
    check (start_ticks >= 0),
    check (end_ticks is null or end_ticks >= start_ticks),
    check (confidence is null or (confidence >= 0 and confidence <= 1))
);

create table if not exists media_external_ids (
    id bigserial primary key,
    media_item_id bigint not null references media_items(id) on delete cascade,
    provider text not null,
    external_id text not null,
    created_at timestamptz not null default now(),
    unique (provider, external_id),
    unique (media_item_id, provider),
    check (length(trim(provider)) > 0),
    check (length(trim(external_id)) > 0)
);

create table if not exists people (
    id bigserial primary key,
    public_id uuid not null default gen_random_uuid(),
    name text not null,
    name_normalized text not null,
    birth_date date,
    death_date date,
    overview text,
    created_at timestamptz not null default now(),
    updated_at timestamptz not null default now(),
    unique (public_id),
    unique (name_normalized),
    check (length(trim(name)) > 0)
);

create table if not exists media_item_people (
    id bigserial primary key,
    media_item_id bigint not null references media_items(id) on delete cascade,
    person_id bigint not null references people(id) on delete cascade,
    role_type text not null,
    role_name text not null default '',
    sort_order integer not null default 0,
    unique (media_item_id, person_id, role_type, role_name),
    check (role_type in ('actor', 'director', 'writer', 'producer', 'composer', 'artist', 'guest_star'))
);

create table if not exists genres (
    id bigserial primary key,
    name text not null,
    name_normalized text not null,
    unique (name_normalized),
    check (length(trim(name)) > 0)
);

create table if not exists media_item_genres (
    media_item_id bigint not null references media_items(id) on delete cascade,
    genre_id bigint not null references genres(id) on delete cascade,
    primary key (media_item_id, genre_id)
);

create table if not exists tags (
    id bigserial primary key,
    name text not null,
    name_normalized text not null,
    unique (name_normalized),
    check (length(trim(name)) > 0)
);

create table if not exists media_item_tags (
    media_item_id bigint not null references media_items(id) on delete cascade,
    tag_id bigint not null references tags(id) on delete cascade,
    primary key (media_item_id, tag_id)
);

create table if not exists artwork (
    id bigserial primary key,
    media_item_id bigint references media_items(id) on delete cascade,
    person_id bigint references people(id) on delete cascade,
    artwork_type text not null,
    source text not null,
    storage_key text,
    remote_url text,
    width integer,
    height integer,
    blurhash text,
    is_primary boolean not null default false,
    created_at timestamptz not null default now(),
    check ((media_item_id is not null) <> (person_id is not null)),
    check (artwork_type in ('primary', 'poster', 'backdrop', 'logo', 'thumb', 'banner', 'disc', 'artist', 'album')),
    check (length(trim(source)) > 0),
    check (storage_key is not null or remote_url is not null),
    check (width is null or width > 0),
    check (height is null or height > 0)
);

create table if not exists collections (
    id bigserial primary key,
    public_id uuid not null default gen_random_uuid(),
    library_id bigint references libraries(id) on delete cascade,
    name text not null,
    name_normalized text not null,
    overview text,
    created_at timestamptz not null default now(),
    updated_at timestamptz not null default now(),
    unique (public_id),
    unique (library_id, name_normalized),
    check (length(trim(name)) > 0)
);

create table if not exists collection_items (
    collection_id bigint not null references collections(id) on delete cascade,
    media_item_id bigint not null references media_items(id) on delete cascade,
    sort_order integer not null default 0,
    created_at timestamptz not null default now(),
    primary key (collection_id, media_item_id)
);
