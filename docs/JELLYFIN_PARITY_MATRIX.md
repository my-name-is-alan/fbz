# Jellyfin 能力对照 / FBZ 全量缺口矩阵

| 字段 | 值 |
|------|-----|
| 盘点日 | 2026-10-15（PT）/ 验收窗口 2026-10-16 晨（上海 CST） |
| 刷新 | 2026-10-15 ~10:15 PT（附录 C SyncPlay/Books）；~10:20 PT 再核 Live TV 空桩不 500 / **非明早必验** |
| 仓库 | `/Users/edy/Code/fbz`（未 commit 工作区） |
| 对照对象 | Jellyfin **公开**能力（文档 / 发行功能面），不抄代码 |
| 相关文档 | `ACCEPTANCE_2026-10-16.md`、`CLIENT_COMPAT.md`、`WEBHOOKS_MINIMAL.md`、`FBZ-目标架构与落地路线.md` |
| 冻结 | 后台 UI 视觉大改；鉴权/网关大改需审（本轮仅 Emby 客户端所需接口补齐） |
| Live TV | **非目标 / 非明早必验**（空列表·404·409 兼容桩；不 500；**不**实现真 tuner） |

**状态：** `已有` = 可验收闭环 · `部分` = 骨架/半接线/仅一端 · `缺失` = 待建或仅文档 · `后续` = 非明早必验。

**优先级：** **P0** = 明早必验（能力对齐）· **后续** = 晨验后 / 非必验大项（勿当阻断）。

---

## 总览

