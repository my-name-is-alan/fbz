# FBZ → Claude Code 交接（C 端 UI 重写 + 真插件化）

| 字段 | 值 |
|------|-----|
| 仓库 | `github.com/my-name-is-alan/fbz` · 本地 `/Users/edy/Code/fbz` |
| 基准 commit | `27eebbb` on `main`（`feat: Jellyfin parity overnight work — plugins, books/syncplay, FE + docs`） |
| 受众 | Claude Code：接管 **C 端 UI 全量 rewrite** + **清理假插件 / 走向真插件架构** |
| 语气 | 硬约束优先；功能全集要保住；UI 与插件债务要诚实 |
| 日期 | 2026-10-15 PT |

**先读（按顺序）：**

1. `docs/FBZ-C端重设计-HDHive对齐.md`
2. `docs/hdhive-tokens.css`
3. `docs/FBZ-目标架构与落地路线.md`（执行真源）
4. `docs/JELLYFIN_PARITY_MATRIX.md`
5. `docs/ACCEPTANCE_2026-10-16.md`
6. 私仓参考：`my-name-is-alan/HDHive-web`（tokens / 组件气质）

---

## 0. 给 Claude Code 的硬约束

### 0.1 C 端 UI 当前很烂 — 必须按 HDHive 重写

用户原话级结论：**破烂**。现状问题：大圆角、emoji 图标、错位/布局破碎。  
**方向（已定）：** daisyUI + Lucide（`@lucide/vue`），视觉对齐 HDHive-web。

| Token | 值 |
|-------|-----|
| 背景 | `#09090e` |
| surface / raised / hover | `#111119` / `#171722` / `#20202c` |
| 边框 | `#282838` / strong `#3a3a50` |
| 文字 | `#eeeef6` / muted `#9c9cb2` / subtle `#707085` |
| 强调（amber） | `#d08700` → hover `#f0b100`；contrast `#0a0a0a` |
| 圆角 | sm `7px` / md `11px` / lg `16px` — **禁止** blob 超大圆角 |
| 图标 | **仅 Lucide**；**禁止 emoji** |
| 气质 | 密集媒体工作站（Emby/Jellyfin/HDHive），非营销落地页 |

权威 tokens：`docs/hdhive-tokens.css`。  
Superdesign 五屏稿（Home / Library / Movie detail / Player / Playlists）见 `docs/FBZ-C端重设计-HDHive对齐.md`（当时未改 Vue；现由 Claude Code 落地）。

**改动范围：** 只动 `fbz-fe` **C 端**（`layouts/default`、`views/{home,library,detail,playlists,search,collections,music,books,…}`、`components/{layout,media,detail,playback,home}` 等）。

### 0.2 冻结（不要碰）

| 冻结项 | 说明 |
|--------|------|
| **后台 Admin 视觉** | 视觉冻结；metadata sources 等可有 API，不要借机大改 admin UI |
| **鉴权 / 网关大改** | 禁止重写 token / admin 鉴权策略；**不要做 Quick Connect**（鉴权冻结已明确砍） |
| **STRM / stream** | 可修接线，但「先鉴权后 302」策略不动 |

### 0.3 功能全集 — 不要砍

能力已基本按 Jellyfin 公开能力面对齐（见 §3 与 `JELLYFIN_PARITY_MATRIX.md`）。  
**UI 重写 = 换皮 + 接回现有 API，不是减功能。**  
搜索/筛选/右键/封面/分页/多版本/进度/Skip Intro/播放列表/合集/SyncPlay 面板/书籍阅读器/Trickplay 预览等 — **全部要接回来**。缺 UI 入口 ≠ 可以删后端。

### 0.4 官方插件现状：「不像插件」— 债务，不是成就

用户原话：**不像插件**。诚实定义见 §4。  
目标态（架构真源）：Provider 协议、独立包、热插拔或正式装载生命周期、配置、invoke；**不要再堆假插件 / 旁路模块 / 只挂 manifest 的空壳。**

### 0.5 Git / 提交

默认 **不 commit**，除非用户明确新指令。

### 0.6 图标必须是复合/逻辑图标（禁止 emoji）— MUST

对标 **Jellyfin / Emby** 的能力标识与复合状态，**不是**装饰性 emoji，也不是单个无关 icon。

| 要求 | 说明 |
|------|------|
| 图标体系 | **Lucide 矢量**（`@lucide/vue`）或与 JF/Emby 同气质的 **composed badge**（文字/小标叠在海报角） |
| 禁止 | `🎬` `📚` `🔎` `♫` `✓` 当 icon prop、空态 emoji、营销贴纸 |
| 复合状态 | 一态一语义：多版本、音轨/字幕可切、库类型、剧/电影/合集、筛选激活、收藏/已看、分辨率/HDR 等可叠标 |
| 参考 | JF/Emby Web：海报角分辨率条、收藏心形、已看勾、多版本入口、播放器轨切换菜单——用 Lucide + badge 复现，不抄其资源文件 |

