create index if not exists idx_job_runs_job_started_keyset
    on job_runs (job_id, started_at desc, id desc);

create index if not exists idx_job_runs_job_status_started_keyset
    on job_runs (job_id, status, started_at desc, id desc);

create index if not exists idx_job_events_job_created_keyset
    on job_events (job_id, created_at desc, id desc);

create index if not exists idx_job_events_job_level_created_keyset
    on job_events (job_id, event_level, created_at desc, id desc);
