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

4. 如果需要本地依赖，可以启动 PostgreSQL 和 Redis。优先使用仓库现有 Docker 配置或脚本；如果没有现成脚本，再按 `.env.example` 的 `DATABASE_URL` / `REDIS_URL` 启动本地服务。

5. 每轮修改后至少运行：

```powershell
cargo fmt
cargo test --quiet
cargo build --quiet
cargo fmt -- --check
git diff --check
```

涉及 HTTP 插件 helper 或示例时，再运行：

```powershell
node --test examples/plugins/_shared/fbz-plugin-http.test.mjs
node --check examples/plugins/_shared/fbz-plugin-http.mjs
node --check examples/plugins/http-notification-bridge/server.mjs
node --check examples/plugins/http-marker-importer/server.mjs
```

## 当前已推进的方向

- 基础 Rust 后端骨架：健康检查、配置、日志、优雅关闭。
- Emby 兼容路由逐步扩展：系统信息、认证、用户、设备、媒体库、Items、Genres、Artists、Persons、播放状态、图片、下载、PlaybackInfo、转码、音乐相关入口等。
- `/ready` 已真实探测 PostgreSQL `select 1` 和 Redis `PING`，并支持 `FBZ_READINESS_TIMEOUT_MS`。
- 数据库性能方向：多处列表和审计接口已改 keyset pagination，并新增对应索引迁移。
- 插件系统：manifest、权限、hook、计划任务、菜单、安装审批、HTTP/WASI runtime、Host API、通知 worker、运行审计和 Host API 调用审计。
- 插件开发生态：`docs/plugin-development.md`、HTTP helper、notification bridge 示例、marker importer 示例、打包脚本和 Node helper 测试。
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
- 针对 5PB / 百万级媒体量继续补复合索引、partial index、查询上限、批处理上限。
- 必要时设计分区策略、归档策略、冷热数据边界和物化统计。
- SQL 改动要验证查询形态，避免 `public_id::text = any(...)` 这类破坏索引的写法。

### 5. 插件生态生产闭环

- 基于 HTTP helper 增加 Telegram / 企业微信 / webhook 一等插件模板。
- 补插件包签名工具或签名文档。
- 扩展插件 smoke，覆盖签名、失败重试、Host API budget、菜单、计划任务和通知投递。
- WASI 插件后续可补模板，但联网插件继续优先 HTTP runtime。

### 6. 多用户和管理权限

- 继续检查所有管理 API 是否强制服务器管理权限。
- 继续补 Emby 用户策略字段和客户端真实行为映射。
- 媒体库权限、下载、转码、新设备登录、会话撤销需要保持端到端一致。

### 7. 运行态可靠性和可观测

- `/ready` 后续可增加 worker 开关、队列 backlog、事件镜像 backlog 等摘要。
- 增加慢 SQL、慢 HTTP、worker 租约过期、队列重试和转码失败的结构化日志。
- 根据部署角色 `all/api/worker/scheduler` 区分 API 节点和 worker 节点 readiness 语义。

### 8. 部署和运维

- 补 Docker 生产部署说明和 NAS 部署注意事项。
- 明确环境变量模板、卷挂载、FFmpeg/ffprobe 覆盖、插件目录、缓存目录、转码目录。
- 增加本地开发依赖启动脚本或 compose 文档，方便另一台电脑快速恢复环境。

## 当前建议的下一轮任务

优先级建议：

1. 运行态可靠性：给 `/ready` 增加 worker/队列摘要，便于 Docker/NAS 多节点部署排障。
2. Emby 兼容：继续补真实客户端高频缺口，尤其音乐播放和播放控制。
3. 插件模板：基于 HTTP helper 增加 Telegram 或企业微信通知插件模板。
4. 数据库规模化：继续审计 Admin API 和高增长表的分页/索引形态。

每一轮只选一个方向推进，完成后更新本文或相关文档。
