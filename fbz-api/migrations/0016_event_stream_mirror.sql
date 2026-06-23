alter table event_outbox
    add column if not exists stream_mirrored_at timestamptz,
    add column if not exists stream_mirror_attempts integer not null default 0,
    add column if not exists stream_mirror_locked_by text,
    add column if not exists stream_mirror_locked_until timestamptz,
    add column if not exists stream_mirror_last_error text,
    add column if not exists stream_mirror_last_stream_id text;

alter table event_outbox
    add constraint event_outbox_stream_mirror_attempts_non_negative
    check (stream_mirror_attempts >= 0) not valid;

alter table event_outbox
    validate constraint event_outbox_stream_mirror_attempts_non_negative;

create index if not exists idx_event_outbox_stream_mirror_available
    on event_outbox (stream_mirror_locked_until, id)
    where stream_mirrored_at is null;
