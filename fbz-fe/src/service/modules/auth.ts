/**
 * 认证 service：对接 fbz-api 的 Emby 兼容登录端点 `POST /Users/AuthenticateByName`。
 *
 * 登录走根路径的 {@link embyRequest}（非 `/api`，同源）。成功后持久化访问令牌，
 * 后续 `/api` 与图片请求由 request 拦截器自动带上 `x-emby-token` / `api_key`。
 */
import { embyRequest, setAccessToken } from "@/service/request.ts";
import type { AuthSession, AuthenticationResult, LoginPayload } from "@/types/navigation.ts";

/** 本应用标识，写入 Emby Authorization 头用于命名会话（仅展示用途，非鉴权）。 */
const CLIENT_NAME = "FBZ Web";
const CLIENT_VERSION = "0.1.0";

/** 取一个稳定的设备 ID（首次登录生成并持久化），便于后端会话管理。 */
function deviceId(): string {
  const key = "fbz_device_id";
  let id = localStorage.getItem(key);
  if (!id) {
    id = crypto.randomUUID();
    localStorage.setItem(key, id);
  }
  return id;
}

/** 构造 Emby Authorization 头：`Emby Client="…",Device="…",DeviceId="…",Version="…"`。 */
function authorizationHeader(): string {
  const device = typeof navigator !== "undefined" ? navigator.platform || "Browser" : "Browser";
  const pairs = [
    `Client="${CLIENT_NAME}"`,
    `Device="${device}"`,
    `DeviceId="${deviceId()}"`,
    `Version="${CLIENT_VERSION}"`,
  ];
  return `Emby ${pairs.join(",")}`;
}

/**
 * 用户名 + 密码登录。成功时持久化令牌并返回归一化会话；
 * 失败抛出 axios 错误，调用方据 `error.response?.status` 给文案。
 */
export async function login(payload: LoginPayload): Promise<AuthSession> {
  const { data } = await embyRequest.post<AuthenticationResult>(
    "/Users/AuthenticateByName",
    { Username: payload.username, Pw: payload.password },
    { headers: { Authorization: authorizationHeader() } },
  );

  setAccessToken(data.AccessToken);

  return {
    accessToken: data.AccessToken,
    userId: data.User.Id,
    username: data.User.Name,
    serverId: data.ServerId,
  };
}

/** 清除本地会话（不调后端；后端会话随令牌过期自然失效）。 */
export function logout(): void {
  setAccessToken(null);
}
