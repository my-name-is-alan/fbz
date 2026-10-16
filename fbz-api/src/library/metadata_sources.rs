//! 库级 Match 刮削源链（配置面 + Match 接线，§6.4 / §5.9 / §6.6 / §10）。
//!
//! - 持久化 NFO 槽 + Provider 引用的开关与优先级。
//! - Match 刷新经 [`load_chain_plan_for_match`] / [`match_steps_from_chain`] 读链：
//!   已启用条目按 priority 编排；`provider` 优先走已启用官方插件 SyncInvoker，
//!   内置客户端仅作 fallback（过渡期）。
//! - 本地 NFO→Match：[`NFO_MATCH_INTEGRATION`]=`local`（读 sidecar；云盘 URI 跳过，仍走 storage importer）。
//! - 不进插件私有 KV；属 library 配置。

use sqlx::{Row, postgres::PgRow};

use crate::db::DbPool;

/// 本地 NFO 进 Match 的集成状态（配置面 API 回显；`local` = 读本地 sidecar）。
pub const NFO_MATCH_INTEGRATION: &str = "local";

/// 链上可作为 `provider` 引用的内置 provider id（不含 `nfo`：NFO 走独立槽）。
pub const LIBRARY_CHAIN_BUILTIN_PROVIDERS: &[&str] = &["tmdb", "tvdb", "imdb", "fanart", "spotify"];

/// 官方元数据插件 manifest id（不含 `plugin:` 前缀）与内置 provider id 对照。
/// 过渡期：库链写 `tmdb` 或 `plugin:fbz.provider.tmdb` 时，Match 都优先 SyncInvoker。
pub const OFFICIAL_PROVIDER_PLUGIN_BY_BUILTIN: &[(&str, &str)] = &[
    ("tmdb", "fbz.provider.tmdb"),
    ("tvdb", "fbz.provider.tvdb"),
    ("fanart", "fbz.provider.fanart"),
    ("imdb", "fbz.provider.imdb"),
    ("douban", "fbz.provider.douban"),
];

/// 内置 id → 官方插件 manifest id。
pub fn official_plugin_id_for_builtin(builtin: &str) -> Option<&'static str> {
    let key = builtin.trim().to_ascii_lowercase();
    OFFICIAL_PROVIDER_PLUGIN_BY_BUILTIN
        .iter()
        .find(|(b, _)| *b == key)
        .map(|(_, p)| *p)
}

/// 官方插件 manifest id → 内置 fallback id（无内置则 `None`，如 douban）。
pub fn builtin_fallback_for_official_plugin(plugin_id: &str) -> Option<&'static str> {
    let key = plugin_id.trim().to_ascii_lowercase();
    OFFICIAL_PROVIDER_PLUGIN_BY_BUILTIN
        .iter()
        .find(|(_, p)| *p == key)
        .and_then(|(b, _)| {
            if LIBRARY_CHAIN_BUILTIN_PROVIDERS.contains(b) {
                Some(*b)
            } else {
                None
            }
        })
}

/// 从链上 `source_key`（`tmdb` / `plugin:fbz.provider.tmdb` / 社区 `plugin:…`）解析
/// （优先插件 id, 内置 fallback id）。
pub fn resolve_provider_source_key(source_key: &str) -> (Option<String>, Option<String>) {
    let key = source_key.trim().to_ascii_lowercase();
    if let Some(rest) = key.strip_prefix("plugin:") {
        let plugin_id = rest.trim();
        if plugin_id.is_empty() {
            return (None, None);
        }
        let builtin = builtin_fallback_for_official_plugin(plugin_id).map(str::to_owned);
        return (Some(plugin_id.to_owned()), builtin);
    }
    let prefer_plugin = official_plugin_id_for_builtin(&key).map(str::to_owned);
    // 裸 key：有官方插件映射或内置列表则带 fallback；其它仍当作内置 id 尝试
    // （registry 会记 unsupported skip，保持历史 settings 兼容）。
    let builtin = Some(key);
    (prefer_plugin, builtin)
}

