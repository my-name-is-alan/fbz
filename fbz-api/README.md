# fbz-api

FBZ API 是一个 Rust 后端骨架，当前已具备基础 Emby 兼容接口、媒体库管理、插件执行、通知投递、计划任务和转码队列能力。后续可以在此基础上继续设计高可用、低延迟、低资源占用的服务架构。

## 当前能力

- `axum` HTTP 服务
- `tokio` 多线程异步运行时
- `tracing` 结构化日志
- `tower-http` CORS 与请求追踪中间件
- `x-request-id` 请求 ID 生成与透传
- `Ctrl+C` / `SIGTERM` 优雅退出
- `/health` 健康检查探针
- `/ready` 启动配置就绪探针
- 强类型启动配置与启动期校验
- PostgreSQL 连接池与 SQL 迁移
- Redis 连接与启动期 PING 验证
- 可选 Redis Streams 事件镜像 worker，用于跨节点事件分发
- 运行时 settings 表与审计表
- 用户全局新增设备登录策略与设备撤销校验
- 统一 HTTP 错误模型
- FFmpeg/ffprobe 外部优先、内置回退解析与版本诊断
- `media.probe` 队列和 ffprobe worker，用于异步写入容器、时长、码率、流信息，并关联同目录外部字幕 sidecar
- `core.metadata.refresh` 计划任务、`metadata.refresh` 元数据刷新队列与 provider 尝试链路审计
- Emby `PlaybackInfo` 按用户/媒体库转码权限创建 HLS 转码排队会话
- Emby `PlaybackInfo` 使用 probe-backed `RunTimeTicks`、`Size`、bitrate 和 `MediaStreams`
- Emby `System/Info/Public` 返回 `LocalAddress`、`LocalAddresses`、`WanAddress` 和 `RemoteAddresses` 启动发现字段
- Emby `LiveTv/Info` 明确返回未启用，`LiveTv/Channels`、`Programs`、`Timers`、`SeriesTimers`、`Recordings` 等入口返回空结果，避免未规划直播业务前客户端能力探测 404
- Emby `Auth/Providers` 返回本地认证提供者列表，并按管理员权限保护
- Emby `Devices`、`Devices/Info`、`Devices/Options` 和 `Devices/Delete` 按管理员权限管理登录设备，删除入口使用软撤销并同步撤销关联 session
- Emby `Devices/CameraUploads` 按用户权限返回当前设备空上传历史；二进制上传入口当前明确禁用，避免未设计存储边界前吞写文件
- Emby `DisplayPreferences/{Id}` 返回只读默认显示偏好，并按 `UserId` 校验当前登录用户边界
- Emby `Library/MediaFolders` 和 `Library/SelectableMediaFolders` 返回当前用户可见媒体库入口与启用中的物理路径
- Emby `/Items`、latest、resume、show 列表和 item 详情返回 probe-backed 媒体源摘要
- Emby `Items/{Id}/Ancestors` 返回当前用户可访问媒体项的媒体库根与父级链
- Emby `Users/{Id}/Items/Root` 返回用户媒体库根目录，`ParentId=root` 会映射到媒体库视图查询
- Emby `Items/{Id}/Similar` 返回当前用户可访问范围内的同库同类型相似内容，支持分页和基础排序
- Emby `Videos/{Id}/AdditionalParts` 校验用户可见权限后返回兼容空结果，为后续多文件视频入库保留接口边界
- Emby `Items/{Id}/RemoteSearch/Subtitles/{Language}` 校验用户可见权限后返回空远程字幕搜索结果，为后续字幕 provider 插件接入保留边界；`Items` / `Videos` 字幕流入口会校验媒体源和字幕 stream，当前支持按同目录安全边界读取外部字幕文件，内嵌字幕抽取留给后续 FFmpeg 任务
- Emby `Albums/{Id}/Similar` 和 `Artists/{Id}/Similar` 复用相似内容查询边界，兼容音乐客户端的专辑/艺术家推荐入口
- Emby `Items/{Id}/InstantMix`、`Songs/{Id}/InstantMix`、`Albums/{Id}/InstantMix` 复用相似内容查询边界，`Artists/InstantMix`、`MusicGenres/InstantMix` 和 `MusicGenres/{Name}/InstantMix` 返回兼容空结果，先覆盖音乐客户端的即时混合入口
- Emby `Albums` 和 `Songs` 复用 `/Users/{Id}/Items` 的权限过滤、分页、图片字段和 DTO 映射，分别固定为 `MusicAlbum` 与 `Audio` 类型；`Songs` 同时可透传 `Albums` / `AlbumIds` 过滤，兼容音乐客户端的顶层专辑/歌曲浏览和按专辑取歌入口
- Emby `Playlists`、`Playlists/{Id}/Items` 和 `Users/{Id}/Items?IncludeItemTypes=Playlist` 从 `collections` / `collection_items` 返回当前用户可见的只读播放列表和列表条目，兼容音乐客户端播放队列入口
- Emby `Items/{Id}/ThemeMedia`、`ThemeSongs` 和 `ThemeVideos` 校验用户可见权限后返回兼容空结果，为后续主题曲/主题视频入库保留边界
- Emby `Genres`、`Genres/{Name}`、`MusicGenres` 和 `MusicGenres/{Name}` 按当前用户可见媒体库聚合类型，支持分页、搜索和基础名称排序
- Emby `Artists`、`Artists/AlbumArtists` 和 `Artists/{Name}` 按当前用户可见音乐内容聚合艺术家，支持分页、搜索、基础名称排序和 `Albums` / `AlbumIds` 专辑过滤
- Emby `Artists/Prefixes` 和 `Items/Prefixes` 按当前用户可见内容聚合标题首字符，供客户端字母索引和快速跳转使用
- Emby `Persons` 和 `Persons/{Name}` 按当前用户可见媒体项聚合人物，支持分页、搜索、人物类型过滤和基础名称排序
- Emby `Items/{Id}/Images/{Type}` 和 `Items/{Id}/Images/{Type}/{Index}` 按用户媒体库权限返回本地 artwork 缓存文件或安全远端图片 302
- Emby `PlayedItems`、`FavoriteItems` 和 `Items/{Id}/Rating` 写入用户播放状态、收藏和个人评分，响应返回 `UserItemData`
- Emby `Items/Counts` 和 `Users/{Id}/Items/Counts` 返回当前用户可访问媒体数量统计，用于客户端首页和媒体库概览
- Emby `/Items`、latest、resume、show/episodes/next-up 列表接受 `IncludeItemTypes`、`SortBy`、`SortOrder` 和 `Fields` 查询参数；`/Users/{Id}/Items` 额外支持 `Ids`、`ExcludeItemIds`、`Years`、`SearchTerm`、`NameStartsWith`、`NameStartsWithOrGreater`、`NameLessThan`、`AnyProviderIdEquals`、`ImageTypes`、`EnableImages`、`ImageTypeLimit`、`EnableImageTypes`、`Genres`、`GenreIds`、`OfficialRatings`、`Tags`、`ExcludeTags`、`Studios`、`StudioIds`、`Person`、`PersonIds`、`PersonTypes`、`Artists`、`ArtistIds`、`Albums`、`AlbumIds`、`MediaTypes`、`Containers`、`AudioCodecs`、`VideoCodecs`、`SubtitleCodecs`、`IsPlayed`、`IsFavorite`、`IsFolder`、`IsMovie`、`IsSeries` 和 `Filters=IsFolder/IsNotFolder/IsPlayed/IsUnplayed/IsFavorite/IsResumable/Likes/Dislikes` 过滤，类型、图片类型、媒体类型、provider id、排序字段、人物类型、元数据名称和 filter token 通过内部 allowlist/normalized 字段转换后再进入 SQL
- Emby `Users/Me`、`Users/{Id}` 和 `Sessions` 返回当前用户详情、策略边界、真实 `EnabledFolders` 媒体库访问范围、下载/转码有效权限和活跃会话，用于客户端启动后的会话同步
- Emby `Sessions/Capabilities` 和 `Sessions/Capabilities/Full` 接收客户端设备能力上报，按当前用户有效 session 写入关联 device 能力快照
- Emby `Items/{Id}/Download` 按用户/媒体库下载权限返回本地文件或安全 STRM 302
- Emby 兼容 POST 请求体支持 JSON 与 XML 解析
- Emby `DirectStreamUrl` 视频/音频本地文件异步流式读取与基础 Range 支持
- STRM 内网链接与安全域名 302 跳转控制
- Emby HLS `TranscodingUrl` 的 manifest 与 segment 安全读取
- 插件 hook 支持扫描开始/完成/失败、元数据刷新完成/失败、播放开始/停止、下载开始、转码开始/完成/失败、用户登录和计划任务派发
- 管理员可读取插件宿主能力清单，用于安装前校验 runtime、权限、hook、HTTP scheme 和 Host API 契约
- Windows 本地热重载脚本

