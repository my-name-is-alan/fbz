# 瘦 API + 按需 Worker：路径边界与本轮护栏

日期：2026-10-15。对照架构文档 §8「优先路径：瘦 API + 按需 worker」  
（`docs/FBZ-目标架构与落地路线.md`）。本文只固化**哪些 HTTP 路径必须保持瘦**、**哪些重活卸到 worker**，以及本轮已落地/已有的 page size / 并发 / 查询超时帽。

## 1. 原则

| 角色 | 职责 | 禁止 |
|------|------|------|
| **API（瘦）** | 鉴权、权限过滤 SQL、分页组装 DTO、入队 jobs / 转码 session | 请求内跑 ffprobe、刮削 RPC 长链、FFmpeg、无界扫盘/扫表 |
| **Worker（按需）** | `library.scan` / `media.probe` / `metadata.refresh` / 转码 / 插件 hook / 通知 | 在 HTTP 线程同步做同一套重活「图省事」 |
| **Scheduler** | 定时入队 | 自己执行探针/转码 |

部署：`FBZ_NODE_ROLE=api|worker|scheduler|all` + 各 `FBZ_*_WORKER_ENABLED`；`api` 常驻不跑重 worker。

## 2. 应保持瘦的路径（读库 / 入队即可）

| 路径族 | 模块 | 说明 |
|--------|------|------|
| Items 浏览 / 搜索 / Similar / Hints | `compat/emby/routes/items.rs` + `library/repository.rs` | Limit/StartIndex 帽 + lower-bound 计数；详情可补 people/extras，不触发刮削 |
| 导航 BFF | `navigation/` | 固定 `SECTION_LIMIT` / `FEATURED_LIMIT`；overview/画质批量补查 |
| PlaybackInfo | `compat/emby/routes/playback.rs` + `media/repository.rs` | 只读已落库 MediaSource/streams；需要转码时**只 create session** |
| stream / Download / STRM 302 | `compat/emby/routes/streaming.rs` | 鉴权后直读或 302；不中继视频体、不现场 probe |
| PlayMethod 决策 | `playback/play_method.rs` | 纯函数；输入为已有能力摘要 + DeviceProfile 摘要 |
| Admin / Emby 列表类 | 各 routes | 沿用 StartIndex/Limit 钳制与 bounded probe（见 backend-execution-goal） |

**PlaybackInfo 明确不做：** 同步 ffprobe、同步 metadata provider、等待 FFmpeg 出片。客户端拿到的 streams/container/bitrate 来自先前 `media.probe`（及扫描）写入。

## 3. 应卸到 worker 的路径

| 重活 | Job / 机制 | 门控 |
|------|-------------|------|
| 库扫描 / 发现 / 入库 | `library.scan` | `FBZ_SCAN_WORKER_ENABLED` |
| 容器/流探针（ffprobe） | `media.probe` | `FBZ_PROBE_WORKER_ENABLED` |
| 元数据刮削 / 刷新 | `metadata.refresh` + 插件 SyncInvoker（在 **job 上下文**） | `FBZ_METADATA_WORKER_ENABLED` |
| 转码执行（FFmpeg） | `transcoding_sessions` + transcode worker | `FBZ_TRANSCODE_WORKER_ENABLED` + `TRANSCODE_MAX_CONCURRENT` |
| 插件异步 Hook | `plugin.hook.dispatch` outbox | `FBZ_PLUGIN_WORKER_ENABLED` |
| 通知投递 | notification worker | `FBZ_NOTIFICATION_WORKER_ENABLED` |
| 片头片尾探测（P2） | marker worker（设计中） | 建议与转码分帽 |

管理员「刷新元数据 / 扫描」HTTP 入口只负责**鉴权 + 入队**，与上表一致。

## 4. 本轮热点盘点（Items / navigation / PlaybackInfo）

