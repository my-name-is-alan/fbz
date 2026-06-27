-- Bounded plugin execution stale recovery scans the oldest still-running
-- execution audits first, then joins their outbox events to determine whether
-- the dispatch lease is stale or already terminal. Keep that candidate batch
-- selective when plugin_execution_runs grows large.
create index if not exists idx_plugin_execution_runs_stale_recovery
    on plugin_execution_runs (started_at asc, id asc)
    where status = 'running'
      and finished_at is null;
