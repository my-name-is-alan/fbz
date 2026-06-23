# 插件开发指南

本文面向外部插件作者，描述如何为 FBZ API 编写受控插件。系统级边界见
`docs/plugin-system.md`；这里聚焦插件作者需要遵守的开发契约。

## 设计原则

- 插件不能访问数据库、媒体文件路径、STRM 真实地址或播放 URL。
- 插件只能通过 Host API 读取或写入受控资源。
- 插件必须声明最小权限；未声明权限的 Host API 会返回 403。
- 插件收到同一个 dispatch 的重试时，必须按幂等键去重。
- 通知目标、bot token、webhook URL 等敏感出站配置由管理员管理，插件只提交通知请求。
- 大库扫描必须使用 cursor 分页，不应使用 offset 扫全库。

## 插件类型

当前推荐两类 runtime：

- `http`：适合需要联网、需要调用外部服务的插件，例如 Telegram/企业微信桥接、外部 webhook、TiDb marker 导入。
- `wasi`：适合本地纯计算或本地数据导入。WASI 当前不开放网络，只通过 stdin、环境变量和 Host API token 获得上下文。

生产环境建议优先使用 `http` runtime 承载需要网络能力的插件，把网络隔离、重试和依赖管理留在插件进程内。

## Manifest 最小形态

插件包根目录必须包含 `manifest.json`。HTTP 插件示例：

```json
{
  "id": "dev.example.notify",
  "name": "Example Notification Plugin",
  "version": "0.1.0",
  "apiVersion": "1",
  "runtime": "http",
  "entrypoint": "http://127.0.0.1:19091/fbz-plugin",
  "permissions": [
    {
      "key": "notification.send",
      "reason": "Forward selected host events to admin-managed notification targets"
    }
  ],
  "hooks": [
    {
      "event": "library.scan.completed",
      "handler": "notify"
    }
  ],
  "configSchema": [
    {
      "key": "channel",
      "label": "Notification channel",
      "type": "string",
      "required": false
    }
  ]
}
```

常见字段边界：

- `id` 使用小写命名空间，不能以 `core.` 开头。
- `apiVersion` 当前必须是 `1`。
- `runtime` 当前可用 `http` 或 `wasi`。
- `entrypoint` 对 HTTP 插件必须是 `http://` 或 `https://`，且 host 需要被 `PLUGIN_HTTP_ALLOWED_HOSTS` 放行。
- `menu` 路径只能位于 `/admin/plugins/{pluginId}` 或其子路径。
- 声明 `menu` 需要 `admin.menu` 权限。
- 声明 `schedules` 需要 `scheduler.register` 权限。

## HTTP Dispatch 请求

HTTP 插件接收来自后端 worker 的 `POST` JSON 请求。请求头包括：

- `X-FBZ-Plugin-Id`
- `X-FBZ-Plugin-Token`
- `X-FBZ-Plugin-Idempotency-Key`
- `X-FBZ-Host-Base-Url`

启用 `PLUGIN_SECRET_KEY` 后还会带签名头：

- `X-FBZ-Plugin-Signature-Version`
- `X-FBZ-Plugin-Signature-Timestamp`
- `X-FBZ-Plugin-Body-Sha256`
- `X-FBZ-Plugin-Signature`

签名 v1 canonical string：

```text
v1
{timestamp}
{pluginId}
{idempotencyKey}
{bodySha256}
```

插件应在执行副作用前完成：

1. 校验 HTTP method 和 path。
2. 校验插件 id 是否匹配自身。
3. 校验时间戳是否在允许窗口内。
4. 校验 body SHA-256。
5. 校验 HMAC-SHA256 签名。
6. 使用 `X-FBZ-Plugin-Idempotency-Key` 做幂等去重。

## Dispatch Payload

dispatch body 是 JSON 对象，核心字段包括：

```json
{
  "pluginId": "dev.example.notify",
  "packageId": "package-public-id",
  "hookId": 123,
  "handler": "notify",
  "hookEvent": "library.scan.completed",
  "source": {
    "eventType": "library.scan.completed"
  }
}
```

插件应根据 `handler` 分派逻辑，而不是只依赖 `hookEvent`。同一个 hook event 可以绑定多个 handler。

## HTTP Helper

一等 HTTP 示例复用 `examples/plugins/_shared/fbz-plugin-http.mjs`。它不依赖第三方包，提供：

- `createHttpPluginServer`：统一处理 method/path、body limit、签名校验、JSON 解析、幂等缓存和 JSON 响应。
- `hostJson`：携带 `x-fbz-plugin-token` 调用 Host API，并限制响应体大小。
- `loadPluginConfig`：读取插件配置，失败时返回 fallback。
- `readLimitedResponse`：读取外部 HTTP 响应并限制大小。