## 配置

通过环境变量配置服务。完整模板见 `.env.example`。

| 变量 | 默认值 | 说明 |
| --- | --- | --- |
| `FBZ_API_HOST` | `127.0.0.1` | 监听地址 |
| `FBZ_API_PORT` | `8080` | 监听端口 |
| `FBZ_READINESS_TIMEOUT_MS` | `500` | `/ready` 对 PostgreSQL 和 Redis 单次探测的外层超时，避免依赖抖动时探针挂起 |
| `PUBLIC_BASE_URL` | `http://127.0.0.1:8080` | 对客户端暴露的服务地址，后续可由管理员在后台修改 |
| `DATABASE_URL` | `postgres://fbz:fbz@127.0.0.1:5432/fbz` | PostgreSQL 连接地址，启动时会连接并执行迁移 |
| `DATABASE_MIN_CONNECTIONS` | `1` | PostgreSQL 最小连接数 |
| `DATABASE_MAX_CONNECTIONS` | `20` | PostgreSQL 最大连接数 |
| `DATABASE_ACQUIRE_TIMEOUT_SECONDS` | `5` | 从连接池获取连接的最长等待时间，避免依赖抖动时请求无限排队 |
| `DATABASE_IDLE_TIMEOUT_SECONDS` | `600` | 空闲连接回收时间 |
| `DATABASE_MAX_LIFETIME_SECONDS` | `1800` | 单个 PostgreSQL 连接最大生命周期，便于负载均衡和主从切换后回收旧连接 |
| `DATABASE_STATEMENT_TIMEOUT_MS` | `30000` | 每个 PostgreSQL session 的 `statement_timeout`，限制单条 SQL 最长执行时间 |
| `REDIS_URL` | `redis://127.0.0.1:6379` | Redis 连接地址，启动时会连接并执行 PING |
| `REDIS_OPERATION_TIMEOUT_MS` | `2000` | 单次 Redis 命令最长等待时间，适用于启动 PING、readiness 和 Redis Streams 镜像 |
| `REDIS_EVENT_STREAMS_ENABLED` | `false` | 是否启动 outbox 到 Redis Streams 的镜像 worker；仅 `all` / `worker` 节点会运行 |
| `REDIS_EVENT_STREAM_KEY` | `fbz:events` | Redis Stream key，外部消费者可以从这里订阅核心事件镜像 |
| `REDIS_EVENT_STREAM_MAX_LEN` | `50000` | Stream 近似最大保留长度，使用 `XADD MAXLEN ~` 控制内存 |
| `REDIS_EVENT_STREAM_BATCH_SIZE` | `100` | 单轮从 `event_outbox` 领取并镜像的最大事件数，最大 1000 |
| `REDIS_EVENT_STREAM_INTERVAL_SECONDS` | `5` | 事件镜像 worker 轮询间隔 |
| `REDIS_EVENT_STREAM_LEASE_SECONDS` | `30` | 事件镜像领取租约，过期后允许其他 worker 接管 |
| `REDIS_EVENT_STREAM_RETRY_BASE_SECONDS` | `5` | Redis Streams 镜像失败后的基础退避时间 |
| `REDIS_EVENT_STREAM_RETRY_MAX_SECONDS` | `300` | Redis Streams 镜像失败后的最大退避时间 |
| `FBZ_SECRET_KEY` | 空 | 用于加密通知目标 secret 的应用密钥；创建/替换通知目标时必须配置，至少 32 字符 |
| `FBZ_NODE_ROLE` | `all` | 进程角色：`all`、`api`、`worker`、`scheduler` |
| `FBZ_BOOTSTRAP_ADMIN_USERNAME` | 空 | 可选初始管理员用户名，仅在显式配置时创建 |
| `FBZ_BOOTSTRAP_ADMIN_PASSWORD` | 空 | 可选初始管理员密码，至少 12 字符，使用 Argon2 存储 |
| `FBZ_SCAN_WORKER_ENABLED` | `false` | 是否启动后台媒体库扫描 worker；仅 `all` / `worker` 节点会运行 |
| `FBZ_SCAN_WORKER_INTERVAL_SECONDS` | `5` | 扫描 worker 轮询 queued/failed 扫描任务的间隔 |
| `FBZ_SCHEDULER_ENABLED` | `false` | 是否启动计划任务调度器；仅 `all` / `scheduler` 节点会运行 |
| `FBZ_SCHEDULER_INTERVAL_SECONDS` | `5` | 调度器轮询 due scheduled tasks 的间隔 |
| `FBZ_TRANSCODE_WORKER_ENABLED` | `false` | 是否启动转码 worker；仅 `all` / `worker` 节点会运行 |
| `FBZ_TRANSCODE_WORKER_INTERVAL_SECONDS` | `5` | 转码 worker 轮询 queued sessions 的间隔 |
| `FBZ_METADATA_WORKER_ENABLED` | `false` | 是否启动元数据刷新 worker；仅 `all` / `worker` 节点会运行 |
| `FBZ_METADATA_WORKER_INTERVAL_SECONDS` | `10` | 元数据 worker 轮询 `metadata.refresh` job 的间隔 |
| `FBZ_PROBE_WORKER_ENABLED` | `false` | 是否启动媒体探测 worker；仅 `all` / `worker` 节点会运行 |
| `FBZ_PROBE_WORKER_INTERVAL_SECONDS` | `10` | 媒体探测 worker 轮询 `media.probe` job 的间隔 |
| `FBZ_PLUGIN_WORKER_ENABLED` | `false` | 是否启动插件 dispatch worker；仅 `all` / `worker` 节点会运行 |
| `FBZ_PLUGIN_WORKER_INTERVAL_SECONDS` | `5` | 插件 worker 轮询 `plugin.hook.dispatch` outbox 的间隔 |
| `FBZ_NOTIFICATION_WORKER_ENABLED` | `false` | 是否启动通知投递 worker；仅 `all` / `worker` 节点会运行 |
| `FBZ_NOTIFICATION_WORKER_INTERVAL_SECONDS` | `5` | 通知 worker 轮询 `notification.send.requested` outbox 的间隔 |
| `FBZ_NOTIFICATION_DELIVERY_TIMEOUT_MS` | `5000` | 单个通知目标 HTTP 投递超时时间 |
| `METADATA_PROVIDERS` | `tmdb,tvdb,fanart` | 元数据 provider 顺序；worker 会记录每个 provider 的 matched / not_matched / skipped 边界，当前 TMDB 和 TVDB 可执行基础匹配查询，Fanart 在已有 TMDB/TVDB ID 时补充 artwork |
| `TMDB_ACCESS_TOKEN` | 空 | TMDB v4 Read Access Token，未配置时 TMDB 自动跳过 |
| `TMDB_API_BASE_URL` | `https://api.themoviedb.org/3` | TMDB API 地址，可替换为镜像地址 |
| `TMDB_IMAGE_BASE_URL` | `https://image.tmdb.org/t/p` | TMDB 图片地址，可替换为镜像地址；metadata worker 会用它生成 poster/backdrop 远程 artwork URL |
| `TVDB_API_KEY` | 空 | TVDB API key，未配置时 TVDB 自动跳过；配置后会通过 TVDB v4 `/login` 获取 Bearer token 并缓存用于搜索 |
| `TVDB_API_BASE_URL` | `https://api4.thetvdb.com/v4` | TVDB API 地址，可替换为镜像地址 |
| `FANART_API_KEY` | 空 | Fanart API key，未配置时 Fanart artwork enrichment 自动跳过 |
| `FANART_API_BASE_URL` | `https://webservice.fanart.tv/v3` | Fanart API 地址，可替换为镜像地址 |
| `HTTP_PROXY` / `HTTPS_PROXY` | 空 | 外部 provider HTTP 客户端代理 |
| `FFMPEG_PATH` | `ffmpeg` | 外部 FFmpeg 路径，优先于内置二进制 |
| `FFPROBE_PATH` | `ffprobe` | 外部 ffprobe 路径，优先于内置二进制 |
| `FBZ_BUNDLED_FFMPEG_DIR` | `./vendor/ffmpeg` | 内置 FFmpeg/ffprobe 目录 |
| `MEDIA_ROOTS` | `D:/Media/Movies,D:/Media/TV,D:/Media/Music` | 媒体根目录列表 |
| `STRM_ALLOW_PRIVATE_NETWORKS` | `true` | 是否允许 STRM 跳转到内网、localhost 和私有地址 |
| `STRM_ALLOWED_DOMAINS` | 空 | 允许 STRM 302 跳转的公网域名列表，支持子域名 |
| `TRANSCODE_MAX_CONCURRENT` | `3` | 最大并发转码数 |
| `TRANSCODE_LEASE_SECONDS` | `900` | 转码 worker 单次领取会话的租约秒数 |
| `PLUGIN_DIR` | `./plugins` | 开发态插件目录 |
| `PLUGIN_PACKAGE_DIR` | `./var/plugin-packages` | 已安装插件包目录 |
| `PLUGIN_DATA_DIR` | `./var/plugin-data` | WASI 插件可写 `/data` 的宿主根目录，按 `pluginId` 隔离 |
| `PLUGIN_CACHE_DIR` | `./var/plugin-cache` | WASI 插件可写 `/cache` 的宿主根目录，按 `pluginId` 隔离 |
| `PLUGIN_TMP_DIR` | `./var/plugin-tmp` | WASI 插件可写 `/tmp` 的宿主根目录，按单次 dispatch 隔离并在执行后清理 |
| `PLUGIN_TMP_MAX_AGE_SECONDS` | `86400` | WASI 插件崩溃残留 dispatch 临时目录的清理 TTL；每次同插件执行前只扫描该插件自己的 tmp 目录 |
| `PLUGIN_REQUIRE_APPROVAL` | `true` | 插件启用前是否需要管理员审批 |
| `PLUGIN_ALLOW_UNSIGNED` | `false` | 是否允许安装未签名插件；生产环境建议保持 `false` |
| `PLUGIN_TRUSTED_SIGNATURE_KEYS` | 空 | 可信 Ed25519 插件包签名公钥列表，格式为 `keyId:publicKeyHex,...` |
| `PLUGIN_MAX_CONCURRENCY` | `4` | 单个插件 worker 进程同时执行 `plugin.hook.dispatch` 的最大数量 |
| `PLUGIN_WASI_FUEL` | `100000000` | 单次 WASI 插件执行的 fuel 上限，用于限制 CPU 消耗 |
| `PLUGIN_WASI_STDIO_MAX_BYTES` | `65536` | WASI 插件 stdout/stderr 各自捕获上限；超过后本次执行会 trap |
| `PLUGIN_WASI_MAX_MODULE_BYTES` | `67108864` | 单个 WASI entrypoint 文件大小上限 |
| `PLUGIN_HTTP_MAX_RESPONSE_BODY_BYTES` | `65536` | HTTP 插件响应体读取上限；超过后本次插件执行失败，避免不可信插件返回大响应占用主进程内存 |
| `PLUGIN_HOST_API_MAX_CALLS_PER_RUN` | `10000` | 单次插件执行可调用 Host API 的最大次数；超限请求返回 429 并写入审计 |
| `PLUGIN_HTTP_ALLOWED_HOSTS` | `127.0.0.1,localhost,::1,host.docker.internal` | HTTP 插件执行 allowlist；支持精确 host 和 `*.example.test` 后缀通配，公网或外部服务需显式放行 |
| `PLUGIN_SECRET_KEY` | 空 | 配置后启用 HTTP 插件请求签名；至少 32 字符，外部插件可用它校验 `x-fbz-plugin-signature` |
| `RUST_LOG` | `fbz_api=info,tower_http=info` | 日志过滤规则 |

