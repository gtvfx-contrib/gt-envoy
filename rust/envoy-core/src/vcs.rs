//! Version control adapters for `envoy-core`.
//!
//! This module provides a small adapter layer over Git, Perforce, and Lore.
//! Detection prefers an explicit `ENVOY_VCS` override when it names a known
//! backend. If that override does not match the current working copy, envoy
//! falls back to automatic detection in Git, Perforce, then Lore order.
//!
//! All adapters shell out to the user-facing CLI for each backend. This keeps
//! the integration light and lets envoy degrade cleanly when a backend is not
//! installed or a workspace is only partially configured. Lore status parsing
//! follows the public CLI docs and example output and should be checked
//! against a live `lore` build if its plain-text format changes before 1.0.

use std::env;
use std::path::{Path, PathBuf};
use std::process::Command;

use thiserror::Error;

use crate::executor::ProcessExecutor;

const ENVOY_VCS_VAR: &str = "ENVOY_VCS";
const P4CLIENT_VAR: &str = "P4CLIENT";
const P4PORT_VAR: &str = "P4PORT";

/// Represents a detected or configured VCS backend kind.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum VcsKind {
    /// Git backend.
    Git,
    /// Perforce backend.
    Perforce,
    /// Lore backend.
    Lore,
}

impl VcsKind {
    /// Return the backend name as a user-facing string.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Git => "git",
            Self::Perforce => "perforce",
            Self::Lore => "lore",
        }
    }

    fn from_override(value: &str) -> Option<Self> {
        if value.eq_ignore_ascii_case("git") {
            return Some(Self::Git);
        }

        if value.eq_ignore_ascii_case("perforce") {
            return Some(Self::Perforce);
        }

        if value.eq_ignore_ascii_case("lore") {
            return Some(Self::Lore);
        }

        None
    }
}

/// A single line-item change entry normalized across backends.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct VcsChange {
    /// Repository-relative path for the changed item.
    pub path: String,
    /// Normalized status such as `modified`, `added`, or `deleted`.
    pub status: String,
}

/// Normalized status snapshot for a working copy.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct VcsStatus {
    /// Detected backend kind.
    pub kind: VcsKind,
    /// Root path used for status queries.
    pub root: PathBuf,
    /// Normalized change entries.
    pub changes: Vec<VcsChange>,
}

/// Error type for VCS adapter operations.
#[derive(Debug, Error)]
pub enum VcsError {
    /// The requested backend CLI was not found on `PATH`.
    #[error("command '{command}' not found on PATH: {source}")]
    CommandNotFound {
        command: String,
        source: std::io::Error,
    },

    /// The backend CLI could not be executed successfully.
    #[error("command '{command}' failed: {details}")]
    CommandExecution { command: String, details: String },

    /// The backend CLI returned output that envoy could not normalize.
    #[error("could not parse output from '{command}': {reason}")]
    UnparseableOutput { command: String, reason: String },

    /// No backend matched the requested starting directory.
    #[error("no VCS backend detected from {start_dir}")]
    NoBackendDetected { start_dir: PathBuf },
}

/// Common trait implemented by each backend adapter.
pub trait VcsAdapter: Send + Sync {
    /// Return this adapter's backend kind.
    fn kind(&self) -> VcsKind;

    /// Return the detected working-copy root.
    fn root(&self) -> &Path;

    /// Return a normalized status snapshot for the working copy.
    fn status(&self) -> Result<VcsStatus, VcsError> {
        let changes = self.get_changes()?;
        Ok(VcsStatus {
            kind: self.kind(),
            root: self.root().to_path_buf(),
            changes,
        })
    }

    /// Return normalized change entries for the working copy.
    fn get_changes(&self) -> Result<Vec<VcsChange>, VcsError>;
}

/// Git adapter backed by the `git` CLI.
pub struct GitAdapter {
    root: PathBuf,
}

/// Perforce adapter backed by the `p4` CLI.
pub struct PerforceAdapter {
    root: PathBuf,
}

/// Lore adapter backed by the `lore` CLI.
pub struct LoreAdapter {
    root: PathBuf,
}

impl GitAdapter {
    fn from_start_dir(start_dir: &Path) -> Option<Self> {
        let root = find_git_root(start_dir)?;
        let resolved_root = resolve_git_root(&root).unwrap_or(root);
        Some(Self {
            root: resolved_root,
        })
    }
}

