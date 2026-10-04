<script setup lang="ts">
import { useUiStore } from "@/stores/ui.ts";
import { useLibraryStore } from "@/stores/library.ts";
import {
  addLibraryPath,
  createLibrary,
  deleteLibrary,
  listLibraries,
  listLibraryPaths,
  queueLibraryMetadataRefresh,
  queueLibraryScan,
  removeLibraryPath,
  updateLibrarySettings,
} from "@/service/modules/admin.ts";
import {
  LIBRARY_TYPE_OPTIONS,
  type AdminLibraryType,
  type LibrarySettings,
} from "@/types/admin.ts";
import { useBodyScrollLock } from "@/composables/useBodyScrollLock.ts";

const uiStore = useUiStore();
const { libraryEditor } = storeToRefs(uiStore);
const libraryStore = useLibraryStore();
useBodyScrollLock(() => libraryEditor.value.open);

interface LibSettings {
  id: string;
  name: string;
  libraryType: AdminLibraryType;
  paths: string[];
  metadataLanguage: string;
  imageLanguage: string;
  preferOriginalPoster: boolean;
  isHidden: boolean;
}

const selectedLib = ref<LibSettings | null>(null);
const showDeleteConfirm = ref(false);
const isNewLibrary = ref(false);
const saving = ref(false);
const loadingDetails = ref(false);

const languageOptions = [
  { label: "继承系统全局语言", value: "system" },
  { label: "简体中文 (zh-CN)", value: "zh-CN" },
  { label: "英语 (en-US)", value: "en-US" },
  { label: "日语 (ja-JP)", value: "ja-JP" },
];

const imageLanguageOptions = [
  { label: "继承系统图片语言", value: "system" },
  { label: "中文海报优先", value: "zh" },
  { label: "英文海报优先", value: "en" },
  { label: "无字图优先", value: "none" },
];

function toNullable(value: string): string | null {
  return value === "system" || value.trim() === "" ? null : value;
}

function fromNullable(value: string | null | undefined): string {
  return value ?? "system";
}

function settingsToForm(lib: LibrarySettings, paths: string[]): LibSettings {
  return {
    id: lib.id,
    name: lib.name,
    libraryType: lib.libraryType as AdminLibraryType,
    paths: paths.length ? paths : [""],
    metadataLanguage: fromNullable(lib.preferredMetadataLanguage),
    imageLanguage: fromNullable(lib.preferredImageLanguage),
    preferOriginalPoster: lib.preferredImagePreferOriginal ?? true,
    isHidden: lib.isHidden,
  };
}

async function refreshLibraryStore() {
  const page = await listLibraries({ limit: 500 });
  libraryStore.replaceFromSettings(page.items);
  return page.items;
}

watch(
  () => libraryEditor.value.open,
  async (open) => {
    if (open) {
      showDeleteConfirm.value = false;
      const libId = libraryEditor.value.libraryId;
      if (libId) {
        isNewLibrary.value = false;
        loadingDetails.value = true;
        selectedLib.value = null;
        try {
          const [librariesPage, paths] = await Promise.all([
            listLibraries({ limit: 500 }),
            listLibraryPaths(libId),
          ]);
          const lib = librariesPage.items.find((item) => item.id === libId);
          if (!lib) {
            uiStore.showToast("媒体库不存在或已被删除。", "error");
            uiStore.closeLibraryEditor();
            return;
          }
          libraryStore.replaceFromSettings(librariesPage.items);
          selectedLib.value = settingsToForm(
            lib,
            paths.filter((path) => path.isEnabled).map((path) => path.path),
          );
        } catch {
          uiStore.showToast("加载媒体库配置失败，请检查网络与权限。", "error");
          uiStore.closeLibraryEditor();
        } finally {
          loadingDetails.value = false;
        }
      } else {
        isNewLibrary.value = true;
        selectedLib.value = {
          id: "",
          name: "",
          libraryType: "movies",
          paths: [""],
          metadataLanguage: "system",
          imageLanguage: "system",
          preferOriginalPoster: true,
          isHidden: false,
        };
      }
    } else {
      selectedLib.value = null;
      saving.value = false;
      loadingDetails.value = false;
    }
  },
);

