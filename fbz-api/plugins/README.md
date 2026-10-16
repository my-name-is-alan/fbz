# FBZ 官方插件包（first-party）

本目录存放**可随发行附带**的官方插件工程。运行时仍是进程外 HTTP 包：安装 → 审批 → 启用 → SyncInvoker / Host API，**不是**链进 core 的 DLL。

| 包目录 | Manifest id | 扩展点 / 合同 | 本轮状态 |
| --- | --- | --- | --- |
| `guangya-storage/` | `dev.fbz.storage.guangya` | Storage RPC（既有） | 已有实现 |
| `tmdb-provider/` | `fbz.provider.tmdb` | `metadata.provider.query` | **真实 TMDB v3 查询** |
| `tvdb-provider/` | `fbz.provider.tvdb` | `metadata.provider.query` | **真实 TVDB v4 查询** |
| `fanart-provider/` | `fbz.provider.fanart` | `metadata.provider.query` | **真实 Fanart 图片 enrichment** |
| `imdb-provider/` | `fbz.provider.imdb` | `metadata.provider.query` | **最小可用（OMDb / suggest）** |
| `douban-provider/` | `fbz.provider.douban` | `metadata.provider.query` | **最小可用（suggest + 详情 fallback）** |
| `openlibrary-provider/` | `fbz.provider.books` | `metadata.provider.query`（仅 itemType=book） | **Open Library（免 key）+ Google Books（key 可选）** |
| `lrclib-provider/` | `fbz.lyrics.lrclib` | `lyrics.provider.query` | **LRCLIB 远程歌词（免 key）** |
| `marker-detect/` | `fbz.marker.detect` | `media.marker.detect` | **章节名 + 静音/黑场启发式（可装包）** |

共享工具：`_shared/provider-utils.mjs`（各包内另有打包用 `utils.mjs` 副本）。示例模板仍在 `examples/plugins/`。

## 密钥策略（冻结）

1. **供应商 API Key / Token 只进插件 `configSchema`（secret）或插件进程环境**，落 `plugin_config_secrets`（或部署 secret）。  
2. **禁止**再往 core 加深新的 `TMDB_*` / `TVDB_*` / `FANART_*` / 豆瓣 / IMDb 写死客户端与密钥路径（见架构 §5.9 / §5.8）。  
3. Manifest **不得**含密钥；打包物与 README 示例用占位符。  
4. 过渡期：core `MetadataConfig` / `metadata_provider_secrets` 可继续驱动内置客户端，直到官方包切流 PR 删除内置实现。

## 过渡期接线（插件优先，内置 fallback）

```text
metadata.refresh job
  → library_metadata_sources（库级 priority；无配置则全局 providers 序）
  → 每步：
       1) PluginSyncInvoker → metadata.provider.query（已启用官方/社区包）← 优先
       2) core built-in TMDB/TVDB/Fanart/IMDb                         ← fallback
  → NFO 槽：pending 占位（skipped），不读本地 .nfo
```

库级 `source_key` 可写 `tmdb` 或 `plugin:fbz.provider.tmdb`（都会优先尝试官方插件，若已启用）。装载与光鸭相同：`package-plugin.ps1` → admin packages API → 审批 → 启用。

详细：`docs/plans/official-metadata-providers-migration.md`。各包 README 含密钥表与 **curl sync 冒烟**。

## 打包

```powershell
./scripts/package-plugin.ps1 -PluginDir plugins/tmdb-provider -Force
./scripts/package-plugin.ps1 -PluginDir plugins/tvdb-provider -Force
./scripts/package-plugin.ps1 -PluginDir plugins/fanart-provider -Force
./scripts/package-plugin.ps1 -PluginDir plugins/marker-detect -Force
./scripts/package-plugin.ps1 -PluginDir plugins/imdb-provider -Force
./scripts/package-plugin.ps1 -PluginDir plugins/douban-provider -Force
```

不改鉴权网关；本目录变更默认不 commit（由调用方决定）。