**现状债（必须修）：** C 端仍有 emoji / 字符当 icon，例如 `views/library/index.vue` `📚`、`library/detail` `🔎`/`🎬`、`playlists` `♫`、`MediaCard` 已看用 `✓`。重写时一律换成 Lucide / 组合徽章。

完整清单见下方 **「图标与复合状态」**。

### 0.7 媒体库逻辑禁止「清理简化」— MUST（先前错误，已纠正）

用户原话级纠正：**把媒体库逻辑清理掉是错的。** Claude Code **不得**为了 UI 好看而简化、折叠、阉割库模型与 C 端库流。

后端真源：`fbz-api/src/media_types.rs` — `LibraryType::ALL` = `movies` / `tvshows` / `music` / `homevideos` / `mixed` / `livetv` / `books`；与 Emby `CollectionType` 对齐。条目层级：`Movie` / `Series`→`Season`→`Episode` / `Audio`/`MusicAlbum`/`MusicArtist` / `Book`/`AudioBook` / `Video`/`Photo` 等。

**必须保留（API + FE 行为都要在）：**

| 域 | 必须保住 |
|----|----------|
| 库实体 | 多库、名称、`libraryType`/`CollectionType`、隐藏、权限可见 |
| 路径 / 卷 | 多路径、`VirtualFolders` Add/Remove Paths、`PathInfos`、扫库触发 |
| 扫描 / 监控 | `library.scan`、Realtime Monitor（0104）、增量/续扫语义 |
| 浏览 | Views、库详情网格、Filters/Filters2、题材/标签/年/已看/收藏、排序、分页 |
| 层级 | 剧集：Series → Season → Episode（`ParentIndexNumber`/`IndexNumber`/`SeriesName`）；合集 BoxSet；播放列表 |
| 首页 | HomeSections / 导航 BFF 行（续看、最新、按库）— 不要收成单页营销墙 |
| 管理面配置 | 库设置（语言、图片偏好、监控开关等）可继续走既有 API；**不要**借 C 端重写删掉库概念 |

**已知错误简化（FE，不得扩大、重写时要修回）：**

| 错误 | 现状 | 正确 |
|------|------|------|
| `MediaKind` 折叠 | `types/media.ts` 仅 `movie\|series\|anime\|documentary\|music\|book`；`stores/library.ts` 把 `homevideos`/`mixed`/`livetv` **并进** movie/series | 以后端 `libraryType`/`CollectionType` 为事实源展示；前端 kind 只做展示映射，**不得**丢掉类型语义与路由差异 |
| 空态 emoji 库 | 库总览/详情用 📚/🎬 占位 | Lucide 库类型图标 + 真实空态文案 |
| 详情路由粗暴二分 | `detailTypeOf` 非 Series/Season/Episode 一律 movie | 尊重 `Type`（含 Music/Book/Collection/Person…）与库类型 |

**原则：** UI rewrite = 换皮；**库域模型与 Emby Items/Views/VirtualFolders 合同不动。** 若某屏暂时做不出，保留入口与 API 接线，标 Partial——**禁止删除库流。**

### 0.8 媒体信息 / A/V 元数据展示 — MUST（JF/Emby 级）

详情页、播放器、信息面板必须展示 **技术媒体信息**，不能只剩标题/海报/剧情。数据已在 Emby DTO / PlaybackInfo 里，C 端要接出来。

**权威字段（`fbz-api/src/compat/emby/dto/mod.rs`，勿发明）：**

| 层 | 字段（PascalCase 出站） |
|----|-------------------------|
| `BaseItemDto` | `RunTimeTicks`、`Container`、`Bitrate`、`Size`、`ProductionYear`、`OfficialRating`、`CommunityRating`、`MediaSources[]`、`Chapters` |
| `MediaSourceDto` | `Id`、`Name`、`Container`、`RunTimeTicks`、`Size`、`Bitrate`、`MediaStreams[]`、`DefaultAudioStreamIndex`、`DefaultSubtitleStreamIndex`、`SupportsDirectPlay`/`SupportsDirectStream`/`SupportsTranscoding`、`Chapters` |
| `MediaStreamDto` | `Index`、`Type`（Video/Audio/Subtitle）、`Codec`、`CodecTag`、`Profile`、`Level`、`Width`/`Height`、`Channels`、`SampleRate`、`BitDepth`、`BitRate`、`Language`、`Title`/`DisplayTitle`、`IsDefault`/`IsForced`/`IsExternal`、`DeliveryMethod`/`DeliveryUrl` |

