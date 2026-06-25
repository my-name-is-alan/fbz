-- Partition the high-growth Host API call audit log by finished_at (monthly RANGE).
--
-- Category B from docs/database-partitioning-design.md: a leaf table (no inbound
-- foreign keys) whose `public_id` UNIQUE constraint cannot survive partitioning
-- (a partitioned unique constraint must include the partition key). public_id is
-- a random uuid and is never used for upsert (the audit insert is a plain INSERT
-- with no ON CONFLICT), so it is relaxed to a non-unique index — practical global
-- uniqueness still holds via uuid randomness. Structural change, confirmed.
--
-- The existing table + its constraint/index names are moved aside so the new
-- partitioned parent can reuse the original names; the legacy table is dropped
-- after backfill. Runs inside the migration transaction.

-- Preserve the id sequence across the swap.
alter sequence plugin_host_api_calls_id_seq owned by none;

-- Free the names we reuse (PK, checks, FKs, indexes). The public_id UNIQUE
-- constraint name is intentionally not reused (relaxed to an index below).
alter table plugin_host_api_calls rename to plugin_host_api_calls_legacy;
alter table plugin_host_api_calls_legacy rename constraint plugin_host_api_calls_pkey to phac_legacy_pkey;
alter table plugin_host_api_calls_legacy rename constraint plugin_host_api_calls_check to phac_legacy_check;
alter table plugin_host_api_calls_legacy rename constraint plugin_host_api_calls_duration_ms_check to phac_legacy_duration_ms_check;
alter table plugin_host_api_calls_legacy rename constraint plugin_host_api_calls_error_code_check to phac_legacy_error_code_check;
alter table plugin_host_api_calls_legacy rename constraint plugin_host_api_calls_method_check to phac_legacy_method_check;
alter table plugin_host_api_calls_legacy rename constraint plugin_host_api_calls_package_id_check to phac_legacy_package_id_check;
alter table plugin_host_api_calls_legacy rename constraint plugin_host_api_calls_path_check to phac_legacy_path_check;
alter table plugin_host_api_calls_legacy rename constraint plugin_host_api_calls_plugin_id_check to phac_legacy_plugin_id_check;
alter table plugin_host_api_calls_legacy rename constraint plugin_host_api_calls_required_permission_check to phac_legacy_required_permission_check;
alter table plugin_host_api_calls_legacy rename constraint plugin_host_api_calls_status_code_check to phac_legacy_status_code_check;
alter table plugin_host_api_calls_legacy rename constraint plugin_host_api_calls_execution_run_id_fkey to phac_legacy_execution_run_id_fkey;
alter table plugin_host_api_calls_legacy rename constraint plugin_host_api_calls_host_token_id_fkey to phac_legacy_host_token_id_fkey;
alter table plugin_host_api_calls_legacy rename constraint plugin_host_api_calls_plugin_id_fkey to phac_legacy_plugin_id_fkey;
alter index plugin_host_api_calls_public_id_key rename to phac_legacy_public_id_key;
alter index idx_plugin_host_api_calls_plugin_finished rename to idx_phac_legacy_plugin_finished;
alter index idx_plugin_host_api_calls_execution_finished rename to idx_phac_legacy_execution_finished;
alter index idx_plugin_host_api_calls_status_finished rename to idx_phac_legacy_status_finished;
alter index idx_plugin_host_api_calls_recent_keyset rename to idx_phac_legacy_recent_keyset;
alter index idx_plugin_host_api_calls_plugin_finished_keyset rename to idx_phac_legacy_plugin_finished_keyset;
alter index idx_plugin_host_api_calls_status_finished_keyset rename to idx_phac_legacy_status_finished_keyset;
alter index idx_plugin_host_api_calls_plugin_status_finished_keyset rename to idx_phac_legacy_plugin_status_finished_keyset;
alter index idx_plugin_host_api_calls_execution_finished_keyset rename to idx_phac_legacy_execution_finished_keyset;
alter index idx_plugin_host_api_calls_execution_status_finished_keyset rename to idx_phac_legacy_execution_status_finished_keyset;
alter index idx_plugin_host_api_calls_execution_plugin_budget rename to idx_phac_legacy_execution_plugin_budget;

