//! Team configuration loading and merging for `envoy`.
//!
//! This module implements team configuration discovery. Each bundle's
//! `.envoy/team.json` defines team-level settings (bundle roots, stack
//! roots, user config paths). These are merged with per-user host configs to
//! produce the final resolved [`TeamConfig`] used throughout envoy.

use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};

use thiserror::Error;

use crate::config_crypto::{
    configured_key_file_path, decrypt_value, is_encrypted_value, ConfigCryptoError,
};
use crate::json_util::parse_json_with_comments;

/// Error type for team configuration operations.
#[derive(Debug, Error)]
pub enum TeamConfigError {
    #[error("I/O error reading team config at {path}: {source}")]
    Io {
        path: PathBuf,
        source: std::io::Error,
    },

    #[error("invalid JSON in team config at {path}: {source}")]
    Json {
        path: PathBuf,
        source: serde_json::Error,
    },

    #[error("team '{name}' not found; no .envoy/team.json discovered")]
    NotFound { name: String },

    #[error("missing required field '{field}' in team config at {path}")]
    MissingField { field: String, path: PathBuf },

    #[error("failed to decrypt field '{field}' in user config at {path}: {source}")]
    DecryptUserField {
        field: String,
        path: PathBuf,
        source: ConfigCryptoError,
    },
}

/// A resolved team configuration with all settings merged.
#[derive(Clone, Debug)]
pub struct TeamConfig {
    /// Human-readable team name (e.g., `"bfd"`).
    pub name: String,
    /// Absolute or UNC path to the production bundles root directory.
    pub prod_bundles_root: Option<PathBuf>,
    /// Absolute or UNC path to the production stacks root directory.
    pub prod_stacks_root: Option<PathBuf>,
    /// Path (possibly with `~` expansion) to a user/host config JSON file.
    pub user_host_config_file: Option<String>,
    /// Arbitrary additional settings from team.json.
    pub metadata: HashMap<String, serde_json::Value>,
}

impl TeamConfig {
    /// Create an empty default team configuration (no team discovered).
    pub fn empty() -> Self {
        Self {
            name: String::new(),
            prod_bundles_root: None,
            prod_stacks_root: None,
            user_host_config_file: None,
            metadata: HashMap::new(),
        }
    }

    /// Expand `~` in a path string to the current user's home directory.
    fn expand_tilde(path_str: &str) -> PathBuf {
        if let Some(stripped) = path_str.strip_prefix('~') {
            if let Ok(home) = std::env::var("USERPROFILE") {
                return PathBuf::from(home).join(stripped.trim_start_matches('/'));
            }
        }
        PathBuf::from(path_str)
    }

    /// Load a team config from a JSON file.
    pub fn load_from_file(path: &Path) -> Result<Self, TeamConfigError> {
        let content = fs::read_to_string(path).map_err(|e| TeamConfigError::Io {
            path: path.to_path_buf(),
            source: e,
        })?;

        let value: serde_json::Value =
            parse_json_with_comments(&content).map_err(|e| TeamConfigError::Json {
                path: path.to_path_buf(),
                source: e,
            })?;

        // Validate required fields.
        if !value.is_object() {
            return Err(TeamConfigError::MissingField {
                field: "object".to_string(),
                path: path.to_path_buf(),
            });
        }

        let name = value.get("name").and_then(|v| v.as_str()).ok_or_else(|| {
            TeamConfigError::MissingField {
                field: "name".to_string(),
                path: path.to_path_buf(),
            }
        })?;

        // Parse optional fields.
        let prod_bundles_root = value
            .get("prodBundlesRoot")
            .and_then(|v| v.as_str())
            .map(Self::expand_tilde);

        let prod_stacks_root = value
            .get("prodStacksRoot")
            .and_then(|v| v.as_str())
            .map(Self::expand_tilde);

        let user_host_config_file = value
            .get("userHostConfigFile")
            .and_then(|v| v.as_str())
            .map(String::from);

        // Collect remaining fields as metadata.
        let mut metadata = HashMap::new();
        for (key, val) in value.as_object().unwrap() {
            if !matches!(
                key.as_str(),
                "name" | "prodBundlesRoot" | "prodStacksRoot" | "userHostConfigFile"
            ) {
                metadata.insert(key.clone(), val.clone());
            }
        }

        Ok(TeamConfig {
            name: name.to_string(),
            prod_bundles_root,
            prod_stacks_root,
            user_host_config_file,
            metadata,
        })
    }

