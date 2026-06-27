# 高增长表分区与归档设计

本文是 `database-scale.md`「Partition Readiness」一节的展开:给出 FBZ 在 PB 级媒体、
百万级条目、约 1000 并发在线下,对**追加型 / 审计型高增长表**的具体分区键、保留窗口、
归档与冷热边界、物化统计设计。

> **状态:首张表已落地(`job_events`),其余仍为设计。** 表分区是结构性 schema 变更,按
> `backend-execution-goal.md` 的硬约束(「执行生产或真实数据库结构变更前要明确确认」)和 CLAUDE.md
> 的迁移规则,每张表在产生分区迁移前都需明确确认。`job_events` 已按本设计在确认后实现(迁移
> `0064_partition_job_events.sql`,见下「已落地」),并在本地 dockerized PostgreSQL 上实跑校验。
> 其余表先保持设计固化,便于后续按确认逐张实施。现有针对这些表的查询都已 keyset 化且时间/状态可索引,
> 分区是规模化的下一步,而非前提。

## 已落地

- **`job_events`**(迁移 `0064`):按 `created_at` 月度 RANGE 分区,PK 改为 `(id, created_at)`,
  复用原 `job_events_id_seq` 保持 id 连续,保留 jobs/job_runs 外键与四个 keyset/时间索引(原名,级联到分区),
  现有行经 `insert ... select` 回填后删除 legacy 表。建有 `2026m06/07/08` 月度分区 + `default` 兜底,
  滚动维护任务负责后续按月预建与归档。本地实跑校验:迁移 `success=t`、`job_events` 为分区表(relkind `p`)、
  28 行全部保留并落入 `2026m06`、`created_at='2026-07-15'` 的插入路由到 `2026m07` 且 id 续为 29、legacy 表已删除。
  应用 `record_job_event` 的 INSERT SQL 未变,透明路由进分区。
- **`plugin_host_api_calls`**(迁移 `0065`,category B 首张):按 `finished_at` 月度 RANGE 分区,PK 改 `(id, finished_at)`,
  复用 `plugin_host_api_calls_id_seq`;`public_id` UNIQUE 按方案 (a) 降级为普通索引 `idx_plugin_host_api_calls_public_id`
  (该列为随机 uuid 且审计 INSERT 无 ON CONFLICT,降级安全),保留全部 11 个原审计索引(含预算上限索引
  `idx_plugin_host_api_calls_execution_plugin_budget`)和三个出站外键(plugin_installations/host_tokens/execution_runs)。
  本地实跑校验:`success=t`、relkind `p`、11 行保留、无 UNIQUE 约束且 public_id 索引为非唯一、预算索引在、
  `finished_at='2026-08-10'` 插入路由到 `2026m08`、legacy 表已删。
- **`scheduled_task_runs`**(迁移 `0066`,category B):按 `started_at` 月度 RANGE 分区,`public_id` UNIQUE → 普通索引,
  保留 active(`task_id, lease_expires_at` where status='running')、task_recent(INCLUDE public_id)、task_started、
  task_status keyset 四索引与 scheduled_tasks 外键。`started_at` 插入后不变,running-lease/claim 语义安全。
  本地实跑校验:`success=t`、relkind `p`、2 行保留、无 UNIQUE、active partial 索引在;active-query 兼容性实测——
  插入 running run 路由 `2026m06`、按 id 更新状态成功、lease 回收查询(`status='running' and lease_expires_at<=now()`)正确返回。
  后续:`notification_delivery_attempts` 经复核有成功投递幂等唯一索引,暂不分区;C 类(先改造入站外键)、
  D 类活跃队列,并补滚动维护任务与 `queue_stats_rollup` 物化统计。

## 各表落地就绪分析(基于实际 schema 巡检)

逐张评估(来自本地实跑 schema 巡检),按落地难度分四类。**每张表落地仍需单独确认**,
且只在该表达到分区判据(见下)时才实施。

### A. 纯 leaf、无 `public_id` —— 最干净(已落地)

- `job_events`:无 `public_id`、无入站外键。已按迁移 `0064` 落地(见「已落地」)。这是唯一无需语义决策的表。

