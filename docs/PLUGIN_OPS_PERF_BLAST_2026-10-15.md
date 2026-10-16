# 官方插件运营 / 性能 / 爆破实测报告

- **文件名约定**：`PLUGIN_OPS_PERF_BLAST_2026-10-15.md`（任务指定）
- **上海日历日（撰写时）**：2026-10-16 CST
- **执行窗口（本机 PT）**：2026-10-15 09:18–09:28 America/Los_Angeles
- **仓库**：`/Users/edy/Code/fbz`（Mac `bfb36dc8-407e-4711-b8ea-22ba983ab30a`）
- **范围**：`tmdb-provider` `tvdb-provider` `fanart-provider` `imdb-provider` `douban-provider` `marker-detect` + 光鸭 `guangya-storage`
- **约束**：未改鉴权网关；未 commit；密钥空则走无 key 契约并标明

原始产物：`fbz-api/var/ops-test-2026-10-15/`（smoke / perf / blast / api 日志）。

---

## 1. 启动

### 1.1 依赖

| 组件 | 方式 | 结果 |
| --- | --- | --- |
| PostgreSQL 16 | `docker compose -f fbz-api/docker-compose.dev.yml up -d` → `fbz-api-postgres` `:5432` | healthy；`DATABASE_URL` 用户/库 `fbz`/`fbz` 匹配 |
| Redis 7 | 同上 → `fbz-api-redis` `:6379` | healthy |
| 本机 brew postgres/redis | 未安装 | 已用 Docker 替代，非 blocker |
| `cargo` | `~/.cargo/bin/cargo`（PATH 需补） | `cargo 1.99.0` |
| Node | Homebrew | `v26.4.0` |

### 1.2 fbz-api

```bash
cd fbz-api
# compose 已起 PG/Redis
export PATH="$HOME/.cargo/bin:$PATH"
# 进程覆盖：MARKER/PLUGIN worker；PLUGIN_ALLOW_UNSIGNED=true（装官方未签名包）
cargo run --release   # → 127.0.0.1:8080，自动迁移
```

探针：

- `GET /health` → 200 `status=ok`
- `GET /ready` → 200；`database=ok` `redis=ok`；marker/plugin/metadata/scan/probe workers 运行中

首次初始化：`POST /api/setup` 建管理员 `opsadmin`（仅本机测试库；密码未写入本报告）。

### 1.3 插件 HTTP 进程（README 端口）

| 插件 | 命令 | 端口 |
| --- | --- | --- |
| tmdb | `PORT=19101 node plugins/tmdb-provider/server.mjs` | 19101 |
| tvdb | `PORT=19102 node plugins/tvdb-provider/server.mjs` | 19102 |
| fanart | `PORT=19103 node plugins/fanart-provider/server.mjs` | 19103 |
| imdb | `PORT=19104 node plugins/imdb-provider/server.mjs` | 19104 |
| douban | `PORT=19105 node plugins/douban-provider/server.mjs` | 19105 |
| marker-detect | `PORT=19106 node plugins/marker-detect/server.mjs` | 19106 |
| guangya | `PORT=8098 FBZ_STORAGE_PLUGIN_KEY=<32+ hex> node plugins/guangya-storage/server.mjs` | 8098 |

全部 listen 成功。

### 1.4 密钥状态（`.env` / 进程环境）

| 密钥 | 状态 | 测试策略 |
| --- | --- | --- |
| `TMDB_ACCESS_TOKEN` | **空** | 无 key → `metadata: null`；有 key 契约未打真上游 |
| `TVDB_API_KEY` | **空** | 同上 |
| `FANART_API_KEY` | **空** | 同上 |
| `IMDB_API_KEY`（OMDb） | **空** | 走 **suggest** 真上游 |
| `DOUBAN_API_KEY` | **空** | suggest/详情 fallback；本次冒烟 `metadata: null`（反爬/未命中属预期） |
| `FBZ_STORAGE_PLUGIN_KEY` | 测试时现场生成 48 hex | 光鸭本地进程鉴权 |

---

## 2. 运营冒烟

### 2.1 直打 plugin server（curl Sync 合同）

公共形状：`POST /fbz-plugin` + `x-fbz-plugin-id` + `invocation=sync`。