async function handleBrowsePath(index: number) {
  if (!selectedLib.value) return;
  try {
    const chosenPath = await uiStore.openFilePicker();
    if (chosenPath) {
      selectedLib.value.paths[index] = chosenPath;
    }
  } catch (err) {
    console.error("Browse failed", err);
  }
}

function addPathField() {
  if (!selectedLib.value) return;
  selectedLib.value.paths.push("");
}

function removePathField(index: number) {
  if (!selectedLib.value) return;
  if (selectedLib.value.paths.length > 1) {
    selectedLib.value.paths.splice(index, 1);
  }
}

async function handleSaveLibrary() {
  if (!selectedLib.value) return;
  if (!selectedLib.value.name.trim()) {
    uiStore.showToast("请输入媒体库标题！", "warning");
    return;
  }
  const paths = selectedLib.value.paths.map((path) => path.trim()).filter(Boolean);
  if (!paths.length) {
    uiStore.showToast("请至少填写一个服务器物理路径。", "warning");
    return;
  }

  saving.value = true;
  try {
    if (isNewLibrary.value) {
      const created = await createLibrary({
        name: selectedLib.value.name.trim(),
        libraryType: selectedLib.value.libraryType,
        paths,
        preferredMetadataLanguage: toNullable(selectedLib.value.metadataLanguage),
        preferredImageLanguage: toNullable(selectedLib.value.imageLanguage),
        preferredImagePreferOriginal: selectedLib.value.preferOriginalPoster,
      });
      await queueLibraryScan(created.id, "library-created");
      await refreshLibraryStore();
      uiStore.showToast("媒体库创建成功，已加入扫描队列。", "success");
    } else {
      await updateLibrarySettings(selectedLib.value.id, {
        isHidden: selectedLib.value.isHidden,
        preferredMetadataLanguage: toNullable(selectedLib.value.metadataLanguage),
        preferredImageLanguage: toNullable(selectedLib.value.imageLanguage),
        preferredImagePreferOriginal: selectedLib.value.preferOriginalPoster,
        preferredImageFallbackLanguages: [],
      });

      const existingPaths = await listLibraryPaths(selectedLib.value.id);
      const existingPathSet = new Set(existingPaths.map((path) => path.path));
      const nextPathSet = new Set(paths);
      await Promise.all(
        existingPaths
          .filter((path) => !nextPathSet.has(path.path))
          .map((path) => removeLibraryPath(selectedLib.value!.id, path.id)),
      );
      await Promise.all(
        paths
          .filter((path) => !existingPathSet.has(path))
          .map((path) => addLibraryPath(selectedLib.value!.id, path)),
      );
      await queueLibraryScan(selectedLib.value.id, "library-settings-updated");
      await queueLibraryMetadataRefresh(selectedLib.value.id, {
        reason: "library-settings-updated",
      });
      await refreshLibraryStore();
      uiStore.showToast("媒体库配置已保存，扫描与元数据刷新已入队。", "success");
    }
    uiStore.closeLibraryEditor();
  } catch {
    uiStore.showToast("保存媒体库配置失败，请检查输入、权限和服务器状态。", "error");
  } finally {
    saving.value = false;
  }
}

async function handleDeleteLibrary() {
  if (!selectedLib.value) return;
  saving.value = true;
  try {
    await deleteLibrary(selectedLib.value.id);
    await refreshLibraryStore();
    uiStore.showToast(
      `媒体库【${selectedLib.value.name}】已从系统卸载。物理文件未被删除。`,
      "success",
    );
    uiStore.closeLibraryEditor();
    showDeleteConfirm.value = false;
  } catch {
    uiStore.showToast("删除媒体库失败，请检查权限和服务器状态。", "error");
  } finally {
    saving.value = false;
  }
}

useEventListener(window, "keydown", (e) => {
  if (e.key === "Escape" && libraryEditor.value.open && !uiStore.filePicker.open) {
    uiStore.closeLibraryEditor();
  }
});
</script>

