# Theme tokens — FBZ FE

## Part 1 — Compact token summary (C-end TARGET = HDHive-aligned)

> Implementation target for C-end rewrite. Source: `docs/hdhive-tokens.css` / HDHive-web `tokens.css`.
> Current runtime SCSS (`src/styles/theme/tokens.scss`) still has legacy green brand — do not treat as visual target.

| Token               | Value                            |
| ------------------- | -------------------------------- |
| background          | `#09090e`                        |
| surface             | `#111119`                        |
| surface-raised      | `#171722`                        |
| surface-hover       | `#20202c`                        |
| input               | `#0d0d14`                        |
| border              | `#282838`                        |
| border-strong       | `#3a3a50`                        |
| text                | `#eeeef6`                        |
| text-muted          | `#9c9cb2`                        |
| text-subtle         | `#707085`                        |
| accent              | `#d08700`                        |
| accent-hover        | `#f0b100`                        |
| accent-contrast     | `#0a0a0a`                        |
| danger              | `#fb7185`                        |
| success             | `#34d399`                        |
| warning             | `#fbbf24`                        |
| radius-sm / md / lg | `7px` / `11px` / `16px`          |
| header-height       | `64px`                           |
| content-width       | `1280px`                         |
| font-sans           | Noto Sans SC / PingFang SC stack |
| icons               | Lucide only — no emoji           |

Stack: Vue 3, daisyUI 5, Tailwind 4, `@lucide/vue`.

---

## Legacy / prior init content (may include old admin/green notes)

# Theme

## Part 1 — Compact token summary

### Stack

- Tokens: `src/styles/theme/tokens.scss` → `--fbz-*` CSS vars via `@mixin fbz-theme-vars`

- Global: `src/style.scss` (imports tokens, scrollbar, form-group, daisyUI bridging)

- Daisy entry: `src/styles/daisy.css` (Tailwind 4 layers + daisyUI plugin `prefix: du-`, themes:false)

- Atomic: `uno.config.ts` maps brand/surface/line/content/radius/spacing/shadow to `--fbz-*`

- Runtime theme: Pinia `stores/theme.ts` toggles `data-theme=light|dark` + brand color override

### Colors (dark default / light override)

| Token                        | Dark                      | Light     |
| ---------------------------- | ------------------------- | --------- |
| `--fbz-color-bg`             | `#0c0e11`                 | `#f5f5f7` |
| `--fbz-color-bg-strong`      | `#050506`                 | `#ffffff` |
| `--fbz-color-panel`          | `#15181d`                 | `#ffffff` |
| `--fbz-color-panel-strong`   | `#1d2127`                 | `#e9ecef` |
| `--fbz-color-panel-elevated` | `#262c34`                 | `#dee2e6` |
| `--fbz-color-line`           | `#2b3038`                 | `#dee2e6` |
| `--fbz-color-line-bright`    | `#3a3a40`                 | `#ced4da` |
| `--fbz-color-line-soft`      | `#20242b`                 | `#e5e8ec` |
| `--fbz-color-text`           | `#f3f3f4`                 | `#1c1c1e` |
| `--fbz-color-text-soft`      | `#b2b2b8`                 | `#48484a` |
| `--fbz-color-text-muted`     | `#9299a4`                 | `#69737f` |
| `--fbz-color-text-disabled`  | `#4a4a52`                 | `#aeaeb2` |
| `--fbz-color-brand-500`      | `#1ed760` (Spotify-green) | same      |
| `--fbz-color-brand-600`      | mix 84% black             | same      |
| `--fbz-color-cyan-500`       | `#33d6ff`                 | same      |
| `--fbz-color-amber-500`      | `#ffb82e`                 | same      |
| `--fbz-color-danger-500`     | `#ff5f79`                 | same      |

### Typography

- Sans: system UI stack (Segoe / PingFang / HarmonyOS / MiSans / YaHei…)

- Display: Orbitron (overridden later in `style.scss` to fall back to sans)

