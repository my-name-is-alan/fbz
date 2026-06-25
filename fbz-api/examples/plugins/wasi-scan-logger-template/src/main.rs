//! First-party example WASI plugin for FBZ.
//!
//! The FBZ WASI runtime (wasmtime + WASIp1) invokes this module as a command
//! (`_start`) once per dispatched hook/schedule, providing:
//!
//! - **argv**: `argv[0]` = the entrypoint path, `argv[1]` = the dispatched
//!   handler key (e.g. `hooks.onScanCompleted`).
//! - **env**: `FBZ_PLUGIN_ID`, `FBZ_PLUGIN_HANDLER`, `FBZ_PLUGIN_IDEMPOTENCY_KEY`,
//!   `FBZ_HOST_BASE_URL`, and (when issued) `FBZ_PLUGIN_TOKEN`.
//! - **stdin**: the dispatched event payload as JSON.
//! - **preopened dirs**: `/plugin` (read-only package), `/data` and `/cache`
//!   (read/write, persistent), `/tmp` (read/write, per-run scratch).
//!
//! The module's **stdout** is captured as the execution response. WASIp1 has no
//! sockets, so a WASI plugin cannot call the Host API or the network — keep WASI
//! plugins to deterministic compute and use the HTTP runtime for networked /
//! Host-API plugins. Execution is bounded by fuel, memory, an epoch timeout, and
//! stdio/module size caps configured by the host.

use std::io::{self, Read, Write};

fn main() {
    let handler = std::env::args().nth(1).unwrap_or_default();
    let plugin_id = std::env::var("FBZ_PLUGIN_ID").unwrap_or_default();
    let idempotency_key = std::env::var("FBZ_PLUGIN_IDEMPOTENCY_KEY").unwrap_or_default();

    // The dispatched event payload arrives as JSON on stdin.
    let mut payload = String::new();
    let _ = io::stdin().read_to_string(&mut payload);
    let event: serde_json::Value =
        serde_json::from_str(&payload).unwrap_or(serde_json::Value::Null);

    // Pure compute: summarize what we received. A real plugin would derive a
    // result (normalized tags, computed markers, a digest, ...) and may persist
    // small state under /data.
    let summary = serde_json::json!({
        "plugin": plugin_id,
        "handler": handler,
        "idempotencyKey": idempotency_key,
        "receivedEvent": event,
    });

    // stdout is the execution response captured by the host.
    let mut stdout = io::stdout();
    let _ = writeln!(stdout, "{summary}");
    let _ = stdout.flush();
}
