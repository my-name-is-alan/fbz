<script setup lang="ts">
import {
  listScheduledTaskRuns,
  listScheduledTasks,
  runScheduledTask,
} from "@/service/modules/admin.ts";
import { useUiStore } from "@/stores/ui.ts";
import type { ScheduledTask, ScheduledTaskRunHistory } from "@/types/admin.ts";

const uiStore = useUiStore();

const tasks = ref<ScheduledTask[]>([]);
const runs = ref<ScheduledTaskRunHistory[]>([]);
const selectedTaskKey = shallowRef("");
const loading = shallowRef(false);
const runsLoading = shallowRef(false);
const runningTaskKey = shallowRef("");
const error = shallowRef("");

const selectedTask = computed(
  () => tasks.value.find((task) => task.taskKey === selectedTaskKey.value) ?? null,
);

const enabledCount = computed(() => tasks.value.filter((task) => task.enabled).length);
const activeRunCount = computed(() =>
  tasks.value.reduce((sum, task) => sum + task.activeRunCount, 0),
);
const failureCount = computed(() => tasks.value.reduce((sum, task) => sum + task.failureCount, 0));

onMounted(() => {
  void refreshTasks();
});

function formatTime(value: string | null): string {
  if (!value) return "未运行";
  return new Intl.DateTimeFormat("zh-CN", {
    month: "2-digit",
    day: "2-digit",
    hour: "2-digit",
    minute: "2-digit",
  }).format(new Date(value));
}

function scheduleLabel(task: ScheduledTask): string {
  return `${task.scheduleKind}: ${task.scheduleValue}`;
}

async function refreshTasks() {
  loading.value = true;
  error.value = "";
  try {
    const page = await listScheduledTasks({ limit: 200 });
    tasks.value = page.items;
    if (!selectedTaskKey.value && page.items[0]) {
      selectedTaskKey.value = page.items[0].taskKey;
      await refreshRuns(page.items[0].taskKey);
    } else if (selectedTaskKey.value) {
      await refreshRuns(selectedTaskKey.value);
    }
  } catch {
    error.value = "计划任务加载失败，请检查登录状态和管理员权限。";
  } finally {
    loading.value = false;
  }
}

async function selectTask(task: ScheduledTask) {
  selectedTaskKey.value = task.taskKey;
  await refreshRuns(task.taskKey);
}

async function refreshRuns(taskKey: string) {
  runsLoading.value = true;
  try {
    const page = await listScheduledTaskRuns(taskKey, { limit: 20 });
    runs.value = page.items;
  } catch {
    runs.value = [];
  } finally {
    runsLoading.value = false;
  }
}

async function handleRunTask(task: ScheduledTask) {
  runningTaskKey.value = task.taskKey;
  try {
    const summary = await runScheduledTask(task.taskKey);
    uiStore.showToast(
      `任务 ${summary.taskKey} 已触发，入队 ${summary.queuedJobs} 个后台 job。`,
      "success",
    );
    await refreshTasks();
  } catch {
    uiStore.showToast("手动执行计划任务失败，可能任务被禁用或已有并发运行。", "error");
  } finally {
    runningTaskKey.value = "";
  }
}
</script>

