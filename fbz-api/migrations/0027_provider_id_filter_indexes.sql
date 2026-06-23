create index if not exists idx_media_external_ids_provider_external_lower_item
    on media_external_ids ((lower(provider || '.' || external_id)), media_item_id);
