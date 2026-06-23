create table if not exists plugin_host_api_calls (
    id bigserial primary key,
    public_id uuid not null default gen_random_uuid(),
    plugin_id text not null references plugin_installations(plugin_id) on delete cascade,
    package_id text not null,
    host_token_id bigint references plugin_host_tokens(id) on delete set null,
    execution_run_id bigint references plugin_execution_runs(id) on delete set null,
    method text not null,
    path text not null,
    required_permission text,
    status_code integer not null,
    error_code text,
    error_message text,
    started_at timestamptz not null,
    finished_at timestamptz not null default now(),
    duration_ms integer not null,
    unique (public_id),
    check (length(trim(plugin_id)) > 0),
    check (length(trim(package_id)) > 0),
    check (method in ('GET', 'POST', 'PUT', 'DELETE')),
    check (length(trim(path)) > 0),
    check (required_permission is null or length(trim(required_permission)) > 0),
    check (status_code between 100 and 599),
    check (error_code is null or length(trim(error_code)) > 0),
    check (duration_ms >= 0),
    check (finished_at >= started_at)
);

create index if not exists idx_plugin_host_api_calls_plugin_finished
    on plugin_host_api_calls (plugin_id, finished_at desc);

create index if not exists idx_plugin_host_api_calls_execution_finished
    on plugin_host_api_calls (execution_run_id, finished_at desc);

create index if not exists idx_plugin_host_api_calls_status_finished
    on plugin_host_api_calls (status_code, finished_at desc);
