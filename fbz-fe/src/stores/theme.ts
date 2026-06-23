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
