-- Bounded stale job recovery probes one job_type at a time, ordered by the
-- oldest expired running lease. Keep that candidate batch selective even when
-- the jobs table contains a large queued/finished history.
create index if not exists idx_jobs_stale_recovery
    on jobs (job_type, locked_until asc, id asc)
    where status = 'running'
      and locked_until is not null;
