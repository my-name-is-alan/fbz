-- Partition the append-only job_events log by created_at (monthly RANGE).
--
-- First table from docs/database-partitioning-design.md. This is a structural
-- change (explicitly confirmed). job_events has no inbound foreign keys, and the
-- application inserts via record_job_event with unchanged SQL that routes
-- transparently into partitions, so no application change is required.
--
-- The table is swapped in place: the existing table is renamed aside (with its
-- constraint/index names freed), a partitioned parent reusing the same id
-- sequence is created, existing rows are backfilled, then the legacy table is
-- dropped. Runs inside the migration transaction.

-- Detach the sequence so dropping the legacy table does not drop it.
alter sequence job_events_id_seq owned by none;

-- Move the existing table and free its constraint/index names for reuse.
alter table job_events rename to job_events_legacy;
alter table job_events_legacy
    rename constraint job_events_pkey to job_events_legacy_pkey;
alter table job_events_legacy
    rename constraint job_events_event_level_check to job_events_legacy_event_level_check;
alter table job_events_legacy
    rename constraint job_events_event_type_check to job_events_legacy_event_type_check;
alter table job_events_legacy
    rename constraint job_events_job_id_fkey to job_events_legacy_job_id_fkey;
alter table job_events_legacy
    rename constraint job_events_job_run_id_fkey to job_events_legacy_job_run_id_fkey;
alter index idx_job_events_created_at rename to idx_job_events_legacy_created_at;
alter index idx_job_events_job_created rename to idx_job_events_legacy_job_created;
alter index idx_job_events_job_created_keyset rename to idx_job_events_legacy_job_created_keyset;
alter index idx_job_events_job_level_created_keyset
    rename to idx_job_events_legacy_job_level_created_keyset;

-- Partitioned parent. The primary key must include the partition key.
create table job_events (
    id bigint not null default nextval('job_events_id_seq'),
    job_id bigint references jobs(id) on delete cascade,
    job_run_id bigint references job_runs(id) on delete set null,
    event_type text not null,
    event_level text not null default 'info',
    message text,
    payload jsonb not null default '{}'::jsonb,
    created_at timestamptz not null default now(),
    primary key (id, created_at),
    check (length(trim(event_type)) > 0),
    check (event_level in ('debug', 'info', 'warn', 'error'))
) partition by range (created_at);

alter sequence job_events_id_seq owned by job_events.id;

-- Monthly partitions around the current window plus a default catch-all for
-- older/overflow rows. A rolling-maintenance task creates future months and
-- archives/detaches cold partitions (see docs/database-partitioning-design.md).
create table job_events_2026m06 partition of job_events
    for values from ('2026-06-01') to ('2026-07-01');
create table job_events_2026m07 partition of job_events
    for values from ('2026-07-01') to ('2026-08-01');
create table job_events_2026m08 partition of job_events
    for values from ('2026-08-01') to ('2026-09-01');
create table job_events_default partition of job_events default;

-- Recreate indexes under the original names (cascade to all partitions).
create index idx_job_events_created_at on job_events (created_at desc);
create index idx_job_events_job_created on job_events (job_id, created_at desc);
create index idx_job_events_job_created_keyset
    on job_events (job_id, created_at desc, id desc);
create index idx_job_events_job_level_created_keyset
    on job_events (job_id, event_level, created_at desc, id desc);

-- Backfill existing rows (routed into partitions), preserving ids.
insert into job_events (
    id, job_id, job_run_id, event_type, event_level, message, payload, created_at
)
select id, job_id, job_run_id, event_type, event_level, message, payload, created_at
from job_events_legacy;

-- Keep the sequence ahead of backfilled ids.
select setval(
    'job_events_id_seq',
    (select coalesce(max(id), 0) from job_events) + 1,
    false
);

drop table job_events_legacy;
