# Telegram Notification Template

This first-party HTTP template receives selected FBZ plugin hook dispatches and submits Telegram-oriented notifications to `POST /api/plugin/notifications`.

The plugin does not store Telegram credentials or call Telegram directly. Delivery remains owned by administrator-managed notification targets whose logical channel matches the plugin config value.

## Files

- `manifest.json`: plugin contract for installation.
- `server.mjs`: dependency-free Node.js HTTP runtime endpoint.
- `fbz-plugin-http.mjs`: copied into packaged zips by `scripts/package-plugin.ps1`.

## Local Run

Requires Node.js 18+ for the built-in `fetch` API.

```powershell
$env:PORT="19093"
node examples/plugins/telegram-notifier-template/server.mjs
```

The default entrypoint is `http://127.0.0.1:19093/fbz-plugin`.

## Configuration

- `channel`: logical notification channel. Defaults to `telegram`.
- `title_prefix`: short title prefix. Defaults to `FBZ Telegram`.
- `include_payload`: appends a truncated JSON payload when true.

Configure the actual Telegram target through the Admin API notification target routes. This template only submits controlled notification requests to administrator-managed notification targets.

## Package

```powershell
./scripts/package-plugin.ps1 -PluginDir examples/plugins/telegram-notifier-template -Force
```

Keep `PLUGIN_SECRET_KEY` aligned between the host and this process when signed HTTP dispatches are enabled.
