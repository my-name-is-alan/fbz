# Plugin System Rules

The plugin system starts as a controlled declaration and approval model. Runtime execution is added only after the database model, manifest validation, permissions, hooks, schedules, and admin approval boundaries are stable.

## Manifest Contract

Every plugin package must provide a manifest with:

- `id`: lowercase plugin identifier, 3-128 characters, not in the reserved `core.` namespace.
- `name`: display name.
- `version`: semver-like package version.
- `apiVersion`: currently `1`.
- `runtime`: currently `wasi` or `http`.
- `entrypoint`: relative package path for `wasi`, HTTP(S) URL for `http`.
- `permissions`: explicit permission declarations with optional scope and reason.
- `hooks`: event subscriptions and handler names.
- `schedules`: plugin-owned scheduled task declarations.
- `menu`: optional admin UI entries under `/admin/plugins/{plugin_id}`.

Manifest validation is strict by design. A plugin may not declare unknown permissions, duplicate permission key/scope pairs, unsupported hook events, duplicate hook event/handler pairs, duplicate or unscoped schedule keys, menu paths outside its admin namespace, or WASI entrypoints that escape the package. Menu paths must be exactly `/admin/plugins/{plugin_id}` or a child path under that namespace, so prefix collisions such as `/admin/plugins/{plugin_id}-other` are rejected. Menu keys must be unique, parent keys must reference another declared menu item in the same package, and `menu.requiredPermission` must reference a permission declared by the plugin. Hook event support is also enforced by the `plugin_hooks` database constraint so package installation cannot persist declarations outside the host contract.

## Permission Boundary

Plugins never receive raw database access. They declare permissions and must later call controlled host APIs. Current permission keys are:

- `admin.menu`
- `library.read`
- `library.write`
- `media.read`
- `metadata.read`
- `metadata.write`
- `notification.send`
- `playback.read`
- `scheduler.register`
- `webhook.emit`

Menu declarations require `admin.menu`. Schedule declarations require `scheduler.register`. A menu item can additionally declare `requiredPermission`, but only for a permission that the same manifest declares; this keeps the admin UI from advertising plugin pages that imply capabilities the runtime will not grant.

## Database Model

Plugin package metadata and declarations are stored separately:

- `plugin_packages`: immutable package-level manifest, hash, runtime, status, and package path.
- `plugin_installations`: active plugin state, approval status, enabled flag, config, permission fingerprint, and errors.
- `plugin_permissions`: normalized permission declarations.
- `plugin_hooks`: normalized event hook declarations.
- `plugin_menu_items`: normalized admin menu declarations.
- `plugin_schedule_definitions`: normalized schedule declarations.
- `plugin_kv`: plugin-private key/value storage.

The manifest JSON is kept for audit/debugging, while normalized tables support fast admin views, hook dispatch, approval checks, and future plugin runtime loading.

## Admin Lifecycle

The current admin API supports installing a plugin package, listing plugins, reviewing a package's normalized declarations, approving or rejecting a package, and enabling or disabling a plugin installation. Approval and enablement are deliberately separate:

Administrators can also call `GET /api/admin/plugins/capabilities` before installing a package. It returns the same host contract that running plugins see through `GET /api/plugin/capabilities`: supported manifest API version, accepted and executable runtimes, HTTP schemes, permission keys, structured permission details, hook event keys, and Host API route requirements. `permissionDetails` keeps the stable key list compatible while adding category, risk level, description, manifest feature bindings, and the Host API routes opened by each permission. This lets the web admin, packaging tools, and CI checks reject incompatible manifests and explain permission risk before writing package rows.

Administrators can list plugin installations with `GET /api/admin/plugins`. The list supports `approvalStatus`, `enabled`, `runtime`, `cursor`, and `limit`; its JSON body remains a plain array for compatibility, while `x-fbz-has-more` and `x-fbz-next-cursor` expose keyset pagination over `(updated_at desc, id desc)`. The route is intentionally installation-oriented: package history belongs to the package list, while this view tells the web admin which plugin is approved, enabled, and currently active.

Administrators can list package versions with `GET /api/admin/plugins/packages`. The list supports `pluginId`, `packageStatus`, `runtime`, `cursor`, and `limit`; its JSON body remains a plain array for compatibility, while `x-fbz-has-more` and `x-fbz-next-cursor` expose keyset pagination over `(created_at desc, id desc)`. Each row includes the immutable package id, plugin id, version, runtime, signature presence, package status, installation approval status, enabled state when present, and whether that package is the active installation target. This gives the web admin a stable review and rollback surface without scanning package history by offset.

