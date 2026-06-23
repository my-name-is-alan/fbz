create index if not exists idx_devices_device_id_recent_active
    on devices (device_id, (coalesce(last_seen_at, created_at)) desc, id desc)
    where revoked_at is null;