探针侧已有 `bit_depth` 等（`media/probe.rs` → DB → streams）。**当前 DTO 未见独立 `VideoRange`/`HDR` 顶栏字段**——HDR/DV 类展示优先从 `MediaStreams`（BitDepth/Profile/Codec）与版本 tags 推导；若后续 core 补 JF 形 `VideoRange`，再挂字段，**不要前端瞎编枚举。**

**C 端必须有的 UI：**

1. **详情「媒体信息」面板**：容器、时长、分辨率、视频编码、音频编码+声道、比特率、文件大小、字幕轨列表（内嵌/外挂）、多版本列表。  
2. **播放器轨切换**：音轨 / 字幕（含外挂 `DeliveryUrl`）— 已有 `FullscreenPlayer` + `AudioStreamIndex`/`SubtitleStreamIndex` 进度上报语义，重写时保留并做成清晰 Lucide 菜单。  
3. **卡片/海报角标**：分辨率（现有 `Height`→`4K/1080P…`）；有多 `MediaSources` 时多版本暗示；收藏/已看/进度。

**现状债：** `detail.ts` 的 `mediaSourceToVersion` 已把 Height/Codec/Container/Audio Channels 收成 **版本 tags**；详情主区仍偏叙事、**缺完整 A/V 信息面板**。重写必须补上面板，不能再「清理掉」技术信息。

### 0.9 图标与复合状态 · Checklist（Lucide / JF·Emby 气质）

重写验收时逐项打勾（全部 MUST 可达；无数据则隐藏，不假造）：

| 状态 / 入口 | UI 形态 | 数据来源 |
|-------------|---------|----------|
| 库类型（电影/剧集/音乐/家庭视频/混合/直播/图书） | 侧栏/总览 Lucide 分型图标 | `CollectionType` / `libraryType` |
| 电影 vs 剧集 vs 季 vs 集 | 详情壳 + 面包屑；集列表 `SxxExx` | `Type`、`ParentIndexNumber`、`IndexNumber`、`SeriesName` |
| 合集 BoxSet / 播放列表 | 独立图标与列表壳 | `/collections`、`/playlists` |
| 筛选激活 | 筛选条 chip + 清除；激活态高亮 | Filters API / URL query |
| 收藏 | 心形 Lucide + 实心/空心 | `UserData.IsFavorite` |
| 已看 | 角标勾（Lucide `Check`，**不要**字符 ✓ 当设计系统） | `UserData.Played` |
| 观看进度 | 海报底进度条 | `PlaybackPositionTicks` / progress% |
| 分辨率角标 | `4K`/`1080P`/… badge | Video `MediaStreams.Height` |
| HDR / 高比特深度暗示 | 小标（有数据才显示） | `BitDepth`/`Profile`/`Codec`（勿假 HDR 标） |
| 多版本 | 详情下拉 + 播放器版本切换；卡片可暗示「多文件」 | `MediaSources[]`（`Id`/`Name`/`Container`） |
| 音轨切换 | 播放器菜单 | Audio `MediaStreams` + `AudioStreamIndex` |
| 字幕切换（内嵌+外挂） | 播放器菜单 | Subtitle streams + `IsExternal`/`DeliveryUrl` |
| 外挂字幕搜索入口 | 详情操作 | `RemoteSearch/Subtitles` |
| Skip Intro/Credits | 播放器按钮 | Chapters / markers |
| SyncPlay | 播放器面板入口 | SyncPlay API |
| Trickplay 预览 | 进度条悬停帧 | Trickplay tiles |

---

## 1. 项目是什么

**FBZ** = 自托管、**Emby 协议兼容**的媒体服务器（能力面对齐 Jellyfin 类产品；协议是 Emby REST，不做 Jellyfin API 兼容）。

| 包 | 技术 | 角色 |
|----|------|------|
| `fbz-api/` | Rust 2024 · axum · sqlx · PostgreSQL · Redis · FFmpeg · Wasmtime | Modular monolith 后端 |
| `fbz-fe/` | Vue3 · TS · Vite+ · Pinia · daisyUI · Lucide · webplayer | C 端 + Admin SPA |
| `docs/` | 架构 / 验收 / 对照矩阵 | 真源文档 |
| `demo/` `canvas/` `chatcontinue/` | — | **非产品**，勿当交付物 |

部署：同二进制 `FBZ_NODE_ROLE=all|api|worker|scheduler`；PG jobs/outbox 权威，Redis Streams 可选镜像。

---

## 2. 架构要点（准确摘要）

真源：`docs/FBZ-目标架构与落地路线.md`。瘦 API 边界：`docs/THIN_API_AND_WORKER_BOUNDARIES.md`。

### 2.1 Core vs Plugins

