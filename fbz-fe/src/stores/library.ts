import type { MediaKind, MediaLibrary } from "@/types/media.ts";
import { fetchNavigation } from "@/service/modules/navigation.ts";
import type { LibrarySettings } from "@/types/admin.ts";

/**
 * 后端 kind（导航 BFF 已做 library_type → 展示类型的 best-effort 映射）→ 前端 MediaKind。
 * BFF 给的是 movie/series/music/mixed/livetv；前端的 anime/documentary 是用户语义，
 * 后端无法区分，故归入最接近的 movie/series，待展示层扩展。
 */
function kindFromBackendKind(kind: string): MediaKind {
  switch (kind) {
    case "series":
    case "livetv":
      return "series";
    case "music":
      return "music";
    case "movie":
    case "mixed":
    default:
      return "movie";
  }
}

function kindFromLibraryType(libraryType: string): MediaKind {
  switch (libraryType) {
    case "tvshows":
    case "livetv":
      return "series";
    case "music":
      return "music";
    case "movies":
    case "homevideos":
    case "mixed":
    default:
      return "movie";
  }
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
        kind: kindFromBackendKind(lib.kind),
        count: lib.count,
        libraryType: lib.collectionType,
      }));
      loaded.value = true;
      return true;
    } catch (err) {
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
      kind: kindFromLibraryType(lib.libraryType),
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
    replaceFromSettings,
  };
});
