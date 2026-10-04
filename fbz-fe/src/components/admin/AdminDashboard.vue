<script setup lang="ts">
import { useLibraryStore } from "@/stores/library.ts";
import {
  readSession,
  serverRequest,
  errorMessage,
  serverAddress,
} from "@/service/modules/server.ts";
import type { ServerJob, ServerPlayback } from "@/service/modules/server.ts";
const library = useLibraryStore();
const loading = ref(false);
const error = ref("");
const online = ref(false);
const version = ref("—");
const sessions = ref<ServerPlayback[]>([]);
const jobs = ref<ServerJob[]>([]);
const counts = ref({ MovieCount: 0, EpisodeCount: 0, SeriesCount: 0 });
const updated = ref("");
async function refresh() {
  if (loading.value || !readSession()) return;
  loading.value = true;
  error.value = "";
  try {
    const [info, totals, playing, tasks] = await Promise.all([
      serverRequest<{ Version: string }>("/emby/System/Info"),
      serverRequest<typeof counts.value>("/emby/Items/Counts"),
      serverRequest<ServerPlayback[]>("/api/admin/playback"),
      serverRequest<ServerJob[]>("/api/admin/jobs?limit=8"),
    ]);
    version.value = info.Version;
    counts.value = totals;
    sessions.value = playing;
    jobs.value = tasks;
    online.value = true;
    updated.value = new Date().toLocaleTimeString();
    await library.refresh();
  } catch (err) {
    error.value = errorMessage(err);
    online.value = false;
  } finally {
    loading.value = false;
  }
}
const jobNames: Record<string, string> = {
  "library.scan": "媒体库扫描",
  "media.probe": "媒体信息探测",
  "metadata.refresh": "元数据刷新",
};
const status: Record<string, string> = {
  queued: "排队中",
  running: "执行中",
  completed: "已完成",
  succeeded: "已完成",
  failed: "失败",
  cancelled: "已取消",
};
function time(ticks: number) {
  const seconds = Math.floor(ticks / 1e7);
  return `${Math.floor(seconds / 60)}:${String(seconds % 60).padStart(2, "0")}`;
}
onMounted(refresh);
useIntervalFn(refresh, 5000);
</script>

