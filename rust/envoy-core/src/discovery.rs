//! Bundle discovery and bundle-config loading for `envoy-core`.
//!
//! This module ports `py/envoy/_discovery.py` into Rust. It is responsible
//! for:
//! - resolving bundle IDs like `gt:pythoncore`
//! - scanning bundle root directories for checkout and published bundles
//! - loading bundle lists from JSON bundle-config files
//! - exposing small model types used by later environment/command ports
//!
//! Discovery currently supports two sources:
//! 1. Auto-discovery from `ENVOY_BNDL_ROOTS`
//! 2. Explicit bundle-config JSON files
//!
//! Published bundles are detected by a `.bundle` marker file. Checkout
//! bundles are detected by a `.git/` directory. In both cases a valid envoy
//! bundle must also contain a `.envoy/` directory.

use std::cell::RefCell;
use std::collections::{HashMap, HashSet};
use std::env;
use std::ffi::OsStr;
use std::fmt;
use std::fs;
use std::path::{Component, Path, PathBuf};
use std::sync::OnceLock;

use regex::{Captures, Regex};
use serde_json::Value;

use crate::config_registry::{is_config_name, resolve_named_config};
use crate::error::{EnvoyError, Result};
use crate::user_config::UserConfig;

const BUNDLE_ROOTS_VAR: &str = "ENVOY_BNDL_ROOTS";
const BUNDLES_CONFIG_VAR: &str = "ENVOY_BUNDLES_CONFIG";

/// Version sentinel for a bundle that lives directly in a git checkout.
pub const BUNDLE_CHECKOUT: &str = "checkout";

/// Default namespace prefix for bundles.
pub const BUNDLE_DEFAULT_NAMESPACE: &str = "gt";

/// Marker file written by `engit publish`.
pub const BUNDLE_MARKER_FILE: &str = ".bundle";

/// Per-bundle envoy config directory name.
pub const BUNDLE_ENV_DIR: &str = ".envoy";

fn namespace_regex() -> &'static Regex {
    static REGEX: OnceLock<Regex> = OnceLock::new();

    REGEX.get_or_init(|| {
        Regex::new(r"^[A-Za-z][A-Za-z0-9_]{1,19}$").expect("namespace regex must compile")
    })
}

fn bndlid_regex() -> &'static Regex {
    static REGEX: OnceLock<Regex> = OnceLock::new();

    REGEX.get_or_init(|| {
        Regex::new(r"^([A-Za-z][A-Za-z0-9_]{1,19}):([A-Za-z][A-Za-z0-9_-]*)$")
            .expect("bundle-id regex must compile")
    })
}

fn bundle_path_var_regex() -> &'static Regex {
    static REGEX: OnceLock<Regex> = OnceLock::new();

    REGEX.get_or_init(|| {
        Regex::new(r"\$\{([A-Za-z_][A-Za-z0-9_]*)\}")
            .expect("bundle-path-variable regex must compile")
    })
}

/// Expand `${VARNAME}` references in a bundle-config path string.
///
/// Returns `None` when any referenced variable is undefined so callers can
/// skip the entry, matching the Python implementation's effective behavior.
///
/// Note:
/// Logging for skipped variables is intentionally deferred until `envoy-core`
/// adopts a concrete logging backend.
pub(crate) fn expand_bundle_path(raw: &str, config_file: &Path) -> Option<String> {
    let _ = config_file;

    let mut unresolved = Vec::new();
    let expanded = bundle_path_var_regex().replace_all(raw, |captures: &Captures<'_>| {
        let var_name = captures
            .get(1)
            .expect("bundle-path variable regex must capture one group")
            .as_str();

        match env::var(var_name) {
            Ok(value) => value,
            Err(_) => {
                unresolved.push(var_name.to_string());
                String::new()
            }
        }
    });

    if unresolved.is_empty() {
        Some(expanded.into_owned())
    } else {
        None
    }
}

/// Return `true` if `spec` looks like a bundle ID (`<namespace>:<name>`).
pub fn is_bndlid(spec: &str) -> bool {
    bndlid_regex().is_match(spec)
}

/// Resolve a bundle ID to a filesystem path via `ENVOY_BNDL_ROOTS`.
///
/// The resolution order matches Python:
/// 1. Fast path: `<root>/<namespace>/<name>`
/// 2. Fallback scan: full bundle discovery under each root
pub fn resolve_bndlid(bndlid: &str) -> Result<PathBuf> {
    let Some((namespace, name)) = parse_bndlid(bndlid) else {
        return Err(EnvoyError::EnvironmentBuild(format!(
            "Invalid bundle ID: {bndlid:?}"
        )));
    };

    let roots_str = env::var(BUNDLE_ROOTS_VAR).unwrap_or_default();
    if roots_str.is_empty() {
        return Err(EnvoyError::EnvironmentBuild(format!(
            "Cannot resolve bndlid {bndlid:?}: {BUNDLE_ROOTS_VAR} is not set"
        )));
    }

    let roots = split_root_dirs(&roots_str);

    for root in &roots {
        let candidate = resolve_input_path(&root.join(&namespace).join(&name));
        if candidate.is_dir() && candidate.join(BUNDLE_ENV_DIR).is_dir() {
            return Ok(candidate);
        }
    }

    let root_strings = roots
        .iter()
        .map(|root| root.display().to_string())
        .collect::<Vec<_>>();
    let infos = discover_bundles_from_roots(&root_strings);
    for info in infos {
        if info.bndlid() == bndlid {
            return Ok(info.root);
        }
    }

    let searched = roots
        .iter()
        .map(|root| root.display().to_string())
        .collect::<Vec<_>>()
        .join(", ");

    Err(EnvoyError::EnvironmentBuild(format!(
        "Bundle {bndlid:?} not found in {BUNDLE_ROOTS_VAR} ({searched})"
    )))
}

/// Infer a bundle namespace from the parent directory name.
pub fn infer_namespace(bundle_root: &Path) -> String {
    let Some(parent_name) = bundle_root.parent().and_then(Path::file_name) else {
        return String::from(BUNDLE_DEFAULT_NAMESPACE);
    };
    let parent_name = parent_name.to_string_lossy();

    if namespace_regex().is_match(&parent_name) {
        parent_name.into_owned()
    } else {
        String::from(BUNDLE_DEFAULT_NAMESPACE)
    }
}

