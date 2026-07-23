//! Application-wrapper orchestration ported from `py/envoy/_wrapper.py`.
//!
//! [`ApplicationWrapper`] ties together environment preparation, command
//! resolution, subprocess spawning, callback invocation, timeout handling, and
//! best-effort termination.
//!
//! # Review notes
//!
//! Two behavior differences are intentional and should be kept in mind during
//! review:
//!
//! 1. Ctrl+C handling is implemented with a single process-global `ctrlc`
//!    handler installed lazily once per process. Python installs and restores
//!    a per-call `SIGINT` handler; Rust's `ctrlc` crate does not support that
//!    model, so each `run()` call instead registers/unregisters its active
//!    child PID in shared state that the global handler consults.
//! 2. Timeout enforcement happens *after*
//!    [`ProcessExecutor::stream_process_output`] returns. That means a child
//!    that keeps its captured pipes open can delay timeout detection. This is
//!    a parity quirk inherited from the Python implementation and is preserved
//!    intentionally.

use std::collections::HashMap;
use std::panic::{self, AssertUnwindSafe};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, ExitStatus, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, OnceLock};
use std::thread;
use std::time::{Duration, Instant};

use crate::environment::EnvironmentManager;
use crate::error::{EnvoyError, Result};
use crate::executor::ProcessExecutor;
use crate::models::{ExecutionResult, OnStartCallback, WrapperConfig};

const LOG_LEVEL_INFO: i32 = 20;
const LOG_LEVEL_WARNING: i32 = 30;
const LOG_LEVEL_ERROR: i32 = 40;

static CTRL_C_STATE: OnceLock<std::result::Result<Arc<CtrlCState>, String>> = OnceLock::new();

#[derive(Default)]
struct CtrlCState {
    active_run: Mutex<Option<ActiveRun>>,
}

struct ActiveRun {
    interrupted: Arc<AtomicBool>,
    child_pid: Option<u32>,
}

struct ActiveRunGuard {
    state: Arc<CtrlCState>,
}

impl ActiveRunGuard {
    fn activate(state: Arc<CtrlCState>, interrupted: Arc<AtomicBool>) -> Result<Self> {
        let mut active_run = state
            .active_run
            .lock()
            .map_err(|_| EnvoyError::Execution(String::from("Failed to lock Ctrl+C run state")))?;
        *active_run = Some(ActiveRun {
            interrupted,
            child_pid: None,
        });
        drop(active_run);

        Ok(Self { state })
    }

    fn set_child_pid(&self, child_pid: u32) {
        if let Ok(mut active_run) = self.state.active_run.lock() {
            if let Some(active_run) = active_run.as_mut() {
                active_run.child_pid = Some(child_pid);
            }
        }
    }
}

impl Drop for ActiveRunGuard {
    fn drop(&mut self) {
        if let Ok(mut active_run) = self.state.active_run.lock() {
            *active_run = None;
        }
    }
}

fn ctrlc_state() -> Result<Arc<CtrlCState>> {
    let state_result = CTRL_C_STATE.get_or_init(|| {
        let state = Arc::new(CtrlCState::default());
        let handler_state = Arc::clone(&state);

        match ctrlc::set_handler(move || handle_ctrl_c(&handler_state)) {
            Ok(()) => Ok(state),
            Err(error) => Err(error.to_string()),
        }
    });

    match state_result {
        Ok(state) => Ok(Arc::clone(state)),
        Err(message) => Err(EnvoyError::Execution(format!(
            "Failed to install Ctrl+C handler: {message}"
        ))),
    }
}

fn handle_ctrl_c(state: &Arc<CtrlCState>) {
    let Ok(mut active_run) = state.active_run.lock() else {
        eprintln!("Ctrl+C handler state lock poisoned");
        return;
    };
    let Some(active_run) = active_run.as_mut() else {
        return;
    };

    active_run.interrupted.store(true, Ordering::SeqCst);

    if let Some(child_pid) = active_run.child_pid {
        terminate_process_by_pid(child_pid);
    }
}

