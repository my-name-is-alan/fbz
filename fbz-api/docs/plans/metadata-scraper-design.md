# 元数据刮削子系统设计方案（混合架构：内部 trait 化 + 开放插件 provider 契约）

状态：阶段 0–4 已实现并验证（`cargo test --lib` 1068 项全绿、`build`/`fmt` 通过），阶段 5 基础契约层已落地、同步执行路径按设计标注待续；迁移 0077/0078 与 settings 仓储已补本地 dockerized PG 实跑冒烟校验。本文是 `backend-execution-goal.md` 第 3 节「媒体库扫描和元数据入库」的细化设计，落地前每张迁移表、每个破坏性结构变更都需单独确认。详见 §0 实施进度。

## 0. 实施进度（可观测）

> 本节随实现实时更新。每阶段以 `cargo fmt --check` / `cargo test --lib` / `cargo build --lib` 全绿为完成判据。

| 阶段 | 状态 | 关键产出 | 验证 |
| --- | --- | --- | --- |
| 阶段 0 — provider 重构 | ✅ 已完成 | `provider.rs` 拆为 `provider/{mod,shared,tmdb,tvdb,fanart}.rs`；`MetadataProvider` trait + `ProviderMatchOutcome`/`ProviderEnrichOutcome` + `MetadataProviderRegistry` 取代硬编码 `match`；`service.rs` 切到 registry；旧 `provider.rs` 删除；新增 `async-trait` 依赖 | `cargo test --lib metadata` 41 项全绿；`cargo build --lib` 通过；`cargo fmt --check` 通过 |
| 阶段 1 — 配置层 + DB | ✅ 已完成 | 迁移 0077（三表）；`metadata/settings.rs`：`MetadataSettingsRepository` + 纯函数 `resolve_metadata_config`（env←DB）+ `mask_secret` + 校验；`MetadataService` 改为按 job 解析+缓存 registry（配置变更前复用，保留 TVDB token 缓存）；worker/main 透传 `SecretConfig`；6 个 admin API（GET/POST settings、POST provider、POST/DELETE key、POST test）key 加密复用 `SecretCipher` scope=`metadata-provider`，永不回显明文（仅 `hasKey`+末四位掩码） | `cargo test --lib metadata` 47 项全绿（含 6 项 settings 单测）；`every_admin_route_handler_enforces_server_admin` 守卫通过；`cargo build --lib`/`fmt --check` 通过 |
| 阶段 2 — 代理修复 | ✅ 已完成 | `provider/proxy.rs`：`build_provider_clients` 按 (mode,url,no_proxy) 去重缓存 client；`no_proxy` 经 `NoProxy::from_string` 真正生效；per-provider `inherit`/`direct`/`custom`；`ProviderContext` 改持 `ProviderClients`（per-provider client + default 回退），各 provider 调 `ctx.client(id)`；`registry()` 由 `resolve` 构造 override map，缓存键含 override；admin test 探测传 per-provider override | `cargo test --lib metadata` 50 项全绿（3 项 proxy 单测）；`cargo build --lib`/`fmt --check` 通过 |
| 阶段 3 — 语言/区域 + 海报原语言 | ✅ 已完成 | `MetadataLookup` 加 `image_language`/`image_prefer_original`/`image_fallback_languages`；`shared.rs` 加 `ImageLanguagePolicy` + `image_language_rank`（prefer_original→image_language→fallback→textless `none`/`xx`）；TMDB 调 `/images?include_image_language=` + `pick_localized_tmdb_image`（rank 升序、同 rank vote 降序）；`service.rs::build_lookup` 折叠全局默认层（库级←全局←provider 内置） | `cargo test --lib metadata` 59 项全绿（9 项新增：5 排序 + 4 海报选取）；`cargo build --lib`/`fmt --check` 通过 |
| 阶段 4 — IMDb + TVDB 详情富化 | ✅ 已完成 | `provider/imdb.rs`：enrichment-only，`normalize_imdb_id` 规范 `tt` 前缀（裸数字补零 7 位）+ upsert 去重，预留评分富化点；registry 注册 imdb；TVDB `search_tvdb` 命中后拉 v4 `/{movies\|series}/{id}/extended`（`tvdb_numeric_id` 提取数字 id），`apply_tvdb_extended` 填 genres/companies/characters→people + overview 兜底，best-effort 零回归 | `cargo test --lib metadata` 64 项全绿（5 项新增：3 IMDb + 2 TVDB extended）；`cargo build --lib`/`fmt --check` 通过 |
| 阶段 5 — 开放插件 provider | ✅ 基础层完成 | 新 hook 事件 `metadata.provider.query` 加入 `SUPPORTED_HOOK_EVENTS`；新增 additive 迁移 0078 重定义 `plugin_hooks_event_key_allowed` 约束含新事件，守卫测试指向 0078；激活 `metadata.read` 权限（描述+manifest_features）；`provider/plugin.rs`：`PluginMetadataProvider` 适配器（Enrichment 角色，id=`plugin:{id}`）+ `PluginMetadataQuerier` trait 同步 seam（默认 `DisabledPluginQuerier` 返回 None）+ 纯函数 `merge_plugin_metadata`（§9：内置优先、仅补空字段、external_id 累积去重、artwork scoped 追加、genres/studios/people 仅在空时填） | `cargo test --lib` 1032 项全绿（6 项新增 merge 单测 + manifest 守卫）；`cargo build --lib`/`fmt --check` 通过 |
| live-PG 校验 — 迁移 0077/0078 + settings 仓储 | ✅ 已完成 | `metadata_settings_repository_executes_against_live_schema`（settings.rs）在迁移后真实 PG 上跑 0077 三表 + `MetadataSettingsRepository` 全量往返（global upsert/load、provider upsert/load、secret set/resolve 解密/providers_with_key/delete、`resolve_metadata_config` 折叠），自清理不污染 dev DB；`hook_event_constraint_executes_against_live_schema`（manifest.rs）证明 0078 动态 drop+re-add 后约束接受 `metadata.provider.query` 且拒绝未知事件 | 本地 dockerized PG 实跑两条 `#[ignore]` 冒烟均绿；`cargo test --lib` 1068 全绿；`cargo build --lib`/`fmt --check` 通过 |

