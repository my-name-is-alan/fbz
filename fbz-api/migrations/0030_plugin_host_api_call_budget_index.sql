create index if not exists idx_plugin_host_api_calls_execution_plugin_budget
    on plugin_host_api_calls (execution_run_id, plugin_id, id)
    where execution_run_id is not null;
