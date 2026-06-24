<script setup lang="ts">
import { useUiStore } from "@/stores/ui.ts";

const uiStore = useUiStore();
const { filePicker } = storeToRefs(uiStore);

// Mock server directories
const diskOptions = [
  { name: "系统盘 (C:)", root: "/c" },
  { name: "NAS 存储池 (NAS_SHARE)", root: "/media/nas" },
  { name: "备份存储盘 (USB_BACKUP)", root: "/media/backup" },
];

const mockDirectories: Record<string, string[]> = {
  "/c": ["Program Files", "Users", "Windows", "System32"],
  "/c/Users": ["Administrator", "Default", "Public"],
  "/c/Users/Administrator": ["Downloads", "Documents", "Desktop"],
  "/c/Windows": ["Fonts", "System32", "Temp"],
  "/c/Windows/System32": ["drivers", "config", "cmd.exe"],
  "/media/nas": ["电影", "电视剧", "动漫", "纪录片", "音乐", "未分类"],
  "/media/nas/电影": ["2026", "2025", "科幻电影", "动作片", "奥斯卡获奖影片"],
  "/media/nas/电影/2026": ["阿凡达：水之道", "流浪地球3", "奥本海默", "沙丘2"],
  "/media/nas/电视剧": ["权力的游戏", "狂飙", "风骚律师", "三体"],
  "/media/nas/动漫": ["名侦探柯南", "航海王", "火影忍者"],
  "/media/nas/纪录片": ["地球脉动", "舌尖上的中国"],
  "/media/nas/音乐": ["周杰伦", "Taylor Swift", "陈奕迅"],
  "/media/backup": ["2025备份", "个人相册", "工作文档"],
};

const currentPath = ref("/media/nas");
const activeDisk = ref("/media/nas");
const folderList = ref<string[]>([]);
const loading = ref(false);
const search = ref("");
const manualPathInput = ref("");
const permissionWarning = ref("");

// Initialize when modal opens
watch(
  () => filePicker.value.open,
  (open) => {
    if (open) {
      currentPath.value = filePicker.value.currentPath;
      manualPathInput.value = currentPath.value;
      // Sync active disk
      const matchingDisk = diskOptions.find((d) => currentPath.value.startsWith(d.root));
      if (matchingDisk) {
        activeDisk.value = matchingDisk.root;
      }
      loadFolder(currentPath.value);
    }
  },
);

async function loadFolder(path: string) {
  loading.value = true;
  permissionWarning.value = "";

  // Simulate network latency
  await new Promise((resolve) => setTimeout(resolve, 300));

  currentPath.value = path;
  manualPathInput.value = path;

  // Custom permission warnings for mock
  if (path.includes("System32") || path === "/c/Windows") {
    permissionWarning.value = "⚠️ 系统敏感文件夹，管理用户缺乏写入 NFO 及海报权限。";
  } else if (path === "/media/backup") {
    permissionWarning.value = "💡 只读存储空间，不支持自动下载海报，建议开启直连 TMDB 模式。";
  }

  // Retrieve folders
  folderList.value = mockDirectories[path] || [];
  loading.value = false;
}

const filteredFolders = computed(() => {
  if (!search.value) return folderList.value;
  return folderList.value.filter((f) => f.toLowerCase().includes(search.value.toLowerCase()));
});

// Breadcrumbs computed
const breadcrumbs = computed(() => {
  const parts = currentPath.value.split("/").filter(Boolean);
  const crumbs = [{ name: "根目录", path: "/" }];

  let tempPath = "";
  for (const part of parts) {
    tempPath += "/" + part;
    // Map custom disk names for better readability in crumbs
    const matchedDisk = diskOptions.find((d) => d.root === tempPath);
    crumbs.push({
      name: matchedDisk ? matchedDisk.name.split(" ")[0] : part,
      path: tempPath,
    });
  }
  return crumbs;
});

function selectDisk(rootPath: string) {
  activeDisk.value = rootPath;
  loadFolder(rootPath);
}

function handleFolderClick(folderName: string) {
  const newPath =
    currentPath.value === "/" ? `/${folderName}` : `${currentPath.value}/${folderName}`;
  loadFolder(newPath);
}

function navigateBack() {
  if (currentPath.value === "/" || diskOptions.some((d) => d.root === currentPath.value)) {
    loadFolder("/");
    return;
  }
  const parts = currentPath.value.split("/");
  parts.pop();
  const parent = parts.join("/") || "/";
  loadFolder(parent);
}

