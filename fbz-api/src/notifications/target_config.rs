use std::{
    error::Error,
    fmt::{Display, Formatter},
};

use reqwest::header::{HeaderMap, HeaderName, HeaderValue};
use serde_json::{Map, Value, json};

use crate::notifications::secrets::{TargetSecretInput, secret_ref};

pub const DEFAULT_TELEGRAM_API_BASE_URL: &str = "https://api.telegram.org";

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum NotificationTargetConfigError {
    InvalidConfig(String),
    InvalidHeader(String),
    UnsupportedTargetType(String),
}

#[derive(Clone, Debug, PartialEq)]
pub struct SecretizedTargetConfig {
    pub config: Value,
    pub secrets: Vec<TargetSecretInput>,
}

pub fn validate_target_config(
    target_type: &str,
    config: &Value,
) -> Result<(), NotificationTargetConfigError> {
    require_config_object(config)?;
    match target_type.trim() {
        "webhook" => {
            let url = required_config_string(config, "url")?;
            validate_delivery_url(&url)?;
            let _ = optional_webhook_headers(config)?;
            Ok(())
        }
        "telegram" => {
            let _ = required_config_string(config, "botToken")?;
            let _ = required_config_string(config, "chatId")?;
            let api_base_url = optional_config_string(config, "apiBaseUrl")
                .unwrap_or_else(|| DEFAULT_TELEGRAM_API_BASE_URL.to_owned());
            validate_delivery_url(&api_base_url)
        }
        "wecom" => {
            let url = required_config_string(config, "webhookUrl")?;
            validate_delivery_url(&url)
        }
        "plugin" => Err(NotificationTargetConfigError::UnsupportedTargetType(
            "plugin notification targets are reserved for a future plugin bridge".to_owned(),
        )),
        other => Err(NotificationTargetConfigError::UnsupportedTargetType(
            other.to_owned(),
        )),
    }
}

pub fn secretize_target_config(
    target_type: &str,
    config: &Value,
) -> Result<SecretizedTargetConfig, NotificationTargetConfigError> {
    validate_target_config(target_type, config)?;
    match target_type.trim() {
        "webhook" => secretize_webhook_config(config),
        "telegram" => secretize_telegram_config(config),
        "wecom" => secretize_wecom_config(config),
        other => Err(NotificationTargetConfigError::UnsupportedTargetType(
            other.to_owned(),
        )),
    }
}

pub fn redacted_target_config(target_type: &str, config: &Value) -> Value {
    match target_type.trim() {
        "webhook" => json!({
            "url": redact_if_present(config, "url"),
            "headers": redact_headers(config.get("headers")),
        }),
        "telegram" => json!({
            "apiBaseUrl": optional_config_string(config, "apiBaseUrl")
                .unwrap_or_else(|| DEFAULT_TELEGRAM_API_BASE_URL.to_owned()),
            "botToken": redact_if_present(config, "botToken"),
            "chatId": optional_config_string(config, "chatId"),
        }),
        "wecom" => json!({
            "webhookUrl": redact_if_present(config, "webhookUrl"),
        }),
        _ => json!({}),
    }
}

pub fn required_config_string(
    config: &Value,
    key: &'static str,
) -> Result<String, NotificationTargetConfigError> {
    optional_config_string(config, key).ok_or_else(|| {
        NotificationTargetConfigError::InvalidConfig(format!("target config `{key}` is required"))
    })
}

pub fn optional_config_string(config: &Value, key: &'static str) -> Option<String> {
    config
        .get(key)
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned)
}

