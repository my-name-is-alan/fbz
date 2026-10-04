<script setup lang="ts">
import { useLibraryStore } from "@/stores/library.ts";
import { serverRequest, errorMessage, readSession } from "@/service/modules/server.ts";
import type { ServerJob } from "@/service/modules/server.ts";
const library = useLibraryStore();
const emit = defineEmits<{ linked: [ids: string[]] }>();
const creating = ref(false);
const busy = ref(false);
const name = ref("");
const path = ref("");
const kind = ref("movies");
interface CloudMount {
  id: string;
  name: string;
  mountPath: string;
  libraryId: string | null;
  status: string;
  refreshMinutes: number;
  nfoSource: string;
  imageCache: string;
  lastError?: string;
  scanned: number;
  imported: number;
  lastRefreshedAt?: string;
  nextRefreshAt?: string;
}
const mounts = ref<CloudMount[]>([]);
const cloudStatus: Record<string, string> = {
  idle: "就绪",
  scanning: "读取目录中",
  importing: "导入资料中",
  failed: "等待重试",
};
const source = ref("cloud");
const mountId = ref("");
const nfoSource = ref("cloud");
const imageCache = ref("on_demand");
const refreshMinutes = ref(60);
const editing = ref<CloudMount>();
const mountOptions = computed(() =>
  mounts.value
    .filter((m) => !m.libraryId)
    .map((m) => ({ label: `${m.mountPath} · ${m.name}`, value: m.id })),
);
watch(mountId, (id) => {
  const mount = mounts.value.find((m) => m.id === id);
  if (mount) refreshMinutes.value = mount.refreshMinutes;
});
watch(source, (value) => {
  if (value === "cloud" && kind.value === "music") kind.value = "movies";
});
function editSettings(mount: CloudMount) {
  editing.value = mount;
  nfoSource.value = mount.nfoSource;
  imageCache.value = mount.imageCache;
  refreshMinutes.value = mount.refreshMinutes;
}
function newLibrary() {
  creating.value = true;
  source.value = "cloud";
  mountId.value = "";
  nfoSource.value = "cloud";
  imageCache.value = "on_demand";
  refreshMinutes.value = 60;
}
async function saveSettings() {
  if (!editing.value) return;
  busy.value = true;
  error.value = "";
  try {
    await serverRequest(`/api/admin/storage/mounts/${editing.value.id}/settings`, {
      nfoSource: nfoSource.value,
      imageCache: imageCache.value,
      refreshMinutes: refreshMinutes.value,
    });
    editing.value = undefined;
    await refresh();
  } catch (e) {
    error.value = errorMessage(e);
  } finally {
    busy.value = false;
  }
}
async function scanCloud(mount: CloudMount) {
  busy.value = true;
  error.value = "";
  try {
    await serverRequest(`/api/admin/storage/mounts/${mount.id}/scan`, {});
    message.value = "云端扫描已排队";
    await refresh();
  } catch (e) {
    error.value = errorMessage(e);
  } finally {
    busy.value = false;
  }
}

