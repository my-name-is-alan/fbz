<script setup lang="ts">
import {
  getDefaultDirectoryBrowser,
  getParentPath,
  listDirectoryContents,
  listDrives,
  validatePath,
} from "@/service/modules/environment.ts";
import { useUiStore } from "@/stores/ui.ts";
import { useBodyScrollLock } from "@/composables/useBodyScrollLock.ts";
import type { FileSystemEntryInfo } from "@/types/environment.ts";

const uiStore = useUiStore();
const { filePicker } = storeToRefs(uiStore);
useBodyScrollLock(() => filePicker.value.open);

interface Breadcrumb {
  name: string;
  path: string;
}

const currentPath = shallowRef("");
const manualPathInput = shallowRef("");
const entries = ref<FileSystemEntryInfo[]>([]);
const drives = ref<FileSystemEntryInfo[]>([]);
const loading = shallowRef(false);
const loadingRoots = shallowRef(false);
const validating = shallowRef(false);
const errorMessage = shallowRef("");
const rootsErrorMessage = shallowRef("");

const directoryEntries = computed(() =>
  entries.value.filter(
    (entry) =>
      entry.Type === "Directory" ||
      entry.Type === "NetworkComputer" ||
      entry.Type === "NetworkShare",
  ),
);

const canGoUp = computed(() => Boolean(currentPath.value.trim()));

const breadcrumbs = computed<Breadcrumb[]>(() => buildBreadcrumbs(currentPath.value));

watch(
  () => filePicker.value.open,
  async (open) => {
    if (!open) return;
    await initializeBrowser();
  },
);

function buildBreadcrumbs(path: string): Breadcrumb[] {
  const normalized = path.trim();
  if (!normalized) {
    return [{ name: "服务器", path: "" }];
  }

  if (/^[A-Za-z]:[\\/]/.test(normalized)) {
    const drive = normalized.slice(0, 3);
    const rest = normalized
      .slice(3)
      .split(/[\\/]+/)
      .filter(Boolean);
    const crumbs: Breadcrumb[] = [{ name: drive, path: drive }];
    let nextPath = drive.replace(/[\\/]$/, "");
    for (const part of rest) {
      nextPath += `\\${part}`;
      crumbs.push({ name: part, path: nextPath });
    }
    return crumbs;
  }

  const parts = normalized.split("/").filter(Boolean);
  const crumbs: Breadcrumb[] = [{ name: "/", path: "/" }];
  let nextPath = "";
  for (const part of parts) {
    nextPath += `/${part}`;
    crumbs.push({ name: part, path: nextPath });
  }
  return crumbs;
}

function isActiveRoot(rootPath: string): boolean {
  const current = currentPath.value.trim().toLowerCase().replaceAll("/", "\\");
  const root = rootPath.trim().toLowerCase().replaceAll("/", "\\");
  if (!root) return !current;
  return current === root || current.startsWith(root.endsWith("\\") ? root : `${root}\\`);
}

function setCurrentPath(path: string) {
  currentPath.value = path;
  manualPathInput.value = path;
}

function extractErrorMessage(err: unknown, fallback: string): string {
  if (typeof err === "object" && err && "response" in err) {
    const response = (err as { response?: { data?: unknown } }).response;
    if (typeof response?.data === "string" && response.data.trim()) {
      return response.data;
    }
    if (
      typeof response?.data === "object" &&
      response.data &&
      "message" in response.data &&
      typeof response.data.message === "string"
    ) {
      return response.data.message;
    }
    if (
      typeof response?.data === "object" &&
      response.data &&
      "error" in response.data &&
      typeof response.data.error === "object" &&
      response.data.error &&
      "message" in response.data.error &&
      typeof response.data.error.message === "string"
    ) {
      return response.data.error.message;
    }
  }
  if (err instanceof Error && err.message) return err.message;
  return fallback;
}

