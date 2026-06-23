# Emby Compatible Media Server Implementation Plan

> **For Claude:** Use `${SUPERPOWERS_SKILLS_ROOT}/skills/collaboration/executing-plans/SKILL.md` to implement this plan task-by-task.

**Goal:** Build `fbz-api` into a high-performance, low-resource, highly extensible media server backend that can be used by third-party Emby-compatible clients by entering this server's address.

**Architecture:** Use an Emby compatibility adapter at the HTTP boundary and keep internal domain models independent. Start as a modular Rust monolith, with clean module boundaries so scanner, metadata, queue, transcoding, plugin, and scheduling workers can later split into separate services.

**Tech Stack:** Rust, axum, tokio, tower-http, PostgreSQL, Redis Streams, FFmpeg, ffprobe, TMDB API, SQL migrations, structured tracing, OpenAPI/compatibility tests.

---

## Scope Notes

This is a planning draft. It intentionally does not lock all architecture decisions yet.

The server should eventually behave like an Emby-compatible backend for supported clients while keeping FBZ's own internal model clean. External API compatibility should not force database table names, Rust structs, or internal service boundaries to copy Emby one-to-one.

## Confirmed Decisions

- First target is backend protocol compatibility. Web UI is deferred until the backend architecture is stable.
- Third-party Emby-compatible clients should be able to enter this project's server URL, connect, browse, and play supported media.
- Compatibility target: Emby REST API only, not Jellyfin compatibility in the first architecture pass.
- JSON and XML request/response support are required in v1.
- Deployment targets: Windows, Linux, Docker, and NAS environments. Docker is production-only, not required for local development.
- PostgreSQL is a required primary database.
- Redis is a required cache, queue, and coordination dependency.
- Public server address should be configurable by an administrator from the backend/admin settings later; environment config is only the bootstrap default.
- FFmpeg and ffprobe must be bundled with the server distribution and can be overridden by external paths.
- Hardware transcoding is allowed. Default behavior should prefer hardware acceleration and fall back to software transcoding when hardware is unavailable.
- STRM playback initially supports intranet links only, with configurable safe external domains.
- Maximum concurrent transcoding sessions: 3 initially; extra transcode requests must queue.
- Expected scale: about 5 PB media storage, hundreds of thousands of movies, hundreds of thousands of TV items, up to 1000 concurrent users, and large scan/index workloads.
- Plugin system is required. Notifications should be implemented as plugins, including Telegram, WeCom/Enterprise WeChat, and generic webhook plugins.
- Plugins may contribute left-side admin menu entries, scheduled tasks, event hooks, and notification handlers, but must operate inside constrained boundaries.

## Recommended Defaults

These defaults are selected to make the first implementation directly usable by Emby-compatible clients while avoiding premature complexity. They can be revised later by administrator settings or follow-up architecture decisions.

- Emby endpoint exposure:
  - Canonical route prefix: `/emby/*`.
  - Compatibility aliases: support direct `/*` aliases for implemented Emby endpoints when a client calls them without `/emby`.
  - Route aliases must share the same handler and auth policy to avoid behavior drift.
- First connect/browse/play endpoint subset:
  - System discovery: `GET /emby/System/Info`, `GET /emby/System/Info/Public`.
  - User discovery/auth: `GET /emby/Users/Public`, `POST /emby/Users/AuthenticateByName`, `POST /emby/Sessions/Logout`.
  - Library roots: `GET /emby/Users/{UserId}/Views`.
  - Browse/search: `GET /emby/Users/{UserId}/Items`, `GET /emby/Items`.
  - Item detail: `GET /emby/Users/{UserId}/Items/{Id}`.
  - Images: `GET /emby/Items/{Id}/Images/{ImageType}`.
  - Playback info: `POST /emby/Items/{Id}/PlaybackInfo`.
  - Stream delivery: `GET /emby/Videos/{Id}/stream`, `GET /emby/Videos/{Id}/master.m3u8` where applicable.
  - Playback progress: `POST /emby/Sessions/Playing`, `POST /emby/Sessions/Playing/Progress`, `POST /emby/Sessions/Playing/Stopped`.
- Local development deployment:
  - Run Rust API, PostgreSQL, Redis, and local media paths directly on the developer machine.
  - Docker is for production packaging and production-like smoke tests only.
- Process model:
  - Start as a modular single service with embedded API and worker loops for local development.
  - Add `FBZ_NODE_ROLE=all|api|worker|scheduler` so production can split API, worker, and scheduler containers without rewriting modules.
  - Redis Streams and PostgreSQL leases should be used from the beginning so worker split remains a deployment choice.
- Media path model:
  - Windows examples: `D:/Media/Movies`, `D:/Media/TV`, `D:/Media/Music`.
  - Linux/NAS examples: `/mnt/media/movies`, `/mnt/media/tv`, `/mnt/media/music`.
  - Docker production examples: host paths mounted into `/media/movies`, `/media/tv`, `/media/music`.
  - SMB/NFS should be mounted by the OS/NAS first; scanner reads mounted paths instead of implementing network filesystem clients.
- Artwork and transcode cache:
  - Filesystem first for local, NAS, and simple Docker production.
  - Keep a storage adapter boundary so object storage can be added later for multi-node deployments.
- Scan metadata retention:
  - Keep latest normalized scan/probe state on hot media tables.
  - Retain scan/job/probe event history in partitioned audit tables with configurable retention.
  - Do not keep full raw provider payloads forever unless explicitly configured for debugging.
- Metadata providers:
  - TMDB enabled by default when `TMDB_ACCESS_TOKEN` is configured.
  - TVDB disabled until `TVDB_API_KEY` is configured.
  - Fanart disabled until `FANART_API_KEY` is configured.
  - Provider base URLs and image base URLs are administrator-editable and use official URLs as bootstrap defaults.
- Proxy policy:
  - Global proxy from `HTTP_PROXY` / `HTTPS_PROXY` is the bootstrap default.
  - Runtime settings should allow per-provider proxy override later.
  - `NO_PROXY` always applies to localhost, intranet service discovery, and configured media hosts.
- Hardware acceleration priority:
  - Prefer low-power integrated hardware first: Intel QSV on Windows, Intel QSV/VAAPI on Linux/NAS.
  - Prefer NVIDIA NVENC/NVDEC next when available.
  - Prefer AMD AMF on Windows or VAAPI on Linux after Intel/NVIDIA.
  - Fall back to software transcoding when hardware initialization fails and fallback is enabled.
- STRM policy:
  - Intranet IP ranges are allowed by default.
  - External STRM domains default to empty allowlist.
  - Admin-added safe domains must be exact hostnames or explicit suffix rules; wildcard `*` is not allowed.
- Default role presets:
  - Owner: full server, system, library, plugin, device, user, and scheduled task management.
  - Admin: server management except ownership transfer and destructive system reset.
  - Library Manager: manage assigned libraries, paths, scans, metadata, and artwork.
  - User: play assigned libraries, remember playback, login existing devices.
  - Restricted User: play assigned libraries only, no download, no transcode unless explicitly enabled, no new device login by default.
- New device login policy:
  - Owner/Admin accounts can add devices by default.
  - Normal users require per-user permission for new device login.
  - Restricted users cannot add new devices unless explicitly allowed.
- Music metadata:
  - Read embedded tags first.
  - Use MusicBrainz-compatible identifiers later if added as a provider/plugin.
  - Use Fanart for artist/album artwork when configured.
  - Keep album artist, track artist, composer, disc number, track number, duration, codec, bitrate, and lyrics fields.
- Scheduled task defaults:
  - Incremental library scan: every 15 minutes.
  - Full library scan: daily during a configurable low-traffic window.
  - Metadata refresh: daily for recent items, weekly for older items.
  - Artwork cache cleanup: daily.
  - Transcode cache cleanup: hourly.
  - Session cleanup: every 10 minutes.
  - Plugin scheduled tasks: disabled until plugin is enabled and approved.

## Non-Goals for the First Implementation Pass

- Do not fully implement every Emby endpoint in one pass.
- Do not build a microservice deployment from day one.
- Do not build the FBZ Web UI before the backend compatibility architecture is ready.
- Do not store media files, posters, or transcoding segments in PostgreSQL.
- Do not implement aggressive transcoding features before DirectPlay, DirectStream, and STRM behavior are defined.

## Key External Contracts

- Emby-compatible REST prefix: `/emby/{apipath}`.
- Authentication compatibility:
  - `Authorization: Emby ...`
  - `X-Emby-Token`
  - `api_key` query parameter.
- Response compatibility:
  - JSON support.
  - XML support in v1 through Emby-compatible content negotiation and request parsing.
- Health and native API:
  - Keep `/health`.
  - Keep future FBZ-native API under `/api/*` if needed.

## Proposed Module Layout

```text
fbz-api/src/
  main.rs
  app.rs
  config.rs
  state.rs
  settings/
  compat/
    emby/
      mod.rs
      auth.rs
      format.rs
      dto/
      routes/
  auth/
  library/
  scanner/
  metadata/
  media/
  playback/
  jobs/
  plugins/
  notifications/
  music/
  scheduler/
  storage/
  db/
```

## Environment Variables Draft