<template>
  <div class="scheduled-tasks-view">
    <div class="summary-grid">
      <div class="summary-card">
        <span class="label">任务总数</span>
        <span class="value">{{ tasks.length }}</span>
      </div>
      <div class="summary-card">
        <span class="label">已启用</span>
        <span class="value">{{ enabledCount }}</span>
      </div>
      <div class="summary-card">
        <span class="label">运行中</span>
        <span class="value">{{ activeRunCount }}</span>
      </div>
      <div class="summary-card">
        <span class="label">失败计数</span>
        <span class="value danger">{{ failureCount }}</span>
      </div>
    </div>

    <div class="toolbar">
      <span class="hint">数据来自 `/api/admin/scheduled-tasks`，手动执行会调用后端调度器。</span>
      <button class="refresh-btn" type="button" :disabled="loading" @click="refreshTasks">
        {{ loading ? "刷新中..." : "刷新" }}
      </button>
    </div>

    <p v-if="error" class="error-text">{{ error }}</p>

    <div class="tasks-layout">
      <section class="tasks-list" aria-label="计划任务列表">
        <div v-if="loading" class="empty-state">正在加载计划任务...</div>
        <div v-else-if="tasks.length === 0" class="empty-state">
          后端当前没有注册计划任务。启用 scheduler 或安装带 schedule 的插件后会显示在这里。
        </div>
        <button
          v-for="task in tasks"
          v-else
          :key="task.id"
          class="task-row"
          :class="{ active: selectedTaskKey === task.taskKey, disabled: !task.enabled }"
          type="button"
          @click="selectTask(task)"
        >
          <span class="task-main">
            <span class="task-name">{{ task.taskKey }}</span>
            <span class="task-meta">{{ task.taskType }} / {{ task.ownerType }}</span>
          </span>
          <span class="task-state" :class="{ enabled: task.enabled }">
            {{ task.enabled ? "启用" : "禁用" }}
          </span>
        </button>
      </section>

      <section class="task-detail" aria-label="计划任务详情">
        <template v-if="selectedTask">
          <div class="detail-header">
            <div>
              <h3>{{ selectedTask.taskKey }}</h3>
              <p>{{ scheduleLabel(selectedTask) }}</p>
            </div>
            <button
              class="run-btn"
              type="button"
              :disabled="runningTaskKey === selectedTask.taskKey || !selectedTask.enabled"
              @click="handleRunTask(selectedTask)"
            >
              {{ runningTaskKey === selectedTask.taskKey ? "执行中..." : "立即运行" }}
            </button>
          </div>

          <div class="detail-grid">
            <div>
              <span class="label">下次运行</span>
              <span>{{ formatTime(selectedTask.nextRunAt) }}</span>
            </div>
            <div>
              <span class="label">上次运行</span>
              <span>{{ formatTime(selectedTask.lastRunAt) }}</span>
            </div>
            <div>
              <span class="label">超时</span>
              <span>{{ selectedTask.timeoutSeconds }}s</span>
            </div>
            <div>
              <span class="label">最大并发</span>
              <span>{{ selectedTask.maxConcurrency }}</span>
            </div>
          </div>

          <p v-if="selectedTask.lastError" class="last-error">{{ selectedTask.lastError }}</p>

          <div class="runs-section">
            <div class="section-title">运行历史</div>
            <div v-if="runsLoading" class="empty-state compact">正在读取运行历史...</div>
            <div v-else-if="runs.length === 0" class="empty-state compact">暂无运行历史。</div>
            <div v-for="run in runs" v-else :key="run.id" class="run-row">
              <div class="run-main">
                <span class="run-status" :class="run.status">{{ run.status }}</span>
                <span>{{ run.triggerType }} / {{ run.workerId }}</span>
              </div>
              <span class="run-time">{{ formatTime(run.startedAt) }}</span>
            </div>
          </div>
        </template>

        <div v-else class="empty-state">选择一个计划任务查看详情。</div>
      </section>
    </div>
  </div>
</template>

<style scoped lang="scss">
.scheduled-tasks-view {
  display: flex;
  flex-direction: column;
  gap: var(--fbz-space-4);
}

.summary-grid {
  display: grid;
  grid-template-columns: repeat(4, 1fr);
  gap: var(--fbz-space-3);
}

.summary-card {
  background: var(--fbz-color-panel-strong);
  border: 1px solid var(--fbz-color-line-soft);
  border-radius: 6px;
  padding: var(--fbz-space-4);
  display: flex;
  flex-direction: column;
  gap: 6px;

  .label {
    color: var(--fbz-color-text-muted);
    font-size: var(--fbz-font-size-xs);
    font-weight: 700;
  }

  .value {
    color: var(--fbz-color-text);
    font-family: var(--fbz-font-display);
    font-size: 22px;
    font-weight: 800;

    &.danger {
      color: var(--fbz-color-danger-500);
    }
  }
}

.toolbar {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: var(--fbz-space-3);

  .hint {
    color: var(--fbz-color-text-muted);
    font-size: var(--fbz-font-size-sm);
  }
}

.refresh-btn,
.run-btn {
  height: 34px;
  padding: 0 var(--fbz-space-4);
  border: 1px solid var(--fbz-color-line);
  border-radius: var(--fbz-radius-control);
  background: var(--fbz-color-panel-strong);
  color: var(--fbz-color-text-soft);
  font-size: var(--fbz-font-size-sm);
  font-weight: 700;
  cursor: pointer;

  &:hover:not(:disabled) {
    color: var(--fbz-color-text);
    background: var(--fbz-color-panel-elevated);
  }

  &:disabled {
    opacity: 0.55;
    cursor: not-allowed;
  }
}

