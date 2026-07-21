//! Shared runtime helpers used by the in-progress CLI and process ports.
//!
//! In the original Python implementation, `py/envoy/_cli.py` and
//! `py/envoy/proc.py` each carried near-identical logic for:
//! - deciding whether a command spec is a raw executable path or a registered
//!   envoy command name
//! - discovering bundles and loading a [`CommandRegistry`]
//! - collecting the ordered list of environment files for a command
//! - preparing a subprocess environment from those env files
//!
//! This module consolidates that duplicated behavior into one framework-agnostic
//! place inside `envoy-core`. Upcoming migration phases will consume it from:
//! - `envoy-cli` (the native `envoy` binary replacing `py/envoy/_cli.py`)
//! - `envoy-py::proc` (PyO3 bindings replacing `py/envoy/proc.py`)
//!
//! The functions here are intentionally small and direct ports of the Python
//! helpers so behavior remains easy to compare during the migration.

use std::collections::{HashMap, HashSet};
use std::env;
use std::path::{Path, PathBuf};

use crate::commands::{find_commands_file, CommandDefinition, CommandRegistry};
use crate::discovery::{discover_bundles_auto, discover_bundles_from_roots, BundleInfo};
use crate::environment::EnvironmentManager;
use crate::error::{EnvoyError, Result};
use crate::retry::{retry_sync, RetryConfig};

/// Return `true` when `spec` should be treated as a direct executable path.
///
/// This ports `_isRawPath()` from both `py/envoy/_cli.py` and
/// `py/envoy/proc.py`: a spec is raw when it is absolute or contains an
/// explicit directory separator. Plain names like `maya` or `python` are
/// treated as registered envoy command names instead.
pub fn is_raw_path(spec: &str) -> bool {
    let path = Path::new(spec);
    path.is_absolute() || spec.contains(std::path::MAIN_SEPARATOR) || spec.contains('/')
}

/// Resolve the preferred command prefix for invoking the envoy CLI.
///
/// Python's `_resolveEnvoyExe()` was anchored relative to `proc.py`'s
/// `__file__`, walking up to the repository root to probe `dist/envoy.exe`
/// and `bin/envoy.bat`. That anchor does not exist for a native Rust binary
/// or a compiled PyO3 extension, so the Rust port adapts the probe strategy:
///
/// 1. Look for a sibling `envoy.exe` (Windows) or `envoy` (Unix) next to the
///    currently running executable returned by [`std::env::current_exe`].
/// 2. Search `PATH` for the same executable names.
/// 3. Fall back to `python -m envoy` on Windows or `python3 -m envoy` on Unix,
///    preferring the first matching interpreter found on `PATH`.
///
/// The returned vector is the executable prefix that callers can prepend to
/// envoy CLI arguments.
pub fn resolve_envoy_exe() -> Vec<String> {
    if let Ok(current_exe) = env::current_exe() {
        if let Some(current_dir) = current_exe.parent() {
            for candidate_name in envoy_program_names() {
                let candidate = current_dir.join(candidate_name);
                if candidate.is_file() {
                    return vec![path_to_string(&candidate)];
                }
            }
        }
    }

    if let Some(candidate) = find_on_path(envoy_program_names()) {
        return vec![path_to_string(&candidate)];
    }

    let python_fallback = find_on_path(python_program_names())
        .map(|candidate| path_to_string(&candidate))
        .unwrap_or_else(default_python_program);

    vec![python_fallback, String::from("-m"), String::from("envoy")]
}

/// Discover bundles, load commands, and return the populated registry.
///
/// This ports `_loadRegistry()` from `py/envoy/proc.py`.
///
/// `Ok((registry, bundles))` is returned even when no bundles and no commands
/// were found; an empty registry is a valid outcome matching Python. Errors are
/// only returned for genuine discovery, I/O, or parse failures.
/// Retry configuration for bundle discovery and config file I/O.
///
/// File-system operations on network shares or under heavy lock contention
/// can fail transiently; a short exponential-backoff retry avoids spurious
/// failures without adding noticeable latency for local files.
const IO_RETRY_CONFIG: RetryConfig = RetryConfig {
    max_attempts: 3,
    initial_delay: std::time::Duration::from_millis(50),
    max_delay: std::time::Duration::from_secs(2),
};

