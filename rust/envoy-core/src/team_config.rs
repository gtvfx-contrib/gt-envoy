//! Team configuration loading and merging for `envoy`.
//!
//! This module implements team configuration discovery. Each bundle's
//! `.envoy/team.json` defines team-level settings (package roots, pipeline
//! roots, user config paths). These are merged with per-user host configs to
//! produce the final resolved [`TeamConfig`] used throughout envoy.

use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use thiserror::Error;

/// Error type for team configuration operations.
#[derive(Debug, Error)]
pub enum TeamConfigError {
    #[error("I/O error reading team config at {path}: {source}")]
    Io { path: PathBuf, source: std::io::Error },

    #[error("invalid JSON in team config at {path}: {source}")]
    Json { path: PathBuf, source: serde_json::Error },

    #[error("team '{name}' not found; no .envoy/team.json discovered")]
    NotFound { name: String },

    #[error("missing required field '{field}' in team config at {path}")]
    MissingField { field: String, path: PathBuf },
}

/// A resolved team configuration with all settings merged.
#[derive(Clone, Debug)]
pub struct TeamConfig {
    /// Human-readable team name (e.g., `"bfd"`).
    pub name: String,
    /// Absolute or UNC path to the production packages root directory.
    pub prod_packages_root: Option<PathBuf>,
    /// Absolute or UNC path to the production pipelines root directory.
    pub prod_pipelines_root: Option<PathBuf>,
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
            prod_packages_root: None,
            prod_pipelines_root: None,
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

        let value: serde_json::Value = serde_json::from_str(&content)
            .map_err(|e| TeamConfigError::Json {
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

        let name = value.get("name")
            .and_then(|v| v.as_str())
            .ok_or_else(|| TeamConfigError::MissingField {
                field: "name".to_string(),
                path: path.to_path_buf(),
            })?;

        // Parse optional fields.
        let prod_packages_root = value.get("prodPackagesRoot")
            .and_then(|v| v.as_str())
            .map(Self::expand_tilde);

        let prod_pipelines_root = value.get("prodPipelinesRoot")
            .and_then(|v| v.as_str())
            .map(Self::expand_tilde);

        let user_host_config_file = value.get("userHostConfigFile")
            .and_then(|v| v.as_str())
            .map(String::from);

        // Collect remaining fields as metadata.
        let mut metadata = HashMap::new();
        for (key, val) in value.as_object().unwrap() {
            if !matches!(key.as_str(), "name" | "prodPackagesRoot" | "prodPipelinesRoot" | "userHostConfigFile") {
                metadata.insert(key.clone(), val.clone());
            }
        }

        Ok(TeamConfig {
            name: name.to_string(),
            prod_packages_root,
            prod_pipelines_root,
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
        if !user.prod_packages_root.is_empty() {
            merged.prod_packages_root = Some(PathBuf::from(&user.prod_packages_root));
        }
        if !user.prod_pipelines_root.is_empty() {
            merged.prod_pipelines_root = Some(PathBuf::from(&user.prod_pipelines_root));
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
    /// Override for the production packages root (empty = use team default).
    pub prod_packages_root: String,
    /// Override for the production pipelines root (empty = use team default).
    pub prod_pipelines_root: String,
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

        let value: serde_json::Value = serde_json::from_str(&content)
            .map_err(|e| TeamConfigError::Json {
                path: path.to_path_buf(),
                source: e,
            })?;

        if !value.is_object() {
            return Err(TeamConfigError::MissingField {
                field: "object".to_string(),
                path: path.to_path_buf(),
            });
        }

        let prod_packages_root = value.get("prodPackagesRoot")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();

        let prod_pipelines_root = value.get("prodPipelinesRoot")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();

        // Collect remaining fields as metadata.
        let mut metadata = HashMap::new();
        for (key, val) in value.as_object().unwrap() {
            if !matches!(key.as_str(), "prodPackagesRoot" | "prodPipelinesRoot") {
                metadata.insert(key.clone(), val.clone());
            }
        }

        Ok(UserHostConfig {
            prod_packages_root,
            prod_pipelines_root,
            metadata,
        })
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
            Err(e) => eprintln!("Warning: failed to load team config from {}: {}", team_path.display(), e),
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
        return Err(TeamConfigError::NotFound { name: String::new() });
    }

    // Use the first discovered team config.
    let mut active = configs.into_iter().next().unwrap();

    // Merge with user/host config if available.
    if let Some(user_path) = user_config_path {
        if user_path.is_file() {
            match UserHostConfig::load_from_file(user_path) {
                Ok(user) => active = active.merge_with_user(&user),
                Err(e) => eprintln!("Warning: failed to load user config from {}: {}", user_path.display(), e),
            }
        } else if let Some(ref team_config_file) = active.user_host_config_file {
            // Fall back to the path specified in team.json.
            let expanded = TeamConfig::expand_tilde(team_config_file);
            if expanded.is_file() {
                match UserHostConfig::load_from_file(&expanded) {
                    Ok(user) => active = active.merge_with_user(&user),
                    Err(e) => eprintln!("Warning: failed to load user config from {}: {}", expanded.display(), e),
                }
            }
        }
    }

    Ok(active)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use tempfile::TempDir;

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
                "prodPackagesRoot": "\\\\server\\packages",
                "prodPipelinesRoot": "\\\\server\\pipelines"
            }"#,
        )
        .unwrap();

        let config = TeamConfig::load_from_file(&path).unwrap();
        assert_eq!(config.name, "bfd");
        assert_eq!(
            config.prod_packages_root.as_deref(),
            Some(PathBuf::from("\\\\server\\packages").as_path())
        );
    }

    #[test]
    fn load_team_config_expands_tilde() {
        let temp = TempDir::new().unwrap();
        let path = temp.path().join("team.json");
        fs::write(&path, r#"{"name": "myteam", "prodPackagesRoot": "~/packages"}"#).unwrap();

        // Set USERPROFILE for the test.
        std::env::set_var("USERPROFILE", &temp.path());
        let config = TeamConfig::load_from_file(&path).unwrap();
        let expected_home_pkg = temp.path().join("packages");
        assert_eq!(config.prod_packages_root.as_deref(), Some(expected_home_pkg.as_path()));
    }

    #[test]
    fn load_team_config_rejects_missing_name() {
        let temp = TempDir::new().unwrap();
        let path = temp.path().join("team.json");
        fs::write(&path, r#"{"prodPackagesRoot": "\\\\server\\packages"}"#).unwrap();

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
                "prodPackagesRoot": "\\\\server\\packages",
                "customSetting": true,
                "tags": ["build", "ci"]
            }"#,
        )
        .unwrap();

        let config = TeamConfig::load_from_file(&path).unwrap();
        assert_eq!(config.metadata.len(), 2);
        assert_eq!(config.metadata.get("customSetting").and_then(|v| v.as_bool()), Some(true));
    }

    #[test]
    fn discover_team_configs_finds_all_in_bundles() {
        let temp = TempDir::new().unwrap();
        let bundles = vec![
            create_test_bundle(
                &temp,
                "bfd",
                "build-pipeline",
                Some(r#"{"name": "bfd"}"#),
            ),
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
                "prodPackagesRoot": "\\\\server\\packages"
            }"#,
        )
        .unwrap();

        let team = TeamConfig::load_from_file(&path).unwrap();
        let user = UserHostConfig {
            prod_packages_root: "\\\\local\\packages".to_string(),
            ..Default::default()
        };

        let merged = team.merge_with_user(&user);
        let expected_local = PathBuf::from("\\\\local\\packages");
        assert_eq!(merged.prod_packages_root.as_deref(), Some(expected_local.as_path()));
    }

    #[test]
    fn merge_preserves_team_when_user_empty() {
        let temp = TempDir::new().unwrap();
        let path = temp.path().join("team.json");
        fs::write(
            &path,
            r#"{
                "name": "bfd",
                "prodPackagesRoot": "\\\\server\\packages"
            }"#,
        )
        .unwrap();

        let team = TeamConfig::load_from_file(&path).unwrap();
        let user = UserHostConfig { ..Default::default() };

        let merged = team.merge_with_user(&user);
        let expected_server = PathBuf::from("\\\\server\\packages");
        assert_eq!(merged.prod_packages_root.as_deref(), Some(expected_server.as_path()));
    }

