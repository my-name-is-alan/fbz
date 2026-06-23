import type { MediaLibrary } from "@/types/media.ts";
import { libraries as mockLibraries } from "@/service/modules/media.ts";
import { libraryCounts } from "@/service/modules/tmdb.ts";

/**
 * 媒体库 store —— header 下拉、移动端抽屉、媒体库总览页共享同一份库列表。
 * 库的元信息（名称/类型）来自 mock，条目数用 TMDB 真实目录统计覆盖。
 * 真实接入后端后，把这两处换成 service 请求即可，消费方不变。
 */
export const useLibraryStore = defineStore("library", () => {
  const counts = libraryCounts();
  const libraries = ref<MediaLibrary[]>(
    mockLibraries.map((lib) => ({ ...lib, count: counts[lib.id] ?? lib.count })),
  );

  const totalCount = computed(() => libraries.value.reduce((sum, lib) => sum + lib.count, 0));

  function getById(id: string) {
    return libraries.value.find((lib) => lib.id === id);
  }

  return { libraries, totalCount, getById };
});
