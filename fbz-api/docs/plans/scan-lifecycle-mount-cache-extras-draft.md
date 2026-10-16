# 统一扫描生命周期事件 + 挂载缓存 / Extra 最小集（P1 草稿）

状态：**可验收草稿**（常量 / 类型 / 规则 + 本文）。**未**扩 `plugin_hooks` allow-list、**未**改 `storage/*` 落盘逻辑、**未**实现预告片下载、**未**改鉴权。

对齐真源：`docs/FBZ-目标架构与落地路线.md` §6.1 / §6.5 / §6.8 / §10 P1。

冲突规避：StorageProvider / `storage/*` 同事并行时，本草稿落在独立 mod + 本文；不改 `storage/settings.rs`、不写 `storage_mounts` 新列迁移。

代码落点：

| 域 | 路径 |
| --- | --- |
| 扫描阶段 / 事件名 | `fbz-api/src/scan/lifecycle.rs` |
| 挂载缓存 stub | `fbz-api/src/cache/mount.rs` |
| Extra 类型 + 扫描规则 | `fbz-api/src/media/extras.rs` |

---

## 1. 扫描生命周期阶段（与 §6.1 对齐）

| 阶段 | `ScanStage` | 说明 |
| --- | --- | --- |
| Discover | `discover` | 列出候选（本地 walk / StorageProvider.list） |
| Observe | `observe` | size/mtime/remote version → 未变则跳过 |
| Recognize | `recognize` | `recognition::recognize`（+ 可选 path_hint） |
| Upsert | `upsert` | `media_items` / `media_files` |
| Probe | `probe` | `media.probe` |
| Match | `match` | `metadata.refresh`（NFO + Provider 链） |
| Artwork | `artwork` | 海报等缓存拉取 / photo job |
| Extras | `extras` | Extra 关联或可选缓存（§6.8） |
| Emit | `emit` | 作业级 `library.scan.*` |
| PostScan | `post_scan` | 插件 `library.post_scan`（P1 目标） |
| Markers | `markers` | `media.marker.detect`（P2，不阻塞） |

本地 `scan/service.rs` 与云 `storage/scan.rs` 目标态都应对齐同一阶段语义；本轮只冻结名称，不改 worker。

---

## 2. 事件名收敛 + 与 `PluginHookEvent` 映射

### 2.1 已接线（可订阅 / 已在 allow-list）

| 事件键 | 现网用途 | 阶段 |
| --- | --- | --- |
| `library.scan.started` | 扫描作业开始 | Emit |
| `library.scan.completed` | 扫描作业成功（含续扫摘要） | Emit |
| `library.scan.failed` | 扫描作业失败 | Emit |
| `media.item.created` | 条目创建 | Upsert |
| `media.item.updated` | 条目更新 | Upsert |
| `metadata.refresh.completed` | 刮削刷新成功 | Match |
| `metadata.refresh.failed` | 刮削刷新失败 | Match |

现存常量：`scan/service.rs` 的 `LIBRARY_SCAN_{STARTED,COMPLETED,FAILED}_EVENT`；manifest allow-list：`plugins/manifest.rs::SUPPORTED_HOOK_EVENTS`；DB：`plugin_hooks_event_key_allowed`（最近一次扩在 `0078_…`）。

### 2.2 草稿细粒度事件（**尚未**进 allow-list）

命名约定：`library.scan.<stage_snake>`，与现有 `library.scan.*` 同族，便于监控与插件订阅。

| 草稿事件键 | 用户口语 | 建议触发点 | 与现网映射（过渡） |
| --- | --- | --- | --- |
| `library.scan.discovered` | discovered | Discover 批结束（本批路径列表摘要） | 无直接等价；可先只记 job_events |
| `library.scan.probe_done` | probe_done | 单文件 / 批 `media.probe` 成功落流 | 无 hook；job 完成后可发 |
| `library.scan.metadata_matched` | metadata_matched | Match 链命中并合并完成 | **过渡**：复用 `metadata.refresh.completed`，payload 带 `scanId`/`libraryId` |
| `library.scan.artwork_done` | — | Artwork 批完成 | 无 |
| `library.scan.extras_linked` | — | Extra 关联写入 | 无 |
| `library.post_scan` | — | 成功扫描后的插件短任务扩展点（§5.2） | 无；P1 扩 allow-list |

**收敛原则：**

1. **作业边界**继续只用 `library.scan.{started,completed,failed}` 作为对外稳定 Hook（已有示例插件订阅）。
2. **细粒度**先以 `ScanLifecycleEvent` 常量存在；默认进 `job_events` / 内部审计，再择要升格为 `PluginHookEvent`。
3. `metadata_matched` **不**另造平行刮削完成语义：升格前映射到 `metadata.refresh.completed`。
4. `probe_done` 升格前不虚构 `media.probe.completed` hook，避免与 job_type 混淆；若将来要插件订阅探针，再单独评审键名。
5. 升格任一草稿键 = 迁移重写 `plugin_hooks_event_key_allowed` + 同步 `SUPPORTED_HOOK_EVENTS`（同 `0078` / marker 草稿模式）。

### 2.3 云扫描桥接（语义，非本轮实现）

`storage/scan.rs` 状态机（`scanning` → `importing` → `idle`）应对齐：

- 挂载刷新开始 → 视作 `library.scan.started`（aggregate=`library` 或未来 `storage_mount`）
- list 批入库 → `library.scan.discovered`
- import 视频项 → Upsert + 既有 item 事件
- 整轮 idle → `library.scan.completed`；`failed` → `library.scan.failed`

本轮不改 `storage/scan.rs`（避冲突）。

---

## 3. 挂载缓存 stub（§6.5）

