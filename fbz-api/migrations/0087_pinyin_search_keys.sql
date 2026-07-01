-- 0087_pinyin_search_keys.sql（additive，新增列 + 索引）
-- 目标：含中文的名称（演员 people / 系列合集 collections / 影视音乐条目 media_items）入库时
-- 拆出拼音检索键，支撑「全拼」与「首字母」模糊查询。键由应用层 text::pinyin 生成（小写、
-- 无分隔），中文逐字转拼音，ASCII 字母数字原样并入；纯非中文名不写键（留空）。
-- 全部 nullable，回填失败或无中文即留空，不影响既有查询。

-- 演员 / 人物（含音乐艺术家，同表）。
alter table people add column if not exists pinyin_full text;      -- liudehua
alter table people add column if not exists pinyin_initials text;  -- ldh

-- 系列 / 合集（电影系列名常为中文）。
alter table collections add column if not exists pinyin_full text;
alter table collections add column if not exists pinyin_initials text;

-- 影视 / 音乐 / 图片条目（标题）。
alter table media_items add column if not exists pinyin_full text;
alter table media_items add column if not exists pinyin_initials text;

-- 前缀匹配（like 'term%'）走 text_pattern_ops；子串匹配（like '%term%'）走 trigram。
-- 键已是小写，索引直接建在列上（无需 lower()）。

create index if not exists idx_people_pinyin_full_pattern
    on people (pinyin_full text_pattern_ops, public_id);
create index if not exists idx_people_pinyin_initials_pattern
    on people (pinyin_initials text_pattern_ops, public_id);
create index if not exists idx_people_pinyin_full_trgm
    on people using gin (pinyin_full gin_trgm_ops);

create index if not exists idx_collections_pinyin_full_pattern
    on collections (pinyin_full text_pattern_ops, public_id);
create index if not exists idx_collections_pinyin_initials_pattern
    on collections (pinyin_initials text_pattern_ops, public_id);

create index if not exists idx_media_items_pinyin_full_pattern
    on media_items (pinyin_full text_pattern_ops, public_id);
create index if not exists idx_media_items_pinyin_initials_pattern
    on media_items (pinyin_initials text_pattern_ops, public_id);
create index if not exists idx_media_items_pinyin_full_trgm
    on media_items using gin (pinyin_full gin_trgm_ops);
