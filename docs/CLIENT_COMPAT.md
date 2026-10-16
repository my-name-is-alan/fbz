# 客户端兼容矩阵（Emby 协议）

日期：2026-10-15。目标：追踪 Infuse / VidHub / SenPlayer / FBZ Web 等客户端在 Emby 兼容链路上的实测状态。  
行为可借鉴 go-emby / Lux 的公开协议时序，**不复制其源码**；Lux 为非商业许可，本仓库实现保持独立。

## 协议链路（每列）

| 阶段 | 典型入口 | 服务端要点 |
|------|----------|------------|
| 登录 | `POST /emby/Users/AuthenticateByName` | MediaBrowser / `X-Emby-Authorization`；签发 AccessToken |
| Views | `GET /emby/Users/{Id}/Views` | 用户可见媒体库 / CollectionType |
| Items | `GET /emby/Users/{Id}/Items`、`Items/{Id}` | 浏览、详情、`MediaSources` 摘要 |
| PlaybackInfo | `GET\|POST …/PlaybackInfo` | 选源、`DirectStreamUrl` / 转码 URL、PlaySessionId |
| stream / 302 | `GET /emby/Videos\|Audio/{Id}/stream…` | 鉴权后本地直读，或 STRM / 云盘 **302**（不中继视频体） |
| 进度 | `Sessions/Playing`、`Progress`、`Stopped` | PositionTicks、PlayMethod、心跳 |
| 字幕 | `Videos/{Id}/{MsId}/Subtitles/{Index}/Stream…` | 内嵌 / 外挂；STRM 源可能无本地字幕文件 |

状态约定：**未测** = 尚无该客户端实机或自动化证据；**部分** = 链路部分阶段通过或仅有模拟客户端；**通过** = 有可复现证据（留链接到 issue / 冒烟记录）。

## 目标客户端矩阵

| 客户端 | 登录 | Views | Items | PlaybackInfo | stream/302 | 进度 | 字幕 | 备注 |
|--------|------|-------|-------|--------------|------------|------|------|------|
| FBZ Web（Shaka） | 部分 | 部分 | 部分 | 部分 | 部分 | 部分 | 未测 | 本地 MP4 直放闭环见 `docs/PLAYBACK_LOOP_20261004.md`；≠ 原生客户端认证 |
| Infuse | 未测 | 未测 | 未测 | 未测 | 未测 | 未测 | 未测 | iOS/tvOS；关注 DirectPlay / STRM 302 / 外挂字幕 |
| VidHub | 未测 | 未测 | 未测 | 未测 | 未测 | 未测 | 未测 | 关注 PlaybackInfo query-only、容器名（mkv vs matroska） |
| SenPlayer | 未测 | 未测 | 未测 | 未测 | 未测 | 未测 | 未测 | 关注 MediaBrowser 头与进度上报形态 |
| Emby 官方 / 模拟客户端 | 部分 | 部分 | 部分 | 部分 | 部分 | 部分 | 部分 | 路由与 DTO 单测覆盖较多；非商店 App 实机 |
| Jellyfin 客户端（误连） | 未测 | 未测 | 未测 | 未测 | 未测 | 未测 | 未测 | 非目标；仅作差异观察时可记 |

## STRM / 云盘 302 约定（与实现一致）

1. **先鉴权，后 302**：`stream` / `Download` / `File` 等入口必须先通过用户与媒体可见性校验，再解析跳转目标。
2. **不中继视频体**：FBZ 只返回 `302 Location` + `Cache-Control: no-store`；Range / 206 由客户端与上游 CDN 协商。
3. **STRM Resolver 扩展点**：在既有 allowlist 302 之前可挂解析器（见 `fbz-api/src/media/strm.rs`）。首版 stub「直读 URL」只透出 `.strm` 已存目标；网盘临时链解析另接，不把上游密钥写入 STRM 或播放地址。
4. **安全**：默认内网 STRM；公网域名须在 `STRM_ALLOWED_DOMAINS`；禁止把鉴权头转发到 CDN。

## DeviceProfile → PlayMethod

决策表骨架见 `fbz-api/src/playback/play_method.rs`（DirectPlay / Remux≈DirectStream / Transcode）。  
PlaybackInfo 从请求 `DeviceProfile` / `ClientCapabilities.DeviceProfile` 解析 summary，驱动 MediaSource `SupportsDirect*` 与 Transcode 入队（无提示时仍默认直放）。

## 容器名

MediaSource 对外容器名做规范化（如 ffprobe `matroska` → `mkv`），减少 Infuse/VidHub 等对扩展名敏感的客户端误判。实现：`fbz-api/src/media/container.rs`。

## 更新方式

实机或冒烟通过后，将对应单元格改为 **通过**，并在「备注」或下方「证据」追加日期、版本与路径（脚本 / 截图 / issue）。勿把「模拟 Emby 协议」记成原生客户端通过。

### 证据（初始）

- 2026-10-04：Web + 模拟协议本地 MP4 闭环 — `docs/PLAYBACK_LOOP_20261004.md`
- 2026-10-15：本矩阵初稿；STRM resolver stub、PlayMethod 决策骨架、容器规范化落地