- install writes package declarations and keeps the package in `pending_approval`;
- installing a new package for an existing plugin does not replace `plugin_installations.active_package_id`, does not disable the current active package, and does not rewrite the active permission fingerprint;
- approve marks the package and installation as `approved`, switches `active_package_id` to the approved package, stores that package's permission fingerprint on the installation, and keeps `enabled = false`;
- activate switches `active_package_id` to an already approved package without changing the plugin's enabled flag, so administrators can roll back to an older approved package; if the plugin is enabled, plugin schedules are synchronized in the same transaction;
- reject marks the target package as `rejected`; if the package is not the active package, the current active installation remains unchanged;
- enable requires both the installation and active package to be `approved`;
- disable only toggles the installation off and does not erase package approval history.

## Package Signatures

Plugin package installation verifies package identity before persisting declarations. The host always hashes the ZIP payload with SHA-256 and compares `checksumSha256` when the admin provides one. Unless `PLUGIN_ALLOW_UNSIGNED=true`, the request must also include a trusted signature envelope in the form `ed25519:{keyId}:{signatureHex}`.

Trusted signing keys are configured with `PLUGIN_TRUSTED_SIGNATURE_KEYS=keyId:publicKeyHex,...`, where each public key is a 32-byte Ed25519 key encoded as 64 hex characters. The signature message is:

```text
fbz-plugin-package-v1
{pluginId}
{packageVersion}
{zipSha256Hex}
{manifestHashHex}
```

The signature therefore binds both the archive bytes and the normalized manifest contract. Package details expose `signaturePresent` for admin review, but the signature text itself is not returned in normal detail responses.

## Hook Dispatch

Approved and enabled plugins can receive declared hook events through the core `event_outbox`. Current core hook coverage includes library scan lifecycle events (`library.scan.started`, `library.scan.completed`, `library.scan.failed`), metadata refresh outcomes (`metadata.refresh.completed`, `metadata.refresh.failed`), playback lifecycle events (`playback.started`, `playback.stopped`), successful download entrypoints (`media.download.started`), transcode lifecycle events (`transcode.started`, `transcode.completed`, `transcode.failed`), successful user logins (`user.login`), and plugin schedules (`scheduler.tick`).

The dispatcher writes one `plugin.hook.dispatch` outbox row per matching enabled hook. The payload includes the target plugin, package, hook id, handler name, hook event key, and source event payload. This keeps core scanning decoupled from plugin runtime execution.

Administrators can inspect plugin dispatch outbox rows with `GET /api/admin/plugin-dispatches`. The list supports `status`, `cursor`, and `limit`; its JSON body remains a plain array for compatibility, while `x-fbz-has-more` and `x-fbz-next-cursor` expose keyset pagination over `(created_at desc, id desc)`. This keeps long-running hook histories browsable without offset scans.

Execution attempts for a dispatch are visible through `GET /api/admin/plugin-dispatches/{dispatchId}/runs`. The list supports `status`, `cursor`, and `limit`, uses the same pagination headers, and orders by `(started_at desc, id desc)` under the target dispatch id.

When `REDIS_EVENT_STREAMS_ENABLED=true`, a separate mirror worker publishes committed `event_outbox` rows into the configured Redis Stream. The mirror worker uses PostgreSQL lease columns and `FOR UPDATE SKIP LOCKED`, so multiple worker nodes can share the work and an expired lease can be picked up by another node. PostgreSQL remains the source of truth for plugin dispatch and notification delivery; Redis Streams are an external distribution layer for cross-node subscribers, observability, and future realtime fan-out. Stream entries include the public outbox event id, event type, aggregate boundary, serialized payload, delivery status, timestamps, and the internal outbox id for idempotent consumers.

