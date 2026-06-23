create index if not exists idx_plugin_execution_runs_running_outbox
    on plugin_execution_runs (outbox_event_id, started_at)
    where status = 'running' and finished_at is null;

create index if not exists idx_plugin_host_tokens_active_execution_run
    on plugin_host_tokens (execution_run_id)
    where revoked_at is null;