impl VcsAdapter for GitAdapter {
    fn kind(&self) -> VcsKind {
        VcsKind::Git
    }

    fn root(&self) -> &Path {
        &self.root
    }

    fn get_changes(&self) -> Result<Vec<VcsChange>, VcsError> {
        let output = run_command(&self.root, "git", &["status", "--porcelain"])?;
        parse_git_status_output(&output)
    }
}

impl PerforceAdapter {
    fn from_start_dir(start_dir: &Path) -> Option<Self> {
        let has_env = env::var_os(P4CLIENT_VAR).is_some() || env::var_os(P4PORT_VAR).is_some();

        if has_env {
            let root =
                perforce_root_from_info(start_dir).unwrap_or_else(|| start_dir.to_path_buf());
            return Some(Self { root });
        }

        if run_command(start_dir, "p4", &["info"]).is_ok() {
            let root =
                perforce_root_from_info(start_dir).unwrap_or_else(|| start_dir.to_path_buf());
            return Some(Self { root });
        }

        None
    }
}

impl VcsAdapter for PerforceAdapter {
    fn kind(&self) -> VcsKind {
        VcsKind::Perforce
    }

    fn root(&self) -> &Path {
        &self.root
    }

    fn get_changes(&self) -> Result<Vec<VcsChange>, VcsError> {
        match run_command(&self.root, "p4", &["status"]) {
            Ok(output) => parse_perforce_output(&output),
            Err(_) => {
                let output = run_command(&self.root, "p4", &["opened"])?;
                parse_perforce_output(&output)
            }
        }
    }
}

impl LoreAdapter {
    fn from_start_dir(start_dir: &Path) -> Option<Self> {
        if let Some(root) = find_lore_root(start_dir) {
            return Some(Self { root });
        }

        if run_command(start_dir, "lore", &["status", "--revision-only"]).is_ok() {
            return Some(Self {
                root: start_dir.to_path_buf(),
            });
        }

        None
    }
}

impl VcsAdapter for LoreAdapter {
    fn kind(&self) -> VcsKind {
        VcsKind::Lore
    }

    fn root(&self) -> &Path {
        &self.root
    }

    fn get_changes(&self) -> Result<Vec<VcsChange>, VcsError> {
        let output = run_command(&self.root, "lore", &["status"])?;
        parse_lore_status_output(&output)
    }
}

/// Detect a backend from `start_dir`.
///
/// Detection first honors `ENVOY_VCS` when it names one of the supported
/// backends. If that preferred backend does not match the current directory,
/// detection falls back to automatic probing in Git, Perforce, then Lore
/// order. `None` is returned when no backend matches.
pub fn detect(start_dir: &Path) -> Option<Box<dyn VcsAdapter>> {
    let normalized_start = normalize_start_dir(start_dir);
    let mut order = Vec::new();

    if let Ok(value) = env::var(ENVOY_VCS_VAR) {
        if let Some(kind) = VcsKind::from_override(&value) {
            order.push(kind);
        }
    }

    for kind in [VcsKind::Git, VcsKind::Perforce, VcsKind::Lore] {
        if !order.contains(&kind) {
            order.push(kind);
        }
    }

    for kind in order {
        let adapter: Option<Box<dyn VcsAdapter>> = match kind {
            VcsKind::Git => GitAdapter::from_start_dir(&normalized_start)
                .map(|adapter| Box::new(adapter) as Box<dyn VcsAdapter>),
            VcsKind::Perforce => PerforceAdapter::from_start_dir(&normalized_start)
                .map(|adapter| Box::new(adapter) as Box<dyn VcsAdapter>),
            VcsKind::Lore => LoreAdapter::from_start_dir(&normalized_start)
                .map(|adapter| Box::new(adapter) as Box<dyn VcsAdapter>),
        };

        if adapter.is_some() {
            return adapter;
        }
    }

    None
}

/// Detect a backend from `start_dir` and return a typed error on failure.
pub fn detect_or_error(start_dir: &Path) -> Result<Box<dyn VcsAdapter>, VcsError> {
    detect(start_dir).ok_or_else(|| VcsError::NoBackendDetected {
        start_dir: start_dir.to_path_buf(),
    })
}

fn normalize_start_dir(start_dir: &Path) -> PathBuf {
    if start_dir.is_file() {
        return start_dir.parent().unwrap_or(start_dir).to_path_buf();
    }

    start_dir.to_path_buf()
}

