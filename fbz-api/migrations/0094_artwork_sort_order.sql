-- 0094_artwork_sort_order.sql（additive）：artwork 显式排序列，支撑 Emby 图片
-- 重排（POST /Items/{Id}/Images/{Type}/{Index}/Index）。既有行 sort_order=0，
-- 排序键从 (is_primary desc, id) 变为 (is_primary desc, sort_order, id)，
-- 旧数据顺序不变。

alter table artwork add column if not exists sort_order integer not null default 0;

create index if not exists idx_artwork_item_type_order
    on artwork (media_item_id, artwork_type, is_primary desc, sort_order, id);
