<script setup lang="ts">
import { listCollections } from "@/service/modules/collections.ts";
import { useAuthStore } from "@/stores/auth.ts";
import type { MediaItem } from "@/types/media.ts";

const authStore = useAuthStore();
const items = ref<MediaItem[]>([]);
const total = ref(0);
const loading = ref(true);
const error = ref<string | null>(null);

async function load() {
  loading.value = true;
  error.value = null;
  try {
    const userId = authStore.userId;
    if (!userId) {
      items.value = [];
      error.value = "请先登录后查看合集";
      return;
    }
    const page = await listCollections(userId);
    items.value = page.items;
    total.value = page.total;
  } catch {
    error.value = "无法加载合集列表，请稍后重试";
    items.value = [];
    total.value = 0;
  } finally {
    loading.value = false;
  }
}

onMounted(() => {
  void load();
});
</script>

<template>
  <main class="collections-overview">
    <header class="page-head">
      <h1>合集</h1>
      <p class="sub">{{ total }} 个合集 · BoxSet</p>
    </header>

    <p v-if="loading" class="state-text">正在加载合集…</p>
    <p v-else-if="error" class="state-text is-error">{{ error }}</p>
    <BaseEmptyState
      v-else-if="!items.length"
      icon="▣"
      title="还没有合集"
      description="后端已支持 Collections/BoxSet；创建合集后将显示在此，点开可看成员。"
    />

    <div v-else class="grid">
      <MediaCard
        v-for="(item, i) in items"
        :key="item.id"
        :item="item"
        layout="poster"
        :variant="(i % 2) as 0 | 1"
      />
    </div>
  </main>
</template>

<style scoped lang="scss">
.collections-overview {
  padding: calc(var(--header-h, 60px) + var(--fbz-space-8)) var(--fbz-space-8) 80px;
  min-height: 100vh;
  max-width: 1280px;
  margin: 0 auto;
}

.page-head {
  margin-bottom: var(--fbz-space-6);

  h1 {
    margin: 0 0 var(--fbz-space-2);
    font-size: var(--fbz-font-size-xl);
    font-weight: 700;
  }

  .sub {
    margin: 0;
    color: var(--fbz-color-text-muted);
    font-size: var(--fbz-font-size-sm);
  }
}

.state-text {
  margin: var(--fbz-space-8) 0;
  color: var(--fbz-color-text-muted);

  &.is-error {
    color: var(--fbz-color-danger, #e57373);
  }
}

.grid {
  display: grid;
  grid-template-columns: repeat(auto-fill, minmax(150px, 1fr));
  gap: var(--fbz-space-5) var(--fbz-space-4);
}

@media (max-width: 600px) {
  .collections-overview {
    padding: calc(var(--header-h, 60px) + var(--fbz-space-5)) var(--fbz-space-4) 60px;
  }

  .grid {
    grid-template-columns: repeat(auto-fill, minmax(110px, 1fr));
    gap: var(--fbz-space-4) var(--fbz-space-3);
  }
}
</style>