fn find_git_root(start_dir: &Path) -> Option<PathBuf> {
    find_parent_with(start_dir, |path| path.join(".git").is_dir())
}

fn resolve_git_root(start_dir: &Path) -> Result<PathBuf, VcsError> {
    let output = run_command(start_dir, "git", &["rev-parse", "--show-toplevel"])?;
    let root = output.trim();

    if root.is_empty() {
        return Err(VcsError::UnparseableOutput {
            command: "git rev-parse --show-toplevel".to_string(),
            reason: "expected a repository root path".to_string(),
        });
    }

    Ok(PathBuf::from(root))
}

fn find_lore_root(start_dir: &Path) -> Option<PathBuf> {
    find_parent_with(start_dir, |path| {
        path.join(".lore").is_dir() || path.join(".lore").join("config.toml").is_file()
    })
}

fn perforce_root_from_info(start_dir: &Path) -> Option<PathBuf> {
    let output = run_command(start_dir, "p4", &["info"]).ok()?;

    for line in output.lines() {
        let trimmed = line.trim();
        let lowered = trimmed.to_ascii_lowercase();

        if lowered.starts_with("client root:") {
            let value = trimmed["Client root:".len()..].trim();
            if !value.is_empty() {
                return Some(PathBuf::from(value));
            }
        }

        if lowered.starts_with("root:") {
            let value = trimmed["Root:".len()..].trim();
            if !value.is_empty() {
                return Some(PathBuf::from(value));
            }
        }
    }

    None
}

fn find_parent_with(start_dir: &Path, predicate: impl Fn(&Path) -> bool) -> Option<PathBuf> {
    let mut current = Some(normalize_start_dir(start_dir));

    while let Some(path) = current {
        if predicate(&path) {
            return Some(path);
        }

        current = path.parent().map(Path::to_path_buf);
    }

    None
}

fn run_command(working_dir: &Path, command: &str, args: &[&str]) -> Result<String, VcsError> {
    let executable =
        ProcessExecutor::resolve_executable(Path::new(command), None).map_err(|error| {
            VcsError::CommandNotFound {
                command: command.to_string(),
                source: std::io::Error::new(std::io::ErrorKind::NotFound, error.to_string()),
            }
        })?;

    let output = Command::new(&executable)
        .current_dir(working_dir)
        .args(args)
        .output()
        .map_err(|error| VcsError::CommandExecution {
            command: format_command(command, args),
            details: error.to_string(),
        })?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
        let stdout = String::from_utf8_lossy(&output.stdout).trim().to_string();
        let details = if !stderr.is_empty() {
            stderr
        } else if !stdout.is_empty() {
            stdout
        } else {
            format!("exit code {:?}", output.status.code())
        };

        return Err(VcsError::CommandExecution {
            command: format_command(command, args),
            details,
        });
    }

    Ok(String::from_utf8_lossy(&output.stdout).into_owned())
}

fn format_command(command: &str, args: &[&str]) -> String {
    if args.is_empty() {
        return command.to_string();
    }

    format!("{command} {}", args.join(" "))
}

fn parse_git_status_output(output: &str) -> Result<Vec<VcsChange>, VcsError> {
    let mut changes = Vec::new();

    for line in output.lines() {
        if line.trim().is_empty() {
            continue;
        }

        if line.len() < 4 {
            return Err(VcsError::UnparseableOutput {
                command: "git status --porcelain".to_string(),
                reason: format!("short status line: {line}"),
            });
        }

        let code = &line[..2];
        let raw_path = line[3..].trim();
        if raw_path.is_empty() {
            return Err(VcsError::UnparseableOutput {
                command: "git status --porcelain".to_string(),
                reason: format!("missing path in line: {line}"),
            });
        }

        let path = raw_path
            .rsplit_once(" -> ")
            .map(|(_, dest)| dest)
            .unwrap_or(raw_path);

        changes.push(VcsChange {
            path: path.to_string(),
            status: normalize_git_status(code)?,
        });
    }

    Ok(changes)
}

