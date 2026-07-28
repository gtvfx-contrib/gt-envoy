//! Command-definition loading and command-environment resolution.
//!
//! This module ports `py/envoy/_commands.py` into `envoy-core`.
//! It is responsible for:
//! - loading `commands.json` files
//! - representing individual command definitions
//! - resolving recursive environment references between commands
//! - locating the nearest `commands.json` by walking parent directories

use std::collections::{HashMap, HashSet};
use std::env;
use std::ffi::OsStr;
use std::fmt;
use std::fs;
use std::path::{Component, Path, PathBuf};

use serde_json::Value;

use crate::discovery::{Bundle, BundleInfo, BUNDLE_ENV_DIR};
use crate::environment::EnvironmentManager;
use crate::error::{EnvoyError, Result};
use crate::json_util::parse_json_with_comments;

/// A command definition loaded from `commands.json`.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CommandDefinition {
    /// Command name, corresponding to the JSON key.
    pub name: String,
    /// Ordered list of environment file names or command references.
    pub environment: Vec<String>,
    /// Optional alias vector replacing the executable and base arguments.
    pub alias: Option<Vec<String>>,
    /// Optional bundle ID supplying this command.
    pub bundle: Option<String>,
    /// Bundle `.envoy/` directory containing referenced environment files.
    pub envoy_env_dir: Option<PathBuf>,
    /// `commands.json` file this definition was loaded from.
    pub source_file: Option<PathBuf>,
    /// Platform override levels applied while loading this definition.
    ///
    /// Values use the canonical Rust target names, for example `linux` and
    /// `x86_64`. An empty list means the base command definition was used.
    pub platform_overrides: Vec<String>,
}

impl CommandDefinition {
    /// Describe how the effective command configuration was resolved.
    pub fn platform_resolution(&self) -> String {
        if self.platform_overrides.is_empty() {
            String::from("base")
        } else {
            format!("base -> {}", self.platform_overrides.join(" -> "))
        }
    }

    /// Return the executable for this command.
    ///
    /// This is `alias[0]` when an alias exists, otherwise `name`.
    pub fn executable(&self) -> &str {
        self.alias
            .as_ref()
            .and_then(|alias| alias.first())
            .map(String::as_str)
            .unwrap_or(&self.name)
    }

    /// Return the base arguments for this command.
    ///
    /// This is `alias[1..]` when present, otherwise an empty slice.
    pub fn base_args(&self) -> &[String] {
        match self.alias.as_ref() {
            Some(alias) if alias.len() > 1 => &alias[1..],
            _ => &[],
        }
    }

    /// Expand this command's alias.
    ///
    /// `${__BUNDLE__}`, `${__BUNDLE_ENV__}`, and `${__BUNDLE_NAME__}` are
    /// resolved from `envoy_env_dir`. Remaining `${VAR}` references are
    /// resolved from `env` when provided.
    ///
    /// When no alias exists, the expanded result is `[self.name]`.
    pub fn expand_alias(&self, env: Option<&HashMap<String, String>>) -> Vec<String> {
        let raw = self
            .alias
            .clone()
            .unwrap_or_else(|| vec![self.name.clone()]);

        let Some(envoy_env_dir) = self.envoy_env_dir.as_ref() else {
            return raw;
        };

        let empty_env = HashMap::new();
        let current_env = env.unwrap_or(&empty_env);
        let bundle_root = envoy_env_dir.parent().unwrap_or(envoy_env_dir.as_path());
        let bundle_name = bundle_root
            .file_name()
            .map(|value| value.to_string_lossy().into_owned())
            .unwrap_or_default();

        let special_vars = HashMap::from([
            (
                String::from("__BUNDLE__"),
                path_to_forward_slashes(bundle_root),
            ),
            (
                String::from("__BUNDLE_ENV__"),
                path_to_forward_slashes(envoy_env_dir),
            ),
            (String::from("__BUNDLE_NAME__"), bundle_name),
        ]);

        raw.into_iter()
            .map(|part| {
                let expanded =
                    EnvironmentManager::expand_env_value(&part, current_env, Some(&special_vars));
                EnvironmentManager::normalize_path(&expanded)
            })
            .collect()
    }
}

impl fmt::Display for CommandDefinition {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        let alias_str = self
            .alias
            .as_ref()
            .map(|alias| format!(" (alias: {})", alias.join(" ")))
            .unwrap_or_default();
        let bundle_str = self
            .bundle
            .as_ref()
            .map(|bundle| format!(" [{bundle}]"))
            .unwrap_or_default();

        write!(
            formatter,
            "CommandDefinition({}{}{alias_str}, env={})",
            self.name,
            bundle_str,
            format_python_list(&self.environment)
        )
    }
}

