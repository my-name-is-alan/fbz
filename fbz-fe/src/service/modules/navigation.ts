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
  // 同步权威头像版本到 auth store：null = 未设置头像（BaseAvatar 不发请求）。
  // 动态引入避免 service 模块在 Pinia 安装前建立 store 依赖。
  try {
    const { useAuthStore } = await import("@/stores/auth.ts");
    const auth = useAuthStore();
    auth.setAvatarVersion(data.user.avatarVersion ?? null);
    auth.setIsAdmin(Boolean(data.user.isAdmin));
  } catch {
    // store 不可用（如测试环境）时忽略，头像回退首字母占位。
  }
  return data;
}

/** 拉取用户收藏（Items?Filters=IsFavorite；FavoriteItems 仅 mark/unmark）；失败返回空。 */
async function fetchFavoriteHomeItems(userId: string, limit = 24): Promise<ContinueItem[]> {
  try {
    const { embyRequest, mediaImageUrl: img } = await import("@/service/request.ts");
    const { data } = await embyRequest.get<{
      Items?: Array<{
        Id: string;
        Name: string;
        Type: string;
        ProductionYear?: number | null;
        CommunityRating?: number | null;
        ImageTags?: Record<string, string> | null;
        UserData?: {
          Played?: boolean;
          IsFavorite?: boolean;
          PlaybackPositionTicks?: number;
        } | null;
        RunTimeTicks?: number | null;
      }>;
    }>(`/Users/${encodeURIComponent(userId)}/Items`, {
      params: {
        recursive: true,
        includeItemTypes: "Movie,Series",
        filters: "IsFavorite",
        isFavorite: true,
        sortBy: "DateCreated",
        sortOrder: "Descending",
        limit,
        fields: "UserData,PrimaryImageAspectRatio",
        enableImages: true,
        enableImageTypes: "Primary",
      },
    });
    return (data.Items ?? []).map((item) => {
      const detailType = item.Type === "Series" || item.Type === "Episode" ? "tv" : "movie";
      const played = item.UserData?.Played ?? false;
      const position = item.UserData?.PlaybackPositionTicks ?? 0;
      const runtime = item.RunTimeTicks ?? 0;
      const progress =
        !played && position > 0 && runtime > 0
          ? Math.min(100, Math.max(0, (position / runtime) * 100))
          : undefined;
      return {
        id: item.Id,
        libraryId: detailType === "tv" ? "series" : "movie",
        title: item.Name,
        meta: `${item.ProductionYear ?? "—"} · ${detailType === "tv" ? "剧集" : "电影"}`,
        detailType,
        poster: item.ImageTags?.Primary ? img(`/Items/${item.Id}/Images/Primary`) : undefined,
        year: item.ProductionYear ?? undefined,
        rating: item.CommunityRating ?? undefined,
        isFavorite: true,
        isPlayed: played,
        progress,
      };
    });
  } catch {
    return [];
  }
}