<template>
  <Transition name="modal-fade">
    <div
      v-if="libraryEditor.open && selectedLib"
      class="editor-modal-overlay"
      @click="uiStore.closeLibraryEditor"
    >
      <div
        class="editor-modal-container"
        role="dialog"
        aria-modal="true"
        aria-labelledby="modal-title"
        @click.stop
      >
        <div v-if="loadingDetails" class="modal-loading">正在加载媒体库配置...</div>
        <header class="modal-title-bar">
          <div class="title-left">
            <svg
              viewBox="0 0 24 24"
              width="18"
              height="18"
              fill="none"
              stroke="currentColor"
              stroke-width="2"
              stroke-linecap="round"
              stroke-linejoin="round"
              class="header-icon"
              aria-hidden="true"
            >
              <path
                d="M22 19a2 2 0 0 1-2 2H4a2 2 0 0 1-2-2V5a2 2 0 0 1 2-2h5l2 3h9a2 2 0 0 1 2 2z"
              />
            </svg>
            <h2 id="modal-title">{{ isNewLibrary ? "新建媒体库" : "配置媒体库参数" }}</h2>
          </div>
          <button
            class="modal-close-btn"
            type="button"
            aria-label="关闭"
            @click="uiStore.closeLibraryEditor"
          >
            <svg
              viewBox="0 0 24 24"
              width="16"
              height="16"
              fill="none"
              stroke="currentColor"
              stroke-width="2.5"
              stroke-linecap="round"
              stroke-linejoin="round"
              aria-hidden="true"
            >
              <line x1="18" y1="6" x2="6" y2="18" />
              <line x1="6" y1="6" x2="18" y2="18" />
            </svg>
          </button>
        </header>

        <div class="modal-scroll-body">
          <div class="settings-cards-stack">
            <!-- Section 1: Basic Parameters -->
            <section class="settings-card" aria-labelledby="section-basic-title">
              <div class="card-header">
                <span class="indicator" aria-hidden="true" />
                <h3 id="section-basic-title">基础挂载参数</h3>
              </div>

              <div class="card-body">
                <div class="form-group">
                  <label for="lib-title-input">媒体库标题</label>
                  <input
                    id="lib-title-input"
                    v-model="selectedLib.name"
                    type="text"
                    placeholder="例如：电影库、4K 原盘"
                    class="control-input"
                  />
                </div>

                <div class="form-group">
                  <label id="lib-kind-label">媒体类型</label>
                  <BaseSelect
                    v-model="selectedLib.libraryType"
                    ariaLabel="选择媒体类型"
                    :options="LIBRARY_TYPE_OPTIONS"
                    class="w-full"
                  />
                </div>

                <!-- Path List Builder -->
                <div class="form-group">
                  <label>物理存储目录 (支持挂载多个物理磁盘目录)</label>
                  <div class="paths-stack">
                    <div v-for="(path, idx) in selectedLib.paths" :key="idx" class="path-row-item">
                      <div class="input-glow-wrapper">
                        <svg
                          viewBox="0 0 24 24"
                          width="14"
                          height="14"
                          fill="none"
                          stroke="currentColor"
                          stroke-width="2"
                          stroke-linecap="round"
                          stroke-linejoin="round"
                          class="path-svg"
                          aria-hidden="true"
                        >
                          <path
                            d="M22 19a2 2 0 0 1-2 2H4a2 2 0 0 1-2-2V5a2 2 0 0 1 2-2h5l2 3h9a2 2 0 0 1 2 2z"
                          />
                        </svg>
                        <input
                          v-model="selectedLib.paths[idx]"
                          type="text"
                          placeholder="绝对路径，如：/media/nas/电影"
                          class="control-input flex-1"
                          :aria-label="`物理存储目录路径 ${idx + 1}`"
                        />
                      </div>
                      <button
                        class="browse-action-btn"
                        type="button"
                        @click="handleBrowsePath(idx)"
                        :aria-label="`浏览服务器路径进行选择 ${idx + 1}`"
                      >
                        浏览
                      </button>
                      <button
                        class="remove-action-btn"
                        type="button"
                        @click="removePathField(idx)"
                        :disabled="selectedLib.paths.length <= 1"
                        title="删除路径"
                        :aria-label="`删除物理路径 ${idx + 1}`"
                      >
                        <svg
                          viewBox="0 0 24 24"
                          width="14"
                          height="14"
                          fill="none"
                          stroke="currentColor"
                          stroke-width="2"
                          stroke-linecap="round"
                          stroke-linejoin="round"
                          aria-hidden="true"
                        >
                          <line x1="18" y1="6" x2="6" y2="18" />
                          <line x1="6" y1="6" x2="18" y2="18" />
                        </svg>
                      </button>
                    </div>
                  </div>
                  <button class="add-path-action-btn" type="button" @click="addPathField">
                    <svg
                      viewBox="0 0 24 24"
                      width="12"
                      height="12"
                      fill="none"
                      stroke="currentColor"
                      stroke-width="2.5"
                      stroke-linecap="round"
                      stroke-linejoin="round"
                      aria-hidden="true"
                    >
                      <line x1="12" y1="5" x2="12" y2="19" />
                      <line x1="5" y1="12" x2="19" y2="12" />
                    </svg>
                    添加额外媒体目录
                  </button>
                </div>
              </div>
            </section>

            <!-- Section 2: Metadata options -->
            <section class="settings-card" aria-labelledby="section-scraper-title">
              <div class="card-header">
                <span class="indicator" aria-hidden="true" />
                <h3 id="section-scraper-title">元数据与搜刮设置</h3>
              </div>

              <div class="card-body">
                <div class="form-group">
                  <label id="lib-lang-label">搜刮元数据语言偏好</label>
                  <BaseSelect
                    v-model="selectedLib.metadataLanguage"
                    ariaLabel="选择搜刮元数据语言偏好"
                    :options="languageOptions"
                    class="w-full"
                  />
                </div>

                <div class="form-group">
                  <label id="lib-image-lang-label">图片语言偏好</label>
                  <BaseSelect
                    v-model="selectedLib.imageLanguage"
                    ariaLabel="选择图片语言偏好"
                    :options="imageLanguageOptions"
                    class="w-full"
                  />
                </div>

                <div class="toggles-list" role="group" aria-label="高级图片选项">
                  <div class="toggle-row-item">
                    <div class="toggle-text">
                      <span class="title">优先原语言海报</span>
                      <span class="desc"
                        >即使设定语言为中文，海报封面抓取也优先选用原产国语言版本。</span
                      >
                    </div>
                    <label class="glow-switch">
                      <input
                        v-model="selectedLib.preferOriginalPoster"
                        type="checkbox"
                        aria-label="优先原语言海报"
                      />
                      <span class="switch-slide-thumb" />
                    </label>
                  </div>

                  <div class="toggle-row-item">
                    <div class="toggle-text">
                      <span class="title">图片本地物理缓存</span>
                      <span class="desc">后端元数据刷新会自动缓存海报和剧照到本地服务器。</span>
                    </div>
                    <label class="glow-switch">
                      <input checked disabled type="checkbox" aria-label="图片本地物理缓存" />
                      <span class="switch-slide-thumb" />
                    </label>
                  </div>
                </div>
              </div>
            </section>

            <!-- Section 3: Sync and Performance -->
            <section class="settings-card" aria-labelledby="section-sync-title">
              <div class="card-header">
                <span class="indicator" aria-hidden="true" />
                <h3 id="section-sync-title">监控与同步选项</h3>
              </div>

              <div class="card-body">
                <div class="toggles-list" role="group" aria-label="系统后台同步选项">
                  <div class="toggle-row-item">
                    <div class="toggle-text">
                      <span class="title">预加载媒体信息 (Preload)</span>
                      <span class="desc"
                        >利用后台任务静默提取视频编码、音轨声道布局、分辨率规格及字幕流。</span
                      >
                    </div>
                    <label class="glow-switch">
                      <input disabled type="checkbox" aria-label="预加载媒体信息" />
                      <span class="switch-slide-thumb" />
                    </label>
                  </div>

                  <div class="toggle-row-item">
                    <div class="toggle-text">
                      <span class="title">开启实时文件系统监控 (Realtime Monitor)</span>
                      <span class="desc"
                        >使用 File Watcher
                        监听，物理磁盘内一经新增或修改视频文件立即执行搜刮。</span
                      >
                    </div>
                    <label class="glow-switch">
                      <input disabled type="checkbox" aria-label="开启实时文件系统监控" />
                      <span class="switch-slide-thumb" />
                    </label>
                  </div>

                  <div class="toggle-row-item">
                    <div class="toggle-text">
                      <span class="title">消息推送通知</span>
                      <span class="desc"
                        >新视频搜刮入库成功后，向控制中心和配置推送终端发送即时消息。</span
                      >
                    </div>
                    <label class="glow-switch">
                      <input disabled type="checkbox" aria-label="消息推送通知" />
                      <span class="switch-slide-thumb" />
                    </label>
                  </div>
                </div>
              </div>
            </section>
          </div>
        </div>

        <!-- Bottom Actions Inside Modal -->
        <footer class="editor-actions-footer">
          <button
            v-if="!isNewLibrary"
            class="action-btn-danger"
            type="button"
            @click="showDeleteConfirm = true"
          >
            <svg
              viewBox="0 0 24 24"
              width="14"
              height="14"
              fill="none"
              stroke="currentColor"
              stroke-width="2"
              stroke-linecap="round"
              stroke-linejoin="round"
              aria-hidden="true"
            >
              <polyline points="3 6 5 6 21 6" />
              <path
                d="M19 6v14a2 2 0 0 1-2 2H7a2 2 0 0 1-2-2V6m3 0V4a2 2 0 0 1 2-2h4a2 2 0 0 1 2 2v2"
              />
              <line x1="10" y1="11" x2="10" y2="17" />
              <line x1="14" y1="11" x2="14" y2="17" />
            </svg>
            <span>删除媒体库</span>
          </button>
          <div class="spacer" />
          <button class="action-btn-secondary" type="button" @click="uiStore.closeLibraryEditor">
            取消
          </button>
          <button
            class="action-btn-primary"
            type="button"
            :disabled="saving"
            @click="handleSaveLibrary"
          >
            {{ saving ? "保存中..." : "保存修改" }}
          </button>
        </footer>

        <!-- Deletion Double Confirm In Modal -->
        <Transition name="fade">
          <div v-if="showDeleteConfirm" class="confirm-glass-overlay">
            <div
              class="confirm-panel-card"
              role="dialog"
              aria-modal="true"
              aria-labelledby="confirm-title"
            >
              <div class="warn-icon" aria-hidden="true">
                <svg
                  viewBox="0 0 24 24"
                  width="24"
                  height="24"
                  fill="none"
                  stroke="currentColor"
                  stroke-width="2"
                  stroke-linecap="round"
                  stroke-linejoin="round"
                >
                  <path
                    d="M10.29 3.86L1.82 18a2 2 0 0 0 1.71 3h16.94a2 2 0 0 0 1.71-3L13.71 3.86a2 2 0 0 0-3.42 0z"
                  />
                  <line x1="12" y1="9" x2="12" y2="13" />
                  <line x1="12" y1="17" x2="12.01" y2="17" />
                </svg>
              </div>
              <h3>确认卸载媒体库【{{ selectedLib.name }}】吗？</h3>
              <p>
                卸载只会删除 Fbz 元数据映射及数据库索引，<b>绝不会</b>删除您磁盘上的视频源文件。
              </p>
              <div class="actions-row">
                <button class="confirm-btn cancel" type="button" @click="showDeleteConfirm = false">
                  取消
                </button>
                <button class="confirm-btn confirm" type="button" @click="handleDeleteLibrary">
                  确认删除
                </button>
              </div>
            </div>
          </div>
        </Transition>
      </div>
    </div>
  </Transition>
