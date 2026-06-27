create index if not exists idx_event_outbox_notification_delivery_available
    on event_outbox (available_at, id)
    where event_type = 'notification.send.requested'
      and status in ('pending', 'failed')
      and attempts < max_attempts;

create index if not exists idx_event_outbox_notification_delivery_expired_lease
    on event_outbox (locked_until, id)
    where event_type = 'notification.send.requested'
      and status = 'delivering'
      and attempts < max_attempts;