pub fn load_registry(
    bundle_roots: Option<&[String]>,
    commands_file: Option<&Path>,
) -> Result<(CommandRegistry, Option<Vec<BundleInfo>>)> {
    let mut registry = CommandRegistry::empty();
    let mut bundles = None;

    if let Some(bundle_roots) = bundle_roots {
        // `discover_bundles_from_roots` is infallible (returns Vec directly),
        // but we still wrap it for consistency with the retry pattern.
        let discovered: Vec<_> = retry_sync(&IO_RETRY_CONFIG, || -> std::result::Result<Vec<BundleInfo>, EnvoyError> {
            Ok(discover_bundles_from_roots(bundle_roots))
        }).map_err(|e| EnvoyError::EnvironmentBuild(format!("Bundle discovery failed: {e}")))?;
        if !discovered.is_empty() {
            registry.load_from_bundles(&discovered);
            bundles = Some(discovered);
        }
    } else {
        let discovered = retry_sync(&IO_RETRY_CONFIG, || discover_bundles_auto())
            .map_err(|e| EnvoyError::EnvironmentBuild(format!("Bundle discovery failed: {e}")))?;
        if !discovered.is_empty() {
            registry.load_from_bundles(&discovered);
            bundles = Some(discovered);
        }
    }

    if registry.is_empty() {
        let commands_file = match commands_file {
            Some(path) => Some(path.to_path_buf()),
            None => find_commands_file(None)?,
        };

        if let Some(commands_file) = commands_file {
            retry_sync(&IO_RETRY_CONFIG, || registry.load_from_file(&commands_file, None))
                .map_err(|e| EnvoyError::EnvironmentBuild(format!("Failed to load commands file: {e}")))?;
        }
    }

    Ok((registry, bundles))
}

/// Collect the ordered env-file list for `command_name`.
///
/// This ports `_collectEnvFiles()` from `py/envoy/proc.py`, including both:
/// - multi-bundle lookup mode using each bundle's indexed `env_files()`
/// - legacy single-`.envoy/` lookup mode using the command's `envoy_env_dir`
///
/// Global env files are always collected before command-specific env files.
pub fn collect_env_files(
    command_name: &str,
    registry: &CommandRegistry,
    bundles: Option<&[BundleInfo]>,
) -> Result<Vec<PathBuf>> {
    let command = registry.get(command_name).ok_or_else(|| {
        EnvoyError::CommandNotFound(format!(
            "Command '{command_name}' is not registered. Run 'envoy --list' \
to see available commands."
        ))
    })?;

    let resolved_env = registry
        .resolve_environment(command_name)
        .map_err(|error| {
            EnvoyError::EnvironmentBuild(format!(
                "Failed to resolve environment for '{command_name}': {error}"
            ))
        })?;

    let mut env_files = Vec::new();

    if let Some(bundles) = bundles.filter(|bundles| !bundles.is_empty()) {
        for bundle in bundles {
            if let Some(global_env) = bundle.env_files().get("global_env.json") {
                env_files.push(global_env.clone());
            }
        }

        for (env_file_name, _env_dir) in resolved_env {
            for bundle in bundles {
                if let Some(env_file) = bundle.env_files().get(&env_file_name) {
                    env_files.push(env_file.clone());
                }
            }
        }
    } else {
        let env_dir = match command.envoy_env_dir.clone() {
            Some(env_dir) => env_dir,
            None => {
                let commands_file = find_commands_file(None)?.ok_or_else(|| {
                    EnvoyError::EnvironmentBuild(format!(
                        "Cannot determine .envoy directory for '{command_name}'."
                    ))
                })?;
                commands_file
                    .parent()
                    .map(Path::to_path_buf)
                    .ok_or_else(|| {
                        EnvoyError::EnvironmentBuild(format!(
                            "Cannot determine .envoy directory for '{command_name}'."
                        ))
                    })?
            }
        };

        let global_env = env_dir.join("global_env.json");
        if global_env.exists() {
            env_files.push(global_env);
        }

        for (env_file_name, entry_env_dir) in resolved_env {
            let dir_to_use = entry_env_dir.unwrap_or_else(|| env_dir.clone());
            let file_path = dir_to_use.join(&env_file_name);
            if !file_path.exists() {
                return Err(EnvoyError::EnvironmentBuild(format!(
                    "Environment file not found: {}",
                    file_path.display()
                )));
            }
            env_files.push(file_path);
        }
    }

    Ok(env_files)
}

