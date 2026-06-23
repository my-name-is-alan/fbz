alter table transcoding_sessions
    add column if not exists worker_id text,
    add column if not exists lease_expires_at timestamptz,
    add column if not exists attempts integer not null default 0,
    add column if not exists max_attempts integer not null default 3,
    add column if not exists updated_at timestamptz not null default now();

alter table transcoding_sessions
    add constraint transcoding_sessions_attempts_check
    check (attempts >= 0) not valid;

alter table transcoding_sessions
    add constraint transcoding_sessions_max_attempts_check
    check (max_attempts > 0) not valid;

alter table transcoding_sessions
    add constraint transcoding_sessions_lease_worker_check
    check (lease_expires_at is null or worker_id is not null) not valid;

create index if not exists idx_transcoding_sessions_queue_claim
    on transcoding_sessions (created_at, id)
    where status = 'queued';

create index if not exists idx_transcoding_sessions_running_lease
    on transcoding_sessions (lease_expires_at, id)
    where status = 'running';
