# FBZ 验收清单与自测说明（2026-10-16 晨 · 上海时间）

| 字段 | 值 |
|------|-----|
| 验收窗口 | 2026-10-16 晨（CST / UTC+8） |
| 盘点基准 | 仓库 `/Users/edy/Code/fbz`，相对 `HEAD` 的未提交改动 + 未跟踪文件（**未 commit**） |
| 文档刷新 | 2026-10-15 ~10:15 PT：附录 C SyncPlay/Books；~10:20 PT Live TV 空桩不 500 再核、**非明早必验**；~17:10 PT 第五轮（附录 C.6 + 下方晨验速查）；~17:35 PT 第六轮（附录 C.7）；~17:50 PT 第七轮（附录 C.8）；~18:40 PT 第八轮（附录 C.9，晨验速查已更新） |
| 默认 API | `http://127.0.0.1:8080`（`FBZ_API_HOST` / `FBZ_API_PORT`） |
| 默认 Web | 本地 `fbz-fe` 开发服（代理到上述 API） |
| 鉴权约定 | C 端 Emby：`X-Emby-Token: <AccessToken>` 或 `Authorization: MediaBrowser Token="<AccessToken>"`；Admin：须 **AccessToken**（不可用长期 ApiKey）且账号具备管服权限 |

**勾选约定：** `[ ]` 未验 → `[x]` 通过。失败时在项下记实际结果与证据路径。

**状态标签：**

- **已落地**：工作区已有实现/文档，可按「怎么测」验收。
- **待合入**：骨架文件已出现但未挂进 `mod`/路由，或架构约定尚未实现；晨验可核对文件存在，功能项记缺口。

---

## 晨验速查（按优先级，约 30 分钟）

> 详细步骤见各节与附录 C.4–C.9；冒烟脚本在 `/tmp/fbz-smoke/*.sh`（独立库 `fbz_smoke_1016`，可复跑）。全部**未 commit**。

**P0 — 必验（构建与核心链路）**
1. [ ] 构建/测试全绿：`cargo test --lib`（1414 passed / 50 ignored）、`cargo build`；`fbz-fe`：`npm test`（83 passed）、`npx vp check --no-fmt`（0 error）、`npm run build`；插件 `node --test provider.test.mjs`（opensubtitles 8、openlibrary 8、lrclib 5）。
2. [ ] 起服 `GET /ready` 200；迁移到 `0110`（含 `0110_user_access_policy_image_locks`、 `0107_books_library`、`0108_subtitle_provider_hook`、`0109_metadata_locks_nfo_lyrics`）。
3. [ ] 登录 C 端，首页/媒体库/播放/续播正常（§2、§2.5）。
4. [ ] 冻结项未动：鉴权网关、后台视觉、Quick Connect 未做、LiveTV 非目标（§6）。

**P1 — 本夜新增能力（逐项点验）**
5. [ ] SyncPlay 两端：建组/加入 → 同步暂停/跳转 → 随机/循环 → 队列 Queue/QueueNext/移动/移除（附录 C.5/C.6；`sp2b.sh`、`sp3.sh`）。
6. [ ] Books：建 books 库 → 电子书带封面/作者/年份、cbz 在子目录 Folder 下 → 阅读器 epub/cbz/pdf → 有声书点开从上次段落/位置续听（C.6）。
7. [ ] 远程字幕：启用 `fbz.subtitle.opensubtitles` → 搜索有结果 → 管理员下载后 PlaybackInfo 出现外挂字幕、文本无广告行（C.5/C.6）。
8. [ ] 播放列表共享弹窗：公开开关 / 共享（只读→可编辑）/ 移除 / 非创建者 403 提示；`DELETE /Items/{playlistId}` 属主可删（`pl.sh`）。
9. [ ] Trickplay：入库后自动生成，进度条悬停有预览（C.2/C.5）。
10. [ ] Books 第六轮（C.7）：`/Users/{uid}/Items?IncludeItemTypes=Book&Fields=Overview,People` 返回作者/简介（不带 Fields 不返回）；`POST /Items/{书库id}/Refresh` 后已入库书补齐元数据；mobi/azw3 阅读器分章+插图、cbr 翻页；启用 `fbz.provider.books`（`cd fbz-api/plugins/openlibrary-provider && PORT=19108 node server.mjs`）后，无元数据的书（如 `Dune.pdf`）刷新得到作者/简介/年份/ISBN。
11. [ ] 元数据编辑器 `POST /Items/{id}`（管理员 204、普通用户 403）→ 详情回显 `ProviderIds/Studios/Tags/SortName/LockData`；`NFO_SAVER_ENABLED=true` 时同目录生成 `{文件名}.nfo`，手写 NFO 不被覆盖。

12. [ ] 锁定（C.8，`s7a.sh`）：编辑器提交 `LockData=true` → 刷新/重扫/裸 SQL `update` 都改不动；`LockedFields=["Overview"]` → 简介不变而 SortName 仍可改；编辑器再提交 `LockData=false` 可解锁。
13. [ ] NFO 按库（C.8，`s7b.sh`，**不设** `NFO_SAVER_ENABLED`）：`POST /Library/VirtualFolders/LibraryOptions {"Id":库id,"LibraryOptions":{"MetadataSavers":["Nfo"]}}`（普通用户 403）→ VirtualFolders 回显 → 该库条目刷新后生成 `{文件名}.nfo`；未开启的库不生成。
14. [ ] 远程歌词（C.8，`s7c.sh`）：启用 `fbz.lyrics.lrclib`（`cd fbz-api/plugins/lrclib-provider && PORT=19109 node server.mjs`）→ `GET /Audio/{id}/RemoteSearch/Lyrics` 返回同步歌词 → `POST /Audio/{id}/RemoteSearch/Lyrics/{lyricId}` 写 `.lrc` → `GET /Audio/{id}/Lyrics` 有内容；停用插件后回退内置 LRCLIB。
15. [ ] 文件夹权限（C.9，`s8a.sh`）：`EnableAllFolders=true + BlockedMediaFolders=[书库]` → GET Policy **原样回显**、Views 不含书库；管理员新建库 → 该用户**自动可见**；`EnableAllFolders=false, EnabledFolders=[电影库]` → 只剩电影库，再新建库不可见；只带 `IsDisabled` 的 POST 不改已存策略。
15a. [ ] 家长控制（C.9，`s8b.sh`）：`MaxParentalRating=5` → R / PG-13 电影与 TV-MA 剧的分集在列表/Latest/详情/Episodes 全部消失（详情 404）；`BlockedTags=["horror"]` → 带 Horror 标签的条目消失；管理员不受影响。
15b. [ ] 图片锁 + 季 NFO + 歌词回退（C.9，`s8c.sh`）：`LockedFields=["Images"]` → 裸 SQL 增删 artwork 无效、刷新不改图，手动上传/删除仍可；剧库开 Nfo 后刷新分集生成 `season.nfo`；停用歌词插件时回退结果为「标题 | Queen | 专辑」而非「歌名 - 歌手」×3。
16. [ ] Trickplay 状态重启后保留（`…/Trickplay/Status`）；阅读器标题旁显示 `MOBI → EPUB` 徽标与「下载原文件」。

**P2 — 抽查**
17. [ ] NextUp 四参数、字幕上传、歌词上传、PlaylistDto（C.3/C.4）。
18. [ ] 已知缺口知悉（§8、矩阵「未做」与「第七轮增补」）：DRM 电子书、真 RAR 格式 cbr 未实机、strm 文件 moviehash、SyncPlay 跨节点、AllowedTags/AccessSchedules、聚合接口（Genres 计数/SearchHints）未按条目过滤、书籍 NFO。

---

## 0. 冒烟前置

- [ ] 后端可起：`cd fbz-api && cargo run`（或既有 `scripts/dev.ps1`），`GET /ready` → 200。
- [ ] 前端可起：`cd fbz-fe && pnpm dev`，能登录并看到首页/媒体库。
- [ ] 迁移已应用：含 `0100_library_metadata_sources.sql`（库级源链表）。
- [ ] 持有：普通用户 `USER_TOKEN`、管理员 `ADMIN_TOKEN`、至少一个 `LIBRARY_ID`（UUID）、至少一个可播 `ITEM_ID`。

```bash
# 健康
curl -sS -o /dev/null -w '%{http_code}\n' http://127.0.0.1:8080/ready
# 登录拿 Token（按环境改用户名密码）
curl -sS -X POST 'http://127.0.0.1:8080/emby/Users/AuthenticateByName' \
  -H 'Content-Type: application/json' \
  -H 'X-Emby-Authorization: MediaBrowser Client="Acceptance", Device="curl", DeviceId="acc-1", Version="0"' \
  -d '{"Username":"ADMIN","Pw":"YOUR_PASSWORD"}'
```

---

## 1. P0

### 1.1 app 测试外迁 — **已落地**

| | |
|--|--|
| **验收点** | `fbz-api/src/app.rs` 仅保留路由装配；原大段 `#[cfg(test)]` 外迁至 `app_router_tests.rs`，经 `#[path = "app_router_tests.rs"]` 挂载。 |
| **怎么测** | 1) `wc -l fbz-api/src/app.rs` 应约百行量级（盘点时 ~151），`app_router_tests.rs` 数千行（~5058）。2) `grep -n 'app_router_tests' fbz-api/src/app.rs` 可见 `#[path]`。3) `cd fbz-api && cargo test --lib app::tests -- --nocapture`（或全量 `cargo test --lib`）应能编译并跑通挂载模块内测试。 |
| **期望结果** | `app.rs` 不再内联海量路由测试；测试模块可编译执行；无「测不过却声称已外迁」。 |

- [ ] 行数/挂载结构符合
- [ ] `cargo test` 相关模块通过

### 1.2 光鸭正式 plugin 包 — **已落地（包身份；RPC 仍走 env）**

| | |
|--|--|
| **验收点** | `fbz-api/plugins/guangya-storage/manifest.json`（id `dev.fbz.storage.guangya`）+ README「插件包身份」；`plugins/manifest.rs` 单测校验 manifest；密钥不进 manifest。进程仍可由 `node server.mjs` / compose 拉起，宿主继续 `FBZ_GUANGYA_PLUGIN_URL`。 |
| **怎么测** | 1) `cd fbz-api && cargo test --lib plugins::manifest::tests::first_party_guangya_storage_manifest_is_valid`。2) 目视 `manifest.json`：无 `FBZ_STORAGE_PLUGIN_KEY` / OAuth secret。3)（可选）`pwsh scripts/package-plugin.ps1 -PluginDir plugins/guangya-storage` 能打出包。4)（可选）`cd plugins/guangya-storage && node --test provider.test.mjs`。 |
| **期望结果** | Manifest 校验通过；包可识别为正式 HTTP 插件身份；**不**要求本轮已去掉 core 内 `provider='guangya'` 特判（那属 StorageProvider 合入，见 §5）。 |

