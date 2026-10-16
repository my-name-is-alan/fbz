-- 0110_user_access_policy_image_locks.sql（additive）
-- 1) 用户访问策略原样存储（JF UserPolicy 的 EnableAllFolders / EnabledFolders / BlockedMediaFolders / BlockedTags），
--    GET Policy 原样回显；未保存过策略的用户无行，回显沿用 library_permissions 推导（旧行为）。
-- 2) 新建媒体库时自动授予 EnableAllFolders=true 的用户（JF：全部文件夹包括以后新建的）。
-- 3) 条目级可见性函数 fbz_item_visible_to_user：MaxParentalRating（条目或最近祖先的分级）+ BlockedTags（条目或祖先带标签）。
-- 4) 图片锁：条目 LockData 或 LockedFields 含 'Images' 时，非编辑器（未设 fbz.metadata_edit）写 artwork 被静默跳过。

create table if not exists user_access_policies (
    user_id            bigint primary key references users(id) on delete cascade,
    enable_all_folders boolean not null default true,
    enabled_folders    text[]  not null default '{}',
    blocked_folders    text[]  not null default '{}',
    blocked_tags       text[]  not null default '{}',
    updated_at         timestamptz not null default now()
);

comment on table user_access_policies is
    'Raw Emby/JF UserPolicy folder/tag access as saved by POST /Users/{id}/Policy; library_permissions.can_view is the derived enforcement';

create or replace function fbz_grant_new_library_to_all_folder_users() returns trigger
language plpgsql as $$
begin
    insert into library_permissions (library_id, user_id, can_view, can_download, can_transcode)
    select new.id, u.id, true, u.allow_download, true
    from user_access_policies p
    join users u on u.id = p.user_id
    where p.enable_all_folders = true
      and not (replace(new.public_id::text, '-', '') = any (
            select replace(lower(btrim(b)), '-', '') from unnest(p.blocked_folders) b))
    on conflict (library_id, user_id) do update set can_view = true, updated_at = now();
    return new;
end;
$$;

drop trigger if exists trg_libraries_grant_all_folder_users on libraries;
create trigger trg_libraries_grant_all_folder_users
    after insert on libraries
    for each row execute function fbz_grant_new_library_to_all_folder_users();

-- 与 src/library/parental.rs::official_rating_level 同表。
create or replace function fbz_rating_level(rating text) returns integer
language sql immutable as $$
    select case lower(btrim(coalesce(rating, '')))
        when 'g' then 1 when 'tv-y' then 1 when 'tv-g' then 1
        when 'tv-y7' then 3 when 'tv-y7-fv' then 3
        when 'pg' then 5 when 'tv-pg' then 5
        when 'pg-13' then 8 when 'tv-14' then 8
        when 'r' then 9 when 'tv-ma' then 9
        when 'nc-17' then 10
        else null end
$$;

create or replace function fbz_item_visible_to_user(p_user_id bigint, p_item_id bigint) returns boolean
language plpgsql stable as $$
declare
    v_max integer;
    v_tags text[];
    v_level integer;
begin
    select u.max_parental_rating, coalesce(p.blocked_tags, '{}')
      into v_max, v_tags
      from users u left join user_access_policies p on p.user_id = u.id
     where u.id = p_user_id;
    if v_max is null and coalesce(cardinality(v_tags), 0) = 0 then
        return true;
    end if;
    if v_max is not null then
        -- 条目自身分级优先；未分级时继承最近祖先（剧集/季 → 剧）。仍未分级 → 放行（BlockUnratedItems 另管）。
        with recursive chain(id, parent_id, official_rating, depth) as (
            select id, parent_id, official_rating, 0 from media_items where id = p_item_id
            union all
            select m.id, m.parent_id, m.official_rating, c.depth + 1
              from media_items m join chain c on m.id = c.parent_id
             where c.depth < 4
        )
        select fbz_rating_level(official_rating) into v_level
          from chain where fbz_rating_level(official_rating) is not null
         order by depth limit 1;
        if v_level is not null and v_level > v_max then
            return false;
        end if;
    end if;
    if coalesce(cardinality(v_tags), 0) > 0 then
        if exists (
            with recursive chain(id, parent_id, depth) as (
                select id, parent_id, 0 from media_items where id = p_item_id
                union all
                select m.id, m.parent_id, c.depth + 1
                  from media_items m join chain c on m.id = c.parent_id
                 where c.depth < 4
            )
            select 1 from chain c
              join media_item_tags mit on mit.media_item_id = c.id
              join tags t on t.id = mit.tag_id
             where lower(btrim(t.name)) = any (select lower(btrim(x)) from unnest(v_tags) x)
        ) then
            return false;
        end if;
    end if;
    return true;
end;
$$;

create or replace function fbz_artwork_image_lock() returns trigger
language plpgsql as $$
declare
    v_item bigint := coalesce(new.media_item_id, old.media_item_id);
begin
    if v_item is null or coalesce(current_setting('fbz.metadata_edit', true), '') = 'on' then
        return case when tg_op = 'DELETE' then old else new end;
    end if;
    if exists (
        select 1 from media_items
         where id = v_item and (lock_data or 'Images' = any (locked_fields))
    ) then
        return null;
    end if;
    return case when tg_op = 'DELETE' then old else new end;
end;
$$;

drop trigger if exists trg_artwork_image_lock on artwork;
create trigger trg_artwork_image_lock
    before insert or update or delete on artwork
    for each row execute function fbz_artwork_image_lock();
