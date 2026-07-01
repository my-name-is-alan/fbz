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
- 全局壳层布局是 `src/layouts/default.vue`：`AppHeader` + `AppDrawer` + `RouterView`，不做卡片跨路由飞渡或页面切换动画，保证媒体网格进入详情时足够轻量。`App.vue` 只渲染该布局。
- 媒体卡片统一用 `MediaCard`：卡片主体进入详情，海报上的播放按钮打开全屏播放覆盖层，不走播放路由。`MediaItem.detailType`（movie/tv）决定详情与播放类型，与 libraryId 解耦（动漫=tv、纪录片=movie）。
- 路由必须与文件目录对应，例如 `/user/login` 对应 `src/views/user/login/index.vue`；动态路由如 `/library/:id` 放在 `src/views/library/detail/index.vue`（不使用 `[id].vue` 文件名约定，路由表在 `src/router/index.ts` 手动维护）。
- 详情页按类型分路径：`/movie/:id`、`/tv/:id`、`/person/:id`、`/collection/:id`，分别对应 `src/views/detail/{movie,tv,person,collection}/index.vue`。系列（collection）是一等概念，影片详情会链接到其所属系列。详情区块组件在 `src/components/detail/`：`DetailHero`（poster+fanart 头部 + 多版本下拉）、`CastRow`（演职员，左右箭头滚动）、`SeasonEpisodes`（季/集，默认定位到「继续观看」的季集）、`SimilarRow`（相似推荐）。
- 网络请求统一走 `src/service/request.ts` 导出的 `request` 单例；新增**真实接口**模块放在 `src/service/modules/`，**设计态 mock**（TMDB 占位数据等）放在 `src/service/mock/`，两者用目录边界分开。无 TMDB 数据的库（如音乐）的占位数据在 `src/service/mock/media.ts`，接后端后由 `service/modules` 返回即可。本应用是封闭单实例部署，所有请求同源（`request` 固定 `/api`、`embyRequest` 固定根路径），**不支持用户填写服务器地址**；构建期可用 `VITE_API_BASE_URL` 指定后端地址。
- TMDB 真实数据：`scripts/fetch-tmdb.mjs` 一次性抓取（discover 多页 + 详情/相似/季 + 系列 + 高频演员），烤成两个文件：`src/service/mock/tmdb-catalog.json`（轻量目录，约数百条，随包加载，用于首页/媒体库网格）与 `src/service/mock/tmdb-details.json`（完整详情，体积大，**在 `src/service/mock/tmdb.ts` 里用动态 `import()` 懒加载**，只在详情页下载一次）。`tmdb.ts` 提供 `imageUrl()`、`catalogItems`、`itemsByLibrary()`、`getXxxDetail()` 异步取详情、`versionsFor()`（合成播放版本/规格/字幕，TMDB 不提供）等。libraryId（movie/series/anime/documentary）= 归属库，type（movie/tv）= 详情路由类型，二者解耦。**token 只在抓取脚本里用（读 `.env` 的 `api_token`，注意值有行尾中文注释，解析只取引号内），绝不进前端构建包**；图片走公开 CDN `image.tmdb.org`（无需 token）。重新抓取改脚本里的 discover 页数/题材后跑 `node scripts/fetch-tmdb.mjs`。接后端后把 `src/service/mock/tmdb.ts` 的函数换成对 fbz-api 的请求即可，页面消费方不变。
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

- UnoCSS 是首选原子化样式层，优先使用主题 token，不要随意写散落的原始值。
- 项目源码样式强制使用 `SCSS`：全局样式使用 `.scss`，SFC 样式块必须写 `<style lang="scss">` 或 `<style scoped lang="scss">`。
- 不要新增项目内 `.css` 文件；第三方库如确实只有官方 CSS 入口，可按库要求引入。
- `src/styles/theme/` 是主题 token 主目录。
- `src/styles/theme/tokens.scss` 必须通过 `vite.config.ts` 的 `css.preprocessorOptions.scss.additionalData` 注入到每个 SCSS 文件顶部，不要在业务样式文件里逐个手动 `@use` / `@import` token。
- `uno.config.ts` 中的主题 token 必须与 `src/styles/theme/tokens.scss` 保持同步，优先引用 token 暴露的 CSS 变量，例如 `var(--fbz-color-brand-500)`。
- 全局基础样式集中在 `src/style.scss`（含 `--header-h` 头部高度变量，桌面 60px / 手机 56px，布局与各页面顶部留白统一引用它）。
- 字体：正文用 `--fbz-font-sans`（系统优先现代字栈 `ui-sans-serif, system-ui, PingFang SC, MiSans…`），品牌字号/数字展示用 `--fbz-font-display`（Orbitron，`index.html` 里走 Google Fonts，已在 `tokens.scss` 定义 token）。新增展示型数字/Logo 用 display 字体，正文不要硬写字栈。
- 设计基调：纯黑底（`--fbz-color-bg: #0a0a0b`）+ 单一主题色 `--fbz-color-brand-500: #1ed760`（Spotify 绿）。主题绿只用于强调态（导航激活、主按钮、进度条、卡片 hover 边框），其余一律白/灰阶；禁止多彩混用、装饰性渐变、滥用大圆角（卡片 4px / 控件 6px）。**例外**：媒体卡片的清晰度徽章用 `tmdb.ts` 的 `resolutionColors`（4K 绿 / 2K 黄绿 / 1080P 蓝 / 720P 橙，借鉴 ），这是功能性色标不算多彩装饰。
- 横向滚动行一律用 `src/components/BaseScroller.vue`：隐藏原生横向滚动条（不要再出现裸露的横向滚动条），用 vueuse（`useEventListener`+`useResizeObserver`）按需在行首/行尾浮出**半透明渐变遮罩 + 居中 SVG 箭头**，仅在该方向还有内容可滚时显示，触摸设备隐藏。每列宽度由使用方通过 `:deep(.track) { --col: … }` 覆盖。`MediaRow`/`SimilarRow`/`CastRow` 均基于它。
- 下拉选择一律用 `src/components/BaseSelect.vue`（自定义下拉，**不要用原生 `<select>`**）：`v-model` 绑值，`options` 为 `{ label, value }[]`，自带面板样式/选中态/键盘与点击外部关闭。版本选择、季选择、题材筛选均已用它。
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
