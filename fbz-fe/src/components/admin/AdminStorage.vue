<script setup lang="ts">
import QRCode from "qrcode";
import { serverRequest, errorMessage } from "@/service/modules/server.ts";
import { useLibraryStore } from "@/stores/library.ts";
interface Account {
  id: string;
  name: string;
  status: string;
  qps: number;
  provider?: string;
  cloudUserId?: string;
}
interface Mount {
  id: string;
  name: string;
  accountId: string;
  libraryId: string | null;
  mountPath: string;
  refreshMinutes: number;
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
const props = withDefaults(defineProps<{ providerId?: string; providerName?: string }>(), {
  providerId: "guangya",
  providerName: "光鸭",
});
const accounts = ref<Account[]>([]);
const mounts = ref<Mount[]>([]);
const configured = ref(true);
const initialized = ref(false);
const busy = ref(false);
const error = ref("");
const notice = ref("");
const accountName = ref("");
const addingAccount = ref(false);
const choosingDirectory = ref(false);
const disconnectTarget = ref<Account>();
const qps = ref(10);
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
const mountPath = ref("/cloud/");
const refreshMinutes = ref(60);
const editingAccount = ref<Account>();
const editName = ref("");
const editQps = ref(10);
function editAccount(account: Account) {
  editingAccount.value = account;
  editName.value = account.name;
  editQps.value = account.qps;
}
async function saveAccount() {
  if (!editingAccount.value) return;
  busy.value = true;
  try {
    await serverRequest(`/api/admin/storage/accounts/${editingAccount.value.id}/settings`, {
      name: editName.value,
      qps: editQps.value,
    });
    editingAccount.value = undefined;
    await refresh();
  } catch (e) {
    error.value = message(e);
  } finally {
    busy.value = false;
  }
}
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
      encryptionConfigured: boolean;
    }>("/api/admin/storage");
    accounts.value = result.accounts.filter(
      (a) => a.provider === props.providerId || (props.providerId === "guangya" && !a.provider),
    );
    mounts.value = result.mounts.filter((m) => accounts.value.some((a) => a.id === m.accountId));
    configured.value =
      props.providerId === "guangya" ? result.configured : result.encryptionConfigured;
    initialized.value = true;
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
      providerId: props.providerId,
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
      mountPath: mountPath.value.trim(),
      refreshMinutes: refreshMinutes.value,
    });
    notice.value = "目录已挂载。请到媒体库页面添加媒体库并选择此挂载地址。";
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
      尚未配置服务器凭据加密或插件通信密钥。请检查部署配置后再添加账号。
    </p>
    <p v-if="error" role="alert" class="error">{{ error }}</p>
    <p v-if="notice" role="status" class="notice">{{ notice }}</p>
    <p v-if="!initialized && !error" role="status" class="hint">正在读取账号与挂载目录…</p>
    <div v-if="initialized" class="storage-summary" aria-label="挂载概览">
      <div>
        <span>已连接账号</span
        ><strong
          >{{ accounts.filter((a) => a.status === "ready").length
          }}<small> / {{ accounts.length }}</small></strong
        >
      </div>
      <div>
        <span>挂载目录</span><strong>{{ mounts.length }}</strong>
      </div>
      <div>
        <span>已导入视频</span
        ><strong>{{ mounts.reduce((total, mount) => total + mount.imported, 0) }}</strong>
      </div>
      <div>
        <span>读取方式</span
        ><strong class="summary-mode"><BaseIcon name="cloud" :size="17" />云端直读</strong>
      </div>
    </div>
    <div class="section-head">
      <div>
        <h2>
          连接账号 <span class="count">{{ accounts.length }}</span>
        </h2>
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
      :title="`连接${props.providerName}账号`"
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
            max="20" /></label
        ><button class="du-btn du-btn-sm du-btn-primary primary" :disabled="busy || !configured">
          添加账号并扫码
        </button>
      </form></BaseModal
    >
    <BaseEmptyState
      v-if="initialized && !accounts.length"
      icon="cloud"
      :title="`连接你的第一个${props.providerName}账号`"
      description="扫码登录后，选择已刮削的目录即可挂载。"
    />
    <BaseModal :open="!!editingAccount" title="账号设置" @close="editingAccount = undefined"
      ><p v-if="error" class="du-alert du-alert-error">{{ error }}</p>
      <form class="account-form" @submit.prevent="saveAccount">
        <label>账号备注<input class="du-input" v-model="editName" required maxlength="120" /></label
        ><label
          >QPS 上限<input
            class="du-input"
            type="number"
            v-model.number="editQps"
            min="1"
            max="20"
            required
        /></label>
        <p class="hint">默认 10，最高 20。当前账号串行请求，限流时自动退避。</p>
        <button class="du-btn du-btn-primary primary" :disabled="busy">保存设置</button>
      </form></BaseModal
    >
    <article v-for="account in accounts" :key="account.id" class="account-row">
      <div class="account-identity">
        <span class="provider-icon"><BaseIcon name="cloud" :size="24" /></span>
        <div>
          <strong>{{ account.name }}</strong>
          <p>
            {{ props.providerName }}
            <span class="account-status" :class="{ connected: account.status === 'ready' }">{{
              labels[account.status] ?? account.status
            }}</span>
          </p>
        </div>
      </div>
      <span class="account-quota">{{ account.qps }} <small>请求 / 秒</small></span>
      <div class="actions">
        <button class="du-btn du-btn-sm" @click="editAccount(account)">设置</button>
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
    <BaseModal :open="!!login" :title="`扫码连接${props.providerName}`" @close="cancelLogin"
      ><p v-if="error" class="du-alert du-alert-error">{{ error }}</p>
      <section v-if="login" class="login-panel" :aria-label="`扫码连接${props.providerName}`">
        <img :src="qr" :alt="`使用${props.providerName}客户端扫描此二维码登录`" />
        <div>
          <h3>使用 {{ props.providerName }} 扫码授权</h3>
          <p>请在 {{ props.providerName }} 客户端确认登录。二维码剩余 {{ remaining }} 秒。</p>
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
            >挂载名称<input
              class="du-input"
              v-model="libraryName"
              required
              maxlength="120"
              placeholder="例如：云盘电影" /></label
          ><label
            >FBZ 挂载地址<input
              class="du-input"
              v-model="mountPath"
              placeholder="/cloud/movies"
              required
          /></label>
          <label
            >自动刷新周期（分钟，0 为手动）<input
              class="du-input"
              type="number"
              min="0"
              max="10080"
              v-model.number="refreshMinutes"
              required
          /></label>
          <button
            class="du-btn du-btn-sm du-btn-primary primary"
            :disabled="busy || !directoryLoaded || !libraryName.trim()"
          >
            挂载当前目录
          </button>
        </form>
        <p class="hint">挂载地址是 FBZ 内部虚拟路径，不会创建媒体库或修改云端文件。</p>
      </section></BaseModal
    >
    <div class="section-head mounts-heading">
      <div>
        <h2>
          挂载目录 <span class="count">{{ mounts.length }}</span>
        </h2>
        <p>扫描断点自动保存；失败时保留原媒体库，修复问题后继续。</p>
      </div>
    </div>
    <p v-if="initialized && !mounts.length" class="hint">
      还没有挂载目录。连接账号后选择目录，设置虚拟挂载地址。
    </p>
    <article v-for="mount in mounts" :key="mount.id" class="mount-row">
      <div>
        <h3><BaseIcon name="folder" :size="20" />{{ mount.name }}</h3>
        <p>{{ mount.path }}</p>
        <p>挂载地址：{{ mount.mountPath }}</p>
        <p>{{ mount.refreshMinutes ? `每 ${mount.refreshMinutes} 分钟刷新` : "手动刷新" }}</p>
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
        <RouterLink class="browse-link" to="/admin/libraries">{{
          mount.libraryId ? "管理关联媒体库 →" : "添加媒体库 →"
        }}</RouterLink>
      </div>
    </article>
    <BaseModal
      :open="!!disconnectTarget"
      :title="`断开${props.providerName}连接？`"
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
  background: var(--fbz-color-text);
  color: var(--fbz-color-bg);
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

