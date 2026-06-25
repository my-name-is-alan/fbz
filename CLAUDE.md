# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Repo layout

FBZ is a self-hosted, Emby-compatible media server. The repo is a two-package workspace, NOT a unified build:

- `fbz-api/` — Rust modular monolith (axum + tokio + sqlx + PostgreSQL + Redis + wasmtime). Edition 2024. This is the server.
- `fbz-fe/` — Vue 3 + TypeScript SPA (Vite+ toolchain, pnpm, UnoCSS, Pinia, Shaka Player). This is the web UI + admin console.
- `docs/`, `demo/`, `canvas/` — design/planning assets only; not built.

**Cross-cutting rule**: backend and frontend are developed independently. Run their commands inside their own directory; nothing at the repo root builds both.

When working in `fbz-fe/`, also read `fbz-fe/CLAUDE.md` — it is the canonical frontend agent guide and overrides anything generic here.

## Commands

### Backend (`fbz-api/`, PowerShell)

```powershell
./scripts/dev-deps.ps1           # start dockerised PostgreSQL + Redis, wait healthy
./scripts/dev-deps.ps1 -Action status | restart | stop
./scripts/dev.ps1                # run with hot reload (uses cargo-watch if installed, else PS polling)
cargo run                        # one-shot run
cargo test                       # all tests (use `cargo test <name>` for a single test)
cargo build --release
```

Health probes once running: `Invoke-RestMethod http://127.0.0.1:8080/health` and `/ready`.

Plugin smoke tests (require running deps + temp server, scripts handle this with `-StartServer`):

```powershell
./scripts/smoke-plugin-lifecycle.ps1 -StartServer        # install/approve/enable lifecycle
./scripts/smoke-plugin-runtime.ps1 -StartServer          # worker + Host API + audit
./scripts/package-plugin.ps1 -PluginDir examples/plugins/<name> -Force   # build a plugin zip
cargo run --bin sign-plugin-package -- --package <zip> --key-id <keyId>  # sign for prod
```

Configuration: copy `fbz-api/.env.example` to `.env`; never commit `.env`. Full env var reference is in `fbz-api/README.md`. Key knobs: `DATABASE_URL`, `REDIS_URL`, `FBZ_NODE_ROLE` (`all`/`api`/`worker`/`scheduler`), `FBZ_*_WORKER_ENABLED`, `MEDIA_ROOTS`, `PLUGIN_*`, `TMDB_ACCESS_TOKEN` / `TVDB_API_KEY` / `FANART_API_KEY`.

### Frontend (`fbz-fe/`, PowerShell)

This project uses **Vite+** (`vp` CLI). Do NOT call `vite`, `npm`, or `pnpm dlx` directly for everyday dev:

```powershell
vp install                       # after pulling
vp dev                           # dev server
vp build                         # production build (package.json wraps as `tsc && vp build`)
vp preview
vp check                         # format + lint + type check (run before commit)
vp test                          # vitest (single test: `vp test <pattern>`)
vp run build                     # run via package.json script
vp add <pkg> | vp add -D <pkg> | vp remove <pkg>   # dependency changes — never hand-edit package.json versions
vp env doctor                    # diagnose toolchain issues
```

TMDB data refresh (placeholder dataset used until the backend is wired):

```powershell
node scripts/fetch-tmdb.mjs      # regenerates tmdb-catalog.json + tmdb-details.json from TMDB
```

The TMDB token lives only in `fbz-fe/.env`'s `api_token` and is consumed only by this script — never ship it in the bundle (the script bakes static JSON into `src/service/`).

## Architecture — backend (`fbz-api`)

A single `axum` process organized into modules under `src/`, with workers gated by `FBZ_NODE_ROLE` and `FBZ_*_WORKER_ENABLED`. Same binary deploys as API-only, worker-only, or scheduler-only nodes; PostgreSQL is the authoritative queue, Redis Streams is an optional cross-node mirror.

Top-level modules (`src/lib.rs`):

- `app.rs` — axum router composition + main service wiring (very large; the central seam).
- `state.rs` — shared app state, DB/Redis handles, runtime config.
- `config.rs` — strong-typed env parsing + startup validation.
- `db/` — sqlx pool, migration runner (migrations live in `migrations/`, currently 0001–0062 — additive only; ask before destructive changes).
- `auth/` — Argon2 password hashing, session/device policy, login hook.
- `users/`, `admin/` — user/role/library-permission model and admin REST surface (`/api/admin/*`).
- `library/`, `media/`, `scan/` — media library model, scan worker, file/probe pipeline.
- `metadata/` — `metadata.refresh` job + TMDB/TVDB/Fanart provider chain.
- `transcode/` — HLS transcoding sessions, hardware/software FFmpeg planner, output cleanup.
- `events/` — `event_outbox` + Redis Streams mirror worker.
- `jobs.rs`, `scheduler/` — generic job queue (`FOR UPDATE SKIP LOCKED`, leases, attempts) and cron-like scheduled tasks with `scheduled_task_runs` leases.
- `notifications/` — admin-managed notification targets (Telegram / WeCom / generic webhook) + delivery worker; secrets stored encrypted in `notification_target_secrets`.
- `plugins/` — manifest validation, package install + Ed25519 signature verification, `wasmtime`/WASIp1 + HTTP runtimes, dispatch outbox, Host API, signing binary at `src/bin/sign-plugin-package.rs`.
- `compat/emby/` — Emby REST compatibility (DTOs + 40+ route files in `routes/`); compile-only surface for Emby clients. Routes are the boundary that translates Emby concepts to FBZ services — they do not own SQL.

**Route boundaries** (`fbz-api/src/app.rs`):