const message = ref("");
const error = ref("");
const jobs = ref<ServerJob[]>([]);
const kinds = [
  { label: "电影", value: "movies" },
  { label: "剧集", value: "tvshows" },
  { label: "音乐", value: "music" },
];
async function refresh() {
  await library.refresh();
  if (!readSession()) return;
  try {
    mounts.value = (await serverRequest<{ mounts: CloudMount[] }>("/api/admin/storage")).mounts;
    emit(
      "linked",
      mounts.value.flatMap((m) => (m.libraryId ? [m.libraryId] : [])),
    );
    jobs.value = await serverRequest<ServerJob[]>("/api/admin/jobs?limit=20");
  } catch (err) {
    error.value = errorMessage(err);
  }
}
async function create() {
  if (
    busy.value ||
    !name.value.trim() ||
    (source.value === "local" ? !path.value.trim() : !mountId.value)
  )
    return;
  busy.value = true;
  error.value = "";
  message.value = "";
  try {
    if (source.value === "cloud") {
      await serverRequest(`/api/admin/storage/mounts/${mountId.value}/library`, {
        name: name.value.trim(),
        libraryType: kind.value,
        nfoSource: nfoSource.value,
        imageCache: imageCache.value,
        refreshMinutes: refreshMinutes.value,
      });
    } else
      await serverRequest("/api/admin/libraries", {
        name: name.value.trim(),
        libraryType: kind.value,
        paths: path.value
          .split("\n")
          .map((p) => p.trim())
          .filter(Boolean),
      });
    creating.value = false;
    name.value = "";
    path.value = "";
    message.value = "媒体库已创建。点击扫描开始索引文件。";
    await refresh();
  } catch (err) {
    error.value = errorMessage(err);
  } finally {
    busy.value = false;
  }
}
async function scan(id: string) {
  busy.value = true;
  error.value = "";
  message.value = "";
  try {
    const job = await serverRequest<ServerJob>(`/api/admin/libraries/${id}/scan`, {
      reason: "web_manual",
    });
    message.value = `扫描已排队 · ${job.id.slice(0, 8)}`;
    await refresh();
  } catch (err) {
    error.value = errorMessage(err);
  } finally {
    busy.value = false;
  }
}
const jobNames: Record<string, string> = {
  "library.scan": "媒体库扫描",
  "media.probe": "媒体信息探测",
  "metadata.refresh": "元数据刷新",
};
const status: Record<string, string> = {
  queued: "排队中",
  running: "扫描中",
  completed: "已完成",
  succeeded: "已完成",
  failed: "失败",
  cancelled: "已取消",
};
onMounted(refresh);
useIntervalFn(() => {
  if (!busy.value && readSession()) void refresh();
}, 5000);
</script>

