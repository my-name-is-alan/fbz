<script setup lang="ts">
import { useUiStore } from "@/stores/ui.ts";

const uiStore = useUiStore();

interface LogEntry {
  id: string;
  time: string;
  level: "info" | "success" | "warning" | "error";
  category: string;
  message: string;
}

const logs = ref<LogEntry[]>([
  {
    id: "1",
    time: "13:10:02",
    level: "info",
    category: "SYSTEM",
    message: "Fbz Media Server v0.1.0 starting up...",
  },
  {
    id: "2",
    time: "13:10:03",
    level: "info",
    category: "DATABASE",
    message: "Initializing SQLite database connection...",
  },
  {
    id: "3",
    time: "13:10:04",
    level: "success",
    category: "DATABASE",
    message: "Database schema is up-to-date. Loaded 5 tables.",
  },
  {
    id: "4",
    time: "13:10:05",
    level: "info",
    category: "SCRAPER",
    message: "Registering TMDB metadata provider with API key token...",
  },
  {
    id: "5",
    time: "13:10:06",
    level: "success",
    category: "PLUGINS",
    message: "Loaded 3 system plugins: [AutoSubtitles, WebhookNotifier, PlayerVlc]",
  },
  {
    id: "6",
    time: "13:10:08",
    level: "info",
    category: "LIBRARY",
    message: "Scanning library: 电影 (Movie)...",
  },
  {
    id: "7",
    time: "13:10:12",
    level: "success",
    category: "SCRAPER",
    message: "Successfully scraped metadata for '流浪地球 2' from TMDB (ID: 1040148)",
  },
  {
    id: "8",
    time: "13:10:15",
    level: "warning",
    category: "TRANSCODER",
    message: "Hardware acceleration: NVENC init failed, falling back to VAAPI/CPU transcoding.",
  },
  {
    id: "9",
    time: "13:10:20",
    level: "info",
    category: "NETWORK",
    message: "Web UI listener bound to http://0.0.0.0:8096",
  },
  {
    id: "10",
    time: "13:12:44",
    level: "info",
    category: "SESSION",
    message: "User Admin logged in from 192.168.1.5",
  },
]);

const currentLevelFilter = ref<"all" | "info" | "success" | "warning" | "error">("all");
const searchQuery = ref("");
const autoScroll = ref(true);
const terminalBody = ref<HTMLElement | null>(null);

const filteredLogs = computed(() => {
  return logs.value.filter((log) => {
    const matchesLevel =
      currentLevelFilter.value === "all" || log.level === currentLevelFilter.value;
    const matchesSearch =
      !searchQuery.value.trim() ||
      log.message.toLowerCase().includes(searchQuery.value.toLowerCase()) ||
      log.category.toLowerCase().includes(searchQuery.value.toLowerCase());
    return matchesLevel && matchesSearch;
  });
});

function scrollToBottom() {
  if (!autoScroll.value || !terminalBody.value) return;
  nextTick(() => {
    if (terminalBody.value) {
      terminalBody.value.scrollTop = terminalBody.value.scrollHeight;
    }
  });
}

watch(() => logs.value.length, scrollToBottom);
watch(filteredLogs, scrollToBottom);

