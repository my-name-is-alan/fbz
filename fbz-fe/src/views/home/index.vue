<script setup lang="ts">
import { homeContinue, homeFeatured, homeMovies, homeSeries } from "@/service/modules/tmdb.ts";
</script>

<template>
  <main class="home-view">
    <HomeHero :items="homeFeatured" />

    <div class="content">
      <MediaRow
        title="继续观看"
        :items="homeContinue"
        layout="wide"
        size="xlarge"
        :show-resolution="false"
        :show-rating="false"
      />
      <MediaRow title="最新入库" :items="homeMovies" layout="poster" to="/library/movie" />
      <MediaRow title="热门剧集" :items="homeSeries" layout="poster" to="/library/series" />
    </div>
  </main>
</template>

<style scoped lang="scss">
.content {
  position: relative;
  z-index: 3;
  padding: var(--fbz-space-6) var(--fbz-space-8) 80px;
}

@media (max-width: 600px) {
  .content {
    padding: var(--fbz-space-5) var(--fbz-space-4) 60px;
  }
}
</style>