- Scale: xs 11 / sm 12 / md 14 / lg 18 / xl 24

- Body: 14px, line-height 1.55, tnum features

### Spacing / radius / motion / shadow

- Space: 4 / 8 / 12 / 16 / 20 / 24 / 32 (`--fbz-space-1…8`)

- Radius tokens: card 4, control 6, hero 10, round 999 — **overridden in style.scss** to card 12 / control 10 for daisy alignment

- Motion: fast 120ms / base 180ms / slow 320ms

- Shadow: panel `0 20px 50px rgba(0,0,0,.5)` (light: softer), focus ring via brand mix

- Z: sticky 20, overlay 50; header height `--header-h` 72→62 mobile

### daisyUI bridge (`:root`)

- `--color-primary` / accent / success → brand-500; error → danger; base-100/200/300 → bg/panel/panel-strong

- Classes used heavily in admin: `du-btn`, `du-btn-sm`, `du-btn-primary`, `du-btn-ghost`, `du-input`, `du-select`, `du-modal`, `du-alert`, `du-badge`

### Breakpoints (observed in layouts)

- 600px: front-end header collapses to hamburger

- 768px: admin sidebar → mobile drawer

---

## Part 2 — Raw source dumps

### `uno.config.ts`

```ts
import { defineConfig, presetWind3 } from "unocss";

export default defineConfig({
  presets: [presetWind3()],
  theme: {
    colors: {
      brand: {
        DEFAULT: "var(--fbz-color-brand-500)",
        500: "var(--fbz-color-brand-500)",
        600: "var(--fbz-color-brand-600)",
      },
      cyan: {
        500: "var(--fbz-color-cyan-500)",
      },
      success: {
        500: "var(--fbz-color-green-500)",
      },
      warning: {
        500: "var(--fbz-color-amber-500)",
      },
      danger: {
        500: "var(--fbz-color-danger-500)",
      },
      surface: {
        base: "var(--fbz-color-bg)",
        strongest: "var(--fbz-color-bg-strong)",
        panel: "var(--fbz-color-panel)",
        strong: "var(--fbz-color-panel-strong)",
        elevated: "var(--fbz-color-panel-elevated)",
      },
      line: {
        DEFAULT: "var(--fbz-color-line)",
        bright: "var(--fbz-color-line-bright)",
        soft: "var(--fbz-color-line-soft)",
      },
      content: {
        DEFAULT: "var(--fbz-color-text)",
        soft: "var(--fbz-color-text-soft)",
        muted: "var(--fbz-color-text-muted)",
      },
    },
    borderRadius: {
      card: "var(--fbz-radius-card)",
      control: "var(--fbz-radius-control)",
      round: "var(--fbz-radius-round)",
    },
    spacing: {
      1: "var(--fbz-space-1)",
      2: "var(--fbz-space-2)",
      3: "var(--fbz-space-3)",
      4: "var(--fbz-space-4)",
      5: "var(--fbz-space-5)",
      6: "var(--fbz-space-6)",
      8: "var(--fbz-space-8)",
    },
    boxShadow: {
      panel: "var(--fbz-shadow-panel)",
      focus: "var(--fbz-shadow-focus)",
    },
  },
});
```

### `src/styles/theme/tokens.scss`

