create index if not exists idx_media_items_parent_type_index_sort
    on media_items (parent_id, item_type, index_number, sort_title, id)
    include (public_id, title, runtime_ticks, production_year, parent_index_number)
    where is_deleted = false;