- [ ] Manifest 单测通过
- [ ] README / 密钥策略说明在位
- [ ] （可选）打包脚本 / provider 单测

### 1.3 库级 metadata sources 配置 API（GET/PUT）— **已落地（配置面）**

| | |
|--|--|
| **验收点** | `GET|PUT /api/admin/libraries/{library_id}/metadata/sources`；迁移 `0100_…`；创建库 `seed_defaults_in_tx`；响应含 `nfoMatchIntegration`（盘点 `"pending"` → 夜间 **`"local"`**，附录 A.1）。**不接后台 UI**。 |
| **怎么测** | 替换 `ADMIN_TOKEN`、`LIBRARY_ID`： |

```bash
# GET
curl -sS "http://127.0.0.1:8080/api/admin/libraries/${LIBRARY_ID}/metadata/sources" \
  -H "X-Emby-Token: ${ADMIN_TOKEN}" | jq .

# PUT 全量替换（示例：NFO + TMDB）
curl -sS -X PUT "http://127.0.0.1:8080/api/admin/libraries/${LIBRARY_ID}/metadata/sources" \
  -H "X-Emby-Token: ${ADMIN_TOKEN}" \
  -H 'Content-Type: application/json' \
  -d '{
    "sources": [
      {"kind":"nfo","key":"nfo","enabled":true,"priority":10},
      {"kind":"provider","key":"tmdb","enabled":true,"priority":20}
    ]
  }' | jq .

# 负例：非 admin → 401/403；非法 libraryId → 422；重复 provider → 422
curl -sS -o /dev/null -w '%{http_code}\n' \
  "http://127.0.0.1:8080/api/admin/libraries/not-a-uuid/metadata/sources" \
  -H "X-Emby-Token: ${ADMIN_TOKEN}"
```

| **期望结果** | GET 返回 `libraryId`、`sources[]`（`kind`/`key`/`enabled`/`priority`，provider 可有 `providerId`）、`nfoMatchIntegration`（盘点时为 `"pending"`；**夜间已改为 `"local"`，见附录 A.1**）。新建 movies 库默认链约 NFO→TMDB→TVDB→Fanart；tvshows 偏 TVDB。PUT 后 GET 一致。 |
| **单测** | `cargo test --lib library::metadata_sources`；admin 路由注册断言 `library_metadata_sources_admin_routes_are_registered`。 |

- [ ] GET/PUT 行为符合
- [ ] 新建库有默认链
- [ ] `nfoMatchIntegration` 为 `local`（附录 A.1；勿再验 pending）
- [ ] 相关单测通过

### 1.4 非产品 README — **已落地**

| | |
|--|--|
| **验收点** | 根 `README.md` 标明 `demo/` / `canvas/` / `chatcontinue/` 为非产品；各目录自有 README 声明不参与 `fbz-api`/`fbz-fe` 构建部署。 |
| **怎么测** | 打开并确认文案：`README.md`、`demo/README.md`、`canvas/README.md`、`chatcontinue/README.md`。 |
| **期望结果** | 新人不会把 demo/canvas/chatcontinue 当产品入口；产品树以 `fbz-api` / `fbz-fe` / `docs` 为准。 |

- [ ] 根 README 结构说明已更新
- [ ] 三目录非产品 README 存在且表述一致

---

## 2. C 端（Web）

前提：浏览器登录普通用户，库内有封面、题材、评分、进度数据更佳。

### 2.1 右键菜单 — **已落地**

| | |
|--|--|
| **怎么测** | 首页/库卡片 **右键**：收藏、标记已看/未看、播放/继续播放；管理员可见刷新元数据等。电影「播放」应起播；剧集进详情。 |
| **期望结果** | 已看切换成功 toast；标已看后进度条清除；继续播放仅在有未看完进度时出现；失败有明确 toast，不静默。 |
| **API 对照** | `POST|DELETE /emby/Users/{userId}/PlayedItems/{itemId}`（`userData.setPlayed`）。 |

- [ ] 收藏 / 已看 / 播放菜单可用

### 2.2 搜索 / 筛选 / 题材 — **已落地**

| | |
|--|--|
| **怎么测** | 1) **搜索页** `/search`：关键词 + 类型下拉 + 排除标签；URL 应带 `q` / `type` / `excludeTags`，前进后退可复现。2) **库详情**：题材下拉、已看/未看、收藏、排除标签；选项优先来自 `GET /Items/Filters`。 |
| **期望结果** | 筛选结果与服务端一致（非仅前端本地滤当前页）；无权限/空库有可读空态。 |

- [ ] 搜索类型/排除标签
- [ ] 库内题材与 IsPlayed / IsFavorite

### 2.3 封面 — **已落地（依赖库内 artwork）**

| | |
|--|--|
| **怎么测** | 库网格 `layout=poster`；有 Primary 的条目显示海报；无图有占位。右上分辨率徽章、左上已看 ✓、底部进度条互不遮死关键区。 |
| **期望结果** | 封面 URL 经 Emby Images 链路可加载（非裂图）；已看/进度叠加正确。 |

- [ ] 封面与徽章

### 2.4 分页 — **已落地**

| | |
|--|--|
| **怎么测** | 库条目数 > 36（`LIBRARY_PAGE_SIZE`）时出现「上一页 / 下一页」；切页应重新请求 `StartIndex`/`Limit`，总数来自服务端 `TotalRecordCount`。 |
| **期望结果** | 页码与 `range` 文案正确；改筛选/排序时回到第 1 页。 |

```bash
curl -sS "http://127.0.0.1:8080/emby/Users/${USER_ID}/Items?ParentId=${LIBRARY_ID}&Recursive=true&Limit=36&StartIndex=0&SortBy=SortName" \
  -H "X-Emby-Token: ${USER_TOKEN}" | jq '{TotalRecordCount, n:(.Items|length)}'
```

- [ ] UI 分页与 API StartIndex 一致

### 2.5 播放器：版本切换 / 进度 / 已看 — **已落地（Web）**

| | |
|--|--|
| **怎么测** | 1) 多 MediaSource 条目：全屏播放器出现版本下拉，切换后尽量保留当前秒数续播。2) 有进度卡片点播放 → 续播；已看完或无进度 → 从头（`startPositionSeconds=0`）。3) 右键标已看后卡片出现 ✓、进度条消失。 |
| **期望结果** | `playback.switchMediaSource` + `FullscreenPlayer` 联动；失败 toast「无法切换该版本…」。进度上报仍走既有 Sessions/Playing|Progress|Stopped（本轮未改鉴权网关）。 |

- [ ] 版本切换续播
- [ ] 续播 / 从头
- [ ] 已看角标与进度

---

## 3. Items：Genres / CommunityRating / SortBy — **已落地**

| | |
|--|--|
| **验收点** | 列表/详情 DTO 可序列化 `Genres`、`CommunityRating`；浏览 SQL 聚合题材与评分；`SortBy=CommunityRating` 映射 `ItemSortField::CommunityRating`。 |
| **怎么测** | |

```bash
# 列表应能出现 Genres / CommunityRating（有数据时）
curl -sS "http://127.0.0.1:8080/emby/Users/${USER_ID}/Items?ParentId=${LIBRARY_ID}&Recursive=true&Limit=5&Fields=Genres" \
  -H "X-Emby-Token: ${USER_TOKEN}" \
  | jq '.Items[0] | {Name, Genres, CommunityRating}'

# 按评分排序
curl -sS "http://127.0.0.1:8080/emby/Users/${USER_ID}/Items?ParentId=${LIBRARY_ID}&Recursive=true&SortBy=CommunityRating&SortOrder=Descending&Limit=5" \
  -H "X-Emby-Token: ${USER_TOKEN}" \
  | jq '[.Items[] | {Name, CommunityRating}]'

# 题材筛选（若库有题材）
curl -sS "http://127.0.0.1:8080/emby/Users/${USER_ID}/Items?ParentId=${LIBRARY_ID}&Recursive=true&Genres=Action&Limit=5" \
  -H "X-Emby-Token: ${USER_TOKEN}" | jq '{TotalRecordCount, names:[.Items[].Name]}'
```

| **期望结果** | 无题材/无评分时字段省略（不强制空数组/0）。SortBy 未知字段回退默认。前端库「按评分」透传 `CommunityRating`。 |
| **单测** | DTO 序列化用例；`emby_sort_field_maps_community_rating_and_existing_keys`。 |

- [ ] 列表字段
- [ ] SortBy=CommunityRating
- [ ] 前端评分排序联调

---

## 4. 兼容与播放骨架

### 4.1 `CLIENT_COMPAT.md` — **已落地**

| | |
|--|--|
| **怎么测** | 阅读 `docs/CLIENT_COMPAT.md`：矩阵列齐全；状态多为「未测/部分」；STRM 302、PlayMethod、容器名约定与实现路径一致。 |
| **期望结果** | 不把「模拟协议单测」写成 Infuse/VidHub「通过」。晨验只需确认文档在位且与代码路径一致，不必实机刷满矩阵。 |

- [ ] 文档存在且路径引用正确

### 4.2 STRM resolver — **已落地（DirectUrl stub + stream 接线）**

| | |
|--|--|
| **验收点** | `fbz-api/src/media/strm.rs`：`StrmResolver` + `DirectUrlStrmResolver`；`streaming` 在鉴权后、302 前调用 `default_strm_resolver()`。 |
| **怎么测** | 1) `cargo test --lib media::strm`。2) 有 `is_strm` 条目时： |

```bash
# 先鉴权再跳转：无 Token 应 401；有 Token + 合法 STRM 目标 → 302 + Location + Cache-Control: no-store
curl -sS -D - -o /dev/null \
  "http://127.0.0.1:8080/emby/Videos/${ITEM_ID}/stream" \
  -H "X-Emby-Token: ${USER_TOKEN}"
```

| **期望结果** | stub 优先 `strm_target`；仅本地 `.strm` 路径且无 target → 422 类错误，**不**代理拉流。公网域名仍受 `STRM_ALLOWED_DOMAINS` 约束（既有策略）。 |

- [ ] 单元测试
- [ ] （有样片时）302 行为

### 4.3 PlayMethod 骨架 — **已落地（纯函数 + PlaybackInfo 接线，见附录 A.2）**

| | |
|--|--|
| **验收点** | `fbz-api/src/playback/play_method.rs`：`decide_play_method` → DirectPlay / Remux(`DirectStream`) / Transcode。**夜间已**驱动 MediaSource `SupportsDirect*`（附录 A.2）。 |
| **怎么测** | `cargo test --lib playback::play_method`；`cargo test --lib -- compat::emby::routes::playback::tests`。 |
| **期望结果** | 无 profile 提示默认 DirectPlay；单测覆盖容器/编码/码率分支；PlaybackInfo 三标志与决策一致。 |

- [ ] 单测通过
- [ ] SupportsDirect* 已接线（附录 A.2）

### 4.4 容器规范化 — **已落地（MediaSource 输出）**