</template>

<style scoped lang="scss">
.editor-modal-overlay {
  position: fixed;
  inset: 0;
  z-index: 130;
  background: rgba(0, 0, 0, 0.7);
  backdrop-filter: blur(8px);
  display: grid;
  place-content: center;
}

.editor-modal-container {
  width: 720px;
  max-width: 95vw;
  height: 80vh;
  max-height: 700px;
  background: var(--fbz-color-panel);
  border: 1px solid var(--fbz-color-line-bright);
  border-radius: 8px;
  box-shadow: var(--fbz-shadow-panel);
  display: flex;
  flex-direction: column;
  overflow: hidden;
  color: var(--fbz-color-text);
  font-family: var(--fbz-font-sans);
}

.modal-loading {
  padding: var(--fbz-space-4) var(--fbz-space-5);
  border-bottom: 1px solid var(--fbz-color-line-soft);
  background: var(--fbz-color-panel-strong);
  color: var(--fbz-color-text-muted);
  font-size: var(--fbz-font-size-sm);
}

.modal-title-bar {
  padding: var(--fbz-space-4) var(--fbz-space-5);
  border-bottom: 1px solid var(--fbz-color-line-soft);
  display: flex;
  justify-content: space-between;
  align-items: center;
  background: var(--fbz-color-bg-strong);

  .title-left {
    display: flex;
    align-items: center;
    gap: 10px;

    .header-icon {
      color: var(--fbz-color-brand-500);
    }

    h2 {
      margin: 0;
      font-size: 15px;
      font-weight: 700;
    }
  }

  .modal-close-btn {
    background: none;
    border: 0;
    color: var(--fbz-color-text-muted);
    cursor: pointer;
    padding: 4px;
    display: grid;
    place-content: center;
    border-radius: var(--fbz-radius-control);
    transition: all var(--fbz-motion-fast);

    &:hover {
      color: var(--fbz-color-text);
      background: var(--fbz-color-panel-strong);
    }
  }
}

