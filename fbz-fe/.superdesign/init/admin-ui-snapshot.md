# 当前后台 UI 现状摘要

> 基于 `src/layouts/admin.vue`、`src/components/admin/*`、`src/views/admin/*` 源码阅读（2026-10-15）。

## 布局骨架

- **整体**：左栏 + 右主区的经典控制台。桌面端左侧 sticky 侧栏宽 **220px**；主区顶栏高 **52px**，内容区 `padding: var(--fbz-space-5)`，页面内容再被 `AdminPageShell` 限宽 **1200px** 居中。
- **侧栏**：品牌区「FBZ / 系统控制台」→ 分组导航（控制面板 / 个人偏好 / 媒体设置 / 系统设置）→ 动态「插件菜单」→ 分隔线 →「返回前台首页」→ 底部主题切换 + 版本点。菜单项为手写 stroke SVG + 小号字（12px / 加粗）。
- **顶栏**：汉堡（≤768px）+ 当前页标题（由路由名反查）+ 主题按钮 + 头像/昵称徽章（链到 `/admin/profile`）。
- **移动端**：侧栏隐藏，左侧滑出 260px drawer + 半透明遮罩；桌面/移动导航模板基本复制了一份。
- **页面套路**：几乎所有 `views/admin/*/index.vue` 只包一层 `AdminPageShell(title, description)` + 对应 `AdminXxx` 业务组件。

## 配色与视觉语言

- 默认 **深色影院控制台**：背景 `#0c0e11`，面板 `#15181d` / `#1d2127`，细线边框，品牌高亮 **Spotify 绿 `#1ed760`**。
- 亮色主题通过 `:root[data-theme="light"]` 翻转表面色；品牌色可在主题页自定义并写回 CSS 变量。
- 控件混用：**自研 SCSS**（metric-card、settings-card、task-row）+ **daisyUI `du-*`**（btn / input / select / modal / alert）+ 少量 Uno 工具类；圆角在 tokens（4/6）与 `style.scss` 覆盖（12/10）之间不一致。
- 字体：系统无衬线；展示体声明了 Orbitron，但全局又被覆写成 sans，品牌字母间距偏「科技感」但实际字重混杂。

## 典型页面模式

| 页面                 | 模式                                                                     |
| -------------------- | ------------------------------------------------------------------------ |
| 控制面板             | 指标卡网格 + 中部统计/流媒体 + 任务列表 + 最近活动                       |
| 用户                 | 提示条 + 主按钮 → 用户卡片网格（头像/角色徽章/权限事实/操作）            |
| 媒体库               | 云库区块 + 本地库预览卡（色条图标 + 路径）+ 虚线「添加」卡               |
| 插件                 | 摘要卡 + 子 Tab（状态/包/Dispatch）+ 插件卡；内嵌云存储面板              |
| 转码 / 主题 / 元数据 | `settings-stack` + 左侧绿色 indicator 的 settings-card + BaseSelect/开关 |
| 任务计划             | 摘要卡 + 左列表/右详情双栏                                               |
| 日志                 | 仿终端工具条（级别过滤/搜索/自动滚）+ 等宽日志行                         |

## 丑点 / 问题（设计债）

1. **双标题冗余**：顶栏已有 `page-title`，`AdminPageShell` 再渲染一遍大号 `h1` + description，信息重复、首屏浪费垂直空间。
2. **侧栏与路由不同步**：`/admin/photos` 有路由无侧栏入口；`/admin/storage` 仅 redirect 到 plugins，但存储 UI 嵌在插件页里，心智模型绕。
3. **样式体系分裂**：同一后台里同时出现 emoji 按钮（「➕ 添加系统用户」）、手写 SVG、BaseIcon(Lucide)、daisyUI 按钮多种视觉，密度/字号/圆角不统一。
4. **组件膨胀与复制**：`AdminPlugins.vue` ~1300 行、`AdminStorage.vue` ~950 行；桌面/移动侧栏导航几乎整段复制，后续改菜单成本高。
5. **摘要卡/设置卡未抽原语**：dashboard / plugins / scheduled-tasks 的 summary-card、transcode/theme 的 settings-card 模式重复，却各自写 scoped SCSS。
6. **空态与错误态不统一**：有的用 `BaseEmptyState`，有的用本地 `empty-state` 文案块，有的只是一行 `error-text` / `ops-error`。
7. **英文/中英混排**：如「暗黑模式 (Dark Mode)」「Dispatch」「queued/running/failed」直接进 UI，运营感偏工程后台而非产品控制台。
8. **间距与触控**：侧栏菜单 padding 偏紧（7×10），顶栏 52px 与前台 header 72px 节奏不一；部分事实标签（用户卡）一行塞过多 chip，小屏易挤。
9. **品牌指示器过炫**：footer 绿色 status-dot 带 glow、多处 `color-mix` 品牌 10% 底，深色下尚可，浅色主题对比与焦点环一致性一般。
10. **设计系统文档与实现落差**：`docs/frontend-design-system.md` 仍提 `src/components/base`、BaseButton/BaseTable 等，实际只有少量 Base\*，后台大量未沉淀的页内样式。

## 改造时可优先动刀的面

- 统一 Admin shell：侧栏信息架构、单标题策略、摘要卡/设置卡/空态原语。
- 存储与插件入口产品化（明确「云存储」导航，而不是藏在 plugins 子面板）。
- 收敛按钮/图标/徽章到 daisyUI + BaseIcon，去掉 emoji 主按钮。
- 把 `AdminPageShell` 与顶栏标题合并，把 description 收到页内或折叠提示。