/// Match 编排的一步（由库级源链展开；NFO 仅记 pending）。
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum LibraryMatchStep {
    /// 链上启用了 NFO 槽：运行时读本地 sidecar（见 `metadata::nfo_match`）。
    NfoPending,
    /// 在线 / 插件 Provider：优先 SyncInvoker，内置可选 fallback。
    Provider {
        /// 链上原始 source_key（审计用）。
        source_key: String,
        /// 不含 `plugin:` 前缀的插件 manifest id。
        prefer_plugin_id: Option<String>,
        /// 内置 registry id（`tmdb`…）；插件未命中时再试。
        builtin_fallback: Option<String>,
    },
}

/// 将库级源链展开为 Match 步骤（仅 enabled；已按 priority 升序）。
pub fn match_steps_from_chain(plan: &LibrarySourceChainPlan) -> Vec<LibraryMatchStep> {
    let mut steps = Vec::new();
    for entry in &plan.entries {
        if !entry.enabled {
            continue;
        }
        match entry.source_kind.as_str() {
            "nfo" => steps.push(LibraryMatchStep::NfoPending),
            "provider" => {
                let (prefer_plugin_id, builtin_fallback) =
                    resolve_provider_source_key(&entry.source_key);
                steps.push(LibraryMatchStep::Provider {
                    source_key: entry.source_key.clone(),
                    prefer_plugin_id,
                    builtin_fallback,
                });
            }
            _ => {}
        }
    }
    steps
}


/// 源条目种类。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SourceKind {
    /// Core 本地/缓存 NFO 槽（§6.6）。
    Nfo,
    /// 元数据 Provider（内置或 `plugin:{id}`）。
    Provider,
}

impl SourceKind {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Nfo => "nfo",
            Self::Provider => "provider",
        }
    }

    pub fn parse(raw: &str) -> Option<Self> {
        match raw.trim().to_ascii_lowercase().as_str() {
            "nfo" => Some(Self::Nfo),
            "provider" => Some(Self::Provider),
            _ => None,
        }
    }
}

/// 一条已持久化的库级源条目。
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LibraryMetadataSourceRecord {
    pub id: String,
    pub library_id: String,
    pub source_kind: String,
    pub source_key: String,
    pub enabled: bool,
    pub priority: i32,
}

/// 写入/替换时的一条源意图（尚无 public_id）。
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LibraryMetadataSourceInput {
    pub source_kind: SourceKind,
    pub source_key: String,
    pub enabled: bool,
    pub priority: i32,
}

/// Match 侧读取用的计划（配置面已可构造；NFO 进 Match 仍待接线）。
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LibrarySourceChainPlan {
    pub library_id: String,
    pub entries: Vec<LibraryMetadataSourceRecord>,
    /// 恒为 [`NFO_MATCH_INTEGRATION`]（当前 `"local"`）。
    pub nfo_match_integration: &'static str,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum LibraryMetadataSourceError {
    Validation(String),
}

impl std::fmt::Display for LibraryMetadataSourceError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Validation(msg) => write!(f, "{msg}"),
        }
    }
}