| # | 能力域（Jellyfin） | FBZ 状态 | 优先级 | FBZ 落点 / 待建 |
|--:|--------------------|----------|--------|-----------------|
| 1 | 媒体库 / 路径 / CollectionType | **已有** | P0 | `library/`、`views.rs`、`media_folders.rs`、`fbz-fe` 库页 |
| 2 | VirtualFolders Add/Remove Paths | **已有** | P0 | `POST /Library/VirtualFolders/Paths`；JF `DELETE …/Paths` + Emby `POST …/Paths/Delete`；可选 `refreshLibrary` |
| 3 | 库扫描 / Library Monitor | **已有**（轮询最小） | P0 | `scan/` job+hooks；`scan/monitor.rs` + `enable_realtime_monitor`（0104）+ `spawn_library_monitor`；notify / 局部 Discover **后续** |
| 4 | 元数据源链（库级） | **已有**（配置+Match） | P0 | `library/metadata_sources*.rs`；NFO→Match #6；官方 Provider 加深 **后续** |
| 5 | 在线刮削 Provider（TMDB/TVDB/Fanart/…） | 部分 | 后续 | core 内置 + 官方插件 stub/`SyncInvoker`；删 core 加深冻结 |
| 6 | 本地 NFO 读入 Match | **已有**（本地）/ 部分（云） | P0 | `metadata/nfo_match.rs`；`NFO_MATCH_INTEGRATION=local`；云走 importer |
| 7 | NFO 写回 / 导出 | **部分（可用）** | — | 第六轮：`metadata/nfo_write.rs`，`NFO_SAVER_ENABLED=true` 时元数据编辑器保存（`POST /Items/{id}`）后写 Kodi/JF 风格 NFO：movie/video/episode → `{文件名}.nfo`，series → `tvshow.nfo`；含 title/originaltitle/sorttitle/plot/year/premiered/mpaa/rating/genre/tag/studio/uniqueid/director/credits/actor；**只覆盖本保存器写出的 NFO（带标记），手写 NFO 不动**；tmp+rename。第七轮：**按库保存器**——`POST /Library/VirtualFolders/LibraryOptions`（`MetadataSavers` 含 `Nfo` 或 `SaveLocalMetadata`，存 `libraries.save_local_metadata`，迁移 0109），`VirtualFolders` 回显 `SaveLocalMetadata`/`MetadataSavers`，`AvailableOptions.MetadataSavers=[Nfo]`；env 与库开关二者满足其一即写；**刷新后自动写**（匹配或未匹配都在刷新结束时落盘）。**未做**：书籍 NFO；第八轮：**季 NFO**——分集写 NFO 时同步写所属季目录 `season.nfo`（`<season>` 根、`<seasonnumber>`；无季目录时不写） |
| 8 | 插件系统（目录/装载/生命周期） | 部分 | P0 最小 / 后续 ABI | HTTP/manifest/装包审批已有；**JF 插件 ABI / 独立 SDK / 市场** 后续 |
| 9 | DirectPlay | **已有** | P0 | stream 直读；PlaybackInfo `SupportsDirect*` ← `play_method` + DeviceProfile |
| 10 | DirectStream / Remux | **已有** | P0 | Remux≈`DirectStream`；DeviceProfile 驱动 |
| 11 | DeviceProfile → PlayMethod | **已有** | P0 | `device_profile_summary_from_playback_hints`；body/query/`ClientCapabilities.DeviceProfile` |
| 12 | Transcode（软件） | **已有**（可入队） | P0 | `transcode/` + HLS；PlayMethod::Transcode 可入队 |
| 13 | 硬件转码 / 多档 ladder | 部分（planner 骨架） | **后续** | `HardwareMode`/encoder 名有；**全量探测/多档/实机** 未照搬完 |
| 14 | 外挂字幕（sidecar） | **已有** | P0 | 入库外挂轨；PlaybackInfo `DeliveryMethod=External`+`DeliveryUrl`；**C 端 FullscreenPlayer 挂轨** |
| 15 | 远程字幕下载（OpenSubtitles 类） | **已有（插件源）** | — | ~19:00 PT：hook `subtitle.provider.query`（迁移 0108）；`RemoteSearch/Subtitles/{lang}` 聚合已启用插件；`POST …/RemoteSearch/Subtitles/{pluginId~id}`（管理员）写旁挂并入队 probe；`/Providers/Subtitles/Subtitles/{id}` 返回字幕文本。插件 `plugins/opensubtitles-provider`（有 key 走 api.opensubtitles.com v1，无 key 走旧版 rest.opensubtitles.org）。~17:10 PT（第五轮）：core 计算 OpenSubtitles moviehash（本地文件 ≥64KiB）随请求下发，插件哈希搜索优先、合并文本结果；下载后 core 剔除广告 cue/行。**未做**：strm/远端存储文件的哈希；旧版无 key API 已被官方标为弃用 |
| 16 | Live TV / IPTV / 录制 | 缺失（非目标） | **后续 / 非目标 / 非明早必验** | `live_tv.rs` 空桩（空列表/Info disabled/单条 404/写 409；禁 500；无真 tuner） |
| 17 | SyncPlay | **部分（可用子集，含 C 端播放器入口）** | 后续加深 | 2026-10-15 ~16:15 PT（C 端 ~16:18 PT：播放器「同步观看」面板 建/加入/退出，暂停/继续/跳转经服务端广播后按 When 执行）：`src/syncplay.rs` 进程内组注册表 + `routes/syncplay.rs`：New/Join/Leave/List/Get/SetNewQueue/Pause/Unpause/Seek/Stop/Ping/Buffering/Ready；经 `/embywebsocket` 广播 `SyncPlayGroupUpdate`/`SyncPlayCommand`（When=+500ms）；ws 断开自动离组。FE `service/modules/syncplay.ts`（REST+消息归约）。~17:30 PT 增补：全员缓冲等待（Buffering/Ready → Waiting，全员 Ready 自动 Unpause；播放中有人缓冲→Pause+Waiting）、队列切换 NextItem/PreviousItem/SetPlaylistItem（C 端选集/上下集经组同步）；实机两会话 curl+WS 冒烟通过。~19:00 PT：Shuffle/Repeat（`SetShuffleMode`/`SetRepeatMode`，PlayQueue 带真实模式；RepeatAll 循环、RepeatOne 停留；随机保持当前项在首位）、NextItem/PreviousItem 带 `PlaylistItemId` 时过期请求 204 不切换；C 端全屏播放器面板「随机播放/循环」按钮，片尾自动请求下一项。第五轮：`Queue`（Queue/QueueNext）、`RemoveFromPlaylist`（含 ClearPlaylist/ClearPlayingItem）、`MovePlaylistItem`，广播 PlayQueue（Reason Queue/QueueNext/RemoveItems/MoveItem）；C 端同步面板显示队列（上移/移除）与「下一集加入同步队列」。**未做**：跨节点、组内权限细分 |
| 18 | 用户 / 权限 / 偏好 | **已有** | P0 | `users/`、`user_data.rs`、`display_preferences.rs`；Configuration GET+UserDto 回显；DisplayPreferences 一等字段经 CustomPrefs 保留键；HomeSections 认 `homesectionN` |
| 19 | 家长控制（评级 / 未分级类型） | **已有**（最小） | P0 | `0105`；Policy `MaxParentalRating`/`BlockUnratedItems`；Items 过滤 `library/parental.rs` |
| 20 | 家长文件夹 ACL 全量（EnabledFolders / BlockedTags / …） | **已有（文件夹 + 标签 + 分级）** | — | 第八轮：策略原样存 `user_access_policies`（0110），GET Policy 原样回显 EnableAllFolders/EnabledFolders/BlockedMediaFolders/BlockedTags；新建库触发器自动授予 EnableAllFolders 用户；条目级 SQL 函数 `fbz_item_visible_to_user`（MaxParentalRating 取条目或最近祖先分级，BlockedTags 查条目及祖先）接入 Items 列表（含 playstate/Ids/ProviderIds 变体）、Resume、Latest、Similar、季、集、NextUp、Upcoming、详情。**未做**：AllowedTags（包含模式）、AccessSchedules、搜索提示/Genres 等聚合计数未按条目过滤。以下为第七轮原文： 第七轮：`POST /Users/{id}/Policy` 按 JF 语义生效——`EnableAllFolders` → 全部库；否则 `EnabledFolders`；再减 `BlockedMediaFolders`/`BlockedFolders`（id 大小写/连字符不敏感），落到 `library_permissions.can_view`（Views/Items/详情均受限，越权 404）。**未做**：`EnableAllFolders=true` 的用户不会自动看到**之后新建**的库（需再保存一次策略）；GET Policy 由权限行回推（黑名单形式回显为 EnabledFolders 显式列表）；BlockedTags/AllowedTags、ParentalRating 上限未照搬 |
| 21 | 合集 BoxSet / Collections | **已有** | P0 | `collections.rs`；C 端 `/collections` + 详情播放 + 右键加入 |
| 22 | 播放列表 Playlists | **已有**（API+C 编辑+共享） | P0 | CRUD+改名；C 创建/改名/加删 + 右键加入；~17:30 PT 共享：迁移 `0106_playlist_shares`，`GET/POST/DELETE /Playlists/{id}/Users[/{userId}]`（CanEdit）、`IsPublic`→OpenAccess，PlaylistDto 真实 Shares；实机冒烟通过。~19:00 PT C 端播放列表详情「共享」弹窗（公开开关、共享列表 可编辑/移除、添加用户；非创建者提示）；第五轮按弹窗调用顺序 API 级实机冒烟通过；`DELETE /Items/{playlistId}` 现可删播放列表（属主/管理员，其他人 403）。拖拽排序后续 |
| 23 | 章节 Chapters | **已有** | P0 | Items + PlaybackInfo MediaSource.Chapters ← `media_markers` |
| 24 | Intro/Credits 探测与 Skip | **已有** | P0 | `marker_detect`+插件；C 端 Skip Intro/Credits |
| 25 | 搜索 / 筛选 / Genres | **已有** | P0 | Filters/Filters2、Suggestions、ExcludeItemTypes、Min\|MaxCommunityRating |
| 26 | Sessions / NowPlaying | **已有**（最小） | P0 | `LastActivityDate`/`RemoteEndPoint`/`NowPlayingItem`/`PlayState` |
| 27 | 远程访问（WAN/公网 URL/HTTPS） | 部分 | 后续 | `WanAddress`/`WanDdns` 字段；无内置穿透 |
| 28 | STRM / 外链 302 | **已有** | P0 | `media/strm.rs` + 鉴权后 302 |
| 29 | Trickplay / BIF / 章节图 | **部分（JF Trickplay 可用）** | 后续加深 | 2026-10-15 ~16:20 PT：`media/trickplay.rs` + `routes/trickplay.rs`：`GET /Videos/{id}/Trickplay/{width}/tiles.m3u8`、`/{index}.jpg`、`GET /Videos/{id}/Trickplay`（fbz 扩展列表）、`POST …/Trickplay/Generate?Width=`（管理员，后台 ffmpeg `fps+scale+tile 10×10`，宽 160/320/480，产物在 `ARTWORK_CACHE_DIR/trickplay`）。C 端进度条悬停/拖动预览。~17:30 PT 增补：item 详情 `Trickplay` 字段（JF 形状，按 MediaSourceId→Width 分组）、`GET …/Trickplay/Status` 进度、定时任务 `trickplay.generate`（`TRICKPLAY_AUTO_GENERATE`/`TRICKPLAY_SCHEDULE`/`TRICKPLAY_MAX_PER_RUN`/`TRICKPLAY_WIDTH`，需 `FBZ_SCHEDULER_ENABLED=true`）；实机冒烟通过。~19:00 PT：入库后触发——probe 批次处理完即单飞触发生成（`TRICKPLAY_AFTER_SCAN`，默认随 `TRICKPLAY_AUTO_GENERATE`）。第七轮：状态持久化到 `ARTWORK_CACHE_DIR/trickplay/{item}/status.json`（tmp+rename），重启后 `…/Trickplay/Status` 从文件回读；重启前处于 Running/Queued 的任务回读为 `Interrupted`。**未做**：STRM |
| 30 | Extras / Theme / Trailers | **已有**（本地 Extra） | P0 | `media/extras.rs`；ThemeMedia/LocalTrailers/Trailers(`ParentId`)；全局无 ParentId 仍空形状 |
| 31 | 音乐库 | 部分 | 后续 | `music/`、C 端 music 视图 |
| 32 | 照片库 | 部分 | 后续 | `media/photo.rs` |
| 33 | Books / 有声书 | **部分（可用）** | — | ~19:00 PT：库类型 `books`、项目类型 Book/AudioBook（迁移 0107）；扫描 epub/pdf/cbz/cbr/mobi/azw3 → Book，m4b/mp3 等音频 → AudioBook（按上级目录建 Folder 作 ParentId）；Items 过滤；阅读端点 `/Books/{id}/Manifest`、`/Chapters/{n}`（epub，脚本剥离）、`/Pages/{n}`（cbz）、`/Resource`、`/File`；C 端书库页（电子书网格+有声书分组，经音乐播放条播放）与简易阅读器（epub/cbz/pdf，记忆位置）。第五轮：扫描期元数据——epub OPF（标题/作者→people writer/简介/年份）、cbz ComicInfo.xml、封面（epub OPF 封面、cbz 首页、pdf 首页经 `pdftoppm`/macOS `sips` 渲染）写 `ARTWORK_CACHE_DIR/book-covers/{id}.jpg` 并入 artwork；电子书也按子目录挂 Folder 父项；有声书续听（按 UserData 选段落+位置，音乐播放条按起播位置跳转）。第六轮：Items 列表按 JF `Fields` 语义返回 `Overview`/`People`（每项≤20 人，单 SQL；不请求则不返回）；刷新元数据——`POST /Items/{bookId}/Refresh` 与 `POST /Items/{libraryId}/Refresh`（库级入队≤500，后台 metadata worker）对已入库书籍重新本地提取（替换）并查询在线插件（只补空，已有作者不追加，只接受带 isbn/openlibrary/googlebooks 外部 id 的贡献）；mobi/azw3（无 DRM；MOBI6/KF8 图片引用改写，pagebreak 切章，首图作封面；自行 PalmDOC 解压与 trailing entries 剥离，绕开 mobi crate 0.8 丢最后正文记录的 bug）→ 缓存 epub；cbr → cbz（bsdtar/unar，仅图片+ComicInfo）；Manifest 带 `SourceFormat`/`Converted`，`/File` 仍给原文件；官方插件 `plugins/openlibrary-provider`（`fbz.provider.books`：Open Library 免 key + Google Books key 可选，标题搜索防误配）。**未做**：DRM 书；真 RAR 格式 cbr 未实机验（无 rar 工具造样本，libarchive 路径已接）；pdf 文本元数据。第七轮：C 端阅读器标题旁显示格式徽标（`MOBI → EPUB` / `CBR → CBZ` / `PDF`），转换过的书给「下载原文件」链接 |
| 34 | 下载 / Sync / 离线 | 部分 | 后续 | `sync.rs` 兼容面；非完整 JF Sync |
| 35 | Scheduled Tasks | **已有**（Trigger/Cancel） | P0 可核对 | `POST\|DELETE …/ScheduledTasks/Running/{id}` |
| 36 | 通知 / Webhooks | **已有**（最小） | P0 可核对 | `notification-targets`；播放/扫描 → `enqueue_event_notification` → worker POST；见 `WEBHOOKS_MINIMAL.md` |
| 37 | DLNA | 部分 | 后续 | `dlna.rs` 兼容桩 |
| 38 | 客户端矩阵（Infuse 等） | 部分（文档） | **后续** | `CLIENT_COMPAT.md`；**Infuse/VidHub/SenPlayer 实机** 非明早必验 |
| 39 | StorageProvider / 云盘 | **已有**（扩展点+光鸭）/ 部分（第二源） | P0 | `storage/provider.rs` 已 `pub mod`；光鸭包身份；第二源产品化后续 |
| 40 | 扫描阶段插件事件（Discover 等） | 部分 | 后续 | hooks 边界已有；`scan/lifecycle.rs` draft **未晋升 allow-list** |

