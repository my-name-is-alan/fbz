create table if not exists plugin_packages (
    id bigserial primary key,
    public_id uuid not null default gen_random_uuid(),
    plugin_id text not null,
    package_version text not null,
    api_version text not null,
    runtime text not null,
    name text not null,
    description text,
    entrypoint text not null,
    package_path text not null,
    manifest jsonb not null,
    manifest_hash bytea not null,
    checksum_sha256 bytea,
    signature text,
    package_status text not null default 'pending_approval',
    created_at timestamptz not null default now(),
    updated_at timestamptz not null default now(),
    unique (public_id),
    unique (plugin_id, package_version),
    check (length(trim(plugin_id)) between 3 and 128),
    check (length(trim(package_version)) > 0),
    check (api_version = '1'),
    check (runtime in ('wasi', 'http')),
    check (length(trim(name)) > 0),
    check (length(trim(entrypoint)) > 0),
    check (length(trim(package_path)) > 0),
    check (jsonb_typeof(manifest) = 'object'),
    check (length(manifest_hash) = 32),
    check (checksum_sha256 is null or length(checksum_sha256) = 32),
    check (package_status in ('pending_approval', 'approved', 'rejected', 'disabled'))
);

create table if not exists plugin_installations (
    id bigserial primary key,
    public_id uuid not null default gen_random_uuid(),
    plugin_id text not null,
    active_package_id bigint references plugin_packages(id) on delete restrict,
    enabled boolean not null default false,
    approval_status text not null default 'pending_approval',
    permission_fingerprint bytea not null,
    config jsonb not null default '{}'::jsonb,
    approved_by bigint references users(id) on delete set null,
    approved_at timestamptz,
    disabled_at timestamptz,
    last_error text,
    created_at timestamptz not null default now(),
    updated_at timestamptz not null default now(),
    unique (public_id),
    unique (plugin_id),
    check (length(trim(plugin_id)) between 3 and 128),
    check (approval_status in ('pending_approval', 'approved', 'rejected', 'requires_reapproval')),
    check (length(permission_fingerprint) = 32),
    check (jsonb_typeof(config) = 'object'),
    check (enabled = false or approval_status = 'approved'),
    check ((approved_at is null and approved_by is null) or approval_status in ('approved', 'requires_reapproval'))
);

create table if not exists plugin_permissions (
    id bigserial primary key,
    package_id bigint not null references plugin_packages(id) on delete cascade,
    permission_key text not null,
    permission_scope text,
    reason text,
    created_at timestamptz not null default now(),
    unique (package_id, permission_key, permission_scope),
    check (length(trim(permission_key)) > 0),
    check (permission_key in (
        'admin.menu',
        'library.read',
        'library.write',
        'media.read',
        'metadata.read',
        'metadata.write',
        'notification.send',
        'playback.read',
        'scheduler.register',
        'webhook.emit'
    ))
);

create table if not exists plugin_hooks (
    id bigserial primary key,
    package_id bigint not null references plugin_packages(id) on delete cascade,
    event_key text not null,
    handler text not null,
    priority integer not null default 0,
    enabled boolean not null default true,
    created_at timestamptz not null default now(),
    unique (package_id, event_key, handler),
    check (length(trim(event_key)) > 0),
    check (length(trim(handler)) > 0),
    check (event_key in (
        'library.scan.started',
        'library.scan.completed',
        'library.scan.failed',
        'media.item.created',
        'media.item.updated',
        'playback.started',
        'playback.stopped',
        'scheduler.tick',
        'user.login',
        'webhook.received'
    ))
);

create table if not exists plugin_menu_items (
    id bigserial primary key,
    package_id bigint not null references plugin_packages(id) on delete cascade,
    item_key text not null,
    label text not null,
    path text not null,
    parent_key text,
    required_permission text,
    weight integer not null default 0,
    enabled boolean not null default true,
    created_at timestamptz not null default now(),
    unique (package_id, item_key),
    check (length(trim(item_key)) > 0),
    check (length(trim(label)) > 0),
    check (path like '/admin/plugins/%'),
    check (required_permission is null or length(trim(required_permission)) > 0)
);

create table if not exists plugin_schedule_definitions (
    id bigserial primary key,
    package_id bigint not null references plugin_packages(id) on delete cascade,
    task_key text not null,
    schedule_kind text not null,
    schedule_value text not null,
    handler text not null,
    enabled_by_default boolean not null default false,
    timeout_seconds integer not null default 300,
    created_at timestamptz not null default now(),
    unique (package_id, task_key),
    check (length(trim(task_key)) > 0),
    check (schedule_kind in ('interval', 'cron')),
    check (length(trim(schedule_value)) > 0),
    check (length(trim(handler)) > 0),
    check (timeout_seconds > 0)
);

create table if not exists plugin_kv (
    plugin_id text not null references plugin_installations(plugin_id) on delete cascade,
    key text not null,
    value jsonb not null,
    updated_at timestamptz not null default now(),
    primary key (plugin_id, key),
    check (length(trim(key)) > 0)
);

create index if not exists idx_plugin_packages_status
    on plugin_packages (package_status, plugin_id, package_version);

create index if not exists idx_plugin_installations_enabled
    on plugin_installations (enabled, plugin_id)
    where enabled = true;

create index if not exists idx_plugin_permissions_key
    on plugin_permissions (permission_key, package_id);

create index if not exists idx_plugin_hooks_event_enabled
    on plugin_hooks (event_key, priority desc, package_id)
    where enabled = true;

create index if not exists idx_plugin_menu_items_package_weight
    on plugin_menu_items (package_id, weight, item_key)
    where enabled = true;

create index if not exists idx_plugin_schedule_definitions_package
    on plugin_schedule_definitions (package_id, task_key);
