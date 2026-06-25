-- Partition the scheduled task run history by started_at (monthly RANGE).
--
-- Category B from docs/database-partitioning-design.md: a leaf table (no inbound
-- foreign keys) with a `public_id` UNIQUE constraint relaxed to a non-unique
-- index (random uuid; the run insert is a plain INSERT with no ON CONFLICT).
-- started_at is set once at insert and never mutated, so rows never move between
-- partitions; the scheduler's claim/lease/update queries (update by id, lease
-- recovery on status='running') stay correct via per-partition indexes.
-- Structural change, confirmed. Build via aside-rename swap; runs in the
-- migration transaction.

alter sequence scheduled_task_runs_id_seq owned by none;

alter table scheduled_task_runs rename to scheduled_task_runs_legacy;
alter table scheduled_task_runs_legacy rename constraint scheduled_task_runs_pkey to str_legacy_pkey;
alter table scheduled_task_runs_legacy rename constraint scheduled_task_runs_check to str_legacy_check;
alter table scheduled_task_runs_legacy rename constraint scheduled_task_runs_check1 to str_legacy_check1;
alter table scheduled_task_runs_legacy rename constraint scheduled_task_runs_queued_jobs_check to str_legacy_queued_jobs_check;
alter table scheduled_task_runs_legacy rename constraint scheduled_task_runs_status_check to str_legacy_status_check;
alter table scheduled_task_runs_legacy rename constraint scheduled_task_runs_task_key_check to str_legacy_task_key_check;
alter table scheduled_task_runs_legacy rename constraint scheduled_task_runs_trigger_type_check to str_legacy_trigger_type_check;
alter table scheduled_task_runs_legacy rename constraint scheduled_task_runs_worker_id_check to str_legacy_worker_id_check;
alter table scheduled_task_runs_legacy rename constraint scheduled_task_runs_task_id_fkey to str_legacy_task_id_fkey;
alter index scheduled_task_runs_public_id_key rename to str_legacy_public_id_key;
alter index idx_scheduled_task_runs_active rename to idx_str_legacy_active;
alter index idx_scheduled_task_runs_task_recent rename to idx_str_legacy_task_recent;
alter index idx_scheduled_task_runs_task_started rename to idx_str_legacy_task_started;
alter index idx_scheduled_task_runs_task_status_recent_keyset rename to idx_str_legacy_task_status_recent_keyset;

create table scheduled_task_runs (
    id bigint not null default nextval('scheduled_task_runs_id_seq'),
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
    primary key (id, started_at),
    check (lease_expires_at > started_at),
    check (finished_at is null or finished_at >= started_at),
    check (queued_jobs is null or queued_jobs >= 0),
    check (status in ('running', 'succeeded', 'failed', 'expired', 'cancelled')),
    check (length(trim(task_key)) > 0),
    check (trigger_type in ('due', 'manual')),
    check (length(trim(worker_id)) > 0)
) partition by range (started_at);

alter sequence scheduled_task_runs_id_seq owned by scheduled_task_runs.id;

create table scheduled_task_runs_2026m06 partition of scheduled_task_runs
    for values from ('2026-06-01') to ('2026-07-01');
create table scheduled_task_runs_2026m07 partition of scheduled_task_runs
    for values from ('2026-07-01') to ('2026-08-01');
create table scheduled_task_runs_2026m08 partition of scheduled_task_runs
    for values from ('2026-08-01') to ('2026-09-01');
create table scheduled_task_runs_default partition of scheduled_task_runs default;

-- public_id: relaxed from UNIQUE to a plain index (see header).
create index idx_scheduled_task_runs_public_id on scheduled_task_runs (public_id);

-- Recreate indexes under their original names (cascade to partitions).
create index idx_scheduled_task_runs_active on scheduled_task_runs (task_id, lease_expires_at)
    where status = 'running';
create index idx_scheduled_task_runs_task_recent
    on scheduled_task_runs (task_id, started_at desc, id desc) include (public_id);
create index idx_scheduled_task_runs_task_started
    on scheduled_task_runs (task_id, started_at desc);
create index idx_scheduled_task_runs_task_status_recent_keyset
    on scheduled_task_runs (task_id, status, started_at desc, id desc);

-- Backfill existing rows (routed into partitions), preserving ids and public_ids.
insert into scheduled_task_runs (
    id, public_id, task_id, task_key, trigger_type, worker_id, status, lease_expires_at,
    queued_jobs, error_message, started_at, finished_at, created_at, updated_at
)
select id, public_id, task_id, task_key, trigger_type, worker_id, status, lease_expires_at,
    queued_jobs, error_message, started_at, finished_at, created_at, updated_at
from scheduled_task_runs_legacy;

select setval(
    'scheduled_task_runs_id_seq',
    (select coalesce(max(id), 0) from scheduled_task_runs) + 1,
    false
);

drop table scheduled_task_runs_legacy;
