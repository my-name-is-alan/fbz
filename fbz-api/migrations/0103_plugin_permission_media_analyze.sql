-- Adds `media.analyze` and storage.* keys to plugin_permissions.permission_key allow-list
-- (architecture / docs/plans/media-marker-detect-draft.md + storage provider extension).
-- Additive: drop prior allow-list constraint and re-add with the full supported set.
-- Must stay aligned with SUPPORTED_PERMISSIONS in src/plugins/manifest.rs.

do $$
declare
    constraint_name text;
begin
    for constraint_name in
        select con.conname
        from pg_constraint con
        where con.conrelid = 'plugin_permissions'::regclass
          and con.contype = 'c'
          and pg_get_constraintdef(con.oid) like '%permission_key%'
          and pg_get_constraintdef(con.oid) like '%admin.menu%'
    loop
        execute format('alter table plugin_permissions drop constraint %I', constraint_name);
    end loop;
end
$$;

alter table plugin_permissions
    drop constraint if exists plugin_permissions_permission_key_allowed;

alter table plugin_permissions
    add constraint plugin_permissions_permission_key_allowed
    check (permission_key in (
        'admin.menu',
        'library.read',
        'library.write',
        'media.read',
        'media.analyze',
        'metadata.read',
        'metadata.write',
        'notification.send',
        'playback.read',
        'scheduler.register',
        'storage.account',
        'storage.read',
        'storage.resolve',
        'webhook.emit'
    ));
