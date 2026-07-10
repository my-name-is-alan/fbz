<script setup lang="ts">
import {
  activatePluginPackage,
  approvePluginPackage,
  getPluginPackageDetail,
  installPluginPackage,
  listPluginDispatches,
  listPluginExecutionRuns,
  listPluginPackages,
  listPlugins,
  rejectPluginPackage,
  replayPluginDispatch,
  setPluginEnabled,
  uninstallPlugin,
  uploadPluginPackage,
} from "@/service/modules/admin.ts";
import { useUiStore } from "@/stores/ui.ts";
import type {
  PluginDispatch,
  PluginExecutionRun,
  PluginPackageDetail,
  PluginPackageSummary,
  PluginSummary,
} from "@/types/admin.ts";

const route = useRoute();
const router = useRouter();
const uiStore = useUiStore();

type PluginsSubTab = "plugins" | "packages" | "dispatches";

function tabFromQuery(raw: unknown): PluginsSubTab {
  if (raw === "packages" || raw === "dispatches") return raw;
  return "plugins";
}

const activeSubTab = shallowRef<PluginsSubTab>(tabFromQuery(route.query.tab));
const plugins = ref<PluginSummary[]>([]);
const packages = ref<PluginPackageSummary[]>([]);
const dispatches = ref<PluginDispatch[]>([]);
const loading = shallowRef(false);
const loadingAction = shallowRef("");
const error = shallowRef("");

// 待卸载确认的插件（二次确认）
const pendingUninstall = ref<PluginSummary | null>(null);

// 手工安装表单：支持浏览器直传 zip（推荐）或直接填服务器上的相对路径。
const installOpen = shallowRef(false);
const installPath = ref("");
const installChecksum = ref("");
const installSignature = ref("");
const installing = shallowRef(false);
const uploadingPackage = shallowRef(false);
const uploadedFileName = shallowRef("");
const packageFileInput = shallowRef<HTMLInputElement | null>(null);

// 包详情审查（审批前查看权限/hook/菜单/计划任务声明）
const packageDetail = ref<PluginPackageDetail | null>(null);
const loadingDetail = shallowRef(false);

// dispatch 运行明细
const expandedDispatchId = shallowRef("");
const dispatchRuns = ref<PluginExecutionRun[]>([]);
const loadingRuns = shallowRef(false);

const activePlugins = computed(() => plugins.value.filter((plugin) => plugin.enabled).length);
const pendingPackages = computed(
  () => packages.value.filter((pkg) => pkg.approvalStatus === "pending_approval").length,
);
const failedDispatches = computed(
  () => dispatches.value.filter((dispatch) => dispatch.status === "failed").length,
);

watch(
  () => route.query.tab,
  (tab) => {
    activeSubTab.value = tabFromQuery(tab);
  },
);

onMounted(() => {
  void refreshAll();
});

function pluginName(plugin: PluginSummary): string {
  return plugin.name ?? plugin.pluginId;
}

function formatTime(value: string | null | undefined): string {
  if (!value) return "-";
  return new Intl.DateTimeFormat("zh-CN", {
    month: "2-digit",
    day: "2-digit",
    hour: "2-digit",
    minute: "2-digit",
  }).format(new Date(value));
}

async function refreshAll() {
  loading.value = true;
  error.value = "";
  try {
    const [pluginPage, packagePage, dispatchPage] = await Promise.all([
      listPlugins({ limit: 200 }),
      listPluginPackages({ limit: 200 }),
      listPluginDispatches({ limit: 20 }),
    ]);
    plugins.value = pluginPage.items;
    packages.value = packagePage.items;
    dispatches.value = dispatchPage.items;
  } catch {
    error.value = "插件管理数据加载失败，请确认后端已初始化且当前用户具备管理员权限。";
  } finally {
    loading.value = false;
  }
}

async function handleTogglePlugin(plugin: PluginSummary) {
  loadingAction.value = `plugin:${plugin.pluginId}`;
  try {
    await setPluginEnabled(plugin.pluginId, !plugin.enabled);
    uiStore.showToast(
      `插件 ${pluginName(plugin)} 已${plugin.enabled ? "停用" : "启用"}。`,
      "success",
    );
    await refreshAll();
  } catch {
    uiStore.showToast("更新插件启用状态失败。", "error");
  } finally {
    loadingAction.value = "";
  }
}

