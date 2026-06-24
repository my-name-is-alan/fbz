<script setup lang="ts">
import { useLibraryStore } from "@/stores/library.ts";

const libraryStore = useLibraryStore();

// System stats mock data
const cpuUsage = ref(34);
const memUsed = ref(2.4);
const memTotal = ref(8.0);
const diskUsed = ref(1.2);
const diskTotal = ref(4.0);

// Active transcoding streams mock data
const activeStreams = ref([
  {
    id: "stream-1",
    user: "Alan",
    title: "流浪地球3 (2026)",
    quality: "1080P - Direct Play",
    progress: 42,
    player: "Chrome / WebPlayer",
    bitrate: "15.4 Mbps",
  },
]);

// System activities logs
const recentActivities = ref([
  { id: 1, time: "10:24", type: "success", msg: "《阿凡达：水之道》元数据搜刮及海报下载成功" },
  { id: 2, time: "09:42", type: "info", msg: "用户 Alan 登录系统控制台 (192.168.1.10)" },
  { id: 3, time: "08:15", type: "warning", msg: "物理目录 /media/backup 扫描中：跳过只读文件" },
  { id: 4, time: "07:00", type: "success", msg: "系统数据库日常备份成功 (45.2 MB)" },
]);

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

// Simulate live CPU / Mem / Disk changes slightly
let interval: any;
onMounted(() => {
  interval = setInterval(() => {
    cpuUsage.value = Math.max(
      12,
      Math.min(95, cpuUsage.value + Math.floor(Math.random() * 11) - 5),
    );
    const memOffset = Math.random() * 0.2 - 0.1;
    memUsed.value = Math.max(
      1.8,
      Math.min(6.5, parseFloat((memUsed.value + memOffset).toFixed(1))),
    );
  }, 3000);
});

onBeforeUnmount(() => {
  clearInterval(interval);
});
</script>

<template>
  <div class="admin-dashboard-view">
    <!-- Row 1: System Hardware Metrics -->
    <div class="metrics-grid">
      <!-- CPU Usage -->
      <div class="metric-card">
        <div class="card-title">CPU 使用率</div>
        <div class="progress-container">
          <div class="progress-circle-wrap">
            <svg class="progress-ring" width="80" height="80">
              <circle
                class="progress-ring-bg"
                stroke="var(--fbz-color-line)"
                stroke-width="6"
                fill="transparent"
                r="34"
                cx="40"
                cy="40"
              />
              <circle
                class="progress-ring-bar"
                stroke="var(--fbz-color-brand-500)"
                stroke-width="6"
                fill="transparent"
                r="34"
                cx="40"
                cy="40"
                :style="{
                  strokeDasharray: `${2 * Math.PI * 34}`,
                  strokeDashoffset: `${2 * Math.PI * 34 * (1 - cpuUsage / 100)}`,
                }"
              />
            </svg>
            <div class="percentage-display">{{ cpuUsage }}%</div>
          </div>
          <div class="metric-meta">Intel Core i7-12700H</div>
        </div>
      </div>

      <!-- Memory Usage -->
      <div class="metric-card">
        <div class="card-title">系统内存</div>
        <div class="progress-bar-wrap">
          <div class="value-row">
            <span class="value">{{ memUsed }} GB / {{ memTotal }} GB</span>
            <span class="percent">{{ Math.round((memUsed / memTotal) * 100) }}%</span>
          </div>
          <div class="track">
            <div class="fill" :style="{ width: `${(memUsed / memTotal) * 100}%` }" />
          </div>
          <div class="metric-meta">DDR5 4800MHz</div>
        </div>
      </div>

      <!-- Disk Space -->
      <div class="metric-card">
        <div class="card-title">存储空间</div>
        <div class="progress-bar-wrap">
          <div class="value-row">
            <span class="value">{{ diskUsed }} TB / {{ diskTotal }} TB</span>
            <span class="percent">{{ Math.round((diskUsed / diskTotal) * 100) }}%</span>
          </div>
          <div class="track">
            <div class="fill" :style="{ width: `${(diskUsed / diskTotal) * 100}%` }" />
          </div>
          <div class="metric-meta">已挂载 3 个硬盘分区</div>
        </div>
      </div>
    </div>

    <!-- Row 2: Library Totals & Active Streams -->
    <div class="dashboard-mid-row">
      <!-- Media Library Totals -->
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
              <span class="number">{{ libraryStore.libraries.length }}</span>
              <span class="label">媒体库数量</span>
            </div>
          </div>
        </div>
      </div>

      <!-- Active Streams -->
      <div class="streams-section">
        <div class="sub-label">
          <span>正在播放的流媒体</span>
          <span class="active-count-badge" v-if="activeStreams.length > 0">{{
            activeStreams.length
          }}</span>
        </div>

        <div class="streams-list" v-if="activeStreams.length > 0">
          <div v-for="stream in activeStreams" :key="stream.id" class="stream-card">
            <div class="stream-user">
              <span class="user-avatar">{{ stream.user.charAt(0) }}</span>
              <div class="user-details">
                <span class="username">{{ stream.user }}</span>
                <span class="client">{{ stream.player }}</span>
              </div>
            </div>
            <div class="stream-playback">
              <span class="title">{{ stream.title }}</span>
              <span class="quality">{{ stream.quality }} · {{ stream.bitrate }}</span>
              <div class="progress-bar">
                <div class="fill" :style="{ width: `${stream.progress}%` }" />
              </div>
            </div>
          </div>
        </div>
        <div class="streams-empty" v-else>
          <span class="empty-icon">💤</span>
          <span>当前没有活动的播放连接</span>
        </div>
      </div>
    </div>

    <!-- Row 3: Activity Logs -->
    <div class="activity-section">
      <div class="sub-label">最近任务动态</div>
      <div class="activity-list">
        <div v-for="act in recentActivities" :key="act.id" class="activity-item">
          <span class="time">{{ act.time }}</span>
          <span class="status-dot" :class="act.type" />
          <span class="message">{{ act.msg }}</span>
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
  gap: 16px;

  .card-title {
    font-size: 11px;
    font-weight: 700;
    text-transform: uppercase;
    color: var(--fbz-color-text-muted);
    letter-spacing: 0.5px;
  }
}

