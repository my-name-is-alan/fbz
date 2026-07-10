-- 0091_playlists.sql（additive）：真正的用户播放列表写模型。
-- 之前 /Playlists 读侧临时复用 collections（BoxSet）；本迁移引入用户自有、可写、
-- 可排序的播放列表。collections 回归只承载合集（BoxSet），两者互不影响。
-- playlist_entries.public_id 即 Emby 客户端的 PlaylistItemId / EntryId（删除与移动用）。

create table if not exists playlists (
    id bigserial primary key,
    public_id uuid not null default gen_random_uuid(),
    owner_user_id bigint not null references users(id) on delete cascade,
    name text not null,
    media_type text,
    created_at timestamptz not null default now(),
    updated_at timestamptz not null default now(),
    unique (public_id),
    check (length(trim(name)) > 0),
    check (media_type is null or media_type in ('audio', 'video'))
);

-- 属主列表页：owner + 名称排序 keyset。
create index if not exists idx_playlists_owner_name
    on playlists (owner_user_id, lower(name), id);

create table if not exists playlist_entries (
    id bigserial primary key,
    public_id uuid not null default gen_random_uuid(),
    playlist_id bigint not null references playlists(id) on delete cascade,
    media_item_id bigint not null references media_items(id) on delete cascade,
    sort_order integer not null default 0,
    created_at timestamptz not null default now(),
    unique (public_id),
    check (sort_order >= 0)
);

-- 播放列表成员按序分页读取。
create index if not exists idx_playlist_entries_playlist_order
    on playlist_entries (playlist_id, sort_order, id);
-- 媒体条目删除/反查成员。
create index if not exists idx_playlist_entries_media_item
    on playlist_entries (media_item_id);
