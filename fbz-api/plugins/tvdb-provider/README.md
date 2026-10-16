# TVDB Provider（官方元数据 Provider）

可装包身份：`fbz.provider.tvdb` · `runtime: http` · 订阅 `metadata.provider.query`。

真实实现：TVDB v4 `POST /login` → `GET /search` → `GET /{movies|series}/{id}/extended`，对齐 core `tvdb.rs`。

## 合同字段

同 TMDB 插件白名单（见 `plugins/tmdb-provider/README.md`）。`externalIds` 含 `tvdb` 及 remote imdb/tmdb。

## 密钥配置

| 来源 | 键 |
| --- | --- |
| `configSchema` | `api_key`（secret）；可选 `api_base_url`、`language` |
| 环境 | `TVDB_API_KEY`（默认 base `https://api4.thetvdb.com/v4`） |

## 本地运行 + curl 冒烟

```bash
cd fbz-api
PORT=19102 TVDB_API_KEY=… node plugins/tvdb-provider/server.mjs
```

```bash
curl -sS -X POST 'http://127.0.0.1:19102/fbz-plugin' \
  -H 'content-type: application/json' \
  -H 'x-fbz-plugin-id: fbz.provider.tvdb' \
  -H 'x-fbz-plugin-idempotency-key: smoke-tvdb-1' \
  -d '{
    "invocation": "sync",
    "hookEvent": "metadata.provider.query",
    "request": {
      "lookup": {
        "itemType": "series",
        "title": "Game of Thrones",
        "productionYear": 2011
      }
    }
  }'
```

期望：`result.metadata.title` + `externalIds` 含 tvdb。

## 打包

`./scripts/package-plugin.ps1 -PluginDir plugins/tvdb-provider -Force`
