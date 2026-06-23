create table if not exists plugin_notification_requests (
    id bigserial primary key,
    public_id uuid not null default gen_random_uuid(),
    plugin_id text not null references plugin_installations(plugin_id) on delete cascade,
    package_id text not null,
    title text not null,
    message text not null,
    level text not null default 'info',
    channel text,
    metadata jsonb not null default '{}'::jsonb,
    status text not null default 'queued',
    outbox_event_id bigint references event_outbox(id) on delete set null,
    created_at timestamptz not null default now(),
    updated_at timestamptz not null default now(),
    unique (public_id),
    check (length(trim(plugin_id)) > 0),
    check (length(trim(package_id)) > 0),
    check (length(trim(title)) between 1 and 160),
    check (length(trim(message)) between 1 and 4000),
    check (level in ('info', 'success', 'warning', 'error')),
    check (channel is null or length(trim(channel)) between 1 and 64),
    check (jsonb_typeof(metadata) = 'object'),
    check (status in ('queued', 'delivering', 'delivered', 'failed', 'discarded'))
);

create index if not exists idx_plugin_notification_requests_plugin_created
    on plugin_notification_requests (plugin_id, created_at desc);

create index if not exists idx_plugin_notification_requests_status_created
    on plugin_notification_requests (status, created_at)
    where status in ('queued', 'failed');
