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
use crate::discovery::{
    discover_bundles_auto, discover_bundles_from_roots, is_published_bundle, Bundle, BundleInfo,
    BUNDLE_CHECKOUT, BUNDLE_ENV_DIR,
};
use crate::environment::EnvironmentManager;
use crate::error::{EnvoyError, Result};
use crate::package_cache::PackageCache;
use crate::retry::{retry_sync, RetryConfig};
use crate::semver::VersionSpec;
use crate::team_config::TeamConfig;

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
    package_cache: Option<&PackageCache>,
) -> Result<(CommandRegistry, Option<Vec<BundleInfo>>)> {
    let mut registry = CommandRegistry::empty();
    let mut bundles = None;
    let mut package_cache = package_cache.and_then(|cache| match PackageCache::new(cache.root()) {
        Ok(cache) => Some(cache),
        Err(error) => {
            tracing::warn!(
                cache_root = %cache.root().display(),
                "Failed to open package cache for runtime resolution: {error}"
            );
            None
        }
    });

    if let Some(bundle_roots) = bundle_roots {
        // `discover_bundles_from_roots` is infallible (returns Vec directly),
        // but we still wrap it for consistency with the retry pattern.
        let discovered: Vec<_> = retry_sync(
            &IO_RETRY_CONFIG,
            || -> std::result::Result<Vec<BundleInfo>, EnvoyError> {
                Ok(discover_bundles_from_roots(bundle_roots))
            },
        )
        .map_err(|e| EnvoyError::EnvironmentBuild(format!("Bundle discovery failed: {e}")))?;
        if !discovered.is_empty() {
            // Try to resolve cached versions for each discovered bundle.
            let team_config = resolve_team_config_for_bundles(Some(&discovered));
            let resolved =
                resolve_cached_bundles(discovered, package_cache.as_mut(), team_config.as_ref());
            registry.load_from_bundles(&resolved);
            bundles = Some(resolved);
        }
    } else {
        let discovered = retry_sync(&IO_RETRY_CONFIG, discover_bundles_auto)
            .map_err(|e| EnvoyError::EnvironmentBuild(format!("Bundle discovery failed: {e}")))?;
        if !discovered.is_empty() {
            // Try to resolve cached versions for each discovered bundle.
            let team_config = resolve_team_config_for_bundles(Some(&discovered));
            let resolved =
                resolve_cached_bundles(discovered, package_cache.as_mut(), team_config.as_ref());
            registry.load_from_bundles(&resolved);
            bundles = Some(resolved);
        }
    }

    if registry.is_empty() {
        let commands_file = match commands_file {
            Some(path) => Some(path.to_path_buf()),
            None => find_commands_file(None)?,
        };

        if let Some(commands_file) = commands_file {
            retry_sync(&IO_RETRY_CONFIG, || {
                registry.load_from_file(&commands_file, None)
            })
            .map_err(|e| {
                EnvoyError::EnvironmentBuild(format!("Failed to load commands file: {e}"))
            })?;
        }
    }

    Ok((registry, bundles))
}

/// Resolve cached versions for discovered bundles.
///
/// For each **published/production** bundle, attempts to find a matching
/// entry in the package cache using the bundle's bndlid as the package_id.
/// If found, returns a new BundleInfo with the cached path. On a cache miss,
/// envoy can also try to fetch the published package from the active team's
/// production package root and populate the cache before retrying the
/// substitution. Any fetch failure is logged and treated as a soft fallback
/// to the original discovered path.
///
/// Checkout (dev) bundles -- i.e. any bundle root without a `.bundle` marker
/// file, per [`is_published_bundle`] -- are always returned unchanged and
/// never consulted against the cache. Package caching exists to speed up
/// *production* package resolution (per its own design intent); silently
/// swapping a developer's own working checkout for a stale cached snapshot
/// just because its `namespace:name` happens to match a cached entry would
/// be actively harmful, not a performance optimization.
///
/// Public so callers with their own bundle-discovery path (e.g. `envoy-cli`'s
/// `load_registry_for_cli`, which does not go through [`load_registry`]) can
/// still apply package-cache resolution consistently. Pass `None` to skip
/// caching entirely; see [`crate::package_cache::open_default_package_cache`]
/// for the standard way to obtain a cache honoring `ENVOY_PACKAGE_CACHE` /
/// `ENVOY_DISABLE_PACKAGE_CACHE` / the `package_cache_dir` user setting.
pub fn resolve_cached_bundles(
    bundles: Vec<BundleInfo>,
    package_cache: Option<&mut PackageCache>,
    team_config: Option<&TeamConfig>,
) -> Vec<BundleInfo> {
    let Some(cache) = package_cache else {
        return bundles;
    };

    // Parse a wildcard version spec that matches any version.
    let wildcard_spec = match VersionSpec::parse(">=0.0.0") {
        Ok(spec) => spec,
        Err(_) => return bundles, // Fallback to original if parsing fails
    };

    let mut resolved_bundles = Vec::with_capacity(bundles.len());

    for bundle in bundles {
        // Never substitute a developer's own checkout for a cached copy.
        if !is_published_bundle(&bundle.root) {
            resolved_bundles.push(bundle);
            continue;
        }

        let bndlid = bundle.bndlid();
        let resolved_root = match cache.resolve(&bndlid, &wildcard_spec) {
            Ok(Some(cached)) => Some(cached.path),
            Ok(None) => fetch_and_cache_published_package(&bundle, team_config, cache),
            Err(error) => {
                tracing::warn!(
                    package_id = %bndlid,
                    "Failed to resolve package cache entry: {error}"
                );
                None
            }
        };

        if let Some(root) = resolved_root {
            resolved_bundles.push(BundleInfo::new(
                root,
                bundle.name.clone(),
                bundle.namespace.clone(),
            ));
        } else {
            resolved_bundles.push(bundle);
        }
    }

    resolved_bundles
}