/** NextUp：跨剧「下一集待看」，补给首页继续观看；播放器内「下一集」仍走剧集 playlist。 */
async function fetchNextUpForContinue(userId: string, limit = 16): Promise<ContinueItem[]> {
  try {
    const { embyRequest, mediaImageUrl: img } = await import("@/service/request.ts");
    const { data } = await embyRequest.get<{
      Items?: Array<{
        Id: string;
        Name: string;
        Type: string;
        SeriesId?: string | null;
        SeriesName?: string | null;
        ParentIndexNumber?: number | null;
        IndexNumber?: number | null;
        ProductionYear?: number | null;
        CommunityRating?: number | null;
        ImageTags?: Record<string, string> | null;
        UserData?: {
          Played?: boolean;
          IsFavorite?: boolean;
          PlaybackPositionTicks?: number;
        } | null;
        RunTimeTicks?: number | null;
      }>;
    }>("/Shows/NextUp", {
      params: {
        userId,
        limit,
        fields: "UserData,PrimaryImageAspectRatio,Overview",
        enableImages: true,
        enableImageTypes: "Primary",
      },
    });
    return (data.Items ?? []).map((item) => {
      const season = item.ParentIndexNumber;
      const episode = item.IndexNumber;
      const epLabel =
        season != null && episode != null
          ? `S${String(season).padStart(2, "0")}E${String(episode).padStart(2, "0")}`
          : "剧集";
      const seriesBit = item.SeriesName ? `${item.SeriesName} · ` : "";
      const played = item.UserData?.Played ?? false;
      const position = item.UserData?.PlaybackPositionTicks ?? 0;
      const runtime = item.RunTimeTicks ?? 0;
      const progress =
        !played && position > 0 && runtime > 0
          ? Math.min(100, Math.max(0, (position / runtime) * 100))
          : undefined;
      return {
        id: item.Id,
        libraryId: "series",
        title: item.Name,
        meta: `${seriesBit}${epLabel}`,
        detailType: "tv" as const,
        poster: item.ImageTags?.Primary
          ? img(`/Items/${item.Id}/Images/Primary`)
          : item.SeriesId
            ? img(`/Items/${item.SeriesId}/Images/Primary`)
            : undefined,
        year: item.ProductionYear ?? undefined,
        rating: item.CommunityRating ?? undefined,
        isFavorite: item.UserData?.IsFavorite ?? false,
        isPlayed: played,
        progress,
      };
    });
  } catch {
    return [];
  }
}

/** 规范化 Emby 首页分区 id（对齐后端 `normalize_home_section_pref`）。 */
export function normalizeHomeSectionId(value: string): string {
  return value.trim().toLowerCase().replace(/[_-]/g, "");
}

/** Emby HomeSection / DisplayPreferences id → C 端 HomeRow.key；无对应行则 null。 */
export function mapHomeSectionIdToRowKey(sectionId: string): string | null {
  switch (normalizeHomeSectionId(sectionId)) {
    case "resume":
    case "continuewatching":
      return "continue";
    case "latestmedia":
    case "latest":
    case "recentlyadded":
      return "latest";
    case "favorites":
    case "favourites":
      return "favorites";
    default:
      return null;
  }
}

function isKnownHomeSectionId(id: string): boolean {
  return (
    mapHomeSectionIdToRowKey(id) != null ||
    ["movies", "series", "tvshows", "music", "librarybuttons", "smalllibrarytiles"].includes(
      normalizeHomeSectionId(id),
    )
  );
}

/** 从 DisplayPreferences.CustomPrefs 解析 homesection0..N（跳过空 / none）。 */
export function parseHomesectionPrefs(
  customPrefs: Record<string, string> | null | undefined,
): string[] {
  if (!customPrefs) return [];
  const ordered: string[] = [];
  for (let index = 0; index < 32; index += 1) {
    const raw = customPrefs[`homesection${index}`];
    if (raw == null) break;
    const trimmed = raw.trim();
    if (!trimmed || trimmed.toLowerCase() === "none") continue;
    ordered.push(normalizeHomeSectionId(trimmed));
  }
  return ordered;
}

/**
 * 按偏好序重排已有首页行；偏好里没有的行保持相对顺序追加在末尾。
 * preferredRowKeys 为空时原样返回（保持现状）。
 */
export function applyHomeSectionOrder(rows: HomeRow[], preferredRowKeys: string[]): HomeRow[] {
  if (!preferredRowKeys.length || !rows.length) return rows;
  const byKey = new Map(rows.map((row) => [row.key, row]));
  const ordered: HomeRow[] = [];
  const used = new Set<string>();
  for (const key of preferredRowKeys) {
    if (used.has(key)) continue;
    const row = byKey.get(key);
    if (!row) continue;
    ordered.push(row);
    used.add(key);
  }
  for (const row of rows) {
    if (!used.has(row.key)) ordered.push(row);
  }
  return ordered;
}

function uniqueMappedRowKeys(sectionIds: string[]): string[] {
  const keys: string[] = [];
  const seen = new Set<string>();
  for (const id of sectionIds) {
    const key = mapHomeSectionIdToRowKey(id);
    if (!key || seen.has(key)) continue;
    seen.add(key);
    keys.push(key);
  }
  return keys;
}