- `/health`, `/ready` — probes.
- `/api/admin/*` — server admin operations, requires admin role token.
- `/api/plugin/*` — controlled Host API, requires short-lived `x-fbz-plugin-token` issued per dispatch.
- `/emby/*` and unprefixed Emby paths — Emby-client compatibility.

**Architectural invariants** (from `fbz-api/docs/plans/backend-execution-goal.md` and `database-scale.md`):

- Controllers/routes do auth, parsing, DTO mapping, error mapping. Business logic and SQL live in service/repository layers.
- All user input into SQL must go through bind parameters, enum allowlists, or normalized filters — never string-concatenated.
- Repository queries do permission filtering themselves; do not let upper layers filter after the fact.
- Keyset pagination for large/audit tables; do not pull full ID lists into the app to filter.
- IDs: compact `bigint` for hot joins, `public_id uuid` exposed to API/Emby.
- Separate `media_items` (logical) from `media_files` (physical) from `media_streams` (ffprobe rows).
- Migrations are additive. Adding a file is fine; destructive schema changes need explicit confirmation.
- Workers recover stale leases on every poll (jobs, scheduled tasks, transcode sessions, plugin runs, event-stream mirror) and emit `recovered stale …` structured warn logs.
- Plugin permission keys (`library.read`, `metadata.write`, `notification.send`, `admin.menu`, etc.) are enforced both at manifest validation and at Host API call time; see `docs/plugin-system.md`.

When changing Emby-compatible behavior, the `fbz-api/README.md` "当前能力" list is the spec — it enumerates every endpoint's current contract (including the "returns 409 / empty result / not implemented" boundary cases). Keep it in sync when you add/change a route.

## Architecture — frontend (`fbz-fe`)

Vue 3 SPA, Composition API + `<script setup lang="ts">` everywhere. Toolchain is Vite+ (`vp`).

Layout (`src/`):

- `main.ts` → `App.vue` → `layouts/default.vue` (public shell) or `layouts/admin.vue` (admin shell). `App.vue` is entry-only — no business logic.
- `router/index.ts` — manually maintained route table. Routes match folder paths (`/movie/:id` → `views/detail/movie/index.vue`). The admin section currently dispatches every child route to `views/account/index.vue` until the in-progress migration to dedicated admin pages lands; admin components are split out under `components/admin/`.
- `views/` — pages. `components/` — auto-imported components (don't write `import Xxx from …` for in-project components). Base components use a `Base` prefix.
- `stores/` — Pinia function-style stores (`auth`, `library`, `playback`, `theme`, `ui`).
- `service/request.ts` — single `axios` instance (`baseURL = import.meta.env.VITE_API_BASE_URL ?? "/api"`); per-domain modules live in `service/modules/`.
- `service/tmdb.ts` + `tmdb-catalog.json` (bundled, lightweight) + `tmdb-details.json` (dynamically `import()`-ed in `tmdb.ts`, only fetched on detail pages) — placeholder data source until the backend is connected. Replace `tmdb.ts` functions with `fbz-api` calls when wiring up; page consumers will not change.
- `composables/`, `utils/`, `plugins/`, `types/`, `styles/`, `assets/` — standard buckets.
- `styles/theme/tokens.scss` is injected globally via Vite `scss.additionalData`; `uno.config.ts` theme tokens must mirror it.

Auto-imports cover Vue / Vue Router / Pinia / `@vueuse/core` and a curated set of `lodash-es`. Do not hand-import `ref`, `computed`, `useRoute`, `defineStore`, `debounce`, etc.

**Design constraints** (`fbz-fe/CLAUDE.md` is authoritative — open it before any UI work):

- Strict TypeScript; `import type` for types; no `enum`/`namespace`; `@/*` alias.
- All SFC styles must be `<style lang="scss">`; never add a `.css` file under `src/`.
- Single brand color `--fbz-color-brand-500: #1ed760` on a `#0a0a0b` background; no gradients/multicolor decoration. Resolution badges in `tmdb.ts:resolutionColors` are the documented exception.
- Use `MediaCard` for all media tiles, `BaseScroller` for all horizontal rows, `BaseSelect` for all dropdowns (never native `<select>`), `MediaPoster` for posters.
- Responsive breakpoints: desktop ≥1024, tablet 600–1024, phone <600.
- Detail routes by type: `/movie/:id`, `/tv/:id`, `/person/:id`, `/collection/:id`. `libraryId` (movie/series/anime/documentary) and `detailType` (movie/tv) are decoupled.
- Run `vp check` and `vp test` before committing; run `vp run build` when changes touch Vite config, routing, styles injection, or dependencies.

## Dependency hygiene

- Backend dependencies: edit `fbz-api/Cargo.toml` directly, run `cargo build` to refresh `Cargo.lock`. Edition is `2024`.
- Frontend dependencies: only via `vp add` / `vp add -D` / `vp remove`. The pnpm `catalog:` entries in `package.json` must not be replaced with hand-written semver. After dep changes, ensure `package.json` and `pnpm-lock.yaml` come from the same install command, then run `vp check`.

## Operational notes for Claude

- Default branch is `main`. The repo lives on Windows; expect CRLF/LF noise on auto-generated files like `fbz-fe/src/components.d.ts` (don't commit it manually — it's regenerated by `unplugin-vue-components`).
- The backend is highly capability-driven: most Emby endpoints return a controlled empty/409 boundary instead of a generic 404 to keep clients happy. When asked to "implement endpoint X", first check if it already exists as a boundary stub in `src/compat/emby/routes/` and upgrade it in place rather than adding a new route.
- Plugins are sandboxed by design. New Host API surface goes in `src/plugins/` and must declare a permission key — see `docs/plugin-system.md` and `docs/plugin-development.md`.
- Long-term backend goals and hard constraints are in `fbz-api/docs/plans/backend-execution-goal.md` — read it before making architectural decisions.
