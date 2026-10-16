import type { MediaLibrary } from "@/types/media.ts";
import { normalizeLibraryType } from "@/utils/libraryType.ts";

/**
 * 库卡片/菜单项的目标路由。音乐库走专属浏览路径 `/music/:id`（artist→album→track 三级），
 * 书库走 `/books/:id`，其余库走通用网格 `/library/:id`（网格按 libraryType 透传条目类型）。
 * 集中在此，避免库列表的多个消费方（总览页、抽屉、header 下拉）各自判断导致漂移。
 */
export function libraryRoute(library: Pick<MediaLibrary, "id" | "kind" | "libraryType">): string {
  const type = normalizeLibraryType(library.libraryType);
  if (type === "music" || (!type && library.kind === "music")) return `/music/${library.id}`;
  if (type === "books" || (!type && library.kind === "book")) return `/books/${library.id}`;
  return `/library/${library.id}`;
}