<template>
  <section class="dashboard">
    <header class="server-header">
      <div>
        <p class="eyebrow">你的私人媒体服务器</p>
        <h1>FBZ Server</h1>
      </div>
      <div class="server-state">
        <span class="connection" :class="{ online }"><i />{{ online ? "已连接" : "未连接" }}</span
        ><span class="version">版本 {{ version }}</span
        ><button class="du-btn du-btn-sm" :disabled="loading" @click="refresh">
          {{ loading ? "更新中…" : "刷新" }}
        </button>
      </div>
    </header>
    <p v-if="error" class="error" role="alert">
      {{ error }} <RouterLink to="/user/login">重新连接 →</RouterLink>
    </p>
    <div v-if="!readSession()" class="connect-prompt">
      <h2>连接你的媒体服务器</h2>
      <p>登录后，在这里查看媒体库、播放会话和扫描任务。</p>
      <RouterLink to="/user/login">连接服务器 →</RouterLink>
    </div>
    <dl class="metrics">
      <div>
        <dt>电影</dt>
        <dd>{{ online ? counts.MovieCount : "—" }}</dd>
      </div>
      <div>
        <dt>剧集</dt>
        <dd>{{ online ? counts.SeriesCount : "—" }}</dd>
      </div>
      <div>
        <dt>分集</dt>
        <dd>{{ online ? counts.EpisodeCount : "—" }}</dd>
      </div>
      <div>
        <dt>媒体库</dt>
        <dd>{{ online ? library.libraries.length : "—" }}</dd>
      </div>
      <div>
        <dt>播放会话</dt>
        <dd>{{ online ? sessions.length : "—" }}</dd>
      </div>
    </dl>
    <section class="playback-section">
      <div class="section-head">
        <div>
          <h2>正在播放</h2>
          <p>查看播放状态与进度，暂停后也会保留当前会话。</p>
        </div>
        <span class="meta">{{ sessions.length }} 个会话</span>
      </div>
      <div v-if="sessions.length" class="session-grid">
        <article
          v-for="(session, index) in sessions"
          :key="`${session.Id}-${index}`"
          class="session-card"
        >
          <div class="session-heading">
            <div class="media-monogram">{{ session.Name.slice(0, 1) }}</div>
            <div>
              <h3>{{ session.Name }}</h3>
              <p>{{ session.UserName }}</p>
              <span class="playing-label">{{ session.IsPaused ? "已暂停" : "正在播放" }}</span>
            </div>
          </div>
          <div class="timeline">
            <span
              >{{ time(session.PositionTicks) }} /
              {{ session.RunTimeTicks ? time(session.RunTimeTicks) : "—" }}</span
            ><span>{{
              session.RunTimeTicks
                ? Math.min(100, Math.round((session.PositionTicks / session.RunTimeTicks) * 100)) +
                  "%"
                : ""
            }}</span>
          </div>
          <progress
            :value="session.PositionTicks"
            :max="session.RunTimeTicks || Math.max(1, session.PositionTicks)"
          />
          <div class="stream-info">
            <span>播放方式</span
            ><strong>{{
              {
                direct_play: "直接播放",
                direct_stream: "直接串流",
                transcode: "转码播放",
                strm_redirect: "远程直连",
              }[session.PlayMethod] ?? session.PlayMethod
            }}</strong>
          </div>
        </article>
      </div>
      <div v-else class="empty-playback">
        <svg
          viewBox="0 0 48 48"
          width="42"
          height="42"
          fill="none"
          stroke="currentColor"
          stroke-width="1.2"
        >
          <rect x="5" y="9" width="38" height="26" rx="4" />
          <path d="m21 17 9 5-9 5z M17 41h14 M24 35v6" />
        </svg>
        <div>
          <h3>{{ online ? "还没有正在播放的内容" : "等待服务器连接" }}</h3>
          <p>
            {{
              online
                ? "从媒体库选择一部影片，或使用 Emby 兼容客户端开始播放。"
                : "连接后将显示服务器返回的播放状态。"
            }}
          </p>
        </div>
        <RouterLink to="/library">打开媒体库 →</RouterLink>
      </div>
    </section>
    <section>
      <div class="section-head">
        <div>
          <h2>任务动态</h2>
          <p>扫描、媒体探测与元数据任务的最新状态。</p>
        </div>
        <RouterLink to="/admin/libraries">管理媒体库 →</RouterLink>
      </div>
      <p v-if="!jobs.length" class="empty-activity">暂无任务。添加媒体库后，启动第一次扫描。</p>
      <div v-for="job in jobs" :key="job.id" class="activity">
        <span class="activity-dot" :class="job.status" />
        <div>
          <strong>{{ jobNames[job.jobType] ?? "后台任务" }}</strong>
          <p v-if="job.lastError" class="error">{{ job.lastError }}</p>
        </div>
        <span>{{ status[job.status] ?? job.status }}</span
        ><time>{{ new Date(job.updatedAt).toLocaleTimeString() }}</time>
      </div>
    </section>
    <footer>
      <span>{{ readSession() ? serverAddress() : "FBZ · 自托管媒体库" }}</span
      ><span v-if="updated">最后更新 {{ updated }}</span>
    </footer>
  </section>
</template>

