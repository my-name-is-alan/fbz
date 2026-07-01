# 初始化向导接后端 + 服务器地址固定 + 请求层 mock 分离

> 计划文档 · 2026-06-30 · 跨 `fbz-api` + `fbz-fe` 两个包
> 状态：待审阅（未动代码）

## 背景与问题

用户提出三个问题（均已核对代码）：

1. **初始化向导是纯本地 mock，完全不连服务器。** `SetupWizardModal.vue:91-94` 的"完成初始化"只调
   `uiStore.completeInitialization()`（`stores/ui.ts:69-75`），实体仅 `localStorage.setItem("fbz_initialized","true")`。
   管理员账号、媒体库都没发后端；"是否首次启动"也只读 localStorage。
2. **项目不应支持自定义服务器地址，所有请求应固定指向对应后端。** 当前 `request.ts` 全套
   （`normalizeServerBase/getServerBase/setServerBase/resolveApiBase/resolveEmbyBase`）+ 登录页输入框
   都按"用户填任意 origin"设计，且 `request.ts`(`fbz_server_base`) 与 `auth.ts`(`fbz_server_address`)
   两套 localStorage key 冲突。
3. **请求层 mock 与真 API 混在一起。** `tmdb.ts`/`media.ts` 是同梱 mock，`auth/navigation/music/library/admin`
   是真 API，没有类型层面的边界；`auth` store 还内嵌假用户 CRUD。

## 已确认的关键事实（决定方案形态）

- 后端**已有** `auth/bootstrap.rs::ensure_bootstrap_admin`：启动时从环境变量
  `FBZ_BOOTSTRAP_ADMIN_USERNAME/PASSWORD`（`config.rs:677-679`，密码 ≥12 位）建首个 Owner，已存在则跳过。
- 后端**故意禁用**运行时用户增删改：`compat/emby/routes/users.rs` 的 `create_user` 等 8 处都返回
  `user_mutation_disabled_error()`（409）。
- `IsStartupWizardCompleted` 在 `compat/emby/dto/mod.rs:349` **写死 `true`**。
- 后端 `/api/admin/users` **只有** `GET list_users` + `PUT update_user_policy`（`admin/routes.rs:147-148`），
  **没有运行时建/删用户**。前端 `service/modules/admin.ts` 也没有用户增删改函数。
- `/api` 自定义路由模式：每个域 `mod.rs/routes.rs/service.rs/access.rs`，在 `app.rs`
  用 `.merge(xxx::routes::router())` 挂载（参照 `navigation/`）。
- users 表已存在，**本计划不需要新数据库迁移**——只读它的计数。

## 已定方向（用户拍板）

- ①：**动态向导 + 锁定式 setup 接口**（Emby/Jellyfin 做法）。
- ③：**request 层 + store 的 mock 都分离**。
- 执行：**先出本计划文档**，审阅后再写代码。

---

## A. 后端 fbz-api（①的地基）

新建 `src/setup/` 模块，仿 `navigation/` 结构。**先做后端、cargo test 跑通，再接前端。**

### A1. 动态 `IsStartupWizardCompleted`

- 现状：`compat/emby/dto/mod.rs:262` 的 `ServerConfigurationDto::from(ServerConfigurationSource)` 是纯转换，
  第 349 行硬编码 `is_startup_wizard_completed: true`。
- 改法：给 `ServerConfigurationSource` 增一个 `has_users: bool` 字段（DTO 转换里映射成
  `is_startup_wizard_completed: source.has_users`）。在 `system.rs` 组装 source 的
  `server_configuration_source(state)`（约 `system.rs:504`）里查一次用户数注入。
  - 用户数查询封装为 setup service 的 `has_any_user(pool) -> bool`，避免 SQL 散落。
  - `system.rs` 的几个 configuration handler 已 `async`，可直接 await。
- 影响测试：`system.rs:417`、`dto/mod.rs:1846` 两处断言 `IsStartupWizardCompleted == true`，
  改为构造 `has_users: true` 的 source（保持语义）。

### A2. `GET /api/setup/status`（无认证）

- 返回 `{ "initialized": bool }`，`initialized = has_any_user(pool)`。
- 无认证：开机即可问，取代前端 localStorage 的 `fbz_initialized`。
- 控制器在 `setup/routes.rs`，逻辑在 `setup/service.rs`。

### A3. `POST /api/setup`（锁定式无认证）

- 请求体：`{ username, password }`（后续可扩展 displayName/偏好，先最小集）。
- **锁定语义**：仅当用户数为 0 时可建首个 Owner；否则 409 `setup already completed`。
- **并发安全（关键）**：不能先查后插（TOCTOU）。在**单个事务**内：
  1. `select count(*) from users` —— 若 >0，回滚并返回 409。
  2. upsert `Owner` role（复用 bootstrap 的 `on conflict` SQL）。
  3. `insert into users(...)`，靠 `username_normalized` 唯一约束兜底并发。
  4. 若 insert 命中唯一冲突 → 转成 409（已被另一并发请求建好）。
