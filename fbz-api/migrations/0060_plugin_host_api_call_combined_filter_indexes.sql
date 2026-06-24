create index if not exists idx_plugin_host_api_calls_plugin_status_finished_keyset
    on plugin_host_api_calls (plugin_id, status_code, finished_at desc, id desc);

create index if not exists idx_plugin_host_api_calls_execution_status_finished_keyset
    on plugin_host_api_calls (execution_run_id, status_code, finished_at desc, id desc)
    where execution_run_id is not null;
