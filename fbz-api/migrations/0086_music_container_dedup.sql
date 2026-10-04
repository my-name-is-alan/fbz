-- 0086_music_container_dedup.sql（additive，新增部分唯一索引）
-- 音乐层级归组：扫描识别出 track 后，要找/建 artist→album 容器并填 parent_id 链
-- （track.parent_id → album，album.parent_id → artist）。这两个部分唯一索引支撑并发安全
-- 的 find-or-create（insert ... on conflict）：同一库内同名 artist 只一个容器、同一 artist
-- 下同名 album 只一个容器，避免并发扫描产生重复。
--
-- 容器去重键（与 0083 剧集容器同构）：
--   artist → (library_id, 规范化 title)，仅 item_type='artist' 且未删除。
--   album  → (parent_id, 规范化 title)，仅 item_type='album' 且未删除（同 artist 下按专辑名去重）。
-- 规范化用 lower(btrim(title)) 消除大小写/首尾空白差异，与 audio_tags 读出的标签一致。

create unique index if not exists uq_media_items_artist_container
    on media_items (library_id, lower(btrim(title)))
    where item_type = 'artist' and is_deleted = false;

create unique index if not exists uq_media_items_album_container
    on media_items (parent_id, lower(btrim(title)))
    where item_type = 'album' and is_deleted = false and parent_id is not null;