    /// Merge this team config with a host/user configuration.
    ///
    /// Host settings take precedence over team defaults for any overlapping keys.
    pub fn merge_with_user(&self, user: &UserHostConfig) -> Self {
        let mut merged = self.clone();

        // User/host config overrides team-level paths if set.
        if !user.prod_bundles_root.is_empty() {
            merged.prod_bundles_root = Some(PathBuf::from(&user.prod_bundles_root));
        }
        if !user.prod_stacks_root.is_empty() {
            merged.prod_stacks_root = Some(PathBuf::from(&user.prod_stacks_root));
        }

        // Merge user metadata on top of team metadata.
        for (key, val) in &user.metadata {
            merged.metadata.insert(key.clone(), val.clone());
        }

        merged
    }
}

/// Per-user/host configuration that overrides team defaults.
#[derive(Clone, Debug, Default)]
pub struct UserHostConfig {
    /// Override for the production bundles root (empty = use team default).
    pub prod_bundles_root: String,
    /// Override for the production stacks root (empty = use team default).
    pub prod_stacks_root: String,
    /// Arbitrary additional settings from user/host config.
    pub metadata: HashMap<String, serde_json::Value>,
}

impl UserHostConfig {
    /// Load a host/user configuration from a JSON file path.
    pub fn load_from_file(path: &Path) -> Result<Self, TeamConfigError> {
        let content = fs::read_to_string(path).map_err(|e| TeamConfigError::Io {
            path: path.to_path_buf(),
            source: e,
        })?;

        let value: serde_json::Value =
            parse_json_with_comments(&content).map_err(|e| TeamConfigError::Json {
                path: path.to_path_buf(),
                source: e,
            })?;

        if !value.is_object() {
            return Err(TeamConfigError::MissingField {
                field: "object".to_string(),
                path: path.to_path_buf(),
            });
        }

        let key_file_path = configured_key_file_path();
        let prod_bundles_root =
            Self::resolve_string_field(&value, "prodBundlesRoot", path, key_file_path.as_deref())?;
        let prod_stacks_root =
            Self::resolve_string_field(&value, "prodStacksRoot", path, key_file_path.as_deref())?;

        // Collect remaining fields as metadata.
        let mut metadata = HashMap::new();
        for (key, val) in value.as_object().unwrap() {
            if !matches!(key.as_str(), "prodBundlesRoot" | "prodStacksRoot") {
                metadata.insert(key.clone(), val.clone());
            }
        }

        Ok(UserHostConfig {
            prod_bundles_root,
            prod_stacks_root,
            metadata,
        })
    }

    fn resolve_string_field(
        value: &serde_json::Value,
        field: &str,
        path: &Path,
        key_file_path: Option<&Path>,
    ) -> Result<String, TeamConfigError> {
        let Some(raw_value) = value.get(field).and_then(|item| item.as_str()) else {
            return Ok(String::new());
        };

        if is_encrypted_value(raw_value) {
            return decrypt_value(raw_value, key_file_path).map_err(|source| {
                TeamConfigError::DecryptUserField {
                    field: field.to_string(),
                    path: path.to_path_buf(),
                    source,
                }
            });
        }

        Ok(raw_value.to_string())
    }
}

