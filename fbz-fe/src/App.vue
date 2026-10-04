<script setup lang="ts">
import { useThemeStore } from "@/stores/theme.ts";

import { useLibraryStore } from "@/stores/library.ts";
import { useAuthStore } from "@/stores/auth.ts";
const auth = useAuthStore();
const library = useLibraryStore();
watch(
  () => auth.isAuthenticated,
  () => {
    void library.refresh();
  },
  { immediate: true },
);

const themeStore = useThemeStore();
themeStore.applyTheme();
</script>

<template>
  <RouterView />
  <GlobalUiContainer />
  <PlaybackOverlay />
</template>