- **复用**：把 `auth/bootstrap.rs` 里建 Owner+user 的 SQL 抽成共享函数
  `create_owner_admin(tx, username, password)`，供 bootstrap（env 通道）与 setup（HTTP 通道）共用，避免两份漂移。
- 密码校验：复用 config 的 ≥12 位规则（抽成 `validate_admin_password`），不足返回 422。
- env 变量 bootstrap **保留**作为无头部署优先通道（`main.rs:50` 不动）。

### A4. 媒体库创建不进 setup 接口

- 向导第 3 步「建库」在前端**登录后**调已存在的 `POST /api/admin/libraries`
  （`admin/routes.rs:90`，需 admin 鉴权）。setup 接口只管"建管理员"。

### A5. 模块挂载与文档

- `src/lib.rs` 加 `pub mod setup;`；`app.rs` 的 router 组装处 `.merge(setup::routes::router())`。
- `setup/access.rs`：status/setup 都无认证，但可放"请求体大小限制""用户名规整"等校验辅助。
- 同步更新 `fbz-api/README.md` "当前能力"清单（CLAUDE.md 强制）：新增
  `GET /api/setup/status`、`POST /api/setup`，并说明 `IsStartupWizardCompleted` 改为动态。

### A6. 后端测试

- `setup/service.rs` 单测：`has_any_user` 空库/非空；密码校验边界（11/12 位）。
- `setup/routes.rs` 或集成测试：首次 `POST /api/setup` 成功→再次 409；`GET status` 前后变化。
- 调整 A1 提到的两处既有断言。
- 全程 `cargo test`（CLAUDE.md：`cargo test <name>` 可跑单测）。

---

## B. 前端 fbz-fe — ② 服务器地址固定

目标：所有请求同源，删掉"用户填服务器地址"的全部痕迹，消除双 key 冲突。
构建期仍可用 `VITE_API_BASE_URL` 换后端地址（部署用），但运行时不可由用户改。

### B1. `service/request.ts` 瘦身

- 删除：`SERVER_BASE_KEY`、`normalizeServerBase`、`getServerBase`、`setServerBase`、
  `resolveApiBase`、`resolveEmbyBase`。
- `request` 实例 `baseURL = import.meta.env.VITE_API_BASE_URL ?? "/api"`（同源相对路径）。
- `embyRequest` 实例 `baseURL = ""`（同源根路径，axios 拼相对路径）。
- `mediaImageUrl()` 去掉 `resolveEmbyBase()` 前缀，直接用后端给的根路径 + `api_key` 查询串。
- 保留 `ACCESS_TOKEN_KEY/getAccessToken/setAccessToken/attachAccessToken`。

### B2. 登录页 `views/user/login/index.vue`

- 删除"服务器地址"表单项（模板 129-150 行）、`serverAddress` ref（33-35 行）、
  `handleLogin` 里传 `serverAddress`（59 行）。
- 文案"输入服务器与账户凭据"改为"输入账户凭据"。

### B3. `stores/auth.ts` + `service/modules/auth.ts`

- 删 `serverAddress` 状态与 `"fbz_server_address"` key（39、153-156、292 行）。
- `LoginPayload` 去掉 `serverAddress`；`auth.ts`(service) 删 `setServerBase` 调用（7、43 行）。
- 登录失败文案"请检查服务器地址"改为"请检查网络与服务器状态"。

---

## C. 前端 fbz-fe — ① 向导真连后端

### C1. 新建 `service/modules/setup.ts`

- `getSetupStatus(): Promise<{ initialized: boolean }>` → `request.get("/setup/status")`。
- `submitSetup(payload: { username; password }): Promise<void>` → `request.post("/setup", payload)`。
- 类型放 `types/setup.ts`。

### C2. `stores/ui.ts` 初始化态改为问后端

- `isInitialized` 不再读 localStorage（删 35 行），改为应用启动时
  `await getSetupStatus()` 设置；`setupWizardOpen = !initialized`。
- `completeInitialization()` 去掉 `localStorage.setItem("fbz_initialized")`，仅切 UI 态。
- `resetInitialization()` 相应调整（仅供开发用，或删除）。
- 在 `App.vue` 或路由守卫触发一次 status 拉取（启动时机见 C4）。

### C3. `SetupWizardModal.vue` 接真接口

- 第 2 步"创建管理员"：`nextStep` 到第 2→3 步时，先 `await submitSetup({username,password})`；
  成功才前进，失败 toast 并停留。
- 完成后引导用户用新管理员登录（建管理员后后端无会话，需走登录拿 token）。
  - 方案：建完管理员直接调 `authStore.login({username,password})` 自动登录，再进第 3 步建库。
- 第 3 步"建库"：调 `adminApi.createLibrary(...)`（登录后才有 admin token），失败可跳过。
- 第 4 步偏好：保持本地 theme store（纯前端偏好，无需后端）。

### C4. 启动时序

- Pinia 先于 Router 安装（已符合 CLAUDE.md）。
- 应用启动 → 拉 `getSetupStatus()`：
  - `initialized=false` → 打开向导（且路由 replace 到 `/`，沿用现有 `SetupWizardModal` onMounted 逻辑）。
  - `initialized=true` → 正常走登录守卫。