当前阶段启动时会连接 PostgreSQL、执行 SQL 迁移、写入默认 runtime settings，并连接 Redis 执行 PING。媒体库扫描任务已使用 PostgreSQL 队列表和 `FOR UPDATE SKIP LOCKED` 领取；扫描完成后会为本次触达的 `pending` / `failed` 视频类媒体项自动创建去重后的 `metadata.refresh` job，响应和日志会返回 `metadataRefreshJobs`；同时会为新增或变更的非 STRM 媒体文件创建去重后的 `media.probe` job，响应和日志会返回 `probeJobs`，再派发 `library.scan.completed` 插件 hook。计划任务调度器当前负责把 `core.library.incremental_scan` 转换为扫描 job，也会把 `core.metadata.refresh` 转换为分批 `metadata.refresh` job，并把已启用插件的 interval / cron schedules 转换为 `plugin.hook.dispatch` outbox。计划任务执行会写入 `scheduled_task_runs` 运行租约，按 `max_concurrency` 限制多实例并发，并在租约过期后允许其他节点接管。元数据 worker 领取 `metadata.refresh` job 后会按 provider 顺序查询，匹配成功后更新 `media_items`、`media_external_ids`、TMDB/TVDB/Fanart artwork、official rating、genre、studio 和主要演职员关系，并在 job run metrics 中写入 `providerAttempts`；TMDB detail 会补充 IMDB/TVDB external IDs，TVDB 会通过 v4 token 搜索电影或剧集基础记录并写入 IMDB/TMDB remote IDs，Fanart 会在已有 TMDB/TVDB ID 且配置 API key 时按独立 `fanart` source 追加远程 artwork；未配置 token/key、缺少外部 ID 或无匹配时会形成明确的 skipped / not_matched 记录，最终无匹配会把媒体项标记为 `failed`，provider HTTP 错误仍保留 job 重试；任务最终会按结果派发 `metadata.refresh.completed` 或 `metadata.refresh.failed` 插件 hook。媒体探测 worker 默认关闭；启用后会领取 `media.probe` job，调用 ffprobe 写入 `media_files.container`、`duration_ticks`、`bitrate` 和 `media_streams`，STRM 或本地文件不可访问会记录为 skipped 结果避免重试风暴；这些 probe 字段会进入 Emby `/Items`、latest、resume、show 列表和详情响应的 `RunTimeTicks`、`Size`、`Container`、`Bitrate` 和 `MediaSources` 摘要，也会进入 `PlaybackInfo` 的 `RunTimeTicks`、`Size`、`Bitrate` 和 `MediaStreams`，并作为转码码率判断和播放完成判断的输入。Emby `Items/Counts` 会按当前用户可访问媒体库聚合电影、剧集、集、音乐、专辑、艺术家和合集数量，统计查询使用单次 PostgreSQL 聚合完成。Emby 列表入口会接受常见客户端传入的 `IncludeItemTypes`、`SortBy`、`SortOrder` 和 `Fields` 参数；`IncludeItemTypes` 会映射为内部媒体类型过滤，未知显式类型不会进入 SQL，排序字段只允许名称、创建时间、时长、年份和集序号等固定枚举；`/Items` 在没有 `ParentId` 且带 `Recursive=true` 或类型过滤时会查询所有可访问媒体库，普通根请求仍返回媒体库视图；resume / show / next-up 保留原业务排序。Emby 登录会校验用户状态、密码、`allow_new_device_login` 和设备 `revoked_at`，已撤销设备的存量 session 也不能继续通过鉴权；登录成功创建 session 后会派发不含密码和 access token 的 `user.login` 插件 hook。客户端登录后可通过 `System/Info/Public` 读取公开服务地址字段，通过 `DisplayPreferences/{Id}` 读取只读默认显示偏好；也可通过 `Users/Me` 或 `Users/{Id}` 读取当前用户详情、Emby `Policy` 和 `Configuration` 基础字段，其中 `Policy.EnableAllFolders`、`Policy.EnabledFolders`、`Policy.EnableContentDownloading` 和播放转码相关字段会根据非隐藏媒体库上的 `library_permissions.can_view` / `can_download` / `can_transcode` 叠加用户全局策略实时聚合；也可通过 `Sessions` 读取当前用户活跃 session 列表；`Sessions?UserId=...` 当前只允许查询本人，跨用户会话管理后续放入管理员接口。Emby 兼容 POST 入口可以按 `Content-Type` 解析 JSON、标准 XML 和 `+xml` vendor XML 请求体，当前响应仍保持 JSON DTO。Emby `Items/{Id}/Images/{Type}` 和 `Items/{Id}/Images/{Type}/{Index}` 会按当前用户可见媒体库权限读取 `artwork`，本地 `storage_key` 仅允许解析到 `ARTWORK_CACHE_DIR` 内并流式返回，远端 `remote_url` 仅允许 `http/https` 302 跳转。Emby `UserData` 会从 `user_playstates` 返回播放位置、播放次数、已看、收藏和个人评分；`PlayedItems`、`FavoriteItems` 与 `Items/{Id}/Rating` 会校验用户媒体库可见权限后写入状态，Rating 兼容 `Likes=true/false` 并保留 0-10 分扩展。Emby `PlaybackInfo` 会返回 `DirectStreamUrl`；本地视频文件通过 `/Videos/{Id}/stream` 异步流式读取并支持单段 `Range` 请求，音乐 track 会返回 `/Audio/{Id}/{StreamFileName}` 并复用同一套流式读取和鉴权逻辑；`/Items/{Id}/Download` 会要求用户全局 `allowDownload` 和媒体库 `canDownload` 同时启用，返回本地文件时附带安全的 `Content-Disposition`，STRM 下载会复用安全域名 302 控制，并在成功进入下载入口后派发 `media.download.started` 插件 hook。STRM 媒体源会在目标为内网地址或命中 `STRM_ALLOWED_DOMAINS` 时返回 302，其他公网目标会拒绝。Emby `PlaybackInfo` 在用户和媒体库都允许转码、且 `MaxStreamingBitrate` 低于媒体源码率时，会创建 queued `transcoding_sessions` 记录，并返回 Emby 兼容的 `TranscodingUrl`、`TranscodingSubProtocol=hls` 和 `TranscodingContainer=ts`。`TranscodingUrl` 已支持读取 HLS manifest 和 segment；manifest 中相对 segment 路径会重写为带 `TranscodeSessionId` / `MediaSourceId` / `api_key` 的 Emby URL，读取文件前会校验 session 权限、状态和输出目录边界，避免路径逃逸。转码 worker 领取时会按 `TRANSCODE_MAX_CONCURRENT` 计算全局运行数，并写入 `worker_id` / `lease_expires_at`；过期租约会按 attempts/max_attempts 重新排队或失败。转码 worker 默认关闭；启用后会按硬件优先级生成 FFmpeg HLS 参数、创建输出目录并执行，退出状态会写回转码会话。管理员可以查看计划任务并手动触发启用中的 core/plugin task。插件 worker 当前消费 `plugin.hook.dispatch` outbox，把执行审计写入 `plugin_execution_runs`，HTTP runtime 支持 `http://` 和 `https://` entrypoint，并在执行前按 `PLUGIN_HTTP_ALLOWED_HOSTS` 校验目标 host；每个 HTTP dispatch 会带稳定的 `x-fbz-plugin-idempotency-key`，配置 `PLUGIN_SECRET_KEY` 后还会为请求添加 HMAC-SHA256 签名头；插件运行时会通过短期 Host Token 访问自己的 `/api/plugin/config`、`/api/plugin/kv/{key}`、媒体库摘要、媒体项公开详情、受控元数据补丁写入、source 隔离的 artwork/marker 写入和通知请求接口；`wasi` runtime 已可执行 WASIp1 command 模块，会从 `PLUGIN_PACKAGE_DIR/extracted/{pluginId}/{version}` 解析 manifest entrypoint，把 dispatch JSON 写入 stdin，并通过 argv/env 传入 handler、plugin id、幂等键和 host base URL；WASI 执行受 `PLUGIN_TIMEOUT_MS`、`PLUGIN_MEMORY_LIMIT_MB`、`PLUGIN_WASI_FUEL`、`PLUGIN_WASI_STDIO_MAX_BYTES` 和 `PLUGIN_WASI_MAX_MODULE_BYTES` 限制，当前预打开只读 `/plugin` 包目录，按插件隔离懒创建可写 `/data` 与 `/cache`，并为每次 dispatch 创建可写 `/tmp` 且在执行后 best-effort 清理；同一插件下次执行前会按 `PLUGIN_TMP_MAX_AGE_SECONDS` 清理崩溃残留的 dispatch 临时目录，不开放网络。插件包安装要求 `packagePath` 指向 `PLUGIN_PACKAGE_DIR` 下的真实非空 ZIP 文件，服务会流式计算 SHA-256；未传 `checksumSha256` 时自动持久化实际 hash，传入时必须匹配；默认要求 `signature=ed25519:{keyId}:{signatureHex}` 命中 `PLUGIN_TRUSTED_SIGNATURE_KEYS` 并通过包签名验证，只有 `PLUGIN_ALLOW_UNSIGNED=true` 时才允许未签名包。ZIP 根目录必须包含与请求 manifest 完全一致的 `manifest.json`，解包会写入 `PLUGIN_PACKAGE_DIR/extracted/{pluginId}/{version}`，并拒绝路径逃逸、重复 entry、符号链接和超限解包。插件 manifest 可声明配置 schema，管理员 API 会按 schema 校验并保存插件配置；`secret` / `password` 字段会写入 `plugin_config_secrets` 加密表，公开配置和响应只保留 `secretRef`，插件 Host API 会在运行时按自身 `plugin_id` 解密物化配置。声明 `library.read` 的插件可以读取非隐藏媒体库和媒体项摘要，声明 `media.read` 的插件可以读取单个媒体项公开详情、外部 ID、genre/tag、marker 和 artwork 摘要但不会获得文件路径、STRM 目标或真实播放地址，声明 `metadata.write` 的插件可以补丁式写入基础元数据和外部 ID，也可以按自身 source 替换单个媒体项远程 artwork 和 marker 集合，声明 `notification.send` 的插件可以提交通知请求并写入通知 outbox。管理员可通过 `/api/admin/notification-targets` 管理 Telegram、企业微信和通用 webhook 通知目标，敏感值会写入 `notification_target_secrets`，`notification_targets.config` 只保留 `secretRef`，响应会脱敏 token、webhook URL 和 header 值。通知 worker 可消费 `notification.send.requested`，按管理员配置的 `notification_targets` 解密并投递，并把每次目标投递写入 `notification_delivery_attempts`。管理员也可以查看通知请求、目标投递尝试和失败原因，并对失败或丢弃的通知重新入队。启用 `REDIS_EVENT_STREAMS_ENABLED=true` 后，事件镜像 worker 会从 PostgreSQL `event_outbox` 使用租约领取未镜像事件，写入 `REDIS_EVENT_STREAM_KEY`，再回写 `stream_mirrored_at` 和 Redis stream id；PostgreSQL 仍是权威队列，Redis Streams 只作为跨节点、外部消费者和未来实时推送的分发层。