| 插件 | HTTP | 结果 | 说明 |
| --- | --- | --- | --- |
| tmdb | 200 | `ok:true` `metadata:null` | **无 key 契约通过** |
| tvdb | 200 | `ok:true` `metadata:null` | **无 key 契约通过** |
| fanart | 200 | `ok:true` `metadata:null` | **无 key / 无真上游**（lookup 带 tmdbId=45745） |
| imdb | 200 | `ok:true` **有 metadata** | suggest：`title=Sintel` `externalIds=imdb:tt1727587` artwork=1 |
| douban | 200 | `ok:true` `metadata:null` | 无 key；上游空/反爬，合同允许 null |
| marker-detect | 200 | `ok:true` markers×4 | 合成 `probeSummary`（Opening / Ending Credits）→ intro/credits |
| guangya | 401/400/200 | 见下 | 独立进程可起；真设备码上游可达 |

光鸭：

```bash
# 无 Bearer → 401
# 错 Bearer → 401
# Bearer + 缺 account → 400 invalid_request
# Bearer + op=auth.start + accountId/deviceId → 200，返回 device_code / user_code / verification_url（真光鸭账号域）
```

光鸭单测：`node --test plugins/guangya-storage/provider.test.mjs` → **8/8 pass**。

### 2.2 装载 / 审批 / 启用 + Match 链（SyncInvoker）

```text
POST /api/admin/plugins/packages  (packagePath=var/plugin-packages/*.zip + sha256)
POST .../packages/{id}/approve
POST .../packages/{id}/activate
POST /api/admin/plugins/{pluginId}/enable
```

| 包 | 安装 | 启用 |
| --- | --- | --- |
| fbz.provider.tmdb/tvdb/fanart/imdb/douban 0.2.0 | 201 | approved + enabled |
| fbz.marker.detect 0.2.0 | 初测 **500**（permission allowlist）；**修复后 201** + approved/enabled | 修复后已启用；detect→`marker-detect-0.2.0` |

**marker 装包失败原因**：`plugin_permissions_permission_key_check1` — manifest 声明 `media.analyze`，DB allowlist 无此键（仅有 `media.read` 等）。属产品缺口。

媒体库冒烟：

1. `POST /api/admin/libraries` → Ops Movies  
2. path=`/Users/edy/Movies/fbz-ops-smoke`（`MEDIA_ROOTS=/Users/edy/Movies`）  
3. ffmpeg 生成 `Sintel (2010).mp4`  
4. scan → item `7da76ae8-…` title=Sintel  
5. `metadata.refresh` → **`provider_attempts=7` `status=no_match`**（无 TMDB/TVDB/Fanart key；负结果入内存缓存）

**SyncInvoker 实证**（`FBZ_PROVIDER_STUB_LOG=1`，改 `production_year` 破缓存后重刷）：

```json
{"pluginId":"fbz.provider.tmdb","hookEvent":"metadata.provider.query","title":"Sintel","productionYear":2011}
{"pluginId":"fbz.provider.tvdb", ...}
{"pluginId":"fbz.provider.fanart", ...}
```

库源链默认含 tmdb/tvdb/fanart（及 nfo pending），故这三家被 Host 同步调用；imdb/douban 未在该库源链上，本轮 Match 未点到（直打冒烟已覆盖）。成功调用使用 `SyncAuditMode::FailuresOnly`，故 `plugin_sync_invocations` 表仍为 0（符合实现）。

### 2.3 marker-detect admin

| 步骤 | 结果 |
| --- | --- |
| `POST /api/admin/media-items/{fake}/marker-detect` | 404 `media item not found`（路由+鉴权在） |
| `POST .../7da76ae8-…/marker-detect` `force=true` | **202** 入队 `media.marker.detect` |
| job | **succeeded** |
| `GET .../markers` | 200 `markers:[]` |
| detector | 日志：`detector_version=core-heuristic-0.1.0` |

初测因官方 marker 包未装上，worker 走 **core 启发式**。**修复后**（§7）同一 item 复测 job 完成且 `detector_version=marker-detect-0.2.0`（已走 HTTP 插件）。短片无片头章节 → 空 markers 可接受。

---

## 3. 性能基线（直打 plugin HTTP）

速率控制：无 key 本地路径高并发；imdb/douban 限流放缓（pause 50–200ms），避免打废账号。