When an approved plugin is enabled, its manifest `schedules` are synchronized into `scheduled_tasks` with `owner_type = 'plugin'`. Schedules marked `enabledByDefault` are enabled automatically; disabling, rejecting, or installing a replacement package disables the plugin's existing scheduled tasks in the same transaction. Interval schedules and a standard five-field cron subset are executable. Supported cron fields include `*`, numeric values, comma lists, ranges, and step values such as `*/5`. When due, the scheduler writes a `plugin.hook.dispatch` outbox row with `hookEvent = 'scheduler.tick'`, `hookId = null`, and a `plugin_schedule` source payload. PostgreSQL computes the next cron run through `fbz_next_cron_run_at`. Due and manual executions create `scheduled_task_runs` leases before work starts, enforce each task's `max_concurrency`, and mark late unfinished leases as `expired` before a later claim. Administrators can inspect scheduled tasks, manually trigger enabled tasks without changing `next_run_at`, and cancel active scheduled-task leases; manual execution uses the same scheduler task handlers as due execution.

Scheduled task operations are visible through Admin API routes:

- `GET /api/admin/scheduled-tasks`
- `GET /api/admin/scheduled-tasks/{taskKey}/runs`
- `POST /api/admin/scheduled-tasks/{taskKey}/run`

Scheduled task lists support `taskType`, `ownerType`, `enabled`, `cursor`, and `limit`, keep a plain array response body for compatibility, and expose `x-fbz-has-more` / `x-fbz-next-cursor` response headers for keyset pagination over enabled state, `next_run_at`, `updated_at`, and id. Scheduled task run history supports `status`, `cursor`, and `limit` with the same response headers and keyset pagination over `(started_at desc, id desc)`.

The plugin worker is disabled by default and can be enabled with `FBZ_PLUGIN_WORKER_ENABLED=true`. It claims `plugin.hook.dispatch` rows with PostgreSQL row locking, writes one `plugin_execution_runs` audit record per attempt, and marks outbox rows as `delivered`, `failed`, or `discarded`. Dispatch leases are at least five minutes and grow with `PLUGIN_TIMEOUT_MS` plus a grace window, so long-running plugins are not retried while still inside their configured timeout. Before each drain loop, the worker marks stale `running` execution runs as `failed` when their dispatch lease has expired or their outbox row is already terminal, and revokes any still-active Host API tokens for those runs. Each worker process runs dispatches in bounded batches capped by `PLUGIN_MAX_CONCURRENCY`; PostgreSQL `FOR UPDATE SKIP LOCKED` keeps concurrent workers from claiming the same outbox row.

HTTP plugin execution is bounded by `PLUGIN_TIMEOUT_MS`, `PLUGIN_HTTP_ALLOWED_HOSTS`, and `PLUGIN_HTTP_MAX_RESPONSE_BODY_BYTES`. The response body is read with the configured byte limit before audit truncation; a response larger than the limit fails the current dispatch attempt instead of being fully buffered in the host process. Successful and failed responses still store only a short audit body, currently capped at 4096 bytes.

WASI plugin execution runs WASIp1 command modules from the extracted package directory. The worker resolves the manifest `entrypoint` under `PLUGIN_PACKAGE_DIR/extracted/{pluginId}/{version}`, rejects traversal and missing files, writes the full dispatch JSON to stdin, and passes the entrypoint and hook `handler` as argv. Runtime context is also exposed through `FBZ_PLUGIN_ID`, `FBZ_PLUGIN_HANDLER`, `FBZ_PLUGIN_IDEMPOTENCY_KEY`, `FBZ_HOST_BASE_URL`, and `FBZ_PLUGIN_TOKEN` environment variables. The sandbox preopens the immutable package directory as read-only `/plugin`, then lazily creates and preopens `PLUGIN_DATA_DIR/{pluginId}` as writable `/data`, `PLUGIN_CACHE_DIR/{pluginId}` as writable `/cache`, and a dispatch-scoped `PLUGIN_TMP_DIR/{pluginId}/{dispatchId}-{nanos}` as writable `/tmp`. The dispatch temp directory is removed after execution on a best-effort basis; before creating a new dispatch temp directory, the runtime scans only the same plugin's tmp root and removes stale dispatch directories older than `PLUGIN_TMP_MAX_AGE_SECONDS`, so interrupted runs do not accumulate unbounded temp data. WASI does not currently expose network capabilities. Wall-clock execution is bounded with Wasmtime epoch interruption using `PLUGIN_TIMEOUT_MS`, while deterministic CPU, memory, captured stdio, and module size are bounded by `PLUGIN_WASI_FUEL`, `PLUGIN_MEMORY_LIMIT_MB`, `PLUGIN_WASI_STDIO_MAX_BYTES`, and `PLUGIN_WASI_MAX_MODULE_BYTES`.