插件 worker 的 dispatch lease 会随 `PLUGIN_TIMEOUT_MS` 增长；worker 每轮领取前会收敛过期的 `running` execution run，并撤销对应 Host Token，避免节点崩溃后插件审计和权限状态长期悬挂。

扫描和元数据刷新 worker 在领取任务前会回收过期的 `running` job lease，把对应 `job_runs` 标记为失败，并按 `attempts/max_attempts` 允许后续 worker 重试或终止，避免节点崩溃后队列被 stale lock 长期阻塞。

媒体库扫描按 batch 执行，单个 job 最多处理 `10000` 个媒体文件；如果目录树仍有剩余路径，服务会把游标写入 continuation job 的 `payload.cursor` 并继续入队。管理员手动运行扫描 job 时，响应中的 `hasMore` 和 `continuationJobId` 表示该扫描仍有后续分片。

扫描会把文件大小和修改时间写入 `media_files.file_size` / `media_files.modified_at`。后续扫描遇到相同 `path_hash` 且文件大小、mtime、STRM 目标都未变化时，会跳过 `media_items` / `media_files` 写入，也不会重复创建元数据刷新或媒体探测任务。

同一轮分片扫描会通过 `scanId` 写入 `media_files.last_seen_scan_id` / `last_seen_at`。最后一个分片结束且所有媒体库根路径可访问时，旧文件如果没有在本轮被触达，对应媒体项会被标记为 `scan_status = 'missing'`；如果存在不可达根路径，本轮会跳过 missing 收敛，避免 NAS/挂载点临时断开导致全库误标。

