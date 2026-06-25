# FBZ API 部署与运维指南

本文覆盖 FBZ API 后端的生产部署（Docker / Linux / NAS）、环境变量模板、卷挂载、
FFmpeg/ffprobe 覆盖、多节点拓扑和运维恢复检查清单。本地开发依赖请看 `README.md`
的「本地依赖」和 `scripts/dev-deps.ps1`，本文只讲生产/类生产部署。

> 文中的 `Dockerfile` 和生产 `docker-compose` 均为**可直接采用的起点示例**，请按你的镜像仓库、
> 卷路径和密钥管理体系调整后再上线。

## 1. 部署拓扑

FBZ API 是单二进制（包名 `fbz-api`）的 modular monolith，同一镜像可作为不同角色节点部署。
角色由 `FBZ_NODE_ROLE` 控制，取值 `all` / `api` / `worker` / `scheduler`。

后台职责的**双重门控**（二者同时满足才会运行，见 `src/main.rs`）：

| 职责 | 运行条件 |
| --- | --- |
| HTTP API（含 Emby 兼容、`/health`、`/ready`） | `FBZ_NODE_ROLE ∈ {all, api}` |
| scan / metadata / probe / transcode / plugin / notification worker | 对应 `FBZ_*_WORKER_ENABLED=true` **且** `FBZ_NODE_ROLE ∈ {all, worker}` |
| scheduler（计划任务调度） | `FBZ_SCHEDULER_ENABLED=true` **且** `FBZ_NODE_ROLE ∈ {all, scheduler}` |

也就是说：worker 节点即便设了 `FBZ_NODE_ROLE=worker`，仍要逐个打开
`FBZ_SCAN_WORKER_ENABLED` 等开关；api 节点无论开关如何都不会跑 worker/scheduler。

常见形态：

- **单机 all**：一个 `FBZ_NODE_ROLE=all` 进程承担全部职责。最简单，适合个人/小型部署。
- **多节点**：多个共享同一 PostgreSQL + Redis 的进程，例如 1×`api` + N×`worker` + 1×`scheduler`，
  适合大库、PB 级媒体或高并发（预估上限约 1000 同时在线）。PostgreSQL 是权威任务队列，
  Redis Streams 是可选的跨节点事件镜像（`REDIS_EVENT_STREAMS_ENABLED=true` 时启用）。

## 2. 前置依赖

- **PostgreSQL 16**（开发与示例 compose 使用 `postgres:16-alpine`）。迁移在进程启动时自动执行
  （`db::migrate`，`src/main.rs`，基于 sqlx 迁移 + 顾问锁，幂等）。多节点首次上线建议先让单个节点
  完成迁移再扩容其余节点。
- **Redis 7**（`redis:7-alpine`，示例开启 AOF 持久化）。事件镜像关闭时 Redis 只用于运行态。
- **FFmpeg / ffprobe**：转码与媒体探测依赖。默认走 `PATH`（`FFMPEG_PATH=ffmpeg` /
  `FFPROBE_PATH=ffprobe`），可显式覆盖为绝对路径，或启用内置目录（见 §5）。

## 3. 环境变量

完整模板见 `fbz-api/.env.example`（106 项），逐项语义见 `README.md` 的「配置」章节。
切勿提交真实 `.env`。以下是上线前**必须确认**的生产关键项：

### 3.1 网络与身份

| 变量 | 生产建议 |
| --- | --- |
| `FBZ_API_HOST` | 容器内监听 `0.0.0.0`（再由反代/编排暴露），而非默认 `127.0.0.1` |
| `FBZ_API_PORT` | 默认 `8080`，与端口映射/反代一致 |
| `PUBLIC_BASE_URL` | 客户端可达的外部地址（反代后填公网/内网域名），影响给客户端下发的链接 |
| `FBZ_SECRET_KEY` | **必须设为强随机值**（会话/令牌相关），不要留空 |
| `PLUGIN_SECRET_KEY` | 启用插件时设为独立强随机值 |
| `FBZ_BOOTSTRAP_ADMIN_USERNAME` / `_PASSWORD` | 仅首启引导管理员用，引导后清空 |
| `RUST_LOG` | 生产用 `fbz_api=info,tower_http=info`，排障时临时调高 |

