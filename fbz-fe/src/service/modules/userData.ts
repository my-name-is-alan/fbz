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

/**
 * 标记已看 / 未看。对接 `POST|DELETE /Users/{userId}/PlayedItems/{itemId}`。
 */
export async function setPlayed(userId: string, itemId: string, played: boolean): Promise<void> {
  const path = `/Users/${encodeURIComponent(userId)}/PlayedItems/${encodeURIComponent(itemId)}`;
  if (played) {
    await embyRequest.post(path);
  } else {
    await embyRequest.delete(path);
  }
}

/** Emby UserConfiguration（字幕语言、首页分区 OrderedViews 等）。 */
export interface UserConfiguration {
  AudioLanguagePreference?: string | null;
  PlayDefaultAudioTrack?: boolean;
  SubtitleLanguagePreference?: string | null;
  DisplayMissingEpisodes?: boolean;
  SubtitleMode?: string;
  OrderedViews?: string[];
  LatestItemsExcludes?: string[];
  MyMediaExcludes?: string[];
  HidePlayedInLatest?: boolean;
  RememberAudioSelections?: boolean;
  RememberSubtitleSelections?: boolean;
  EnableNextEpisodeAutoPlay?: boolean;
}

/** GET /Users/{id}/Configuration —— 可读已持久化偏好（缺省为服务端默认）。 */
export async function getUserConfiguration(userId: string): Promise<UserConfiguration> {
  const { data } = await embyRequest.get<UserConfiguration>(
    `/Users/${encodeURIComponent(userId)}/Configuration`,
  );
  return data ?? {};
}

/** POST /Users/{id}/Configuration —— 整份覆盖写回。 */
export async function updateUserConfiguration(
  userId: string,
  configuration: UserConfiguration,
): Promise<void> {
  await embyRequest.post(`/Users/${encodeURIComponent(userId)}/Configuration`, configuration);
}

/** DisplayPreferences 最小形状（CustomPrefs 可存 homesectionN 等）。 */
export interface DisplayPreferences {
  Id?: string;
  SortBy?: string;
  SortOrder?: string;
  Client?: string;
  CustomPrefs?: Record<string, string>;
  ViewType?: string | null;
  ScrollDirection?: string;
  ShowBackdrop?: boolean;
  RememberSorting?: boolean;
}

/** GET /DisplayPreferences/{id}?userId=&client= */
export async function getDisplayPreferences(
  displayPreferencesId: string,
  userId: string,
  client = "emby",
): Promise<DisplayPreferences> {
  const { data } = await embyRequest.get<DisplayPreferences>(
    `/DisplayPreferences/${encodeURIComponent(displayPreferencesId)}`,
    { params: { userId, client } },
  );
  return data ?? {};
}

/** POST /DisplayPreferences/{id}?userId=&client= */
export async function updateDisplayPreferences(
  displayPreferencesId: string,
  userId: string,
  body: DisplayPreferences,
  client = "emby",
): Promise<void> {
  await embyRequest.post(
    `/DisplayPreferences/${encodeURIComponent(displayPreferencesId)}`,
    { ...body, Client: body.Client ?? client },
    { params: { userId, client } },
  );
}

/** Emby ContentSection（`GET /Users/{id}/HomeSections`）。 */
export interface HomeSection {
  Id: string;
  Name?: string;
  SectionType?: string;
  CollectionType?: string | null;
}

/**
 * GET /Users/{id}/HomeSections —— 服务端已按 DisplayPreferences.homesectionN /
 * UserConfiguration.OrderedViews 排好序；无显式偏好时返回 Emby 默认序。
 */
export async function getHomeSections(userId: string): Promise<HomeSection[]> {
  const { data } = await embyRequest.get<HomeSection[]>(
    `/Users/${encodeURIComponent(userId)}/HomeSections`,
  );
  return Array.isArray(data) ? data : [];
}
