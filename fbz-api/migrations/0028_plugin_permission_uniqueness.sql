delete from plugin_permissions duplicate
using plugin_permissions original
where duplicate.package_id = original.package_id
  and duplicate.permission_key = original.permission_key
  and coalesce(duplicate.permission_scope, '') = coalesce(original.permission_scope, '')
  and duplicate.id > original.id;

create unique index if not exists idx_plugin_permissions_package_key_scope_normalized
    on plugin_permissions (package_id, permission_key, coalesce(permission_scope, ''));
