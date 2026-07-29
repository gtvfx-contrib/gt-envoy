//! Named stack registry for `envoy`.
//!
//! Manages versioned bundle-stack files stored in one or more *stack root*
//! directories. Each named stack lives in its own subdirectory and is
//! versioned by timestamp, with a `latest` text file that always points to the
//! most recently published version.
//!
//! Directory layout under a stack root:
//!
//! ```text
//! <stack-root>/
//! └── studio/
//!     ├── 2026-06-21T10-13-00.estack    ← versioned stack file
//!     ├── 2026-06-22T09-00-00.estack    ← newer version
//!     └── latest                      ← plain text: "2026-06-22T09-00-00.estack"
//! ```

use std::collections::HashSet;
use std::ffi::{OsStr, OsString};
use std::fs;
use std::path::{Component, Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use crate::error::{EnvoyError, Result};

/// Environment variable containing the stack root directories.
///
/// The value is semicolon-separated on Windows and colon-separated on Unix.
pub const STACK_ROOTS_VAR: &str = "ENVOY_STACK_ROOTS";

const LATEST_FILE: &str = "latest";
const TIMESTAMP_FMT: &str = "%Y-%m-%dT%H-%M-%S";

/// A single named stack entry discovered from `ENVOY_STACK_ROOTS`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NamedStackEntry {
    /// Named stack identifier.
    pub name: String,
    /// Version string derived from the published filename stem.
    pub version: String,
    /// Resolved path to the latest published stack file.
    pub path: PathBuf,
    /// Stack root directory that provided this entry.
    pub stack_root: PathBuf,
}

fn stack_roots() -> Vec<PathBuf> {
    let roots_str = std::env::var(STACK_ROOTS_VAR).unwrap_or_default();

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
        .map(resolve_stack_root)
        .collect()
}

fn resolve_stack_root(root: &str) -> PathBuf {
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

/// Return `true` if `value` looks like a named stack rather than a path.
///
/// A value is treated as a name when it contains no path separator characters
/// (`/`, `\`, `:`), does not start with a dot, and does not end in `.estack`.
/// Everything else is treated as a filesystem path.
///
/// # Examples
///
/// ```rust
/// # use envoy_core::stack_registry::is_stack_name;
/// assert!(is_stack_name("studio"));
/// assert!(is_stack_name("my-stack"));
/// assert!(!is_stack_name("/path/to/f.estack"));
/// assert!(!is_stack_name("R:/stacks/studio.estack"));
/// assert!(!is_stack_name("./relative.estack"));
/// ```
pub fn is_stack_name(value: &str) -> bool {
    !value.is_empty()
        && !value.starts_with('.')
        && !value.ends_with(".estack")
        && !value.contains('/')
        && !value.contains('\\')
        && !value.contains(':')
}

/// Resolve a named stack to the path of its latest version.
///
/// Searches each directory in `ENVOY_STACK_ROOTS` for a subdirectory named
/// `name` that contains a `latest` pointer file. Returns the first match.
pub fn resolve_named_stack(name: &str) -> Option<PathBuf> {
    for root in stack_roots() {
        let name_dir = root.join(name);
        if !name_dir.is_dir() {
            continue;
        }

        let Some(latest_filename) = read_latest(&name_dir) else {
            continue;
        };

        let stack_path = name_dir.join(latest_filename);
        if stack_path.is_file() && stack_path.extension() == Some(OsStr::new("estack")) {
            return Some(stack_path);
        }
    }

    None
}

/// List all available named stacks across all `ENVOY_STACK_ROOTS` roots.
///
/// Deduplicates by name — the first root that defines a given name wins.
/// Returns entries sorted by name.
pub fn list_named_stacks() -> Vec<NamedStackEntry> {
    let mut seen = HashSet::new();
    let mut entries = Vec::new();

    for root in stack_roots() {
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

            let stack_path = name_dir.join(&latest_filename);
            if !stack_path.is_file() || stack_path.extension() != Some(OsStr::new("estack")) {
                continue;
            }

            let version = latest_filename
                .strip_suffix(".estack")
                .unwrap_or(&latest_filename)
                .to_string();

            entries.push(NamedStackEntry {
                name: name.clone(),
                version,
                path: stack_path,
                stack_root: root.clone(),
            });
            seen.insert(name);
        }
    }

    entries.sort_by(|left, right| left.name.cmp(&right.name));
    entries
}

