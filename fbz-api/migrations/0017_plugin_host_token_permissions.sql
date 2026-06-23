alter table plugin_host_tokens
    add column if not exists permission_snapshot jsonb not null default '[]'::jsonb;

alter table plugin_host_tokens
    add constraint plugin_host_tokens_permission_snapshot_array
    check (jsonb_typeof(permission_snapshot) = 'array') not valid;

alter table plugin_host_tokens
    validate constraint plugin_host_tokens_permission_snapshot_array;

create index if not exists idx_plugin_host_tokens_execution_active
    on plugin_host_tokens (execution_run_id, expires_at)
    where revoked_at is null;
