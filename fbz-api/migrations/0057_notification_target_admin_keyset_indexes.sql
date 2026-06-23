create index if not exists idx_notification_targets_admin_recent_keyset
    on notification_targets (target_type, name asc, id asc);

create index if not exists idx_notification_targets_admin_enabled_recent_keyset
    on notification_targets (is_enabled, target_type, name asc, id asc);

create index if not exists idx_notification_targets_admin_channel_recent_keyset
    on notification_targets (channel, target_type, name asc, id asc)
    where channel is not null;

create index if not exists idx_notification_targets_admin_channel_enabled_recent_keyset
    on notification_targets (channel, is_enabled, target_type, name asc, id asc)
    where channel is not null;
