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

一等通知模板位于：

- `examples/plugins/telegram-notifier-template`：默认 channel 为 `telegram`。
- `examples/plugins/wecom-notifier-template`：默认 channel 为 `wecom`。
- `examples/plugins/webhook-notifier-template`：默认 channel 为 `webhook`。

这些模板都只声明 `notification.send`，配置项只包含逻辑 channel、标题前缀和是否附带截断 payload；真实 Telegram、企业微信和 webhook 目标仍通过管理员通知目标配置管理。

## Marker 导入插件推荐边界

片头片尾、章节和广告 marker 建议走 `metadata.write` 权限：

1. 插件接收 `metadata.refresh.completed`。
2. 插件用 `GET /api/plugin/items/{itemId}` 读取公开 metadata、外部 ID、集季号。
3. 插件从外部 TiDb/章节数据源解析 marker。
4. 插件用 `PUT /api/plugin/items/{itemId}/markers` 写入自己的 marker source。

Host API 会限制 marker 类型、tick 范围、confidence、重复项和单次写入数量。插件只能替换自己 source 下的 marker，不会覆盖核心或其他插件的数据。

## WASI 运行时契约与模板

`wasi` runtime 适合**沙箱内纯计算**（标签归一化、marker/摘要计算、payload 转换）。WASIp1 没有 socket，
**WASI 插件无法访问网络或调用 Host API**——需要联网 / Host API 的插件请用 `http` runtime（见上文）。
这也是「联网插件继续优先 HTTP runtime」的原因。

宿主以 wasmtime + WASIp1 把模块当作命令（`_start`）每次 dispatch 调用一次，契约如下：

| 通道 | 含义 |
| --- | --- |
| `argv[0]` / `argv[1]` | entrypoint 路径 / 派发的 handler（如 `hooks.onScanCompleted`） |
| env `FBZ_PLUGIN_ID` / `FBZ_PLUGIN_HANDLER` / `FBZ_PLUGIN_IDEMPOTENCY_KEY` | dispatch 上下文 |
| env `FBZ_HOST_BASE_URL` / `FBZ_PLUGIN_TOKEN` | Host API 基址 + 短期 token（仅 HTTP runtime 可用） |
| stdin | 派发的事件 payload（JSON） |
| stdout | 执行响应（被捕获，受 `PLUGIN_WASI_STDIO_MAX_BYTES` 限制） |
| 预挂载 `/plugin` | 只读插件包目录 |
| 预挂载 `/data`、`/cache` | 读写持久目录 |
| 预挂载 `/tmp` | 读写单次执行临时目录 |

执行受 fuel（`PLUGIN_WASI_FUEL`）、内存（`PLUGIN_MEMORY_LIMIT_MB`）、epoch 超时（`PLUGIN_TIMEOUT_MS`）、
模块大小（`PLUGIN_WASI_MAX_MODULE_BYTES`）和 stdio 上限约束。

WASI manifest 示例（`entrypoint` 是包内 `.wasm` 的相对路径，不是 URL；纯计算插件可不声明权限）：

```json
{
  "id": "dev.example.wasi.logger",
  "name": "Example WASI Plugin",
  "version": "0.1.0",
  "apiVersion": "1",
  "runtime": "wasi",
  "entrypoint": "plugin.wasm",
  "permissions": [],
  "hooks": [
    { "event": "library.scan.completed", "handler": "hooks.onScanCompleted" }
  ]
}
```

最小 Rust 入口（stdin → stdout）：

```rust
use std::io::{self, Read, Write};

fn main() {
    let handler = std::env::args().nth(1).unwrap_or_default();
    let plugin_id = std::env::var("FBZ_PLUGIN_ID").unwrap_or_default();
    let mut payload = String::new();
    let _ = io::stdin().read_to_string(&mut payload);
    let event: serde_json::Value = serde_json::from_str(&payload).unwrap_or(serde_json::Value::Null);
    let result = serde_json::json!({ "plugin": plugin_id, "handler": handler, "receivedEvent": event });
    let _ = writeln!(io::stdout(), "{result}");
}
```

构建为 wasm（产物即 manifest 的 `entrypoint`）：

```powershell
rustup target add wasm32-wasip1
cargo build --release --target wasm32-wasip1   # 产出 plugin.wasm
```

完整可构建模板见 `examples/plugins/wasi-scan-logger-template/`（含 `manifest.json`、`Cargo.toml`、
`src/main.rs`、`README.md`）。该模板有两层校验：`manifest.rs` 的 `first_party_wasi_scan_logger_manifest_is_valid`
用真实校验器验证 manifest（随 `cargo test` 运行）；`wasi.rs` 的 `wasi_scan_logger_template_executes_end_to_end`
把编译出的 `plugin.wasm` 经真实 `PluginWasiRuntime::execute` 跑通(stdin→stdout),标了 `#[ignore]`
以免默认 `cargo test` 依赖 `wasm32-wasip1` 目标。

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

