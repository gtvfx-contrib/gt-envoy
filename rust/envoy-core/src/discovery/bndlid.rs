use std::env;
use std::path::{Path, PathBuf};
use std::sync::OnceLock;

use regex::{Captures, Regex};

use crate::error::{EnvoyError, Result};

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

    let roots_str = env::var(super::BUNDLE_ROOTS_VAR).unwrap_or_default();
    if roots_str.is_empty() {
        return Err(EnvoyError::EnvironmentBuild(format!(
            "Cannot resolve bndlid {bndlid:?}: {} is not set",
            super::BUNDLE_ROOTS_VAR
        )));
    }

    let roots = split_root_dirs(&roots_str);

    for root in &roots {
        let candidate =
            super::util::resolve_input_path(&root.join(&namespace).join(&name));
        if candidate.is_dir() && candidate.join(super::BUNDLE_ENV_DIR).is_dir() {
            return Ok(candidate);
        }
    }

    let root_strings = roots
        .iter()
        .map(|root| root.display().to_string())
        .collect::<Vec<_>>();
    let infos = super::discover_bundles_from_roots(&root_strings);
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
        "Bundle {bndlid:?} not found in {} ({searched})",
        super::BUNDLE_ROOTS_VAR
    )))
}

pub fn namespace_regex() -> &'static Regex {
    static REGEX: OnceLock<Regex> = OnceLock::new();

    REGEX.get_or_init(|| {
        Regex::new(r"^[A-Za-z][A-Za-z0-9_]{1,19}$").expect("namespace regex must compile")
    })
}

pub fn bndlid_regex() -> &'static Regex {
    static REGEX: OnceLock<Regex> = OnceLock::new();

    REGEX.get_or_init(|| {
        Regex::new(r"^([A-Za-z][A-Za-z0-9_]{1,19}):([A-Za-z][A-Za-z0-9_-]*)$")
            .expect("bundle-id regex must compile")
    })
}

pub fn bundle_path_var_regex() -> &'static Regex {
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

pub fn parse_bndlid(bndlid: &str) -> Option<(String, String)> {
    let captures = bndlid_regex().captures(bndlid)?;
    let namespace = captures.get(1)?.as_str().to_string();
    let name = captures.get(2)?.as_str().to_string();

    Some((namespace, name))
}

pub fn split_root_dirs(roots_str: &str) -> Vec<PathBuf> {
    roots_str
        .split(super::util::root_separator())
        .map(str::trim)
        .filter(|root| !root.is_empty())
        .map(|root| super::util::resolve_input_path(Path::new(root)))
        .collect()
}