const logCategories = ["SCRAPER", "TRANSCODER", "LIBRARY", "SESSION", "DISK", "SYSTEM"];
const logMessages = {
  info: [
    "Scanning folder /media/nas/电影/科幻 for new items...",
    "Transcoding session stream-4 initialized.",
    "Checking for updates for TMDB Scraper plugin...",
    "Incoming connection from client: AppleTV / Infuse.",
    "Disk controller checked partitions: /dev/sdb1 is healthy.",
    "Background cache cleanup routine scheduled.",
  ],
  success: [
    "Successfully matching metadata for '蜘蛛侠：纵横宇宙' (ID: 569094).",
    "Transcoding chunk 45 written to memory buffer.",
    "Database backup successfully saved to /media/backups/db-backup.sql.",
    "Plugin 'AutoSubtitles' updated to version 1.2.4.",
    "Completed physical path rescan in 12.4 seconds.",
  ],
  warning: [
    "TMDB API rate-limit threshold reached 80%. Delaying requests by 500ms.",
    "Bitrate exceeds Wi-Fi bandwidth for user Alan (suggesting transcode).",
    "Missing NFO metadata file for movie /media/nas/电影/未知影片.mp4.",
    "Subtitles download failed for 'Oppenheimer' (language: en). Status: 404.",
    "System memory usage exceeded 85%.",
  ],
  error: [
    "Failed to write metadata cache for movie ID 440212. Read-only filesystem?",
    "Transcoding engine error: Codec h264_nvenc device not found.",
    "Unable to connect to subtitle indexer opensubtitles.org. Timeout.",
    "Failed to start filesystem monitor on /media/remote_share: Connection refused.",
    "Critical Exception: Database lock acquisition timeout.",
  ],
};

let intervalId: any = null;

onMounted(() => {
  scrollToBottom();

  intervalId = setInterval(() => {
    const rand = Math.random();
    let level: "info" | "success" | "warning" | "error" = "info";
    if (rand > 0.9) level = "error";
    else if (rand > 0.65) level = "warning";
    else if (rand > 0.35) level = "success";

    const messages = logMessages[level];
    const message = messages[Math.floor(Math.random() * messages.length)];
    const category = logCategories[Math.floor(Math.random() * logCategories.length)];

    const now = new Date();
    const time = now.toTimeString().split(" ")[0];

    logs.value.push({
      id: String(Date.now() + Math.random()),
      time,
      level,
      category,
      message,
    });

    if (logs.value.length > 150) {
      logs.value.shift();
    }
  }, 2500);
});

onBeforeUnmount(() => {
  if (intervalId) clearInterval(intervalId);
});

function clearLogs() {
  logs.value = [];
  uiStore.showToast("系统控制台日志已清空。", "info");
}

function copyLogs() {
  const text = filteredLogs.value
    .map((log) => `[${log.time}] [${log.level.toUpperCase()}] [${log.category}] ${log.message}`)
    .join("\n");

  navigator.clipboard
    .writeText(text)
    .then(() => {
      uiStore.showToast("日志已成功复制到剪贴板！", "success");
    })
    .catch(() => {
      uiStore.showToast("复制日志失败，请手动选择复制。", "error");
    });
}
</script>