### B. Leaf(无入站外键),但有 `public_id uuid` 唯一约束 —— 需先定唯一性处理

分区表的唯一约束必须包含分区键,故 `unique(public_id)` 不能原样保留。每张需二选一(语义决策):
(a) 降级为普通 `public_id` 索引——uuid 随机,实际唯一,DB 不再强制(本设计推荐);或
(b) 改 `unique(public_id, <时间键>)`——仅分区内强制。

- `plugin_host_api_calls`(分区键 `finished_at`;11 个索引、9 个 check、3 个出站外键;高增长审计,价值最高)
  —— **已落地**(迁移 `0065`,采用方案 (a))。
- `notification_delivery_attempts`(分区键 `created_at`)—— **暂不分区。** 除 `public_id`
  唯一约束外,该表还有 partial unique index `idx_notification_delivery_attempts_target_success`
  (`notification_request_id, target_id where status = 'succeeded' and target_id is not null`),
  与 delivery worker 的 `has_successful_attempt(request.id, target.id)` 前置检查共同保护
  **同一通知请求同一目标最多一个成功投递**(same request + target may have at most one successful delivery)。
  按 `created_at` 分区会迫使该唯一索引包含分区键或降级为普通索引,两者都不能等价维持跨月份的成功幂等不变量。
  该表达规模前先保留现状;若未来必须分区,需单独设计例如请求级目标成功状态表、显式幂等键表,
  或让 `plugin_notification_requests` 与 attempts 共生命周期后再重评;在替代防线落地前 do not partition,
  不能沿用普通 B 类 `public_id` 降级方案。
- `scheduled_task_runs`(分区键 `started_at`)—— **已落地**(迁移 `0066`,方案 (a))。虽有 running-lease/claim 语义,但 `started_at` 插入后不变(行不跨分区),lease 回收/并发查询经各分区 partial 索引仍正确。

### C. 有入站外键 —— 需先改造引用方外键

分区后 PK 含时间键,被引用的 `id` 不再单独唯一,入站外键失效,必须先处理引用方:

逐张安全性分析结论(实跑 schema + 代码核查):

- `job_runs` ← `job_events`(及其分区)的 `job_run_id` —— **已落地**(迁移 `0067`):先 drop 入站 FK
  `job_events_job_run_id_fkey`(安全:job_runs 仅经 jobs 级联删除,该级联同时删除引用方 job_events,故 SET NULL 路径永不触发;`job_run_id` 列保留),再按 `started_at` 分区(无 public_id,无需唯一性降级)。**这是唯一可简单安全 drop 的 C 表。**
- `plugin_notification_requests` ← `notification_delivery_attempts.notification_request_id`(CASCADE)——
  **暂不分区。** attempts 是通知请求的子审计行,该入站 FK 当前负责把请求删除与 attempts 生命周期绑定。
  分区 `plugin_notification_requests` 会使单列 `id` 不再能被 attempts 作为 FK 目标;而 attempts 本身又有
  `idx_notification_delivery_attempts_target_success` 成功幂等阻塞点。request/attempt pair must be redesigned together:
  未来若必须分区,需一起设计请求/尝试共分区、显式 request public-id 引用、或替代级联清理机制。在该方案落地前
  do not partition `plugin_notification_requests`。
- `plugin_execution_runs` ← `plugin_host_tokens`(CASCADE)、`plugin_host_api_calls`(SET NULL)——
  **暂不分区。** 入站 FK 本身可安全 drop(execution_runs 仅经 plugin 级联删除,host_tokens/host_api_calls 也各自经 plugin_id 级联同删,路径被吸收;代码无独立 `delete from plugin_execution_runs`),但该表另带**业务唯一约束** `unique(outbox_event_public_id, attempt)`——派发幂等不变量(DB 强制,非 ON CONFLICT),按 `started_at` 分区将被迫 drop 该不变量(含分区键也无法等价强制),属移除防御性保证而非「安全 drop」,需单独决策。
