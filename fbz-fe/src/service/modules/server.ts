import { request, getAccessToken, setAccessToken } from "@/service/request.ts";

export interface ServerSession {
  token: string;
  userId: string;
  username: string;
  address: string;
}
export interface ServerItem {
  Id: string;
  Name: string;
  Type: string;
  Overview?: string;
  Genres?: string[];
  CommunityRating?: number;
  BackdropImageTags?: string[];
  ParentId?: string;
  SeriesId?: string;
  SeriesName?: string;
  PrimaryImageItemId?: string;
  BackdropImageItemId?: string;
  IndexNumber?: number;
  ParentIndexNumber?: number;
  MediaSources?: { Id: string; Size?: number; Container?: string }[];
  RunTimeTicks?: number;
  ImageTags?: Record<string, string>;
  ProductionYear?: number;
  UserData?: { PlaybackPositionTicks?: number; Played?: boolean };
}
export interface ServerLibrary {
  Id: string;
  ItemId: string;
  Name: string;
  Locations: string[];
  CollectionType: string;
}
export interface ServerJob {
  id: string;
  jobType: string;
  status: string;
  lastError?: string;
  updatedAt: string;
  payload?: { library_id?: string };
}
export interface ServerPlayback {
  Id: string;
  Name: string;
  UserName: string;
  PositionTicks: number;
  RunTimeTicks: number;
  IsPaused: boolean;
  PlayMethod: string;
}
export function readSession(): ServerSession | undefined {
  const token = getAccessToken();
  const userId = localStorage.getItem("fbz_auth_user_id");
  if (!token || !userId) return undefined;
  return {
    token,
    userId,
    username: localStorage.getItem("fbz_auth_username") || "",
    address: window.location.origin,
  };
}