```scss
$fbz-color-bg: #0c0e11;
$fbz-color-bg-strong: #050506;
$fbz-color-panel: #15181d;
$fbz-color-panel-strong: #1d2127;
$fbz-color-panel-elevated: #262c34;
$fbz-color-line: #2b3038;
$fbz-color-line-bright: #3a3a40;
$fbz-color-line-soft: #20242b;
$fbz-color-text: #f3f3f4;
$fbz-color-text-soft: #b2b2b8;
$fbz-color-text-muted: #9299a4;
$fbz-color-text-disabled: #4a4a52;
$fbz-color-brand-500: #1ed760;
$fbz-color-brand-600: #1bc356;
$fbz-color-cyan-500: #33d6ff;
$fbz-color-green-500: #1ed760;
$fbz-color-amber-500: #ffb82e;
$fbz-color-danger-500: #ff5f79;
$fbz-color-focus: #2ee06b;
$fbz-space-1: 4px;
$fbz-space-2: 8px;
$fbz-space-3: 12px;
$fbz-space-4: 16px;
$fbz-space-5: 20px;
$fbz-space-6: 24px;
$fbz-space-8: 32px;
$fbz-font-size-xs: 11px;
$fbz-font-size-sm: 12px;
$fbz-font-size-md: 14px;
$fbz-font-size-lg: 18px;
$fbz-font-size-xl: 24px;
$fbz-radius-card: 4px;
$fbz-radius-control: 6px;
$fbz-radius-hero: 10px;
$fbz-radius-round: 999px;
$fbz-motion-fast: 120ms;
$fbz-motion-base: 180ms;
$fbz-motion-slow: 320ms;
$fbz-z-sticky: 20;
$fbz-z-overlay: 50;

@mixin fbz-theme-vars {
  :root {
    --fbz-color-bg: #{$fbz-color-bg};
    --fbz-color-bg-strong: #{$fbz-color-bg-strong};
    --fbz-color-panel: #{$fbz-color-panel};
    --fbz-color-panel-strong: #{$fbz-color-panel-strong};
    --fbz-color-panel-elevated: #{$fbz-color-panel-elevated};
    --fbz-color-line: #{$fbz-color-line};
    --fbz-color-line-bright: #{$fbz-color-line-bright};
    --fbz-color-line-soft: #{$fbz-color-line-soft};
    --fbz-color-text: #{$fbz-color-text};
    --fbz-color-text-soft: #{$fbz-color-text-soft};
    --fbz-color-text-muted: #{$fbz-color-text-muted};
    --fbz-color-text-disabled: #{$fbz-color-text-disabled};
    --fbz-color-brand-500: #{$fbz-color-brand-500};
    --fbz-color-brand-600: color-mix(in srgb, var(--fbz-color-brand-500) 84%, black);
    --fbz-color-cyan-500: #{$fbz-color-cyan-500};
    --fbz-color-green-500: var(--fbz-color-brand-500);
    --fbz-color-amber-500: #{$fbz-color-amber-500};
    --fbz-color-danger-500: #{$fbz-color-danger-500};
    --fbz-color-focus: color-mix(in srgb, var(--fbz-color-brand-500) 30%, transparent);
    --fbz-space-1: #{$fbz-space-1};
    --fbz-space-2: #{$fbz-space-2};
    --fbz-space-3: #{$fbz-space-3};
    --fbz-space-4: #{$fbz-space-4};
    --fbz-space-5: #{$fbz-space-5};
    --fbz-space-6: #{$fbz-space-6};
    --fbz-space-8: #{$fbz-space-8};
    --fbz-font-size-xs: #{$fbz-font-size-xs};
    --fbz-font-size-sm: #{$fbz-font-size-sm};
    --fbz-font-size-md: #{$fbz-font-size-md};
    --fbz-font-size-lg: #{$fbz-font-size-lg};
    --fbz-font-size-xl: #{$fbz-font-size-xl};
    --fbz-radius-card: #{$fbz-radius-card};
    --fbz-radius-control: #{$fbz-radius-control};
    --fbz-radius-hero: #{$fbz-radius-hero};
    --fbz-radius-round: #{$fbz-radius-round};
    --fbz-shadow-panel: 0 20px 50px rgba(0, 0, 0, 0.5);
    --fbz-shadow-focus: 0 0 0 3px color-mix(in srgb, var(--fbz-color-brand-500) 30%, transparent);
    --fbz-color-panel-transparent: rgba(20, 20, 22, 0);
    // 字体：正文走系统优先的现代字栈（借鉴 HDHive），数字/品牌用 Orbitron 展示体
    --fbz-font-sans:
      ui-sans-serif, system-ui, -apple-system, "Segoe UI", Roboto, "PingFang SC",
      "HarmonyOS Sans SC", "MiSans", "Microsoft YaHei", "Noto Sans SC", Helvetica, Arial, sans-serif;
    --fbz-font-display: "Orbitron", var(--fbz-font-sans);
    --fbz-motion-fast: #{$fbz-motion-fast};
    --fbz-motion-base: #{$fbz-motion-base};
    --fbz-motion-slow: #{$fbz-motion-slow};
    --fbz-z-sticky: #{$fbz-z-sticky};
    --fbz-z-overlay: #{$fbz-z-overlay};
  }

  :root[data-theme="light"] {
    --fbz-color-bg: #f5f5f7;
    --fbz-color-bg-strong: #ffffff;
    --fbz-color-panel: #ffffff;
    --fbz-color-panel-strong: #e9ecef;
    --fbz-color-panel-elevated: #dee2e6;
    --fbz-color-line: #dee2e6;
    --fbz-color-line-bright: #ced4da;
    --fbz-color-line-soft: #e5e8ec;
    --fbz-color-text: #1c1c1e;
    --fbz-color-text-soft: #48484a;
    --fbz-color-text-muted: #69737f;
    --fbz-color-text-disabled: #aeaeb2;
    --fbz-color-panel-transparent: rgba(255, 255, 255, 0);
    --fbz-shadow-panel: 0 10px 30px rgba(0, 0, 0, 0.06), 0 1px 3px rgba(0, 0, 0, 0.02);
  }
}
```