async function handlePackageAction(
  pkg: PluginPackageSummary,
  action: "approve" | "reject" | "activate",
) {
  loadingAction.value = `${action}:${pkg.packageId}`;
  try {
    if (action === "approve") {
      await approvePluginPackage(pkg.packageId);
      uiStore.showToast(`插件包 ${pkg.name} 已审批通过。`, "success");
    } else if (action === "reject") {
      await rejectPluginPackage(pkg.packageId);
      uiStore.showToast(`插件包 ${pkg.name} 已拒绝。`, "success");
    } else {
      await activatePluginPackage(pkg.packageId);
      uiStore.showToast(`插件包 ${pkg.name} 已激活。`, "success");
    }
    await refreshAll();
  } catch {
    uiStore.showToast("插件包操作失败，请检查包状态和审批状态。", "error");
  } finally {
    loadingAction.value = "";
  }
}

function openPluginConfig(plugin: PluginSummary) {
  void router.push(`/admin/plugins/${encodeURIComponent(plugin.pluginId)}/config`);
}

function requestUninstall(plugin: PluginSummary) {
  pendingUninstall.value = plugin;
}

async function confirmUninstall() {
  const plugin = pendingUninstall.value;
  if (!plugin) return;
  loadingAction.value = `uninstall:${plugin.pluginId}`;
  try {
    await uninstallPlugin(plugin.pluginId);
    uiStore.showToast(`插件 ${pluginName(plugin)} 已卸载。`, "success");
    pendingUninstall.value = null;
    await refreshAll();
  } catch {
    uiStore.showToast("卸载插件失败，请稍后重试。", "error");
  } finally {
    loadingAction.value = "";
  }
}

/** 选择本地 zip → 直传服务器，成功后自动回填 packagePath 与校验和。 */
async function handlePackageFileChange(event: Event) {
  const input = event.target as HTMLInputElement;
  const file = input.files?.[0];
  if (!file) return;
  uploadingPackage.value = true;
  try {
    const uploaded = await uploadPluginPackage(file);
    installPath.value = uploaded.packagePath;
    installChecksum.value = uploaded.checksumSha256;
    uploadedFileName.value = file.name;
    uiStore.showToast(`插件包 ${file.name} 已上传，确认后点击安装。`, "success");
  } catch {
    uiStore.showToast("上传插件包失败，请确认文件是有效的 zip 包。", "error");
  } finally {
    uploadingPackage.value = false;
    input.value = "";
  }
}

async function handleInstallPackage() {
  const path = installPath.value.trim();
  if (!path) {
    uiStore.showToast("请先上传插件包，或填写服务器插件包目录内的相对路径。", "warning");
    return;
  }
  installing.value = true;
  try {
    const pkg = await installPluginPackage({
      packagePath: path,
      checksumSha256: installChecksum.value.trim() || undefined,
      signature: installSignature.value.trim() || undefined,
    });
    uiStore.showToast(
      `插件 ${pkg.pluginId} ${pkg.packageVersion} 已安装，请审批并激活后启用。`,
      "success",
    );
    installOpen.value = false;
    installPath.value = "";
    installChecksum.value = "";
    installSignature.value = "";
    uploadedFileName.value = "";
    activeSubTab.value = "packages";
    await refreshAll();
  } catch {
    uiStore.showToast("安装插件包失败，请检查路径、校验和与签名。", "error");
  } finally {
    installing.value = false;
  }
}

/** 打开包详情审查弹层（审批前查看权限/hook/菜单/计划任务声明）。 */
async function handleShowPackageDetail(pkg: PluginPackageSummary) {
  loadingDetail.value = true;
  loadingAction.value = `detail:${pkg.packageId}`;
  try {
    packageDetail.value = await getPluginPackageDetail(pkg.packageId);
  } catch {
    uiStore.showToast("读取插件包详情失败。", "error");
  } finally {
    loadingDetail.value = false;
    loadingAction.value = "";
  }
}

/** 在详情弹层里直接审批。 */
async function approveFromDetail() {
  const detail = packageDetail.value;
  if (!detail) return;
  loadingAction.value = `approve:${detail.packageId}`;
  try {
    await approvePluginPackage(detail.packageId);
    uiStore.showToast(`插件包 ${detail.name} 已审批通过。`, "success");
    packageDetail.value = null;
    await refreshAll();
  } catch {
    uiStore.showToast("插件包审批失败，请检查包状态。", "error");
  } finally {
    loadingAction.value = "";
  }
}

/** 展开/收起某条 dispatch 的运行明细。 */
async function toggleDispatchRuns(dispatch: PluginDispatch) {
  if (expandedDispatchId.value === dispatch.id) {
    expandedDispatchId.value = "";
    dispatchRuns.value = [];
    return;
  }
  expandedDispatchId.value = dispatch.id;
  loadingRuns.value = true;
  dispatchRuns.value = [];
  try {
    const page = await listPluginExecutionRuns(dispatch.id, { limit: 20 });
    dispatchRuns.value = page.items;
  } catch {
    uiStore.showToast("读取运行明细失败。", "error");
  } finally {
    loadingRuns.value = false;
  }
}

