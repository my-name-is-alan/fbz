<script setup lang="ts">
import QRCode from "qrcode";
import { serverRequest, errorMessage } from "@/service/modules/server.ts";
import { useLibraryStore } from "@/stores/library.ts";
interface Account {
  id: string;
  name: string;
  status: string;
  qps: number;
  cloudUserId?: string;
}
interface Mount {
  id: string;
  name: string;
  accountId: string;
  libraryId: string;
  path: string;
  status: string;
  scanned: number;
  imported: number;
  lastError?: string;
}
interface Entry {
  id: string;
  name: string;
  directory: boolean;
}
interface Login {
  attemptId: string;
  url: string;
  userCode?: string;
  interval: number;
  expiresIn: number;
}
const accounts = ref<Account[]>([]);
const mounts = ref<Mount[]>([]);
const configured = ref(true);
const busy = ref(false);
const error = ref("");
const notice = ref("");
const accountName = ref("");
const addingAccount = ref(false);
const choosingDirectory = ref(false);
const disconnectTarget = ref<Account>();
const qps = ref(1);
const selected = ref("");
const login = ref<Login>();
const qr = ref("");
const loginAccount = ref("");
const expiresAt = ref(0);
const remaining = ref(0);
const path = ref<{ id: string; name: string }[]>([{ id: "", name: "根目录" }]);
const entries = ref<Entry[]>([]);
const nextPage = ref<number | null>(null);
const directoryLoaded = ref(false);
const libraryName = ref("");
const libraryType = ref("movies");
const library = useLibraryStore();
let timer: ReturnType<typeof setTimeout> | undefined;
let generation = 0;
const current = computed(() => path.value[path.value.length - 1]!);
const selectedAccount = computed(() => accounts.value.find((a) => a.id === selected.value));
const labels: Record<string, string> = {
  pending: "未登录",
  ready: "已连接",
  expired: "需重新扫码",
  disconnected: "已断开",
  idle: "等待扫描 / 已完成",
  scanning: "读取目录",
  importing: "导入刮削资料",
  failed: "已暂停，等待重试",
};
function message(err: unknown) {
  const response = (err as { response?: { data?: { error?: { message?: string } } } })?.response;
  return response?.data?.error?.message || errorMessage(err);
}
async function refresh() {
  try {
    const result = await serverRequest<{
      accounts: Account[];
      mounts: Mount[];
      configured: boolean;
    }>("/api/admin/storage");
    accounts.value = result.accounts;
    mounts.value = result.mounts;
    configured.value = result.configured;
  } catch (err) {
    error.value = message(err);
  }
}
function cancelLogin() {
  generation++;
  if (timer) clearTimeout(timer);
  login.value = undefined;
  qr.value = "";
}
async function startLogin(id: string) {
  addingAccount.value = false;
  cancelLogin();
  busy.value = true;
  error.value = "";
  loginAccount.value = id;
  const attemptGeneration = generation;
  try {
    const result = await serverRequest<Login>(`/api/admin/storage/accounts/${id}/login`, {});
    if (attemptGeneration !== generation) return;
    qr.value = await QRCode.toDataURL(result.url, { width: 240, margin: 2 });
    login.value = result;
    expiresAt.value = Date.now() + result.expiresIn * 1000;
    remaining.value = result.expiresIn;
    timer = setTimeout(() => poll(attemptGeneration), result.interval * 1000);
  } catch (err) {
    error.value = message(err);
  } finally {
    busy.value = false;
  }
}
async function poll(version: number) {
  if (version !== generation || !login.value) return;
  if (Date.now() >= expiresAt.value) {
    cancelLogin();
    notice.value = "二维码已过期，请重新生成。";
    return;
  }
  try {
    const result = await serverRequest<{ authenticated?: boolean; slowDown?: boolean }>(
      `/api/admin/storage/accounts/${loginAccount.value}/poll`,
      { attemptId: login.value.attemptId },
    );
    if (version !== generation) return;
    if (result.authenticated) {
      cancelLogin();
      notice.value = "账号连接成功，请选择已刮削的目录。";
      await refresh();
      await selectAccount(loginAccount.value);
      return;
    }
    if (result.slowDown && login.value)
      login.value.interval = Math.min(60, login.value.interval + 5);
  } catch (err) {
    if (version === generation) error.value = message(err);
  }
  if (version === generation && login.value)
    timer = setTimeout(() => poll(version), login.value.interval * 1000);
}
async function addAccount() {
  if (!accountName.value.trim() || busy.value) return;
  busy.value = true;
  error.value = "";
  let id = "";
  try {
    const result = await serverRequest<{ id: string }>("/api/admin/storage/accounts", {
      name: accountName.value.trim(),
      qps: qps.value,
    });
    id = result.id;
    accountName.value = "";
    await refresh();
  } catch (err) {
    error.value = message(err);
  } finally {
    busy.value = false;
  }
  if (id) await startLogin(id);
}
async function selectAccount(id: string) {
  choosingDirectory.value = true;
  selected.value = id;
  path.value = [{ id: "", name: "根目录" }];
  entries.value = [];
  await browse(0);
}
async function browse(page = 0) {
  if (page === 0) directoryLoaded.value = false;
  if (!selected.value) return;
  busy.value = true;
  error.value = "";
  try {
    const result = await serverRequest<{ entries: Entry[]; nextPage: number | null }>(
      `/api/admin/storage/accounts/${selected.value}/directories?parentId=${encodeURIComponent(current.value.id)}&page=${page}`,
    );
    entries.value = page === 0 ? result.entries : [...entries.value, ...result.entries];
    nextPage.value = result.nextPage;
    directoryLoaded.value = true;
  } catch (err) {
    error.value = message(err);
  } finally {
    busy.value = false;
  }
}
async function enter(entry: Entry) {
  path.value.push({ id: entry.id, name: entry.name });
  entries.value = [];
  await browse();
}
async function up(index: number) {
  path.value = path.value.slice(0, index + 1);
  entries.value = [];
  await browse();
}
async function mountDirectory() {
  busy.value = true;
  error.value = "";
  try {
    await serverRequest("/api/admin/storage/mounts", {
      accountId: selected.value,
      rootId: current.value.id,
      name: libraryName.value.trim(),
      displayPath:
        "/" +
        path.value
          .slice(1)
          .map((p) => p.name)
          .join("/"),
      libraryType: libraryType.value,
    });
    notice.value = "云端媒体库已创建，点击开始扫描导入 NFO 和图片。";
    choosingDirectory.value = false;
    libraryName.value = "";
    await refresh();
    await library.refresh();
  } catch (err) {
    error.value = message(err);
  } finally {
    busy.value = false;
  }
}
async function scan(mount: Mount) {
  busy.value = true;
  error.value = "";
  try {
    await serverRequest(`/api/admin/storage/mounts/${mount.id}/scan`, {});
    await refresh();
  } catch (err) {
    error.value = message(err);
  } finally {
    busy.value = false;
  }
}
async function disconnect(id: string) {
  cancelLogin();
  busy.value = true;
  error.value = "";
  try {
    await serverRequest(`/api/admin/storage/accounts/${id}/disconnect`, {});
    if (selected.value === id) {
      selected.value = "";
      entries.value = [];
    }
    await refresh();
    notice.value = "账号已断开，媒体索引保留。";
  } catch (err) {
    error.value = message(err);
  } finally {
    busy.value = false;
  }
}
onMounted(refresh);
onScopeDispose(cancelLogin);
useIntervalFn(() => {
  if (login.value) remaining.value = Math.max(0, Math.ceil((expiresAt.value - Date.now()) / 1000));
}, 1000);
useIntervalFn(() => {
  if (!busy.value) void refresh();
}, 5000);
</script>