| 必须进 Core | 必须（或承诺）插件化 |
|-------------|----------------------|
| auth / users / sessions | `metadata.provider.query`（TMDB/TVDB/Fanart/豆瓣/IMDb/OpenLibrary…） |
| library / media 模型 + 权限 SQL | `storage.provider`（list/stat/resolve/read/auth） |
| scan 编排 + recognition 确定性核心 | `media.marker.detect` |
| **元数据编排** + **NFO 本地/缓存读**（NFO 属 core） | `subtitle.provider.query` / `lyrics.provider.query` |
| markers/chapters **存储与播放消费** | 异步 hooks（playback / scan / notify…） |
| transcode / jobs / scheduler / event_outbox | 管理菜单 `admin.menu`、plugin schedules |
| 插件宿主本身 + `compat/emby`（仅 DTO/路由，禁业务 SQL） | |

### 2.2 元数据 Match 链（现状过渡态）

```text
metadata.refresh job
  → library_metadata_sources（库级 priority；迁移 0100）
  → 每步优先 PluginSyncInvoker → metadata.provider.query
  → fallback：core 内置 TMDB/TVDB/Fanart/IMDb 客户端（过渡期，加深已冻结）
  → NFO：core `nfo_match.rs`（本地已接；云走 storage importer）
```

库级 API：`GET|PUT /api/admin/libraries/{id}/metadata/sources`。  
`nfoMatchIntegration=local`。NFO 写回：`nfo_write.rs` + 库开关 `SaveLocalMetadata` / env `NFO_SAVER_ENABLED`。

### 2.3 StorageProvider

- Trait + registry：`fbz-api/src/storage/provider.rs`
- 光鸭：`plugins/guangya-storage`（manifest id `dev.fbz.storage.guangya`）+ 仍可靠 env URL 拉起
- 文档：`docs/storage-provider-extension.md`、`docs/GUANGYA_STORAGE_PLUGIN_DESIGN.md`
- **第二商业网盘源**未产品化

### 2.4 Markers / Scan / 播放

- Markers 存 core（`media_markers`）；探测可插件 `media.marker.detect`（`plugins/marker-detect`）
- 扫描：本地 `jobs.library.scan`；云 `storage_mounts` 状态机（双链路，事件语义未完全统一）
- `scan/lifecycle.rs` draft **未晋升** Discover 等插件 allow-list
- Library Monitor：轮询最小（0104 + `scan/monitor.rs`）；fsnotify 局部 Discover 后续
- CLIENT_COMPAT + STRM 302（先鉴权后 302，不中继视频体）+ PlayMethod（DirectPlay / Remux≈DirectStream / Transcode）+ 容器名规范化（`matroska`→`mkv`）

### 2.5 瘦 API

Items / navigation / PlaybackInfo = 读库 + 入队；**禁止**请求内同步 ffprobe / 刮削长链 / FFmpeg。重活进 worker。

---

## 3. 功能清单（完整 · 按域）

状态约定（与矩阵一致）：**Done** = 可验收闭环 · **Partial** = 骨架/半接线/一端可用 · **Stub** = 空桩/兼容形状 · **Out** = 非目标。

数据源：`docs/JELLYFIN_PARITY_MATRIX.md` + `docs/ACCEPTANCE_2026-10-16.md` + 代码树。**不要发明功能；不要砍 Done/Partial。**

### 3.1 库 / 扫描 / 条目

> **§0.7：** 本域是产品脊梁。C 端重写 **禁止** 把多库/路径/类型/剧集层级「收成简单海报墙」。

| 能力 | 状态 | 落点 |
|------|------|------|
| 多库 / CollectionType / Views | Done | `library/`、`views`、FE 库页；`LibraryType::ALL` 七类 |
| 路径 / 卷 / VirtualFolders Add·Remove | Done | admin + Emby VF Paths；多 `PathInfos` |
| 扫描触发 / scan job | Done | admin、`scan/service.rs` |
| Library Monitor（轮询） | Done | `scan/monitor.rs` + 0104 |
| Monitor notify / 局部 Discover | Partial/缺 | 后续；**不要**因此删监控开关 |
| Series→Season→Episode 层级 | Done | Items + FE `SeasonEpisodes`；`ParentIndexNumber`/`IndexNumber` |
| Items Genres / CommunityRating / SortBy | Done | DTO + SQL |
| Filters / Filters2 / Suggestions / 长尾筛选 | Done | Emby Items；库详情筛选条 |
| HomeSections / 导航行 | Done/Partial | navigation BFF；首页续看/分行 |
| DELETE Items（软删，可选删文件） | Done | 管理员 |
| Similar / NextUp / Resume / Latest | Done | 兼容路由 |
| **FE 库类型折叠（债）** | **错误简化** | `MediaKind` 合并 homevideos/mixed/livetv → 重写时按 §0.7 修回 |

### 3.2 元数据刮削链

