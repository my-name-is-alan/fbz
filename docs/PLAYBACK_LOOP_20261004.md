# 管理端与真实播放闭环

日期：2026-10-04。视觉参考为 Lux 仓库公开的管理台截图；Vue、SCSS、接口适配代码均在 FBZ 内独立实现，未复制 Lux 源码、Logo 或图片资源。

## 已接通

- 真实登录：`POST /emby/Users/AuthenticateByName`。会话保存在 sessionStorage；勾选记住设备后才进入 localStorage，不保存密码。
- `/admin/libraries`：创建媒体库、填写服务器路径、扫描排队、轮询任务及失败信息。
- 首页与单库页：用户限定的 Items 查询、搜索、分页、Resume 继续观看。
- 播放：PlaybackInfo 获取流地址，Shaka 加载真实媒体，按保存的 ticks 续播。
- 回传：开始、暂停、每 5 秒播放进度、暂停期间心跳、停止和结束；串行提交避免停止请求被较早的进度覆盖。
- `/admin`：真实条目统计、近期活动播放会话、任务状态；新增管理员保护的 `/api/admin/playback`，查询近 90 秒有报告且未停止的会话，最多 50 条。

## 联调修复

1. 支持 `MediaBrowser` 认证方案及 `X-Emby-Authorization` 别名；标准 Authorization 优先，原有 token 优先级保留。
2. ffprobe 的 `mov,mp4,m4a,3gp,3g2,mj2` 等容器别名列表不再导致视频返回 octet-stream，使用实际扩展名判断 MIME；M4A 返回 audio/mp4。
3. `find_playback_target` 的 lateral 子查询补选 duration_ticks，修复开始播放、进度与停止上报的 SQL 500。
4. 播放失败明确提示，取消原先自动模拟播放；进度保存错误单独显示，不阻断暂停控制。

## 本地运行

依赖由 `fbz-api/docker-compose.dev.yml` 启动。后端在 127.0.0.1:8080，前端 `vp dev --host 127.0.0.1` 在 5173；Vite 代理 `/api`、`/emby`、`/health` 到后端。其他部署须配置同源反向代理或在登录页输入可访问的后端地址。

首次创建管理员使用 `FBZ_BOOTSTRAP_ADMIN_USERNAME` 与 `FBZ_BOOTSTRAP_ADMIN_PASSWORD` 环境变量。启用 `FBZ_SCAN_WORKER_ENABLED=true` 与 `FBZ_PROBE_WORKER_ENABLED=true`。本次验证扫描与探测间隔为 2 秒；未启用元数据 worker，因此测试文件的元数据刷新任务仍排队，不影响直放。

本次测试账号名 `fbz-check`，测试文件目录 `fbz-api/var/closed-loop/media`。密码不写入此文档。测试数据保留以便用户在页面复查，未修改或扫描真实媒体目录。

## 验证证据

- 通过浏览器表单创建“闭环验证”媒体库并启动扫描，library.scan 与 media.probe 均为 succeeded。
- 自生成 120 秒 H.264/AAC MP4，浏览器 video 元素显示 960×540、readyState=4、paused=false，真实解码播放。
- API 验证流 Range 返回 206、1024 字节、Content-Type video/mp4。
- 冒烟脚本保存 25 秒进度，并验证条目 UserData 与 Resume 列表。
- 浏览器从 25 秒续播，实际观察 currentTime=25.176491；后续数据库记录 30.250485 秒。
- 暂停并关闭后，数据库最终记录 36.876649 秒、is_paused=true、stopped_at 非空。
- 第二个浏览器登录会话可在管理台看到第一端的已暂停视频、0:36 / 2:00、31%。
- 1440 桌面与 390 手机视口验证；手机无横向溢出，抽屉能切换媒体库页面。
- Rust 认证 8 项、流读取 19 项、播放 25 项定向测试通过；cargo build 与 cargo fmt 检查通过。
- 前端 11 项测试通过，vp check 和生产构建通过。当前安装的 vite-plus-test 包缺少 CLI bin：`vp test` 本身报错，`vp env doctor` 正常；使用 `vp exec node node_modules/vitest/vitest.mjs run` 运行同一引擎通过。

截图：`docs/screenshots/admin-live-desktop.jpg`、`docs/screenshots/admin-live-mobile.jpg`。

## 可重复检查

准备不少于 30 秒的测试 MP4，设置 `FBZ_SMOKE_USERNAME` 与 `FBZ_SMOKE_PASSWORD`，在 fbz-api 中执行：

```powershell
./scripts/smoke-playback-loop.ps1 -MediaDirectory 'H:/Code/fbz/fbz-api/var/closed-loop/media'
```

不传 LibraryId 会创建独立测试库；复用现有测试库可传 `-LibraryId`。脚本保留库和媒体，仅关闭自己创建的播放与登录会话，不输出 token。只应用于测试库，因为它会更新测试账号的观看进度。

## 验证边界

本次跑通的是本地 MP4 直放闭环及模拟 Emby 协议客户端，不等于 Infuse/VidHub 等原生客户端实机认证。自动转码选择、HLS、外挂字幕、多版本与真实大媒体库未纳入本次端到端验证。旧 TMDB 详情页及其他管理设置仍有设计阶段逻辑，需要后续按模块接入；新真实媒体列表直接打开播放器，未把演示详情伪装成服务端详情。