async function handleReplayDispatch(dispatch: PluginDispatch) {
  loadingAction.value = `dispatch:${dispatch.id}`;
  try {
    await replayPluginDispatch(dispatch.id);
    uiStore.showToast("插件 dispatch 已重新入队。", "success");
    await refreshAll();
  } catch {
    uiStore.showToast("重放 dispatch 失败，可能状态不允许重放。", "error");
  } finally {
    loadingAction.value = "";
  }
}
</script>

<template>
  <div class="admin-plugins-view">
    <div class="summary-grid">
      <div class="summary-card">
        <span class="label">插件</span>
        <span class="value">{{ plugins.length }}</span>
        <span class="meta">启用 {{ activePlugins }}</span>
      </div>
      <div class="summary-card">
        <span class="label">插件包</span>
        <span class="value">{{ packages.length }}</span>
        <span class="meta">待审批 {{ pendingPackages }}</span>
      </div>
      <div class="summary-card">
        <span class="label">Dispatch</span>
        <span class="value">{{ dispatches.length }}</span>
        <span class="meta">失败 {{ failedDispatches }}</span>
      </div>
    </div>

    <nav class="sub-tabs-bar" role="tablist" aria-label="插件管理标签">
      <button
        class="sub-tab-btn"
        :class="{ active: activeSubTab === 'plugins' }"
        type="button"
        role="tab"
        :aria-selected="activeSubTab === 'plugins'"
        @click="activeSubTab = 'plugins'"
      >
        插件状态
      </button>
      <button
        class="sub-tab-btn"
        :class="{ active: activeSubTab === 'packages' }"
        type="button"
        role="tab"
        :aria-selected="activeSubTab === 'packages'"
        @click="activeSubTab = 'packages'"
      >
        插件包
      </button>
      <button
        class="sub-tab-btn"
        :class="{ active: activeSubTab === 'dispatches' }"
        type="button"
        role="tab"
        :aria-selected="activeSubTab === 'dispatches'"
        @click="activeSubTab = 'dispatches'"
      >
        运行审计
      </button>
      <button class="refresh-btn install" type="button" @click="installOpen = true">
        手工安装
      </button>
      <button class="refresh-btn" type="button" :disabled="loading" @click="refreshAll">
        {{ loading ? "刷新中..." : "刷新" }}
      </button>
    </nav>

    <p v-if="error" class="error-text">{{ error }}</p>

    <div class="plugins-panel-content">
      <div v-if="activeSubTab === 'plugins'" class="plugins-grid">
        <div v-if="!loading && plugins.length === 0" class="empty-state">
          后端当前没有已注册插件。通过插件包安装接口安装后会显示在这里。
        </div>
        <div
          v-for="plugin in plugins"
          :key="plugin.pluginId"
          class="plugin-card"
          :class="{ inactive: !plugin.enabled }"
        >
          <div class="plugin-main">
            <div class="plugin-header">
              <div class="title-row">
                <span class="plugin-name">{{ pluginName(plugin) }}</span>
                <div class="badge-row">
                  <span class="plugin-ver">{{ plugin.packageVersion ?? "no package" }}</span>
                  <span class="status-badge" :class="{ active: plugin.enabled }">
                    {{ plugin.enabled ? "已启用" : "已停用" }}
                  </span>
                </div>
              </div>
              <span class="author">
                {{ plugin.pluginId }} / {{ plugin.runtime ?? "runtime 未知" }}
              </span>
            </div>
            <p class="plugin-desc">
              包状态 {{ plugin.packageStatus ?? "-" }}，审批 {{ plugin.approvalStatus }}
            </p>
          </div>
          <div class="plugin-actions">
            <button class="plugin-btn secondary" type="button" @click="openPluginConfig(plugin)">
              配置
            </button>
            <button
              class="plugin-btn secondary"
              type="button"
              :disabled="loadingAction === `plugin:${plugin.pluginId}`"
              @click="handleTogglePlugin(plugin)"
            >
              {{ plugin.enabled ? "停用" : "启用" }}
            </button>
            <button
              class="plugin-btn danger"
              type="button"
              :disabled="loadingAction === `uninstall:${plugin.pluginId}`"
              @click="requestUninstall(plugin)"
            >
              卸载
            </button>
          </div>
        </div>
      </div>

      <div v-else-if="activeSubTab === 'packages'" class="plugins-grid">
        <div v-if="!loading && packages.length === 0" class="empty-state">
          后端当前没有插件包记录。
        </div>
        <div v-for="pkg in packages" :key="pkg.packageId" class="plugin-card store-card">
          <div class="plugin-main">
            <div class="plugin-header">
              <div class="title-row">
                <span class="plugin-name">{{ pkg.name }}</span>
                <span class="plugin-ver">{{ pkg.packageVersion }}</span>
              </div>
              <span class="author">{{ pkg.pluginId }} / {{ pkg.runtime }}</span>
            </div>
            <p class="plugin-desc">
              状态 {{ pkg.packageStatus }}，审批 {{ pkg.approvalStatus ?? "-" }}，签名
              {{ pkg.signaturePresent ? "存在" : "无" }}
            </p>
          </div>
          <div class="plugin-actions">
            <button
              class="plugin-btn secondary"
              type="button"
              :disabled="loadingAction === `detail:${pkg.packageId}`"
              @click="handleShowPackageDetail(pkg)"
            >
              详情
            </button>
            <button
              class="plugin-btn secondary"
              type="button"
              :disabled="loadingAction === `approve:${pkg.packageId}`"
              @click="handlePackageAction(pkg, 'approve')"
            >
              审批
            </button>
            <button
              class="plugin-btn secondary"
              type="button"
              :disabled="loadingAction === `activate:${pkg.packageId}`"
              @click="handlePackageAction(pkg, 'activate')"
            >
              激活
            </button>
            <button
              class="plugin-btn danger"
              type="button"
              :disabled="loadingAction === `reject:${pkg.packageId}`"
              @click="handlePackageAction(pkg, 'reject')"
            >
              拒绝
            </button>
          </div>
        </div>
      </div>

      <div v-else class="dispatch-list">
        <div v-if="!loading && dispatches.length === 0" class="empty-state">
          暂无插件 dispatch 审计记录。
        </div>
        <template v-for="dispatch in dispatches" :key="dispatch.id">
          <div class="dispatch-row">
            <div class="dispatch-main">
              <span class="dispatch-title">
                {{ dispatch.hookEvent ?? dispatch.aggregateType }}
              </span>
              <span class="dispatch-meta">
                {{ dispatch.pluginId ?? "unknown plugin" }} / {{ dispatch.handler ?? "handler" }}
              </span>
              <span v-if="dispatch.lastError" class="dispatch-error">{{ dispatch.lastError }}</span>
            </div>
            <div class="dispatch-side">
              <span class="status-badge" :class="{ active: dispatch.status === 'delivered' }">
                {{ dispatch.status }}
              </span>
              <span class="dispatch-meta">{{ formatTime(dispatch.createdAt) }}</span>
              <button
                class="plugin-btn secondary"
                type="button"
                @click="toggleDispatchRuns(dispatch)"
              >
                {{ expandedDispatchId === dispatch.id ? "收起" : "明细" }}
              </button>
              <button
                class="plugin-btn secondary"
                type="button"
                :disabled="loadingAction === `dispatch:${dispatch.id}`"
                @click="handleReplayDispatch(dispatch)"
              >
                重放
              </button>
            </div>
          </div>
          <div v-if="expandedDispatchId === dispatch.id" class="dispatch-runs">
            <p v-if="loadingRuns" class="dispatch-meta">运行明细加载中...</p>
            <p v-else-if="dispatchRuns.length === 0" class="dispatch-meta">
              该 dispatch 暂无运行记录。
            </p>
            <div v-for="run in dispatchRuns" :key="run.id" class="run-row">
              <span class="status-badge" :class="{ active: run.status === 'succeeded' }">
                {{ run.status }}
              </span>
              <span class="dispatch-meta">
                第 {{ run.attempt }} 次 · {{ run.runtime }} · {{ run.handler }}
              </span>
              <span class="dispatch-meta">
                {{ formatTime(run.startedAt) }}
                <template v-if="run.durationMs != null">/ {{ run.durationMs }}ms</template>
              </span>
              <span v-if="run.errorMessage" class="dispatch-error">{{ run.errorMessage }}</span>
            </div>
          </div>
        </template>
      </div>
    </div>

    <Transition name="fade">
      <div v-if="installOpen" class="config-overlay" @click="installOpen = false">
        <section class="config-panel" @click.stop>
          <header>
            <div>
              <h3>导入插件包</h3>
              <p>上传本地 zip 或填写服务器插件包目录内的相对路径，安装后进入待审批状态。</p>
            </div>
            <button type="button" class="close-btn" @click="installOpen = false">关闭</button>
          </header>
          <form class="config-list" @submit.prevent="handleInstallPackage">
            <div class="config-field">
              <label class="field-name" for="install-file">上传插件包（zip）</label>
              <input
                id="install-file"
                ref="packageFileInput"
                class="file-input-hidden"
                type="file"
                accept=".zip,application/zip"
                @change="handlePackageFileChange"
              />
              <div class="upload-row">
                <button
                  type="button"
                  class="plugin-btn secondary"
                  :disabled="uploadingPackage"
                  @click="packageFileInput?.click()"
                >
                  {{ uploadingPackage ? "上传中..." : "选择文件" }}
                </button>
                <span class="field-help upload-name">
                  {{ uploadedFileName || "未选择文件" }}
                </span>
              </div>
              <p class="field-help">上传成功后自动填入路径与校验和；也可以跳过上传直接填路径。</p>
            </div>
            <div class="config-field">
              <label class="field-name" for="install-path"
                >插件包路径 <span class="req">*</span></label
              >
              <input
                id="install-path"
                v-model="installPath"
                class="cfg-input"
                type="text"
                placeholder="uploads/xxxx-plugin.zip"
                required
              />
              <p class="field-help">
                相对于服务器插件包目录（PLUGIN_PACKAGE_DIR）的路径，不接受绝对路径。
              </p>
            </div>
            <div class="config-field">
              <label class="field-name" for="install-checksum">SHA-256 校验和（可选）</label>
              <input
                id="install-checksum"
                v-model="installChecksum"
                class="cfg-input"
                type="text"
                placeholder="用于完整性校验"
              />
            </div>
            <div class="config-field">
              <label class="field-name" for="install-signature">签名（可选）</label>
              <input
                id="install-signature"
                v-model="installSignature"
                class="cfg-input"
                type="text"
                placeholder="Ed25519 签名，用于来源校验"
              />
            </div>
            <div class="config-actions">
              <button type="submit" class="plugin-btn primary" :disabled="installing">
                {{ installing ? "安装中..." : "安装" }}
              </button>
            </div>
          </form>
        </section>
      </div>
    </Transition>

    <!-- 插件包详情审查 -->
    <Transition name="fade">
      <div v-if="packageDetail" class="config-overlay" @click="packageDetail = null">
        <section class="config-panel" @click.stop>
          <header>
            <div>
              <h3>{{ packageDetail.name }} {{ packageDetail.packageVersion }}</h3>
              <p>
                {{ packageDetail.pluginId }} / {{ packageDetail.runtime }} / API
                {{ packageDetail.apiVersion }}
              </p>
            </div>
            <button type="button" class="close-btn" @click="packageDetail = null">关闭</button>
          </header>
          <div class="config-list detail-body">
            <p v-if="packageDetail.description" class="detail-desc">
              {{ packageDetail.description }}
            </p>
            <div class="detail-section">
              <h4>权限声明（{{ packageDetail.permissions.length }}）</h4>
              <p v-if="packageDetail.permissions.length === 0" class="field-help">
                未声明任何 Host API 权限。
              </p>
              <ul>
                <li v-for="perm in packageDetail.permissions" :key="perm.permissionKey">
                  <code>{{ perm.permissionKey }}</code>
                  <span v-if="perm.reason" class="field-help"> — {{ perm.reason }}</span>
                </li>
              </ul>
            </div>
            <div class="detail-section">
              <h4>Hook 订阅（{{ packageDetail.hooks.length }}）</h4>
              <p v-if="packageDetail.hooks.length === 0" class="field-help">未订阅任何事件。</p>
              <ul>
                <li v-for="hook in packageDetail.hooks" :key="`${hook.eventKey}-${hook.handler}`">
                  <code>{{ hook.eventKey }}</code>
                  <span class="field-help"> → {{ hook.handler }}</span>
                </li>
              </ul>
            </div>
            <div class="detail-section">
              <h4>管理菜单（{{ packageDetail.menu.length }}）</h4>
              <p v-if="packageDetail.menu.length === 0" class="field-help">未注册管理菜单。</p>
              <ul>
                <li v-for="item in packageDetail.menu" :key="item.path">
                  {{ item.label }} <span class="field-help">（{{ item.path }}）</span>
                </li>
              </ul>
            </div>
            <div class="detail-section">
              <h4>计划任务（{{ packageDetail.schedules.length }}）</h4>
              <p v-if="packageDetail.schedules.length === 0" class="field-help">未注册计划任务。</p>
              <ul>
                <li v-for="schedule in packageDetail.schedules" :key="schedule.taskKey">
                  <code>{{ schedule.taskKey }}</code>
                  <span class="field-help">
                    — {{ schedule.scheduleKind }} {{ schedule.scheduleValue }}
                  </span>
                </li>
              </ul>
            </div>
            <p class="field-help">
              包路径 {{ packageDetail.packagePath }} · 签名
              {{ packageDetail.signaturePresent ? "存在" : "无" }} · 状态
              {{ packageDetail.packageStatus }} / {{ packageDetail.approvalStatus ?? "-" }}
            </p>
            <div class="config-actions">
              <button
                v-if="packageDetail.packageStatus === 'pending_approval'"
                type="button"
                class="plugin-btn primary"
                :disabled="loadingAction === `approve:${packageDetail.packageId}`"
                @click="approveFromDetail"
              >
                审批通过
              </button>
            </div>
          </div>
        </section>
      </div>
    </Transition>

    <!-- 卸载二次确认 -->
    <Transition name="fade">
      <div v-if="pendingUninstall" class="config-overlay" @click="pendingUninstall = null">
        <section class="config-panel confirm-panel" @click.stop>
          <header>
            <div>
              <h3>确认卸载插件</h3>
              <p>{{ pendingUninstall.pluginId }}</p>
            </div>
          </header>
          <div class="confirm-body">
            <p>
              即将卸载插件 <b>{{ pluginName(pendingUninstall) }}</b
              >。系统会先停用再删除其包与配置， 此操作不可撤销。是否继续？
            </p>
            <div class="config-actions">
              <button type="button" class="plugin-btn secondary" @click="pendingUninstall = null">
                取消
              </button>
              <button
                type="button"
                class="plugin-btn danger"
                :disabled="loadingAction === `uninstall:${pendingUninstall.pluginId}`"
                @click="confirmUninstall"
              >
                {{
                  loadingAction === `uninstall:${pendingUninstall.pluginId}`
                    ? "卸载中..."
                    : "确认卸载"
                }}
              </button>
            </div>
          </div>
        </section>
      </div>
    </Transition>
  </div>
