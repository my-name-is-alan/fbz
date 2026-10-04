# 光鸭云盘媒体源插件设计

2026-10-04。状态：首版实现已落地（2026-10-04）。下文保留目标设计；当前实现与限制以文末记录及 fbz-api/plugins/guangya-storage/README.md 为准。

## 目标与选择

在 FBZ 管理端完成光鸭扫码登录、选择已刮削目录、创建媒体库。FBZ 导入云端 NFO、图片、字幕与媒体信息，播放时按文件 ID 获取临时直链，再经 Emby 播放入口 302 到 CDN。视频不需要落盘，也不需要 CD2、FUSE 或系统盘符。

推荐原生媒体源插件。现有 WebDAV/STRM 可作为联调参照，但不作为最终依赖，不安装整个 hack-guangya 应用作为前置服务。允许本地缓存小型元数据和缩略图，这不等于下载视频或重新刮削。

## 核查依据

本地 hack-guangya HEAD 为 `25949c1`，FBZ HEAD 为 `660b47e`；两边均有未提交改动，保留原状。公开参考：https://github.com/my-name-is-alan/hack-guangya 。

- `H:/Code/hack-guangya/server/server.mjs`：startDeviceLogin、pollDeviceLogin、refreshSavedSession、getCloudDownload、目录 API 适配。扫码授权使用 device code；刷新使用进程内共享 promise。
- `server/guangya-protocol.mjs`：光鸭请求头、客户端协议与认证失效码边界。
- `server/download-url-cache.mjs`：签名到期解析、LRU、同文件并发合并。当前缓存最低 30 秒的逻辑可能将缓存时间延伸到签名真实过期时间之后，新插件不得照搬这个下限。
- `server/directory-cache.mjs` 与 `remote-directory-cache.mjs`：目录缓存、账号切换时隔离、不完整分页不判断删除。
- `server/webdav-read.mjs`：HEAD 读取索引信息、GET 302、中转时传 Range、403/410 强制更新一次。
- `server/virtual-library.mjs`：已实现稳定签名入口在请求时解析 CDN 地址，而不是永久把过期 CDN URL 写入 STRM；可以借鉴此时序。
- `fbz-api/src/scan/service.rs`：扫描依赖本地 fs::read_dir；当前不能扫描云目录。
- `fbz-api/src/plugins/manifest.rs`、`plugins/host.rs`：支持 http/wasi 和事件、元数据、KV 等能力，尚无存储源注册、目录列举、按需播放解析协议。
- `fbz-api/src/plugins/repository.rs`、`notifications/secrets.rs`：已有 plugin_config_secrets 和 XChaCha20-Poly1305 加密，可扩展为账号作用域，不能把 token 放普通插件 JSON 或 KV。
- 全部 FBZ Rust 源码检索没有发现 NFO 文件解析导入实现；metadata/provider.rs 主要为远程 provider。需要新增 NFO 导入器。

执行了以下现有测试：download-url-cache、remote-directory-cache、guangya-protocol、webdav-read、virtual-library，共 29 项通过。未据此推断当前账号的权限、官方 QPS 或 CDN 行为已验证。

## 宿主与插件职责

FBZ 负责用户/库权限、账号密文、挂载配置、媒体索引、NFO 解析、缩略图缓存、任务状态和播放进度。光鸭插件负责设备授权协议、账号身份验证、目录分页、文件 stat、获取原始文件直链和上游错误归类。

首版建议用独立 HTTP 插件进程：容易复用 Node 协议与测试，先由 Compose 或服务管理器托管，监听受限网络并使用每实例认证。现有 HTTP 插件运行时不应被描述为自动安装并启动任意可执行程序；若需要“一键安装即启动”，还要实现插件进程生命周期。WASI 的网络/宿主能力尚需扩展，不宜作为首版前提。

新增通用 StorageProvider 协议，而不是把光鸭专有路径写进本地 scanner：

