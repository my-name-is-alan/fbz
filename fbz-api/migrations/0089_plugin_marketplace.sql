-- Plugin marketplace: remote registry sources + cached catalog entries.
--
-- `plugin_market_sources` holds admin-configured remote registries. Each source
-- exposes a catalog JSON document at `url`. `plugin_market_entries` caches the
-- parsed catalog rows for browsing/searching without re-fetching the remote on
-- every request; a sync replaces the cached rows for a source atomically.

create table if not exists plugin_market_sources (
    id bigserial primary key,
    public_id uuid not null default gen_random_uuid(),
    name text not null,
    url text not null,
    enabled boolean not null default true,
    last_synced_at timestamptz,
    created_at timestamptz not null default now(),
    updated_at timestamptz not null default now(),
    unique (public_id),
    unique (url),
    check (length(trim(name)) between 1 and 200),
    check (length(url) between 1 and 2048),
    check (url ~* '^https?://')
);

create table if not exists plugin_market_entries (
    id bigserial primary key,
    source_id bigint not null references plugin_market_sources(id) on delete cascade,
    plugin_id text not null,
    name text not null,
    version text not null,
    description text,
    author text,
    permissions jsonb not null default '[]'::jsonb,
    icon_url text,
    download_url text not null,
    checksum_sha256 text,
    signature text,
    raw jsonb not null default '{}'::jsonb,
    synced_at timestamptz not null default now(),
    unique (source_id, plugin_id, version),
    check (length(trim(plugin_id)) between 1 and 128),
    check (length(trim(name)) > 0),
    check (length(trim(version)) > 0),
    check (length(download_url) between 1 and 2048),
    check (download_url ~* '^https?://'),
    check (checksum_sha256 is null or checksum_sha256 ~* '^[0-9a-f]{64}$'),
    check (jsonb_typeof(permissions) = 'array'),
    check (jsonb_typeof(raw) = 'object')
);

create index if not exists idx_plugin_market_entries_source_lookup
    on plugin_market_entries (source_id, plugin_id, version);

create index if not exists idx_plugin_market_entries_plugin
    on plugin_market_entries (plugin_id);

-- Uninstalling a plugin deletes its `plugin_installations` row. The plugin
-- dispatch/run audit tables denormalize `plugin_id` as text and must OUTLIVE the
-- installation so audit history is preserved. Relax their ON DELETE CASCADE
-- foreign keys to `plugin_installations(plugin_id)` (the text column stays; only
-- the referential cascade is removed). Ephemeral state tables (plugin_kv,
-- plugin_config_secrets, plugin_host_tokens) intentionally keep their cascade.
alter table plugin_execution_runs
    drop constraint if exists plugin_execution_runs_plugin_id_fkey;

alter table plugin_notification_requests
    drop constraint if exists plugin_notification_requests_plugin_id_fkey;

alter table plugin_host_api_calls
    drop constraint if exists plugin_host_api_calls_plugin_id_fkey;