### `src/styles/daisy.css`

```css
@layer theme, base, daisyui, components, utilities;
@import "@unocss/reset/tailwind.css" layer(base);
@import "tailwindcss/theme.css" layer(theme);
@import "tailwindcss/utilities.css" layer(utilities);
@plugin "daisyui" {
  themes: false;
  prefix: "du-";
  logs: false;
}
```

### `src/style.scss`

```scss
@include fbz-theme-vars;

* {
  box-sizing: border-box;
}

html {
  // 永久预留滚动条槽位：避免不同页面高度导致路由切换时滚动条出现/消失、宽度抖动闪烁
  scrollbar-gutter: stable;
}

:root {
  color-scheme: dark;
  font-family: var(--fbz-font-sans);
  font-synthesis: none;
  text-rendering: optimizeLegibility;
  -webkit-font-smoothing: antialiased;
  -moz-osx-font-smoothing: grayscale;
  font-feature-settings: "tnum" 1;

  --header-h: 72px;
}

@media (max-width: 600px) {
  :root {
    --header-h: 62px;
  }
}

body {
  min-width: 320px;
  min-height: 100vh;
  margin: 0;
  color: var(--fbz-color-text);
  background: var(--fbz-color-bg);
}

html.fbz-scroll-locked,
body.fbz-scroll-locked {
  overflow: hidden;
}

button,
input {
  font: inherit;
}

button {
  cursor: pointer;
}

a {
  color: inherit;
}

#app {
  min-height: 100vh;
}

/* ───── 全局滚动条 ───── */

// Firefox / 标准
* {
  scrollbar-width: thin;
  scrollbar-color: var(--fbz-color-line-bright) transparent;
}

// WebKit (Chrome / Edge / Safari)
::-webkit-scrollbar {
  width: 6px;
  height: 6px;
}

::-webkit-scrollbar-track {
  background: transparent;
}

::-webkit-scrollbar-thumb {
  background: var(--fbz-color-line);
  border-radius: 3px;

  &:hover {
    background: var(--fbz-color-line-bright);
  }
}

::-webkit-scrollbar-corner {
  background: transparent;
}

.form-group {
  display: flex;
  flex-direction: column;
  gap: 8px;

  label {
    font-size: var(--fbz-font-size-xs);
    font-weight: 700;
    color: var(--fbz-color-text-soft);
    margin-bottom: 2px;
  }

  .control-input {
    height: 38px;
    background: var(--fbz-color-panel);
    border: 1px solid var(--fbz-color-line);
    border-radius: var(--fbz-radius-control);
    padding: 0 var(--fbz-space-3);
    color: var(--fbz-color-text);
    font-size: var(--fbz-font-size-sm);
    transition: all var(--fbz-motion-fast);

    &:focus {
      outline: none;
      border-color: var(--fbz-color-brand-500);
      box-shadow: var(--fbz-shadow-focus);
    }

    &:disabled {
      opacity: 0.5;
      cursor: not-allowed;
    }
  }

  .base-select {
    width: 100%;
    display: block;
  }

  .field-hint {
    font-size: 11px;
    color: var(--fbz-color-text-muted);
    line-height: 1.4;
    margin-top: 2px;
  }
}

/* Shared daisyUI contract: both themes and every surface use the same tokens. */
:root {
  --color-base-100: var(--fbz-color-bg);
  --color-base-200: var(--fbz-color-panel);
  --color-base-300: var(--fbz-color-panel-strong);
  --color-base-content: var(--fbz-color-text);
  --color-primary: var(--fbz-color-brand-500);
  --color-primary-content: #07150e;
  --color-secondary: var(--fbz-color-text-soft);
  --color-secondary-content: var(--fbz-color-bg);
  --color-accent: var(--fbz-color-brand-500);
  --color-accent-content: #07150e;
  --color-neutral: var(--fbz-color-panel-elevated);
  --color-neutral-content: var(--fbz-color-text);
  --color-info: #79b8dc;
  --color-info-content: #07121d;
  --color-success: var(--fbz-color-brand-500);
  --color-success-content: #07150e;
  --color-warning: #edc674;
  --color-warning-content: #261a08;
  --color-error: var(--fbz-color-danger-500);
  --color-error-content: #210b0e;
  --radius-selector: 0.65rem;
  --radius-field: 0.65rem;
  --radius-box: 1rem;
  --size-selector: 0.25rem;
  --size-field: 0.25rem;
  --border: 1px;
  --depth: 0;
  --noise: 0;
  --fbz-radius-card: 12px;
  --fbz-radius-control: 10px;
}
body {
  font-size: 14px;
  line-height: 1.55;
}
.du-btn {
  text-transform: none;
  letter-spacing: 0;
  box-shadow: none;
  font-weight: 550;
}
.du-btn:focus-visible,
.du-input:focus-visible,
.du-select:focus-visible {
  outline: 2px solid var(--fbz-color-brand-500);
  outline-offset: 3px;
}
.du-input,
.du-select,
.du-textarea {
  background: var(--fbz-color-panel);
  border-color: var(--fbz-color-line);
  color: var(--fbz-color-text);
}
.du-input::placeholder,
.du-textarea::placeholder {
  color: var(--fbz-color-text-muted);
}
.du-alert {
  box-shadow: none;
  border-radius: 10px;
  font-size: 13px;
}
.du-badge {
  font-size: 11px;
  font-weight: 500;
}
@media (prefers-reduced-motion: reduce) {
  *,
  *::before,
  *::after {
    animation-duration: 0.01ms !important;
    transition-duration: 0.01ms !important;
    scroll-behavior: auto !important;
  }
}

:root {
  --fbz-font-display: var(--fbz-font-sans);
}
```