- capabilities：分页、增量能力、Range、直连/中转、可用的媒体技术信息。
- auth.start / auth.poll / auth.refresh / auth.disconnect：宿主控制的账号授权操作。
- list(accountRef, parentId, cursor)：文件 ID、名称、类型、大小、修改时间、版本标识与下一页。
- stat(accountRef, fileId)：查文件元信息。
- resolve(accountRef, fileId, playbackContext)：返回 ephemeral URL、真实有效期（若已知）、必要请求头、是否支持重定向。不得把 OAuth 令牌返回给播放器。
- readSmallFile：宿主控制的有上限小文件读取，用于 NFO、字幕和图片。不能成为无限制的任意 URL 代理。

这些调用是点播请求/扫描任务的 RPC，不应塞进异步播放开始 hook：播放需要先拿到地址才会开始。

## 账号和扫码

非秘密账号记录建议包含 account_id、plugin_id、云端 user_id、显示名、稳定 device_id、状态、expires_at、credential_version。每个账号独立持有 token，不能共享全局 token 变量。

密文存 access_token、refresh_token；主密钥沿用 FBZ_SECRET_KEY，从部署 secret 注入，不与数据库备份一同明文存放。通过 AAD 绑定 plugin/account/字段，限制解密用途，支持 key_id 与轮换。缺主密钥拒绝保存凭据，禁止降级明文。插件普通配置仅保存 accountRef，不在 UI 回显令牌。

扫码流程：管理员创建短期 login_attempt → 服务端取得 device_code 与验证地址 → 页面显示二维码/用户码 → 服务端按上游 interval 轮询 → 成功后读取身份并确认绑定账号 → 原子保存 token 对并刷新账号状态。device_code 留服务端，Redis 短期存储须加密或使用不落盘存储；页面只拿 attempt_id、二维码地址、状态与到期时间。对 pending、slow_down、拒绝、到期分别处理。

刷新采用账号级 singleflight；多进程用 Redis 锁和持久化 credential_version 条件更新，防止旧 refresh_token 覆盖轮换后的值。获取锁后再次检查版本。仅明确失效才要求重新扫码；网络故障保留账号和目录，显示暂不可用。更换到不同云端用户必须新建账号绑定，不能把原有媒体的 fileId 自动解释为新账号文件。

退出/解绑清除密文、URL 缓存、进行中的授权任务；账号失效不会删除已索引媒体。已发出的 CDN 链接可能在其过期前继续可用，FBZ 无法保证即时撤销它。

## 选目录与索引

管理端路径：插件 → 光鸭 → 添加账号 → 扫码 → 浏览云端目录 → 选择电影/剧集根目录 → 预览 NFO/海报匹配 → 创建媒体库。

目录树懒加载、分页、面包屑，按账号和目录 ID 缓存；打开选择器不递归扫描整个网盘。保存 root_file_id 为身份、display_path 为展示。FBZ 媒体身份按 account + mount + remote_file_id 关联，避免同名或跨账号冲突；改名/移动时优先按 fileId 保留媒体 ID 与观看进度，若云端更换 ID，再按可靠的版本/内容标识核对。

建议新增 storage_accounts、storage_mounts、remote_entries（或 media_files 的独立来源关联表）、scan_checkpoints。media_files 保留本地路径分支，云端来源存 provider/account/file ID，不把 guangya:// 伪路径交给 std::fs；不把签名 CDN URL 当作文件身份。

首轮分页广度遍历，按目录与游标保存断点；每次完整成功扫描形成 generation。只有成功完成对应范围的完整枚举后才能标记缺失，分页失败、限流、超时和账号失效绝不能触发批量删除。云端没有可靠变更游标时使用周期扫描+手工刷新，不宣称实时监听。

## 已刮削资料

支持 movie.nfo、同名 .nfo、tvshow.nfo、季/集 NFO；以目录层级、季集字段和 uniqueid 建立电影/剧/季/集关系。NFO 成功导入的字段标记来源并保护，缺失字段才允许用户显式开启 TMDB 补全。默认不调用线上重新刮削，不回写云盘。

