create table if not exists server_settings (
    key text primary key,
    value jsonb not null,
    requires_restart boolean not null default false,
    value_version bigint not null default 1 check (value_version > 0),
    created_at timestamptz not null default now(),
    updated_at timestamptz not null default now(),
    check (length(trim(key)) > 0)
);

create table if not exists server_setting_audit (
    id bigserial primary key,
    setting_key text not null references server_settings(key) on delete cascade,
    old_value jsonb,
    new_value jsonb not null,
    changed_by text not null,
    change_reason text,
    value_version bigint not null check (value_version > 0),
    changed_at timestamptz not null default now(),
    check (length(trim(changed_by)) > 0)
);

create index if not exists idx_server_setting_audit_setting_changed_at
    on server_setting_audit (setting_key, changed_at desc);