| 热点 | 状态 | 处理 |
|------|------|------|
| Items 缺 Limit 上限 | **已有** | `DEFAULT_ITEMS_LIMIT=100`，`MAX_ITEMS_LIMIT=200`，`MAX_ITEMS_START_INDEX=10_000`；`ItemWindow` / SearchHints / CriticReviews / Trailers 等统一钳制 |
| Items exact `count(*) over()` | **已有** | lower-bound `limit+1`（见 library repository 结构守卫） |
| Navigation 无界拉首页 | **已有固定窗** | `SECTION_LIMIT=16`，`FEATURED_LIMIT=5`，views `USER_VIEW_LIMIT=1000` |
| Navigation `list_user_view_counts` exact count | **已知缺口** | 库数有界；大库 per-library exact count 仍偏重，后续可改 bounded probe（未在本轮改语义） |
| PlaybackInfo 按版本 N+1 | **本轮已修** | `list_playback_media_sources`：1 次源 + 1 次 streams（`ANY` + per-file `row_number` 帽） |
| PlaybackInfo 版本无界 | **已有/对齐** | `MAX_PLAYBACK_MEDIA_SOURCES=32` |
| MediaStreams 无界 | **本轮已修** | `MAX_STREAMS_PER_MEDIA_FILE=64`（单源与批量共用） |
| 详情 People 无界 | **本轮已修** | `MAX_ITEM_PEOPLE=200` |
| PlaybackInfo 内同步 probe | **未发现** | probe 仅 worker；保持 |

前端对照：`fbz-fe` `LIBRARY_PAGE_SIZE=36`（小于后端最大帽），搜索默认 40。

## 5. 已有并发帽与 query 超时（文档化；本轮无改 env 默认）

| 护栏 | 默认 | 配置键 | 作用面 |
|------|------|--------|--------|
| PG `statement_timeout` | 30s | `DATABASE_STATEMENT_TIMEOUT_MS` | 单条 SQL 上限（连接 session） |
| PG 取连接超时 | 5s | `DATABASE_ACQUIRE_TIMEOUT_SECONDS` | 池耗尽背压 |
| Redis 命令超时 | 2s | `REDIS_OPERATION_TIMEOUT_MS` | readiness / 镜像等 |
| Readiness 外层超时 | 500ms | `FBZ_READINESS_TIMEOUT_MS` | `/ready` |
| 转码并发 | 3（禁止 0） | `TRANSCODE_MAX_CONCURRENT` | FFmpeg 峰值 |
| 插件 HTTP 超时 | 5s | `PLUGIN_TIMEOUT_MS` | 异步插件调用 |
| 插件同步 RPC | 10s / 每插件并发 4 | `PLUGIN_SYNC_TIMEOUT_MS` / `PLUGIN_SYNC_MAX_CONCURRENCY_PER_PLUGIN` | 刮削/存储 SyncInvoker |
| 通知投递超时 | 5s | `FBZ_NOTIFICATION_DELIVERY_TIMEOUT_MS` | 通知 worker |
| Probe worker | 单次 claim 串行 | `FBZ_PROBE_WORKER_*` | 无独立「多路并发」配置；靠多进程水平扩展 |

**本轮未改鉴权网关。** 完整环境变量表见 `fbz-api/README.md` 与 `.env.example`。

## 6. 验收点（本轮）

1. Items 列表 `Limit` 省略 → 默认 ≤100；`Limit=9999` → 响应条数 ≤200；病态 `StartIndex` 被钳到 ≤10000。
2. PlaybackInfo 多版本条目：仓储走 `list_playback_media_sources`，不再对每个 `media_file_id` 循环 `find_playback_media_source`。
3. 单文件 streams / 详情 people 查询带 SQL `limit`。
4. 本文与代码注释一致：PlaybackInfo / Items / navigation 标注为瘦路径；刮削/probe/转码指向 worker。
5. `cargo test --lib` 相关模块尽量绿；不 commit；不改 auth 网关。

## 7. 相关文件

- `fbz-api/src/compat/emby/routes/items.rs` — page size / StartIndex 帽
- `fbz-api/src/compat/emby/routes/playback.rs` — 瘦 PlaybackInfo 编排
- `fbz-api/src/media/repository.rs` — 批量 MediaSource + streams 帽
- `fbz-api/src/library/repository.rs` — people 帽；Items SQL
- `fbz-api/src/navigation/service.rs` — 首页固定窗
- `fbz-api/src/config.rs` / `.env.example` / `README.md` — 超时与并发
- `docs/FBZ-目标架构与落地路线.md` §8 — 架构源
