---

version: "superdesign-alpha"
name: "FBZ C-end Media Client (HDHive-aligned)"
description: "Dense dark media workstation for FBZ consumer UI. Visual DNA from HDHive-web: near-black canvas, amber accent, medium radii 7/11/16, Lucide icons only. daisyUI-compatible. Not bubbly, not emoji, not marketing landing."
source_refs:

- "HDHive-web tokens.css — dark #09090e, amber #d08700/#f0b100, radius-sm 7 / md 11 / lg 16"
- "User order 2026-10-15: full C-end rewrite; daisyUI + Lucide; align to HDHive"
- "Product: FBZ media client (Home / Library / Detail / Player / Playlists)"
  colors:
  background: "#09090e"
  surface: "#111119"
  surface-raised: "#171722"
  surface-hover: "#20202c"
  input: "#0d0d14"
  border: "#282838"
  border-strong: "#3a3a50"
  text: "#eeeef6"
  text-muted: "#9c9cb2"
  text-subtle: "#707085"
  accent: "#d08700"
  accent-hover: "#f0b100"
  accent-channel: "208 135 0"
  accent-contrast: "#0a0a0a"
  danger: "#fb7185"
  success: "#34d399"
  warning: "#fbbf24"
  overlay: "rgb(2 3 8 / 72%)"
  glass: "rgb(17 17 25 / 82%)"
  typography:
  font-sans: '"Noto Sans SC", "PingFang SC", "Hiragino Sans GB", "Microsoft YaHei", ui-sans-serif, system-ui, sans-serif'
  font-mono: '"Roboto Mono", ui-monospace, Menlo, monospace'
  page-title: { fontSize: "22px", fontWeight: 600, letterSpacing: "-0.02em" }
  section-title: { fontSize: "15px", fontWeight: 600, letterSpacing: "-0.01em" }
  body: { fontSize: "13.5px", fontWeight: 400, lineHeight: "1.5" }
  meta: { fontSize: "12px", fontWeight: 500, color: "text-muted" }
  button: { fontSize: "13px", fontWeight: 500 }
  spacing:
  base: "4px"
  gutter: "24px"
  content-max: "1280px"
  shell-max: "1536px"
  header-height: "64px"
  radius:
  sm: "7px"
  md: "11px"
  lg: "16px"

# NEVER use 24px+ blob pills for cards/panels

shadows:
surface: "0 16px 42px rgb(0 0 0 / 24%)"
float: "0 24px 70px rgb(0 0 0 / 42%)"
dialog: "0 28px 90px rgb(0 0 0 / 52%)"
accent-btn: "0 9px 24px rgb(208 135 0 / 18%)"
icons:
library: "Lucide only (@lucide/vue)"
style: "1.5–2px stroke vector icons, 16–20px in chrome, 24px in empty states"
forbidden: "emoji, cartoon glyphs, invent SVG logos"
components:
shell: - "Top AppHeader 64px: logo wordmark FBZ, library nav chips, search, user avatar" - "Optional left drawer for libraries on compact widths" - "Main canvas #09090e with subtle amber radial wash at top (10% channel)"
home: - "Hero continue-watching banner (16:9 backdrop, title, meta, Play + More)" - "Horizontal MediaRows: Continue / Recently Added / library shelves" - "Poster cards ~2:3, radius-md, hover raise + amber focus ring"
library: - "Toolbar: title, sort, filter chips, view toggle grid" - "Dense poster grid, consistent gutters, no oversized padding"
movie-detail: - "Backdrop hero with gradient scrim; poster left; title/year/runtime/rating" - "Primary Play (amber), secondary Add to playlist / Favorite / More" - "Synopsis, cast row, similar row"
player: - "Fullscreen dark overlay; bottom control bar glass; Lucide play/pause/volume/settings" - "Seek bar amber progress; title + remaining time; no emoji"
playlists: - "Page header + Create playlist (amber)" - "Grid or list of playlist cards with cover stack, title, item count" - "Empty state with Lucide list-music icon + short CTA"
daisyui_mapping:
primary: "accent #d08700"
base-100: "#09090e"
base-200: "#111119"
base-300: "#171722"
rounded-btn: "11px"
rounded-box: "16px"
rounded-badge: "7px"
motion:
fast: "160ms"
base: "220ms"
ease: "cubic-bezier(0.22, 1, 0.36, 1)"
constraints:

- "Dense media workstation — not bubbly consumer social UI"
- "Medium radii only (7/11/16)"
- "Lucide icons only; zero emoji"
- "Keep admin UI visually frozen; this system is C-end only"
- "Use ONLY fonts/colors/radii defined here; no serif display, no neon purple/pink"
  product_context:
  jtbd: "Browse personal media libraries, open titles, play video, manage playlists"
  key_pages: ["Home", "Library grid", "Movie detail", "Player overlay", "Playlists"]
  stack: "Vue 3 + daisyUI 5 + Tailwind 4 + @lucide/vue"
