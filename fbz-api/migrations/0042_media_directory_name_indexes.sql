create extension if not exists pg_trgm;

create index if not exists idx_genres_lower_name_pattern
    on genres (lower(name) text_pattern_ops, id);

create index if not exists idx_genres_lower_name_trgm
    on genres using gin (lower(name) gin_trgm_ops);

create index if not exists idx_people_lower_name_pattern
    on people (lower(name) text_pattern_ops, public_id);

create index if not exists idx_people_lower_name_trgm
    on people using gin (lower(name) gin_trgm_ops);

create index if not exists idx_media_items_artist_lower_title_pattern
    on media_items (lower(title) text_pattern_ops, public_id)
    where item_type = 'artist'
      and is_deleted = false;

create index if not exists idx_media_items_artist_lower_title_trgm
    on media_items using gin (lower(title) gin_trgm_ops)
    where item_type = 'artist'
      and is_deleted = false;

create index if not exists idx_media_items_visible_title_lower_trgm
    on media_items using gin (lower(title) gin_trgm_ops)
    where is_deleted = false;

create index if not exists idx_media_items_visible_original_title_lower_trgm
    on media_items using gin (lower(original_title) gin_trgm_ops)
    where original_title is not null
      and is_deleted = false;

create index if not exists idx_media_items_visible_sort_title_lower_trgm
    on media_items using gin (lower(sort_title) gin_trgm_ops)
    where sort_title is not null
      and is_deleted = false;

create index if not exists idx_media_items_visible_sort_key_lower_pattern
    on media_items (
        library_id,
        lower(coalesce(nullif(sort_title, ''), title)) text_pattern_ops,
        id
    )
    where is_deleted = false;

create index if not exists idx_media_item_people_item_role_person
    on media_item_people (media_item_id, role_type, person_id);