/// Small accessor contract used by `load_from_bundles`.
pub trait BundleLike {
    /// Return the bundle identifier (`<namespace>:<name>`).
    fn bndlid(&self) -> String;

    /// Return the bundle `.envoy/` directory.
    fn envoy_env(&self) -> &Path;

    /// Return the bundle name.
    fn name(&self) -> &str;
}

impl BundleLike for BundleInfo {
    fn bndlid(&self) -> String {
        BundleInfo::bndlid(self)
    }

    fn envoy_env(&self) -> &Path {
        BundleInfo::envoy_env(self)
    }

    fn name(&self) -> &str {
        &self.name
    }
}

impl BundleLike for Bundle {
    fn bndlid(&self) -> String {
        Bundle::bndlid(self)
    }

    fn envoy_env(&self) -> &Path {
        Bundle::envoy_env(self)
    }

    fn name(&self) -> &str {
        Bundle::name(self)
    }
}

impl<T> BundleLike for &T
where
    T: BundleLike + ?Sized,
{
    fn bndlid(&self) -> String {
        (*self).bndlid()
    }

    fn envoy_env(&self) -> &Path {
        (*self).envoy_env()
    }

    fn name(&self) -> &str {
        (*self).name()
    }
}

/// Registry of available commands.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct CommandRegistry {
    commands: HashMap<String, CommandDefinition>,
    bundle_sources: HashMap<String, String>,
}

impl CommandRegistry {
    /// Create an empty command registry.
    pub fn empty() -> Self {
        Self::default()
    }

    /// Create a registry and optionally load an initial `commands.json`.
    pub fn new(commands_file: Option<&Path>) -> Result<Self> {
        let mut registry = Self::default();
        if let Some(commands_file) = commands_file {
            registry.load_from_file(commands_file, None)?;
        }

        Ok(registry)
    }

    /// Load command definitions from a JSON file.
    pub fn load_from_file(
        &mut self,
        commands_file: &Path,
        bundle_name: Option<&str>,
    ) -> Result<()> {
        if !commands_file.exists() {
            return Err(EnvoyError::EnvironmentBuild(format!(
                "Commands file not found: {}",
                commands_file.display()
            )));
        }

        let wrapper_env_dir = commands_file
            .parent()
            .map(Path::to_path_buf)
            .unwrap_or_default();

        let text = fs::read_to_string(commands_file).map_err(|error| {
            EnvoyError::EnvironmentBuild(format!(
                "Error reading commands file {}: {error}",
                commands_file.display()
            ))
        })?;
        let commands_data = parse_json_with_comments::<Value>(&text).map_err(|error| {
            EnvoyError::EnvironmentBuild(format!(
                "Invalid JSON in commands file {}: {error}",
                commands_file.display()
            ))
        })?;
        let Value::Object(commands_data) = commands_data else {
            return Err(EnvoyError::EnvironmentBuild(format!(
                "Commands file must contain a JSON object: {}",
                commands_file.display()
            )));
        };

        for (command_name, command_config) in commands_data {
            let Some(command_object) = command_config.as_object() else {
                log_warning(&format!(
                    "Skipping invalid command definition: {command_name}"
                ));
                continue;
            };

            let Some(environment_value) = command_object.get("environment") else {
                log_warning(&format!(
                    "Command '{command_name}' missing 'environment' field, skipping"
                ));
                continue;
            };
            let Some(mut environment) = json_array_to_strings(environment_value) else {
                log_warning(&format!(
                    "Command '{command_name}' has invalid 'environment' field, skipping"
                ));
                continue;
            };

            let mut alias = match command_object.get("alias") {
                Some(alias_value) => match json_array_to_strings(alias_value) {
                    Some(alias) => Some(alias),
                    None => {
                        log_warning(&format!(
                            "Command '{command_name}' has invalid 'alias' field, skipping"
                        ));
                        continue;
                    }
                },
                None => None,
            };

            let Some(platform_overrides) = apply_platform_overrides(
                &command_name,
                commands_file,
                command_object.get("platforms"),
                &mut environment,
                &mut alias,
            ) else {
                continue;
            };

            let command_definition = CommandDefinition {
                name: command_name.clone(),
                environment,
                alias,
                bundle: bundle_name.map(ToOwned::to_owned),
                envoy_env_dir: Some(wrapper_env_dir.clone()),
                source_file: Some(commands_file.to_path_buf()),
                platform_overrides,
            };

            if self.commands.contains_key(&command_name) {
                let existing_bundle = self
                    .bundle_sources
                    .get(&command_name)
                    .map(String::as_str)
                    .unwrap_or("unknown");
                log_warning(&format!(
                    "Command '{command_name}' from {} overrides existing command from {existing_bundle}",
                    bundle_name.unwrap_or("local")
                ));
            }

            self.commands
                .insert(command_name.clone(), command_definition);
            self.bundle_sources
                .insert(command_name, bundle_name.unwrap_or("local").to_string());
        }

        Ok(())
    }