<template>
  <div class="admin-logs-view">
    <div class="terminal-container" aria-label="系统实时日志控制台">
      <header class="terminal-toolbar">
        <div class="filter-group">
          <button
            type="button"
            class="tb-btn"
            :class="{ active: currentLevelFilter === 'all' }"
            @click="currentLevelFilter = 'all'"
          >
            全部日志
          </button>
          <button
            type="button"
            class="tb-btn level-info"
            :class="{ active: currentLevelFilter === 'info' }"
            @click="currentLevelFilter = 'info'"
          >
            信息 (Info)
          </button>
          <button
            type="button"
            class="tb-btn level-success"
            :class="{ active: currentLevelFilter === 'success' }"
            @click="currentLevelFilter = 'success'"
          >
            成功 (Success)
          </button>
          <button
            type="button"
            class="tb-btn level-warning"
            :class="{ active: currentLevelFilter === 'warning' }"
            @click="currentLevelFilter = 'warning'"
          >
            警告 (Warn)
          </button>
          <button
            type="button"
            class="tb-btn level-error"
            :class="{ active: currentLevelFilter === 'error' }"
            @click="currentLevelFilter = 'error'"
          >
            错误 (Error)
          </button>
        </div>

        <div class="actions-group">
          <div class="search-wrapper">
            <input
              type="text"
              v-model="searchQuery"
              placeholder="搜索日志关键字..."
              class="terminal-search-input"
              aria-label="检索日志内容"
            />
            <span class="search-icon">⌕</span>
          </div>

          <button
            type="button"
            class="action-icon-btn"
            title="复制日志"
            aria-label="复制当前过滤日志"
            @click="copyLogs"
          >
            📋
          </button>

          <button
            type="button"
            class="action-icon-btn"
            title="清空面板"
            aria-label="清空当前日志面板"
            @click="clearLogs"
          >
            🗑️
          </button>

          <label class="autoscroll-chk" title="自动滚动到底部">
            <input type="checkbox" v-model="autoScroll" />
            <span class="chk-label">自动滚动</span>
          </label>
        </div>
      </header>

      <div ref="terminalBody" class="terminal-body">
        <div v-if="filteredLogs.length > 0" class="terminal-lines">
          <div
            v-for="log in filteredLogs"
            :key="log.id"
            class="log-line"
            :class="`log-${log.level}`"
          >
            <span class="log-time">[{{ log.time }}]</span>
            <span class="log-level-badge">{{ log.level.toUpperCase() }}</span>
            <span class="log-category">[{{ log.category }}]</span>
            <span class="log-message">{{ log.message }}</span>
          </div>
        </div>
        <div v-else class="terminal-empty">
          <span class="empty-glow">⚡</span>
          <span class="text">暂无符合过滤条件的系统日志输出</span>
        </div>
      </div>

      <footer class="terminal-footer">
        <span>实时连接: ACTIVE</span>
        <span class="spacer" />
        <span>已显示: {{ filteredLogs.length }} / {{ logs.length }} 条</span>
      </footer>
    </div>
  </div>
</template>

<style scoped lang="scss">
.admin-logs-view {
  display: flex;
  flex-direction: column;
}

.terminal-container {
  display: flex;
  flex-direction: column;
  height: 60vh;
  min-height: 480px;
  background: #0d0d10;
  border: 1px solid var(--fbz-color-line-bright);
  border-radius: 6px;
  overflow: hidden;
  box-shadow: 0 10px 30px rgba(0, 0, 0, 0.4);
  font-family:
    ui-monospace, SFMono-Regular, Menlo, Monaco, Consolas, "Liberation Mono", "Courier New",
    monospace;
  font-size: 13px;
}

.terminal-toolbar {
  background: var(--fbz-color-panel-strong);
  border-bottom: 1px solid var(--fbz-color-line);
  padding: 8px 12px;
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 12px;
  flex-wrap: wrap;

  @media (max-width: 768px) {
    flex-direction: column;
    align-items: stretch;
  }
}

.filter-group {
  display: flex;
  gap: 4px;
  flex-wrap: wrap;
}

.tb-btn {
  background: transparent;
  border: 1px solid var(--fbz-color-line);
  color: var(--fbz-color-text-muted);
  height: 28px;
  padding: 0 10px;
  border-radius: var(--fbz-radius-control);
  font-weight: 700;
  font-size: 11px;
  cursor: pointer;
  transition: all var(--fbz-motion-fast);

  &:hover {
    color: var(--fbz-color-text);
    border-color: var(--fbz-color-line-bright);
    background: var(--fbz-color-panel-elevated);
  }

  &.active {
    background: var(--fbz-color-panel-elevated);
    color: var(--fbz-color-text);
    border-color: var(--fbz-color-line-bright);
  }

  &.level-info.active {
    color: #38bdf8;
    border-color: #38bdf8;
    background: rgba(56, 189, 248, 0.08);
  }

  &.level-success.active {
    color: #22c55e;
    border-color: #22c55e;
    background: rgba(34, 197, 94, 0.08);
  }

  &.level-warning.active {
    color: #f59e0b;
    border-color: #f59e0b;
    background: rgba(245, 158, 11, 0.08);
  }

  &.level-error.active {
    color: #ef4444;
    border-color: #ef4444;
    background: rgba(239, 68, 68, 0.08);
  }
}

