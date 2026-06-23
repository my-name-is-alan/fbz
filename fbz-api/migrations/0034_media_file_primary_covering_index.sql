create index if not exists idx_media_files_item_primary_covering
    on media_files (media_item_id, is_primary desc, id)
    include (file_size, container, duration_ticks, bitrate, is_strm);
