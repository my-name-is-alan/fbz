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
- Browse, resume, and latest parent scopes should parse valid `ParentId` values to UUID once in a request CTE and compare `libraries.public_id` / `media_items.public_id` as UUIDs; invalid non-empty parent IDs should naturally produce an empty scope without casting the indexed column to text.
- Continue-watching reads should use `user_playstates(user_id, updated_at desc, media_item_id desc)` with the partial predicate `played = false and position_ticks > 0`, keeping playback-state summary columns covered for the Emby resume surface.
- Positive user-state-only filters should start from `user_playstates` and stay aligned with narrow covering indexes for favorite, played, resumable, and rating-backed likes/dislikes before joining back to permission-filtered media items. Do not treat `IsUnplayed` as the same index shape because it also matches items without a playstate row.
- Latest-added reads should use `media_items(library_id, created_at desc, id desc)` and must not calculate exact total counts when the Emby-compatible endpoint returns only an item array.
- UUID-only `Ids` browse filters should convert bounded request text values to UUIDs before joining `media_items.public_id`; avoid `public_id::text` comparisons on this hot path so the unique public ID index remains usable.
- Browse, latest, resume, show, and item-detail reads should fetch primary media file summaries with `media_files(media_item_id, is_primary desc, id)` so list pages do not sort file rows per item.
- Browse reads that request Emby image tags should aggregate artwork with `artwork(media_item_id, artwork_type, is_primary desc, id)` so each item can emit stable image tags without per-row artwork sorting.
- Show season, episode, and next-up reads should use parent-child predicates backed by `media_items(parent_id, item_type, index_number, sort_title, id)` instead of sorting every child row for large series.
- Search should use `media_items.search_vector` and the GIN index first.
- Provider-id-only browse filters should start from `media_external_ids` with `lower(provider || '.' || external_id)` and stay aligned with `idx_media_external_ids_provider_external_lower_item`, then join back to permission-filtered media items.
- Scheduled task run history should keep recent-run reads aligned with `scheduled_task_runs(task_id, started_at desc, id desc)` so admin and Emby-compatible task surfaces can fetch the last run with stable ordering.
- Notification audit reads should keep global request history aligned with `plugin_notification_requests(created_at desc, id desc)` and per-request delivery attempts aligned with `notification_delivery_attempts(notification_request_id, created_at desc, id desc)`.
- Plugin Host API media item summaries should parse `libraryId` to UUID on the request side, join `libraries.public_id` as UUID, default to cursor/keyset pagination, and keep the stable `library_id + sort key + id` order aligned with `idx_media_items_library_sort_visible`. Avoid exact total counts on cursor reads; reserve `startIndex` / offset compatibility and `count(*) over()` for explicit legacy callers.
- Plugin Host API call-budget checks should stay aligned with `idx_plugin_host_api_calls_execution_plugin_budget` so each execution run's limit check uses a bounded index scan instead of walking the audit table.
- Plugin Host API audit reads that combine `pluginId` or `executionRunId` with `statusCode` should stay aligned with `idx_plugin_host_api_calls_plugin_status_finished_keyset` and `idx_plugin_host_api_calls_execution_status_finished_keyset`, preserving the `(finished_at desc, id desc)` keyset order without falling back to status-only scans.
- Transcode session reads and writes that receive public session or item IDs should parse request values to UUID before comparing with `transcoding_sessions.public_id` or `media_items.public_id`; HLS manifest/segment lookup, cancellation, and terminal status updates should not cast indexed public ID columns to text.
- Worker job claims that accept an optional public job ID should parse that value to UUID in a request CTE before comparing with `jobs.public_id`. Scan and metadata target lookups should do the same before comparing `libraries.public_id` or `media_items.public_id`.
- Admin queue entrypoints that receive public library or item IDs should parse request values to UUID before comparing with `libraries.public_id` or `media_items.public_id`; queued payloads should use canonical public IDs read from PostgreSQL.

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
- Metadata, probe, and library-scan jobs should be queued only when no active duplicate exists. Keep those active-job lookups aligned with expression indexes on `payload->>'itemId'`, `payload->>'mediaFileId'`, and `payload->>'libraryId'` plus bounded active statuses.

## Partition Readiness

Do not partition small tables early. The current schema keeps append-heavy tables time-keyed so they can be partitioned later:

- `job_events(created_at)`
- `event_outbox(created_at, available_at)`
- `playback_sessions(started_at, last_progress_at)`
- future scan/probe event tables
- future audit tables

When a table is partitioned later, use time-range partitions and keep query predicates time-bounded for operational views.

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