<style scoped lang="scss">
.dashboard {
  max-width: 1180px;
  font-size: 13px;
}
.server-header,
.server-state,
.section-head {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 24px;
}
.server-header {
  padding: 10px 0 28px;
}
h1 {
  margin: 8px 0 0;
  font-size: 28px;
  font-weight: 600;
  letter-spacing: -0.8px;
}
.eyebrow {
  color: var(--fbz-color-text-muted);
  font-size: 11px;
  letter-spacing: 1px;
  margin: 0;
}
.server-state {
  justify-content: flex-end;
  font-size: 12px;
}
.version {
  color: var(--fbz-color-text-muted);
}
.connection {
  display: flex;
  align-items: center;
  gap: 8px;
  padding: 7px 12px;
  border: 1px solid var(--fbz-color-line);
  border-radius: 24px;
  color: var(--fbz-color-text-muted);
}
.connection i {
  width: 5px;
  height: 5px;
  border-radius: 50%;
  background: currentColor;
}
.connection.online {
  color: var(--fbz-color-brand-500);
}
button {
  background: none;
  color: var(--fbz-color-text-soft);
  border: 1px solid var(--fbz-color-line);
  border-radius: 6px;
  padding: 7px 12px;
  cursor: pointer;
}
button:disabled {
  opacity: 0.5;
}
.metrics {
  display: grid;
  grid-template-columns: repeat(5, 1fr);
  border-top: 1px solid var(--fbz-color-line-soft);
  padding: 26px 0;
  margin: 0 0 34px;
  gap: 24px;
}
dt {
  color: var(--fbz-color-text-muted);
  font-size: 12px;
  margin-bottom: 12px;
}
dd {
  margin: 0;
  font-size: 22px;
  font-weight: 600;
  font-variant-numeric: tabular-nums;
  color: var(--fbz-color-brand-500);
}
h2 {
  font-size: 22px;
  font-weight: 550;
  margin: 0 0 9px;
  letter-spacing: -0.4px;
}
h3 {
  font-size: 14px;
  margin: 0 0 8px;
  font-weight: 550;
}
p {
  color: var(--fbz-color-text-muted);
  line-height: 1.6;
  margin: 0;
}
.section-head {
  margin-bottom: 24px;
}
.meta,
time {
  color: var(--fbz-color-text-muted);
  font-size: 12px;
}
.playback-section {
  margin-bottom: 56px;
}
.session-grid {
  display: grid;
  grid-template-columns: repeat(auto-fill, minmax(250px, 1fr));
  gap: 20px;
}
.session-card {
  padding: 18px;
  background: var(--fbz-color-panel);
  border: 1px solid var(--fbz-color-line-soft);
  border-radius: 8px;
  max-width: 360px;
}
.session-heading {
  display: flex;
  gap: 16px;
  margin-bottom: 20px;
}
.media-monogram {
  display: grid;
  place-items: center;
  width: 64px;
  height: 88px;
  border-radius: 4px;
  background: var(--fbz-color-panel-elevated);
  color: var(--fbz-color-text-soft);
  font-size: 28px;
  flex-shrink: 0;
}
.playing-label {
  display: inline-block;
  color: var(--fbz-color-brand-500);
  font-size: 11px;
  margin-top: 10px;
}
.timeline,
.stream-info {
  display: flex;
  justify-content: space-between;
  gap: 12px;
  font-size: 11px;
  color: var(--fbz-color-text-muted);
}
.stream-info {
  border-top: 1px solid var(--fbz-color-line-soft);
  padding-top: 14px;
  margin-top: 16px;
}
.stream-info strong {
  font-weight: 500;
  color: var(--fbz-color-text-soft);
}
progress {
  appearance: none;
  width: 100%;
  height: 3px;
  margin: 10px 0 0;
  border: 0;
}
progress::-webkit-progress-bar {
  background: var(--fbz-color-line);
}
progress::-webkit-progress-value {
  background: var(--fbz-color-brand-500);
}
.empty-playback {
  display: flex;
  align-items: center;
  gap: 22px;
  padding: 32px 0;
  border-top: 1px solid var(--fbz-color-line-soft);
  border-bottom: 1px solid var(--fbz-color-line-soft);
}
.empty-playback svg {
  color: var(--fbz-color-text-muted);
  flex-shrink: 0;
}
.empty-playback a {
  margin-left: auto;
  white-space: nowrap;
}
a {
  color: var(--fbz-color-text-soft);
  text-decoration: none;
  font-size: 12px;
}
a:hover {
  color: var(--fbz-color-brand-500);
}
.activity {
  display: flex;
  align-items: center;
  gap: 16px;
  padding: 18px 0;
  border-top: 1px solid var(--fbz-color-line-soft);
}
.activity > div {
  flex: 1;
  min-width: 0;
}
.activity strong {
  font-weight: 500;
}
.activity > span:not(.activity-dot) {
  color: var(--fbz-color-text-soft);
  font-size: 12px;
}
.activity-dot {
  width: 6px;
  height: 6px;
  border-radius: 50%;
  background: var(--fbz-color-text-muted);
}
.activity-dot.completed,
.activity-dot.succeeded {
  background: var(--fbz-color-brand-500);
}
.activity-dot.failed {
  background: var(--fbz-color-danger-500);
}
.empty-activity {
  padding: 24px 0;
  border-top: 1px solid var(--fbz-color-line-soft);
}
.error {
  color: var(--fbz-color-danger-500);
  overflow-wrap: anywhere;
}
footer {
  margin-top: 48px;
  display: flex;
  justify-content: space-between;
  gap: 16px;
  color: var(--fbz-color-text-muted);
  font-size: 11px;
  overflow-wrap: anywhere;
}
.connect-prompt {
  padding: 24px;
  margin-bottom: 24px;
  background: var(--fbz-color-panel);
  border-radius: 8px;
}
.connect-prompt a {
  display: inline-block;
  margin-top: 14px;
  color: var(--fbz-color-brand-500);
}
:is(button, a):focus-visible {
  outline: 2px solid var(--fbz-color-brand-500);
  outline-offset: 4px;
}
@media (max-width: 600px) {
  .server-header {
    align-items: flex-start;
    flex-direction: column;
    gap: 20px;
  }
  .server-state {
    gap: 14px;
  }
  .metrics {
    grid-template-columns: repeat(3, 1fr);
    gap: 24px 12px;
  }
  .empty-playback {
    flex-wrap: wrap;
  }
  .empty-playback a {
    margin: 0;
  }
  .section-head {
    gap: 12px;
    align-items: flex-start;
  }
  .activity time {
    display: none;
  }
  footer {
    flex-direction: column;
  }
}
</style>
