create index if not exists idx_media_items_album_lower_title_public
    on media_items (lower(title), public_id)
    where item_type = 'album'
      and is_deleted = false;
