create index if not exists idx_user_playstates_continue_covering
    on user_playstates (user_id, updated_at desc, media_item_id desc)
    include (position_ticks, play_count, is_favorite, rating, played)
    where played = false and position_ticks > 0;
