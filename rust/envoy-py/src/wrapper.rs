#![allow(clippy::too_many_arguments)]

//! PyO3 bindings for `ApplicationWrapper`, `WrapperConfig`,
//! `ExecutionResult`, and `createWrapper`.
//!
//! The public Python API accepts real Python callables for
//! `preRun`/`postRun`/`onStart`/`onOutput`/`onError`, while
//! `envoy_core::models::WrapperConfig` stores Rust closures. The binding keeps
//! the Python-facing config separate and only translates scalar fields into a
//! core config. Python callbacks are retained as `Py<PyAny>` values and are
//! invoked from Rust by reacquiring the GIL on demand.
//!
//! # Ctrl+C design in the PyO3 path
//!
//! `envoy_core::wrapper::ApplicationWrapper` installs a process-global
//! `ctrlc` handler, which is appropriate for the native CLI. The Python
//! binding intentionally does **not** reuse that handler because CPython owns
//! SIGINT handling for extension modules. Instead, `run()` releases the GIL
//! around the blocking subprocess work and periodically calls
//! `Python::check_signals()` from Rust. When Python reports a pending signal
//! exception (typically `KeyboardInterrupt` from the default SIGINT handler),
//! the binding terminates the child process, marks the run as interrupted, and
//! finishes with the same `return_code == -2` / `ExecutionError` behavior as
//! the original Python wrapper.
//!
//! This keeps Ctrl+C ownership on the Python side and avoids competing signal
//! registrations inside the embedding Python process.

use std::collections::{HashMap, HashSet};
use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, ExitStatus, Stdio};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

use crate::exceptions::envoy_error_to_pyerr;
use envoy_core::environment::EnvironmentManager;
use envoy_core::error::EnvoyError;
use envoy_core::executor::ProcessExecutor;
use envoy_core::models::{
    ExecutionResult as CoreExecutionResult, WrapperConfig as CoreWrapperConfig, LOG_LEVEL_INFO,
};
use pyo3::exceptions::PyTypeError;
use pyo3::prelude::*;
use pyo3::types::{PyAny, PyModule, PyTuple};

const LOG_LEVEL_WARNING: i32 = 30;
const LOG_LEVEL_ERROR: i32 = 40;

#[pyclass(module = "envoy")]
#[derive(Clone)]
pub struct ExecutionResult {
    inner: CoreExecutionResult,
}

#[pymethods]
impl ExecutionResult {
    #[getter]
    fn return_code(&self) -> i64 {
        self.inner.return_code
    }

    #[getter]
    fn stdout(&self) -> Option<String> {
        self.inner.stdout.clone()
    }

    #[getter]
    fn stderr(&self) -> Option<String> {
        self.inner.stderr.clone()
    }

    #[getter]
    fn execution_time(&self) -> f64 {
        self.inner.execution_time
    }

    #[getter]
    fn pid(&self) -> Option<i64> {
        self.inner.pid
    }

    #[getter]
    fn command(&self) -> Vec<String> {
        self.inner.command.clone()
    }

    #[getter]
    fn timed_out(&self) -> bool {
        self.inner.timed_out
    }

    #[getter]
    fn success(&self) -> bool {
        self.inner.success()
    }

    fn __repr__(&self) -> String {
        self.inner.to_string()
    }

    fn __str__(&self) -> String {
        self.__repr__()
    }
}

impl ExecutionResult {
    fn from_inner(inner: CoreExecutionResult) -> Self {
        Self { inner }
    }
}

#[pyclass(module = "envoy")]
pub struct WrapperConfig {
    executable: String,
    args: Vec<String>,
    env: Option<HashMap<String, String>>,
    env_files: Option<Vec<String>>,
    inherit_env: bool,
    env_allowlist: Option<HashSet<String>>,
    cwd: Option<String>,
    capture_output: bool,
    stream_output: bool,
    timeout: Option<f64>,
    shell: bool,
    pre_run: Option<Py<PyAny>>,
    post_run: Option<Py<PyAny>>,
    on_start: Option<Py<PyAny>>,
    on_output: Option<Py<PyAny>>,
    on_error: Option<Py<PyAny>>,
    raise_on_error: bool,
    continue_on_pre_run_error: bool,
    continue_on_post_run_error: bool,
    log_execution: bool,
    log_level: i32,
}