/// Try to fill a published package cache miss from the team package root.
///
/// The production package root is expected to mirror bundle discovery's
/// namespace/name layout with a version leaf:
/// `<prod_packages_root>\namespace\name\version\`.
/// That keeps package lookup deterministic from a discovered published
/// bundle's `namespace:name` and its `.bundle` marker version.
fn fetch_and_cache_published_package(
    bundle: &BundleInfo,
    team_config: Option<&TeamConfig>,
    cache: &mut PackageCache,
) -> Option<PathBuf> {
    let bndlid = bundle.bndlid();
    let Some(team_config) = team_config else {
        tracing::warn!(
            package_id = %bndlid,
            "Package cache miss could not be filled because no team config was resolved"
        );
        return None;
    };
    let Some(prod_packages_root) = team_config.prod_packages_root.as_ref() else {
        tracing::warn!(
            package_id = %bndlid,
            team = %team_config.name,
            "Package cache miss could not be filled because prod_packages_root is not configured"
        );
        return None;
    };

    let version = Bundle::from_info(bundle.clone()).version();
    if version == BUNDLE_CHECKOUT {
        tracing::warn!(
            package_id = %bndlid,
            bundle_root = %bundle.root.display(),
            "Package cache miss could not be filled because the published version is unavailable"
        );
        return None;
    }

    let source_dir = prod_packages_root
        .join(&bundle.namespace)
        .join(&bundle.name)
        .join(&version);

    if !source_dir.is_dir() {
        tracing::warn!(
            package_id = %bndlid,
            source_dir = %source_dir.display(),
            "Package cache miss could not be filled because the package directory was not found"
        );
        return None;
    }

    let envoy_dir = source_dir.join(BUNDLE_ENV_DIR);
    if !envoy_dir.is_dir() {
        tracing::warn!(
            package_id = %bndlid,
            source_dir = %source_dir.display(),
            "Package cache miss could not be filled because the package directory is invalid"
        );
        return None;
    }

    match cache.store(&bndlid, &version, &source_dir) {
        Ok(cached) => Some(cached.path),
        Err(error) => {
            tracing::warn!(
                package_id = %bndlid,
                source_dir = %source_dir.display(),
                "Package cache miss could not be filled because storing the package failed: {error}"
            );
            None
        }
    }
}

/// Resolve the active team configuration for the given bundles, if any.
///
/// Returns `None` when no discovered bundle defines a `.envoy/team.json` (the
/// common case) or when discovery otherwise fails. This mirrors the
/// graceful-degradation approach used elsewhere in envoy (e.g. malformed
/// team.json files are logged as warnings, not propagated as hard errors) --
/// an absent or unresolvable team config simply means "no team configured"
/// rather than aborting the caller.
///
/// This is the wiring point that makes [`crate::team_config::TeamConfig`]
/// reachable from the default runtime/CLI flow instead of only via direct,
/// manual calls to `team_config::resolve_team_config`.
pub fn resolve_team_config_for_bundles(
    bundles: Option<&[BundleInfo]>,
) -> Option<crate::team_config::TeamConfig> {
    let bundles = bundles?;
    if bundles.is_empty() {
        return None;
    }
    crate::team_config::resolve_team_config(bundles, None).ok()
}

