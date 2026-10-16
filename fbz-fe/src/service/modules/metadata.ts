/**
 * 元数据识别 / 外挂字幕：对接 Emby RemoteSearch 与 Subtitles 搜索。
 * 识别需管服权限；外挂字幕搜索对当前用户可见条目开放（后端无 provider 时返回空数组）。
 */
import { embyRequest } from "@/service/request.ts";

export interface RemoteSearchCandidate {
  Name?: string | null;
  OriginalTitle?: string | null;
  ProviderIds?: Record<string, string> | null;
  ProductionYear?: number | null;
  Overview?: string | null;
  ImageUrl?: string | null;
  SearchProviderName?: string | null;
}

export type RemoteSearchKind = "Movie" | "Series";

export interface RemoteSearchParams {
  kind: RemoteSearchKind;
  itemId: string;
  name: string;
  year?: number;
}

/** POST /Items/RemoteSearch/{Movie|Series} */
export async function remoteSearchItems(
  params: RemoteSearchParams,
): Promise<RemoteSearchCandidate[]> {
  const path =
    params.kind === "Series" ? "/Items/RemoteSearch/Series" : "/Items/RemoteSearch/Movie";
  const { data } = await embyRequest.post<RemoteSearchCandidate[]>(path, {
    SearchInfo: {
      Name: params.name.trim(),
      Year: params.year ?? undefined,
    },
    ItemId: params.itemId,
  });
  return Array.isArray(data) ? data : [];
}

/** POST /Items/RemoteSearch/Apply/{itemId} —— 写入 ProviderIds 并入队刷新。 */
export async function applyRemoteSearch(
  itemId: string,
  candidate: RemoteSearchCandidate,
  replaceAllImages = false,
): Promise<void> {
  await embyRequest.post(`/Items/RemoteSearch/Apply/${encodeURIComponent(itemId)}`, candidate, {
    params: { ReplaceAllImages: replaceAllImages },
  });
}

export interface RemoteSubtitleInfo {
  ThreeLetterISOLanguageName?: string | null;
  Id?: string | null;
  ProviderName?: string | null;
  Name?: string | null;
  Format?: string | null;
  Author?: string | null;
  Comment?: string | null;
  DateCreated?: string | null;
  CommunityRating?: number | null;
  DownloadCount?: number | null;
  IsHashMatch?: boolean | null;
  IsForced?: boolean | null;
  IsHearingImpaired?: boolean | null;
  Language?: string | null;
}

/** GET /Items/{id}/RemoteSearch/Subtitles/{language} —— 无 provider 时为空数组。 */
export async function searchRemoteSubtitles(
  itemId: string,
  language = "chi",
  mediaSourceId?: string,
): Promise<RemoteSubtitleInfo[]> {
  const { data } = await embyRequest.get<RemoteSubtitleInfo[]>(
    `/Items/${encodeURIComponent(itemId)}/RemoteSearch/Subtitles/${encodeURIComponent(language)}`,
    { params: mediaSourceId ? { mediaSourceId } : undefined },
  );
  return Array.isArray(data) ? data : [];
}

/**
 * POST /Items/{id}/RemoteSearch/Subtitles/{subtitleId} —— 下载远程字幕并写为外挂 sidecar。
 * 后端要求管服权限（`subtitle.remote.download` 审计）。
 */
export async function downloadRemoteSubtitle(
  itemId: string,
  subtitleId: string,
  mediaSourceId?: string,
): Promise<void> {
  await embyRequest.post(
    `/Items/${encodeURIComponent(itemId)}/RemoteSearch/Subtitles/${encodeURIComponent(subtitleId)}`,
    null,
    { params: mediaSourceId ? { mediaSourceId } : undefined },
  );
}