| | |
|--|--|
| **验收点** | `normalize_media_source_container`：`matroska`→`mkv`，`mpegts`→`ts` 等；Items 组 MediaSource 时调用。 |
| **怎么测** | `cargo test --lib media::container`。对已知 mkv 片： |

```bash
curl -sS "http://127.0.0.1:8080/emby/Users/${USER_ID}/Items/${ITEM_ID}?Fields=MediaSources" \
  -H "X-Emby-Token: ${USER_TOKEN}" \
  | jq '.MediaSources[0].Container'
```

| **期望结果** | 对外为 `mkv`/`mp4`/`ts` 等扩展名风格，而非裸 `matroska`（若源为 matroska）。 |

- [ ] 单元测试
- [ ] （有样片时）PlaybackInfo/Items Container 字段

---

## 5. 插件迁出骨架 / StorageProvider / 扫描事件

### 5.1 官方元数据插件迁出骨架 — **已落地（stub）**

| | |
|--|--|
| **验收点** | `fbz-api/plugins/README.md` + `tmdb|tvdb|fanart|imdb|douban-provider`、`marker-detect-stub`；各含 `manifest.json` / `server.mjs` / README；core 内置客户端带冻结加深注释。计划见 `fbz-api/docs/plans/official-metadata-providers-migration.md`。 |
| **怎么测** | 目视目录表与 README 密钥策略；可选对 stub `node server.mjs`（应 query 空贡献、不破坏 merge）。**不**要求删除 core TMDB 客户端。 |
| **期望结果** | 官方包目录可随发行识别；密钥不进 manifest；过渡期 core 仍可刮削。 |

- [ ] plugins/ 官方包目录齐全
- [ ] 冻结策略文档可读

### 5.2 StorageProvider — **已落地（扩展点 + 光鸭）；第二源产品化后续**

| | |
|--|--|
| **现状** | `fbz-api/src/storage/provider.rs` 已 `pub mod provider`；账号创建走 registry（默认光鸭 id）；文档 `docs/storage-provider-extension.md` 在位。光鸭仍可经 env URL 拉起。**第二网盘源产品化** 见附录 C（非明早必验）。 |
| **怎么测（晨验）** | 1) `grep -n 'pub mod provider' fbz-api/src/storage/mod.rs` 有匹配。2) （可选）`GET` 存储相关 admin/配置面可见已注册 provider ids。3) **不**要求装上第二商业网盘。 |
| **期望结果** | 扩展点可注册；光鸭包身份 + registry 存在即可；勿把「第二源未产品化」当 P0 阻断。 |

- [ ] `pub mod provider` 已挂
- [ ] 光鸭 / registry 可识别
- [ ] （后续）第二源产品化 — 见附录 C

### 5.3 扫描事件（插件向）— **部分 / 后续（非明早必验）**

| | |
|--|--|
| **现状** | 既有 `events/` outbox（如 `plugin.hook.dispatch`）与配置 `SCAN_EVENT_RETENTION_DAYS`；**本轮未见**面向 Storage/库扫描的插件扫描事件骨架（Discover/文件变更等）作为可验收模块。光鸭 README 仍写「不走事件 hook」。 |
| **怎么测** | 搜索 `ScanEvent` / `library.scanned` 类扩展点 —— 无则勾选「待合入」。 |
| **期望结果（合入后）** | 扫描生命周期可对插件广播；与鉴权网关无关。 |

- [ ] **待合入**：插件扫描事件扩展点

---

## 6. 冻结项（不验收「做成了」，只验收「没误做」）

| 冻结 | 晨验怎么确认 | 期望 |
|------|--------------|------|
| **后台 UI 视觉不落地** | 抽查 admin 路由/页面：无本轮视觉大改 PR；metadata sources **仅 API**，无新后台配置页。架构文案：`docs/FBZ-目标架构与落地路线.md` §11。 | 不出现「源链配置 UI 已交付」类宣称 |
| **鉴权 / 网关大改需多轮审** | `git diff` 抽查：无 Emby token 校验重写、无 admin `authenticate_admin` 策略大改、无把 ApiKey 放行进 admin metadata sources。STRM/播放只挂 resolver，先鉴权后 302。 | 若有鉴权文件大 diff → **阻断**，记入缺口并要求评审 |

- [ ] 无后台源链 UI
- [ ] 无未评审鉴权/网关大改

---

## 7. 建议自动化命令清单（可并行）

```bash
cd fbz-api
cargo test --lib app::tests
cargo test --lib plugins::manifest::tests::first_party_guangya_storage_manifest_is_valid
cargo test --lib library::metadata_sources
cargo test --lib media::strm
cargo test --lib media::container
cargo test --lib playback::play_method

cd ../fbz-fe
pnpm test -- src/stores/playback.test.ts src/service/modules/server.test.ts
```

（前端用例名以仓库实际为准；失败时贴输出到缺口。）

---

## 8. 已知缺口（盘点时 → 夜间订正）

> **订正：** 下列多项已在 **附录 A** 落地；晨验勿再按「缺口」判失败。大项「未照搬完」见 **附录 C（非明早必验 / 后续）**。

1. **本地 NFO → Match**：已为 `local`（A.1）。
2. **PlayMethod / DeviceProfile → PlaybackInfo SupportsDirect\***：已接线（A.2）。
3. **StorageProvider**：`pub mod provider` 已挂；第二源产品化 → **附录 C**。
4. **扫描阶段插件事件晋升**：仍 draft / 未晋升 allow-list → **附录 C**。
5. **官方元数据包**：多为 stub；core 客户端冻结加深（预期）→ 加深属后续。
6. **CLIENT_COMPAT 实机**：Infuse / VidHub / SenPlayer 「未测」→ **附录 C（非明早必验）**。
7. **STRM stub**：不读 `.strm` 文件体；无 target 需导入写 `strm_target`（行为约定，非阻断）。
8. **storage-provider-extension.md**：已在 `docs/`（相对盘点快照已补）。
9. **后台源链 UI**：刻意不做（冻结）。
10. **未 commit**：工作区未推远程；晨验前勿假设已合入。
11. **今晚已标绿（勿当缺口）**：VF Add/Remove Paths、Monitor 轮询、Webhooks 最小、Trailers/Theme、Sessions NowPlaying、外挂 C 挂轨、Parental 最小、Playlists 编辑、Collections — 见附录 A.5–A.14。

---

## 9. 验收记录（晨验填写）

| 项 | 结果 (pass/fail/skip) | 执行人 | 备注/证据 |
|----|----------------------|--------|-----------|
| P0 app 测试外迁 | | | |
| P0 光鸭 plugin 包 | | | |
| P0 metadata sources API | | | |
| P0 非产品 README | | | |
| C 端右键/搜索筛选/封面/分页/播放器 | | | |
| Items Genres/Rating/SortBy | | | |
| CLIENT_COMPAT / STRM / PlayMethod / 容器 | | | |
| 插件迁出骨架 | | | |
| StorageProvider / 扫描事件 | | | |
| 冻结项未误做 | | | |
| 附录 A NFO Match / PlayMethod·DeviceProfile / Chapters+Skip | | | |
| 附录 A marker 装包修复 | | | |
| 附录 A Playlists C 编辑 / Collections C | | | |
| 附录 A Parental 最小 / Activity Log | | | |
| 附录 A DeliveryUrl+C 挂轨 / Filters2 / Suggestions / DELETE | | | |
| 附录 A Sessions NowPlaying | | | |
| 附录 A VF Add/Remove Paths | | | |
| 附录 A Library Monitor 轮询 | | | |
| 附录 A Webhooks 最小 / ScheduledTasks | | | |
| 附录 A Theme / LocalTrailers / Trailers(ParentId) | | | |
| 附录 C 大项（仅确认未误当 P0 阻断） | | | |

**结论：** □ 可继续合入 / □ 有阻断缺口（列出编号） / □ 仅文档与骨架，功能待合入后再验

> 附录 C 所列 LiveTV / SyncPlay / 硬件转码全量 / JF 插件 ABI / Books·Comics 高级 / 家长文件夹 ACL 全量 / Infuse 实机等：**失败不构成明早阻断**（非必验）。


---

## 附录 A · JF 照搬 P0 夜间合入（2026-10-15 PT）

> 对照 `docs/JELLYFIN_PARITY_MATRIX.md`。本附录覆盖当晚落地的 NFO Match、PlayMethod→PlaybackInfo、marker 装包修复、PlaybackInfo Chapters / C 端 Skip、Playlists C、Collections C、以及 **外挂字幕 DeliveryUrl / DELETE Items / Filters2 / Suggestions**。样式不大改；**未 commit**；**未改鉴权网关**。

### A.1 本地 NFO → Match — **已落地（本地）**

| | |
|--|--|
| **验收点** | `fbz-api/src/metadata/nfo_match.rs`；`library/metadata_sources.rs` 常量 `NFO_MATCH_INTEGRATION = "local"`；Match 链可读本地 sidecar `.nfo`。云盘 URI 仍跳过、走 `storage/nfo` importer。 |
| **怎么测** | `cd fbz-api && cargo test --lib -- metadata::nfo_match match_chain_nfo nfo_match_integration`。API：`GET …/metadata/sources` 的 `nfoMatchIntegration` 应为 **`"local"`**（覆盖正文 §1.3 盘点时的 `"pending"` 表述）。 |
| **期望结果** | 单测通过；配置面回显 `local`；本地库 Match 可消费 NFO；云盘不因此误读本地路径。 |

- [ ] NFO Match 单测
- [ ] `nfoMatchIntegration` = `local`

### A.2 PlayMethod / DeviceProfile → PlaybackInfo — **已落地**

| | |
|--|--|
| **验收点** | `decide_play_method` + `device_profile_summary_from_playback_hints`（body / query / `ClientCapabilities.DeviceProfile`）驱动 MediaSource `SupportsDirectPlay` / `SupportsDirectStream` / `SupportsTranscoding`；Transcode 决策可入队。 |
| **怎么测** | `cargo test --lib playback::play_method`；`cargo test --lib -- compat::emby::routes::playback::tests`（含 `device_profile_decision_*`）。有样片时抽查三标志与决策一致。 |
| **期望结果** | 无 DeviceProfile 时默认 DirectPlay 向；有 profile 时 Direct/Remux/Transcode 标志与决策一致；单测通过。 |

- [ ] play_method / playback 单测
- [ ] （有样片）SupportsDirect* 与决策一致

### A.3 PlaybackInfo Chapters + C 端 Skip Intro/Credits — **已落地（核对）**

| | |
|--|--|
| **验收点** | PlaybackInfo MediaSource.Chapters ← `media_markers`（与 Items 同源）；C 端 `preparePlayback` 映射 chapters；`FullscreenPlayer` 显示 Skip Intro/Credits。 |
| **怎么测** | playback 单测含 intro chapters；浏览器有 IntroStart/CreditsStart marker 的条目起播，片头/片尾窗口出现跳过按钮。 |
| **期望结果** | 有 marker 可跳；无 marker 不误显。 |