.modal-scroll-body {
  flex: 1;
  overflow-y: auto;
  padding: var(--fbz-space-5);
  background: var(--fbz-color-bg);
}

.settings-cards-stack {
  display: flex;
  flex-direction: column;
  gap: var(--fbz-space-4);
}

.settings-card {
  background: var(--fbz-color-panel-strong);
  border: 1px solid var(--fbz-color-line-soft);
  border-radius: 6px;
  overflow: hidden;

  .card-header {
    padding: var(--fbz-space-3) var(--fbz-space-5);
    border-bottom: 1px solid var(--fbz-color-line-soft);
    display: flex;
    align-items: center;
    gap: 10px;

    .indicator {
      width: 3px;
      height: 12px;
      background: var(--fbz-color-brand-500);
      border-radius: 2px;
    }

    h3 {
      margin: 0;
      font-size: 12px;
      font-weight: 700;
      text-transform: uppercase;
      letter-spacing: 0.5px;
      color: var(--fbz-color-text-soft);
    }
  }

  .card-body {
    padding: var(--fbz-space-5);
    display: flex;
    flex-direction: column;
    gap: var(--fbz-space-4);
  }
}

.form-group {
  display: flex;
  flex-direction: column;
  gap: 8px;

  label {
    font-size: var(--fbz-font-size-xs);
    font-weight: 700;
    color: var(--fbz-color-text-soft);
  }

  .control-input {
    height: 38px;
    background: var(--fbz-color-panel);
    border: 1px solid var(--fbz-color-line);
    border-radius: var(--fbz-radius-control);
    padding: 0 var(--fbz-space-3);
    color: var(--fbz-color-text);
    font-size: var(--fbz-font-size-sm);
    transition: all var(--fbz-motion-fast);

    &:focus {
      outline: none;
      border-color: var(--fbz-color-brand-500);
      box-shadow: var(--fbz-shadow-focus);
    }
  }

  .hint {
    font-size: var(--fbz-font-size-xs);
    color: var(--fbz-color-text-muted);
    line-height: 1.4;
  }
}

