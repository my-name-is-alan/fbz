create index if not exists idx_user_playstates_favorites_covering
    on user_playstates (user_id, updated_at desc, media_item_id desc)
    include (position_ticks, play_count, rating, played)
    where is_favorite = true;

create index if not exists idx_user_playstates_played_covering
    on user_playstates (user_id, updated_at desc, media_item_id desc)
    include (position_ticks, play_count, is_favorite, rating)
    where played = true;

create index if not exists idx_user_playstates_rating_covering
    on user_playstates (user_id, rating, media_item_id)
    include (position_ticks, play_count, is_favorite, played, updated_at)
    where rating is not null;