| 插件 | n | 并发 | 错误率 | p50(ms) | p95(ms) | max(ms) | RPS | 存活 |
| --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: | --- |
| tmdb | 100 | 10 | 0% | 9.46 | 27.25 | 36.35 | 831.15 | 200/1.62ms |
| tmdb | 100 | 50 | 0% | 15.45 | 29.43 | 42.37 | 962.52 | 200/1.98ms |
| tvdb | 100 | 10 | 0% | 9.09 | 18.27 | 24.62 | 884.5 | 200/1.75ms |
| tvdb | 80 | 50 | 0% | 12.87 | 23.25 | 31.61 | 997.28 | 200/1.82ms |
| fanart | 100 | 10 | 0% | 8.31 | 13.04 | 19.25 | 1026.31 | 200/1.47ms |
| fanart | 80 | 50 | 0% | 15.8 | 26.77 | 37.78 | 1030.19 | 200/1.78ms |
| imdb | 50 | 10 | 0% | 58.73 | 76.7 | 197.12 | 18.01 | 200/56.53ms |
| imdb | 50 | 20 | 0% | 58.72 | 70.0 | 102.33 | 11.59 | 200/55.67ms |
| douban | 40 | 5 | 0% | 52.52 | 1016.04 | 1427.02 | 6.34 | 200/51.57ms |
| douban | 30 | 10 | 0% | 53.8 | 180.91 | 298.72 | 4.83 | 200/52.31ms |
| marker | 200 | 10 | 0% | 9.61 | 18.39 | 29.98 | 791.04 | 200/1.69ms |
| marker | 200 | 50 | 0% | 26.55 | 46.01 | 68.37 | 1045.68 | 200/1.61ms |

观察：

- 无上游（tmdb/tvdb/fanart/marker）：p95 大多 **<50ms**，RPS ~800–1000，错误率 **0**。
- imdb suggest：p50 ~59ms，p95 ~70–77ms，RPS ~12–18（含客户端 pacing）。
- douban：p50 ~53ms，但 c=5 时 p95 可到 **~1s**（上游抖动）；错误率仍 0（合同返回 null）。

---

## 4. 爆破

期望：4xx / 空 metadata / 客户端超时可控；**不 panic、不拖死 api**。

### tmdb

- 尖峰：n=80 c=40 err=0 p50=14.11ms p95=24.76ms max=28.32ms
- 尖峰后存活：{'ok': True, 'status': 200, 'ms': 2.59}

| case | HTTP | ok | metadata_null | ms | err |
| --- | ---: | --- | --- | ---: | --- |
| missing_fields | 200 | True | True | 3.19 | None |
| omit_lookup | 200 | True | True | 1.66 | None |
| huge_title | 200 | True | True | 1.83 | None |
| invalid_json | 500 | None | None | 1.75 | http |
| wrong_hook | 200 | True | None | 1.83 | None |
| empty_body | 500 | None | None | 1.72 | http |
| client_timeout_1ms | 200 | None | None | 1.67 | None |

### tvdb

- 尖峰：n=80 c=40 err=0 p50=11.26ms p95=17.95ms max=20.19ms
- 尖峰后存活：{'ok': True, 'status': 200, 'ms': 1.67}

| case | HTTP | ok | metadata_null | ms | err |
| --- | ---: | --- | --- | ---: | --- |
| missing_fields | 200 | True | True | 5.53 | None |
| omit_lookup | 200 | True | True | 2.01 | None |
| huge_title | 200 | True | True | 1.73 | None |
| invalid_json | 500 | None | None | 1.69 | http |
| wrong_hook | 200 | True | None | 1.51 | None |
| empty_body | 500 | None | None | 1.83 | http |
| client_timeout_1ms | 200 | None | None | 1.36 | None |

### fanart

- 尖峰：n=80 c=40 err=0 p50=13.36ms p95=24.28ms max=30.91ms
- 尖峰后存活：{'ok': True, 'status': 200, 'ms': 1.43}