.scrapers-grid {
  display: grid;
  grid-template-columns: repeat(3, 1fr);
  gap: var(--fbz-space-3);
}

.scraper-item-card {
  border: 1px solid var(--fbz-color-line);
  background: var(--fbz-color-panel);
  border-radius: var(--fbz-radius-control);
  padding: var(--fbz-space-4) var(--fbz-space-3);
  cursor: pointer;
  display: flex;
  flex-direction: column;
  align-items: center;
  gap: 8px;
  text-align: center;
  transition: all var(--fbz-motion-base);
  position: relative;

  input {
    position: absolute;
    top: 10px;
    right: 10px;
    accent-color: var(--fbz-color-brand-500);
  }

  .scraper-logo {
    font-family: var(--fbz-font-display);
    font-size: 12px;
    font-weight: 800;
    padding: 2px 8px;
    border-radius: 4px;

    &.tmdb-logo {
      background: #0d253f;
      color: #01b4e4;
    }

    &.imdb-logo {
      background: #f5c518;
      color: #000000;
    }

    &.nfo-logo {
      background: var(--fbz-color-line-bright);
      color: var(--fbz-color-text);
    }
  }

  .scraper-name {
    font-size: 10px;
    font-weight: 600;
    color: var(--fbz-color-text-muted);
  }

  &:hover {
    border-color: var(--fbz-color-line-bright);
    background: var(--fbz-color-panel-elevated);
  }

  &.active {
    border-color: var(--fbz-color-brand-500);
    background: color-mix(in srgb, var(--fbz-color-brand-500) 5%, var(--fbz-color-panel));
  }
}

