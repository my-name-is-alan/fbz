<!--VITE PLUS START-->

# Using Vite+, the Unified Toolchain for the Web

This project is using Vite+, a unified toolchain built on top of Vite, Rolldown, Vitest, tsdown, Oxlint, Oxfmt, and Vite Task. Vite+ wraps runtime management, package management, and frontend tooling in a single global CLI called `vp`. Vite+ is distinct from Vite, and it invokes Vite through `vp dev` and `vp build`. Run `vp help` to print a list of commands and `vp <command> --help` for information about a specific command.

Docs are local at `node_modules/vite-plus/docs` or online at https://viteplus.dev/guide/.

## Review Checklist

- [ ] Run `vp install` after pulling remote changes and before getting started.
- [ ] Run `vp check` and `vp test` to format, lint, type check and test changes.
- [ ] Check if there are `vite.config.ts` tasks or `package.json` scripts necessary for validation, run via `vp run <script>`.
- [ ] If setup, runtime, or package-manager behavior looks wrong, run `vp env doctor` and include its output when asking for help.

<!--VITE PLUS END-->

# Admin V3 Agent Guide

本文件是当前仓库的正式协作规范。修改代码、依赖、构建配置、目录结构或公共约定时，必须同步维护本文件，避免规则和实现漂移。

## 基础原则

- 项目是 `Vue 3 + TypeScript` 单页应用，默认使用 `Composition API` 和 `<script setup lang="ts">`。
- 日常开发统一使用 `vp`，不要直接使用 `vite`、`npm`、`pnpm dlx` 执行常规开发、构建、校验命令。
- 常用命令优先使用 `vp dev`、`vp build`、`vp preview`、`vp check`、`vp test`、`vp install`、`vp run <script>`。
- 新增、移除或升级依赖必须通过 `vp add <pkg>`、`vp add -D <pkg>`、`vp remove <pkg>` 等包管理命令执行，不要手写 `package.json` 版本号。
- 写代码前先阅读相关现有实现和仓库内技能文档；不要依赖个人机器上的全局技能目录。
- 除非用户明确要求，否则不要引入超出当前技术栈的大型替代方案。

## 仓库内技能参考

共享前端技能资料位于 [ai-skills/antfu/README.md](ai-skills/antfu/README.md)，不要依赖 `~/.codex/skills`、`~/.claude/skills` 或其他个人全局目录。

涉及以下主题时，先读取对应目录下的 `SKILL.md`，再按需展开 `references/`：

- `vite`
- `vue`
- `vue-best-practices`
- `vue-router-best-practices`
- `pinia`
- `unocss`
- `vueuse-functions`
- `web-design-guidelines`
- `pnpm`

如果当前 AI 工具不支持技能机制，就把这些 Markdown 当作普通参考文档直接读取。

## 构建与环境

- `vite.config.ts` 必须使用函数返回形式，例如 `defineConfig(({ mode, command }) => { return { ... } })`，不要直接导出静态对象。
- 读取环境变量时使用 `loadEnv(mode, ".", "")` 或等价写法。
- 环境文件按 Vite mode 匹配，例如 `.env`、`.env.local`、`.env.prod`；使用 `.env.prod` 时命令必须显式传入 `--mode prod`。
- UnoCSS、自动导入、组件自动导入、Vite+ 相关配置必须放在独立配置文件或清晰的 Vite 配置段中，不要散落在业务代码里。
- 如果 `vite.config.ts` 或 `package.json` 中存在额外验证任务，提交前必须通过 `vp run <script>` 一并执行。

## 应用架构

