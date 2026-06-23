create index if not exists idx_event_outbox_plugin_dispatch_available
    on event_outbox (available_at, id)
    where event_type = 'plugin.hook.dispatch'
      and status in ('pending', 'failed')
      and attempts < max_attempts;

create index if not exists idx_event_outbox_plugin_dispatch_expired_lease
    on event_outbox (locked_until, id)
    where event_type = 'plugin.hook.dispatch'
      and status = 'delivering'
      and attempts < max_attempts;