async function initializeBrowser() {
  errorMessage.value = "";
  rootsErrorMessage.value = "";
  entries.value = [];
  const initialPath = filePicker.value.currentPath.trim();

  await loadRoots();

  try {
    const defaultInfo = await getDefaultDirectoryBrowser();
    const path = initialPath || defaultInfo.Path || drives.value[0]?.Path || "";
    if (path) {
      await loadFolder(path);
    } else {
      setCurrentPath("");
      errorMessage.value = "后端未返回可浏览的默认目录。请手动输入服务器绝对路径。";
    }
  } catch (err) {
    const fallbackPath = initialPath || drives.value[0]?.Path || "";
    if (fallbackPath) {
      await loadFolder(fallbackPath);
      return;
    }
    setCurrentPath(initialPath);
    errorMessage.value = extractErrorMessage(err, "读取默认目录失败，请手动输入服务器绝对路径。");
  }
}

async function loadRoots() {
  loadingRoots.value = true;
  rootsErrorMessage.value = "";
  try {
    drives.value = await listDrives();
  } catch (err) {
    drives.value = [];
    rootsErrorMessage.value = extractErrorMessage(err, "读取服务器磁盘列表失败。");
  } finally {
    loadingRoots.value = false;
  }
}

async function loadFolder(path: string) {
  const nextPath = path.trim();
  if (!nextPath) return;

  loading.value = true;
  errorMessage.value = "";
  try {
    const nextEntries = await listDirectoryContents({
      path: nextPath,
      includeFiles: false,
      includeDirectories: true,
    });
    entries.value = nextEntries;
    setCurrentPath(nextPath);
  } catch (err) {
    entries.value = [];
    setCurrentPath(nextPath);
    errorMessage.value = extractErrorMessage(err, "目录不可读取，请确认路径存在且后端有权限访问。");
  } finally {
    loading.value = false;
  }
}

async function loadParentFolder() {
  if (!currentPath.value.trim()) return;
  loading.value = true;
  errorMessage.value = "";
  try {
    const parent = await getParentPath(currentPath.value);
    if (parent) {
      await loadFolder(parent);
    } else {
      loading.value = false;
    }
  } catch (err) {
    errorMessage.value = extractErrorMessage(err, "读取父目录失败。");
    loading.value = false;
  }
}

async function handleManualSubmit() {
  const path = manualPathInput.value.trim();
  if (!path) return;
  await loadFolder(path);
}

async function handleConfirm() {
  const path = currentPath.value.trim();
  if (!path) {
    uiStore.showToast("请先选择或输入服务器目录。", "warning");
    return;
  }

  validating.value = true;
  errorMessage.value = "";
  try {
    await validatePath(path, { isFile: false, validateWriteable: false });
    uiStore.closeFilePicker(path);
  } catch (err) {
    errorMessage.value = extractErrorMessage(err, "路径校验失败，请选择后端可访问的目录。");
  } finally {
    validating.value = false;
  }
}

function handleCancel() {
  uiStore.closeFilePicker();
}

useEventListener(window, "keydown", (e: KeyboardEvent) => {
  if (e.key === "Escape" && filePicker.value.open) {
    handleCancel();
  }
});
</script>