同目录识别 poster、fanart、season poster 与同名字幕；图片、字幕通过 FBZ 鉴权接口提供，缓存 key 包含账号、文件 ID 与版本。XML 禁用外部实体，限制字节大小和嵌套，错误按单文件记录，不让一个坏 NFO 使全库失败。NFO 内外部图片 URL 不自动无条件下载。

视频技术信息优先来自已有可信 NFO/上游字段；缺失时再做受预算约束的远程探测，并对 fileId+版本缓存。NFO 不保证包含准确 codec、duration 或所有轨道，不能伪造。不要默认对全库每个视频执行远程 ffprobe；它可能产生多次 Range 或大量读取。播放时按需补探测，独立控制并发、超时与字节预算。

## QPS 与并发

所有光鸭 API 调用必须经过同一个按账号/应用维度的调度器：扫码轮询、刷新、目录、详情、解析下载 URL 都算；重试也算。并发数不能替代每秒请求限制。多节点用 Redis 原子令牌桶/共享租约，不能每个 worker 各自占满额度。

首版可采用保守的可配置默认值：每账号默认 10 QPS（可配置至 20）、突发 2、总并发 2、后台扫描并发 1、小文件读取并发 2；这些是工程起点，不是官方限额。扫码轮询严格服从上游 interval/slow_down；优先级为认证恢复与播放解析、手工浏览、后台扫描，并为后台设置老化避免永久饥饿。收到 429/明确限流遵守 Retry-After，指数退避加抖动，降低速率，稳定后渐进恢复。

hack-guangya 当前把业务码 120 等归类为限流；迁移时保留类型化错误，但不能只看到“缺少 clientId”文字就无限重试真正配置错误。401/明确认证失效走一次刷新而非并发刷新；异常响应不得当作空目录。

目录列表不获取每个视频的 CDN URL；海报/NFO 按版本缓存。直链解析按 account + credential generation + fileId + 必需播放上下文合并请求。URL 缓存到期必须早于真实签名过期；未知 TTL 使用短缓存并按错误失效，不延长已到期链接。CDN 数据连接单独限并发/带宽，不把媒体分片数当作 API QPS。

## 播放与 302

客户端 PlaybackInfo → FBZ 返回自身稳定 stream 入口 → 客户端 GET/HEAD → FBZ 校验用户、媒体库与文件归属 → 插件 resolve → FBZ 返回 302 Location: 临时 CDN URL，Cache-Control: no-store → 客户端从 CDN 读取媒体。

数据库保留 fileId，播放器入口保留 FBZ 身份与权限。不要把光鸭 access_token、refresh_token 或静态全局播放密钥放入 STRM/播放地址；可复用 Emby 会话鉴权或签发短期、限文件与用户的播放票据。返回 302 之前必须鉴权。若光鸭签名 URL 自身再 302，可由客户端跟随，但不为探测最终地址额外滥发请求。

优先直连，视频带宽不经过 FBZ。Range、HEAD、Content-Range、206/416、跨域行为由目标播放器和 CDN 实际组合验证，302 本身不会保证这些能力。跳转给 CDN 时不转发 FBZ Authorization 或光鸭 OAuth token。

关键边界：

- 浏览器最终 CDN 必须满足 CORS；FBZ 的 302 加 Access-Control-Allow-Origin 不能修复最终 CDN 缺失 CORS。
- 浏览器若遇 CDN 403/410，可重新调用 FBZ 解析并在当前时间恢复；FBZ 在发出 302 后看不到该 CDN 响应。
- 原生客户端可能缓存最终 CDN URL，签名过期后 seek 是否回到 FBZ 无法保证；不能宣传所有客户端都自动续签。
- 若签名绑定出口 IP/UA/请求头，服务端拿到的链接是否能由用户设备直接使用需要实际验证。
- 编码不被浏览器支持与 302 是不同问题，转码属于另一条链路。