/// 按库类型给出创建库预填推荐链（§6.4：本地 NFO 优先 → 官方 Provider）。
/// 未安装/未启用的 Provider 仍可预填；Match 侧日后按全局 enabled 再过滤。
pub fn default_source_chain_for_library_type(
    library_type: &str,
) -> Vec<LibraryMetadataSourceInput> {
    let mut chain = vec![LibraryMetadataSourceInput {
        source_kind: SourceKind::Nfo,
        source_key: "nfo".to_owned(),
        enabled: true,
        priority: 10,
    }];

    match library_type.trim().to_ascii_lowercase().as_str() {
        "music" => {
            chain.push(LibraryMetadataSourceInput {
                source_kind: SourceKind::Provider,
                source_key: "spotify".to_owned(),
                enabled: true,
                priority: 20,
            });
        }
        "tvshows" => {
            // 剧集：TVDB 略靠前，再 TMDB / Fanart。
            chain.extend([
                LibraryMetadataSourceInput {
                    source_kind: SourceKind::Provider,
                    source_key: "tvdb".to_owned(),
                    enabled: true,
                    priority: 20,
                },
                LibraryMetadataSourceInput {
                    source_kind: SourceKind::Provider,
                    source_key: "tmdb".to_owned(),
                    enabled: true,
                    priority: 30,
                },
                LibraryMetadataSourceInput {
                    source_kind: SourceKind::Provider,
                    source_key: "fanart".to_owned(),
                    enabled: true,
                    priority: 40,
                },
            ]);
        }
        // movies / homevideos / mixed / livetv / 其它：NFO → TMDB → TVDB → Fanart
        _ => {
            chain.extend([
                LibraryMetadataSourceInput {
                    source_kind: SourceKind::Provider,
                    source_key: "tmdb".to_owned(),
                    enabled: true,
                    priority: 20,
                },
                LibraryMetadataSourceInput {
                    source_kind: SourceKind::Provider,
                    source_key: "tvdb".to_owned(),
                    enabled: true,
                    priority: 30,
                },
                LibraryMetadataSourceInput {
                    source_kind: SourceKind::Provider,
                    source_key: "fanart".to_owned(),
                    enabled: true,
                    priority: 40,
                },
            ]);
        }
    }

    chain
}

/// 校验并规范化一条写入意图。
pub fn validate_source_input(
    kind_raw: &str,
    key_raw: &str,
    enabled: bool,
    priority: i32,
) -> Result<LibraryMetadataSourceInput, LibraryMetadataSourceError> {
    let Some(kind) = SourceKind::parse(kind_raw) else {
        return Err(LibraryMetadataSourceError::Validation(
            "source kind must be `nfo` or `provider`".to_owned(),
        ));
    };
    let key = key_raw.trim().to_ascii_lowercase();
    if key.is_empty() || key.len() > 64 {
        return Err(LibraryMetadataSourceError::Validation(
            "source key must be 1 to 64 characters".to_owned(),
        ));
    }
    let shape_ok = key.bytes().all(|byte| {
        byte.is_ascii_lowercase() || byte.is_ascii_digit() || matches!(byte, b':' | b'_' | b'-')
    });
    let starts_ok = key
        .bytes()
        .next()
        .is_some_and(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit());
    if !shape_ok || !starts_ok {
        return Err(LibraryMetadataSourceError::Validation(
            "source key contains unsupported characters".to_owned(),
        ));
    }

    match kind {
        SourceKind::Nfo => {
            if key != "nfo" {
                return Err(LibraryMetadataSourceError::Validation(
                    "nfo slot source key must be `nfo`".to_owned(),
                ));
            }
        }
        SourceKind::Provider => {
            if key == "nfo" {
                return Err(LibraryMetadataSourceError::Validation(
                    "nfo is a dedicated slot, not a provider reference".to_owned(),
                ));
            }
            let known = LIBRARY_CHAIN_BUILTIN_PROVIDERS.contains(&key.as_str())
                || official_plugin_id_for_builtin(&key).is_some()
                || key.starts_with("plugin:");
            if !known {
                return Err(LibraryMetadataSourceError::Validation(format!(
                    "unknown metadata provider `{key}`"
                )));
            }
            if key.starts_with("plugin:") && key.len() <= "plugin:".len() {
                return Err(LibraryMetadataSourceError::Validation(
                    "plugin provider id must be non-empty after `plugin:`".to_owned(),
                ));
            }
        }
    }

    if !(0..=1_000_000).contains(&priority) {
        return Err(LibraryMetadataSourceError::Validation(
            "priority must be between 0 and 1000000".to_owned(),
        ));
    }

    Ok(LibraryMetadataSourceInput {
        source_kind: kind,
        source_key: key,
        enabled,
        priority,
    })
}