Plugin execution operations are visible through Admin API routes:

- `GET /api/admin/plugin-dispatches`
- `GET /api/admin/plugin-dispatches/{dispatchId}/runs`
- `POST /api/admin/plugin-dispatches/{dispatchId}/replay`
- `GET /api/admin/event-stream-mirror/status`

The replay route only accepts dispatches in `failed` or `discarded` status. It creates a fresh `plugin.hook.dispatch` outbox row with the same payload and leaves the original failed row unchanged for audit. Dispatches in `pending`, `delivering`, or `delivered` status are rejected to avoid accidental duplicate hook side effects.

HTTP plugins receive execution context through headers:

- `X-FBZ-Plugin-Id`: target plugin id.
- `X-FBZ-Plugin-Token`: short-lived Host API token, stored only as a SHA-256 hash by the server.
- `X-FBZ-Plugin-Idempotency-Key`: stable public outbox event id for this dispatch; retries of the same dispatch keep the same value.
- `X-FBZ-Host-Base-Url`: base URL for calling the server's Host API.

When `PLUGIN_SECRET_KEY` is configured, HTTP plugin requests also include signature headers:

- `X-FBZ-Plugin-Signature-Version`: currently `v1`.
- `X-FBZ-Plugin-Signature-Timestamp`: Unix timestamp in seconds.
- `X-FBZ-Plugin-Body-Sha256`: lowercase hex SHA-256 of the exact request body bytes.
- `X-FBZ-Plugin-Signature`: `sha256=` followed by lowercase hex HMAC-SHA256.

The v1 canonical string is `v1 + "\n" + timestamp + "\n" + pluginId + "\n" + idempotencyKey + "\n" + bodySha256`, signed with `PLUGIN_SECRET_KEY`. External plugin services should reject stale timestamps and mismatched signatures before executing side effects, and should use the idempotency key to dedupe side effects across host retries.

Plugins can call `GET /api/plugin/capabilities` with the short-lived host token to discover the current host contract. The response includes the supported manifest `apiVersion`, Host API version, accepted manifest runtimes, currently executable runtimes, accepted HTTP schemes, permission keys, structured `permissionDetails`, hook event keys, and each Host API route with its additional permission requirement.

Host tokens are scoped to a single plugin execution run. A token is valid only while its `plugin_execution_runs` row is still `running`, the package is still the approved active package, the installation remains enabled, the token has not expired, and it has not been revoked. At issue time the host stores a JSON permission snapshot on `plugin_host_tokens`; Host API permission checks use that snapshot instead of re-reading live package permissions on every call. This keeps a running dispatch on the permissions it was launched with and prevents a stale token from gaining capabilities after package state changes.

Each execution run is also capped by `PLUGIN_HOST_API_MAX_CALLS_PER_RUN`. The host counts already audited calls for the current `executionRunId` before running a Host API handler. When the limit is reached, the request returns `429 too_many_requests` and the rejected call is still audited. This protects API nodes from runaway plugins while still allowing large libraries to be paged with cursor-based APIs.

Authenticated Host API calls are written to `plugin_host_api_calls` after token authentication succeeds. Each audit row records the plugin, package, host token, execution run, method, route template, required permission, status code, error code/message when present, and duration. Permission denials and business validation errors are audited; calls that never authenticate a token are rejected before a plugin context exists and are not attributed to a plugin.

Host API audit records are visible through Admin API routes:

- `GET /api/admin/plugin-host-api-calls`
- `GET /api/admin/plugin-execution-runs/{runId}/host-api-calls`

The global Host API audit list supports `pluginId`, `executionRunId`, `statusCode`, `cursor`, and `limit`. The run-scoped audit route supports `cursor` and `limit`. Both JSON bodies remain plain arrays for compatibility, while `x-fbz-has-more` and `x-fbz-next-cursor` response headers expose keyset pagination. The cursor is the last returned call public id and the query orders by `(finished_at desc, id desc)`, so admin pages can browse large audit tables without offset scans.

The first Host API is plugin-private KV storage under `/api/plugin/kv/{key}`. KV access is always scoped to the authenticated plugin id and never exposes another plugin's keys.

Plugins with `library.read` can also call read-only media library Host APIs:

