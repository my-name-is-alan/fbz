import { defineStore } from "pinia";
import type { MediaItem } from "@/types/media.ts";

export interface ContextMenuState {
  open: boolean;
  x: number;
  y: number;
  item: MediaItem | null;
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
  const isInitialized = ref(localStorage.getItem("fbz_initialized") === "true");
  const setupWizardOpen = ref(!isInitialized.value);
  const guidedTourActive = ref(false);

  // Context Menu state
  const contextMenu = ref<ContextMenuState>({
    open: false,
    x: 0,
    y: 0,
    item: null,
  });

  // Metadata manager modal state
  const metadataManager = ref<MetadataManagerState>({
    open: false,
    item: null,
  });

  // File Picker modal state
  const filePicker = ref<FilePickerState>({
    open: false,
    currentPath: "/media/nas/电影",
    resolve: null,
  });

  // Library Editor modal state
  const libraryEditor = ref<LibraryEditorState>({
    open: false,
    libraryId: null,
  });

  // Global Toast Notifications
  const toasts = ref<ToastMessage[]>([]);

  function completeInitialization() {
    isInitialized.value = true;
    localStorage.setItem("fbz_initialized", "true");
    setupWizardOpen.value = false;
    // Trigger guided tour after setup
    guidedTourActive.value = true;
  }

  function resetInitialization() {
    isInitialized.value = false;
    localStorage.removeItem("fbz_initialized");
    setupWizardOpen.value = true;
    guidedTourActive.value = false;
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
        currentPath: filePicker.value.currentPath || "/media/nas/电影",
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
    guidedTourActive,
    contextMenu,
    metadataManager,
    filePicker,
    libraryEditor,
    toasts,
    completeInitialization,
    resetInitialization,
    openContextMenu,
    closeContextMenu,
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
