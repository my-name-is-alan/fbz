create index if not exists idx_plugin_execution_runs_dispatch_started_keyset
    on plugin_execution_runs (outbox_event_public_id, started_at desc, id desc);

create index if not exists idx_plugin_execution_runs_dispatch_status_started_keyset
    on plugin_execution_runs (outbox_event_public_id, status, started_at desc, id desc);
