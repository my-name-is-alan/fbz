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
  <main class="c-page collections-overview">
    <header class="page-head">
      <h1 class="c-page-title"><BaseIcon name="collection" :size="22" class="lead" />合集</h1>
      <p class="c-meta">{{ total }} 个合集 · BoxSet</p>
    </header>

    <p v-if="loading" class="c-state">
      <BaseIcon name="loading" :size="16" class="c-spin" />正在加载合集…
    </p>
    <p v-else-if="error" class="c-state is-error">
      <BaseIcon name="error" :size="16" />{{ error }}
    </p>
    <BaseEmptyState
      v-else-if="!items.length"
      icon="collection"
      title="还没有合集"
      description="管理员可在任意媒体卡片右键「加入合集」创建 BoxSet，创建后会显示在这里。"
    />

    <div v-else class="grid">
      <MediaCard
        v-for="(item, i) in items"
        :key="item.id"
        :item="item"
        layout="poster"
        :show-rating="false"
        :variant="(i % 2) as 0 | 1"
      />
    </div>
  </main>
</template>

<style scoped lang="scss">
.collections-overview {
  display: flex;
  flex-direction: column;
  gap: 20px;
}

.page-head {
  .c-page-title {
    display: flex;
    align-items: center;
    gap: 8px;
  }

  .lead {
    color: var(--ui-accent);
  }

  .c-meta {
    margin: 4px 0 0;
  }
}

.grid {
  display: grid;
  grid-template-columns: repeat(auto-fill, minmax(150px, 1fr));
  gap: 20px 16px;
}

@media (max-width: 600px) {
  .grid {
    grid-template-columns: repeat(auto-fill, minmax(104px, 1fr));
  }
}
</style>
