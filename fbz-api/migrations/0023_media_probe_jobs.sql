create index if not exists idx_jobs_media_probe_claim
    on jobs (status, run_at, priority desc, id)
    where job_type = 'media.probe'
      and status in ('queued', 'failed');

create index if not exists idx_jobs_media_probe_active_file
    on jobs ((payload->>'mediaFileId'))
    where job_type = 'media.probe'
      and status in ('queued', 'running', 'failed');