</template>

<style scoped lang="scss">
.admin-plugins-view {
  display: flex;
  flex-direction: column;
  gap: var(--fbz-space-4);
}

.summary-grid {
  display: grid;
  grid-template-columns: repeat(3, 1fr);
  gap: var(--fbz-space-3);
}

.summary-card {
  background: var(--fbz-color-panel-strong);
  border: 1px solid var(--fbz-color-line-soft);
  border-radius: 6px;
  padding: var(--fbz-space-4);
  display: flex;
  flex-direction: column;
  gap: 5px;

  .label,
  .meta {
    color: var(--fbz-color-text-muted);
    font-size: var(--fbz-font-size-xs);
    font-weight: 700;
  }

  .value {
    color: var(--fbz-color-text);
    font-family: var(--fbz-font-display);
    font-size: 22px;
    font-weight: 800;
  }
}

.sub-tabs-bar {
  display: flex;
  align-items: center;
  gap: var(--fbz-space-2);
  border-bottom: 1px solid var(--fbz-color-line-soft);
  padding-bottom: 8px;
}

.sub-tab-btn,
.refresh-btn {
  height: 32px;
  background: transparent;
  border: 0;
  color: var(--fbz-color-text-muted);
  font-size: var(--fbz-font-size-sm);
  font-weight: 700;
  padding: 0 var(--fbz-space-3);
  cursor: pointer;
  border-radius: var(--fbz-radius-control);
  transition: all var(--fbz-motion-fast);

  &:hover:not(:disabled) {
    color: var(--fbz-color-text);
    background: var(--fbz-color-panel-strong);
  }

  &.active {
    color: var(--fbz-color-brand-500);
    background: color-mix(in srgb, var(--fbz-color-brand-500) 8%, var(--fbz-color-panel-strong));
  }

  &:disabled {
    opacity: 0.5;
    cursor: not-allowed;
  }
}