pub fn optional_webhook_headers(
    config: &Value,
) -> Result<HeaderMap, NotificationTargetConfigError> {
    let mut headers = HeaderMap::new();
    let Some(value) = config.get("headers") else {
        return Ok(headers);
    };
    let object = value.as_object().ok_or_else(|| {
        NotificationTargetConfigError::InvalidConfig(
            "target config `headers` must be an object".to_owned(),
        )
    })?;
    if object.len() > 32 {
        return Err(NotificationTargetConfigError::InvalidConfig(
            "target config `headers` must contain at most 32 entries".to_owned(),
        ));
    }

    for (name, value) in object {
        let lower_name = name.to_ascii_lowercase();
        if matches!(lower_name.as_str(), "host" | "content-length") {
            return Err(NotificationTargetConfigError::InvalidHeader(format!(
                "header `{name}` cannot be overridden"
            )));
        }
        let header_name = HeaderName::from_bytes(name.as_bytes())
            .map_err(|err| NotificationTargetConfigError::InvalidHeader(err.to_string()))?;
        let header_value = value.as_str().ok_or_else(|| {
            NotificationTargetConfigError::InvalidHeader(format!(
                "header `{name}` must be a string"
            ))
        })?;
        if header_value.len() > 1024 {
            return Err(NotificationTargetConfigError::InvalidHeader(format!(
                "header `{name}` must be at most 1024 characters"
            )));
        }
        let header_value = HeaderValue::from_str(header_value)
            .map_err(|err| NotificationTargetConfigError::InvalidHeader(err.to_string()))?;
        headers.insert(header_name, header_value);
    }

    Ok(headers)
}

pub fn validate_delivery_url(value: &str) -> Result<(), NotificationTargetConfigError> {
    if value.len() > 2048 {
        return Err(NotificationTargetConfigError::InvalidConfig(
            "delivery URL must be at most 2048 characters".to_owned(),
        ));
    }
    if value.contains(char::is_whitespace) {
        return Err(NotificationTargetConfigError::InvalidConfig(
            "delivery URL must not contain whitespace".to_owned(),
        ));
    }
    if value.starts_with("http://") || value.starts_with("https://") {
        return Ok(());
    }
    Err(NotificationTargetConfigError::InvalidConfig(
        "delivery URL must start with http:// or https://".to_owned(),
    ))
}

fn require_config_object(config: &Value) -> Result<(), NotificationTargetConfigError> {
    if config.is_object() {
        return Ok(());
    }
    Err(NotificationTargetConfigError::InvalidConfig(
        "target config must be a JSON object".to_owned(),
    ))
}

fn redact_if_present(config: &Value, key: &'static str) -> Option<&'static str> {
    config.get(key).and_then(redact_value)
}

fn redact_headers(value: Option<&Value>) -> Option<Value> {
    let object = value?.as_object()?;
    let headers = object
        .keys()
        .map(|key| (key.clone(), Value::String("[redacted]".to_owned())))
        .collect::<Map<_, _>>();
    Some(Value::Object(headers))
}

fn redact_value(value: &Value) -> Option<&'static str> {
    if value
        .as_str()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .is_some()
    {
        return Some("[redacted]");
    }
    value.as_object()?.get("secretRef")?.as_str()?;
    Some("[redacted]")
}

fn secretize_webhook_config(
    config: &Value,
) -> Result<SecretizedTargetConfig, NotificationTargetConfigError> {
    let url = required_config_string(config, "url")?;
    let mut public_headers = Map::new();
    let mut secrets = vec![TargetSecretInput {
        key: "webhook.url".to_owned(),
        value: url,
    }];

    if let Some(headers) = config.get("headers").and_then(Value::as_object) {
        for (name, value) in headers {
            let header_value = value.as_str().ok_or_else(|| {
                NotificationTargetConfigError::InvalidHeader(format!(
                    "header `{name}` must be a string"
                ))
            })?;
            let key = format!("headers.{name}");
            public_headers.insert(name.clone(), secret_ref(&key));
            secrets.push(TargetSecretInput {
                key,
                value: header_value.trim().to_owned(),
            });
        }
    }

    let mut public_config = Map::new();
    public_config.insert("url".to_owned(), secret_ref("webhook.url"));
    if !public_headers.is_empty() {
        public_config.insert("headers".to_owned(), Value::Object(public_headers));
    }

    Ok(SecretizedTargetConfig {
        config: Value::Object(public_config),
        secrets,
    })
}

fn secretize_telegram_config(
    config: &Value,
) -> Result<SecretizedTargetConfig, NotificationTargetConfigError> {
    let bot_token = required_config_string(config, "botToken")?;
    let chat_id = required_config_string(config, "chatId")?;
    let mut public_config = Map::new();
    public_config.insert("botToken".to_owned(), secret_ref("telegram.botToken"));
    public_config.insert("chatId".to_owned(), Value::String(chat_id));
    if let Some(api_base_url) = optional_config_string(config, "apiBaseUrl") {
        public_config.insert("apiBaseUrl".to_owned(), Value::String(api_base_url));
    }

    Ok(SecretizedTargetConfig {
        config: Value::Object(public_config),
        secrets: vec![TargetSecretInput {
            key: "telegram.botToken".to_owned(),
            value: bot_token,
        }],
    })
}