- `App.vue` 只保留应用入口级结构，不写页面布局、业务逻辑或临时样式；布局放在 `src/layouts/`。
- 全局壳层布局是 `src/layouts/default.vue`：`AppHeader` + `AppDrawer` + `RouterView` + `MusicPlayerBar`，不做卡片跨路由飞渡或页面切换动画，保证媒体网格进入详情时足够轻量。`App.vue` 渲染 RouterView、GlobalUiContainer 和唯一的 PlaybackOverlay，播放器跨路由保留。
- 媒体卡片统一用 `MediaCard`：卡片主体进入详情，海报上的播放按钮打开全屏播放覆盖层，不走播放路由。`MediaItem.detailType`（movie/tv）决定详情与播放类型，与 libraryId 解耦（动漫=tv、纪录片=movie）。
- 路由必须与文件目录对应，例如 `/user/login` 对应 `src/views/user/login/index.vue`；动态路由如 `/library/:id` 放在 `src/views/library/detail/index.vue`（不使用 `[id].vue` 文件名约定，路由表在 `src/router/index.ts` 手动维护）。
- 详情页按类型分路径：`/movie/:id`、`/tv/:id`、`/person/:id`、`/collection/:id`，分别对应 `src/views/detail/{movie,tv,person,collection}/index.vue`。系列（collection）是一等概念，影片详情会链接到其所属系列。详情区块组件在 `src/components/detail/`：`DetailHero`（poster+fanart 头部 + 多版本下拉）、`CastRow`（演职员，左右箭头滚动）、`SeasonEpisodes`（季/集，默认定位到「继续观看」的季集）、`SimilarRow`（相似推荐）。
- 网络请求统一走 `src/service/request.ts` 导出的 `request` 单例；新增接口模块放在 `src/service/modules/`。真实登录、媒体库、条目、PlaybackInfo 与进度回传由 `service/modules/server.ts` 和 `playback-reporter.ts` 提供，禁止在失败后退回模拟成功。
- Pinia store 使用函数式 `defineStore("id", () => {})`，状态优先使用 `ref` / `computed`。
- 组件中解构 store 优先使用 `storeToRefs()`。
- 安装 Pinia 使用官网写法：`const pinia = createPinia(); app.use(pinia)`；如果路由守卫会读取 store，插件安装顺序保持 `Pinia` 先于 `Router`。
- 公共表格组合逻辑统一放在 `src/composables/Table/`；迁移或扩展时参考历史项目 `H:\Code\ai-assistant-admin\src\composables\Table`，并同步检查 `vxe-table`、`vxe-pc-ui`、`xe-utils` 依赖。

## 目录规范

- `src/` 下应保持清晰模块边界，优先使用 `router`、`layouts`、`utils`、`plugins`、`composables`、`components`、`styles`、`views`、`types`、`stores`、`service` 等目录。
- `src/components` 下的组件会被自动导入，`SFC` 中不要重复手写导入项目内组件。
- 普通基础组件使用 `Base` 前缀；业务组件按业务域放置，避免混入基础组件目录。
- 新建组件时优先保持 headless 或低耦合设计，把数据、状态、展示边界拆清楚。
- 不要把一次性页面逻辑沉淀为全局工具；只有跨页面复用、边界清晰的逻辑才放入 `utils` 或 `composables`。

## UI 与组件库