/// Information about a discovered bundle.
#[derive(Clone, Eq, PartialEq)]
pub struct BundleInfo {
    /// Bundle root directory.
    pub root: PathBuf,
    /// Bundle name.
    pub name: String,
    /// Bundle namespace.
    pub namespace: String,
    envoy_env: PathBuf,
    env_files: HashMap<String, PathBuf>,
}

impl BundleInfo {
    /// Create bundle metadata for a discovered bundle root.
    pub fn new(root: PathBuf, name: String, namespace: String) -> Self {
        let envoy_env = root.join(BUNDLE_ENV_DIR);
        let env_files = Self::index_env_files_for(&envoy_env);

        Self {
            root,
            name,
            namespace,
            envoy_env,
            env_files,
        }
    }

    /// Return the namespaced bundle identifier.
    pub fn bndlid(&self) -> String {
        format!("{}:{}", self.namespace, self.name)
    }

    /// Return the `.envoy/` directory for this bundle.
    pub fn envoy_env(&self) -> &Path {
        &self.envoy_env
    }

    /// Return all indexed `.json` files in `.envoy/`.
    pub fn env_files(&self) -> &HashMap<String, PathBuf> {
        &self.env_files
    }

    /// Re-scan `.envoy/` and index all `.json` files by filename.
    #[cfg_attr(not(test), allow(dead_code))]
    pub(crate) fn index_env_files(&self) -> HashMap<String, PathBuf> {
        Self::index_env_files_for(&self.envoy_env)
    }

    fn index_env_files_for(envoy_env: &Path) -> HashMap<String, PathBuf> {
        if !envoy_env.is_dir() {
            return HashMap::new();
        }

        let Ok(read_dir) = fs::read_dir(envoy_env) else {
            return HashMap::new();
        };

        let mut files = HashMap::new();
        for entry in read_dir.flatten() {
            let path = entry.path();
            if path.is_file() && path.extension() == Some(OsStr::new("json")) {
                if let Some(file_name) = path.file_name().and_then(OsStr::to_str) {
                    files.insert(file_name.to_string(), path);
                }
            }
        }

        files
    }
}

impl fmt::Debug for BundleInfo {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            formatter,
            "BundleInfo(bndlid='{}', root={})",
            self.bndlid(),
            self.root.display()
        )
    }
}

impl fmt::Display for BundleInfo {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{} ({})", self.name, self.root.display())
    }
}

/// A discovered envoy bundle.
#[derive(Clone, Eq, PartialEq)]
pub struct Bundle {
    info: BundleInfo,
}

impl Bundle {
    /// Construct a bundle from a bundle ID or filesystem path.
    pub fn new(spec: impl AsRef<Path>, namespace: Option<&str>) -> Result<Self> {
        let spec_path = spec.as_ref();
        let raw_spec = spec_path.as_os_str().to_string_lossy().into_owned();

        let (root, namespace) = if is_bndlid(&raw_spec) {
            let namespace = parse_bndlid(&raw_spec)
                .expect("bundle-id regex must parse after is_bndlid() succeeds")
                .0;

            (resolve_bndlid(&raw_spec)?, namespace)
        } else {
            let root = resolve_input_path(spec_path);
            if !root.is_dir() {
                return Err(EnvoyError::Validation(format!(
                    "Bundle path does not exist: {}",
                    root.display()
                )));
            }
            if !root.join(BUNDLE_ENV_DIR).is_dir() {
                return Err(EnvoyError::Validation(format!(
                    "Not a valid bundle (no {BUNDLE_ENV_DIR}/): {}",
                    root.display()
                )));
            }

            let namespace = namespace
                .map(ToOwned::to_owned)
                .unwrap_or_else(|| infer_namespace(&root));

            (root, namespace)
        };

        let name = root
            .file_name()
            .map(|value| value.to_string_lossy().into_owned())
            .unwrap_or_default();

        Ok(Self {
            info: BundleInfo::new(root, name, namespace),
        })
    }

    /// Construct a bundle directly from cached discovery info.
    pub(crate) fn from_info(info: BundleInfo) -> Self {
        Self { info }
    }

    /// Return the bundle name.
    pub fn name(&self) -> &str {
        &self.info.name
    }

    /// Return the bundle namespace.
    pub fn namespace(&self) -> &str {
        &self.info.namespace
    }

    /// Return the namespaced bundle identifier.
    pub fn bndlid(&self) -> String {
        self.info.bndlid()
    }

    /// Return the bundle version.
    ///
    /// Production bundles read `version` from the `.bundle` marker file.
    /// Checkout bundles return [`BUNDLE_CHECKOUT`].
    pub fn version(&self) -> String {
        let marker = self.info.root.join(BUNDLE_MARKER_FILE);
        let Ok(text) = fs::read_to_string(marker) else {
            return String::from(BUNDLE_CHECKOUT);
        };
        let Ok(data) = serde_json::from_str::<Value>(&text) else {
            return String::from(BUNDLE_CHECKOUT);
        };

        match data.get("version") {
            Some(value) if json_value_truthy(value) => json_value_to_string(value),
            _ => String::from(BUNDLE_CHECKOUT),
        }
    }

    /// Return `true` when the bundle is a published bundle directory.
    pub fn is_production(&self) -> bool {
        self.info.root.join(BUNDLE_MARKER_FILE).is_file()
    }

    /// Return `true` when the bundle is a checkout bundle.
    pub fn is_checkout(&self) -> bool {
        !self.is_production()
    }

    /// Return the bundle root directory.
    pub fn path(&self) -> &Path {
        &self.info.root
    }

    /// Return the bundle `.envoy/` directory.
    pub fn envoy_env(&self) -> &Path {
        self.info.envoy_env()
    }

    /// Return all indexed `.json` files in `.envoy/`.
    pub fn env_files(&self) -> HashMap<String, PathBuf> {
        self.info.env_files().clone()
    }