### 阶段 0 落地说明（与原设计的偏差）

- trait 方法签名未照搬 §4.1 的 `match_item -> Result<Option<MetadataMatch>>`，改为返回 `ProviderMatchOutcome`（Matched/NotMatched/Skipped）与 `ProviderEnrichOutcome`，因为原签名无法表达「缺 key 跳过」与「搜了但没结果」的区别，而 `MetadataLookupReport.attempts` 契约依赖这一区分。registry 因此保持为零 provider 知识的纯编排器。
- 一处行为微调（无测试覆盖、影响极小）：Fanart 在「无基础 match」时，registry 先记 `Skipped("requires a matched metadata item")`，不再先报 `missing Fanart API key`。有 match 时行为完全不变。
- TVDB 仍为搜索级（无详情富化），与原状一致，列阶段 4 follow-up。

### 阶段 1 落地说明

- 运行时合并是纯函数 `resolve_metadata_config(base, resolved)`，env 为基线、DB 行逐字段覆盖；provider 顺序取全局行（非空时）否则取 env，DB 显式 `enabled=false` 的 provider 从最终顺序剔除。该纯函数有 6 项单测覆盖优先级与掩码。
- `MetadataService` 不再启动时构建一次 registry，而是每个 job 经 `MetadataSettingsRepository::resolve` 读 DB → `resolve_metadata_config` → 比对缓存的 effective config，相同则复用（保留 provider 内部状态如 TVDB token 缓存），不同才重建。即「下次 job 重读 DB」热更新（§12 首版方案）。
- key 加密复用 `SecretCipher::encrypt_scoped(scope="metadata-provider", provider_id, "api_key", value)`，AAD 绑定 provider，密文不可跨 provider 重放。admin API 永不回显明文，仅 `hasKey` + `mask_secret` 末四位。
- **live-PG 校验（已完成）**：迁移 0077 与所有 `MetadataSettingsRepository` 的 sqlx 查询此前仅经编译期检查；现已补 `#[ignore]` 实跑冒烟 `metadata_settings_repository_executes_against_live_schema`（settings.rs），在迁移后的真实 PG 上跑 0077 三表 DDL + 仓储全量往返（global/provider upsert+load、secret set+加密 resolve+delete、`resolve_metadata_config` 折叠），用 `plugin:metadata-smoke` provider id 隔离并自清理（含恢复单行 global）。本地 dockerized PG 实跑通过。

