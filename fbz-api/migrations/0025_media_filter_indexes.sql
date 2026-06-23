create index if not exists idx_media_files_item_container_lower
    on media_files (media_item_id, (lower(container)))
    where container is not null;

create index if not exists idx_media_streams_type_codec_file
    on media_streams (stream_type, (lower(codec)), media_file_id)
    where codec is not null;