- 参考[figma](https://www.figma.com/design/dZCl5F9i14ywuTT3evWEzx/Untitled?t=fdizBYSKNTI2x08Q-0)做设计组件/样式
- 需要参考demo的原型设计

## 样式与设计

- 基础交互组件统一使用用户指定的 daisyUI（du- 前缀），图标统一用 @lucide/vue 的 BaseIcon。UnoCSS 暂保留给旧页面工具类；禁止新增另一套按钮、输入框、弹窗体系。
- 项目源码样式强制使用 `SCSS`：全局样式使用 `.scss`，SFC 样式块必须写 `<style lang="scss">` 或 `<style scoped lang="scss">`。
- 业务样式继续使用 SCSS。用户指定 daisyUI/Tailwind 后，允许唯一的构建入口 `src/styles/daisy.css` 使用原生 CSS 的 @import/@plugin；reset 放在 base 层，避免压过组件层。
- `src/styles/theme/` 是主题 token 主目录。
- `src/styles/theme/tokens.scss` 必须通过 `vite.config.ts` 的 `css.preprocessorOptions.scss.additionalData` 注入到每个 SCSS 文件顶部，不要在业务样式文件里逐个手动 `@use` / `@import` token。
- `uno.config.ts` 中的主题 token 必须与 `src/styles/theme/tokens.scss` 保持同步，优先引用 token 暴露的 CSS 变量，例如 `var(--fbz-color-brand-500)`。
- 全局基础样式集中在 `src/style.scss`（含 `--header-h` 头部高度变量，桌面 60px / 手机 56px，布局与各页面顶部留白统一引用它）。
- 字体：正文用 `--fbz-font-sans`（系统优先现代字栈 `ui-sans-serif, system-ui, PingFang SC, MiSans…`），品牌和数字展示与正文使用同一字栈，数字使用 tabular-nums，`--fbz-font-display` 与 sans 保持一致。新增展示型数字/Logo 用 display 字体，正文不要硬写字栈。
- 设计基调：纯黑底（`--fbz-color-bg: #0a0a0b`）+ 单一主题色 `--fbz-color-brand-500: #1ed760`（Spotify 绿）。主题绿只用于强调态（导航激活、主按钮、进度条、卡片 hover 边框），其余一律白/灰阶；禁止多彩混用、装饰性渐变、滥用大圆角（卡片 4px / 控件 6px）。**例外**：媒体卡片的清晰度徽章用 `tmdb.ts` 的 `resolutionColors`（4K 绿 / 2K 黄绿 / 1080P 蓝 / 720P 橙，借鉴 ），这是功能性色标不算多彩装饰。
- 横向滚动行一律用 `src/components/BaseScroller.vue`：隐藏原生横向滚动条（不要再出现裸露的横向滚动条），用 vueuse（`useEventListener`+`useResizeObserver`）按需在行首/行尾浮出**半透明渐变遮罩 + 居中 SVG 箭头**，仅在该方向还有内容可滚时显示，触摸设备隐藏。每列宽度由使用方通过 `:deep(.track) { --col: … }` 覆盖。`MediaRow`/`SimilarRow`/`CastRow` 均基于它。
- 下拉选择统一用 `src/components/BaseSelect.vue`（包装 daisyUI select，使用原生 select 的键盘与可访问性行为）：`v-model` 绑值，`options` 为 `{ label, value }[]`，自带面板样式/选中态/键盘与点击外部关闭。版本选择、季选择、题材筛选均已用它。
- 媒体卡片统一用 `src/components/media/MediaCard.vue`（纯 props 驱动：`item`/`layout`/`variant`/`port`），新增展示需求改这一个文件即可。其海报占位与圆角在 `MediaPoster.vue`。`CastRow` 演员头像列宽 64px（手机 56px），不要再放大。
- 响应式三档：桌面 ≥1024、平板 600–1024、手机 <600；手机端 `AppHeader` 收起为汉堡，导航走 `AppDrawer` 抽屉。
- 媒体海报/剧照统一用 `MediaPoster` 组件：有 `src` 显示真实图，无 `src` 渲染纯色占位块；设计阶段默认走占位，接后端后填地址即可。
- 没有明确设计要求时，不要重新引入暗黑主题分支。

## 自动导入与 TypeScript

- `unplugin-auto-import` 已覆盖 `Vue`、`Vue Router`、`Pinia`、`@vueuse/core`、`lodash-es`。
- 一般不需要手动导入 `ref`、`computed`、`watch`、`useRoute`、`useRouter`、`defineStore`、`debounce` 等常用 API。
- 类型导入必须使用 `import type`。
- 遵守当前严格 TypeScript 配置，未使用的局部变量和参数会直接报错。
- 不使用 `enum`、`namespace`、参数属性等不符合当前配置的语法。
- 遵循当前项目的 `.ts` 扩展名导入方式和 `@/*` 别名。
- 写 JSX/TSX 时使用 `class`，不要使用 `className`，也不要按模板语法思路写指令。

## 测试与校验

- 提交前至少运行 `vp check` 和 `vp test`。
- 涉及构建、路由、依赖、样式注入或 Vite 配置时，额外运行 `vp run build`。
- 测试文件优先与源码同目录放置，并使用 `*.test.ts` 命名。
- 组件测试优先使用 `@vue/test-utils`。
- 新增公共逻辑、组合式函数、请求模块或 bugfix 时，应优先补充行为测试。

## 依赖边界

正式依赖以 `package.json` 为准。新增依赖前先确认是否已有等价能力；新增后必须更新本节和相关规则。

- 禁止为了“补依赖”直接编辑 `package.json` 或 `pnpm-lock.yaml` 写入版本号；必须让 `vp add` / `vp add -D` / `vp remove` 这类命令解析版本并更新 lockfile。
- 禁止从示例项目、文档片段或记忆中复制依赖版本号到本项目；如果确实需要固定版本，先说明兼容性原因，再用包管理命令安装明确的 package spec，例如 `vp add some-package@1.2.3`。
- 现有 `catalog:`、workspace、lockfile 解析规则必须保留；不要把 `catalog:` 依赖改成手写 semver。
- 依赖变更后至少检查 `package.json` 和 `pnpm-lock.yaml` 是否由同一次安装命令产生，并运行 `vp check`；涉及构建链路或运行时代码时额外运行 `vp run build`。

核心运行依赖：

- `vue`
- `vue-router`
- `pinia`
- `axios`
- `@vueuse/core`
- `lodash-es`

核心开发与构建依赖：

- `vite-plus`
- `vite`
- `typescript`
- `sass`
- `unocss`
- `@unocss/reset`
- `@vitejs/plugin-vue`
- `@vitejs/plugin-vue-jsx`
- `unplugin-auto-import`
- `unplugin-vue-components`
- `vitest`
- `@vue/test-utils`
- `@types/lodash-es`

## 真实媒体闭环与管理台（2026-10-04）

- `/admin` 使用黑底、灰阶导航、横向统计、播放会话与任务列表；布局参考 Lux 公开截图，代码独立实现，不引入其源码或素材。只用服务端返回的统计，不模拟 CPU、内存、播放记录。
- `/admin/libraries` 使用 `AdminLibraries`，真实创建库、扫描排队、轮询任务；路径必须是后端机器上的实际路径。扫描需要启用后端 scan worker，探测需要 probe worker。
- 首页与单库页使用 `ServerMediaBrowser`，从用户限定的 Emby Items / Resume 入口读取媒体，支持搜索、分页和直放。旧详情页设计尚未迁移，不能将 TMDB ID 当服务端条目 ID。
- 登录通过 `AuthenticateByName`；未勾选记住设备时会话仅保存在 sessionStorage，勾选后在 localStorage；不保存密码。开发服务器代理 `/api`、`/emby` 和 `/health` 到本机 8080。
- 开始、暂停、播放中每 5 秒、暂停时每 15 秒、关闭和结束均上报真实进度。关闭报告串行排在此前报告之后；错误单独提示，不切换到演示播放。
- 旧本地初始化向导不再自动弹出；管理员在后端通过 bootstrap 环境变量创建。
- 当前 `vp test` 的包别名缺少 bin 入口时，先执行 `vp env doctor`，可用 `vp exec node node_modules/vitest/vitest.mjs run` 执行同一已安装测试引擎，不手改依赖和 lockfile。
- 仓库说明引用的 `ai-skills/antfu/` 当前未随仓库提供；恢复资料前不要假称已读取其中规范。

## 光鸭云盘媒体源（2026-10-04）

- `/admin/storage` 使用 `AdminStorage`，二维码由 `qrcode` 本地生成，不使用第三方二维码服务；账号凭据只保存在后端加密存储。
- 新依赖 `qrcode`、`@types/qrcode` 通过 vp 安装。Vite+ 和 core 的 catalog 锁定已验证的 0.2.1，避免新增依赖时 latest 隐式升级造成原生模块/类型不兼容，仍保持 catalog 引用。
- Storage 管理请求允许 65 秒超时，扫码遵守服务端 interval/slow_down，页面卸载取消本地轮询。
- 云端目录 ID 是挂载身份，路径只显示；未成功读完分页不得解释为文件删除。扫描失败界面保留断点，用户可继续。
- 详细部署、已实现范围与验证边界见 `../docs/GUANGYA_STORAGE_PLUGIN_DESIGN.md` 和 `../fbz-api/plugins/guangya-storage/README.md`。

## daisyUI 与 webplayer 统一改版（2026-10-04）

- 使用 `du-btn`、`du-input`、`du-select`、`du-modal`、`du-tabs`、`du-menu`、`du-alert`、`du-skeleton` 等实际 daisyUI 组件类；共享 `BaseModal` / `BaseSelect` / `BaseEmptyState` / `BaseIcon`。模态弹窗用原生 dialog 管理焦点，手机显示底部面板。
- C 端首页、媒体库及继续观看统一复用 `MediaCard` / `MediaPoster`，真实条目带 serverItem；电影/剧集详情改用 `ServerMediaDetail`，不再把真实 UUID 转为 TMDB 数字 ID。无数据、无搜索结果和加载失败使用不同状态。
- 列表按 Movie/Series 展示；集号、剧名、简介、题材及父级图片由受权限保护的 presentation 接口批量补齐。媒体库计数用专用真实计数，分页下限显示 +，不伪装成精确总页数。
- 播放核心使用 vendored webplayer（MIT 源码，具体上游提交与各 WASM 依赖许可见 vendor/webplayer/NOTICE.md），取代 Shaka。Vite buildStart 自动生成 public/webplayer，产物不提交；vendor 原始代码排除统一格式化和 lint，避免改写上游。
- 播放优先直接读取，CORS 阻止重封装时尝试鉴权同源 Range 入口，最后才用原生兼容路径；同源入口仅支持注册云盘媒体，逐跳检查并固定公网 DNS、限制 32 MiB Range 和 8 路并发、使用背压流，不缓存完整视频。
- 音轨/内嵌字幕选择、倍速、音量、全屏、续播和进度回传沿用同一个播放覆盖层。没有真实轨道时不能展示虚假的音轨选项或假章节。
- UI 图标不使用 emoji；字距、字号、控件圆角均从同一主题体系派生。深浅色共享组件结构，播放器保持适合视频观看的深色覆盖层。

### 后台视觉尺度

后台布局使用 220px 导航与有上限的内容区；导航 13px、正文 13px、页面标题 26px。云盘页面按概览、账号、挂载目录分层，主按钮使用中性高对比表面，绿色用于真实连接状态。首屏加载完成前不得呈现空账号或零计数作为已知结果。

### 光鸭插件与媒体库边界

光鸭入口位于 AdminPlugins 的配置弹窗，旧 /admin/storage 重定向至插件页。AdminStorage 只负责账号、扫码和虚拟挂载，不创建媒体库。AdminLibraries 保留远端本地媒体库编辑器，CloudLibraries 通过独立挂载关联接口创建云盘媒体库，CloudLibraryPolicy 提供实际支持的 NFO 来源、图片缓存、刷新周期。不要恢复演示插件安装/卸载成功反馈。自动刷新由后端持久状态驱动，前端轮询仅显示结果。

## 2026-10-05 远端合并约定

- 保留远端真实插件市场、安装包审核、插件配置与菜单路由；CloudStoragePlugin 是 AdminPlugins 中的存储集成，不替换插件市场。
- 后台沿用 views/admin/\*\* 独立路由及 AdminPageShell，不再使用 account/index.vue 集中切换器。
- 登录以 request.ts 的 fbz_access_token 与 fbz_auth_user_id 为准，旧 fbz_session 仅做一次迁移；退出必须清理旧记录。server.ts 复用该会话，避免两套登录状态分裂。
- App.vue 全局只挂载一个 PlaybackOverlay；默认布局不能重复挂载。电影、分集、首页入口必须保留 source.proxyUri 与 server 播放会话信息。
- TMDB mock 已随远端移除，媒体与详情来自真实 API。保留远端音乐、照片、搜索、设置和插件功能。
- 光鸭迁移重编号为 0097–0099；db/legacy_storage.rs 只对精确匹配的历史校验值做迁移记录兼容，禁止通用关闭 SQLx 校验。