impl Clone for WrapperConfig {
    fn clone(&self) -> Self {
        Self {
            executable: self.executable.clone(),
            args: self.args.clone(),
            env: self.env.clone(),
            env_files: self.env_files.clone(),
            inherit_env: self.inherit_env,
            env_allowlist: self.env_allowlist.clone(),
            cwd: self.cwd.clone(),
            capture_output: self.capture_output,
            stream_output: self.stream_output,
            timeout: self.timeout,
            shell: self.shell,
            pre_run: clone_callback(self.pre_run.as_ref()),
            post_run: clone_callback(self.post_run.as_ref()),
            on_start: clone_callback(self.on_start.as_ref()),
            on_output: clone_callback(self.on_output.as_ref()),
            on_error: clone_callback(self.on_error.as_ref()),
            raise_on_error: self.raise_on_error,
            continue_on_pre_run_error: self.continue_on_pre_run_error,
            continue_on_post_run_error: self.continue_on_post_run_error,
            log_execution: self.log_execution,
            log_level: self.log_level,
        }
    }
}

#[pymethods]
impl WrapperConfig {
    #[new]
    #[allow(non_snake_case)]
    #[pyo3(signature = (
        executable,
        *,
        args=Vec::new(),
        env=None,
        env_files=None,
        inherit_env=false,
        env_allowlist=None,
        cwd=None,
        capture_output=false,
        stream_output=true,
        timeout=None,
        shell=false,
        preRun=None,
        postRun=None,
        onStart=None,
        onOutput=None,
        onError=None,
        raise_on_error=true,
        continue_on_pre_run_error=false,
        continue_on_post_run_error=true,
        log_execution=true,
        log_level=LOG_LEVEL_INFO
    ))]
    fn new(
        executable: &Bound<'_, PyAny>,
        args: Vec<String>,
        env: Option<HashMap<String, String>>,
        env_files: Option<&Bound<'_, PyAny>>,
        inherit_env: bool,
        env_allowlist: Option<Vec<String>>,
        cwd: Option<&Bound<'_, PyAny>>,
        capture_output: bool,
        stream_output: bool,
        timeout: Option<f64>,
        shell: bool,
        preRun: Option<Py<PyAny>>,
        postRun: Option<Py<PyAny>>,
        onStart: Option<Py<PyAny>>,
        onOutput: Option<Py<PyAny>>,
        onError: Option<Py<PyAny>>,
        raise_on_error: bool,
        continue_on_pre_run_error: bool,
        continue_on_post_run_error: bool,
        log_execution: bool,
        log_level: i32,
    ) -> PyResult<Self> {
        let py = executable.py();

        Ok(Self {
            executable: path_like_to_string(py, executable)?,
            args,
            env,
            env_files: normalize_env_files(py, env_files)?,
            inherit_env,
            env_allowlist: env_allowlist.map(|items| items.into_iter().collect()),
            cwd: cwd
                .map(|value| path_like_to_string(py, value))
                .transpose()?,
            capture_output,
            stream_output,
            timeout,
            shell,
            pre_run: validate_callable("preRun", preRun)?,
            post_run: validate_callable("postRun", postRun)?,
            on_start: validate_callable("onStart", onStart)?,
            on_output: validate_callable("onOutput", onOutput)?,
            on_error: validate_callable("onError", onError)?,
            raise_on_error,
            continue_on_pre_run_error,
            continue_on_post_run_error,
            log_execution,
            log_level,
        })
    }

    fn __repr__(&self) -> String {
        format!(
            "WrapperConfig(executable='{}', args={:?}, capture_output={}, \
stream_output={}, timeout={:?}, shell={})",
            self.executable,
            self.args,
            self.capture_output,
            self.stream_output,
            self.timeout,
            self.shell
        )
    }

    fn __str__(&self) -> String {
        self.__repr__()
    }
}

