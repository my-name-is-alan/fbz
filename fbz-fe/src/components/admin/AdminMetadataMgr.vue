<script setup lang="ts">
import { useUiStore } from "@/stores/ui.ts";

const uiStore = useUiStore();

const cacheSize = ref("1.8 GB");
const dbSize = ref("45.2 MB");
const cacheFiles = ref(4820);

const processingId = ref<string | null>(null);

function runMaintenance(id: string, name: string) {
  processingId.value = id;
  setTimeout(() => {
    processingId.value = null;
    if (id === "clean-cache") {
      cacheSize.value = "0 KB";
      cacheFiles.value = 0;
      uiStore.showToast("已成功清空本地临时海报缓存！下次浏览将即时抓取。", "success");
    } else if (id === "scan-unmatched") {
      uiStore.showToast("扫描完成！系统内已不存在未匹配 TMDB 的影视条目。", "success");
    } else if (id === "backup-db") {
      uiStore.showToast("系统核心数据库备份完成！文件 [fbz_backup.db] 导出成功。", "success");
    }
  }, 1500);
}
</script>

<template>
  <div class="admin-metadata-mgr-view">
    <!-- Stat grid -->
    <div class="stats-overview-grid">
      <div class="stat-card">
        <span class="label">数据库体积</span>
        <span class="value">{{ dbSize }}</span>
        <span class="meta">SQLite 嵌入式存储</span>
      </div>
      <div class="stat-card">
        <span class="label">图片缓存体积</span>
        <span class="value">{{ cacheSize }}</span>
        <span class="meta">已保存 {{ cacheFiles }} 个海报文件</span>
      </div>
      <div class="stat-card">
        <span class="label">未匹配条目</span>
        <span class="value zero-val">0</span>
        <span class="meta">所有内容匹配度 100%</span>
      </div>
    </div>

    <!-- Actions List -->
    <section class="settings-card">
      <div class="card-header">
        <span class="indicator" />
        <h3>数据库与存储库日常维护</h3>
      </div>
      <div class="card-body">
        <p class="settings-hint">执行系统文件整理及图像缓存维护命令，释放物理存储空间。</p>

        <div class="maintenance-actions-stack">
          <!-- Clean cache -->
          <div class="action-row">
            <div class="row-left">
              <span class="title">清理本地海报缓存</span>
              <span class="desc"
                >清空从 TMDB 下载的图片缓存。再次浏览时，系统会重新自动抓取生成缓存。</span
              >
            </div>
            <button
              class="maintenance-btn"
              type="button"
              :disabled="processingId !== null"
              @click="runMaintenance('clean-cache', '清理海报缓存')"
            >
              <span class="spinner" v-if="processingId === 'clean-cache'" />
              <span>立即清理</span>
            </button>
          </div>

          <!-- Scan unmatched -->
          <div class="action-row">
            <div class="row-left">
              <span class="title">一键识别并匹配未关联条目</span>
              <span class="desc"
                >强制检索系统中未成功匹配 TMDB 数据的影视条目，再次运行批量匹配搜刮。</span
              >
            </div>
            <button
              class="maintenance-btn"
              type="button"
              :disabled="processingId !== null"
              @click="runMaintenance('scan-unmatched', '扫描未关联条目')"
            >
              <span class="spinner" v-if="processingId === 'scan-unmatched'" />
              <span>全局检索</span>
            </button>
          </div>

          <!-- Backup DB -->
          <div class="action-row">
            <div class="row-left">
              <span class="title">全局核心数据库备份</span>
              <span class="desc">导出系统已挂载的影视目录路径、元数据缓存及配置参数。</span>
            </div>
            <button
              class="maintenance-btn"
              type="button"
              :disabled="processingId !== null"
              @click="runMaintenance('backup-db', '核心数据库备份')"
            >
              <span class="spinner" v-if="processingId === 'backup-db'" />
              <span>立即备份</span>
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

.stats-overview-grid {
  display: grid;
  grid-template-columns: repeat(3, 1fr);
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

    &.zero-val {
      color: var(--fbz-color-brand-500);
    }
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
