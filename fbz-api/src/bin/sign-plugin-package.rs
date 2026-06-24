use std::{
    env,
    error::Error,
    fmt::{Display, Formatter},
    fs,
    io::{Cursor, Read},
    path::{Path, PathBuf},
};

use fbz_api::plugins::{
    manifest::{PluginManifest, ValidatedPluginManifest},
    signing::{
        hex_encode, parse_ed25519_private_key_hex, sign_plugin_package,
        validate_plugin_signature_key_id,
    },
};
use serde_json::json;
use sha2::{Digest, Sha256};
use zip::ZipArchive;

const PRIVATE_KEY_ENV: &str = "PLUGIN_SIGNING_PRIVATE_KEY_HEX";

fn main() {
    if let Err(err) = run() {
        eprintln!("error: {err}");
        eprintln!("{}", usage());
        std::process::exit(1);
    }
}

fn run() -> Result<(), Box<dyn Error>> {
    let args = CliArgs::parse(env::args().skip(1), env::var(PRIVATE_KEY_ENV).ok())?;
    let package = load_plugin_package(&args.package_path)?;
    validate_plugin_signature_key_id(&args.key_id)?;
    let private_key = parse_ed25519_private_key_hex(&args.private_key_hex)?;
    let signed = sign_plugin_package(
        &package.validated_manifest,
        &package.checksum_sha256,
        &args.key_id,
        &private_key,
    );

    let output = json!({
        "packagePath": package.package_path,
        "archivePath": package.archive_path.display().to_string(),
        "checksumSha256": hex_encode(&package.checksum_sha256),
        "manifestHashSha256": hex_encode(&package.validated_manifest.manifest_hash),
        "pluginId": package.validated_manifest.manifest.id,
        "packageVersion": package.validated_manifest.manifest.version,
        "keyId": signed.key_id,
        "publicKeyHex": signed.public_key_hex,
        "signature": signed.envelope,
        "signatureMessage": signed.message,
    });
    println!("{}", serde_json::to_string_pretty(&output)?);
    Ok(())
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct CliArgs {
    package_path: PathBuf,
    key_id: String,
    private_key_hex: String,
}

impl CliArgs {
    fn parse<I, S>(args: I, env_private_key_hex: Option<String>) -> Result<Self, CliError>
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        let mut package_path = None;
        let mut key_id = None;
        let mut private_key_hex = None;
        let mut iter = args.into_iter().map(Into::into);

        while let Some(arg) = iter.next() {
            match arg.as_str() {
                "--package" => package_path = Some(next_value(&mut iter, "--package")?),
                "--key-id" => key_id = Some(next_value(&mut iter, "--key-id")?),
                "--private-key-hex" => {
                    private_key_hex = Some(next_value(&mut iter, "--private-key-hex")?)
                }
                "--help" | "-h" => return Err(CliError::new(usage())),
                other => return Err(CliError::new(format!("unknown argument `{other}`"))),
            }
        }

        Ok(Self {
            package_path: package_path
                .map(PathBuf::from)
                .ok_or_else(|| CliError::new("missing required --package"))?,
            key_id: key_id.ok_or_else(|| CliError::new("missing required --key-id"))?,
            private_key_hex: private_key_hex.or(env_private_key_hex).ok_or_else(|| {
                CliError::new(format!(
                    "missing --private-key-hex or {PRIVATE_KEY_ENV} environment variable"
                ))
            })?,
        })
    }
}

#[derive(Debug)]
struct PluginPackageInput {
    package_path: String,
    archive_path: PathBuf,
    checksum_sha256: Vec<u8>,
    validated_manifest: ValidatedPluginManifest,
}

fn load_plugin_package(path: &Path) -> Result<PluginPackageInput, Box<dyn Error>> {
    let archive_path = fs::canonicalize(path)?;
    let archive_bytes = fs::read(&archive_path)?;
    let checksum_sha256 = Sha256::digest(&archive_bytes).to_vec();
    let manifest_json = read_manifest_from_zip(&archive_bytes)?;
    let manifest: PluginManifest = serde_json::from_str(&manifest_json)?;
    let validated_manifest = manifest
        .validate()
        .map_err(|err| CliError::new(format!("manifest validation failed: {err:?}")))?;
    let package_path = archive_path
        .file_name()
        .and_then(|name| name.to_str())
        .ok_or_else(|| CliError::new("package archive path must end with a file name"))?
        .to_owned();

    Ok(PluginPackageInput {
        package_path,
        archive_path,
        checksum_sha256,
        validated_manifest,
    })
}