/// Prepare a subprocess environment and return it alongside the command.
///
/// This ports `_prepareEnv()` from `py/envoy/proc.py`.
///
/// `env_override`, when present, changes only which command contributes env
/// files; the returned [`CommandDefinition`] still represents `command_name`.
/// Raw executable paths therefore bypass command lookup for the executable
/// itself, but still require a registered `env_override` if envoy-managed env
/// files should be loaded.
pub fn prepare_env(
    command_name: &str,
    registry: &CommandRegistry,
    bundles: Option<&[BundleInfo]>,
    inherit_env: bool,
    allowlist: Option<&[String]>,
    env_override: Option<&str>,
) -> Result<(HashMap<String, String>, CommandDefinition)> {
    let env_source = env_override.unwrap_or(command_name);

    if let Some(env_override) = env_override {
        if registry.get(env_override).is_none() {
            return Err(EnvoyError::CommandNotFound(format!(
                "Environment override command '{env_override}' is not registered."
            )));
        }
    }

    let command = if is_raw_path(command_name) {
        CommandDefinition {
            name: command_name.to_string(),
            environment: Vec::new(),
            alias: Some(vec![command_name.to_string()]),
            bundle: None,
            envoy_env_dir: None,
            source_file: None,
        }
    } else {
        registry.get(command_name).cloned().ok_or_else(|| {
            EnvoyError::CommandNotFound(format!("Command '{command_name}' is not registered."))
        })?
    };

    let env_files = collect_env_files(env_source, registry, bundles)?;
    let allowlist = allowlist.map(|items| items.iter().cloned().collect::<HashSet<_>>());
    let env_manager = EnvironmentManager::new(inherit_env, allowlist);

    let env = env_manager
        .prepare_environment(&env_files, None, None, None)
        .map_err(|error| {
            EnvoyError::EnvironmentBuild(format!(
                "Failed to prepare environment for '{env_source}': {error}"
            ))
        })?;

    Ok((env, command))
}

#[cfg(windows)]
fn envoy_program_names() -> &'static [&'static str] {
    &["envoy.exe", "envoy"]
}

#[cfg(not(windows))]
fn envoy_program_names() -> &'static [&'static str] {
    &["envoy"]
}

#[cfg(windows)]
fn python_program_names() -> &'static [&'static str] {
    &["python.exe", "python"]
}

#[cfg(not(windows))]
fn python_program_names() -> &'static [&'static str] {
    &["python3"]
}

#[cfg(windows)]
fn default_python_program() -> String {
    String::from("python")
}

#[cfg(not(windows))]
fn default_python_program() -> String {
    String::from("python3")
}

fn find_on_path(candidate_names: &[&str]) -> Option<PathBuf> {
    let path_var = env::var_os("PATH")?;

    for path_dir in env::split_paths(&path_var) {
        for candidate_name in candidate_names {
            let candidate = path_dir.join(candidate_name);
            if candidate.is_file() {
                return Some(candidate);
            }
        }
    }

    None
}

fn path_to_string(path: &Path) -> String {
    path.to_string_lossy().into_owned()
}

#[cfg(test)]
mod tests {
    use std::env;
    use std::ffi::{OsStr, OsString};
    use std::fs;
    use std::path::{Path, PathBuf};

    use serde_json::{json, Value};
    use tempfile::tempdir;

    use super::{collect_env_files, is_raw_path, load_registry, prepare_env};
    use crate::commands::CommandRegistry;
    use crate::discovery::{BundleInfo, BUNDLE_ENV_DIR};
    use crate::error::EnvoyError;

    struct EnvVarGuard {
        previous: Vec<(String, Option<OsString>)>,
    }

