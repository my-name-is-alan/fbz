<script setup lang="ts">
import { useCSurface } from "@/composables/useCSurface.ts";

useCSurface();

const drawerOpen = ref(false);
</script>

<template>
  <div class="app-shell">
    <AppHeader @open-drawer="drawerOpen = true" />
    <AppDrawer v-model="drawerOpen" />

    <div class="app-main">
      <RouterView />
    </div>
    <MusicPlayerBar />
  </div>
</template>

<style scoped lang="scss">
.app-shell {
  position: relative;
  min-height: 100vh;
  display: flex;
  flex-direction: column;
  background:
    radial-gradient(circle at 50% -20%, rgb(var(--ui-accent-channel) / 7%), transparent 34rem),
    var(--ui-background);
}

.app-main {
  position: relative;
  flex: 1;
  min-width: 0;
}
</style>
