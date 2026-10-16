-- 0101_storage_provider_extension.sql（additive）
-- StorageProvider 扩展点：放宽 storage_accounts.provider 单值特判，允许注册表内短 id。
-- 不改鉴权网关；光鸭仍为默认内置适配器（provider='guangya'）。

ALTER TABLE storage_accounts DROP CONSTRAINT IF EXISTS storage_accounts_provider_check;

ALTER TABLE storage_accounts
  ADD CONSTRAINT storage_accounts_provider_check
  CHECK (
    length(trim(provider)) BETWEEN 1 AND 64
    AND provider ~ '^[a-z][a-z0-9._-]*$'
  );
