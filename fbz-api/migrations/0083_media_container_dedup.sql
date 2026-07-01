-- 0083_media_container_dedup.sql（additive，新增部分唯一索引）
-- 剧集层级归组（design §8）：扫描识别出 episode 后，要找/建 series→season 容器并填
-- parent_id 链。这两个部分唯一索引支撑并发安全的 find-or-create（insert ... on conflict）：
-- 同一库内同名 series 只一个容器、同一 series 下同季号 season 只一个容器，避免并发扫描产生重复。
--
-- 容器去重键：
--   series  → (library_id, 规范化 title)，仅 item_type='series' 且未删除。
--   season  → (parent_id, season_number)，仅 item_type='season' 且未删除。
-- 规范化用 lower(btrim(title)) 消除大小写/首尾空白差异。

create unique index if not exists uq_media_items_series_container
    on media_items (library_id, lower(btrim(title)))
    where item_type = 'series' and is_deleted = false;

create unique index if not exists uq_media_items_season_container
    on media_items (parent_id, season_number)
    where item_type = 'season' and is_deleted = false and parent_id is not null;