/// List all published versions of a named stack, newest first.
///
/// Returns an empty list if the name is not found in any root.
pub fn list_stack_versions(name: &str) -> Vec<(String, PathBuf)> {
    for root in stack_roots() {
        let name_dir = root.join(name);
        if !name_dir.is_dir() {
            continue;
        }

        let mut versions = Vec::new();
        for path in sorted_dir_paths(&name_dir) {
            if !path.is_file() {
                continue;
            }

            if path.extension() != Some(OsStr::new("estack")) {
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

/// Publish a new version of a named stack.
///
/// Copies `source_path` into `<stack_root>/<name>/<timestamp>.estack` and updates
/// the `<stack_root>/<name>/latest` pointer file.
///
/// Returns a validation error if `source_path` does not exist or is not a file.
pub fn publish_stack(
    stack_root: &Path,
    name: &str,
    source_path: &Path,
    dry_run: bool,
) -> Result<PathBuf> {
    if !source_path.is_file() {
        return Err(EnvoyError::Validation(format!(
            "Source stack file does not exist: {}",
            source_path.display()
        )));
    }

    let stack = crate::stack::Stack::new(source_path)?;
    if stack.name() != name {
        return Err(EnvoyError::Validation(format!(
            "Stack name {:?} does not match registry slot {name:?}",
            stack.name()
        )));
    }

    let timestamp = current_timestamp();
    let filename = format!("{timestamp}.estack");
    let name_dir = stack_root.join(name);
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
        format_system_time, is_stack_name, list_named_stacks, list_stack_versions, publish_stack,
        read_latest, resolve_named_stack, roots_from_str, EnvoyError, NamedStackEntry, LATEST_FILE,
        STACK_ROOTS_VAR, TIMESTAMP_FMT,
    };

    struct EnvVarGuard {
        previous: Option<OsString>,
    }

    impl EnvVarGuard {
        fn set(value: Option<&OsStr>) -> Self {
            let previous = std::env::var_os(STACK_ROOTS_VAR);

            match value {
                Some(value) => std::env::set_var(STACK_ROOTS_VAR, value),
                None => std::env::remove_var(STACK_ROOTS_VAR),
            }

            Self { previous }
        }
    }

    impl Drop for EnvVarGuard {
        fn drop(&mut self) {
            match &self.previous {
                Some(value) => std::env::set_var(STACK_ROOTS_VAR, value),
                None => std::env::remove_var(STACK_ROOTS_VAR),
            }
        }
    }

    /// Locks the crate-wide `crate::env_test_lock::MUTEX` rather than a
    /// module-local mutex: both `stack_registry` and `discovery` tests
    /// mutate the same real `ENVOY_STACK_ROOTS` process environment variable,
    /// so a single shared lock is required to prevent cross-module test
    /// races under `cargo test`'s default parallel execution.
    fn with_stack_roots_env<T>(value: Option<&OsStr>, test_fn: impl FnOnce() -> T) -> T {
        let _lock = crate::env_test_lock::MUTEX
            .lock()
            .unwrap_or_else(|poison| poison.into_inner());
        let _env_guard = EnvVarGuard::set(value);

        test_fn()
    }

    fn write_named_stack(root: &Path, name: &str, versions: &[(&str, &str)], latest: &str) {
        let name_dir = root.join(name);
        fs::create_dir_all(&name_dir).expect("failed to create named stack directory");

        for (version, contents) in versions {
            fs::write(name_dir.join(format!("{version}.estack")), contents)
                .expect("failed to write versioned stack file");
        }

        fs::write(name_dir.join(LATEST_FILE), latest).expect("failed to write latest file");
    }

    fn join_roots(roots: &[&Path]) -> OsString {
        std::env::join_paths(roots).expect("failed to join stack roots")
    }

    fn write_valid_stack(path: &Path, name: &str) -> String {
        let bundle_path = path
            .parent()
            .expect("stack path should have a parent")
            .join("bundle");
        fs::create_dir_all(bundle_path.join(".envoy")).expect("failed to create bundle fixture");
        let escaped_path = bundle_path.to_string_lossy().replace('\'', "''");
        let contents = format!("name: {name}\nbundles:\n  - path: '{escaped_path}'\n");
        fs::write(path, &contents).expect("failed to write source stack");
        contents
    }

    #[test]
    fn roots_from_str_trims_entries_and_uses_absolute_paths() {
        let temp = tempdir().expect("failed to create temp dir");
        let existing_root = temp.path().join("existing");
        let missing_root = temp.path().join("missing");
        fs::create_dir_all(&existing_root).expect("failed to create existing root");

        // roots_from_str splits on ';' on Windows and ':' on Unix (matching
        // std::env::join_paths / the ENVOY_STACK_ROOTS_VAR convention).
        let separator = if cfg!(windows) { ';' } else { ':' };
        let roots = roots_from_str(&format!(
            "  {}  {separator}  {}  {separator}  ",
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
    fn is_stack_name_matches_python_examples() {
        assert!(is_stack_name("studio"));
        assert!(is_stack_name("my-stack"));
        assert!(!is_stack_name("studio.estack"));
        assert!(!is_stack_name("/path/to/f.estack"));
        assert!(!is_stack_name("R:/stacks/studio.estack"));
        assert!(!is_stack_name("./relative.estack"));
        assert!(!is_stack_name(""));
        assert!(!is_stack_name(".hidden"));
        assert!(!is_stack_name(r"relative\path.estack"));
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
    fn publish_stack_writes_file_and_updates_latest() {
        let temp = tempdir().expect("failed to create temp dir");
        let stack_root = temp.path().join("stack-root");
        let source_path = temp.path().join("source.estack");
        let source_contents = write_valid_stack(&source_path, "studio");

        let published_path =
            publish_stack(&stack_root, "studio", &source_path, false).expect("publish failed");

        assert!(published_path.is_file());
        assert_eq!(
            fs::read_to_string(&published_path).expect("failed to read published Stack"),
            source_contents
        );

        let latest_path = stack_root.join("studio").join(LATEST_FILE);
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
            read_latest(&stack_root.join("studio")),
            Some(expected_filename)
        );
    }

    #[test]
    fn resolve_named_stack_finds_the_latest_published_version() {
        let temp = tempdir().expect("failed to create temp dir");
        let stack_root = temp.path().join("stack-root");
        let source_path = temp.path().join("source.estack");
        write_valid_stack(&source_path, "studio");

        let published_path =
            publish_stack(&stack_root, "studio", &source_path, false).expect("publish failed");
        let roots = join_roots(&[stack_root.as_path()]);
        let expected_path =
            fs::canonicalize(&published_path).expect("failed to canonicalize published path");

        with_stack_roots_env(Some(roots.as_os_str()), || {
            assert_eq!(resolve_named_stack("studio"), Some(expected_path));
        });
    }

    #[test]
    fn list_named_stacks_deduplicates_by_first_root_and_sorts_by_name() {
        let temp = tempdir().expect("failed to create temp dir");
        let root_a = temp.path().join("root-a");
        let root_b = temp.path().join("root-b");
        fs::create_dir_all(&root_a).expect("failed to create root-a");
        fs::create_dir_all(&root_b).expect("failed to create root-b");

        write_named_stack(
            &root_a,
            "beta",
            &[("2026-06-21T10-13-00", "{\"root\":\"a\"}")],
            "2026-06-21T10-13-00.estack",
        );
        write_named_stack(
            &root_b,
            "alpha",
            &[("2026-06-22T09-00-00", "{\"root\":\"b\"}")],
            "2026-06-22T09-00-00.estack",
        );
        write_named_stack(
            &root_b,
            "beta",
            &[("2026-06-23T11-00-00", "{\"root\":\"b\"}")],
            "2026-06-23T11-00-00.estack",
        );

        let roots = join_roots(&[root_a.as_path(), root_b.as_path()]);

        with_stack_roots_env(Some(roots.as_os_str()), || {
            let entries = list_named_stacks();
            let expected_root_a = fs::canonicalize(&root_a).expect("failed to canonicalize root-a");
            let expected_root_b = fs::canonicalize(&root_b).expect("failed to canonicalize root-b");

            assert_eq!(
                entries,
                vec![
                    NamedStackEntry {
                        name: String::from("alpha"),
                        version: String::from("2026-06-22T09-00-00"),
                        path: expected_root_b
                            .join("alpha")
                            .join("2026-06-22T09-00-00.estack"),
                        stack_root: expected_root_b,
                    },
                    NamedStackEntry {
                        name: String::from("beta"),
                        version: String::from("2026-06-21T10-13-00"),
                        path: expected_root_a
                            .join("beta")
                            .join("2026-06-21T10-13-00.estack"),
                        stack_root: expected_root_a,
                    },
                ]
            );
        });
    }

    #[test]
    fn list_stack_versions_returns_newest_first_from_first_matching_root() {
        let temp = tempdir().expect("failed to create temp dir");
        let root_a = temp.path().join("root-a");
        let root_b = temp.path().join("root-b");
        let studio_a = root_a.join("studio");
        let studio_b = root_b.join("studio");
        fs::create_dir_all(&studio_a).expect("failed to create studio dir in root-a");
        fs::create_dir_all(&studio_b).expect("failed to create studio dir in root-b");

        fs::write(studio_a.join("2026-06-21T10-13-00.estack"), "{}")
            .expect("failed to write older Stack");
        fs::write(studio_a.join("2026-06-22T09-00-00.estack"), "{}")
            .expect("failed to write newer Stack");
        fs::write(studio_a.join(LATEST_FILE), "2026-06-22T09-00-00.estack")
            .expect("failed to write latest file");
        fs::write(studio_b.join("2026-06-30T08-00-00.estack"), "{}")
            .expect("failed to write ignored Stack");

        let roots = join_roots(&[root_a.as_path(), root_b.as_path()]);

        with_stack_roots_env(Some(roots.as_os_str()), || {
            let versions = list_stack_versions("studio");
            let expected_root_a = fs::canonicalize(&root_a).expect("failed to canonicalize root-a");

            assert_eq!(
                versions,
                vec![
                    (
                        String::from("2026-06-22T09-00-00"),
                        expected_root_a
                            .join("studio")
                            .join("2026-06-22T09-00-00.estack"),
                    ),
                    (
                        String::from("2026-06-21T10-13-00"),
                        expected_root_a
                            .join("studio")
                            .join("2026-06-21T10-13-00.estack"),
                    ),
                ]
            );
        });
    }

    #[test]
    fn publish_stack_dry_run_returns_destination_without_writing() {
        let temp = tempdir().expect("failed to create temp dir");
        let stack_root = temp.path().join("stack-root");
        let source_path = temp.path().join("source.estack");
        write_valid_stack(&source_path, "studio");

        let dest_path =
            publish_stack(&stack_root, "studio", &source_path, true).expect("dry run failed");
        let name_dir = stack_root.join("studio");

        assert_eq!(dest_path.parent(), Some(name_dir.as_path()));
        assert_eq!(dest_path.extension(), Some(OsStr::new("estack")));
        assert!(!name_dir.exists());
    }

    #[test]
    fn publish_stack_returns_validation_error_for_missing_source_file() {
        let temp = tempdir().expect("failed to create temp dir");
        let missing_path = temp.path().join("missing.estack");

        let error = publish_stack(temp.path(), "studio", &missing_path, false)
            .expect_err("missing source file should fail");

        match error {
            EnvoyError::Validation(message) => {
                assert!(message.contains("Source stack file does not exist"));
                assert!(message.contains("missing.estack"));
            }
            other => panic!("expected validation error, got {other:?}"),
        }
    }
}
