# FBZ API 后端长期执行 Goal

本文用于把当前 Codex 线程里的长期 goal 固化到仓库。换电脑继续开发时，先让 Codex 读取本文，再继续分步执行。

## 当前长期 Goal

分步开始完成计划，每一个控制器、函数、方法都需要考虑拓展性，避免不必要的拆分，合理规划项目结构，SQL 安全，性能安全；本地有 Docker，可以启动需要的 Redis 和数据库。

## 项目定位

- `fbz-api` 是类 Emby 的 Rust API 后端，目标是让支持 Emby 协议的客户端填写本项目地址后可以连接和使用。
- 当前后端优先做 Web/API 后端架构和协议兼容，前端管理页后续再接。
- 架构形态优先保持 modular Rust monolith，使用 `axum + tokio + tower-http + tracing`，逐步扩展模块边界。
- 基础依赖是 PostgreSQL 和 Redis；Docker 当前主要用于本地依赖和后续生产部署验证。
- 目标部署环境包括 Windows、Linux、Docker、NAS。

## 已确认硬约束

- Emby-only compatibility：优先兼容 Emby REST API 和常见客户端行为。
- v1 支持 XML 请求体解析，但响应仍以当前 JSON DTO 为主。
- PostgreSQL / Redis 是基础依赖。
- `FFMPEG_PATH=ffmpeg`、`FFPROBE_PATH=ffprobe` 作为默认值，并支持外部覆盖。
- 支持 TMDB / TVDB / Fanart 元数据源，API base URL 可由管理员替换为镜像。
- 支持系统 HTTP 代理配置。
- STRM 默认只允许内网链接；公网域名必须通过安全域名 allowlist 配置。
- 支持硬件转码，默认硬解，不可用时回落软件转码。
- 转码默认最多 3 个并发，其余排队。
- 插件系统需要支持外部插件、hook、计划任务、Host API、通知插件、后台菜单，但必须约束插件边界。
- 通知目标包括 Telegram、企业微信和通用 webhook，但敏感目标配置由管理员管理，插件只提交受控通知请求。
- 多用户权限包括服务器管理权限、媒体库权限、下载/转码权限、新设备登录权限和 Emby 用户策略映射。
- 预估规模：最多约 1000 人同时使用；媒体量可能达到 PB 级、电影/电视剧/音乐条目几十万到百万级。

## 执行原则

- 先读当前代码和文档，再修改。
- 每轮只推进一个清晰方向，完成后运行验证。
- 不为“未来可能需要”过度拆分；只有重复、复杂度或边界真实存在时再抽象。
- 控制器/路由层只做认证、参数解析、DTO 映射和错误转换；业务和 SQL 边界放到 service/repository。
- Repository 查询必须做权限过滤，不能让上层靠事后过滤兜底。
- 所有用户输入进入 SQL 必须走 bind 参数、枚举 allowlist 或规范化 filter，不拼接 SQL 文本。
- 大表和高增长审计表优先 keyset pagination，避免 offset 扫描。
- 对可能很大的列表，不要把全量 ID 拉回应用层再过滤。
- 迁移文件可以新增；执行生产或真实数据库结构变更前要明确确认。
- 默认不做 git commit、push、reset、checkout，除非用户明确要求。

## 另一台电脑继续执行步骤

1. 拉取或同步仓库后进入后端目录：

```powershell
cd C:\Code\fbz\fbz-api
```

2. 让 Codex 先读取这些文件：

```text
docs/plans/backend-execution-goal.md
README.md
docs/database-scale.md
docs/plugin-system.md
docs/plugin-development.md
```

3. 先检查当前工作树和依赖状态：

```powershell
git status --short
cargo test --quiet
cargo build --quiet
```

4. 如果需要本地依赖，可以启动 PostgreSQL 和 Redis。当前开发依赖优先用 `./scripts/dev-deps.ps1`；脚本会调用 `docker-compose.dev.yml` 并等待 Postgres/Redis healthy。如果脚本不可用，再按 `.env.example` 的 `DATABASE_URL` / `REDIS_URL` 启动本地服务。

5. 每轮修改后至少运行：

```powershell
cargo fmt
cargo test --quiet
cargo build --quiet
cargo fmt -- --check
git diff --check
```

涉及 PowerShell 脚本时，再运行相关脚本测试，例如：

```powershell
./scripts/dev-deps.test.ps1
./scripts/smoke-plugin-signature.test.ps1
```

涉及 HTTP 插件 helper 或示例时，再运行：

```powershell
node --test examples/plugins/_shared/fbz-plugin-http.test.mjs
node --test examples/plugins/first-party-notifier-templates.test.mjs
node --check examples/plugins/_shared/fbz-plugin-http.mjs
node --check examples/plugins/http-notification-bridge/server.mjs
node --check examples/plugins/http-marker-importer/server.mjs
node --check examples/plugins/telegram-notifier-template/server.mjs
node --check examples/plugins/wecom-notifier-template/server.mjs
node --check examples/plugins/webhook-notifier-template/server.mjs
```

## 当前已推进的方向

