-- 0092_display_preferences.sql（additive）：Emby 显示偏好与用户设置持久化。
-- DisplayPreferences/{id}?client= 按 (user, client, key) 一行；CustomPrefs 存 jsonb。
-- /Users/{id}/Settings 与 typed settings（TypedSettings/{key}）按 (user, key) 一行，
-- 值统一存 jsonb（普通设置为 json 字符串，typed 设置为任意 json）。

create table if not exists user_display_preferences (
    id bigserial primary key,
    user_id bigint not null references users(id) on delete cascade,
    client text not null,
    preferences_key text not null,
    sort_by text,
    sort_order text,
    custom_prefs jsonb not null default '{}'::jsonb,
    created_at timestamptz not null default now(),
    updated_at timestamptz not null default now(),
    unique (user_id, client, preferences_key),
    check (length(trim(client)) > 0),
    check (length(trim(preferences_key)) > 0),
    check (sort_order is null or sort_order in ('Ascending', 'Descending'))
);

create table if not exists user_settings (
    id bigserial primary key,
    user_id bigint not null references users(id) on delete cascade,
    setting_key text not null,
    setting_value jsonb not null,
    created_at timestamptz not null default now(),
    updated_at timestamptz not null default now(),
    unique (user_id, setting_key),
    check (length(trim(setting_key)) > 0)
);
