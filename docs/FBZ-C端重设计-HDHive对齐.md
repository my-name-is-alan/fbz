# FBZ C-end 重设计 — HDHive 视觉对齐

**状态**: Superdesign 设计稿阶段（未改 Vue）  
**日期**: 2026-10-15 PT  
**决策**: daisyUI + Lucide（`@lucide/vue`）；视觉语言对齐 HDHive-web；不做巨大圆角 / 不做 emoji 图标。

## 背景

当前 C 端前端被认为已毁：过大圆角、emoji 图标、布局破碎。用户要求 **C 端全量 UI rewrite**。Admin 视觉此前已冻结——本轮优先只动 C 端；共享 token 仅在不可避免时轻触。

## 参考与资产

| 来源 | 路径 |
|------|------|
| HDHive tokens（权威） | `docs/hdhive-tokens.css`（自 `/workspace/HDHive-web/src/ui/theme/tokens.css` 复制） |
| FE 工程 | `/Users/edy/Code/fbz/fbz-fe`（Vue3 + daisyUI + Lucide） |
| Dev | `http://127.0.0.1:5173`，API `8080` |
| Box 副本 | `/workspace/fbz-docs/` |

## 目标视觉语言（HDHive dark）

- **背景** `#09090e`；surface `#111119` / raised `#171722` / hover `#20202c`
- **边框** `#282838` / strong `#3a3a50`
- **文字** `#eeeef6` / muted `#9c9cb2` / subtle `#707085`
- **强调色（amber）** `#d08700` → hover `#f0b100`；accent contrast `#0a0a0a`
- **圆角** sm `7px` / md `11px` / lg `16px`（禁止 blob 超大圆角）
- **字体** Noto Sans SC / PingFang SC 系；无装饰 serif
- **图标** 仅 Lucide 矢量线标；**禁止 emoji**
- **气质** 密集媒体工作站（Emby/Jellyfin/HDHive 信息密度），非气泡/营销落地页
- **组件** daisyUI 语义可映射（btn / card / input / modal / drawer），token 走 CSS 变量

## 本轮设计范围（C 端）

1. **Home** — 顶栏 + 英雄续看 + 横向媒体行
2. **Library grid** — 筛选条 + 海报网格
3. **Movie detail** — 海报/backdrop 英雄区 + 元数据 + 操作 + 相似
4. **Player overlay** — 全屏播放控件叠层
5. **Playlists** — 列表/网格 + 空态

**不在范围**: Admin UI 视觉改版、auth/gateway、提交 git commit。

## Superdesign 门禁

- 必须先在 Superdesign 出稿；**用户批准或明确说 skip design** 后才改 Vue。
- Init：`.superdesign/init/` 六文件已齐；C 端 design-system 已切到 HDHive（admin 备份见 `design-system-admin-backup.md`）。

## 实现约束（批准后）

- 只改 `fbz-fe` C 端（`layouts/default`、`views/{home,library,detail,playlists}`、`components/{layout,media,detail,playback,home}` 等）
- 不改 auth / gateway
- 无 commit（除非用户另下指令）

## 请用户批准的事项

1. 是否认可 HDHive dark + amber + 中等圆角（7/11/16）作为 C 端唯一视觉方向？
2. 五张画布稿（Home / Library / Movie detail / Player / Playlists）选哪套为主落地？
3. 批准后是否直接按稿改 Vue（仍不碰 admin）？


## Superdesign 产出（2026-10-15 PT）

- **Project**: FBZ C-end Redesign (HDHive)
- **Canvas (live)**: https://superdesign.dev/teams/62d6898e-5c68-4a83-a81f-e51eb60fbb7a/projects/8d6d84eb-a0b9-451a-9a55-c2d28bb14cdc?live=1
- **Project id**: `8d6d84eb-a0b9-451a-9a55-c2d28bb14cdc`
- **Model**: gemini-3.1-pro

| Screen | Draft id | Preview |
|--------|----------|---------|
| Home | `1491d4f1-787e-431f-b9d2-4dd29beaa775` | https://p.superdesign.dev/draft/1491d4f1-787e-431f-b9d2-4dd29beaa775 |
| Library grid | `24e35234-24eb-4b82-a895-5c60f05049c1` | https://p.superdesign.dev/draft/24e35234-24eb-4b82-a895-5c60f05049c1 |
| Movie detail | `fe3573d2-4281-449d-8bf5-6919bc38adc1` | https://p.superdesign.dev/draft/fe3573d2-4281-449d-8bf5-6919bc38adc1 |
| Player overlay | `5b9a8ede-0208-4f97-b84a-767e2aa7d483` | https://p.superdesign.dev/draft/5b9a8ede-0208-4f97-b84a-767e2aa7d483 |
| Playlists | `6bffef99-f1ba-43ba-8e4f-ea0306eae824` | https://p.superdesign.dev/draft/6bffef99-f1ba-43ba-8e4f-ea0306eae824 |

**未改任何 Vue 组件。** 等待用户批准后再实现。