impl WrapperConfig {
    fn to_core_config(&self) -> CoreWrapperConfig {
        let mut config = CoreWrapperConfig::new(self.executable.clone());
        config.args = self.args.clone();
        config.env = self.env.clone();
        config.env_files = self.env_files.clone();
        config.inherit_env = self.inherit_env;
        config.env_allowlist = self.env_allowlist.clone();
        config.cwd = self.cwd.clone();
        config.capture_output = self.capture_output;
        config.stream_output = self.stream_output;
        config.timeout = self.timeout;
        config.shell = self.shell;
        config.raise_on_error = self.raise_on_error;
        config.continue_on_pre_run_error = self.continue_on_pre_run_error;
        config.continue_on_post_run_error = self.continue_on_post_run_error;
        config.log_execution = self.log_execution;
        config.log_level = self.log_level;
        config
    }
}

#[pyclass(module = "envoy")]
pub struct ApplicationWrapper {
    config: WrapperConfig,
    process: Option<Child>,
    interrupted: bool,
}

#[pymethods]
impl ApplicationWrapper {
    #[new]
    fn new(config: WrapperConfig) -> Self {
        Self {
            config,
            process: None,
            interrupted: false,
        }
    }

    #[getter]
    fn config(&self, py: Python<'_>) -> PyResult<Py<WrapperConfig>> {
        Py::new(py, self.config.clone())
    }