/**
 * 解析首页分区偏好序（对齐后端 preferred_home_section_ids）：
 * 1) DisplayPreferences/usersettings CustomPrefs.homesectionN（多 client 试探）
 * 2) 若有显式偏好，可用 HomeSections 的服务端已排序 Id 列表
 * 3) 回退 UserConfiguration.OrderedViews 中的已知首页分区 id
 * 无显式偏好返回 []，调用方保持现状（不把 Emby 默认 HomeSections 序强加到 C 端）。
 */
async function resolvePreferredHomeRowKeys(userId: string): Promise<string[]> {
  const { getDisplayPreferences, getHomeSections, getUserConfiguration } =
    await import("@/service/modules/userData.ts");

  // 1) DisplayPreferences CustomPrefs.homesectionN（与后端一致：emby / Unknown / FBZ）
  for (const client of ["emby", "Unknown", "FBZ"] as const) {
    try {
      const prefs = await getDisplayPreferences("usersettings", userId, client);
      const fromPrefs = parseHomesectionPrefs(prefs.CustomPrefs);
      if (fromPrefs.length) {
        // 优先采信服务端 HomeSections（已做别名归一）；失败则用本地解析。
        try {
          const sections = await getHomeSections(userId);
          const mapped = uniqueMappedRowKeys(sections.map((section) => section.Id).filter(Boolean));
          if (mapped.length) return mapped;
        } catch {
          // fall through to local parse
        }
        return uniqueMappedRowKeys(fromPrefs);
      }
    } catch {
      // try next client
    }
  }

  // 2) UserConfiguration.OrderedViews 中的已知首页分区 id
  try {
    const configuration = await getUserConfiguration(userId);
    const fromViews = (configuration.OrderedViews ?? [])
      .map(normalizeHomeSectionId)
      .filter(isKnownHomeSectionId);
    if (fromViews.length) {
      try {
        const sections = await getHomeSections(userId);
        const mapped = uniqueMappedRowKeys(sections.map((section) => section.Id).filter(Boolean));
        if (mapped.length) return mapped;
      } catch {
        // fall through
      }
      return uniqueMappedRowKeys(fromViews);
    }
  } catch {
    // ignore
  }

  return [];
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

  // Resume（BFF continue）+ NextUp（Shows/NextUp）合并为继续观看；播放器下一集不走此 API。
  const nextUp = await fetchNextUpForContinue(nav.user.id);
  if (nextUp.length) {
    const continueIdx = rows.findIndex((row) => row.key === "continue");
    if (continueIdx >= 0) {
      const seen = new Set(rows[continueIdx]!.items.map((item) => item.id));
      const extras = nextUp.filter((item) => !seen.has(item.id));
      if (extras.length) {
        rows[continueIdx] = {
          ...rows[continueIdx]!,
          items: [...rows[continueIdx]!.items, ...extras],
        };
      }
    } else {
      rows.unshift({
        key: "continue",
        title: "继续观看",
        layout: "wide",
        items: nextUp,
      });
    }
  }

  // JF 风格：在「继续观看 / 最新」之外补「收藏」分区（后端 navigation 尚未下发时由 C 端补齐）。
  // 列表用 Items?Filters=IsFavorite（FavoriteItems 路径仅 mark/unmark）。
  if (!rows.some((row) => row.key === "favorites" || row.title.includes("收藏"))) {
    const favorites = await fetchFavoriteHomeItems(nav.user.id);
    if (favorites.length) {
      const continueIdx = rows.findIndex((row) => row.key === "continue");
      const insertAt = continueIdx >= 0 ? continueIdx + 1 : 0;
      rows.splice(insertAt, 0, {
        key: "favorites",
        title: "收藏",
        layout: "poster",
        items: favorites,
      });
    }
  }

  // 若有 HomeSections / DisplayPreferences / OrderedViews 排序则按序排分区；否则保持现状。
  const preferredKeys = await resolvePreferredHomeRowKeys(nav.user.id);
  const orderedRows = preferredKeys.length ? applyHomeSectionOrder(rows, preferredKeys) : rows;

  return { featured: nav.featured.map(toFeatured), rows: orderedRows };
}
