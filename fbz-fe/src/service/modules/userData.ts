/**
 * Emby 用户数据 service：收藏、评分、播放状态等用户维度操作。
 */
import { embyRequest } from "@/service/request.ts";

/** 设置或取消收藏。 */
export async function setFavorite(
  userId: string,
  itemId: string,
  favorite: boolean,
): Promise<void> {
  const path = `/Users/${encodeURIComponent(userId)}/FavoriteItems/${encodeURIComponent(itemId)}`;
  if (favorite) {
    await embyRequest.post(path);
  } else {
    await embyRequest.delete(path);
  }
}
