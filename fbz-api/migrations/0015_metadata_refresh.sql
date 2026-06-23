create index if not exists idx_media_items_metadata_pending
    on media_items (metadata_status, updated_at, id)
    where is_deleted = false
      and metadata_status = 'pending';

create index if not exists idx_jobs_metadata_refresh_claim
    on jobs (status, run_at, priority desc, id)
    where job_type = 'metadata.refresh'
      and status in ('queued', 'failed');

create unique index if not exists idx_jobs_metadata_refresh_active_item
    on jobs ((payload->>'itemId'))
    where job_type = 'metadata.refresh'
      and status in ('queued', 'running', 'failed')
      and attempts < max_attempts;
