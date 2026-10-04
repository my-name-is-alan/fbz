//! Metadata provider settings: admin-configurable, DB-persisted overrides on
//! top of the environment-variable baseline.
//!
//! Runtime config resolution = environment defaults (baseline) ← DB rows
//! (admin overrides). The DB layer (repository + secret decryption) produces a
//! [`ResolvedMetadataSettings`] bundle; the pure [`resolve_metadata_config`]
//! folds it over the env [`MetadataConfig`]. Keeping the merge pure makes the
//! precedence rules unit-testable without a live database.
//!
//! See `docs/plans/metadata-scraper-design.md` §5.

use std::collections::HashMap;

use sqlx::Row;
use sqlx::postgres::PgRow;
use tracing::warn;

use crate::config::MetadataConfig;
use crate::db::DbPool;
use crate::notifications::secrets::{SecretCipher, SecretError};

/// Encryption scope for metadata provider secrets (AAD prefix).
pub const METADATA_SECRET_SCOPE: &str = "metadata-provider";
/// The single secret key stored per provider (its API key / token).
pub const METADATA_SECRET_KEY: &str = "api_key";
/// Built-in provider ids the admin surface understands.
pub const SUPPORTED_PROVIDER_IDS: &[&str] = &["tmdb", "tvdb", "imdb", "fanart", "nfo"];

/// 多 key 令牌池在单条密文里的分隔符：一个 provider 可配多个 API key，存储时用换行
/// join 成一个值加密（不改 schema），resolve 后 split 回 `Vec<String>`。API key 不含换行，
/// 换行是安全分隔符；空白段会被过滤。单 key（无换行）= len 1 的 Vec = 退化为原行为。
const METADATA_KEY_SEPARATOR: char = '\n';

/// 把多个 key join 成单条存储值（空白 key 过滤、去重保序）。
pub fn join_provider_keys(keys: &[String]) -> String {
    let mut seen = std::collections::HashSet::new();
    keys.iter()
        .map(|k| k.trim())
        .filter(|k| !k.is_empty())
        .filter(|k| seen.insert(k.to_owned()))
        .collect::<Vec<_>>()
        .join(&METADATA_KEY_SEPARATOR.to_string())
}

/// 从单条存储值 split 回多个 key（空白段过滤）。
pub fn split_provider_keys(value: &str) -> Vec<String> {
    value
        .split(METADATA_KEY_SEPARATOR)
        .map(str::trim)
        .filter(|k| !k.is_empty())
        .map(str::to_owned)
        .collect()
}

/// Global metadata defaults (single DB row, id = 1).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct MetadataGlobalSettings {
    pub provider_order: Vec<String>,
    pub default_language: Option<String>,
    pub default_country: Option<String>,
    pub image_language: Option<String>,
    pub image_prefer_original: bool,
    pub image_fallback_languages: Vec<String>,
}

/// Per-provider override row. All override fields are `Option`: `None` means
/// "inherit the global default / provider built-in".
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MetadataProviderSettings {
    pub provider_id: String,
    pub enabled: bool,
    pub api_base_url: Option<String>,
    pub image_base_url: Option<String>,
    pub proxy_mode: String,
    pub proxy_url: Option<String>,
    pub language: Option<String>,
    pub country: Option<String>,
    pub image_language: Option<String>,
    pub image_prefer_original: Option<bool>,
}

impl MetadataProviderSettings {
    /// A fresh row with the DB defaults (enabled, proxy inherit, no overrides).
    pub fn new(provider_id: impl Into<String>) -> Self {
        Self {
            provider_id: provider_id.into(),
            enabled: true,
            api_base_url: None,
            image_base_url: None,
            proxy_mode: "inherit".to_owned(),
            proxy_url: None,
            language: None,
            country: None,
            image_language: None,
            image_prefer_original: None,
        }
    }
}

/// Everything the runtime needs from the DB, with secrets already decrypted.
#[derive(Clone, Debug, Default)]
pub struct ResolvedMetadataSettings {
    pub global: Option<MetadataGlobalSettings>,
    pub providers: HashMap<String, MetadataProviderSettings>,
    /// provider_id -> 解密后的 API key 令牌池（多 key 轮转用；单 key 即 len 1）。
    pub keys: HashMap<String, Vec<String>>,
}

/// Errors surfaced by the settings repository / admin handlers.
#[derive(Debug)]
pub enum MetadataSettingsError {
    Database(sqlx::Error),
    Secret(SecretError),
    Validation(String),
}

impl std::fmt::Display for MetadataSettingsError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Database(err) => write!(f, "database error: {err}"),
            Self::Secret(err) => write!(f, "{err}"),
            Self::Validation(message) => write!(f, "{message}"),
        }
    }
}

