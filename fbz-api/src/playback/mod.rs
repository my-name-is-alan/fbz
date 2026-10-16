//! 播放决策与跳转相关纯逻辑（与 HTTP 路由解耦）。
//!
//! STRM 302 入口仍在 `compat/emby/routes/streaming`；鉴权网关逻辑不在此模块修改。

pub mod play_method;
