import type { MediaLibrary, MediaKind } from "@/types/media.ts";
import { readSession, serverRequest, errorMessage } from "@/service/modules/server.ts";
import type { ServerLibrary } from "@/service/modules/server.ts";

export const useLibraryStore = defineStore("library", () => {
  const libraries = ref<MediaLibrary[]>([]);
  const loading = ref(false);
  const error = ref("");
  const totalCount = computed(() => libraries.value.reduce((sum, lib) => sum + lib.count, 0));
  function getById(id: string) {
    return libraries.value.find((lib) => lib.id === id);
  }
  async function refresh() {
    if (!readSession()) {
      libraries.value = [];
      return;
    }
    loading.value = true;
    error.value = "";
    try {
      const rows = await serverRequest<ServerLibrary[]>("/emby/Library/VirtualFolders");
      const totals = await serverRequest<{ Id: string; Count: number }[]>(
        "/api/media/library-counts",
      );
      libraries.value = rows.map((lib) => ({
        id: lib.ItemId || lib.Id,
        name: lib.Name,
        kind: ({ movies: "movie", tvshows: "series", tv: "series", music: "music" }[
          lib.CollectionType
        ] ?? "movie") as MediaKind,
        count: totals.find((t) => t.Id === (lib.ItemId || lib.Id))?.Count ?? 0,
        paths: lib.Locations,
      }));
    } catch (err) {
      error.value = errorMessage(err);
    } finally {
      loading.value = false;
    }
  }
  return { libraries, totalCount, getById, refresh, loading, error };
});
