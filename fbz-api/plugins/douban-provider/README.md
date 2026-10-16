# 豆瓣 Provider（官方元数据 Provider · 最小可用）

可装包身份：`fbz.provider.douban` · 订阅 `metadata.provider.query`。

豆瓣无稳定公开开放 API，且强反爬。本包策略：

| 步骤 | 接口 | 说明 |
| --- | --- | --- |
| 搜索 | `movie.douban.com/j/subject_suggest?q=` | 无 key；可能 403/验证码 |
| 详情（优先） | `api.douban.com/v2/movie/subject/{id}?apikey=` | 需配置 `api_key`（旧 v2 apikey，限额/可用性自担） |
| 详情（fallback） | `m.douban.com/rexxar/api/v2/subject/{id}` | 无 key best-effort |
| 失败 | — | 返回 `{ metadata: null }`，不抛死刮削 |

## 合同字段

能填则填：`title, originalTitle, overview, productionYear, communityRating, externalIds(douban[,imdb]), artwork, genres, people`。

## 密钥配置

| 来源 | 键 | 说明 |
| --- | --- | --- |
| `configSchema` | `api_key`（secret，可选） | Douban API v2 apikey；仅详情增强 |
| `configSchema` | `language` | 默认 zh-CN（提示项） |
| 环境 | `DOUBAN_API_KEY` | 同上 |

## 本地运行 + curl 冒烟

```bash
cd fbz-api
PORT=19105 node plugins/douban-provider/server.mjs
# 可选：DOUBAN_API_KEY=… 增强详情
```

```bash
curl -sS -X POST 'http://127.0.0.1:19105/fbz-plugin' \
  -H 'content-type: application/json' \
  -H 'x-fbz-plugin-id: fbz.provider.douban' \
  -H 'x-fbz-plugin-idempotency-key: smoke-douban-1' \
  -d '{
    "invocation": "sync",
    "hookEvent": "metadata.provider.query",
    "request": {
      "lookup": { "itemType": "movie", "title": "辛特尔", "productionYear": 2010, "language": "zh-CN" }
    }
  }'
```

期望：成功时 `externalIds` 含 `douban`；若上游反爬，`metadata: null`（属预期，见上表）。

## 打包

`./scripts/package-plugin.ps1 -PluginDir plugins/douban-provider -Force`

## 合规 / 反爬

仅供自建媒体库 enrichment；请遵守豆瓣 ToS 与本地法规，控制频率。生产建议自备合规镜像或代理，并在插件配置中指向（可通过将来扩展 `api_base_url`）。
