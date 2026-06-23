create table if not exists plugin_execution_runs (
    id bigserial primary key,
    public_id uuid not null default gen_random_uuid(),
    outbox_event_id bigint references event_outbox(id) on delete set null,
    outbox_event_public_id text not null,
    attempt integer not null,
    plugin_id text not null references plugin_installations(plugin_id) on delete cascade,
    package_id text not null,
    hook_id bigint references plugin_hooks(id) on delete set null,
    handler text not null,
    event_key text not null,
    runtime text not null,
    entrypoint text not null,
    status text not null default 'running',
    request_payload jsonb not null default '{}'::jsonb,
    response_status integer,
    response_body text,
    error_message text,
    started_at timestamptz not null default now(),
    finished_at timestamptz,
    duration_ms integer,
    unique (public_id),
    unique (outbox_event_public_id, attempt),
    check (attempt > 0),
    check (length(trim(outbox_event_public_id)) > 0),
    check (length(trim(plugin_id)) > 0),
    check (length(trim(package_id)) > 0),
    check (length(trim(handler)) > 0),
    check (length(trim(event_key)) > 0),
    check (runtime in ('wasi', 'http')),
    check (length(trim(entrypoint)) > 0),
    check (status in ('running', 'succeeded', 'failed')),
    check (jsonb_typeof(request_payload) = 'object'),
    check (response_status is null or response_status between 100 and 599),
    check (duration_ms is null or duration_ms >= 0),
    check (finished_at is null or finished_at >= started_at)
);

create index if not exists idx_plugin_execution_runs_plugin_started
    on plugin_execution_runs (plugin_id, started_at desc);

create index if not exists idx_plugin_execution_runs_status_started
    on plugin_execution_runs (status, started_at desc);
