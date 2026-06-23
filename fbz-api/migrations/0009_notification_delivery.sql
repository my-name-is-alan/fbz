alter table plugin_notification_requests
    add column if not exists last_error text;

alter table notification_targets
    add column if not exists channel text,
    add column if not exists last_error text,
    add column if not exists last_success_at timestamptz,
    add column if not exists last_failure_at timestamptz,
    add column if not exists delivery_count bigint not null default 0,
    add column if not exists failure_count bigint not null default 0;

do $$
begin
    if not exists (
        select 1
        from pg_constraint
        where conname = 'notification_targets_channel_len'
    ) then
        alter table notification_targets
            add constraint notification_targets_channel_len
            check (channel is null or length(trim(channel)) between 1 and 64);
    end if;

    if not exists (
        select 1
        from pg_constraint
        where conname = 'notification_targets_config_object'
    ) then
        alter table notification_targets
            add constraint notification_targets_config_object
            check (jsonb_typeof(config) = 'object');
    end if;

    if not exists (
        select 1
        from pg_constraint
        where conname = 'notification_targets_delivery_count_non_negative'
    ) then
        alter table notification_targets
            add constraint notification_targets_delivery_count_non_negative
            check (delivery_count >= 0);
    end if;

    if not exists (
        select 1
        from pg_constraint
        where conname = 'notification_targets_failure_count_non_negative'
    ) then
        alter table notification_targets
            add constraint notification_targets_failure_count_non_negative
            check (failure_count >= 0);
    end if;
end $$;

create table if not exists notification_delivery_attempts (
    id bigserial primary key,
    public_id uuid not null default gen_random_uuid(),
    notification_request_id bigint not null references plugin_notification_requests(id) on delete cascade,
    outbox_event_id bigint references event_outbox(id) on delete set null,
    target_id bigint references notification_targets(id) on delete set null,
    target_public_id uuid,
    target_type text not null,
    target_name text not null,
    attempt integer not null,
    status text not null default 'running',
    response_status integer,
    response_body text,
    error_message text,
    duration_ms integer,
    created_at timestamptz not null default now(),
    finished_at timestamptz,
    unique (public_id),
    check (length(trim(target_type)) > 0),
    check (length(trim(target_name)) > 0),
    check (attempt > 0),
    check (status in ('running', 'succeeded', 'failed', 'skipped')),
    check (duration_ms is null or duration_ms >= 0),
    check (finished_at is null or finished_at >= created_at)
);

create index if not exists idx_notification_targets_channel_enabled
    on notification_targets (channel, target_type, id)
    where is_enabled = true;

create index if not exists idx_notification_delivery_attempts_request_target
    on notification_delivery_attempts (notification_request_id, target_id, created_at desc);

create unique index if not exists idx_notification_delivery_attempts_target_success
    on notification_delivery_attempts (notification_request_id, target_id)
    where status = 'succeeded' and target_id is not null;