### 阶段 3 落地说明

- 图片语言独立于文本语言：`MetadataLookup.effective_image_language()` 在未单独设 `image_language` 时回退文本 `language`；`image_prefer_original` 用 TMDB 详情的 `original_language` 命中条目原语言海报。
- TMDB `/images` 用 `include_image_language` 把 policy 关心的语言（含 `null` 文字版）一次性请求回来，再用 `image_language_rank` + vote 排序选 poster/backdrop，覆盖 `apply_tmdb_detail` 写入的单图。无命中则保留详情图。
- 全局默认层在 `build_lookup` 实现：库级字段非空优先，否则取 `metadata_global_settings`，再否则 provider 内置。`registry()` 改为返回 (registry, global) 以便折叠。
- **TVDB 语言下发推迟到阶段 4**：TVDB v4 `/search` 无 language 参数（只有 country，且用 3 字母码），强行传会有过滤回归风险。语言下发随阶段 4 的 TVDB 详情富化（translations 端点）一起做，此处不动 TVDB 搜索，零回归。

### 阶段 4 落地说明

- IMDb 定位 enrichment-only（无公开搜索 API）：在已有 match 上把 imdb 外部 ID 规范为 `tt` 前缀（裸数字补零 7 位），评分/分级富化预留点待合规数据源（§12 风险项）。默认 provider 顺序不含 imdb，管理员在全局 `provider_order` 加 `imdb` 即启用。
- TVDB 详情富化为 best-effort：搜索命中后拉 v4 extended 记录，失败/无数字 id 时保留搜索级 match（零回归）。`apply_tvdb_extended` 仅在拿到非空 genres/companies/characters 时覆盖，overview 仅在原为空时兜底。
- **待 live-API 校验**：TVDB extended 的 JSON 字段名（`genres[].name`/`companies[].name`/`characters[].personName`）按 v4 文档形态写，serde 全 optional+default，字段名猜错只会退化为空而非崩溃；需在真实 TVDB key 下校验字段映射。

### 阶段 5 落地说明（基础层，同步执行路径待续）

- 已落地：hook 事件常量 + 迁移 0078（additive，drop+re-add 约束）+ `metadata.read` 权限激活 + `PluginMetadataProvider` 适配器 + `merge_plugin_metadata` 纯合并逻辑（6 项单测覆盖优先级、scoped artwork、external_id 去重、空 contribution no-op）。
- **未落地（明确边界）**：真正的同步 `metadata.provider.query` 调用路径。调研确认（见对话）现有插件 dispatch 全部走异步 `event_outbox` worker（fire-and-forget），`PluginExecutionClient::execute` 及 HTTP 签名/allowlist/entrypoint 校验均为 `execution.rs` 私有；registry 同步阻塞调用插件需暴露或重写这些 security-sensitive 私有原语，且本地无 PG/无 live 插件无法验证。按设计 §8/§11「首版建议 (a) 仅 HTTP runtime 带硬超时…建议最后做」与风险评估，此处以 `PluginMetadataQuerier` trait 作为干净 seam 隔离：默认 `DisabledPluginQuerier` 返回 `None`，registry 暂不静态注册插件 provider（插件 provider 应由已安装插件动态发现，正是同步路径的接入点）。
- **接续做同步路径时**：实现一个 `PluginMetadataQuerier`，在其 `query` 内复用（需先 `pub(crate)` 暴露）`execution.rs` 的 `execute_http` + 签名 + allowlist + 硬超时，经 `enabled_hooks_for_event("metadata.provider.query")` 发现订阅插件，响应过 `validate_plugin_metadata_patch` 白名单后转 `PluginMetadataContribution`，再由 registry 在 enrichment 阶段调用本适配器。合并逻辑已就绪、已测，无需改动。