---

## 分域明细

### A. 库与扫描

| JF 能力 | 状态 | P | 落点 |
|---------|------|---|------|
| 多库 / Movies·Shows·Music·… | **已有** | P0 | `libraries`、Views、`fbz-fe/views/library` |
| 路径管理 / 扫描触发 | **已有** | P0 | admin 库 API、`scan/service.rs` |
| VirtualFolders AddMediaPath | **已有** | P0 | `POST …/VirtualFolders/Paths` |
| VirtualFolders RemoveMediaPath | **已有** | P0 | JF `DELETE …/Paths` + Emby `POST …/Paths/Delete` |
| Library Monitor（轮询） | **已有** | P0 | `scan/monitor.rs` + 0104；debounce → `library.scan` |
| Monitor notify / 局部 Discover | 缺失 | **后续** | stub 名保留 |
| 增量 / 缺失追踪 | 部分 | 后续 | `0022_scan_missing_tracking` |
| 扫描进度推送 | 部分 | 后续 | realtime / jobs |

### B. 元数据与 NFO

| JF 能力 | 状态 | P | 落点 |
|---------|------|---|------|
| 库级源优先级 | **已有** | P0 | `GET\|PUT …/metadata/sources`；`nfoMatchIntegration=local` |
| TMDB/TVDB/Fanart/OMDb·IMDb | 部分 | 后续 | core + `plugins/*-provider` stub |
| 本地 `.nfo` 参与 Match | **已有**（本地） | P0 | `metadata/nfo_match.rs` |
| Identify / RemoteSearch | 部分 | 后续 | `remote_search.rs`、`item_lookup.rs` |
| 写回 NFO | 部分（可用） | — | 见 #7（编辑器保存后写回；`NFO_SAVER_ENABLED`） |