## 管理接口

当前 Admin API 均要求服务器管理权限访问令牌，API key 不可访问：

- `POST /api/admin/libraries`：创建媒体库。
- `POST /api/admin/libraries/{libraryId}/paths`：添加媒体库路径。
- `POST /api/admin/libraries/{libraryId}/scan`：队列化扫描任务；扫描完成后响应中的 `metadataRefreshJobs` 表示自动入队的元数据刷新任务数，`probeJobs` 表示自动入队的媒体探测任务数，`missingItems` / `missingMarkSkipped` 表示缺失文件收敛结果，手动运行 job 时 `hasMore` / `continuationJobId` 表示还有后续扫描分片。
- `POST /api/admin/libraries/{libraryId}/metadata/refresh`：按媒体库批量队列化待刷新或失败的元数据任务，支持 `limit` 控制单次入队上限。
- `POST /api/admin/media-items/{itemId}/metadata/refresh`：为单个媒体项队列化元数据刷新任务，已存在活跃任务时返回现有任务。
- `GET /api/admin/metadata/providers`：查看 TMDB / TVDB / Fanart provider 启用状态、凭据配置状态、基础 URL 和代理配置状态。
- `GET /api/admin/users`：列出用户、角色、全局下载/转码/新增设备登录策略、设备数量和活跃 session 数，支持 `roleName`、`isDisabled`、`cursor` 和 `limit`；响应体保持数组兼容，翻页状态通过 `x-fbz-has-more` / `x-fbz-next-cursor` 响应头返回。
- `PUT /api/admin/users/{userId}/policy`：替换用户全局策略，支持 `displayName`、`isDisabled`、`allowDownload`、`allowTranscode`、`allowNewDeviceLogin`；当前管理员不能禁用自己。
- `GET /api/admin/users/{userId}/libraries`：列出用户对所有媒体库的 `canView`、`canDownload`、`canTranscode` 配置值和叠加全局策略后的 effective 权限，支持 `libraryType`、`permissionConfigured`、`cursor` 和 `limit`；响应体保持数组兼容，翻页状态通过 `x-fbz-has-more` / `x-fbz-next-cursor` 响应头返回。
- `PUT /api/admin/users/{userId}/libraries/{libraryId}/permissions`：替换用户在单个媒体库上的权限配置，使用唯一约束 upsert。
- `GET /api/admin/jobs`：按最新任务列出通用 job 队列状态、运行租约、attempts、payload 和最后错误，支持 `status`、`jobType`、`queueName`、`cursor` 和 `limit`；响应体保持数组兼容，翻页状态通过 `x-fbz-has-more` / `x-fbz-next-cursor` 响应头返回。
- `GET /api/admin/jobs/{jobId}`：查看单个 job 详情、最近运行记录和事件，元数据刷新会在 run metrics 中包含 `providerAttempts`。
- `GET /api/admin/jobs/{jobId}/runs`：查看单个 job 的运行历史，支持 `status`、`cursor` 和 `limit`；响应体保持数组兼容，翻页状态通过 `x-fbz-has-more` / `x-fbz-next-cursor` 响应头返回。
- `GET /api/admin/jobs/{jobId}/events`：查看单个 job 的事件日志，支持 `eventLevel`、`cursor` 和 `limit`；响应体保持数组兼容，翻页状态通过 `x-fbz-has-more` / `x-fbz-next-cursor` 响应头返回。
- `POST /api/admin/jobs/{jobId}/run`：开发态手动执行扫描任务。
- `GET /api/admin/scheduled-tasks`：列出 core/plugin 计划任务、下次运行时间、活跃运行数、最近运行记录、失败次数和最后错误，支持 `taskType`、`ownerType`、`enabled`、`cursor` 和 `limit`；响应体保持数组兼容，翻页状态通过 `x-fbz-has-more` / `x-fbz-next-cursor` 响应头返回。
- `GET /api/admin/scheduled-tasks/{taskKey}/runs`：查看单个计划任务最近运行历史、租约状态、耗时和错误信息，支持 `status`、`cursor` 和 `limit`；响应体保持数组兼容，翻页状态通过 `x-fbz-has-more` / `x-fbz-next-cursor` 响应头返回。
- `POST /api/admin/scheduled-tasks/{taskKey}/run`：手动触发一个已启用的计划任务，复用 scheduler 的任务执行逻辑。
- `GET /api/admin/transcoding-sessions`：列出转码会话、运行租约、attempts 和错误信息，支持 `status`、`hardwareAcceleration`、`cursor` 和 `limit`；响应体保持数组兼容，翻页状态通过 `x-fbz-has-more` / `x-fbz-next-cursor` 响应头返回。
- `POST /api/admin/transcoding-sessions/{sessionId}/cancel`：取消 queued / running 转码会话。
- `GET /api/admin/notification-targets`：列出通知目标，敏感配置脱敏，支持 `targetType`、`channel`、`enabled`、`cursor` 和 `limit`；响应体保持数组兼容，翻页状态通过 `x-fbz-has-more` / `x-fbz-next-cursor` 响应头返回。
- `POST /api/admin/notification-targets`：创建通知目标。
- `PUT /api/admin/notification-targets/{targetId}`：替换通知目标配置。
- `POST /api/admin/notification-targets/{targetId}/enable`：启用通知目标。
- `POST /api/admin/notification-targets/{targetId}/disable`：禁用通知目标。
- `GET /api/admin/notification-requests`：列出插件通知请求和最终状态，支持 `status`、`channel`、`cursor` 和 `limit`；响应体保持数组兼容，翻页状态通过 `x-fbz-has-more` / `x-fbz-next-cursor` 响应头返回。
- `GET /api/admin/notification-requests/{requestId}/attempts`：查看单个通知请求的目标投递尝试，支持 `status`、`cursor` 和 `limit`，并使用同样的分页响应头。
- `POST /api/admin/notification-requests/{requestId}/retry`：把 `failed` / `discarded` 通知请求重新入队，已成功投递过的目标会被 worker 跳过。
- `GET /api/admin/plugins`：列出插件安装状态、当前活动包版本、审批状态和启用状态，支持 `approvalStatus`、`enabled`、`runtime`、`cursor` 和 `limit`；响应体保持数组兼容，翻页状态通过 `x-fbz-has-more` / `x-fbz-next-cursor` 响应头返回。
- `GET /api/admin/plugins/capabilities`：读取当前插件宿主能力清单，用于后台安装前校验 manifest runtime、权限、hook 事件、HTTP scheme 和 Host API 契约；响应保留 `permissions` 字符串列表，并通过 `permissionDetails` 返回每个权限的分类、风险等级、说明、manifest 能力绑定和对应 Host API。
- `GET /api/admin/plugins/menu-items`：列出已审批、已启用并声明 `admin.menu` 权限的插件管理菜单项；菜单路径只能位于 `/admin/plugins/{pluginId}` 命名空间内，父级和附加权限必须来自同一 manifest 声明。
- `GET /api/admin/plugins/packages`：列出插件包版本、运行时、签名状态、安装状态和是否为当前活动包，支持 `pluginId`、`packageStatus`、`runtime`、`cursor` 和 `limit`；响应体保持数组兼容，翻页状态通过 `x-fbz-has-more` / `x-fbz-next-cursor` 响应头返回。
- `POST /api/admin/plugins/packages`：安装插件包，要求 `packagePath` 指向 `PLUGIN_PACKAGE_DIR` 下真实非空 ZIP 文件，校验或自动持久化 SHA-256；默认要求可信 Ed25519 包签名，并要求包内 `manifest.json` 与请求 manifest 一致后安全解包；安装新版本只登记待审批包，不替换当前活动版本。
- `GET /api/admin/plugins/packages/{packageId}`：查看插件包 manifest、权限、hooks、菜单和计划任务定义。
- `POST /api/admin/plugins/packages/{packageId}/approve`：审批插件包并激活安装。
- `POST /api/admin/plugins/packages/{packageId}/reject`：拒绝插件包。
- `POST /api/admin/plugins/packages/{packageId}/activate`：激活一个已审批包，可用于回滚到旧版本；若插件当前已启用，会在同一事务内同步该包的计划任务。
- `GET /api/admin/plugins/{pluginId}/config`：读取插件 manifest 配置 schema 和当前配置值。
- `PUT /api/admin/plugins/{pluginId}/config`：按插件 manifest schema 校验并保存配置值；`secret` / `password` 字段传字符串会写入加密表，传 `{"secretRef":"字段名"}` 会保留已有密钥。
- `GET /api/admin/plugin-dispatches`：列出插件 hook 派发 outbox 事件和失败原因，支持 `status`、`cursor` 和 `limit`；响应体保持数组兼容，翻页状态通过 `x-fbz-has-more` / `x-fbz-next-cursor` 响应头返回。
- `GET /api/admin/plugin-dispatches/{dispatchId}/runs`：查看单个插件派发事件的执行记录，支持 `status`、`cursor` 和 `limit`；响应体保持数组兼容，翻页状态通过 `x-fbz-has-more` / `x-fbz-next-cursor` 响应头返回。
- `GET /api/admin/plugin-host-api-calls`：列出插件 Host API 调用审计，支持 `pluginId`、`executionRunId`、`statusCode`、`cursor` 和 `limit` 查询参数；响应体保持数组兼容，翻页状态通过 `x-fbz-has-more` / `x-fbz-next-cursor` 响应头返回，后台应使用 cursor 做高增长审计表翻页。
- `GET /api/admin/plugin-execution-runs/{runId}/host-api-calls`：查看单次插件执行中的 Host API 调用明细，支持 `cursor` 和 `limit`；响应体保持数组兼容，翻页状态通过 `x-fbz-has-more` / `x-fbz-next-cursor` 响应头返回。
- `GET /api/admin/event-stream-mirror/status`：查看 Redis Stream 镜像配置和未镜像 backlog 状态，包括可领取、运行租约、回退等待和失败计数。
- `POST /api/admin/plugin-dispatches/{dispatchId}/replay`：把 `failed` / `discarded` 插件派发事件重放为新的 pending outbox，保留原失败事件审计。

