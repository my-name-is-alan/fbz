use std::{
    error::Error,
    fmt::{Display, Formatter},
};

use ed25519_dalek::{Signer, SigningKey};

use crate::plugins::manifest::ValidatedPluginManifest;

pub const PLUGIN_PACKAGE_SIGNATURE_SCHEME: &str = "ed25519";
pub const PLUGIN_PACKAGE_SIGNATURE_CONTEXT: &str = "fbz-plugin-package-v1";

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PluginPackageSignature {
    pub key_id: String,
    pub public_key_hex: String,
    pub signature_hex: String,
    pub envelope: String,
    pub message: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PluginPackageSigningError {
    message: &'static str,
}

impl PluginPackageSigningError {
    pub fn message(&self) -> &'static str {
        self.message
    }
}

impl Display for PluginPackageSigningError {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.message)
    }
}

impl Error for PluginPackageSigningError {}

pub fn plugin_package_signature_message(
    validated_manifest: &ValidatedPluginManifest,
    checksum_sha256: &[u8],
) -> String {
    format!(
        "{}\n{}\n{}\n{}\n{}",
        PLUGIN_PACKAGE_SIGNATURE_CONTEXT,
        validated_manifest.manifest.id.trim(),
        validated_manifest.manifest.version.trim(),
        hex_encode(checksum_sha256),
        hex_encode(&validated_manifest.manifest_hash),
    )
}

pub fn sign_plugin_package(
    validated_manifest: &ValidatedPluginManifest,
    checksum_sha256: &[u8],
    key_id: &str,
    private_key: &[u8; 32],
) -> PluginPackageSignature {
    let signing_key = SigningKey::from_bytes(private_key);
    let message = plugin_package_signature_message(validated_manifest, checksum_sha256);
    let signature = signing_key.sign(message.as_bytes());
    let signature_hex = hex_encode(&signature.to_bytes());
    let public_key_hex = hex_encode(&signing_key.verifying_key().to_bytes());
    let key_id = key_id.trim().to_owned();
    let envelope = format!("{PLUGIN_PACKAGE_SIGNATURE_SCHEME}:{key_id}:{signature_hex}");

    PluginPackageSignature {
        key_id,
        public_key_hex,
        signature_hex,
        envelope,
        message,
    }
}

pub fn validate_plugin_signature_key_id(key_id: &str) -> Result<(), PluginPackageSigningError> {
    if key_id.is_empty()
        || key_id.len() > 64
        || !key_id
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'_' | b'-'))
    {
        return Err(signing_error("plugin package signature key id is invalid"));
    }
    Ok(())
}

pub fn parse_ed25519_private_key_hex(value: &str) -> Result<[u8; 32], PluginPackageSigningError> {
    parse_hex_array::<32>(
        value,
        "ed25519 private key hex must be 64 characters",
        "ed25519 private key hex is invalid",
    )
}

pub fn parse_ed25519_signature_hex(value: &str) -> Result<[u8; 64], PluginPackageSigningError> {
    parse_hex_array::<64>(
        value,
        "plugin package signature hex must be 128 characters",
        "plugin package signature hex is invalid",
    )
}

pub fn hex_encode(bytes: &[u8]) -> String {
    let mut encoded = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        use std::fmt::Write as _;
        let _ = write!(encoded, "{byte:02x}");
    }
    encoded
}

fn parse_hex_array<const N: usize>(
    value: &str,
    wrong_length_message: &'static str,
    invalid_message: &'static str,
) -> Result<[u8; N], PluginPackageSigningError> {
    let value = value.trim();
    if value.len() != N * 2 {
        return Err(signing_error(wrong_length_message));
    }
    let bytes = parse_hex_bytes(value, invalid_message)?;
    bytes.try_into().map_err(|_| signing_error(invalid_message))
}

fn parse_hex_bytes(
    value: &str,
    invalid_message: &'static str,
) -> Result<Vec<u8>, PluginPackageSigningError> {
    let mut bytes = Vec::with_capacity(value.len() / 2);
    for index in (0..value.len()).step_by(2) {
        let byte = u8::from_str_radix(&value[index..index + 2], 16)
            .map_err(|_| signing_error(invalid_message))?;
        bytes.push(byte);
    }
    Ok(bytes)
}

fn signing_error(message: &'static str) -> PluginPackageSigningError {
    PluginPackageSigningError { message }
}

#[cfg(test)]
mod tests {
    use ed25519_dalek::{Signature, Verifier, VerifyingKey};
    use sha2::{Digest, Sha256};

    use super::*;
    use crate::plugins::manifest::PluginManifest;

    #[test]
    fn signs_plugin_package_message_with_ed25519_envelope() {
        let manifest = test_manifest("dev.fbz.signed-tool").validate().unwrap();
        let checksum = Sha256::digest(b"plugin zip bytes");
        let private_key = [9_u8; 32];

        let signed = sign_plugin_package(&manifest, &checksum, "dev-key", &private_key);

        assert_eq!(signed.key_id, "dev-key");
        assert_eq!(signed.public_key_hex.len(), 64);
        assert_eq!(signed.signature_hex.len(), 128);
        assert_eq!(
            signed.envelope,
            format!("ed25519:dev-key:{}", signed.signature_hex)
        );
        assert_eq!(
            signed.message,
            format!(
                "fbz-plugin-package-v1\ndev.fbz.signed-tool\n0.1.0\n{}\n{}",
                hex_encode(&checksum),
                hex_encode(&manifest.manifest_hash)
            )
        );

        let public_key_bytes = parse_hex_32(&signed.public_key_hex);
        let signature_bytes = parse_hex_64(&signed.signature_hex);
        let verifying_key = VerifyingKey::from_bytes(&public_key_bytes).unwrap();
        let signature = Signature::from_bytes(&signature_bytes);
        verifying_key
            .verify(signed.message.as_bytes(), &signature)
            .unwrap();
    }

    fn test_manifest(id: &str) -> PluginManifest {
        PluginManifest {
            id: id.to_owned(),
            name: "Signed Tool".to_owned(),
            version: "0.1.0".to_owned(),
            api_version: "1".to_owned(),
            runtime: "http".to_owned(),
            entrypoint: "http://127.0.0.1:19091/fbz-plugin".to_owned(),
            description: Some("signing test".to_owned()),
            permissions: Vec::new(),
            hooks: Vec::new(),
            schedules: Vec::new(),
            menu: Vec::new(),
            config_schema: Vec::new(),
        }
    }

    fn parse_hex_32(value: &str) -> [u8; 32] {
        parse_hex(value).try_into().unwrap()
    }

    fn parse_hex_64(value: &str) -> [u8; 64] {
        parse_hex(value).try_into().unwrap()
    }

    fn parse_hex(value: &str) -> Vec<u8> {
        (0..value.len())
            .step_by(2)
            .map(|index| u8::from_str_radix(&value[index..index + 2], 16).unwrap())
            .collect()
    }
}
