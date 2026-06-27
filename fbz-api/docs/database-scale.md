# Database Scale Rules

FBZ is designed for libraries around 5 PB with hundreds of thousands of movies, TV items, episodes, music tracks, artwork variants, playback events, and scan/job history.

## Identity Model

- Use compact `bigint` IDs for hot joins inside PostgreSQL.
- Expose `public_id uuid` to API clients and Emby-compatible DTOs.
- Keep Emby-compatible IDs at the compatibility boundary; do not make Emby DTOs the database model.
- Separate media identity from file identity:
  - `media_items` represents the logical movie, series, season, episode, artist, album, track, folder, or collection item.
  - `media_files` represents physical files, STRM entries, path hashes, sizes, mtimes, and container-level probe data.
  - `media_streams` represents ffprobe stream rows.

## Query Rules

- Use keyset/cursor pagination for large browse and search endpoints. Avoid offset pagination on hot paths.
- Browse queries should start from indexed predicates such as `library_id`, `item_type`, `parent_id`, `created_at`, and `id`.
- Browse, resume, and latest parent scopes should parse valid `ParentId` values to UUID once in a request CTE and compare `libraries.public_id` / `media_items.public_id` as UUIDs; invalid non-empty parent IDs should naturally produce an empty scope without casting the indexed column to text. SQL UUID guards should use the canonical `8-4-4-4-12` UUID text shape so valid public IDs are not accidentally rejected before the indexed UUID comparison.
- Continue-watching reads should use `user_playstates(user_id, updated_at desc, media_item_id desc)` with the partial predicate `played = false and position_ticks > 0`, keeping playback-state summary columns covered for the Emby resume surface.
- Positive user-state-only filters should start from `user_playstates` and stay aligned with narrow covering indexes for favorite, played, resumable, and rating-backed likes/dislikes before joining back to permission-filtered media items. Do not treat `IsUnplayed` as the same index shape because it also matches items without a playstate row. These browse windows should fetch `limit + 1` rows and return lower-bound counts instead of exact `count(*) over()` totals.
- Latest-added reads should use `media_items(library_id, created_at desc, id desc)`, fetch `limit + 1` rows for a bounded probe, and trim the extra row before the Emby-compatible array response. They must not calculate exact total counts when the endpoint returns only an item array.
- UUID-only `Ids` browse filters should convert bounded request text values to UUIDs before joining `media_items.public_id`; avoid `public_id::text` comparisons on this hot path so the unique public ID index remains usable. Keep this fast path on `limit + 1` lower-bound pagination rather than exact window counts.
- Browse, latest, resume, show, and item-detail reads should fetch primary media file summaries with `media_files(media_item_id, is_primary desc, id)` so list pages do not sort file rows per item.
- Browse reads that request Emby image tags should aggregate artwork with `artwork(media_item_id, artwork_type, is_primary desc, id)` so each item can emit stable image tags without per-row artwork sorting.
- Show season, episode, and next-up reads should use parent-child predicates backed by `media_items(parent_id, item_type, index_number, sort_title, id)` instead of sorting every child row for large series. Season and episode child lists should fetch `limit + 1` rows and return lower-bound counts instead of forcing exact `count(*) over()` windows across a large series.
- Search should use `media_items.search_vector` and the GIN index first.
- Provider-id-only browse filters should start from `media_external_ids` with `lower(provider || '.' || external_id)` and stay aligned with `idx_media_external_ids_provider_external_lower_item`, then join back to permission-filtered media items. Keep this fast path on `limit + 1` lower-bound pagination rather than exact window counts.
- Emby Items-family `StartIndex` compatibility windows (`/Users/{Id}/Items`, latest/resume aliases, similar lists, search hints, music detail lists, instant mixes, trailers, special features, intros, and critic reviews) should clamp pathological start indexes in the route layer before they reach repository offset queries, in-memory skips, or empty compatibility responses. Deep exact paging should move to cursor/keyset endpoints instead of unbounded client offsets.
- Emby `Items/Counts` is homepage/library overview telemetry. Keep each item-type bucket and the total item count on bounded `limit 10001` lower-bound probes over repository-side permission-filtered media items instead of exact-counting the full visible library scope.
- Scheduled task run history should keep recent-run reads aligned with `scheduled_task_runs(task_id, started_at desc, id desc)` so admin and Emby-compatible task surfaces can fetch the last run with stable ordering.
- Admin scheduled-task run history reads should keep status filters and cursor joins aligned with `idx_scheduled_task_runs_task_status_recent_keyset`. Because `scheduled_task_runs` is a partitioned history parent, keep live-schema smoke coverage for status, cursor, invalid cursor, and missing task-key behavior when the query shape changes.
- Admin job list reads should keep global job history aligned with `jobs(created_at desc, id desc)` and the status/type/queue keyset indexes. Cursor joins should continue to parse public job IDs to UUID without casting `jobs.public_id` to text, and live-schema smoke coverage should stay with the query when filters change.
- Admin scheduled-task list reads should keep `task_type`, `owner_type`, and `enabled` filters aligned with the scheduled-task admin keyset indexes while preserving `(enabled desc, next_run_at asc nulls last, updated_at desc, id desc)` ordering. Cursor joins should parse task public IDs as UUIDs without casting `scheduled_tasks.public_id` to text, and live-schema smoke coverage should stay with type/owner/enabled/cursor behavior when the query shape changes.
- Admin scheduled-task list/detail active-run summaries should only probe up to each task's `max_concurrency`; exact run history belongs in the scheduled-task run keyset list.
- Admin user list reads should keep role, disabled, and role+disabled filters aligned with the `idx_users_admin_*_keyset` indexes while preserving `(username_normalized asc, id asc)` keyset ordering. Cursor joins should parse user public IDs as UUIDs without casting `users.public_id` to text, and live-schema smoke coverage should stay with role/disabled/cursor behavior when the query shape changes.
- Admin user list/update policy device and active-session counts are summary telemetry. Keep them on bounded samples over `devices(user_id, last_seen_at desc)` and active `sessions(user_id, revoked_at, expires_at)` predicates instead of exact-counting every device or session per user in list responses.
- Admin user library-permission reads should stay driven by `libraries(name asc, id asc)` and `libraries(library_type, name asc, id asc)` keyset indexes, with the per-user `library_permissions` join kept as a left join so configured and unconfigured filters remain bounded by the library window. Keep live-schema smoke coverage for type, configured/unconfigured, cursor, hidden-library effective permissions, and valid-missing user IDs when the query shape changes.
- Notification audit reads should keep global request history aligned with `plugin_notification_requests(created_at desc, id desc)` and per-request delivery attempts aligned with `notification_delivery_attempts(notification_request_id, created_at desc, id desc)`. Status/channel filters and delivery-attempt status filters should keep matching their keyset indexes and retain live-schema smoke coverage when the query shape changes.
- Admin notification target reads should keep `target_type`, `channel`, and `is_enabled` filters aligned with `idx_notification_targets_admin_*_keyset`, preserving the `(target_type asc, name asc, id asc)` keyset order without offset scans. Keep live-schema smoke coverage for type/channel/enabled filters, cursor pagination, and invalid cursor handling when the query shape changes.
- Admin job run/event history reads should keep per-job runs aligned with `job_runs(job_id, started_at desc, id desc)` and per-job events aligned with `job_events(job_id, created_at desc, id desc)`. Status/level filters, cursor joins, and detail summaries must retain live-schema smoke coverage because both tables are partitioned parents in the migrated schema.
- Notification delivery worker claims should stay aligned with `event_outbox` partial indexes for `event_type = 'notification.send.requested'`: one `(available_at, id)` index for pending/failed retries and one `(locked_until, id)` index for expired delivering leases. Do not rely on the generic status/available outbox index for this high-growth worker path.
- Plugin dispatch worker claims should use the same prior-state pattern for `plugin.hook.dispatch`: capture `status` and `locked_by` in the atomic claim CTE, log expired delivering lease takeovers, and keep the pending/failed and expired-lease paths aligned with their event-type partial indexes.
- Plugin dispatch Admin audit reads should keep global dispatch history aligned with `event_outbox(created_at desc, id desc) where event_type = 'plugin.hook.dispatch'` and per-dispatch execution history aligned with `plugin_execution_runs(outbox_event_public_id, started_at desc, id desc)`. Status filters and cursor pagination must keep matching their keyset indexes and retain live-schema smoke coverage when the query shape changes.
- Plugin Host API media item summaries should parse `libraryId` to UUID on the request side, join `libraries.public_id` as UUID, use cursor/keyset pagination only, and keep the stable `library_id + sort key + id` order aligned with `idx_media_items_library_sort_visible`. Avoid exact total counts; parse `startIndex` only as legacy input metadata, not as an offset scan trigger.
- Plugin Host API call-budget checks should stay aligned with `idx_plugin_host_api_calls_execution_plugin_budget` so each execution run's limit check uses a bounded index scan instead of walking the audit table.
- Plugin Host API audit reads that combine `pluginId` or `executionRunId` with `statusCode` should stay aligned with `idx_plugin_host_api_calls_plugin_status_finished_keyset` and `idx_plugin_host_api_calls_execution_status_finished_keyset`, preserving the `(finished_at desc, id desc)` keyset order without falling back to status-only scans. Keep live-schema smoke coverage for plugin/status, run/status, cursor, joined token/run IDs, and invalid execution-run IDs when the query shape changes.
- Plugin execution stale-run recovery should claim a bounded candidate batch (`limit 1000`) ordered by `started_at, id` with `FOR UPDATE SKIP LOCKED`, aligned with `idx_plugin_execution_runs_stale_recovery`, before joining to outbox lease state and revoking Host Tokens. Do not update every stale running execution run in one transaction after a plugin worker outage.
- Emby playlist list and playlist-item reads keep `StartIndex` compatibility for clients, but the compatibility route must clamp pathological `StartIndex` values before reaching the repository offset query. Full cursor pagination for exact deep playlist pages remains future work.
- Emby facet and dictionary aggregation endpoints such as genres, artists, persons, studios, tags, official ratings, years, containers, codecs, stream languages, and prefixes should fetch `limit + 1` rows instead of forcing exact `count(*) over()` windows across the full visible aggregation scope. QueryResult-shaped endpoints should return lower-bound counts; array-shaped prefix endpoints should trim the extra probe row before responding. Keep their `StartIndex` compatibility windows capped in the route layer before offset-backed repository queries.
- Emby `Users/Query` and `Users/Prefixes` keep `StartIndex` compatibility for clients, but should clamp pathological start indexes, fetch `limit + 1` rows, and return lower-bound `TotalRecordCount` instead of running a separate exact `count(*)` over the users table before every page.
- Emby admin and compatibility boundary windows such as `System/ActivityLog/Entries`, `Auth/Keys`, `Items/{Id}/RemoteImages`, `Library/VirtualFolders/Query`, `Channels`, Live TV list probes, and Sync list probes should clamp pathological `StartIndex` values in the route layer even while they return empty or small compatibility results. If these surfaces become repository-backed later, they should already enter storage with bounded windows.
- `/ready` queue and scheduler summaries, plus single-row admin queue status summaries, are probe telemetry, not exact audit reports. Keep them on bounded `limit + 1` samples over indexed queue and lease predicates and return lower-bound backlog/run counts instead of exact full-table counts on `jobs`, `event_outbox`, `transcoding_sessions`, notification queues, `scheduled_tasks`, and `scheduled_task_runs`. Expose explicit precision metadata when a summary can be truncated; exact history inspection belongs in Admin keyset list endpoints.
- Scheduler due and manual claim capacity checks should only prove whether `max_concurrency` active runs already exist for that task. Use bounded active-run probes aligned with `scheduled_task_runs(task_id, lease_expires_at) where status = 'running'` instead of exact-counting every active run for a task before every claim.
- Scheduler stale-run recovery should claim a bounded candidate batch (`limit 1000`) ordered by `lease_expires_at, id` with `FOR UPDATE SKIP LOCKED`, aligned with the global running lease index, instead of expiring every stale `scheduled_task_runs` row in one transaction after a scheduler outage.
- Transcode worker claim capacity checks should only prove whether `max_concurrent` active sessions already exist. Use a bounded running-session probe aligned with the running lease index instead of exact-counting all active `transcoding_sessions`.
- Transcode stale-lease recovery should claim a bounded candidate batch (`limit 1000`) ordered by `lease_expires_at, id` with `FOR UPDATE SKIP LOCKED`, aligned with `idx_transcoding_sessions_running_lease`, instead of requeueing or failing every expired running transcode session in one transaction after a worker outage.
- Transcode output cleanup should list only terminal `failed` / `cancelled` sessions with `output_cleaned_at is null` and `output_path is not null`, ordered by `(finished_at asc nulls first, id asc)` to stay aligned with `idx_transcoding_sessions_output_cleanup_pending`; marking a session cleaned should keep using UUID-safe `public_id = case ... then $1::uuid` comparisons.
- Transcode session reads and writes that receive public session or item IDs should parse request values to UUID before comparing with `transcoding_sessions.public_id` or `media_items.public_id`; HLS manifest/segment lookup, cancellation, and terminal status updates should not cast indexed public ID columns to text.
- Worker job claims that accept an optional public job ID should parse that value to UUID in a request CTE before comparing with `jobs.public_id`. Scan and metadata target lookups should do the same before comparing `libraries.public_id` or `media_items.public_id`.
- Generic job stale-lease recovery should claim a bounded candidate batch (`limit 1000`) ordered by `locked_until, id` with `FOR UPDATE SKIP LOCKED`, aligned with `idx_jobs_stale_recovery`, instead of updating every expired running job in one transaction after a long outage.
- Admin queue entrypoints that receive public library or item IDs should parse request values to UUID before comparing with `libraries.public_id` or `media_items.public_id`; queued payloads should use canonical public IDs read from PostgreSQL. Library-scan enqueue paths should treat queued, running, and retryable failed jobs (`attempts < max_attempts`) as active duplicates aligned with the `payload->>'libraryId'` expression index; manual admin enqueue should return the existing active scan job instead of inserting another row.

