create index if not exists idx_users_admin_username_keyset
    on users (username_normalized asc, id asc);

create index if not exists idx_users_admin_role_username_keyset
    on users (role_id, username_normalized asc, id asc);

create index if not exists idx_users_admin_disabled_username_keyset
    on users (is_disabled, username_normalized asc, id asc);

create index if not exists idx_users_admin_role_disabled_username_keyset
    on users (role_id, is_disabled, username_normalized asc, id asc);