- [ ] Chapters 接线存在
- [ ] （有样片）Skip Intro/Credits

### A.4 marker 装包修复 — **已落地（见 PLUGIN_OPS）**

| | |
|--|--|
| **验收点** | 迁移 `0103_plugin_permission_media_analyze.sql`：allowlist 含 `media.analyze`（及 `storage.*` 对齐）。`list_plugin_packages_page` 对 `active` 使用 `coalesce(..., false)`。HTTP 插件空/非法 JSON → **400**（非 500）。详录 `docs/PLUGIN_OPS_PERF_BLAST_2026-10-15.md` §5/§7。 |
| **怎么测** | 1) 确认迁移已应用。2) 装 `fbz.marker.detect` 包 → 201 → approve/activate/enable 200。3) `GET /api/admin/plugins/packages` → 200，marker `active=true`。4) `POST …/marker-detect` → 202；job `detector_version=marker-detect-0.2.0`。 |
| **期望结果** | 装包不再因 `media.analyze` 缺键 500；packages 列表不 500；detect 走 HTTP 插件而非仅 core 启发式。 |

- [ ] 迁移 0103 已应用
- [ ] marker 装包/启用
- [ ] packages 列表 200
- [ ] detect 入队且 detector_version 为插件

### A.5 Playlists C 端（列表 / 打开 / 播放 / 编辑）— **已落地**

| | |
|--|--|
| **验收点** | 后端：`GET/POST /emby/Playlists`、`POST /emby/Playlists/{id}?Name=` 改名、`GET/POST/DELETE …/Items`（`playlists.rs` + `rename_playlist`）。C 端：`/playlists` 新建；`/playlists/:id` 改名/添加条目/行内移除；播放；媒体卡片右键「加入播放列表」。导航：顶栏/抽屉。 |
| **怎么测** | 1) Web：登录 →「播放列表」→「新建」→ 改名 →「添加条目」贴 ItemId → 行内「移除」。2) 库页右键电影 →「加入播放列表」。3) curl 见下。 |
| **期望结果** | 创建/改名/加删可闭环；可播放 Movie/Episode；不改鉴权；样式沿用现有 token。拖拽排序仍为 P1。 |

```bash
# 列表
curl -sS "http://127.0.0.1:8080/emby/Playlists?UserId=${USER_ID}&Limit=20"   -H "X-Emby-Token: ${USER_TOKEN}" | jq '{TotalRecordCount, names:[.Items[].Name]}'

# 创建
curl -sS -X POST "http://127.0.0.1:8080/emby/Playlists?Name=AccEdit"   -H "X-Emby-Token: ${USER_TOKEN}" | jq .

# 改名
curl -sS -X POST "http://127.0.0.1:8080/emby/Playlists/${PLAYLIST_ID}?Name=AccRenamed"   -H "X-Emby-Token: ${USER_TOKEN}" | jq .

# 成员
curl -sS "http://127.0.0.1:8080/emby/Playlists/${PLAYLIST_ID}/Items?UserId=${USER_ID}&EnableImages=true"   -H "X-Emby-Token: ${USER_TOKEN}" | jq '{TotalRecordCount, items:[.Items[]|{Name,Type,Id,PlaylistItemId}]}'

# 加条目
curl -sS -X POST "http://127.0.0.1:8080/emby/Playlists/${PLAYLIST_ID}/Items?Ids=${ITEM_ID}&UserId=${USER_ID}"   -H "X-Emby-Token: ${USER_TOKEN}" | jq .

# 删条目（EntryIds = PlaylistItemId）
curl -sS -X DELETE "http://127.0.0.1:8080/emby/Playlists/${PLAYLIST_ID}/Items?EntryIds=${ENTRY_ID}"   -H "X-Emby-Token: ${USER_TOKEN}" -w "%{http_code}\n"
```

- [ ] API 列表/成员/创建/改名/加删
- [ ] C 端列表页 + 新建
- [ ] C 端打开 + 播放 + 改名/加删
- [ ] 右键加入播放列表

### A.5b Collections / BoxSet C 端浏览 + 加入 — **已落地**

| | |
|--|--|
| **验收点** | 后端既有 `GET /Collections/{id}`、`…/Items`、`POST /Collections`、`POST …/Items?Ids=`（写限管理员），以及 `Users/{userId}/Items?IncludeItemTypes=BoxSet`。C 端：`/collections` → `/collection/:id` 打开成员并可播放；右键「加入合集」；顶栏/抽屉「合集」。 |
| **怎么测** | 1) BoxSet 列表 curl 见下。2) Web：合集详情 Hero「播放」/成员卡片点开或起播。3) 管理员右键电影 →「加入合集」。4) 库右键已有「扫描媒体库」「刷新元数据」；条目右键已有「刷新元数据」。 |
| **期望结果** | 详情不因成员可加载而整页退回列表；子项可进详情/播放；加入合集可闭环（管理员）；不改鉴权与大样式。 |

```bash
curl -sS "http://127.0.0.1:8080/emby/Users/${USER_ID}/Items?IncludeItemTypes=BoxSet&Recursive=true&EnableImages=true&Limit=20" \
  -H "X-Emby-Token: ${USER_TOKEN}" | jq '{TotalRecordCount, items:[.Items[]|{Id,Name,Type,ChildCount}]}'
```

- [ ] API BoxSet 列表 / 成员 / 创建加删
- [ ] C 端 `/collections` 浏览
- [ ] 进详情打开子项并播放
- [ ] 右键加入合集（管理员）


### A.9 外挂字幕 DeliveryUrl / DELETE Items / Filters2 / Suggestions — **已落地**

| | |
|--|--|
| **验收点** | 1) PlaybackInfo MediaStreams：外挂字幕 `IsExternal=true` → `DeliveryMethod=External` + `DeliveryUrl`（`/Videos/{item}/{ms}/Subtitles/{idx}/0/Stream.{fmt}?api_key=`）。2) `DELETE /emby/Items/{id}` 与 `POST /emby/Items/{id}/Delete`（可选 `DeleteFiles`）管理员软删。3) `GET /emby/Items/Filters2` 返回 Genres/Studios（NameLongIdPair）+ Tags/OfficialRatings/Years。4) `GET /emby/Users/{userId}/Suggestions` 与 `GET /emby/Items/Suggestions?UserId=`。5) Items 长尾：`ExcludeItemTypes`、`MinCommunityRating`、`MaxCommunityRating`。 |
| **怎么测** | `cargo test --lib -- compat::emby::routes::playback::tests external_subtitle`；`cargo test --lib -- exclude_item_types_and_community_rating`；路由注册/单测含 Filters2、Suggestions、delete_item_query。有样片时： |

```bash
# Filters2
curl -sS "http://127.0.0.1:8080/emby/Items/Filters2?UserId=${USER_ID}&ParentId=${LIBRARY_ID}" \
  -H "X-Emby-Token: ${USER_TOKEN}" | jq '{genres:(.Genres|length),studios:(.Studios|length),years:(.Years|length)}'

# Suggestions
curl -sS "http://127.0.0.1:8080/emby/Users/${USER_ID}/Suggestions?Limit=5" \
  -H "X-Emby-Token: ${USER_TOKEN}" | jq '{TotalRecordCount,n:(.Items|length)}'

# Exclude + rating range
curl -sS "http://127.0.0.1:8080/emby/Users/${USER_ID}/Items?ParentId=${LIBRARY_ID}&Recursive=true&ExcludeItemTypes=Folder,BoxSet&MinCommunityRating=6&MaxCommunityRating=10&Limit=5" \
  -H "X-Emby-Token: ${USER_TOKEN}" | jq '{TotalRecordCount, items:[.Items[]|{Name,Type,CommunityRating}]}'

# DELETE（慎用；建议仅测 DeleteInfo）
curl -sS "http://127.0.0.1:8080/emby/Items/${ITEM_ID}/DeleteInfo" \
  -H "X-Emby-Token: ${ADMIN_TOKEN}" | jq .
```

| **期望结果** | 外挂轨有 DeliveryUrl；Filters2/Suggestions 200；Exclude/评分筛选生效；DeleteInfo 可读；**未改鉴权**。 |
| **关联** | 外挂入库仍靠 probe sidecar；远程 OpenSubtitles 真源仍 P1。NFO Match / Playlists C / marker 装包见 A.1/A.4/A.5。 |

- [ ] DeliveryUrl 单测 /（有样片）PlaybackInfo
- [ ] Filters2 / Suggestions
- [ ] ExcludeItemTypes + Min/MaxCommunityRating
- [ ] DeleteInfo /（可选）软删


### A.5c Parental 控制 + Activity Log（最小，2026-10-15 续）

| | |
|--|--|
| **验收点** | 迁移 `0105_user_parental_controls.sql`：`users.max_parental_rating` / `block_unrated_items`。`POST …/Users/{id}/Policy` 可写 `MaxParentalRating` / `BlockUnratedItems`；User DTO Policy 回显。Items 主列表按官方分级等级 ≤ Max、未分级类型 ∈ Block 过滤（`library/parental.rs` + `list_user_items` `$80/$81`）。Activity Log：`GET /emby/System/ActivityLog/Entries`（及无前缀）已聚合 sessions 登录 / playback_sessions 播放 / jobs 扫描收尾 — **本轮核对已有，未改**。 |
| **怎么测** | 1) 迁移已应用。2) 管理员 `POST /emby/Users/{id}/Policy` body `{"MaxParentalRating":8,"BlockUnratedItems":["Movie"]}` → 再 `GET /emby/Users/{id}` Policy 回显。3) 该用户 token 拉 Items：高于 PG-13 的条目不出现；无分级且类型 movie 的不出现。4) 管理员 `GET /emby/System/ActivityLog/Entries?Limit=20` → 可见登录/播放/任务类条目（有数据时）。 |
| **期望结果** | Policy 读写闭环；Items 过滤生效；Activity Log 200 且形状 PascalCase；**未改鉴权网关；未 commit**。 |

```bash
curl -sS -X POST "http://127.0.0.1:8080/emby/Users/${USER_ID}/Policy" \
  -H "X-Emby-Token: ${ADMIN_TOKEN}" -H "Content-Type: application/json" \
  -d '{"MaxParentalRating":8,"BlockUnratedItems":["Movie"]}'

curl -sS "http://127.0.0.1:8080/emby/Users/${USER_ID}" \
  -H "X-Emby-Token: ${ADMIN_TOKEN}" | jq '.Policy|{MaxParentalRating,BlockUnratedItems}'

curl -sS "http://127.0.0.1:8080/emby/Users/${USER_ID}/Items?Recursive=true&IncludeItemTypes=Movie&Limit=50" \
  -H "X-Emby-Token: ${USER_TOKEN}" | jq '{TotalRecordCount, ratings:[.Items[]|.OfficialRating]}'

curl -sS "http://127.0.0.1:8080/emby/System/ActivityLog/Entries?StartIndex=0&Limit=20" \
  -H "X-Emby-Token: ${ADMIN_TOKEN}" | jq '{TotalRecordCount, types:[.Items[]|.Type]}'
```