- 基础 Rust 后端骨架：健康检查、配置、日志、优雅关闭。
- Emby 兼容路由逐步扩展：系统信息、认证、用户、设备、媒体库、Items、Genres、Artists、Persons、播放状态、图片、下载、PlaybackInfo、转码、音乐相关入口等。
- Emby 启动流程已补 `Users/{Id}/Views` 和 `UserViews?UserId=...` 媒体库视图入口。
- Emby 系统探活和版本说明已补 `System/Ping` GET/HEAD/POST 空成功响应，以及 `System/ReleaseNotes` / `System/ReleaseNotes/Versions` 认证读取入口，返回当前 FBZ API 版本的受控 `PackageVersionInfo` 记录，避免启动探测或更新说明探测落到 404。
- Emby 系统配置服务已补 `System/Configuration/{Key}`、`POST System/Configuration` 和 `POST System/Configuration/Partial`；命名读取按认证用户返回受控系统/metadata/transcoding/branding 配置切片，写入口按管理员权限和体积上限校验后返回受控冲突，避免真实运行配置持久化模型外制造未落库成功状态。
- Emby 管理页功能探测已补 `Features`，按管理员权限返回静态 `FeatureInfo[]`，暴露核心后端和 Emby 兼容系统能力，后续可接真实 feature/license/plugin feature 聚合。
- Emby 插件管理探测已补 `Plugins`、`Plugins/{Id}/Configuration`、`Plugins/{Id}/Thumb`、`DELETE Plugins/{Id}` 和 `Plugins/{Id}/Delete`；列表按管理员权限映射 FBZ 插件摘要，配置读取返回内部插件配置值，写配置和卸载暂保持受控冲突响应，避免绕过 FBZ 插件审批、签名和启停生命周期。
- Emby 包管理探测已补 `Packages`、`Packages/{Name}`、`Packages/Updates`、`Packages/Installed/{Name}`、`Packages/Installing/{Id}` 和 `Packages/Installing/{Id}/Delete`；包目录按认证用户返回受控 FBZ Core 系统包并支持官方 `PackageType`、`TargetSystems`、`IsPremium` 和 `IsAdult` 过滤，更新列表返回空数组，安装入口保持受控冲突响应并指向 FBZ 签名插件包链路，取消安装为认证管理员 no-op。
- Emby DLNA profile 管理探测已补 `Dlna/ProfileInfos`、`Dlna/Profiles/Default`、`Dlna/Profiles/{Id}`、`POST Dlna/Profiles`、`POST Dlna/Profiles/{Id}` 和 `DELETE Dlna/Profiles/{Id}`；读取入口按管理员权限返回受控 FBZ 默认 DLNA profile，写入和删除保持受控冲突响应，避免真实 profile 持久化、设备识别规则和 DLNA 转码策略未设计完成前制造未落库成功状态。
- Emby 环境/目录浏览探测已补 `Environment/DefaultDirectoryBrowser`、`Environment/Drives`、`Environment/DirectoryContents`、`Environment/ParentPath`、`Environment/NetworkDevices`、`Environment/NetworkShares` 和 `Environment/ValidatePath`；入口按管理员权限保护，目录浏览只读且单次最多返回 1000 项，网络发现暂返回空数组，路径验证只检查存在性、文件/目录类型和只读属性，不创建探针文件。
- Emby 编码设置探测已补 `Encoding/CodecConfiguration/Defaults`、`Encoding/CodecInformation/Video`、`Encoding/ToneMapOptions`、`Encoding/FullToneMapOptions`、`Encoding/PublicToneMapOptions`、`Encoding/CodecParameters` 和 `Encoding/SubtitleOptions`；读入口按官方认证边界返回空/默认 DTO，写入口按管理员权限和体积上限校验后返回受控冲突，避免真实转码配置模型、硬件检测和字幕设置持久化未设计完成前制造未落库成功状态。
- Emby 活动日志管理探测已补 `System/ActivityLog/Entries`；入口按管理员权限保护，受控解析 `StartIndex`、`Limit` 和 `MinDate`，当前返回空 `QueryResult<ActivityLogEntry>`，后续可接真实活动日志 repository 和 keyset 查询。
- Emby 通知管理探测已补 `Notifications/Types`、`Notifications/Admin`、`Notifications/Services/Defaults` 和 `Notifications/Services/Test`；类型查询按认证用户返回空分类，默认服务返回禁用的 FBZ Host Notifications，管理员通知和服务测试入口按管理员权限解析官方字段后返回受控冲突，避免真实投递模型外绕过 FBZ 通知目标审批与审计。
- Emby 本地化启动/设置探测已补 `Localization/Countries`、`Localization/Cultures`、`Localization/Options` 和 `Localization/ParentalRatings`，按认证用户边界返回常见国家、语言和分级静态 DTO，后续可接真实多语言/分级配置。
- Emby Live TV 探测已补 `LiveTv/Info` / `GuideInfo` 禁用信息，`LiveTv/Folder` 禁用顶层直播文件夹，`Channels` / `Programs` / `Programs/Recommended` / `RecommendedPrograms` / `UpcomingPrograms` / `EPG` / `ChannelTags` / `ChannelTags/Prefixes` / `ChannelMappingOptions` / `ChannelMappings` / `ListingProviders` / `ListingProviders/Available` / `ListingProviders/Default` / `ListingProviders/Lineups` / `Manage/Channels` / `TunerHosts` / `TunerHosts/Default/{Type}` / `TunerHosts/Types` / `Tuners/Discover` / `Tuners/Discvover` / `Recordings` / `Recordings/Folders` / `Recordings/Series` / `Timers` / `Timers/Defaults` / `SeriesTimers` 空列表或受控默认 DTO，`POST LiveTv/Programs` 兼容空 EPG 查询，以及 `LiveTv/Programs/{Id}` / `Channels/{Id}` / `Recordings/{Id}` / timer 详情受控 not found；直播源、频道映射、节目单 provider、频道管理、录制、timer、series timer、tuner host 和 tuner reset 写/删入口返回受控冲突，避免未接真实直播源模型前客户端能力和节目详情探测走缺省 404 或制造未落库成功状态。
- Emby 首页内容服务已补 `Users/{Id}/HomeSections` 和 `Users/{Id}/Sections/{SectionId}/Items`；section 列表按认证用户返回 latest/resume/favorites/library 常见分区，分区 items 通过 section allowlist 映射回已有用户可见 Items 查询，继续由 repository 权限过滤兜住媒体库边界。
- Emby 首页兼容入口已补 `Users/{Id}/Suggestions`，复用权限过滤列表查询返回最近加入推荐窗口。
- Emby 首页两大高频栏目已补查询形入口,与既有路径形并存(很多客户端用 query 形)：最近加入 `Items/Latest?UserId=...`(对应 `Users/{Id}/Items/Latest`)和继续观看 `Items/Resume?UserId=...`(对应 `Users/{Id}/Items/Resume`)。各抽出 `latest_items_response` / `resume_items_response` 共享逻辑,query 形经 `authenticate_query_user` 鉴权,复用同一套权限过滤、排序和分页;`MediaListQuery` 补 `UserId` 字段;`emby_items_latest_query_user_alias_exists` 集成测试覆盖四条 URI;README 当前能力同步。
- Emby 音乐即时混音已把 `MusicGenres/{Name}/InstantMix` 从兼容空结果升级为真实流派种子混音：路径流派名作为权威种子（覆盖客户端传入的流派过滤，空名返回空结果），复用 `Songs?Genres=...` 的 `list_items_for_authenticated_user` Audio 查询链路，权限过滤与 DTO 映射与既有歌曲列表一致。
- Emby 音乐即时混音的 bare `Artists/InstantMix?Id=`（艺术家 `public_id` 种子）和 `MusicGenres/InstantMix?Id=`（流派 id 种子）也已从兼容空结果升级为真实混音，复用 `artist_ids` / `genre_ids` 权限过滤 Audio 查询（缺省/空种子返回空结果）；至此原 `instant_mix::empty_instant_mix` 空桩模块已全部被真实处理器取代并移除。
- Emby MoviesService 推荐入口已补 `Movies/Recommendations`，按认证用户边界返回最近加入电影推荐分类并受控解析 `UserId`、`CategoryLimit`、`ItemLimit` 和图片字段；`Movies/{Id}/Similar` 复用现有相似内容查询，避免电影详情页推荐探测 404。
- Emby 旧搜索提示入口已补 `Search/Hints`，复用权限过滤 Items 查询和 `SearchTerm` / `IncludeItemTypes` / `MediaTypes` 解析返回 legacy `SearchHints`。
- Emby 媒体库物理路径探测已补 `Library/PhysicalPaths`，按管理员权限返回启用中的媒体库路径数组，并在路由层规范化空路径和重复路径，避免普通用户侧媒体库浏览接口扩大为全局路径泄露。
- Emby 媒体库选项探测已补 `Libraries/AvailableOptions`，按认证用户返回受控默认 `LibraryOptionsResult`，包含 FBZ Metadata/Artwork、movies/tvshows/music/mixed 类型选项、默认图片选项和保守库默认配置，后续可接真实 provider/plugin 聚合。
- Emby 媒体库刷新入口已补 `POST Library/Refresh`，按管理员权限触发现有 `core.library.incremental_scan` 计划任务，复用调度租约、并发限制和扫描 job 入队链路，并保持官方 200 空响应形态。
- Emby 单项元数据刷新入口已补 `POST Items/{Id}/Refresh`，按服务器管理员权限解析官方 `Recursive`、`MetadataRefreshMode`、`ImageRefreshMode`、替换标记和 `BaseRefreshRequest` 字段，复用现有 `metadata.refresh` job 入队链路，并保持官方 200 空响应形态。
- Emby ImageService 图片信息入口已补 `Items/{Id}/Images`，按当前用户媒体库权限返回 `ImageInfo[]`，并将内部 artwork 类型映射为官方 `ImageType` / 同类型 `ImageIndex`；`Items/{Id}/Images/{Type}` 与 `Items/{Id}/Images/{Type}/{Index}` 的 `HEAD` 探测也纳入路由覆盖，继续复用现有本地 artwork / 安全远端图片读取边界；`POST/DELETE Items/{Id}/Images/{Type}`、`POST/DELETE Items/{Id}/Images/{Type}/{Index}`、`Images/{Type}/Delete`、`Images/{Type}/{Index}/Delete`、`Images/{Type}/{Index}/Index` 和 `Images/{Type}/{Index}/Url` 写入口探测已补管理员保护和字段规范化，当前返回受控冲突，避免真实 artwork 写模型、缓存落盘和元数据审计未设计完成前制造未落库成功状态。
- Emby RemoteImageService 已补 `Items/{Id}/RemoteImages`、`Items/{Id}/RemoteImages/Providers`、`Items/{Id}/RemoteImages/Download` 和 `Images/Remote`；读入口按认证用户和媒体库可见权限返回空远程图片结果与受控 provider 列表，下载/代理入口按管理员权限解析官方字段后返回受控冲突，避免真实 artwork provider、缓存和元数据写链路未设计完成前制造假成功或绕过权限。
- Emby ItemLookupService 已补 `Items/{Id}/ExternalIdInfos`、`Items/RemoteSearch/Image`、`Items/Metadata/Reset`、`Items/RemoteSearch/Apply/{Id}` 和类型化 `Items/RemoteSearch/{Book|BoxSet|Game|Movie|MusicAlbum|MusicArtist|MusicVideo|Person|Series|Trailer}`；外部 ID 信息返回 TMDB/TVDB/IMDb 受控 identifier 列表，类型化远程搜索按认证用户解析官方 body 后返回空 `RemoteSearchResult[]`，图片代理、应用搜索结果和元数据重置入口按管理员权限解析官方字段后返回受控冲突，避免真实 provider 搜索、图片缓存和元数据重置链路未设计完成前制造未落库成功状态。
- Emby 媒体库结构读取已补 `Library/VirtualFolders` 和 `Library/VirtualFolders/Query`，复用当前用户可见媒体库权限过滤，返回 `VirtualFolderInfo` 的 `Locations`、`LibraryOptions.PathInfos`、`ItemId/Id/Guid` 和刷新状态，并对 Query 入口支持 `StartIndex` / `Limit` 兼容分页。
- Emby 在线频道探测已补 `Channels`，按认证用户边界返回空 `QueryResult<BaseItemDto>`，并受控解析 `UserId`、`StartIndex` 和 `Limit`，后续可接真实 channel/provider 聚合。
- Emby 用户数据读取/写入已补 `Users/{Id}/Items/{ItemId}/UserData`，复用权限过滤目标查询返回或更新播放进度、播放次数、收藏、评分和已看状态；`Users/{Id}/Items/{ItemId}/HideFromResume` 已补 `Hide=true/false` 兼容入口，当前以受权限保护的播放位置清零方式隐藏继续观看项。
- Emby UserLibraryService 共享/访问写入口已补 `Items/Access`、`Items/{Id}/MakePrivate`、`Items/{Id}/MakePublic` 和 `Items/Shared/Leave`，按认证用户边界解析官方 item/user/access payload 后返回受控冲突，避免真实 item share 权限模型未设计完成前客户端探测 404/405 或制造未落库成功状态。
- Emby item / playback media source DTO 已补兼容空 `Chapters` 字段，`Fields=Chapters` 会被解析，后续可接真实章节扫描入库。
- Emby item / playback `MediaSourceInfo` 已补 `Type`、`Name`、`ItemId`、remote/open/probe/native-framerate 标记和默认音轨/字幕轨索引等基础兼容字段，便于真实客户端在 PlaybackInfo 后判断直连和播放面板状态。
- Emby 详情页兼容已补 `Items/{Id}/SpecialFeatures` 和 `Users/{Id}/Items/{Id}/SpecialFeatures`，按当前用户可见权限返回兼容空 special features 列表，为后续真实 special feature 入库保留边界。
- Emby 播放前 extras 探测已补 `Items/{Id}/Intros`、`Users/{Id}/Items/{Id}/Intros`、`Items/{Id}/LocalTrailers` 和 `Users/{Id}/Items/{Id}/LocalTrailers`，按当前用户可见权限返回兼容空 intros/local trailers 边界。
- Emby 删除菜单探测已补 `Items/{Id}/DeleteInfo`，按当前用户可见权限返回空 `Paths`，避免客户端探测 404，同时不暴露本地文件系统路径、不启用真实删除流程。
- Emby 详情页影评探测已补 `Items/{Id}/CriticReviews`，按当前用户可见权限返回空 `QueryResult<BaseItemDto>`，并受控解析 `StartIndex`、`Limit` 和 `UserId`，后续可接真实外部影评 provider 或元数据缓存。
- Emby Trailers 聚合探测已补 `Trailers`，按认证用户边界返回空 `QueryResult<BaseItemDto>`，并受控解析 `UserId`、分页窗口、父级、搜索、类型、媒体类型和图片字段，避免真实 trailer provider / trailer item 模型未接入前客户端探测 404 或伪造电影结果。
- Emby SyncService 探测已补 `Sync/Options`、`Sync/Targets`、`Sync/Jobs`、`Sync/Jobs/{Id}`、`Sync/JobItems`、`Sync/Items/Ready`、`Sync/JobItems/{Id}/AdditionalFiles` 和 `Sync/JobItems/{Id}/File` / `HEAD` 读入口，按认证用户边界返回官方空 dialog/list/query、受控 not found 或受控冲突形态，并受控解析 `UserId`、`TargetId`、`ItemIds`、文件名、分页和分类字段；`Sync/{TargetId}/Items`、`Sync/Items/Cancel`、`Sync/Jobs/{Id}` 和 `Sync/JobItems/{Id}` 取消/删除入口为认证 no-op，创建/更新同步任务、状态上报、离线动作和 job item 状态变更返回受控冲突，避免真实离线同步任务模型未设计完成前客户端探测 404/405 或制造假同步状态。
- Emby 播放队列探测已补 `Sessions/PlayQueue` 兼容空队列边界，受控解析 `Id` / `DeviceId` 查询范围，后续可接真实 client dispatcher。
- Emby CollectionService 写入口探测已补 `POST Collections`、`POST/DELETE Collections/{Id}/Items` 和 `Collections/{Id}/Items/Delete`；入口按认证用户边界解析官方 `Name`、`IsLocked`、`ParentId` 和逗号分隔 `Ids`，当前返回受控冲突，避免真实合集模型、权限审计和成员写入未设计完成前制造未落库成功状态。
- Emby 播放列表写入口探测已补 `POST Playlists`、`Playlists/{Id}/AddToPlaylistInfo`、`POST/DELETE Playlists/{Id}/Items`、`Playlists/{Id}/Items/Delete` 和 `Playlists/{Id}/Items/{ItemId}/Move/{NewIndex}`，当前按认证用户边界和官方查询字段做安全解析；写动作保持受控冲突响应，避免真实播放列表写模型、审计和权限更新未设计完成前制造未落库成功状态。
- Emby Dynamic HLS 已补官方 `/Videos/{Id}/master.m3u8` 形态、视频/音频 `master.m3u8` / `main.m3u8` / `live.m3u8` manifest 入口、`hls1/{PlaylistId}/{SegmentId}.{SegmentContainer}` segment 入口，以及 `subtitles.m3u8` / `live_subtitles.m3u8` 空字幕播放列表边界，复用转码 session 鉴权、用户可见权限和输出目录安全读取；manifest 内可识别 segment 会按视频/音频入口分别重写到 `/emby/Videos/{Id}/hls1/...` 或 `/emby/Audio/{Id}/hls1/...`；客户端结束 HLS 后可通过 `DELETE /Videos/ActiveEncodings` 按当前用户、`PlaySessionId` 和可选 `DeviceId` 取消 queued / running 转码会话，并 best-effort 清理受限于 `TRANSCODE_CACHE_DIR` 的 session 输出目录；转码失败或状态已被取消/丢失时，worker 也会再次尝试清理输出目录并记录失败原因；`core.transcode.cleanup` 会按 `SCHEDULE_TRANSCODE_CLEANUP` 周期重试 failed / cancelled 且尚未标记清理完成的输出目录。
- Emby VideoService 文件名流入口已补官方 `Videos/{Id}/{StreamFileName}` 和大小写/`/emby` 别名：无 `TranscodeSessionId` 时复用 direct-stream 鉴权、Range、本地文件/安全 STRM 边界，带 `TranscodeSessionId` 时继续按 HLS 输出文件读取分流；`Videos/{Id}/stream` 与 `Videos/{Id}/stream.{Container}` 的 `HEAD` 播放探测已纳入路由覆盖测试。
- Emby SubtitleService 管理探测已补 `Providers/Subtitles/Subtitles/{Id}`、`Items/{Id}/RemoteSearch/Subtitles/{SubtitleId}`、`Items/Videos/{Id}/Subtitles/{Index}` 删除别名和 `Videos/{Id}/{MediaSourceId}/Attachments/{Index}/Stream`；入口按认证用户和用户可见媒体项边界保护，远程字幕下载、字幕删除和内嵌附件流当前返回受控冲突，避免真实 provider、索引和附件抽取模型未设计完成前制造未落库成功状态。
- Emby 音乐播放已显式补官方 `/Audio/{Id}/universal`，普通请求复用现有流式读取、STRM allowlist 和用户可见权限边界，`TranscodingProtocol=hls` 请求会创建 audio-only HLS 转码 session 并跳转到 `/emby/Audio/{Id}/master.m3u8`；Universal Audio 查询参数会先做 allowlist/范围规范化，完整渐进式音频转码语义后续继续补齐。
- Emby/Jellyfin 音乐歌词探测已补 `Audio/{Id}/Lyrics` 和 `Items/{Id}/Lyrics`，按当前用户可见权限读取音频同目录同名 `.lrc` / `.elrc` / `.txt` sidecar 并返回兼容 `LyricDto`；`Audio/{Id}/RemoteSearch/Lyrics` 返回受权限保护的空远程搜索结果，后续可继续补歌词扫描入库、缓存和插件 provider。
- Emby Studios 浏览已补 `Studios` 和 `Studios/{Name}`，按当前用户可见媒体项聚合制作公司，支持分页、搜索、父级范围和基础名称排序。
- Emby Tags / OfficialRatings / Years / technical facets 浏览已补 `Tags`、`OfficialRatings`、`Years`、`Containers`、`AudioCodecs`、`VideoCodecs`、`SubtitleCodecs` 和 `StreamLanguages`，按当前用户可见媒体项聚合标签、官方分级、生产年份、容器、编解码器和流语言，支持分页、搜索、父级范围和基础排序；`Items/Filters` 已补 legacy 筛选面板入口，返回 `Genres`、`Tags`、`OfficialRatings` 和 `Years`，并按 `ParentId`、`IncludeItemTypes` 和 `MediaTypes` 缩小聚合上下文。
- Emby 管理员用户列表和索引已补 `Users/Query`、`Users/ItemAccess` 与 `Users/Prefixes`，按管理员权限返回 `QueryResult<UserDto>` 或去重后的 `NameIdPair[]` 前缀，支持官方 `IsHidden`、`IsDisabled`、`StartIndex`、`Limit`、`NameStartsWithOrGreater` 和 `SortOrder` 查询字段的受控映射。
- Emby 用户 ID 登录已补 `POST Users/{Id}/Authenticate`，按官方 `Pw` payload 复用真实密码校验、设备策略、session 生成和登录 hook，仓储层使用 `public_id = case ... then $1::uuid` 的索引友好查询形态。
- Emby 忘记密码探测已补 `Users/ForgotPassword` 和 `Users/ForgotPassword/Pin`，按官方 DTO 解析 `EnteredUsername` / `Pin` 并返回 `ContactAdmin` 或失败的 PIN 兑换结果，不生成 PIN 文件、不落库修改密码，后续真实密码重置状态机设计完成后再接持久化流程。
- Emby 用户管理写入口已补 `POST Users/New`、`POST Users/{Id}`、`DELETE Users/{Id}`、`POST Users/{Id}/Delete`、`POST Users/{Id}/Configuration`、`POST Users/{Id}/Configuration/Partial`、`POST Users/{Id}/Policy` 和 `POST Users/{Id}/Password`；入口按本人或管理员边界保护，受控解析创建用户与密码 DTO、规范化路径和字段、限制 64 KiB payload，并返回受控冲突，避免真实用户生命周期、策略和密码模型外制造未落库成功状态。
- Emby API key 管理探测已补 `Auth/Keys`、`DELETE Auth/Keys/{Key}` 和 `Auth/Keys/{Key}/Delete`，按管理员权限保护并受控解析分页、`App` 和 key 路径；当前 `GET` 返回空 `QueryResult`，删除为兼容 no-op，创建长期 key 暂不启用，后续需先设计真实密钥生命周期和审计。
- Emby 显示偏好写入口已补 `POST DisplayPreferences/{Id}`，按认证用户和 `UserId` 边界接收并规范化 `SortBy`、`SortOrder`、`Client` 和 `CustomPrefs`，当前作为兼容写入边界返回官方空成功响应，后续可接真实偏好持久化。
- Emby 用户设置入口已补 `UserSettings/{UserId}`、`POST UserSettings/{UserId}`、`POST UserSettings/{UserId}/Partial`、`Users/{Id}/TypedSettings/{Key}`、`POST Users/{Id}/TypedSettings/{Key}`、`DELETE Users/{Id}/TrackSelections/{TrackType}` 和 `POST Users/{Id}/TrackSelections/{TrackType}/Delete`，按认证用户边界返回兼容空设置字典或接收受尺寸上限保护的 key/value、typed setting、binary payload，并对 Audio/Subtitle track selection 清理返回 no-op 成功；后续可接真实用户偏好持久化。
- Emby 会话详情已补 `Sessions/{Id}`，按当前用户返回单个活跃 session，仓储层同时约束 session 归属、过期、撤销和设备撤销状态。
- Emby 会话结束已补官方 `Sessions/Logout` 空响应形态，使用当前 access token 撤销会话，避免客户端登出阶段收到非官方 JSON 响应。
- Emby 播放前网络测速已补 `Playback/BitrateTest?Size=...`，按认证用户返回受 64 MiB 上限保护的流式二进制响应，避免自动码率探测阶段 404 或无界内存分配。
- Emby BifService 已补 `Videos/{Id}/index.bif` 和 `Items/{Id}/ThumbnailSet`；入口按认证用户和媒体库可见权限保护，`Width` 参数先做必填、正数和上限校验，`ThumbnailSet` 在尚未生成缩略图时返回空 `ThumbnailSetInfo`，`index.bif` 返回受控 not found，避免被通用视频文件名路由误吞或制造假 BIF 成功状态。
- Emby live stream 媒体信息入口已补 `LiveStreams/Open`、`LiveStreams/MediaInfo` 和 `LiveStreams/Close` 的认证保护兼容空边界，避免未接真实 live source 状态机前客户端播放媒体信息探测 404。
- Emby 播放 session 保活已补 `Sessions/Playing/Ping`，按认证用户接收并规范化可选 `PlaySessionId`，避免客户端播放保活阶段 404，后续可接真实 session state 心跳更新时间。
- Emby 老式播放上报入口已补 `Users/{Id}/PlayingItems/{ItemId}`、`Users/{Id}/PlayingItems/{ItemId}/Progress`、`DELETE Users/{Id}/PlayingItems/{ItemId}` 和 `Users/{Id}/PlayingItems/{ItemId}/Delete`，复用现有播放开始/进度/停止写入与 hook 派发逻辑，并支持路径参数加 query-only 或 JSON body 的客户端上报形态，同时保留常见播放状态扩展字段。
- Emby 播放上报 DTO 已兼容 `Item` 对象补 `ItemId`，并解析 `QueueableMediaTypes`、`CanSeek`、`EventName`、音轨/字幕轨、静音、音量、直播流、播放队列位置、当前播放队列、重复/随机/睡眠定时和播放速率等常见客户端状态字段，后续可接真实 session state。
- Emby Sessions 远程控制入口已补 `/Sessions/{Id}/Playing`、`Playing/{Command}`、带 session id 或不带 session id 的 `Command/{Command}`、`System/{Command}`、`Message` 和 `Viewing` 的兼容 no-op 边界。
- Emby session additional-user 入口已补 `Sessions/{Id}/Users/{UserId}`、`DELETE Sessions/{Id}/Users/{UserId}` 和 `Sessions/{Id}/Users/{UserId}/Delete` 的认证保护兼容空响应边界，后续可接真实多人 session state。
- Emby 原始文件入口已补官方 `Items/{Id}/File`，复用 `Items/{Id}/Download` 的用户/媒体库下载权限、Range、本地文件/安全 STRM 302 和下载 hook 边界，避免原始文件读取绕过下载权限。
- `/ready` 已真实探测 PostgreSQL `select 1` 和 Redis `PING`，并支持 `FBZ_READINESS_TIMEOUT_MS`。
- `/ready` 已返回 worker 配置/节点角色运行条件和队列 backlog 摘要，便于 Docker/NAS 多节点部署排障。
- `/ready` 已补 `runtime.roles`，按 `FBZ_NODE_ROLE=all/api/worker/scheduler` 明确暴露当前进程承担的 api、worker、scheduler 职责，便于 API-only、worker-only 和 scheduler-only 节点排障。
- `/ready` 已补 `runtime.queues.event_stream_mirror`，按 PostgreSQL `event_outbox` 未镜像事件统计 unmirrored、claimable、locked、backoff、failed 和 max_attempts，便于定位 Redis Streams 镜像 worker 停滞或回退。
- `/ready` 队列摘要已补按节点职责的 `drained_by_node` 标记：jobs / event_outbox / transcodes / notifications / event_stream_mirror 各队列的 backlog 计数是全局的，新增布尔标记表明当前节点是否真的运行消费该队列的 worker（均需 worker 角色，且各自对应 worker 启用）。映射按真实消费者：generic `jobs` 由 scan/metadata/probe worker 消费；transcodes 由转码 worker；notifications（`plugin_notification_requests`）由通知投递 worker；`event_outbox` 由三类消费者共享——插件 hook 派发（plugins::execution）、通知投递（notifications::delivery）和 Redis Streams 镜像（events），任一在 worker 节点启用即视为本节点消费；`event_stream_mirror`（`stream_mirrored_at is null` backlog）专属 Redis 镜像 worker。api-only / scheduler-only 节点能看到全局 backlog 但 `drained_by_node=false`，便于多节点排障时区分「本节点负责的积压」与「可见但不归本节点处理的积压」。
- HTTP 请求追踪已补 `HTTP_SLOW_LOG_THRESHOLD_MS` 慢请求阈值，超过阈值的请求会写入包含 status、latency_ms 和 threshold_ms 的 `slow http request` 结构化 warn 日志。
- SQLx 慢 SQL 观测已补 `DATABASE_SLOW_LOG_THRESHOLD_MS`，数据库连接会按配置把超过阈值的 SQL 语句写入 warn 日志，同时保留 PostgreSQL `statement_timeout` 作为硬超时。
- 通用 job lease 回收已补 `recovered stale running jobs` 结构化 warn 日志，按 job_type 记录 expired_jobs、retryable_jobs 和 terminal_jobs，便于区分可重试过期任务与达到 max attempts 的终止任务。
- 通用 job handler 失败重试路径已补结构化 warn 日志：`library.scan`、`metadata.refresh`、`media.probe` 三个 worker 的失败收尾统一走 `jobs::mark_job_failed`，释放租约后按 `attempts < max_attempts` 输出 `job failed; scheduled retry`（retryable=true）或 `job failed; max attempts reached`（retryable=false），记录 job_type、job public_id、attempt/max_attempts 和 error；此前 handler 直接报错（如扫描期间 NAS 临时不可达）只把 job 标记 failed 而无日志，仅 lease 过期路径可观测。
- 计划任务 run lease 回收已补 `recovered stale scheduled task runs` 结构化 warn 日志，记录 expired_runs、due_runs 和 manual_runs，便于区分周期调度和管理员手动触发的过期任务。
- 计划任务 run 执行失败路径已补 `scheduled task run failed` 结构化 warn 日志：`run_next_due_task`（周期调度）和 `run_task_once`（管理员手动触发）在 `mark_task_failure` 落库后统一调用 `log_scheduled_task_failure`，记录 task_key、task_type、run_id 和 error；此前执行期失败只更新 `scheduled_task_runs`/`scheduled_tasks`，仅由 worker 循环输出不带任务身份的通用 `scheduler worker failed to dispatch task`，手动触发路径无日志。
- 转码 lease 回收已补 `recovered stale transcode sessions` 结构化 warn 日志，记录 expired_sessions、retryable_sessions 和 terminal_sessions，便于区分可重新排队的过期转码与达到 max attempts 的终止转码。
- 转码 worker 失败路径已补 `transcode session failed` 结构化 warn 日志上下文，记录 session、user、item、media_file_id、worker、attempts/max_attempts、硬件加速、编码容器和 bitrate，便于区分硬件/编码/特定媒体项导致的失败。
- 插件 execution run lease 回收已补 `recovered stale plugin execution runs` 结构化 warn 日志，记录 expired_runs 和 revoked_tokens，便于定位插件 worker 崩溃后执行审计与 Host Token 回收。
- Redis Streams 事件镜像 lease 接管已补 `recovered stale event stream mirror leases` 结构化 warn 日志，记录 stream key、worker id、recovered_stale_leases 和 claimed，便于定位镜像 worker 崩溃后旧锁接管。
- Redis Streams 事件镜像发布失败并进入回退等待时，已补 `event stream mirror publish failed; scheduled retry` 结构化 warn 日志，记录事件 public id、event type、stream key、mirror attempts 和 retry delay，便于定位队列重试风暴。
- 插件 hook dispatch 失败并重新入队时，已补 `plugin dispatch failed; scheduled retry` 结构化 warn 日志，记录 outbox public id、attempt/max_attempts 和 retry delay，便于定位插件执行失败重试。
- 通知投递 outbox 失败并重新入队时，已补 `notification delivery failed; scheduled retry` 结构化 warn 日志，记录 outbox public id、attempt/max_attempts 和 retry delay，便于定位通知投递重试。
- 数据库性能方向：多处列表和审计接口已改 keyset pagination；Host API 调用审计已补单条件和 `pluginId/statusCode`、`executionRunId/statusCode` 组合过滤索引。
- job 队列 claim 热路径索引已对齐：`metadata.refresh`（0015）和 `media.probe`（0023）此前各有专用 `(status, run_at, priority desc, id)` 部分 claim 索引，而 `library.scan` 只有按 libraryId 的去重索引（0021），claim 查询回退到跨 job_type 的通用 `idx_jobs_status_run_at`；新增迁移 0063 `idx_jobs_library_scan_claim` 补齐同形态部分索引，让每轮 worker 轮询的扫描 claim 在大表上保持选择性。已在本地 dockerized PostgreSQL 上实跑校验：迁移链 0001–0063 全部 `success=t`，`idx_jobs_library_scan_claim` 以预期定义创建（`btree(status, run_at, priority desc, id) where job_type='library.scan' and status in ('queued','failed')`），`enable_seqscan=off` 下规划器对扫描 claim 查询使用该索引（小表默认 seq scan 属正常）。
- 本地开发依赖恢复：`scripts/dev-deps.ps1` 可一键 start/status/restart/stop PostgreSQL 和 Redis，并等待 Docker health 到位。
- 插件系统：manifest、权限、hook、计划任务、菜单、安装审批、HTTP/WASI runtime、Host API、通知 worker、运行审计和 Host API 调用审计。
- 插件开发生态：`docs/plugin-development.md`、HTTP helper、notification bridge 示例、Telegram / 企业微信 / webhook 通知模板、marker importer 示例、打包脚本、`sign-plugin-package` 签名工具、signed package smoke wiring、Host API budget runtime smoke、plugin notification delivery smoke、plugin schedule lifecycle smoke、plugin schedule dispatch smoke 和 Node helper / 模板结构测试。
- 元数据方向：TMDB / TVDB / Fanart provider 边界、镜像 URL 和代理配置。
- STRM、下载、转码和媒体探测已有基础安全边界。

