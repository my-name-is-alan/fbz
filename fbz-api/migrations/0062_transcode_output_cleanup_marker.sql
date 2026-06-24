alter table transcoding_sessions
    add column if not exists output_cleaned_at timestamptz;

create index if not exists idx_transcoding_sessions_output_cleanup_pending
    on transcoding_sessions (finished_at asc nulls first, id asc)
    where status in ('cancelled', 'failed')
      and output_cleaned_at is null
      and output_path is not null;