function createNewFolder() {
  const name = prompt("请输入新建文件夹的名称:", "新建文件夹");
  if (!name) return;

  if (!mockDirectories[currentPath.value]) {
    mockDirectories[currentPath.value] = [];
  }
  mockDirectories[currentPath.value].push(name);
  loadFolder(currentPath.value);
}

function handleManualSubmit() {
  let path = manualPathInput.value.trim();
  if (!path) return;
  if (!path.startsWith("/")) {
    path = "/" + path;
  }
  loadFolder(path);
}

function handleConfirm() {
  uiStore.closeFilePicker(currentPath.value);
}

function handleCancel() {
  uiStore.closeFilePicker();
}

function handleFolderKeydown(event: KeyboardEvent, folder: string) {
  if (event.key === "Enter") {
    handleFolderClick(folder);
  } else if (event.key === " ") {
    event.preventDefault();
    manualPathInput.value =
      currentPath.value === "/" ? `/${folder}` : `${currentPath.value}/${folder}`;
  }
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
        <!-- Modal Header -->
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

        <!-- Manual Path Input -->
        <div class="path-bar">
          <input
            v-model="manualPathInput"
            type="text"
            placeholder="手动输入服务器绝对路径..."
            aria-label="服务器绝对路径"
            @keyup.enter="handleManualSubmit"
          />
          <button
            class="action-btn"
            type="button"
            aria-label="前往输入的路径"
            @click="handleManualSubmit"
          >
            前往
          </button>
        </div>

        <!-- Breadcrumbs -->
        <nav class="breadcrumbs-container" aria-label="路径导航">
          <span class="label">当前路径:</span>
          <div class="crumbs">
            <template v-for="(crumb, idx) in breadcrumbs" :key="crumb.path">
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
              <span v-if="idx < breadcrumbs.length - 1" class="crumb-separator" aria-hidden="true"
                >/</span
              >
            </template>
          </div>
        </nav>

        <!-- Main Body -->
        <div class="modal-body">
          <!-- Sidebar: Disks -->
          <aside class="disk-sidebar" aria-label="存储盘选择">
            <h3>存储介质</h3>
            <div class="disk-list" role="radiogroup" aria-label="存储介质列表">
              <button
                v-for="disk in diskOptions"
                :key="disk.root"
                class="disk-item"
                :class="{ active: activeDisk === disk.root }"
                type="button"
                role="radio"
                :aria-checked="activeDisk === disk.root"
                @click="selectDisk(disk.root)"
              >
                <span class="disk-icon" aria-hidden="true">💽</span>
                <span class="disk-name">{{ disk.name }}</span>
              </button>
            </div>
          </aside>

          <!-- Folder Explorer -->
          <section class="folder-explorer" aria-label="文件夹浏览器">
            <div class="explorer-toolbar">
              <div class="search-input">
                <span class="search-icon" aria-hidden="true">🔍</span>
                <input
                  v-model="search"
                  type="text"
                  placeholder="过滤当前目录..."
                  aria-label="过滤当前目录"
                />
              </div>
              <div class="action-buttons">
                <button
                  class="util-btn"
                  type="button"
                  @click="navigateBack"
                  :disabled="currentPath === '/'"
                >
                  <span aria-hidden="true">↩️ </span>返回上级
                </button>
                <button class="util-btn brand-btn" type="button" @click="createNewFolder">
                  <span aria-hidden="true">➕ </span>新建文件夹
                </button>
              </div>
            </div>

            <!-- Loader / List / Empty State -->
            <div class="explorer-content">
              <div v-if="loading" class="loader-view">
                <div class="spinner" />
                <span>正在加载服务器目录...</span>
              </div>
              <div v-else-if="filteredFolders.length === 0" class="empty-view" role="status">
                <span class="empty-icon" aria-hidden="true">📂</span>
                <span class="empty-title">当前目录为空</span>
                <span class="empty-sub">没有在此处找到子文件夹。</span>
              </div>
              <div v-else class="folder-grid" role="list" aria-label="文件夹列表">
                <div
                  v-for="folder in filteredFolders"
                  :key="folder"
                  class="folder-card"
                  tabindex="0"
                  role="listitem"
                  :aria-label="`文件夹: ${folder}`"
                  @dblclick="handleFolderClick(folder)"
                  @click="
                    manualPathInput =
                      currentPath === '/' ? `/${folder}` : `${currentPath}/${folder}`
                  "
                  @keydown="handleFolderKeydown($event, folder)"
                >
                  <span class="folder-icon" aria-hidden="true">📁</span>
                  <span class="folder-name">{{ folder }}</span>
                </div>
              </div>
            </div>
          </section>
        </div>

        <!-- Footer Warnings & Confirm Buttons -->
        <footer class="modal-footer">
          <div class="warning-text">
            <span v-if="permissionWarning">{{ permissionWarning }}</span>
          </div>
          <div class="footer-actions">
            <button class="footer-btn secondary" type="button" @click="handleCancel">取消</button>
            <button class="footer-btn primary" type="button" @click="handleConfirm">
              确定选择
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
  width: 820px;
  max-width: 95vw;
  height: 580px;
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

    &:hover {
      background: var(--fbz-color-panel-elevated);
      color: var(--fbz-color-text);
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
  overflow: hidden;
}

.disk-sidebar {
  width: 200px;
  border-right: 1px solid var(--fbz-color-line-soft);
  background: var(--fbz-color-panel-strong);
  padding: var(--fbz-space-4);
  display: flex;
  flex-direction: column;
  gap: var(--fbz-space-3);

  h3 {
    margin: 0;
    font-size: var(--fbz-font-size-xs);
    text-transform: uppercase;
    color: var(--fbz-color-text-muted);
    letter-spacing: 1px;
    font-weight: 700;
  }

  .disk-list {
    display: flex;
    flex-direction: column;
    gap: 6px;
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

  .search-input {
    flex: 1;
    max-width: 280px;
    position: relative;

    .search-icon {
      position: absolute;
      left: 10px;
      top: 50%;
      transform: translateY(-50%);
      font-size: 13px;
      color: var(--fbz-color-text-muted);
    }

    input {
      width: 100%;
      height: 34px;
      background: var(--fbz-color-panel-strong);
      border: 1px solid var(--fbz-color-line);
      border-radius: var(--fbz-radius-control);
      padding: 0 10px 0 32px;
      color: var(--fbz-color-text);
      font-size: var(--fbz-font-size-sm);

      &:focus {
        outline: none;
        border-color: var(--fbz-color-brand-500);
      }
    }
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

      &:hover {
        background: color-mix(in srgb, var(--fbz-color-brand-500) 8%, transparent);
      }
    }
  }
}

.explorer-content {
  flex: 1;
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
  height: 100%;
  display: flex;
  flex-direction: column;
  justify-content: center;
  align-items: center;
  text-align: center;
  color: var(--fbz-color-text-muted);

  .empty-icon {
    font-size: 40px;
    margin-bottom: var(--fbz-space-3);
  }

  .empty-title {
    font-size: var(--fbz-font-size-md);
    font-weight: 700;
    color: var(--fbz-color-text-soft);
    margin-bottom: 4px;
  }

  .empty-sub {
    font-size: var(--fbz-font-size-sm);
  }
}

.folder-grid {
  display: grid;
  grid-template-columns: repeat(auto-fill, minmax(130px, 1fr));
  gap: var(--fbz-space-3);
}

.folder-card {
  border: 1px solid var(--fbz-color-line);
  background: var(--fbz-color-panel);
  border-radius: var(--fbz-radius-card);
  padding: var(--fbz-space-3) var(--fbz-space-2);
  display: flex;
  flex-direction: column;
  align-items: center;
  gap: 8px;
  cursor: pointer;
  user-select: none;
  transition: all var(--fbz-motion-fast);

  &:hover {
    border-color: var(--fbz-color-brand-500);
    background: var(--fbz-color-panel-strong);
    transform: translateY(-2px);
  }

  .folder-icon {
    font-size: 32px;
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

  .warning-text {
    font-size: var(--fbz-font-size-sm);
    font-weight: 600;
    color: var(--fbz-color-amber-500);
    max-width: 450px;
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

      &:hover {
        background: var(--fbz-color-brand-600);
      }
    }
  }
}

// Fade animations
.fade-enter-active,
.fade-leave-active {
  transition: opacity var(--fbz-motion-base) ease;
}
.fade-enter-from,
.fade-leave-to {
  opacity: 0;
}
</style>