/// Resolve the current pipeline for the given bundles based on
/// `ENVOY_PIPELINE_CONTEXT`, if any bundle defines a pipeline.
///
/// Returns `None` when no bundle defines a `.envoy/pipeline.json`, or no
/// pipeline matches the current context or default namespace -- this is
/// "no pipeline configured" rather than a hard error, matching the
/// graceful-degradation approach used elsewhere in envoy.
///
/// This is the wiring point that makes [`crate::pipeline::Pipeline`]
/// resolution reachable from the default runtime/CLI flow instead of only
/// via direct, manual calls to `pipeline::get_current_pipeline`.
pub fn resolve_current_pipeline_for_bundles(
    bundles: Option<&[BundleInfo]>,
) -> Option<crate::pipeline::Pipeline> {
    let bundles = bundles?;
    if bundles.is_empty() {
        return None;
    }
    crate::pipeline::get_current_pipeline(bundles, &crate::pipeline::PipelineConfig::default()).ok()
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

    use super::{
        collect_env_files, is_raw_path, load_registry, prepare_env, resolve_cached_bundles,
        resolve_current_pipeline_for_bundles, resolve_team_config_for_bundles,
    };
    use crate::commands::CommandRegistry;
    use crate::discovery::{BundleInfo, BUNDLE_ENV_DIR};
    use crate::error::EnvoyError;
    use crate::package_cache::PackageCache;
    use crate::semver::VersionSpec;
    use crate::team_config::TeamConfig;

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
            load_registry(Some(&roots), None, None).expect("explicit bundle roots should load");

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
                load_registry(None, None, None).expect("auto-discovery should load");

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

            let (registry, bundles) = load_registry(None, Some(&commands_file), None)
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

    fn bundle_with_file(
        temp_dir: &Path,
        namespace: &str,
        name: &str,
        file_name: &str,
        contents: &str,
    ) -> BundleInfo {
        let bundle_dir = temp_dir.join(namespace).join(name);
        fs::create_dir_all(bundle_dir.join(".envoy")).expect(".envoy dir should be created");
        fs::write(bundle_dir.join(".envoy").join(file_name), contents)
            .expect("fixture file should be written");
        BundleInfo::new(bundle_dir, name.to_string(), namespace.to_string())
    }

    fn create_published_bundle(
        temp_dir: &Path,
        namespace: &str,
        name: &str,
        version: &str,
        commands: &Value,
    ) -> BundleInfo {
        let bundle_dir = temp_dir.join(namespace).join(name);
        let envoy_dir = bundle_dir.join(".envoy");
        fs::create_dir_all(&envoy_dir).expect(".envoy dir should be created");
        write_json(&envoy_dir.join("commands.json"), commands);
        fs::write(
            bundle_dir.join(".bundle"),
            format!(r#"{{"version": "{version}"}}"#),
        )
        .expect(".bundle marker should write");
        BundleInfo::new(bundle_dir, name.to_string(), namespace.to_string())
    }

    fn create_prod_package(
        prod_root: &Path,
        namespace: &str,
        name: &str,
        version: &str,
        commands: &Value,
    ) -> PathBuf {
        let package_dir = prod_root.join(namespace).join(name).join(version);
        let envoy_dir = package_dir.join(".envoy");
        fs::create_dir_all(&envoy_dir).expect("package .envoy dir should be created");
        write_json(&envoy_dir.join("commands.json"), commands);
        fs::write(
            package_dir.join(".bundle"),
            format!(r#"{{"version": "{version}"}}"#),
        )
        .expect(".bundle marker should write");
        package_dir
    }

    fn team_config_with_prod_root(prod_root: &Path) -> TeamConfig {
        TeamConfig {
            name: String::from("team"),
            prod_packages_root: Some(prod_root.to_path_buf()),
            prod_pipelines_root: None,
            user_host_config_file: None,
            metadata: std::collections::HashMap::new(),
        }
    }

    fn any_version_spec() -> VersionSpec {
        VersionSpec::parse(">=0.0.0").expect("wildcard version spec should parse")
    }

    #[test]
    fn resolve_team_config_for_bundles_returns_none_without_bundles() {
        assert!(resolve_team_config_for_bundles(None).is_none());
        assert!(resolve_team_config_for_bundles(Some(&[])).is_none());
    }

    #[test]
    fn resolve_team_config_for_bundles_returns_none_when_no_team_json_present() {
        let temp_dir = tempdir().expect("tempdir should be created");
        let bundle = bundle_with_file(temp_dir.path(), "gt", "maya", "global_env.json", "{}");

        assert!(resolve_team_config_for_bundles(Some(&[bundle])).is_none());
    }

    #[test]
    fn resolve_team_config_for_bundles_finds_team_json() {
        let temp_dir = tempdir().expect("tempdir should be created");
        let bundle = bundle_with_file(
            temp_dir.path(),
            "gt",
            "maya",
            "team.json",
            r#"{"name": "bfd", "prodPackagesRoot": "\\\\server\\packages"}"#,
        );

        let team = resolve_team_config_for_bundles(Some(&[bundle]))
            .expect("team config should be discovered automatically");

        assert_eq!(team.name, "bfd");
    }

    #[test]
    fn resolve_current_pipeline_for_bundles_returns_none_without_bundles() {
        with_env_lock(|| {
            let _env_guard = EnvVarGuard::set_many(&[("ENVOY_PIPELINE_CONTEXT", None)]);
            assert!(resolve_current_pipeline_for_bundles(None).is_none());
            assert!(resolve_current_pipeline_for_bundles(Some(&[])).is_none());
        });
    }

    #[test]
    fn resolve_current_pipeline_for_bundles_finds_pipeline_json() {
        with_env_lock(|| {
            let _env_guard = EnvVarGuard::set_many(&[("ENVOY_PIPELINE_CONTEXT", None)]);
            let temp_dir = tempdir().expect("tempdir should be created");
            let bundle = bundle_with_file(
                temp_dir.path(),
                "gt",
                "maya",
                "pipeline.json",
                r#"{"name": "build", "namespace": "bfd"}"#,
            );

            let pipeline = resolve_current_pipeline_for_bundles(Some(&[bundle]))
                .expect("pipeline should be discovered automatically");

            assert_eq!(pipeline.name, "build");
            assert_eq!(pipeline.namespace, "bfd");
        });
    }

    #[test]
    fn resolve_cached_bundles_never_substitutes_checkout_bundles() {
        let temp_dir = tempdir().expect("tempdir should be created");
        let checkout = bundle_with_file(temp_dir.path(), "gt", "maya", "global_env.json", "{}");

        let cache_root = temp_dir.path().join("cache");
        let mut cache = PackageCache::new(&cache_root).expect("cache should open");
        let source_dir = temp_dir.path().join("cached_source");
        fs::create_dir_all(&source_dir).expect("cached source dir should be created");
        fs::write(source_dir.join("marker.txt"), "cached").expect("marker file should write");
        cache
            .store(&checkout.bndlid(), "1.0.0", &source_dir)
            .expect("store should succeed");

        let resolved = resolve_cached_bundles(vec![checkout.clone()], Some(&mut cache), None);

        assert_eq!(resolved.len(), 1);
        assert_eq!(
            resolved[0].root, checkout.root,
            "a checkout (dev) bundle must never be swapped for a cached copy, \
even when a matching cache entry exists"
        );
    }

    #[test]
    fn resolve_cached_bundles_substitutes_published_bundles() {
        let temp_dir = tempdir().expect("tempdir should be created");
        let published = create_published_bundle(
            temp_dir.path(),
            "gt",
            "maya",
            "1.0.0",
            &json!({"tool": {"alias": ["cached.exe"]}}),
        );

        let cache_root = temp_dir.path().join("cache");
        let mut cache = PackageCache::new(&cache_root).expect("cache should open");
        let source_dir = temp_dir.path().join("cached_source");
        fs::create_dir_all(source_dir.join(".envoy")).expect("cached source dir should be created");
        write_json(
            &source_dir.join(".envoy").join("commands.json"),
            &json!({"tool": {"alias": ["cached.exe"]}}),
        );
        cache
            .store(&published.bndlid(), "1.0.0", &source_dir)
            .expect("store should succeed");

        let resolved = resolve_cached_bundles(vec![published.clone()], Some(&mut cache), None);

        assert_eq!(resolved.len(), 1);
        assert_ne!(
            resolved[0].root, published.root,
            "a published/production bundle with a matching cache entry should \
be substituted with the cached path"
        );
    }

    #[test]
    fn resolve_cached_bundles_fetches_published_packages_from_prod_root() {
        let temp_dir = tempdir().expect("tempdir should be created");
        let original = create_published_bundle(
            temp_dir.path(),
            "gt",
            "maya",
            "1.2.3",
            &json!({"tool": {"alias": ["original.exe"]}}),
        );
        let prod_root = temp_dir.path().join("prod_packages");
        create_prod_package(
            &prod_root,
            "gt",
            "maya",
            "1.2.3",
            &json!({"tool": {"alias": ["fetched.exe"]}}),
        );

        let cache_root = temp_dir.path().join("cache");
        let mut cache = PackageCache::new(&cache_root).expect("cache should open");
        let team_config = team_config_with_prod_root(&prod_root);

        let resolved =
            resolve_cached_bundles(vec![original.clone()], Some(&mut cache), Some(&team_config));

        assert_eq!(resolved.len(), 1);
        assert_ne!(resolved[0].root, original.root);
        let commands_file = resolved[0].root.join(".envoy").join("commands.json");
        let commands = fs::read_to_string(&commands_file).expect("cached commands should read");
        assert!(
            commands.contains("fetched.exe"),
            "the fetched package copy should be the one written into the cache"
        );

        let cached = cache
            .resolve(&original.bndlid(), &any_version_spec())
            .expect("cache lookup should succeed")
            .expect("fetched package should be cached");
        assert_eq!(cached.path, resolved[0].root);
    }

    #[test]
    fn resolve_cached_bundles_reuses_cached_copy_after_source_is_removed() {
        let temp_dir = tempdir().expect("tempdir should be created");
        let original = create_published_bundle(
            temp_dir.path(),
            "gt",
            "maya",
            "2.0.0",
            &json!({"tool": {"alias": ["original.exe"]}}),
        );
        let prod_root = temp_dir.path().join("prod_packages");
        let source_dir = create_prod_package(
            &prod_root,
            "gt",
            "maya",
            "2.0.0",
            &json!({"tool": {"alias": ["fetched.exe"]}}),
        );

        let cache_root = temp_dir.path().join("cache");
        let mut cache = PackageCache::new(&cache_root).expect("cache should open");
        let team_config = team_config_with_prod_root(&prod_root);

        let first_resolved =
            resolve_cached_bundles(vec![original.clone()], Some(&mut cache), Some(&team_config));
        let first_path = first_resolved[0].root.clone();

        fs::remove_dir_all(&source_dir).expect("prod package source should be removed");

        let second_resolved =
            resolve_cached_bundles(vec![original.clone()], Some(&mut cache), Some(&team_config));

        assert_eq!(second_resolved.len(), 1);
        assert_eq!(second_resolved[0].root, first_path);
        let commands =
            fs::read_to_string(second_resolved[0].root.join(".envoy").join("commands.json"))
                .expect("cached commands should still read");
        assert!(commands.contains("fetched.exe"));
    }

    #[test]
    fn resolve_cached_bundles_falls_back_when_prod_package_source_is_missing() {
        let temp_dir = tempdir().expect("tempdir should be created");
        let original = create_published_bundle(
            temp_dir.path(),
            "gt",
            "maya",
            "3.0.0",
            &json!({"tool": {"alias": ["original.exe"]}}),
        );
        let prod_root = temp_dir.path().join("prod_packages");
        fs::create_dir_all(&prod_root).expect("prod root should be created");

        let cache_root = temp_dir.path().join("cache");
        let mut cache = PackageCache::new(&cache_root).expect("cache should open");
        let team_config = team_config_with_prod_root(&prod_root);

        let resolved =
            resolve_cached_bundles(vec![original.clone()], Some(&mut cache), Some(&team_config));

        assert_eq!(resolved.len(), 1);
        assert_eq!(resolved[0].root, original.root);

        let without_prod_root = TeamConfig::empty();
        let resolved_without_prod_root = resolve_cached_bundles(
            vec![original.clone()],
            Some(&mut cache),
            Some(&without_prod_root),
        );
        assert_eq!(resolved_without_prod_root[0].root, original.root);
    }

    #[test]
    fn resolve_cached_bundles_never_fetches_checkout_bundles() {
        let temp_dir = tempdir().expect("tempdir should be created");
        let checkout = bundle_with_file(temp_dir.path(), "gt", "maya", "commands.json", "{}");
        let prod_root = temp_dir.path().join("prod_packages");
        create_prod_package(
            &prod_root,
            "gt",
            "maya",
            "4.0.0",
            &json!({"tool": {"alias": ["fetched.exe"]}}),
        );

        let cache_root = temp_dir.path().join("cache");
        let mut cache = PackageCache::new(&cache_root).expect("cache should open");
        let team_config = team_config_with_prod_root(&prod_root);

        let resolved =
            resolve_cached_bundles(vec![checkout.clone()], Some(&mut cache), Some(&team_config));

        assert_eq!(resolved.len(), 1);
        assert_eq!(resolved[0].root, checkout.root);
        assert!(cache
            .resolve(&checkout.bndlid(), &any_version_spec())
            .expect("cache lookup should succeed")
            .is_none());
    }
}
