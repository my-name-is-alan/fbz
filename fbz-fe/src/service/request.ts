import type { InternalAxiosRequestConfig } from "axios";
import axios from "axios";

/**
 * fbz-api 暴露两个并存的 HTTP 面，前端据此分两个 axios 实例：
 * - `/api/*`：BFF 聚合端点（`/api/navigation`、`/api/setup`）+ 管理端（`/api/admin/*`），走 {@link request}。
 * - 根路径 Emby 兼容面：登录（`/Users/AuthenticateByName`）、图片（`/Items/{id}/Images/...`），走 {@link embyRequest}。
 *
 * 本应用是「封闭单实例」部署：前端只服务于对应的那一个后端，**不支持用户填写服务器地址**。
 * origin 固定为同源（相对路径）；仅构建期可用 `VITE_API_BASE_URL` 指定绝对后端地址（部署用）。
 */

/** localStorage key：登录后签发的 Emby 兼容访问令牌。 */
export const ACCESS_TOKEN_KEY = "fbz_access_token";

/** `/api` 面的前缀；允许构建期用 `VITE_API_BASE_URL` 覆盖（可为绝对 URL）。 */
const API_PREFIX = import.meta.env.VITE_API_BASE_URL ?? "/api";

/** 读取持久化的访问令牌。 */
export function getAccessToken(): string | null {
  return localStorage.getItem(ACCESS_TOKEN_KEY);
}

/** 写入（或清除）后续请求使用的访问令牌。 */
export function setAccessToken(token: string | null): void {
  if (token) {
    localStorage.setItem(ACCESS_TOKEN_KEY, token);
  } else {
    localStorage.removeItem(ACCESS_TOKEN_KEY);
  }
}

/**
 * 拼接媒体图片的可直接用于 `<img src>` 的地址。
 *
 * 后端 DTO 给出的是服务器根路径（如 `/Items/{id}/Images/Primary`），不含鉴权。
 * 图片端点接受查询串 token，而 `<img>` 不会自动带 `x-emby-token` 头，故这里补 `api_key`。
 * 同源部署下直接用相对路径；路径为空（无图）时返回 `undefined`，交前端渲染占位块。
 */
export function mediaImageUrl(path: string | null | undefined): string | undefined {
  if (!path) return undefined;
  const token = getAccessToken();
  const query = token
    ? `${path.includes("?") ? "&" : "?"}api_key=${encodeURIComponent(token)}`
    : "";
  return `${path}${query}`;
}

/** 为图片 URL 追加查询参数，保留已有 `api_key`。 */
function withImageParams(url: string, params: Record<string, string | number>): string {
  const [base, query = ""] = url.split("?");
  const search = new URLSearchParams(query);
  for (const [key, value] of Object.entries(params)) {
    search.set(key, String(value));
  }
  const serialized = search.toString();
  return serialized ? `${base}?${serialized}` : base;
}

/**
 * 生成响应式图片分层。当前后端已支持同一图片端点读取本地缓存；
 * 这里先把期望尺寸写入查询串，后端具备缩放后浏览器会按 media/sizes 自动选更小资源。
 */
export function mediaImageSrcSet(
  src: string | null | undefined,
  widths: number[],
): string | undefined {
  if (!src) return undefined;
  return widths
    .map(
      (width) =>
        `${withImageParams(src, { maxWidth: width, quality: width >= 1280 ? 86 : 78 })} ${width}w`,
    )
    .join(", ");
}

/**
 * 拼接音频直出流地址，可直接用于 `<audio src>`。
 *
 * 走 Emby 根面 `GET /Audio/{id}/universal`（见 `streaming.rs:universal_audio_stream`）：
 * 直出原文件、支持 RANGE 请求，故 `<audio>` 能边下边播且可拖动进度。`<audio>` 不会自动带
 * `x-emby-token` 头，端点接受查询串 token，故这里补 `api_key`。无 token 时返回 `undefined`。
 */
export function audioStreamUrl(itemId: string): string | undefined {
  const token = getAccessToken();
  if (!token) return undefined;
  const query = `?api_key=${encodeURIComponent(token)}`;
  return `/Audio/${encodeURIComponent(itemId)}/universal${query}`;
}

/**
 * 拼接用户头像地址，可直接用于 `<img src>`。
 *
 * 头像 GET 面 `GET /api/users/{id}/avatar` 无需鉴权（同实例部署、public_id 不可枚举），
 * 未设置头像时返回 404，交给 `<img>` 的 onerror 回退首字母头像。`version` 传头像更新时间戳
 * 做缓存击穿——更换头像后 URL 变化，浏览器立即重新拉取。userId 为空时返回 `undefined`。
 */
export function userAvatarUrl(
  userId: string | null | undefined,
  version?: string | number | null,
): string | undefined {
  if (!userId) return undefined;
  const base = `${API_PREFIX}/users/${encodeURIComponent(userId)}/avatar`;
  return version != null && version !== ""
    ? `${base}?v=${encodeURIComponent(String(version))}`
    : base;
}

/** 把访问令牌注入到请求头（fbz-api 用 `x-emby-token` 鉴权）。 */
function attachAccessToken(config: InternalAxiosRequestConfig): void {
  const token = getAccessToken();
  if (token) {
    config.headers.set("x-emby-token", token);
  }
}

/** `/api` 面客户端：BFF 聚合 + 管理端。 */
export const request = axios.create({ baseURL: API_PREFIX, timeout: 10_000 });

request.interceptors.request.use((config: InternalAxiosRequestConfig) => {
  attachAccessToken(config);
  return config;
});

/** Emby 根面客户端：登录、图片等兼容端点（同源根路径）。 */
export const embyRequest = axios.create({ timeout: 10_000 });

embyRequest.interceptors.request.use((config: InternalAxiosRequestConfig) => {
  attachAccessToken(config);
  return config;
});