### 3.2 数据库 / Redis

`DATABASE_URL`、`REDIS_URL` 指向生产实例。连接池与超时按规模调整：
`DATABASE_MAX_CONNECTIONS`（多节点时注意各节点之和不超过 PG `max_connections`）、
`DATABASE_STATEMENT_TIMEOUT_MS`（硬超时）、`DATABASE_SLOW_LOG_THRESHOLD_MS` /
`HTTP_SLOW_LOG_THRESHOLD_MS`（慢查询/慢请求结构化告警阈值）。
跨节点事件镜像：`REDIS_EVENT_STREAMS_ENABLED=true` 并按需调 `REDIS_EVENT_STREAM_*`。

### 3.3 节点角色与 worker 开关

`FBZ_NODE_ROLE` 加各 `FBZ_*_WORKER_ENABLED` / `FBZ_SCHEDULER_ENABLED` 决定职责（见 §1 表）。
注意 `.env.example` 里这些开关**默认 `false`**，单机 all 部署要显式打开需要的 worker 与 scheduler。

### 3.4 转码

`TRANSCODE_MAX_CONCURRENT=3`（默认 3 路并发，其余排队）、`TRANSCODE_LEASE_SECONDS`、
`TRANSCODE_HARDWARE_MODE=auto`（默认硬解，不可用回落软件，见 `TRANSCODE_SOFTWARE_FALLBACK`）、
`TRANSCODE_HARDWARE_PRIORITY=intel,nvidia,amd`。硬件转码需容器能访问对应设备（见 §7/§9）。

### 3.5 STRM 与媒体安全

`MEDIA_ROOTS` 为媒体库根目录（容器内路径，逗号分隔）。
`STRM_ALLOW_PRIVATE_NETWORKS`（默认仅允许内网链接）与 `STRM_ALLOWED_DOMAINS`（公网域名白名单）
共同约束 STRM 跳转目标，公网 STRM 必须显式加入 allowlist。

### 3.6 插件

`PLUGIN_REQUIRE_APPROVAL=true`、`PLUGIN_ALLOW_UNSIGNED=false`、`PLUGIN_TRUSTED_SIGNATURE_KEYS`、
`PLUGIN_HTTP_ALLOWED_HOSTS`（联网插件出站白名单，容器内访问宿主用 `host.docker.internal`）、
以及资源上限 `PLUGIN_TIMEOUT_MS` / `PLUGIN_MAX_CONCURRENCY` / `PLUGIN_MEMORY_LIMIT_MB` /
`PLUGIN_WASI_FUEL` 等。详见 `docs/plugin-system.md`。

## 4. 持久化目录与卷挂载

| 路径变量 | 用途 | 挂载建议 |
| --- | --- | --- |
| `MEDIA_ROOTS` | 媒体库源文件 | **只读**挂载（`:ro`），NAS 共享见 §9 |
| `TRANSCODE_CACHE_DIR`（默认 `./var/transcode`） | HLS 转码输出 | 可写卷，容量充足，可定期清理 |
| `ARTWORK_CACHE_DIR`（默认 `./var/artwork`） | 封面/图片缓存 | 可写卷 |
| `PLUGIN_DIR`（默认 `./plugins`） | 已安装插件 | 可写卷（启用插件时） |
| `PLUGIN_PACKAGE_DIR` / `PLUGIN_DATA_DIR` / `PLUGIN_CACHE_DIR` / `PLUGIN_TMP_DIR` | 插件包/数据/缓存/临时 | 可写卷 |
| PostgreSQL data | 数据库 | 独立持久卷（示例 `fbz_postgres_data`） |
| Redis data | AOF | 独立持久卷（示例 `fbz_redis_data`） |

多节点共享时，转码/插件/缓存目录若需跨节点共享须使用共享存储；否则按节点本地盘并让产生该输出的
节点自行清理（转码清理由 `core.transcode.cleanup` 计划任务兜底）。

## 5. FFmpeg / ffprobe

优先级：

1. 显式 `FFMPEG_PATH` / `FFPROBE_PATH` 指向绝对路径（生产镜像内安装 ffmpeg 后通常即 `PATH` 默认值）。
2. 未显式设置且 `FBZ_ENABLE_BUNDLED_FFMPEG=true` 时，回退到 `FBZ_BUNDLED_FFMPEG_DIR`（默认 `./vendor/ffmpeg`）。
3. 默认 `PATH` 查找 `ffmpeg` / `ffprobe`。