### `src/stores/theme.ts`

```ts
import { ref } from "vue";
import { defineStore } from "pinia";

/**
 * 主题样式 Store —— 管理暗色/亮色主题模式，以及自定义品牌主题色。
 * 偏好自动同步至 LocalStorage，并在启动时应用至 html DOM 树。
 */
export const useThemeStore = defineStore("theme", () => {
  const themeMode = ref<"dark" | "light">(
    (localStorage.getItem("fbz-theme-mode") as "dark" | "light") ?? "dark",
  );
  const brandColor = ref<string>(localStorage.getItem("fbz-brand-color") ?? "#1ed760");

  function setThemeMode(mode: "dark" | "light") {
    themeMode.value = mode;
    localStorage.setItem("fbz-theme-mode", mode);
    applyTheme();
  }

  function setBrandColor(color: string) {
    brandColor.value = color;
    localStorage.setItem("fbz-brand-color", color);
    applyTheme();
  }

  function applyTheme() {
    if (typeof window === "undefined") return;

    const root = document.documentElement;

    // 应用主题属性与 class，使 Scss 与 UnoCSS 自动响应
    root.setAttribute("data-theme", themeMode.value);
    root.classList.toggle("light-theme", themeMode.value === "light");
    root.classList.toggle("dark-theme", themeMode.value === "dark");

    // 动态应用自定义主题色 CSS 变量，底层 dependent 变量（600/focus 等）由 Scss color-mix 自动计算
    root.style.setProperty("--fbz-color-brand-500", brandColor.value);
  }

  return {
    themeMode,
    brandColor,
    setThemeMode,
    setBrandColor,
    applyTheme,
  };
});
```