| 能力 | 状态 | 落点 |
|------|------|------|
| 库级源链配置 | Done | `GET\|PUT …/metadata/sources` · 0100 |
| TMDB / TVDB / Fanart | Partial | core 内置 fallback + `plugins/*-provider` 真 HTTP 查询（插件优先） |
| IMDb / Douban | Partial | 官方最小包 + builtin/fallback |
| 本地 NFO → Match | Done（本地）/ Partial（云） | `metadata/nfo_match.rs` |
| NFO 写回 | Partial（可用） | `nfo_write.rs` · 0109 · 季 `season.nfo` |
| Identify / RemoteSearch | Partial | `remote_search.rs` |
| 图片 / RemoteImages / locks | Done/Partial | artwork + 0110 image locks |
| OpenLibrary / Google Books（书） | Partial | `plugins/openlibrary-provider` |

### 3.3 播放

| 能力 | 状态 | 落点 |
|------|------|------|
| PlaybackInfo / MediaSources | Done | `compat/emby/routes/playback.rs` |
| DeviceProfile → SupportsDirect* | Done | `playback/play_method.rs` |
| stream / Download / Range / STRM 302 | Done | `streaming.rs` + `media/strm.rs` |
| HLS 软件转码 | Done（可入队） | `transcode/` |
| 硬件转码全量 / 多档 | Partial | planner 有 HW 名；全量未验 |
| 多版本切换 / 进度 / 已看（C） | Done | `FullscreenPlayer` + playback store |
| Chapters + Skip Intro/Credits（C） | Done | markers + FE |
| 外挂字幕 DeliveryUrl + C 挂轨 | Done | PlaybackInfo + player |
| 远程字幕 OpenSubtitles | Done（插件） | `subtitle.provider.query` · 0108 · `opensubtitles-provider` |
| 歌词 LRCLIB + 上传/删除/远程下载 | Done/Partial | `lyrics.provider.query` · `lrclib-provider` · 0109 |
| Trickplay（生成 + C 预览 + 入库触发） | Partial（可用） | `media/trickplay.rs`；STRM 未做 |
| SyncPlay（组/队列/缓冲/Shuffle/Repeat + C 面板） | Partial（可用子集） | `syncplay.rs`；跨节点未做 |
| Live TV / 真 tuner | Out / Stub | 空桩不 500；**不做真 tuner** |
| **媒体信息 / A/V 面板（C）** | **Partial（债）** | 版本 tags 有 Codec/Height/Container/Channels；**缺**详情完整 Media Info 面板 — 见 §0.8 MUST |
| 音轨 / 字幕切换 UI | Done/Partial | `FullscreenPlayer`；重写须 Lucide 菜单 + 保留外挂 `DeliveryUrl` |
| 多版本 UI | Done | `DetailHero` versions + 播放器 `switchMediaSource`；图标见 §0.9 |

### 3.4 组织 / 用户 / 家长

| 能力 | 状态 | 落点 |
|------|------|------|
| Played / Favorite / 进度 / Sessions NowPlaying | Done | user_data / sessions |
| Playlists CRUD + C 编辑 + 共享 | Done | 0106 + FE；拖拽排序后续 |
| Collections / BoxSet + C | Done | collections + FE |
| 家长评级 / BlockUnrated | Done（最小） | 0105 + `parental.rs` |
| 家长文件夹 ACL + BlockedTags | Done（第八轮加深） | 0110；AllowedTags/日程未做 |
| 用户偏好 / DisplayPreferences / HomeSections | Done | users / display_preferences |

### 3.5 Extras / Theme / Trailers

| 能力 | 状态 | 落点 |
|------|------|------|
| ThemeMedia / LocalTrailers / Trailers(ParentId) | Done | `extras.rs` / theme_media |
| 全局 Trailers（无 ParentId） | Stub | 空形状兼容 |

### 3.6 音乐 / 照片 / 书籍

| 能力 | 状态 | 落点 |
|------|------|------|
| 音乐库 | Partial | `music/` + FE `music/` |
| 照片库 | Partial | `media/photo.rs` + admin photos |
| Books / AudioBook / 阅读器 | Partial（可用） | 0107；epub/pdf/cbz/mobi/azw3/cbr；C 阅读器；DRM 不做 |

### 3.7 运维 / 通知 / 兼容客户端

| 能力 | 状态 | 落点 |
|------|------|------|
| Scheduled Tasks Trigger/Cancel | Done | Emby ScheduledTasks |
| Webhooks 最小（播放/扫描） | Done | `WEBHOOKS_MINIMAL.md` |
| Activity Log | Done（可核对） | admin logs |
| DLNA | Stub/Partial | 兼容桩 |
| Infuse / VidHub / SenPlayer 实机 | Partial（文档未测） | `CLIENT_COMPAT.md` |
| 远程访问 / 证书向导 | Partial | 依赖反代；无内置穿透 |
| Quick Connect | **Out（冻结）** | 鉴权大改冻结，未做 |