- [ ] 迁移 0105
- [ ] Policy MaxParentalRating / BlockUnratedItems 读写
- [ ] Items 过滤
- [ ] Activity Log GET 晨验



### A.10 C 端外挂字幕挂轨 + Sessions/NowPlaying 最小字段 — **已落地**

| | |
|--|--|
| **验收点** | 1) `preparePlayback` 从 PlaybackInfo `MediaStreams` 取 `IsExternal`+`DeliveryUrl` → `subtitleTracks`；`FullscreenPlayer` 字幕菜单含外挂项，选中后 `fetch(DeliveryUrl)` + `attachSrt`/`<track>` 挂轨（与内嵌 demux 互斥）。2) `GET /emby/Sessions`：`LastActivityDate`、`RemoteEndPoint`、`ServerId`；有活跃 `playback_sessions`（90s 内）时带 `NowPlayingItem`+`PlayState`。C 端管理大 UI **不做**。 |
| **怎么测** | FE：`vp test` 含 `maps external subtitle DeliveryUrl`；有样片时播外挂轨。API：`cargo test --lib -- session_dto_includes_now_playing`；`GET /emby/Sessions` 看字段。 |
| **期望结果** | 外挂可选可显；Sessions 列表字段可用；**未改鉴权；未 commit**。 |

- [ ] C 端外挂字幕挂轨
- [ ] Sessions LastActivityDate / NowPlayingItem / PlayState


### A.11 VirtualFolders Add/Remove Paths — **已落地**

| | |
|--|--|
| **验收点** | `POST /emby/Library/VirtualFolders/Paths`（AddMediaPath）；JF `DELETE /Library/VirtualFolders/Paths` + Emby `POST …/Paths/Delete`（RemoveMediaPath）；复用 admin 路径校验/增删；可选 `refreshLibrary`→扫描；管服权限；**未改鉴权网关**。 |
| **怎么测** | `cargo test --lib -- compat::emby::routes::media_folders::tests emby_virtual_folder_paths_write_aliases_exist emby_virtual_folder_paths_remove_aliases_exist`；有环境时用管理员 token 对测试库增删一路径（慎用生产路径）。 |
| **期望结果** | 路由注册；单测通过；增删后 VirtualFolders 列表路径变化；可选触发 scan。 |

- [ ] AddMediaPath
- [ ] RemoveMediaPath（JF DELETE + Emby Paths/Delete）
- [ ] （可选）refreshLibrary

### A.12 Library Monitor（轮询最小）— **已落地**

| | |
|--|--|
| **验收点** | 迁移 `0104_library_realtime_monitor.sql`；`libraries.enable_realtime_monitor`；`scan/monitor.rs` 轮询 fingerprint + debounce → `library.scan`；`main` `spawn_library_monitor`。notify / 局部 Discover **后续**（附录 C）。 |
| **怎么测** | 1) 迁移已应用。2) 打开某库 EnableRealtimeMonitor。3) 在监控根下增删文件，debounce 后出现 `library.scan`（reason `realtime_monitor`）。4) 单测模块 `scan::monitor`（若有）。 |
| **期望结果** | 开关库可因轮询入队扫描；关闭则不入队；**不要求** fsnotify。 |

- [ ] 迁移 0104
- [ ] 开关 + 轮询入队（有样例路径时）

### A.13 Webhooks 最小事件 + ScheduledTasks 核对 — **已落地**

| | |
|--|--|
| **验收点** | admin `notification-targets`（webhook + `channel`）；播放 `playback.started/stopped`、扫描 `library.scan.*` → `enqueue_event_notification` → worker POST JSON。见 `docs/WEBHOOKS_MINIMAL.md`。ScheduledTasks：`POST|DELETE /ScheduledTasks/Running/{id}`（及 Emby `/Delete`）已有 — **核对**。 |
| **怎么测** | `cargo test --lib -- notifications::system playback_hooks_enqueue_webhook scan_hooks_enqueue_webhook emby_scheduled_task_aliases_exist`；可选登记 webhook 目标后起停播放看投递。 |
| **期望结果** | 事件入队；channel 匹配才投递；Trigger/Cancel 路由存在；**未改鉴权**。 |

- [ ] Webhook 事件单测 /（可选）实投递
- [ ] ScheduledTasks Trigger/Cancel 路由

### A.14 ThemeMedia / LocalTrailers / Trailers(ParentId) — **已落地**

| | |
|--|--|
| **验收点** | `media/extras.rs`；`ThemeMedia` / `ThemeSongs` / `ThemeVideos`；`LocalTrailers`；`GET /Trailers?ParentId=` 挂本地 Extra Trailer + `media_videos` trailer/teaser。无 ParentId 的全局 Trailers 保持空形状兼容。 |
| **怎么测** | 有 extras 样片时：`GET /emby/Items/{id}/ThemeMedia`、`…/LocalTrailers`、`GET /emby/Trailers?ParentId={id}&UserId=`。 |
| **期望结果** | 有本地 trailer/theme 时非空；无则空列表；不 500。 |

- [ ] ThemeMedia / LocalTrailers
- [ ] Trailers(ParentId)

### A.11 DisplayPreferences / UserConfiguration 可存回显 — **已落地**

| | |
|--|--|
| **验收点** | 1) `GET\|POST /emby/Users/{id}/Configuration`：字幕语言、OrderedViews 等可存；GET 与 `GET /Users/{id}` 的 `Configuration` 回显一致。2) `GET\|POST /DisplayPreferences/{id}`：除 SortBy/SortOrder/CustomPrefs 外，ViewType/ScrollDirection/ShowBackdrop 等经 CustomPrefs 保留键回显；`homesectionN` 影响 `HomeSections` 顺序。3) C 端：登录/恢复会话拉 Configuration；个人资料保存写回；播放器按偏好选字幕轨。 |
| **怎么测** | `cargo test --lib -- display_preferences user_configuration_from_value order_home_sections emby_display_preferences_aliases`；curl： |

```bash
curl -sS -X POST "http://127.0.0.1:8080/emby/Users/${USER_ID}/Configuration" \
  -H "X-Emby-Token: ${USER_TOKEN}" -H "Content-Type: application/json" \
  -d '{"SubtitleLanguagePreference":"zh-CN","AudioLanguagePreference":"zh","OrderedViews":["resume","latestmedia"]}'

curl -sS "http://127.0.0.1:8080/emby/Users/${USER_ID}/Configuration" \
  -H "X-Emby-Token: ${USER_TOKEN}" | jq '{SubtitleLanguagePreference,AudioLanguagePreference,OrderedViews}'

curl -sS "http://127.0.0.1:8080/emby/Users/${USER_ID}" \
  -H "X-Emby-Token: ${USER_TOKEN}" | jq '.Configuration|{SubtitleLanguagePreference,OrderedViews}'

curl -sS -X POST "http://127.0.0.1:8080/emby/DisplayPreferences/usersettings?userId=${USER_ID}&client=emby" \
  -H "X-Emby-Token: ${USER_TOKEN}" -H "Content-Type: application/json" \
  -d '{"Id":"usersettings","Client":"emby","SortBy":"SortName","SortOrder":"Ascending","ScrollDirection":"Vertical","CustomPrefs":{"homesection0":"resume","homesection1":"favorites"}}'

curl -sS "http://127.0.0.1:8080/emby/DisplayPreferences/usersettings?userId=${USER_ID}&client=emby" \
  -H "X-Emby-Token: ${USER_TOKEN}" | jq '{ScrollDirection,CustomPrefs}'

curl -sS "http://127.0.0.1:8080/emby/Users/${USER_ID}/HomeSections" \
  -H "X-Emby-Token: ${USER_TOKEN}" | jq '[.[].Id]'
```

| **期望结果** | Configuration GET/UserDto 回显；DisplayPreferences 一等字段与 homesection 顺序生效；**未改鉴权网关；未 commit**。 |

- [ ] Configuration GET/POST 回显
- [ ] UserDto.Configuration 回显
- [ ] DisplayPreferences meta + HomeSections 顺序
- [ ] C 端读/写偏好 + 播放器选轨

### A.6 冻结与未做（本附录）

- 无后台 UI 视觉大改；无鉴权/网关重写。
- Playlists 拖拽排序 / 多选编辑深化：后续（非明早阻断）。
- Library Monitor **轮询最小已落地**（A.12）；notify / 局部 Discover 仍后续。
- CLIENT_COMPAT **Infuse 等实机**：附录 C（非明早必验）。
- Live TV 真实现 / SyncPlay **完整**（仅有 NotImplemented+空 List 骨架）/ 硬件转码全量 / JF 插件 ABI / Books·Comics（库类型不支持）/ 家长文件夹 ACL 全量：附录 C。
- **未 commit**。

### A.7 建议追加自动化

```bash
cd fbz-api
cargo test --lib -- metadata::nfo_match playback::play_method match_chain_nfo nfo_match_integration
cargo test --lib -- compat::emby::routes::playback::tests
cargo test --lib -- exclude_item_types_and_community_rating
cargo test --lib -- official_filters2_routes_are_registered
cargo test --lib -- compat::emby::routes::playlists
cargo test --lib -- compat::emby::routes::media_folders::tests emby_virtual_folder_paths_write_aliases_exist emby_virtual_folder_paths_remove_aliases_exist
cargo test --lib -- notifications::system playback_hooks_enqueue_webhook scan_hooks_enqueue_webhook emby_scheduled_task_aliases_exist
cargo test --lib -- session_dto_includes_now_playing
```

### A.8 变更落点（未 commit）

| 域 | 路径 |
|----|------|
| NFO Match | `fbz-api/src/metadata/nfo_match.rs` 等（见矩阵「今晚进度」） |
| PlayMethod | `fbz-api/src/compat/emby/routes/playback.rs`、`playback/play_method.rs` |
| marker 装包 | `migrations/0103_plugin_permission_media_analyze.sql`；packages coalesce；plugin-http 400 |
| Playlists C | `fbz-fe/src/service/modules/playlists.ts`；`views/playlists/**`；`ContextMenu` 加入列表；`router`；`AppHeader`/`AppDrawer` |
| Playlists API | `playlists.rs` + `repository::rename_playlist`；`POST /Playlists/{id}`；既有 SQL `total_record_count` 修 |
| Collections C | `service/modules/collections.ts`；`views/collections`；导航「合集」 |
| 外挂字幕 DeliveryUrl | `compat/emby/routes/playback.rs` `external_subtitle_delivery_url` |
| C 端外挂挂轨 | `server.ts` `preparePlayback`；`FullscreenPlayer.vue`；`webplayer.ts` `attachSrt` |
| Sessions 最小字段 | `SessionInfoDto`/`sessions.rs`/`auth/repository.rs` LastActivityDate+NowPlaying |
| DELETE Items | `items.rs` `delete_item`；`media/repository.rs` `soft_delete_item` |
| Filters2 / Suggestions | `classifications.rs` Filters2；`items.rs` Suggestions |
| Items 长尾筛选 | `ItemsQuery` + `library/repository.rs` browse `$76–$79` |
| Parental | 迁移 `0105`；`users`/`UpdateUserPolicy`；`library/parental.rs`；Items `$80/$81` |
| Activity Log | 既有 `activity_log.rs` + admin 聚合（本轮核对） |
| DisplayPreferences / UserConfiguration | `display_preferences.rs` meta；`users.rs` GET+UserDto 回显；`content.rs` HomeSections 序；C 端 `userData`/`auth`/`FullscreenPlayer` |
| DeviceProfile | `playback/play_method.rs`；`playback.rs` hints 接线 |
| VF Paths | `media_folders.rs` Add/Remove；`admin/repository` path helpers |
| Library Monitor | `scan/monitor.rs`；迁移 `0104`；`main.rs` `spawn_library_monitor` |
| Webhooks | `notifications/system.rs`；playback/scan hooks；`WEBHOOKS_MINIMAL.md` |
| Theme / Trailers | `theme_media.rs`；`media/extras.rs`；`items::trailers` |
| 文档 | 本附录；`JELLYFIN_PARITY_MATRIX.md`；`PLUGIN_OPS_PERF_BLAST_2026-10-15.md` |