    /// Return the sorted command names defined in `commands.json`.
    ///
    /// Invalid or missing files return an empty list, matching Python.
    pub fn commands(&self) -> Vec<String> {
        let commands_file = self.info.envoy_env().join("commands.json");
        let Ok(text) = fs::read_to_string(commands_file) else {
            return Vec::new();
        };
        let Ok(value) = serde_json::from_str::<Value>(&text) else {
            return Vec::new();
        };
        let Value::Object(object) = value else {
            return Vec::new();
        };

        let mut commands = object.keys().cloned().collect::<Vec<_>>();
        commands.sort();
        commands
    }
}

impl fmt::Debug for Bundle {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            formatter,
            "Bundle(bndlid='{}', path={})",
            self.bndlid(),
            self.path().display()
        )
    }
}

impl fmt::Display for Bundle {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Debug::fmt(self, formatter)
    }
}

/// An envoy bundle configuration file.
pub struct BundleConfig {
    path: PathBuf,
    bundles: RefCell<Option<Vec<Bundle>>>,
    name: Option<String>,
    cfg_version: Option<String>,
}

impl BundleConfig {
    /// Create a bundle config from a filesystem path.
    pub fn new(path: impl AsRef<Path>) -> Result<Self> {
        let path = resolve_input_path(path.as_ref());
        if !path.is_file() {
            return Err(EnvoyError::Validation(format!(
                "BundleConfig path does not exist: {}",
                path.display()
            )));
        }

        Ok(Self {
            path,
            bundles: RefCell::new(None),
            name: None,
            cfg_version: None,
        })
    }

    /// Construct a config already resolved from a named config slot.
    pub(crate) fn from_named(path: PathBuf, name: String, version: String) -> Self {
        Self {
            path: normalize_windows_path(path),
            bundles: RefCell::new(None),
            name: Some(name),
            cfg_version: Some(version),
        }
    }

    /// Resolve and load a bundle config from a named config slot.
    pub fn from_name(name: &str) -> Result<Self> {
        let Some(resolved) = resolve_named_config(name) else {
            return Err(EnvoyError::Validation(format!(
                "Named config {name:?} not found in ENVOY_CFG_ROOTS. Check that \
ENVOY_CFG_ROOTS is set and the config has been published."
            )));
        };

        let version = resolved
            .file_stem()
            .map(|stem| stem.to_string_lossy().into_owned())
            .unwrap_or_default();

        Ok(Self::from_named(resolved, name.to_string(), version))
    }

    /// Return the active bundle config from user config, if present.
    pub fn current(ignore_user_config: bool) -> Result<Option<Self>> {
        if ignore_user_config {
            return Ok(None);
        }

        let user_config = UserConfig::load(None);
        let Some(raw) = user_config.get("bundles_config") else {
            return Ok(None);
        };

        if is_config_name(raw) {
            let Some(resolved) = resolve_named_config(raw) else {
                return Err(EnvoyError::Validation(format!(
                    "Named config {raw:?} (from user config) not found in ENVOY_CFG_ROOTS."
                )));
            };

            let version = resolved
                .file_stem()
                .map(|stem| stem.to_string_lossy().into_owned())
                .unwrap_or_default();

            return Ok(Some(Self::from_named(resolved, raw.to_string(), version)));
        }

        Self::new(raw).map(Some)
    }

    /// Return the absolute config-file path.
    pub fn path(&self) -> &Path {
        &self.path
    }

    /// Return the named config slot, if this config was resolved by name.
    pub fn name(&self) -> Option<&str> {
        self.name.as_deref()
    }

    /// Return the resolved named-config version, if any.
    pub fn cfg_version(&self) -> Option<&str> {
        self.cfg_version.as_deref()
    }

    /// Return the bundles declared in this config.
    ///
    /// The successful result is cached after first load.
    pub fn bundles(&self) -> Result<Vec<Bundle>> {
        if let Some(cached) = self.bundles.borrow().as_ref() {
            return Ok(cached.clone());
        }

        let bundles = load_bundles_from_config(&self.path)?
            .into_iter()
            .map(Bundle::from_info)
            .collect::<Vec<_>>();

        *self.bundles.borrow_mut() = Some(bundles.clone());
        Ok(bundles)
    }

    /// Return the sorted union of command names across all bundles.
    pub fn commands(&self) -> Result<Vec<String>> {
        let mut seen = HashSet::new();
        for bundle in self.bundles()? {
            seen.extend(bundle.commands());
        }

        let mut commands = seen.into_iter().collect::<Vec<_>>();
        commands.sort();
        Ok(commands)
    }
}

impl fmt::Debug for BundleConfig {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match &self.name {
            Some(name) => {
                write!(
                    formatter,
                    "BundleConfig(name='{}', path={})",
                    name,
                    self.path.display()
                )
            }
            None => write!(formatter, "BundleConfig(path={})", self.path.display()),
        }
    }
}

impl fmt::Display for BundleConfig {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Debug::fmt(self, formatter)
    }
}

/// Return `true` if `path` contains a `.git/` directory.
pub fn is_git_repo(path: &Path) -> bool {
    path.join(".git").is_dir()
}

/// Return `true` if `path` contains a `.bundle` marker file.
pub fn is_published_bundle(path: &Path) -> bool {
    path.join(BUNDLE_MARKER_FILE).is_file()
}

/// Return `true` if `path` contains a `.envoy/` directory.
pub fn has_envoy_env(path: &Path) -> bool {
    path.join(BUNDLE_ENV_DIR).is_dir()
}

/// Return `true` if `path` is a valid envoy bundle directory.
pub fn validate_bundle(path: &Path) -> bool {
    path.is_dir() && has_envoy_env(path)
}

/// Recursively find checkout or published bundle roots below `root_dir`.
pub fn find_bundle_roots(root_dir: &Path, max_depth: usize) -> Vec<PathBuf> {
    let mut bundle_roots = Vec::new();

    if !root_dir.is_dir() {
        return bundle_roots;
    }

    search_dir(
        root_dir,
        0,
        max_depth,
        SearchMode::Bundles,
        &mut bundle_roots,
    );
    bundle_roots
}

/// Recursively find git repositories below `root_dir`.
pub fn find_git_repos(root_dir: &Path, max_depth: usize) -> Vec<PathBuf> {
    let mut repos = Vec::new();

    if !root_dir.is_dir() {
        return repos;
    }

    search_dir(root_dir, 0, max_depth, SearchMode::GitRepos, &mut repos);
    repos
}

