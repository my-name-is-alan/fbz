-- 0100_library_metadata_sources.sql（additive）
-- 库级 Match 刮削源链配置面（架构真源 §6.4 / §5.9.3 / §6.6 / §10 P0-D2）。
--
-- 本轮仅配置面：NFO 槽开关 + 优先级，以及 Provider（内置 id / plugin:{id}）引用与
-- 开关/优先级。本地 NFO 发现/解析进 Match 标下一档，不在本迁移落地。
-- 属 library 配置，不进插件私有 KV（§9.1 #10）。

create table if not exists library_metadata_sources (
    id bigserial primary key,
    public_id uuid not null default gen_random_uuid(),
    library_id bigint not null references libraries(id) on delete cascade,
    -- 'nfo' = core 本地/缓存 NFO 槽（与 Provider 同级可配）；'provider' = 元数据 Provider 引用。
    source_kind text not null,
    -- nfo 槽固定为 'nfo'；provider 为 metadata_provider_settings.provider_id 同形
    -- （tmdb / tvdb / fanart / imdb / spotify / plugin:…）。nfo 不得出现在 provider 行。
    source_key text not null,
    enabled boolean not null default true,
    -- 链内优先级：数值越小越先参与 Match；同库内允许相同 priority（次序再靠 id 稳定）。
    priority integer not null default 100,
    created_at timestamptz not null default now(),
    updated_at timestamptz not null default now(),
    unique (public_id),
    unique (library_id, source_kind, source_key),
    check (source_kind in ('nfo', 'provider')),
    check (
        (source_kind = 'nfo' and source_key = 'nfo')
        or (
            source_kind = 'provider'
            and length(trim(source_key)) between 1 and 64
            and source_key ~ '^[a-z0-9][a-z0-9:_-]*$'
            and source_key <> 'nfo'
        )
    )
);

-- Match / admin 列表按库 + priority 升序取链。
create index if not exists idx_library_metadata_sources_library_priority
    on library_metadata_sources (library_id, priority asc, id asc);