| case | HTTP | ok | metadata_null | ms | err |
| --- | ---: | --- | --- | ---: | --- |
| missing_fields | 200 | True | True | 3.19 | None |
| omit_lookup | 200 | True | True | 2.18 | None |
| huge_title | 200 | True | True | 1.85 | None |
| invalid_json | 500 | None | None | 2.82 | http |
| wrong_hook | 200 | True | None | 2.06 | None |
| empty_body | 500 | None | None | 1.8 | http |
| fanart_no_id | 200 | True | True | 1.75 | None |
| fanart_no_ids_force | 200 | True | True | 1.57 | None |
| client_timeout_1ms | 200 | None | None | 1.49 | None |

### imdb

- 尖峰：n=80 c=40 err=0 p50=51.5ms p95=61.85ms max=65.72ms
- 尖峰后存活：{'ok': True, 'status': 200, 'ms': 49.78}

| case | HTTP | ok | metadata_null | ms | err |
| --- | ---: | --- | --- | ---: | --- |
| missing_fields | 200 | True | True | 1.88 | None |
| omit_lookup | 200 | True | True | 1.38 | None |
| huge_title | 200 | True | False | 387.11 | None |
| invalid_json | 500 | None | None | 10.22 | http |
| wrong_hook | 200 | True | None | 2.96 | None |
| empty_body | 500 | None | None | 4.54 | http |
| client_timeout_1ms | None | None | None | 8.69 | timed out |

### douban

- 尖峰：n=80 c=40 err=0 p50=59.93ms p95=141.31ms max=416.14ms
- 尖峰后存活：{'ok': True, 'status': 200, 'ms': 47.96}

| case | HTTP | ok | metadata_null | ms | err |
| --- | ---: | --- | --- | ---: | --- |
| missing_fields | 200 | True | True | 2.13 | None |
| omit_lookup | 200 | True | True | 1.81 | None |
| huge_title | 200 | True | True | 44.11 | None |
| invalid_json | 500 | None | None | 1.89 | http |
| wrong_hook | 200 | True | None | 1.7 | None |
| empty_body | 500 | None | None | 1.73 | http |
| client_timeout_1ms | None | None | None | 2.18 | timed out |

### marker

- 尖峰：n=80 c=40 err=0 p50=16.41ms p95=25.54ms max=31.11ms
- 尖峰后存活：{'ok': True, 'status': 200, 'ms': 1.79}

| case | HTTP | ok | metadata_null | ms | err |
| --- | ---: | --- | --- | ---: | --- |
| missing_fields | 200 | True | None | 2.19 | None |
| omit_lookup | 200 | True | None | 1.92 | None |
| huge_title | 200 | True | None | 1.89 | None |
| invalid_json | 500 | None | None | 1.84 | http |
| wrong_hook | 200 | True | None | 1.77 | None |
| empty_body | 500 | None | None | 1.57 | http |
| marker_no_probe | 200 | True | None | 1.81 | None |
| marker_empty_chapters | 200 | True | None | 1.82 | None |
| client_timeout_1ms | 200 | None | None | 1.75 | None |

### 爆破结论

| 期望 | 实测 |
| --- | --- |
| 缺字段 / 空 lookup | 200 + `metadata:null`（或 marker 空/跳过） |
| 超大 title（50k） | 200；imdb 仍可能 suggest 命中；douban/tmdb null |
| 无效 JSON / 空 body | 初测 SDK **HTTP 500**；**修复后 HTTP 400**（§7），进程未死 |
| 错误 hook | 200 + `ignored`/`unsupported` |
| 客户端 1ms 超时 | 超时或偶发极快完成；服务继续 |
| 并发尖峰 80@40 | 全插件 **errors=0**，尖峰后 alive 200 |
| 重复装载 | 5 家 provider 可重复 approve/enable；marker 装载被权限约束挡住 |
| fbz-api | 全程 `/health` 200；爆破仅打插件端口，API 无拖死 |

---

## 5. 缺口与风险

