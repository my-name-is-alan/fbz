//! 导航 / 首页 BFF 模块。
//!
//! 面向前端 SPA 的聚合端点：把分散在 Emby compat 层的「可见库 / 最新入库 / 继续观看」
//! 一次性聚合成对齐前端 `camelCase` 契约的响应，前端首屏只需一次 `GET /api/navigation`。
//! 与 Emby compat 层并存、互不影响——Emby 客户端继续走 `/emby/*`，Web UI 走 `/api/*`。

pub mod access;
pub mod dto;
pub mod routes;
pub mod service;