容器镜像建议直接 `apt-get install ffmpeg`（包含 ffprobe），保持默认 `PATH` 值即可。

## 6. Dockerfile（多阶段构建示例）

> 起点示例。edition 2024 需 Rust ≥ 1.85（仓库当前用 1.95）。请按需固定基础镜像版本。

```dockerfile
# ---- build ----
FROM rust:1-bookworm AS build
WORKDIR /app
# 先拷贝清单以利用依赖缓存
COPY Cargo.toml Cargo.lock ./
COPY migrations ./migrations
COPY src ./src
RUN cargo build --release --locked

# ---- runtime ----
FROM debian:bookworm-slim AS runtime
RUN apt-get update \
 && apt-get install -y --no-install-recommends ffmpeg ca-certificates curl \
 && rm -rf /var/lib/apt/lists/*
WORKDIR /app
COPY --from=build /app/target/release/fbz-api /usr/local/bin/fbz-api
# 运行期可写目录
RUN mkdir -p /data/transcode /data/artwork /data/plugins /data/plugin-packages \
             /data/plugin-data /data/plugin-cache /data/plugin-tmp
ENV FBZ_API_HOST=0.0.0.0 \
    FBZ_API_PORT=8080 \
    TRANSCODE_CACHE_DIR=/data/transcode \
    ARTWORK_CACHE_DIR=/data/artwork \
    PLUGIN_DIR=/data/plugins \
    PLUGIN_PACKAGE_DIR=/data/plugin-packages \
    PLUGIN_DATA_DIR=/data/plugin-data \
    PLUGIN_CACHE_DIR=/data/plugin-cache \
    PLUGIN_TMP_DIR=/data/plugin-tmp
EXPOSE 8080
HEALTHCHECK --interval=15s --timeout=5s --retries=10 \
  CMD curl -fsS http://127.0.0.1:8080/health || exit 1
ENTRYPOINT ["fbz-api"]
```

迁移在启动时自动执行，无需单独的迁移步骤。

## 7. 生产 docker-compose（单机 all 示例）

```yaml
services:
  postgres:
    image: postgres:16-alpine
    environment:
      POSTGRES_DB: fbz
      POSTGRES_USER: fbz
      POSTGRES_PASSWORD: ${POSTGRES_PASSWORD:?set in .env}
    volumes:
      - fbz_postgres_data:/var/lib/postgresql/data
    healthcheck:
      test: ["CMD-SHELL", "pg_isready -U fbz -d fbz"]
      interval: 5s
      timeout: 3s
      retries: 20

  redis:
    image: redis:7-alpine
    command: ["redis-server", "--appendonly", "yes"]
    volumes:
      - fbz_redis_data:/data
    healthcheck:
      test: ["CMD", "redis-cli", "ping"]
      interval: 5s
      timeout: 3s
      retries: 20

  fbz-api:
    build: .
    depends_on:
      postgres: { condition: service_healthy }
      redis: { condition: service_healthy }
    environment:
      FBZ_NODE_ROLE: all
      DATABASE_URL: postgres://fbz:${POSTGRES_PASSWORD}@postgres:5432/fbz
      REDIS_URL: redis://redis:6379
      PUBLIC_BASE_URL: ${PUBLIC_BASE_URL}
      FBZ_SECRET_KEY: ${FBZ_SECRET_KEY:?set a strong random value}
      FBZ_SCAN_WORKER_ENABLED: "true"
      FBZ_METADATA_WORKER_ENABLED: "true"
      FBZ_PROBE_WORKER_ENABLED: "true"
      FBZ_TRANSCODE_WORKER_ENABLED: "true"
      FBZ_SCHEDULER_ENABLED: "true"
      MEDIA_ROOTS: /media/movies,/media/tv,/media/music
    ports:
      - "8080:8080"
    volumes:
      - /srv/media/movies:/media/movies:ro
      - /srv/media/tv:/media/tv:ro
      - /srv/media/music:/media/music:ro
      - fbz_transcode:/data/transcode
      - fbz_artwork:/data/artwork
    # 硬件转码（Intel VAAPI 示例），按需启用：
    # devices:
    #   - /dev/dri:/dev/dri

volumes:
  fbz_postgres_data:
  fbz_redis_data:
  fbz_transcode:
  fbz_artwork:
```

