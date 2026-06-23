# HTTP Marker Importer Plugin

This is a first-party example HTTP plugin for intro, credits, chapter, and commercial marker imports. It receives `metadata.refresh.completed` dispatches, reads the public media item through the controlled Host API, resolves marker candidates by external provider ID, and writes markers through `PUT /api/plugin/items/{itemId}/markers`.

The example is intentionally conservative: it never receives filesystem paths or playback URLs, never writes PostgreSQL directly, and only replaces markers under its own plugin-scoped source.

## Files

- `manifest.json`: plugin contract used when installing the package.
- `server.mjs`: dependency-free Node.js HTTP runtime endpoint for local development.
- `fbz-plugin-http.mjs`: packaged copy of the shared HTTP helper. The example loads it from the package root first and falls back to `../_shared/fbz-plugin-http.mjs` during repository development.
- `marker-fixture.json`: local marker map used when `markerSourceUrl` is not configured.

## Local Run

Requires Node.js 18+ for the built-in `fetch` API.

```powershell
$env:PORT="19092"
node examples/plugins/http-marker-importer/server.mjs
```

The manifest entrypoint is `http://127.0.0.1:19092/fbz-plugin`, which matches the default `PLUGIN_HTTP_ALLOWED_HOSTS`.

## Marker Lookup

The default fixture map uses keys shaped as:

```text
tmdb:550
tvdb:series-123:season-1:episode-1
```

Each value is a marker array using the Host API shape:

```json
[
  {
    "markerType": "intro_start",
    "startTicks": 900000000,
    "endTicks": 930000000,
    "confidence": 0.98
  }
]
```

For a real TiDb/chapter source, configure `marker_source_url` in the plugin config. The example calls it with query parameters:

```text
itemId=<public item id>
itemType=<movie|episode|...>
provider=<tmdb|tvdb|...>
externalId=<provider id>
seasonNumber=<optional>
episodeNumber=<optional>
```

The endpoint may return either an array of markers or an object with a `markers` array.

## Package

From the repository root, build an installable ZIP:

```powershell
./scripts/package-plugin.ps1 -PluginDir examples/plugins/http-marker-importer -Force
```

The helper writes to `var/plugin-packages` and prints `packagePath`, `checksumSha256`, and the manifest object expected by `POST /api/admin/plugins/packages`.

## Runtime Notes

The manifest declares `media.read` and `metadata.write`. `media.read` is used to load public item details and external IDs; `metadata.write` is required for marker replacement. The server validates marker type, tick ranges, duplicate `(markerType,startTicks)` pairs, confidence values, and the 512-marker Host API limit before sending the write request.

If `PLUGIN_SECRET_KEY` is configured on the host, set the same value in this process as `PLUGIN_SECRET_KEY` so the example can verify HTTP plugin signatures before writing markers.
