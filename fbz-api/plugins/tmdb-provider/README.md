# TMDB Provider（官方元数据 Provider）

可装包身份：`fbz.provider.tmdb` · `runtime: http` · 订阅 `metadata.provider.query`。

真实实现：Bearer 调 TMDB v3（search → detail/`append_to_response` → 可选 `/images` 与 season/episode 下钻），语义对齐 core `src/metadata/provider/tmdb.rs`。密钥**只**进插件配置 / 进程环境，不进 core。

## 合同字段（贡献白名单）

输入 `request.lookup`：`itemType, title, originalTitle, productionYear, season, episode, tmdbId, imdbId, tvdbId, language, country`。

输出 `{ metadata }`（camelCase）：

| 字段 | 说明 |
| --- | --- |
| `title` / `originalTitle` / `overview` | 文本 |
| `productionYear` / `premiereDate` | 年 / `YYYY-MM-DD` |
| `officialRating` / `communityRating` | 分级 / 0–10 |
| `externalIds[]` | `{ provider, externalId }`（tmdb / imdb / tvdb） |
| `artwork[]` | `{ artworkType, remoteUrl, isPrimary }`（poster/backdrop/thumb…） |
| `genres` / `studios` | 字符串数组 |
| `people[]` | `{ name, roleType, roleName?, profileImageUrl? }` |

无命中或无密钥 → `{ metadata: null }`。

## 密钥配置

| 来源 | 键 | 说明 |
| --- | --- | --- |
| 插件 `configSchema` | `access_token`（secret） | TMDB Read Access Token |
| 可选 | `api_base_url` / `image_base_url` / `language` / `include_adult` | 默认官方 URL；语言如 `zh-CN` |
| 进程环境 | `TMDB_ACCESS_TOKEN` | 本地/compose；**不要**再往 core 加深新路径 |

启用后 Host `/api/plugin/config` 优先；本地可只靠 env。

## 本地运行 + curl 冒烟

```bash
cd fbz-api
PORT=19101 TMDB_ACCESS_TOKEN=… node plugins/tmdb-provider/server.mjs
```

```bash
curl -sS -X POST 'http://127.0.0.1:19101/fbz-plugin' \
  -H 'content-type: application/json' \
  -H 'x-fbz-plugin-id: fbz.provider.tmdb' \
  -H 'x-fbz-plugin-idempotency-key: smoke-tmdb-1' \
  -d '{
    "invocation": "sync",
    "hookEvent": "metadata.provider.query",
    "request": {
      "lookup": {
        "itemType": "movie",
        "title": "Sintel",
        "productionYear": 2010,
        "language": "zh-CN"
      },
      "current": null
    }
  }'
```

期望：`ok: true`，`result.metadata` 含 `title`、`externalIds`（含 tmdb）、可选 poster。无 token 时 `metadata: null`。

经 SyncInvoker 的完整路径：安装 ZIP → 审批 → 启用 → 库源链 `plugin:fbz.provider.tmdb` → metadata.refresh。

## 打包

```powershell
./scripts/package-plugin.ps1 -PluginDir plugins/tmdb-provider -Force
```

## 非目标

删除 core 内置客户端、改鉴权网关、commit。
