<script setup lang="ts">
import { useThemeStore } from "@/stores/theme.ts";
import { useUiStore } from "@/stores/ui.ts";

const themeStore = useThemeStore();
themeStore.applyTheme();

// 开机向后端拉初始化状态，决定是否弹出 setup 向导（取代旧的 localStorage 标志）。
const uiStore = useUiStore();
uiStore.refreshSetupStatus();
</script>

<template>
  <RouterView />
  <GlobalUiContainer />
</template>
