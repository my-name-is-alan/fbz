# 片头 / 片尾探测（media.marker.detect）

状态：**已落地最小闭环**（hook allow-list + job/worker + 官方插件启发式 + admin 触发 + Emby Chapters 映射）。

对齐真源：架构 §6.7 / §5.2 / §8.3 / §10。

## 1. 职责划分

| 层 | 职责 |
| --- | --- |
| Core | `media_markers` 持久化（`intro_start` / `intro_end` / `credits_start` / `credits_end` / `commercial` / `chapter`）；FFmpeg/ffprobe 收集 `probeSummary`；作业 `media.marker.detect`；compat Chapters 映射 |
| 插件 | 探测启发式（章节名 / 静音黑场）；经扩展点回传区间；正式数据进 core markers |
| 非目标（本轮） | 指纹云；扫描默认同步入队；鉴权网关变更 |

## 2. 扩展点

| 项 | 值 |
| --- | --- |
| 事件键 | `media.marker.detect`（`SUPPORTED_HOOK_EVENTS` + 迁移 `0102_media_marker_detect_hook.sql`） |
| 调用形态 | 同步短结果（`PluginSyncInvoker`）；core 先跑 ffmpeg 摘要，插件只做映射 |
| 权限 | `media.analyze`（高风险）；可另申 `media.read` / `metadata.write` |
| 输入 | `{ mediaItemId, mediaFileId, durationMs, contentFingerprint?, probeSummary?, locale? }` — **不含**裸路径 |
| 输出 | `{ markers: [{ markerType, startTicks, endTicks?, confidence?, title? }], detectorVersion }` |
| 失败 | 熔断跳过；无订阅插件时 core 使用同语义启发式回退 |

## 3. Job

| 项 | 值 |
| --- | --- |
| `job_type` | `media.marker.detect` |
| 触发 | `POST /api/admin/media-items/{itemId}/marker-detect`（`force` 可重算） |
| Worker | `FBZ_MARKER_DETECT_WORKER_ENABLED=true`；`FBZ_MARKER_DETECT_MAX_CONCURRENT`（默认 1） |
| 查询 | `GET /api/admin/media-items/{itemId}/markers` |

## 4. 官方包

- 路径：`plugins/marker-detect/`
- id：`fbz.marker.detect`
- hooks：`media.marker.detect` → `markers.detect`

## 5. 怎么跑

```bash
# 1) 依赖：PATH 或 FFMPEG_PATH / FFPROBE_PATH 可用
ffmpeg -version && ffprobe -version

# 2) 插件进程
cd fbz-api && node plugins/marker-detect/server.mjs

# 3) API + worker（另开终端）
export FBZ_MARKER_DETECT_WORKER_ENABLED=true
cargo run

# 4) 安装/审批/启用插件包后：
curl -X POST "$API/api/admin/media-items/$ITEM_ID/marker-detect" \
  -H "Authorization: Bearer $TOKEN" -H 'content-type: application/json' \
  -d '{"reason":"manual","force":true}'

curl "$API/api/admin/media-items/$ITEM_ID/markers" -H "Authorization: Bearer $TOKEN"
```

打包：`./scripts/package-plugin.ps1 -PluginDir plugins/marker-detect -Force`

## 6. 已知限制

- 只分析头 ~180s / 尾 ~240s；全片指纹对拍不做。
- strm / 云 `fbz-storage://` 跳过 ffmpeg（无本地样本）。
- 章节名启发式可能误匹配含 “intro” 的非片头章；置信度可配 `min_confidence`。
- 静音/黑场估 intro/credits 偏保守，短剧/无间隙片头可能空结果。
- 不改鉴权网关；本变更默认不 commit。

## 7. 文件索引

- Core worker：`src/media/marker_detect.rs`
- Hook 迁移：`migrations/0102_media_marker_detect_hook.sql`
- Admin：`POST .../marker-detect`、`GET .../markers`
- 插件：`plugins/marker-detect/`
- Emby Chapters：`library/repository.rs` + `compat/emby/routes/items.rs` 详情填充