## 1. 目标与需求

把元数据刮削从「一个硬编码 provider 链 + 只读环境变量」升级为**可配置、可扩展**的子系统，对齐用户需求与 MoviePilot 的形态（识别 provider 是核心模块，不是沙箱插件），同时保留 FBZ 已有的插件沙箱边界用于第三方扩展。

四个用户明确需求：

1. **常用 provider 内置**：TMDB / TVDB / IMDb，各为独立 provider 模块（现有 Fanart 作为图片富化 provider 保留）。
2. **支持设置 key**：管理员在 admin API 配置各 provider 的 API key / token，加密持久化到 DB，覆盖环境变量默认值。
3. **支持设置代理**：全局 + per-provider 代理覆盖，并修复现有 `no_proxy` / `PROXY_POLICY` 被解析但未实现的缺口。
4. **元数据区域 / 语言选择**：全局默认 + 库级覆盖（现有库级字段保留），语言/区域下发到每个 provider。
5. **海报原语言 / 文本单独切换**：图片语言可与文本元数据语言独立设置（例如文本用 zh-CN，海报偏好原语言或无文字版）。

## 2. 现状与差距（带证据）

- provider 无 trait 抽象：`MetadataProviderClient` 是单结构体（`src/metadata/provider.rs:51-56`），调度是 `match_item_with_report` 里对 provider 名的硬编码 `match`（`src/metadata/provider.rs:189-327`）。新增 provider 必须改这个核心函数。
- TVDB 只到搜索级（无详情富化，genres/people/studios 恒空，`src/metadata/provider.rs:840-874`）；IMDb 目前只是被采集的外部 ID，不是独立 provider。
- key / base URL / 代理只从环境变量读：`MetadataConfig`（`src/config.rs:150-160`）、`ProxyConfig`（`src/config.rs:162-168`）。无 admin 配置面。
- 代理实现不完整：`from_config` 只读 `http_proxy`/`https_proxy`（`src/metadata/provider.rs:151-178`），`no_proxy` 与 `policy` 字段被解析但从未使用。
- 语言/区域是库级的（`libraries.preferred_metadata_language`/`preferred_metadata_country`，`src/metadata/service.rs:66-83`），无全局默认层；TVDB 完全不传语言/区域（`src/metadata/provider.rs:426-433`）。
- **海报原语言切换完全没有**：TMDB 只取详情里的单张 `poster_path`/`backdrop_path`（`src/metadata/provider.rs:771-794`），没调 `/images` 端点、没用 `include_image_language`。
- 无元数据响应缓存、无 provider 层限流/重试（重试只在 job 层）。

## 3. 架构总览

三层：

1. **Provider 层（内部 trait 化）**：定义 `MetadataProvider` trait，TMDB/TVDB/IMDb/Fanart 各为实现。引入 `MetadataProviderRegistry` 取代硬编码 `match`，按配置顺序与角色（base-match / enrichment）编排。
2. **配置层（admin 可配置 + DB 持久化 + 加密）**：新增 `metadata_provider_settings` 表存 per-provider 启用状态、API key（密文）、base URL 覆盖、代理覆盖、语言/区域/图片语言策略；新增 `metadata_global_settings`（单行）存全局默认。运行时配置 = 环境变量默认值 ← DB 覆盖。
3. **开放插件 provider 契约（沙箱扩展）**：新增 `metadata.provider.query` hook 事件 + provider Host API + 合并优先级，让第三方 HTTP 插件也能作为一等刮削源参与 registry 编排。

## 4. Provider 层（内部 trait 化）

### 4.1 trait 定义

在 `src/metadata/provider/mod.rs`（把现有单文件 `provider.rs` 拆为目录）定义：