### 3.8 C 端已有交互（重写时必须接回）

| 能力 | 状态 | 备注 |
|------|------|------|
| 右键菜单（收藏/已看/播放/刷新元数据…） | Done | 接 Emby userData |
| 搜索页 + URL 状态 | Done | `/search` |
| 库筛选（题材/已看/收藏/排除标签） | Done | Filters API；激活态图标见 §0.9 |
| 封面网格 + 分辨率徽章 + 进度 + 已看 | Done（图标债） | layout=poster；**去掉 ✓/emoji，改 Lucide/badge** |
| 分页（36） | Done | StartIndex/Limit |
| 多版本 / 音轨 / 字幕 / Skip / SyncPlay / Trickplay | Done/Partial | 见 §0.8–0.9；复合图标 |
| **媒体信息 / A/V 元数据面板** | **Partial → MUST 补齐** | Container、RunTimeTicks、Width×Height、Codec、Channels、BitRate、Size、字幕列表 |
| 播放列表/合集浏览编辑 | Done | 独立图标，禁 ♫/▣ 凑合 |
| 剧集季集导航 | Done | `SeasonEpisodes`；层级勿砍 |
| 书库 + 阅读器 | Partial | |
| 空态 / 库总览图标 | **债** | 现 `📚`/`🎬`/`🔎`/`♫` — 必须换 Lucide |

### 3.9 插件包（功能面，架构债见 §4）

| 包 | Manifest id | 合同 | 功能状态 |
|----|-------------|------|----------|
| guangya-storage | `dev.fbz.storage.guangya` | storage RPC | Done（实现在）；装载仍偏 env |
| tmdb-provider | `fbz.provider.tmdb` | `metadata.provider.query` | Done（真查询）+ core fallback |
| tvdb-provider | `fbz.provider.tvdb` | 同上 | 同上 |
| fanart-provider | `fbz.provider.fanart` | 同上 | 同上 |
| imdb-provider | `fbz.provider.imdb` | 同上 | Partial（最小） |
| douban-provider | `fbz.provider.douban` | 同上 | Partial（反爬敏感） |
| openlibrary-provider | `fbz.provider.books` | 同上（book） | Partial |
| opensubtitles-provider | `fbz.subtitle.opensubtitles` | `subtitle.provider.query` | Done |
| lrclib-provider | `fbz.lyrics.lrclib` | `lyrics.provider.query` | Done |
| marker-detect | `fbz.marker.detect` | `media.marker.detect` | Done（可装包） |

---

## 4. 插件债务（重要 ·「不像插件」指什么）

### 4.1 一句话

**宿主已有半套插件地基（manifest / 签名 / 审批 / SyncInvoker / Host API），但官方能力大量仍是：core 内置客户端、env 旁路进程、目录里的 Node 骨架、或「有包身份但仍手启」——用户体感就是假插件。**

目标态写在架构 §5：对齐 codex-proxy-rs 的**打包/SDK/进程外** + Jellyfin 式**扩展点挂槽**；**永不做** DLL/.so 进程内加载。

### 4.2 已有地基（可保留）

| 能力 | 路径 |
|------|------|
| Manifest / 权限 / hooks / configSchema | `fbz-api/src/plugins/manifest.rs` |
| 安装→审批→启用→回滚 | `plugins/repository.rs` · `routes.rs` · 表 `plugin_packages` / `plugin_installations` |
| Ed25519 签名 | `plugins/signing.rs` |
| 异步 Hook worker | `plugins/hooks.rs` · `worker.rs` · event_outbox |
| 同步 SyncInvoker（HTTP，超时/熔断/并发） | `plugins/invoke.rs` |
| Host API `/api/plugin/*` | `plugins/host.rs` |
| WASI 运行时（能力偏窄） | `plugins/wasi.rs` |
| 打包脚本 | `fbz-api/scripts/package-plugin.ps1` |
| 官方包目录 | `fbz-api/plugins/*` · README：`fbz-api/plugins/README.md` |

### 4.3 「不像插件」的具体罪状（按包/机制）

