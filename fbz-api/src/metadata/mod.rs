pub mod cache;
pub mod nfo_match;
pub mod nfo_write;
pub mod provider;
pub mod remote_search;
pub mod service;
pub mod settings;
pub mod worker;
pub mod write;

// §6.4 / §6.6：库级源链配置面 + Match 接线（插件 SyncInvoker 优先、内置 fallback；
// 本地 NFO→Match 见 `nfo_match`）。
pub use crate::library::metadata_sources::{
    LibraryMatchStep, LibrarySourceChainPlan, NFO_MATCH_INTEGRATION, load_chain_plan_for_match,
    match_steps_from_chain,
};
