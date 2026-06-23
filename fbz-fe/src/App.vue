<script setup lang="ts">
import { useThemeStore } from "@/stores/theme.ts";

const themeStore = useThemeStore();
themeStore.applyTheme();
</script>

<template>
  <RouterView />
  <GlobalUiContainer />
</template>