```env
FBZ_API_HOST=127.0.0.1
FBZ_API_PORT=8080
PUBLIC_BASE_URL=http://127.0.0.1:8080
PUBLIC_BASE_URL_ADMIN_EDITABLE=true

DATABASE_URL=postgres://fbz:fbz@127.0.0.1:5432/fbz
REDIS_URL=redis://127.0.0.1:6379
FBZ_NODE_ROLE=all

METADATA_PROVIDERS=tmdb,tvdb,fanart
TMDB_ACCESS_TOKEN=
TMDB_API_BASE_URL=https://api.themoviedb.org/3
TMDB_IMAGE_BASE_URL=https://image.tmdb.org/t/p
TVDB_API_KEY=
TVDB_API_BASE_URL=https://api4.thetvdb.com/v4
FANART_API_KEY=
FANART_API_BASE_URL=https://webservice.fanart.tv/v3

HTTP_PROXY=
HTTPS_PROXY=
NO_PROXY=127.0.0.1,localhost
PROXY_POLICY=global-with-provider-override

FFMPEG_PATH=ffmpeg
FFPROBE_PATH=ffprobe
FBZ_BUNDLED_FFMPEG_DIR=./vendor/ffmpeg
FBZ_ENABLE_BUNDLED_FFMPEG=true
TRANSCODE_CACHE_DIR=./var/transcode
ARTWORK_CACHE_DIR=./var/artwork
STORAGE_BACKEND=filesystem
SCAN_EVENT_RETENTION_DAYS=90
TRANSCODE_MAX_CONCURRENT=3
TRANSCODE_HARDWARE_MODE=auto
TRANSCODE_HARDWARE_PRIORITY=intel,nvidia,amd
TRANSCODE_SOFTWARE_FALLBACK=true
MEDIA_ROOTS=D:/Media/Movies,D:/Media/TV,D:/Media/Music
STRM_ALLOW_PRIVATE_NETWORKS=true
STRM_ALLOWED_DOMAINS=

JWT_SECRET=
WEBHOOK_SECRET=

PLUGIN_DIR=./plugins
PLUGIN_PACKAGE_DIR=./var/plugin-packages
PLUGIN_DATA_DIR=./var/plugin-data
PLUGIN_CACHE_DIR=./var/plugin-cache
PLUGIN_TMP_DIR=./var/plugin-tmp
PLUGIN_RUNTIME_DEFAULT=wasi
PLUGIN_REQUIRE_APPROVAL=true
PLUGIN_REQUIRE_REAPPROVAL_ON_PERMISSION_CHANGE=true
PLUGIN_ALLOW_UNSIGNED=false
PLUGIN_TIMEOUT_MS=5000
PLUGIN_MAX_CONCURRENCY=4
PLUGIN_MEMORY_LIMIT_MB=128
PLUGIN_SECRET_KEY=
SCHEDULE_INCREMENTAL_SCAN=15m
SCHEDULE_FULL_SCAN=0 4 * * *
SCHEDULE_METADATA_REFRESH=0 5 * * *
SCHEDULE_TRANSCODE_CLEANUP=hourly
SCHEDULE_SESSION_CLEANUP=10m
```

## Runtime Dependency Policy

### FFmpeg and ffprobe

The distribution must include platform-specific FFmpeg and ffprobe binaries for Windows, Linux, Docker, and NAS builds. Runtime resolution order:

1. Use explicit `FFMPEG_PATH` and `FFPROBE_PATH` when configured.
2. Use bundled binaries under `FBZ_BUNDLED_FFMPEG_DIR` when enabled.
3. Fail startup with a clear diagnostic if neither external nor bundled binaries are executable.

Bundled builds must document their license mode and enabled codec flags. FFmpeg official documentation states that FFmpeg is generally LGPL, but optional GPL parts can make the whole build GPL. The build and packaging task must verify this before distribution.

### Plugin Runtime

Plugins are external extensions, not unrestricted in-process code by default. The first implementation should prefer a constrained plugin host model:

- Manifest-based plugin discovery.
- Explicit capability declarations.
- Local package installation from `PLUGIN_PACKAGE_DIR` and administrator upload later.
- Lifecycle states for discovered, installed, pending approval, enabled, disabled, failed, upgrading, rolled back, and uninstalled plugins.
- Permission approval and re-approval when a plugin upgrade requests new capabilities.
- Versioned hook contracts.
- Per-plugin config and data directory.
- Per-plugin secret storage for credentials, with secrets never returned to normal config reads.
- Per-plugin timeouts, concurrency limits, memory/process limits where the runtime supports them, and circuit breaker behavior after repeated failures.
- No direct database access unless a future trusted native plugin tier is explicitly introduced.
- Plugin data must go through a namespaced host storage API so uninstall, backup, and quota behavior stay controllable.
- No arbitrary admin UI injection; admin menu entries must point to plugin-owned pages registered through a menu contribution API.
- Plugin admin pages must be sandboxed and limited to a versioned host SDK/API surface.
- Notification integrations should be plugins instead of hard-coded core modules.

## Phase 0: Confirmed Baseline and Remaining Decisions

### Task 0.1: Confirm Emby Compatibility Target

**Files:**
- Modify: `C:/Code/fbz/docs/plans/2026-06-17-emby-compatible-media-server.md`

**Confirmed:**
- First target is backend compatibility for third-party Emby-compatible clients.
- FBZ Web UI is deferred until the backend architecture and Emby-compatible protocol surface are stable.
- Compatibility target is strict Emby API only.
- Jellyfin compatibility is out of scope for the first implementation pass.
- JSON and XML are both required in v1.
- Canonical route prefix is `/emby/*`.
- Direct `/*` aliases should be supported for implemented Emby endpoints to maximize client compatibility.
- First endpoint group is connect/browse/play.

**Acceptance Criteria:**
- Target clients are listed.
- First endpoint group is selected.
- Compatibility test strategy is clear.

### Task 0.2: Confirm Deployment Shape

**Files:**
- Modify: `C:/Code/fbz/docs/plans/2026-06-17-emby-compatible-media-server.md`

**Confirmed:**
- Deployment targets are Windows, Linux, Docker, and NAS.
- Docker is production-only.
- Hardware transcoding is allowed and must be configurable.
- Default transcoding behavior prefers hardware acceleration and falls back to software when unavailable.
- FFmpeg and ffprobe must be bundled and externally overrideable.
- Public server URL is bootstrap-configured by env but must later be editable by an administrator through server settings.
- Local development does not require Docker.
- Production Docker images should mount media under `/media/*`.
- SMB/NFS access should be handled by OS/NAS mounts before scanner sees paths.
- Process model starts as one modular service with `FBZ_NODE_ROLE=all`, but supports production split into API, worker, and scheduler roles.
- Filesystem storage is the first artwork/transcode cache backend, with a storage adapter boundary for later object storage.

**Acceptance Criteria:**
- Development and production environment assumptions are documented.
- FFmpeg path and hardware acceleration direction are defined.

### Task 0.3: Confirm Data Backends

**Files:**
- Modify: `C:/Code/fbz/docs/plans/2026-06-17-emby-compatible-media-server.md`

**Confirmed:**
- PostgreSQL is required as the primary database.
- Redis is required for cache, queues, and coordination.
- Expected scale is about 5 PB, hundreds of thousands of movies/TV items, and up to 1000 concurrent users.
- PostgreSQL full-text search is the first search backend.
- Redis is the first cache and queue backend.
- Artwork and transcode bytes stay on filesystem/object storage, not PostgreSQL.
- Keep latest normalized scan/probe state on hot tables.
- Retain scan/job/probe event history in partitioned audit tables with configurable retention, default 90 days.

**Acceptance Criteria:**
- Primary database and queue backend are selected.
- Initial search strategy is selected.

### Task 0.4: Confirm Minimum Emby Client Flow

**Files:**
- Modify: `C:/Code/fbz/docs/plans/2026-06-17-emby-compatible-media-server.md`

**Target flow:**
1. User enters FBZ server address in an Emby-compatible client.
2. Client discovers server/system info.
3. Client authenticates user.
4. Client lists libraries.
5. Client browses items.
6. Client opens item detail and images.
7. Client requests playback info.
8. Client plays via DirectPlay, STRM 302, DirectStream, or queued/transcoded playback.
9. Client reports playback progress.

**Recommended first endpoint subset:**
- `GET /emby/System/Info`
- `GET /emby/System/Info/Public`
- `GET /emby/Users/Public`
- `POST /emby/Users/AuthenticateByName`
- `POST /emby/Sessions/Logout`
- `GET /emby/Users/{UserId}/Views`
- `GET /emby/Users/{UserId}/Items`
- `GET /emby/Items`
- `GET /emby/Users/{UserId}/Items/{Id}`
- `GET /emby/Items/{Id}/Images/{ImageType}`
- `POST /emby/Items/{Id}/PlaybackInfo`
- `GET /emby/Videos/{Id}/stream`
- `GET /emby/Videos/{Id}/master.m3u8`
- `POST /emby/Sessions/Playing`
- `POST /emby/Sessions/Playing/Progress`
- `POST /emby/Sessions/Playing/Stopped`

**Acceptance Criteria:**
- First endpoint batch supports this flow before admin/web UI work starts.
- Compatibility tests simulate this flow without requiring FBZ Web.

## Phase 1: Foundation

