use std::collections::HashMap;
use std::ffi::OsStr;
use std::fs;
use std::path::PathBuf;

use crate::discovery::types::BundleInfo;

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
