use std::env;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use assert_cmd::Command;
use envoy_core::package_cache::PackageCache;

fn stdout_text(assert: &assert_cmd::assert::Assert) -> String {
    String::from_utf8_lossy(&assert.get_output().stdout).into_owned()
}

fn stderr_text(assert: &assert_cmd::assert::Assert) -> String {
    String::from_utf8_lossy(&assert.get_output().stderr).into_owned()
}

struct ScratchDir {
    path: PathBuf,
}

impl ScratchDir {
    fn new(prefix: &str) -> Self {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system time should be after epoch")
            .as_nanos();
        let path = env::temp_dir().join(format!("{prefix}_{unique}_{}", std::process::id()));
        fs::create_dir_all(&path).expect("scratch dir should be created");
        Self { path }
    }

    fn path(&self) -> &Path {
        &self.path
    }
}

impl Drop for ScratchDir {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.path);
    }
}

fn base_command() -> Command {
    let mut command = Command::cargo_bin("envoy").expect("envoy binary should build");
    command
        .env_remove("ENVOY_BNDL_ROOTS")
        .env_remove("ENVOY_BUNDLES_CONFIG")
        .env_remove("ENVOY_COMMANDS_FILE")
        .env_remove("ENVOY_ALLOWLIST");
    command
}

#[test]
fn help_lists_expected_flags() {
    let assert = base_command().arg("--help").assert().success();
    let stdout = stdout_text(&assert);

    for expected in [
        "--list",
        "--info <COMMAND>",
        "--which <COMMAND>",
        "--commands-file <PATH>",
        "--bundles-config <PATH>",
        "--set-config <KEY=VALUE>",
        "--get-config [<KEY>]",
        "--list-configs",
        "--ignore-config",
        "--env <ENV_COMMAND>",
        "--trace <VAR>",
        "-cf",
        "-bc",
    ] {
        assert!(
            stdout.contains(expected),
            "expected help to mention {expected}, got:\n{stdout}"
        );
    }
}

#[test]
fn version_prints_a_version_string() {
    let assert = base_command().arg("--version").assert().success();
    let stdout = stdout_text(&assert);

    assert!(stdout.starts_with("envoy "), "stdout was:\n{stdout}");
}

#[test]
fn list_configs_runs_without_error() {
    let scratch = ScratchDir::new("envoy_list_configs");
    let config_path = scratch.path().join("user_config.json");

    base_command()
        .arg("--list-configs")
        .env("ENVOY_USER_CONFIG", &config_path)
        .assert()
        .success();
}

#[test]
fn set_config_and_get_config_round_trip() {
    let scratch = ScratchDir::new("envoy_user_config");
    let config_path = scratch.path().join("user_config.json");

    let set_assert = base_command()
        .args(["--set-config", "verbosity=verbose"])
        .env("ENVOY_USER_CONFIG", &config_path)
        .assert()
        .success();
    let set_stdout = stdout_text(&set_assert);
    assert!(set_stdout.contains("Saved: verbosity = \"verbose\""));

    let get_assert = base_command()
        .args(["--get-config", "verbosity"])
        .env("ENVOY_USER_CONFIG", &config_path)
        .assert()
        .success();
    let get_stdout = stdout_text(&get_assert);
    assert!(get_stdout.contains("verbosity = \"verbose\""));
}

#[test]
fn raw_absolute_path_executable_runs_successfully() {
    let comspec =
        env::var("ComSpec").unwrap_or_else(|_| String::from(r"C:\Windows\System32\cmd.exe"));

    base_command()
        .args([comspec.as_str(), "/c", "exit", "0"])
        .assert()
        .success();
}

#[test]
fn unregistered_command_name_returns_not_found() {
    let scratch = ScratchDir::new("envoy_missing_command");
    let envoy_dir = scratch.path().join(".envoy");
    fs::create_dir_all(&envoy_dir).expect(".envoy dir should be created");
    fs::write(
        envoy_dir.join("commands.json"),
        r#"{
  "known": {
    "environment": []
  }
}"#,
    )
    .expect("commands.json should be written");

    let assert = base_command()
        .arg("missing_command")
        .current_dir(scratch.path())
        .assert()
        .failure();
    let stderr = stderr_text(&assert);

    assert!(
        stderr.contains("Error: Command 'missing_command' not found"),
        "stderr was:\n{stderr}"
    );
}

