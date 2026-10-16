---
version: "superdesign-alpha"
name: "FBZ Media Admin Console"
description: "Modern soft-dark Emby-inspired media-server admin — refined green accent, airy spacing, gentle radii and quiet elevation. Still a console, not a landing page."
source_refs:
  - "https://emby.media/ (extract → Quiet Orchard / #52B54B)"
  - "user feedback: Emby direction preferred; soften hardness; more modern"
  - "product IA: FBZ admin (控制面板 / 媒体设置 / 系统设置)"
colors:
  background: "#0e1116"
  background-deep: "#090b0f"
  sidebar: "#12161c"
  surface: "#171c24"
  surface-raised: "#1d2430"
  surface-elevated: "#242c3a"
  border: "#2c3442"
  border-soft: "#222833"
  text-primary: "#f4f6f9"
  text-secondary: "#c0c6d0"
  text-muted: "#8e96a5"
  text-disabled: "#5a6270"
  accent: "#52B54B"
  accent-hover: "#469E40"
  accent-soft: "rgba(82, 181, 75, 0.12)"
  accent-teal: "#3DADB8"
  accent-glow: "rgba(82, 181, 75, 0.22)"
  danger: "#E85D6F"
  warning: "#E8A838"
  success: "#3DBE6A"
  info: "#54C7EC"
typography:
  font-sans: "Inter, ui-sans-serif, system-ui, -apple-system, Segoe UI, Roboto, PingFang SC, Noto Sans SC, sans-serif"
  font-mono: "SFMono-Regular, ui-monospace, Menlo, Consolas, monospace"
  page-title:
    fontSize: "22px"
    fontWeight: 600
    letterSpacing: "-0.025em"
    lineHeight: "1.3"
  section-label:
    fontSize: "12px"
    fontWeight: 600
    letterSpacing: "0.04em"
    textTransform: "none"
    color: "text-muted"
  body:
    fontSize: "13.5px"
    fontWeight: 400
    lineHeight: "1.55"
  metric-value:
    fontSize: "30px"
    fontWeight: 650
    fontVariantNumeric: "tabular-nums"
    lineHeight: "1.1"
  nav-item:
    fontSize: "13px"
    fontWeight: 500
    lineHeight: "1.25"
spacing:
  base: "4px"
  xs: "4px"
  sm: "8px"
  md: "12px"
  lg: "16px"
  xl: "24px"
  2xl: "32px"
  3xl: "40px"
  sidebar-width: "232px"
  topbar-height: "56px"
  content-max: "1280px"
  content-padding: "28px"
  card-gap: "16px"
rounded:
  control: "10px"
  card: "14px"
  panel: "16px"
  pill: "999px"
shadows:
  card: "0 1px 0 rgba(255,255,255,0.04) inset, 0 10px 28px rgba(0,0,0,0.28)"
  soft: "0 4px 16px rgba(0,0,0,0.18)"
  focus: "0 0 0 3px rgba(82, 181, 75, 0.28)"
components:
  sidebar:
    width: "232px"
    background: "#12161c"
    border-right: "1px solid #222833"
    nav-item-height: "38px"
    nav-item-radius: "10px"
    nav-item-active-bg: "accent-soft"
    nav-item-active-indicator: "soft pill highlight, NOT a hard 3px bar"
  topbar:
    height: "56px"
    background: "transparent / subtle blur over canvas"
    border-bottom: "1px solid #222833"
    title: "single page title — no duplicate H1 banner"
  metric-card:
    background: "#171c24"
    border: "1px solid #2c3442"
    radius: "14px"
    padding: "18px 20px"
    shadow: "soft"
  button-primary:
    height: "36px"
    radius: "10px"
    padding: "0 16px"
    fontSize: "13px"
    fontWeight: 600
  button-ghost:
    height: "36px"
    radius: "10px"
    background: "transparent"
    border: "1px solid #2c3442"
  badge:
    height: "22px"
    radius: "999px"
    fontSize: "11.5px"
    fontWeight: 600
---

# FBZ Media Admin Console (Modern Soft Emby)

Product: **FBZ** — self-hosted Emby-compatible media server. Target: **admin dashboard** (`/admin`).

User direction (locked): keep **Emby-inspired green**, but make it **softer and more modern** — less rigid, less “hard console”, more polished product UI while remaining an ops dashboard.

## Visual direction

- Dark charcoal canvas with gentle contrast steps (not harsh black slabs).
- Emby green `#52B54B` used sparingly: primary buttons, active nav soft fill, status success, focus rings.
- **Softer geometry**: card radius 14–16px, controls 10px — no razor-sharp 4–6px boxes.
- **Quieter chrome**: hairline borders, soft shadows, optional subtle blur on topbar; avoid thick left accent bars and military density.
- **More air**: content padding ~28px, card gap 16px, metric cards breathe; still dense enough for ops, not a marketing hero.
- Section labels: sentence case / muted 12px — avoid shouty uppercase tracking.
- Icons: Lucide-style stroke only; no emoji CTAs.

## Composition (Admin Dashboard)

1. **Left sidebar (~232px)** — brand lockup → grouped nav (控制面板 / 个人偏好 / 媒体设置 / 系统设置) → plugins →「返回前台」→ theme + version.
2. **Top bar (~56px)** — one page title, theme toggle, user chip.
3. **Main** — max-width ~1280px:
   - Soft metric cards (Jobs / 计划任务 / 媒体库)
   - Mid: 媒体统计 + health
   - Lower: 任务 / 最近活动 with rounded list panels

## Guardrails

- Keep FBZ IA labels in Chinese.
- One page title only.
- Primary accent = Emby green only (no Jellyfin cyan mix, no Spotify `#1ed760`).
- Soft ≠ glassy candy: no heavy glassmorphism, no neon gradients, no display fonts.
- Still a media **admin console**, not a landing page.
- Use ONLY fonts, colors, spacing, and component styles defined in this design system.

## Iteration note (active)

- Prefer open layout: minimize borders and discrete cards; use surface tone shifts and sparse dividers.
- Metric areas: one or two panels max, not a grid of many bordered cards.