## 还需要继续执行

### 1. Emby 协议兼容补齐

- 对照官方 Emby REST API 和真实客户端请求继续补缺口。
- 优先覆盖客户端启动、媒体库浏览、音乐播放、播放控制、PlaybackInfo、字幕、图片和用户数据高频接口。
- 每个新增兼容入口都要加路由存在测试和 DTO/查询映射测试。

### 2. 播放和转码生产化

- 完善 HLS 转码 session 生命周期、取消、过期清理、输出目录清理和错误审计。
- 继续验证硬件转码参数、失败回退软件转码、3 并发排队策略。
- 对 STRM 302 安全域名、内网识别和下载权限做更多边界测试。

### 3. 媒体库扫描和元数据入库

- 继续完善大库扫描分片、missing 收敛、NAS 临时不可达保护。
- 完善 TMDB / TVDB / Fanart 匹配策略、缓存、限流、重试和 provider fallback。
- 音乐 metadata、专辑、艺术家、曲目、封面和歌词/章节等后续需要单独补齐。

### 4. 数据库规模化

- 继续检查所有 Admin API、高增长 outbox、execution run、notification attempt、job/event 表是否 keyset 化。
- Host API 调用审计已覆盖 `pluginId/statusCode` 和 `executionRunId/statusCode` 组合过滤索引；后续继续按真实 Admin 查询组合补精确索引。
- 针对 5PB / 百万级媒体量继续补复合索引、partial index、查询上限、批处理上限。
- 分区/归档设计已固化:`docs/database-partitioning-design.md` 给出高增长表(`job_events`、`event_outbox`、`job_runs`、`jobs`、`scheduled_task_runs`、`plugin_host_api_calls`、`plugin_execution_runs`、`notification_delivery_attempts`、`plugin_notification_requests`、`playback_sessions`、`transcoding_sessions`)的按时间 RANGE 分区键、保留热窗口、归档(DETACH+归档+DROP)、冷热边界、物化统计(`queue_stats_rollup` 避免扫全分区)、唯一约束/外键改造和滚动维护方案,并明确分区为结构变更、落地前需确认。应用层 claim/readiness/admin keyset 查询谓词已是时间+状态,分区裁剪天然可用,基本无需改写。
- 分区首张表已落地:经确认后实现迁移 `0064_partition_job_events.sql`,把 `job_events` 按 `created_at` 月度 RANGE 分区(PK 改 `(id, created_at)`、复用 `job_events_id_seq`、保留 jobs/job_runs 外键与四个原名 keyset/时间索引、回填现有行、建 `2026m06/07/08` + `default` 分区)。本地 dockerized PostgreSQL 实跑校验:`success=t`、relkind `p`、28 行全部保留并落 `2026m06`、`2026-07-15` 插入路由 `2026m07` 且 id 续 29、legacy 表已删;`record_job_event` 的 INSERT 未变,透明路由。`jobs.rs` 加迁移结构测试 `job_events_partition_migration_partitions_by_created_at`。
- 分区第二张表已落地(category B 首张):迁移 `0065_partition_plugin_host_api_calls.sql` 把高增长审计表 `plugin_host_api_calls` 按 `finished_at` 月度 RANGE 分区,验证并实现 `public_id` UNIQUE → 普通索引降级(随机 uuid + 审计 INSERT 无 ON CONFLICT,降级安全),保留 11 个原审计索引(含预算上限索引)与三出站外键。本地实跑校验:`success=t`、relkind `p`、11 行保留、无 UNIQUE 约束且 public_id 索引非唯一、预算索引在、`2026-08-10` 插入路由 `2026m08`、legacy 已删。`jobs.rs` 加 `plugin_host_api_calls_partition_migration_relaxes_public_id_and_partitions_by_finished_at` 测试。- 分区第三张表已落地(category B):迁移 `0066_partition_scheduled_task_runs.sql` 把调度运行历史 `scheduled_task_runs` 按 `started_at` 月度 RANGE 分区,`public_id` UNIQUE → 普通索引,保留 active/task_recent/task_started/task_status 四索引与外键。虽有 running-lease/claim 语义,但 `started_at` 插入后不变(行不跨分区),已实测 active-query 兼容:插入 running run 路由 `2026m06`、按 id 更新状态成功、lease 回收查询正确。`success=t`、relkind `p`、2 行保留、active partial 索引在。`jobs.rs` 加 `scheduled_task_runs_partition_migration_partitions_by_started_at` 测试。
- 分区第四张表已落地(category C 首张):迁移 `0067_partition_job_runs.sql` 先 drop 入站 FK `job_events_job_run_id_fkey`(安全分析:job_runs 仅经 jobs 级联删除,该级联同时删除引用方 job_events,SET NULL 路径永不触发;`job_run_id` 列保留),再把 `job_runs` 按 `started_at` 月度 RANGE 分区(无 public_id,无需唯一性降级)。本地实跑校验:`success=t`、relkind `p`、14 行保留、入站 FK 已删且列保留、`2026-08-20` 插入路由 `2026m08`、legacy 已删。`jobs.rs` 加 `job_runs_partition_migration_drops_inbound_fk_and_partitions_by_started_at` 测试。
- 至此 A(`job_events`)+ B(`plugin_host_api_calls`、`scheduled_task_runs`)+ C 首张(`job_runs`)共四表已实现并实跑校验,三种模式(纯净 / public_id 降级 / 入站 FK 安全 drop)均已证。- 分区滚动维护机制已落地:迁移 `0068` 建 `ensure_partition_coverage(months_ahead int) returns int`,幂等为四张已分区表创建当前月 + N 个未来月分区(已存在跳过),并在迁移内调用 `(18)` 把覆盖延伸到 2027m12(各表 19 月分区 + default)。本地实跑校验:函数存在、四表各 20 分区、重复调用返回 0(幂等)。`jobs.rs` 加 `partition_coverage_function_covers_all_partitioned_tables_idempotently` 测试。滚动计划任务已落地:`core.partition.maintenance`(task_type `partition.maintenance`,`SCHEDULE_PARTITION_MAINTENANCE` 默认 `daily`)在 `bootstrap_core_tasks` 注册、`run_claimed_task` 调 `ensure_partition_coverage(6)` 保持向前 6 月覆盖,受 scheduler 角色+开关门控。本地实跑校验:启用 scheduler 后任务注册(enabled=t)、强制 due 后经真实调度器派发并 `succeeded`(queued_jobs=0)。`config.rs`/`scheduler` 加 `partition_maintenance_task_is_wired_end_to_end`、`partition_maintenance_schedule_defaults_to_daily` 测试。待办:冷分区归档/`DETACH`+`DROP` 与 `queue_stats_rollup` 物化统计仍为设计。
- C 类逐张安全性分析后结论:`job_runs` 是唯一可简单安全 drop 的 C 表(已落地);`plugin_execution_runs` 入站 FK 虽可安全 drop,但另带业务唯一 `(outbox_event_public_id, attempt)`(派发幂等不变量,DB 强制),分区将被迫移除该不变量,需单独决策,暂不分区;`playback_sessions` 的入站 SET NULL 路径会被触发(经 users/media_items 级联独立删除),已用**触发器方案**落地(迁移 `0069`):先建 `BEFORE DELETE` 触发器复刻 SET NULL(置空 transcoding_sessions 引用)+ 支撑索引,再 drop 入站 FK,再按 `started_at` 分区(public_id 降级),并加入 `ensure_partition_coverage`。本地实跑校验含触发器行为实测(删除 playback_session 后 transcoding 引用被自动置空)。剩余:B 余 `notification_delivery_attempts`(0 行,达规模再做);C 余两表如上各有阻塞;D 类活跃队列(`jobs`/`event_outbox`/`transcoding_sessions`,最高风险,最后)。`database-partitioning-design.md` 已补「各表落地就绪分析」:基于实跑 schema 巡检把候选表分为 A(无 public_id 的纯 leaf,仅 job_events,已 done)、B(leaf 但有 `public_id uuid` 唯一约束,需先定唯一性降级:`plugin_host_api_calls`/`notification_delivery_attempts`/`scheduled_task_runs`)、C(有入站外键需先改造引用方:`job_runs`←job_events、`plugin_execution_runs`←host_tokens/host_api_calls、`playback_sessions`←transcoding_sessions)、D(活跃队列 `jobs`/`event_outbox`/`transcoding_sessions`,最后单独评估),给出 A→B→C→D 的实施顺序与每表具体阻塞点。
- SQL 改动要验证查询形态，避免 `public_id::text = any(...)` 这类破坏索引的写法（现有代码已无此写法，且 `library/repository.rs` 有守卫测试断言主查询不含该模式）。