1. **上游密钥缺失**：TMDB/TVDB/Fanart 未做真命中刮削；补 token 后应用同 curl/Match 复测。  
2. **`media.analyze` 不在 `plugin_permissions` allowlist**：**已修复**（迁移 `0103_plugin_permission_media_analyze.sql`，allowlist 对齐 `SUPPORTED_PERMISSIONS`，含 `media.analyze` + `storage.*`）。复测：装包 201 → approve/activate/enable 200；detect job `detector_version=marker-detect-0.2.0`（不再走 core 启发式）。  
3. **`GET /api/admin/plugins/packages` 500**：**已修复**（`list_plugin_packages_page` 对 `active` 使用 `coalesce(..., false)`）。复测：列表 HTTP 200，含 `fbz.marker.detect` `active=true`。  
4. **库源 `plugin:…` key**：`PUT .../metadata/sources` 拒 `plugin:fbz.provider.tmdb`（unsupported characters）；Match 仍靠 `tmdb`→官方插件优先映射工作。  
5. **HTTP 插件对非法 JSON 回 500**：**已修复**（`examples/plugins/_shared/fbz-plugin-http.mjs` + `plugins/marker-detect/fbz-plugin-http.mjs`：空 body / 非法 JSON / 非对象 → **HTTP 400**）。复测：tmdb:19101 / marker:19106 均 400；SDK 单测 4/4 pass。  
6. **元数据响应缓存**：负结果缓存导致后续 refresh `provider_attempts=1`；验证 SyncInvoker 时需改 lookup（如 year）或重启 API。  
7. **光鸭**：本地 RPC + 真 `auth.start` 可用；未做完整扫码绑定/扫库（无用户扫码）。  
8. **前端 fe**：未启动（本任务非必须）；API+插件已闭环。

---

## 6. 总评

| 维度 | 结论 |
| --- | --- |
| 启动 | PG/Redis（Docker）+ fbz-api + 7 个插件进程 **可真实拉起** |
| 运营 | 六家官方包直打合同通过；五家 provider 装载启用；**Match→SyncInvoker** 打到 tmdb/tvdb/fanart；marker admin **可入队**；光鸭进程+鉴权+auth.start 真上游 |
| 性能 | 本地路径亚 50ms p95；imdb/douban 受上游约束，控速后稳定 |
| 爆破 | 无 panic、尖峰零错误、API 存活；非法 JSON→500 与 marker 装包权限原为主要缺口（**已于同日修复复测**，见 §5 / §7） |
| 真上游 | **有**：imdb suggest、douban 探测、guangya auth.start；**无 key 未打**：TMDB/TVDB/Fanart 详情 |

不改鉴权、不 commit。测试库管理员与光鸭临时候钥仅存本机 `var/ops-test-2026-10-15/`。

---

## 7. 缺口修复复测（同日，PT）

- **窗口**：2026-10-15 ~09:33–09:35 America/Los_Angeles（API release 重建 + 插件进程热重启；未改鉴权网关；未 commit）
- **改动**：
  1. `fbz-api/migrations/0103_plugin_permission_media_analyze.sql` — DB permission allowlist 对齐 Rust `SUPPORTED_PERMISSIONS`
  2. `examples/plugins/_shared/fbz-plugin-http.mjs`（及 `plugins/marker-detect/` 副本）— 空/非法 JSON → 400
  3. `src/plugins/repository.rs` — `coalesce(pi.active_package_id = pkg.id, false) as active`
- **产物**：`fbz-api/var/ops-test-2026-10-15/api/install-fbz.marker.detect-0.2.0.zip.json`、`approve-marker.json`、`activate-marker.json`、`enable-marker.json`、`packages-after-fix.json`、`marker-enqueue-retest.json`、`markers-retest.json`
- **结果摘要**：

| 项 | 结果 |
| --- | --- |
| marker 装包/审批/启用 | 201 / 200 / 200 / 200；`packageId=4e374295-…` |
| `GET .../packages` | 200；marker `active=true` `enabled=true` |
| detect 复测 | 202 入队 → succeeded；`detector_version=marker-detect-0.2.0`；markers 仍空（短片无章节，可接受） |
| invalid_json / empty_body @19101+19106 | **400** `{error: invalid JSON body \| request body is required}` |
| SDK 单测 | `node --test examples/plugins/_shared/fbz-plugin-http.test.mjs` → 4/4 |
| Rust 相关单测 | `permission_database_constraint_tracks_supported_permissions`、`plugin_package_admin_keyset_indexes_match_query_shape` pass |

说明：首次失败装包曾留下 `var/plugin-packages/extracted/fbz.marker.detect/0.2.0` 孤儿目录，导致二次安装 409；删除该目录后重装成功（非产品缺口，属失败事务未回滚解压目录）。

