use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::Duration;

use fs4::FileExt;
use serde::{Deserialize, Serialize};

use crate::discovery::types::BundleInfo;
use crate::discovery::util::{
    current_timestamp, env_flag_enabled, metadata_modified_timestamp, resolve_input_path,
};

pub(crate) const DISCOVERY_CACHE_DISABLE_VAR: &str = "ENVOY_DISABLE_DISCOVERY_CACHE";
const DISCOVERY_CACHE_FILENAME: &str = "discovery_cache.json";
const DISCOVERY_CACHE_FINGERPRINT_DEPTH: usize = 2;
const DISCOVERY_CACHE_MAX_AGE: Duration = Duration::from_secs(5);

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub(crate) struct DiscoveryCacheManifest {
    pub entries: HashMap<String, DiscoveryCacheEntry>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub(crate) struct DiscoveryCacheEntry {
    pub created_at: u64,
    pub roots: Vec<CachedRootFingerprint>,
    pub bundles: Vec<CachedBundleInfo>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub(crate) struct CachedRootFingerprint {
    root: PathBuf,
    directories: Vec<CachedDirectoryFingerprint>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub(crate) struct CachedDirectoryFingerprint {
    relative_path: PathBuf,
    modified_at: Option<u64>,
    bundle_marker_modified_at: Option<u64>,
    has_envoy_env: bool,
    has_git_dir: bool,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub(crate) struct CachedBundleInfo {
    root: PathBuf,
    name: String,
    namespace: String,
}

pub fn load_cached_discovery_results(
    root_dirs: &[PathBuf],
    max_depth: usize,
) -> Option<Vec<BundleInfo>> {
    if root_dirs.is_empty() || env_flag_enabled(DISCOVERY_CACHE_DISABLE_VAR) {
        return None;
    }

    let _cache_lock = lock_discovery_cache()?;
    let cache_key = discovery_cache_key(root_dirs, max_depth);
    let mut manifest = load_discovery_cache_manifest();
    let cache_entry = manifest.entries.remove(&cache_key)?;

    let age = current_timestamp().saturating_sub(cache_entry.created_at);
    if age > DISCOVERY_CACHE_MAX_AGE.as_secs() {
        return None;
    }

    let expected_fingerprints = root_dirs
        .iter()
        .filter_map(|root| fingerprint_root(root, max_depth))
        .collect::<Vec<_>>();

    if expected_fingerprints.len() != root_dirs.len() || cache_entry.roots != expected_fingerprints
    {
        return None;
    }

    Some(
        cache_entry
            .bundles
            .into_iter()
            .map(|cached| BundleInfo::new(cached.root, cached.name, cached.namespace))
            .collect(),
    )
}

pub fn store_cached_discovery_results(
    root_dirs: &[PathBuf],
    bundles: &[BundleInfo],
    max_depth: usize,
) {
    if root_dirs.is_empty() || env_flag_enabled(DISCOVERY_CACHE_DISABLE_VAR) {
        return;
    }

    let fingerprints = root_dirs
        .iter()
        .filter_map(|root| fingerprint_root(root, max_depth))
        .collect::<Vec<_>>();

    if fingerprints.len() != root_dirs.len() {
        return;
    }

    let Some(_cache_lock) = lock_discovery_cache() else {
        return;
    };
    let mut manifest = load_discovery_cache_manifest();
    manifest.entries.insert(
        discovery_cache_key(root_dirs, max_depth),
        DiscoveryCacheEntry {
            created_at: current_timestamp(),
            roots: fingerprints,
            bundles: bundles
                .iter()
                .map(|bundle| CachedBundleInfo {
                    root: bundle.root.clone(),
                    name: bundle.name.clone(),
                    namespace: bundle.namespace.clone(),
                })
                .collect(),
        },
    );

    save_discovery_cache_manifest(&manifest);
}

fn lock_discovery_cache() -> Option<fs::File> {
    let lock_path = discovery_cache_lock_path();
    let parent = lock_path.parent()?;
    fs::create_dir_all(parent).ok()?;

    let lock_file = fs::OpenOptions::new()
        .create(true)
        .truncate(false)
        .read(true)
        .write(true)
        .open(lock_path)
        .ok()?;
    FileExt::lock(&lock_file).ok()?;

    Some(lock_file)
}

pub fn discovery_cache_key(root_dirs: &[PathBuf], max_depth: usize) -> String {
    let roots = root_dirs
        .iter()
        .map(|root| root.to_string_lossy().into_owned())
        .collect::<Vec<_>>();

    format!(
        "roots:{}|depth:{max_depth}",
        serde_json::to_string(&roots).unwrap_or_default()
    )
}

pub(crate) fn load_discovery_cache_manifest() -> DiscoveryCacheManifest {
    let cache_path = discovery_cache_path();
    let Ok(contents) = fs::read_to_string(&cache_path) else {
        return DiscoveryCacheManifest::default();
    };

    serde_json::from_str(&contents).unwrap_or_default()
}

pub(crate) fn save_discovery_cache_manifest(manifest: &DiscoveryCacheManifest) {
    let cache_path = discovery_cache_path();
    let Some(parent) = cache_path.parent() else {
        return;
    };
    if fs::create_dir_all(parent).is_err() {
        return;
    }

    let Ok(contents) = serde_json::to_string_pretty(manifest) else {
        return;
    };

    let _ = fs::write(cache_path, contents);
}

pub fn discovery_cache_path() -> PathBuf {
    let bundle_cache_root = crate::bundle_cache::default_cache_root();
    let envoy_cache_root = bundle_cache_root
        .parent()
        .map(Path::to_path_buf)
        .unwrap_or(bundle_cache_root);

    envoy_cache_root.join(DISCOVERY_CACHE_FILENAME)
}

pub fn discovery_cache_lock_path() -> PathBuf {
    discovery_cache_path().with_extension("lock")
}

fn fingerprint_root(root: &Path, max_depth: usize) -> Option<CachedRootFingerprint> {
    if !root.is_dir() {
        return None;
    }

    let max_fingerprint_depth = max_depth.min(DISCOVERY_CACHE_FINGERPRINT_DEPTH);
    Some(CachedRootFingerprint {
        root: resolve_input_path(root),
        directories: fingerprint_directory(root, root, 0, max_fingerprint_depth),
    })
}

fn fingerprint_directory(
    root: &Path,
    path: &Path,
    depth: usize,
    max_depth: usize,
) -> Vec<CachedDirectoryFingerprint> {
    let mut directories = vec![CachedDirectoryFingerprint {
        relative_path: path
            .strip_prefix(root)
            .map(Path::to_path_buf)
            .unwrap_or_default(),
        modified_at: metadata_modified_timestamp(path),
        bundle_marker_modified_at: metadata_modified_timestamp(
            &path.join(super::BUNDLE_MARKER_FILE),
        ),
        has_envoy_env: path.join(super::BUNDLE_ENV_DIR).is_dir(),
        has_git_dir: path.join(".git").is_dir(),
    }];

    if depth >= max_depth {
        return directories;
    }

    let Ok(read_dir) = fs::read_dir(path) else {
        return directories;
    };

    let mut child_dirs = read_dir
        .flatten()
        .filter_map(|entry| {
            let child_path = entry.path();
            let is_dir = entry
                .file_type()
                .map(|file_type| file_type.is_dir())
                .unwrap_or(false);
            if !is_dir {
                return None;
            }

            if child_path
                .file_name()
                .map(|name| name.to_string_lossy().starts_with('.'))
                .unwrap_or(false)
            {
                return None;
            }

            Some(child_path)
        })
        .collect::<Vec<_>>();
    child_dirs.sort();

    for child_dir in child_dirs {
        directories.extend(fingerprint_directory(
            root,
            &child_dir,
            depth + 1,
            max_depth,
        ));
    }

    directories
}