### 5. 插件生态生产闭环

- 插件 smoke 已覆盖 signed package、Host API budget、通知投递、计划任务声明同步和计划任务派发执行；后续继续补真实插件模板和 WASI 模板时保持同样的端到端 smoke 标准。
- WASI 插件模板已补：首个一方 WASI 示例 `examples/plugins/wasi-scan-logger-template/`（独立 workspace 的 wasm crate，`manifest.json` + `Cargo.toml` + `src/main.rs` + `README.md`），演示 WASIp1 沙箱纯计算契约(argv/env/stdin→stdout、`/plugin`/`/data`/`/cache`/`/tmp` 预挂载、fuel/内存/epoch/stdio/模块大小上限、无网络)。两层校验：`manifest.rs::first_party_wasi_scan_logger_manifest_is_valid` 用真实校验器验证 manifest(随 `cargo test`)；`wasi.rs::wasi_scan_logger_template_executes_end_to_end` 把编译出的 `plugin.wasm` 经真实 `PluginWasiRuntime::execute` 跑通 stdin→stdout(标 `#[ignore]` 避免默认 `cargo test` 依赖 `wasm32-wasip1` 目标,已本地实跑通过)。`docs/plugin-development.md` 补「WASI 运行时契约与模板」章节。
- WASI 插件用于沙箱纯计算;联网 / Host API 插件继续优先 HTTP runtime(WASIp1 无 socket)。