<template>
  <section class="server-libraries">
    <div class="section-toolbar">
      <span>{{ mounts.filter((m) => m.libraryId).length }} 个云盘媒体库</span
      ><button class="du-btn du-btn-sm du-btn-primary primary" @click="newLibrary">
        <BaseIcon name="plus" :size="16" /> 添加云盘媒体库
      </button>
    </div>
    <p v-if="!readSession()" class="notice">
      连接服务器后，即可添加和扫描媒体库。<RouterLink to="/user/login">连接服务器 →</RouterLink>
    </p>
    <BaseModal
      :open="creating"
      title="添加云盘媒体库"
      description="选择内容类型与服务器上的媒体目录。"
      @close="creating = false"
      ><p v-if="error" class="du-alert du-alert-error">{{ error }}</p>
      <form class="create-library" @submit.prevent="create">
        <div class="form-columns">
          <label
            >名称<input class="du-input" v-model="name" required placeholder="例如：电影" /></label
          ><label
            >内容类型<BaseSelect
              v-model="kind"
              :options="source === 'cloud' ? kinds.filter((k) => k.value !== 'music') : kinds"
          /></label>
        </div>
        <label
          >媒体来源<BaseSelect
            v-model="source"
            :options="[{ label: '插件挂载目录', value: 'cloud' }]"
        /></label>
        <template v-if="source === 'cloud'">
          <label
            >挂载地址<BaseSelect
              v-model="mountId"
              :options="[{ label: '选择未关联的挂载目录', value: '' }, ...mountOptions]"
          /></label>
          <p v-if="!mountOptions.length" class="muted">
            没有可用挂载，请先在<RouterLink to="/admin/plugins">光鸭插件</RouterLink>中添加目录。
          </p>
          <CloudLibraryPolicy
            v-model:nfo-source="nfoSource"
            v-model:image-cache="imageCache"
            v-model:refresh-minutes="refreshMinutes"
          />
        </template>
        <label v-else
          >服务器上的媒体目录<textarea
            class="du-textarea"
            v-model="path"
            required
            rows="3"
            placeholder="每行一个完整路径，例如 D:\Media\Movies 或 /media/movies"
          />
        </label>
        <p v-if="source === 'local'" class="muted">
          填写运行 FBZ 服务的机器可访问的目录。创建后再启动扫描。
        </p>
        <button class="du-btn du-btn-sm du-btn-primary primary" :disabled="busy || !readSession()">
          {{ busy ? "正在创建…" : "创建媒体库" }}
        </button>
      </form></BaseModal
    >
    <BaseModal :open="!!editing" title="媒体库来源与扫描策略" @close="editing = undefined"
      ><p v-if="error" class="du-alert du-alert-error">{{ error }}</p>
      <form class="create-library" @submit.prevent="saveSettings">
        <p class="muted">{{ editing?.mountPath }}</p>
        <CloudLibraryPolicy
          v-model:nfo-source="nfoSource"
          v-model:image-cache="imageCache"
          v-model:refresh-minutes="refreshMinutes"
        />
        <p class="muted">修改资料策略后，下次完整扫描生效；已有缓存不会删除。</p>
        <button class="du-btn du-btn-primary" :disabled="busy">保存设置</button>
      </form></BaseModal
    >
    <p v-if="error || library.error" class="error" role="alert">{{ error || library.error }}</p>
    <p v-if="message" role="status" class="notice">{{ message }}</p>
    <div class="library-list">
      <article
        v-for="lib in library.libraries.filter((l) => mounts.some((m) => m.libraryId === l.id))"
        :key="lib.id"
        class="library-line"
      >
        <span class="folder-icon"
          ><svg
            width="24"
            height="24"
            viewBox="0 0 24 24"
            fill="none"
            stroke="currentColor"
            stroke-width="1.4"
          >
            <path d="M3 6h7l2 3h9v11H3z" /></svg
        ></span>
        <div class="library-description">
          <h3>{{ lib.name }}</h3>
          <p v-for="entry in lib.paths" :key="entry">{{ entry }}</p>
          <template v-for="mount in mounts.filter((m) => m.libraryId === lib.id)" :key="mount.id"
            ><p>{{ mount.mountPath }}</p>
            <p>
              {{ mount.refreshMinutes ? `每 ${mount.refreshMinutes} 分钟刷新` : "手动刷新" }} ·
              {{ cloudStatus[mount.status] ?? mount.status }} · 已导入 {{ mount.imported }} 个视频
            </p>
            <p v-if="mount.lastRefreshedAt">
              最近刷新：{{ new Date(mount.lastRefreshedAt).toLocaleString() }}
            </p>
            <p v-if="mount.refreshMinutes && mount.nextRefreshAt">
              下次检查：{{ new Date(mount.nextRefreshAt).toLocaleString() }}
            </p>
            <p v-if="mount.lastError" class="error">{{ mount.lastError }}</p></template
          >
        </div>
        <RouterLink :to="`/library/${lib.id}`">浏览</RouterLink
        ><button
          class="du-btn du-btn-sm"
          v-if="lib.paths?.length"
          :disabled="busy"
          @click="scan(lib.id)"
        >
          扫描媒体库
        </button>
        <template v-else-if="mounts.find((m) => m.libraryId === lib.id)">
          <button
            class="du-btn du-btn-sm"
            :disabled="busy"
            @click="editSettings(mounts.find((m) => m.libraryId === lib.id)!)"
          >
            设置
          </button>
          <button
            class="du-btn du-btn-sm"
            :disabled="
              busy ||
              ['scanning', 'importing'].includes(mounts.find((m) => m.libraryId === lib.id)!.status)
            "
            @click="scanCloud(mounts.find((m) => m.libraryId === lib.id)!)"
          >
            扫描媒体库
          </button>
        </template>
      </article>
      <p v-if="readSession() && !library.loading && !library.libraries.length" class="empty">
        还没有媒体库。添加一个目录，开始整理你的收藏。
      </p>
    </div>
    <div class="section-heading">
      <h2>扫描与入库任务</h2>
      <span class="muted">每 5 秒更新 · 最近 20 项</span>
    </div>
    <p v-if="!jobs.length" class="empty">暂无任务记录</p>
    <article v-for="job in jobs" :key="job.id" class="job-line">
      <div>
        <strong>{{ jobNames[job.jobType] ?? "后台任务" }}</strong>
        <p class="muted">
          {{ job.id.slice(0, 8) }} · {{ new Date(job.updatedAt).toLocaleString() }}
        </p>
        <p v-if="job.lastError" class="error">{{ job.lastError }}</p>
      </div>
      <span :class="{ error: job.status === 'failed' }">{{
        status[job.status] ?? job.status
      }}</span>
    </article>
  </section>