.storage-summary {
  display: grid;
  grid-template-columns: repeat(4, minmax(0, 1fr));
  gap: 24px;
  padding: 0 0 30px;
  margin-bottom: 32px;
  border-bottom: 1px solid var(--fbz-color-line-soft);
}
.storage-summary > div {
  display: grid;
  gap: 12px;
}
.storage-summary span {
  color: var(--fbz-color-text-muted);
  font-size: 13px;
}
.storage-summary strong {
  font-size: 24px;
  font-weight: 600;
  font-variant-numeric: tabular-nums;
}
.storage-summary small {
  font-size: 14px;
  color: var(--fbz-color-text-muted);
  font-weight: 400;
}
.storage-summary .summary-mode {
  display: flex;
  gap: 8px;
  align-items: center;
  font-size: 15px;
}
h2 {
  font-size: 19px;
  font-weight: 600;
  display: flex;
  align-items: center;
  gap: 10px;
}
.count {
  font-size: 12px;
  font-weight: 500;
  padding: 2px 7px;
  border-radius: 5px;
  background: var(--fbz-color-panel-strong);
  color: var(--fbz-color-text-muted);
}
.section-head {
  margin-bottom: 20px;
}
.section-head p {
  font-size: 13px;
}
.account-row {
  padding: 22px;
  background: var(--fbz-color-panel);
  border: 1px solid var(--fbz-color-line-soft);
  border-radius: 10px;
  margin-bottom: 12px;
  gap: 18px;
}
.account-identity {
  display: flex;
  align-items: center;
  gap: 14px;
  flex: 1;
}
.account-identity strong {
  font-size: 15px;
  font-weight: 600;
}
.provider-icon {
  display: grid;
  place-items: center;
  width: 46px;
  height: 46px;
  border: 1px solid var(--fbz-color-line);
  border-radius: 12px;
  color: var(--fbz-color-text-soft);
}
.account-status {
  margin-left: 10px;
  font-size: 12px;
}
.account-status.connected {
  color: color-mix(in srgb, var(--fbz-color-brand-500) 65%, var(--fbz-color-text));
}
.account-status:before {
  content: "•";
  margin-right: 5px;
}
.account-quota {
  font-size: 14px;
  font-variant-numeric: tabular-nums;
}
.account-quota small {
  color: var(--fbz-color-text-muted);
}
.actions {
  gap: 8px;
}
.actions .du-btn {
  font-size: 12px;
  padding: 0 12px;
  min-height: 34px;
  height: 34px;
}
.mounts-heading {
  margin-top: 40px;
}
.mount-row {
  padding: 22px 4px;
}
.mount-row h3 {
  display: flex;
  align-items: center;
  gap: 10px;
  font-size: 15px;
}
.mount-row h3 svg {
  color: var(--fbz-color-text-muted);
}
.mount-row p {
  font-size: 12px;
}
.browse-link {
  font-size: 12px;
  padding: 8px;
}
@media (max-width: 1100px) {
  .account-row {
    flex-wrap: wrap;
  }
  .account-row .actions {
    width: 100%;
    padding-left: 60px;
  }
}
@media (max-width: 650px) {
  .storage-summary {
    grid-template-columns: repeat(2, minmax(0, 1fr));
    gap: 24px;
  }
  .account-row .actions {
    padding-left: 0;
  }
  .account-row {
    padding: 18px;
  }
  .section-head {
    gap: 14px;
    align-items: flex-start;
  }
  .section-head .du-btn {
    flex-shrink: 0;
  }
  .mount-form {
    grid-template-columns: 1fr;
  }
}
</style>
