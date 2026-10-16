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
  <main class="c-page playlists-overview">
    <header class="page-head">
      <div>
        <h1 class="c-page-title"><BaseIcon name="playlist" :size="22" class="lead" />播放列表</h1>
        <p class="c-meta">{{ playlists.length }} 个列表 · 可共享给其他用户</p>
      </div>
      <button
        class="du-btn du-btn-primary du-btn-sm"
        type="button"
        :disabled="!authStore.userId"
        @click="openCreate"
      >
        <BaseIcon name="plus" :size="15" />新建播放列表
      </button>
    </header>

    <p v-if="loading" class="c-state">
      <BaseIcon name="loading" :size="16" class="c-spin" />正在加载播放列表…
    </p>
    <p v-else-if="error" class="c-state is-error">
      <BaseIcon name="error" :size="16" />{{ error }}
    </p>
    <BaseEmptyState
      v-else-if="!playlists.length"
      icon="playlist"
      title="还没有播放列表"
      description="点击右上角「新建播放列表」，或在任意媒体卡片右键「加入播放列表」。"
    >
      <button class="du-btn du-btn-primary" type="button" @click="openCreate">
        <BaseIcon name="plus" :size="16" />新建播放列表
      </button>
    </BaseEmptyState>

    <ul v-else class="grid" role="list">
      <li v-for="(pl, i) in playlists" :key="pl.id">
        <RouterLink :to="`/playlists/${pl.id}`" class="pl-card">
          <span class="pl-cover" :class="`tone-${i % 4}`">
            <BaseIcon name="playlist" :size="30" :stroke-width="1.4" />
          </span>
          <span class="pl-body">
            <span class="pl-name">{{ pl.name }}</span>
            <span class="c-meta">播放列表</span>
          </span>
          <BaseIcon name="right" :size="16" class="pl-arrow" />
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
          class="du-input field-input"
          type="text"
          maxlength="256"
          placeholder="例如：周末片单"
          @keydown.enter.prevent="submitCreate"
        />
      </label>
      <template #actions>
        <button
          class="du-btn du-btn-ghost"
          type="button"
          :disabled="creating"
          @click="createOpen = false"
        >
          取消
        </button>
        <button
          class="du-btn du-btn-primary"
          type="button"
          :disabled="creating"
          @click="submitCreate"
        >
          <BaseIcon v-if="creating" name="loading" :size="15" class="c-spin" />{{
            creating ? "创建中…" : "创建"
          }}
        </button>
      </template>
    </BaseModal>
  </main>
</template>

<style scoped lang="scss">
.playlists-overview {
  display: flex;
  flex-direction: column;
  gap: 20px;
}

.page-head {
  display: flex;
  align-items: flex-end;
  justify-content: space-between;
  gap: 16px;

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
  grid-template-columns: repeat(auto-fill, minmax(280px, 1fr));
  gap: 12px;
  margin: 0;
  padding: 0;
  list-style: none;
}

.pl-card {
  display: flex;
  align-items: center;
  gap: 14px;
  padding: 10px;
  border: 1px solid var(--ui-border);
  border-radius: var(--ui-radius-md);
  background: var(--ui-surface);
  color: inherit;
  text-decoration: none;
  transition:
    border-color var(--ui-duration-fast),
    background var(--ui-duration-fast);

  &:hover {
    border-color: var(--ui-border-strong);
    background: var(--ui-surface-hover);

    .pl-name {
      color: var(--ui-accent-hover);
    }

    .pl-arrow {
      color: var(--ui-accent);
      transform: translateX(2px);
    }
  }
}

.pl-cover {
  flex: 0 0 auto;
  width: 64px;
  height: 64px;
  display: grid;
  place-items: center;
  border: 1px solid var(--ui-border);
  border-radius: var(--ui-radius-sm);
  background: var(--ui-surface-raised);
  color: var(--ui-text-muted);

  &.tone-1 {
    background: linear-gradient(
      135deg,
      rgb(var(--ui-accent-channel) / 16%),
      var(--ui-surface-raised)
    );
  }

  &.tone-2 {
    background: linear-gradient(135deg, rgb(52 211 153 / 12%), var(--ui-surface-raised));
  }

  &.tone-3 {
    background: linear-gradient(135deg, rgb(251 113 133 / 12%), var(--ui-surface-raised));
  }
}

.pl-body {
  flex: 1;
  min-width: 0;
  display: flex;
  flex-direction: column;
  gap: 2px;
}

.pl-name {
  overflow: hidden;
  color: var(--ui-text);
  font-size: 14px;
  font-weight: 600;
  white-space: nowrap;
  text-overflow: ellipsis;
  transition: color var(--ui-duration-fast);
}

.pl-arrow {
  color: var(--ui-text-subtle);
  transition:
    color var(--ui-duration-fast),
    transform var(--ui-duration-fast);
}

.field {
  display: flex;
  flex-direction: column;
  gap: 6px;
}

.field-input {
  width: 100%;
}

.field-label {
  color: var(--ui-text-muted);
  font-size: 12px;
  font-weight: 600;
}
</style>