---

## 附录 B · 正文缺口订正（相对 §8）

盘点正文 §8 下列项已被附录 A 覆盖，晨验以附录为准：

1. ~~本地 NFO → Match~~ → **A.1**（`local`）
2. ~~PlayMethod / DeviceProfile → PlaybackInfo~~ → **A.2**
3. marker 装包 / packages 500 / 非法 JSON 500 → **A.4**
4. Playlists C 端编辑 → **A.5**
5. Collections C 端 → **A.5b**
6. Parental / Activity Log 最小 → **A.5c**
7. 外挂字幕 DeliveryUrl / DELETE / Filters2 / Suggestions / 长尾 → **A.9**
8. C 端外挂挂轨 + Sessions NowPlaying → **A.10**
9. VirtualFolders Add/Remove Paths → **A.11**
10. Library Monitor 轮询最小 → **A.12**
11. Webhooks 最小 + ScheduledTasks 核对 → **A.13**
12. Theme / LocalTrailers / Trailers(ParentId) → **A.14**
13. StorageProvider `mod` 已挂 → **§5.2**（第二源仍附录 C）

§1.3 / §4.3 以本附录期望覆盖盘点时的 `pending` /「未接」表述。

---

## 附录 C · 非明早必验 / 后续（诚实未照搬完）

> 下列为大项缺口或仅骨架。**晨验不得因其未完成而判 P0 失败**；空响应/文档在位即可。对照 `JELLYFIN_PARITY_MATRIX.md`「非明早必验 / 后续」。

| 大项 | 现状 | 晨验态度 |
|------|------|----------|
| **Live TV / IPTV / 录制** | `live_tv.rs` 空兼容桩（空列表 / Info disabled / 单条 404 / 写 409；**不** 500） | **非明早必验**；可选抽查不 500；**真 tuner 非目标、不实现** |
| **SyncPlay** | **可用子集（2026-10-15 ~16:15 PT）**：进程内组；New/Join/Leave/List/Get/SetNewQueue/Pause/Unpause/Seek/Stop/Ping；WS 广播 `SyncPlayGroupUpdate`/`SyncPlayCommand`；FE service+归约+播放器「同步观看」面板（右下控制栏人形按钮）。**未做**：全员缓冲等待、Shuffle/Repeat、跨节点、跨集队列 | 可选按 C.1 验两会话组同步；**非 P0** |
| **Quick Connect** | **未做**：属登录/鉴权流，受 §6「鉴权/网关大改需多轮审」冻结 | 非明早必验 |
| **硬件转码全量** | `HardwareMode`/planner 编码器名有；多档 ladder / 实机探测未完 | 软件转码可入队即可；**全量后续** |
| **JF 插件 ABI** | HTTP/manifest/装包审批最小；非官方 JF 插件 ABI / SDK / 市场 | 光鸭+官方 stub 包身份即可；**ABI 后续** |
| **Books / Comics 高级** | **库类型不支持**：`LibraryType` 无 `books`（`parse("books")=None`，admin 拒建）；`ItemType` 无 Book；`IncludeItemTypes=Book` 为未知类型 → 过滤空 → 浏览空列表。仅有 `RemoteSearch/Book` 边角。**无**图书库/阅读器 | **不验**浏览/阅读器；文档在位即可 |
| **家长文件夹 ACL 全量** | 评级 + BlockUnratedItems 已有；EnabledFolders / BlockedTags 等未全量生效 | 验 A.5c 最小即可；**文件夹 ACL 后续** |
| **Infuse / VidHub / SenPlayer 实机** | `CLIENT_COMPAT.md` 多为「未测」 | 文档在位即可；**实机刷绿后续** |
| Monitor notify / 局部 Discover | 轮询最小已有（A.12） | **加深后续** |
| NFO 写回 / 远程字幕真源 | 缺失/部分 | **后续** |
| Trickplay | **可用（2026-10-15 ~16:20 PT）**：JF `tiles.m3u8`/`{n}.jpg` + 管理员 `POST /Videos/{id}/Trickplay/Generate` 后台 ffmpeg；C 端进度条预览。自动生成/item DTO 字段未做 | 可选 C.2 |
| Playlists 拖拽排序 UI | 后端 Move 有；C 端拖拽未做 | **后续** |
| 第二网盘源产品化 | StorageProvider 扩展点已有 | **后续** |
| 扫描阶段 hook 晋升 allow-list | `lifecycle.rs` draft | **后续** |
| WAN/DDNS/证书向导、音乐/照片深化 | 部分 | **后续** |


### C.0 Live TV 空桩自检（非目标 · **非明早必验** · 可选）

> 真 tuner / IPTV / 录制流水线 **不做**。晨验勿因未实现 Live TV 判失败。可选确认探测路由不 500：

```bash
# Info → IsEnabled=false
curl -sS "http://127.0.0.1:8080/emby/LiveTv/Info" -H "X-Emby-Token: ${USER_TOKEN}" | jq '{IsEnabled,EnabledUsers}'
# Channels → 空列表
curl -sS "http://127.0.0.1:8080/emby/LiveTv/Channels?Limit=10" -H "X-Emby-Token: ${USER_TOKEN}" | jq '{TotalRecordCount,n:(.Items|length)}'
# 单条 → 404；mutation → 409
curl -sS -o /dev/null -w '%{http_code}\n' "http://127.0.0.1:8080/emby/LiveTv/Channels/nope" -H "X-Emby-Token: ${USER_TOKEN}"
curl -sS -o /dev/null -w '%{http_code}\n' -X POST "http://127.0.0.1:8080/emby/LiveTv/Timers"   -H "X-Emby-Token: ${USER_TOKEN}" -H 'Content-Type: application/json' -d '{}'
```

单测：`cargo test --lib -- compat::emby::routes::live_tv::tests emby_live_tv`

### C.1 SyncPlay 自检（可用子集 · 可选）

需两个设备会话 token（`TOKEN_A`/`TOKEN_B`，各自 AuthenticateByName，不同 DeviceId），可选各开 `/embywebsocket?api_key=…` 观察推送。

```bash
H='-H Content-Type:application/json'
# A 建组 → 返回 GroupInfoDto（GroupId/GroupName/State=Idle/Participants）
GID=$(curl -sS -X POST "http://127.0.0.1:8080/emby/SyncPlay/New" -H "X-Emby-Token: ${TOKEN_A}" $H -d '{"GroupName":"night"}' | jq -r .GroupId)
# B 加入 → 204；List 可见 2 人
curl -sS -o /dev/null -w '%{http_code}\n' -X POST "http://127.0.0.1:8080/emby/SyncPlay/Join" -H "X-Emby-Token: ${TOKEN_B}" $H -d "{\"GroupId\":\"$GID\"}"
curl -sS "http://127.0.0.1:8080/emby/SyncPlay/List" -H "X-Emby-Token: ${TOKEN_A}" | jq '.[0].Participants'
# 队列 + 播放/跳转/暂停 → 204；WS 两端收到 SyncPlayCommand
curl -sS -X POST "http://127.0.0.1:8080/emby/SyncPlay/SetNewQueue" -H "X-Emby-Token: ${TOKEN_A}" $H -d "{\"PlayingQueue\":[\"${ITEM_ID}\"]}"
curl -sS -X POST "http://127.0.0.1:8080/emby/SyncPlay/Unpause" -H "X-Emby-Token: ${TOKEN_B}"
curl -sS -X POST "http://127.0.0.1:8080/emby/SyncPlay/Seek" -H "X-Emby-Token: ${TOKEN_A}" $H -d '{"PositionTicks":600000000}'
curl -sS "http://127.0.0.1:8080/emby/SyncPlay/$GID" -H "X-Emby-Token: ${TOKEN_A}" | jq .State
# 离开：最后一人离开组即删除；未在组内 Leave → 409；Join 不存在组 → 404
curl -sS -X POST "http://127.0.0.1:8080/emby/SyncPlay/Leave" -H "X-Emby-Token: ${TOKEN_B}"
# Books：第四轮已支持 books 库类型（见 C.5）
```

单测：`cargo test --lib syncplay`（10 项）；FE：`vp exec node --no-experimental-webstorage node_modules/vitest/vitest.mjs run src/service/modules/syncplay.test.ts`（5 项）

### C.2 Trickplay 自检（可选）

```bash
# 管理员触发生成（202；后台 ffmpeg，时长×约 1/10 实时）
curl -sS -o /dev/null -w '%{http_code}\n' -X POST "http://127.0.0.1:8080/emby/Videos/${ITEM_ID}/Trickplay/Generate?Width=320" -H "X-Emby-Token: ${ADMIN_TOKEN}"
# 完成后：列表 → [{Width:320,...}]；m3u8 → #EXT-X-IMAGES-ONLY；0.jpg → image/jpeg
curl -sS "http://127.0.0.1:8080/emby/Videos/${ITEM_ID}/Trickplay" -H "X-Emby-Token: ${USER_TOKEN}" | jq .
curl -sS "http://127.0.0.1:8080/emby/Videos/${ITEM_ID}/Trickplay/320/tiles.m3u8?api_key=${USER_TOKEN}" | head
curl -sS -o /dev/null -w '%{http_code} %{content_type}\n' "http://127.0.0.1:8080/emby/Videos/${ITEM_ID}/Trickplay/320/0.jpg?api_key=${USER_TOKEN}"
```

C 端：播放该视频，鼠标悬停/拖动进度条应出现缩略图。单测：`cargo test --lib trickplay`（8 项，含本机 ffmpeg 真生成）；FE `npm test`。

### C.3 歌词上传 / PlaylistDto（可选）

