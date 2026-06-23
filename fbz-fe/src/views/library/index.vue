<script setup lang="ts">
import { useLibraryStore } from "@/stores/library.ts";

const libraryStore = useLibraryStore();
const { libraries, totalCount } = storeToRefs(libraryStore);

const fmt = new Intl.NumberFormat("en-US");
</script>

<template>
  <main class="library-overview">
    <header class="page-head">
      <h1>媒体库</h1>
      <p class="sub">{{ libraries.length }} 个库 · 共 {{ fmt.format(totalCount) }} 个条目</p>
    </header>

    <div class="grid">
      <RouterLink
        v-for="(lib, i) in libraries"
        :key="lib.id"
        :to="`/library/${lib.id}`"
        class="lib-card"
      >
        <div class="cover" :class="{ alt: i % 2 === 1 }">
          <span class="cover-name">{{ lib.name }}</span>
        </div>
        <div class="info">
          <span class="name">{{ lib.name }}</span>
          <span class="count">{{ fmt.format(lib.count) }}</span>
        </div>
      </RouterLink>
    </div>
  </main>
</template>

<style scoped lang="scss">
.library-overview {
  padding: calc(var(--header-h, 60px) + var(--fbz-space-8)) var(--fbz-space-8) 80px;
  min-height: 100vh;
}

.page-head {
  margin-bottom: var(--fbz-space-6);

  h1 {
    margin: 0 0 var(--fbz-space-2);
    font-size: var(--fbz-font-size-xl);
    font-weight: 800;
  }

  .sub {
    margin: 0;
    color: var(--fbz-color-text-muted);
    font-size: var(--fbz-font-size-md);
  }
}

.grid {
  display: grid;
  grid-template-columns: repeat(auto-fill, minmax(220px, 1fr));
  gap: var(--fbz-space-5);
}

.lib-card {
  text-decoration: none;
  color: inherit;
}

.cover {
  aspect-ratio: 16 / 9;
  border-radius: var(--fbz-radius-card);
  border: 1px solid var(--fbz-color-line-soft);
  background: var(--fbz-color-panel);
  display: grid;
  place-content: center;
  transition:
    border-color var(--fbz-motion-fast),
    transform var(--fbz-motion-fast);

  &.alt {
    background: var(--fbz-color-panel-strong);
  }

  .lib-card:hover & {
    border-color: var(--fbz-color-brand-500);
    transform: translateY(-3px);
  }
}

.cover-name {
  font-size: var(--fbz-font-size-lg);
  font-weight: 700;
  color: var(--fbz-color-text-muted);
}

.info {
  display: flex;
  align-items: baseline;
  justify-content: space-between;
  margin-top: var(--fbz-space-3);

  .name {
    font-size: var(--fbz-font-size-md);
    font-weight: 600;
  }

  .count {
    font-size: var(--fbz-font-size-sm);
    color: var(--fbz-color-text-muted);
  }
}

@media (max-width: 600px) {
  .library-overview {
    padding: calc(var(--header-h, 56px) + var(--fbz-space-5)) var(--fbz-space-4) 60px;
  }

  .grid {
    grid-template-columns: repeat(auto-fill, minmax(150px, 1fr));
    gap: var(--fbz-space-4);
  }
}
</style>
