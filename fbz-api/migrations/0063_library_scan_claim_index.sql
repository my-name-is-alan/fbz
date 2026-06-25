-- Dedicated partial claim index for the library.scan worker queue.
--
-- metadata.refresh (0015) and media.probe (0023) each already have a
-- `(status, run_at, priority desc, id)` partial claim index filtered by their
-- job_type. library.scan only had a libraryId-keyed dedupe index
-- (idx_jobs_library_scan_library_active, 0021), so its claim query — which
-- filters `job_type = 'library.scan' and status in ('queued','failed') and
-- attempts < max_attempts and run_at <= now()` ordered by
-- `priority desc, run_at asc, id asc` — fell back to the cross-job-type
-- generic idx_jobs_status_run_at. This adds the matching dedicated partial
-- index so the scan claim (run on every worker poll) stays selective at scale.
create index if not exists idx_jobs_library_scan_claim
    on jobs (status, run_at, priority desc, id)
    where job_type = 'library.scan'
      and status in ('queued', 'failed');