.refresh-btn {
  margin-left: auto;
  border: 1px solid var(--fbz-color-line);
  background: var(--fbz-color-panel-strong);

  &.install {
    margin-left: auto;
    color: var(--fbz-color-brand-500);
    border-color: color-mix(in srgb, var(--fbz-color-brand-500) 40%, var(--fbz-color-line));
  }

  &.install + .refresh-btn {
    margin-left: 0;
  }
}

.plugins-grid {
  display: grid;
  grid-template-columns: repeat(auto-fill, minmax(320px, 1fr));
  gap: var(--fbz-space-3);
  margin-top: var(--fbz-space-2);
}

.plugin-card {
  background: var(--fbz-color-panel-strong);
  border: 1px solid var(--fbz-color-line-soft);
  border-radius: 6px;
  padding: var(--fbz-space-4);
  display: flex;
  flex-direction: column;
  justify-content: space-between;
  gap: 16px;
  transition: all var(--fbz-motion-fast);

  &:hover {
    border-color: var(--fbz-color-brand-500);
    transform: translateY(-1px);
    box-shadow: 0 4px 12px rgba(0, 0, 0, 0.15);
  }

  &.inactive {
    opacity: 0.65;
  }
}

.plugin-header {
  display: flex;
  flex-direction: column;
  gap: 4px;
  margin-bottom: 8px;
}

