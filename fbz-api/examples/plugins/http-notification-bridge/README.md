# HTTP Notification Bridge Plugin

This is a first-party example HTTP plugin. It receives FBZ plugin hook dispatches and forwards selected events into the controlled Host API notification endpoint.

## Files

- `manifest.json`: plugin contract used when installing the package.
- `server.mjs`: dependency-free Node.js HTTP runtime endpoint for local development.
- `fbz-plugin-http.mjs`: packaged copy of the shared HTTP helper. The example loads it from the package root first and falls back to `../_shared/fbz-plugin-http.mjs` during repository development.

## Local Run

Requires Node.js 18+ for the built-in `fetch` API.

```powershell
$env:PORT="19091"
node examples/plugins/http-notification-bridge/server.mjs
```

The manifest entrypoint is `http://127.0.0.1:19091/fbz-plugin`, which matches the default `PLUGIN_HTTP_ALLOWED_HOSTS`.

## Package Shape

Create a zip whose root contains `manifest.json` and any runtime files you want to keep with the package:

```text
manifest.json
fbz-plugin-http.mjs
server.mjs
README.md
```

From the repository root, the helper below writes the archive to `var/plugin-packages` and prints an install-ready `packagePath`, `checksumSha256`, and manifest payload:

```powershell
./scripts/package-plugin.ps1 -PluginDir examples/plugins/http-notification-bridge -Force
```

Install the package through the admin plugin package API using this manifest. In production, keep `PLUGIN_ALLOW_UNSIGNED=false` and provide a trusted package signature.

## Runtime Notes

The example expects the host to send:

- `X-FBZ-Plugin-Token`
- `X-FBZ-Host-Base-Url`
- `X-FBZ-Plugin-Idempotency-Key`

For each dispatch, the example reads `/api/plugin/config` and uses `values.channel` as the logical notification channel. `NOTIFICATION_CHANNEL` is only a local fallback when config is absent or unavailable.

If `PLUGIN_SECRET_KEY` is configured on the host, set the same value in this process as `PLUGIN_SECRET_KEY` so the example can verify HTTP plugin signatures before forwarding notifications.

The example keeps idempotency keys in a bounded in-memory set, controlled by `PROCESSED_KEY_CACHE_LIMIT`. A production plugin should persist idempotency through plugin KV or its own durable store.