    impl EnvVarGuard {
        fn set_many(updates: &[(&str, Option<&OsStr>)]) -> Self {
            let mut previous = Vec::new();

            for (key, value) in updates {
                previous.push(((*key).to_string(), env::var_os(key)));
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

    fn write_json(path: &Path, value: &Value) {
        fs::write(
            path,
            serde_json::to_string_pretty(value).expect("test json should serialize"),
        )
        .expect("test json should write");
    }

    fn create_checkout_bundle(
        root: &Path,
        namespace: &str,
        name: &str,
        commands: Value,
        env_files: &[(&str, Value)],
    ) -> PathBuf {
        let bundle_root = root.join(namespace).join(name);
        let envoy_env = bundle_root.join(BUNDLE_ENV_DIR);
        fs::create_dir_all(bundle_root.join(".git")).expect("bundle .git should be created");
        fs::create_dir_all(&envoy_env).expect("bundle .envoy should be created");
        write_json(&envoy_env.join("commands.json"), &commands);

        for (file_name, contents) in env_files {
            write_json(&envoy_env.join(file_name), contents);
        }

        bundle_root
    }

    fn create_legacy_commands_dir(
        root: &Path,
        commands: Value,
        env_files: &[(&str, Value)],
    ) -> PathBuf {
        let envoy_env = root.join(BUNDLE_ENV_DIR);
        fs::create_dir_all(&envoy_env).expect("legacy .envoy should be created");
        let commands_file = envoy_env.join("commands.json");
        write_json(&commands_file, &commands);

        for (file_name, contents) in env_files {
            write_json(&envoy_env.join(file_name), contents);
        }

        commands_file
    }

    fn build_bundle_registry(root: &Path) -> (Vec<BundleInfo>, CommandRegistry) {
        let base_root = create_checkout_bundle(
            root,
            "gt",
            "base_bundle",
            json!({
                "base": {
                    "environment": ["base_env.json"]
                }
            }),
            &[
                ("global_env.json", json!({"GLOBAL_BASE": "base-global"})),
                ("base_env.json", json!({"BASE_ONLY": "base"})),
            ],
        );
        let tool_root = create_checkout_bundle(
            root,
            "gt",
            "tool_bundle",
            json!({
                "tool": {
                    "environment": ["base", "tool_env.json"],
                    "alias": ["tool.exe", "--bundle"]
                }
            }),
            &[
                ("global_env.json", json!({"GLOBAL_TOOL": "tool-global"})),
                ("tool_env.json", json!({"TOOL_ONLY": "tool"})),
            ],
        );

        let bundles = vec![
            BundleInfo::new(
                base_root.clone(),
                String::from("base_bundle"),
                String::from("gt"),
            ),
            BundleInfo::new(
                tool_root.clone(),
                String::from("tool_bundle"),
                String::from("gt"),
            ),
        ];

        let mut registry = CommandRegistry::empty();
        registry.load_from_bundles(&bundles);
        (bundles, registry)
    }

    fn platform_absolute_raw_path() -> &'static str {
        if cfg!(windows) {
            r"C:\tools\krita.exe"
        } else {
            "/opt/tools/krita"
        }
    }

    fn platform_relative_raw_path() -> &'static str {
        if cfg!(windows) {
            r".\tools\krita.exe"
        } else {
            "./tools/krita"
        }
    }

    #[test]
    fn is_raw_path_matches_python_truth_table() {
        assert!(is_raw_path(platform_absolute_raw_path()));
        assert!(is_raw_path(platform_relative_raw_path()));
        assert!(is_raw_path("tools/krita"));
        assert!(!is_raw_path("krita"));
    }

    #[test]
    fn load_registry_loads_commands_from_explicit_bundle_roots() {
        let temp_dir = tempdir().expect("tempdir should be created");
        let bundle_root = create_checkout_bundle(
            temp_dir.path(),
            "gt",
            "pythoncore",
            json!({
                "python_dev": {
                    "environment": ["python_env.json"]
                }
            }),
            &[("python_env.json", json!({"PYTHON_HOME": "C:/python"}))],
        );

        let roots = vec![temp_dir.path().display().to_string()];
        let (registry, bundles) =
            load_registry(Some(&roots), None).expect("explicit bundle roots should load");

        assert!(registry.contains("python_dev"));
        let bundles = bundles.expect("bundle discovery should be present");
        assert_eq!(bundles.len(), 1);
        assert_eq!(bundles[0].root, bundle_root);
    }

