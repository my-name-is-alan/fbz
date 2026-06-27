create index if not exists idx_event_outbox_readiness_status
    on event_outbox (status, locked_until, id)
    where status in ('pending', 'delivering', 'failed');

create index if not exists idx_event_outbox_stream_mirror_failed
    on event_outbox (id)
    where stream_mirrored_at is null
      and stream_mirror_last_error is not null;