### C. 播放

| JF 能力 | 状态 | P | 落点 |
|---------|------|---|------|
| PlaybackInfo | **已有** | P0 | `compat/emby/routes/playback.rs` |
| DeviceProfile → SupportsDirect* | **已有** | P0 | `play_method.rs` + playback 接线 |
| stream / Download / Range | **已有** | P0 | `streaming.rs` |
| HLS 软件转码 | **已有**（可入队） | P0 | `transcode/` |
| 硬件转码全量 / 多档 | 部分 | **后续** | planner 有 HW 名；全量未验 |
| 外挂字幕 DeliveryUrl + C 挂轨 | **已有** | P0 | PlaybackInfo + `FullscreenPlayer`/`attachSrt` |
| C 端版本切换 / 续播 / 已看 | **已有** | P0 | `FullscreenPlayer`、playback store |
| C 端 Skip Intro/Credits | **已有** | P0 | Chapters + Skip 按钮 |
| SyncPlay | 部分（可用子集） | 后续加深 | 组/加入/离开/播放暂停跳转同步经 WS；见 #17 |
| Live TV 真 tuner | 非目标 / **非明早必验** | **后续** | 空桩不 500 即可；真 tuner 不做 |

### D. 用户数据与组织

| JF 能力 | 状态 | P | 落点 |
|---------|------|---|------|
| Played / Favorite / 进度 | **已有** | P0 | `user_data.rs`、Sessions Playing/Progress |
| Sessions NowPlaying 最小字段 | **已有** | P0 | `sessions.rs` / `SessionInfoDto` |
| Collections | **已有** | P0 | API + C 浏览/详情/加入 |
| Playlists API + C 编辑 | **已有** | P0 | 创建/改名/加删/右键加入 |
| DELETE Items | **已有** | P0 | 管理员软删（可选 DeleteFiles） |
| 搜索筛选 / Filters2 / Suggestions | **已有** | P0 | 见总览 #25 |
| 家长评级 / BlockUnrated | **已有**（最小） | P0 | `0105` + `parental.rs` |
| 家长文件夹 ACL 全量 | 部分 | **后续** | EnabledFolders 等未全量生效 |