/// 校验整条替换链：无重复 (kind,key)、至多一个 NFO 槽。
pub fn validate_source_chain(
    entries: &[LibraryMetadataSourceInput],
) -> Result<(), LibraryMetadataSourceError> {
    let mut seen = std::collections::HashSet::new();
    let mut nfo_count = 0usize;
    for entry in entries {
        let token = (entry.source_kind.as_str(), entry.source_key.as_str());
        if !seen.insert(token) {
            return Err(LibraryMetadataSourceError::Validation(format!(
                "duplicate source `{}:{}`",
                entry.source_kind.as_str(),
                entry.source_key
            )));
        }
        if entry.source_kind == SourceKind::Nfo {
            nfo_count += 1;
        }
    }
    if nfo_count > 1 {
        return Err(LibraryMetadataSourceError::Validation(
            "at most one nfo slot is allowed".to_owned(),
        ));
    }
    Ok(())
}

#[derive(Clone)]
pub struct LibraryMetadataSourcesRepository {
    pool: DbPool,
}

impl LibraryMetadataSourcesRepository {
    pub fn new(pool: DbPool) -> Self {
        Self { pool }
    }

    /// 列出某库源链（priority 升序）。库不存在返回 `Ok(None)`。
    pub async fn list_for_library(
        &self,
        library_public_id: &str,
    ) -> Result<Option<Vec<LibraryMetadataSourceRecord>>, sqlx::Error> {
        let Some(library_id) = resolve_library_id(&self.pool, library_public_id).await? else {
            return Ok(None);
        };
        let rows = sqlx::query(
            r#"
            select s.public_id::text as id,
                   l.public_id::text as library_id,
                   s.source_kind,
                   s.source_key,
                   s.enabled,
                   s.priority
            from library_metadata_sources s
            join libraries l on l.id = s.library_id
            where s.library_id = $1
            order by s.priority asc, s.id asc
            "#,
        )
        .bind(library_id)
        .fetch_all(&self.pool)
        .await?;

        rows.into_iter()
            .map(LibraryMetadataSourceRecord::from_row)
            .collect::<Result<Vec<_>, _>>()
            .map(Some)
    }

    /// 全量替换某库源链（事务：删旧 + 插新）。库不存在返回 `Ok(None)`。
    pub async fn replace_for_library(
        &self,
        library_public_id: &str,
        entries: &[LibraryMetadataSourceInput],
    ) -> Result<Option<Vec<LibraryMetadataSourceRecord>>, sqlx::Error> {
        let mut tx = self.pool.begin().await?;
        let Some(library_row) = sqlx::query(
            r#"
            select id, public_id::text as public_id
            from libraries
            where public_id = case
                when $1::text ~* '^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$'
                then $1::uuid
                else null::uuid
            end
            "#,
        )
        .bind(library_public_id.trim())
        .fetch_optional(&mut *tx)
        .await?
        else {
            return Ok(None);
        };

        let library_row_id: i64 = library_row.try_get("id")?;
        let library_public: String = library_row.try_get("public_id")?;

        sqlx::query("delete from library_metadata_sources where library_id = $1")
            .bind(library_row_id)
            .execute(&mut *tx)
            .await?;

        for entry in entries {
            sqlx::query(
                r#"
                insert into library_metadata_sources (
                    library_id, source_kind, source_key, enabled, priority
                )
                values ($1, $2, $3, $4, $5)
                "#,
            )
            .bind(library_row_id)
            .bind(entry.source_kind.as_str())
            .bind(&entry.source_key)
            .bind(entry.enabled)
            .bind(entry.priority)
            .execute(&mut *tx)
            .await?;
        }

        let rows = sqlx::query(
            r#"
            select public_id::text as id,
                   $2::text as library_id,
                   source_kind,
                   source_key,
                   enabled,
                   priority
            from library_metadata_sources
            where library_id = $1
            order by priority asc, id asc
            "#,
        )
        .bind(library_row_id)
        .bind(&library_public)
        .fetch_all(&mut *tx)
        .await?;

        tx.commit().await?;

        rows.into_iter()
            .map(LibraryMetadataSourceRecord::from_row)
            .collect::<Result<Vec<_>, _>>()
            .map(Some)
    }

