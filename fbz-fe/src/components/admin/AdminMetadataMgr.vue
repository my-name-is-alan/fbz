<script setup lang="ts">
import { cleanCache, getMaintenanceStats } from "@/service/modules/admin.ts";
import { useUiStore } from "@/stores/ui.ts";

const uiStore = useUiStore();

const cacheSizeBytes = ref(0);
const dbSizeBytes = ref(0);
const cacheFileCount = ref(0);

const loading = ref(false);
const loadError = ref("");
const cleaning = ref(false);

const cacheSize = computed(() => formatBytes(cacheSizeBytes.value));
const dbSize = computed(() => formatBytes(dbSizeBytes.value));

/** 字节格式化为 B / KB / MB / GB（1024 进制，保留 1 位小数）。 */
function formatBytes(bytes: number): string {
  if (!Number.isFinite(bytes) || bytes <= 0) return "0 KB";
  const units = ["B", "KB", "MB", "GB", "TB"];
  const exp = Math.min(Math.floor(Math.log(bytes) / Math.log(1024)), units.length - 1);
  const value = bytes / 1024 ** exp;
  return `${value.toFixed(exp === 0 ? 0 : 1)} ${units[exp]}`;
}

onMounted(() => {
  void loadStats();
});

async function loadStats() {
  loading.value = true;
  loadError.value = "";
  try {
    const stats = await getMaintenanceStats();
    cacheSizeBytes.value = stats.cacheSizeBytes;
    dbSizeBytes.value = stats.dbSizeBytes;
    cacheFileCount.value = stats.cacheFileCount;
  } catch {
    loadError.value = "存储统计加载失败，请确认后端已就绪且当前账号具备管理员权限。";
  } finally {
    loading.value = false;
  }
}

async function handleCleanCache() {
  cleaning.value = true;
  try {
    const result = await cleanCache();
    uiStore.showToast(
      `已清理 ${result.removedFiles} 个缓存文件，释放 ${formatBytes(result.freedBytes)}。`,
      "success",
    );
    await loadStats();
  } catch {
    uiStore.showToast("清理缓存失败，请稍后重试。", "error");
  } finally {
    cleaning.value = false;
  }
}
</script>

<template>
  <div class="admin-metadata-mgr-view">
    <p v-if="loadError" class="load-error">{{ loadError }}</p>

    <!-- Stat grid -->
    <div class="stats-overview-grid">
      <div class="stat-card">
        <span class="label">数据库体积</span>
        <span class="value">{{ dbSize }}</span>
        <span class="meta">PostgreSQL 存储</span>
      </div>
      <div class="stat-card">
        <span class="label">图片缓存体积</span>
        <span class="value">{{ cacheSize }}</span>
        <span class="meta">已保存 {{ cacheFileCount }} 个缓存文件</span>
      </div>
    </div>

    <!-- Actions List -->
    <section class="settings-card">
      <div class="card-header">
        <span class="indicator" />
        <h3>数据库与存储库日常维护</h3>
      </div>
      <div class="card-body">
        <p class="settings-hint">执行图像缓存维护命令，释放物理存储空间。</p>

        <div class="maintenance-actions-stack">
          <!-- Clean cache -->
          <div class="action-row">
            <div class="row-left">
              <span class="title">清理本地图片缓存</span>
              <span class="desc">清空下载的图片缓存。再次浏览时，系统会重新自动抓取生成缓存。</span>
            </div>
            <button
              class="maintenance-btn"
              type="button"
              :disabled="cleaning || loading"
              @click="handleCleanCache"
            >
              <span class="spinner" v-if="cleaning" />
              <span>{{ cleaning ? "清理中..." : "立即清理" }}</span>
            </button>
          </div>
        </div>
      </div>
    </section>
  </div>
</template>

<style scoped lang="scss">
.admin-metadata-mgr-view {
  display: flex;
  flex-direction: column;
  gap: var(--fbz-space-5);
}

.load-error {
  margin: 0;
  color: var(--fbz-color-danger-500);
  font-size: var(--fbz-font-size-sm);
}

.stats-overview-grid {
  display: grid;
  grid-template-columns: repeat(2, 1fr);
  gap: var(--fbz-space-4);

  @media (max-width: 768px) {
    grid-template-columns: 1fr;
  }
}

.stat-card {
  background: var(--fbz-color-panel-strong);
  border: 1px solid var(--fbz-color-line-soft);
  border-radius: 6px;
  padding: var(--fbz-space-4) var(--fbz-space-5);
  display: flex;
  flex-direction: column;
  gap: 6px;

  .label {
    font-size: 11px;
    font-weight: 700;
    text-transform: uppercase;
    color: var(--fbz-color-text-muted);
    letter-spacing: 0.5px;
  }

  .value {
    font-family: var(--fbz-font-display);
    font-size: 20px;
    font-weight: 800;
    color: var(--fbz-color-text);
  }

  .meta {
    font-size: 11px;
    color: var(--fbz-color-text-muted);
  }
}

.maintenance-actions-stack {
  display: flex;
  flex-direction: column;
  gap: 12px;
}

.action-row {
  display: flex;
  justify-content: space-between;
  align-items: center;
  gap: var(--fbz-space-5);
  padding: var(--fbz-space-4);
  background: var(--fbz-color-panel);
  border: 1px solid var(--fbz-color-line-soft);
  border-radius: 6px;

  @media (max-width: 600px) {
    flex-direction: column;
    align-items: flex-start;
    gap: var(--fbz-space-3);
  }

  .row-left {
    display: flex;
    flex-direction: column;
    gap: 4px;

    .title {
      font-size: 13px;
      font-weight: 700;
      color: var(--fbz-color-text);
    }

    .desc {
      font-size: 11px;
      color: var(--fbz-color-text-muted);
      line-height: 1.4;
    }
  }

  .maintenance-btn {
    height: 32px;
    padding: 0 14px;
    background: var(--fbz-color-panel-strong);
    border: 1px solid var(--fbz-color-line);
    color: var(--fbz-color-text-soft);
    font-size: var(--fbz-font-size-xs);
    font-weight: 700;
    border-radius: 4px;
    cursor: pointer;
    display: inline-flex;
    align-items: center;
    gap: 8px;
    transition: all var(--fbz-motion-fast);
    flex-shrink: 0;

    &:hover:not(:disabled) {
      background: var(--fbz-color-panel-elevated);
      color: var(--fbz-color-text);
    }

    &:disabled {
      opacity: 0.5;
      cursor: not-allowed;
    }
  }
}

.spinner {
  width: 12px;
  height: 12px;
  border: 2px solid var(--fbz-color-text-soft);
  border-top-color: transparent;
  border-radius: 50%;
  animation: spin 0.8s linear infinite;
}

@keyframes spin {
  to {
    transform: rotate(360deg);
  }
}
</style>
