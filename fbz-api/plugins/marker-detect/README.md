# Marker Detect（片头 / 片尾探测）

可装包身份：`fbz.marker.detect` · `runtime: http` · 订阅 `media.marker.detect`。

Core worker（`media/marker_detect.rs`）用本机 **FFmpeg / ffprobe** 收集：

- 内嵌章节标题与时间（`ffprobe -show_chapters`）
- 片头/片尾窗口的静音与黑场（`silencedetect` / `blackdetect`，默认头 180s / 尾 240s）

摘要以 `probeSummary` 同步下发；**不含裸存储路径**。本插件做章节名 + 静音/黑场启发式，回传 `intro_*` / `credits_*`（可选 `chapter`），由 core 写入 `media_markers`。

## 依赖

- Host：`ffmpeg` + `ffprobe`（`FFMPEG_PATH` / `FFPROBE_PATH` 或 `PATH` / 内置 `vendor/ffmpeg`）
- 本进程：Node.js 18+（仅 HTTP runtime，不直接调 ffmpeg）

## 本地运行

```bash
cd fbz-api
node plugins/marker-detect/server.mjs
# 可选：FBZ_MARKER_DETECT_LOG=1 观察请求摘要
```

默认入口：`http://127.0.0.1:19106/fbz-plugin`（需在 `PLUGIN_HTTP_ALLOWED_HOSTS` 内）。

## Host 作业闭环

1. 安装并审批/启用本包（hooks 含 `media.marker.detect`）。
2. 启动 API，并打开 worker：`FBZ_MARKER_DETECT_WORKER_ENABLED=true`。
3. 触发探测：

```http
POST /api/admin/media-items/{itemId}/marker-detect
{"reason":"manual","force":true}
```

4. 查询结果：

```http
GET /api/admin/media-items/{itemId}/markers
```

Emby 详情 `Fields=Chapters` 会映射 `media_markers`（IntroStart / CreditsStart 等）。

## 打包

```powershell
./scripts/package-plugin.ps1 -PluginDir plugins/marker-detect -Force
```

## 已知限制

- 启发式保守：无章节名且无头/尾静音黑场时可能返回空列表。
- strm / `fbz-storage://` 云路径跳过 ffmpeg，仅在有章节摘要时可能命中。
- 不做指纹云 / 跨集对拍；全片扫描不在范围内（只扫头尾窗口）。
- 同步调用受 `PLUGIN_SYNC_TIMEOUT_MS` 约束；重分析在 core worker，插件侧应保持快速。

## 非目标

- 改 Emby 鉴权网关
- 删除 `examples/plugins/http-marker-importer`