.paths-stack {
  display: flex;
  flex-direction: column;
  gap: var(--fbz-space-2);
}

.path-row-item {
  display: flex;
  gap: var(--fbz-space-2);
  align-items: center;

  .input-glow-wrapper {
    flex: 1;
    display: flex;
    align-items: center;
    background: var(--fbz-color-panel);
    border: 1px solid var(--fbz-color-line);
    border-radius: var(--fbz-radius-control);
    padding: 0 10px;
    height: 38px;
    transition: all var(--fbz-motion-fast);

    &:focus-within {
      border-color: var(--fbz-color-brand-500);
      box-shadow: var(--fbz-shadow-focus);
    }

    .path-svg {
      color: var(--fbz-color-text-muted);
      margin-right: 8px;
      flex-shrink: 0;
    }

    input {
      background: transparent;
      border: 0;
      height: 100%;
      width: 100%;
      color: var(--fbz-color-text);
      font-size: var(--fbz-font-size-sm);

      &:focus {
        outline: none;
      }
    }
  }

  .browse-action-btn {
    height: 38px;
    padding: 0 var(--fbz-space-4);
    background: var(--fbz-color-panel);
    border: 1px solid var(--fbz-color-line);
    border-radius: var(--fbz-radius-control);
    color: var(--fbz-color-text-soft);
    font-weight: 700;
    font-size: var(--fbz-font-size-sm);
    cursor: pointer;
    transition: all var(--fbz-motion-fast);

    &:hover {
      background: var(--fbz-color-panel-elevated);
      color: var(--fbz-color-text);
    }
  }

  .remove-action-btn {
    width: 38px;
    height: 38px;
    background: transparent;
    border: 1px solid var(--fbz-color-line);
    border-radius: var(--fbz-radius-control);
    color: var(--fbz-color-danger-500);
    cursor: pointer;
    display: grid;
    place-content: center;
    transition: all var(--fbz-motion-fast);

    &:hover:not(:disabled) {
      background: color-mix(in srgb, var(--fbz-color-danger-500) 8%, transparent);
      border-color: var(--fbz-color-danger-500);
    }

    &:disabled {
      opacity: 0.3;
      cursor: not-allowed;
    }
  }
}

.add-path-action-btn {
  align-self: flex-start;
  background: transparent;
  border: 1px dashed var(--fbz-color-line-bright);
  border-radius: var(--fbz-radius-control);
  color: var(--fbz-color-text-soft);
  height: 34px;
  padding: 0 var(--fbz-space-4);
  font-size: var(--fbz-font-size-xs);
  font-weight: 700;
  cursor: pointer;
  display: flex;
  align-items: center;
  gap: 6px;
  transition: all var(--fbz-motion-fast);

  &:hover {
    border-color: var(--fbz-color-brand-500);
    color: var(--fbz-color-brand-500);
    background: color-mix(in srgb, var(--fbz-color-brand-500) 3%, transparent);
  }
}

.toggles-list {
  display: flex;
  flex-direction: column;
  gap: var(--fbz-space-4);
}

.toggle-row-item {
  display: flex;
  justify-content: space-between;
  align-items: center;
  gap: var(--fbz-space-6);

  .toggle-text {
    display: flex;
    flex-direction: column;
    gap: 3px;

    .title {
      font-size: var(--fbz-font-size-sm);
      font-weight: 700;
      color: var(--fbz-color-text);
    }

    .desc {
      font-size: var(--fbz-font-size-xs);
      color: var(--fbz-color-text-muted);
      line-height: 1.4;
    }
  }
}

.glow-switch {
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
}