    #[test]
    fn load_registry_uses_envoy_bndl_roots_auto_discovery() {
        with_env_lock(|| {
            let temp_dir = tempdir().expect("tempdir should be created");
            let bundle_root = create_checkout_bundle(
                temp_dir.path(),
                "gt",
                "maya",
                json!({
                    "maya_dev": {
                        "environment": ["maya_env.json"]
                    }
                }),
                &[("maya_env.json", json!({"MAYA_HOME": "C:/maya"}))],
            );
            let roots = env::join_paths([temp_dir.path()]).expect("roots should join");
            let _env_guard = EnvVarGuard::set_many(&[
                ("ENVOY_BNDL_ROOTS", Some(roots.as_os_str())),
                ("ENVOY_BUNDLES_CONFIG", None),
            ]);

            let (registry, bundles) =
                load_registry(None, None).expect("auto-discovery should load");

            assert!(registry.contains("maya_dev"));
            let bundles = bundles.expect("bundle discovery should be present");
            assert_eq!(bundles.len(), 1);
            assert_eq!(bundles[0].root, bundle_root);
        });
    }

    #[test]
    fn load_registry_falls_back_to_bare_commands_file() {
        with_env_lock(|| {
            let temp_dir = tempdir().expect("tempdir should be created");
            let commands_file = create_legacy_commands_dir(
                temp_dir.path(),
                json!({
                    "houdini_dev": {
                        "environment": ["houdini_env.json"]
                    }
                }),
                &[("houdini_env.json", json!({"HOUDINI_HOME": "C:/houdini"}))],
            );
            let _env_guard = EnvVarGuard::set_many(&[
                ("ENVOY_BNDL_ROOTS", None),
                ("ENVOY_BUNDLES_CONFIG", None),
            ]);

            let (registry, bundles) = load_registry(None, Some(&commands_file))
                .expect("commands file fallback should load");

            assert!(registry.contains("houdini_dev"));
            assert!(bundles.is_none());
        });
    }

    #[test]
    fn collect_env_files_uses_bundle_indexes_in_multi_bundle_mode() {
        let temp_dir = tempdir().expect("tempdir should be created");
        let (bundles, registry) = build_bundle_registry(temp_dir.path());

        let env_files = collect_env_files("tool", &registry, Some(&bundles))
            .expect("bundle env files should collect");

        let expected = vec![
            bundles[0].envoy_env().join("global_env.json"),
            bundles[1].envoy_env().join("global_env.json"),
            bundles[0].envoy_env().join("base_env.json"),
            bundles[1].envoy_env().join("tool_env.json"),
        ];

        assert_eq!(env_files, expected);
    }

    #[test]
    fn collect_env_files_uses_legacy_env_dir_and_global_env_first() {
        let temp_dir = tempdir().expect("tempdir should be created");
        let commands_file = create_legacy_commands_dir(
            temp_dir.path(),
            json!({
                "base": {
                    "environment": ["base_env.json"]
                },
                "tool": {
                    "environment": ["base", "tool_env.json"],
                    "alias": ["tool.exe"]
                }
            }),
            &[
                ("global_env.json", json!({"GLOBAL": "global"})),
                ("base_env.json", json!({"BASE_ONLY": "base"})),
                ("tool_env.json", json!({"TOOL_ONLY": "tool"})),
            ],
        );
        let registry =
            CommandRegistry::new(Some(&commands_file)).expect("legacy commands file should load");

        let env_files =
            collect_env_files("tool", &registry, None).expect("legacy env files should collect");
        let envoy_env = commands_file
            .parent()
            .expect("commands file should have a parent");

        assert_eq!(
            env_files,
            vec![
                envoy_env.join("global_env.json"),
                envoy_env.join("base_env.json"),
                envoy_env.join("tool_env.json"),
            ]
        );
    }

