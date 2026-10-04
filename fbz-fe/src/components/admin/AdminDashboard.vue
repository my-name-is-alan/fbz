<script setup lang="ts">
import { listAdminJobs, listScheduledTasks } from "@/service/modules/admin.ts";
import { useLibraryStore } from "@/stores/library.ts";
import type { AdminJob, ScheduledTask } from "@/types/admin.ts";

const libraryStore = useLibraryStore();

const jobs = ref<AdminJob[]>([]);
const scheduledTasks = ref<ScheduledTask[]>([]);
const loadingOps = shallowRef(false);
const opsError = shallowRef("");

const totalMovies = computed(() => {
  return libraryStore.libraries
    .filter((l) => l.kind === "movie" || l.kind === "documentary")
    .reduce((sum, l) => sum + l.count, 0);
});

const totalSeries = computed(() => {
  return libraryStore.libraries
    .filter((l) => l.kind === "series" || l.kind === "anime")
    .reduce((sum, l) => sum + l.count, 0);
});

const queuedJobs = computed(() => jobs.value.filter((job) => job.status === "queued").length);
const runningJobs = computed(
  () => jobs.value.filter((job) => job.status === "running" || job.lockActive).length,
);
const failedJobs = computed(() => jobs.value.filter((job) => job.status === "failed").length);
const enabledScheduledTasks = computed(
  () => scheduledTasks.value.filter((task) => task.enabled).length,
);
const activeScheduledRuns = computed(() =>
  scheduledTasks.value.reduce((sum, task) => sum + task.activeRunCount, 0),
);

onMounted(async () => {
  if (!libraryStore.loaded) void libraryStore.loadFromBackend();
  await loadOpsSummary();
});

function formatTime(value: string | null): string {
  if (!value) return "未完成";
  return new Intl.DateTimeFormat("zh-CN", {
    month: "2-digit",
    day: "2-digit",
    hour: "2-digit",
    minute: "2-digit",
  }).format(new Date(value));
}

async function loadOpsSummary() {
  loadingOps.value = true;
  opsError.value = "";
  try {
    const [jobPage, taskPage] = await Promise.all([
      listAdminJobs({ limit: 8 }),
      listScheduledTasks({ limit: 20 }),
    ]);
    jobs.value = jobPage.items;
    scheduledTasks.value = taskPage.items;
  } catch {
    opsError.value = "后台任务摘要加载失败，请检查管理员权限。";
  } finally {
    loadingOps.value = false;
  }
}
</script>