.actions-group {
  display: flex;
  align-items: center;
  gap: 10px;
  flex-wrap: wrap;

  @media (max-width: 768px) {
    justify-content: space-between;
  }
}

.search-wrapper {
  position: relative;
  display: flex;
  align-items: center;

  .terminal-search-input {
    width: 160px;
    height: 28px;
    background: var(--fbz-color-panel);
    border: 1px solid var(--fbz-color-line);
    border-radius: var(--fbz-radius-control);
    padding: 0 24px 0 8px;
    color: var(--fbz-color-text);
    font-size: 11px;
    font-family: inherit;
    transition: all var(--fbz-motion-fast);

    &:focus {
      outline: none;
      border-color: var(--fbz-color-brand-500);
      width: 200px;
    }
  }

  .search-icon {
    position: absolute;
    right: 8px;
    color: var(--fbz-color-text-muted);
    font-size: 13px;
    pointer-events: none;
  }
}

.action-icon-btn {
  background: transparent;
  border: 1px solid var(--fbz-color-line);
  color: var(--fbz-color-text-soft);
  width: 28px;
  height: 28px;
  border-radius: var(--fbz-radius-control);
  display: grid;
  place-content: center;
  cursor: pointer;
  font-size: 12px;
  transition: all var(--fbz-motion-fast);

  &:hover {
    border-color: var(--fbz-color-line-bright);
    background: var(--fbz-color-panel-elevated);
  }
}

.autoscroll-chk {
  display: flex;
  align-items: center;
  gap: 6px;
  cursor: pointer;
  font-size: 11px;
  color: var(--fbz-color-text-soft);
  user-select: none;

  input {
    accent-color: var(--fbz-color-brand-500);
  }
}

.terminal-body {
  flex: 1;
  overflow-y: auto;
  padding: 12px;
  background: #09090b;
  display: flex;
  flex-direction: column;
  scrollbar-width: thin;
  scrollbar-color: var(--fbz-color-line-bright) transparent;
}

.terminal-lines {
  display: flex;
  flex-direction: column;
  gap: 4px;
}

.log-line {
  line-height: 1.6;
  white-space: pre-wrap;
  word-break: break-all;

  .log-time {
    color: #71717a;
    margin-right: 6px;
  }

  .log-level-badge {
    font-size: 10px;
    font-weight: 800;
    padding: 0px 4px;
    border-radius: 2px;
    margin-right: 6px;
  }

  .log-category {
    color: #a1a1aa;
    margin-right: 6px;
    font-weight: 600;
  }

  &.log-info {
    color: #e4e4e7;
    .log-level-badge {
      background: rgba(56, 189, 248, 0.15);
      color: #38bdf8;
    }
  }

  &.log-success {
    color: #a7f3d0;
    .log-level-badge {
      background: rgba(34, 197, 94, 0.15);
      color: #22c55e;
    }
  }

  &.log-warning {
    color: #fde68a;
    .log-level-badge {
      background: rgba(245, 158, 11, 0.15);
      color: #f59e0b;
    }
  }

  &.log-error {
    color: #fca5a5;
    .log-level-badge {
      background: rgba(239, 68, 68, 0.15);
      color: #ef4444;
    }
  }
}

.terminal-empty {
  flex: 1;
  display: flex;
  flex-direction: column;
  align-items: center;
  justify-content: center;
  color: var(--fbz-color-text-muted);
  gap: 12px;

  .empty-glow {
    font-size: 32px;
    animation: pulse 2s infinite ease-in-out;
  }
}

@keyframes pulse {
  0%,
  100% {
    opacity: 0.3;
  }
  50% {
    opacity: 0.8;
  }
}

.terminal-footer {
  background: var(--fbz-color-panel-strong);
  border-top: 1px solid var(--fbz-color-line);
  padding: 6px 12px;
  display: flex;
  font-size: 11px;
  color: var(--fbz-color-text-muted);

  .spacer {
    flex: 1;
  }
}
</style>