    /// Resolve the full, flattened environment file list for a command.
    ///
    /// Entries whose basename contains no dot are treated as references to
    /// another command and are recursively spliced into the result at that
    /// position.
    pub fn resolve_environment(
        &self,
        command_name: &str,
    ) -> Result<Vec<(String, Option<PathBuf>)>> {
        self.resolve_environment_inner(command_name, &mut HashSet::new())
    }

    fn resolve_environment_inner(
        &self,
        command_name: &str,
        seen: &mut HashSet<String>,
    ) -> Result<Vec<(String, Option<PathBuf>)>> {
        if seen.contains(command_name) {
            return Err(EnvoyError::EnvironmentBuild(format!(
                "Circular environment reference detected at command '{command_name}'"
            )));
        }

        let Some(command) = self.get(command_name) else {
            return Err(EnvoyError::EnvironmentBuild(format!(
                "Environment reference '{command_name}' does not match any known command"
            )));
        };

        seen.insert(command_name.to_string());

        let mut resolved = Vec::new();
        for entry in &command.environment {
            let entry_name = Path::new(entry)
                .file_name()
                .and_then(OsStr::to_str)
                .unwrap_or(entry);

            if !entry_name.contains('.') {
                resolved.extend(self.resolve_environment_inner(entry, seen)?);
            } else {
                resolved.push((entry.clone(), command.envoy_env_dir.clone()));
            }
        }

        seen.remove(command_name);
        Ok(resolved)
    }

    /// Load commands from multiple bundles, skipping bundles with invalid
    /// `commands.json` files.
    pub fn load_from_bundles<T, I>(&mut self, bundles: I)
    where
        T: BundleLike,
        I: IntoIterator<Item = T>,
    {
        for bundle in bundles {
            let commands_file = bundle.envoy_env().join("commands.json");
            if !commands_file.exists() {
                continue;
            }

            if let Err(error) = self.load_from_file(&commands_file, Some(&bundle.bndlid())) {
                eprintln!(
                    "warning: Failed to load commands from bundle {}: {error}",
                    bundle.bndlid()
                );
            }
        }
    }

    /// Return the command definition for `command_name`, if present.
    pub fn get(&self, command_name: &str) -> Option<&CommandDefinition> {
        self.commands.get(command_name)
    }

    /// Return all command names in sorted order.
    pub fn list_commands(&self) -> Vec<String> {
        let mut commands = self.commands.keys().cloned().collect::<Vec<_>>();
        commands.sort();
        commands
    }

    /// Return `true` when the registry contains `command_name`.
    pub fn contains(&self, command_name: &str) -> bool {
        self.commands.contains_key(command_name)
    }

    /// Return the number of registered commands.
    pub fn len(&self) -> usize {
        self.commands.len()
    }

    /// Return `true` when the registry contains no commands.
    pub fn is_empty(&self) -> bool {
        self.commands.is_empty()
    }
}

/// Search for `commands.json` by walking upward for `.envoy/commands.json`.
///
/// Resolution order matches Python:
/// 1. `ENVOY_COMMANDS_FILE` override
/// 2. upward directory walk from `start_path` or the current directory
pub fn find_commands_file(start_path: Option<&Path>) -> Result<Option<PathBuf>> {
    if let Some(env_override) = env::var_os("ENVOY_COMMANDS_FILE").filter(|value| !value.is_empty())
    {
        let override_path = absolute_lexical_path(Path::new(&env_override));
        if override_path.is_file() {
            return Ok(Some(override_path));
        }
        if override_path.exists() {
            return Err(EnvoyError::EnvironmentBuild(format!(
                "ENVOY_COMMANDS_FILE does not point to a file: {:?}",
                env_override
            )));
        }
    }

    let current = match start_path {
        Some(path) => absolute_lexical_path(path),
        None => env::current_dir().map_err(|error| {
            EnvoyError::EnvironmentBuild(format!(
                "Error resolving current directory while searching for commands.json: {error}"
            ))
        })?,
    };

    for parent in current.ancestors() {
        let wrapper_env_dir = parent.join(BUNDLE_ENV_DIR);
        if wrapper_env_dir.is_dir() {
            let commands_file = wrapper_env_dir.join("commands.json");
            if commands_file.exists() {
                return Ok(Some(commands_file));
            }
        }
    }

    Ok(None)
}