### Task 1.1: Add Configuration Layer

**Files:**
- Modify: `C:/Code/fbz/fbz-api/Cargo.toml`
- Modify: `C:/Code/fbz/fbz-api/src/config.rs`
- Modify: `C:/Code/fbz/fbz-api/.env.example`
- Test: `C:/Code/fbz/fbz-api/src/config.rs`

**Steps:**
1. Add typed config for database, Redis, TMDB, proxy, FFmpeg, media roots, cache paths, and public base URL.
2. Validate required fields at startup.
3. Add unit tests for defaults and environment override behavior.
4. Run `cargo fmt`, `cargo test`, `cargo check`.

**Acceptance Criteria:**
- Invalid config fails early with actionable error.
- Defaults are suitable for local development.

### Task 1.2: Add Error Model and Response Envelope

**Files:**
- Create: `C:/Code/fbz/fbz-api/src/error.rs`
- Modify: `C:/Code/fbz/fbz-api/src/app.rs`
- Test: `C:/Code/fbz/fbz-api/src/error.rs`

**Steps:**
1. Define internal `AppError`.
2. Map errors to HTTP status codes.
3. Add Emby-compatible error response adapter later under `compat/emby`.
4. Test 401, 403, 404, 409, 422, and 500 mapping.

**Acceptance Criteria:**
- Internal services return domain errors, not raw HTTP responses.
- HTTP layer owns response formatting.

### Task 1.3: Add Observability Baseline

**Files:**
- Modify: `C:/Code/fbz/fbz-api/src/main.rs`
- Modify: `C:/Code/fbz/fbz-api/src/app.rs`
- Create: `C:/Code/fbz/fbz-api/src/telemetry.rs`

**Steps:**
1. Add request ID propagation.
2. Add structured logs for startup, shutdown, request, and job events.
3. Add `/health` and future `/ready` distinction.
4. Keep metrics endpoint optional until backend choice is confirmed.

**Acceptance Criteria:**
- Logs can correlate API requests with background jobs.
- Health checks stay lightweight.

### Task 1.4: Add Bundled FFmpeg Resolver

**Files:**
- Create: `C:/Code/fbz/fbz-api/src/media/tools.rs`
- Modify: `C:/Code/fbz/fbz-api/src/config.rs`
- Modify: `C:/Code/fbz/fbz-api/.env.example`
- Create: `C:/Code/fbz/fbz-api/vendor/ffmpeg/README.md`
- Test: `C:/Code/fbz/fbz-api/src/media/tools.rs`

**Steps:**
1. Resolve `FFMPEG_PATH` and `FFPROBE_PATH` first when explicitly configured.
2. Resolve bundled binaries from `FBZ_BUNDLED_FFMPEG_DIR` when external paths are not configured.
3. Validate executability and collect version output on startup.
4. Record the chosen binary source as `external` or `bundled` in diagnostics.
5. Document FFmpeg binary source, license mode, codec flags, and platform packaging rules.

**Acceptance Criteria:**
- External paths override bundled binaries.
- Bundled binaries work without system-level FFmpeg installation.
- Startup fails clearly if neither FFmpeg nor ffprobe is available.
- Packaging notes include FFmpeg LGPL/GPL license verification requirements.

### Task 1.5: Add Runtime Server Settings

**Files:**
- Create: `C:/Code/fbz/fbz-api/src/settings/mod.rs`
- Create: `C:/Code/fbz/fbz-api/src/settings/repository.rs`
- Create or modify: `C:/Code/fbz/fbz-api/migrations/0001_initial.sql`
- Modify: `C:/Code/fbz/fbz-api/src/config.rs`
- Test: `C:/Code/fbz/fbz-api/src/settings/mod.rs`

**Settings:**
- Public base URL.
- Metadata provider base URLs.
- Metadata provider enable/priority order.
- Proxy settings.
- STRM safe external domains.
- Hardware transcoding mode.
- New device login policy defaults.
- Default role presets.
- Scheduled task intervals.

**Steps:**
1. Keep environment variables as bootstrap defaults.
2. Persist administrator-edited settings in PostgreSQL.
3. Cache settings in memory with invalidation.
4. Define which settings require restart and which can update live.

**Acceptance Criteria:**
- Public server URL can be changed later by an administrator without editing `.env`.
- Provider mirror URLs can be changed by an administrator.
- Runtime settings have validation and audit history.

## Phase 2: Database and Storage Model

### Task 2.1: Add SQL Migration Tooling

**Files:**
- Modify: `C:/Code/fbz/fbz-api/Cargo.toml`
- Create: `C:/Code/fbz/fbz-api/migrations/0001_initial.sql`
- Create: `C:/Code/fbz/fbz-api/src/db/mod.rs`

**Steps:**
1. Choose `sqlx` or `tokio-postgres`.
2. Add connection pool.
3. Add migration command or startup migration strategy.
4. Add integration test strategy using a test database or container.

**Acceptance Criteria:**
- Database connection is pooled.
- Migrations are repeatable and versioned.

### Task 2.2: Create Core Tables

**Files:**
- Create: `C:/Code/fbz/fbz-api/migrations/0002_core_media.sql`

**Tables:**
- `users`
- `roles`
- `api_keys`
- `devices`
- `sessions`
- `libraries`
- `library_paths`
- `library_permissions`
- `media_items`
- `media_files`
- `media_streams`
- `media_markers`
- `media_external_ids`
- `people`
- `genres`
- `tags`
- `artwork`
- `collections`

**Acceptance Criteria:**
- Media identity is separated from file identity.
- External provider IDs are normalized.
- Library permissions are queryable without expensive joins.

### Task 2.3: Create Job, Event, and Playback Tables

**Files:**
- Create: `C:/Code/fbz/fbz-api/migrations/0003_jobs_playback.sql`

**Tables:**
- `jobs`
- `job_runs`
- `job_events`
- `event_outbox`
- `scheduled_tasks`
- `webhook_subscriptions`
- `notification_targets`
- `playback_sessions`
- `user_playstates`
- `transcoding_sessions`

**Acceptance Criteria:**
- Jobs are idempotent.
- Event outbox supports reliable webhook delivery.
- Playback state is user-specific.

### Task 2.4: Add Database Index Strategy

**Files:**
- Create: `C:/Code/fbz/fbz-api/migrations/0004_indexes.sql`

**Indexes:**
- `media_items(library_id, item_type, parent_id)`
- `media_items(created_at desc)`
- `media_external_ids(provider, external_id)`
- `media_files(path_hash)`
- `user_playstates(user_id, item_id)`
- `jobs(status, run_at)`
- `scheduled_tasks(enabled, next_run_at)`
- Full-text GIN index for title/original title/sort title.

**Acceptance Criteria:**
- Common browse, search, continue-watching, and latest-added queries have indexes.
- Large append-only tables are ready for time partitioning if needed.

### Task 2.5: Add Large Library Data Model Rules

**Files:**
- Modify: `C:/Code/fbz/fbz-api/migrations/0002_core_media.sql`
- Modify: `C:/Code/fbz/fbz-api/migrations/0004_indexes.sql`
- Create: `C:/Code/fbz/fbz-api/docs/database-scale.md`

**Scale assumptions:**
- About 5 PB total media storage.
- Hundreds of thousands of movies.
- Hundreds of thousands of TV items, plus seasons, episodes, subtitles, artwork, and metadata variants.

**Rules:**
1. Use compact internal numeric IDs for hot joins where possible.
2. Expose stable public IDs compatible with Emby response expectations.
3. Avoid offset pagination for large browse/search endpoints; use cursor/keyset pagination internally.
4. Keep mutable scan/probe/job logs out of hot media item tables.
5. Partition append-heavy tables such as `job_events`, scan events, playback events, and audit logs by time.
6. Keep path hash, normalized path, library ID, and file size indexed for scan deduplication.
7. Store large artwork/transcode/media bytes outside PostgreSQL.

**Acceptance Criteria:**
- Common UI queries remain index-driven at hundreds of thousands of items.
- Scans can detect unchanged files without re-probing every file.
- API responses can remain Emby-compatible without making Emby DTOs the database model.

## Phase 3: Emby Compatibility Layer

### Task 3.1: Add Emby Auth Parser

**Files:**
- Create: `C:/Code/fbz/fbz-api/src/compat/emby/auth.rs`
- Create: `C:/Code/fbz/fbz-api/src/compat/emby/mod.rs`
- Test: `C:/Code/fbz/fbz-api/src/compat/emby/auth.rs`

**Steps:**
1. Parse `Authorization: Emby UserId="...", Client="...", Device="...", DeviceId="...", Version="..."`.
2. Support `X-Emby-Token`.
3. Support `api_key` query parameter.
4. Normalize client/device context for session tracking.

**Acceptance Criteria:**
- Auth parsing is tested independently.
- Missing or invalid tokens return Emby-compatible 401 behavior.

### Task 3.2: Add Compatibility DTO Boundary

**Files:**
- Create: `C:/Code/fbz/fbz-api/src/compat/emby/dto/mod.rs`

**Steps:**
1. Define only DTOs needed by the first endpoint group.
2. Keep DTOs separate from domain structs.
3. Add conversion functions from internal domain models.

**Acceptance Criteria:**
- Internal modules do not depend on Emby DTOs.
- Compatibility changes are isolated.