fn secretize_wecom_config(
    config: &Value,
) -> Result<SecretizedTargetConfig, NotificationTargetConfigError> {
    let webhook_url = required_config_string(config, "webhookUrl")?;
    Ok(SecretizedTargetConfig {
        config: json!({
            "webhookUrl": secret_ref("wecom.webhookUrl")
        }),
        secrets: vec![TargetSecretInput {
            key: "wecom.webhookUrl".to_owned(),
            value: webhook_url,
        }],
    })
}

impl Display for NotificationTargetConfigError {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::InvalidConfig(message) => {
                write!(f, "invalid notification target config: {message}")
            }
            Self::InvalidHeader(message) => {
                write!(f, "invalid notification target header: {message}")
            }
            Self::UnsupportedTargetType(target_type) => {
                write!(f, "unsupported notification target type: {target_type}")
            }
        }
    }
}

impl Error for NotificationTargetConfigError {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn validates_supported_target_configs() {
        assert!(
            validate_target_config(
                "webhook",
                &json!({
                    "url": "https://notify.example.test/hook",
                    "headers": {
                        "x-api-key": "secret"
                    }
                })
            )
            .is_ok()
        );
        assert!(
            validate_target_config(
                "telegram",
                &json!({
                    "botToken": "token",
                    "chatId": "chat",
                    "apiBaseUrl": "https://api.telegram.org"
                })
            )
            .is_ok()
        );
        assert!(
            validate_target_config(
                "wecom",
                &json!({
                    "webhookUrl": "https://qyapi.weixin.qq.com/cgi-bin/webhook/send?key=secret"
                })
            )
            .is_ok()
        );
    }

    #[test]
    fn rejects_invalid_target_configs() {
        assert!(validate_target_config("webhook", &json!({})).is_err());
        assert!(
            validate_target_config(
                "webhook",
                &json!({
                    "url": "ftp://example.test/hook"
                })
            )
            .is_err()
        );
        assert!(
            validate_target_config(
                "webhook",
                &json!({
                    "url": "https://example.test/hook",
                    "headers": {
                        "host": "evil.example.test"
                    }
                })
            )
            .is_err()
        );
        assert!(validate_target_config("plugin", &json!({})).is_err());
    }

    #[test]
    fn redacts_sensitive_target_config() {
        let webhook = redacted_target_config(
            "webhook",
            &json!({
                "url": { "secretRef": "webhook.url" },
                "headers": {
                    "x-api-key": { "secretRef": "headers.x-api-key" }
                }
            }),
        );

        assert_eq!(webhook["url"], "[redacted]");
        assert_eq!(webhook["headers"]["x-api-key"], "[redacted]");

        let telegram = redacted_target_config(
            "telegram",
            &json!({
                "botToken": { "secretRef": "telegram.botToken" },
                "chatId": "chat-1"
            }),
        );

        assert_eq!(telegram["botToken"], "[redacted]");
        assert_eq!(telegram["chatId"], "chat-1");
    }

    #[test]
    fn secretizes_target_configs() {
        let webhook = secretize_target_config(
            "webhook",
            &json!({
                "url": "https://notify.example.test/hook?token=secret",
                "headers": {
                    "x-api-key": "secret"
                }
            }),
        )
        .unwrap();

        assert_eq!(webhook.config["url"]["secretRef"], "webhook.url");
        assert_eq!(
            webhook.config["headers"]["x-api-key"]["secretRef"],
            "headers.x-api-key"
        );
        assert_eq!(webhook.secrets.len(), 2);

        let telegram = secretize_target_config(
            "telegram",
            &json!({
                "botToken": "secret-token",
                "chatId": "chat-1",
                "apiBaseUrl": "https://api.telegram.org"
            }),
        )
        .unwrap();

        assert_eq!(
            telegram.config["botToken"]["secretRef"],
            "telegram.botToken"
        );
        assert_eq!(telegram.config["chatId"], "chat-1");
        assert_eq!(telegram.secrets[0].value, "secret-token");
    }
}