| 现象 | 现实 | 缺什么才算真插件 |
|------|------|------------------|
| **元数据仍双轨** | Match：**插件优先**，但 core `metadata/provider/{tmdb,tvdb,fanart,imdb}.rs` **仍在**作 fallback；密钥路径仍可走 core env | 删内置客户端；只留编排 + NFO；密钥只进 plugin config |
| **光鸭旁路** | 有 manifest，但进程常 `node server.mjs` / compose；endpoint 靠 `FBZ_GUANGYA_PLUGIN_URL` / `FBZ_STORAGE_PROVIDER_ENDPOINT_guangya`；历史 `provider='guangya'` 特判刚淡化 | 正式装包生命周期驱动进程；registry 无 env 旁路；无字符串特判 |
| **HTTP 插件要手启** | 各 provider 写死 `entrypoint: http://127.0.0.1:1910x/...`；运维靠多终端/脚本 | 「装包即跑」进程监督（健康检查+重启）；或正式热加载合同 |
| **扩展点未产品化** | capabilities 有权限/hook 键；缺稳定「扩展槽注册表」产品文档与版本策略 | `GET /api/plugin/capabilities` 扩展点清单版本化；安装=贡献点接线 |
| **扫描事件未晋升** | `scan/lifecycle.rs` draft；光鸭「不走事件 hook」 | Discover/文件变更进 allow-list；云/本地统一事件 |
| **SDK / 另仓未拆** | 官方包仍住在宿主仓 `fbz-api/plugins/`；`_shared` 复制 utils | 独立工程 + 公开 SDK；宿主只留契约测试夹具 |
| **WASI 偏窄** | 无网络、无原生 Host import；重活只能 HTTP | 文档与「首选 WASI」对齐；或明确 HTTP 为重活默认 |
| **市场/ABI** | 有 marketplace 迁移痕迹；JF 插件 ABI **不做** | 不要假装能装 JF/Emby DLL |

### 4.4 Claude Code 对插件轨的要求

1. **诚实命名**：骨架就写 Stub/Partial，别在 README 写成「已插件化完成」。
2. **真边界**：新能力走扩展点键 + SyncInvoker/Hook + 权限键；禁止再开 `src/storage` 式旁路专用模块。
3. **与 UI 轨分离**：C 端 rewrite 先接现有 API；插件硬化另轨，不要为了「看起来像」在 FE 假造插件市场闭环。
4. **冻结加深 core 供应商客户端**（架构已写死）。

运维/爆破证据：`docs/PLUGIN_OPS_PERF_BLAST_2026-10-15.md`（七进程可拉起、Match→SyncInvoker 打到官方包；同时暴露装包权限/列表 bug——多数已同日修）。

---

## 5. API / 目录地图

### 5.1 `fbz-api/src` 顶层模块

```text
app.rs / app_router_tests.rs   路由装配（测试已外迁）
auth/ users/ admin/            身份、用户、原生管理 API
library/ media/ recognition/   库、文件/流、识别
scan/ storage/ cache/          扫描、云存储、挂载缓存
metadata/                      刮削编排、NFO、provider
playback/ transcode/ syncplay/ 播放决策、转码、同步观看
plugins/                       宿主（manifest/invoke/host/worker…）
compat/emby/                   Emby DTO + routes（禁业务 SQL）
jobs.rs scheduler/ events/     队列、定时、outbox
notifications/ navigation/     通知、首页 BFF
music/ realtime/ settings/ …
```

**HTTP 边界：**

| 前缀 | 用途 |
|------|------|
| `/health` `/ready` | 探针 |
| `/api/admin/*` | 管理（AccessToken + 管服权限；**不可**长期 ApiKey 充 admin） |
| `/api/plugin/*` | Host API（短时 `x-fbz-plugin-token`） |
| `/emby/*` + 无前缀别名 | Emby 客户端兼容 |

### 5.2 FE 路由（C 端优先）

| 路径 | 视图 |
|------|------|
| `/` | `views/home` |
| `/search` | `views/search` |
| `/library` `/library/:id` | `views/library*` |
| `/movie/:id` `/tv/:id` `/person/:id` `/collection/:id` | `views/detail/*` |
| `/collections` `/playlists` `/playlists/:id` | 合集/播放列表 |
| `/music/:id` `/artist/:id` `/album/:id` | 音乐 |
| `/books/:id` `/book/:id` | 书库/阅读器 |
| `/admin/**` | Admin（**视觉冻结**） |
| `/user/login` | 登录 |

组件桶：`components/{layout,media,detail,playback,home,admin}`。  
服务：`service/request.ts` + `service/modules/*`。

### 5.3 迁移 0100–0110（亮点）

| 迁移 | 内容 |
|------|------|
| 0100 | 库级 `metadata_sources` |
| 0101 | StorageProvider 扩展（放宽 provider id） |
| 0102 | `media.marker.detect` hook |
| 0103 | `media.analyze` + storage.* 权限 allowlist |
| 0104 | Library realtime monitor 开关 |
| 0105 | 家长控制最小 |
| 0106 | playlist shares |
| 0107 | books / audiobook 库类型 |
| 0108 | subtitle.provider hook |
| 0109 | metadata locks / NFO saver / lyrics |
| 0110 | user access policy + image locks |

更早迁移仍在 `fbz-api/migrations/`（加法迁移；破坏性改表需确认）。

### 5.4 官方插件目录

