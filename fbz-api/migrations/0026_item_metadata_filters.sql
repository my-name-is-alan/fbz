alter table media_items
    add column if not exists official_rating text;

create table if not exists studios (
    id bigserial primary key,
    public_id uuid not null default gen_random_uuid(),
    name text not null,
    name_normalized text not null,
    created_at timestamptz not null default now(),
    updated_at timestamptz not null default now(),
    unique (public_id),
    unique (name_normalized),
    check (length(trim(name)) > 0),
    check (length(trim(name_normalized)) > 0)
);

create table if not exists media_item_studios (
    media_item_id bigint not null references media_items(id) on delete cascade,
    studio_id bigint not null references studios(id) on delete cascade,
    primary key (media_item_id, studio_id)
);

create index if not exists idx_media_items_official_rating_lower
    on media_items ((lower(official_rating)))
    where official_rating is not null;

create index if not exists idx_media_item_studios_studio
    on media_item_studios (studio_id, media_item_id);