impl std::error::Error for MetadataSettingsError {}

impl From<sqlx::Error> for MetadataSettingsError {
    fn from(error: sqlx::Error) -> Self {
        Self::Database(error)
    }
}

impl From<SecretError> for MetadataSettingsError {
    fn from(error: SecretError) -> Self {
        Self::Secret(error)
    }
}

/// Repository over the three `metadata_*` tables from migration 0077.
#[derive(Clone)]
pub struct MetadataSettingsRepository {
    pool: DbPool,
}

impl MetadataSettingsRepository {
    pub fn new(pool: DbPool) -> Self {
        Self { pool }
    }

    /// Loads the single global-settings row, if present.
    pub async fn load_global(&self) -> Result<Option<MetadataGlobalSettings>, sqlx::Error> {
        sqlx::query(
            r#"
            select provider_order,
                   default_language,
                   default_country,
                   image_language,
                   image_prefer_original,
                   image_fallback_languages
            from metadata_global_settings
            where id = 1
            "#,
        )
        .fetch_optional(&self.pool)
        .await?
        .map(global_settings_from_row)
        .transpose()
    }

    /// Writes (upserts) the global-settings row.
    pub async fn upsert_global(
        &self,
        settings: &MetadataGlobalSettings,
    ) -> Result<(), sqlx::Error> {
        sqlx::query(
            r#"
            insert into metadata_global_settings (
                id,
                provider_order,
                default_language,
                default_country,
                image_language,
                image_prefer_original,
                image_fallback_languages,
                updated_at
            )
            values (1, $1, $2, $3, $4, $5, $6, now())
            on conflict (id) do update
            set provider_order = excluded.provider_order,
                default_language = excluded.default_language,
                default_country = excluded.default_country,
                image_language = excluded.image_language,
                image_prefer_original = excluded.image_prefer_original,
                image_fallback_languages = excluded.image_fallback_languages,
                updated_at = now()
            "#,
        )
        .bind(&settings.provider_order)
        .bind(settings.default_language.as_deref())
        .bind(settings.default_country.as_deref())
        .bind(settings.image_language.as_deref())
        .bind(settings.image_prefer_original)
        .bind(&settings.image_fallback_languages)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    /// Loads all per-provider override rows, keyed by provider id.
    pub async fn load_all_providers(
        &self,
    ) -> Result<HashMap<String, MetadataProviderSettings>, sqlx::Error> {
        let rows = sqlx::query(
            r#"
            select provider_id,
                   enabled,
                   api_base_url,
                   image_base_url,
                   proxy_mode,
                   proxy_url,
                   language,
                   country,
                   image_language,
                   image_prefer_original
            from metadata_provider_settings
            "#,
        )
        .fetch_all(&self.pool)
        .await?;

        let mut providers = HashMap::with_capacity(rows.len());
        for row in rows {
            let settings = provider_settings_from_row(row)?;
            providers.insert(settings.provider_id.clone(), settings);
        }
        Ok(providers)
    }

    /// Writes (upserts) a per-provider override row.
    pub async fn upsert_provider(
        &self,
        settings: &MetadataProviderSettings,
    ) -> Result<(), sqlx::Error> {
        sqlx::query(
            r#"
            insert into metadata_provider_settings (
                provider_id,
                enabled,
                api_base_url,
                image_base_url,
                proxy_mode,
                proxy_url,
                language,
                country,
                image_language,
                image_prefer_original,
                updated_at
            )
            values ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, now())
            on conflict (provider_id) do update
            set enabled = excluded.enabled,
                api_base_url = excluded.api_base_url,
                image_base_url = excluded.image_base_url,
                proxy_mode = excluded.proxy_mode,
                proxy_url = excluded.proxy_url,
                language = excluded.language,
                country = excluded.country,
                image_language = excluded.image_language,
                image_prefer_original = excluded.image_prefer_original,
                updated_at = now()
            "#,
        )
        .bind(settings.provider_id.trim())
        .bind(settings.enabled)
        .bind(settings.api_base_url.as_deref())
        .bind(settings.image_base_url.as_deref())
        .bind(settings.proxy_mode.trim())
        .bind(settings.proxy_url.as_deref())
        .bind(settings.language.as_deref())
        .bind(settings.country.as_deref())
        .bind(settings.image_language.as_deref())
        .bind(settings.image_prefer_original)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    /// Encrypts and stores (upserts) a provider's API key.
    pub async fn set_secret(
        &self,
        provider_id: &str,
        value: &str,
        cipher: &SecretCipher,
    ) -> Result<(), MetadataSettingsError> {
        self.set_secrets(provider_id, std::slice::from_ref(&value.to_owned()), cipher)
            .await
            .map(|_| ())
    }

    /// 存储一个 provider 的多 key 令牌池（join 成单条密文，不改 schema）。
    /// 空白/重复 key 过滤去重，返回实际存储的 key 数。空集合返回 `Client` 错误。
    pub async fn set_secrets(
        &self,
        provider_id: &str,
        keys: &[String],
        cipher: &SecretCipher,
    ) -> Result<usize, MetadataSettingsError> {
        let provider_id = provider_id.trim();
        let joined = join_provider_keys(keys);
        if joined.is_empty() {
            return Err(MetadataSettingsError::Validation(
                "at least one non-empty key is required".to_owned(),
            ));
        }
        let stored_count = split_provider_keys(&joined).len();
        let encrypted = cipher.encrypt_scoped(
            METADATA_SECRET_SCOPE,
            provider_id,
            METADATA_SECRET_KEY,
            &joined,
        )?;
        sqlx::query(
            r#"
            insert into metadata_provider_secrets (
                provider_id,
                secret_key,
                algorithm,
                nonce,
                ciphertext,
                value_hash
            )
            values ($1, $2, $3, $4, $5, $6)
            on conflict (provider_id, secret_key) do update
            set algorithm = excluded.algorithm,
                nonce = excluded.nonce,
                ciphertext = excluded.ciphertext,
                value_hash = excluded.value_hash,
                updated_at = now()
            "#,
        )
        .bind(provider_id)
        .bind(METADATA_SECRET_KEY)
        .bind(encrypted.algorithm)
        .bind(encrypted.nonce)
        .bind(encrypted.ciphertext)
        .bind(encrypted.value_hash)
        .execute(&self.pool)
        .await?;
        Ok(stored_count)
    }

    /// Deletes a provider's stored API key. Returns whether a row was removed.
    pub async fn delete_secret(&self, provider_id: &str) -> Result<bool, sqlx::Error> {
        let result = sqlx::query(
            r#"
            delete from metadata_provider_secrets
            where provider_id = $1
              and secret_key = $2
            "#,
        )
        .bind(provider_id.trim())
        .bind(METADATA_SECRET_KEY)
        .execute(&self.pool)
        .await?;
        Ok(result.rows_affected() > 0)
    }

    /// Returns the set of provider ids that currently have a stored key.
    pub async fn providers_with_key(&self) -> Result<Vec<String>, sqlx::Error> {
        let rows = sqlx::query(
            r#"
            select provider_id
            from metadata_provider_secrets
            where secret_key = $1
            "#,
        )
        .bind(METADATA_SECRET_KEY)
        .fetch_all(&self.pool)
        .await?;
        rows.into_iter()
            .map(|row| row.try_get::<String, _>("provider_id"))
            .collect()
    }

    /// Loads the full settings bundle, decrypting secrets when a cipher is
    /// available. Without a cipher (no `FBZ_SECRET_KEY`), keys are skipped and
    /// the runtime falls back to environment credentials.
    pub async fn resolve(
        &self,
        cipher: Option<&SecretCipher>,
    ) -> Result<ResolvedMetadataSettings, MetadataSettingsError> {
        let global = self.load_global().await?;
        let providers = self.load_all_providers().await?;

        let mut keys = HashMap::new();
        if let Some(cipher) = cipher {
            let rows = sqlx::query(
                r#"
                select provider_id, nonce, ciphertext
                from metadata_provider_secrets
                where secret_key = $1
                "#,
            )
            .bind(METADATA_SECRET_KEY)
            .fetch_all(&self.pool)
            .await?;

            for row in rows {
                let provider_id: String = row.try_get("provider_id")?;
                let nonce: Vec<u8> = row.try_get("nonce")?;
                let ciphertext: Vec<u8> = row.try_get("ciphertext")?;
                // A single undecryptable secret (key rotation without re-encrypt,
                // corrupted row) must not crash resolution for every provider:
                // skip it and warn, leaving that provider keyless (degraded) while
                // all other providers resolve normally.
                match cipher.decrypt_scoped(
                    METADATA_SECRET_SCOPE,
                    &provider_id,
                    METADATA_SECRET_KEY,
                    &nonce,
                    &ciphertext,
                ) {
                    Ok(value) => {
                        // 多 key：解密值按分隔符 split 成令牌池（单 key = len 1）。
                        let pool = split_provider_keys(&value);
                        if !pool.is_empty() {
                            keys.insert(provider_id, pool);
                        }
                    }
                    Err(err) => {
                        warn!(
                            provider_id = %provider_id,
                            error = %err,
                            "skipping undecryptable metadata provider secret"
                        );
                    }
                }
            }
        }

        Ok(ResolvedMetadataSettings {
            global,
            providers,
            keys,
        })
    }
}

/// Folds DB overrides over the environment baseline to produce the effective
/// [`MetadataConfig`]. Pure: no I/O, fully unit-testable.
///
/// Precedence (per the design §5.1): environment defaults are the baseline; DB
/// rows override field-by-field when present. Provider order comes from the
/// global row when non-empty, else the env order; providers explicitly disabled
/// in the DB are removed from the effective order.
pub fn resolve_metadata_config(
    base: &MetadataConfig,
    resolved: &ResolvedMetadataSettings,
) -> MetadataConfig {
    let mut order = resolved
        .global
        .as_ref()
        .map(|global| global.provider_order.clone())
        .filter(|order| !order.is_empty())
        .unwrap_or_else(|| base.providers.clone());
    order.retain(|provider| {
        resolved
            .providers
            .get(&provider.trim().to_ascii_lowercase())
            .map(|settings| settings.enabled)
            .unwrap_or(true)
    });

    let provider = |id: &str| resolved.providers.get(id);
    // MetadataConfig 保持单 token 形态（向后兼容 + env 基线）：取池里第一个 key。
    // 完整令牌池经 ResolvedMetadataSettings.keys 暴露给 provider 轮转消费。
    let key = |id: &str| resolved.keys.get(id).and_then(|pool| pool.first().cloned());

    MetadataConfig {
        providers: order,
        tmdb_access_token: key("tmdb").or_else(|| base.tmdb_access_token.clone()),
        tmdb_api_base_url: provider("tmdb")
            .and_then(|settings| settings.api_base_url.clone())
            .unwrap_or_else(|| base.tmdb_api_base_url.clone()),
        tmdb_image_base_url: provider("tmdb")
            .and_then(|settings| settings.image_base_url.clone())
            .unwrap_or_else(|| base.tmdb_image_base_url.clone()),
        tvdb_api_key: key("tvdb").or_else(|| base.tvdb_api_key.clone()),
        tvdb_api_base_url: provider("tvdb")
            .and_then(|settings| settings.api_base_url.clone())
            .unwrap_or_else(|| base.tvdb_api_base_url.clone()),
        fanart_api_key: key("fanart").or_else(|| base.fanart_api_key.clone()),
        fanart_api_base_url: provider("fanart")
            .and_then(|settings| settings.api_base_url.clone())
            .unwrap_or_else(|| base.fanart_api_base_url.clone()),
        // Spotify：client_secret 可经 DB key 体系加密存（provider id = "spotify"）；
        // client_id 与 base url 走 env 基线 + provider 覆盖。
        spotify_client_id: base.spotify_client_id.clone(),
        spotify_client_secret: key("spotify").or_else(|| base.spotify_client_secret.clone()),
        spotify_api_base_url: provider("spotify")
            .and_then(|settings| settings.api_base_url.clone())
            .unwrap_or_else(|| base.spotify_api_base_url.clone()),
        spotify_auth_url: base.spotify_auth_url.clone(),
    }
}

/// Masks a secret for display: keeps the last four characters, never the rest.
/// Short secrets collapse to a fixed mask so length isn't leaked.
pub fn mask_secret(value: &str) -> String {
    let trimmed = value.trim();
    let count = trimmed.chars().count();
    if count <= 4 {
        return "••••".to_owned();
    }
    let last4: String = trimmed.chars().skip(count - 4).collect();
    format!("••••{last4}")
}

/// Validates a provider id is a known built-in or a `plugin:{id}` reference.
pub fn validate_provider_id(provider_id: &str) -> Result<String, MetadataSettingsError> {
    let id = provider_id.trim().to_ascii_lowercase();
    if id.is_empty() || id.len() > 64 {
        return Err(MetadataSettingsError::Validation(
            "provider id must be 1 to 64 characters".to_owned(),
        ));
    }
    let valid_shape = id.bytes().all(|byte| {
        byte.is_ascii_lowercase() || byte.is_ascii_digit() || matches!(byte, b':' | b'_' | b'-')
    });
    let starts_ok = id
        .bytes()
        .next()
        .is_some_and(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit());
    if !valid_shape || !starts_ok {
        return Err(MetadataSettingsError::Validation(
            "provider id contains unsupported characters".to_owned(),
        ));
    }
    if SUPPORTED_PROVIDER_IDS.contains(&id.as_str()) || id.starts_with("plugin:") {
        Ok(id)
    } else {
        Err(MetadataSettingsError::Validation(format!(
            "unknown metadata provider `{id}`"
        )))
    }
}

/// Validates a per-provider settings row (proxy mode coherence, locale shape).
pub fn validate_provider_settings(
    settings: &MetadataProviderSettings,
) -> Result<(), MetadataSettingsError> {
    match settings.proxy_mode.trim() {
        "inherit" | "direct" => {
            if settings.proxy_url.is_some() {
                return Err(MetadataSettingsError::Validation(
                    "proxy_url is only allowed when proxy_mode is `custom`".to_owned(),
                ));
            }
        }
        "custom" => {
            if settings
                .proxy_url
                .as_deref()
                .map(|url| url.trim().is_empty())
                .unwrap_or(true)
            {
                return Err(MetadataSettingsError::Validation(
                    "proxy_mode `custom` requires a non-empty proxy_url".to_owned(),
                ));
            }
        }
        other => {
            return Err(MetadataSettingsError::Validation(format!(
                "invalid proxy_mode `{other}`"
            )));
        }
    }
    if let Some(country) = settings.country.as_deref() {
        if !is_valid_country(country) {
            return Err(MetadataSettingsError::Validation(
                "country must be a 2-letter uppercase code".to_owned(),
            ));
        }
    }
    Ok(())
}

fn is_valid_country(value: &str) -> bool {
    let value = value.trim();
    value.len() == 2 && value.bytes().all(|byte| byte.is_ascii_uppercase())
}

fn global_settings_from_row(row: PgRow) -> Result<MetadataGlobalSettings, sqlx::Error> {
    Ok(MetadataGlobalSettings {
        provider_order: row.try_get("provider_order")?,
        default_language: row.try_get("default_language")?,
        default_country: row.try_get("default_country")?,
        image_language: row.try_get("image_language")?,
        image_prefer_original: row.try_get("image_prefer_original")?,
        image_fallback_languages: row.try_get("image_fallback_languages")?,
    })
}

fn provider_settings_from_row(row: PgRow) -> Result<MetadataProviderSettings, sqlx::Error> {
    Ok(MetadataProviderSettings {
        provider_id: row.try_get("provider_id")?,
        enabled: row.try_get("enabled")?,
        api_base_url: row.try_get("api_base_url")?,
        image_base_url: row.try_get("image_base_url")?,
        proxy_mode: row.try_get("proxy_mode")?,
        proxy_url: row.try_get("proxy_url")?,
        language: row.try_get("language")?,
        country: row.try_get("country")?,
        image_language: row.try_get("image_language")?,
        image_prefer_original: row.try_get("image_prefer_original")?,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn base_config() -> MetadataConfig {
        MetadataConfig {
            providers: vec!["tmdb".to_owned(), "tvdb".to_owned(), "fanart".to_owned()],
            tmdb_access_token: Some("env-tmdb".to_owned()),
            tmdb_api_base_url: "https://api.themoviedb.org/3".to_owned(),
            tmdb_image_base_url: "https://image.tmdb.org/t/p".to_owned(),
            tvdb_api_key: None,
            tvdb_api_base_url: "https://api4.thetvdb.com/v4".to_owned(),
            fanart_api_key: None,
            fanart_api_base_url: "https://webservice.fanart.tv/v3".to_owned(),
            spotify_client_id: None,
            spotify_client_secret: None,
            spotify_api_base_url: "https://api.spotify.com/v1".to_owned(),
            spotify_auth_url: "https://accounts.spotify.com/api/token".to_owned(),
        }
    }

    #[test]
    fn empty_settings_pass_through_env_baseline() {
        let base = base_config();
        let resolved = ResolvedMetadataSettings::default();
        let effective = resolve_metadata_config(&base, &resolved);
        assert_eq!(effective, base);
    }

    #[test]
    fn db_key_overrides_env_and_db_base_url_overrides() {
        let base = base_config();
        let mut providers = HashMap::new();
        let mut tmdb = MetadataProviderSettings::new("tmdb");
        tmdb.api_base_url = Some("https://tmdb.mirror.test/3".to_owned());
        tmdb.image_base_url = Some("https://img.mirror.test".to_owned());
        providers.insert("tmdb".to_owned(), tmdb);
        let mut keys = HashMap::new();
        keys.insert("tmdb".to_owned(), vec!["db-tmdb".to_owned()]);
        keys.insert("tvdb".to_owned(), vec!["db-tvdb".to_owned()]);

        let resolved = ResolvedMetadataSettings {
            global: None,
            providers,
            keys,
        };
        let effective = resolve_metadata_config(&base, &resolved);

        // DB key wins over the env token.
        assert_eq!(effective.tmdb_access_token.as_deref(), Some("db-tmdb"));
        // DB key fills in where env had none.
        assert_eq!(effective.tvdb_api_key.as_deref(), Some("db-tvdb"));
        // DB base/image url overrides.
        assert_eq!(effective.tmdb_api_base_url, "https://tmdb.mirror.test/3");
        assert_eq!(effective.tmdb_image_base_url, "https://img.mirror.test");
        // Untouched provider keeps env defaults.
        assert_eq!(effective.fanart_api_base_url, base.fanart_api_base_url);
    }

    #[test]
    fn global_order_overrides_and_disabled_provider_is_dropped() {
        let base = base_config();
        let global = MetadataGlobalSettings {
            provider_order: vec!["tvdb".to_owned(), "tmdb".to_owned(), "fanart".to_owned()],
            ..MetadataGlobalSettings::default()
        };
        let mut providers = HashMap::new();
        let mut fanart = MetadataProviderSettings::new("fanart");
        fanart.enabled = false;
        providers.insert("fanart".to_owned(), fanart);

        let resolved = ResolvedMetadataSettings {
            global: Some(global),
            providers,
            keys: HashMap::new(),
        };
        let effective = resolve_metadata_config(&base, &resolved);

        // Global order wins, and the disabled fanart is filtered out.
        assert_eq!(
            effective.providers,
            vec!["tvdb".to_owned(), "tmdb".to_owned()]
        );
    }

    #[test]
    fn mask_secret_keeps_only_last_four() {
        assert_eq!(mask_secret("abcdef1234"), "••••1234");
        assert_eq!(mask_secret("xyz"), "••••");
        assert_eq!(mask_secret("    "), "••••");
    }

    #[test]
    fn provider_key_join_split_round_trips_and_dedups() {
        // 多 key join → split 往返。
        let keys = vec!["k1".to_owned(), "k2".to_owned(), "k3".to_owned()];
        let joined = join_provider_keys(&keys);
        assert_eq!(split_provider_keys(&joined), keys);

        // 空白过滤 + 去重保序。
        let messy = vec![
            " k1 ".to_owned(),
            "".to_owned(),
            "k2".to_owned(),
            "k1".to_owned(),
            "   ".to_owned(),
        ];
        assert_eq!(
            split_provider_keys(&join_provider_keys(&messy)),
            vec!["k1".to_owned(), "k2".to_owned()]
        );

        // 单 key（无分隔符）= len 1，退化为原行为。
        assert_eq!(split_provider_keys("single"), vec!["single".to_owned()]);
        // 全空白 → 空池。
        assert!(split_provider_keys("   ").is_empty());
        assert!(join_provider_keys(&["".to_owned()]).is_empty());
    }

    #[test]
    fn provider_id_validation_accepts_known_and_plugin_ids() {
        assert_eq!(validate_provider_id(" TMDB ").unwrap(), "tmdb");
        assert_eq!(validate_provider_id("plugin:acme").unwrap(), "plugin:acme");
        assert!(validate_provider_id("bogus").is_err());
        assert!(validate_provider_id("").is_err());
    }

    #[test]
    fn provider_settings_validation_enforces_proxy_coherence() {
        let mut settings = MetadataProviderSettings::new("tmdb");
        settings.proxy_mode = "custom".to_owned();
        assert!(validate_provider_settings(&settings).is_err());

        settings.proxy_url = Some("http://proxy.test:8080".to_owned());
        assert!(validate_provider_settings(&settings).is_ok());

        settings.proxy_mode = "direct".to_owned();
        assert!(validate_provider_settings(&settings).is_err());

        settings.proxy_url = None;
        assert!(validate_provider_settings(&settings).is_ok());

        settings.country = Some("us".to_owned());
        assert!(validate_provider_settings(&settings).is_err());
        settings.country = Some("US".to_owned());
        assert!(validate_provider_settings(&settings).is_ok());
    }

    // Live-DB smoke: validates migration 0077's three metadata tables and every
    // MetadataSettingsRepository sqlx query against the real migrated schema.
    // Until now these queries were only compile-checked (see
    // docs/plans/metadata-scraper-design.md §阶段 1 落地说明 "待 live-PG 校验").
    // The test writes to a `plugin:metadata-smoke` provider id that never
    // collides with real provider config, exercises the full round-trip
    // (global upsert/load, provider upsert/load, secret set/resolve/delete,
    // and resolve_metadata_config folding), then deletes everything it created
    // so it leaves the dev DB as it found it.
    //   cargo test -- --ignored metadata_settings_repository_executes_against_live_schema
    #[tokio::test]
    #[ignore = "requires a running PostgreSQL from ./scripts/dev-deps.ps1"]
    async fn metadata_settings_repository_executes_against_live_schema() {
        use sqlx::postgres::PgPoolOptions;

        let url = std::env::var("DATABASE_URL")
            .unwrap_or_else(|_| "postgres://fbz:fbz@127.0.0.1:5432/fbz".to_owned());
        let pool = PgPoolOptions::new()
            .max_connections(2)
            .connect(&url)
            .await
            .expect("connect to live PostgreSQL");
        crate::db::migrate(&pool).await.expect("run migrations");

        let repository = MetadataSettingsRepository::new(pool.clone());
        let cipher =
            SecretCipher::from_key_material("metadata-settings-smoke-key-0123456789abcdef")
                .expect("build secret cipher");
        let provider_id = "plugin:metadata-smoke";

        // Start from a clean slate for the rows this test owns (a prior aborted
        // run may have left them behind), but do not touch real provider rows.
        sqlx::query("delete from metadata_provider_secrets where provider_id = $1")
            .bind(provider_id)
            .execute(&pool)
            .await
            .expect("clear prior smoke secret");
        sqlx::query("delete from metadata_provider_settings where provider_id = $1")
            .bind(provider_id)
            .execute(&pool)
            .await
            .expect("clear prior smoke provider");

        // Capture the existing global row (if any) so we can restore it; the
        // global table is single-row so the test must not clobber real config.
        let prior_global = repository
            .load_global()
            .await
            .expect("load global should execute against the live schema");

        // --- global upsert + load round-trip ---
        let global = MetadataGlobalSettings {
            provider_order: vec!["tvdb".to_owned(), "tmdb".to_owned(), "fanart".to_owned()],
            default_language: Some("zh-CN".to_owned()),
            default_country: Some("CN".to_owned()),
            image_language: Some("ja".to_owned()),
            image_prefer_original: true,
            image_fallback_languages: vec!["en".to_owned(), "none".to_owned()],
        };
        repository
            .upsert_global(&global)
            .await
            .expect("upsert global should execute against the live schema");
        let loaded_global = repository
            .load_global()
            .await
            .expect("load global should execute against the live schema")
            .expect("global row should exist after upsert");
        assert_eq!(loaded_global, global, "global row should round-trip");

        // --- provider upsert + load round-trip ---
        let mut provider = MetadataProviderSettings::new(provider_id);
        provider.api_base_url = Some("https://mirror.smoke.test/3".to_owned());
        provider.image_base_url = Some("https://img.smoke.test".to_owned());
        provider.proxy_mode = "custom".to_owned();
        provider.proxy_url = Some("http://proxy.smoke.test:8080".to_owned());
        provider.language = Some("zh-CN".to_owned());
        provider.country = Some("CN".to_owned());
        provider.image_language = Some("ja".to_owned());
        provider.image_prefer_original = Some(true);
        repository
            .upsert_provider(&provider)
            .await
            .expect("upsert provider should execute against the live schema");
        let providers = repository
            .load_all_providers()
            .await
            .expect("load providers should execute against the live schema");
        assert_eq!(
            providers.get(provider_id),
            Some(&provider),
            "provider row should round-trip"
        );

        // --- secret set + encrypted resolve + delete round-trip ---
        repository
            .set_secret(provider_id, "smoke-secret-value", &cipher)
            .await
            .expect("set secret should execute against the live schema");
        let with_key = repository
            .providers_with_key()
            .await
            .expect("providers_with_key should execute against the live schema");
        assert!(
            with_key.iter().any(|id| id == provider_id),
            "provider should report a stored key"
        );
        let resolved = repository
            .resolve(Some(&cipher))
            .await
            .expect("resolve should execute against the live schema");
        assert_eq!(
            resolved.keys.get(provider_id).map(Vec::as_slice),
            Some(["smoke-secret-value".to_owned()].as_slice()),
            "resolve should decrypt the stored key into a single-key pool"
        );
        assert!(
            resolved.providers.contains_key(provider_id),
            "resolve should include the provider override"
        );

        // --- 多 key 令牌池往返：set_secrets 存 3 个 key → resolve 出 len 3 的池（顺序保持） ---
        let pool_keys = vec![
            "pool-k1".to_owned(),
            "pool-k2".to_owned(),
            "pool-k3".to_owned(),
        ];
        let stored = repository
            .set_secrets(provider_id, &pool_keys, &cipher)
            .await
            .expect("set_secrets should execute against the live schema");
        assert_eq!(stored, 3, "three keys should be stored");
        let resolved_pool = repository
            .resolve(Some(&cipher))
            .await
            .expect("resolve should execute against the live schema");
        assert_eq!(
            resolved_pool.keys.get(provider_id).map(Vec::as_slice),
            Some(pool_keys.as_slice()),
            "resolve should decrypt the stored multi-key pool preserving order"
        );

        // --- resolve_metadata_config folding over the env baseline ---
        let base = base_config();
        let effective = resolve_metadata_config(&base, &resolved);
        // The smoke provider is a plugin id, so it does not change the built-in
        // tmdb/tvdb/fanart fields, but folding must execute without panicking
        // and preserve the env baseline for untouched providers.
        assert_eq!(effective.tmdb_api_base_url, base.tmdb_api_base_url);

        let removed = repository
            .delete_secret(provider_id)
            .await
            .expect("delete secret should execute against the live schema");
        assert!(removed, "delete should report the key was removed");
        let after_delete = repository
            .resolve(Some(&cipher))
            .await
            .expect("resolve after delete should execute against the live schema");
        assert!(
            !after_delete.keys.contains_key(provider_id),
            "deleted key should no longer resolve"
        );

        // --- cleanup: leave the dev DB exactly as we found it ---
        sqlx::query("delete from metadata_provider_settings where provider_id = $1")
            .bind(provider_id)
            .execute(&pool)
            .await
            .expect("cleanup smoke provider");
        match prior_global {
            Some(prior) => repository
                .upsert_global(&prior)
                .await
                .expect("restore prior global row"),
            None => {
                sqlx::query("delete from metadata_global_settings where id = 1")
                    .execute(&pool)
                    .await
                    .expect("remove smoke global row");
            }
        }
    }

    // Live-DB smoke: validates the DB + fold half of the provider-test probe
    // chain (admin `POST /api/admin/metadata/providers/{id}/test`) against the
    // real migrated schema. That handler runs resolve(cipher) +
    // resolve_metadata_config and feeds the effective config to probe_provider;
    // the probe itself hits the network, so this asserts the DB-backed inputs
    // the probe would receive, not the HTTP call. Seeds a tmdb override + key,
    // resolves, folds, asserts the override wins, then restores prior state.
    //   cargo test -- --ignored metadata_provider_test_resolve_executes_against_live_schema
    #[tokio::test]
    #[ignore = "requires a running PostgreSQL from ./scripts/dev-deps.ps1"]
    async fn metadata_provider_test_resolve_executes_against_live_schema() {
        use sqlx::postgres::PgPoolOptions;

        let url = std::env::var("DATABASE_URL")
            .unwrap_or_else(|_| "postgres://fbz:fbz@127.0.0.1:5432/fbz".to_owned());
        let pool = PgPoolOptions::new()
            .max_connections(2)
            .connect(&url)
            .await
            .expect("connect to live PostgreSQL");
        crate::db::migrate(&pool).await.expect("run migrations");

        let repository = MetadataSettingsRepository::new(pool.clone());
        let cipher =
            SecretCipher::from_key_material("metadata-provider-test-smoke-key-0123456789abcdef")
                .expect("build secret cipher");

        // The probe is keyed by the real built-in provider id "tmdb". Capture and
        // restore any existing override/secret so we never clobber dev config.
        let prior_providers = repository
            .load_all_providers()
            .await
            .expect("load providers should execute against the live schema");
        let prior_tmdb = prior_providers.get("tmdb").cloned();

        let mut tmdb = MetadataProviderSettings::new("tmdb");
        tmdb.api_base_url = Some("https://tmdb.mirror.smoke.test/3".to_owned());
        repository
            .upsert_provider(&tmdb)
            .await
            .expect("upsert tmdb override should execute against the live schema");
        repository
            .set_secret("tmdb", "smoke-tmdb-token", &cipher)
            .await
            .expect("set tmdb secret should execute against the live schema");

        // resolve + fold: exactly what test_metadata_provider does before probing.
        let resolved = repository
            .resolve(Some(&cipher))
            .await
            .expect("resolve should execute against the live schema");
        let base = base_config();
        let effective = resolve_metadata_config(&base, &resolved);

        // The DB override + decrypted key win over the env baseline — these are
        // the inputs probe_provider would receive.
        assert_eq!(
            effective.tmdb_api_base_url,
            "https://tmdb.mirror.smoke.test/3"
        );
        assert_eq!(
            effective.tmdb_access_token.as_deref(),
            Some("smoke-tmdb-token")
        );

        // Restore prior state: delete our key, then restore or remove the row.
        repository
            .delete_secret("tmdb")
            .await
            .expect("delete smoke secret");
        match prior_tmdb {
            Some(prior) => repository
                .upsert_provider(&prior)
                .await
                .expect("restore prior tmdb override"),
            None => {
                sqlx::query("delete from metadata_provider_settings where provider_id = 'tmdb'")
                    .execute(&pool)
                    .await
                    .expect("remove smoke tmdb override");
            }
        }
    }
}