```bash
curl -sS -X POST "http://127.0.0.1:8080/emby/Audio/${AUDIO_ID}/Lyrics?fileName=x.lrc" -H "X-Emby-Token: ${ADMIN_TOKEN}" --data-binary $'[00:01.00]hello' | jq '.Lyrics[0]'
curl -sS -o /dev/null -w '%{http_code}\n' -X DELETE "http://127.0.0.1:8080/emby/Audio/${AUDIO_ID}/Lyrics" -H "X-Emby-Token: ${ADMIN_TOKEN}"
curl -sS "http://127.0.0.1:8080/emby/Playlists/${PLAYLIST_ID}" -H "X-Emby-Token: ${USER_TOKEN}" | jq .
```

**FE 测试命令：** `cd fbz-fe && npm test`（= `vp exec node node_modules/vitest/vitest.mjs run`；`vp test` 因 vite-plus-test 0.1.24 无 vitest bin 仍不可用，未改依赖）。

**冻结（仍适用）：** 后台 UI 视觉大改；鉴权/网关重写需多轮审；**未 commit**。

### C.4 第三轮（2026-10-15 ~17:30 PT）实机冒烟记录
环境：独立库 `fbz_smoke_1016`（docker `fbz-api-postgres`，**未动用户 dev 数据**），`target/debug/fbz-api` :8091，测试媒体 `/Users/edy/Movies/fbz-ops-smoke`（Sintel 2s）。
- SyncPlay 两会话：建组/加入→Waiting，全员 Ready→自动 Unpause；Buffering→Pause+Waiting；队尾 NextItem→404；重复 Leave→409；Join 不存在→404；两端 WS 收到相同命令。✅
- Trickplay：Generate 202→Status Completed→jpg 200；定时任务自动生成 160；item `Trickplay` 字段出现。✅（入库后即时生成**未做**）
- NextUp 四参数：200；非法 NextUpDateCutoff→422。（无剧集数据，结果内容未验）
- 播放列表共享：共享 204、非 owner 共享 403、被共享者可见且无 CanEdit 编辑 403、IsPublic→OpenAccess=true、取消共享 204、owner 端 ItemIds 正确。✅
- 字幕上传：管理员 204 生成 `.eng.srt`，probe 后 PlaybackInfo 出现外挂字幕（Index 2, eng, srt）；普通用户 403。✅ 测试文件已删除。
- 新环境变量：`TRICKPLAY_AUTO_GENERATE`、`TRICKPLAY_SCHEDULE`(默认 24h)、`TRICKPLAY_MAX_PER_RUN`(默认 10)、`TRICKPLAY_WIDTH`(160/320/480)；需 `FBZ_SCHEDULER_ENABLED=true`。
- 测试：`cargo test --lib` 1370 passed / 50 ignored；`npm test`（fbz-fe）70 passed / 14 files；`vp check --no-fmt` 0 error / 17 warning；`cargo build` 绿。
- 未做：Books/有声书库类型；远程字幕源；C 端播放列表共享 UI；Trickplay 状态持久化。

### C.5 第四轮（2026-10-15 ~19:00 PT）实机冒烟记录
环境同 C.4（`fbz_smoke_1016`，:8091，额外 `MEDIA_ROOTS=/Users/edy/Movies,/tmp/fbz-smoke`、`PLUGIN_ALLOW_UNSIGNED=true`）。冒烟完成后 :8091 与插件进程已停，数据库保留；用户 :8080 未动。
- Books：建 books 库 → 3 Book + 2 AudioBook（MediaType Audio）+ Folder「The Hobbit」作 ParentId；epub Manifest 2 章、章节脚本已剥离、缺章 404；Resource 图片 200、html 422；cbz 3 页、越界 404；pdf File 200 application/pdf；未登录 401；无库权限用户 404；有声书 `/Audio/{id}/universal` Range 206。✅
- Trickplay 入库后：调度设 24h 时新入库片段 probe 后自动生成（日志 `trickplay after-scan generation finished generated=1`），item 出现 `Trickplay` 字段。✅
- 远程字幕：安装/审批/启用 `fbz.subtitle.opensubtitles`；无 key 搜索 Sintel 得 4 条；普通用户下载 403；管理员下载 204 写 `.eng.srt`，PlaybackInfo 出现外挂 eng；Providers 取文本 200；坏 id 422、未知插件 409；停用插件后搜索 `[]`。✅ 测试字幕文件已删除。注：旧版接口下载内容含站点广告行（上游内容）。
- SyncPlay 两会话：SetShuffleMode Shuffle → 当前项置首、两端 PlayQueue 同步；Sorted 恢复原序与索引；非法模式 422；RepeatNone 队尾 Next 404；RepeatAll Next 回到首项、Previous 回到末项；RepeatOne Next 停留；过期 PlaylistItemId 204 且无广播；不在组内 409。✅
- 播放列表共享 UI：后端端点 C.4 已验；本轮 UI 仅单测（3），实机点按未验。
- 测试：`cargo test --lib` 1382 passed / 50 ignored；`npm test` 79 passed / 16 files；`vp check --no-fmt` 0 error / 21 warning；`npm run build` 绿；`cargo build` 绿；插件 `node --test` 7 passed。
- 未做：字幕 hash 匹配；cbr/mobi 阅读；书籍元数据/封面；有声书断点续听；SyncPlay 队列增删/移动。

### C.6 第五轮（2026-10-15 ~17:10 PT）实机冒烟记录
环境同 C.5；冒烟后 :8091 与插件进程已停，`fbz_smoke_1016` 保留；用户 :8080 未动；用户媒体目录无残留测试字幕。
- 播放列表共享（按 C 端弹窗调用顺序，`pl.sh`）：打开弹窗 `GET /Playlists/{id}`（OpenAccess/Shares）+ `GET /Users/Public`；公开开→OpenAccess=true、他人可读 200；公开关→他人 404；共享只读 204→他人可读、加条目 403；切可编辑 204→`CanEdit:true`；非创建者管理共享/公开 403；移除 204→列表空、他人 404；重复移除 404、共享未知用户 404。新增：`DELETE /Items/{playlistId}` 非属主 403、属主 204、之后 404；非管理员删媒体条目仍 403。✅（注：被共享者向列表加入其无权访问的条目返回 `ItemAddedCount:0`，符合库权限。）
- Books 元数据：epub → 标题「The Smoke Chronicles」、作者 Ada Smoke（Writer）、年份 2019、简介（HTML 已剥离）、OPF 封面；cbz（放在 `Comics A/` 子目录）→ ComicInfo 标题/作者/年份、首页封面、ParentId=Folder「Comics A」；pdf → 首页渲染封面（macOS `sips`）。`/Items/{id}/Images/Primary` 200 image/jpeg；列表需 `EnableImageTypes=Primary`（C 端已带）。✅ 限制：Items 列表不回 People/Overview（详情有）。
- 有声书续听：对 Chapter 02 上报 Playing/Progress/Stopped（1.5s）→ UserData.PlaybackPositionTicks=15000000，`Items/Resume` 含 Chapter 02；C 端按此选第 2 段并从 1.5s 起播（单测覆盖选择逻辑；列表无 LastPlayedDate 时按顺序回退）。✅
- 字幕 moviehash：捕获插件入参，core 下发 `movieHash=d64bba41c2f8b2c7`、`movieByteSize=1383174`，与独立 Python 实现一致；<64KiB 文件不下发。真实插件：Sintel 搜索 4 条；`/Providers/Subtitles/Subtitles/{id}` 返回文本 **0 处广告行**（C.5 时曾有）。✅
- SyncPlay 队列（`sp3.sh`，两会话 + WS）：空组 Queue→NewPlaylist；Queue 追加队尾；QueueNext 插当前项后；坏 Mode/空 ItemIds 422；Move cccc→0（当前项索引跟随 0→1）；Move 未知 404、缺 id 422；Remove 当前项→下一项接替；Remove 未知 404、空体 422；ClearPlaylist 保留当前项；再加 ClearPlayingItem→清空、组 Idle；不在组 409；两端 PlayQueue 一致。✅
- 测试：`cargo test --lib` 1390 passed / 50 ignored；`cargo build` 绿；`npm test` 82 passed / 16 files；`vp check --no-fmt` 0 error / 21 warning；`npm run build` 绿；插件 `node --test` 8 passed。
- 未做：cbr/mobi/azw3 阅读；联网书目刮削；strm/远端文件 moviehash；已入库书籍需删库重扫才补元数据；SyncPlay 跨节点。


### C.7 第六轮（2026-10-15 ~17:35 PT）— 书籍字段/刷新/格式转换/在线书目 + UpdateItem + NFO 写回

- 实现：
  - Items 列表 `Fields` 支持 `Overview`、`People`（大小写不敏感；单 SQL 取每项前 20 人）。
  - 书籍刷新：`metadata::service` 对 book 走 `refresh_book`（本地重提取并替换 → 在线插件只补空）；`/Items/{libraryId}/Refresh` 回退为整库入队（≤500）；库级刷新的可选类型含 book。
  - `media/book_convert.rs`：mobi/azw3 → epub（DRM 拒绝；自带 PalmDOC 解压 + trailing entries 剥离，修正 mobi 0.8 丢最后正文记录；图片引用改写，`<img>`/`<br>` XHTML 闭合，pagebreak 切章，首图作封面，OPF 元数据含 ISBN）；cbr → cbz（bsdtar/unar）。缓存按路径+大小+mtime。
  - 插件 `plugins/openlibrary-provider`（`fbz.provider.books`，端口 19108）：Open Library（ISBN 直查 / 标题+作者搜索）+ Google Books（key 可选）合并；仅处理 itemType=book；标题搜索防误配（标题相同或互为前缀、作者姓氏重叠）。
  - `POST /Items/{id}`（UpdateItem），详情附加 `ProviderIds/Studios/Tags/SortName/ForcedSortName/LockData`；`metadata/nfo_write.rs`（`NFO_SAVER_ENABLED`）。
- 冒烟（8091 / `fbz_smoke_1016`；结束后已停服务与插件，库保留）：
  - 测试样本：`/tmp/fbz-smoke/tools/mkmobi.py` 生成 MOBI（`Smoke Mobi.mobi`、`Smoke Kindle.azw3`，含 EXTH 作者/简介/ISBN/日期 + PNG）；`Smoke CBR.cbr`（zip 容器，含 ComicInfo）。✅
  - 列表 `Fields=Overview,People`：6 本书均返回作者/简介/年份；不带 Fields 时两个字段都不出现。✅
  - 清空书籍的简介/年份/作者/外部 id 后：`POST /Items/{epub}/Refresh` → 该书恢复；`POST /Items/{书库}/Refresh` → 其余全部恢复（cbr/mobi/azw3/cbz）。✅
  - Manifest：mobi/azw3 为 `Format=epub, SourceFormat=mobi|azw3, Converted=true`，2 章；章 0 含插图与 Part One，章 1 含 Part Two；`Resource` 取插图 200 image/png；cbr `PageCount=2`，Pages 200 image/png；`/File` 返回原文件。✅
  - 在线插件（真实网络）：`Dune.pdf`（无本地元数据）刷新后 → 1965 / Frank Herbert / 简介 / `isbn` + `openlibrary` 外部 id；`Smoke Kindle` 凭 ISBN 得到 `openlibrary` id，作者不被追加；`Smoke Paper` 等虚构书无误配（首轮曾误配一次，已加防误配并修复「已有作者仍追加」）。电影类型查询返回 `metadata:null`。Google Books 匿名配额当日 429，自动降级为仅 Open Library。✅
  - UpdateItem：普通用户 403、管理员 204、未知 id 404；详情回显 Name/OriginalTitle/SortName/Overview/年份/首映/分级/评分/Genres/Tags/Studios/ProviderIds(Imdb)/LockData=true，People 带类型/角色。`NFO_SAVER_ENABLED=true` → 生成 `movies/Hash Clip (2026).nfo`（转义、uniqueid、导演/演员正确）；预置手写 NFO 后再保存 → 原文未被覆盖。✅
