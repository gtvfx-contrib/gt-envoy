//! Optional encryption helpers for sensitive `.envoy/*.json` string values.
//!
//! This module keeps encryption strictly opt-in. Plain JSON strings continue to
//! work unchanged unless a caller explicitly checks [`is_encrypted_value`] and
//! decrypts a tagged value. Encrypted strings use the standard age file format,
//! serialized as base64 with a fixed prefix, so they remain ordinary JSON
//! strings and compose cleanly with
//! [`crate::json_util::parse_json_with_comments`].
//!
//! Key distribution, storage, and rotation are manual, out-of-band processes.
//! This module only loads existing age X25519 identity files from disk; it
//! does not provision keys, synchronize them across machines, or rotate them.

use std::fs;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};

use age::{Decryptor, Encryptor};
use base64::engine::general_purpose::STANDARD as BASE64_STANDARD;
use base64::Engine as _;
use thiserror::Error;

use crate::user_config::UserConfig;

/// Prefix marking a JSON string as an encrypted config value.
pub const ENCRYPTED_VALUE_PREFIX: &str = "age-encrypted:";

/// Environment variable pointing at the age identity file.
pub const CONFIG_KEY_FILE_ENV_VAR: &str = "ENVOY_CONFIG_KEY_FILE";

/// User-config setting pointing at the age identity file.
pub const CONFIG_KEY_FILE_SETTING: &str = "config_key_file";

const AGE_IDENTITY_PREFIX: &str = "AGE-SECRET-KEY-";

/// Error type for config encryption and decryption operations.
#[derive(Debug, Error)]
pub enum ConfigCryptoError {
    #[error("config value is encrypted but does not start with {ENCRYPTED_VALUE_PREFIX}")]
    MissingPrefix,

    #[error(
        "config value is encrypted but no \
{CONFIG_KEY_FILE_ENV_VAR}/{CONFIG_KEY_FILE_SETTING} is configured"
    )]
    MissingKeyFileConfiguration,

    #[error("age recipient encryption requires at least one recipient")]
    MissingRecipient,

    #[error("failed to initialize age encryption: {source}")]
    Encrypt { source: age::EncryptError },

    #[error("failed to read config key file at {path}: {source}")]
    ReadKeyFile {
        path: PathBuf,
        source: std::io::Error,
    },

    #[error("unsupported identity line {line_number} in key file at {path}")]
    UnsupportedIdentityLine { path: PathBuf, line_number: usize },

    #[error("failed to parse age identity in key file at {path}: {message}")]
    InvalidIdentity { path: PathBuf, message: String },

    #[error("no supported age identities found in key file at {path}")]
    MissingIdentity { path: PathBuf },

    #[error("failed to write encrypted config value: {source}")]
    EncryptIo { source: std::io::Error },

    #[error("failed to base64-decode encrypted config value: {source}")]
    DecodeBase64 { source: base64::DecodeError },

    #[error("failed to open age payload from encrypted config value: {source}")]
    OpenAgePayload { source: age::DecryptError },

    #[error("failed to decrypt encrypted config value: {source}")]
    Decrypt { source: age::DecryptError },

    #[error("failed to read decrypted config value: {source}")]
    DecryptIo { source: std::io::Error },

    #[error("decrypted config value is not valid UTF-8: {source}")]
    Utf8 { source: std::string::FromUtf8Error },
}

/// Return whether `value` uses envoy's encrypted-value prefix.
pub fn is_encrypted_value(value: &str) -> bool {
    value.starts_with(ENCRYPTED_VALUE_PREFIX)
}

/// Generate a new X25519 age keypair for config encryption.
pub fn generate_keypair() -> (age::x25519::Identity, age::x25519::Recipient) {
    let identity = age::x25519::Identity::generate();
    let recipient = identity.to_public();
    (identity, recipient)
}

/// Return the configured age identity file path, if any.
///
/// Resolution order is:
/// 1. `ENVOY_CONFIG_KEY_FILE`
/// 2. `config_key_file` from the user config file
pub fn configured_key_file_path() -> Option<PathBuf> {
    if let Some(path) = std::env::var_os(CONFIG_KEY_FILE_ENV_VAR) {
        if !path.is_empty() {
            return Some(PathBuf::from(path));
        }
    }

    UserConfig::load(None)
        .get(CONFIG_KEY_FILE_SETTING)
        .filter(|value| !value.trim().is_empty())
        .map(PathBuf::from)
}

