create index if not exists idx_plugin_packages_recent_keyset
    on plugin_packages (created_at desc, id desc);

create index if not exists idx_plugin_packages_status_recent_keyset
    on plugin_packages (package_status, created_at desc, id desc);

create index if not exists idx_plugin_packages_plugin_recent_keyset
    on plugin_packages (plugin_id, created_at desc, id desc);

create index if not exists idx_plugin_packages_runtime_recent_keyset
    on plugin_packages (runtime, created_at desc, id desc);
