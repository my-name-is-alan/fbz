# StorageProvider 扩展点（P1 草稿）

对照 `docs/FBZ-目标架构与落地路线.md` §4.2 / §5.2 / §5.7 / §6.5 / §6.8。

## 合同

- 扩展点键：`storage.provider`（capabilities 列出；同步 RPC）
- 权限键：`storage.account` / `storage.read` / `storage.resolve`
- Ops：`list` / `stat`（可选）/ `resolve` / `read`（readSmallFile）/ `auth.start` / `auth.poll`
- 宿主 trait：`fbz-api/src/storage/provider.rs`（`StorageProvider` + `StorageProviderRegistry`）
- HTTP 插件可实现现有 JSON RPC；WASI 后续共用同一 ops 面

## 去 guangya 特判（本轮）

| 原硬编码 | 现路径 |
| --- | --- |
| `FBZ_GUANGYA_PLUGIN_URL` 直连 | 注册表 `HttpRpcStorageAdapter`；优先 `FBZ_STORAGE_PROVIDER_ENDPOINT_guangya`，回退旧 env |
| `insert … provider='guangya'` | `create_account` 按注册表校验；默认仍为 `guangya` |
| `check (provider = 'guangya')` | migration `0101` 放宽为短 id 正则 |
| 宿主错误文案写死「光鸭」 | 通用「云盘/存储插件」；适配器可保留显示名 |

过渡期：光鸭 Node 进程与 `FBZ_STORAGE_PLUGIN_KEY` 不变；adapter 包住既有协议。

## 挂载缓存 / Extra（本轨不重复造轮）

并行草稿已落在独立模块（避免与 `storage/*` 冲突），本轮 StorageProvider 接线**直接复用**：

| 域 | 路径 |
| --- | --- |
| 挂载缓存 key / 失效事件 | `fbz-api/src/cache/mount.rs` |
| Extra 类型 + 扫描启发式 | `fbz-api/src/media/extras.rs` |
| 扫描生命周期 / 事件名 | `fbz-api/src/scan/lifecycle.rs` + `fbz-api/docs/plans/scan-lifecycle-mount-cache-extras-draft.md` |

已有挂载开关 `nfo_source` / `image_cache` 保持；预告片缓存列与 Extra 关联表仍属后续 PR。

## 验收

1. `cargo check -p fbz-api` / `cargo test -p fbz-api --lib storage::`
2. `GET /api/plugin/capabilities` 含 `storage.account|read|resolve` 与 `extensionPoints` 含 `storage.provider`
3. Admin `GET /api/admin/storage` 的 `providers` 含 `guangya`
4. 既有光鸭扫码 / list / 302 播放路径仍经 `storage::rpc` → registry → adapter
5. 不 commit；不改 Emby/鉴权网关

## 还剩什么

- 第二 mock StorageProvider 示例包另仓 + 真装包生命周期（非 env 旁路）
- `stat` 在光鸭插件侧落地；trailer_cache 列与 Extra 关联最小表
- 云扫描事件与 `library.scan.*` 统一
- 进程监督 / 官方 Provider 迁出（同 P1 其它轨）