### 3.1 种类（`MountCacheKind`）

| Kind | 对应挂载选项（现状 / 草稿） | 默认建议 |
| --- | --- | --- |
| `nfo` | 现状 `nfo_source`（`cloud`/`filename`）+ 将来「是否落盘」开关 | 偏保守，按需 |
| `image` | 现状 `image_cache`（`prefetch`/`on_demand`） | 可 prefetch |
| `trailer` | **草稿** `trailer_cache`（未落库） | 默认关 |

开关只决定是否落盘副本；Extra 关联见 §4。容量/LRU 与 `ARTWORK_CACHE_DIR` 护栏一致（架构 §8.4）。

### 3.2 Key 约定

```text
fbz:mount-cache:{mount_id}:{kind}:{content_key}
```

- `mount_id`：`storage_mounts.id`（uuid 文本）
- `kind`：`nfo` | `image` | `trailer`
- `content_key`：稳定相对键，优先 `sha256(remote_id + ':' + version)`；无 version 时用 `sha256(remote_id + ':' + name + ':' + size)`
- 进程内索引键与 Redis 键同一格式；**落盘路径**草案：`{MOUNT_CACHE_DIR}/{mount_id}/{kind}/{content_key[:2]}/{content_key}`（`MOUNT_CACHE_DIR` 未定，可并列 `ARTWORK_CACHE_DIR`）

### 3.3 失效触发点（注释级，见 `cache/mount.rs`）

1. 挂载删除 / 解绑库  
2. `storage_mounts.generation` 递增且旧 generation 条目对账删除  
3. 管理员关闭对应缓存种类  
4. 远端 `version` / size+mtime 变化  
5. 容量上限或 LRU 驱逐  
6. 账号 disconnect / `login_epoch` 变更（凭据失效后禁读旧密文缓存元数据）

**本轮不**实现下载、不改 settings API、不加列。

---

## 4. Extra 最小集（§6.8）

### 4.1 类型枚举（`ExtraType`）

对齐 Emby/JF 常见 Extra，**不**扩 `media_items.item_type` CHECK（避 taxonomy 冲突）；草稿以独立枚举 + 将来关联表/字段表达。

| 值 | 含义 | 扫描提示（文件名/目录） |
| --- | --- | --- |
| `trailer` | 预告片 | `-trailer`、`.trailer.`、目录 `trailers/` |
| `theme_song` | 主题曲 | `theme.`、目录 `theme-music/` / `theme/`（音频） |
| `theme_video` | 主题视频 | `theme-video`、目录 `backdrops/` 内视频（弱提示） |
| `behind_the_scenes` | 花絮 | `behindthescenes`、`-behindthescenes`、目录 `behind the scenes/` |
| `interview` | 采访 | `-interview`、目录 `interviews/` |
| `scene` | 删减/选场 | `-scene`、目录 `scenes/` |
| `sample` | 样片 | `-sample`、`.sample.`、目录 `samples/` |
| `short` | 短片 | `-short`、目录 `shorts/` |
| `clip` | 片段 | `-clip`、目录 `clips/` |
| `featurette` | 专题短片 | `-featurette`、目录 `featurettes/` / `extras/` |
| `deleted_scene` | 删减场景 | `-deleted`、`deletedscene`、目录 `deleted scenes/` |
| `other` | 未识别额外视频 | 目录 `extras/` 内未命中更具体类型 |

### 4.2 扫描忽略 / 收录规则（草稿）

**忽略为主内容（勿当 Movie/Episode 入库）：**

- 文件名含独立 token：`sample`、`samples`、`trailer`、`trailers`（作主片 stem 时）
- 目录名（祖先）：`extras`、`trailers`、`featurettes`、`samples`、`theme-music`、`interviews`、`behind the scenes`、`deleted scenes`、`clips`、`shorts`
- 扩展名非媒体的 sidecar（`.nfo` / 图片 / 字幕）继续由现网 discover 过滤，不进 Extra 关联

**收录为 Extra（关联到邻近主条目，不单独当主片）：**

1. 路径命中上表提示 → 得到 `ExtraType`  
2. 主条目解析：同目录「去掉 extra 后缀的主片」、或上一级剧集/电影文件夹（细节留给 recognition 词典增强）  
3. **不**在刮削同步路径默认定时下载远端 trailer；仅当挂载 `trailer` 缓存打开时落盘（§3）

`sample`：默认可配置为「忽略不入库」或「录为 ExtraType::Sample」；草稿 API 用 `SampleDisposition::{Ignore,AsExtra}`。

### 4.3 非目标（本轮）

下载实现、Emby `LocalTrailers` 填实、DB 迁移、`ItemType::Trailer`、鉴权变更。

---

## 5. 验收清单（本草稿）

- [x] 生命周期阶段与事件名有单一常量模（`scan/lifecycle.rs`）  
- [x] 与现网 `PluginHookEvent` / allow-list 映射写清（本文 §2）  
- [x] 挂载缓存 key / kind / 失效点 stub（`cache/mount.rs`）  
- [x] Extra 类型枚举 + 忽略/收录纯函数（`media/extras.rs`）  
- [x] 不改鉴权；不 commit；避开 `storage/*` 与 hook 迁移冲突  

## 6. 后续落地顺序

1. 云扫描桥接发 `library.scan.{started,completed,failed}`（改 `storage/scan.rs` 时与 StorageProvider 同学协调）  
2. 择要升格 `discovered` / `probe_done` / `metadata_matched` → 迁移 + `SUPPORTED_HOOK_EVENTS`  
3. `trailer_cache` 列或 JSON 选项 + 真实落盘与 LRU  
4. Extra 关联表 + recognition 词典 + Emby LocalTrailers 填实  