fn normalize_git_status(code: &str) -> Result<String, VcsError> {
    if code == "??" {
        return Ok("untracked".to_string());
    }

    if code == "!!" {
        return Ok("ignored".to_string());
    }

    let status = if code.contains('U') {
        "conflicted"
    } else if code.contains('R') {
        "renamed"
    } else if code.contains('C') {
        "copied"
    } else if code.contains('A') {
        "added"
    } else if code.contains('D') {
        "deleted"
    } else if code.contains('M') {
        "modified"
    } else {
        return Err(VcsError::UnparseableOutput {
            command: "git status --porcelain".to_string(),
            reason: format!("unknown status code: {code}"),
        });
    };

    Ok(status.to_string())
}

fn parse_perforce_output(output: &str) -> Result<Vec<VcsChange>, VcsError> {
    let mut changes = Vec::new();

    for line in output.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }

        let (status_token, path) = if let Some((path, details)) = trimmed.split_once(" - ") {
            let token = details.split_whitespace().next().unwrap_or_default();
            (token, path.trim())
        } else if let Some((token, path)) = split_token_and_path(trimmed) {
            (token, path)
        } else {
            return Err(VcsError::UnparseableOutput {
                command: "p4 status/opened".to_string(),
                reason: format!("could not parse line: {trimmed}"),
            });
        };

        changes.push(VcsChange {
            path: path.to_string(),
            status: normalize_perforce_status(status_token)?,
        });
    }

    Ok(changes)
}

fn split_token_and_path(line: &str) -> Option<(&str, &str)> {
    let mut parts = line.splitn(2, char::is_whitespace);
    let token = parts.next()?.trim();
    let path = parts.next()?.trim();

    if token.is_empty() || path.is_empty() {
        return None;
    }

    Some((token, path))
}

fn normalize_perforce_status(token: &str) -> Result<String, VcsError> {
    let normalized = token.to_ascii_lowercase();

    let status = match normalized.as_str() {
        "a" | "add" | "move/add" | "branch" => "added",
        "m" | "edit" | "integrate" | "reopen" => "modified",
        "d" | "delete" | "move/delete" => "deleted",
        "r" | "rename" => "renamed",
        "?" | "??" => "untracked",
        other => {
            return Err(VcsError::UnparseableOutput {
                command: "p4 status/opened".to_string(),
                reason: format!("unknown status token: {other}"),
            });
        }
    };

    Ok(status.to_string())
}

fn parse_lore_status_output(output: &str) -> Result<Vec<VcsChange>, VcsError> {
    let mut changes = Vec::new();

    for line in output.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() || is_lore_header_line(trimmed) {
            continue;
        }

        let Some((token, path)) = split_token_and_path(trimmed) else {
            return Err(VcsError::UnparseableOutput {
                command: "lore status".to_string(),
                reason: format!("could not parse line: {trimmed}"),
            });
        };

        changes.push(VcsChange {
            path: path.to_string(),
            status: normalize_lore_status(token)?,
        });
    }

    Ok(changes)
}

fn is_lore_header_line(line: &str) -> bool {
    line.starts_with("Repository ")
        || line.starts_with("On branch ")
        || line.starts_with("Remote revision ")
        || line.starts_with("Local branch ")
        || line.starts_with("Changes staged for commit:")
        || line.starts_with("No changes")
}

