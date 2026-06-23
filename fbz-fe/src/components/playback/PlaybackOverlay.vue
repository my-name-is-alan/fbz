<script setup lang="ts">
import { storeToRefs } from "pinia";
import FullscreenPlayer from "@/components/playback/FullscreenPlayer.vue";
import { useBodyScrollLock } from "@/composables/useBodyScrollLock.ts";
import { usePlaybackStore } from "@/stores/playback.ts";

const playback = usePlaybackStore();
const {
  item,
  isOpen,
  playlist,
  currentEpisodeIndex,
  hasPreviousEpisode,
  hasNextEpisode,
} = storeToRefs(playback);

useBodyScrollLock(isOpen);

useEventListener(window, "keydown", (event) => {
  if (event.key === "Escape" && isOpen.value) playback.close();
});
</script>

<template>
  <Teleport to="body">
    <Transition name="playback-fade">
      <FullscreenPlayer
        v-if="isOpen && item"
        :item="item"
        :playlist="playlist"
        :current-episode-index="currentEpisodeIndex"
        :has-previous-episode="hasPreviousEpisode"
        :has-next-episode="hasNextEpisode"
        @close="playback.close"
        @select-episode="playback.selectEpisode"
        @previous-episode="playback.playPreviousEpisode"
        @next-episode="playback.playNextEpisode"
      />
    </Transition>
  </Teleport>
</template>

<style scoped lang="scss">
.playback-fade-enter-active,
.playback-fade-leave-active {
  transition: opacity var(--fbz-motion-base);
}

.playback-fade-enter-from,
.playback-fade-leave-to {
  opacity: 0;
}
</style>