## Write Rules

- Do not write scan/probe/job/playback event history into `media_items`.
- Keep mutable file observations in `media_files` and stream observations in `media_streams`.
- Keep user-specific playback state in `user_playstates`.
- Keep append-heavy logs in dedicated tables such as `job_events`, `event_outbox`, and playback/session history tables.
- Use idempotency keys such as `jobs.dedupe_key` for queue work that may be submitted more than once.

## SQL Safety

- Application SQL must use parameter binding through `sqlx` or repository methods.
- Do not concatenate user input into SQL strings.
- If dynamic ordering or filtering is needed, map user input to a closed enum or allowlist before building SQL.
- Keep each repository method responsible for one aggregate or query family; avoid generic "run arbitrary SQL" helpers.
- Use transactions for multi-step writes that must update data and audit/outbox rows together.
- Long-running library scans must run in bounded batches. Persist continuation cursors in queue payloads or future scan-session rows so worker restarts, NAS disconnects, and large directory trees do not drop unvisited files.
- Repeated scans should compare stable file observations (`path_hash`, `file_size`, `modified_at`, STRM target) before updating hot media rows or queueing metadata/probe work. Unchanged files count as scanned, not updated.
- Missing-file detection should only run at the end of a complete scan generation. Use a shared scan id across continuation jobs and skip missing convergence when any configured library root is unavailable.
- Metadata, probe, and library-scan jobs should be queued only when no active duplicate exists. Keep those active-job lookups aligned with expression indexes on `payload->>'itemId'`, `payload->>'mediaFileId'`, and `payload->>'libraryId'` plus bounded active statuses. Retryable failed jobs are still active dedupe entries because the worker claim queries will pick them up again.

