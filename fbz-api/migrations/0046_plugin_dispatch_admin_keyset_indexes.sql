create index if not exists idx_event_outbox_plugin_dispatch_recent_keyset
    on event_outbox (created_at desc, id desc)
    where event_type = 'plugin.hook.dispatch';

create index if not exists idx_event_outbox_plugin_dispatch_status_recent_keyset
    on event_outbox (status, created_at desc, id desc)
    where event_type = 'plugin.hook.dispatch';
