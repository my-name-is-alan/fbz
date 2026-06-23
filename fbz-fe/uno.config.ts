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