fn read_manifest_from_zip(archive_bytes: &[u8]) -> Result<String, Box<dyn Error>> {
    let mut archive = ZipArchive::new(Cursor::new(archive_bytes))?;
    let mut manifest = archive.by_name("manifest.json")?;
    let mut manifest_json = String::new();
    manifest.read_to_string(&mut manifest_json)?;
    Ok(manifest_json)
}

fn next_value<I>(iter: &mut I, flag: &'static str) -> Result<String, CliError>
where
    I: Iterator<Item = String>,
{
    iter.next()
        .filter(|value| !value.trim().is_empty())
        .ok_or_else(|| CliError::new(format!("missing value for {flag}")))
}

fn usage() -> &'static str {
    "usage: cargo run --bin sign-plugin-package -- --package <zip> --key-id <keyId> [--private-key-hex <64 hex>]\n\
     Set PLUGIN_SIGNING_PRIVATE_KEY_HEX instead of passing --private-key-hex to avoid shell history exposure."
}

#[derive(Debug)]
struct CliError {
    message: String,
}

impl CliError {
    fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
        }
    }
}

impl Display for CliError {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.message)
    }
}

impl Error for CliError {}

#[cfg(test)]
mod tests {
    use std::{
        fs as std_fs,
        io::Write,
        process,
        time::{SystemTime, UNIX_EPOCH},
    };

    use ed25519_dalek::{Signature, Verifier, VerifyingKey};
    use zip::{ZipWriter, write::SimpleFileOptions};

    use super::*;

    #[test]
    fn cli_args_use_private_key_env_fallback() {
        let args = CliArgs::parse(
            ["--package", "plugin.zip", "--key-id", "dev-key"],
            Some("11".repeat(32)),
        )
        .unwrap();

        assert_eq!(args.package_path, PathBuf::from("plugin.zip"));
        assert_eq!(args.key_id, "dev-key");
        assert_eq!(args.private_key_hex, "11".repeat(32));
    }

    #[test]
    fn signs_zip_manifest_with_installable_envelope() {
        let base_dir = unique_test_dir("fbz-sign-plugin-package-test");
        std_fs::create_dir_all(&base_dir).unwrap();
        let archive_path = base_dir.join("plugin.zip");
        write_zip_entries(
            &archive_path,
            &[(
                "manifest.json",
                br#"{
  "id": "dev.fbz.signing-cli",
  "name": "Signing CLI",
  "version": "0.1.0",
  "apiVersion": "1",
  "runtime": "http",
  "entrypoint": "http://127.0.0.1:19091/fbz-plugin"
}"#,
            )],
        );

        let package = load_plugin_package(&archive_path).unwrap();
        let private_key = [11_u8; 32];
        let signed = sign_plugin_package(
            &package.validated_manifest,
            &package.checksum_sha256,
            "dev-key",
            &private_key,
        );

        assert_eq!(package.package_path, "plugin.zip");
        assert_eq!(package.checksum_sha256.len(), 32);
        assert!(signed.envelope.starts_with("ed25519:dev-key:"));
        let public_key_bytes = parse_hex_32(&signed.public_key_hex);
        let signature_bytes = parse_hex_64(&signed.signature_hex);
        let verifying_key = VerifyingKey::from_bytes(&public_key_bytes).unwrap();
        let signature = Signature::from_bytes(&signature_bytes);
        verifying_key
            .verify(signed.message.as_bytes(), &signature)
            .unwrap();

        let _ = std_fs::remove_dir_all(base_dir);
    }

    fn unique_test_dir(prefix: &str) -> PathBuf {
        env::temp_dir().join(format!(
            "{prefix}-{}-{}",
            process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ))
    }

    fn write_zip_entries(path: &Path, entries: &[(&str, &[u8])]) {
        let file = std_fs::File::create(path).unwrap();
        let mut writer = ZipWriter::new(file);
        let options =
            SimpleFileOptions::default().compression_method(zip::CompressionMethod::Deflated);
        for (name, contents) in entries {
            writer.start_file(*name, options).unwrap();
            writer.write_all(contents).unwrap();
        }
        writer.finish().unwrap();
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