### E. 章节 / Intro / Extras

| JF 能力 | 状态 | P | 落点 |
|---------|------|---|------|
| Chapters（Items + PlaybackInfo） | **已有** | P0 | `media_markers` |
| Intro 探测 worker | **已有** | P0 | `marker_detect` + 插件装包 |
| ThemeMedia / LocalTrailers / Trailers(ParentId) | **已有** | P0 | `theme_media.rs`、`extras.rs`、`items::trailers` |
| 全局 Trailers（无 ParentId） | 空形状兼容 | 后续 | 无 Trailer ItemType 目录 |

### F. 插件 / 存储 / 远程 / 其它大项

| JF 能力 | 状态 | P | 落点 |
|---------|------|---|------|
| 插件包 / 审批 / Sync HTTP | **已有**（最小） | P0 | `plugins/`、ACCEPTANCE §1.2/§5.1 |
| JF 插件 ABI / WASI 产品化 / 市场 | 部分/缺失 | **后续** | 非明早必验 |
| StorageProvider 注册表 | **已有**（扩展点） | P0 | `storage/provider.rs` 已挂；第二源后续 |
| Webhooks 最小事件 | **已有** | P0 可核对 | `WEBHOOKS_MINIMAL.md` |
| ScheduledTasks Trigger/Cancel | **已有** | P0 可核对 | 既有路由 |
| 扫描 Discover 事件晋升 | 部分 | 后续 | `lifecycle.rs` draft |
| Books/有声书 | 部分（可用） | 后续加深 | 见 #33（第六轮：Fields 列表字段、刷新重提取、mobi/azw3/cbr 阅读、在线书目插件） |
| Infuse / VidHub 实机 | 未测 | **后续** | `CLIENT_COMPAT.md` |
| 远程访问 / 证书向导 | 部分 | 后续 | 依赖反代 |

---

## P0 明早必验清单（能力向）

1. 冒烟：API `/ready`、登录、库浏览、起播（ACCEPTANCE §0）
2. 库级 metadata sources GET/PUT + `nfoMatchIntegration=local`（§1.3 / A.1）
3. C 端：右键 / 搜索筛选 / 封面分页 / 版本切换续播（§2）
4. Items Genres/CommunityRating/SortBy；Filters2；Suggestions；DELETE Items；Exclude/Min|MaxCommunityRating（§3 / A.9）
5. STRM、容器规范化、**DeviceProfile → PlayMethod → SupportsDirect\***（A.2）
6. PlaybackInfo Chapters + C 端 Skip Intro/Credits（A.3）
7. 本地 NFO→Match（A.1）
8. Playlists C 编辑 + Collections C（A.5 / A.5b）
9. 外挂字幕 DeliveryUrl + **C 端挂轨**；Sessions NowPlaying 最小字段（A.9 / A.10）
10. Parental 最小（评级 / BlockUnrated）（A.5c）
11. VirtualFolders Add/Remove Paths（A.11）
12. Library Monitor 轮询最小（0104 + 开关库可触发 scan）（A.12）
13. Theme/LocalTrailers/Trailers(ParentId)（A.14）
14. Webhooks 最小事件 + ScheduledTasks Trigger/Cancel 核对（A.13；`WEBHOOKS_MINIMAL.md`）
15. 插件/光鸭包身份、StorageProvider 注册表存在（§5）
16. 冻结项未误做（§6）；**Live TV 非明早必验**（附录 C：空桩不 500 即可，勿当 P0 阻断）

---

## 非明早必验 / 后续（诚实缺口 · 勿当晨验阻断）