## 插件 Host API

插件执行时通过短期 `x-fbz-plugin-token` 访问 Host API：

- `GET /api/plugin/capabilities`：读取当前 host 支持的插件 API 版本、runtime、HTTP scheme、权限、结构化 `permissionDetails`、hook 事件和 Host API 能力清单。
- `GET /api/plugin/config`：读取当前插件自身配置；`secretRef` 会按 `plugin_id` 解密为运行时明文值。
- `GET /api/plugin/kv/{key}` / `PUT /api/plugin/kv/{key}` / `DELETE /api/plugin/kv/{key}`：读写插件私有 KV。
- `GET /api/plugin/libraries`：声明 `library.read` 后读取非隐藏媒体库摘要。
- `GET /api/plugin/libraries/{libraryId}/items`：声明 `library.read` 后读取媒体项摘要，默认走 keyset 翻页，响应 `nextCursor` 可作为下一页 `cursor`；keyset 模式下 `totalRecordCountIsExact=false`，插件不应把 `totalRecordCount` 当作全库精确总数。显式传 `startIndex` 时保留 offset 兼容路径并在返回行存在时给出精确计数，但不建议用于大库扫描。
- `GET /api/plugin/items/{itemId}`：声明 `media.read` 后读取单个媒体项公开详情、外部 ID、official rating、genre/studio/tag、people、marker 和 artwork 摘要，不返回路径、STRM 目标或真实播放地址。
- `PATCH /api/plugin/items/{itemId}/metadata`：声明 `metadata.write` 后补丁式写入基础元数据字段、upsert 外部 ID，并可替换 genre/studio/tag/people 列表；字段白名单、数值范围、日期格式、列表大小、重复名称/人物关系和外部 ID 冲突由 Host API 校验。
- `PUT /api/plugin/items/{itemId}/artwork`：声明 `metadata.write` 后按插件 source 幂等替换单个媒体项远程 artwork；仅允许 `http/https` 图片 URL，不允许插件直接写本地缓存路径。
- `PUT /api/plugin/items/{itemId}/markers`：声明 `metadata.write` 后按插件 source 幂等替换单个媒体项 marker 集合，支持片头、片尾、广告和章节 marker。
- `POST /api/plugin/notifications`：声明 `notification.send` 后提交通知请求。

