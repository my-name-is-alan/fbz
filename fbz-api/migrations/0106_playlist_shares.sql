-- Jellyfin 10.9 播放列表共享：OpenAccess（全体可见）+ 按用户共享（CanEdit）。
alter table playlists
    add column if not exists open_access boolean not null default false;

create table if not exists playlist_shares (
    playlist_id bigint not null references playlists(id) on delete cascade,
    user_id bigint not null references users(id) on delete cascade,
    can_edit boolean not null default false,
    created_at timestamptz not null default now(),
    primary key (playlist_id, user_id)
);

create index if not exists idx_playlist_shares_user on playlist_shares (user_id);