```rust
#[async_trait]
pub trait MetadataProvider: Send + Sync {
    /// 稳定标识，如 "tmdb" / "tvdb" / "imdb" / "fanart" / "plugin:{id}"。
    fn id(&self) -> &str;
    /// 角色：基础匹配源 or 仅富化（需已有 match）。
    fn role(&self) -> ProviderRole;
    /// 支持的 item 类型（movie/series/season/episode/...）。
    fn supports(&self, item_type: &str) -> bool;
    /// 基础匹配：返回候选 match（含 external_ids，供后续 provider 复用）。
    async fn match_item(
        &self,
        ctx: &ProviderContext,
        input: &MetadataLookup,
    ) -> Result<Option<MetadataMatch>, MetadataProviderError>;
    /// 富化：在已有 match 上补充字段/图片（Fanart、二级详情等）。
    async fn enrich(
        &self,
        ctx: &ProviderContext,
        input: &MetadataLookup,
        current: &mut MetadataMatch,
    ) -> Result<(), MetadataProviderError> { Ok(()) }
}

pub enum ProviderRole { BaseMatch, Enrichment }
```

- `ProviderContext` 携带：`reqwest::Client`（按 provider 代理策略构建/选择）、解析后的 provider 配置（key、base URL、语言/区域/图片语言策略）、共享 token 缓存、限流器句柄。
- `MetadataLookup` 扩展：新增 `image_language: Option<String>` 和 `image_prefer_original: bool`（见 §7）。

### 4.2 registry 编排

`MetadataProviderRegistry` 取代 `match_item_with_report` 的硬编码 `match`（`src/metadata/provider.rs:189-327`）：

- 按配置 `providers` 顺序排列 `BaseMatch` provider；首个返回 `Some` 即为基础 match，后续 BaseMatch 标 `Skipped("base metadata match already exists")`（保留现有语义）。
- 所有 `Enrichment` provider 在基础 match 命中后依次 `enrich`（Fanart 现有行为不变）。
- 保留 `MetadataLookupReport` / `MetadataProviderAttempt` 报告结构（`src/metadata/provider.rs:121-143`）不变，registry 逐 provider 记录 attempt。
- provider 实例由 registry 在配置变更时重建（配置来自 §5 的运行时合并结果）。

### 4.3 各 provider 落地

- **TMDB**：迁移现有 `search_tmdb`/`fetch_tmdb_detail`/`apply_tmdb_detail` 逻辑到 `TmdbProvider`，BaseMatch 角色；新增 `/images` 调用支持海报语言（§7）。
- **TVDB**：迁移现有逻辑到 `TvdbProvider`，BaseMatch；补传 language/region（现状完全不传），后续可补详情富化（暂保留搜索级，列为 follow-up）。
- **IMDb**：新增 `ImdbProvider`。IMDb 无官方公开 JSON API，定位为 **Enrichment**：用已匹配项的 imdb 外部 ID 拉取评分/分级等可获取字段（具体数据源在实现阶段再定，避免抓取 ToS 风险；首版可只做「确保 imdb 外部 ID 被规范为 `tt` 前缀」并预留富化点）。
- **Fanart**：迁移现有 `search_fanart`/`fanart_artwork` 到 `FanartProvider`，Enrichment 角色，行为不变。

## 5. 配置层（admin 可配置 + DB 持久化 + 加密）

### 5.1 运行时合并语义

最终 provider 配置 = **环境变量默认值（基线） ← DB 覆盖（admin 配置）**。环境变量保留为开箱默认与 CI/容器注入路径；DB 行存在时逐字段覆盖。这样既不破坏现有 `.env` 部署，又满足「admin 可配置」。

加密复用插件配置已有的 `SecretCipher` + secret 表模式（`src/plugins/routes.rs:25,648-662` 把 `secret`/`password` 字段加密进 `plugin_config_secrets`，运行时解密回填）。元数据 key 用同一 cipher，存独立的 `metadata_provider_secrets` 表，**API 永不回显明文 key**（只回显 `hasKey: bool` / 末四位掩码）。