- `GET /api/plugin/libraries`: list non-hidden media libraries.
- `GET /api/plugin/libraries/{libraryId}/items?limit=50`: list non-deleted media item summaries for one library. The default response uses keyset pagination and includes `nextCursor` when more rows are available; pass it back as `cursor` for the next page. In keyset mode `totalRecordCountIsExact=false`, so plugins should use `nextCursor` instead of treating `totalRecordCount` as a full library count. `startIndex` remains accepted for compatibility and returns an exact count when rows are available, but it is an offset path and should not be used for large-library scans.

Plugins with `media.read` can call `GET /api/plugin/items/{itemId}` to read one non-deleted item in a non-hidden library. The response includes public item metadata, external provider IDs, official rating, genres, studios, tags, people, markers, and artwork summaries.

These APIs return public IDs and summary metadata only. They deliberately do not expose filesystem paths, STRM targets, media source URLs, database internal IDs, or user playback state.

Plugins with `metadata.write` can call `PATCH /api/plugin/items/{itemId}/metadata` to update selected public metadata fields, upsert external provider IDs, and replace genre/studio/tag/people lists for one media item. The request is a patch, not a raw row update: the host accepts only title/original title/sort title/overview/year/premiere date/official rating/ratings/runtime ticks, external IDs, genres, studios, tags, and people. It validates text lengths, date/rating ranges, duplicate providers, duplicate classification names, duplicate person-role relationships, list sizes, role types, and rejects external ID conflicts with another media item. Omitted `genres`, `studios`, `tags`, or `people` are left unchanged; an explicit empty array clears that list.

The same permission also allows `PUT /api/plugin/items/{itemId}/artwork` to replace the plugin's own remote artwork set for one media item. The host scopes the stored `source` to `plugin:{pluginId}` or `plugin:{pluginId}:{source}`, accepts only supported artwork types, absolute `http` / `https` URLs without credentials, positive bounded dimensions, and at most one primary image per artwork type. Plugins cannot write local artwork cache paths directly.

`PUT /api/plugin/items/{itemId}/markers` replaces the plugin's own marker set for one media item with the same source scoping model, so a plugin can repeat imports or clear its own TiDb/chapter marker data without deleting markers owned by another plugin or by the core server. Marker writes are bounded to 512 rows per request and validate marker type, tick ranges, and confidence before touching the database.

Plugins with `notification.send` can call `POST /api/plugin/notifications` with `title`, `message`, optional `level`, optional `channel`, and optional object `metadata`. The Host API records the request in `plugin_notification_requests` and writes a `notification.send.requested` outbox event. It does not directly call Telegram, WeCom, or webhook endpoints.

The notification worker is disabled by default and can be enabled with `FBZ_NOTIFICATION_WORKER_ENABLED=true`. It claims `notification.send.requested` outbox rows with PostgreSQL row locking, loads enabled admin-managed `notification_targets` for the request channel, and writes one `notification_delivery_attempts` audit row per target attempt. Supported target types are:

- `webhook`: POSTs the notification payload as JSON to `config.url`, with optional admin-configured headers.
- `telegram`: POSTs `sendMessage` to `config.apiBaseUrl` or Telegram's official API base URL using `config.botToken` and `config.chatId`.
- `wecom`: POSTs a text message to `config.webhookUrl`.

Plugins can choose a logical channel, but they cannot choose target URLs, bot tokens, or webhook secrets. Those remain administrator-managed target config so plugin notifications do not become unrestricted outbound HTTP.

## First-Party Examples

Plugin author workflow, manifest examples, HTTP dispatch signing, idempotency, Host API usage, packaging, and local smoke guidance are documented in `docs/plugin-development.md`.

- `examples/plugins/http-notification-bridge`: HTTP runtime plugin that subscribes to selected host events, verifies signed dispatches when `PLUGIN_SECRET_KEY` is configured, reads its `channel` config through `GET /api/plugin/config`, dedupes by dispatch idempotency key, and forwards notifications through `POST /api/plugin/notifications`.
- `examples/plugins/http-marker-importer`: HTTP runtime plugin that subscribes to `metadata.refresh.completed`, reads public item details through `GET /api/plugin/items/{itemId}`, resolves marker candidates by external provider ID, and replaces only its own plugin-scoped intro/credits marker source through `PUT /api/plugin/items/{itemId}/markers`.