/// Discover team configurations from a set of bundles.
///
/// Scans each bundle's `.envoy/` directory for `team.json`. Returns the first
/// discovered config (or empty if none found). Only one team is active
/// per environment.
pub fn discover_team_configs(bundles: &[crate::discovery::BundleInfo]) -> Vec<TeamConfig> {
    let mut configs = Vec::new();

    for bundle in bundles {
        let team_path = bundle.envoy_env().join("team.json");
        if !team_path.is_file() {
            continue;
        }

        match TeamConfig::load_from_file(&team_path) {
            Ok(config) => configs.push(config),
            Err(e) => tracing::warn!(
                path = %team_path.display(),
                error = %e,
                "failed to load team config"
            ),
        }
    }

    configs
}

/// Resolve the active team configuration for a set of bundles.
///
/// Returns the first discovered team config, optionally merged with user/host
/// settings if `user_config_path` is provided and points to an existing file.
pub fn resolve_team_config(
    bundles: &[crate::discovery::BundleInfo],
    user_config_path: Option<&Path>,
) -> Result<TeamConfig, TeamConfigError> {
    let configs = discover_team_configs(bundles);

    if configs.is_empty() {
        return Err(TeamConfigError::NotFound {
            name: String::new(),
        });
    }

    // Use the first discovered team config.
    let mut active = configs.into_iter().next().unwrap();

    // Merge with user/host config if available.
    if let Some(user_path) = user_config_path {
        if user_path.is_file() {
            match UserHostConfig::load_from_file(user_path) {
                Ok(user) => active = active.merge_with_user(&user),
                Err(error @ TeamConfigError::DecryptUserField { .. }) => {
                    return Err(error);
                }
                Err(e) => tracing::warn!(
                    path = %user_path.display(),
                    error = %e,
                    "failed to load user config"
                ),
            }
        } else if let Some(ref team_config_file) = active.user_host_config_file {
            // Fall back to the path specified in team.json.
            let expanded = TeamConfig::expand_tilde(team_config_file);
            if expanded.is_file() {
                match UserHostConfig::load_from_file(&expanded) {
                    Ok(user) => active = active.merge_with_user(&user),
                    Err(error @ TeamConfigError::DecryptUserField { .. }) => {
                        return Err(error);
                    }
                    Err(e) => tracing::warn!(
                        path = %expanded.display(),
                        error = %e,
                        "failed to load user config"
                    ),
                }
            }
        }
    }

    Ok(active)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config_crypto::{
        encrypt_value, generate_keypair, CONFIG_KEY_FILE_ENV_VAR, CONFIG_KEY_FILE_SETTING,
    };
    use crate::user_config::UserConfig;
    use age::secrecy::ExposeSecret;
    use serde_json::json;
    use std::env;
    use std::ffi::OsString;
    use std::io::Write;
    use tempfile::TempDir;

    struct EnvVarGuard {
        previous: Vec<(&'static str, Option<OsString>)>,
    }

    impl EnvVarGuard {
        fn set_many(entries: &[(&'static str, Option<&Path>)]) -> Self {
            let mut previous = Vec::new();

            for (key, value) in entries {
                previous.push((*key, env::var_os(key)));
                match value {
                    Some(value) => env::set_var(key, value),
                    None => env::remove_var(key),
                }
            }

            Self { previous }
        }
    }

    impl Drop for EnvVarGuard {
        fn drop(&mut self) {
            for (key, previous) in &self.previous {
                match previous {
                    Some(value) => env::set_var(key, value),
                    None => env::remove_var(key),
                }
            }
        }
    }

    fn with_env_lock<T>(test_fn: impl FnOnce() -> T) -> T {
        let _lock = crate::env_test_lock::MUTEX
            .lock()
            .unwrap_or_else(|poison| poison.into_inner());
        test_fn()
    }

    fn create_test_bundle(
        temp: &TempDir,
        namespace: &str,
        name: &str,
        team_json: Option<&str>,
    ) -> crate::discovery::BundleInfo {
        let bundle_dir = temp.path().join(namespace).join(name);
        fs::create_dir_all(bundle_dir.join(".envoy")).unwrap();

        if let Some(json) = team_json {
            let mut f = fs::File::create(bundle_dir.join(".envoy").join("team.json")).unwrap();
            write!(f, "{}", json).unwrap();
        }

        crate::discovery::BundleInfo::new(
            bundle_dir.clone(),
            name.to_string(),
            namespace.to_string(),
        )
    }

    #[test]
    fn load_team_config_parses_correctly() {
        let temp = TempDir::new().unwrap();
        let path = temp.path().join("team.json");
        fs::write(
            &path,
            r#"{
                "name": "bfd",
                "prodBundlesRoot": "\\\\server\\bundles",
                "prodStacksRoot": "\\\\server\\stacks"
            }"#,
        )
        .unwrap();

        let config = TeamConfig::load_from_file(&path).unwrap();
        assert_eq!(config.name, "bfd");
        assert_eq!(
            config.prod_bundles_root.as_deref(),
            Some(PathBuf::from("\\\\server\\bundles").as_path())
        );
    }

    #[test]
    fn load_team_config_expands_tilde() {
        with_env_lock(|| {
            let temp = TempDir::new().unwrap();
            let path = temp.path().join("team.json");
            fs::write(
                &path,
                r#"{"name": "myteam", "prodBundlesRoot": "~/bundles"}"#,
            )
            .unwrap();

            let _env_guard = EnvVarGuard::set_many(&[("USERPROFILE", Some(temp.path()))]);
            let config = TeamConfig::load_from_file(&path).unwrap();
            let expected_home_bundle = temp.path().join("bundles");
            assert_eq!(
                config.prod_bundles_root.as_deref(),
                Some(expected_home_bundle.as_path())
            );
        });
    }

    #[test]
    fn load_team_config_rejects_missing_name() {
        let temp = TempDir::new().unwrap();
        let path = temp.path().join("team.json");
        fs::write(&path, r#"{"prodBundlesRoot": "\\\\server\\bundles"}"#).unwrap();

        assert!(TeamConfig::load_from_file(&path).is_err());
    }

    #[test]
    fn load_team_config_collects_metadata() {
        let temp = TempDir::new().unwrap();
        let path = temp.path().join("team.json");
        fs::write(
            &path,
            r#"{
                "name": "bfd",
                "prodBundlesRoot": "\\\\server\\bundles",
                "customSetting": true,
                "tags": ["build", "ci"]
            }"#,
        )
        .unwrap();

        let config = TeamConfig::load_from_file(&path).unwrap();
        assert_eq!(config.metadata.len(), 2);
        assert_eq!(
            config
                .metadata
                .get("customSetting")
                .and_then(|v| v.as_bool()),
            Some(true)
        );
    }

    #[test]
    fn load_team_config_accepts_comment_annotated_json() {
        let temp = TempDir::new().unwrap();
        let path = temp.path().join("team.json");
        fs::write(
            &path,
            r#"{
                // Team identifier.
                "name": "bfd",
                "prodBundlesRoot": "\\\\server\\bundles", /* bundle share */
                "prodStacksRoot": "\\\\server\\stacks",
                # extra metadata
                "customSetting": true
            }"#,
        )
        .unwrap();

        let config = TeamConfig::load_from_file(&path).unwrap();
        assert_eq!(config.name, "bfd");
        assert_eq!(
            config.prod_bundles_root.as_deref(),
            Some(PathBuf::from("\\\\server\\bundles").as_path())
        );
        assert_eq!(
            config.prod_stacks_root.as_deref(),
            Some(PathBuf::from("\\\\server\\stacks").as_path())
        );
        assert_eq!(
            config
                .metadata
                .get("customSetting")
                .and_then(|value| value.as_bool()),
            Some(true)
        );
    }

    #[test]
    fn discover_team_configs_finds_all_in_bundles() {
        let temp = TempDir::new().unwrap();
        let bundles = vec![
            create_test_bundle(&temp, "bfd", "build-stack", Some(r#"{"name": "bfd"}"#)),
            create_test_bundle(&temp, "gt", "test-bundle", None), // no team.json
        ];

        let configs = discover_team_configs(&bundles);
        assert_eq!(configs.len(), 1);
        assert_eq!(configs[0].name, "bfd");
    }

    #[test]
    fn resolve_team_config_returns_error_when_none_found() {
        let temp = TempDir::new().unwrap();
        create_test_bundle(&temp, "gt", "no-team-bundle", None);

        let bundles = vec![crate::discovery::BundleInfo::new(
            temp.path().join("gt").join("no-team-bundle"),
            "no-team-bundle".to_string(),
            "gt".to_string(),
        )];

        assert!(resolve_team_config(&bundles, None).is_err());
    }

    #[test]
    fn merge_with_user_overrides_team_defaults() {
        let temp = TempDir::new().unwrap();
        let path = temp.path().join("team.json");
        fs::write(
            &path,
            r#"{
                "name": "bfd",
                "prodBundlesRoot": "\\\\server\\bundles"
            }"#,
        )
        .unwrap();

        let team = TeamConfig::load_from_file(&path).unwrap();
        let user = UserHostConfig {
            prod_bundles_root: "\\\\local\\bundles".to_string(),
            ..Default::default()
        };

        let merged = team.merge_with_user(&user);
        let expected_local = PathBuf::from("\\\\local\\bundles");
        assert_eq!(
            merged.prod_bundles_root.as_deref(),
            Some(expected_local.as_path())
        );
    }

    #[test]
    fn merge_preserves_team_when_user_empty() {
        let temp = TempDir::new().unwrap();
        let path = temp.path().join("team.json");
        fs::write(
            &path,
            r#"{
                "name": "bfd",
                "prodBundlesRoot": "\\\\server\\bundles"
            }"#,
        )
        .unwrap();

        let team = TeamConfig::load_from_file(&path).unwrap();
        let user = UserHostConfig {
            ..Default::default()
        };

        let merged = team.merge_with_user(&user);
        let expected_server = PathBuf::from("\\\\server\\bundles");
        assert_eq!(
            merged.prod_bundles_root.as_deref(),
            Some(expected_server.as_path())
        );
    }

    #[test]
    fn load_user_host_config_parses_correctly() {
        let temp = TempDir::new().unwrap();
        let path = temp.path().join("user.json");
        fs::write(
            &path,
            r#"{
                "prodBundlesRoot": "\\\\local\\bundles",
                "customKey": 42
            }"#,
        )
        .unwrap();

        let user = UserHostConfig::load_from_file(&path).unwrap();
        assert_eq!(user.prod_bundles_root, "\\\\local\\bundles");
        assert_eq!(user.metadata.len(), 1);
    }

    #[test]
    fn load_user_host_config_accepts_comment_annotated_json() {
        let temp = TempDir::new().unwrap();
        let path = temp.path().join("user.json");
        fs::write(
            &path,
            r#"{
                "prodBundlesRoot": "\\\\local\\bundles", // override bundles
                "prodStacksRoot": "\\\\local\\stacks", /* override stacks */
                # retained as metadata
                "customKey": 42
            }"#,
        )
        .unwrap();

        let user = UserHostConfig::load_from_file(&path).unwrap();
        assert_eq!(user.prod_bundles_root, "\\\\local\\bundles");
        assert_eq!(user.prod_stacks_root, "\\\\local\\stacks");
        assert_eq!(
            user.metadata
                .get("customKey")
                .and_then(|value| value.as_i64()),
            Some(42)
        );
    }

    #[test]
    fn load_user_host_config_decrypts_encrypted_fields_with_configured_key() {
        with_env_lock(|| {
            let temp = TempDir::new().unwrap();
            let user_config_path = temp.path().join("user.json");
            let shared_user_config_path = temp.path().join("envoy_user_config.json");
            let key_file_path = temp.path().join("config.agekey");
            let plain_stacks_root = "\\\\local\\stacks";
            let decrypted_bundles_root = "\\\\secure\\bundles";
            let (identity, recipient) = generate_keypair();
            let encrypted_bundles_root = encrypt_value(decrypted_bundles_root, &recipient).unwrap();
            let encoded_identity = identity.to_string();

            fs::write(
                &key_file_path,
                format!(
                    "# envoy config encryption key\n{}\n",
                    encoded_identity.expose_secret()
                ),
            )
            .unwrap();
            fs::write(&shared_user_config_path, "{}").unwrap();
            fs::write(
                &user_config_path,
                json!({
                    "prodBundlesRoot": encrypted_bundles_root,
                    "prodStacksRoot": plain_stacks_root,
                    "customKey": 42,
                })
                .to_string(),
            )
            .unwrap();

            let _env_guard = EnvVarGuard::set_many(&[
                (CONFIG_KEY_FILE_ENV_VAR, Some(key_file_path.as_path())),
                ("ENVOY_USER_CONFIG", Some(shared_user_config_path.as_path())),
            ]);

            let user = UserHostConfig::load_from_file(&user_config_path).unwrap();

            assert_eq!(user.prod_bundles_root, decrypted_bundles_root);
            assert_eq!(user.prod_stacks_root, plain_stacks_root);
            assert_eq!(
                user.metadata
                    .get("customKey")
                    .and_then(|value| value.as_i64()),
                Some(42)
            );
        });
    }

    #[test]
    fn load_user_host_config_errors_when_encrypted_field_has_no_key() {
        with_env_lock(|| {
            let temp = TempDir::new().unwrap();
            let user_config_path = temp.path().join("user.json");
            let shared_user_config_path = temp.path().join("envoy_user_config.json");
            let decrypted_bundles_root = "\\\\secure\\bundles";
            let (_, recipient) = generate_keypair();
            let encrypted_bundles_root = encrypt_value(decrypted_bundles_root, &recipient).unwrap();

            fs::write(
                &user_config_path,
                json!({
                    "prodBundlesRoot": encrypted_bundles_root,
                    "prodStacksRoot": "\\\\local\\stacks",
                })
                .to_string(),
            )
            .unwrap();

            let mut user_config = UserConfig::load(Some(shared_user_config_path.clone()));
            user_config.set(CONFIG_KEY_FILE_SETTING, "").unwrap();
            user_config.save().unwrap();

            let _env_guard = EnvVarGuard::set_many(&[
                (CONFIG_KEY_FILE_ENV_VAR, None),
                ("ENVOY_USER_CONFIG", Some(shared_user_config_path.as_path())),
            ]);

            let error = UserHostConfig::load_from_file(&user_config_path)
                .expect_err("missing key configuration should fail");

            match error {
                TeamConfigError::DecryptUserField { field, source, .. } => {
                    assert_eq!(field, "prodBundlesRoot");
                    assert!(matches!(
                        source,
                        ConfigCryptoError::MissingKeyFileConfiguration
                    ));
                }
                other => panic!("unexpected error: {other}"),
            }
        });
    }

    #[test]
    fn resolve_team_config_merges_with_encrypted_user_path() {
        with_env_lock(|| {
            let temp = TempDir::new().unwrap();
            create_test_bundle(
                &temp,
                "bfd",
                "build-stack",
                Some(
                    r#"{
                        "name": "bfd",
                        "prodBundlesRoot": "\\\\server\\bundles",
                        "prodStacksRoot": "\\\\server\\stacks"
                    }"#,
                ),
            );

            let user_path = temp.path().join("user.json");
            let shared_user_config_path = temp.path().join("envoy_user_config.json");
            let key_file_path = temp.path().join("config.agekey");
            let (identity, recipient) = generate_keypair();
            let encrypted_bundles_root = encrypt_value("\\\\secure\\bundles", &recipient).unwrap();
            let encoded_identity = identity.to_string();

            fs::write(
                &key_file_path,
                format!(
                    "# envoy config encryption key\n{}\n",
                    encoded_identity.expose_secret()
                ),
            )
            .unwrap();
            fs::write(&shared_user_config_path, "{}").unwrap();
            fs::write(
                &user_path,
                json!({
                    "prodBundlesRoot": encrypted_bundles_root,
                    "prodStacksRoot": "\\\\local\\stacks",
                })
                .to_string(),
            )
            .unwrap();

            let _env_guard = EnvVarGuard::set_many(&[
                (CONFIG_KEY_FILE_ENV_VAR, Some(key_file_path.as_path())),
                ("ENVOY_USER_CONFIG", Some(shared_user_config_path.as_path())),
            ]);

            let bundles = vec![crate::discovery::BundleInfo::new(
                temp.path().join("bfd").join("build-stack"),
                "build-stack".to_string(),
                "bfd".to_string(),
            )];

            let config = resolve_team_config(&bundles, Some(user_path.as_ref())).unwrap();

            assert_eq!(
                config.prod_bundles_root.as_deref(),
                Some(Path::new("\\\\secure\\bundles"))
            );
            assert_eq!(
                config.prod_stacks_root.as_deref(),
                Some(Path::new("\\\\local\\stacks"))
            );
        });
    }

    #[test]
    fn resolve_team_config_errors_when_encrypted_user_path_has_no_key() {
        with_env_lock(|| {
            let temp = TempDir::new().unwrap();
            create_test_bundle(&temp, "bfd", "build-stack", Some(r#"{"name": "bfd"}"#));

            let user_path = temp.path().join("user.json");
            let shared_user_config_path = temp.path().join("envoy_user_config.json");
            let (_, recipient) = generate_keypair();
            let encrypted_bundles_root = encrypt_value("\\\\secure\\bundles", &recipient).unwrap();

            fs::write(&shared_user_config_path, "{}").unwrap();
            fs::write(
                &user_path,
                json!({
                    "prodBundlesRoot": encrypted_bundles_root,
                })
                .to_string(),
            )
            .unwrap();

            let _env_guard = EnvVarGuard::set_many(&[
                (CONFIG_KEY_FILE_ENV_VAR, None),
                ("ENVOY_USER_CONFIG", Some(shared_user_config_path.as_path())),
            ]);

            let bundles = vec![crate::discovery::BundleInfo::new(
                temp.path().join("bfd").join("build-stack"),
                "build-stack".to_string(),
                "bfd".to_string(),
            )];

            let error = resolve_team_config(&bundles, Some(user_path.as_ref()))
                .expect_err("missing key configuration should fail");

            assert!(matches!(error, TeamConfigError::DecryptUserField { .. }));
        });
    }

    #[test]
    fn resolve_team_config_merges_with_user_path() {
        let temp = TempDir::new().unwrap();
        create_test_bundle(&temp, "bfd", "build-stack", Some(r#"{"name": "bfd"}"#));

        // Create a user config file.
        let user_path = temp.path().join("user.json");
        fs::write(
            &user_path,
            r#"{ "prodBundlesRoot": "\\\\override\\bundles" }"#,
        )
        .unwrap();

        let bundles = vec![crate::discovery::BundleInfo::new(
            temp.path().join("bfd").join("build-stack"),
            "build-stack".to_string(),
            "bfd".to_string(),
        )];

        let config = resolve_team_config(&bundles, Some(user_path.as_ref())).unwrap();
        let expected_override = PathBuf::from("\\\\override\\bundles");
        assert_eq!(
            config.prod_bundles_root.as_deref(),
            Some(expected_override.as_path())
        );
    }

    #[test]
    fn empty_team_config_has_no_fields() {
        let config = TeamConfig::empty();
        assert_eq!(config.name, "");
        assert!(config.prod_bundles_root.is_none());
        assert!(config.metadata.is_empty());
    }
}