- `playback_sessions` ← `transcoding_sessions` 的 `playback_session_id`(SET NULL)—— **已落地**(迁移 `0069`,触发器方案)。
  该 SET NULL 路径会被触发(playback_sessions 可经 `users`/`media_items` 级联独立删除,引用方 transcoding_session 可能存活),故不能简单 drop:
  先建 `BEFORE DELETE` 触发器 `trg_playback_sessions_null_transcode_refs`(置空 `transcoding_sessions.playback_session_id`,复刻原 SET NULL)+ 支撑索引,再 drop 入站 FK,再按 `started_at` 分区(`public_id` UNIQUE 降级为普通索引)。并把 `playback_sessions` 加入 `ensure_partition_coverage`。
  本地实跑校验:`success=t`、relkind `p`、3 行保留、入站 FK 删除而列保留、触发器存在、无 UNIQUE、20 分区;**触发器行为实测**——令 transcoding_session 指向某 playback_session 后删除该 playback_session,引用被自动置空(无悬挂引用)。

### D. 活跃队列 —— 最后、单独确认

- `jobs`、`event_outbox`、`transcoding_sessions`:既是活跃队列又是历史,状态会变更,风险最高,放到最后单独评估。

**建议实施顺序**:A(已 done)→ B(逐张先定 `public_id` 处理)→ C(先改造引用方外键)→ D。

## 适用判据(何时才分区)

沿用 `database-scale.md`:**不要过早对小表分区。** 仅当某表满足以下任一条件时才进入分区实施:

- 行数进入千万级,且 `autovacuum` / 索引膨胀开始影响写入或 claim 延迟;
- 历史数据需要按窗口批量归档 / 丢弃,而 `DELETE` 已无法在维护窗口内完成;
- 运营只关心近窗口(热数据),冷数据仅供审计 / 合规查询。

分区前先用 `EXPLAIN (ANALYZE, BUFFERS)` 确认瓶颈确实在表规模,而非缺索引。

## 分区候选表与分区键

全部采用 **PRIVATE RANGE 时间分区**(`PARTITION BY RANGE`),分区键为各表已有的时间列,
与现有查询谓词一致(claim、readiness backlog、admin keyset 列表都已按时间/状态过滤)。

| 表 | 分区键 | 建议粒度 | 热窗口(在线保留) | 冷处理 |
| --- | --- | --- | --- | --- |
| `job_events` | `created_at` | 月 | 3 个月 | 归档后 drop |
| `event_outbox` | `created_at` | 月 | 1 个月(终态已投递/已镜像) | 归档后 drop |
| `job_runs` | `started_at` | 月 | 3 个月 | 归档后 drop |
| `jobs` | `created_at` | 月 | 热:活跃 + 近 1 个月终态 | 终态归档,见下「活跃行注意」 |
| `scheduled_task_runs` | `started_at` | 月 | 6 个月 | 归档后 drop |
| `plugin_host_api_calls` | `finished_at` | 月 | 3 个月 | 归档后 drop |
| `plugin_execution_runs` | `started_at` | 月 | 3 个月 | 归档后 drop |
| `notification_delivery_attempts` | `created_at` | 月 | 3 个月 | 暂不分区;需先设计成功幂等替代 |
| `plugin_notification_requests` | `created_at` | 月 | 6 个月 | 暂不分区;需与 attempts 共设计 |
| `playback_sessions` | `started_at` | 月 | 按合规需求(默认 12 个月) | 归档后 drop |
| `transcoding_sessions` | `created_at` | 月 | 热:活跃 + 近 7 天终态 | 终态由 `core.transcode.cleanup` 清理后归档 |

粒度选择:写入量大、保留短的(`job_events`、`event_outbox`)用月分区即可;若单月仍达数亿行,
再降到周分区。粒度不要细于周,否则分区数量本身成为规划器负担。

### 活跃行注意(queue 表)

`jobs`、`event_outbox`、`transcoding_sessions` 既是**活跃队列**又是历史。分区必须保证:

- claim 查询(`status in ('queued','failed') and run_at/available_at <= now()`)只命中近窗口分区——
  活跃行天然集中在最新分区,规划器按时间键做分区裁剪即可;
- 终态行(`succeeded`/`delivered`/`cancelled`)随分区老化进入冷区;
- **不要**按 `status` 分区(状态会变更,会触发跨分区行迁移,代价高且破坏 claim 局部性)。时间键分区下状态只在分区内变更。