### Task 3.3: Implement First Emby Endpoint Group

**Files:**
- Create: `C:/Code/fbz/fbz-api/src/compat/emby/routes/system.rs`
- Create: `C:/Code/fbz/fbz-api/src/compat/emby/routes/users.rs`
- Create: `C:/Code/fbz/fbz-api/src/compat/emby/routes/views.rs`
- Create: `C:/Code/fbz/fbz-api/src/compat/emby/routes/items.rs`
- Create: `C:/Code/fbz/fbz-api/src/compat/emby/routes/images.rs`
- Create: `C:/Code/fbz/fbz-api/src/compat/emby/routes/playback.rs`
- Create: `C:/Code/fbz/fbz-api/src/compat/emby/routes/sessions.rs`
- Modify: `C:/Code/fbz/fbz-api/src/app.rs`

**First endpoints:**
- `GET /emby/System/Info`
- `GET /emby/System/Info/Public`
- `GET /emby/Users/Public`
- `POST /emby/Users/AuthenticateByName`
- `POST /emby/Sessions/Logout`
- `GET /emby/Users/{UserId}/Views`
- `GET /emby/Users/{UserId}/Items`
- `GET /emby/Items`
- `GET /emby/Users/{UserId}/Items/{Id}`
- `GET /emby/Items/{Id}/Images/{ImageType}`
- `POST /emby/Items/{Id}/PlaybackInfo`
- `GET /emby/Videos/{Id}/stream`
- `GET /emby/Videos/{Id}/master.m3u8`
- `POST /emby/Sessions/Playing`
- `POST /emby/Sessions/Playing/Progress`
- `POST /emby/Sessions/Playing/Stopped`

**Alias policy:**
- Implement `/emby/*` as canonical routes.
- Add direct `/*` compatibility aliases for the same implemented endpoints.
- Aliases must call the same handlers and share auth, rate limit, logging, and compatibility behavior.

**Acceptance Criteria:**
- A basic Emby-compatible connect/browse/play flow can be tested.
- `AccessToken` is returned and accepted by protected endpoints.
- Implemented endpoints work through `/emby/*` and direct alias paths where aliases are enabled.

### Task 3.4: Add JSON and XML Content Negotiation

**Files:**
- Create: `C:/Code/fbz/fbz-api/src/compat/emby/format.rs`
- Modify: `C:/Code/fbz/fbz-api/src/compat/emby/dto/mod.rs`
- Modify: `C:/Code/fbz/fbz-api/src/app.rs`
- Test: `C:/Code/fbz/fbz-api/tests/compat_emby.rs`

**Steps:**
1. Accept JSON and XML request bodies where Emby-compatible clients send XML.
2. Return JSON or XML according to Emby-compatible `Content-Type`, `Accept`, and route behavior.
3. Keep internal domain structs independent from serialization format.
4. Add golden response fixtures for both JSON and XML for the first endpoint group.

**Acceptance Criteria:**
- v1 supports both JSON and XML for implemented Emby-compatible endpoints.
- XML compatibility is tested before claiming an endpoint is supported.
- Format handling is isolated in the compatibility layer.

### Task 3.5: Add Third-Party Client Compatibility Flow Test

**Files:**
- Create: `C:/Code/fbz/fbz-api/tests/emby_client_flow.rs`
- Modify: `C:/Code/fbz/fbz-api/docs/emby-compatibility.md`

**Flow:**
1. Discover server/system info.
2. Authenticate user.
3. List libraries.
4. Browse items.
5. Load item detail and images.
6. Request playback info.
7. Start playback.
8. Report playback progress.

**Acceptance Criteria:**
- The test models an external Emby-compatible client using only Emby-compatible endpoints.
- FBZ Web is not required for backend compatibility validation.

## Phase 4: Library and Scanner

### Task 4.1: Create Library Management Domain

**Files:**
- Create: `C:/Code/fbz/fbz-api/src/library/mod.rs`
- Create: `C:/Code/fbz/fbz-api/src/library/service.rs`
- Create: `C:/Code/fbz/fbz-api/src/library/repository.rs`

**Steps:**
1. Add create/update/delete library operations.
2. Add library paths.
3. Add user/library permission attachment.

**Acceptance Criteria:**
- A library can contain multiple root paths.
- Permissions can be evaluated before browse/search.

### Task 4.2: Add Scanner Job Model

**Files:**
- Create: `C:/Code/fbz/fbz-api/src/scanner/mod.rs`
- Create: `C:/Code/fbz/fbz-api/src/scanner/planner.rs`
- Create: `C:/Code/fbz/fbz-api/src/scanner/worker.rs`

**Steps:**
1. Plan full scan and incremental scan separately.
2. Detect files by extension and path.
3. Compute stable path hash and optional content fingerprint.
4. Emit metadata probe jobs.

**Acceptance Criteria:**
- Scanner can resume after interruption.
- Repeated scans do not duplicate media items.

### Task 4.3: Add Batch Import and Webhook Trigger

**Files:**
- Create: `C:/Code/fbz/fbz-api/src/scanner/webhook.rs`
- Modify: `C:/Code/fbz/fbz-api/src/compat/emby/routes/library.rs`

**Candidate endpoints:**
- `POST /emby/Library/Refresh`
- `POST /emby/Library/Media/Updated`

**Acceptance Criteria:**
- External systems can trigger a library refresh.
- Trigger requests enqueue jobs instead of doing scan work inline.

## Phase 5: Metadata Providers

### Task 5.1: Add Metadata Provider Abstraction

**Files:**
- Create: `C:/Code/fbz/fbz-api/src/metadata/provider.rs`
- Create: `C:/Code/fbz/fbz-api/src/metadata/provider_registry.rs`
- Modify: `C:/Code/fbz/fbz-api/src/config.rs`
- Modify: `C:/Code/fbz/fbz-api/src/settings/mod.rs`

**Provider capabilities:**
- Movie metadata.
- TV series metadata.
- Season metadata.
- Episode metadata.
- Person metadata.
- Artwork metadata.
- External ID mapping.

**Steps:**
1. Define provider trait/contracts independent of TMDB-specific DTOs.
2. Support provider enable/disable and priority order.
3. Load provider API base URLs from runtime settings with env defaults.
4. Route all metadata lookups through the provider registry.

**Acceptance Criteria:**
- TMDB, TVDB, and Fanart can be added without changing scanner logic.
- Admin-modified base URLs and proxy settings are used by provider clients.

### Task 5.2: Add TMDB Provider

**Files:**
- Create: `C:/Code/fbz/fbz-api/src/metadata/tmdb.rs`
- Modify: `C:/Code/fbz/fbz-api/src/config.rs`

**Steps:**
1. Support bearer token auth.
2. Support `TMDB_API_BASE_URL` from runtime settings with env bootstrap default.
3. Support `HTTP_PROXY` / `HTTPS_PROXY`.
4. Support timeout, retry, and rate limit backoff.
5. Fetch configuration for image base URLs.

**Acceptance Criteria:**
- TMDB base URL can be replaced by mirror URL.
- Image base URL can be replaced independently.

### Task 5.3: Add TVDB Provider

**Files:**
- Create: `C:/Code/fbz/fbz-api/src/metadata/tvdb.rs`
- Modify: `C:/Code/fbz/fbz-api/src/settings/mod.rs`

**Steps:**
1. Support TVDB API key authentication.
2. Support `TVDB_API_BASE_URL` from runtime settings with env bootstrap default.
3. Store provider-specific rate limit and licensing notes.
4. Normalize TVDB series, season, episode, artwork, and external IDs into internal provider result models.

**Acceptance Criteria:**
- TVDB can be enabled, disabled, prioritized, and mirrored independently of TMDB.
- Provider-specific licensing/API-key requirements are documented before production use.

### Task 5.4: Add Fanart Provider

**Files:**
- Create: `C:/Code/fbz/fbz-api/src/metadata/fanart.rs`
- Modify: `C:/Code/fbz/fbz-api/src/settings/mod.rs`

**Steps:**
1. Support Fanart API key authentication.
2. Support `FANART_API_BASE_URL` from runtime settings with env bootstrap default.
3. Normalize movie, TV, and music artwork into internal artwork models.
4. De-duplicate artwork across providers.

**Acceptance Criteria:**
- Fanart can be used as an artwork provider without becoming a primary metadata source.
- Artwork provider priority can be configured.

### Task 5.5: Add Matching Pipeline

**Files:**
- Create: `C:/Code/fbz/fbz-api/src/metadata/matcher.rs`
- Create: `C:/Code/fbz/fbz-api/src/metadata/service.rs`

**Steps:**
1. Parse title/year/season/episode from path.
2. Search enabled metadata providers by configured priority.
3. Store candidates and selected match.
4. Allow manual override later.

**Acceptance Criteria:**
- Automated match is explainable.
- Incorrect matches can be corrected without re-importing files.

### Task 5.6: Add Artwork Cache

**Files:**
- Create: `C:/Code/fbz/fbz-api/src/metadata/artwork.rs`

**Steps:**
1. Store remote artwork metadata in database.
2. Cache downloaded images on filesystem or object storage.
3. Return Emby-compatible image endpoints.

