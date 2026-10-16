-- 0109（additive）
-- 1) JF LockData / LockedFields：media_items.lock_data / locked_fields，触发器在库层兜底，
--    任何写入方（刷新、扫描、导入、插件）都改不动被锁字段；元数据编辑器在事务内
--    `set_config('fbz.metadata_edit','on',true)` 绕过（编辑器本身就是改锁定数据的入口）。
-- 2) JF LibraryOptions.SaveLocalMetadata / MetadataSavers=Nfo：libraries.save_local_metadata。
-- 3) `lyrics.provider.query` 加入插件 hook allow-list（远程歌词 provider，如 LRCLIB）。

alter table media_items add column if not exists lock_data boolean not null default false;
alter table media_items add column if not exists locked_fields text[] not null default '{}';
comment on column media_items.lock_data is 'Jellyfin LockData：true 时刷新/扫描不改元数据列';
comment on column media_items.locked_fields is 'Jellyfin LockedFields（Name/Overview/OfficialRating/Runtime/Genres/Studios/Tags/Cast/ProductionLocations）';

create or replace function fbz_media_items_metadata_lock() returns trigger
language plpgsql as $$
begin
    if coalesce(current_setting('fbz.metadata_edit', true), '') = 'on' then
        return new;
    end if;
    if old.lock_data then
        new.title := old.title;
        new.original_title := old.original_title;
        new.sort_title := old.sort_title;
        new.overview := old.overview;
        new.production_year := old.production_year;
        new.premiere_date := old.premiere_date;
        new.community_rating := old.community_rating;
        new.critic_rating := old.critic_rating;
        new.official_rating := old.official_rating;
        new.pinyin_full := old.pinyin_full;
        new.pinyin_initials := old.pinyin_initials;
        new.provider_fingerprint := old.provider_fingerprint;
        new.metadata_status := old.metadata_status;
        new.lock_data := old.lock_data;
        new.locked_fields := old.locked_fields;
        return new;
    end if;
    if 'Name' = any(old.locked_fields) then
        new.title := old.title;
        new.original_title := old.original_title;
        new.sort_title := old.sort_title;
        new.pinyin_full := old.pinyin_full;
        new.pinyin_initials := old.pinyin_initials;
    end if;
    if 'Overview' = any(old.locked_fields) then
        new.overview := old.overview;
    end if;
    if 'OfficialRating' = any(old.locked_fields) then
        new.official_rating := old.official_rating;
    end if;
    if 'Runtime' = any(old.locked_fields) then
        new.runtime_ticks := old.runtime_ticks;
    end if;
    new.lock_data := old.lock_data;
    new.locked_fields := old.locked_fields;
    return new;
end;
$$;

drop trigger if exists trg_media_items_metadata_lock on media_items;
create trigger trg_media_items_metadata_lock
    before update on media_items
    for each row
    when (old.lock_data or cardinality(old.locked_fields) > 0 or new.lock_data is distinct from old.lock_data or new.locked_fields is distinct from old.locked_fields)
    execute function fbz_media_items_metadata_lock();

alter table libraries add column if not exists save_local_metadata boolean not null default false;
comment on column libraries.save_local_metadata is 'Jellyfin LibraryOptions.SaveLocalMetadata / MetadataSavers=Nfo：保存/刷新后写 NFO';

alter table plugin_hooks drop constraint if exists plugin_hooks_event_key_allowed;
alter table plugin_hooks
    add constraint plugin_hooks_event_key_allowed
    check (event_key in (
        'library.scan.started',
        'library.scan.completed',
        'library.scan.failed',
        'media.item.created',
        'media.item.updated',
        'media.download.started',
        'metadata.refresh.completed',
        'metadata.refresh.failed',
        'metadata.provider.query',
        'media.marker.detect',
        'subtitle.provider.query',
        'lyrics.provider.query',
        'playback.started',
        'playback.stopped',
        'scheduler.tick',
        'transcode.started',
        'transcode.completed',
        'transcode.failed',
        'user.login',
        'webhook.received'
    ));