fn json_array_to_strings(value: &Value) -> Option<Vec<String>> {
    value
        .as_array()?
        .iter()
        .map(|item| item.as_str().map(ToOwned::to_owned))
        .collect()
}

const SUPPORTED_OPERATING_SYSTEMS: &[&str] = &["windows", "linux", "macos"];
const SUPPORTED_ARCHITECTURES: &[&str] = &["x86_64", "aarch64"];

fn apply_platform_overrides(
    command_name: &str,
    commands_file: &Path,
    platforms_value: Option<&Value>,
    environment: &mut Vec<String>,
    alias: &mut Option<Vec<String>>,
) -> Option<Vec<String>> {
    let Some(platforms_value) = platforms_value else {
        return Some(Vec::new());
    };
    let Some(platforms) = platforms_value.as_object() else {
        log_warning(&format!(
            "Command '{command_name}' in {} has invalid 'platforms' field, skipping",
            commands_file.display()
        ));
        return None;
    };

    for operating_system in platforms.keys() {
        if !SUPPORTED_OPERATING_SYSTEMS.contains(&operating_system.as_str()) {
            log_warning(&format!(
                "Command '{command_name}' in {} has unknown operating system \
override '{operating_system}'",
                commands_file.display()
            ));
        }
    }

    let operating_system = env::consts::OS;
    let Some(os_value) = platforms.get(operating_system) else {
        return Some(Vec::new());
    };
    let Some(os_override) = os_value.as_object() else {
        log_warning(&format!(
            "Command '{command_name}' in {} has invalid '{operating_system}' \
override, skipping",
            commands_file.display()
        ));
        return None;
    };

    if !apply_command_override(
        command_name,
        commands_file,
        operating_system,
        os_override,
        environment,
        alias,
    ) {
        return None;
    }

    let mut applied = vec![operating_system.to_string()];
    let Some(architectures_value) = os_override.get("architectures") else {
        return Some(applied);
    };
    let Some(architectures) = architectures_value.as_object() else {
        log_warning(&format!(
            "Command '{command_name}' in {} has invalid 'architectures' field \
in its '{operating_system}' override, skipping",
            commands_file.display()
        ));
        return None;
    };

    for architecture in architectures.keys() {
        if !SUPPORTED_ARCHITECTURES.contains(&architecture.as_str()) {
            log_warning(&format!(
                "Command '{command_name}' in {} has unknown architecture \
override '{architecture}' under '{operating_system}'",
                commands_file.display()
            ));
        }
    }

    let architecture = env::consts::ARCH;
    let Some(architecture_value) = architectures.get(architecture) else {
        return Some(applied);
    };
    let Some(architecture_override) = architecture_value.as_object() else {
        log_warning(&format!(
            "Command '{command_name}' in {} has invalid '{architecture}' \
override under '{operating_system}', skipping",
            commands_file.display()
        ));
        return None;
    };

    let override_name = format!("{operating_system}.{architecture}");
    if !apply_command_override(
        command_name,
        commands_file,
        &override_name,
        architecture_override,
        environment,
        alias,
    ) {
        return None;
    }

    applied.push(architecture.to_string());
    Some(applied)
}

fn apply_command_override(
    command_name: &str,
    commands_file: &Path,
    override_name: &str,
    command_override: &serde_json::Map<String, Value>,
    environment: &mut Vec<String>,
    alias: &mut Option<Vec<String>>,
) -> bool {
    if let Some(environment_value) = command_override.get("environment") {
        let Some(override_environment) = json_array_to_strings(environment_value) else {
            log_warning(&format!(
                "Command '{command_name}' in {} has invalid 'environment' \
field in its '{override_name}' override, skipping",
                commands_file.display()
            ));
            return false;
        };
        *environment = override_environment;
    }

    if let Some(alias_value) = command_override.get("alias") {
        let Some(override_alias) = json_array_to_strings(alias_value) else {
            log_warning(&format!(
                "Command '{command_name}' in {} has invalid 'alias' field in \
its '{override_name}' override, skipping",
                commands_file.display()
            ));
            return false;
        };
        *alias = Some(override_alias);
    }

    true
}

fn format_python_list(values: &[String]) -> String {
    let contents = values
        .iter()
        .map(|value| format!("'{}'", value.replace('\\', "\\\\").replace('\'', "\\'")))
        .collect::<Vec<_>>()
        .join(", ");

    format!("[{contents}]")
}

fn log_warning(message: &str) {
    eprintln!("warning: {message}");
}