<template>
  <div class="admin-dashboard-view">
    <div class="metrics-grid">
      <div class="metric-card">
        <div class="card-title">后台 Job</div>
        <div class="metric-line">
          <span class="metric-value">{{ jobs.length }}</span>
          <span class="metric-meta">最近采样任务</span>
        </div>
        <div class="mini-stats">
          <span>queued {{ queuedJobs }}</span>
          <span>running {{ runningJobs }}</span>
          <span>failed {{ failedJobs }}</span>
        </div>
      </div>

      <div class="metric-card">
        <div class="card-title">计划任务</div>
        <div class="metric-line">
          <span class="metric-value">{{ scheduledTasks.length }}</span>
          <span class="metric-meta">已注册任务</span>
        </div>
        <div class="mini-stats">
          <span>enabled {{ enabledScheduledTasks }}</span>
          <span>active {{ activeScheduledRuns }}</span>
        </div>
      </div>

      <div class="metric-card">
        <div class="card-title">媒体库</div>
        <div class="metric-line">
          <span class="metric-value">{{ libraryStore.libraries.length }}</span>
          <span class="metric-meta">真实库配置</span>
        </div>
        <div class="mini-stats">
          <span>items {{ libraryStore.totalCount }}</span>
        </div>
      </div>
    </div>

    <p v-if="opsError" class="ops-error">{{ opsError }}</p>

    <div class="dashboard-mid-row">
      <div class="summary-section">
        <div class="sub-label">媒体统计</div>
        <div class="stats-grid">
          <div class="stat-item">
            <span class="icon">🎬</span>
            <div class="stat-info">
              <span class="number">{{ totalMovies }}</span>
              <span class="label">电影总数</span>
            </div>
          </div>
          <div class="stat-item">
            <span class="icon">📺</span>
            <div class="stat-info">
              <span class="number">{{ totalSeries }}</span>
              <span class="label">剧集与动漫</span>
            </div>
          </div>
          <div class="stat-item">
            <span class="icon">📁</span>
            <div class="stat-info">
              <span class="number">{{ libraryStore.totalCount }}</span>
              <span class="label">媒体条目总数</span>
            </div>
          </div>
        </div>
      </div>

      <div class="streams-section">
        <div class="sub-label">
          <span>计划任务状态</span>
          <RouterLink class="sub-link" to="/admin/scheduled-tasks">查看全部</RouterLink>
        </div>

        <div v-if="loadingOps" class="streams-empty">
          <span>正在读取任务状态...</span>
        </div>
        <div v-else-if="scheduledTasks.length === 0" class="streams-empty">
          <span>后端当前未注册计划任务。</span>
        </div>
        <div v-else class="task-list">
          <div v-for="task in scheduledTasks.slice(0, 5)" :key="task.id" class="task-item">
            <div>
              <span class="task-name">{{ task.taskKey }}</span>
              <span class="task-meta">{{ task.scheduleKind }} / {{ task.scheduleValue }}</span>
            </div>
            <span class="task-status" :class="{ enabled: task.enabled }">
              {{ task.enabled ? "启用" : "禁用" }}
            </span>
          </div>
        </div>
      </div>
    </div>

    <div class="activity-section">
      <div class="sub-label">
        <span>最近任务动态</span>
        <button class="text-refresh" type="button" :disabled="loadingOps" @click="loadOpsSummary">
          刷新
        </button>
      </div>
      <div v-if="loadingOps" class="activity-empty">正在读取后台任务...</div>
      <div v-else-if="jobs.length === 0" class="activity-empty">后端当前没有后台任务记录。</div>
      <div v-else class="job-list">
        <div v-for="job in jobs" :key="job.id" class="job-row">
          <div class="job-main">
            <span class="job-type">{{ job.jobType }}</span>
            <span class="job-meta">{{ job.queueName }} / attempts {{ job.attempts }}</span>
            <span v-if="job.lastError" class="job-error">{{ job.lastError }}</span>
          </div>
          <div class="job-side">
            <span class="job-status" :class="job.status">{{ job.status }}</span>
            <span class="job-time">{{ formatTime(job.finishedAt ?? job.updatedAt) }}</span>
          </div>
        </div>
      </div>
    </div>
  </div>
</template>

<style scoped lang="scss">
.admin-dashboard-view {
  display: flex;
  flex-direction: column;
  gap: var(--fbz-space-5);
}

.metrics-grid {
  display: grid;
  grid-template-columns: repeat(3, 1fr);
  gap: var(--fbz-space-4);

  @media (max-width: 768px) {
    grid-template-columns: 1fr;
  }
}

.metric-card {
  background: var(--fbz-color-panel-strong);
  border: 1px solid var(--fbz-color-line-soft);
  border-radius: 6px;
  padding: var(--fbz-space-4) var(--fbz-space-5);
  display: flex;
  flex-direction: column;
  gap: 14px;

  .card-title {
    font-size: 11px;
    font-weight: 700;
    text-transform: uppercase;
    color: var(--fbz-color-text-muted);
    letter-spacing: 0.5px;
  }
}

.metric-line {
  display: flex;
  align-items: baseline;
  gap: var(--fbz-space-3);
}

.metric-value {
  font-family: var(--fbz-font-display);
  font-size: 24px;
  font-weight: 800;
  color: var(--fbz-color-text);
}

.metric-meta,
.mini-stats,
.job-meta,
.job-time,
.task-meta {
  font-size: var(--fbz-font-size-xs);
  color: var(--fbz-color-text-muted);
}

