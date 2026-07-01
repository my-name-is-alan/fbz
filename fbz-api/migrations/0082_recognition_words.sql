-- 0082_recognition_words.sql（additive，新增表）
-- 自定义识别词规则（design §4.2 / §7）：管理员录入的屏蔽/替换/集偏移规则，
-- 启动/变更时编译为内存 RuleSet 供识别管线阶段 A/D 应用。
-- 0080/0081 已被 media-type-taxonomy / photo 占用，故识别词表顺延至 0082。
create table if not exists recognition_words (
    id bigserial primary key,
    public_id uuid not null default gen_random_uuid(),
    kind text not null,              -- 'block' | 'replace' | 'offset' | 'replace_offset'
    pattern text not null,           -- 左件（屏蔽词 / 被替换词 / 前定位词）
    replacement text,                -- 右件（替换为；offset 类可空）
    anchor_after text,               -- offset 类：后定位词
    offset_expr text,                -- offset 类：集数偏移表达式，如 '-26' 或 'EP*2-1'
    is_regex boolean not null default false,
    enabled boolean not null default true,
    library_id bigint references libraries(id) on delete cascade,  -- null = 全局
    priority integer not null default 100,   -- 应用顺序，小者先
    note text,
    created_at timestamptz not null default now(),
    updated_at timestamptz not null default now(),
    unique (public_id),
    check (kind in ('block', 'replace', 'offset', 'replace_offset')),
    -- block/replace 的 pattern 是必填屏蔽/被替换词；offset/replace_offset 的 pattern 是
    -- 前定位词，design §7.2 允许为空（无窗口约束，整串生效），故只对前两类要求非空。
    check (
        kind in ('offset', 'replace_offset')
        or length(trim(pattern)) > 0
    )
);

-- 编译 RuleSet 时按 (全局 + 该库) + enabled + priority 取规则。
create index if not exists idx_recognition_words_scope
    on recognition_words (coalesce(library_id, 0), enabled, priority, id);