### 6. 多用户和管理权限

- 管理 API 服务器管理权限已审计：`/api/admin/*` 无统一中间件，靠每个 handler 调用 `authenticate_admin`（内部校验 `can_manage_server`）；审计确认全部 handler 都强制该门控（`enable/disable_notification_target` 经 `set_notification_target_enabled` 间接校验）。已加回归守卫测试 `every_admin_route_handler_enforces_server_admin`，扫描 `routes.rs` 确保新增 admin handler 不会漏掉权限门控。
- 继续补 Emby 用户策略字段和客户端真实行为映射。
- 媒体库权限、下载、转码、新设备登录、会话撤销需要保持端到端一致。

### 7. 运行态可靠性和可观测

- `/ready` 后续可继续按节点职责扩展 lease / worker 健康摘要；worker 开关、通用队列 backlog、事件镜像 backlog 已有基础输出。
- 增加尚未覆盖的 worker 租约过期和其他队列重试的结构化日志；慢 HTTP、慢 SQL、通用 job lease、计划任务 run lease、转码 lease、插件 execution run lease、事件镜像 lease 回收、通用 job handler 失败重试（scan/metadata/probe）、转码失败、事件镜像重试、插件 dispatch 重试和通知投递重试已有基础结构化日志。
- 已补按节点职责的队列归属：`/ready` 各队列 backlog 现带 `drained_by_node`，标记当前节点是否运行消费该队列的 worker；后续可继续按节点职责裁剪 lease / worker 健康摘要的展示粒度。