/// Encrypt `plaintext` for `recipient`.
pub fn encrypt_value(
    plaintext: &str,
    recipient: &age::x25519::Recipient,
) -> Result<String, ConfigCryptoError> {
    let Some(encryptor) = Encryptor::with_recipients(vec![
        Box::new(recipient.clone()) as Box<dyn age::Recipient + Send>
    ]) else {
        return Err(ConfigCryptoError::MissingRecipient);
    };

    let mut ciphertext = Vec::new();
    let mut writer = encryptor
        .wrap_output(&mut ciphertext)
        .map_err(|source| ConfigCryptoError::Encrypt { source })?;
    writer
        .write_all(plaintext.as_bytes())
        .map_err(|source| ConfigCryptoError::EncryptIo { source })?;
    writer
        .finish()
        .map_err(|source| ConfigCryptoError::EncryptIo { source })?;

    Ok(format!(
        "{ENCRYPTED_VALUE_PREFIX}{}",
        BASE64_STANDARD.encode(ciphertext)
    ))
}

/// Decrypt a prefixed encrypted config value using `key_file_path`.
pub fn decrypt_value(
    encrypted: &str,
    key_file_path: Option<&Path>,
) -> Result<String, ConfigCryptoError> {
    if !is_encrypted_value(encrypted) {
        return Err(ConfigCryptoError::MissingPrefix);
    }

    let key_file_path = key_file_path.ok_or(ConfigCryptoError::MissingKeyFileConfiguration)?;

    let identities = load_identities_from_file(key_file_path)?;
    let payload = encrypted.trim_start_matches(ENCRYPTED_VALUE_PREFIX);
    let ciphertext = BASE64_STANDARD
        .decode(payload)
        .map_err(|source| ConfigCryptoError::DecodeBase64 { source })?;

    let decryptor = match Decryptor::new_buffered(ciphertext.as_slice())
        .map_err(|source| ConfigCryptoError::OpenAgePayload { source })?
    {
        Decryptor::Recipients(decryptor) => decryptor,
        Decryptor::Passphrase(_) => {
            return Err(ConfigCryptoError::InvalidIdentity {
                path: key_file_path.to_path_buf(),
                message: String::from(
                    "passphrase-encrypted payloads are not supported for config values",
                ),
            });
        }
    };

    let identity_refs = identities
        .iter()
        .map(|identity| identity as &dyn age::Identity);
    let mut reader = decryptor
        .decrypt(identity_refs)
        .map_err(|source| ConfigCryptoError::Decrypt { source })?;
    let mut plaintext = Vec::new();
    reader
        .read_to_end(&mut plaintext)
        .map_err(|source| ConfigCryptoError::DecryptIo { source })?;

    String::from_utf8(plaintext).map_err(|source| ConfigCryptoError::Utf8 { source })
}

fn load_identities_from_file(path: &Path) -> Result<Vec<age::x25519::Identity>, ConfigCryptoError> {
    let contents = fs::read_to_string(path).map_err(|source| ConfigCryptoError::ReadKeyFile {
        path: path.to_path_buf(),
        source,
    })?;
    let mut identities = Vec::new();

    for (index, line) in contents.lines().enumerate() {
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }

        if !trimmed.starts_with(AGE_IDENTITY_PREFIX) {
            return Err(ConfigCryptoError::UnsupportedIdentityLine {
                path: path.to_path_buf(),
                line_number: index + 1,
            });
        }

        let identity = trimmed.parse::<age::x25519::Identity>().map_err(|source| {
            ConfigCryptoError::InvalidIdentity {
                path: path.to_path_buf(),
                message: source.to_string(),
            }
        })?;
        identities.push(identity);
    }

    if identities.is_empty() {
        return Err(ConfigCryptoError::MissingIdentity {
            path: path.to_path_buf(),
        });
    }

    Ok(identities)
}

#[cfg(test)]
mod tests {
    use std::env;
    use std::fs;