## 唯一约束与外键

PostgreSQL 原生分区要求**唯一约束/主键必须包含分区键**:

- 现有这些表多以 `id bigserial primary key` + `unique(public_id)` 为约束。分区后主键需改为
  `(id, <时间键>)`,`public_id` 唯一约束需改为 `(public_id, <时间键>)` 或由应用保证唯一(public_id 为 uuid,碰撞概率可忽略,可降级为普通索引 + 应用层唯一)。
- 指向分区表的**外键**(如 `job_runs.job_id -> jobs.id`、`job_events.job_id`)需要重新评估:
  PostgreSQL 支持引用分区表的外键(PG12+),但跨分区 FK 校验有成本。可选方案:
  (a) 保留 FK 并接受成本;(b) 子表与父表用相同时间键共同老化,靠应用层与 `on delete` 策略维持一致;
  (c) 归档时父子分区同窗口一起归档,避免悬挂引用。
- 这些是分区迁移里最需要在真实数据上验证的点,也是为何必须先确认再实施。

## 归档与冷热边界

- **热(online)**:近 N 个月分区,挂在主表下,支撑 claim、readiness backlog、admin 近窗口列表。
- **冷(archive)**:超出热窗口的分区先 `pg_dump` 单分区到对象存储 / 归档库,再 `DETACH PARTITION` 后
  `DROP`。`DETACH` 是元数据操作,不阻塞热分区读写。
- **滚动维护**:用一个计划任务(可复用 `scheduler` 节点的 cron 调度)按月预创建下一分区、归档并分离过期分区。
  预创建必须早于写入到达,避免落到默认分区。
  - **预创建机制已落地**(迁移 `0068`,并由 `0069` 扩展):`ensure_partition_coverage(months_ahead int) returns int`
    幂等地为所有已分区表
    (`job_events`/`plugin_host_api_calls`/`scheduled_task_runs`/`job_runs`/`playback_sessions`)
    创建当前月 + N 个未来月分区(已存在则跳过),返回新建数量。
    `0068` 先把四表覆盖延伸到 2027m12(各 19 月分区 + default),`0069` 再把 `playback_sessions`
    加入同一函数并调用 `ensure_partition_coverage(18)`;本地实跑校验:
    函数存在、五表各 20 分区、重复调用返回 0(幂等)。
  - **滚动计划任务已落地**:`core.partition.maintenance`(task_type `partition.maintenance`,默认 `SCHEDULE_PARTITION_MAINTENANCE=daily`)
    在 `bootstrap_core_tasks` 注册,`run_claimed_task` 调用 `ensure_partition_coverage(6)` 保持向前 6 个月覆盖。该任务受 scheduler 节点角色 + 开关门控,
    并刷新当前月的 `queue_stats_rollup` 热 bucket。当前月刷新按五张已分区表写入幂等月度计数:
    `job_events.event_level`、`plugin_host_api_calls.status_code`、`scheduled_task_runs.status`、
    `job_runs.status`、`playback_sessions` 的 active/stopped 状态桶。本地实跑校验:启用 scheduler 后任务注册(enabled=t)、
    强制 due 后经真实调度器派发并 `succeeded`;`queue_stats_rollup_refresh_writes_against_live_schema`
    证明刷新 SQL 可在迁移后 schema 上计划、写入、重复刷新不累加并可恢复 bucket。
    冷分区归档的只读候选发现已在调度仓储落地:`list_partition_archive_candidates(retention_months, limit)`
    从 `pg_inherits` 枚举五张已分区表的月度子分区,排除 default 分区,只返回早于热窗口且已有
    `queue_stats_rollup.source_partition` bucket 的候选;真实 schema 冒烟
    `partition_archive_candidates_plan_against_live_schema` 已验证 SQL 可计划、当前月不会被误判为候选、
    有 rollup 证据的冷分区可被发现。实际 `DETACH`+归档+`DROP` 执行任务仍为后续单独实施。
  - **只读 Admin 可观测入口已落地**:`GET /api/admin/partition-maintenance/rollups`
    按 allowlist 表名、bucket 日期范围和上限读取 `queue_stats_rollup`;
    `GET /api/admin/partition-maintenance/archive-candidates` 按 retention/month limit 读取冷分区候选。
    两个入口都要求服务器管理员权限,只调用调度仓储只读查询,不执行 `DETACH`、`DROP`、`DELETE` 或归档写动作;
    `queue_stats_rollup_read_query_executes_against_live_schema` 已在真实迁移 schema 上验证读查询可计划、可读取 marker row 且 limit 生效。