fn terminate_process_by_pid(child_pid: u32) {
    let pid_text = child_pid.to_string();

    #[cfg(target_os = "windows")]
    {
        if let Err(error) = Command::new("taskkill")
            .args(["/PID", &pid_text, "/T", "/F"])
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()
        {
            eprintln!("Error terminating Ctrl+C child process: {error}");
        }
    }

    #[cfg(unix)]
    {
        if let Err(error) = Command::new("kill")
            .args(["-TERM", &pid_text])
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()
        {
            eprintln!("Error terminating Ctrl+C child process: {error}");
        }
    }
}

/// Wrapper orchestrating environment preparation and process execution.
///
/// Unlike Python's class, this type owns its [`WrapperConfig`] outright because
/// the configuration contains boxed callbacks that are not clonable.
///
/// Rust does not have direct equivalents for Python's `__call__`,
/// `__enter__`, or `__exit__`. The port therefore exposes [`run`](Self::run)
/// as the primary execution API and implements [`Drop`] so an in-flight child
/// is terminated if the wrapper is dropped.
pub struct ApplicationWrapper {
    /// Immutable runtime configuration for the wrapper.
    pub config: WrapperConfig,
    env_manager: EnvironmentManager,
    executor: ProcessExecutor,
    process: Option<Child>,
    interrupted: Arc<AtomicBool>,
}

impl ApplicationWrapper {
    /// Construct a wrapper and wire executor callbacks from the config.
    ///
    /// `on_output` and `on_error` are moved into the internal executor because
    /// that is the component responsible for line-by-line output draining.
    pub fn new(mut config: WrapperConfig) -> Self {
        let env_manager = EnvironmentManager::new(config.inherit_env, config.env_allowlist.clone());
        let mut executor = ProcessExecutor::new(config.stream_output);

        if let Some(callback) = config.on_output.take() {
            executor = executor.with_on_output(move |line| callback(line));
        }

        if let Some(callback) = config.on_error.take() {
            executor = executor.with_on_error(move |line| callback(line));
        }

        Self {
            config,
            env_manager,
            executor,
            process: None,
            interrupted: Arc::new(AtomicBool::new(false)),
        }
    }

    fn execute_pre_run(&self) -> Result<()> {
        let Some(callback) = self.config.pre_run.as_ref() else {
            return Ok(());
        };

        self.log_info("Executing pre-run operations...");

        match panic::catch_unwind(AssertUnwindSafe(callback)) {
            Ok(()) => {
                self.log_info("Pre-run operations completed");
                Ok(())
            }
            Err(payload) => {
                let message = panic_payload_message(payload.as_ref());
                self.log_error(&format!("Pre-run operation failed: {message}"));
                if self.config.continue_on_pre_run_error {
                    Ok(())
                } else {
                    Err(EnvoyError::PreRun(format!(
                        "Pre-run operation failed: {message}"
                    )))
                }
            }
        }
    }

    fn execute_post_run(&self, result: &ExecutionResult) -> Result<()> {
        let Some(callback) = self.config.post_run.as_ref() else {
            return Ok(());
        };

        self.log_info("Executing post-run operations...");

        match panic::catch_unwind(AssertUnwindSafe(|| callback(result))) {
            Ok(()) => {
                self.log_info("Post-run operations completed");
                Ok(())
            }
            Err(payload) => {
                let message = panic_payload_message(payload.as_ref());
                self.log_error(&format!("Post-run operation failed: {message}"));
                if self.config.continue_on_post_run_error {
                    Ok(())
                } else {
                    Err(EnvoyError::PostRun(format!(
                        "Post-run operation failed: {message}"
                    )))
                }
            }
        }
    }