fn normalize_lore_status(token: &str) -> Result<String, VcsError> {
    let normalized = token.to_ascii_lowercase();

    let status = match normalized.as_str() {
        "a" | "add" | "added" => "added",
        "m" | "edit" | "modified" | "dirty" => "modified",
        "d" | "delete" | "deleted" => "deleted",
        "r" | "rename" | "renamed" => "renamed",
        "c" | "copy" | "copied" => "copied",
        "?" | "??" => "untracked",
        other => {
            return Err(VcsError::UnparseableOutput {
                command: "lore status".to_string(),
                reason: format!("unknown status token: {other}"),
            });
        }
    };

    Ok(status.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::tempdir;

    #[test]
    fn detect_vcs_finds_git_root_from_nested_dir() {
        let _guard = crate::env_test_lock::MUTEX
            .lock()
            .unwrap_or_else(|poison| poison.into_inner());

        let temp = tempdir().expect("tempdir should be created");
        let repo_root = temp.path().join("repo");
        let nested = repo_root.join("src").join("nested");
        fs::create_dir_all(repo_root.join(".git")).expect("git marker should be created");
        fs::create_dir_all(&nested).expect("nested directory should be created");

        let old_vcs = env::var_os(ENVOY_VCS_VAR);
        let old_client = env::var_os(P4CLIENT_VAR);
        let old_port = env::var_os(P4PORT_VAR);
        env::remove_var(ENVOY_VCS_VAR);
        env::remove_var(P4CLIENT_VAR);
        env::remove_var(P4PORT_VAR);

        let adapter = detect(&nested).expect("git adapter should be detected");

        assert_eq!(adapter.kind(), VcsKind::Git);
        assert_eq!(adapter.root(), repo_root.as_path());

        restore_env_var(ENVOY_VCS_VAR, old_vcs);
        restore_env_var(P4CLIENT_VAR, old_client);
        restore_env_var(P4PORT_VAR, old_port);
    }

    #[test]
    fn parse_git_status_output_normalizes_status_codes() {
        let changes = parse_git_status_output(
            " M src/lib.rs\nA  src/new.rs\nD  src/old.rs\nR  src/from.rs -> src/to.rs\n?? notes.txt\n",
        )
        .expect("git status output should parse");

        assert_eq!(
            changes,
            vec![
                VcsChange {
                    path: "src/lib.rs".to_string(),
                    status: "modified".to_string(),
                },
                VcsChange {
                    path: "src/new.rs".to_string(),
                    status: "added".to_string(),
                },
                VcsChange {
                    path: "src/old.rs".to_string(),
                    status: "deleted".to_string(),
                },
                VcsChange {
                    path: "src/to.rs".to_string(),
                    status: "renamed".to_string(),
                },
                VcsChange {
                    path: "notes.txt".to_string(),
                    status: "untracked".to_string(),
                },
            ]
        );
    }

    #[test]
    fn parse_perforce_output_normalizes_status_codes() {
        let changes = parse_perforce_output(
            "edit C:\\repo\\src\\main.rs\nadd C:\\repo\\src\\new.rs\ndelete C:\\repo\\src\\old.rs\n",
        )
        .expect("perforce output should parse");

        assert_eq!(
            changes,
            vec![
                VcsChange {
                    path: "C:\\repo\\src\\main.rs".to_string(),
                    status: "modified".to_string(),
                },
                VcsChange {
                    path: "C:\\repo\\src\\new.rs".to_string(),
                    status: "added".to_string(),
                },
                VcsChange {
                    path: "C:\\repo\\src\\old.rs".to_string(),
                    status: "deleted".to_string(),
                },
            ]
        );
    }

    #[test]
    fn parse_lore_status_output_normalizes_status_codes() {
        let changes = parse_lore_status_output(
            "Repository 3f2a1b4c5d6e7f8a\nOn branch main revision 0 -> 0000000000000000\n\
Remote revision 0 -> 0000000000000000\nLocal branch in sync with remote\n\
Changes staged for commit:\nA hello.txt\nM src\\main.rs\nD old.txt\n",
        )
        .expect("lore status output should parse");

        assert_eq!(
            changes,
            vec![
                VcsChange {
                    path: "hello.txt".to_string(),
                    status: "added".to_string(),
                },
                VcsChange {
                    path: "src\\main.rs".to_string(),
                    status: "modified".to_string(),
                },
                VcsChange {
                    path: "old.txt".to_string(),
                    status: "deleted".to_string(),
                },
            ]
        );
    }

    #[test]
    fn detect_vcs_honors_override_before_auto_detection() {
        let _guard = crate::env_test_lock::MUTEX
            .lock()
            .unwrap_or_else(|poison| poison.into_inner());

        let temp = tempdir().expect("tempdir should be created");
        let repo_root = temp.path().join("repo");
        fs::create_dir_all(repo_root.join(".git")).expect("git marker should be created");

        let old_vcs = env::var_os(ENVOY_VCS_VAR);
        let old_client = env::var_os(P4CLIENT_VAR);

        env::set_var(ENVOY_VCS_VAR, "perforce");
        env::set_var(P4CLIENT_VAR, "test-client");

        let adapter = detect(&repo_root).expect("override should produce an adapter");
        assert_eq!(adapter.kind(), VcsKind::Perforce);

        restore_env_var(ENVOY_VCS_VAR, old_vcs);
        restore_env_var(P4CLIENT_VAR, old_client);
    }

    fn restore_env_var(name: &str, value: Option<std::ffi::OsString>) {
        if let Some(value) = value {
            env::set_var(name, value);
        } else {
            env::remove_var(name);
        }
    }
}