.progress-container {
  display: flex;
  align-items: center;
  gap: 20px;
}

.progress-circle-wrap {
  position: relative;
  width: 80px;
  height: 80px;

  .progress-ring {
    transform: rotate(-90deg);
  }

  .progress-ring-bar {
    transition: stroke-dashoffset 0.4s ease;
  }

  .percentage-display {
    position: absolute;
    inset: 0;
    display: grid;
    place-content: center;
    font-family: var(--fbz-font-display);
    font-size: 16px;
    font-weight: 800;
    color: var(--fbz-color-text);
  }
}

.progress-bar-wrap {
  display: flex;
  flex-direction: column;
  gap: 8px;

  .value-row {
    display: flex;
    justify-content: space-between;
    font-size: 13px;

    .value {
      font-weight: 700;
      color: var(--fbz-color-text-soft);
    }

    .percent {
      font-family: var(--fbz-font-display);
      font-weight: 800;
      color: var(--fbz-color-brand-500);
    }
  }

  .track {
    height: 6px;
    background: var(--fbz-color-line);
    border-radius: 3px;
    overflow: hidden;
  }

  .fill {
    height: 100%;
    background: var(--fbz-color-brand-500);
    border-radius: 3px;
    transition: width 0.4s ease;
  }
}

.metric-meta {
  font-size: var(--fbz-font-size-xs);
  color: var(--fbz-color-text-muted);
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

  .active-count-badge {
    background: var(--fbz-color-brand-500);
    color: #07120a;
    font-family: var(--fbz-font-display);
    font-size: 9px;
    font-weight: 900;
    border-radius: var(--fbz-radius-round);
    padding: 1px 6px;
    line-height: 1.2;
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

.streams-section {
  display: flex;
  flex-direction: column;
}

.streams-list {
  display: flex;
  flex-direction: column;
  gap: var(--fbz-space-2);
}

.stream-card {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: var(--fbz-space-4);
  padding: var(--fbz-space-4);
  background: var(--fbz-color-panel-strong);
  border: 1px solid var(--fbz-color-line-soft);
  border-radius: 6px;

  @media (max-width: 600px) {
    flex-direction: column;
    align-items: flex-start;
    gap: var(--fbz-space-3);
  }

  .stream-user {
    display: flex;
    align-items: center;
    gap: 10px;
    flex-shrink: 0;

    .user-avatar {
      width: 32px;
      height: 32px;
      border-radius: 50%;
      background: var(--fbz-color-brand-500);
      color: #07120a;
      display: grid;
      place-content: center;
      font-weight: 800;
      font-size: 13px;
    }

    .user-details {
      display: flex;
      flex-direction: column;

      .username {
        font-size: var(--fbz-font-size-md);
        font-weight: 700;
        color: var(--fbz-color-text);
      }

      .client {
        font-size: 10px;
        color: var(--fbz-color-text-muted);
      }
    }
  }

  .stream-playback {
    flex: 1;
    display: flex;
    flex-direction: column;
    gap: 4px;
    width: 100%;

    .title {
      font-size: 13px;
      font-weight: 700;
      color: var(--fbz-color-text-soft);
    }

    .quality {
      font-size: 11px;
      color: var(--fbz-color-text-muted);
    }

    .progress-bar {
      height: 4px;
      background: var(--fbz-color-line);
      border-radius: 2px;
      overflow: hidden;
      margin-top: 4px;

      .fill {
        height: 100%;
        background: var(--fbz-color-brand-500);
        border-radius: 2px;
      }
    }
  }
}

.streams-empty {
  flex: 1;
  border: 1px dashed var(--fbz-color-line-soft);
  border-radius: 6px;
  background: var(--fbz-color-panel-strong);
  display: flex;
  flex-direction: column;
  align-items: center;
  justify-content: center;
  gap: 10px;
  padding: 40px;
  color: var(--fbz-color-text-muted);
  text-align: center;
  font-size: var(--fbz-font-size-sm);

  .empty-icon {
    font-size: 24px;
  }
}

.activity-section {
  display: flex;
  flex-direction: column;
}

.activity-list {
  background: var(--fbz-color-panel-strong);
  border: 1px solid var(--fbz-color-line-soft);
  border-radius: 6px;
  padding: var(--fbz-space-4) var(--fbz-space-5);
  display: flex;
  flex-direction: column;
  gap: 12px;
}

.activity-item {
  display: flex;
  align-items: center;
  gap: 12px;
  font-size: var(--fbz-font-size-sm);

  .time {
    font-family: var(--fbz-font-display);
    color: var(--fbz-color-text-muted);
    width: 45px;
    flex-shrink: 0;
  }

  .status-dot {
    width: 6px;
    height: 6px;
    border-radius: 50%;
    flex-shrink: 0;

    &.success {
      background: var(--fbz-color-brand-500);
      box-shadow: 0 0 6px var(--fbz-color-brand-500);
    }

    &.info {
      background: var(--fbz-color-cyan-500);
      box-shadow: 0 0 6px var(--fbz-color-cyan-500);
    }

    &.warning {
      background: var(--fbz-color-amber-500);
      box-shadow: 0 0 6px var(--fbz-color-amber-500);
    }
  }

  .message {
    color: var(--fbz-color-text-soft);
    line-height: 1.4;
  }
}
</style>
