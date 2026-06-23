create index if not exists idx_scheduled_task_runs_task_status_recent_keyset
    on scheduled_task_runs (task_id, status, started_at desc, id desc);