### 多节点拆分（片段）

共享上面的 `postgres` / `redis`，把 `fbz-api` 拆成多个服务：

- `api` 服务：`FBZ_NODE_ROLE=api`，发布 `8080`，所有 worker 开关可不设。
- `worker` 服务（可水平扩 N 份）：`FBZ_NODE_ROLE=worker` + 需要的 `FBZ_*_WORKER_ENABLED=true`，
  挂载媒体（只读）与转码/插件可写卷，不发布端口。
- `scheduler` 服务（仅 1 份）：`FBZ_NODE_ROLE=scheduler` + `FBZ_SCHEDULER_ENABLED=true`。

跨节点事件镜像按需打开 `REDIS_EVENT_STREAMS_ENABLED=true`。各节点 `/ready` 的
`runtime.roles` 会明确该进程承担的 api/worker/scheduler 职责，便于按节点排障。

## 8. 反向代理、TLS 与备份

### 反向代理 / TLS

FBZ API 本身只提供 HTTP，生产环境建议在前面放反向代理（nginx / Caddy / Traefik）做 TLS 终止：

- 把 `FBZ_API_HOST=0.0.0.0`、`FBZ_API_PORT=8080` 暴露给代理，仅由代理对外提供 443。
- `PUBLIC_BASE_URL` 必须填代理对外的地址（如 `https://media.example.com`），它决定下发给客户端的链接。
- 透传 `Host` 头并放行长连接 / 大响应：HLS 分片和直连下载是流式响应，代理需关闭对这些路径的缓冲并放宽
  `proxy_read_timeout` / body size，否则转码播放与下载会被截断。
- 健康检查仍走容器内 `/health`；对外只暴露必要路径，`/api/admin/*` 与 `/api/plugin/*` 建议按网络策略额外收敛。

nginx 关键片段示例（仅示意）：

```nginx
location / {
    proxy_pass http://127.0.0.1:8080;
    proxy_set_header Host $host;
    proxy_set_header X-Forwarded-For $proxy_add_x_forwarded_for;
    proxy_set_header X-Forwarded-Proto $scheme;
    proxy_buffering off;            # 流式 HLS / 下载
    proxy_read_timeout 3600s;       # 长转码会话
    client_max_body_size 0;         # 不限制上传体积（图片/插件包）
}
```

### 备份与恢复

- **PostgreSQL 是唯一权威状态**（用户、媒体库、任务队列、审计、插件、通知配置等）。务必对其数据卷做定期备份：
  逻辑备份 `pg_dump`（或物理备份 / PITR），并定期演练恢复。
- **Redis 可重建**：仅承载运行态与可选事件镜像，丢失不影响权威数据；按需开启 AOF（示例 compose 已开）。
- **媒体源文件**按你既有的存储/NAS 备份策略保护，FBZ 不复制媒体本体。
- **可重建的派生数据**：`TRANSCODE_CACHE_DIR`（转码输出）和 `ARTWORK_CACHE_DIR`（图片缓存）可不备份，
  丢失后会按需重新生成；插件目录（`PLUGIN_DIR` 等）建议随 PostgreSQL 一并备份以保持插件安装与审批状态一致。
- **恢复顺序**：先恢复 PostgreSQL 卷 → 启动一个节点让启动期迁移对齐 → 校验 `/ready` → 再扩容其余节点。

### 迁移与上线锁

迁移在进程启动时自动执行且为幂等。其中包含 `CREATE INDEX`（非 `CONCURRENTLY`，因为迁移在事务内运行），
在百万级 / PB 级 `jobs`、`event_outbox` 等大表上首次建索引会短暂持有写锁。大规模部署建议：迁移类上线安排在低峰期、
先用单个节点完成迁移再扩容、并观察队列 backlog（`/ready`）确认无长时间阻塞。

## 9. NAS 部署注意事项

