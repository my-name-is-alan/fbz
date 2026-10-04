-- Plugin synchronous invocation audit.
--
-- Unlike async outbox dispatches (event_outbox + plugin_execution_runs), sync
-- invocations are made inline by core services (metadata provider queries,
-- storage provider calls, ...) and complete before the row is written, so the
-- table stores terminal outcomes only: no lease columns, no running state, and
-- therefore no stale-recovery machinery.
--
-- Volume control: failures are always audited; successes are audited only when
-- the caller opts in, so hot provider paths do not write one row per call.

create table if not exists plugin_sync_invocations (
    id bigserial primary key,
    public_id uuid not null default gen_random_uuid(),
    plugin_id text not null,
    package_id text not null,
    capability text not null,
    handler text not null,
    entrypoint text not null,
    status text not null,
    response_status integer,
    error_message text,
    duration_ms integer not null default 0,
    created_at timestamptz not null default now(),
    unique (public_id),
    check (status in ('succeeded', 'failed')),
    check (length(trim(plugin_id)) between 1 and 128),
    check (length(trim(capability)) between 1 and 128),
    check (duration_ms >= 0)
);

create index if not exists idx_plugin_sync_invocations_recent
    on plugin_sync_invocations (created_at desc, id desc);

create index if not exists idx_plugin_sync_invocations_plugin_recent
    on plugin_sync_invocations (plugin_id, created_at desc, id desc);

create index if not exists idx_plugin_sync_invocations_capability_recent
    on plugin_sync_invocations (capability, created_at desc, id desc);