fn absolute_lexical_path(path: &Path) -> PathBuf {
    let absolute = if path.is_absolute() {
        path.to_path_buf()
    } else {
        env::current_dir()
            .map(|cwd| cwd.join(path))
            .unwrap_or_else(|_| path.to_path_buf())
    };

    lexical_normalize(&absolute)
}

fn lexical_normalize(path: &Path) -> PathBuf {
    let mut normalized = PathBuf::new();

    for component in path.components() {
        match component {
            Component::CurDir => {}
            Component::ParentDir => {
                if matches!(
                    normalized.components().next_back(),
                    Some(Component::Normal(_))
                ) {
                    normalized.pop();
                } else {
                    normalized.push(component.as_os_str());
                }
            }
            Component::Prefix(_) | Component::RootDir | Component::Normal(_) => {
                normalized.push(component.as_os_str());
            }
        }
    }

    normalized
}

fn path_to_forward_slashes(path: &Path) -> String {
    path.to_string_lossy().replace('\\', "/")
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;
    use std::env;
    use std::ffi::{OsStr, OsString};
    use std::fs;
    use std::path::{Path, PathBuf};

    use serde_json::{json, Value};
    use tempfile::tempdir;

    use super::{
        find_commands_file, BundleInfo, CommandDefinition, CommandRegistry, EnvoyError,
        BUNDLE_ENV_DIR,
    };

    struct EnvVarGuard {
        key: &'static str,
        previous: Option<OsString>,
    }

    impl EnvVarGuard {
        fn set(key: &'static str, value: Option<&OsStr>) -> Self {
            let previous = env::var_os(key);

            match value {
                Some(value) => env::set_var(key, value),
                None => env::remove_var(key),
            }

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

    /// Locks the crate-wide `crate::env_test_lock::MUTEX` rather than a
    /// module-local mutex: several modules' tests mutate the same real
    /// process environment variables (e.g. `ENVOY_COMMANDS_FILE` here,
    /// `ENVOY_STACK_ROOTS` in `discovery`/`stack_registry`), so a single
    /// shared lock is required to prevent cross-module test races under
    /// `cargo test`'s default parallel execution.
    fn with_env_lock<T>(test_fn: impl FnOnce() -> T) -> T {
        let _lock = crate::env_test_lock::MUTEX
            .lock()
            .unwrap_or_else(|poison| poison.into_inner());
        test_fn()
    }

    fn write_json(path: &Path, value: &Value) {
        fs::write(
            path,
            serde_json::to_string_pretty(value).expect("failed to serialize test json"),
        )
        .expect("failed to write test json");
    }

    #[test]
    fn command_definition_accessors_and_display_match_python_shape() {
        let command = CommandDefinition {
            name: String::from("python"),
            environment: vec![
                String::from("base_env.json"),
                String::from("python_env.json"),
            ],
            alias: Some(vec![
                String::from("python.exe"),
                String::from("-m"),
                String::from("pip"),
            ]),
            bundle: Some(String::from("gt:pythoncore")),
            envoy_env_dir: None,
            source_file: None,
            platform_overrides: Vec::new(),
        };

        assert_eq!(command.executable(), "python.exe");
        assert_eq!(
            command.base_args(),
            &[String::from("-m"), String::from("pip")]
        );
        assert_eq!(
            command.to_string(),
            "CommandDefinition(python [gt:pythoncore] (alias: python.exe -m pip), env=['base_env.json', 'python_env.json'])"
        );
    }

    #[test]
    fn expand_alias_supports_bundle_special_vars_and_env_values() {
        let temp_dir = tempdir().expect("failed to create tempdir");
        let bundle_root = temp_dir.path().join("gt").join("pythoncore");
        let envoy_env_dir = bundle_root.join(BUNDLE_ENV_DIR);
        fs::create_dir_all(&envoy_env_dir).expect("failed to create .envoy directory");

        let command = CommandDefinition {
            name: String::from("python"),
            environment: Vec::new(),
            alias: Some(vec![
                String::from("${__BUNDLE__}/bin/python.exe"),
                String::from("${__BUNDLE_ENV__}/python_env.json"),
                String::from("${__BUNDLE_NAME__}"),
                String::from("${HOME}/tools"),
            ]),
            bundle: None,
            envoy_env_dir: Some(envoy_env_dir.clone()),
            source_file: None,
            platform_overrides: Vec::new(),
        };
        let env_vars = HashMap::from([(String::from("HOME"), String::from("C:/Users/Test"))]);

        let expanded = command.expand_alias(Some(&env_vars));
        let expected_home_tools = if cfg!(windows) {
            "C:\\Users\\Test\\tools"
        } else {
            "C:/Users/Test/tools"
        };

        assert_eq!(
            expanded,
            vec![
                bundle_root
                    .join("bin")
                    .join("python.exe")
                    .display()
                    .to_string(),
                envoy_env_dir.join("python_env.json").display().to_string(),
                String::from("pythoncore"),
                String::from(expected_home_tools),
            ]
        );
    }

    #[test]
    fn load_from_file_loads_valid_commands_and_skips_invalid_entries() {
        let temp_dir = tempdir().expect("failed to create tempdir");
        let envoy_env_dir = temp_dir.path().join(BUNDLE_ENV_DIR);
        fs::create_dir_all(&envoy_env_dir).expect("failed to create .envoy directory");
        let commands_file = envoy_env_dir.join("commands.json");

        write_json(
            &commands_file,
            &json!({
                "python": {
                    "environment": ["base_env.json", "python_env.json"],
                    "alias": ["python.exe", "-m", "pip"]
                },
                "not_an_object": "skip me",
                "missing_environment": {
                    "alias": ["python.exe"]
                },
                "bad_environment": {
                    "environment": "base_env.json"
                },
                "bad_alias": {
                    "environment": ["base_env.json"],
                    "alias": "python.exe"
                }
            }),
        );

        let registry =
            CommandRegistry::new(Some(&commands_file)).expect("failed to load commands file");

        assert_eq!(registry.len(), 1);
        assert!(registry.contains("python"));
        assert_eq!(registry.list_commands(), vec![String::from("python")]);

        let command = registry
            .get("python")
            .expect("python command should be present");
        assert_eq!(
            command.environment,
            vec![
                String::from("base_env.json"),
                String::from("python_env.json")
            ]
        );
        assert_eq!(
            command.alias.as_ref().expect("alias should be present"),
            &vec![
                String::from("python.exe"),
                String::from("-m"),
                String::from("pip"),
            ]
        );
        assert_eq!(
            command
                .source_file
                .as_ref()
                .expect("source file should be set"),
            &commands_file
        );
    }

    #[test]
    fn load_from_file_accepts_comment_annotated_commands_json() {
        let temp_dir = tempdir().expect("failed to create tempdir");
        let envoy_env_dir = temp_dir.path().join(BUNDLE_ENV_DIR);
        fs::create_dir_all(&envoy_env_dir).expect("failed to create .envoy directory");
        let commands_file = envoy_env_dir.join("commands.json");
        fs::write(
            &commands_file,
            r#"{
                // Main Python entry point.
                "python": {
                    "environment": [
                        "base_env.json", /* shared bootstrap */
                        "python_env.json" # interpreter-specific
                    ],
                    "alias": ["python.exe", "-m", "pip"]
                }
            }"#,
        )
        .expect("failed to write comment-annotated commands file");

        let registry =
            CommandRegistry::new(Some(&commands_file)).expect("failed to load commands file");
        let command = registry
            .get("python")
            .expect("python command should be present");

        assert_eq!(registry.len(), 1);
        assert_eq!(
            command.environment,
            vec![
                String::from("base_env.json"),
                String::from("python_env.json"),
            ]
        );
        assert_eq!(
            command.alias.as_ref().expect("alias should be present"),
            &vec![
                String::from("python.exe"),
                String::from("-m"),
                String::from("pip"),
            ]
        );
    }

    #[test]
    fn load_from_file_applies_os_and_architecture_overrides() {
        let temp_dir = tempdir().expect("failed to create tempdir");
        let envoy_env_dir = temp_dir.path().join(BUNDLE_ENV_DIR);
        fs::create_dir_all(&envoy_env_dir).expect("failed to create .envoy directory");
        let commands_file = envoy_env_dir.join("commands.json");

        write_json(
            &commands_file,
            &json!({
                "tool": {
                    "environment": ["base.json"],
                    "alias": ["base-tool"],
                    "platforms": {
                        "windows": {
                            "environment": ["windows.json"],
                            "alias": ["windows-tool"],
                            "architectures": {
                                "x86_64": {"alias": ["windows-x86_64-tool"]},
                                "aarch64": {"alias": ["windows-aarch64-tool"]}
                            }
                        },
                        "linux": {
                            "environment": ["linux.json"],
                            "alias": ["linux-tool"],
                            "architectures": {
                                "x86_64": {"alias": ["linux-x86_64-tool"]},
                                "aarch64": {"alias": ["linux-aarch64-tool"]}
                            }
                        },
                        "macos": {
                            "environment": ["macos.json"],
                            "alias": ["macos-tool"],
                            "architectures": {
                                "x86_64": {"alias": ["macos-x86_64-tool"]},
                                "aarch64": {"alias": ["macos-aarch64-tool"]}
                            }
                        }
                    }
                }
            }),
        );

        let registry =
            CommandRegistry::new(Some(&commands_file)).expect("failed to load commands file");
        let command = registry
            .get("tool")
            .expect("tool command should be present");

        assert_eq!(
            command.environment,
            vec![format!("{}.json", env::consts::OS)]
        );
        assert_eq!(
            command.alias,
            Some(vec![format!(
                "{}-{}-tool",
                env::consts::OS,
                env::consts::ARCH
            )])
        );
        assert_eq!(
            command.platform_overrides,
            vec![env::consts::OS.to_string(), env::consts::ARCH.to_string()]
        );
        assert_eq!(
            command.platform_resolution(),
            format!("base -> {} -> {}", env::consts::OS, env::consts::ARCH)
        );
    }

    #[test]
    fn platform_overrides_inherit_unspecified_fields() {
        let temp_dir = tempdir().expect("failed to create tempdir");
        let envoy_env_dir = temp_dir.path().join(BUNDLE_ENV_DIR);
        fs::create_dir_all(&envoy_env_dir).expect("failed to create .envoy directory");
        let commands_file = envoy_env_dir.join("commands.json");
        let config_text = format!(
            r#"{{
                "tool": {{
                    "environment": ["base.json"],
                    "alias": ["base-tool"],
                    "platforms": {{
                        "{}": {{
                            "alias": ["os-tool"],
                            "architectures": {{
                                "{}": {{"environment": ["arch.json"]}}
                            }}
                        }}
                    }}
                }}
            }}"#,
            env::consts::OS,
            env::consts::ARCH
        );
        fs::write(&commands_file, config_text).expect("failed to write commands file");

        let registry =
            CommandRegistry::new(Some(&commands_file)).expect("failed to load commands file");
        let command = registry
            .get("tool")
            .expect("tool command should be present");

        assert_eq!(command.environment, vec![String::from("arch.json")]);
        assert_eq!(command.alias, Some(vec![String::from("os-tool")]));
    }

    #[test]
    fn invalid_current_platform_override_skips_command() {
        let temp_dir = tempdir().expect("failed to create tempdir");
        let envoy_env_dir = temp_dir.path().join(BUNDLE_ENV_DIR);
        fs::create_dir_all(&envoy_env_dir).expect("failed to create .envoy directory");
        let commands_file = envoy_env_dir.join("commands.json");

        write_json(
            &commands_file,
            &json!({
                "tool": {
                    "environment": [],
                    "platforms": {
                        "windows": "invalid",
                        "linux": "invalid",
                        "macos": "invalid"
                    }
                }
            }),
        );

        let registry =
            CommandRegistry::new(Some(&commands_file)).expect("commands file should be readable");
        assert!(!registry.contains("tool"));
    }

    #[test]
    fn resolve_environment_splices_references_in_declaration_order() {
        let mut registry = CommandRegistry::empty();
        let fake_dir = PathBuf::from("C:\\fake\\bundle\\.envoy");

        registry.commands.insert(
            String::from("base"),
            CommandDefinition {
                name: String::from("base"),
                environment: vec![String::from("base_env.json")],
                alias: None,
                bundle: None,
                envoy_env_dir: Some(fake_dir.clone()),
                source_file: None,
                platform_overrides: Vec::new(),
            },
        );
        registry.commands.insert(
            String::from("child"),
            CommandDefinition {
                name: String::from("child"),
                environment: vec![
                    String::from("pre.json"),
                    String::from("base"),
                    String::from("child_env.json"),
                ],
                alias: None,
                bundle: None,
                envoy_env_dir: Some(fake_dir.clone()),
                source_file: None,
                platform_overrides: Vec::new(),
            },
        );

        let resolved = registry
            .resolve_environment("child")
            .expect("environment resolution should succeed");

        assert_eq!(
            resolved,
            vec![
                (String::from("pre.json"), Some(fake_dir.clone())),
                (String::from("base_env.json"), Some(fake_dir.clone())),
                (String::from("child_env.json"), Some(fake_dir)),
            ]
        );
    }

    #[test]
    fn resolve_environment_reports_missing_and_circular_references() {
        let mut missing_registry = CommandRegistry::empty();
        missing_registry.commands.insert(
            String::from("cmd"),
            CommandDefinition {
                name: String::from("cmd"),
                environment: vec![String::from("ghost")],
                alias: None,
                bundle: None,
                envoy_env_dir: None,
                source_file: None,
                platform_overrides: Vec::new(),
            },
        );

        let missing_error = missing_registry
            .resolve_environment("cmd")
            .expect_err("missing command reference should error");
        assert!(missing_error
            .to_string()
            .contains("Environment reference 'ghost' does not match any known command"));

        let mut cyclic_registry = CommandRegistry::empty();
        cyclic_registry.commands.insert(
            String::from("a"),
            CommandDefinition {
                name: String::from("a"),
                environment: vec![String::from("b")],
                alias: None,
                bundle: None,
                envoy_env_dir: None,
                source_file: None,
                platform_overrides: Vec::new(),
            },
        );
        cyclic_registry.commands.insert(
            String::from("b"),
            CommandDefinition {
                name: String::from("b"),
                environment: vec![String::from("a")],
                alias: None,
                bundle: None,
                envoy_env_dir: None,
                source_file: None,
                platform_overrides: Vec::new(),
            },
        );

        let cycle_error = cyclic_registry
            .resolve_environment("a")
            .expect_err("cyclic reference should error");
        assert!(cycle_error
            .to_string()
            .contains("Circular environment reference detected at command 'a'"));
    }

    #[test]
    fn load_from_bundles_uses_bundle_info_and_skips_invalid_bundle_files() {
        let temp_dir = tempdir().expect("failed to create tempdir");
        let good_root = temp_dir.path().join("gt").join("good_bundle");
        let bad_root = temp_dir.path().join("gt").join("bad_bundle");
        let good_env = good_root.join(BUNDLE_ENV_DIR);
        let bad_env = bad_root.join(BUNDLE_ENV_DIR);
        fs::create_dir_all(&good_env).expect("failed to create good .envoy directory");
        fs::create_dir_all(&bad_env).expect("failed to create bad .envoy directory");

        write_json(
            &good_env.join("commands.json"),
            &json!({
                "good": {
                    "environment": ["good_env.json"]
                }
            }),
        );
        fs::write(bad_env.join("commands.json"), "{ not valid json ")
            .expect("failed to write bad commands file");

        let bundles = vec![
            BundleInfo::new(
                good_root.clone(),
                String::from("good_bundle"),
                String::from("gt"),
            ),
            BundleInfo::new(
                bad_root.clone(),
                String::from("bad_bundle"),
                String::from("gt"),
            ),
        ];

        let mut registry = CommandRegistry::empty();
        registry.load_from_bundles(&bundles);

        assert!(registry.contains("good"));
        assert_eq!(
            registry
                .get("good")
                .and_then(|command| command.bundle.as_deref()),
            Some("gt:good_bundle")
        );
        assert_eq!(registry.len(), 1);
    }

    #[test]
    fn find_commands_file_prefers_env_override_file() {
        with_env_lock(|| {
            let temp_dir = tempdir().expect("failed to create tempdir");
            let commands_file = temp_dir.path().join("commands.json");
            fs::write(&commands_file, "{}").expect("failed to write commands file");
            let _env_guard =
                EnvVarGuard::set("ENVOY_COMMANDS_FILE", Some(commands_file.as_os_str()));

            let found = find_commands_file(Some(temp_dir.path())).expect("search should succeed");

            assert_eq!(found, Some(commands_file));
        });
    }

    #[test]
    fn find_commands_file_errors_when_env_override_is_not_a_file() {
        with_env_lock(|| {
            let temp_dir = tempdir().expect("failed to create tempdir");
            let _env_guard =
                EnvVarGuard::set("ENVOY_COMMANDS_FILE", Some(temp_dir.path().as_os_str()));

            let error = find_commands_file(Some(temp_dir.path()))
                .expect_err("directory override should error");

            match error {
                EnvoyError::EnvironmentBuild(message) => {
                    assert!(message.contains("ENVOY_COMMANDS_FILE does not point to a file"));
                }
                other => panic!("unexpected error variant: {other:?}"),
            }
        });
    }

    #[test]
    fn find_commands_file_walks_upward_from_start_path() {
        with_env_lock(|| {
            let temp_dir = tempdir().expect("failed to create tempdir");
            let project_root = temp_dir.path().join("project");
            let nested = project_root.join("a").join("b").join("c");
            let envoy_env_dir = project_root.join(BUNDLE_ENV_DIR);
            let commands_file = envoy_env_dir.join("commands.json");
            fs::create_dir_all(&nested).expect("failed to create nested directory");
            fs::create_dir_all(&envoy_env_dir).expect("failed to create .envoy directory");
            fs::write(&commands_file, "{}").expect("failed to write commands file");
            let _env_guard = EnvVarGuard::set("ENVOY_COMMANDS_FILE", None);

            let found = find_commands_file(Some(&nested)).expect("upward search should succeed");

            assert_eq!(found, Some(commands_file));
        });
    }
}