    /// Execute the configured application.
    ///
    /// This mirrors Python's `ApplicationWrapper.run()` control flow:
    ///
    /// - pre-run failures map to [`EnvoyError::PreRun`] unless configured to
    ///   continue;
    /// - general execution failures are wrapped as
    ///   [`EnvoyError::Execution`] only when `raise_on_error` is enabled;
    /// - non-zero exit codes and timeouts return `Ok(result)` when
    ///   `raise_on_error` is `false`, and `Err(...)` otherwise;
    /// - post-run always executes and can override an earlier pending error if
    ///   `continue_on_post_run_error` is disabled.
    pub fn run(&mut self) -> Result<ExecutionResult> {
        let start_time = Instant::now();
        let mut result = ExecutionResult::new(-1);
        let mut pending_error = None;

        self.interrupted.store(false, Ordering::SeqCst);

        if let Err(error) = self.execute_pre_run() {
            pending_error = Some(error);
        }

        if pending_error.is_none() {
            if let Err(message) = self.execute_process(&mut result, start_time) {
                result.execution_time = start_time.elapsed().as_secs_f64();
                self.log_error(&format!("Execution failed: {message}"));

                if self.config.raise_on_error {
                    pending_error = Some(EnvoyError::Execution(format!(
                        "Execution failed: {message}"
                    )));
                }
            }
        }

        self.process = None;

        self.execute_post_run(&result)?;

        if let Some(error) = pending_error {
            return Err(error);
        }

        if self.config.raise_on_error && !result.success() {
            if result.timed_out {
                let timeout = format_timeout_value(self.config.timeout.unwrap_or_default());
                return Err(EnvoyError::Execution(format!(
                    "Process timed out after {timeout}s"
                )));
            }

            if result.return_code != 0 {
                return Err(EnvoyError::Execution(format!(
                    "Process exited with code {}\nCommand: {}",
                    result.return_code,
                    result.command.join(" ")
                )));
            }
        }

        Ok(result)
    }

    fn execute_process(
        &mut self,
        result: &mut ExecutionResult,
        start_time: Instant,
    ) -> std::result::Result<(), String> {
        let env_files = self
            .config
            .env_files
            .as_ref()
            .map(|items| items.iter().map(PathBuf::from).collect::<Vec<_>>())
            .unwrap_or_default();
        let env = self
            .env_manager
            .prepare_environment(&env_files, self.config.env.as_ref(), None, None)
            .map_err(|error| error.to_string())?;
        let command = self
            .executor
            .prepare_command(
                Path::new(&self.config.executable),
                &self.config.args,
                env.get("PATH").map(String::as_str),
            )
            .map_err(|error| error.to_string())?;

        result.command = command.clone();

        self.log_info(&format!("Executing: {}", command.join(" ")));
        if let Some(cwd) = self.config.cwd.as_deref() {
            self.log_info(&format!("Working directory: {cwd}"));
        }

        let active_run = ActiveRunGuard::activate(
            ctrlc_state().map_err(|error| error.to_string())?,
            Arc::clone(&self.interrupted),
        )
        .map_err(|error| error.to_string())?;

        let child = self.spawn_child(&command, &env)?;
        self.process = Some(child);

        if let Some(process) = self.process.as_mut() {
            let child_pid = process.id();
            result.pid = Some(i64::from(child_pid));
            active_run.set_child_pid(child_pid);
        }

        if let (Some(callback), Some(pid)) = (self.config.on_start.as_ref(), result.pid) {
            self.invoke_on_start(callback.as_ref(), pid);
        }

        if let Some(pid) = result.pid {
            self.log_info(&format!("Process started with PID: {pid}"));
        }

        if self.config.capture_output || self.config.stream_output {
            let process = self
                .process
                .as_mut()
                .ok_or_else(|| String::from("Process handle missing during output streaming"))?;
            let (stdout, stderr) = self
                .executor
                .stream_process_output(process)
                .map_err(|error| error.to_string())?;

            if !stdout.is_empty() {
                result.stdout = Some(stdout);
            }
            if !stderr.is_empty() {
                result.stderr = Some(stderr);
            }
        }

        let wait_result = {
            let process = self
                .process
                .as_mut()
                .ok_or_else(|| String::from("Process handle missing during wait"))?;

            wait_for_child(process, self.config.timeout)
                .map_err(|error| format!("Failed waiting for process: {error}"))?
        };

        let return_code = match wait_result {
            Some(status) => exit_status_code(status),
            None => {
                let timeout = format_timeout_value(self.config.timeout.unwrap_or_default());
                self.log_error(&format!("Process timed out after {timeout}s"));
                let process = self.process.as_mut().ok_or_else(|| {
                    String::from("Process handle missing during timeout termination")
                })?;
                ProcessExecutor::terminate_process(Some(process));
                result.timed_out = true;
                -1
            }
        };

        result.return_code = return_code;
        result.execution_time = start_time.elapsed().as_secs_f64();

        if self.interrupted.load(Ordering::SeqCst) {
            self.log_warning("Process was interrupted");
            result.return_code = -2;
        }

        self.log_info(&format!("Process finished: {result}"));

        Ok(())
    }