<template>
  <section class="storage-panel">
    <p v-if="!configured" class="warning">
      尚未配置服务器凭据加密和插件通信密钥。请按部署文档启动光鸭插件后再添加账号。
    </p>
    <p v-if="error" role="alert" class="error">{{ error }}</p>
    <p v-if="notice" role="status" class="notice">{{ notice }}</p>
    <div class="section-head">
      <div>
        <h2>光鸭账号</h2>
        <p>扫码授权，无需提供密码。每个账号独立保存登录状态。</p>
      </div>
      <button
        class="du-btn du-btn-primary du-btn-sm"
        :disabled="!configured"
        @click="addingAccount = true"
      >
        <BaseIcon name="plus" :size="16" />添加账号
      </button>
    </div>
    <BaseModal
      :open="addingAccount"
      title="连接光鸭账号"
      description="凭据由服务器加密保存，不需要提供账号密码。"
      @close="addingAccount = false"
      ><p v-if="error" class="du-alert du-alert-error">{{ error }}</p>
      <form class="account-form" @submit.prevent="addAccount">
        <label
          >账号备注<input
            class="du-input"
            v-model="accountName"
            placeholder="例如：家庭媒体库"
            maxlength="120"
            required /></label
        ><label
          >每秒请求上限<input
            class="du-input"
            v-model.number="qps"
            type="number"
            min="1"
            max="5" /></label
        ><button class="du-btn du-btn-sm du-btn-primary primary" :disabled="busy || !configured">
          添加账号并扫码
        </button>
      </form></BaseModal
    >
    <p class="hint">默认 1 QPS。播放与目录请求共享额度，遇到限流会冷却；这不是光鸭官方限额。</p>
    <article v-for="account in accounts" :key="account.id" class="account-row">
      <div>
        <strong>{{ account.name }}</strong>
        <p>{{ labels[account.status] ?? account.status }} · {{ account.qps }} QPS</p>
      </div>
      <div class="actions">
        <button
          class="du-btn du-btn-sm"
          v-if="account.status === 'ready'"
          :disabled="busy"
          @click="selectAccount(account.id)"
        >
          选择目录</button
        ><button class="du-btn du-btn-sm" :disabled="busy" @click="startLogin(account.id)">
          扫码连接</button
        ><button
          class="du-btn du-btn-sm"
          v-if="account.status === 'ready'"
          :disabled="busy"
          @click="disconnectTarget = account"
        >
          断开
        </button>
      </div>
    </article>
    <BaseModal :open="!!login" title="扫码连接光鸭" @close="cancelLogin"
      ><p v-if="error" class="du-alert du-alert-error">{{ error }}</p>
      <section v-if="login" class="login-panel" aria-label="扫码连接光鸭">
        <img :src="qr" alt="使用光鸭客户端扫描此二维码登录" />
        <div>
          <h3>使用光鸭扫码授权</h3>
          <p>请在光鸭客户端确认登录。二维码剩余 {{ remaining }} 秒。</p>
          <p v-if="login.userCode">授权码：{{ login.userCode }}</p>
          <a :href="login.url" target="_blank" rel="noopener noreferrer">打开官方授权页面 ↗</a
          ><button class="du-btn du-btn-sm" @click="cancelLogin">关闭二维码</button>
        </div>
      </section></BaseModal
    >
    <BaseModal
      :open="choosingDirectory"
      title="选择云端媒体目录"
      wide
      @close="choosingDirectory = false"
      ><p v-if="error" class="du-alert du-alert-error">{{ error }}</p>
      <section v-if="selectedAccount?.status === 'ready'" class="directory-panel">
        <div class="section-head">
          <div>
            <h2>选择已刮削目录</h2>
            <p>{{ selectedAccount.name }} · 只加载当前目录，选择后再扫描。</p>
          </div>
        </div>
        <nav class="breadcrumbs" aria-label="云盘目录">
          <button
            class="du-btn du-btn-sm"
            v-for="(part, index) in path"
            :key="part.id"
            :disabled="busy"
            @click="up(index)"
          >
            {{ part.name }} /
          </button>
        </nav>
        <div class="entries">
          <button
            class="du-btn du-btn-sm"
            v-for="entry in entries.filter((e) => e.directory)"
            :key="entry.id"
            :disabled="busy"
            @click="enter(entry)"
          >
            <span>▱ {{ entry.name }}</span
            ><BaseIcon name="right" :size="16" />
          </button>
          <p v-if="directoryLoaded && !entries.length && !busy" class="hint">当前目录为空。</p>
          <p class="hint">
            当前已加载 {{ entries.filter((e) => !e.directory).length }} 个文件，其中
            {{ entries.filter((e) => /\.nfo$/i.test(e.name)).length }} 个 NFO。
          </p>
          <button
            class="du-btn du-btn-sm"
            v-if="nextPage !== null"
            :disabled="busy"
            @click="browse(nextPage)"
          >
            加载更多
          </button>
        </div>
        <form class="mount-form" @submit.prevent="mountDirectory">
          <label
            >媒体库名称<input
              class="du-input"
              v-model="libraryName"
              required
              maxlength="120"
              placeholder="例如：光鸭电影" /></label
          ><label
            >内容类型<BaseSelect
              v-model="libraryType"
              :options="[
                { label: '电影', value: 'movies' },
                { label: '剧集', value: 'tv' },
              ]" /></label
          ><button
            class="du-btn du-btn-sm du-btn-primary primary"
            :disabled="busy || !directoryLoaded || !libraryName.trim()"
          >
            挂载当前目录
          </button>
        </form>
        <p class="hint">使用目录中已有 NFO、海报和字幕，不重新刮削、不修改云端文件。</p>
      </section></BaseModal
    >
    <div class="section-head mounts-heading">
      <div>
        <h2>已挂载目录</h2>
        <p>扫描断点自动保存；失败时保留原媒体库，修复问题后继续。</p>
      </div>
    </div>
    <p v-if="!mounts.length" class="hint">还没有云端媒体库。连接账号并选择一个目录开始。</p>
    <article v-for="mount in mounts" :key="mount.id" class="mount-row">
      <div>
        <h3>{{ mount.name }}</h3>
        <p>{{ mount.path }}</p>
        <p>
          {{
            mount.status === "idle"
              ? mount.imported
                ? "扫描完成"
                : "等待扫描"
              : (labels[mount.status] ?? mount.status)
          }}
          · 已读取 {{ mount.scanned }} 项 · 已导入 {{ mount.imported }} 个视频
        </p>
        <p v-if="mount.lastError" class="error">{{ mount.lastError }}</p>
      </div>
      <div class="actions">
        <RouterLink :to="`/library/${mount.libraryId}`">浏览媒体库 →</RouterLink
        ><button
          class="du-btn du-btn-sm"
          :disabled="busy || ['scanning', 'importing'].includes(mount.status)"
          @click="scan(mount)"
        >
          {{ mount.status === "failed" ? "继续扫描" : "开始扫描" }}
        </button>
      </div>
    </article>
    <BaseModal
      :open="!!disconnectTarget"
      title="断开光鸭连接？"
      description="媒体索引会保留；继续访问云端文件时需要重新扫码。"
      @close="disconnectTarget = undefined"
      ><template #actions
        ><button class="du-btn du-btn-ghost" @click="disconnectTarget = undefined">取消</button
        ><button
          class="du-btn du-btn-error"
          :disabled="busy"
          @click="disconnect(disconnectTarget!.id).then(() => (disconnectTarget = undefined))"
        >
          断开连接
        </button></template
      ></BaseModal
    >
  </section>
