-- Raise the former conservative default; preserve explicitly higher saved settings.
ALTER TABLE storage_accounts DROP CONSTRAINT storage_accounts_qps_check;
ALTER TABLE storage_accounts ALTER COLUMN qps SET DEFAULT 10;
ALTER TABLE storage_accounts ADD CONSTRAINT storage_accounts_qps_check CHECK (qps BETWEEN 1 AND 20);
UPDATE storage_accounts SET qps = 10, updated_at = now() WHERE provider = 'guangya' AND qps = 1;