- 测试：`cargo test --lib` 1403 passed / 50 ignored；`cargo build` 绿；`npm test` 82 passed / 16 files；`vp check --no-fmt` 0 error / 21 warning；`npm run build` 绿；插件 `node --test`：opensubtitles 8、openlibrary 8。
- 未做：DRM 书；真 RAR 格式 cbr（本机无 rar 工具造样本）；pdf 文本元数据；FE 未显示原始格式；远程歌词下载；刷新后自动写 NFO；按库保存器开关；LockData 不阻止后续刷新。
- 注意：冒烟库中 `Hash Clip` 的元数据已被编辑器改写；`/tmp/fbz-smoke/movies/Hash Clip (2026).nfo` 是本轮产物（可删）。

### C.8 第七轮（2026-10-15 ~17:50 PT）— 锁定 / NFO 按库 / Trickplay 持久化 / 歌词插件 / 文件夹权限 / 阅读器格式

- 实现：
  - 迁移 `0109_metadata_locks_nfo_lyrics.sql`：`media_items.lock_data`、`locked_fields text[]`，触发器 `trg_media_items_metadata_lock`（会话变量 `fbz.metadata_edit=on` 才能绕过；锁列本身只能由编辑器改）；`libraries.save_local_metadata`；插件 hook 白名单加 `lyrics.provider.query`。
  - `POST /Items/{id}` 接收 `LockData`/`LockedFields`，详情回显；`metadata::service` 刷新遇 `LockData` 返回 `locked`，按 LockedFields 门控 Genres/Studios/Cast；插件 Host patch 同样门控子表。
  - `POST /Library/VirtualFolders/LibraryOptions`（管理员）；`nfo_write` = env **或** 库开关；刷新结束写 NFO。
  - `media/trickplay.rs` 状态文件 `status.json`。
  - `routes/remote_lyrics.rs` + `routes/lyrics.rs`：插件优先搜索、`GET /Providers/Lyrics/{id}`、`POST /Audio/{id}/RemoteSearch/Lyrics/{id}`；插件 `plugins/lrclib-provider`（端口 19109）。
  - `routes/users.rs`：Policy 的 EnableAllFolders/EnabledFolders/BlockedMediaFolders/BlockedFolders → `library_permissions.can_view`。
  - FE：`BookManifest.SourceFormat/Converted`、`formatBadge()`，阅读器标题旁徽标 +「下载原文件」。
  - 修复：插件同步调用幂等键跨 invoker 重复（`plugins/invoke.rs`，进程级计数器）；内置 LRCLIB 数字 id 解析。
- 冒烟（8091 / `fbz_smoke_1016`，start.sh 已去掉 `NFO_SAVER_ENABLED`；结束后已停服务与插件，库保留；用户自己的 8080 与 19101–19106 未动）：
  - 锁定（`s7a.sh`）：Chronicles 设 LockData → 库内 `lock_data=t`；刷新、裸 SQL 改 title/overview、重扫书库后仍为 `The Smoke Chronicles | LOCKED overview`；详情 `LockData=true`。Smoke Mobi 设 `LockedFields=[Overview,Cast]` → 裸 SQL 改 overview 无效、sort_title 生效；替换式刷新后简介仍为 `MY overview`。编辑器解锁后裸 SQL 可写。✅
  - NFO 按库（`s7b.sh`）：普通用户 403；管理员设电影库 `MetadataSavers=[Nfo]` → VirtualFolders 仅该库 `SaveLocalMetadata=True, MetadataSavers=['Nfo']`；新建 `Nfo Film (2024)/`（手写 `movie.nfo`）刷新后生成 `Nfo Film (2024).nfo`（带 fbz 标记，title/plot/year/genre/imdb），手写 movie.nfo 未动；书库未开启，无 NFO。首测发现「未匹配时不写」，已改为刷新结束都写。✅
  - Trickplay：`artwork/trickplay/{id}/status.json` = `Completed/160`；重启服务后 `GET /Videos/{id}/Trickplay/Status` 原样返回。✅
  - 歌词（`s7c.sh`，真实网络）：生成 `music/Queen/11 Bohemian Rhapsody.mp3`（ffmpeg，带标签）建音乐库；插件停用时回退内置（`lrclib-30585262`）；装+启用 `fbz.lyrics.lrclib` 后搜索返回 `fbz.lyrics.lrclib~19080`（Queen / A Night at the Opera，同步）；`GET /Providers/Lyrics/{id}` 连续 3 次 200（首测第 2 次起 404，查出幂等键 bug 并修复）；普通用户下载 404；管理员下载 200 → 写 `11 Bohemian Rhapsody.lrc`，`GET /Audio/{id}/Lyrics` 有同步行；停用后插件 id 取词 409、内置 id 取词/下载 200。✅
  - 文件夹权限（用户 B）：`EnableAllFolders=false, EnabledFolders=[Smoke Clips]` → Views 仅 `Smoke Clips`，书籍详情 404；`EnableAllFolders=true + BlockedMediaFolders=[Smoke Clips]` → Views `Smoke, Smoke Books`；恢复全部 → 三个库。注意：B 冒烟前 Views 为空（无权限行），恢复后可见全部；之后新建的 `Smoke Music` 对 B 不自动可见（已知缺口）。✅（含缺口）
  - 阅读器：`/Books/{Smoke Mobi}/Manifest` → `epub / mobi / Converted=true`，徽标渲染 `MOBI → EPUB`（单测覆盖；未开浏览器目测）。✅
- 测试：`cargo test --lib` 1410 passed / 50 ignored；`cargo build` 绿；`npm test` 83 passed / 16 files；`vp check --no-fmt` 0 error / 21 warning；`npm run build` 绿；插件 `node --test`：opensubtitles 8、openlibrary 8、lrclib 5。
- 未做：季 NFO、书籍 NFO；图片锁；`EnableAllFolders` 用户对新建库不自动可见、GET Policy 回推形状；BlockedTags/分级上限；歌词内置回退的 Metadata 沿用 LRCLIB 原字段（该条目的 artist 字段本身就是「歌名 - 歌手」）。
- 冒烟残留：`/tmp/fbz-smoke/music`（音乐库 + .lrc）、`movies/Nfo Film (2024)/`、Chronicles/Mobi 编辑过的简介。

### C.9 第八轮（2026-10-15 ~18:40 PT）— 原样策略 / 新库自动授权 / BlockedTags + 分级 / 图片锁 / 季 NFO / LRCLIB 规范化 / 恢复用户 B

- 实现：迁移 `0110_user_access_policy_image_locks.sql`；`src/users/access_policy.rs`（新）；`routes/users.rs`（保存+回显）；`library/repository.rs` 12 个查询接条目级可见性；`media/repository.rs` 手动图片操作放行；`plugins/host.rs` 图片被锁时跳过（不报错）；`item_update.rs` LockedFields 增 `Images`；`metadata/nfo_write.rs` 季 NFO；`routes/lyrics.rs` LRCLIB 回退规范化与结构化搜索。**触及权限判定的文件清单见矩阵「第八轮 · 触及权限判定的文件」。**
- 冒烟（8091 / `fbz_smoke_1016`；结束后已停服；8080 pid 16203 与 19101–19106 未动）：
  - `s8a.sh`：B 设 `EnableAllFolders=true, BlockedMediaFolders=[Smoke Books]` → Views `Smoke, Smoke Clips, Smoke Music`，GET Policy 原样 `True / [] / [books] `；新建 `Smoke TV` → B 立即可见；改 `EnableAllFolders=false, EnabledFolders=[Smoke Clips]` → 仅 Clips，新建 `Smoke Extra` 不可见；只带 IsDisabled 的 POST 后策略不变。✅
  - `s8b.sh`：Smoke Clip=R、Hash Clip=PG-13+标签 Horror、Smoke Show=TV-MA（分集未分级，继承剧）。B `MaxParentalRating=5` → Clips 列表/Latest 仅 `Nfo Film`，TV 递归为空，Episodes 0，三个详情 404；`BlockedTags=[horror]` → 仅 Hash Clip 消失（详情 404），其余恢复；管理员列表三部都在。✅
  - `s8c.sh`：剧库开 Nfo，刷新 S01E01 → `Smoke Show S01E01.nfo` + `Season 01/season.nfo`（`<season><seasonnumber>1</seasonnumber>`）。Dune `LockedFields=[Images]` → 详情回显；裸 SQL 删除+插入后仍为 1 张原图；替换图片刷新后不变；手动上传 Backdrop 204 并入库、手动删除 204；解锁后裸 SQL 可写。LRCLIB 回退（插件停用）：`Bohemian Rhapsody | Queen | <真实专辑>`；按 id `lrclib-30585262`（原三栏「Bohemian Rhapsody - Queen」）→ `Title=Bohemian Rhapsody, Artist=Queen`。✅
  - 恢复 B：第七轮冒烟前 B **没有任何 library_permissions 行**（行的 created_at 均为第七轮冒烟时刻；当时 Views 为空）。已执行 `/tmp/fbz-smoke/restore_userB.sql`（删 B 的权限行与策略行、清分级/未分级限制）→ B Views 为空、GET Policy `EnableAllFolders=false, EnabledFolders=[]`，与冒烟前一致。冒烟用的 `Smoke Extra` 库已删。✅
- 测试：`cargo test --lib` 1414 passed / 50 ignored；`cargo build` 绿；`npm test` 83 passed；`vp check --no-fmt` 0 error / 21 warning；`npm run build` 绿；插件 opensubtitles 8、openlibrary 8、lrclib 5。
- 未做：AllowedTags（包含模式）、AccessSchedules；Genres/Studios/Tags 聚合计数与 SearchHints 未做条目级过滤（库级仍生效）；书籍 NFO；`Images` 锁为 fbz 扩展（JF 无此字段）。
- 冒烟残留：Smoke Clip 分级 R、Hash Clip 标签 Horror、Smoke Show TV-MA；`Smoke TV` 库与 `/tmp/fbz-smoke/tv`（含 NFO）。