.run-btn {
  border: 0;
  background: var(--fbz-color-brand-500);
  color: #07120a;

  &:hover:not(:disabled) {
    background: var(--fbz-color-brand-600);
  }
}

.error-text,
.last-error {
  margin: 0;
  color: var(--fbz-color-danger-500);
  font-size: var(--fbz-font-size-sm);
}

.tasks-layout {
  display: grid;
  grid-template-columns: minmax(280px, 0.9fr) minmax(0, 1.4fr);
  gap: var(--fbz-space-4);
}

.tasks-list,
.task-detail {
  min-height: 420px;
  background: var(--fbz-color-panel-strong);
  border: 1px solid var(--fbz-color-line-soft);
  border-radius: 6px;
  padding: var(--fbz-space-3);
}

.task-row {
  width: 100%;
  border: 1px solid transparent;
  border-radius: var(--fbz-radius-control);
  background: transparent;
  color: var(--fbz-color-text-soft);
  padding: 12px;
  display: flex;
  justify-content: space-between;
  align-items: center;
  gap: var(--fbz-space-3);
  cursor: pointer;
  text-align: left;

  &:hover,
  &.active {
    border-color: var(--fbz-color-line-bright);
    background: var(--fbz-color-panel-elevated);
  }

  &.disabled {
    opacity: 0.65;
  }
}

.task-main {
  min-width: 0;
  display: flex;
  flex-direction: column;
  gap: 4px;
}

.task-name {
  color: var(--fbz-color-text);
  font-size: var(--fbz-font-size-sm);
  font-weight: 800;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.task-meta,
.task-state,
.run-time {
  color: var(--fbz-color-text-muted);
  font-size: var(--fbz-font-size-xs);
}

.task-state {
  border: 1px solid var(--fbz-color-line);
  border-radius: 4px;
  padding: 2px 6px;
  flex-shrink: 0;

  &.enabled {
    border-color: color-mix(in srgb, var(--fbz-color-brand-500) 30%, transparent);
    color: var(--fbz-color-brand-500);
  }
}

.detail-header {
  display: flex;
  justify-content: space-between;
  gap: var(--fbz-space-4);
  padding: var(--fbz-space-2) var(--fbz-space-2) var(--fbz-space-4);
  border-bottom: 1px solid var(--fbz-color-line-soft);

  h3 {
    margin: 0 0 6px;
    font-size: 16px;
  }

  p {
    margin: 0;
    color: var(--fbz-color-text-muted);
    font-size: var(--fbz-font-size-sm);
  }
}

.detail-grid {
  display: grid;
  grid-template-columns: repeat(4, 1fr);
  gap: var(--fbz-space-3);
  padding: var(--fbz-space-4) var(--fbz-space-2);

  div {
    display: flex;
    flex-direction: column;
    gap: 4px;
  }

  .label {
    color: var(--fbz-color-text-muted);
    font-size: var(--fbz-font-size-xs);
    font-weight: 700;
  }
}

.runs-section {
  padding: 0 var(--fbz-space-2) var(--fbz-space-2);
}

.section-title {
  margin-bottom: var(--fbz-space-2);
  color: var(--fbz-color-text-soft);
  font-size: var(--fbz-font-size-sm);
  font-weight: 800;
}

.run-row {
  border-top: 1px solid var(--fbz-color-line-soft);
  padding: 10px 0;
  display: flex;
  justify-content: space-between;
  gap: var(--fbz-space-3);
}

.run-main {
  display: flex;
  gap: var(--fbz-space-2);
  color: var(--fbz-color-text-soft);
  font-size: var(--fbz-font-size-sm);
}

.run-status {
  color: var(--fbz-color-text-muted);
  font-weight: 800;

  &.completed,
  &.succeeded {
    color: var(--fbz-color-brand-500);
  }

  &.failed {
    color: var(--fbz-color-danger-500);
  }
}

.empty-state {
  min-height: 180px;
  display: grid;
  place-content: center;
  text-align: center;
  color: var(--fbz-color-text-muted);
  font-size: var(--fbz-font-size-sm);
  line-height: 1.6;
  padding: var(--fbz-space-4);

  &.compact {
    min-height: 88px;
  }
}

@media (max-width: 900px) {
  .summary-grid,
  .detail-grid {
    grid-template-columns: repeat(2, 1fr);
  }

  .tasks-layout {
    grid-template-columns: 1fr;
  }
}
</style>
