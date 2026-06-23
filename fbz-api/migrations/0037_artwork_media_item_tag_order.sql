create index if not exists idx_artwork_media_item_type_primary_id
    on artwork (media_item_id, artwork_type, is_primary desc, id)
    where media_item_id is not null;