/// Discover bundles under the provided root directories.
pub fn discover_bundles_from_roots(root_dirs: &[String]) -> Vec<BundleInfo> {
    let mut bundles = Vec::new();

    for root_str in root_dirs {
        let root = resolve_input_path(Path::new(root_str));
        let candidates = find_bundle_roots(&root, 5);

        for candidate_path in candidates {
            if validate_bundle(&candidate_path) {
                let (name, namespace) = name_and_namespace(&candidate_path);
                bundles.push(BundleInfo::new(candidate_path, name, namespace));
            }
        }
    }

    bundles
}

fn name_and_namespace(bundle_root: &Path) -> (String, String) {
    let marker = bundle_root.join(BUNDLE_MARKER_FILE);
    if marker.is_file() {
        if let Ok(text) = fs::read_to_string(marker) {
            if let Ok(data) = serde_json::from_str::<Value>(&text) {
                if let Some(bndlid) = data.get("bndlid").and_then(Value::as_str) {
                    if let Some((namespace, name)) = bndlid.split_once(':') {
                        return (name.to_string(), namespace.to_string());
                    }
                }

                if let Some(name) = data.get("name").filter(|value| json_value_truthy(value)) {
                    return (json_value_to_string(name), infer_namespace(bundle_root));
                }
            }
        }
    }

    (
        bundle_root
            .file_name()
            .map(|value| value.to_string_lossy().into_owned())
            .unwrap_or_default(),
        infer_namespace(bundle_root),
    )
}

/// Auto-discover bundles using `ENVOY_BNDL_ROOTS`.
///
/// If `ENVOY_BUNDLES_CONFIG` points at an existing bundle-config file, that
/// file is loaded instead of scanning roots.
pub fn discover_bundles_auto() -> Result<Vec<BundleInfo>> {
    let bundles_config = env::var(BUNDLES_CONFIG_VAR).unwrap_or_default();
    let bundles_config = bundles_config.trim();
    if !bundles_config.is_empty() {
        let config_path = resolve_input_path(Path::new(bundles_config));
        if config_path.is_file() {
            return load_bundles_from_config(&config_path);
        }
    }

    let roots_str = env::var(BUNDLE_ROOTS_VAR).unwrap_or_default();
    if roots_str.is_empty() {
        return Ok(Vec::new());
    }

    let root_dirs = roots_str
        .split(root_separator())
        .map(str::trim)
        .filter(|root| !root.is_empty())
        .map(ToOwned::to_owned)
        .collect::<Vec<_>>();

    if root_dirs.is_empty() {
        return Ok(Vec::new());
    }

    Ok(discover_bundles_from_roots(&root_dirs))
}

/// Load bundle definitions from a bundle-config JSON file.
pub fn load_bundles_from_config(config_file: &Path) -> Result<Vec<BundleInfo>> {
    let config_file = resolve_input_path(config_file);
    if !config_file.is_file() {
        return Err(EnvoyError::EnvironmentBuild(format!(
            "Config file not found: {}",
            config_file.display()
        )));
    }

    let contents = fs::read_to_string(&config_file).map_err(|source| {
        EnvoyError::EnvironmentBuild(format!("Error reading config file: {source}"))
    })?;
    let data = serde_json::from_str::<Value>(&contents).map_err(|source| {
        EnvoyError::EnvironmentBuild(format!("Invalid JSON in config file: {source}"))
    })?;

    let bundle_paths = match data {
        Value::Object(mut object) => object.remove("bundles").unwrap_or(Value::Array(Vec::new())),
        Value::Array(entries) => Value::Array(entries),
        _ => {
            return Err(EnvoyError::EnvironmentBuild(String::from(
                "Config file must be a JSON object or array",
            )));
        }
    };

    let Value::Array(bundle_entries) = bundle_paths else {
        return Err(EnvoyError::EnvironmentBuild(String::from(
            "Config file must contain a bundles array",
        )));
    };

    let mut bundles = Vec::new();
    for bundle_entry in bundle_entries {
        let Value::String(raw_path) = bundle_entry else {
            continue;
        };

        let Some(expanded) = expand_bundle_path(&raw_path, &config_file) else {
            continue;
        };

        let path = resolve_input_path(Path::new(&expanded));
        if !validate_bundle(&path) {
            continue;
        }

        let name = path
            .file_name()
            .map(|value| value.to_string_lossy().into_owned())
            .unwrap_or_default();

        bundles.push(BundleInfo::new(path.clone(), name, infer_namespace(&path)));
    }

    Ok(bundles)
}

/// Return bundles from an explicit config file or from auto-discovery.
pub fn get_bundles(config_file: Option<&Path>) -> Result<Vec<BundleInfo>> {
    match config_file {
        Some(config_file) => load_bundles_from_config(config_file),
        None => discover_bundles_auto(),
    }
}

/// Return all non-`commands.json` env files grouped by bundle name.
pub fn get_bundle_env_files(bundles: &[BundleInfo]) -> HashMap<String, Vec<PathBuf>> {
    let mut env_files = HashMap::new();

    for bundle in bundles {
        let mut files = Vec::new();
        let Ok(read_dir) = fs::read_dir(bundle.envoy_env()) else {
            continue;
        };

        for entry in read_dir.flatten() {
            let path = entry.path();
            if path.is_file()
                && path.extension() == Some(OsStr::new("json"))
                && path.file_name() != Some(OsStr::new("commands.json"))
            {
                files.push(path);
            }
        }

        files.sort();
        if !files.is_empty() {
            env_files.insert(bundle.name.clone(), files);
        }
    }

    env_files
}

/// Return `commands.json` files grouped by bundle name.
pub fn get_bundle_commands_files(bundles: &[BundleInfo]) -> HashMap<String, PathBuf> {
    let mut commands_files = HashMap::new();

    for bundle in bundles {
        let commands_file = bundle.envoy_env().join("commands.json");
        if commands_file.is_file() {
            commands_files.insert(bundle.name.clone(), commands_file);
        }
    }

    commands_files
}

#[derive(Clone, Copy)]
enum SearchMode {
    Bundles,
    GitRepos,
}