**Acceptance Criteria:**
- Remote provider outages do not break already cached artwork.
- Image URLs respect configured public base URL.

## Phase 6: Media Probe, Playback, and Transcoding

### Task 6.1: Add ffprobe Integration

**Files:**
- Create: `C:/Code/fbz/fbz-api/src/media/probe.rs`
- Create: `C:/Code/fbz/fbz-api/src/media/streams.rs`

**Steps:**
1. Call configured `FFPROBE_PATH`.
2. Parse JSON output.
3. Store video/audio/subtitle stream metadata.
4. Mark probe failures as retryable or terminal.

**Acceptance Criteria:**
- Media stream metadata is normalized.
- Probe does not block API request threads.

### Task 6.2: Add Playback Decision Engine

**Files:**
- Create: `C:/Code/fbz/fbz-api/src/playback/decision.rs`

**Rules:**
- DirectPlay when client supports container/codecs and file is reachable.
- DirectStream when remux is enough.
- Transcode when codec, bitrate, subtitle burn-in, or client profile requires it.
- STRM may return 302 only after permission, URL safety, and allowlist checks.

**Acceptance Criteria:**
- Decision result explains why DirectPlay/DirectStream/Transcode was selected.
- Client profile capability can be injected from Emby request parameters.

### Task 6.3: Add STRM Handling

**Files:**
- Create: `C:/Code/fbz/fbz-api/src/media/strm.rs`
- Create: `C:/Code/fbz/fbz-api/src/playback/redirect.rs`

**Steps:**
1. Parse STRM file content.
2. Allow intranet URLs by default using a strict private-network classifier.
3. Allow external domains only when configured in `STRM_ALLOWED_DOMAINS`.
4. Block loopback, link-local metadata endpoints, multicast, unspecified addresses, and unsafe schemes unless explicitly introduced by a later trusted policy.
5. Re-resolve domains at playback time and reject DNS results outside the allowed address policy.
6. Generate signed short-lived playback redirect.

**Acceptance Criteria:**
- STRM playback cannot be used for SSRF.
- Intranet STRM URLs work without external domain configuration.
- External STRM URLs work only when their domain is configured as safe.
- 302 links expire and are user/session scoped.

### Task 6.4: Add FFmpeg Transcoding Jobs

**Files:**
- Create: `C:/Code/fbz/fbz-api/src/playback/transcode.rs`
- Create: `C:/Code/fbz/fbz-api/src/playback/hls.rs`

**Steps:**
1. Generate FFmpeg command from playback decision.
2. Write HLS segments to transcode cache.
3. Track transcoding sessions.
4. Enforce `TRANSCODE_MAX_CONCURRENT=3` as the initial global concurrent transcode limit.
5. Queue extra transcode requests instead of rejecting them when the queue is healthy.
6. Clean stale sessions and cache files.

**Acceptance Criteria:**
- DirectPlay path works before transcoding is expanded.
- Transcoding can be cancelled when playback stops.
- At most 3 transcode workers run concurrently by default.
- Waiting transcode requests have visible queue status.

### Task 6.5: Add Hardware Transcoding Profiles

**Files:**
- Create: `C:/Code/fbz/fbz-api/src/playback/hardware.rs`
- Modify: `C:/Code/fbz/fbz-api/src/playback/transcode.rs`
- Modify: `C:/Code/fbz/fbz-api/.env.example`

**Backends to model:**
- Intel QSV.
- NVIDIA NVENC/NVDEC.
- AMD VAAPI/AMF.
- Linux VAAPI.
- Platform-specific future profiles.

**Default priority:**
- Windows: Intel QSV, NVIDIA NVENC/NVDEC, AMD AMF, software.
- Linux/NAS: Intel QSV/VAAPI, NVIDIA NVENC/NVDEC, AMD VAAPI, software.
- Docker production: use host-provided devices and the Linux/NAS priority order after validating mounted GPU devices.

**Steps:**
1. Default to `TRANSCODE_HARDWARE_MODE=auto`.
2. Detect configured or auto-selected hardware acceleration mode.
3. Validate FFmpeg supports the selected backend at startup or first transcode.
4. Prefer hardware transcoding when available.
5. Fall back to software transcoding when hardware is unavailable and `TRANSCODE_SOFTWARE_FALLBACK=true`.
6. Record selected acceleration mode and fallback reason in transcoding session diagnostics.
7. Expose selected backend, fallback state, and failure reason in admin diagnostics.

**Acceptance Criteria:**
- Hardware acceleration is the default preferred mode.
- Bad hardware configuration fails with actionable diagnostics.
- Hardware-unavailable fallback to software is explicit and logged.
- The transcode decision engine can decide between direct, remux, software transcode, and hardware transcode.

### Task 6.6: Add Intro/Outro Marker Import

**Files:**
- Create: `C:/Code/fbz/fbz-api/src/media/markers.rs`
- Create: `C:/Code/fbz/fbz-api/src/media/marker_import.rs`
- Modify: `C:/Code/fbz/fbz-api/migrations/0002_core_media.sql`
- Modify: `C:/Code/fbz/fbz-api/src/plugins/hooks.rs`

**Marker types:**
- Intro start/end.
- Outro start/end.
- Credits.
- Recap.
- Custom named segment.

**Import sources:**
- Core import adapter for local JSON/CSV/NFO-like marker files.
- Plugin adapter for external sources such as TiDb or other intro/outro marker databases.
- Future detection-generated markers from media analysis jobs.

**Steps:**
1. Store marker time ranges per media item and media file.
2. Keep source, confidence, version, and import timestamp.
3. Allow multiple marker sources without overwriting trusted manual/admin corrections.
4. Expose markers through playback decisions and future skip-intro/skip-outro APIs.
5. Allow plugins to contribute marker import jobs through a controlled hook.

**Acceptance Criteria:**
- Intro/outro data can be imported without changing core scanner logic.
- Plugin-provided marker data is validated before writing to the database.
- Marker source conflicts are auditable.

## Phase 7: Queue, Event Outbox, and Scheduled Tasks

### Task 7.1: Add Redis Streams Queue

**Files:**
- Create: `C:/Code/fbz/fbz-api/src/jobs/mod.rs`
- Create: `C:/Code/fbz/fbz-api/src/jobs/queue.rs`
- Create: `C:/Code/fbz/fbz-api/src/jobs/worker.rs`

**Job types:**
- `scan.library`
- `scan.path`
- `probe.media`
- `metadata.match`
- `metadata.refresh`
- `markers.import`
- `transcode.start`
- `transcode.cleanup`
- `plugin.hook`
- `plugin.notify`
- `webhook.deliver`

**Acceptance Criteria:**
- Jobs have IDs, retry count, dedupe key, status, and error message.
- Failed jobs can be retried safely.
- Transcode jobs respect the global concurrency limit and queue fairly.

### Task 7.2: Add Event Outbox

**Files:**
- Create: `C:/Code/fbz/fbz-api/src/jobs/outbox.rs`
- Create: `C:/Code/fbz/fbz-api/src/jobs/events.rs`

**Steps:**
1. Persist domain events in PostgreSQL.
2. Queue delivery through Redis Streams.
3. Dispatch plugin hooks and notification requests through queue jobs.
4. Retry with backoff.

**Acceptance Criteria:**
- API transactions and event delivery are decoupled.
- Delivery can recover after worker restart.
- Notification channels are implemented by plugins, not hard-coded core modules.

### Task 7.3: Add Scheduler

**Files:**
- Create: `C:/Code/fbz/fbz-api/src/scheduler/mod.rs`
- Create: `C:/Code/fbz/fbz-api/src/scheduler/store.rs`

**Scheduled tasks:**
- Incremental library scan: every 15 minutes.
- Full library scan: daily at 04:00 local server time by default.
- Recent metadata refresh: daily at 05:00 local server time by default.
- Older metadata refresh: weekly.
- Artwork cache cleanup: daily.
- Transcode cache cleanup: hourly.
- Session cleanup: every 10 minutes.
- Job/outbox retry sweep: every 5 minutes.
- Database maintenance hooks: disabled by default until administrator configures them.
- Plugin scheduled tasks: disabled until each plugin is enabled and approved.

**Acceptance Criteria:**
- Tasks have `enabled`, `cron` or interval, `next_run_at`, `last_run_at`.
- Distributed lock strategy is defined before multi-node workers.
- Defaults are safe for a large library and do not trigger full scans too frequently.

## Phase 8: Plugin System

### Task 8.1: Define Plugin Manifest and Registry

**Files:**
- Create: `C:/Code/fbz/fbz-api/src/plugins/mod.rs`
- Create: `C:/Code/fbz/fbz-api/src/plugins/manifest.rs`
- Create: `C:/Code/fbz/fbz-api/src/plugins/registry.rs`
- Create: `C:/Code/fbz/fbz-api/migrations/0005_plugins.sql`
- Create: `C:/Code/fbz/fbz-api/docs/plugins.md`
- Test: `C:/Code/fbz/fbz-api/src/plugins/manifest.rs`

**Manifest fields:**
- Plugin ID.
- Name.
- Version.
- Author.
- Runtime type.
- Minimum FBZ API version.
- Permissions/capabilities.
- Hook subscriptions.
- Scheduled tasks.
- Admin menu contributions.
- Config schema.

