//! Build script: derive a git-tag-based version string for `envoy-cli`.
//!
//! Mirrors the spirit of `py/envoy`'s `hatch-vcs`-derived versioning
//! (`_version.py`, written at install/build time from the nearest git tag)
//! without requiring a registry-publishable static `Cargo.toml` version.
//! `Cargo.toml`'s `[workspace.package] version` stays a fixed placeholder
//! (`0.0.0`) since Cargo requires a static, valid semver string there; the
//! *reported* version (via `envoy --version`) instead comes from this
//! build-time-computed `ENVOY_VERSION` environment variable, consumed in
//! `src/args.rs` via `#[command(version = env!("ENVOY_VERSION"))]`.
//!
//! Resolution order:
//! 1. `git describe --tags --always --dirty` from the repository root, if
//!    this is a git checkout with `git` available (matches what a
//!    contributor or CI checkout normally has).
//! 2. Falls back to `CARGO_PKG_VERSION` (the static `Cargo.toml` version)
//!    when git is unavailable or this isn't a git checkout (e.g. a
//!    source-only tarball) -- never fails the build.

#[cfg(windows)]
use std::path::PathBuf;
use std::process::Command;

fn main() {
    embed_windows_icon();

    let version = git_describe().unwrap_or_else(|| env!("CARGO_PKG_VERSION").to_string());
    println!("cargo:rustc-env=ENVOY_VERSION={version}");

    // Re-run if the git ref changes (new commit/tag), so `--version` output
    // stays current across incremental builds.
    println!("cargo:rerun-if-changed=../.git/HEAD");
    println!("cargo:rerun-if-changed=../.git/refs");
}

#[cfg(windows)]
fn embed_windows_icon() {
    let icon_path =
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../resources/icons/envoy.ico");
    println!("cargo:rerun-if-changed={}", icon_path.display());

    let icon_path = icon_path.to_string_lossy();
    let mut windows_resource = winres::WindowsResource::new();
    windows_resource.set_icon(&icon_path);
    windows_resource
        .compile()
        .expect("failed to embed the Envoy Windows executable icon");
}

#[cfg(not(windows))]
fn embed_windows_icon() {}

/// Run `git describe --tags --always --dirty` from the repo root and return
/// its trimmed stdout, or `None` if `git` isn't available or the command
/// fails (e.g. not a git checkout, no tags reachable, shallow clone without
/// tag history).
fn git_describe() -> Option<String> {
    let output = Command::new("git")
        .args(["describe", "--tags", "--always", "--dirty"])
        // CARGO_MANIFEST_DIR is this crate's directory (rust/envoy-cli);
        // the repository root is two levels up (rust/envoy-cli -> rust ->
        // repo root), which is where the .git directory lives.
        .current_dir(concat!(env!("CARGO_MANIFEST_DIR"), "/../.."))
        .output()
        .ok()?;

    if !output.status.success() {
        return None;
    }

    let version = String::from_utf8(output.stdout).ok()?.trim().to_string();
    if version.is_empty() {
        None
    } else {
        Some(version)
    }
}