一等 HTTP 通知插件示例位于 `examples/plugins/http-notification-bridge`，展示 manifest 权限声明、HTTP 签名校验、幂等去重和受控通知 Host API 调用；HTTP marker 导入示例位于 `examples/plugins/http-marker-importer`，展示 TiDb/章节类片头片尾数据如何通过受控 Host API 写入插件私有 marker source。可用 `./scripts/package-plugin.ps1 -PluginDir examples/plugins/http-notification-bridge -Force` 生成安装 API 所需的 zip、`packagePath` 和 `checksumSha256`。

插件作者开发契约见 `docs/plugin-development.md`，系统级边界和运行模型见 `docs/plugin-system.md`。

插件管理链路可用 `./scripts/smoke-plugin-lifecycle.ps1 -StartServer` 做本地端到端验证；脚本会生成一次性 HTTP smoke 插件，使用真实 API 完成登录、安装、审批、启用、配置保存、菜单暴露和包详情校验，并在结束时停止临时 API 进程。
插件运行链路可用 `./scripts/smoke-plugin-runtime.ps1 -StartServer` 做真实 worker / Host API / 审计验证；追加 `-FailFirstAttempts 1` 可让临时 HTTP 插件先返回 500 再恢复 200，用于验证 `event_outbox` 失败重试、`plugin_execution_runs` 多次记录和 Host API 审计闭环。
HTTP 插件 helper 可用 `node --test examples/plugins/_shared/fbz-plugin-http.test.mjs` 做快速回归，覆盖验签、幂等和 Host API token 透传。