- **媒体只读**：NAS 媒体共享以只读挂载进容器；写操作（转码输出、缓存、插件）落到独立可写卷。
- **路径一致性**：`MEDIA_ROOTS` 必须是**容器内**路径，并与扫描入库时记录的路径保持稳定，避免重挂后路径漂移。
- **NAS 临时不可达**：扫描/探测 worker 失败会按 `attempts < max_attempts` 重试，失败时输出
  `job failed; scheduled retry` / `job failed; max attempts reached` 结构化日志（见 `src/jobs.rs`），
  便于区分「NAS 暂时掉线可重试」与「彻底失败」。建议给媒体挂载配置可靠的重连/超时。
- **STRM 安全**：默认仅允许内网链接；NAS 上若有指向公网的 STRM，必须把域名加入 `STRM_ALLOWED_DOMAINS`。
- **资源约束**：NAS CPU/内存有限时，下调 `TRANSCODE_MAX_CONCURRENT`、worker 并发与
  `DATABASE_MAX_CONNECTIONS`；硬件转码（如群晖核显）需将 `/dev/dri` 透传给容器。
- **时区/权限**：确保容器用户对可写卷有权限，时区与 NAS 一致以免计划任务时间错位。

## 10. 健康探针与可观测

- `GET /health`：进程存活探针。
- `GET /ready`：真实探测 PostgreSQL `select 1` 与 Redis `PING`（超时 `FBZ_READINESS_TIMEOUT_MS`），
  并返回 `runtime.roles`（本节点 api/worker/scheduler 职责）、worker 开关与 `should_run`、通用队列 backlog 与
  `event_stream_mirror` 镜像 backlog，便于多节点排障。各队列 backlog 还带 `drained_by_node`：backlog 计数是全局的，
  该标记表明本节点是否运行消费该队列的 worker，便于区分「本节点负责的积压」与「可见但不归本节点处理的积压」
  （api-only / scheduler-only 节点对这些队列均为 `false`）。编排健康检查用 `/health`，流量就绪用 `/ready`。
- 结构化告警日志（warn）：慢 HTTP / 慢 SQL；通用 job、计划任务 run、转码、插件 execution run、
  事件镜像的 lease 回收；job handler 失败重试（`job failed; scheduled retry`）；计划任务执行失败
  （`scheduled task run failed`）；转码失败、事件镜像/插件 dispatch/通知投递重试。

## 11. 运维恢复检查清单

上线 / 故障恢复时按序确认：

1. **依赖就绪**：PostgreSQL、Redis 容器 healthy；`GET /ready` 返回 ready 且 DB/Redis 探测通过。
2. **迁移**：进程启动日志显示迁移完成、无报错（迁移幂等，仅新增）。
3. **角色与开关**：`/ready` 的 `runtime.roles` 与预期拓扑一致；需要的 worker / scheduler 已启用。
4. **密钥**：`FBZ_SECRET_KEY`（及启用插件时 `PLUGIN_SECRET_KEY`）已设强随机值，未用默认/空值。
5. **卷与权限**：媒体只读可读、转码/缓存/插件可写卷挂载正确且有写权限；`MEDIA_ROOTS` 路径与库内记录一致。
6. **媒体工具**：容器内 `ffmpeg -version` / `ffprobe -version` 可用（或 `FFMPEG_PATH`/`FFPROBE_PATH` 指向有效路径）。
7. **队列健康**：`/ready` 的队列 backlog 不持续增长；关注 `recovered stale …`、`job failed; …`、
   `scheduled task run failed` 等结构化日志判断 worker 是否卡死或反复失败。
8. **转码**：触发一次播放确认转码可用、并发不超过 `TRANSCODE_MAX_CONCURRENT`、硬件不可用时回落软件。
9. **安全边界**：STRM allowlist、插件审批/签名要求、`PLUGIN_HTTP_ALLOWED_HOSTS` 符合预期。
10. **备份**：PostgreSQL data 卷有备份策略；Redis 仅运行态可按需重建。

## 相关文档

- `README.md` —「配置」「本地依赖」「启动」「路由策略」。
- `fbz-api/.env.example` — 完整环境变量模板。
- `docs/database-scale.md` — 数据库规模化与索引/分页约束。
- `docs/plugin-system.md` / `docs/plugin-development.md` — 插件系统与开发。
- `docs/plans/backend-execution-goal.md` — 后端长期执行 goal 与硬约束。
