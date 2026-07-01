<script setup lang="ts">
const drawerOpen = ref(false);
</script>

<template>
  <div class="app-shell">
    <AppHeader @open-drawer="drawerOpen = true" />
    <AppDrawer v-model="drawerOpen" />

    <RouterView />
    <PlaybackOverlay />
    <MusicPlayerBar />
  </div>
</template>

<style scoped lang="scss">
.app-shell {
  min-height: 100vh;
}
</style>