| 大项 | 现状一句话 | 说明 |
|------|------------|------|
| **Live TV / IPTV / 录制** | 空兼容桩（空列表 / 404 / 409；不 500） | **非明早必验 / 非目标**；真 tuner 不做 |
| **SyncPlay** | 最小骨架（NotImplemented + 空 List） | **非完整**；多人同步未照搬 |
| **硬件转码全量** | planner 有 HW 编码器名 | 探测/多档 ladder/实机全量未完 |
| **JF 插件 ABI** | HTTP/manifest 最小 | 官方 JF 插件 ABI / SDK / 市场未产品化 |
| **Books / Comics 高级** | 库类型不支持；RemoteSearch/Book 边角 | 不可建 books；Items Book→空；无阅读器 |
| **家长文件夹 ACL 全量** | 评级/未分级类型已有 | EnabledFolders / BlockedTags 等按文件夹 ACL 未全量 |
| **Infuse / VidHub / SenPlayer 实机** | 文档矩阵「未测」 | 晨验不要求刷绿；见 `CLIENT_COMPAT.md` |
| Monitor notify / 局部 Discover | 轮询已有 | fsnotify 加深后续 |
| NFO 写回 | 部分（可用，第六轮） | 后续加深（远程字幕真源已于第四/五轮落地：OpenSubtitles 插件 + moviehash + 去广告，见 #15） |
| Trickplay 生成、Playlists 拖拽排序 | 弱/未做 | 后续 |
| WAN/DDNS/证书向导、音乐/照片深化 | 部分 | 后续 |
| 扫描阶段 hook 晋升 allow-list | draft | 后续 |
| 第二网盘源产品化 | 扩展点已有 | 后续 |

---

## 今晚进度（标绿汇总）

| 时间 (PT) | 事项 | 状态 |
|-----------|------|------|
| 盘点起 | 矩阵/验收文档落盘 | 已有 |
| 同晚 | 本地 NFO→Match；PlayMethod→SupportsDirect*；DeviceProfile 接线 | **已有** |
| 同晚 | PlaybackInfo Chapters + C Skip Intro/Credits | **已有** |
| 同晚 | Playlists C 编辑；Collections C；Parental 最小；Activity Log 核对 | **已有** |
| 同晚 | 外挂字幕 DeliveryUrl + C 挂轨；Sessions NowPlaying 最小字段 | **已有** |
| 同晚 | Filters2 / Suggestions / DELETE Items / 长尾筛选 | **已有** |
| 同晚 | VirtualFolders Add/Remove Paths | **已有** |
| 同晚 | Library Monitor 轮询最小（0104） | **已有** |
| 同晚 | ThemeMedia / LocalTrailers / Trailers(ParentId) | **已有** |
| 同晚 | Webhooks 播放/扫描事件；ScheduledTasks Trigger/Cancel 核对 | **已有** |
| ~10:15 | SyncPlay 最小骨架（NotImplemented + 空 List）；Books 库类型不支持文档化 | **部分/文档**（附录 C） |
| ~16:20 | Trickplay 路由+ffmpeg 生成+C 端预览；SyncPlay C 端面板；歌词上传/删除（`POST/DELETE /Audio/{id}/Lyrics`）；`GET /Playlists/{id}` PlaylistDto；FE 测试环境修复（Node≥25 webstorage、字幕 URL 断言） | **部分** |
| ~16:15 | SyncPlay 由骨架升级为可用子集（组生命周期 + WS 广播 + FE service）；QuickConnect 因鉴权冻结未做 | **部分** |
| ~10:20 | Live TV 空桩再核（模块注释+单测：不 500；矩阵标明非明早必验） | **非目标文档/测试** |
| — | Live TV 真实现 / SyncPlay **完整** / HW 全量 / JF ABI / Books 库与阅读器 / 文件夹 ACL 全量 / Infuse 实机 | **后续** |
| — | 后台 UI 视觉大改；鉴权网关重写 | **冻结** |
| — | **未 commit** | — |

### 自测命令（本机已跑 / 晨验可复跑）

```bash
cd fbz-api
cargo test --lib -- metadata::nfo_match playback::play_method match_chain_nfo nfo_match_integration
cargo test --lib -- compat::emby::routes::playback::tests
cargo test --lib -- compat::emby::routes::media_folders::tests emby_virtual_folder_paths_write_aliases_exist emby_virtual_folder_paths_remove_aliases_exist
cargo test --lib -- notifications::system playback_hooks_enqueue_webhook scan_hooks_enqueue_webhook emby_scheduled_task_aliases_exist
cargo test --lib -- session_dto_includes_now_playing
cargo test --lib -- compat::emby::routes::live_tv::tests emby_live_tv
```

### 变更文件（未 commit · 摘要）

- 文档：`JELLYFIN_PARITY_MATRIX.md`、`ACCEPTANCE_2026-10-16.md`、`WEBHOOKS_MINIMAL.md`、`CLIENT_COMPAT.md`
- NFO / PlayMethod / DeviceProfile / Chapters / DeliveryUrl：`metadata/nfo_match.rs`、`playback/play_method.rs`、`compat/emby/routes/playback.rs`
- VF Paths / Monitor / Parental / Webhooks / Sessions / Trailers：`media_folders.rs`、`scan/monitor.rs`、`library/parental.rs`、`notifications/system.rs`、`sessions.rs`、`theme_media.rs`、`media/extras.rs`
- C 端：Playlists / Collections / Skip / 外挂挂轨：`fbz-fe` 对应 views/components

## 2026-10-15 ~16:20 PT 兼容面审计（路由存在性 + 抽查形状）