### 5.2 数据库表（迁移 0077 起，需逐张确认）

> 全部为新增表，additive，不动现有 schema。落地前确认。

- `metadata_global_settings`（单行，`id smallint primary key default 1` + check 约束保证单行）：
  - `provider_order text[]`（默认匹配顺序，覆盖 `METADATA_PROVIDERS`）
  - `default_language text` / `default_country text`（全局文本语言/区域默认）
  - `image_language text` / `image_prefer_original bool` / `image_fallback_languages text[]`（全局图片语言策略，§7）
  - 时间戳 `updated_at`
- `metadata_provider_settings`（每 provider 一行，`provider_id text primary key`）：
  - `enabled bool`
  - `api_base_url text`（镜像覆盖）/ `image_base_url text`（TMDB 图片镜像）
  - `proxy_mode text`（`inherit` / `direct` / `custom`）/ `proxy_url text`（custom 时）
  - `language text` / `country text` / `image_language text` / `image_prefer_original bool`（per-provider 覆盖全局）
  - `secret_ref` → 关联 `metadata_provider_secrets`
  - `updated_at`
- `metadata_provider_secrets`（`provider_id text primary key`，`ciphertext bytea`，`nonce bytea`，`updated_at`）：加密的 API key/token。

### 5.3 代理策略（修复现有缺口）

实现 `no_proxy` 与 `policy`，并扩展为 per-provider：

- `policy = global-with-provider-override`（现有默认值，目前是空壳）：全局 `http_proxy`/`https_proxy` 为基线，`no_proxy` 列表生效（命中则直连），provider 的 `proxy_mode` 可覆盖：
  - `inherit`：用全局策略（含 no_proxy）。
  - `direct`：该 provider 强制不走代理。
  - `custom`：该 provider 用自己的 `proxy_url`。
- 实现位置：把现有 `MetadataProviderClient::from_config`（`src/metadata/provider.rs:151-178`）的单 client 构建改为 `ProviderContext` 按 provider 解析的 client 工厂（按 (proxy_mode, proxy_url, no_proxy) 缓存复用 client，避免每次新建）。

### 5.4 Admin API（`/api/admin/metadata/*`，server-admin 门控）

复用现有 admin 权限模式（每 handler 调 `authenticate_admin` / `can_manage_server`，并被 `every_admin_route_handler_enforces_server_admin` 守卫扫描）：

- `GET /api/admin/metadata/settings` — 返回全局 + 各 provider 配置（key 掩码，不回显明文）。
- `POST /api/admin/metadata/settings` — 写全局默认（provider 顺序、语言、区域、图片语言策略）。
- `POST /api/admin/metadata/providers/{id}` — 写单 provider 配置（启用、base URL、代理、语言/区域/图片语言覆盖）。
- `POST /api/admin/metadata/providers/{id}/key` — 设置该 provider 的 API key（加密存储，请求体限大小，响应不回显明文）。
- `DELETE /api/admin/metadata/providers/{id}/key` — 清除 key。
- `POST /api/admin/metadata/providers/{id}/test` — 用当前配置对该 provider 做一次受控连通性/鉴权探测（不写库），返回 ok/错误原因，便于管理员验证 key 与代理。

## 6. 语言 / 区域（文本元数据）

三层优先级：**库级覆盖 ← 全局默认 ← provider 内置默认**。

- 库级：保留现有 `libraries.preferred_metadata_language`/`preferred_metadata_country`（`src/metadata/service.rs:66-83`）。
- 全局：`metadata_global_settings.default_language`/`default_country`（新增）。
- 解析顺序：构造 `MetadataLookup` 时，库级非空则用库级，否则回退全局默认，再否则用 provider 默认。
- 下发：所有 BaseMatch provider 都传 language/country（修复 TVDB 现状不传的问题）；官方分级国家偏好链保留现有 fallback（请求国家 → US → 任意，`src/metadata/provider.rs:1159-1256`）。

## 7. 海报原语言 / 文本单独切换（核心新功能）

