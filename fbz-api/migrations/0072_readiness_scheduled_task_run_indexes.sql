-- Support `/ready` scheduler telemetry probes without exact-counting every
-- running scheduled task run across all partitions.
create index if not exists idx_scheduled_task_runs_readiness_running_lease
    on scheduled_task_runs (lease_expires_at, id)
    where status = 'running';

create index if not exists idx_scheduled_task_runs_readiness_manual_running_lease
    on scheduled_task_runs (trigger_type, lease_expires_at, id)
    where status = 'running';