    /// 在已有事务内为新建库预填推荐源链（create_library 调用）。
    pub async fn seed_defaults_in_tx(
        tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
        library_row_id: i64,
        library_type: &str,
    ) -> Result<(), sqlx::Error> {
        let defaults = default_source_chain_for_library_type(library_type);
        for entry in defaults {
            sqlx::query(
                r#"
                insert into library_metadata_sources (
                    library_id, source_kind, source_key, enabled, priority
                )
                values ($1, $2, $3, $4, $5)
                on conflict (library_id, source_kind, source_key) do nothing
                "#,
            )
            .bind(library_row_id)
            .bind(entry.source_kind.as_str())
            .bind(&entry.source_key)
            .bind(entry.enabled)
            .bind(entry.priority)
            .execute(&mut **tx)
            .await?;
        }
        Ok(())
    }
}

impl LibraryMetadataSourceRecord {
    fn from_row(row: PgRow) -> Result<Self, sqlx::Error> {
        Ok(Self {
            id: row.try_get("id")?,
            library_id: row.try_get("library_id")?,
            source_kind: row.try_get("source_kind")?,
            source_key: row.try_get("source_key")?,
            enabled: row.try_get("enabled")?,
            priority: row.try_get("priority")?,
        })
    }
}

async fn resolve_library_id(
    pool: &DbPool,
    library_public_id: &str,
) -> Result<Option<i64>, sqlx::Error> {
    sqlx::query_scalar(
        r#"
        select id
        from libraries
        where public_id = case
            when $1::text ~* '^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$'
            then $1::uuid
            else null::uuid
        end
        "#,
    )
    .bind(library_public_id.trim())
    .fetch_optional(pool)
    .await
}

