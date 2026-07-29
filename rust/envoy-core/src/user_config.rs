//! Persistent user configuration for envoy.
//!
//! This module ports `py/envoy/_user_config.py` into `envoy-core`.
//! It stores per-user preferences in a platform-appropriate JSON file so
//! flags and paths do not need to be repeated on every invocation.
//!
//! Config file locations:
//! - Windows: `%APPDATA%\envoy\user_config.json`
//! - macOS/Linux: `~/.config/envoy/user_config.json`
//!
//! Settings are stored as a flat JSON object. Use [`UserConfig::load`] to read
//! the config and [`UserConfig::save`] to persist changes.

use std::collections::{BTreeMap, HashMap};
use std::env;
use std::fmt;
use std::fs;
use std::path::PathBuf;

use serde_json::Value;

use crate::error::{EnvoyError, Result};
use crate::json_util::parse_json_with_comments;

/// Metadata describing one supported user-config setting.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct KnownSetting {
    /// Human-readable explanation of what the setting controls.
    pub description: &'static str,
    /// Allowed string values, or `None` when the setting is free-form.
    pub choices: Option<&'static [&'static str]>,
}

const VERBOSITY_CHOICES: &[&str] = &["quiet", "normal", "verbose"];

const KNOWN_SETTINGS: [(&str, KnownSetting); 4] = [
    (
        "stack",
        KnownSetting {
            description: "Path to the default named stack or .estack file.  Used when \
--stack is not supplied on the command line.",
            choices: None,
        },
    ),
    (
        "config_key_file",
        KnownSetting {
            description: "Path to an age identity file used to decrypt opt-in \
encrypted values in `.envoy/*.json` config files. Set to an empty string to leave config \
decryption disabled unless ENVOY_CONFIG_KEY_FILE is set.",
            choices: None,
        },
    ),
    (
        "verbosity",
        KnownSetting {
            description: "Default verbosity level for all envoy invocations.",
            choices: Some(VERBOSITY_CHOICES),
        },
    ),
    (
        "bundle_cache_dir",
        KnownSetting {
            description: "Directory used for the local bundle cache. Set to an empty \
string to fall back to the platform default location. See also the ENVOY_BUNDLE_CACHE \
and ENVOY_DISABLE_BUNDLE_CACHE environment variables.",
            choices: None,
        },
    ),
];

/// Return the registry of all settings that can be stored in the user config.
///
/// A plain static slice is used instead of a map because the registry is tiny
/// and linear lookup keeps the implementation dependency-free.
pub fn known_settings() -> &'static [(&'static str, KnownSetting)] {
    &KNOWN_SETTINGS
}

/// Return the platform-appropriate default user config file path.
///
/// This mirrors the Python implementation:
/// - Windows reads `%APPDATA%`, falling back to `%USERPROFILE%`
/// - non-Windows reads `$XDG_CONFIG_HOME`, falling back to `$HOME/.config`
///
/// Unlike Python's `Path.home()`, this implementation reads environment
/// variables directly to avoid adding another dependency just for home
/// directory discovery. If the expected home-related environment variables are
/// unavailable, a relative fallback is returned instead of panicking.
pub fn default_config_path() -> PathBuf {
    #[cfg(target_os = "windows")]
    {
        windows_default_config_path()
    }

    #[cfg(not(target_os = "windows"))]
    {
        non_windows_default_config_path()
    }
}

/// Return the effective user config path, honoring `ENVOY_USER_CONFIG`.
pub fn user_config_path() -> PathBuf {
    env::var_os("ENVOY_USER_CONFIG")
        .map(PathBuf::from)
        .unwrap_or_else(default_config_path)
}

/// Persistent user configuration for envoy.
///
/// The config is loaded from and saved to [`user_config_path`] unless an
/// explicit path override is supplied to [`UserConfig::load`].
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct UserConfig {
    data: HashMap<String, String>,
    /// Filesystem path where this config instance is loaded from and saved to.
    pub path: PathBuf,
}

impl UserConfig {
    /// Load the user config from disk.
    ///
    /// Returns an empty config when the file does not exist, cannot be read,
    /// or cannot be parsed as a JSON object. This mirrors the Python contract:
    /// missing or corrupt config files never raise at load time.
    ///
    /// When `path` is `None`, the current `ENVOY_USER_CONFIG` environment
    /// variable is checked at call time before falling back to the default
    /// platform-specific location.
    ///
    /// JSON object values are coerced into strings to match the Python port's
    /// permissive `str(value)` behavior. String values are preserved verbatim;
    /// other JSON values use a simple Rust/JSON stringification.
    pub fn load(path: Option<PathBuf>) -> Self {
        let config_path = path.unwrap_or_else(user_config_path);
        let text = match fs::read_to_string(&config_path) {
            Ok(text) => text,
            Err(_) => {
                return Self {
                    data: HashMap::new(),
                    path: config_path,
                };
            }
        };

        let value = match parse_json_with_comments::<Value>(&text) {
            Ok(value) => value,
            Err(_) => {
                return Self {
                    data: HashMap::new(),
                    path: config_path,
                };
            }
        };

        let data = match value {
            Value::Object(entries) => entries
                .into_iter()
                .map(|(key, value)| (key, stringify_json_value(&value)))
                .collect(),
            _ => HashMap::new(),
        };

        Self {
            data,
            path: config_path,
        }
    }

