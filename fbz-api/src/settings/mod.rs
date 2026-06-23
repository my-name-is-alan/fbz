pub mod repository;

use serde_json::{Value, json};

use crate::config::Config;

#[derive(Clone, Debug, PartialEq)]
pub struct SettingDefinition {
    pub key: &'static str,
    pub value: Value,
    pub requires_restart: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SettingValidationError {
    pub key: &'static str,
    pub message: &'static str,
}

impl std::fmt::Display for SettingValidationError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "invalid setting {}: {}", self.key, self.message)
    }
}

impl std::error::Error for SettingValidationError {}

pub fn bootstrap_settings(
    config: &Config,
) -> Result<Vec<SettingDefinition>, SettingValidationError> {
    validate_url("server.public_base_url", &config.server.public_base_url)?;
    validate_url(
        "metadata.tmdb_api_base_url",
        &config.metadata.tmdb_api_base_url,
    )?;
    validate_url(
        "metadata.tmdb_image_base_url",
        &config.metadata.tmdb_image_base_url,
    )?;
    validate_url(
        "metadata.tvdb_api_base_url",
        &config.metadata.tvdb_api_base_url,
    )?;
    validate_url(
        "metadata.fanart_api_base_url",
        &config.metadata.fanart_api_base_url,
    )?;

    Ok(vec![
        SettingDefinition {
            key: "server.public_base_url",
            value: json!(config.server.public_base_url),
            requires_restart: false,
        },
        SettingDefinition {
            key: "metadata.providers",
            value: json!(config.metadata.providers),
            requires_restart: false,
        },
        SettingDefinition {
            key: "metadata.tmdb_api_base_url",
            value: json!(config.metadata.tmdb_api_base_url),
            requires_restart: false,
        },
        SettingDefinition {
            key: "metadata.tmdb_image_base_url",
            value: json!(config.metadata.tmdb_image_base_url),
            requires_restart: false,
        },
        SettingDefinition {
            key: "metadata.tvdb_api_base_url",
            value: json!(config.metadata.tvdb_api_base_url),
            requires_restart: false,
        },
        SettingDefinition {
            key: "metadata.fanart_api_base_url",
            value: json!(config.metadata.fanart_api_base_url),
            requires_restart: false,
        },
        SettingDefinition {
            key: "proxy.policy",
            value: json!(config.proxy.policy),
            requires_restart: false,
        },
        SettingDefinition {
            key: "proxy.no_proxy",
            value: json!(config.proxy.no_proxy),
            requires_restart: false,
        },
        SettingDefinition {
            key: "strm.allow_private_networks",
            value: json!(config.media.strm_allow_private_networks),
            requires_restart: false,
        },
        SettingDefinition {
            key: "strm.allowed_domains",
            value: json!(config.media.strm_allowed_domains),
            requires_restart: false,
        },
        SettingDefinition {
            key: "transcode.hardware_mode",
            value: json!(config.transcode.hardware_mode.as_str()),
            requires_restart: false,
        },
        SettingDefinition {
            key: "transcode.hardware_priority",
            value: json!(config.transcode.hardware_priority),
            requires_restart: false,
        },
        SettingDefinition {
            key: "transcode.software_fallback",
            value: json!(config.transcode.software_fallback),
            requires_restart: false,
        },
        SettingDefinition {
            key: "schedule.incremental_scan",
            value: json!(config.schedules.incremental_scan),
            requires_restart: false,
        },
        SettingDefinition {
            key: "schedule.full_scan",
            value: json!(config.schedules.full_scan),
            requires_restart: false,
        },
        SettingDefinition {
            key: "schedule.metadata_refresh",
            value: json!(config.schedules.metadata_refresh),
            requires_restart: false,
        },
        SettingDefinition {
            key: "schedule.transcode_cleanup",
            value: json!(config.schedules.transcode_cleanup),
            requires_restart: false,
        },
        SettingDefinition {
            key: "schedule.session_cleanup",
            value: json!(config.schedules.session_cleanup),
            requires_restart: false,
        },
    ])
}

fn validate_url(key: &'static str, value: &str) -> Result<(), SettingValidationError> {
    if value.starts_with("http://") || value.starts_with("https://") {
        return Ok(());
    }

    Err(SettingValidationError {
        key,
        message: "must start with http:// or https://",
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::Config;

    #[test]
    fn bootstrap_settings_include_admin_editable_runtime_values() {
        let settings = bootstrap_settings(&Config::default()).unwrap();
        let keys = settings
            .iter()
            .map(|setting| setting.key)
            .collect::<Vec<_>>();

        assert!(keys.contains(&"server.public_base_url"));
        assert!(keys.contains(&"metadata.tmdb_api_base_url"));
        assert!(keys.contains(&"strm.allowed_domains"));
        assert!(keys.contains(&"transcode.hardware_mode"));
        assert!(keys.contains(&"schedule.incremental_scan"));
    }
}