.title-row {
  display: flex;
  justify-content: space-between;
  align-items: center;
  gap: var(--fbz-space-3);
}

.plugin-name {
  font-size: 13px;
  font-weight: 700;
  color: var(--fbz-color-text);
}

.badge-row,
.plugin-actions {
  display: flex;
  align-items: center;
  gap: 6px;
}

.plugin-ver,
.status-badge {
  font-family: var(--fbz-font-display);
  font-size: 9px;
  font-weight: 800;
  background: var(--fbz-color-panel);
  border: 1px solid var(--fbz-color-line);
  padding: 1px 6px;
  border-radius: var(--fbz-radius-control);
  color: var(--fbz-color-text-muted);
}

.status-badge.active {
  background: color-mix(in srgb, var(--fbz-color-brand-500) 10%, transparent);
  color: var(--fbz-color-brand-500);
  border-color: color-mix(in srgb, var(--fbz-color-brand-500) 30%, transparent);
}

.author,
.plugin-desc,
.dispatch-meta {
  font-size: var(--fbz-font-size-xs);
  color: var(--fbz-color-text-muted);
}

.plugin-desc {
  margin: 0;
  line-height: 1.5;
}

.plugin-actions {
  justify-content: flex-end;
  border-top: 1px solid var(--fbz-color-line-soft);
  padding-top: 10px;
}