    /// Save the current config to disk.
    ///
    /// Parent directories are created as needed. The emitted JSON is pretty
    /// printed with sorted keys so on-disk output is deterministic and mirrors
    /// Python's `json.dumps(..., indent=2, sort_keys=True)`.
    pub fn save(&self) -> Result<()> {
        if let Some(parent) = self.path.parent() {
            if !parent.as_os_str().is_empty() {
                fs::create_dir_all(parent).map_err(|source| EnvoyError::Io {
                    path: self.path.clone(),
                    source,
                })?;
            }
        }

        let sorted: BTreeMap<String, String> = self
            .data
            .iter()
            .map(|(key, value)| (key.clone(), value.clone()))
            .collect();
        let json = serde_json::to_string_pretty(&sorted).map_err(|source| EnvoyError::Json {
            path: self.path.clone(),
            source,
        })?;

        fs::write(&self.path, json).map_err(|source| EnvoyError::Io {
            path: self.path.clone(),
            source,
        })?;

        Ok(())
    }

    /// Return the value of `key`, or `None` if it is not set.
    pub fn get(&self, key: &str) -> Option<&str> {
        self.data.get(key).map(String::as_str)
    }

    /// Set `key` to `value` in memory.
    ///
    /// Call [`UserConfig::save`] to persist the change. Unknown keys and
    /// invalid choice values are reported as [`EnvoyError::Validation`].
    pub fn set(&mut self, key: &str, value: &str) -> Result<()> {
        let setting = known_setting(key).ok_or_else(|| {
            let known = known_settings()
                .iter()
                .map(|(name, _)| *name)
                .collect::<Vec<_>>()
                .join(", ");

            EnvoyError::Validation(format!(
                "Unknown config setting {key:?}. Known settings: {known}"
            ))
        })?;

        if let Some(choices) = setting.choices {
            if !choices.contains(&value) {
                return Err(EnvoyError::Validation(format!(
                    "Invalid value {value:?} for {key:?}. Valid choices: {}",
                    choices.join(", ")
                )));
            }
        }

        self.data.insert(key.to_string(), value.to_string());
        Ok(())
    }

    /// Remove `key` from the config.
    ///
    /// Returns `true` if the key existed.
    pub fn unset(&mut self, key: &str) -> bool {
        self.data.remove(key).is_some()
    }

    /// Return a copy of all currently stored settings.
    pub fn items(&self) -> HashMap<String, String> {
        self.data.clone()
    }

    /// Return whether the config currently has no stored settings.
    ///
    /// This mirrors the false-y behavior of Python's `__bool__`.
    pub fn is_empty(&self) -> bool {
        self.data.is_empty()
    }
}

impl fmt::Display for UserConfig {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            formatter,
            "UserConfig(path={}, settings={})",
            self.path.display(),
            format_settings(&self.data)
        )
    }
}

#[cfg(target_os = "windows")]
fn windows_default_config_path() -> PathBuf {
    env::var_os("APPDATA")
        .map(PathBuf::from)
        .or_else(|| env::var_os("USERPROFILE").map(PathBuf::from))
        .unwrap_or_else(|| PathBuf::from("."))
        .join("envoy")
        .join("user_config.json")
}

#[cfg(not(target_os = "windows"))]
fn non_windows_default_config_path() -> PathBuf {
    if let Some(config_home) = non_empty_env_path("XDG_CONFIG_HOME") {
        return config_home.join("envoy").join("user_config.json");
    }

    env::var_os("HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("."))
        .join(".config")
        .join("envoy")
        .join("user_config.json")
}

#[cfg(not(target_os = "windows"))]
fn non_empty_env_path(name: &str) -> Option<PathBuf> {
    env::var_os(name).and_then(|value| {
        if value.is_empty() {
            None
        } else {
            Some(PathBuf::from(value))
        }
    })
}

fn known_setting(key: &str) -> Option<&'static KnownSetting> {
    known_settings().iter().find_map(
        |(name, setting)| {
            if *name == key {
                Some(setting)
            } else {
                None
            }
        },
    )
}

fn stringify_json_value(value: &Value) -> String {
    match value {
        Value::String(text) => text.clone(),
        Value::Number(number) => number.to_string(),
        Value::Bool(flag) => flag.to_string(),
        Value::Null => String::from("null"),
        Value::Array(_) | Value::Object(_) => value.to_string(),
    }
}

fn format_settings(data: &HashMap<String, String>) -> String {
    if data.is_empty() {
        return String::from("{}");
    }

    let mut pairs: Vec<_> = data.iter().collect();
    pairs.sort_by(|left, right| left.0.cmp(right.0));

    let rendered = pairs
        .into_iter()
        .map(|(key, value)| {
            format!(
                "'{}': '{}'",
                escape_repr_string(key),
                escape_repr_string(value)
            )
        })
        .collect::<Vec<_>>()
        .join(", ");

    format!("{{{rendered}}}")
}

