create table if not exists plugin_host_tokens (
    id bigserial primary key,
    public_id uuid not null default gen_random_uuid(),
    token_hash bytea not null,
    token_prefix text not null,
    plugin_id text not null references plugin_installations(plugin_id) on delete cascade,
    package_id text not null,
    execution_run_id bigint references plugin_execution_runs(id) on delete cascade,
    scope text not null default 'execution',
    created_at timestamptz not null default now(),
    expires_at timestamptz not null,
    last_used_at timestamptz,
    revoked_at timestamptz,
    unique (public_id),
    unique (token_hash),
    check (length(token_hash) = 32),
    check (length(trim(token_prefix)) > 0),
    check (length(trim(plugin_id)) > 0),
    check (length(trim(package_id)) > 0),
    check (scope in ('execution')),
    check (expires_at > created_at)
);

create index if not exists idx_plugin_host_tokens_active_hash
    on plugin_host_tokens (token_hash, expires_at)
    where revoked_at is null;

create index if not exists idx_plugin_host_tokens_plugin_created
    on plugin_host_tokens (plugin_id, created_at desc);
