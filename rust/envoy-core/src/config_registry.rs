//! Named config registry for `envoy`.
//!
//! Manages versioned bundle-config files stored in one or more *config root*
//! directories. Each named config lives in its own subdirectory and is
//! versioned by timestamp, with a `latest` text file that always points to the
//! most recently published version.
//!
//! Directory layout under a config root:
//!
//! ```text
//! <cfg-root>/
//! └── studio/
//!     ├── 2026-06-21T10-13-00.json    ← versioned config file
//!     ├── 2026-06-22T09-00-00.json    ← newer version
//!     └── latest                      ← plain text: "2026-06-22T09-00-00.json"
//! ```

use std::collections::HashSet;
use std::ffi::{OsStr, OsString};
use std::fs;
use std::path::{Component, Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use crate::error::{EnvoyError, Result};

/// Environment variable containing the config root directories.
///
/// The value is semicolon-separated on Windows and colon-separated on Unix.
pub const CFG_ROOTS_VAR: &str = "ENVOY_CFG_ROOTS";

const LATEST_FILE: &str = "latest";
const TIMESTAMP_FMT: &str = "%Y-%m-%dT%H-%M-%S";

/// A single named config entry discovered from `ENVOY_CFG_ROOTS`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NamedConfigEntry {
    /// Named config identifier.
    pub name: String,
    /// Version string derived from the published filename stem.
    pub version: String,
    /// Resolved path to the latest published config file.
    pub path: PathBuf,
    /// Config root directory that provided this entry.
    pub cfg_root: PathBuf,
}

fn cfg_roots() -> Vec<PathBuf> {
    let roots_str = std::env::var(CFG_ROOTS_VAR).unwrap_or_default();

    roots_from_str(roots_str.trim())
}

fn roots_from_str(roots_str: &str) -> Vec<PathBuf> {
    if roots_str.is_empty() {
        return Vec::new();
    }

    let separator = if cfg!(windows) { ';' } else { ':' };

    roots_str
        .split(separator)
        .map(str::trim)
        .filter(|root| !root.is_empty())
        .map(resolve_cfg_root)
        .collect()
}