`fbz-api/plugins/{guangya-storage,tmdb,tvdb,fanart,imdb,douban,openlibrary,opensubtitles,lrclib,marker-detect}-provider|…` + `_shared/`。  
示例模板：`fbz-api/examples/plugins/`。

---

## 6. 本地运行

| 服务 | 地址 |
|------|------|
| API | `http://127.0.0.1:8080`（`/health` `/ready`） |
| FE | `http://127.0.0.1:5173`（代理到 API） |

```powershell
# API
cd fbz-api
./scripts/dev-deps.ps1          # PG + Redis
cargo run                       # 或 ./scripts/dev.ps1

# FE
cd fbz-fe
vp install                      # Vite+；日常用 vp，勿直接 vite/npm
vp dev
```

配置：`fbz-api/.env.example` → `.env`（勿提交）。关键：`DATABASE_URL`、`REDIS_URL`、`FBZ_NODE_ROLE`、各 `FBZ_*_WORKER_ENABLED`、插件与刮削密钥。

**本地管理员（仅开发机，勿当生产密钥传播）：**  
本机测试库曾用 setup 建 `opsadmin`；后续本地密码有重置为 **`fbz-admin-1016`** 的记录——**仅 local-dev**。登录走：

`POST /emby/Users/AuthenticateByName` → `AccessToken`；Admin API 必须用该 token（管服权限），不要用长期 ApiKey 顶替。

插件冒烟（可选）：`docs/PLUGIN_OPS_PERF_BLAST_2026-10-15.md`；各包默认端口约 `19101–19109`（光鸭 RPC `8098`）。

---

## 7. Claude Code 建议工作顺序

1. **读** HDHive-web + `docs/FBZ-C端重设计-HDHive对齐.md` + `docs/hdhive-tokens.css`；确认五屏结构与 token。
2. **只改 C 端组件/样式/布局**（daisyUI + Lucide；圆角 7/11/16；**无 emoji**；暗色 #09090e + 琥珀）。复合状态按 §0.6/§0.9。**不要动 admin 视觉。**
3. **逐屏接回现有 API**，并 **保住库逻辑（§0.7）+ 补齐媒体信息面板（§0.8）**。对照 §3：Done/Partial 入口一个都不能无故消失；禁止再折叠 `LibraryType`。
4. **另轨**硬化真插件边界（Provider 协议、装载、配置、invoke、进程生命周期）；删/收敛假旁路；**不要**为了演示再造假插件。
5. **禁止**：admin 视觉大改、鉴权/网关重写、Quick Connect、砍功能、擅自 commit（除非用户新指令）。

### 成功标准（本交接）

- C 端看起来像 HDHive 媒体工作站，不再「大圆角 emoji 错位」；复合状态用 Lucide/徽章。
- 媒体库逻辑完整（多库/路径/类型/剧集层级/筛选/Views），无「清理简化」。
- 详情/播放器具备 JF/Emby 级媒体信息与 A/V 元数据（MediaSources/MediaStreams 真字段）。
- 现有 Jellyfin 对齐功能面在 UI 上仍可达（API 未砍）。
- 插件相关改动让边界更真，而不是更多骨架文件夹。
- Admin 与鉴权看起来像没被「顺手重构」。

---

## 附录 · 关键文档索引

| 文档 | 用途 |
|------|------|
| `docs/FBZ-目标架构与落地路线.md` | 架构真源（core/plugin/scan/NFO/冻结） |
| `docs/JELLYFIN_PARITY_MATRIX.md` | 能力全集对照 |
| `docs/ACCEPTANCE_2026-10-16.md` | 验收步骤与已落地项 |
| `docs/FBZ-C端重设计-HDHive对齐.md` | C 端视觉 brief + Superdesign |
| `docs/hdhive-tokens.css` | 设计 token |
| `docs/CLIENT_COMPAT.md` | Infuse 等客户端矩阵 |
| `docs/THIN_API_AND_WORKER_BOUNDARIES.md` | 瘦 API 护栏 |
| `docs/storage-provider-extension.md` | StorageProvider |
| `docs/PLUGIN_OPS_PERF_BLAST_2026-10-15.md` | 插件运营/爆破 |
| `docs/WEBHOOKS_MINIMAL.md` | 最小 webhook |
| `fbz-api/plugins/README.md` | 官方包表与密钥策略 |
| `CLAUDE.md` / `fbz-fe/CLAUDE.md` | 仓内 agent 指南（FE 以 fe 内为准；**注意**：旧 brand 绿 `#1ed760` 已被本次 HDHive amber 方向覆盖——C 端以 HDHive brief 为准） |

---

*交接文件生成自 commit `27eebbb` 与上述 docs/代码盘点；2026-10-15 PT 增补 §0.6–0.9（复合图标 / 库逻辑禁止简化 / 媒体信息 MUST）。有冲突以架构真源 + 用户硬约束（§0）为准。*