    #[test]
    fn load_user_host_config_parses_correctly() {
        let temp = TempDir::new().unwrap();
        let path = temp.path().join("user.json");
        fs::write(
            &path,
            r#"{
                "prodPackagesRoot": "\\\\local\\packages",
                "customKey": 42
            }"#,
        )
        .unwrap();

        let user = UserHostConfig::load_from_file(&path).unwrap();
        assert_eq!(user.prod_packages_root, "\\\\local\\packages");
        assert_eq!(user.metadata.len(), 1);
    }

    #[test]
    fn resolve_team_config_merges_with_user_path() {
        let temp = TempDir::new().unwrap();
        create_test_bundle(
            &temp,
            "bfd",
            "build-pipeline",
            Some(r#"{"name": "bfd"}"#),
        );

        // Create a user config file.
        let user_path = temp.path().join("user.json");
        fs::write(&user_path, r#"{ "prodPackagesRoot": "\\\\override\\packages" }"#).unwrap();

        let bundles = vec![crate::discovery::BundleInfo::new(
            temp.path().join("bfd").join("build-pipeline"),
            "build-pipeline".to_string(),
            "bfd".to_string(),
        )];

        let config = resolve_team_config(&bundles, Some(user_path.as_ref())).unwrap();
        let expected_override = PathBuf::from("\\\\override\\packages");
        assert_eq!(config.prod_packages_root.as_deref(), Some(expected_override.as_path()));
    }

    #[test]
    fn empty_team_config_has_no_fields() {
        let config = TeamConfig::empty();
        assert_eq!(config.name, "");
        assert!(config.prod_packages_root.is_none());
        assert!(config.metadata.is_empty());
    }
}