<template>
  <Transition name="fade">
    <div v-if="filePicker.open" class="file-picker-overlay" @click="handleCancel">
      <div
        class="file-picker-modal"
        role="dialog"
        aria-modal="true"
        aria-labelledby="file-picker-title"
        @click.stop
      >
        <header class="modal-header">
          <div class="header-title">
            <span class="icon" aria-hidden="true">📁</span>
            <h2 id="file-picker-title">服务器端文件夹选择器</h2>
          </div>
          <button
            class="close-btn"
            type="button"
            aria-label="关闭文件夹选择器"
            @click="handleCancel"
          >
            ✕
          </button>
        </header>

        <div class="path-bar">
          <input
            v-model="manualPathInput"
            type="text"
            placeholder="输入 Rust 后端可访问的绝对路径..."
            aria-label="服务器绝对路径"
            @keyup.enter="handleManualSubmit"
          />
          <button
            class="action-btn"
            type="button"
            :disabled="loading"
            aria-label="前往输入的路径"
            @click="handleManualSubmit"
          >
            前往
          </button>
        </div>

        <nav class="breadcrumbs-container" aria-label="路径导航">
          <span class="label">当前路径:</span>
          <div class="crumbs">
            <template v-for="(crumb, idx) in breadcrumbs" :key="`${crumb.path}-${idx}`">
              <span
                class="crumb-link"
                :class="{ active: idx === breadcrumbs.length - 1 }"
                tabindex="0"
                role="link"
                :aria-label="
                  idx === breadcrumbs.length - 1
                    ? `当前目录: ${crumb.name}`
                    : `前往目录 ${crumb.name}`
                "
                @click="loadFolder(crumb.path)"
                @keydown.enter="loadFolder(crumb.path)"
                @keydown.space.prevent="loadFolder(crumb.path)"
              >
                {{ crumb.name }}
              </span>
              <span v-if="idx < breadcrumbs.length - 1" class="crumb-separator" aria-hidden="true">
                /
              </span>
            </template>
          </div>
        </nav>

        <div class="modal-body">
          <aside class="disk-sidebar" aria-label="服务器磁盘">
            <div class="sidebar-heading">
              <h3>服务器根目录</h3>
              <button
                class="refresh-btn"
                type="button"
                :disabled="loadingRoots"
                aria-label="刷新服务器磁盘列表"
                @click="loadRoots"
              >
                ↻
              </button>
            </div>
            <div v-if="loadingRoots" class="sidebar-status">读取中...</div>
            <div v-else-if="rootsErrorMessage" class="sidebar-status warning">
              {{ rootsErrorMessage }}
            </div>
            <div v-else-if="drives.length" class="disk-list">
              <button
                v-for="drive in drives"
                :key="drive.Path"
                class="disk-item"
                :class="{ active: isActiveRoot(drive.Path) }"
                type="button"
                @click="loadFolder(drive.Path)"
              >
                <span aria-hidden="true">▣</span>
                <span>{{ drive.Name || drive.Path }}</span>
              </button>
            </div>
            <div v-else class="sidebar-status">没有可枚举的根目录。</div>
          </aside>

          <section class="folder-explorer" aria-label="目录内容">
            <div class="explorer-toolbar">
              <div class="current-path-label" :title="currentPath">
                {{ currentPath || "未选择路径" }}
              </div>
              <div class="action-buttons">
                <button
                  class="util-btn"
                  type="button"
                  :disabled="!canGoUp || loading"
                  @click="loadParentFolder"
                >
                  上一级
                </button>
                <button
                  class="util-btn brand-btn"
                  type="button"
                  :disabled="!currentPath || loading"
                  @click="loadFolder(currentPath)"
                >
                  刷新
                </button>
              </div>
            </div>

            <div class="explorer-content">
              <div v-if="loading" class="loader-view" role="status" aria-live="polite">
                <span class="spinner" aria-hidden="true" />
                <span>正在读取后端目录...</span>
              </div>

              <div v-else-if="errorMessage" class="empty-view error-view">
                <span class="empty-icon" aria-hidden="true">!</span>
                <span class="empty-title">目录不可用</span>
                <span class="empty-sub">{{ errorMessage }}</span>
              </div>

              <div v-else-if="directoryEntries.length === 0" class="empty-view">
                <span class="empty-icon" aria-hidden="true">∅</span>
                <span class="empty-title">没有可继续进入的子目录</span>
                <span class="empty-sub">可以直接选择当前目录，或在上方输入其他绝对路径。</span>
              </div>

              <div v-else class="folder-grid">
                <button
                  v-for="entry in directoryEntries"
                  :key="entry.Path"
                  class="folder-card"
                  type="button"
                  :title="entry.Path"
                  @click="loadFolder(entry.Path)"
                >
                  <span class="folder-icon" aria-hidden="true">📁</span>
                  <span class="folder-name">{{ entry.Name || entry.Path }}</span>
                </button>
              </div>
            </div>
          </section>
        </div>

        <footer class="modal-footer">
          <div class="warning-text">
            <span>目录列表来自 Rust 后端，确认时会再次校验路径存在且为目录。</span>
          </div>
          <div class="footer-actions">
            <button class="footer-btn secondary" type="button" @click="handleCancel">取消</button>
            <button
              class="footer-btn primary"
              type="button"
              :disabled="validating || !currentPath.trim()"
              @click="handleConfirm"
            >
              {{ validating ? "校验中..." : "确定选择" }}
            </button>
          </div>
        </footer>
      </div>
    </div>
  </Transition>
</template>

<style scoped lang="scss">
.file-picker-overlay {
  position: fixed;
  inset: 0;
  z-index: 150;
  background: rgba(0, 0, 0, 0.7);
  backdrop-filter: blur(8px);
  display: grid;
  place-content: center;
}

