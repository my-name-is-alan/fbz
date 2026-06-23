create index if not exists idx_media_items_library_sort_visible
    on media_items (library_id, (coalesce(nullif(sort_title, ''), title)), id)
    where is_deleted = false;
