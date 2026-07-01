import { defineStore } from "pinia";
import type { MediaItem } from "@/types/media.ts";
import { getSetupStatus } from "@/service/modules/setup.ts";

export interface ContextMenuState {
  open: boolean;
  x: number;
  y: number;
  item: MediaItem | null;
}

/** 媒体库右键菜单目标：只需 id/name/type 即可驱动扫描、刷新、编辑、删除。 */
export interface LibraryContextTarget {
  id: string;
  name: string;
}

export interface LibraryContextMenuState {
  open: boolean;
  x: number;
  y: number;
  library: LibraryContextTarget | null;
}

export interface MetadataManagerState {
  open: boolean;
  item: MediaItem | null;
}

export interface FilePickerState {
  open: boolean;
  currentPath: string;
  resolve: ((path: string) => void) | null;
}

export interface LibraryEditorState {
  open: boolean;
  libraryId: string | null; // null for new library, string for editing
}

export interface ToastMessage {
  id: string;
  message: string;
  type: "success" | "info" | "warning" | "error";
  duration: number;
}

export const useUiStore = defineStore("ui", () => {
  // 是否已初始化以**后端**为准（`GET /api/setup/status`），不再读 localStorage。
  // null = 尚未拉取；拉取前向导默认不弹，避免空窗期闪现。
  const isInitialized = ref<boolean | null>(null);
  const setupWizardOpen = ref(false);

  // Context Menu state
  const contextMenu = ref<ContextMenuState>({
    open: false,
    x: 0,
    y: 0,
    item: null,
  });

  // 媒体库右键菜单 state（与媒体条目菜单区分：动作集不同）。
  const libraryContextMenu = ref<LibraryContextMenuState>({
    open: false,
    x: 0,
    y: 0,
    library: null,
  });

  // Metadata manager modal state
  const metadataManager = ref<MetadataManagerState>({
    open: false,
    item: null,
  });

  // File Picker modal state（初始路径留空，由文件浏览接口/调用方决定）。
  const filePicker = ref<FilePickerState>({
    open: false,
    currentPath: "",
    resolve: null,
  });

  // Library Editor modal state
  const libraryEditor = ref<LibraryEditorState>({
    open: false,
    libraryId: null,
  });

  // Global Toast Notifications
  const toasts = ref<ToastMessage[]>([]);

  /**
   * 向后端拉取初始化状态，驱动是否弹出 setup 向导。
   * 应用启动时调用一次（见 `App.vue`）。请求失败时不弹向导（视为已初始化），
   * 避免后端短暂不可达把已部署系统重新拖进向导。
   */
  async function refreshSetupStatus(): Promise<void> {
    try {
      const status = await getSetupStatus();
      isInitialized.value = status.initialized;
      setupWizardOpen.value = !status.initialized;
    } catch {
      isInitialized.value = true;
      setupWizardOpen.value = false;
    }
  }

  function completeInitialization() {
    isInitialized.value = true;
    setupWizardOpen.value = false;
  }

  function resetInitialization() {
    isInitialized.value = false;
    setupWizardOpen.value = true;
  }

  function openContextMenu(x: number, y: number, item: MediaItem) {
    contextMenu.value = {
      open: true,
      x,
      y,
      item,
    };
  }

  function closeContextMenu() {
    contextMenu.value.open = false;
  }

  function openLibraryContextMenu(x: number, y: number, library: LibraryContextTarget) {
    libraryContextMenu.value = {
      open: true,
      x,
      y,
      library,
    };
  }

  function closeLibraryContextMenu() {
    libraryContextMenu.value.open = false;
  }

  function openMetadataManager(item: MediaItem) {
    metadataManager.value = {
      open: true,
      item,
    };
    closeContextMenu();
  }

  function closeMetadataManager() {
    metadataManager.value.open = false;
    metadataManager.value.item = null;
  }

  function openFilePicker(): Promise<string> {
    return new Promise((resolve) => {
      filePicker.value = {
        open: true,
        currentPath: filePicker.value.currentPath,
        resolve,
      };
    });
  }

  function closeFilePicker(selectedPath?: string) {
    if (selectedPath && filePicker.value.resolve) {
      filePicker.value.resolve(selectedPath);
    }
    filePicker.value.open = false;
    filePicker.value.resolve = null;
  }

  function openLibraryEditor(libraryId: string | null = null) {
    libraryEditor.value = {
      open: true,
      libraryId,
    };
  }

  function closeLibraryEditor() {
    libraryEditor.value.open = false;
    libraryEditor.value.libraryId = null;
  }

  function showToast(
    message: string,
    type: "success" | "info" | "warning" | "error" = "info",
    duration = 3000,
  ) {
    const id = `toast-${Date.now()}-${Math.random().toString(36).substring(2, 9)}`;
    toasts.value.push({
      id,
      message,
      type,
      duration,
    });
    setTimeout(() => {
      removeToast(id);
    }, duration);
  }

  function removeToast(id: string) {
    const idx = toasts.value.findIndex((t) => t.id === id);
    if (idx > -1) {
      toasts.value.splice(idx, 1);
    }
  }

  return {
    isInitialized,
    setupWizardOpen,
    contextMenu,
    libraryContextMenu,
    metadataManager,
    filePicker,
    libraryEditor,
    toasts,
    refreshSetupStatus,
    completeInitialization,
    resetInitialization,
    openContextMenu,
    closeContextMenu,
    openLibraryContextMenu,
    closeLibraryContextMenu,
    openMetadataManager,
    closeMetadataManager,
    openFilePicker,
    closeFilePicker,
    openLibraryEditor,
    closeLibraryEditor,
    showToast,
    removeToast,
  };
});
