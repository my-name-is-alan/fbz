import type { MediaKind, MediaLibrary } from "@/types/media.ts";
import { fetchNavigation } from "@/service/modules/navigation.ts";
import type { LibrarySettings } from "@/types/admin.ts";
import { kindForLibraryType, normalizeLibraryType } from "@/utils/libraryType.ts";

/**
 * 库类型以后端 `collectionType` / `libraryType` 为事实源（Emby CollectionType 词汇）；
 * `kind` 只是展示分组，由 utils/libraryType.ts 一处映射——homevideos / mixed / livetv
 * 不再并进 movie / series。BFF 的 best-effort `kind` 仅在 collectionType 缺失时兜底。
 */
function kindFromBackend(collectionType: string, backendKind: string): MediaKind {
  if (normalizeLibraryType(collectionType)) return kindForLibraryType(collectionType);
  return kindForLibraryType(backendKind);
}

/**
 * 媒体库 store —— header 下拉、移动端抽屉、媒体库总览页共享同一份库列表。
 * 媒体库列表以 Rust 后端为单一事实源；后端不可达时保持空列表并暴露错误状态。
 */
export const useLibraryStore = defineStore("library", () => {
  const libraries = ref<MediaLibrary[]>([]);

  const totalCount = computed(() => libraries.value.reduce((sum, lib) => sum + lib.count, 0));

  function getById(id: string) {
    return libraries.value.find((lib) => lib.id === id);
  }

  const loaded = ref(false);
  const loading = ref(false);
  const error = ref<string | null>(null);

  /** 从导航 BFF 拉取当前用户可见的真实媒体库列表。 */
  async function loadFromBackend(): Promise<boolean> {
    loading.value = true;
    error.value = null;
    try {
      const nav = await fetchNavigation();
      libraries.value = nav.libraries.map((lib) => ({
        id: lib.id,
        name: lib.name,
        kind: kindFromBackend(lib.collectionType, lib.kind),
        count: lib.count,
        libraryType: normalizeLibraryType(lib.collectionType) ?? lib.collectionType,
      }));
      loaded.value = true;
      return true;
    } catch {
      error.value = "媒体库列表加载失败，请检查网络与服务器状态。";
      return false;
    } finally {
      loading.value = false;
    }
  }

  /** 用管理端完整设置列表替换本地库列表。 */
  function replaceFromSettings(settings: LibrarySettings[]): void {
    libraries.value = settings.map((lib) => ({
      id: lib.id,
      name: lib.name,
      kind: kindForLibraryType(lib.libraryType),
      count: getById(lib.id)?.count ?? 0,
      libraryType: lib.libraryType,
      metadataLanguage: lib.preferredMetadataLanguage ?? undefined,
      metadataCountry: lib.preferredMetadataCountry ?? undefined,
      imageLanguage: lib.preferredImageLanguage ?? undefined,
      preferOriginalPoster: lib.preferredImagePreferOriginal ?? undefined,
      imageFallbackLanguages: lib.preferredImageFallbackLanguages,
      isHidden: lib.isHidden,
    }));
    loaded.value = true;
    error.value = null;
  }

  return {
    libraries,
    totalCount,
    loaded,
    loading,
    error,
    getById,
    loadFromBackend,
    refresh: loadFromBackend,
    replaceFromSettings,
  };
});