**Tables:**
- `plugins`
- `plugin_packages`
- `plugin_installations`
- `plugin_versions`
- `plugin_configs`
- `plugin_capabilities`
- `plugin_approved_capabilities`
- `plugin_hook_subscriptions`
- `plugin_scheduled_tasks`
- `plugin_admin_menu_items`
- `plugin_data_namespaces`
- `plugin_lifecycle_events`
- `plugin_resource_limits`
- `plugin_host_api_grants`
- `plugin_secrets`
- `plugin_migrations`
- `plugin_test_runs`

**Steps:**
1. Define a strict plugin manifest format.
2. Load plugins from `PLUGIN_DIR`.
3. Reject invalid, duplicate, disabled, or unsigned plugins according to config.
4. Persist installed plugin metadata and enabled state.

**Acceptance Criteria:**
- Plugin discovery is deterministic.
- Plugin capabilities are visible before activation.
- A plugin cannot silently gain new permissions without re-approval.

### Task 8.2: Define Plugin Runtime Boundary

**Files:**
- Create: `C:/Code/fbz/fbz-api/src/plugins/runtime.rs`
- Create: `C:/Code/fbz/fbz-api/src/plugins/sandbox.rs`
- Modify: `C:/Code/fbz/fbz-api/docs/plugins.md`

**Initial policy:**
- Do not allow unrestricted native dynamic libraries by default.
- Prefer a constrained runtime such as WASM/WASI or an external sidecar process with a versioned RPC contract.
- Keep direct database access out of the initial plugin API.
- Expose narrow host APIs for notifications, settings, event reads, scheduled task registration, and admin menu contribution.

**Acceptance Criteria:**
- Plugin execution is isolated from core process memory as much as practical.
- Plugin failures cannot crash the main API process.
- Host APIs are capability-gated.

### Task 8.3: Add Plugin Hook Contracts

**Files:**
- Create: `C:/Code/fbz/fbz-api/src/plugins/hooks.rs`
- Create: `C:/Code/fbz/fbz-api/src/plugins/events.rs`

**Hook categories:**
- Server lifecycle: started, stopping.
- Library lifecycle: library created, scan started, scan completed.
- Media lifecycle: item added, item updated, metadata matched, playback started, playback stopped.
- User lifecycle: login, logout.
- Notification lifecycle: notification requested, delivery result.
- Scheduler lifecycle: plugin scheduled task due.

**Acceptance Criteria:**
- Hooks receive stable event payloads.
- Hooks are async and time-limited.
- Hook failures are logged and isolated.

### Task 8.4: Add Plugin Scheduled Tasks

**Files:**
- Modify: `C:/Code/fbz/fbz-api/src/scheduler/mod.rs`
- Create: `C:/Code/fbz/fbz-api/src/plugins/schedule.rs`

**Steps:**
1. Allow plugins to register scheduled tasks through manifest or runtime registration.
2. Store plugin task definitions in `scheduled_tasks`.
3. Enforce per-plugin timeout, retry, and concurrency limits.
4. Disable runaway plugin schedules automatically after repeated failures.

**Acceptance Criteria:**
- Plugin schedules use the same distributed lock model as core schedules.
- Plugin tasks cannot bypass global queue and concurrency rules.

### Task 8.5: Add Plugin Admin Menu Contributions

**Files:**
- Create: `C:/Code/fbz/fbz-api/src/plugins/admin_menu.rs`
- Modify: `C:/Code/fbz/fbz-api/docs/plugins.md`

**Rules:**
- Plugins may contribute left-side admin menu entries.
- Menu entries must declare ID, title, icon key, route, required permission, and plugin owner.
- Routes must be namespaced under the plugin ID.
- Plugin pages may only access host APIs granted by capabilities.
- Core admin pages must never be replaced by plugins.

**Acceptance Criteria:**
- Admin menu contributions are explicit and revocable.
- Menu entries are permission-filtered per user.
- Plugin UI cannot inject arbitrary script into core admin pages.

### Task 8.6: Move Notifications to Plugins

**Files:**
- Create: `C:/Code/fbz/fbz-api/src/plugins/builtin_notifications.rs`
- Create: `C:/Code/fbz/fbz-api/plugins/tg-notify/manifest.json`
- Create: `C:/Code/fbz/fbz-api/plugins/wecom-notify/manifest.json`
- Create: `C:/Code/fbz/fbz-api/plugins/webhook-notify/manifest.json`
- Modify: `C:/Code/fbz/fbz-api/src/notifications/mod.rs`

**Built-in notification plugins:**
- Telegram notification plugin.
- Enterprise WeChat notification plugin.
- Generic webhook notification plugin.

**Acceptance Criteria:**
- Notification delivery is invoked through the plugin host.
- Notification plugins use event outbox and queue retry behavior.
- Core only defines notification contracts and delivery requests.

### Task 8.7: Add Plugin Package and Lifecycle Management

**Files:**
- Create: `C:/Code/fbz/fbz-api/src/plugins/package.rs`
- Create: `C:/Code/fbz/fbz-api/src/plugins/lifecycle.rs`
- Create: `C:/Code/fbz/fbz-api/src/plugins/installer.rs`
- Modify: `C:/Code/fbz/fbz-api/migrations/0005_plugins.sql`
- Modify: `C:/Code/fbz/fbz-api/docs/plugins.md`
- Test: `C:/Code/fbz/fbz-api/src/plugins/lifecycle.rs`

**Package policy:**
- Use a deterministic local plugin package format first, such as `.fbz-plugin` or `.zip` containing `manifest.json`, runtime files, static UI assets, and optional migration files.
- Support development plugins loaded from `PLUGIN_DIR`.
- Support installed packages stored under `PLUGIN_PACKAGE_DIR`.
- Remote marketplace is a non-goal for v1; leave only a package repository boundary for later.
- Unsigned plugins are rejected by default when `PLUGIN_ALLOW_UNSIGNED=false`.

**Lifecycle states:**
- `discovered`
- `installed`
- `pending_approval`
- `enabled`
- `disabled`
- `failed`
- `upgrading`
- `rolled_back`
- `uninstalled`

**Steps:**
1. Validate package checksum, manifest, runtime type, minimum FBZ API version, and declared capabilities before install.
2. Install packages into an immutable versioned directory.
3. Enable and disable plugins without deleting package files or plugin data.
4. Upgrade by installing a new version beside the old version, migrating config/data only after validation succeeds.
5. Roll back to the previous version when startup, migration, or health check fails.
6. Uninstall by disabling the plugin, revoking hooks/menu entries/schedules, and then choosing whether to keep or delete plugin data.
7. Record every lifecycle transition in `plugin_lifecycle_events`.

**Acceptance Criteria:**
- A broken plugin package cannot partially overwrite an existing working version.
- Disable, rollback, and uninstall revoke hooks, schedules, host API grants, and menu contributions.
- Startup failures move the plugin to `failed` without preventing the core API from starting.

### Task 8.8: Add Plugin Capability Approval and Permission Diff

**Files:**
- Create: `C:/Code/fbz/fbz-api/src/plugins/capabilities.rs`
- Create: `C:/Code/fbz/fbz-api/src/plugins/approval.rs`
- Modify: `C:/Code/fbz/fbz-api/src/auth/permissions.rs`
- Modify: `C:/Code/fbz/fbz-api/docs/plugins.md`
- Test: `C:/Code/fbz/fbz-api/src/plugins/approval.rs`

**Capability categories:**
- Read server settings.
- Write server settings.
- Read library metadata.
- Modify library metadata.
- Read playback sessions.
- Send notifications.
- Register webhooks.
- Register scheduled tasks.
- Register admin menu entries.
- Use network access.
- Use filesystem access inside plugin data/cache directories.
- Request transcoding or media analysis jobs.

**Steps:**
1. Store requested capabilities from each manifest version.
2. Store administrator-approved capabilities separately in `plugin_approved_capabilities`.
3. Compare old and new manifest capabilities during upgrade.
4. Require re-approval when a plugin asks for new or broader permissions.
5. Block activation when required capabilities are not approved.
6. Write approval, rejection, and capability-diff events to audit logs.

**Acceptance Criteria:**
- A plugin cannot silently gain permissions after upgrade.
- Approval is restricted to Owner/Admin roles with plugin management permission.
- Runtime host API calls fail closed when the matching capability is missing or revoked.

### Task 8.9: Add Plugin Configuration, Secret Storage, and Config UI Contract

**Files:**
- Create: `C:/Code/fbz/fbz-api/src/plugins/config.rs`
- Create: `C:/Code/fbz/fbz-api/src/plugins/secrets.rs`
- Create: `C:/Code/fbz/fbz-api/src/plugins/config_schema.rs`
- Modify: `C:/Code/fbz/fbz-api/migrations/0005_plugins.sql`
- Modify: `C:/Code/fbz/fbz-api/docs/plugins.md`
- Test: `C:/Code/fbz/fbz-api/src/plugins/config.rs`

**Rules:**
- Plugin config must be described by a JSON-schema-like manifest section.
- Sensitive fields must be marked explicitly and stored in `plugin_secrets`, not in plain config.
- Secrets are encrypted or sealed using a server-managed key derived from `PLUGIN_SECRET_KEY` or the platform secret provider.
- Normal config reads must return secret presence metadata only, never the raw value.
- Config updates must be validated before they are saved or passed to the plugin runtime.
- Built-in notification plugins must expose a test-send action through a capability-gated host API.

