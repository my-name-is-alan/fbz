# AGENTS.md

Repo-wide agent guide. See `CLAUDE.md` (repo root) for the full architecture/command reference, `fbz-api/README.md` for backend env vars, and `fbz-fe/AGENTS.md` for the frontend (Vite+/`vp`) rules. The sections below only capture non-obvious, durable setup/run caveats for this Linux cloud environment.

## Cursor Cloud specific instructions

FBZ is a self-hosted, Emby-compatible media server with two independently-built packages: `fbz-api/` (Rust/axum backend) and `fbz-fe/` (Vue 3 + Vite+ frontend). The repo's helper scripts under `fbz-api/scripts/*.ps1` are PowerShell/Windows-oriented; on this Linux VM run the underlying commands directly (as below).

### Services and how to run them (start order matters)

The backend refuses to boot unless both PostgreSQL and Redis are reachable. These are installed as OS packages (not Docker here) and there is no systemd, so start them per session:

1. `sudo service postgresql start` — PostgreSQL 16 on `127.0.0.1:5432`, role/db `fbz`/`fbz` (password `fbz`). Matches `DATABASE_URL` in `fbz-api/.env`.
2. `sudo service redis-server start` — Redis on `127.0.0.1:6379`.
3. Backend: `cd fbz-api && cargo run` → listens on `127.0.0.1:8080`. It auto-runs migrations and (because `FBZ_BOOTSTRAP_ADMIN_*` is set in `.env`) creates an admin user on first boot. Probes: `curl 127.0.0.1:8080/health` and `/ready`.
4. Frontend: `cd fbz-fe && vp dev` → Vite+ dev server on `:5173`, which proxies `/api`, `/emby`, and the Emby route prefixes to `127.0.0.1:8080`. For UI-only work it also runs standalone against bundled TMDB mock data.

`fbz-api/.env` is gitignored; if missing, `cp fbz-api/.env.example fbz-api/.env`, then set `FBZ_BOOTSTRAP_ADMIN_USERNAME`/`FBZ_BOOTSTRAP_ADMIN_PASSWORD` (used here: `admin`/`admin123`) and point `MEDIA_ROOTS` at local dirs (used here: `./var/media/Movies,./var/media/TV,./var/media/Music`, relative to `fbz-api/`). External metadata providers (TMDB/TVDB/Fanart) are optional and no-op without API tokens.

### Node version gotcha (frontend) — important

The frontend toolchain (`vp`, vite-plus `0.2.1`) needs its native binary `@voidzero-dev/vite-plus-linux-x64-gnu`, whose `engines` require Node `^20.19.0 || ^22.18.0 || >=24.11.0`. The VM's default `/exec-daemon/node` is `v22.14.0`, which fails that constraint, so `pnpm`/`vp install` will SILENTLY skip the native binary and every `vp` command then dies with `Cannot find module './vite-plus.linux-x64-gnu.node'`.

This is handled by a `node`/`npm`/`npx`/`pnpm`/`vp` symlink in `/usr/local/cargo/bin` (first on `PATH`) pointing at nvm's Node `v22.22.2`, so all shells (including non-interactive) get a compatible Node. If `node -v` reports `v22.14.0`, that symlink is missing — recreate it (target nvm `>=22.18`) and re-run `vp install`, otherwise the frontend will not build/run.

### Lint / test / build

- Frontend: `vp check` (lint+format+typecheck), `vp test` (vitest), `vp run build`. All pass. Note: jsdom-environment component tests require a globally-installed `jsdom` because the global `vp` test runner resolves `jsdom` from its own location, not the project's; a global `jsdom@29.1.1` is installed for this. Also, `vp test` uses the global runner — the local runner is broken by a vite-plus (`0.2.1`) vs vite-plus-test (`0.1.24`) version skew in the repo's catalog, so prefer plain `vp test`.
- Backend: `cargo build` and `cargo run` work. `cargo test` / `cargo clippy --all-targets` currently FAIL TO COMPILE due to pre-existing test code that omits struct fields (`current_pw` in `UpdateUserPasswordDto`, `profile_path` in `TmdbCastCredit`/`TmdbCrewCredit`) — this is a repo code issue, not an environment problem. The library/binary itself is clean.