    fn spawn_child(
        &self,
        command: &[String],
        env: &HashMap<String, String>,
    ) -> std::result::Result<Child, String> {
        let mut process_command = build_spawn_command(command, self.config.shell);

        process_command.env_clear().envs(env);

        if let Some(cwd) = self.config.cwd.as_deref() {
            process_command.current_dir(cwd);
        }

        if self.config.capture_output || self.config.stream_output {
            process_command
                .stdout(Stdio::piped())
                .stderr(Stdio::piped());
        }

        process_command
            .spawn()
            .map_err(|error| format!("Failed to spawn process: {error}"))
    }

    fn invoke_on_start(&self, callback: &OnStartCallback, pid: i64) {
        if let Err(payload) = panic::catch_unwind(AssertUnwindSafe(|| callback(pid))) {
            self.log_warning(&format!(
                "onStart callback error: {}",
                panic_payload_message(payload.as_ref())
            ));
        }
    }

    fn log_info(&self, message: &str) {
        self.log_message(LOG_LEVEL_INFO, "INFO", message);
    }

    fn log_warning(&self, message: &str) {
        self.log_message(LOG_LEVEL_WARNING, "WARN", message);
    }

    fn log_error(&self, message: &str) {
        self.log_message(LOG_LEVEL_ERROR, "ERROR", message);
    }

    fn log_message(&self, level: i32, label: &str, message: &str) {
        if !self.config.log_execution || level < self.config.log_level {
            return;
        }

        eprintln!("envoy.wrapper [{label}] {message}");
    }
}

impl Drop for ApplicationWrapper {
    fn drop(&mut self) {
        ProcessExecutor::terminate_process(self.process.as_mut());
    }
}

/// Create a wrapper from a fully constructed [`WrapperConfig`].
///
/// Python's `createWrapper()` spreads positional arguments and `**kwargs`
/// directly into the config object. Rust has no equivalent variadic keyword
/// argument model, so the idiomatic port takes the already-populated config
/// and returns the ready-to-run wrapper.
pub fn create_wrapper(config: WrapperConfig) -> ApplicationWrapper {
    ApplicationWrapper::new(config)
}

fn build_spawn_command(command: &[String], shell: bool) -> Command {
    debug_assert!(!command.is_empty());

    if shell {
        #[cfg(target_os = "windows")]
        {
            let mut process_command = Command::new("cmd");
            process_command
                .arg("/C")
                .arg(windows_shell_command_line(command));
            return process_command;
        }

        #[cfg(not(target_os = "windows"))]
        {
            let mut process_command = Command::new("sh");
            process_command
                .arg("-c")
                .arg(unix_shell_command_line(command));
            return process_command;
        }
    }

    let mut process_command = Command::new(&command[0]);
    process_command.args(command.iter().skip(1));
    process_command
}

#[cfg(target_os = "windows")]
fn windows_shell_command_line(command: &[String]) -> String {
    command
        .iter()
        .map(|item| quote_windows_argument(item))
        .collect::<Vec<_>>()
        .join(" ")
}

#[cfg(target_os = "windows")]
fn quote_windows_argument(argument: &str) -> String {
    if argument.is_empty()
        || argument
            .chars()
            .any(|character| matches!(character, ' ' | '\t' | '"'))
    {
        format!("\"{}\"", argument.replace('"', "\\\""))
    } else {
        argument.to_string()
    }
}

#[cfg(not(target_os = "windows"))]
fn unix_shell_command_line(command: &[String]) -> String {
    command
        .iter()
        .map(|item| quote_unix_argument(item))
        .collect::<Vec<_>>()
        .join(" ")
}

#[cfg(not(target_os = "windows"))]
fn quote_unix_argument(argument: &str) -> String {
    if argument.is_empty()
        || argument.contains(' ')
        || argument.contains('\t')
        || argument.contains('\'')
    {
        format!("'{}'", argument.replace('\'', "'\"'\"'"))
    } else {
        argument.to_string()
    }
}

