//! 音乐浏览 BFF 模块。
//!
//! 面向前端 SPA 的音乐库浏览端点：artist → album → track 三级下钻，输出对齐前端
//! `camelCase` 契约。与 navigation BFF 并列、与 Emby compat 层互不影响——Emby 客户端
//! 继续走 `/emby/*`（`compat/emby/routes/artists.rs` 等），Web UI 走 `/api/music/*`。
//!
//! 三层职责同 navigation：瘦控制器 [`routes`] 只鉴权/调 service/映射错误；编排与 DTO
//! 映射在 [`service`]；SQL 全部下沉在 [`crate::library::repository`]（本模块零新 SQL，
//! 复用 `list_user_artists` + `list_user_items` 的 parent_id/type 过滤）。

pub mod dto;
pub mod routes;
pub mod service;
