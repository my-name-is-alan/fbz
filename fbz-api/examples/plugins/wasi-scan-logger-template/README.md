# WASI Scan Logger (example plugin)

A first-party **WASI runtime** example plugin for FBZ. It is a sandboxed,
no-network compute plugin: the FBZ WASI runtime (wasmtime + WASIp1) runs it once
per dispatched event, hands it the event payload on **stdin**, and captures its
**stdout** as the execution response.

Use this as the starting point when a plugin only needs deterministic in-process
compute (normalizing tags, computing markers/digests, transforming payloads).
For plugins that must call the **Host API** or reach the network, use the **HTTP
runtime** instead (see `http-marker-importer` / `http-notification-bridge`) —
WASIp1 has no sockets.

## Runtime contract

| Channel | Meaning |
| --- | --- |
| `argv[0]` | entrypoint path |
| `argv[1]` | dispatched handler key (e.g. `hooks.onScanCompleted`) |
| env `FBZ_PLUGIN_ID` / `FBZ_PLUGIN_HANDLER` / `FBZ_PLUGIN_IDEMPOTENCY_KEY` | dispatch context |
| env `FBZ_HOST_BASE_URL` / `FBZ_PLUGIN_TOKEN` | Host API base + short-lived token (only usable from the HTTP runtime) |
| stdin | dispatched event payload (JSON) |
| stdout | execution response (captured, size-capped) |
| `/plugin` | read-only package dir |
| `/data`, `/cache` | read/write persistent dirs |
| `/tmp` | read/write per-run scratch |

Execution is bounded by fuel, memory, an epoch timeout, and stdio/module size
caps (`PLUGIN_WASI_*` env on the server).

## Build

Requires the `wasm32-wasip1` target:

```powershell
rustup target add wasm32-wasip1
cargo build --release --target wasm32-wasip1
```

This emits `target/wasm32-wasip1/release/plugin.wasm`, matching the manifest
`entrypoint: "plugin.wasm"`.

## Package & sign

Copy the built `plugin.wasm` next to `manifest.json` into the package root, then
package and (for production) sign it:

```powershell
# stage: plugin.wasm + manifest.json at the package root
./scripts/package-plugin.ps1 -PluginDir <staged-dir> -Force
cargo run --bin sign-plugin-package -- --package <zip> --key-id <keyId>
```

See `docs/plugin-development.md` (WASI section) and `docs/plugin-system.md`.