.file-picker-modal {
  width: 900px;
  max-width: 95vw;
  height: 620px;
  max-height: 92vh;
  background: var(--fbz-color-panel);
  border: 1px solid var(--fbz-color-line);
  border-radius: var(--fbz-radius-card);
  box-shadow: var(--fbz-shadow-panel);
  display: flex;
  flex-direction: column;
  overflow: hidden;
  color: var(--fbz-color-text);
  font-family: var(--fbz-font-sans);
}

.modal-header {
  padding: var(--fbz-space-4) var(--fbz-space-5);
  border-bottom: 1px solid var(--fbz-color-line-soft);
  display: flex;
  justify-content: space-between;
  align-items: center;

  .header-title {
    display: flex;
    align-items: center;
    gap: var(--fbz-space-2);

    .icon {
      font-size: 20px;
    }

    h2 {
      margin: 0;
      font-size: var(--fbz-font-size-lg);
      font-weight: 800;
    }
  }

  .close-btn {
    background: none;
    border: 0;
    color: var(--fbz-color-text-muted);
    font-size: 16px;
    cursor: pointer;
    padding: 4px;
    transition: color var(--fbz-motion-fast);

    &:hover {
      color: var(--fbz-color-text);
    }
  }
}

.path-bar {
  padding: var(--fbz-space-3) var(--fbz-space-5);
  background: var(--fbz-color-bg-strong);
  border-bottom: 1px solid var(--fbz-color-line-soft);
  display: flex;
  gap: var(--fbz-space-2);

  input {
    flex: 1;
    min-width: 0;
    height: 36px;
    background: var(--fbz-color-panel);
    border: 1px solid var(--fbz-color-line);
    border-radius: var(--fbz-radius-control);
    padding: 0 var(--fbz-space-3);
    color: var(--fbz-color-text);
    font-size: var(--fbz-font-size-sm);

    &:focus {
      outline: none;
      border-color: var(--fbz-color-brand-500);
    }
  }

  .action-btn {
    height: 36px;
    padding: 0 var(--fbz-space-4);
    background: var(--fbz-color-panel-strong);
    border: 1px solid var(--fbz-color-line);
    border-radius: var(--fbz-radius-control);
    color: var(--fbz-color-text-soft);
    font-weight: 600;
    font-size: var(--fbz-font-size-sm);
    cursor: pointer;

    &:hover:not(:disabled) {
      background: var(--fbz-color-panel-elevated);
      color: var(--fbz-color-text);
    }

    &:disabled {
      opacity: 0.55;
      cursor: not-allowed;
    }
  }
}

.breadcrumbs-container {
  padding: var(--fbz-space-2) var(--fbz-space-5);
  font-size: var(--fbz-font-size-xs);
  display: flex;
  gap: var(--fbz-space-2);
  align-items: center;
  background: var(--fbz-color-bg-strong);
  border-bottom: 1px solid var(--fbz-color-line-soft);

  .label {
    color: var(--fbz-color-text-muted);
    font-weight: 700;
    flex-shrink: 0;
  }

  .crumbs {
    display: flex;
    align-items: center;
    gap: 4px;
    color: var(--fbz-color-text-soft);
    overflow-x: auto;
    white-space: nowrap;
    scrollbar-width: none;
  }

  .crumb-link {
    cursor: pointer;
    font-weight: 600;

    &:hover {
      color: var(--fbz-color-brand-500);
      text-decoration: underline;
    }

    &.active {
      color: var(--fbz-color-text);
      font-weight: 800;
      pointer-events: none;
    }
  }

  .crumb-separator {
    color: var(--fbz-color-text-disabled);
  }
}

.modal-body {
  flex: 1;
  display: flex;
  min-height: 0;
  overflow: hidden;
}