fn search_dir(
    path: &Path,
    depth: usize,
    max_depth: usize,
    mode: SearchMode,
    results: &mut Vec<PathBuf>,
) {
    if depth > max_depth {
        return;
    }

    match mode {
        SearchMode::Bundles if is_git_repo(path) || is_published_bundle(path) => {
            results.push(path.to_path_buf());
            return;
        }
        SearchMode::GitRepos if is_git_repo(path) => {
            results.push(path.to_path_buf());
            return;
        }
        SearchMode::Bundles | SearchMode::GitRepos => {}
    }

    let Ok(read_dir) = fs::read_dir(path) else {
        return;
    };

    for entry in read_dir.flatten() {
        let entry_path = entry.path();
        let is_dir = entry
            .file_type()
            .map(|file_type| file_type.is_dir())
            .unwrap_or(false);
        if !is_dir {
            continue;
        }

        if entry_path
            .file_name()
            .map(|name| name.to_string_lossy().starts_with('.'))
            .unwrap_or(false)
        {
            continue;
        }

        search_dir(&entry_path, depth + 1, max_depth, mode, results);
    }
}

fn parse_bndlid(bndlid: &str) -> Option<(String, String)> {
    let captures = bndlid_regex().captures(bndlid)?;
    let namespace = captures.get(1)?.as_str().to_string();
    let name = captures.get(2)?.as_str().to_string();

    Some((namespace, name))
}

fn split_root_dirs(roots_str: &str) -> Vec<PathBuf> {
    roots_str
        .split(root_separator())
        .map(str::trim)
        .filter(|root| !root.is_empty())
        .map(|root| resolve_input_path(Path::new(root)))
        .collect()
}

fn root_separator() -> char {
    if cfg!(windows) {
        ';'
    } else {
        ':'
    }
}

fn resolve_input_path(path: &Path) -> PathBuf {
    let absolute = if path.is_absolute() {
        path.to_path_buf()
    } else {
        env::current_dir()
            .map(|cwd| cwd.join(path))
            .unwrap_or_else(|_| path.to_path_buf())
    };

    normalize_windows_path(
        fs::canonicalize(&absolute).unwrap_or_else(|_| lexical_normalize(&absolute)),
    )
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
                }
            }
            Component::Prefix(_) | Component::RootDir | Component::Normal(_) => {
                normalized.push(component.as_os_str());
            }
        }
    }

    normalized
}

fn normalize_windows_path(path: PathBuf) -> PathBuf {
    #[cfg(windows)]
    {
        let text = path.as_os_str().to_string_lossy();

        if let Some(stripped) = text.strip_prefix(r"\\?\UNC\") {
            return PathBuf::from(format!(r"\\{stripped}"));
        }

        if let Some(stripped) = text.strip_prefix(r"\\?\") {
            return PathBuf::from(stripped);
        }
    }

    path
}

fn json_value_truthy(value: &Value) -> bool {
    match value {
        Value::Null => false,
        Value::Bool(flag) => *flag,
        Value::Number(number) => {
            if let Some(integer) = number.as_i64() {
                return integer != 0;
            }
            if let Some(integer) = number.as_u64() {
                return integer != 0;
            }
            if let Some(float) = number.as_f64() {
                return float != 0.0;
            }

            false
        }
        Value::String(text) => !text.is_empty(),
        Value::Array(items) => !items.is_empty(),
        Value::Object(entries) => !entries.is_empty(),
    }
}