fn resolve_cfg_root(root: &str) -> PathBuf {
    let path = Path::new(root);
    let absolute_path = if path.is_absolute() {
        path.to_path_buf()
    } else {
        std::env::current_dir()
            .map(|cwd| cwd.join(path))
            .unwrap_or_else(|_| path.to_path_buf())
    };

    fs::canonicalize(&absolute_path).unwrap_or_else(|_| lexical_normalize(&absolute_path))
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

fn read_latest(name_dir: &Path) -> Option<String> {
    let latest_file = name_dir.join(LATEST_FILE);

    if !latest_file.is_file() {
        return None;
    }

    fs::read_to_string(&latest_file)
        .ok()
        .map(|contents| contents.trim().to_string())
        .filter(|contents| !contents.is_empty())
}

fn write_latest(name_dir: &Path, filename: &str) -> Result<()> {
    let latest_path = name_dir.join(LATEST_FILE);

    fs::write(&latest_path, filename).map_err(|source| EnvoyError::Io {
        path: latest_path,
        source,
    })
}

fn sorted_dir_paths(dir: &Path) -> Vec<PathBuf> {
    let Ok(read_dir) = fs::read_dir(dir) else {
        return Vec::new();
    };

    let mut paths: Vec<PathBuf> = read_dir
        .filter_map(|entry| entry.ok().map(|entry| entry.path()))
        .collect();

    paths.sort_by_key(|path| path_name(path));
    paths
}

fn path_name(path: &Path) -> OsString {
    path.file_name()
        .map_or_else(OsString::new, OsStr::to_os_string)
}

fn current_timestamp() -> String {
    format_system_time(SystemTime::now(), TIMESTAMP_FMT)
}

fn format_system_time(time: SystemTime, format: &str) -> String {
    debug_assert_eq!(format, TIMESTAMP_FMT);

    let duration = time.duration_since(UNIX_EPOCH).unwrap_or_default();
    let total_seconds = duration.as_secs();
    let days = i64::try_from(total_seconds / 86_400).unwrap_or(i64::MAX);
    let seconds_of_day = total_seconds % 86_400;
    let hour = seconds_of_day / 3_600;
    let minute = (seconds_of_day % 3_600) / 60;
    let second = seconds_of_day % 60;
    let (year, month, day) = civil_from_days(days);

    format!("{year:04}-{month:02}-{day:02}T{hour:02}-{minute:02}-{second:02}")
}

fn civil_from_days(days_since_epoch: i64) -> (i32, u32, u32) {
    let z = days_since_epoch + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let day_of_era = z - (era * 146_097);
    let year_of_era =
        (day_of_era - (day_of_era / 1_460) + (day_of_era / 36_524) - (day_of_era / 146_096)) / 365;
    let year = year_of_era + (era * 400);
    let day_of_year = day_of_era - ((365 * year_of_era) + (year_of_era / 4) - (year_of_era / 100));
    let month_prime = ((5 * day_of_year) + 2) / 153;
    let day = day_of_year - (((153 * month_prime) + 2) / 5) + 1;
    let month = month_prime + if month_prime < 10 { 3 } else { -9 };
    let year = year + if month <= 2 { 1 } else { 0 };

    (year as i32, month as u32, day as u32)
}

/// Return `true` if `value` looks like a named config rather than a path.
///
/// A value is treated as a name when it contains no path separator characters
/// (`/`, `\`, `:`) and does not start with a dot. Everything else is treated
/// as a filesystem path.
///
/// # Examples
///
/// ```rust
/// # use envoy_core::config_registry::is_config_name;
/// assert!(is_config_name("studio"));
/// assert!(is_config_name("my-config"));
/// assert!(!is_config_name("/path/to/f.json"));
/// assert!(!is_config_name("R:/configs.json"));
/// assert!(!is_config_name("./relative.json"));
/// ```
pub fn is_config_name(value: &str) -> bool {
    !value.is_empty()
        && !value.starts_with('.')
        && !value.contains('/')
        && !value.contains('\\')
        && !value.contains(':')
}

/// Resolve a named config to the path of its latest version.
///
/// Searches each directory in `ENVOY_CFG_ROOTS` for a subdirectory named
/// `name` that contains a `latest` pointer file. Returns the first match.
pub fn resolve_named_config(name: &str) -> Option<PathBuf> {
    for root in cfg_roots() {
        let name_dir = root.join(name);
        if !name_dir.is_dir() {
            continue;
        }

        let Some(latest_filename) = read_latest(&name_dir) else {
            continue;
        };

        let config_path = name_dir.join(latest_filename);
        if config_path.is_file() {
            return Some(config_path);
        }
    }

    None
}

/// List all available named configs across all `ENVOY_CFG_ROOTS` roots.
///
/// Deduplicates by name — the first root that defines a given name wins.
/// Returns entries sorted by name.
pub fn list_named_configs() -> Vec<NamedConfigEntry> {
    let mut seen = HashSet::new();
    let mut entries = Vec::new();

    for root in cfg_roots() {
        if !root.is_dir() {
            continue;
        }

        for name_dir in sorted_dir_paths(&root) {
            if !name_dir.is_dir() {
                continue;
            }

            let Some(name_os) = name_dir.file_name() else {
                continue;
            };
            let name = name_os.to_string_lossy().into_owned();

            if seen.contains(&name) {
                continue;
            }

            let Some(latest_filename) = read_latest(&name_dir) else {
                continue;
            };

            let config_path = name_dir.join(&latest_filename);
            if !config_path.is_file() {
                continue;
            }

            let version = latest_filename
                .strip_suffix(".json")
                .unwrap_or(&latest_filename)
                .to_string();

            entries.push(NamedConfigEntry {
                name: name.clone(),
                version,
                path: config_path,
                cfg_root: root.clone(),
            });
            seen.insert(name);
        }
    }

    entries.sort_by(|left, right| left.name.cmp(&right.name));
    entries
}

/// List all published versions of a named config, newest first.
///
/// Returns an empty list if the name is not found in any root.
pub fn list_config_versions(name: &str) -> Vec<(String, PathBuf)> {
    for root in cfg_roots() {
        let name_dir = root.join(name);
        if !name_dir.is_dir() {
            continue;
        }

        let mut versions = Vec::new();
        for path in sorted_dir_paths(&name_dir) {
            if !path.is_file() {
                continue;
            }

            if path.extension() != Some(OsStr::new("json")) {
                continue;
            }

            let Some(stem) = path.file_stem() else {
                continue;
            };

            versions.push((stem.to_string_lossy().into_owned(), path));
        }

        versions.sort_by(|left, right| right.0.cmp(&left.0));
        return versions;
    }

    Vec::new()
}

/// Publish a new version of a named config.
///
/// Copies `source_path` into `<cfg_root>/<name>/<timestamp>.json` and updates
/// the `<cfg_root>/<name>/latest` pointer file.
///
/// Returns a validation error if `source_path` does not exist or is not a file.
pub fn publish_config(
    cfg_root: &Path,
    name: &str,
    source_path: &Path,
    dry_run: bool,
) -> Result<PathBuf> {
    if !source_path.is_file() {
        return Err(EnvoyError::Validation(format!(
            "Source config file does not exist: {}",
            source_path.display()
        )));
    }

    let timestamp = current_timestamp();
    let filename = format!("{timestamp}.json");
    let name_dir = cfg_root.join(name);
    let dest_path = name_dir.join(&filename);

    if dry_run {
        println!("Would publish: {}", source_path.display());
        println!("          to: {}", dest_path.display());
        println!(
            "     (latest: {} → {filename})",
            name_dir.join(LATEST_FILE).display()
        );

        return Ok(dest_path);
    }

    fs::create_dir_all(&name_dir).map_err(|source| EnvoyError::Io {
        path: name_dir.clone(),
        source,
    })?;

    let source_contents = fs::read_to_string(source_path).map_err(|source| EnvoyError::Io {
        path: source_path.to_path_buf(),
        source,
    })?;

    fs::write(&dest_path, source_contents).map_err(|source| EnvoyError::Io {
        path: dest_path.clone(),
        source,
    })?;

    write_latest(&name_dir, &filename)?;

    Ok(dest_path)
}

#[cfg(test)]
mod tests {
    use std::ffi::{OsStr, OsString};
    use std::fs;
    use std::path::Path;
    use std::time::{Duration, UNIX_EPOCH};

    use tempfile::tempdir;

    use super::{
        format_system_time, is_config_name, list_config_versions, list_named_configs,
        publish_config, read_latest, resolve_named_config, roots_from_str, EnvoyError,
        NamedConfigEntry, CFG_ROOTS_VAR, LATEST_FILE, TIMESTAMP_FMT,
    };

    struct EnvVarGuard {
        previous: Option<OsString>,
    }

    impl EnvVarGuard {
        fn set(value: Option<&OsStr>) -> Self {
            let previous = std::env::var_os(CFG_ROOTS_VAR);

            match value {
                Some(value) => std::env::set_var(CFG_ROOTS_VAR, value),
                None => std::env::remove_var(CFG_ROOTS_VAR),
            }

            Self { previous }
        }
    }

    impl Drop for EnvVarGuard {
        fn drop(&mut self) {
            match &self.previous {
                Some(value) => std::env::set_var(CFG_ROOTS_VAR, value),
                None => std::env::remove_var(CFG_ROOTS_VAR),
            }
        }
    }

    /// Locks the crate-wide `crate::env_test_lock::MUTEX` rather than a
    /// module-local mutex: both `config_registry` and `discovery` tests
    /// mutate the same real `ENVOY_CFG_ROOTS` process environment variable,
    /// so a single shared lock is required to prevent cross-module test
    /// races under `cargo test`'s default parallel execution.
    fn with_cfg_roots_env<T>(value: Option<&OsStr>, test_fn: impl FnOnce() -> T) -> T {
        let _lock = crate::env_test_lock::MUTEX
            .lock()
            .unwrap_or_else(|poison| poison.into_inner());
        let _env_guard = EnvVarGuard::set(value);

        test_fn()
    }

    fn write_named_config(root: &Path, name: &str, versions: &[(&str, &str)], latest: &str) {
        let name_dir = root.join(name);
        fs::create_dir_all(&name_dir).expect("failed to create named config directory");

        for (version, contents) in versions {
            fs::write(name_dir.join(format!("{version}.json")), contents)
                .expect("failed to write versioned config file");
        }

        fs::write(name_dir.join(LATEST_FILE), latest).expect("failed to write latest file");
    }

    fn join_roots(roots: &[&Path]) -> OsString {
        std::env::join_paths(roots).expect("failed to join config roots")
    }

    #[test]
    fn roots_from_str_trims_entries_and_uses_absolute_paths() {
        let temp = tempdir().expect("failed to create temp dir");
        let existing_root = temp.path().join("existing");
        let missing_root = temp.path().join("missing");
        fs::create_dir_all(&existing_root).expect("failed to create existing root");

        let roots = roots_from_str(&format!(
            "  {}  ;  {}  ;  ",
            existing_root.display(),
            missing_root.display()
        ));

        assert_eq!(roots.len(), 2);
        assert_eq!(
            roots[0],
            fs::canonicalize(&existing_root).expect("failed to canonicalize existing root")
        );
        assert_eq!(roots[1], missing_root);
    }

    #[test]
    fn is_config_name_matches_python_examples() {
        assert!(is_config_name("studio"));
        assert!(is_config_name("my-config"));
        assert!(!is_config_name("/path/to/f.json"));
        assert!(!is_config_name("R:/configs.json"));
        assert!(!is_config_name("./relative.json"));
        assert!(!is_config_name(""));
        assert!(!is_config_name(".hidden"));
        assert!(!is_config_name(r"relative\path.json"));
    }

    #[test]
    fn format_system_time_matches_expected_utc_layout() {
        assert_eq!(
            format_system_time(UNIX_EPOCH, TIMESTAMP_FMT),
            "1970-01-01T00-00-00"
        );
        assert_eq!(
            format_system_time(UNIX_EPOCH + Duration::from_secs(86_401), TIMESTAMP_FMT),
            "1970-01-02T00-00-01"
        );
    }

    #[test]
    fn publish_config_writes_file_and_updates_latest() {
        let temp = tempdir().expect("failed to create temp dir");
        let cfg_root = temp.path().join("cfg-root");
        let source_path = temp.path().join("source.json");
        let source_contents = "{\"name\":\"studio\"}\n";
        fs::write(&source_path, source_contents).expect("failed to write source config");

        let published_path =
            publish_config(&cfg_root, "studio", &source_path, false).expect("publish failed");

        assert!(published_path.is_file());
        assert_eq!(
            fs::read_to_string(&published_path).expect("failed to read published config"),
            source_contents
        );

        let latest_path = cfg_root.join("studio").join(LATEST_FILE);
        let latest_filename = fs::read_to_string(&latest_path)
            .expect("failed to read latest pointer")
            .trim()
            .to_string();
        let expected_filename = published_path
            .file_name()
            .expect("published path missing file name")
            .to_string_lossy()
            .into_owned();

        assert_eq!(latest_filename, expected_filename);
        assert_eq!(
            read_latest(&cfg_root.join("studio")),
            Some(expected_filename)
        );
    }

    #[test]
    fn resolve_named_config_finds_the_latest_published_version() {
        let temp = tempdir().expect("failed to create temp dir");
        let cfg_root = temp.path().join("cfg-root");
        let source_path = temp.path().join("source.json");
        fs::write(&source_path, "{\"name\":\"studio\"}").expect("failed to write source config");

        let published_path =
            publish_config(&cfg_root, "studio", &source_path, false).expect("publish failed");
        let roots = join_roots(&[cfg_root.as_path()]);
        let expected_path =
            fs::canonicalize(&published_path).expect("failed to canonicalize published path");

        with_cfg_roots_env(Some(roots.as_os_str()), || {
            assert_eq!(resolve_named_config("studio"), Some(expected_path));
        });
    }

    #[test]
    fn list_named_configs_deduplicates_by_first_root_and_sorts_by_name() {
        let temp = tempdir().expect("failed to create temp dir");
        let root_a = temp.path().join("root-a");
        let root_b = temp.path().join("root-b");
        fs::create_dir_all(&root_a).expect("failed to create root-a");
        fs::create_dir_all(&root_b).expect("failed to create root-b");

        write_named_config(
            &root_a,
            "beta",
            &[("2026-06-21T10-13-00", "{\"root\":\"a\"}")],
            "2026-06-21T10-13-00.json",
        );
        write_named_config(
            &root_b,
            "alpha",
            &[("2026-06-22T09-00-00", "{\"root\":\"b\"}")],
            "2026-06-22T09-00-00.json",
        );
        write_named_config(
            &root_b,
            "beta",
            &[("2026-06-23T11-00-00", "{\"root\":\"b\"}")],
            "2026-06-23T11-00-00.json",
        );

        let roots = join_roots(&[root_a.as_path(), root_b.as_path()]);

        with_cfg_roots_env(Some(roots.as_os_str()), || {
            let entries = list_named_configs();
            let expected_root_a = fs::canonicalize(&root_a).expect("failed to canonicalize root-a");
            let expected_root_b = fs::canonicalize(&root_b).expect("failed to canonicalize root-b");

            assert_eq!(
                entries,
                vec![
                    NamedConfigEntry {
                        name: String::from("alpha"),
                        version: String::from("2026-06-22T09-00-00"),
                        path: expected_root_b
                            .join("alpha")
                            .join("2026-06-22T09-00-00.json"),
                        cfg_root: expected_root_b,
                    },
                    NamedConfigEntry {
                        name: String::from("beta"),
                        version: String::from("2026-06-21T10-13-00"),
                        path: expected_root_a
                            .join("beta")
                            .join("2026-06-21T10-13-00.json"),
                        cfg_root: expected_root_a,
                    },
                ]
            );
        });
    }

    #[test]
    fn list_config_versions_returns_newest_first_from_first_matching_root() {
        let temp = tempdir().expect("failed to create temp dir");
        let root_a = temp.path().join("root-a");
        let root_b = temp.path().join("root-b");
        let studio_a = root_a.join("studio");
        let studio_b = root_b.join("studio");
        fs::create_dir_all(&studio_a).expect("failed to create studio dir in root-a");
        fs::create_dir_all(&studio_b).expect("failed to create studio dir in root-b");

        fs::write(studio_a.join("2026-06-21T10-13-00.json"), "{}")
            .expect("failed to write older config");
        fs::write(studio_a.join("2026-06-22T09-00-00.json"), "{}")
            .expect("failed to write newer config");
        fs::write(studio_a.join(LATEST_FILE), "2026-06-22T09-00-00.json")
            .expect("failed to write latest file");
        fs::write(studio_b.join("2026-06-30T08-00-00.json"), "{}")
            .expect("failed to write ignored config");

        let roots = join_roots(&[root_a.as_path(), root_b.as_path()]);

        with_cfg_roots_env(Some(roots.as_os_str()), || {
            let versions = list_config_versions("studio");
            let expected_root_a = fs::canonicalize(&root_a).expect("failed to canonicalize root-a");

            assert_eq!(
                versions,
                vec![
                    (
                        String::from("2026-06-22T09-00-00"),
                        expected_root_a
                            .join("studio")
                            .join("2026-06-22T09-00-00.json"),
                    ),
                    (
                        String::from("2026-06-21T10-13-00"),
                        expected_root_a
                            .join("studio")
                            .join("2026-06-21T10-13-00.json"),
                    ),
                ]
            );
        });
    }

    #[test]
    fn publish_config_dry_run_returns_destination_without_writing() {
        let temp = tempdir().expect("failed to create temp dir");
        let cfg_root = temp.path().join("cfg-root");
        let source_path = temp.path().join("source.json");
        fs::write(&source_path, "{\"name\":\"studio\"}").expect("failed to write source config");

        let dest_path =
            publish_config(&cfg_root, "studio", &source_path, true).expect("dry run failed");
        let name_dir = cfg_root.join("studio");

        assert_eq!(dest_path.parent(), Some(name_dir.as_path()));
        assert_eq!(dest_path.extension(), Some(OsStr::new("json")));
        assert!(!name_dir.exists());
    }

    #[test]
    fn publish_config_returns_validation_error_for_missing_source_file() {
        let temp = tempdir().expect("failed to create temp dir");
        let missing_path = temp.path().join("missing.json");

        let error = publish_config(temp.path(), "studio", &missing_path, false)
            .expect_err("missing source file should fail");

        match error {
            EnvoyError::Validation(message) => {
                assert!(message.contains("Source config file does not exist"));
                assert!(message.contains("missing.json"));
            }
            other => panic!("expected validation error, got {other:?}"),
        }
    }
}