/// Match 刷新作业读取库级源链的入口。
///
/// 调用方用 [`match_steps_from_chain`] 展开为编排步骤（插件优先、内置 fallback）。
/// NFO：仍返回 `nfo_match_integration = pending`；步骤里对启用的 NFO 槽记 skipped，
/// 配置面只读写链；实际 `.nfo` I/O 在 Match 运行时（`nfo_match`）。
pub async fn load_chain_plan_for_match(
    pool: &DbPool,
    library_public_id: &str,
) -> Result<Option<LibrarySourceChainPlan>, sqlx::Error> {
    let repo = LibraryMetadataSourcesRepository::new(pool.clone());
    let Some(entries) = repo.list_for_library(library_public_id).await? else {
        return Ok(None);
    };
    Ok(Some(LibrarySourceChainPlan {
        library_id: library_public_id.trim().to_owned(),
        entries,
        nfo_match_integration: NFO_MATCH_INTEGRATION,
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_chain_movies_puts_nfo_first() {
        let chain = default_source_chain_for_library_type("movies");
        assert_eq!(chain[0].source_kind, SourceKind::Nfo);
        assert_eq!(chain[0].source_key, "nfo");
        assert!(chain[0].enabled);
        assert_eq!(
            chain
                .iter()
                .map(|e| e.source_key.as_str())
                .collect::<Vec<_>>(),
            vec!["nfo", "tmdb", "tvdb", "fanart"]
        );
    }

    #[test]
    fn default_chain_tvshows_prefers_tvdb() {
        let chain = default_source_chain_for_library_type("tvshows");
        assert_eq!(
            chain
                .iter()
                .map(|e| e.source_key.as_str())
                .collect::<Vec<_>>(),
            vec!["nfo", "tvdb", "tmdb", "fanart"]
        );
    }

    #[test]
    fn default_chain_music_uses_spotify() {
        let chain = default_source_chain_for_library_type("music");
        assert_eq!(
            chain
                .iter()
                .map(|e| e.source_key.as_str())
                .collect::<Vec<_>>(),
            vec!["nfo", "spotify"]
        );
    }

    #[test]
    fn validate_rejects_nfo_as_provider() {
        let err = validate_source_input("provider", "nfo", true, 10).unwrap_err();
        assert!(err.to_string().contains("dedicated slot"));
    }

    #[test]
    fn validate_accepts_plugin_provider() {
        let ok = validate_source_input("provider", "plugin:acme-scraper", true, 50).unwrap();
        assert_eq!(ok.source_key, "plugin:acme-scraper");
        assert_eq!(ok.source_kind, SourceKind::Provider);
    }

    #[test]
    fn validate_chain_rejects_duplicates() {
        let a = validate_source_input("nfo", "nfo", true, 10).unwrap();
        let b = validate_source_input("provider", "tmdb", true, 20).unwrap();
        let c = validate_source_input("provider", "tmdb", false, 30).unwrap();
        let err = validate_source_chain(&[a, b, c]).unwrap_err();
        assert!(err.to_string().contains("duplicate"));
    }

    #[test]
    fn nfo_match_integration_is_local() {
        assert_eq!(NFO_MATCH_INTEGRATION, "local");
    }

    #[test]
    fn resolve_tmdb_key_prefers_official_plugin() {
        let (plugin, builtin) = resolve_provider_source_key("tmdb");
        assert_eq!(plugin.as_deref(), Some("fbz.provider.tmdb"));
        assert_eq!(builtin.as_deref(), Some("tmdb"));
    }

    #[test]
    fn resolve_plugin_key_keeps_builtin_fallback() {
        let (plugin, builtin) = resolve_provider_source_key("plugin:fbz.provider.tvdb");
        assert_eq!(plugin.as_deref(), Some("fbz.provider.tvdb"));
        assert_eq!(builtin.as_deref(), Some("tvdb"));
    }

    #[test]
    fn resolve_community_plugin_has_no_builtin() {
        let (plugin, builtin) = resolve_provider_source_key("plugin:acme.scraper");
        assert_eq!(plugin.as_deref(), Some("acme.scraper"));
        assert!(builtin.is_none());
    }

    #[test]
    fn match_steps_skip_disabled_and_mark_nfo_pending() {
        let plan = LibrarySourceChainPlan {
            library_id: "lib-1".to_owned(),
            entries: vec![
                LibraryMetadataSourceRecord {
                    id: "1".to_owned(),
                    library_id: "lib-1".to_owned(),
                    source_kind: "nfo".to_owned(),
                    source_key: "nfo".to_owned(),
                    enabled: true,
                    priority: 10,
                },
                LibraryMetadataSourceRecord {
                    id: "2".to_owned(),
                    library_id: "lib-1".to_owned(),
                    source_kind: "provider".to_owned(),
                    source_key: "tmdb".to_owned(),
                    enabled: false,
                    priority: 20,
                },
                LibraryMetadataSourceRecord {
                    id: "3".to_owned(),
                    library_id: "lib-1".to_owned(),
                    source_kind: "provider".to_owned(),
                    source_key: "plugin:fbz.provider.tvdb".to_owned(),
                    enabled: true,
                    priority: 30,
                },
            ],
            nfo_match_integration: NFO_MATCH_INTEGRATION,
        };
        let steps = match_steps_from_chain(&plan);
        assert_eq!(steps.len(), 2);
        assert_eq!(steps[0], LibraryMatchStep::NfoPending);
        assert_eq!(
            steps[1],
            LibraryMatchStep::Provider {
                source_key: "plugin:fbz.provider.tvdb".to_owned(),
                prefer_plugin_id: Some("fbz.provider.tvdb".to_owned()),
                builtin_fallback: Some("tvdb".to_owned()),
            }
        );
    }
}