    use age::secrecy::ExposeSecret;
    use serde_json::json;
    use tempfile::tempdir;

    use super::*;

    struct EnvVarGuard {
        key: &'static str,
        previous: Option<std::ffi::OsString>,
    }

    impl EnvVarGuard {
        fn set(key: &'static str, value: &Path) -> Self {
            let previous = env::var_os(key);
            env::set_var(key, value);
            Self { key, previous }
        }

        fn remove(key: &'static str) -> Self {
            let previous = env::var_os(key);
            env::remove_var(key);
            Self { key, previous }
        }
    }

    impl Drop for EnvVarGuard {
        fn drop(&mut self) {
            match &self.previous {
                Some(value) => env::set_var(self.key, value),
                None => env::remove_var(self.key),
            }
        }
    }

    fn with_env_lock<T>(test_fn: impl FnOnce() -> T) -> T {
        let _lock = crate::env_test_lock::MUTEX
            .lock()
            .unwrap_or_else(|poison| poison.into_inner());
        test_fn()
    }

    fn write_identity_file(path: &Path, identity: &age::x25519::Identity) {
        let encoded = identity.to_string();
        let contents = format!(
            "# envoy config encryption key\n{}\n",
            encoded.expose_secret()
        );
        fs::write(path, contents).expect("identity file should be written");
    }

    #[test]
    fn encrypt_and_decrypt_round_trip_with_matching_key() {
        let temp_dir = tempdir().expect("tempdir should be created");
        let identity_path = temp_dir.path().join("config.agekey");
        let plaintext = "C:\\secure\\packages";
        let (identity, recipient) = generate_keypair();
        write_identity_file(&identity_path, &identity);

        let encrypted = encrypt_value(plaintext, &recipient).expect("value should encrypt");
        let decrypted =
            decrypt_value(&encrypted, Some(&identity_path)).expect("value should decrypt");

        assert_eq!(decrypted, plaintext);
    }

    #[test]
    fn decrypt_without_key_configuration_returns_clear_error() {
        let (_, recipient) = generate_keypair();
        let encrypted = encrypt_value("secret", &recipient).expect("value should encrypt");

        let error = decrypt_value(&encrypted, None).expect_err("missing key should fail");

        assert!(matches!(
            error,
            ConfigCryptoError::MissingKeyFileConfiguration
        ));
        assert_eq!(
            error.to_string(),
            "config value is encrypted but no \
ENVOY_CONFIG_KEY_FILE/config_key_file is configured"
        );
    }

    #[test]
    fn decrypt_with_wrong_key_fails() {
        let temp_dir = tempdir().expect("tempdir should be created");
        let wrong_identity_path = temp_dir.path().join("wrong.agekey");
        let (_, recipient) = generate_keypair();
        let (wrong_identity, _) = generate_keypair();
        write_identity_file(&wrong_identity_path, &wrong_identity);

        let encrypted = encrypt_value("secret", &recipient).expect("value should encrypt");
        let error = decrypt_value(&encrypted, Some(&wrong_identity_path))
            .expect_err("wrong key should fail");

        assert!(matches!(error, ConfigCryptoError::Decrypt { .. }));
    }

    #[test]
    fn is_encrypted_value_distinguishes_prefixed_strings() {
        assert!(is_encrypted_value("age-encrypted:payload"));
        assert!(!is_encrypted_value("C:\\plain\\path"));
    }

    #[test]
    fn configured_key_file_path_reads_user_config_setting() {
        with_env_lock(|| {
            let temp_dir = tempdir().expect("tempdir should be created");
            let user_config_path = temp_dir.path().join("user_config.json");
            let expected_key_path = temp_dir.path().join("config.agekey");

            fs::write(
                &user_config_path,
                json!({
                    "config_key_file": expected_key_path.display().to_string(),
                })
                .to_string(),
            )
            .expect("user config should be written");

            let _env_guard = EnvVarGuard::remove(CONFIG_KEY_FILE_ENV_VAR);
            let _user_config_guard = EnvVarGuard::set("ENVOY_USER_CONFIG", &user_config_path);

            assert_eq!(configured_key_file_path(), Some(expected_key_path));
        });
    }
}
