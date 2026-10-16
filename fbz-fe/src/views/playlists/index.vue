<script setup lang="ts">
import {
  createPlaylist,
  listPlaylists,
  type PlaylistSummary,
} from "@/service/modules/playlists.ts";
import { useAuthStore } from "@/stores/auth.ts";
import { useUiStore } from "@/stores/ui.ts";

const authStore = useAuthStore();
const ui = useUiStore();
const router = useRouter();
const playlists = ref<PlaylistSummary[]>([]);
const loading = ref(true);
const error = ref<string | null>(null);
const createOpen = ref(false);
const createName = ref("");
const creating = ref(false);

async function load() {
  loading.value = true;
  error.value = null;
  try {
    const userId = authStore.userId;
    if (!userId) {
      playlists.value = [];
      error.value = "请先登录后查看播放列表";
      return;
    }
    playlists.value = await listPlaylists(userId);
  } catch {
    error.value = "无法加载播放列表，请稍后重试";
    playlists.value = [];
  } finally {
    loading.value = false;
  }
}

function openCreate() {
  createName.value = "";
  createOpen.value = true;
}

async function submitCreate() {
  const name = createName.value.trim();
  if (!name) {
    ui.showToast("请输入播放列表名称", "warning");
    return;
  }
  if (!authStore.userId) {
    ui.showToast("请先登录", "warning");
    return;
  }
  creating.value = true;
  try {
    const created = await createPlaylist({ name });
    createOpen.value = false;
    ui.showToast(`已创建「${created.name}」`, "success");
    await router.push(`/playlists/${created.id}`);
  } catch {
    ui.showToast("创建失败，请稍后重试", "error");
  } finally {
    creating.value = false;
  }
}

onMounted(() => {
  void load();
});
</script>

<template>
  <main class="playlists-overview">
    <header class="page-head">
      <div class="head-text">
        <h1>播放列表</h1>
        <p class="sub">{{ playlists.length }} 个列表</p>
      </div>
      <button
        class="btn-create"
        type="button"
        :disabled="!!error && !authStore.userId"
        @click="openCreate"
      >
        新建
      </button>
    </header>

    <p v-if="loading" class="state-text">正在加载播放列表…</p>
    <p v-else-if="error" class="state-text is-error">{{ error }}</p>
    <BaseEmptyState
      v-else-if="!playlists.length"
      icon="♫"
      title="还没有播放列表"
      description="点击右上角「新建」，或在媒体卡片右键「加入播放列表」。"
    />

    <ul v-else class="list" role="list">
      <li v-for="pl in playlists" :key="pl.id">
        <RouterLink :to="`/playlists/${pl.id}`" class="pl-card">
          <span class="pl-ic" aria-hidden="true">♫</span>
          <span class="pl-name">{{ pl.name }}</span>
          <span class="pl-arrow" aria-hidden="true">›</span>
        </RouterLink>
      </li>
    </ul>

    <BaseModal
      :open="createOpen"
      title="新建播放列表"
      description="创建后可在详情页添加条目，或从媒体卡片右键加入。"
      @close="createOpen = false"
    >
      <label class="field">
        <span class="field-label">名称</span>
        <input
          v-model="createName"
          class="field-input"
          type="text"
          maxlength="256"
          placeholder="例如：周末片单"
          @keydown.enter.prevent="submitCreate"
        />
      </label>
      <template #actions>
        <button class="btn-ghost" type="button" :disabled="creating" @click="createOpen = false">
          取消
        </button>
        <button class="btn-primary" type="button" :disabled="creating" @click="submitCreate">
          {{ creating ? "创建中…" : "创建" }}
        </button>
      </template>
    </BaseModal>
  </main>
</template>

<style scoped lang="scss">
.playlists-overview {
  padding: calc(var(--header-h, 60px) + var(--fbz-space-8)) var(--fbz-space-8) 80px;
  min-height: 100vh;
  max-width: 960px;
  margin: 0 auto;
}

.page-head {
  display: flex;
  align-items: flex-end;
  justify-content: space-between;
  gap: var(--fbz-space-4);
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

.btn-create,
.btn-primary {
  appearance: none;
  border: none;
  border-radius: 999px;
  padding: 10px 18px;
  font-weight: 700;
  cursor: pointer;
  background: var(--fbz-color-brand-500);
  color: #fff;

  &:disabled {
    opacity: 0.5;
    cursor: not-allowed;
  }
}

.btn-ghost {
  appearance: none;
  border: 1px solid var(--fbz-color-line);
  border-radius: 999px;
  padding: 10px 18px;
  font-weight: 600;
  cursor: pointer;
  background: transparent;
  color: inherit;
}

.state-text {
  margin: var(--fbz-space-8) 0;
  color: var(--fbz-color-text-muted);

  &.is-error {
    color: var(--fbz-color-danger, #e57373);
  }
}

.list {
  list-style: none;
  margin: 0;
  padding: 0;
  display: grid;
  gap: var(--fbz-space-3);
}

.pl-card {
  display: flex;
  align-items: center;
  gap: var(--fbz-space-3);
  padding: var(--fbz-space-4) var(--fbz-space-5);
  border-radius: 10px;
  background: var(--fbz-color-panel, rgba(255, 255, 255, 0.04));
  border: 1px solid var(--fbz-color-line);
  color: inherit;
  text-decoration: none;
  transition: background var(--fbz-motion-base, 0.15s ease);

  &:hover {
    background: var(--fbz-color-panel-strong, rgba(255, 255, 255, 0.08));
  }
}

.pl-ic {
  width: 36px;
  height: 36px;
  display: grid;
  place-content: center;
  border-radius: 8px;
  background: var(--fbz-color-bg-strong, rgba(0, 0, 0, 0.25));
  color: var(--fbz-color-brand-500);
  font-size: 16px;
}

.pl-name {
  flex: 1;
  font-weight: 600;
  font-size: var(--fbz-font-size-md, 15px);
}

.pl-arrow {
  color: var(--fbz-color-text-muted);
  font-size: 20px;
}

.field {
  display: grid;
  gap: var(--fbz-space-2);
}

.field-label {
  font-size: var(--fbz-font-size-sm);
  color: var(--fbz-color-text-muted);
}

.field-input {
  appearance: none;
  border: 1px solid var(--fbz-color-line);
  border-radius: 10px;
  padding: 10px 12px;
  background: var(--fbz-color-bg-strong, rgba(0, 0, 0, 0.2));
  color: inherit;
  font-size: var(--fbz-font-size-md, 15px);
}

@media (max-width: 600px) {
  .playlists-overview {
    padding: calc(var(--header-h, 60px) + var(--fbz-space-5)) var(--fbz-space-4) 60px;
  }
}
</style>