.disk-sidebar {
  width: 220px;
  border-right: 1px solid var(--fbz-color-line-soft);
  background: var(--fbz-color-panel-strong);
  padding: var(--fbz-space-4);
  display: flex;
  flex-direction: column;
  gap: var(--fbz-space-3);

  .sidebar-heading {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: var(--fbz-space-2);
  }

  h3 {
    margin: 0;
    font-size: var(--fbz-font-size-xs);
    text-transform: uppercase;
    color: var(--fbz-color-text-muted);
    letter-spacing: 1px;
    font-weight: 700;
  }

  .refresh-btn {
    width: 26px;
    height: 26px;
    border: 1px solid var(--fbz-color-line);
    border-radius: var(--fbz-radius-control);
    background: var(--fbz-color-panel);
    color: var(--fbz-color-text-soft);
    cursor: pointer;

    &:hover:not(:disabled) {
      color: var(--fbz-color-text);
      background: var(--fbz-color-panel-elevated);
    }

    &:disabled {
      opacity: 0.5;
      cursor: not-allowed;
    }
  }

  .disk-list {
    display: flex;
    flex-direction: column;
    gap: 6px;
    overflow-y: auto;
  }

  .sidebar-status {
    font-size: var(--fbz-font-size-xs);
    color: var(--fbz-color-text-muted);
    line-height: 1.5;

    &.warning {
      color: var(--fbz-color-amber-500);
    }
  }

  .disk-item {
    width: 100%;
    border: 1px solid transparent;
    background: transparent;
    padding: 10px 12px;
    border-radius: var(--fbz-radius-control);
    color: var(--fbz-color-text-soft);
    display: flex;
    align-items: center;
    gap: 10px;
    font-size: var(--fbz-font-size-sm);
    font-weight: 600;
    cursor: pointer;
    text-align: left;
    transition: all var(--fbz-motion-fast);

    span:last-child {
      overflow: hidden;
      text-overflow: ellipsis;
      white-space: nowrap;
    }

    &:hover {
      background: var(--fbz-color-panel-elevated);
      color: var(--fbz-color-text);
    }

    &.active {
      background: color-mix(
        in srgb,
        var(--fbz-color-brand-500) 8%,
        var(--fbz-color-panel-elevated)
      );
      color: var(--fbz-color-brand-500);
      border-color: color-mix(in srgb, var(--fbz-color-brand-500) 20%, transparent);
    }
  }
}

.folder-explorer {
  flex: 1;
  min-width: 0;
  padding: var(--fbz-space-4);
  display: flex;
  flex-direction: column;
  gap: var(--fbz-space-4);
  overflow: hidden;
}