| 能力 | 结论 | 说明 |
|------|------|------|
| Next Up `/Shows/NextUp` | 已有 | QueryResult；~17:30 PT 已实现四个参数语义（以最近已看集为锚，按最近观看排序；非法 cutoff→422）；实机仅验 200/422，dev 无剧集数据未验结果内容 |
| Similar `/Items/{id}/Similar` | 已有 | 未深审排序算法 |
| Lyrics | 已有 + 本轮补 | GET 形状对齐 JF `LyricDto`；**新增** `POST /Audio/{id}/Lyrics?fileName=`（管理员写 sidecar）/`DELETE`；第七轮：远程歌词下载 `POST /Audio/{id}/RemoteSearch/Lyrics/{lyricId}`（管理员，写 `.lrc`/`.txt` sidecar，先删旧 sidecar）、`GET /Providers/Lyrics/{lyricId}`；搜索插件优先（hook `lyrics.provider.query`，官方插件 `plugins/lrclib-provider` = `fbz.lyrics.lrclib`，免 key），无插件时回退内置 LRCLIB；插件 id 形如 `fbz.lyrics.lrclib~19080`，内置为 `lrclib-{n}` |
| Remote images | 已有 | `RemoteImages`/`Providers`/`Download` 路由在位 |
| Collections 编辑 | 已有 | 建/加/删成员 |
| Playlists 编辑 | 已有 + 本轮补 | Move 已有；**新增** `GET /Playlists/{id}`（PlaylistDto，OpenAccess=false/Shares=[]）；~17:30 PT 共享用户 `/Playlists/{id}/Users*` 已实现（迁移 0106） |
| 字幕 | 已有 | 远程搜索/下载、删除、流式；**上传** `POST /Videos/{id}/Subtitles` 仍缺 |
| Identify | 已有 | `Items/RemoteSearch/*` + `Apply/{id}` |
| Quick Connect | 不做 | 鉴权冻结 |
| Books/audiobook 库类型 | 已有（第四轮） | 见 #33 |

## 2026-10-15 ~17:30 PT 第三轮增补
| 项 | 状态 | 说明 |
|---|---|---|
| 字幕上传 `POST /Videos/{id}/Subtitles` | **已有** | 管理员；JF UploadSubtitleDto（Base64 Data）；写 `{stem}.{lang}[.sdh][.forced].{fmt}` 旁挂并入队 media.probe；非管理员 403；实机冒烟：PlaybackInfo 出现外挂 eng/srt |
| 远程字幕搜索/下载 | **已有（第四轮）** | 见 #15 |
| Books/有声书库类型 | 已有（第四轮） | 见 #33 |

## 2026-10-15 ~19:00 PT 第四轮增补
| 项 | 状态 | 说明 |
|---|---|---|
| Books/有声书 | **部分（可用）** | 见 #33；迁移 0107 |
| Trickplay 入库后生成 | **已有** | 见 #29；新环境变量 `TRICKPLAY_AFTER_SCAN` |
| C 端播放列表共享 UI | **已有** | 见 #22 |
| 远程字幕插件 OpenSubtitles | **已有** | 见 #15；迁移 0108 |
| SyncPlay Shuffle/Repeat | **已有** | 见 #17 |

## 2026-10-15 ~17:10 PT 第五轮增补
| 项 | 状态 | 说明 |
|---|---|---|
| 播放列表共享（弹窗流程 API 级冒烟） | **已验** | 见 #22；新增 `DELETE /Items/{playlistId}` |
| Books 元数据 + 封面 + 电子书父目录 + 有声书续听 | **已有** | 见 #33；无新迁移 |
| 字幕 moviehash + 去广告 | **已有** | 见 #15 |
| SyncPlay 队列增删移 | **已有** | 见 #17 |


## 2026-10-15 ~17:35 PT 第六轮增补
| 项 | 状态 | 说明 |
|---|---|---|
| Items 列表 `Fields=Overview,People` | **已有** | JF 语义：请求才返回；见 #33 |
| 书籍刷新元数据（单项 / 整库） | **已有** | `POST /Items/{id}/Refresh`；传库 id 时整库入队；见 #33 |
| mobi/azw3 → epub、cbr → cbz 服务端转换阅读 | **已有** | 缓存 `ARTWORK_CACHE_DIR/book-cache`；DRM 拒绝；见 #33 |
| 在线书目插件 `fbz.provider.books` | **已有** | Open Library + Google Books（key 可选）；`metadata.provider.query` |
| 元数据编辑器保存 `POST /Items/{id}`（UpdateItem） | **已有（新）** | 复盘发现此前缺失：管理员提交 BaseItemDto 子集（Name/OriginalTitle/ForcedSortName/Overview/年份/首映/分级/评分/Genres/Tags/Studios/People/ProviderIds），复用插件 Host 校验+事务，置 `manual`；非管理员 403、未知 404 |
| 条目详情 `ProviderIds`/`Studios`/`Tags`/`SortName`/`ForcedSortName`/`LockData` | **已有（新）** | 复盘发现详情缺 ProviderIds（客户端外链/识别依赖）；仅详情附加，列表不加 |
| NFO 写回 | **部分（可用）** | 见 #7 |
| 复盘后仍缺（非鉴权、非 LiveTV） | 后续 | 远程歌词下载（需歌词 provider 插件）、Trickplay 状态持久化、刷新后自动写 NFO、按库元数据保存器开关、LockData 阻止刷新、家长文件夹 ACL 全量、硬件转码全量、SyncPlay 跨节点、DLNA/Sync 深化 |