## 本地依赖

本地可以直接使用 Docker 启动开发用 PostgreSQL 和 Redis：

```powershell
cd C:/Code/fbz/fbz-api
docker compose -f docker-compose.dev.yml up -d postgres redis
```

查看健康状态：

```powershell
docker compose -f docker-compose.dev.yml ps
```

## 启动

```powershell
cd C:/Code/fbz/fbz-api
cargo run
```

健康检查：

```powershell
Invoke-RestMethod http://127.0.0.1:8080/health
Invoke-RestMethod http://127.0.0.1:8080/ready
```

## 热重载开发

推荐直接使用内置脚本：

```powershell
cd C:/Code/fbz/fbz-api
./scripts/dev.ps1
```

如果本机已经安装 `cargo-watch`，脚本会自动使用：

```powershell
cargo watch -c -w src -w Cargo.toml -x run
```

如果没有安装 `cargo-watch`，脚本会回退到 PowerShell 文件轮询，检测 `src/` 或 `Cargo.toml` 变化后重启服务。

## 路由策略

当前路由按三类边界组织：`/health` 和 `/ready` 用于探针，`/api/admin/*` 用于服务器管理能力，`/emby/*` 与无前缀 Emby 路由用于客户端兼容，`/api/plugin/*` 用于受控插件 Host API。
