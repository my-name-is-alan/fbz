alter table media_files
    add column if not exists last_seen_scan_id text,
    add column if not exists last_seen_at timestamptz;

do $$
begin
    if not exists (
        select 1
        from pg_constraint
        where conname = 'media_files_last_seen_scan_id_non_empty'
          and conrelid = 'media_files'::regclass
    ) then
        alter table media_files
            add constraint media_files_last_seen_scan_id_non_empty
            check (last_seen_scan_id is null or length(trim(last_seen_scan_id)) > 0);
    end if;
end
$$;

create index if not exists idx_media_files_item_last_seen_scan
    on media_files (media_item_id, last_seen_scan_id);

create index if not exists idx_media_files_last_seen_at
    on media_files (last_seen_at)
    where last_seen_at is not null;
