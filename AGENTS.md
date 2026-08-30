# AGENTS.md

Repo-wide agent guide. See `CLAUDE.md` (repo root) for the full architecture/command reference, `fbz-api/README.md` for backend env vars, and `fbz-fe/AGENTS.md` for the frontend (Vite+/`vp`) rules. The sections below only capture non-obvious, durable setup/run caveats.

FBZ is a self-hosted, Emby-compatible media server with two independently-built packages: `fbz-api/` (Rust/axum backend) and `fbz-fe/` (Vue 3 + Vite+ frontend). Frontend catalog/detail data comes from `fbz-api`; there is no bundled TMDB mock path.

## Cursor Cloud / Linux VM

The helper scripts under `fbz-api/scripts/*.ps1` are PowerShell/Windows-oriented. On a Linux VM run the underlying commands directly.

### Services (start order matters)

The backend refuses to boot unless both PostgreSQL and Redis are reachable. On Cursor Cloud VMs these are often OS packages (not Docker) and there may be no systemd, so start them per session:

1. `sudo service postgresql start` — PostgreSQL 16 on `127.0.0.1:5432`. Role/db should match `DATABASE_URL` in `fbz-api/.env` (common local values: `fbz`/`fbz`).
2. `sudo service redis-server start` — Redis on `127.0.0.1:6379`.
3. Backend: `cd fbz-api && cargo run` → `127.0.0.1:8080`. It auto-runs migrations. If `FBZ_BOOTSTRAP_ADMIN_*` is set in `.env`, first boot creates an admin user. Probes: `curl 127.0.0.1:8080/health` and `/ready`.
4. Frontend: `cd fbz-fe && vp dev` → Vite+ on `:5173`, proxying `/api`, `/emby`, `/Shows`, `/Search`, and the other Emby route prefixes to `127.0.0.1:8080`. The UI needs the API for library/home/search/detail/playback.

`fbz-api/.env` is gitignored. If missing: `cp fbz-api/.env.example fbz-api/.env`, set `FBZ_BOOTSTRAP_ADMIN_USERNAME` / `FBZ_BOOTSTRAP_ADMIN_PASSWORD`, and point `MEDIA_ROOTS` at local dirs (relative to `fbz-api/`). TMDB/TVDB/Fanart are optional and no-op without tokens.

On Windows/Docker hosts prefer `./scripts/dev-deps.ps1` instead of OS-package services.

### Node version gotcha (frontend)

The frontend toolchain (`vp`, vite-plus `0.2.1`) needs its native binary (e.g. `@voidzero-dev/vite-plus-linux-x64-gnu`), whose `engines` require Node `^20.19.0 || ^22.18.0 || >=24.11.0`. Some VMs ship Node `v22.14.0`, which fails that constraint: `pnpm`/`vp install` will **silently skip** the native binary and every `vp` command then dies with `Cannot find module './vite-plus.linux-x64-gnu.node'`.

Fix: put a Node `>=22.18` (nvm or otherwise) first on `PATH`, then re-run `vp install`. If `node -v` still reports `v22.14.0`, the frontend will not build/run.

jsdom-environment component tests may need `jsdom` resolvable by the `vp` test runner (project `devDependencies` already include `jsdom`; a global copy is only needed if the runner resolves from its own install). Prefer `vp test` (the catalog pins `vite-plus` `0.2.1` vs `vite-plus-test` `0.1.24`).

### Lint / test / build

- Frontend: `vp check`, `vp test`, `vp run build`.
- Backend: `cargo build`, `cargo run`, `cargo test --lib`. Test DTO literals that previously omitted `current_pw` / `profile_path` were fixed; if `cargo test --lib` fails, treat it as a real regression, not an environment issue.