也可以把 `PluginDir` 替换为 `examples/plugins/telegram-notifier-template`、`examples/plugins/wecom-notifier-template` 或 `examples/plugins/webhook-notifier-template` 打包对应通知模板。

脚本会输出：

- `packagePath`
- `checksumSha256`
- `manifest`

生产签名：

```powershell
$key = [byte[]]::new(32)
[System.Security.Cryptography.RandomNumberGenerator]::Fill($key)
$env:PLUGIN_SIGNING_PRIVATE_KEY_HEX = -join ($key | ForEach-Object { $_.ToString("x2") })
cargo run --bin sign-plugin-package -- --package var/plugin-packages/dev.fbz.notify.bridge-0.1.0.zip --key-id dev-key
```

签名工具会从 ZIP 根目录读取 `manifest.json`，用和安装端一致的 manifest hash 规则生成 `signature`，并输出 `publicKeyHex`。服务端生产环境配置：

```powershell
$env:PLUGIN_TRUSTED_SIGNATURE_KEYS="dev-key:<publicKeyHex>"
$env:PLUGIN_ALLOW_UNSIGNED="false"
```

安装包时把打包脚本输出的 `packagePath` / `checksumSha256`、manifest，以及签名工具输出的 `signature` 一起提交给 `POST /api/admin/plugins/packages`。

管理员通过 `POST /api/admin/plugins/packages` 安装包。生产环境建议保持：

- `PLUGIN_ALLOW_UNSIGNED=false`
- 配置 `PLUGIN_TRUSTED_SIGNATURE_KEYS`
- 提供 `ed25519:{keyId}:{signatureHex}` 包签名

## 本地验证

生命周期 smoke：

```powershell
./scripts/smoke-plugin-lifecycle.ps1 -StartServer
./scripts/smoke-plugin-lifecycle.ps1 -StartServer -SignedPackage
./scripts/smoke-plugin-lifecycle.ps1 -StartServer -SignedPackage -IncludeSchedule
```

运行时 smoke：

```powershell
./scripts/smoke-plugin-runtime.ps1 -StartServer
./scripts/smoke-plugin-runtime.ps1 -StartServer -SignedPackage
./scripts/smoke-plugin-runtime.ps1 -StartServer -ExhaustHostApiBudget
./scripts/smoke-plugin-runtime.ps1 -StartServer -SignedPackage -DeliverNotification
./scripts/smoke-plugin-runtime.ps1 -StartServer -SignedPackage -DispatchSchedule
```

生命周期 smoke 会验证安装、审批、启用、配置保存、菜单暴露和包详情归一化；追加 `-IncludeSchedule` 会让临时插件声明 `scheduler.register` 权限和 enabled-by-default interval schedule，并验证该 schedule 已同步到 Admin 计划任务列表。
运行时 smoke 会验证真实 worker、Host API、运行审计和失败重试闭环。
追加 `-SignedPackage` 会让 smoke 使用 `sign-plugin-package` 生成 Ed25519 签名，并以 `PLUGIN_ALLOW_UNSIGNED=false` 和临时 `PLUGIN_TRUSTED_SIGNATURE_KEYS` 启动 API，覆盖生产默认签名安装链路。
追加 `-ExhaustHostApiBudget` 会把 `PLUGIN_HOST_API_MAX_CALLS_PER_RUN` 降为 `1`，让临时插件第二次调用 `/api/plugin/config`，并验证超限调用返回和审计记录为 `429 too_many_requests`。
追加 `-DeliverNotification` 会给临时插件声明 `notification.send` 权限，创建管理员管理的本地 webhook 通知目标，启用通知 worker，并验证 Host API 通知请求、Admin delivery attempt 和本地 webhook 接收日志形成闭环。该选项不能和 `-ExhaustHostApiBudget` 同时使用。
追加 `-DispatchSchedule` 会给临时插件声明 `scheduler.register` 权限和 enabled-by-default interval schedule，通过 Admin manual-run 触发该插件 `plugin.schedule` task，并验证生成的 `scheduler.tick` dispatch 被 plugin worker 执行成功。

如果只改 HTTP helper 或示例插件，可先运行：

```powershell
node --test examples/plugins/_shared/fbz-plugin-http.test.mjs
node --test examples/plugins/first-party-notifier-templates.test.mjs
./scripts/smoke-plugin-signature.test.ps1
node --check examples/plugins/http-notification-bridge/server.mjs
node --check examples/plugins/http-marker-importer/server.mjs
node --check examples/plugins/telegram-notifier-template/server.mjs
node --check examples/plugins/wecom-notifier-template/server.mjs
node --check examples/plugins/webhook-notifier-template/server.mjs
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