Use `scripts/package-plugin.ps1 -PluginDir examples/plugins/http-notification-bridge -Force` or pass another example plugin directory such as `examples/plugins/http-marker-importer` to build a ZIP whose root contains `manifest.json`. The helper writes to `var/plugin-packages` by default, refuses output paths inside the plugin source directory, and prints the `packagePath`, `checksumSha256`, and manifest object expected by `POST /api/admin/plugins/packages`.

Use `scripts/smoke-plugin-lifecycle.ps1 -StartServer` for a local lifecycle smoke. It generates a one-off HTTP plugin, starts the API against local PostgreSQL and Redis, then verifies login, package install, approval, enablement, config persistence, active menu visibility, and package detail normalization through the real Admin API.

Notification targets are managed through Admin API routes:

- `GET /api/admin/notification-targets`
- `POST /api/admin/notification-targets`
- `PUT /api/admin/notification-targets/{targetId}`
- `POST /api/admin/notification-targets/{targetId}/enable`
- `POST /api/admin/notification-targets/{targetId}/disable`

Target create and replace requests validate the same config contract used by the delivery worker. `FBZ_SECRET_KEY` must be configured before writing targets because webhook URLs, Telegram bot tokens, WeCom webhook URLs, and custom header values are encrypted into `notification_target_secrets`. `notification_targets.config` keeps only non-secret values and `secretRef` markers. Responses redact all secret references. This keeps plugin-facing APIs stable while preventing target credentials from living in the main target config JSON.

Notification delivery operations are visible through Admin API routes:

- `GET /api/admin/notification-requests`
- `GET /api/admin/notification-requests/{requestId}/attempts`
- `POST /api/admin/notification-requests/{requestId}/retry`

The notification request list supports `status`, `channel`, `cursor`, and `limit`; per-request delivery attempts support `status`, `cursor`, and `limit`. Both endpoints keep a plain array response body for compatibility and expose `x-fbz-has-more` / `x-fbz-next-cursor` response headers for keyset pagination over `(created_at desc, id desc)`.

The retry route only accepts requests in `failed` or `discarded` status. It inserts a fresh `notification.send.requested` outbox row for the same notification request and moves the request back to `queued`. During delivery, targets with an existing `succeeded` attempt for that request are skipped, so retrying a partial failure does not resend to targets that already accepted the notification.

Current execution support is intentionally narrow:

- `http` runtime: supported for `http://` and `https://` POST JSON endpoints with timeout and HTTP status checks. The target host must match `PLUGIN_HTTP_ALLOWED_HOSTS`; entries are bare hosts such as `plugins.internal` or suffix wildcards such as `*.example.test`.
- `wasi` runtime: supported for WASIp1 command modules inside the safely extracted plugin package. The host passes dispatch payload through stdin and handler context through argv/env, captures stdout/stderr for audit, enforces epoch-based wall-clock timeout plus fuel, memory, stdio, and module size limits, and scavenges stale dispatch temp directories by plugin scope.
- request signing: enabled when `PLUGIN_SECRET_KEY` is set; the key must be at least 32 characters.
- idempotency key: every HTTP dispatch includes `X-FBZ-Plugin-Idempotency-Key`; the value remains stable for retries of the same outbox dispatch.
- retries use bounded exponential backoff based on the outbox attempt count.

Cross-runtime idempotency helpers, WASIp2/component support, stronger stale temp-dir scavenging, and native Host API imports still belong to the next executor layer.

## Execution Guardrails

WASI is the preferred in-process runtime because it gives stronger default isolation across Windows, Linux, Docker, and NAS deployments. HTTP plugins remain supported as controlled external services, but they still need allowlists, timeouts, signatures, and explicit permission checks before production use.

Plugins must not:

- execute arbitrary system commands through the host;
- read or write arbitrary filesystem paths;
- mutate PostgreSQL directly;
- bypass media library permissions;
- register menu entries outside `/admin/plugins/{plugin_id}`;
- register schedules outside their plugin key prefix.

## Next Implementation Steps

1. Add more first-party plugin SDK templates for WASI and metadata/marker imports.
2. Add WASIp2/component support and native Host API imports.
3. Add stronger stale temp-dir scavenging for interrupted WASI runs.
4. Expand hook coverage as new Emby-compatible domains land.