</template>

<style scoped lang="scss">
.storage-panel {
  max-width: 1100px;
  font-size: 13px;
}
.section-head,
.account-row,
.mount-row {
  display: flex;
  justify-content: space-between;
  align-items: center;
  gap: 24px;
}
.section-head {
  margin-bottom: 24px;
}
h2 {
  font-size: 21px;
  font-weight: 550;
  margin: 0 0 10px;
}
h3 {
  font-size: 16px;
  font-weight: 550;
  margin: 0 0 10px;
}
p {
  line-height: 1.8;
  color: var(--fbz-color-text-muted);
  margin: 4px 0;
}
.account-form,
.mount-form {
  display: grid;
  grid-template-columns: 2fr 1fr auto;
  gap: 16px;
  align-items: end;
  margin: 24px 0 12px;
}
label {
  display: grid;
  gap: 10px;
}
input,
button {
  background: var(--fbz-color-panel);
  color: var(--fbz-color-text);
  border: 1px solid var(--fbz-color-line);
  border-radius: 6px;
  padding: 10px 14px;
  font: inherit;
}
input {
  width: 100%;
  min-width: 0;
}
button {
  cursor: pointer;
}
button:disabled {
  opacity: 0.45;
  cursor: default;
}
button:hover:not(:disabled) {
  border-color: var(--fbz-color-text-muted);
}
.primary {
  background: var(--fbz-color-brand-500);
  color: #07120a;
  font-weight: 600;
  border-color: transparent;
}
.account-row,
.mount-row {
  padding: 24px 0;
  border-bottom: 1px solid var(--fbz-color-line-soft);
}
.actions {
  display: flex;
  align-items: center;
  gap: 12px;
  flex-wrap: wrap;
}
.hint {
  font-size: 12px;
}
.badge {
  color: var(--fbz-color-brand-500);
  font-size: 12px;
  white-space: nowrap;
}
.login-panel {
  display: flex;
  align-items: center;
  gap: 32px;
  padding: 0;
  background: transparent;
  margin: 24px 0;
  border-radius: 8px;
}
.login-panel img {
  width: 220px;
  height: 220px;
  border-radius: 4px;
}
.login-panel button {
  display: block;
  margin-top: 24px;
}
.login-panel a {
  display: inline-block;
  margin-top: 16px;
}
a {
  color: var(--fbz-color-text-soft);
  text-decoration: none;
}
a:hover {
  color: var(--fbz-color-brand-500);
}
.directory-panel {
  margin-top: 0;
}
.breadcrumbs {
  display: flex;
  gap: 8px;
  flex-wrap: wrap;
  margin: 16px 0;
}
.breadcrumbs button {
  border: 0;
  background: none;
  color: var(--fbz-color-text-soft);
  padding: 6px;
}
.entries {
  border-block: 1px solid var(--fbz-color-line-soft);
  padding: 12px 0;
}
.entries > button {
  display: flex;
  justify-content: space-between;
  width: 100%;
  background: none;
  border: 0;
  padding: 14px 8px;
  text-align: left;
}
.entries > button:hover {
  background: var(--fbz-color-panel);
}
.mounts-heading {
  margin-top: 48px;
}
.error {
  color: var(--fbz-color-danger-500);
  overflow-wrap: anywhere;
}
.warning {
  padding: 16px;
  background: var(--fbz-color-panel);
  color: var(--fbz-color-amber-500);
}
.notice {
  color: var(--fbz-color-brand-500);
  margin-bottom: 20px;
}
.mount-row > div:first-child {
  min-width: 0;
}
.mount-row p {
  overflow-wrap: anywhere;
}
:is(a, button, input):focus-visible {
  outline: 2px solid var(--fbz-color-brand-500);
  outline-offset: 3px;
}
@media (max-width: 650px) {
  .account-form,
  .mount-form {
    grid-template-columns: 1fr;
  }
  .account-row,
  .mount-row,
  .login-panel {
    align-items: flex-start;
    flex-direction: column;
  }
  .login-panel img {
    align-self: center;
  }
  .actions {
    gap: 8px;
  }
}
.folder-label {
  display: flex;
  align-items: center;
  gap: 12px;
}
.account-form {
  grid-template-columns: 1fr;
}
.login-panel {
  flex-direction: column;
  align-items: center;
  text-align: center;
}
.login-panel img {
  width: 224px;
  height: 224px;
}
.mount-form {
  grid-template-columns: 1fr 1fr;
}
.mount-form > .du-btn {
  grid-column: 1/-1;
  justify-self: end;
}
.directory-panel .entries {
  max-height: 35dvh;
  overflow: auto;
}
</style>