    #[test]
    fn collect_env_files_errors_when_legacy_env_file_is_missing() {
        let temp_dir = tempdir().expect("tempdir should be created");
        let commands_file = create_legacy_commands_dir(
            temp_dir.path(),
            json!({
                "tool": {
                    "environment": ["missing_env.json"]
                }
            }),
            &[],
        );
        let registry =
            CommandRegistry::new(Some(&commands_file)).expect("legacy commands file should load");

        let error = collect_env_files("tool", &registry, None)
            .expect_err("missing legacy env file should fail");

        match error {
            EnvoyError::EnvironmentBuild(message) => {
                assert!(message.contains("Environment file not found:"));
                assert!(message.contains("missing_env.json"));
            }
            other => panic!("unexpected error variant: {other:?}"),
        }
    }

    #[test]
    fn prepare_env_builds_registered_command_environment() {
        with_env_lock(|| {
            let temp_dir = tempdir().expect("tempdir should be created");
            let commands_file = create_legacy_commands_dir(
                temp_dir.path(),
                json!({
                    "tool": {
                        "environment": ["tool_env.json"],
                        "alias": ["tool.exe", "--flag"]
                    }
                }),
                &[
                    ("global_env.json", json!({"GLOBAL": "global"})),
                    ("tool_env.json", json!({"TOOL_ONLY": "tool"})),
                ],
            );
            let registry = CommandRegistry::new(Some(&commands_file))
                .expect("legacy commands file should load");
            let _env_guard =
                EnvVarGuard::set_many(&[("RUNTIME_ALLOWED", Some(OsStr::new("allowed")))]);

            let allowlist = vec![String::from("RUNTIME_ALLOWED")];
            let (env_map, command) =
                prepare_env("tool", &registry, None, false, Some(&allowlist), None)
                    .expect("registered command env should prepare");

            assert_eq!(command.name, "tool");
            assert_eq!(
                command.alias.as_ref(),
                Some(&vec![String::from("tool.exe"), String::from("--flag")])
            );
            assert_eq!(env_map.get("GLOBAL").map(String::as_str), Some("global"));
            assert_eq!(env_map.get("TOOL_ONLY").map(String::as_str), Some("tool"));
            assert_eq!(
                env_map.get("RUNTIME_ALLOWED").map(String::as_str),
                Some("allowed")
            );
        });
    }

    #[test]
    fn prepare_env_supports_raw_path_commands_with_registered_env_override() {
        let temp_dir = tempdir().expect("tempdir should be created");
        let commands_file = create_legacy_commands_dir(
            temp_dir.path(),
            json!({
                "tool": {
                    "environment": ["tool_env.json"]
                }
            }),
            &[("tool_env.json", json!({"TOOL_ONLY": "tool"}))],
        );
        let registry =
            CommandRegistry::new(Some(&commands_file)).expect("legacy commands file should load");
        let raw_path = platform_absolute_raw_path();

        let (env_map, command) = prepare_env(raw_path, &registry, None, false, None, Some("tool"))
            .expect("raw path command should bypass registry lookup");

        assert_eq!(command.name, raw_path);
        assert_eq!(command.alias.as_ref(), Some(&vec![raw_path.to_string()]));
        assert_eq!(env_map.get("TOOL_ONLY").map(String::as_str), Some("tool"));
    }

    #[test]
    fn prepare_env_errors_for_unregistered_env_override() {
        let temp_dir = tempdir().expect("tempdir should be created");
        let commands_file = create_legacy_commands_dir(
            temp_dir.path(),
            json!({
                "tool": {
                    "environment": ["tool_env.json"]
                }
            }),
            &[("tool_env.json", json!({"TOOL_ONLY": "tool"}))],
        );
        let registry =
            CommandRegistry::new(Some(&commands_file)).expect("legacy commands file should load");

        let error = prepare_env("tool", &registry, None, false, None, Some("ghost"))
            .expect_err("missing env override should fail");

        match error {
            EnvoyError::CommandNotFound(message) => {
                assert!(message.contains("Environment override command 'ghost'"));
            }
            other => panic!("unexpected error variant: {other:?}"),
        }
    }
}