## 2026-10-15 ~17:50 PT 第七轮增补
| 项 | 状态 | 说明 |
|---|---|---|
| LockData / LockedFields | **已有（新）** | 迁移 0109：`media_items.lock_data`/`locked_fields` + BEFORE UPDATE 触发器——任何写入者（刷新、扫描、存储 importer、插件、裸 SQL）都无法改已锁字段；`LockData` 锁全部元数据列，`LockedFields` 支持 Name/Overview/OfficialRating/Runtime（列级）与 Genres/Studios/Tags/Cast（代码级门控）。元数据编辑器在事务内设 `fbz.metadata_edit` 绕过并写锁本身。刷新 `LockData` 项直接跳过（状态 `locked`）；书籍重提取尊重 Cast 锁。**未做**：图片锁（仅 LockData 粒度之外无单独门控）、ProductionLocations 等冷门字段 |
| NFO 按库保存器 + 刷新后写 | **已有（新）** | 见 #7 |
| Trickplay 状态持久化 | **已有（新）** | 文件，见 #29 |
| 歌词 provider 插件 + 远程歌词下载 | **已有（新）** | 见 Lyrics 行；hook 白名单加 `lyrics.provider.query`（0109） |
| 家长文件夹 ACL（Enabled/Blocked Folders） | **部分（可用）** | 见 #20 |
| C 端阅读器显示原始格式 | **已有（新）** | 见 #33 |
| 顺带修复 | — | 插件同步调用幂等键改为进程级计数（原按请求新建 invoker 时同一秒内重复键，插件 SDK 当重放返回 `deduped`，导致第二次调用拿不到结果——影响所有同步 hook）；内置 LRCLIB `/api/get/{n}` 数字 id 解析失败 |
| 仍缺（非鉴权、非 LiveTV） | 后续 | 季 NFO、图片锁、新建库对 EnableAllFolders 用户自动可见、BlockedTags/分级上限、硬件转码全量、SyncPlay 跨节点、DLNA/Sync 深化、DRM 书 |

## 2026-10-15 ~18:40 PT 第八轮增补
| 项 | 状态 | 说明 |
|---|---|---|
| EnableAllFolders 用户自动可见新建库 | **已有（新）** | 0110 `trg_libraries_grant_all_folder_users`（AFTER INSERT on libraries；排除该用户 BlockedMediaFolders） |
| GET Policy 原样回显 | **已有（新）** | `user_access_policies`；未保存过策略的用户仍按权限行推导（旧行为） |
| BlockedTags + MaxParentalRating | **已有（新）** | 见 #20；分级表与 `library/parental.rs` 一致（SQL `fbz_rating_level`） |
| 图片锁 `LockedFields: ["Images"]` | **已有（新，fbz 扩展）** | JF `MetadataField` 无 Images，按需求作为扩展；`LockData` 同样锁图。0110 `trg_artwork_image_lock`：自动写入（刷新/扫描/插件/importer/裸 SQL）静默跳过；手动上传/删除/重排/改 URL 在事务内设 `fbz.metadata_edit` 放行 |
| 季 NFO | **已有（新）** | 见 #7 |
| LRCLIB 回退元数据规范化 | **已有（新）** | 内置回退改用 `track_name+artist_name` 结构化搜索（已知艺人时）；「歌名 - 歌手」三栏同值的记录拆分为标题/艺人、专辑置空；艺人等于标题时用曲目已知艺人 |
| 仍缺（非鉴权、非 LiveTV） | 后续 | AllowedTags/AccessSchedules、聚合接口（Genres/Studios 计数、SearchHints）未做条目级过滤、书籍 NFO、硬件转码全量、SyncPlay 跨节点、DLNA/Sync 深化、DRM 书 |

### 第八轮 · 触及权限判定的文件（供评审）
| 文件 | 改动 |
|---|---|
| `migrations/0110_user_access_policy_image_locks.sql` | 新表 `user_access_policies`；触发器 `trg_libraries_grant_all_folder_users`（写 `library_permissions`）；函数 `fbz_rating_level`、`fbz_item_visible_to_user`；图片锁触发器 |
| `src/users/access_policy.rs`（新） | 策略原样读/写/合并（只碰 `user_access_policies`） |
| `src/compat/emby/routes/users.rs` | `apply_emby_policy` 保存原样策略并推导 `library_permissions.can_view`（`apply_folder_access`）；GET 用户详情叠加原样策略（`overlay_access_policy`） |
| `src/library/repository.rs` | 12 个用户查询加 `fbz_item_visible_to_user(...)`：list_user_items、list_user_items_from_playstates、list_user_items_by_include_ids、list_user_items_by_provider_ids、list_resume_items、list_latest_items、list_similar_items、list_series_seasons、list_series_episodes、list_next_up_items（2 处）、list_upcoming_episodes、find_user_item_by_id |
| `src/users/repository.rs` | 仅新增 `pool()` 访问器（无判定逻辑变化） |
未改：鉴权/网关、`library_permissions` 其它读取方（admin、media、transcode、storage、presentation）。