- **冷数据查询**:合规 / 审计场景查冷数据走归档库;在线 `/api/admin/*` 列表只查热窗口,
  并在 UI/文档上明示「仅近 N 个月」。这与现有 keyset 列表「不算精确总数」的取舍一致。

## 物化统计(避免扫全分区)

`/ready` 的队列 backlog 摘要和 admin 仪表盘的计数,不应在分区表上跑全量 `count(*)`:

- **活跃 backlog**(`/ready` 的 jobs/event_outbox/transcodes/notifications/mirror)只统计活跃状态行,
  天然集中在最新分区,继续走现有部分索引即可,**不受归档影响**——这是当前实现已满足的。
- **历史聚合 / 仪表盘计数**改为物化:用一张 `queue_stats_rollup(bucket_date, table_name, status, row_count)`
  汇总表,由滚动维护任务在归档分区前写入该分区的最终计数,在线仪表盘读 rollup 而非扫描历史分区。
  迁移 `0076` 已先落表结构、主键和按 `(table_name, bucket_date desc, status)` 读取的索引;调度仓储
  `refresh_queue_stats_rollup_for_month` 已提供非破坏性刷新,并由 `core.partition.maintenance` 刷新当前月热 bucket。
  Admin 只读入口 `GET /api/admin/partition-maintenance/rollups` 已接入该读取索引,用表名 allowlist、日期范围和 limit 约束查询窗口。
  冷分区归档任务接入时,需复用同一刷新语义在 `DETACH` 前写入待归档分区的最终 bucket。
- 物化刷新与归档同一事务/同一任务完成,保证「分区被分离前其计数已落 rollup」,避免计数丢失。
  当前只读候选发现已经把「有 rollup bucket」作为候选前置条件,为后续执行任务提供防线。

## 迁移实施步骤(确认后执行)

1. 选定首个目标表(建议从纯追加、无活跃语义、保留短的 `job_events` 起步,风险最低)。
2. 新建分区父表 `*_partitioned`(`PARTITION BY RANGE(<键>)`),主键含分区键;创建覆盖现有查询的本地索引。
3. 灰度回填:按时间窗口批量 `INSERT ... SELECT` 历史数据到对应分区,分批提交避免长事务。
4. 切换写入:在维护窗口内 `ALTER TABLE ... RENAME`,把分区表换到原名(或用视图过渡)。
5. 校验:claim 延迟、readiness backlog、admin 列表、外键一致性、`EXPLAIN` 分区裁剪是否生效。
6. 部署滚动维护计划任务(预创建 + 归档 + rollup)。
7. 验证稳定后,对下一张表重复;队列活跃表(`jobs` 等)放到最后、单独确认。

每一步都是结构变更,需在 `dev-deps.ps1` 起的本地 PostgreSQL 上先演练,再按确认上生产。

## 与现有实现的衔接

- 现有迁移均为追加式;本设计落地时新增的分区迁移仍应附 `include_str!` 结构测试(对齐仓库现有迁移测试风格)。
- 现有 claim / readiness / admin keyset 查询的谓词已是时间 + 状态,分区裁剪天然可用,**应用层查询基本无需改写**;
  仅唯一约束、外键和滚动维护任务是新增面。
- 滚动维护任务可复用 `scheduler/` 的 cron 调度、租约与结构化日志(与 `core.transcode.cleanup` 同形态)。

## 相关文档

- `docs/database-scale.md` — 身份模型、查询/写入规则、Partition Readiness、性能护栏。
- `docs/deployment.md` — 备份与恢复、迁移上线锁(大表 `CREATE INDEX` 短暂持写锁)。
- `docs/plans/backend-execution-goal.md` — #4 数据库规模化方向与硬约束。
