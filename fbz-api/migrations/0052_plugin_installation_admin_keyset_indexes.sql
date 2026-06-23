create index if not exists idx_plugin_installations_recent_keyset
    on plugin_installations (updated_at desc, id desc);

create index if not exists idx_plugin_installations_approval_recent_keyset
    on plugin_installations (approval_status, updated_at desc, id desc);

create index if not exists idx_plugin_installations_enabled_recent_keyset
    on plugin_installations (enabled, updated_at desc, id desc);