为不支持直连的客户端提供显式“兼容中转”模式：FBZ 按需串流 CDN 数据，保留 Range 与状态码，403/410 只在还未向客户端输出响应体时刷新重试一次；每次跳转进行目的地址校验，限制重定向跳数、超时、连接数、字节预算并传播客户端取消。不把完整视频缓存在内存。不因为一次失败就全局改成中转。

播放开始/进度/停止继续回 FBZ，云端仅提供文件内容。刷新索引、账号重登、下载 URL 变化不应清空用户观看记录。

## 实施顺序与验收

1. 扩展通用 StorageProvider、账号 secret API、媒体来源模型；用假的云盘 provider 测试分页、断点、失效与账号隔离。
2. 光鸭 HTTP 插件：扫码/刷新、目录选择、全局限流、stat/resolve；复用已核查的协议模块和测试，不共享旧应用正在运行的 SQLite 或登录库。
3. NFO/图片/字幕导入与缓存；先导入一个小型已刮削目录，核对剧季集、图片及匹配错误。
4. 接 FBZ stream 入口的按需 302 与进度回传；用浏览器和至少一个实际 Emby 客户端验证。
5. 验证限流、多 worker 同账号刷新、链接过期 seek、账号失效、目录移动、分页失败不误删；再扩大目录。

真实验收需要用户在 FBZ 内扫码并选择指定小目录。只读接入：不上传、移动、重命名、删除或重新刮削云端媒体。通过标准必须包括真实播放与续播、已有 NFO/海报展示、重启后账号恢复、账号失效时不误删、后台扫描期间播放解析仍可响应，以及记录实际 429/延迟以调整 QPS。


## 首版实施记录

- 新增 0077_storage_providers.sql 与 storage 模块、Node HTTP 插件、AdminStorage 页面、启动脚本和隔离冒烟脚本。
- 凭据以账号作用域加密；授权尝试也加密，登录 epoch 阻止断开后的旧请求重新绑定；刷新与 QPS 由账号数据库行锁跨进程协调。
- 独立 fbz_storage_test 数据库+fixture 插件验证了扫码状态、目录读取、NFO 标题/时长、海报、外挂字幕、鉴权 302、未登录 401、25 秒播放进度保存。没有用模拟 CDN URL 声称真实视频已播放。
- 真实光鸭 Device Code 请求与扫码绑定已通过。用户指定 Media Library 后，分别挂载电影和电视剧子目录，扫描完成：17 个电影/演唱会视频、232 个剧集视频。
- 当前并发为每账号串行（上限 1），QPS 默认 10，可配置 1–20；失败扫描手动继续，尚无优先级调度、周期扫描、自动最佳 QPS、云端视频代理或转码。与上文目标方案的这些差异不应隐瞒。
- 开发运行密钥存放在本机忽略目录 var/storage/keys.json，仅供本地启动脚本使用。生产须通过 secret 注入；不要提交、分享该文件，也不要丢失后换新密钥继续使用旧账号密文。


### 真实账号验收补充

- 扫码后确实恢复出账号身份并加密保存，重启 FBZ 后仍为已连接。
- 用户明确选择 Media Library，配置为 `/Media Library/电影` 和 `/Media Library/电视剧` 两个库；只读云端，共列举 100 和 314 个条目。
- 电影 NFO 与海报真实导入并在网页显示；未调用 TMDB 重新刮削。
- 真实 MKV 的 FBZ stream 返回 302，CDN Range 返回 206、Content-Range `bytes 0-1023/19614151554`，实际读取 1024 字节。
- FFmpeg 通过同一个鉴权入口直接访问 CDN，3 帧视频解码退出码 0。没有下载整部电影。
- 当前网页 Shaka 对这部 MKV 加载失败；没有据此宣称所有浏览器/编码都可播放。Infuse/VidHub 等客户端仍需实机兼容验收，云端中转/转码尚未实现。
- 修复实际目录名称含斜杠时的误拒绝：云端名称按标签处理，账号/文件 ID 决定索引身份，本地缓存路径始终为哈希生成。
- 剧集缺失分集 NFO 时按 SxxExx 文件名回退识别季集；保留既有条目 ID 与播放进度。
- 隔离测试验证了扫描失败不删除媒体、恢复后保持相同媒体 ID 与播放进度；字幕返回 200 并核对内容。