fn escape_repr_string(text: &str) -> String {
    text.replace('\\', "\\\\").replace('\'', "\\'")
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::path::Path;

    use tempfile::tempdir;

    use super::{default_config_path, UserConfig};
    use crate::error::EnvoyError;

    #[test]
    fn load_nonexistent_file_returns_empty_config() {
        let temp_dir = tempdir().expect("tempdir should be created");
        let path = temp_dir.path().join("missing.json");

        let config = UserConfig::load(Some(path.clone()));

        assert!(config.is_empty());
        assert_eq!(config.path, path);
    }

    #[test]
    fn load_invalid_json_returns_empty_config() {
        let temp_dir = tempdir().expect("tempdir should be created");
        let path = temp_dir.path().join("invalid.json");
        fs::write(&path, "{not valid json").expect("invalid test file should be written");

        let config = UserConfig::load(Some(path.clone()));

        assert!(config.is_empty());
        assert_eq!(config.path, path);
    }

    #[test]
    fn save_then_load_round_trips_settings() {
        let temp_dir = tempdir().expect("tempdir should be created");
        let path = temp_dir.path().join("nested").join("user_config.json");
        let mut config = UserConfig::load(Some(path.clone()));

        config
            .set("stack", "C:\\studio\\envoy\\studio.estack")
            .expect("stack should be accepted");
        config
            .set("verbosity", "verbose")
            .expect("verbosity should be accepted");
        config.save().expect("config should save");

        let reloaded = UserConfig::load(Some(path.clone()));

        assert_eq!(
            reloaded.get("stack"),
            Some("C:\\studio\\envoy\\studio.estack")
        );
        assert_eq!(reloaded.get("verbosity"), Some("verbose"));
        assert_eq!(reloaded.path, path);
    }

    #[test]
    fn load_comment_annotated_json_returns_expected_settings() {
        let temp_dir = tempdir().expect("tempdir should be created");
        let path = temp_dir.path().join("user_config.json");
        fs::write(
            &path,
            r#"{
                // default Stack
                "stack": "C:\\studio\\envoy\\studio.estack",
                "verbosity": "verbose", /* CLI default */
                # cache override
                "bundle_cache_dir": "C:\\cache"
            }"#,
        )
        .expect("commented config should be written");

        let config = UserConfig::load(Some(path.clone()));

        assert_eq!(
            config.get("stack"),
            Some("C:\\studio\\envoy\\studio.estack")
        );
        assert_eq!(config.get("verbosity"), Some("verbose"));
        assert_eq!(config.get("bundle_cache_dir"), Some("C:\\cache"));
        assert_eq!(config.path, path);
    }

    #[test]
    fn set_rejects_unknown_key() {
        let temp_dir = tempdir().expect("tempdir should be created");
        let path = temp_dir.path().join("user_config.json");
        let mut config = UserConfig::load(Some(path));

        let error = config
            .set("unknown_setting", "value")
            .expect_err("unknown key should fail validation");

        assert!(matches!(error, EnvoyError::Validation(_)));
        assert_eq!(
            error.to_string(),
            "validation error: Unknown config setting \"unknown_setting\". Known settings: \
stack, config_key_file, verbosity, bundle_cache_dir"
        );
    }

    #[test]
    fn set_validates_choice_values() {
        let temp_dir = tempdir().expect("tempdir should be created");
        let path = temp_dir.path().join("user_config.json");
        let mut config = UserConfig::load(Some(path));

        let error = config
            .set("verbosity", "bogus")
            .expect_err("invalid verbosity should fail validation");

        assert!(matches!(error, EnvoyError::Validation(_)));
        assert_eq!(
            error.to_string(),
            "validation error: Invalid value \"bogus\" for \"verbosity\". Valid choices: quiet, \
normal, verbose"
        );

        config
            .set("verbosity", "quiet")
            .expect("valid verbosity should succeed");

        assert_eq!(config.get("verbosity"), Some("quiet"));
    }

    #[test]
    fn set_accepts_config_key_file() {
        let temp_dir = tempdir().expect("tempdir should be created");
        let path = temp_dir.path().join("user_config.json");
        let mut config = UserConfig::load(Some(path));

        config
            .set("config_key_file", "C:\\keys\\envoy.agekey")
            .expect("config_key_file should be accepted");

        assert_eq!(
            config.get("config_key_file"),
            Some("C:\\keys\\envoy.agekey")
        );
    }

    #[test]
    fn unset_reports_whether_key_existed() {
        let temp_dir = tempdir().expect("tempdir should be created");
        let path = temp_dir.path().join("user_config.json");
        let mut config = UserConfig::load(Some(path));
        config
            .set("stack", "studio.estack")
            .expect("stack should be accepted");

        assert!(config.unset("stack"));
        assert!(!config.unset("stack"));
    }

    #[test]
    fn default_config_path_ends_with_expected_filename() {
        let path = default_config_path();

        assert!(path.ends_with(Path::new("envoy").join("user_config.json")));
    }
}
