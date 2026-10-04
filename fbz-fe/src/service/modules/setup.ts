/**
 * 首次初始化（setup wizard）service：对接 fbz-api 的无认证 setup 端点。
 *
 * - `GET /api/setup/status`：开机判定是否已初始化（取代前端 localStorage 标志）。
 * - `POST /api/setup`：锁定式建首个 Owner 管理员；已初始化时后端返回 409。
 *
 * 两者都走 `/api` 面的 {@link request}（无需令牌）。
 */
import { request } from "@/service/request.ts";
import type { SetupPayload, SetupStatus } from "@/types/setup.ts";

/** 查询初始化状态。 */
export async function getSetupStatus(): Promise<SetupStatus> {
  const { data } = await request.get<SetupStatus>("/setup/status");
  return data;
}

/** 提交首个管理员账号完成初始化。已初始化时抛出 axios 409 错误，调用方据此给文案。 */
export async function submitSetup(payload: SetupPayload): Promise<SetupStatus> {
  const { data } = await request.post<SetupStatus>("/setup", payload);
  return data;
}