### 最终实测结果

- 两个挂载均扫描完成且无 last_error：电影 100 个条目 / 17 个视频；剧集 314 个条目 / 232 个视频，关联 13 部剧、16 季。232 个分集的季号与集号均已填充。
- 再用真实云端 MP4 分集验证网页播放：3840×2160、readyState=4、实际播放超过 16 秒；后端在播放中收到 10.331719 秒进度，停止后保存 16.91552 秒，再次打开准确恢复到 16.91552 秒。
- 因此当前网页实测范围为该 4K MP4 成功，前述 MKV 样片失败；不能推导所有编码/封装均支持。原生客户端实机兼容仍待验证。
- 云端 MediaSource 以 Http / IsRemote 返回 FBZ 鉴权入口，不向客户端暴露内部 fbz-storage URI；本地扫描入口对云端库返回 409，扫描器自身也排除云端库。
- 插件 8 项测试、Rust 存储/NFO 5 项+扫描 15 项+播放 26 项相关测试、前端 11 项测试通过；Rust/前端构建、格式与类型检查通过。
- 截图：guangya-storage-live.jpg、guangya-library-live.jpg、guangya-playback-live.jpg，均在 docs/screenshots 中，不含授权二维码或账号令牌。

## 2026-10-04 UI / webplayer 后续验收

上文 Shaka 的 MKV 失败为历史结果，已由后续实现更新：现使用 webplayer 与有权限、有区间上限的 FBZ Range 中继，真实 MKV 已在浏览器内重封装播放，并提供实际音轨与字幕选择。视频编码仍受浏览器平台限制，未实现服务端转码。详见仓库 docs/UI_SYSTEM_20261004.md。

## 挂载与媒体库分离（0079，待运行服务升级）

- 插件配置弹窗管理账号与目录。挂载以 `/cloud/...` 虚拟地址标识，不创建媒体库；媒体库添加弹窗选择尚未关联的挂载。当前一个挂载对应一个媒体库，可为同一账号添加多个目录挂载。
- 0079 保留旧挂载、媒体库、媒体条目与观看进度 ID，旧库继续关联原挂载；旧虚拟地址自动生成为 `/cloud/{mount-id}`。0078 将历史 1 QPS 默认值提升至 10，允许 1–20。
- NFO 策略实际支持读取云端已有 NFO（缺失按文件名）或只用文件名；未提供未接通的“在线刮削器优先”选项。图片支持扫描预缓存和鉴权后按需缓存，已有缓存不主动清除。
- 默认 60 分钟刷新，0 关闭自动刷新，最短 5 分钟。后台只对已关联媒体库且账号 ready 的目录排队，使用数据库锁去重，失败指数退避并保留断点。首次添加库后可手动扫描；未关联挂载不进行无意义的全目录入库扫描。
- 插件页删除演示插件列表与伪安装成功反馈，光鸭使用真实接口。账号、扫码、挂载、媒体库新增与策略编辑均使用弹窗。
- 已通过：前端 16 测试和构建；Rust 挂载地址/周期 2 测试和编译；独立旧测试库内事务验证迁移保留 3 个关联、定时入队、运行去重、失败断点恢复及手动模式，全部回滚。
- 未完成运行验收：工具自动审批连续拒绝隔离服务启动（未给具体原因），未替换正在运行的主 API，也未应用正式库迁移。新增挂载关联 API、真实图片按需读取与自动刷新端到端仍需新进程实测。smoke-storage.ps1 已更新为先挂载后创建库及按需图片策略。