**Acceptance Criteria:**
- A plugin cannot access another plugin's config or secrets.
- Secrets are redacted from logs, audit events, API responses, and panic reports.
- Invalid config prevents plugin activation with a clear diagnostic.

### Task 8.10: Add Host API Gateway and Resource Limits

**Files:**
- Create: `C:/Code/fbz/fbz-api/src/plugins/host_api.rs`
- Create: `C:/Code/fbz/fbz-api/src/plugins/context.rs`
- Create: `C:/Code/fbz/fbz-api/src/plugins/quota.rs`
- Create: `C:/Code/fbz/fbz-api/src/plugins/circuit_breaker.rs`
- Modify: `C:/Code/fbz/fbz-api/src/plugins/runtime.rs`
- Test: `C:/Code/fbz/fbz-api/src/plugins/host_api.rs`

**Host API principles:**
- Host APIs are versioned and capability-gated.
- Plugins receive a scoped plugin context, not global application state.
- Database, Redis, filesystem, network, and queue operations go through host APIs.
- Each host API call includes plugin ID, version, request ID, user/session context where applicable, and audit metadata.

**Resource controls:**
- Per-plugin hook timeout, default `PLUGIN_TIMEOUT_MS`.
- Per-plugin concurrency, default `PLUGIN_MAX_CONCURRENCY`.
- Memory/process limit where the selected runtime supports it.
- Queue rate limits for notification, scan, webhook, and media-analysis requests.
- Circuit breaker after repeated panics, timeouts, or rejected host API calls.

**Acceptance Criteria:**
- Slow or failing plugins cannot exhaust API worker threads.
- Host API calls are observable through tracing and audit logs.
- Circuit breaker disables plugin execution while preserving administrator diagnostics.

### Task 8.11: Add Plugin Data Isolation and Migrations

**Files:**
- Create: `C:/Code/fbz/fbz-api/src/plugins/storage.rs`
- Create: `C:/Code/fbz/fbz-api/src/plugins/migrations.rs`
- Modify: `C:/Code/fbz/fbz-api/migrations/0005_plugins.sql`
- Modify: `C:/Code/fbz/fbz-api/docs/plugins.md`
- Test: `C:/Code/fbz/fbz-api/src/plugins/storage.rs`

**Data model:**
- Plugin-owned data is namespaced by plugin ID and version.
- Small structured data uses a host-managed key-value or document table.
- Large files use `PLUGIN_DATA_DIR` and `PLUGIN_CACHE_DIR` under plugin-specific directories.
- Plugin migrations are declared in the package and run through the host, not through direct database credentials.
- Uninstall supports two modes: keep plugin data for reinstall, or purge plugin data after explicit administrator confirmation.

**Acceptance Criteria:**
- Plugins cannot write directly to core media, user, auth, or settings tables.
- Backup/export can include or exclude plugin data deterministically.
- Failed plugin migrations roll back the plugin version activation.

### Task 8.12: Add Plugin Admin UI Sandbox

**Files:**
- Create: `C:/Code/fbz/fbz-api/src/plugins/ui.rs`
- Modify: `C:/Code/fbz/fbz-api/src/plugins/admin_menu.rs`
- Modify: `C:/Code/fbz/fbz-api/docs/plugins.md`

**Rules:**
- Plugin admin routes are namespaced under `/admin/plugins/{plugin_id}` later when the Web UI exists.
- The backend must already model menu contribution, route ownership, required permission, and plugin owner.
- Plugin UI assets must be served with strict content security policy.
- Plugin UI talks to the backend through a versioned host SDK/API, not through arbitrary internal endpoints.
- Menu entries are hidden when the user lacks the plugin-required permission or the plugin is disabled.

**Acceptance Criteria:**
- A plugin cannot replace core admin pages.
- A plugin cannot inject arbitrary JavaScript into core Web UI shells.
- Disabling a plugin removes its menu entries and denies its UI/API routes immediately.

### Task 8.13: Add Plugin Test Harness and Compatibility Suite

**Files:**
- Create: `C:/Code/fbz/fbz-api/tests/plugins/manifest_validation.rs`
- Create: `C:/Code/fbz/fbz-api/tests/plugins/lifecycle.rs`
- Create: `C:/Code/fbz/fbz-api/tests/plugins/capability_approval.rs`
- Create: `C:/Code/fbz/fbz-api/tests/plugins/hook_contracts.rs`
- Create: `C:/Code/fbz/fbz-api/tests/plugins/sandbox_failures.rs`
- Create: `C:/Code/fbz/fbz-api/tests/fixtures/plugins/`

**Test coverage:**
- Manifest validation.
- Install, enable, disable, upgrade, rollback, and uninstall.
- Capability diff and re-approval.
- Config validation and secret redaction.
- Hook timeout and failure isolation.
- Resource limit and circuit breaker behavior.
- Admin menu contribution filtering.
- Built-in Telegram, Enterprise WeChat, and webhook notification plugin golden tests.

**Acceptance Criteria:**
- Plugin contract regressions fail CI before release.
- A deliberately crashing plugin cannot crash the API process in tests.
- A plugin requesting new permissions after upgrade remains disabled until approved.

### Task 8.14: Harden Built-in Notification Plugin Contracts

**Files:**
- Modify: `C:/Code/fbz/fbz-api/src/plugins/builtin_notifications.rs`
- Modify: `C:/Code/fbz/fbz-api/plugins/tg-notify/manifest.json`
- Modify: `C:/Code/fbz/fbz-api/plugins/wecom-notify/manifest.json`
- Modify: `C:/Code/fbz/fbz-api/plugins/webhook-notify/manifest.json`
- Create: `C:/Code/fbz/fbz-api/tests/plugins/notification_plugins.rs`

**Rules:**
- Notification plugins must use the same lifecycle, approval, config, secret, host API, and queue rules as third-party plugins.
- Telegram, Enterprise WeChat, and generic webhook credentials are stored as plugin secrets.
- Delivery uses event outbox plus retry/backoff.
- Each notification plugin exposes a dry-run/test-send action.
- Delivery logs must redact request secrets and response-sensitive fields.

**Acceptance Criteria:**
- Built-in plugins do not have hidden privileges that third-party plugins cannot be granted later.
- Notification failure does not block the core event pipeline.
- Administrators can inspect delivery status and retry failed deliveries.

## Phase 9: Multi-User Permissions

### Task 9.1: Add User and Role Model

**Files:**
- Create: `C:/Code/fbz/fbz-api/src/auth/users.rs`
- Create: `C:/Code/fbz/fbz-api/src/auth/roles.rs`

**Permissions:**
- Admin.
- Server management.
- Manage libraries.
- Manage library paths.
- Manage users.
- Manage devices.
- Manage system settings.
- Manage plugins.
- Manage scheduled tasks.
- Play media.
- Download media.
- Transcode allowed.
- Remote access.
- New device login allowed.
- Library allow/deny.
- Rating and tag restrictions.

**Default role presets:**
- Owner: all permissions, ownership transfer, destructive maintenance, system reset, and plugin trust decisions.
- Admin: all server management except ownership transfer and destructive system reset.
- Library Manager: assigned library management, paths, scans, metadata, artwork, intro/outro marker correction.
- User: playback for assigned libraries, progress sync, existing device login.
- Restricted User: playback for assigned libraries only; download disabled, transcode disabled unless explicitly enabled, new device login disabled by default.

**Default new device login policy:**
- Owner and Admin: allowed.
- User: disabled until explicitly enabled.
- Restricted User: disabled.

**Acceptance Criteria:**
- Permission checks are centralized.
- Routes do not manually inspect roles ad hoc.
- Server/admin permissions are separated from per-user Emby playback capabilities.
- Each user can be configured for download permission, transcode permission, library access, remote access, and new device login policy.

### Task 9.2: Add Default Role Preset Seeder

**Files:**
- Create: `C:/Code/fbz/fbz-api/src/auth/default_roles.rs`
- Modify: `C:/Code/fbz/fbz-api/migrations/0001_initial.sql`
- Test: `C:/Code/fbz/fbz-api/src/auth/default_roles.rs`

**Steps:**
1. Seed Owner, Admin, Library Manager, User, and Restricted User role presets.
2. Make seeded roles idempotent.
3. Allow administrators to clone presets into custom roles later.
4. Protect Owner permissions from accidental removal.

**Acceptance Criteria:**
- A new installation has usable role presets without manual SQL.
- Default role changes are auditable and migration-safe.

### Task 9.3: Add Session and Device Tracking

**Files:**
- Create: `C:/Code/fbz/fbz-api/src/auth/sessions.rs`

**Steps:**
1. Store token hash, user ID, device ID, client name, version, last seen.
2. Support logout token revocation.
3. Support API keys separately from user sessions.

**Acceptance Criteria:**
- API key auth and user auth are auditable separately.
- Device information from Emby auth header is retained.

## Phase 10: Music Support

### Task 10.1: Add Music Schema

**Files:**
- Create: `C:/Code/fbz/fbz-api/migrations/0006_music.sql`

