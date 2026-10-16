# Fanart Provider（官方元数据 Provider · 图片向 Enrichment）

可装包身份：`fbz.provider.fanart` · 订阅 `metadata.provider.query`。

真实实现：`GET /movies/{tmdbId}` 或 `/tv/{tvdbId}`（`api_key` query），字段映射对齐 core `fanart.rs`。  
**必须**已有 TMDB（电影）或 TVDB（剧）id：来自 `lookup.tmdbId` / `lookup.tvdbId` 或 `request.current.externalIds`。

## 合同字段

主要贡献 `artwork[]`（poster/backdrop/logo/thumb/banner）；附带对应 `externalIds`。`disc` 映射为 `thumb`（宿主 allowlist 无 disc）。

## 密钥配置

| 来源 | 键 |
| --- | --- |
| `configSchema` | `api_key`（secret）；可选 `api_base_url` |
| 环境 | `FANART_API_KEY` |

## 本地运行 + curl 冒烟

```bash
cd fbz-api
PORT=19103 FANART_API_KEY=… node plugins/fanart-provider/server.mjs
```

```bash
curl -sS -X POST 'http://127.0.0.1:19103/fbz-plugin' \
  -H 'content-type: application/json' \
  -H 'x-fbz-plugin-id: fbz.provider.fanart' \
  -H 'x-fbz-plugin-idempotency-key: smoke-fanart-1' \
  -d '{
    "invocation": "sync",
    "hookEvent": "metadata.provider.query",
    "request": {
      "lookup": { "itemType": "movie", "title": "Sintel", "tmdbId": "45745", "language": "zh-CN" },
      "current": { "provider": "tmdb", "externalId": "45745", "externalIds": [{"provider":"tmdb","externalId":"45745"}] }
    }
  }'
```

期望：`result.metadata.artwork` 非空。缺 id / 缺 key → `metadata: null`。

## 打包

`./scripts/package-plugin.ps1 -PluginDir plugins/fanart-provider -Force`
