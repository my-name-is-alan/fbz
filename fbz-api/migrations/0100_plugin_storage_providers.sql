alter table storage_accounts drop constraint storage_accounts_provider_check;
alter table storage_accounts add constraint storage_accounts_provider_check check(provider='guangya' or provider ~ '^[a-z][a-z0-9._-]{2,127}$');

ALTER TABLE plugin_permissions DROP CONSTRAINT plugin_permissions_permission_key_check1;
ALTER TABLE plugin_permissions ADD CONSTRAINT plugin_permissions_permission_key_check1 CHECK(permission_key IN ('admin.menu','library.read','library.write','media.read','metadata.read','metadata.write','notification.send','playback.read','scheduler.register','webhook.emit','storage.provider'));
