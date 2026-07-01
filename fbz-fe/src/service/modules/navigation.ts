/**
 * 导航 service：对接 BFF 聚合端点 `GET /api/navigation`。
 *
 * 一次请求拿齐首屏所需：当前用户、可见库、首页分区、hero 主打项。
 * 这里把后端原始 DTO 映射为组件直接可用的形状——poster 路径拼成带鉴权的绝对地址，
 * 跨库条目的 libraryId 按 detailType 兜底，让 home 页消费方无需感知后端形状。
 */
import { mediaImageUrl, request } from "@/service/request.ts";
import type { ContinueItem, FeaturedItem, MediaItem } from "@/types/media.ts";
import type {
  HomeData,
  HomeRow,
  NavigationFeatured,
  NavigationMediaItem,
  NavigationResponse,
} from "@/types/navigation.ts";

/** detailType → 兜底 libraryId（继续观看等跨库行后端不下发 libraryId）。 */
function libraryIdFor(item: NavigationMediaItem): string {
  if (item.libraryId) return item.libraryId;
  return item.detailType === "tv" ? "series" : "movie";
}

/** 后端媒体条目 → 前端 MediaItem/ContinueItem（拼绝对海报地址）。 */
function toContinueItem(item: NavigationMediaItem): ContinueItem {
  const base: MediaItem = {
    id: item.id,
    libraryId: libraryIdFor(item),
    title: item.title,
    meta: item.meta,
    detailType: item.detailType,
    poster: mediaImageUrl(item.poster),
    year: item.year,
    rating: item.rating,
  };
  return item.progress === undefined ? base : { ...base, progress: item.progress };
}

/** 后端 hero 项 → 前端 FeaturedItem（拼绝对图片地址）。 */
function toFeatured(item: NavigationFeatured): FeaturedItem {
  return {
    id: item.id,
    title: item.title,
    meta: item.meta,
    tags: item.tags,
    overview: item.overview,
    backdrop: mediaImageUrl(item.backdrop),
    thumb: mediaImageUrl(item.thumb),
  };
}

/** 拉取并返回原始导航响应（库列表等其他消费方用）。 */
export async function fetchNavigation(): Promise<NavigationResponse> {
  const { data } = await request.get<NavigationResponse>("/navigation");
  return data;
}

/** 拉取并映射为首页直接可用的数据（featured + rows）。 */
export async function fetchHomeData(): Promise<HomeData> {
  const nav = await fetchNavigation();
  const rows: HomeRow[] = nav.sections.map((section) => ({
    key: section.key,
    title: section.title,
    layout: section.layout,
    to: section.to,
    items: section.items.map(toContinueItem),
  }));
  return { featured: nav.featured.map(toFeatured), rows };
}
