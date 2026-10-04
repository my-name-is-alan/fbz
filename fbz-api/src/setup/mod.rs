//! 首次初始化（startup wizard）后端面。
//!
//! 提供两个**无认证**端点（开机即可访问，取代前端 localStorage 的初始化标志）：
//! - `GET /api/setup/status`：返回 `{ initialized }`，`initialized = 已存在任意用户`。
//! - `POST /api/setup`：**锁定式**——仅当用户数为 0 时可建首个 Owner 管理员，否则 409。
//!
//! 瘦控制器在 [`routes`]，初始化判定与锁定建管理员的事务逻辑在 [`service`]。
//! env 变量 bootstrap（[`crate::auth::bootstrap`]）保留作为无头部署的优先通道，二者共用
//! [`crate::auth::bootstrap::insert_owner_admin`]。

pub mod routes;
pub mod service;
