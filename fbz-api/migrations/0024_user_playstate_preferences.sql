alter table user_playstates
    add column if not exists is_favorite boolean not null default false,
    add column if not exists rating numeric(4, 2);

do $$
begin
    if not exists (
        select 1
        from pg_constraint
        where conname = 'user_playstates_rating_range'
    ) then
        alter table user_playstates
            add constraint user_playstates_rating_range
            check (rating is null or (rating >= 0 and rating <= 10));
    end if;
end $$;

create index if not exists idx_user_playstates_favorites
    on user_playstates (user_id, updated_at desc)
    where is_favorite = true;