.plugin-btn {
  height: 26px;
  padding: 0 10px;
  font-size: 11px;
  font-weight: 700;
  border-radius: 4px;
  cursor: pointer;
  display: inline-flex;
  align-items: center;
  gap: 6px;
  transition: all var(--fbz-motion-fast);

  &.secondary {
    background: var(--fbz-color-panel);
    border: 1px solid var(--fbz-color-line);
    color: var(--fbz-color-text-soft);

    &:hover:not(:disabled) {
      background: var(--fbz-color-panel-elevated);
      color: var(--fbz-color-text);
    }
  }

  &.danger {
    background: transparent;
    border: 1px solid var(--fbz-color-danger-500);
    color: var(--fbz-color-danger-500);

    &:hover:not(:disabled) {
      background: color-mix(in srgb, var(--fbz-color-danger-500) 8%, transparent);
    }
  }

  &.primary {
    height: 32px;
    padding: 0 var(--fbz-space-4);
    background: var(--fbz-color-brand-500);
    border: 0;
    color: #07120a;

    &:hover:not(:disabled) {
      background: var(--fbz-color-brand-600);
    }
  }

  &:disabled {
    opacity: 0.55;
    cursor: not-allowed;
  }
}

.dispatch-list {
  background: var(--fbz-color-panel-strong);
  border: 1px solid var(--fbz-color-line-soft);
  border-radius: 6px;
  overflow: hidden;
}

.dispatch-row {
  padding: var(--fbz-space-4);
  border-top: 1px solid var(--fbz-color-line-soft);
  display: flex;
  justify-content: space-between;
  gap: var(--fbz-space-4);

  &:first-child {
    border-top: 0;
  }
}

.dispatch-main {
  min-width: 0;
  display: flex;
  flex-direction: column;
  gap: 4px;
}

.dispatch-title {
  color: var(--fbz-color-text);
  font-size: var(--fbz-font-size-sm);
  font-weight: 800;
}

.dispatch-error,
.error-text {
  margin: 0;
  color: var(--fbz-color-danger-500);
  font-size: var(--fbz-font-size-xs);
}

.dispatch-side {
  display: flex;
  align-items: center;
  gap: var(--fbz-space-2);
  flex-shrink: 0;
}

.empty-state {
  grid-column: 1 / -1;
  text-align: center;
  padding: 40px;
  background: var(--fbz-color-panel-strong);
  border: 1px dashed var(--fbz-color-line-soft);
  border-radius: 6px;
  color: var(--fbz-color-text-muted);
  font-size: var(--fbz-font-size-sm);

  &.compact {
    padding: var(--fbz-space-4);
  }
}

.config-overlay {
  position: fixed;
  inset: 0;
  z-index: 170;
  background: rgba(0, 0, 0, 0.72);
  display: grid;
  place-content: center;
  padding: var(--fbz-space-4);
}

.config-panel {
  width: min(640px, 94vw);
  max-height: 80vh;
  overflow: auto;
  background: var(--fbz-color-panel);
  border: 1px solid var(--fbz-color-line);
  border-radius: 8px;
  box-shadow: var(--fbz-shadow-panel);

  header {
    display: flex;
    justify-content: space-between;
    gap: var(--fbz-space-4);
    padding: var(--fbz-space-4);
    border-bottom: 1px solid var(--fbz-color-line-soft);

    h3,
    p {
      margin: 0;
    }

    p {
      margin-top: 4px;
      color: var(--fbz-color-text-muted);
      font-size: var(--fbz-font-size-xs);
    }
  }
}

.close-btn {
  height: 30px;
  border: 1px solid var(--fbz-color-line);
  border-radius: var(--fbz-radius-control);
  background: var(--fbz-color-panel-strong);
  color: var(--fbz-color-text-soft);
  cursor: pointer;
}

