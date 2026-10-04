<script setup lang="ts">
import { useLibraryStore } from "@/stores/library.ts";
const route = useRoute();
const library = useLibraryStore();
const id = computed(() => String(route.params.id));
</script>
<template>
  <ServerMediaBrowser :library-id="id" :title="library.getById(id)?.name || '媒体库'" />
</template>