## Partition Readiness

Do not partition small tables early. The current schema keeps append-heavy tables time-keyed so they can be partitioned later:

- `job_events(created_at)`
- `event_outbox(created_at, available_at)`
- `playback_sessions(started_at, last_progress_at)`
- future scan/probe event tables
- future audit tables

When a table is partitioned later, use time-range partitions and keep query predicates time-bounded for operational views.

A concrete partition key / retention / archival / hot-cold / materialized-stats design for the high-growth tables lives in `docs/database-partitioning-design.md`. That design is not yet applied: table partitioning is a structural change and requires explicit confirmation before any partition migration is produced.

## Storage Rules

- Store media bytes, artwork bytes, transcode segments, and HLS manifests outside PostgreSQL.
- PostgreSQL stores paths, storage keys, hashes, dimensions, durations, and metadata needed for lookup.
- `media_files.path_hash` is SHA-256 bytes over normalized path. It is indexed for scan deduplication.
- STRM targets are stored as text metadata only; playback URL safety is enforced by the playback layer with private-network and domain allowlist policy.

## Performance Guardrails

- Prefer a small number of explicit indexes tied to known query shapes over broad speculative indexing.
- Review `EXPLAIN (ANALYZE, BUFFERS)` for browse/search/continue-watching queries before widening API usage.
- Keep hot rows narrow. Large provider payloads, raw probe JSON, and debug responses should go to dedicated history/debug tables with retention.
- Keep ffprobe execution outside scan transactions. Scan only queues `media.probe`; the probe worker owns container, duration, bitrate, and `media_streams` replacement.
- Use Redis for queue coordination, locks, and short-lived cache; PostgreSQL remains the source of truth.