.explorer-toolbar {
  display: flex;
  justify-content: space-between;
  align-items: center;
  gap: var(--fbz-space-4);

  .current-path-label {
    flex: 1;
    min-width: 0;
    height: 34px;
    display: flex;
    align-items: center;
    padding: 0 var(--fbz-space-3);
    border: 1px solid var(--fbz-color-line-soft);
    border-radius: var(--fbz-radius-control);
    background: var(--fbz-color-bg-strong);
    color: var(--fbz-color-text-soft);
    font-size: var(--fbz-font-size-xs);
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .action-buttons {
    display: flex;
    gap: var(--fbz-space-2);
  }

  .util-btn {
    height: 34px;
    padding: 0 12px;
    background: var(--fbz-color-panel-strong);
    border: 1px solid var(--fbz-color-line);
    border-radius: var(--fbz-radius-control);
    color: var(--fbz-color-text-soft);
    font-size: var(--fbz-font-size-sm);
    font-weight: 600;
    cursor: pointer;

    &:hover:not(:disabled) {
      background: var(--fbz-color-panel-elevated);
      color: var(--fbz-color-text);
    }

    &:disabled {
      opacity: 0.5;
      cursor: not-allowed;
    }

    &.brand-btn {
      border-color: color-mix(in srgb, var(--fbz-color-brand-500) 30%, transparent);
      color: var(--fbz-color-brand-500);

      &:hover:not(:disabled) {
        background: color-mix(in srgb, var(--fbz-color-brand-500) 8%, transparent);
      }
    }
  }
}

.explorer-content {
  flex: 1;
  min-height: 0;
  overflow-y: auto;
  border: 1px solid var(--fbz-color-line-soft);
  border-radius: var(--fbz-radius-control);
  background: var(--fbz-color-bg-strong);
  padding: var(--fbz-space-4);
  position: relative;
}

.loader-view {
  position: absolute;
  inset: 0;
  display: flex;
  flex-direction: column;
  justify-content: center;
  align-items: center;
  gap: var(--fbz-space-3);
  background: rgba(10, 10, 11, 0.8);
  font-size: var(--fbz-font-size-sm);
  color: var(--fbz-color-text-soft);

  .spinner {
    width: 28px;
    height: 28px;
    border: 3px solid var(--fbz-color-line);
    border-top-color: var(--fbz-color-brand-500);
    border-radius: 50%;
    animation: spin 0.8s linear infinite;
  }
}

@keyframes spin {
  to {
    transform: rotate(360deg);
  }
}

.empty-view {
  min-height: 100%;
  display: flex;
  flex-direction: column;
  justify-content: center;
  align-items: center;
  text-align: center;
  color: var(--fbz-color-text-muted);
  padding: var(--fbz-space-5);

  .empty-icon {
    width: 40px;
    height: 40px;
    display: grid;
    place-content: center;
    border-radius: 999px;
    border: 1px solid var(--fbz-color-line);
    margin-bottom: var(--fbz-space-3);
    font-weight: 800;
  }

  .empty-title {
    font-size: var(--fbz-font-size-md);
    font-weight: 700;
    color: var(--fbz-color-text-soft);
    margin-bottom: 4px;
  }

  .empty-sub {
    font-size: var(--fbz-font-size-sm);
    line-height: 1.5;
    max-width: 420px;
  }

  &.error-view .empty-icon {
    border-color: color-mix(in srgb, var(--fbz-color-danger-500) 40%, transparent);
    color: var(--fbz-color-danger-500);
  }
}

.folder-grid {
  display: grid;
  grid-template-columns: repeat(auto-fill, minmax(130px, 1fr));
  gap: var(--fbz-space-3);
}

.folder-card {
  min-height: 102px;
  border: 1px solid var(--fbz-color-line);
  background: var(--fbz-color-panel);
  border-radius: var(--fbz-radius-card);
  padding: var(--fbz-space-3) var(--fbz-space-2);
  display: flex;
  flex-direction: column;
  align-items: center;
  justify-content: center;
  gap: 8px;
  cursor: pointer;
  user-select: none;
  color: var(--fbz-color-text);
  transition: all var(--fbz-motion-fast);

  &:hover {
    border-color: var(--fbz-color-brand-500);
    background: var(--fbz-color-panel-strong);
    transform: translateY(-2px);
  }

  .folder-icon {
    font-size: 30px;
  }

  .folder-name {
    font-size: var(--fbz-font-size-xs);
    font-weight: 600;
    text-align: center;
    width: 100%;
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }
}

.modal-footer {
  padding: var(--fbz-space-4) var(--fbz-space-5);
  border-top: 1px solid var(--fbz-color-line-soft);
  display: flex;
  justify-content: space-between;
  align-items: center;
  gap: var(--fbz-space-4);

  .warning-text {
    font-size: var(--fbz-font-size-sm);
    font-weight: 600;
    color: var(--fbz-color-amber-500);
    max-width: 520px;
  }

  .footer-actions {
    display: flex;
    gap: var(--fbz-space-2);
  }

  .footer-btn {
    height: 38px;
    padding: 0 var(--fbz-space-5);
    border-radius: var(--fbz-radius-control);
    font-size: var(--fbz-font-size-sm);
    font-weight: 700;
    cursor: pointer;
    transition: all var(--fbz-motion-fast);

    &.secondary {
      background: var(--fbz-color-panel-strong);
      border: 1px solid var(--fbz-color-line);
      color: var(--fbz-color-text-soft);

      &:hover {
        background: var(--fbz-color-panel-elevated);
        color: var(--fbz-color-text);
      }
    }

    &.primary {
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
}

@media (max-width: 720px) {
  .file-picker-modal {
    width: 96vw;
    height: 88vh;
  }

  .modal-body {
    flex-direction: column;
  }

  .disk-sidebar {
    width: auto;
    max-height: 150px;
    border-right: 0;
    border-bottom: 1px solid var(--fbz-color-line-soft);
  }

  .disk-sidebar .disk-list {
    flex-direction: row;
    overflow-x: auto;
  }

  .disk-sidebar .disk-item {
    min-width: 112px;
  }

  .explorer-toolbar,
  .modal-footer {
    align-items: stretch;
    flex-direction: column;
  }

  .modal-footer .footer-actions {
    justify-content: flex-end;
  }
}

.fade-enter-active,
.fade-leave-active {
  transition: opacity var(--fbz-motion-base) ease;
}
.fade-enter-from,
.fade-leave-to {
  opacity: 0;
}
</style>