-- Partitioned parent reusing the original sequence; PK includes the partition key.
create table plugin_host_api_calls (
    id bigint not null default nextval('plugin_host_api_calls_id_seq'),
    public_id uuid not null default gen_random_uuid(),
    plugin_id text not null references plugin_installations(plugin_id) on delete cascade,
    package_id text not null,
    host_token_id bigint references plugin_host_tokens(id) on delete set null,
    execution_run_id bigint references plugin_execution_runs(id) on delete set null,
    method text not null,
    path text not null,
    required_permission text,
    status_code integer not null,
    error_code text,
    error_message text,
    started_at timestamptz not null,
    finished_at timestamptz not null default now(),
    duration_ms integer not null,
    primary key (id, finished_at),
    check (finished_at >= started_at),
    check (duration_ms >= 0),
    check (error_code is null or length(trim(error_code)) > 0),
    check (method in ('GET', 'POST', 'PUT', 'PATCH', 'DELETE')),
    check (length(trim(package_id)) > 0),
    check (length(trim(path)) > 0),
    check (length(trim(plugin_id)) > 0),
    check (required_permission is null or length(trim(required_permission)) > 0),
    check (status_code >= 100 and status_code <= 599)
) partition by range (finished_at);

alter sequence plugin_host_api_calls_id_seq owned by plugin_host_api_calls.id;

-- Monthly partitions plus a default catch-all; rolling maintenance adds future months.
create table plugin_host_api_calls_2026m06 partition of plugin_host_api_calls
    for values from ('2026-06-01') to ('2026-07-01');
create table plugin_host_api_calls_2026m07 partition of plugin_host_api_calls
    for values from ('2026-07-01') to ('2026-08-01');
create table plugin_host_api_calls_2026m08 partition of plugin_host_api_calls
    for values from ('2026-08-01') to ('2026-09-01');
create table plugin_host_api_calls_default partition of plugin_host_api_calls default;

-- public_id: relaxed from UNIQUE to a plain index (see header).
create index idx_plugin_host_api_calls_public_id on plugin_host_api_calls (public_id);

-- Recreate the audit indexes under their original names (cascade to partitions).
create index idx_plugin_host_api_calls_plugin_finished
    on plugin_host_api_calls (plugin_id, finished_at desc);
create index idx_plugin_host_api_calls_execution_finished
    on plugin_host_api_calls (execution_run_id, finished_at desc);
create index idx_plugin_host_api_calls_status_finished
    on plugin_host_api_calls (status_code, finished_at desc);
create index idx_plugin_host_api_calls_recent_keyset
    on plugin_host_api_calls (finished_at desc, id desc);
create index idx_plugin_host_api_calls_plugin_finished_keyset
    on plugin_host_api_calls (plugin_id, finished_at desc, id desc);
create index idx_plugin_host_api_calls_status_finished_keyset
    on plugin_host_api_calls (status_code, finished_at desc, id desc);
create index idx_plugin_host_api_calls_plugin_status_finished_keyset
    on plugin_host_api_calls (plugin_id, status_code, finished_at desc, id desc);
create index idx_plugin_host_api_calls_execution_finished_keyset
    on plugin_host_api_calls (execution_run_id, finished_at desc, id desc)
    where execution_run_id is not null;
create index idx_plugin_host_api_calls_execution_status_finished_keyset
    on plugin_host_api_calls (execution_run_id, status_code, finished_at desc, id desc)
    where execution_run_id is not null;
create index idx_plugin_host_api_calls_execution_plugin_budget
    on plugin_host_api_calls (execution_run_id, plugin_id, id)
    where execution_run_id is not null;

-- Backfill existing rows (routed into partitions), preserving ids and public_ids.
insert into plugin_host_api_calls (
    id, public_id, plugin_id, package_id, host_token_id, execution_run_id, method, path,
    required_permission, status_code, error_code, error_message, started_at, finished_at, duration_ms
)
select id, public_id, plugin_id, package_id, host_token_id, execution_run_id, method, path,
    required_permission, status_code, error_code, error_message, started_at, finished_at, duration_ms
from plugin_host_api_calls_legacy;

-- Keep the sequence ahead of backfilled ids.
select setval(
    'plugin_host_api_calls_id_seq',
    (select coalesce(max(id), 0) from plugin_host_api_calls) + 1,
    false
);

drop table plugin_host_api_calls_legacy;
