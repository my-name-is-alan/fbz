create index if not exists idx_jobs_library_scan_library_active
    on jobs ((payload->>'libraryId'), status, run_at, id)
    where job_type = 'library.scan'
      and status in ('queued', 'running', 'failed');
