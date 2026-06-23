create index if not exists idx_plugin_notification_requests_status_recent_keyset
    on plugin_notification_requests (status, created_at desc, id desc);

create index if not exists idx_plugin_notification_requests_channel_recent_keyset
    on plugin_notification_requests (channel, created_at desc, id desc)
    where channel is not null;

create index if not exists idx_plugin_notification_requests_channel_status_recent_keyset
    on plugin_notification_requests (channel, status, created_at desc, id desc)
    where channel is not null;

create index if not exists idx_notification_delivery_attempts_request_status_recent_keyset
    on notification_delivery_attempts (notification_request_id, status, created_at desc, id desc);