图片语言**独立于文本语言**配置。`MetadataLookup` 新增：

- `image_language: Option<String>` — 图片偏好语言（默认回退到文本 language）。
- `image_prefer_original: bool` — 偏好原语言海报（用条目 `original_language`）。
- `image_fallback_languages: Vec<String>` — 兜底顺序，含特殊值 `none`/`xx`（无文字版，TMDB 用空 `iso_639_1`）。

### 7.1 TMDB 实现

现状只取详情单张图（`src/metadata/provider.rs:771-794`）。改为调 TMDB `/{movie|tv}/{id}/images?include_image_language={langs}`（或详情 `append_to_response=images` 并带 `include_image_language`），按排序挑选 poster/backdrop。

排序优先级（rank 小者优先，同 rank 按 `vote_average` 降序）：
1. `image_prefer_original=true` 时：`iso_639_1 == original_language` → rank 0。
2. `image_language` 匹配 → rank 1。
3. `image_fallback_languages` 按序 → rank 2+。
4. 无语言版（`iso_639_1` 为 null，`none`/`xx` 命中）→ 对应 fallback rank。
5. 其余 → 最低。

复用现有 Fanart 语言排序的思路（`fanart_language_rank`，`src/metadata/provider.rs:1083-1093`），抽成 provider 共享的 `pick_localized_artwork` helper。

### 7.2 配置位

全局 `metadata_global_settings.image_*` + per-provider `metadata_provider_settings.image_*` 覆盖 + 库级覆盖（**已落地**，迁移 `0079_library_image_language.sql` 给 `libraries` 加 `preferred_image_language` / `preferred_image_prefer_original` / `preferred_image_fallback_languages`，`build_lookup` 实现「库级 ← 全局 ← provider 内置」优先级折叠，并经 `GET` / `POST .../settings` 管理端点读写）。admin UI 暴露「文本语言」与「海报语言 / 偏好原语言」两个独立控件。

## 8. 开放插件 provider 契约（沙箱扩展）

让第三方 HTTP 插件作为一等 BaseMatch/Enrichment provider 参与 registry。在内部 trait 化完成后增量加：

- **新 hook 事件 `metadata.provider.query`**：加入 `SUPPORTED_HOOK_EVENTS`（`src/plugins/manifest.rs:34-51`），加 DB 约束迁移 + 守卫测试（`src/plugins/manifest.rs:1416-1426` 有对应测试需同步）。
- **新权限 key**：现有 `metadata.read` 是预留空壳（`src/plugins/host.rs:369-375`），用它作为「插件读取待刮削上下文并返回 provider 候选」的权限。
- **PluginMetadataProvider**：registry 里一个把插件包成 `MetadataProvider` 的适配器。`match_item`/`enrich` 时通过 hook dispatch 把 `MetadataLookup` 发给插件，插件返回 `MetadataMatch` 形态的 JSON，经现有 `metadata.write` 字段白名单校验（`src/plugins/host.rs:2683-2800`）后并入。
- **同步执行约束**：插件 dispatch 现状是异步 outbox（`src/plugins/hooks.rs:52-71`），而 provider query 需要同步拿结果。需评估两种方案：(a) 给 provider query 走一条同步 HTTP 调用路径（仅 HTTP runtime，带超时与预算）；(b) 刮削链分两阶段，插件结果异步回写。**首版建议 (a)，仅 HTTP runtime，带硬超时**，避免刮削 job 被插件拖死。
- artwork 仍走插件 scoped source 隔离（`plugin:{id}`，`src/plugins/host.rs:2245-2288`），与内置 provider 的 source 命名空间区分。

> 第 8 节是 stretch goal，可在 §11 阶段 5 再做；前四个阶段不依赖它。

## 9. 合并优先级与冲突

多 provider/插件结果合并规则（写进 registry，落库前确定）：

