create table if not exists scheduled_task_runs (
    id bigserial primary key,
    public_id uuid not null default gen_random_uuid(),
    task_id bigint not null references scheduled_tasks(id) on delete cascade,
    task_key text not null,
    trigger_type text not null,
    worker_id text not null,
    status text not null default 'running',
    lease_expires_at timestamptz not null,
    queued_jobs bigint,
    error_message text,
    started_at timestamptz not null default now(),
    finished_at timestamptz,
    created_at timestamptz not null default now(),
    updated_at timestamptz not null default now(),
    unique (public_id),
    check (length(trim(task_key)) > 0),
    check (trigger_type in ('due', 'manual')),
    check (length(trim(worker_id)) > 0),
    check (status in ('running', 'succeeded', 'failed', 'expired')),
    check (queued_jobs is null or queued_jobs >= 0),
    check (lease_expires_at > started_at),
    check (finished_at is null or finished_at >= started_at)
);

create index if not exists idx_scheduled_task_runs_active
    on scheduled_task_runs (task_id, lease_expires_at)
    where status = 'running';

create index if not exists idx_scheduled_task_runs_task_started
    on scheduled_task_runs (task_id, started_at desc);
