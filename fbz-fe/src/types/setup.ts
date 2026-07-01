/** 首次初始化（setup wizard）相关类型，对齐后端 `setup/routes.rs`。 */

/** `GET /api/setup/status` 响应。 */
export interface SetupStatus {
  /** 是否已初始化（后端已存在任意用户）。 */
  initialized: boolean;
}

/** `POST /api/setup` 入参：建首个 Owner 管理员。 */
export interface SetupPayload {
  username: string;
  password: string;
}