- **基础字段**（title/overview/year/rating 等）：首个 BaseMatch 命中者为准，后续 BaseMatch 跳过（保留现有语义）。
- **外部 ID**：所有 provider 的 external_ids 累积去重 upsert（现有 `media_external_ids` 冲突键 `(media_item_id, provider)`，`src/metadata/service.rs:366-385`）。
- **图片**：按 source 分组（内置 provider source vs `plugin:{id}` source），primary 由图片语言策略选定；现有按 source 删旧再插的逻辑保留（`src/metadata/service.rs:387-425`）。
- **genres/studios/people**：现有 delete-then-insert replace 语义（`src/metadata/write.rs:50-209`），多 provider 时由 BaseMatch 提供，Enrichment 仅补充未覆盖项。
- **插件 vs 内置冲突**：内置 provider 优先级高于插件（避免第三方插件覆盖核心刮削），插件结果仅填补空字段 + 追加 scoped artwork。

## 10. 测试策略

- provider trait + registry：单元测试 mock provider，断言编排顺序、BaseMatch 跳过、Enrichment 补充语义。
- 配置合并：环境变量 ← DB 覆盖的逐字段优先级测试；key 掩码不回显明文测试。
- 代理策略：`no_proxy` 命中直连、per-provider `direct`/`custom` 覆盖的单元测试。
- 海报语言：`pick_localized_artwork` 的 rank 排序测试（prefer_original、image_language、fallback、none 版各一组）。
- 迁移：每张新表加迁移结构测试（仿 `jobs.rs` 的 partition 测试模式）+ 本地 dockerized PG 实跑校验迁移链。
- admin API：路由存在测试 + server-admin 门控（被 `every_admin_route_handler_enforces_server_admin` 覆盖）。
- secret 加密：复用插件 secret cipher 的往返测试模式。
- 兼容回归：现有 `provider.rs` 的 TMDB/TVDB/Fanart 测试（`src/metadata/provider.rs:1677-2085`）迁移到新 provider 模块后必须全绿。

## 11. 分步实施计划（每步独立可验证、可单独确认）

每一步完成后跑 `cargo fmt -- --check` / `cargo test --lib` / `cargo build`，并更新 `backend-execution-goal.md`。

- **阶段 0 — 重构准备（无行为变更）**：把 `provider.rs` 拆为 `provider/` 目录（mod + tmdb + tvdb + fanart + 共享类型），现有测试全绿。引入 `MetadataProvider` trait 与 `MetadataProviderRegistry`，用 registry 复刻现有硬编码 `match` 行为（纯重构，输出不变）。
- **阶段 1 — 配置层 + DB**：迁移 0077+ 新增三张表；运行时配置合并（env ← DB）；admin API 读写 + key 加密 + 掩码 + test 探测。**先确认迁移**。
- **阶段 2 — 代理修复**：实现 `no_proxy` + policy + per-provider 代理 client 工厂。
- **阶段 3 — 语言/区域 + 海报语言**：全局默认层 + TVDB 补传语言；TMDB `/images` + `pick_localized_artwork` + 海报原语言/文本独立切换。这步交付用户的核心新功能。
- **阶段 4 — IMDb provider + TVDB 详情富化**：补 IMDb enrichment 与 TVDB 详情（genres/people/studios），按数据源可得性推进。
- **阶段 5（stretch）— 开放插件 provider 契约**：新 hook 事件 + `metadata.read` 权限落地 + PluginMetadataProvider 适配器 + 同步 query 路径 + 合并优先级。

## 12. 已知风险与待确认项

- **破坏性结构变更**：本设计只新增表，不动现有 schema；但 provider 模块拆分是较大重构，阶段 0 必须保证现有测试零回归。
- **IMDb 数据源**：无官方公开 API，需在阶段 4 明确合规的数据获取方式（避免抓取 ToS 风险），否则 IMDb 仅作外部 ID 规范化。
- **插件同步 query**：现有插件 dispatch 是异步 outbox，阶段 5 的同步 provider query 需要新执行路径，是最高复杂度部分，建议最后做。
- **配置热更新**：provider 实例在配置变更时重建，需确认是进程内 watch 还是下次 job 重读 DB（首版可下次 job 重读，简单可靠）。
