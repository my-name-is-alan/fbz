create index if not exists idx_jobs_admin_recent_keyset
    on jobs (created_at desc, id desc);

create index if not exists idx_jobs_admin_status_recent_keyset
    on jobs (status, created_at desc, id desc);

create index if not exists idx_jobs_admin_type_recent_keyset
    on jobs (job_type, created_at desc, id desc);

create index if not exists idx_jobs_admin_queue_recent_keyset
    on jobs (queue_name, created_at desc, id desc);