</template>

<style scoped lang="scss">
.server-libraries {
  max-width: 1100px;
}
.section-toolbar,
.section-heading,
.library-line,
.job-line {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 20px;
}
.section-toolbar {
  margin-bottom: 28px;
  color: var(--fbz-color-text-soft);
}
button,
input,
textarea {
  border: 1px solid var(--fbz-color-line);
  border-radius: 6px;
  background: var(--fbz-color-panel);
  color: var(--fbz-color-text);
  padding: 10px 14px;
  font: inherit;
}
button {
  cursor: pointer;
  white-space: nowrap;
}
button:hover {
  border-color: var(--fbz-color-text-muted);
}
button:disabled {
  opacity: 0.5;
  cursor: wait;
}
.primary {
  background: var(--fbz-color-brand-500);
  color: #07120a;
  border-color: transparent;
  font-weight: 600;
}
label {
  display: flex;
  flex-direction: column;
  gap: 10px;
  font-size: 13px;
}
textarea {
  resize: vertical;
}
.create-library {
  padding: 0;
  background: transparent;
  margin-bottom: 24px;
  border-radius: 8px;
  display: grid;
  gap: 20px;
}
.create-library button {
  justify-self: start;
}
.form-columns {
  display: grid;
  grid-template-columns: 1fr 1fr;
  gap: 20px;
}
h2 {
  font-size: 19px;
  margin: 0;
  font-weight: 600;
}
h3 {
  margin: 0 0 6px;
  font-size: 15px;
}
p {
  margin: 0;
  line-height: 1.7;
}
.library-line {
  padding: 24px 0;
  border-bottom: 1px solid var(--fbz-color-line-soft);
}
.library-description {
  flex: 1;
  min-width: 0;
}
.library-description p {
  color: var(--fbz-color-text-muted);
  font-size: 12px;
  overflow-wrap: anywhere;
}
.folder-icon {
  color: var(--fbz-color-brand-500);
  padding: 14px;
  background: var(--fbz-color-panel);
  border-radius: 8px;
}
.section-heading {
  margin: 48px 0 16px;
}
.job-line {
  padding: 18px 0;
  border-bottom: 1px solid var(--fbz-color-line-soft);
  font-size: 13px;
}
.muted,
.empty {
  color: var(--fbz-color-text-muted);
  font-size: 12px;
}
.empty {
  padding: 32px 0;
}
.notice {
  margin: 16px 0;
  font-size: 13px;
}
.notice a {
  margin-left: 12px;
}
.error {
  color: var(--fbz-color-danger-500);
  font-size: 13px;
  overflow-wrap: anywhere;
}
a {
  color: var(--fbz-color-text-soft);
  text-decoration: none;
  font-size: 13px;
}
a:hover {
  color: var(--fbz-color-brand-500);
}
:is(button, input, textarea, a):focus-visible {
  outline: 2px solid var(--fbz-color-brand-500);
  outline-offset: 4px;
}
@media (max-width: 600px) {
  .form-columns {
    grid-template-columns: 1fr;
  }
  .library-line {
    flex-wrap: wrap;
    gap: 12px;
  }
  .library-description {
    flex-basis: 65%;
  }
  .section-heading {
    align-items: flex-start;
    flex-direction: column;
    gap: 8px;
  }
}
</style>