打包脚本会把该 helper 复制到插件 ZIP 根目录，文件名为 `fbz-plugin-http.mjs`。示例插件的 `server.mjs` 会先尝试加载包内 helper，开发态再 fallback 到 `../_shared/fbz-plugin-http.mjs`。

本地测试 helper：

```powershell
node --test examples/plugins/_shared/fbz-plugin-http.test.mjs
```

## Host API 调用

插件通过短期 `X-FBZ-Plugin-Token` 调用 Host API：

```http
GET /api/plugin/capabilities
X-FBZ-Plugin-Token: <token>
```

常用 API：

- `GET /api/plugin/capabilities`
- `GET /api/plugin/config`
- `GET /api/plugin/kv/{key}`
- `PUT /api/plugin/kv/{key}`
- `DELETE /api/plugin/kv/{key}`
- `GET /api/plugin/libraries`
- `GET /api/plugin/libraries/{libraryId}/items`
- `GET /api/plugin/items/{itemId}`
- `PATCH /api/plugin/items/{itemId}/metadata`
- `PUT /api/plugin/items/{itemId}/artwork`
- `PUT /api/plugin/items/{itemId}/markers`
- `POST /api/plugin/notifications`

Host Token 只在当前 execution run 有效。插件不要缓存 token 到下一次执行。

## 大库分页规则

媒体库可能达到 PB 级、几十万到百万级条目。插件读取媒体项时应使用 keyset cursor：

```http
GET /api/plugin/libraries/{libraryId}/items?limit=200
GET /api/plugin/libraries/{libraryId}/items?limit=200&cursor=<nextCursor>
```

规则：

- 优先使用 `nextCursor` 翻页。
- 不要把 `totalRecordCount` 当成精确全库数量。
- 避免 `startIndex` 扫全库；它只保留给小范围兼容场景。
- 每次处理后将 cursor 写入插件私有 KV，便于失败后续跑。

## 通知插件推荐边界

通知插件不直接保存或访问 Telegram bot token、企业微信 webhook URL、通用 webhook secret。

推荐流程：

1. 插件接收 hook dispatch。
2. 插件读取 `/api/plugin/config` 获得逻辑 channel。
3. 插件把消息提交到 `POST /api/plugin/notifications`。
4. 后端通知 worker 根据管理员配置的通知目标投递。

这样通知凭据只由管理员持有，插件无法把通知能力扩展成任意出站 HTTP。

## Marker 导入插件推荐边界

片头片尾、章节和广告 marker 建议走 `metadata.write` 权限：

1. 插件接收 `metadata.refresh.completed`。
2. 插件用 `GET /api/plugin/items/{itemId}` 读取公开 metadata、外部 ID、集季号。
3. 插件从外部 TiDb/章节数据源解析 marker。
4. 插件用 `PUT /api/plugin/items/{itemId}/markers` 写入自己的 marker source。

Host API 会限制 marker 类型、tick 范围、confidence、重复项和单次写入数量。插件只能替换自己 source 下的 marker，不会覆盖核心或其他插件的数据。

## 打包和安装

包根目录必须包含 `manifest.json`。建议结构：

```text
manifest.json
fbz-plugin-http.mjs
server.mjs
README.md
```

本地打包：

```powershell
./scripts/package-plugin.ps1 -PluginDir examples/plugins/http-notification-bridge -Force
```

脚本会输出：

- `packagePath`
- `checksumSha256`
- `manifest`

管理员通过 `POST /api/admin/plugins/packages` 安装包。生产环境建议保持：

- `PLUGIN_ALLOW_UNSIGNED=false`
- 配置 `PLUGIN_TRUSTED_SIGNATURE_KEYS`
- 提供 `ed25519:{keyId}:{signatureHex}` 包签名

## 本地验证

生命周期 smoke：

```powershell
./scripts/smoke-plugin-lifecycle.ps1 -StartServer
```

运行时 smoke：

```powershell
./scripts/smoke-plugin-runtime.ps1 -StartServer
```

运行时 smoke 会验证真实 worker、Host API、运行审计和失败重试闭环。

如果只改 HTTP helper 或示例插件，可先运行：

```powershell
node --test examples/plugins/_shared/fbz-plugin-http.test.mjs
node --check examples/plugins/http-notification-bridge/server.mjs
node --check examples/plugins/http-marker-importer/server.mjs
```

## 插件作者检查清单

- manifest 权限是否最小化。
- HTTP entrypoint host 是否需要管理员放行。
- dispatch 是否验证签名和时间戳。
- dispatch 是否按 idempotency key 去重。
- Host API token 是否只在当前请求使用。
- 大库读取是否使用 cursor。
- 写 metadata/artwork/marker 是否只写插件自己的 source。
- 失败是否返回非 2xx 让后端重试，而不是吞掉错误。
- 日志是否避免打印 token、secret、webhook URL。