.mini-stats {
  display: flex;
  flex-wrap: wrap;
  gap: var(--fbz-space-2);
}

.ops-error {
  margin: 0;
  color: var(--fbz-color-danger-500);
  font-size: var(--fbz-font-size-sm);
}

.dashboard-mid-row {
  display: grid;
  grid-template-columns: 1fr 1.5fr;
  gap: var(--fbz-space-4);

  @media (max-width: 768px) {
    grid-template-columns: 1fr;
  }
}

.sub-label {
  font-size: 13px;
  font-weight: 700;
  color: var(--fbz-color-text-soft);
  text-transform: uppercase;
  letter-spacing: 0.5px;
  margin-bottom: var(--fbz-space-3);
  display: flex;
  align-items: center;
  justify-content: space-between;
}

.sub-link,
.text-refresh {
  border: 0;
  background: transparent;
  color: var(--fbz-color-brand-500);
  font-size: var(--fbz-font-size-xs);
  font-weight: 800;
  text-decoration: none;
  cursor: pointer;

  &:disabled {
    opacity: 0.55;
    cursor: not-allowed;
  }
}

.stats-grid {
  display: flex;
  flex-direction: column;
  gap: var(--fbz-space-2);
}

.stat-item {
  display: flex;
  align-items: center;
  gap: 12px;
  padding: var(--fbz-space-3) var(--fbz-space-4);
  background: var(--fbz-color-panel-strong);
  border: 1px solid var(--fbz-color-line-soft);
  border-radius: 6px;

  .icon {
    font-size: 20px;
  }

  .stat-info {
    display: flex;
    flex-direction: column;
    gap: 2px;

    .number {
      font-family: var(--fbz-font-display);
      font-size: 18px;
      font-weight: 800;
    }

    .label {
      font-size: 10px;
      color: var(--fbz-color-text-muted);
      font-weight: 700;
    }
  }
}

.streams-section,
.activity-section {
  display: flex;
  flex-direction: column;
}

.streams-empty,
.activity-empty {
  border: 1px dashed var(--fbz-color-line-soft);
  border-radius: 6px;
  background: var(--fbz-color-panel-strong);
  padding: 40px;
  color: var(--fbz-color-text-muted);
  text-align: center;
  font-size: var(--fbz-font-size-sm);
}

.task-list,
.job-list {
  background: var(--fbz-color-panel-strong);
  border: 1px solid var(--fbz-color-line-soft);
  border-radius: 6px;
  overflow: hidden;
}

.task-item,
.job-row {
  padding: var(--fbz-space-3) var(--fbz-space-4);
  border-top: 1px solid var(--fbz-color-line-soft);
  display: flex;
  justify-content: space-between;
  gap: var(--fbz-space-4);

  &:first-child {
    border-top: 0;
  }
}

.task-name,
.job-type {
  display: block;
  color: var(--fbz-color-text);
  font-size: var(--fbz-font-size-sm);
  font-weight: 800;
}

.task-status,
.job-status {
  align-self: flex-start;
  border: 1px solid var(--fbz-color-line);
  border-radius: 4px;
  padding: 2px 6px;
  color: var(--fbz-color-text-muted);
  font-size: var(--fbz-font-size-xs);
  font-weight: 800;

  &.enabled,
  &.completed,
  &.succeeded {
    border-color: color-mix(in srgb, var(--fbz-color-brand-500) 30%, transparent);
    color: var(--fbz-color-brand-500);
  }

  &.failed {
    border-color: color-mix(in srgb, var(--fbz-color-danger-500) 30%, transparent);
    color: var(--fbz-color-danger-500);
  }
}

.job-main {
  min-width: 0;
  display: flex;
  flex-direction: column;
  gap: 3px;
}

.job-side {
  display: flex;
  flex-direction: column;
  align-items: flex-end;
  gap: 4px;
  flex-shrink: 0;
}

.job-error {
  color: var(--fbz-color-danger-500);
  font-size: var(--fbz-font-size-xs);
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
  max-width: 520px;
}
</style>
