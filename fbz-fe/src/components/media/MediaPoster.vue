<script setup lang="ts">
const props = withDefaults(
  defineProps<{ src?: string; title: string; ratio?: "poster" | "wide"; variant?: 0 | 1 }>(),
  { ratio: "poster", variant: 0 },
);
const failed = ref(false);
const loaded = ref(false);
watch(
  () => props.src,
  () => {
    failed.value = false;
    loaded.value = false;
  },
);
</script>
<template>
  <div class="media-poster" :class="`ratio-${ratio}`">
    <img
      v-if="src && !failed"
      :src="src"
      :alt="title"
      loading="lazy"
      @load="loaded = true"
      @error="failed = true"
      :class="{ 'is-loading': !loaded }"
    />
    <div v-if="src && !failed && !loaded" class="du-skeleton poster-skeleton" />
    <div v-if="!src || failed" class="poster-placeholder">
      <BaseIcon :name="ratio === 'poster' ? 'movie' : 'tv'" :size="28" /><span>{{ title }}</span>
    </div>
  </div>
</template>
<style scoped lang="scss">
.media-poster {
  position: relative;
  width: 100%;
  overflow: hidden;
  background: var(--fbz-color-panel-strong);
}
.ratio-poster {
  aspect-ratio: 2/3;
}
.ratio-wide {
  aspect-ratio: 16/9;
}
img {
  width: 100%;
  height: 100%;
  object-fit: cover;
  display: block;
  transition: opacity 0.25s;
}
.is-loading {
  opacity: 0;
}
.poster-skeleton {
  position: absolute;
  inset: 0;
  border-radius: 0;
}
.poster-placeholder {
  position: absolute;
  inset: 0;
  display: flex;
  flex-direction: column;
  align-items: center;
  justify-content: center;
  gap: 16px;
  padding: 18px;
  color: var(--fbz-color-text-muted);
}
.poster-placeholder span {
  display: -webkit-box;
  -webkit-line-clamp: 3;
  -webkit-box-orient: vertical;
  overflow: hidden;
  text-align: center;
  font-size: 12px;
  line-height: 1.7;
  overflow-wrap: anywhere;
}
</style>