.editor-actions-footer {
  padding: var(--fbz-space-3) var(--fbz-space-5);
  border-top: 1px solid var(--fbz-color-line-soft);
  display: flex;
  align-items: center;
  background: var(--fbz-color-bg-strong);

  .spacer {
    flex: 1;
  }

  .action-btn-primary {
    height: 36px;
    padding: 0 var(--fbz-space-5);
    background: var(--fbz-color-brand-500);
    border: 0;
    color: #07120a;
    font-weight: 700;
    font-size: var(--fbz-font-size-sm);
    border-radius: var(--fbz-radius-control);
    cursor: pointer;
    transition: all var(--fbz-motion-fast);

    &:hover {
      background: var(--fbz-color-brand-600);
      box-shadow: 0 0 12px color-mix(in srgb, var(--fbz-color-brand-500) 30%, transparent);
    }
  }

  .action-btn-secondary {
    height: 36px;
    padding: 0 var(--fbz-space-5);
    background: var(--fbz-color-panel-strong);
    border: 1px solid var(--fbz-color-line);
    color: var(--fbz-color-text-soft);
    font-weight: 700;
    font-size: var(--fbz-font-size-sm);
    border-radius: var(--fbz-radius-control);
    cursor: pointer;
    transition: all var(--fbz-motion-fast);
    margin-right: var(--fbz-space-2);

    &:hover {
      background: var(--fbz-color-panel-elevated);
      color: var(--fbz-color-text);
    }
  }

  .action-btn-danger {
    height: 36px;
    padding: 0 var(--fbz-space-4);
    background: transparent;
    border: 1px solid var(--fbz-color-danger-500);
    color: var(--fbz-color-danger-500);
    font-weight: 700;
    font-size: var(--fbz-font-size-sm);
    border-radius: var(--fbz-radius-control);
    cursor: pointer;
    display: flex;
    align-items: center;
    gap: 6px;
    transition: all var(--fbz-motion-fast);

    &:hover {
      background: color-mix(in srgb, var(--fbz-color-danger-500) 8%, transparent);
    }
  }
}

.confirm-glass-overlay {
  position: absolute;
  inset: 0;
  background: rgba(10, 10, 11, 0.88);
  backdrop-filter: blur(12px);
  display: grid;
  place-content: center;
  z-index: 140;
  padding: var(--fbz-space-5);
}

.confirm-panel-card {
  width: 400px;
  max-width: 90vw;
  background: var(--fbz-color-panel-elevated);
  border: 1px solid var(--fbz-color-line-bright);
  border-radius: 8px;
  padding: var(--fbz-space-5);
  text-align: center;
  box-shadow: var(--fbz-shadow-panel);

  .warn-icon {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    width: 48px;
    height: 48px;
    border-radius: 50%;
    background: color-mix(in srgb, var(--fbz-color-danger-500) 10%, transparent);
    color: var(--fbz-color-danger-500);
    margin-bottom: var(--fbz-space-3);
  }

  h3 {
    margin: 0 0 10px;
    font-size: 15px;
    font-weight: 700;
  }

  p {
    margin: 0 0 var(--fbz-space-4);
    font-size: var(--fbz-font-size-sm);
    color: var(--fbz-color-text-soft);
    line-height: 1.6;
    b {
      color: var(--fbz-color-text);
    }
  }

  .actions-row {
    display: flex;
    gap: var(--fbz-space-3);
  }

  .confirm-btn {
    flex: 1;
    height: 36px;
    border-radius: var(--fbz-radius-control);
    font-size: var(--fbz-font-size-sm);
    font-weight: 700;
    cursor: pointer;
    transition: all var(--fbz-motion-fast);

    &.cancel {
      background: var(--fbz-color-panel-strong);
      border: 1px solid var(--fbz-color-line);
      color: var(--fbz-color-text-soft);

      &:hover {
        background: var(--fbz-color-panel);
      }
    }

    &.confirm {
      background: var(--fbz-color-danger-500);
      border: 0;
      color: #ffffff;

      &:hover {
        background: color-mix(in srgb, var(--fbz-color-danger-500) 84%, black);
      }
    }
  }
}

.modal-fade-enter-active,
.modal-fade-leave-active {
  transition: opacity var(--fbz-motion-base) ease;

  .editor-modal-container {
    transition: transform var(--fbz-motion-base) cubic-bezier(0.16, 1, 0.3, 1);
  }
}
.modal-fade-enter-from,
.modal-fade-leave-to {
  opacity: 0;

  .editor-modal-container {
    transform: scale(0.96) translateY(8px);
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
</style>
