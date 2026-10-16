# 官方元数据 Provider 迁出（真实查询落地）

状态：**官方包已实现真实 `metadata.provider.query`**（TMDB/TVDB/Fanart 对齐 core 请求语义；IMDb/豆瓣最小可用）。core 内置客户端**仍可运行**（冻结加深）；合并层插件优先贡献；内置 fallback；命中后补空。不改鉴权网关。

对齐真源：`docs/FBZ-目标架构与落地路线.md` §5.9 / §5.8 / §10 P1。

## 1. 已落地

| 产出 | 路径 |
| --- | --- |
| 官方包目录说明 + 密钥冻结策略 | `plugins/README.md` |
| 共享工具（各包另有 `utils.mjs` 副本） | `plugins/_shared/provider-utils.mjs` |
| TMDB / TVDB / Fanart 真实查询 | `plugins/{tmdb,tvdb,fanart}-provider/`（`provider.mjs` + `server.mjs`） |
| IMDb / 豆瓣最小可用 | `plugins/{imdb,douban}-provider/` |
| 片头片尾探测身份 stub | `plugins/marker-detect/` + `docs/plans/media-marker-detect-draft.md` |
| core 冻结注释 | `src/metadata/provider/{mod,tmdb,tvdb,fanart,imdb}.rs` |

## 2. 合同（不变，接线路径）

- **扩展点：** `metadata.provider.query`（同步 SyncInvoker；`plugin_sync.rs` / `invoke.rs`）
- **输入：** `request.lookup`（类型/标题/年/季集/外部 id 提示…）+ 可选 `request.current`；**不含**其它源密钥
- **输出：** `{ metadata: Contribution | null }`；宿主 `merge_plugin_metadata` 合并（插件优先贡献；内置 fallback；命中后补空）
- **贡献白名单（camelCase）：** `title, originalTitle, overview, productionYear, premiereDate, officialRating, communityRating, externalIds[], artwork[], genres, studios, people[]`
- **密钥解析顺序：** Host `/api/plugin/config` → `dispatch.config` → 进程 env（各包 README）
- **清单：** `hooks[].event = metadata.provider.query`，权限 `metadata.read`，`configSchema` 声明 secret
- **发现：** 启用后 `discover_plugin_providers` → registry 动态加入 `plugin:{id}`
- **库级链：** `library_metadata_sources.source_key` 可写 `plugin:fbz.provider.tmdb` 等

## 3. 密钥策略（冻结）

| 允许 | 禁止 |
| --- | --- |
| 插件 `configSchema` type=`secret` → `plugin_config_secrets` | 新 PR 往 core 加深供应商 SDK / 新的 `*_API_KEY` 写死路径 |
| 插件进程环境（compose/systemd secret） | Key 写入 manifest / git / fixture |
| 过渡期沿用现有 core env 驱动**内置**客户端 | 以「临时」为名新增第二套 core 客户端 |

## 4. 过渡期运行关系（插件优先 + 源链接线）

```text
metadata.refresh
  → load library_metadata_sources（有 enabled 链则按 priority；否则全局 providers 序）
  → 每步 provider：
       1) 已启用官方/社区插件 SyncInvoker（metadata.provider.query）优先
       2) 插件未启用 / 空贡献 / 失败 → 内置 tmdb.rs 等 fallback
  → NFO 槽：skipped（nfoMatchIntegration=pending），不读 .nfo I/O
  → 合并：插件贡献优先落地；内置补空；命中后双方均可 enrich
```

**新路径以插件为准**；内置仅过渡 fallback。禁用官方包 ≠ 失去刮削。删除内置客户端是**后续切流 PR**。

装载：与光鸭同一 loader——`package-plugin.ps1` → `POST /api/admin/plugins/packages` → 审批 → 启用；manifest 校验见 `first_party_official_metadata_provider_manifests_are_valid`。

## 5. 后续切流 PR

1. registry 默认不再静态注册内置；仅发现 `plugin:*`  
2. 发行物预装 ZIP + 默认启用推荐源；`library_metadata_sources` 预填 `plugin:fbz.provider.*`  
3. 删除 core 供应商 HTTP 客户端；保留 merge / cache / job / 图片拉取  

## 6. 验收

- [x] 各 `server.mjs` / `provider.mjs` / `utils.mjs` `node --check`  
- [x] README：密钥 + 本地 curl sync 冒烟  
- [x] IMDb suggest 无 key 冒烟可返回 title + tt id（网络可达时）  
- [x] 官方 provider manifest 校验测试（对照光鸭）  
- [x] Match / `library_metadata_sources` 插件优先 + 内置 fallback 接线  
- [x] NFO 槽 pending 轻量占位（skipped attempt，无 I/O）  
- [ ] （手工）带真实 TMDB/TVDB/Fanart/OMDb key 的 curl / 装包刮削  
- [ ] （非本轮）装包启用后 live 刮削仅走插件（可关内置 fallback）  

## 7. 非目标

鉴权网关 / Emby token、删除内置客户端、完整片头算法、commit。
