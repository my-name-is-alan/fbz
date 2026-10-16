# IMDb Provider（官方元数据 Provider · 最小可用）

可装包身份：`fbz.provider.imdb` · 订阅 `metadata.provider.query`。

IMDb **无**官方公开 JSON API。本包策略：

| 模式 | 条件 | 行为 |
| --- | --- | --- |
| OMDb | `api_key` / `IMDB_API_KEY` 已配置 | `omdbapi.com` 按 id / 标题搜索 + 详情（推荐，合规自持 key） |
| Suggest fallback | 无 key 或 `data_source=suggest` | `v2.sg.media-imdb.com/suggestion/...` 轻量搜索（可能限流/变更） |
| Normalize-only | 仅有 `imdbId`、上游失败 | 规范化为 `tt#######` 写入 `externalIds` |

## 合同字段

能填则填：`title, overview, productionYear, premiereDate, communityRating, officialRating, externalIds(imdb), artwork(poster), genres, people`。

## 密钥配置

| 来源 | 键 | 说明 |
| --- | --- | --- |
| `configSchema` | `api_key`（secret，可选） | **OMDb** API key（不是 IMDb 官方 key） |
| `configSchema` | `data_source` | 可选 `omdb`（默认有 key 时）或 `suggest` |
| 环境 | `IMDB_API_KEY` | 同 OMDb key；`IMDB_DATA_SOURCE=suggest` 强制建议接口 |

申请 OMDb：https://www.omdbapi.com/apikey.aspx

## 本地运行 + curl 冒烟

```bash
cd fbz-api
# 推荐（OMDb）
PORT=19104 IMDB_API_KEY=… node plugins/imdb-provider/server.mjs

# 或无 key（suggest fallback，可能不稳定）
PORT=19104 node plugins/imdb-provider/server.mjs
```

```bash
curl -sS -X POST 'http://127.0.0.1:19104/fbz-plugin' \
  -H 'content-type: application/json' \
  -H 'x-fbz-plugin-id: fbz.provider.imdb' \
  -H 'x-fbz-plugin-idempotency-key: smoke-imdb-1' \
  -d '{
    "invocation": "sync",
    "hookEvent": "metadata.provider.query",
    "request": {
      "lookup": { "itemType": "movie", "title": "Sintel", "productionYear": 2010, "imdbId": "tt1727587" }
    }
  }'
```

期望：`externalIds` 含规范化 `tt…`；有 OMDb key 时还有 overview/rating。

## 打包

`./scripts/package-plugin.ps1 -PluginDir plugins/imdb-provider -Force`

## 合规说明

请自备 OMDb（或其它合规源）密钥。Suggest 仅为开发/冒烟 fallback，生产请配置 key。