### 8. 部署和运维

- 已补 `docs/deployment.md` 生产部署与运维指南：节点拓扑与 worker 双重门控、前置依赖（PG16/Redis7/FFmpeg）、
  环境变量生产关键项、持久化目录与卷挂载、FFmpeg/ffprobe 覆盖优先级、多阶段 Dockerfile 示例、
  单机 all 与多节点拆分的生产 docker-compose 示例、NAS 注意事项（媒体只读/路径一致/临时不可达重试/STRM 安全/资源约束）、
  `/health`+`/ready` 探针与结构化日志、10 步运维恢复检查清单；README 已加「生产部署」入口。
- 已补反向代理 / TLS 终止指引（nginx 关闭流式 buffering、放宽超时与 body size、`PUBLIC_BASE_URL` 对齐）、备份与恢复策略（PostgreSQL 为唯一权威状态、Redis 可重建、派生缓存可丢弃、恢复顺序）和大表迁移上线锁说明（启动期 `CREATE INDEX` 非并发会短暂持写锁，建议低峰单节点先迁移再扩容）。
- 后续可继续补：真实镜像 CI 构建与发布、按真实硬件平台的硬件转码设备透传细则、备份恢复演练脚本。

## 当前建议的下一轮任务

优先级建议：

1. Emby 兼容：继续补真实客户端高频缺口，尤其音乐播放和播放控制。
2. 数据库规模化：继续审计剩余 Admin API 和高增长表的分页/索引形态。
3. 运行态可靠性：继续补尚未覆盖的 worker 租约过期和队列重试结构化日志。
4. 部署和运维：补生产 Docker/NAS 部署说明、卷挂载和本地依赖恢复脚本。

每一轮只选一个方向推进，完成后更新本文或相关文档。