    fn run(&mut self, py: Python<'_>) -> PyResult<Py<ExecutionResult>> {
        let start_time = Instant::now();
        let mut result = CoreExecutionResult::new(-1);
        let mut pending_error = None;

        self.interrupted = false;

        if let Err(error) = self.execute_pre_run() {
            pending_error = Some(error);
        }

        if pending_error.is_none() {
            let run_result = py.allow_threads(|| self.execute_process(&mut result, start_time));
            if let Err(message) = run_result {
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

        if let Err(error) = self.execute_post_run(&result) {
            return Err(envoy_error_to_pyerr(error));
        }

        if let Some(error) = pending_error {
            return Err(envoy_error_to_pyerr(error));
        }

        if self.config.raise_on_error && !result.success() {
            if result.timed_out {
                return Err(envoy_error_to_pyerr(EnvoyError::Execution(format!(
                    "Process timed out after {}s",
                    format_timeout_value(self.config.timeout.unwrap_or_default())
                ))));
            }

            if result.return_code != 0 {
                return Err(envoy_error_to_pyerr(EnvoyError::Execution(format!(
                    "Process exited with code {}\nCommand: {}",
                    result.return_code,
                    result.command.join(" ")
                ))));
            }
        }

        Py::new(py, ExecutionResult::from_inner(result))
    }

    fn __call__(&mut self, py: Python<'_>) -> PyResult<Py<ExecutionResult>> {
        self.run(py)
    }

    fn __enter__(slf: PyRef<'_, Self>) -> PyRef<'_, Self> {
        slf
    }

    #[pyo3(signature = (_exc_type=None, _exc_val=None, _exc_tb=None))]
    fn __exit__(
        &mut self,
        _exc_type: Option<&Bound<'_, PyAny>>,
        _exc_val: Option<&Bound<'_, PyAny>>,
        _exc_tb: Option<&Bound<'_, PyAny>>,
    ) -> bool {
        ProcessExecutor::terminate_process(self.process.as_mut());
        false
    }
}

impl ApplicationWrapper {
    fn execute_pre_run(&self) -> Result<(), EnvoyError> {
        let Some(callback) = self.config.pre_run.as_ref() else {
            return Ok(());
        };

        self.log_info("Executing pre-run operations...");

        match call_python_noarg(callback) {
            Ok(()) => {
                self.log_info("Pre-run operations completed");
                Ok(())
            }
            Err(message) => {
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

    fn execute_post_run(&self, result: &CoreExecutionResult) -> Result<(), EnvoyError> {
        let Some(callback) = self.config.post_run.as_ref() else {
            return Ok(());
        };

        self.log_info("Executing post-run operations...");

        match call_python_post_run(callback, result) {
            Ok(()) => {
                self.log_info("Post-run operations completed");
                Ok(())
            }
            Err(message) => {
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

    fn execute_process(
        &mut self,
        result: &mut CoreExecutionResult,
        start_time: Instant,
    ) -> Result<(), String> {
        let config = self.config.to_core_config();
        let env_manager = EnvironmentManager::new(config.inherit_env, config.env_allowlist.clone());
        let env_files = config
            .env_files
            .as_ref()
            .map(|items| items.iter().map(PathBuf::from).collect::<Vec<_>>())
            .unwrap_or_default();
        let env = env_manager
            .prepare_environment(&env_files, config.env.as_ref(), None, None)
            .map_err(|error| error.to_string())?;
        let executor = ProcessExecutor::new(config.stream_output);
        let command = executor
            .prepare_command(
                Path::new(&config.executable),
                &config.args,
                env.get("PATH").map(String::as_str),
            )
            .map_err(|error| error.to_string())?;

        result.command = command.clone();

        self.log_info(&format!("Executing: {}", command.join(" ")));
        if let Some(cwd) = config.cwd.as_deref() {
            self.log_info(&format!("Working directory: {cwd}"));
        }

        self.process = Some(self.spawn_child(&command, &env, &config)?);

        if let Some(process) = self.process.as_mut() {
            result.pid = Some(i64::from(process.id()));
        }

        if let (Some(callback), Some(pid)) = (self.config.on_start.as_ref(), result.pid) {
            if let Err(message) = call_python_with_value(callback, pid) {
                self.log_warning(&format!("onStart callback error: {message}"));
            }
        }

        if let Some(pid) = result.pid {
            self.log_info(&format!("Process started with PID: {pid}"));
        }

        if config.capture_output || config.stream_output {
            let (stdout, stderr) = self.drain_process_output(config.stream_output)?;
            if !stdout.is_empty() {
                result.stdout = Some(stdout);
            }
            if !stderr.is_empty() {
                result.stderr = Some(stderr);
            }
        }

        let wait_result = wait_for_child(
            self.process
                .as_mut()
                .ok_or_else(|| String::from("Process handle missing during wait"))?,
            config.timeout,
            &mut self.interrupted,
        )?;

        let return_code = match wait_result {
            Some(status) => exit_status_code(status),
            None => {
                self.log_error(&format!(
                    "Process timed out after {}s",
                    format_timeout_value(config.timeout.unwrap_or_default())
                ));
                let process = self
                    .process
                    .as_mut()
                    .ok_or_else(|| String::from("Process handle missing during timeout"))?;
                ProcessExecutor::terminate_process(Some(process));
                result.timed_out = true;
                -1
            }
        };

        result.return_code = return_code;
        result.execution_time = start_time.elapsed().as_secs_f64();

        if self.interrupted {
            self.log_warning("Process was interrupted");
            result.return_code = -2;
        }

        self.log_info(&format!("Process finished: {result}"));
        Ok(())
    }

    fn drain_process_output(&mut self, stream_output: bool) -> Result<(String, String), String> {
        let stdout_handle = self
            .process
            .as_mut()
            .and_then(|process| process.stdout.take())
            .map(|stdout| {
                start_reader_thread(
                    stdout,
                    StreamKind::Stdout,
                    stream_output,
                    clone_callback(self.config.on_output.as_ref()),
                )
            });
        let stderr_handle = self
            .process
            .as_mut()
            .and_then(|process| process.stderr.take())
            .map(|stderr| {
                start_reader_thread(
                    stderr,
                    StreamKind::Stderr,
                    stream_output,
                    clone_callback(self.config.on_error.as_ref()),
                )
            });

        while handle_running(stdout_handle.as_ref()) || handle_running(stderr_handle.as_ref()) {
            self.check_and_handle_interrupt();
            thread::sleep(Duration::from_millis(25));
        }

        let stdout = join_reader_thread(stdout_handle, StreamKind::Stdout)?;
        let stderr = join_reader_thread(stderr_handle, StreamKind::Stderr)?;
        Ok((stdout.join("\n"), stderr.join("\n")))
    }

    fn check_and_handle_interrupt(&mut self) {
        if self.interrupted {
            return;
        }

        let interrupted = Python::with_gil(|py| py.check_signals().is_err());
        if interrupted {
            self.interrupted = true;
            ProcessExecutor::terminate_process(self.process.as_mut());
        }
    }

    fn spawn_child(
        &self,
        command: &[String],
        env: &HashMap<String, String>,
        config: &CoreWrapperConfig,
    ) -> Result<Child, String> {
        let mut process_command = build_spawn_command(command, config.shell);

        process_command.env_clear().envs(env);

        if let Some(cwd) = config.cwd.as_deref() {
            process_command.current_dir(cwd);
        }

        if config.capture_output || config.stream_output {
            process_command
                .stdout(Stdio::piped())
                .stderr(Stdio::piped());
        }

        process_command
            .spawn()
            .map_err(|error| format!("Failed to spawn process: {error}"))
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

#[pyfunction(name = "createWrapper")]
#[allow(non_snake_case)]
#[pyo3(signature = (
    executable,
    *args,
    env=None,
    env_files=None,
    preRun=None,
    postRun=None,
    inherit_env=false,
    timeout=None,
    capture_output=false,
    stream_output=true,
    raise_on_error=true,
    continue_on_pre_run_error=false,
    continue_on_post_run_error=true,
    shell=false,
    cwd=None,
    onStart=None,
    onOutput=None,
    onError=None,
    log_execution=true,
    log_level=LOG_LEVEL_INFO
))]
fn create_wrapper(
    py: Python<'_>,
    executable: &Bound<'_, PyAny>,
    args: &Bound<'_, PyTuple>,
    env: Option<HashMap<String, String>>,
    env_files: Option<&Bound<'_, PyAny>>,
    preRun: Option<Py<PyAny>>,
    postRun: Option<Py<PyAny>>,
    inherit_env: bool,
    timeout: Option<f64>,
    capture_output: bool,
    stream_output: bool,
    raise_on_error: bool,
    continue_on_pre_run_error: bool,
    continue_on_post_run_error: bool,
    shell: bool,
    cwd: Option<&Bound<'_, PyAny>>,
    onStart: Option<Py<PyAny>>,
    onOutput: Option<Py<PyAny>>,
    onError: Option<Py<PyAny>>,
    log_execution: bool,
    log_level: i32,
) -> PyResult<Py<ApplicationWrapper>> {
    let config = WrapperConfig::new(
        executable,
        args.extract()?,
        env,
        env_files,
        inherit_env,
        None,
        cwd,
        capture_output,
        stream_output,
        timeout,
        shell,
        preRun,
        postRun,
        onStart,
        onOutput,
        onError,
        raise_on_error,
        continue_on_pre_run_error,
        continue_on_post_run_error,
        log_execution,
        log_level,
    )?;

    Py::new(py, ApplicationWrapper::new(config))
}

pub fn register_wrapper_bindings(py: Python<'_>, parent: &Bound<'_, PyModule>) -> PyResult<()> {
    parent.add_class::<ExecutionResult>()?;
    parent.add_class::<WrapperConfig>()?;
    parent.add_class::<ApplicationWrapper>()?;
    parent.add_function(wrap_pyfunction!(create_wrapper, parent)?)?;
    let _ = py;
    Ok(())
}

#[derive(Clone, Copy)]
enum StreamKind {
    Stdout,
    Stderr,
}

impl StreamKind {
    fn label(self) -> &'static str {
        match self {
            StreamKind::Stdout => "stdout",
            StreamKind::Stderr => "stderr",
        }
    }
}

fn start_reader_thread<R: std::io::Read + Send + 'static>(
    stream: R,
    stream_kind: StreamKind,
    stream_output: bool,
    callback: Option<Py<PyAny>>,
) -> JoinHandle<Result<Vec<String>, String>> {
    thread::spawn(move || {
        let mut reader = BufReader::new(stream);
        let mut lines = Vec::new();
        let mut buffer = Vec::new();

        loop {
            buffer.clear();
            let bytes_read = reader.read_until(b'\n', &mut buffer).map_err(|error| {
                format!("Failed to read child {}: {error}", stream_kind.label())
            })?;

            if bytes_read == 0 {
                break;
            }

            while matches!(buffer.last(), Some(b'\n' | b'\r')) {
                buffer.pop();
            }

            let line = String::from_utf8_lossy(&buffer).trim_end().to_string();
            if stream_output {
                write_stream_line(stream_kind, &line).map_err(|error| {
                    format!("Failed to forward child {}: {error}", stream_kind.label())
                })?;
            }

            if let Some(ref callback) = callback {
                let callback_result = call_python_with_value(callback, line.clone());
                if let Err(message) = callback_result {
                    eprintln!(
                        "envoy.wrapper [WARN] on{} callback error: {}",
                        match stream_kind {
                            StreamKind::Stdout => "Output",
                            StreamKind::Stderr => "Error",
                        },
                        message
                    );
                }
            }

            lines.push(line);
        }

        Ok(lines)
    })
}

fn join_reader_thread(
    handle: Option<JoinHandle<Result<Vec<String>, String>>>,
    stream_kind: StreamKind,
) -> Result<Vec<String>, String> {
    let Some(handle) = handle else {
        return Ok(Vec::new());
    };

    handle
        .join()
        .map_err(|_| format!("{} reader thread panicked", stream_kind.label()))?
}

fn handle_running(handle: Option<&JoinHandle<Result<Vec<String>, String>>>) -> bool {
    handle.is_some_and(|handle| !handle.is_finished())
}

fn write_stream_line(stream_kind: StreamKind, line: &str) -> std::io::Result<()> {
    match stream_kind {
        StreamKind::Stdout => {
            println!("{line}");
            std::io::stdout().flush()
        }
        StreamKind::Stderr => {
            eprintln!("{line}");
            std::io::stderr().flush()
        }
    }
}

fn wait_for_child(
    child: &mut Child,
    timeout: Option<f64>,
    interrupted: &mut bool,
) -> Result<Option<ExitStatus>, String> {
    let deadline =
        timeout.map(|seconds| Instant::now() + Duration::from_secs_f64(seconds.max(0.0)));

    loop {
        if !*interrupted && Python::with_gil(|py| py.check_signals().is_err()) {
            *interrupted = true;
            ProcessExecutor::terminate_process(Some(child));
        }

        match child.try_wait() {
            Ok(Some(status)) => return Ok(Some(status)),
            Ok(None) => {}
            Err(error) => return Err(format!("Failed waiting for process: {error}")),
        }

        if let Some(deadline) = deadline {
            if Instant::now() >= deadline {
                return Ok(None);
            }
        }

        thread::sleep(Duration::from_millis(25));
    }
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

fn exit_status_code(status: ExitStatus) -> i64 {
    status.code().map(i64::from).unwrap_or(-1)
}

fn format_timeout_value(timeout: f64) -> String {
    if timeout.fract() == 0.0 {
        format!("{timeout:.1}")
    } else {
        timeout.to_string()
    }
}

fn call_python_noarg(callback: &Py<PyAny>) -> Result<(), String> {
    Python::with_gil(|py| {
        callback
            .call0(py)
            .map(|_| ())
            .map_err(|error| pyerr_message(py, error))
    })
}

fn call_python_post_run(callback: &Py<PyAny>, result: &CoreExecutionResult) -> Result<(), String> {
    Python::with_gil(|py| {
        let py_result = Py::new(py, ExecutionResult::from_inner(result.clone()))
            .map_err(|error| pyerr_message(py, error))?;
        callback
            .call1(py, (py_result,))
            .map(|_| ())
            .map_err(|error| pyerr_message(py, error))
    })
}

fn call_python_with_value<T>(callback: &Py<PyAny>, value: T) -> Result<(), String>
where
    T: IntoPy<PyObject>,
{
    Python::with_gil(|py| {
        callback
            .call1(py, (value,))
            .map(|_| ())
            .map_err(|error| pyerr_message(py, error))
    })
}

fn pyerr_message(py: Python<'_>, error: PyErr) -> String {
    match error.value_bound(py).str() {
        Ok(value) => value.to_string_lossy().into_owned(),
        Err(_) => error.to_string(),
    }
}

fn validate_callable(name: &str, callback: Option<Py<PyAny>>) -> PyResult<Option<Py<PyAny>>> {
    let Some(callback) = callback else {
        return Ok(None);
    };

    Python::with_gil(|py| {
        if callback.bind(py).is_callable() {
            Ok(Some(callback))
        } else {
            Err(PyTypeError::new_err(format!(
                "'{name}' must be callable or None"
            )))
        }
    })
}

fn normalize_env_files(
    py: Python<'_>,
    env_files: Option<&Bound<'_, PyAny>>,
) -> PyResult<Option<Vec<String>>> {
    let Some(env_files) = env_files else {
        return Ok(None);
    };

    if let Ok(path) = path_like_to_string(py, env_files) {
        return Ok(Some(vec![path]));
    }

    let mut normalized = Vec::new();
    for item in env_files.iter()? {
        normalized.push(path_like_to_string(py, &item?)?);
    }
    Ok(Some(normalized))
}

fn clone_callback(callback: Option<&Py<PyAny>>) -> Option<Py<PyAny>> {
    callback.map(|callback| Python::with_gil(|py| callback.clone_ref(py)))
}

fn path_like_to_string(py: Python<'_>, value: &Bound<'_, PyAny>) -> PyResult<String> {
    let os = PyModule::import_bound(py, "os")?;
    let path_value = os.getattr("fspath")?.call1((value,))?;
    if let Ok(text) = path_value.extract::<String>() {
        return Ok(text);
    }
    if let Ok(bytes) = path_value.extract::<Vec<u8>>() {
        return Ok(String::from_utf8_lossy(&bytes).into_owned());
    }
    Err(PyTypeError::new_err("Expected a path-like object"))
}

#[cfg(test)]
mod tests {
    use super::{path_like_to_string, validate_callable, ExecutionResult, WrapperConfig};
    use envoy_core::models::ExecutionResult as CoreExecutionResult;
    use pyo3::prelude::*;
    use pyo3::types::{PyDict, PyString};

    fn with_python<T>(test_fn: impl FnOnce(Python<'_>) -> T) -> T {
        pyo3::prepare_freethreaded_python();
        Python::with_gil(test_fn)
    }

    #[test]
    fn execution_result_success_matches_core_model() {
        let mut inner = CoreExecutionResult::new(0);
        inner.execution_time = 1.5;
        inner.pid = Some(42);
        let result = ExecutionResult::from_inner(inner);

        assert!(result.inner.success());
        assert_eq!(
            result.__repr__(),
            "ExecutionResult(SUCCESS, time=1.50s, pid=42)"
        );
    }

    #[test]
    fn path_like_inputs_accept_pathlib_objects() {
        with_python(|py| {
            let pathlib =
                pyo3::types::PyModule::import_bound(py, "pathlib").expect("pathlib should import");
            let path = pathlib
                .getattr("Path")
                .expect("Path should exist")
                .call1(("C:\\temp\\tool.exe",))
                .expect("Path call should succeed");

            let value = path_like_to_string(py, &path).expect("Path should convert");
            assert_eq!(value, "C:\\temp\\tool.exe");
        });
    }

    #[test]
    fn wrapper_config_rejects_non_callable_callback_values() {
        with_python(|py| {
            let error =
                validate_callable("preRun", Some(PyDict::new_bound(py).into_any().unbind()))
                    .expect_err("dict should not be accepted as callback");

            assert!(error.to_string().contains("preRun"));
        });
    }

    #[test]
    fn wrapper_config_defaults_match_python_model() {
        with_python(|py| {
            let config = WrapperConfig::new(
                PyString::new_bound(py, "cmd").as_any(),
                Vec::new(),
                None,
                None,
                false,
                None,
                None,
                false,
                true,
                None,
                false,
                None,
                None,
                None,
                None,
                None,
                true,
                false,
                true,
                true,
                envoy_core::models::LOG_LEVEL_INFO,
            )
            .expect("WrapperConfig should construct");

            assert!(config.args.is_empty());
            assert!(!config.capture_output);
            assert!(config.stream_output);
            assert!(config.raise_on_error);
        });
    }
}