export function saveSession(session: ServerSession, remember = false) {
  clearSession();
  setAccessToken(session.token);
  if (!remember) {
    localStorage.removeItem("fbz_access_token");
    sessionStorage.setItem("fbz_access_token", session.token);
  }
  localStorage.setItem("fbz_auth_user_id", session.userId);
  localStorage.setItem("fbz_auth_username", session.username);
  (remember ? localStorage : sessionStorage).setItem("fbz_session", JSON.stringify(session));
}
export function clearSession() {
  setAccessToken(null);
  sessionStorage.removeItem("fbz_session");
  localStorage.removeItem("fbz_session");
}
export function serverAddress() {
  return readSession()?.address || window.location.origin;
}
export function deviceId() {
  let id = localStorage.getItem("fbz_device_id");
  if (!id) {
    id = crypto.randomUUID();
    localStorage.setItem("fbz_device_id", id);
  }
  return id;
}
export async function serverRequest<T>(
  path: string,
  data?: unknown,
  method = data === undefined ? "GET" : "POST",
) {
  const session = readSession();
  if (!session) throw new Error("请先连接媒体服务器");
  return (
    await request.request<T>({
      baseURL: session.address,
      timeout: path.startsWith("/api/admin/storage") ? 65000 : 10000,
      url: path,
      method,
      data,
      headers: {
        "X-Emby-Token": session.token,
        Authorization: `Emby Client="FBZ Web", Device="Browser", DeviceId="${deviceId()}", Version="0.1.0"`,
      },
    })
  ).data;
}
export async function authenticate(
  address: string,
  username: string,
  password: string,
  remember = false,
) {
  const url = new URL(address || window.location.origin);
  if (
    !["http:", "https:"].includes(url.protocol) ||
    url.username ||
    url.password ||
    url.search ||
    url.hash
  )
    throw new Error("请输入有效的 HTTP 或 HTTPS 服务器地址");
  const base = url.href.replace(/\/$/, "");
  const result = (
    await request.post<{ AccessToken: string; User: { Id: string; Name: string } }>(
      `${base}/emby/Users/AuthenticateByName`,
      { Username: username, Pw: password },
      {
        headers: {
          Authorization: `Emby Client="FBZ Web", Device="Browser", DeviceId="${deviceId()}", Version="0.1.0"`,
        },
      },
    )
  ).data;
  if (!result.AccessToken || !result.User?.Id) throw new Error("服务器返回了无效的会话");
  saveSession(
    {
      token: result.AccessToken,
      userId: result.User.Id,
      username: result.User.Name,
      address: base,
    },
    remember,
  );
  return result.User;
}
export function errorMessage(error: unknown) {
  // Never expose Axios request URLs: media URLs can contain a session token.
  if (error && typeof error === "object" && "isAxiosError" in error)
    return "请求失败，请检查服务器连接、账户权限或任务日志";
  return error instanceof Error ? error.message : "操作失败，请重试";
}
export function imageUrl(item: ServerItem, type: "Primary" | "Backdrop" = "Primary") {
  if (type === "Primary" ? !item.ImageTags?.Primary : !item.BackdropImageTags?.length)
    return undefined;
  const url = new URL(
    `${serverAddress()}/emby/Items/${encodeURIComponent((type === "Primary" ? item.PrimaryImageItemId : item.BackdropImageItemId) || item.Id)}/Images/${type}`,
  );
  url.searchParams.set("api_key", readSession()?.token ?? "");
  url.searchParams.set("maxWidth", type === "Backdrop" ? "1920" : "480");
  return url.href;
}
export interface BrowseOptions {
  types?: string;
  genre?: string;
  year?: string;
  played?: string;
  sort?: string;
  descending?: boolean;
  limit?: number;
}
export async function listItems(
  libraryId = "",
  start = 0,
  search = "",
  options: BrowseOptions = {},
) {
  const query = new URLSearchParams({
    UserId: readSession()?.userId ?? "",
    Recursive: "true",
    IncludeItemTypes: options.types ?? "Movie,Series",
    Limit: String(options.limit ?? 36),
    StartIndex: String(start),
    Fields: "UserData,MediaSources,Overview,Genres,CommunityRating",
    EnableImages: "true",
    EnableImageTypes: "Primary,Backdrop",
    ImageTypeLimit: "1",
    SearchTerm: search,
  });
  if (libraryId) query.set("ParentId", libraryId);
  query.set("SortBy", options.sort || "DateCreated");
  query.set("SortOrder", options.descending === false ? "Ascending" : "Descending");
  if (options.genre) query.set("Genres", options.genre);
  if (options.year) query.set("Years", options.year);
  if (options.played === "true" || options.played === "false")
    query.set("IsPlayed", options.played);
  const result = await serverRequest<{ Items: ServerItem[]; TotalRecordCount: number }>(
    `/emby/Users/${readSession()?.userId}/Items?${query}`,
  );
  result.Items = await enrich(result.Items);
  return result;
}
export async function enrich(items: ServerItem[]) {
  if (!items.length) return items;
  const extra = await serverRequest<ServerItem[]>("/api/media/presentation", {
    ids: items.map((i) => i.Id),
  });
  const map = new Map(extra.map((i) => [i.Id, i]));
  return items.map((item) => ({ ...item, ...map.get(item.Id) }));
}
export async function itemDetail(id: string) {
  const item = await serverRequest<ServerItem>(
    `/emby/Users/${readSession()?.userId}/Items/${encodeURIComponent(id)}?Fields=Overview,Genres,MediaSources,UserData&EnableImages=true&EnableImageTypes=Primary,Backdrop`,
  );
  return (await enrich([item]))[0]!;
}
export async function seriesEpisodes(id: string) {
  const result = await serverRequest<{ Items: ServerItem[] }>(
    `/emby/Shows/${encodeURIComponent(id)}/Episodes?UserId=${readSession()?.userId}&EnableImages=true&EnableImageTypes=Primary,Backdrop&Fields=UserData,MediaSources`,
  );
  const items: ServerItem[] = [];
  for (let i = 0; i < result.Items.length; i += 100)
    items.push(...(await enrich(result.Items.slice(i, i + 100))));
  return { Items: items };
}
export async function preparePlayback(item: ServerItem, mediaSourceId?: string) {
  const detail = await itemDetail(item.Id);
  const info = await serverRequest<{
    PlaySessionId: string;
    MediaSources: {
      Id: string;
      DirectStreamUrl?: string;
      RunTimeTicks?: number;
      Container?: string;
      Size?: number;
      IsRemote?: boolean;
    }[];
  }>(`/emby/Items/${encodeURIComponent(item.Id)}/PlaybackInfo`, {
    UserId: readSession()?.userId,
    DeviceId: deviceId(),
    EnableDirectPlay: true,
    EnableDirectStream: true,
    EnableTranscoding: false,
  });
  const source = mediaSourceId
    ? info.MediaSources.find((s) => s.Id === mediaSourceId)
    : info.MediaSources[0];
  if (!source?.DirectStreamUrl) throw new Error("该条目暂无可播放媒体源");
  const uri = new URL(source.DirectStreamUrl, `${serverAddress()}/`);
  if (!["http:", "https:"].includes(uri.protocol)) throw new Error("不支持的媒体地址");
  const proxy = source.IsRemote
    ? new URL(`${serverAddress()}/api/media/${encodeURIComponent(item.Id)}/bytes`)
    : undefined;
  if (proxy) {
    proxy.searchParams.set("mediaSourceId", source.Id);
    proxy.searchParams.set("api_key", readSession()?.token ?? "");
  }
  return {
    type: (item.Type === "Episode" ? "episode" : "movie") as "movie" | "episode",
    id: item.Id,
    title: item.Type === "Episode" && item.SeriesName ? item.SeriesName : item.Name,
    poster: imageUrl(detail) || imageUrl(item),
    backdrop: imageUrl(detail, "Backdrop"),
    subtitle:
      item.Type === "Episode"
        ? `第 ${item.ParentIndexNumber ?? 1} 季 · 第 ${item.IndexNumber ?? "—"} 集`
        : undefined,
    duration: (source.RunTimeTicks || detail.RunTimeTicks || 0) / 1e7,
    source: {
      uri: uri.href,
      size: source.Size,
      proxyUri: proxy?.href,
      mimeType: source.Container === "mp4" ? "video/mp4" : undefined,
    },
    server: {
      playSessionId: info.PlaySessionId,
      mediaSourceId: source.Id,
      startTicks: detail.UserData?.PlaybackPositionTicks ?? 0,
    },
  };
}

export function mediaCard(item: ServerItem): import("@/types/media.ts").ContinueItem {
  const type = item.Type === "Series" ? "剧集" : item.Type === "Episode" ? "分集" : "电影";
  return {
    id: item.Id,
    title: item.Type === "Episode" && item.SeriesName ? item.SeriesName : item.Name,
    libraryId: "",
    detailType: item.Type === "Series" ? "tv" : "movie",
    poster: imageUrl(item),
    year: item.ProductionYear,
    rating: item.CommunityRating,
    meta:
      item.Type === "Episode"
        ? `第 ${item.ParentIndexNumber ?? 1} 季 · 第 ${item.IndexNumber ?? "—"} 集`
        : [item.ProductionYear, type, item.Genres?.[0]].filter(Boolean).join(" · "),
    serverItem: item,
    progress:
      item.UserData?.PlaybackPositionTicks && item.RunTimeTicks
        ? Math.min(100, (item.UserData.PlaybackPositionTicks / item.RunTimeTicks) * 100)
        : undefined,
  };
}