- status 请求失败（后端没起）→ 给明确 toast，不要静默当未初始化。

---

## D. 前端 fbz-fe — ③ mock 与真 API 分离

### D1. 目录边界

- 新建 `service/mock/`：迁入 `tmdb.ts`、`media.ts` 及其 JSON（`tmdb-catalog.json`/`tmdb-details.json`）。
  文件头统一注释标注"设计态 mock，接后端后替换"。
- `service/modules/` 只留真 API（auth/navigation/library/detail/music/admin/setup）。
- `detail.ts` 的"后端失败回退 TMDB"降级点：显式注释为 `mock-fallback`，import 路径指向 `service/mock/`。
- 调整所有引用方 import 路径（`views/home`、`views/library/detail`、`stores/library` 等，见盘点）。

### D2. 用户增删改：后端补接口 + 前端接真（用户已确认纳入）

**后端 fbz-api（D2-BE）** — 在 `admin/` 现有 users 接口旁补建/删：

- `roles` 现状：无迁移 seed，全部按需 upsert（`auth/bootstrap.rs:39`、各处 `on conflict (name_normalized)`）；
  管理员判定 = `name_normalized in ('owner','admin','administrator')`（`users/repository.rs:86`）。
- `POST /api/admin/users`（需 admin 鉴权）：请求体 `{ username, password, role, displayName?, allowDownload?, allowTranscode?, allowNewDeviceLogin? }`。
  - `role`：前端三档 `admin/user/guest` → 后端 role 名 `Administrator/User/Guest`，事务内 upsert role 拿 `role_id`。
  - 密码复用 `validate_admin_password`（≥12 位，与 setup 共用）。
  - 用户名规整 + `username_normalized` 唯一约束兜底；冲突 → 409。
  - repository 新增 `create_admin_user(input) -> AdminUserRecord`（事务：upsert role → insert user → 回查 AdminUserRecord）。
- `DELETE /api/admin/users/{id}`（需 admin 鉴权）：
  - **守卫**：禁止删除自己（`admin.public_id == id` → 409，参照 `update_user_policy:1890`）。
  - **守卫**：禁止删除最后一个 owner/admin（事务内 `count` 管理员 ≤1 → 409，避免锁死系统）。
  - repository 新增 `delete_admin_user(id) -> bool`（`api_keys`/会话已 `on delete cascade`，见 0002 schema）。
- 测试：建用户成功→重复 409；删自己 409；删最后管理员 409；删普通用户成功。
- 同步 `README.md` 能力清单新增这两条。

**前端 fbz-fe（D2-FE）**：

- `service/modules/admin.ts` 新增 `listSystemUsers()` / `createSystemUser()` / `deleteSystemUser()` / 复用
  `updateUserPolicy()`（启用态切换）。
- `stores/auth.ts`：删 `defaultUsers`(46-73) 与 `fbz_system_users` localStorage；`users` 改异步从后端拉；
  `addUser`/`deleteUser`/`toggleUserStatus`/`updateUser` 全部改接真接口。
- `changePassword`：后端目前**仍无**改密接口（`update_user` 被禁用）→ 仍标"需后端支持"
  （若要做需再扩 `PUT /api/admin/users/{id}/password`，本轮不含，避免范围再膨胀）。

### D3. `stores/ui.ts` filePicker 写死路径

- `currentPath` 默认 `/media/nas/电影`（56、113 行）改为空串；
  文件浏览应由后端文件系统浏览接口驱动（若无则标注 mock-fallback，本计划不强制接）。

---

## E. 风险与边界

- **C3 自动登录**：建完管理员到登录之间若失败，用户可能卡在"管理员已建但未登录"。
  处理：submitSetup 成功后即使自动登录失败，也跳登录页（status 已 initialized，不会再进向导）。
- **D2 范围收缩**：用户管理的"增删改"在后端补接口前只能只读/改 policy，需在交付说明里讲清。
- **图片地址**：B1 改 `mediaImageUrl` 后需回归详情页/卡片海报显示（同源后路径不变，风险低）。
- **不需要 DB 迁移**；不破坏 env bootstrap 通道。

## F. 建议执行顺序

1. 后端 A1-A6（setup 模块 + 动态 flag），`cargo test` 绿。
2. 前端 B（服务器地址固定，独立可测）。
3. 前端 C（向导接 setup/status + setup）。
4. 前端 D（mock 分离 + 用户列表只读化）。
5. 全程 `vp check` + `vp test`；涉及构建/路由/依赖再 `vp run build`。

## G. 验收

- 空库启动：前端自动进向导 → 填管理员 → 自动登录 → （可选）建库 → 进首页。
- 再次刷新：`status.initialized=true`，不再进向导，直接登录页/首页。
- 并发双开向导提交：仅一个成功，另一个 409。
- 全局搜索无 `fbz_server_base`/`fbz_server_address`/服务器地址输入框残留。
- `IsStartupWizardCompleted` 随用户数动态变化。
