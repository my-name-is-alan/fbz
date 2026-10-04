import type { MediaLibrary } from "@/types/media.ts";

/**
 * 库卡片/菜单项的目标路由。音乐库走专属浏览路径 `/music/:id`（artist→album→track 三级），
 * 其余库走通用网格 `/library/:id`。集中在此，避免库列表的多个消费方（总览页、抽屉、
 * header 下拉）各自判断 kind 导致漂移。
 */
export function libraryRoute(library: Pick<MediaLibrary, "id" | "kind">): string {
  return library.kind === "music" ? `/music/${library.id}` : `/library/${library.id}`;
}
