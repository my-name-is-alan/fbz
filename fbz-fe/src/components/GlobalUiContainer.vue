<script setup lang="ts">
// Aggregates all modals and overlays for mounting globally
</script>

<template>
  <SetupWizardModal />
  <FilePickerModal />
  <MetadataManagerModal />
  <GuidedTourOverlay />
  <ContextMenu />
</template>

<style scoped>
/* Core wrapper element has no custom styling needed since child modals are absolute/fixed */
</style>
