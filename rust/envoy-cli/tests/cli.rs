use std::env;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use assert_cmd::Command;

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
