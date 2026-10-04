<script setup lang="ts">
// Aggregates all modals and overlays for mounting globally
</script>

<template>
  <SetupWizardModal />
  <FilePickerModal />
  <LibrarySettingsModal />
  <MetadataManagerModal />
  <ContextMenu />
  <LibraryContextMenu />
  <GlobalToast />
</template>

<style scoped>
/* Core wrapper element has no custom styling needed since child modals are absolute/fixed */
</style>