fn json_value_to_string(value: &Value) -> String {
    match value {
        Value::String(text) => text.clone(),
        Value::Number(number) => number.to_string(),
        Value::Bool(flag) => flag.to_string(),
        Value::Null => String::from("null"),
        Value::Array(_) | Value::Object(_) => value.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use std::collections::{HashMap, HashSet};
    use std::env;
    use std::ffi::{OsStr, OsString};
    use std::fs;
    use std::path::{Path, PathBuf};

    use serde_json::json;
    use serde_json::Value;
    use tempfile::tempdir;

    use super::{
        discover_bundles_auto, discover_bundles_from_roots, expand_bundle_path, find_bundle_roots,
        find_git_repos, get_bundle_commands_files, get_bundle_env_files, get_bundles,
        has_envoy_env, infer_namespace, is_bndlid, is_git_repo, is_published_bundle,
        load_bundles_from_config, resolve_bndlid, validate_bundle, Bundle, BundleConfig,
        BundleInfo, EnvoyError, BUNDLES_CONFIG_VAR, BUNDLE_CHECKOUT, BUNDLE_ENV_DIR,
        BUNDLE_MARKER_FILE, BUNDLE_ROOTS_VAR,
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
    /// process environment variables (e.g. `ENVOY_CFG_ROOTS` is touched by
    /// both `discovery` and `config_registry`), so a single shared lock is
    /// required to prevent cross-module test races under `cargo test`'s
    /// default parallel execution.
    fn with_env_lock<T>(test_fn: impl FnOnce() -> T) -> T {
        let _lock = crate::env_test_lock::MUTEX
            .lock()
            .unwrap_or_else(|poison| poison.into_inner());
        test_fn()
    }

    fn join_roots(roots: &[&Path]) -> OsString {
        env::join_paths(roots).expect("failed to join bundle roots")
    }

    fn write_json(path: &Path, value: &Value) {
        fs::write(
            path,
            serde_json::to_string_pretty(value).expect("failed to serialize test json"),
        )
        .expect("failed to write test json");
    }

    fn create_checkout_bundle(
        root: &Path,
        namespace: &str,
        name: &str,
        env_files: &[&str],
        commands: Option<Value>,
    ) -> PathBuf {
        let bundle_root = root.join(namespace).join(name);
        fs::create_dir_all(bundle_root.join(".git")).expect("failed to create .git");
        let envoy_env = bundle_root.join(BUNDLE_ENV_DIR);
        fs::create_dir_all(&envoy_env).expect("failed to create .envoy");

        for env_file in env_files {
            write_json(&envoy_env.join(env_file), &json!({"name": env_file}));
        }

        if let Some(commands) = commands {
            write_json(&envoy_env.join("commands.json"), &commands);
        }

        bundle_root
    }

    fn create_published_bundle(
        root: &Path,
        dir_name: &str,
        marker: Value,
        env_files: &[&str],
        commands: Option<Value>,
    ) -> PathBuf {
        let bundle_root = root.join(dir_name);
        let envoy_env = bundle_root.join(BUNDLE_ENV_DIR);
        fs::create_dir_all(&envoy_env).expect("failed to create .envoy");
        write_json(&bundle_root.join(BUNDLE_MARKER_FILE), &marker);

        for env_file in env_files {
            write_json(&envoy_env.join(env_file), &json!({"name": env_file}));
        }

        if let Some(commands) = commands {
            write_json(&envoy_env.join("commands.json"), &commands);
        }

        bundle_root
    }

    fn namespaced_map(bundles: &[BundleInfo]) -> HashMap<String, PathBuf> {
        bundles
            .iter()
            .map(|bundle| (bundle.bndlid(), bundle.root.clone()))
            .collect()
    }

    #[test]
    fn is_bndlid_matches_expected_examples() {
        assert!(is_bndlid("gt:pythoncore"));
        assert!(is_bndlid("tools_team:bundle-name"));
        assert!(!is_bndlid("g:pythoncore"));
        assert!(!is_bndlid("C:\\repo\\bundle"));
        assert!(!is_bndlid("1gt:pythoncore"));
        assert!(!is_bndlid("gt:"));
    }

    #[test]
    fn infer_namespace_uses_parent_directory_or_default() {
        let bundle_root = Path::new("C:\\repo\\gt\\pythoncore");
        let fallback_root = Path::new("C:\\repo\\some-dir\\pythoncore");

        assert_eq!(infer_namespace(bundle_root), "gt");
        assert_eq!(infer_namespace(fallback_root), "gt");
    }

    #[test]
    fn expand_bundle_path_expands_defined_vars_and_rejects_undefined_vars() {
        with_env_lock(|| {
            let temp = tempdir().expect("failed to create temp dir");
            let _root_guard = EnvVarGuard::set("TEST_BUNDLE_ROOT", Some(temp.path().as_os_str()));
            let _missing_guard = EnvVarGuard::set("TEST_MISSING_ROOT", None);

            let config_file = temp.path().join("bundles.json");
            assert_eq!(
                expand_bundle_path("${TEST_BUNDLE_ROOT}\\bundle", &config_file),
                Some(format!("{}\\bundle", temp.path().display()))
            );
            assert_eq!(
                expand_bundle_path("${TEST_MISSING_ROOT}\\bundle", &config_file),
                None
            );
        });
    }

    #[test]
    fn predicate_helpers_identify_checkout_and_published_bundles() {
        let temp = tempdir().expect("failed to create temp dir");
        let checkout = create_checkout_bundle(
            temp.path(),
            "gt",
            "pythoncore",
            &["python_env.json"],
            Some(json!({"python_dev": {}})),
        );
        let published = create_published_bundle(
            &temp.path().join("releases"),
            "v1.2.3",
            json!({"bndlid": "tools:render", "version": "1.2.3"}),
            &["render_env.json"],
            None,
        );

        assert!(is_git_repo(&checkout));
        assert!(!is_published_bundle(&checkout));
        assert!(has_envoy_env(&checkout));
        assert!(validate_bundle(&checkout));

        assert!(!is_git_repo(&published));
        assert!(is_published_bundle(&published));
        assert!(has_envoy_env(&published));
        assert!(validate_bundle(&published));
    }

    #[test]
    fn find_bundle_roots_honors_depth_limits_and_skips_hidden_directories() {
        let temp = tempdir().expect("failed to create temp dir");
        let root = temp.path();

        let shallow_checkout = create_checkout_bundle(
            root,
            "gt",
            "pythoncore",
            &["python_env.json"],
            Some(json!({"python_dev": {}})),
        );
        let published = create_published_bundle(
            &root.join("releases"),
            "v1.2.3",
            json!({"bndlid": "tools:render", "version": "1.2.3"}),
            &["render_env.json"],
            None,
        );
        let deep_checkout = create_checkout_bundle(
            &root.join("one").join("two"),
            "gt",
            "too_deep",
            &["too_deep_env.json"],
            None,
        );
        let hidden_checkout = create_checkout_bundle(
            &root.join(".hidden"),
            "gt",
            "skipped",
            &["skipped_env.json"],
            None,
        );

        let roots = find_bundle_roots(root, 3)
            .into_iter()
            .collect::<HashSet<_>>();
        assert!(roots.contains(&shallow_checkout));
        assert!(roots.contains(&published));
        assert!(!roots.contains(&hidden_checkout));

        let limited_roots = find_bundle_roots(root, 2)
            .into_iter()
            .collect::<HashSet<_>>();
        assert!(limited_roots.contains(&shallow_checkout));
        assert!(limited_roots.contains(&published));
        assert!(!limited_roots.contains(&deep_checkout));
    }

    #[test]
    fn find_git_repos_only_returns_checkout_bundles() {
        let temp = tempdir().expect("failed to create temp dir");
        let root = temp.path();

        let checkout = create_checkout_bundle(root, "gt", "pythoncore", &["python_env.json"], None);
        create_published_bundle(
            &root.join("releases"),
            "v1.2.3",
            json!({"bndlid": "tools:render", "version": "1.2.3"}),
            &["render_env.json"],
            None,
        );

        let repos = find_git_repos(root, 5);
        assert_eq!(repos, vec![checkout]);
    }

    #[test]
    fn discover_bundles_from_roots_uses_marker_bndlid_for_published_bundles() {
        let temp = tempdir().expect("failed to create temp dir");
        let root = temp.path();

        let checkout = create_checkout_bundle(root, "gt", "pythoncore", &["python_env.json"], None);
        let published = create_published_bundle(
            &root.join("releases"),
            "v1.2.3",
            json!({"bndlid": "tools:render", "version": "1.2.3"}),
            &["render_env.json"],
            None,
        );

        let bundles = discover_bundles_from_roots(&[root.display().to_string()]);
        let discovered = namespaced_map(&bundles);

        assert_eq!(discovered.get("gt:pythoncore"), Some(&checkout));
        assert_eq!(discovered.get("tools:render"), Some(&published));
    }

    #[test]
    fn resolve_bndlid_returns_environment_build_errors_for_invalid_inputs() {
        with_env_lock(|| {
            let _roots_guard = EnvVarGuard::set(BUNDLE_ROOTS_VAR, None);

            let error = resolve_bndlid("bad").expect_err("invalid bndlid should fail");
            assert!(matches!(error, EnvoyError::EnvironmentBuild(_)));

            let error = resolve_bndlid("gt:pythoncore")
                .expect_err("missing roots env should fail resolution");
            assert!(matches!(error, EnvoyError::EnvironmentBuild(_)));
        });
    }

    #[test]
    fn resolve_bndlid_falls_back_to_scan_for_published_bundle() {
        with_env_lock(|| {
            let temp = tempdir().expect("failed to create temp dir");
            let published = create_published_bundle(
                &temp.path().join("releases"),
                "v1.2.3",
                json!({"bndlid": "tools:render", "version": "1.2.3"}),
                &["render_env.json"],
                None,
            );
            let roots = join_roots(&[temp.path()]);
            let _roots_guard = EnvVarGuard::set(BUNDLE_ROOTS_VAR, Some(roots.as_os_str()));

            let resolved = resolve_bndlid("tools:render").expect("published bundle should resolve");
            assert_eq!(resolved, published);
        });
    }

    #[test]
    fn bundle_supports_path_specs_bndlid_specs_and_namespace_overrides() {
        with_env_lock(|| {
            let temp = tempdir().expect("failed to create temp dir");
            let checkout = create_checkout_bundle(
                temp.path(),
                "gt",
                "pythoncore",
                &["python_env.json", "maya_env.json"],
                Some(json!({"z_cmd": {}, "a_cmd": {}})),
            );
            let roots = join_roots(&[temp.path()]);
            let _roots_guard = EnvVarGuard::set(BUNDLE_ROOTS_VAR, Some(roots.as_os_str()));

            let by_path = Bundle::new(&checkout, None).expect("bundle path should be valid");
            assert_eq!(by_path.name(), "pythoncore");
            assert_eq!(by_path.namespace(), "gt");
            assert_eq!(by_path.bndlid(), "gt:pythoncore");
            assert_eq!(by_path.version(), BUNDLE_CHECKOUT);
            assert!(by_path.is_checkout());
            assert_eq!(by_path.commands(), vec!["a_cmd", "z_cmd"]);
            assert!(by_path.env_files().contains_key("commands.json"));

            let by_bndlid =
                Bundle::new("gt:pythoncore", Some("ignored")).expect("bundle ID should resolve");
            assert_eq!(by_bndlid.path(), checkout.as_path());

            let overridden = Bundle::new(&checkout, Some("tools"))
                .expect("bundle path with namespace override should be valid");
            assert_eq!(overridden.bndlid(), "tools:pythoncore");
        });
    }

    #[test]
    fn bundle_reads_marker_version_and_production_state() {
        let temp = tempdir().expect("failed to create temp dir");
        let published = create_published_bundle(
            &temp.path().join("releases"),
            "v1.2.3",
            json!({"bndlid": "tools:render", "version": "1.2.3"}),
            &["render_env.json"],
            Some(json!({"render": {}})),
        );

        let bundle = Bundle::new(&published, None).expect("published bundle path should be valid");
        assert_eq!(bundle.version(), "1.2.3");
        assert!(bundle.is_production());
        assert!(!bundle.is_checkout());
    }

    #[test]
    fn load_bundles_from_config_expands_env_vars_and_skips_invalid_entries() {
        with_env_lock(|| {
            let temp = tempdir().expect("failed to create temp dir");
            let checkout = create_checkout_bundle(
                temp.path(),
                "gt",
                "pythoncore",
                &["python_env.json"],
                Some(json!({"python_dev": {}})),
            );
            let config = temp.path().join("bundles.json");
            write_json(
                &config,
                &json!({
                    "bundles": [
                        "${TEST_DISCOVERY_BUNDLE}",
                        123,
                        "${TEST_DISCOVERY_MISSING}",
                        temp.path().join("missing").display().to_string()
                    ]
                }),
            );
            let _bundle_guard =
                EnvVarGuard::set("TEST_DISCOVERY_BUNDLE", Some(checkout.as_os_str()));
            let _missing_guard = EnvVarGuard::set("TEST_DISCOVERY_MISSING", None);

            let bundles = load_bundles_from_config(&config).expect("config should load");
            assert_eq!(bundles.len(), 1);
            assert_eq!(bundles[0].bndlid(), "gt:pythoncore");
        });
    }

    #[test]
    fn load_bundles_from_config_returns_environment_build_errors() {
        let temp = tempdir().expect("failed to create temp dir");
        let missing = temp.path().join("missing.json");
        let error = load_bundles_from_config(&missing).expect_err("missing config should fail");
        assert!(matches!(error, EnvoyError::EnvironmentBuild(_)));

        let invalid = temp.path().join("invalid.json");
        fs::write(&invalid, "{not valid json").expect("failed to write invalid config");
        let error = load_bundles_from_config(&invalid).expect_err("invalid config should fail");
        assert!(matches!(error, EnvoyError::EnvironmentBuild(_)));
    }

    #[test]
    fn bundle_config_loads_from_path_named_slot_and_user_config() {
        with_env_lock(|| {
            let temp = tempdir().expect("failed to create temp dir");
            let bundle_root = create_checkout_bundle(
                temp.path(),
                "gt",
                "pythoncore",
                &["python_env.json"],
                Some(json!({"python_dev": {}, "maya_dev": {}})),
            );

            let config_path = temp.path().join("bundles.json");
            write_json(&config_path, &json!([bundle_root.display().to_string()]));

            let bundle_config =
                BundleConfig::new(&config_path).expect("direct config path should be valid");
            assert_eq!(bundle_config.path(), config_path.as_path());
            assert_eq!(bundle_config.name(), None);
            assert_eq!(bundle_config.cfg_version(), None);
            assert_eq!(
                bundle_config
                    .commands()
                    .expect("commands should load from direct config"),
                vec!["maya_dev", "python_dev"]
            );

            let cfg_root = temp.path().join("cfg-root");
            let studio_dir = cfg_root.join("studio");
            fs::create_dir_all(&studio_dir).expect("failed to create named config dir");
            let version = "2026-06-21T10-13-00";
            let published_config = studio_dir.join(format!("{version}.json"));
            write_json(
                &published_config,
                &json!([bundle_root.display().to_string()]),
            );
            fs::write(studio_dir.join("latest"), format!("{version}.json"))
                .expect("failed to write latest pointer");

            let cfg_roots = env::join_paths([cfg_root.as_path()])
                .expect("failed to join config roots for test");
            let _cfg_roots_guard = EnvVarGuard::set("ENVOY_CFG_ROOTS", Some(cfg_roots.as_os_str()));

            let named = BundleConfig::from_name("studio").expect("named config should resolve");
            assert_eq!(named.name(), Some("studio"));
            assert_eq!(named.cfg_version(), Some(version));
            assert_eq!(named.path(), published_config.as_path());

            let user_config_path = temp.path().join("user_config.json");
            write_json(&user_config_path, &json!({"bundles_config": "studio"}));
            let _user_config_guard =
                EnvVarGuard::set("ENVOY_USER_CONFIG", Some(user_config_path.as_os_str()));

            let current = BundleConfig::current(false)
                .expect("current config should resolve")
                .expect("current config should be present");
            assert_eq!(current.name(), Some("studio"));
            assert_eq!(current.cfg_version(), Some(version));
            assert_eq!(current.path(), published_config.as_path());

            assert!(BundleConfig::current(true)
                .expect("ignore_user_config should not fail")
                .is_none());
        });
    }

    #[test]
    fn discover_bundles_auto_prefers_prebuilt_config_and_falls_back_to_roots() {
        with_env_lock(|| {
            let temp = tempdir().expect("failed to create temp dir");
            let root_bundle =
                create_checkout_bundle(temp.path(), "gt", "pythoncore", &["python_env.json"], None);
            let config_bundle = create_checkout_bundle(
                &temp.path().join("other-root"),
                "tools",
                "render",
                &["render_env.json"],
                None,
            );
            let roots = join_roots(&[temp.path()]);
            let _roots_guard = EnvVarGuard::set(BUNDLE_ROOTS_VAR, Some(roots.as_os_str()));

            let config_path = temp.path().join("bundles.json");
            write_json(&config_path, &json!([config_bundle.display().to_string()]));
            let config_guard = EnvVarGuard::set(BUNDLES_CONFIG_VAR, Some(config_path.as_os_str()));

            let bundles = discover_bundles_auto().expect("prebuilt config should load");
            let discovered = namespaced_map(&bundles);
            assert_eq!(discovered.get("tools:render"), Some(&config_bundle));
            assert!(!discovered.contains_key("gt:pythoncore"));

            drop(config_guard);
            let missing_config = temp.path().join("missing.json");
            let _missing_config_guard =
                EnvVarGuard::set(BUNDLES_CONFIG_VAR, Some(missing_config.as_os_str()));

            let bundles =
                discover_bundles_auto().expect("missing prebuilt config should fall back");
            let discovered = namespaced_map(&bundles);
            assert_eq!(discovered.get("gt:pythoncore"), Some(&root_bundle));
        });
    }

    #[test]
    fn get_bundles_uses_explicit_config_or_auto_discovery() {
        with_env_lock(|| {
            let temp = tempdir().expect("failed to create temp dir");
            let auto_bundle =
                create_checkout_bundle(temp.path(), "gt", "pythoncore", &["python_env.json"], None);
            let explicit_bundle = create_checkout_bundle(
                &temp.path().join("explicit-root"),
                "tools",
                "render",
                &["render_env.json"],
                None,
            );
            let roots = join_roots(&[temp.path()]);
            let _roots_guard = EnvVarGuard::set(BUNDLE_ROOTS_VAR, Some(roots.as_os_str()));

            let explicit_config = temp.path().join("explicit.json");
            write_json(
                &explicit_config,
                &json!([explicit_bundle.display().to_string()]),
            );

            let explicit =
                get_bundles(Some(&explicit_config)).expect("explicit config should load");
            assert_eq!(
                namespaced_map(&explicit).get("tools:render"),
                Some(&explicit_bundle)
            );

            let auto = get_bundles(None).expect("auto-discovery should succeed");
            assert_eq!(
                namespaced_map(&auto).get("gt:pythoncore"),
                Some(&auto_bundle)
            );
        });
    }

    #[test]
    fn get_bundle_file_helpers_collect_expected_files() {
        let temp = tempdir().expect("failed to create temp dir");
        let bundle_root = create_checkout_bundle(
            temp.path(),
            "gt",
            "pythoncore",
            &["python_env.json", "maya_env.json"],
            Some(json!({"python_dev": {}})),
        );

        let info = BundleInfo::new(
            bundle_root.clone(),
            String::from("pythoncore"),
            String::from("gt"),
        );
        let env_files = get_bundle_env_files(std::slice::from_ref(&info));
        let commands_files = get_bundle_commands_files(std::slice::from_ref(&info));

        let expected_env_files = vec![
            bundle_root.join(BUNDLE_ENV_DIR).join("maya_env.json"),
            bundle_root.join(BUNDLE_ENV_DIR).join("python_env.json"),
        ]
        .into_iter()
        .collect::<HashSet<_>>();

        assert_eq!(
            env_files
                .get("pythoncore")
                .expect("bundle env files should be present")
                .iter()
                .cloned()
                .collect::<HashSet<_>>(),
            expected_env_files
        );
        assert_eq!(
            commands_files.get("pythoncore"),
            Some(&bundle_root.join(BUNDLE_ENV_DIR).join("commands.json"))
        );
    }

    #[test]
    fn bundle_info_display_and_debug_match_python_style() {
        let temp = tempdir().expect("failed to create temp dir");
        let bundle_root = create_checkout_bundle(temp.path(), "gt", "pythoncore", &[], None);
        let info = BundleInfo::new(
            bundle_root.clone(),
            String::from("pythoncore"),
            String::from("gt"),
        );

        assert_eq!(
            format!("{info}"),
            format!("pythoncore ({})", bundle_root.display())
        );
        assert_eq!(
            format!("{info:?}"),
            format!(
                "BundleInfo(bndlid='gt:pythoncore', root={})",
                bundle_root.display()
            )
        );
        assert_eq!(info.index_env_files(), info.env_files().clone());
    }
}
