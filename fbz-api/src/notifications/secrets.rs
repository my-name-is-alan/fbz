use std::{
    collections::HashMap,
    error::Error,
    fmt::{Display, Formatter},
};

use chacha20poly1305::{
    Key, XChaCha20Poly1305, XNonce,
    aead::{Aead, AeadCore, KeyInit, OsRng, Payload},
};
use serde_json::{Map, Value, json};
use sha2::{Digest, Sha256};

use crate::config::SecretConfig;

pub const SECRET_ALGORITHM: &str = "xchacha20poly1305-sha256-key-v1";

#[derive(Clone)]
pub struct SecretCipher {
    cipher: XChaCha20Poly1305,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct EncryptedSecret {
    pub algorithm: &'static str,
    pub nonce: Vec<u8>,
    pub ciphertext: Vec<u8>,
    pub value_hash: Vec<u8>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TargetSecretInput {
    pub key: String,
    pub value: String,
}

#[derive(Debug)]
pub enum SecretError {
    MissingKey,
    InvalidKey(String),
    InvalidReference(String),
    MissingSecret(String),
    Encrypt,
    Decrypt,
    Utf8,
}

impl SecretCipher {
    pub fn from_config(config: &SecretConfig) -> Result<Self, SecretError> {
        let Some(key) = config.key.as_deref() else {
            return Err(SecretError::MissingKey);
        };
        Self::from_key_material(key)
    }

    pub fn from_key_material(key: &str) -> Result<Self, SecretError> {
        let key = key.trim();
        if key.len() < 32 {
            return Err(SecretError::InvalidKey(
                "FBZ_SECRET_KEY must be at least 32 characters".to_owned(),
            ));
        }
        let mut hasher = Sha256::new();
        hasher.update(b"fbz-api-secret-key-v1\0");
        hasher.update(key.as_bytes());
        let derived = hasher.finalize();
        Ok(Self {
            cipher: XChaCha20Poly1305::new(Key::from_slice(&derived)),
        })
    }

    pub fn encrypt(
        &self,
        target_id: i64,
        secret_key: &str,
        value: &str,
    ) -> Result<EncryptedSecret, SecretError> {
        self.encrypt_scoped(
            "notification-target",
            &target_id.to_string(),
            secret_key,
            value,
        )
    }

    pub fn encrypt_scoped(
        &self,
        scope: &str,
        resource_key: &str,
        secret_key: &str,
        value: &str,
    ) -> Result<EncryptedSecret, SecretError> {
        validate_associated_data_segment("scope", scope)?;
        validate_associated_data_segment("resource key", resource_key)?;
        validate_secret_key(secret_key)?;
        let nonce = XChaCha20Poly1305::generate_nonce(&mut OsRng);
        let associated_data = associated_data(scope, resource_key, secret_key);
        let ciphertext = self
            .cipher
            .encrypt(
                &nonce,
                Payload {
                    msg: value.as_bytes(),
                    aad: associated_data.as_bytes(),
                },
            )
            .map_err(|_| SecretError::Encrypt)?;

        Ok(EncryptedSecret {
            algorithm: SECRET_ALGORITHM,
            nonce: nonce.to_vec(),
            ciphertext,
            value_hash: Sha256::digest(value.as_bytes()).to_vec(),
        })
    }

    pub fn decrypt(
        &self,
        target_id: i64,
        secret_key: &str,
        nonce: &[u8],
        ciphertext: &[u8],
    ) -> Result<String, SecretError> {
        self.decrypt_scoped(
            "notification-target",
            &target_id.to_string(),
            secret_key,
            nonce,
            ciphertext,
        )
    }

    pub fn decrypt_scoped(
        &self,
        scope: &str,
        resource_key: &str,
        secret_key: &str,
        nonce: &[u8],
        ciphertext: &[u8],
    ) -> Result<String, SecretError> {
        validate_associated_data_segment("scope", scope)?;
        validate_associated_data_segment("resource key", resource_key)?;
        validate_secret_key(secret_key)?;
        if nonce.len() != 24 {
            return Err(SecretError::Decrypt);
        }
        let nonce = XNonce::from_slice(nonce);
        let associated_data = associated_data(scope, resource_key, secret_key);
        let plaintext = self
            .cipher
            .decrypt(
                nonce,
                Payload {
                    msg: ciphertext,
                    aad: associated_data.as_bytes(),
                },
            )
            .map_err(|_| SecretError::Decrypt)?;
        String::from_utf8(plaintext).map_err(|_| SecretError::Utf8)
    }
}

pub fn secret_ref(secret_key: &str) -> Value {
    json!({ "secretRef": secret_key })
}

pub fn secret_ref_key(value: &Value) -> Option<&str> {
    let object = value.as_object()?;
    if object.len() != 1 {
        return None;
    }
    object.get("secretRef")?.as_str()
}

pub fn materialize_secret_refs(
    value: &Value,
    secrets: &HashMap<String, String>,
) -> Result<Value, SecretError> {
    if let Some(secret_key) = secret_ref_key(value) {
        validate_secret_key(secret_key)?;
        let secret = secrets
            .get(secret_key)
            .ok_or_else(|| SecretError::MissingSecret(secret_key.to_owned()))?;
        return Ok(Value::String(secret.clone()));
    }

    match value {
        Value::Array(items) => items
            .iter()
            .map(|item| materialize_secret_refs(item, secrets))
            .collect::<Result<Vec<_>, _>>()
            .map(Value::Array),
        Value::Object(object) => object
            .iter()
            .map(|(key, value)| Ok((key.clone(), materialize_secret_refs(value, secrets)?)))
            .collect::<Result<Map<_, _>, SecretError>>()
            .map(Value::Object),
        _ => Ok(value.clone()),
    }
}

pub fn contains_secret_refs(value: &Value) -> bool {
    if secret_ref_key(value).is_some() {
        return true;
    }
    match value {
        Value::Array(items) => items.iter().any(contains_secret_refs),
        Value::Object(object) => object.values().any(contains_secret_refs),
        _ => false,
    }
}

pub fn validate_secret_key(value: &str) -> Result<(), SecretError> {
    let value = value.trim();
    if value.is_empty() || value.len() > 128 || value.contains(char::is_whitespace) {
        return Err(SecretError::InvalidReference(
            "secret reference must be 1 to 128 non-whitespace characters".to_owned(),
        ));
    }
    if !value
        .bytes()
        .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'-' | b'_' | b':'))
    {
        return Err(SecretError::InvalidReference(
            "secret reference contains unsupported characters".to_owned(),
        ));
    }
    Ok(())
}