### `docs/frontend-design-system.md`

```md
# AetherNAS Frontend Design System

## Visual Direction

AetherNAS uses a dark home-theater control surface: low-light panels, crisp borders, cool active states, and compact information density. Components should feel operational rather than promotional.

## Token Rules

- Use `src/styles/theme/tokens.scss` as the source of truth for colors, spacing, radius, typography, shadow, focus, and motion.
- Expose runtime values as `--fbz-*` CSS variables through `fbz-theme-vars`.
- Keep `uno.config.ts` aligned with token names when a token is useful in atomic classes.
- Do not hardcode new colors in page components unless they represent one-off artwork or imported media.

## Component Layers

- `src/components/base`: generic UI primitives. They know about visual states, slots, accessibility, and sizing, but not media-library domain data.
- `src/components/media`: AetherNAS domain components. They can accept `MediaCardItem`, `EpisodeItem`, `MetricItem`, or media-specific props.
- `src/views`: route composition only. Route views choose data, layout, and navigation; they should not duplicate reusable component internals.

## Base Component Naming

- Use the `Base` prefix for primitives: `BaseButton`, `BaseInput`, `BaseSelect`, `BaseTextarea`, `BaseSwitch`, `BaseSegmentedControl`, `BaseFilterBar`, `BaseTable`, `BaseModal`, `BaseTag`, `BasePanel`, `BaseProgress`, `BaseEmpty`, `BaseIconButton`.
- Use PascalCase filenames and component names.
- Use typed props and emits.
- Prefer slots for optional icons, actions, and rich content.

## Media Components

- Use `MediaCard` for poster/continue-watching cards.
- Use `MediaInfoCard` when poster artwork, tags, and definition-list metadata must travel together.
- Use `MediaTagGroup` for all media capability/status tags instead of hand-placing `BaseTag` lists.
- Use `MediaPlayerControls` for playback bars, keeping progress as a `v-model:progress` contract and actions as explicit emits.
- Keep page-level media views as composition surfaces; do not duplicate card, metadata, tag, or player markup in route files.

## State Semantics

- `brand`: primary product action.
- `info`: neutral technical or playback state.
- `success`: healthy/available/completed state.
- `warning`: attention-needed or queued state.
- `danger`: destructive or failed state.
- `gold`: premium media capability, rating, or quality badge.

## Styling Rules

- SFC styles must use `<style scoped lang="scss">`.
- Prefer token variables over raw values.
- Use class selectors in scoped styles.
- Use UnoCSS for layout utilities when local component styles do not need to encode reusable behavior.
- Keep card radius at `8px` unless the component is an application shell or hero surface.

## Accessibility Rules

- Icon-only controls require an `aria-label`.
- Inputs need a stable `id` and `name`.
- Progress indicators expose `role="progressbar"` and `aria-valuenow`.
- Avoid using color alone to communicate status; pair color with text labels.
```
