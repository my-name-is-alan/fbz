create index if not exists idx_event_outbox_stream_mirror_created
    on event_outbox (created_at asc, id asc)
    where stream_mirrored_at is null;