**Tables:**
- `music_artists`
- `music_albums`
- `music_tracks`
- `music_track_artists`
- `music_album_artists`
- `music_composers`
- `lyrics`
- `playlists`
- `playlist_items`

**Fields to support:**
- Artist name, sort name, provider IDs.
- Album title, album artist, release date, disc count, provider IDs.
- Track title, track artist, album artist, composer, disc number, track number, duration, codec, bitrate, sample rate, channels.
- Embedded lyrics and external lyrics path.
- ReplayGain fields later if needed.

**Acceptance Criteria:**
- Album artist and track artist are modeled separately.
- Composer and multi-artist tracks are supported.
- Multi-disc albums and track numbers are supported.

### Task 10.2: Add Music Scanner and Tags

**Files:**
- Create: `C:/Code/fbz/fbz-api/src/music/scanner.rs`
- Create: `C:/Code/fbz/fbz-api/src/music/tags.rs`

**Steps:**
1. Read embedded tags.
2. Extract cover art when present.
3. Handle missing tags with path fallback.
4. Store embedded lyrics when present.
5. Support external lyrics files later.
6. Use Fanart for artist/album artwork when configured.
7. Keep provider boundary open for MusicBrainz-compatible identifiers later.

**Acceptance Criteria:**
- Music library browsing does not depend on video media tables only.
- Albums, artists, and tracks have stable identity rules.
- Music scanning works without a metadata provider and improves when providers are configured.

## Phase 11: Performance and High Availability

### Task 11.1: Add Read Model and Cache Strategy

**Files:**
- Create: `C:/Code/fbz/fbz-api/src/storage/cache.rs`
- Create: `C:/Code/fbz/fbz-api/migrations/0007_read_models.sql`

**Read models:**
- Continue watching.
- Latest added.
- Library home sections.
- Recently played music.
- Search suggestions.

**Acceptance Criteria:**
- Expensive home screen queries do not scan raw media tables repeatedly.
- Cache invalidation is event-driven where possible.

### Task 11.2: Add Search Strategy

**Files:**
- Create: `C:/Code/fbz/fbz-api/src/storage/search.rs`

**Steps:**
1. Start with PostgreSQL full-text search.
2. Add normalized title aliases and pinyin/transliteration hook later if needed.
3. Keep adapter boundary for future Meilisearch/Typesense.

**Acceptance Criteria:**
- Search is fast enough for local libraries.
- Search provider can be swapped without rewriting API routes.

### Task 11.3: Add HA Deployment Notes

**Files:**
- Create: `C:/Code/fbz/fbz-api/docs/deployment.md`

**Topics:**
- PostgreSQL primary/replica.
- Redis persistence and failover.
- Stateless API instances.
- Worker concurrency.
- Distributed locks for scheduler.
- Transcode cache locality.
- Shared media mounts.

**Acceptance Criteria:**
- API can scale horizontally.
- Workers can scale independently.
- Single-node development remains simple.

## Phase 12: Compatibility and Regression Tests

### Task 12.1: Create Compatibility Test Matrix

**Files:**
- Create: `C:/Code/fbz/fbz-api/tests/compat_emby.rs`
- Create: `C:/Code/fbz/fbz-api/docs/emby-compatibility.md`

**Test groups:**
- Auth.
- System info.
- Users.
- Library browse.
- Items.
- Images.
- Playback info.
- Playback progress.
- Scan triggers.
- JSON/XML content negotiation.
- Third-party client connect/browse/play flow.

**Acceptance Criteria:**
- Every supported Emby endpoint has at least one compatibility test.
- Unsupported endpoints are documented.
- A supported endpoint is not complete until both required JSON and XML behavior are covered.

### Task 12.2: Add API Golden Responses

**Files:**
- Create: `C:/Code/fbz/fbz-api/tests/fixtures/emby/`

**Steps:**
1. Store sample Emby-compatible responses.
2. Compare field presence and types.
3. Allow additive internal fields only where clients tolerate them.

**Acceptance Criteria:**
- Compatibility regressions are visible before release.
- DTO changes are intentional.

## Suggested Implementation Order

1. Lock remaining Phase 0 details.
2. Foundation config/error/telemetry.
3. Bundled FFmpeg/ffprobe resolver and diagnostics.
4. PostgreSQL + migration baseline.
5. Redis queue + event outbox baseline.
6. Emby auth + system/user endpoints.
7. Library creation and scanner queue.
8. ffprobe media probing.
9. Metadata provider registry plus TMDB/TVDB/Fanart providers.
10. Items/images/playback endpoints.
11. STRM intranet/allowlist playback and DirectPlay.
12. Transcoding queue with max 3 concurrent jobs.
13. Default hardware transcoding with software fallback.
14. Intro/outro marker import and plugin hook.
15. Plugin manifest, package format, registry, and lifecycle management.
16. Plugin capability approval, config schema, secret storage, and host API gateway.
17. Plugin runtime isolation, resource limits, hook contracts, scheduler integration, data isolation, and admin menu sandbox.
18. Notification plugins: Telegram, Enterprise WeChat, generic webhook.
19. Plugin compatibility test harness and failure-isolation suite.
20. Multi-user permission hardening.
21. Music library.
22. HA/performance hardening.

## User Supplement Checklist

Confirmed:

- [x] First target client(s): third-party Emby-compatible clients; FBZ Web deferred.
- [x] Compatibility target: Emby only.
- [x] v1 JSON/XML support: required.
- [x] Deployment OS: Windows, Linux, Docker, NAS.
- [x] Docker usage: production only.
- [x] Process model: modular single service by default, production can split API/worker/scheduler by `FBZ_NODE_ROLE`.
- [x] PostgreSQL required or optional: required.
- [x] Redis required or optional: required.
- [x] Cache storage: filesystem first, object storage adapter boundary later.
- [x] Scan event retention: latest normalized state plus 90-day partitioned audit/event history by default.
- [x] Public server address: administrator-editable later, env bootstrap default first.
- [x] First Emby endpoint subset: connect/browse/play MVP.
- [x] Direct path aliases: support both `/emby/*` and direct `/*` aliases for implemented endpoints.
- [x] Recommended media path examples: Windows `D:/Media/*`, Linux/NAS `/mnt/media/*`, Docker `/media/*`.
- [x] Metadata provider token policy: TMDB enabled with access token; TVDB and Fanart disabled until API keys are configured.
- [x] Proxy policy: global bootstrap proxy with future per-provider override.
- [x] FFmpeg/ffprobe: bundled, with external override support.
- [x] Hardware acceleration: default hardware first, software fallback.
- [x] Hardware backend priority: Intel first, NVIDIA second, AMD third, software fallback.
- [x] STRM allowed URL rules: intranet first, configurable safe external domains.
- [x] Metadata providers: TMDB, TVDB, Fanart, with admin-editable base URLs/mirrors.
- [x] Notification channels: plugin-based Telegram, Enterprise WeChat, generic webhook.
- [x] Intro/outro marker data: support import and plugin-provided sources such as TiDb/external marker datasets.
- [x] Permission model: server management permissions plus Emby-style user playback/device capabilities.
- [x] Default role presets: Owner, Admin, Library Manager, User, Restricted User.
- [x] Default new device login policy: Owner/Admin allowed, User disabled until enabled, Restricted disabled.
- [x] Music metadata default: embedded tags first, Fanart artwork when configured, MusicBrainz-compatible IDs later via provider/plugin.
- [x] Scheduled task defaults: incremental scan 15m, full scan daily, metadata refresh daily/weekly, cleanup jobs on safe intervals.
- [x] Expected library size: about 5 PB, hundreds of thousands of movies and TV items.
- [x] Expected concurrent transcodes: 3 initially; later requests queue.
- [x] Expected concurrent users: up to 1000.

Still fill these before implementation or production deployment:

- [ ] Real production media root paths:
- [ ] Actual TMDB/TVDB/Fanart credentials:
- [ ] Actual proxy values, if needed:
- [ ] Actual safe external STRM domains, if any:

## Verification Commands

Run after each implementation batch:

```powershell
cd C:/Code/fbz/fbz-api
cargo fmt --check
cargo check
cargo test
```

When database integration begins, add:

```powershell
cd C:/Code/fbz/fbz-api
cargo test --test '*'
```

## Documentation Sources

- Emby REST API: `https://dev.emby.media/doc/restapi/index.html`
- Emby API Browser: `https://swagger.emby.media/?staticview=true`
- TMDB API: `https://developer.themoviedb.org/docs/getting-started`
- TMDB images: `https://developer.themoviedb.org/docs/image-basics`
- TheTVDB API v4: `https://thetvdb.github.io/v4-api/`
- TheTVDB API licensing: `https://www.thetvdb.com/api-information`
- Fanart.tv API: `https://api.fanart.tv/`
- FFmpeg: `https://ffmpeg.org/ffmpeg.html`
- ffprobe: `https://ffmpeg.org/ffprobe.html`
- FFmpeg legal: `https://www.ffmpeg.org/legal.html`
- Emby plugin development: `https://dev.emby.media/doc/plugins/dev/index.html`
- Emby server plugins: `https://dev.emby.media/doc/plugins/index.html`
- PostgreSQL HA: `https://www.postgresql.org/docs/current/high-availability.html`
- Redis Streams: `https://redis.io/docs/latest/develop/data-types/streams/`
