/**
 * Trickplay C 端 service：`GET /Videos/{id}/Trickplay` 取已生成规格，
 * 按时间算出拼图图片 URL 与背景偏移，供进度条悬停/拖动预览。
 */
import { embyRequest, mediaImageUrl } from "@/service/request.ts";

export interface TrickplayInfo {
  Width: number;
  Height: number;
  TileWidth: number;
  TileHeight: number;
  ThumbnailCount: number;
  Interval: number;
  Bandwidth: number;
}

export interface TrickplayFrame {
  url: string;
  width: number;
  height: number;
  /** CSS background-position（负偏移，px）。 */
  offsetX: number;
  offsetY: number;
  /** CSS background-size（整张拼图，px）。 */
  sheetWidth: number;
  sheetHeight: number;
}

export async function fetchTrickplay(itemId: string): Promise<TrickplayInfo | null> {
  try {
    const { data } = await embyRequest.get<TrickplayInfo[]>(
      `/Videos/${encodeURIComponent(itemId)}/Trickplay`,
    );
    if (!Array.isArray(data) || !data.length) return null;
    // 优先 320 宽，其次最接近的。
    return [...data].sort((a, b) => Math.abs(a.Width - 320) - Math.abs(b.Width - 320))[0] ?? null;
  } catch {
    return null;
  }
}

export function trickplayFrame(
  itemId: string,
  info: TrickplayInfo,
  seconds: number,
): TrickplayFrame | null {
  if (!info.ThumbnailCount || !info.Interval) return null;
  const index = Math.min(
    info.ThumbnailCount - 1,
    Math.max(0, Math.floor((seconds * 1000) / info.Interval)),
  );
  const perSheet = Math.max(1, info.TileWidth * info.TileHeight);
  const sheet = Math.floor(index / perSheet);
  const inSheet = index % perSheet;
  const col = inSheet % info.TileWidth;
  const row = Math.floor(inSheet / info.TileWidth);
  const url = mediaImageUrl(
    `/Videos/${encodeURIComponent(itemId)}/Trickplay/${info.Width}/${sheet}.jpg`,
  );
  if (!url) return null;
  return {
    url,
    width: info.Width,
    height: info.Height,
    offsetX: -col * info.Width,
    offsetY: -row * info.Height,
    sheetWidth: info.Width * info.TileWidth,
    sheetHeight: info.Height * info.TileHeight,
  };
}
