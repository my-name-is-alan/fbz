create index if not exists idx_scheduled_task_runs_task_recent
    on scheduled_task_runs (task_id, started_at desc, id desc)
    include (public_id);