fn validate_associated_data_segment(field: &str, value: &str) -> Result<(), SecretError> {
    let value = value.trim();
    if value.is_empty() || value.contains('\0') {
        return Err(SecretError::InvalidReference(format!(
            "{field} must be a non-empty text value"
        )));
    }
    Ok(())
}

fn associated_data(scope: &str, resource_key: &str, secret_key: &str) -> String {
    format!("{}:{}:{secret_key}", scope.trim(), resource_key.trim())
}

impl Display for SecretError {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::MissingKey => {
                f.write_str("FBZ_SECRET_KEY is required to store notification target secrets")
            }
            Self::InvalidKey(message) => write!(f, "invalid secret key: {message}"),
            Self::InvalidReference(message) => write!(f, "invalid secret reference: {message}"),
            Self::MissingSecret(secret_key) => {
                write!(f, "notification target secret `{secret_key}` was not found")
            }
            Self::Encrypt => f.write_str("failed to encrypt secret"),
            Self::Decrypt => f.write_str("failed to decrypt secret"),
            Self::Utf8 => f.write_str("decrypted secret is not valid UTF-8"),
        }
    }
}

impl Error for SecretError {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cipher_round_trips_and_binds_associated_data() {
        let cipher = SecretCipher::from_key_material("0123456789abcdef0123456789abcdef").unwrap();
        let encrypted = cipher.encrypt(42, "webhook.url", "secret").unwrap();

        let decrypted = cipher
            .decrypt(42, "webhook.url", &encrypted.nonce, &encrypted.ciphertext)
            .unwrap();

        assert_eq!(decrypted, "secret");
        assert_eq!(encrypted.algorithm, SECRET_ALGORITHM);
        assert_eq!(encrypted.nonce.len(), 24);
        assert_eq!(encrypted.value_hash.len(), 32);
        assert!(
            cipher
                .decrypt(43, "webhook.url", &encrypted.nonce, &encrypted.ciphertext)
                .is_err()
        );
        assert!(
            cipher
                .decrypt_scoped(
                    "plugin-config",
                    "dev.test",
                    "webhook.url",
                    &encrypted.nonce,
                    &encrypted.ciphertext
                )
                .is_err()
        );
    }

    #[test]
    fn materializes_nested_secret_refs() {
        let mut secrets = HashMap::new();
        secrets.insert(
            "webhook.url".to_owned(),
            "https://example.test/hook".to_owned(),
        );
        secrets.insert("headers.x-api-key".to_owned(), "secret".to_owned());
        let config = json!({
            "url": secret_ref("webhook.url"),
            "headers": {
                "x-api-key": secret_ref("headers.x-api-key")
            }
        });

        let materialized = materialize_secret_refs(&config, &secrets).unwrap();

        assert_eq!(materialized["url"], "https://example.test/hook");
        assert_eq!(materialized["headers"]["x-api-key"], "secret");
        assert!(contains_secret_refs(&config));
        assert!(!contains_secret_refs(&materialized));
    }

    #[test]
    fn invalid_secret_refs_are_rejected() {
        assert!(validate_secret_key("webhook.url").is_ok());
        assert!(validate_secret_key("../bad").is_err());
        assert!(validate_secret_key("has space").is_err());
    }
}
