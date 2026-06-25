-- Rolling partition-maintenance helper + initial forward coverage.
--
-- The partitioned tables (0064-0067) only have explicit partitions around the
-- migration window plus a `default` catch-all; without ongoing maintenance, new
-- rows would pile into `default`. ensure_partition_coverage(months_ahead)
-- idempotently creates the current month + N upcoming monthly partitions for
-- every time-partitioned table, skipping any that already exist, and returns the
-- count created. It is the mechanism the rolling-maintenance scheduled task will
-- call periodically (see docs/database-partitioning-design.md); wiring that task
-- (which runs DDL from a worker) is a separate step.

create or replace function ensure_partition_coverage(months_ahead int)
returns int
language plpgsql
as $$
declare
    tbl text;
    i int;
    start_bound date;
    end_bound date;
    part_name text;
    created int := 0;
begin
    foreach tbl in array
        array['job_events', 'plugin_host_api_calls', 'scheduled_task_runs', 'job_runs']
    loop
        for i in 0..greatest(months_ahead, 0) loop
            start_bound := (date_trunc('month', now()) + make_interval(months => i))::date;
            end_bound := (start_bound + interval '1 month')::date;
            part_name := tbl || '_' || to_char(start_bound, 'YYYY"m"MM');
            if not exists (select 1 from pg_class where relname = part_name) then
                execute format(
                    'create table %I partition of %I for values from (%L) to (%L)',
                    part_name, tbl, start_bound, end_bound
                );
                created := created + 1;
            end if;
        end loop;
    end loop;
    return created;
end
$$;

-- Create forward coverage now (current month + 18 ahead). Idempotent: existing
-- partitions are skipped.
select ensure_partition_coverage(18);
