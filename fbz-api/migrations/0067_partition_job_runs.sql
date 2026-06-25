-- Partition job_runs by started_at (monthly RANGE).
--
-- Category C from docs/database-partitioning-design.md: job_runs has an inbound
-- FK (job_events.job_run_id -> job_runs(id) ON DELETE SET NULL). A partitioned
-- table's id is no longer uniquely referenceable, so the inbound FK is dropped
-- first. This is safe: job_runs are only ever deleted via the jobs cascade
-- (jobs -> job_runs ON DELETE CASCADE), which ALSO deletes the referencing
-- job_events (job_events.job_id -> jobs ON DELETE CASCADE) — so the SET NULL
-- path is never exercised, and the app only ever writes valid job_run_ids
-- (created by start_job_run before the event). job_events.job_run_id is kept as
-- a plain column. Structural change, confirmed.
--
-- job_runs has no public_id, so (like job_events) no uniqueness relaxation is
-- needed. started_at is set once at insert and never mutated. Aside-rename swap,
-- in the migration transaction.

-- 1. Drop the inbound FK (job_events is already partitioned; dropping the
--    constraint on the parent cascades to its partitions). The column remains.
alter table job_events drop constraint job_events_job_run_id_fkey;

-- 2. Partition job_runs.
alter sequence job_runs_id_seq owned by none;

alter table job_runs rename to job_runs_legacy;
alter table job_runs_legacy rename constraint job_runs_pkey to job_runs_legacy_pkey;
alter table job_runs_legacy rename constraint job_runs_check to job_runs_legacy_check;
alter table job_runs_legacy rename constraint job_runs_status_check to job_runs_legacy_status_check;
alter table job_runs_legacy rename constraint job_runs_worker_id_check to job_runs_legacy_worker_id_check;
alter table job_runs_legacy rename constraint job_runs_job_id_fkey to job_runs_legacy_job_id_fkey;
alter index idx_job_runs_job_started rename to idx_job_runs_legacy_job_started;
alter index idx_job_runs_job_started_keyset rename to idx_job_runs_legacy_job_started_keyset;
alter index idx_job_runs_job_status_started_keyset rename to idx_job_runs_legacy_job_status_started_keyset;

create table job_runs (
    id bigint not null default nextval('job_runs_id_seq'),
    job_id bigint not null references jobs(id) on delete cascade,
    worker_id text not null,
    status text not null default 'running',
    started_at timestamptz not null default now(),
    finished_at timestamptz,
    error_message text,
    metrics jsonb not null default '{}'::jsonb,
    primary key (id, started_at),
    check (finished_at is null or finished_at >= started_at),
    check (status in ('running', 'succeeded', 'failed', 'cancelled')),
    check (length(trim(worker_id)) > 0)
) partition by range (started_at);

alter sequence job_runs_id_seq owned by job_runs.id;

create table job_runs_2026m06 partition of job_runs
    for values from ('2026-06-01') to ('2026-07-01');
create table job_runs_2026m07 partition of job_runs
    for values from ('2026-07-01') to ('2026-08-01');
create table job_runs_2026m08 partition of job_runs
    for values from ('2026-08-01') to ('2026-09-01');
create table job_runs_default partition of job_runs default;

create index idx_job_runs_job_started on job_runs (job_id, started_at desc);
create index idx_job_runs_job_started_keyset on job_runs (job_id, started_at desc, id desc);
create index idx_job_runs_job_status_started_keyset
    on job_runs (job_id, status, started_at desc, id desc);

insert into job_runs (
    id, job_id, worker_id, status, started_at, finished_at, error_message, metrics
)
select id, job_id, worker_id, status, started_at, finished_at, error_message, metrics
from job_runs_legacy;

select setval(
    'job_runs_id_seq',
    (select coalesce(max(id), 0) from job_runs) + 1,
    false
);

drop table job_runs_legacy;
