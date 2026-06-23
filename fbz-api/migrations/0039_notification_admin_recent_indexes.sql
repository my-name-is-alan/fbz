create index if not exists idx_plugin_notification_requests_recent
    on plugin_notification_requests (created_at desc, id desc)
    include (public_id, plugin_id, package_id, level, channel, status, outbox_event_id, updated_at);

create index if not exists idx_notification_delivery_attempts_request_recent
    on notification_delivery_attempts (notification_request_id, created_at desc, id desc)
    include (
        public_id,
        outbox_event_id,
        target_public_id,
        target_type,
        attempt,
        status,
        response_status,
        duration_ms,
        finished_at
    );