fn wait_for_child(child: &mut Child, timeout: Option<f64>) -> std::io::Result<Option<ExitStatus>> {
    let Some(timeout) = timeout else {
        return child.wait().map(Some);
    };

    let deadline = Instant::now() + Duration::from_secs_f64(timeout.max(0.0));

    loop {
        if let Some(status) = child.try_wait()? {
            return Ok(Some(status));
        }

        if Instant::now() >= deadline {
            return Ok(None);
        }

        thread::sleep(Duration::from_millis(25));
    }
}

fn exit_status_code(status: ExitStatus) -> i64 {
    status.code().map(i64::from).unwrap_or(-1)
}

fn panic_payload_message(payload: &(dyn std::any::Any + Send)) -> String {
    if let Some(message) = payload.downcast_ref::<&str>() {
        (*message).to_string()
    } else if let Some(message) = payload.downcast_ref::<String>() {
        message.clone()
    } else {
        String::from("panic without string payload")
    }
}

fn format_timeout_value(timeout: f64) -> String {
    if timeout.fract() == 0.0 {
        format!("{timeout:.1}")
    } else {
        timeout.to_string()
    }
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;
    use std::fs;
    use std::path::PathBuf;
    use std::sync::{Arc, Mutex};
    use std::time::{SystemTime, UNIX_EPOCH};

    use super::{create_wrapper, ApplicationWrapper};
    use crate::error::EnvoyError;
    use crate::models::{ExecutionResult, WrapperConfig};

    #[test]
    fn successful_execution_captures_output() {
        let mut config = WrapperConfig::new(command_executable());
        config.args = echo_command_args();
        config.capture_output = true;
        config.stream_output = false;
        config.log_execution = false;

        let mut wrapper = ApplicationWrapper::new(config);
        let result = wrapper.run().expect("wrapper execution should succeed");

        assert_eq!(result.return_code, 0);
        assert!(result.success());
        assert_eq!(result.stdout.as_deref(), Some("hello"));
        assert!(result.pid.is_some());
    }

    #[test]
    fn environment_overrides_and_working_directory_are_applied() {
        let scratch_dir = ScratchDir::new("envoy_wrapper_cwd");
        let mut env_overrides = HashMap::new();
        env_overrides.insert(String::from("TEST_VAR"), String::from("hello_world"));

        let mut config = WrapperConfig::new(command_executable());
        config.args = env_and_cwd_command_args();
        config.env = Some(env_overrides);
        config.cwd = Some(scratch_dir.path.to_string_lossy().into_owned());
        config.capture_output = true;
        config.stream_output = false;
        config.log_execution = false;

        let mut wrapper = ApplicationWrapper::new(config);
        let result = wrapper.run().expect("wrapper execution should succeed");
        let output = result.stdout.expect("stdout should be captured");
        let mut lines = output.lines();

        assert_eq!(lines.next(), Some("hello_world"));
        assert_eq!(
            normalize_test_path(lines.next().expect("cwd line")),
            normalize_test_path(scratch_dir.path.to_string_lossy().as_ref())
        );
    }

    #[test]
    fn non_zero_exit_code_raises_when_configured() {
        let mut config = WrapperConfig::new(command_executable());
        config.args = exit_with_code_command_args(42);
        config.stream_output = false;
        config.raise_on_error = true;
        config.log_execution = false;

        let mut wrapper = ApplicationWrapper::new(config);
        let error = wrapper.run().expect_err("non-zero exit should raise");

        match error {
            EnvoyError::Execution(message) => {
                assert!(message.contains("Process exited with code 42"));
                assert!(message.contains("Command:"));
            }
            other => panic!("unexpected error variant: {other}"),
        }
    }

    #[test]
    fn non_zero_exit_code_returns_result_when_not_raising() {
        let mut config = WrapperConfig::new(command_executable());
        config.args = exit_with_code_command_args(42);
        config.stream_output = false;
        config.raise_on_error = false;
        config.log_execution = false;

        let mut wrapper = ApplicationWrapper::new(config);
        let result = wrapper
            .run()
            .expect("non-raising wrapper should return result");

        assert_eq!(result.return_code, 42);
        assert!(!result.success());
    }

    #[test]
    fn pre_run_failure_blocks_execution_and_still_invokes_post_run() {
        let post_run_result = Arc::new(Mutex::new(None::<ExecutionResult>));
        let post_run_result_ref = Arc::clone(&post_run_result);

        let mut config = WrapperConfig::new(command_executable());
        config.args = echo_command_args();
        config.pre_run = Some(Box::new(|| panic!("boom")));
        config.post_run = Some(Box::new(move |result| {
            *post_run_result_ref
                .lock()
                .expect("post-run mutex should lock") = Some(result.clone());
        }));
        config.capture_output = true;
        config.stream_output = false;
        config.log_execution = false;

        let mut wrapper = ApplicationWrapper::new(config);
        let error = wrapper.run().expect_err("pre-run failure should raise");

        match error {
            EnvoyError::PreRun(message) => {
                assert!(message.contains("Pre-run operation failed: boom"));
            }
            other => panic!("unexpected error variant: {other}"),
        }

        let stored_result = post_run_result
            .lock()
            .expect("post-run result mutex should lock")
            .clone()
            .expect("post-run should receive a result");
        assert_eq!(stored_result.return_code, -1);
        assert!(stored_result.command.is_empty());
    }

    #[test]
    fn pre_run_failure_can_continue_when_configured() {
        let mut config = WrapperConfig::new(command_executable());
        config.args = echo_command_args();
        config.pre_run = Some(Box::new(|| panic!("boom")));
        config.continue_on_pre_run_error = true;
        config.capture_output = true;
        config.stream_output = false;
        config.log_execution = false;

        let mut wrapper = ApplicationWrapper::new(config);
        let result = wrapper.run().expect("wrapper should continue past pre-run");

        assert_eq!(result.return_code, 0);
        assert_eq!(result.stdout.as_deref(), Some("hello"));
    }

    #[test]
    fn post_run_is_invoked_on_failure_path() {
        let post_run_result = Arc::new(Mutex::new(None::<ExecutionResult>));
        let post_run_result_ref = Arc::clone(&post_run_result);

        let mut config = WrapperConfig::new(command_executable());
        config.args = exit_with_code_command_args(7);
        config.raise_on_error = false;
        config.post_run = Some(Box::new(move |result| {
            *post_run_result_ref
                .lock()
                .expect("post-run mutex should lock") = Some(result.clone());
        }));
        config.stream_output = false;
        config.log_execution = false;

        let mut wrapper = ApplicationWrapper::new(config);
        let result = wrapper
            .run()
            .expect("non-raising wrapper should return failure result");

        assert_eq!(result.return_code, 7);
        let stored_result = post_run_result
            .lock()
            .expect("post-run result mutex should lock")
            .clone()
            .expect("post-run should receive a result");
        assert_eq!(stored_result.return_code, 7);
        assert!(!stored_result.success());
    }

    #[test]
    fn timeout_terminates_long_running_process() {
        let mut config = WrapperConfig::new(command_executable());
        config.args = long_running_command_args();
        config.stream_output = false;
        config.timeout = Some(1.0);
        config.raise_on_error = false;
        config.log_execution = false;

        let mut wrapper = ApplicationWrapper::new(config);
        let result = wrapper.run().expect("timeout should return a result");

        assert!(result.timed_out);
        assert!(!result.success());
        assert!(result.execution_time < 5.0);
    }

    #[test]
    fn callbacks_fire_for_start_stdout_and_stderr() {
        let started_pid = Arc::new(Mutex::new(None::<i64>));
        let stdout_lines = Arc::new(Mutex::new(Vec::<String>::new()));
        let stderr_lines = Arc::new(Mutex::new(Vec::<String>::new()));
        let started_pid_ref = Arc::clone(&started_pid);
        let stdout_lines_ref = Arc::clone(&stdout_lines);
        let stderr_lines_ref = Arc::clone(&stderr_lines);

        let mut config = WrapperConfig::new(command_executable());
        config.args = stdout_stderr_command_args();
        config.capture_output = true;
        config.stream_output = false;
        config.on_start = Some(Box::new(move |pid| {
            *started_pid_ref.lock().expect("on-start mutex should lock") = Some(pid);
        }));
        config.on_output = Some(Box::new(move |line| {
            stdout_lines_ref
                .lock()
                .expect("stdout callback mutex should lock")
                .push(line.to_string());
        }));
        config.on_error = Some(Box::new(move |line| {
            stderr_lines_ref
                .lock()
                .expect("stderr callback mutex should lock")
                .push(line.to_string());
        }));
        config.log_execution = false;

        let mut wrapper = ApplicationWrapper::new(config);
        let result = wrapper.run().expect("wrapper execution should succeed");

        assert_eq!(
            *started_pid.lock().expect("on-start mutex should lock"),
            result.pid
        );
        assert_eq!(
            stdout_lines
                .lock()
                .expect("stdout callback mutex should lock")
                .as_slice(),
            ["line1", "line2"]
        );
        assert_eq!(
            stderr_lines
                .lock()
                .expect("stderr callback mutex should lock")
                .as_slice(),
            ["err1"]
        );
    }

    #[test]
    fn create_wrapper_returns_ready_to_run_wrapper() {
        let mut config = WrapperConfig::new(command_executable());
        config.args = echo_command_args();
        config.capture_output = true;
        config.stream_output = false;
        config.log_execution = false;

        let mut wrapper = create_wrapper(config);
        let result = wrapper.run().expect("wrapper execution should succeed");

        assert_eq!(result.stdout.as_deref(), Some("hello"));
    }

    fn normalize_test_path(path: &str) -> String {
        path.replace('/', "\\").trim().to_string()
    }

    #[cfg(target_os = "windows")]
    fn command_executable() -> &'static str {
        "cmd"
    }

    #[cfg(not(target_os = "windows"))]
    fn command_executable() -> &'static str {
        "sh"
    }

    #[cfg(target_os = "windows")]
    fn echo_command_args() -> Vec<String> {
        vec![String::from("/C"), String::from("echo hello")]
    }

    #[cfg(not(target_os = "windows"))]
    fn echo_command_args() -> Vec<String> {
        vec![String::from("-c"), String::from("printf 'hello\\n'")]
    }

    #[cfg(target_os = "windows")]
    fn env_and_cwd_command_args() -> Vec<String> {
        vec![String::from("/C"), String::from("echo %TEST_VAR% & cd")]
    }

    #[cfg(not(target_os = "windows"))]
    fn env_and_cwd_command_args() -> Vec<String> {
        vec![
            String::from("-c"),
            String::from("printf \"$TEST_VAR\\n\"; pwd"),
        ]
    }

    #[cfg(target_os = "windows")]
    fn exit_with_code_command_args(exit_code: i32) -> Vec<String> {
        vec![String::from("/C"), format!("exit {exit_code}")]
    }

    #[cfg(not(target_os = "windows"))]
    fn exit_with_code_command_args(exit_code: i32) -> Vec<String> {
        vec![String::from("-c"), format!("exit {exit_code}")]
    }

    #[cfg(target_os = "windows")]
    fn long_running_command_args() -> Vec<String> {
        vec![
            String::from("/C"),
            String::from("for /L %i in (1,1,100000000) do @rem"),
        ]
    }

    #[cfg(not(target_os = "windows"))]
    fn long_running_command_args() -> Vec<String> {
        vec![String::from("-c"), String::from("sleep 30")]
    }

    #[cfg(target_os = "windows")]
    fn stdout_stderr_command_args() -> Vec<String> {
        vec![
            String::from("/C"),
            String::from("echo line1 & echo line2 & echo err1 1>&2"),
        ]
    }

    #[cfg(not(target_os = "windows"))]
    fn stdout_stderr_command_args() -> Vec<String> {
        vec![
            String::from("-c"),
            String::from("printf \"line1\\nline2\\n\"; printf \"err1\\n\" >&2"),
        ]
    }

    struct ScratchDir {
        path: PathBuf,
    }

    impl ScratchDir {
        fn new(prefix: &str) -> Self {
            let path = unique_test_path(prefix);
            fs::create_dir_all(&path).expect("scratch directory should be created");
            Self { path }
        }
    }

    impl Drop for ScratchDir {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.path);
        }
    }

    fn unique_test_path(prefix: &str) -> PathBuf {
        let timestamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system time should be after Unix epoch")
            .as_nanos();
        let directory_name = format!("{}_{}_{}", prefix, std::process::id(), timestamp);

        std::env::current_dir()
            .expect("current directory should resolve")
            .join(directory_name)
    }
}
