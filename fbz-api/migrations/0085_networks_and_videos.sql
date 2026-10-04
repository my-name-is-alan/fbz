-- 0085_networks_and_videos.sql（additive，新增表）
-- 目标扩展：(1) 播出平台 networks（TMDB tv networks：Netflix / 爱奇艺 / Disney+ 等），
-- (2) 主题曲 / 宣传片 media_videos（TMDB videos：Trailer / Teaser / Opening Theme 等）。
-- networks 仿 studios 的「实体表 + 关联表」形态；videos 直接挂 media_item（含类型与外链）。

-- 播出平台实体（去重共享，名称归一）。
create table if not exists networks (
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

-- media_item ↔ network 关联（剧集/电影的播出/发行平台）。
create table if not exists media_item_networks (
    media_item_id bigint not null references media_items(id) on delete cascade,
    network_id bigint not null references networks(id) on delete cascade,
    primary key (media_item_id, network_id)
);

create index if not exists idx_media_item_networks_network
    on media_item_networks (network_id, media_item_id);

-- 主题曲 / 宣传片 / 预告等附属视频（外链，按 media_item 归属）。
-- video_type: trailer | teaser | clip | featurette | behind_the_scenes | opening_theme |
--             ending_theme | theme（宽松 allowlist，未知归 clip）。
-- site: youtube | vimeo | bilibili | ... ；key 是站点内视频 id；url 是可直接打开的完整链接之一。
create table if not exists media_videos (
    id bigserial primary key,
    public_id uuid not null default gen_random_uuid(),
    media_item_id bigint not null references media_items(id) on delete cascade,
    video_type text not null,
    name text,
    site text,
    site_key text,
    url text,
    is_official boolean not null default false,
    sort_order integer not null default 0,
    created_at timestamptz not null default now(),
    unique (public_id),
    unique (media_item_id, site, site_key),
    check (length(trim(video_type)) > 0),
    check (video_type in (
        'trailer', 'teaser', 'clip', 'featurette', 'behind_the_scenes',
        'opening_theme', 'ending_theme', 'theme'
    ))
);

create index if not exists idx_media_videos_item_type
    on media_videos (media_item_id, video_type, sort_order, id);