.config-list {
  padding: var(--fbz-space-4);
  display: flex;
  flex-direction: column;
  gap: var(--fbz-space-3);
}

.config-field {
  border: 1px solid var(--fbz-color-line-soft);
  border-radius: 6px;
  padding: var(--fbz-space-3);
  display: flex;
  flex-direction: column;
  gap: 8px;

  .field-name {
    display: flex;
    align-items: center;
    gap: 4px;
    color: var(--fbz-color-text);
    font-size: var(--fbz-font-size-sm);
    font-weight: 800;

    .req {
      color: var(--fbz-color-danger-500);
    }
  }

  .field-meta,
  .field-help {
    color: var(--fbz-color-text-muted);
    font-size: var(--fbz-font-size-xs);
    margin: 0;
    line-height: 1.4;
  }
}

.cfg-input {
  height: 36px;
  padding: 0 12px;
  border-radius: var(--fbz-radius-control);
  border: 1px solid var(--fbz-color-line);
  background: var(--fbz-color-panel);
  color: var(--fbz-color-text);
  font-size: var(--fbz-font-size-sm);
  width: 100%;

  &::placeholder {
    color: var(--fbz-color-text-muted);
  }

  &:focus-visible {
    outline: none;
    border-color: var(--fbz-color-brand-500);
    box-shadow: var(--fbz-shadow-focus);
  }
}

.cfg-switch {
  position: relative;
  display: inline-block;
  width: 44px;
  height: 22px;
  flex-shrink: 0;

  input {
    opacity: 0;
    width: 0;
    height: 0;
  }

  .switch-slide-thumb {
    position: absolute;
    cursor: pointer;
    inset: 0;
    background-color: var(--fbz-color-line-bright);
    border-radius: 22px;
    transition: background-color var(--fbz-motion-fast);

    &::before {
      position: absolute;
      content: "";
      height: 16px;
      width: 16px;
      left: 3px;
      bottom: 3px;
      background-color: white;
      border-radius: 50%;
      transition: transform var(--fbz-motion-fast);
      box-shadow: 0 1px 3px rgba(0, 0, 0, 0.2);
    }
  }

  input:checked + .switch-slide-thumb {
    background-color: var(--fbz-color-brand-500);

    &::before {
      transform: translateX(22px);
    }
  }

  input:focus-visible + .switch-slide-thumb {
    box-shadow: var(--fbz-shadow-focus);
  }
}

.config-actions {
  display: flex;
  justify-content: flex-end;
  gap: var(--fbz-space-2);
  padding-top: var(--fbz-space-2);
}

.file-input-hidden {
  display: none;
}

.upload-row {
  display: flex;
  align-items: center;
  gap: var(--fbz-space-3);

  .upload-name {
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
}

.detail-body {
  .detail-desc {
    margin: 0;
    color: var(--fbz-color-text-soft);
    font-size: var(--fbz-font-size-sm);
    line-height: 1.6;
  }

  .detail-section {
    border: 1px solid var(--fbz-color-line-soft);
    border-radius: 6px;
    padding: var(--fbz-space-3);

    h4 {
      margin: 0 0 8px;
      color: var(--fbz-color-text);
      font-size: var(--fbz-font-size-sm);
    }

    ul {
      margin: 0;
      padding-left: 18px;
      display: flex;
      flex-direction: column;
      gap: 4px;
      color: var(--fbz-color-text-soft);
      font-size: var(--fbz-font-size-xs);

      code {
        color: var(--fbz-color-brand-500);
        font-family: var(--fbz-font-display);
        font-size: 10px;
      }
    }
  }
}

.dispatch-runs {
  padding: var(--fbz-space-3) var(--fbz-space-4);
  border-top: 1px dashed var(--fbz-color-line-soft);
  background: var(--fbz-color-panel);
  display: flex;
  flex-direction: column;
  gap: 6px;

  .run-row {
    display: flex;
    align-items: center;
    gap: var(--fbz-space-3);
    flex-wrap: wrap;
  }

  p {
    margin: 0;
  }
}

.confirm-panel .confirm-body {
  padding: var(--fbz-space-4);

  p {
    margin: 0 0 var(--fbz-space-4);
    color: var(--fbz-color-text-soft);
    font-size: var(--fbz-font-size-sm);
    line-height: 1.6;

    b {
      color: var(--fbz-color-text);
    }
  }
}

.fade-enter-active,
.fade-leave-active {
  transition: opacity var(--fbz-motion-fast) ease;
}
.fade-enter-from,
.fade-leave-to {
  opacity: 0;
}

@media (max-width: 760px) {
  .summary-grid {
    grid-template-columns: 1fr;
  }

  .dispatch-row,
  .dispatch-side {
    align-items: flex-start;
    flex-direction: column;
  }
}
</style>