/// End-to-end coverage for the Phase 1-3 wiring gaps closed in this pass:
/// a *published* bundle whose `.envoy/team.json` / `.envoy/pipeline.json` /
/// `.envoy/commands.json` come from a warm package-cache entry, all
/// resolved automatically through the real `envoy` binary -- not just via
/// direct, isolated unit calls to each module.
#[test]
fn verbose_run_resolves_team_config_pipeline_and_warm_package_cache_together() {
    let scratch = ScratchDir::new("envoy_full_wiring");

    // A published bundle (has a `.bundle` marker) discovered via
    // ENVOY_BNDL_ROOTS. Its own commands.json defines a command that should
    // NOT end up loaded, because a cached snapshot takes precedence.
    let bundle_root = scratch.path().join("gt").join("maya");
    let envoy_dir = bundle_root.join(".envoy");
    fs::create_dir_all(&envoy_dir).expect(".envoy dir should be created");
    fs::write(bundle_root.join(".bundle"), r#"{"version": "2.0.0"}"#)
        .expect(".bundle marker should be written");
    fs::write(
        envoy_dir.join("commands.json"),
        r#"{"from_checkout_root": {"environment": []}}"#,
    )
    .expect("original commands.json should be written");

    // The cached snapshot: a *different* commands.json (so we can prove it
    // -- not the original bundle root -- is what actually gets loaded), plus
    // the team.json / pipeline.json that should be auto-resolved.
    let cached_source = scratch.path().join("cached_source");
    let cached_envoy_dir = cached_source.join(".envoy");
    fs::create_dir_all(&cached_envoy_dir).expect("cached .envoy dir should be created");
    fs::write(
        cached_envoy_dir.join("commands.json"),
        r#"{"known_from_cache": {"environment": []}}"#,
    )
    .expect("cached commands.json should be written");
    fs::write(cached_envoy_dir.join("team.json"), r#"{"name": "bfd"}"#)
        .expect("cached team.json should be written");
    fs::write(
        cached_envoy_dir.join("pipeline.json"),
        r#"{"name": "build", "namespace": "bfd"}"#,
    )
    .expect("cached pipeline.json should be written");

    let cache_root = scratch.path().join("package_cache");
    let mut cache = PackageCache::new(&cache_root).expect("package cache should open");
    cache
        .store("gt:maya", "1.0.0", &cached_source)
        .expect("storing the cached snapshot should succeed");

    let assert = base_command()
        .args(["--verbose", "--list"])
        .env("ENVOY_BNDL_ROOTS", scratch.path())
        .env("ENVOY_PACKAGE_CACHE", &cache_root)
        .assert()
        .success();

    let stdout = stdout_text(&assert);
    let stderr = stderr_text(&assert);

    assert!(
        stdout.contains("known_from_cache"),
        "expected the warm cache's commands to be loaded, stdout was:\n{stdout}"
    );
    assert!(
        !stdout.contains("from_checkout_root"),
        "the original bundle's commands should be shadowed once the cache is \
warm, stdout was:\n{stdout}"
    );
    assert!(
        stderr.contains("debug: Resolved team config: bfd"),
        "stderr was:\n{stderr}"
    );
    assert!(
        stderr.contains("debug: Resolved pipeline: bfd:build"),
        "stderr was:\n{stderr}"
    );
}

/// A developer's own checkout must never be silently swapped for a cached
/// snapshot, even when a package-cache entry exists under the same bndlid.
#[test]
fn verbose_run_never_substitutes_a_checkout_bundle_for_a_cache_entry() {
    let scratch = ScratchDir::new("envoy_checkout_not_cached");

    // A checkout bundle: `.git` marker only, no `.bundle` marker.
    let bundle_root = scratch.path().join("gt").join("maya");
    let envoy_dir = bundle_root.join(".envoy");
    fs::create_dir_all(bundle_root.join(".git")).expect(".git dir should be created");
    fs::create_dir_all(&envoy_dir).expect(".envoy dir should be created");
    fs::write(
        envoy_dir.join("commands.json"),
        r#"{"from_checkout_root": {"environment": []}}"#,
    )
    .expect("original commands.json should be written");

    // A cache entry under the same bndlid that must be ignored.
    let cached_source = scratch.path().join("cached_source");
    let cached_envoy_dir = cached_source.join(".envoy");
    fs::create_dir_all(&cached_envoy_dir).expect("cached .envoy dir should be created");
    fs::write(
        cached_envoy_dir.join("commands.json"),
        r#"{"known_from_cache": {"environment": []}}"#,
    )
    .expect("cached commands.json should be written");

    let cache_root = scratch.path().join("package_cache");
    let mut cache = PackageCache::new(&cache_root).expect("package cache should open");
    cache
        .store("gt:maya", "1.0.0", &cached_source)
        .expect("storing the cache entry should succeed");

    let assert = base_command()
        .args(["--verbose", "--list"])
        .env("ENVOY_BNDL_ROOTS", scratch.path())
        .env("ENVOY_PACKAGE_CACHE", &cache_root)
        .assert()
        .success();

    let stdout = stdout_text(&assert);

    assert!(
        stdout.contains("from_checkout_root"),
        "the developer's own checkout should still be loaded, stdout was:\n{stdout}"
    );
    assert!(
        !stdout.contains("known_from_cache"),
        "a checkout bundle must never be substituted for a cached snapshot, \
stdout was:\n{stdout}"
    );
}
