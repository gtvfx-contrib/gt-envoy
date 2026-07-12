//! Data models for process execution and wrapper configuration.
//!
//! This module ports the dataclasses from `py/envoy/_models.py` into
//! `envoy-core`. The goal is to preserve the original model shape and default
//! behavior while expressing it in idiomatic Rust types.
//!
//! [`ExecutionResult`] mirrors the Python execution result container used by
//! wrapper and executor code.
//! [`WrapperConfig`] mirrors the Python wrapper configuration object.
//! Callback invocation remains a Phase 3 concern while `_wrapper.py` and
//! `_executor.py` are ported, but the callback slots are preserved here so the
//! Python-to-Rust field mapping stays explicit and traceable.

use std::collections::{HashMap, HashSet};
use std::fmt;

/// Numeric value of Python's `logging.INFO`.
///
/// Real logging crate integration is intentionally deferred. Until a concrete
/// Rust logging backend is chosen, `WrapperConfig::log_level` stores the same
/// numeric value as the Python model for traceability.
pub const LOG_LEVEL_INFO: i32 = 20;

/// Placeholder callback for Python's `preRun`.
///
/// The Phase 3 port of `_wrapper.py` / `_executor.py` will decide how these
/// callbacks are invoked in the Rust execution pipeline.
pub type PreRunCallback = dyn Fn() + Send + Sync;

/// Placeholder callback for Python's `postRun`.
///
/// The callback receives the completed [`ExecutionResult`].
pub type PostRunCallback = dyn Fn(&ExecutionResult) + Send + Sync;

/// Placeholder callback for Python's `onStart`.
///
/// The callback receives the spawned process identifier.
pub type OnStartCallback = dyn Fn(i64) + Send + Sync;

/// Placeholder callback for Python's `onOutput`.
///
/// The callback receives one output line at a time.
pub type OutputCallback = dyn Fn(&str) + Send + Sync;

/// Placeholder callback for Python's `onError`.
///
/// The callback receives one stderr line at a time.
pub type ErrorCallback = dyn Fn(&str) + Send + Sync;

/// Container for execution results.
///
/// This is the Rust equivalent of the Python `ExecutionResult` dataclass. It
/// stores subprocess status, captured output, timing information, and the
/// executed command.
#[derive(Debug, Clone)]
pub struct ExecutionResult {
    /// Process exit code.
    pub return_code: i64,
    /// Captured stdout, if output capture was enabled.
    pub stdout: Option<String>,
    /// Captured stderr, if output capture was enabled.
    pub stderr: Option<String>,
    /// Total execution time in seconds.
    pub execution_time: f64,
    /// Process identifier, when available.
    pub pid: Option<i64>,
    /// Command line that was executed.
    pub command: Vec<String>,
    /// Whether execution exceeded the configured timeout.
    pub timed_out: bool,
}

impl ExecutionResult {
    /// Create a new execution result with Python-matching defaults.
    ///
    /// Only `return_code` is required. All optional fields default to
    /// `None`, `command` defaults to an empty vector, `execution_time`
    /// defaults to `0.0`, and `timed_out` defaults to `false`.
    pub fn new(return_code: i64) -> Self {
        Self {
            return_code,
            stdout: None,
            stderr: None,
            execution_time: 0.0,
            pid: None,
            command: Vec::new(),
            timed_out: false,
        }
    }

    /// Return whether execution completed successfully.
    ///
    /// This matches the Python `success` property exactly:
    /// `return_code == 0 and not timed_out`.
    pub fn success(&self) -> bool {
        self.return_code == 0 && !self.timed_out
    }
}

impl fmt::Display for ExecutionResult {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        let status = if self.success() {
            String::from("SUCCESS")
        } else {
            format!("FAILED (code={})", self.return_code)
        };
        let pid = match self.pid {
            Some(pid) => pid.to_string(),
            None => String::from("None"),
        };

        write!(
            formatter,
            "ExecutionResult({}, time={:.2}s, pid={})",
            status, self.execution_time, pid
        )
    }
}

/// Configuration for the application wrapper.
///
/// This struct ports the Python `WrapperConfig` dataclass into Rust. The field
/// set is intentionally close to the source model so later ports of
/// `_wrapper.py` and `_executor.py` can reuse it without lossy translation.
///
/// `WrapperConfig` intentionally does not derive `Debug` or `Clone`. The
/// placeholder callback fields are stored as `Box<dyn Fn...>` trait objects,
/// which do not implement those traits in a way that cleanly matches Python's
/// closure semantics.
pub struct WrapperConfig {
    /// Executable path or command name.
    pub executable: String,
    /// Command-line arguments.
    pub args: Vec<String>,
    /// Explicit environment variables to inject.
    pub env: Option<HashMap<String, String>>,
    /// Environment file paths.
    ///
    /// The Python model accepts either a single path or multiple paths. The
    /// Rust port collapses that union into `Vec<String>` for a simpler, more
    /// uniform representation.
    pub env_files: Option<Vec<String>>,
    /// Whether to inherit the parent process environment.
    pub inherit_env: bool,
    /// System variables allowed through when inheriting selectively.
    pub env_allowlist: Option<HashSet<String>>,
    /// Working directory for the spawned process.
    pub cwd: Option<String>,
    /// Whether stdout/stderr should be captured.
    pub capture_output: bool,
    /// Whether stdout/stderr should be streamed live.
    pub stream_output: bool,
    /// Timeout in seconds.
    pub timeout: Option<f64>,
    /// Whether to execute through a shell.
    pub shell: bool,
    /// Placeholder for Python's `preRun` callback.
    ///
    /// Real callback wiring is deferred to Phase 3.
    pub pre_run: Option<Box<PreRunCallback>>,
    /// Placeholder for Python's `postRun` callback.
    ///
    /// Real callback wiring is deferred to Phase 3.
    pub post_run: Option<Box<PostRunCallback>>,
    /// Placeholder for Python's `onStart` callback.
    ///
    /// Real callback wiring is deferred to Phase 3.
    pub on_start: Option<Box<OnStartCallback>>,
    /// Placeholder for Python's `onOutput` callback.
    ///
    /// Real callback wiring is deferred to Phase 3.
    pub on_output: Option<Box<OutputCallback>>,
    /// Placeholder for Python's `onError` callback.
    ///
    /// Real callback wiring is deferred to Phase 3.
    pub on_error: Option<Box<ErrorCallback>>,
    /// Whether non-zero execution should raise an error.
    pub raise_on_error: bool,
    /// Whether pre-run callback errors should be ignored.
    pub continue_on_pre_run_error: bool,
    /// Whether post-run callback errors should be ignored.
    pub continue_on_post_run_error: bool,
    /// Whether execution should be logged.
    pub log_execution: bool,
    /// Deferred numeric log level, matching Python's `logging` constants.
    pub log_level: i32,
}

impl WrapperConfig {
    /// Create a new wrapper configuration with Python-matching defaults.
    ///
    /// Only `executable` is required. All other fields are initialized to the
    /// same defaults as the Python dataclass, with the Phase 3 callback slots
    /// starting as `None`.
    pub fn new(executable: impl Into<String>) -> Self {
        Self {
            executable: executable.into(),
            args: Vec::new(),
            env: None,
            env_files: None,
            inherit_env: false,
            env_allowlist: None,
            cwd: None,
            capture_output: false,
            stream_output: true,
            timeout: None,
            shell: false,
            pre_run: None,
            post_run: None,
            on_start: None,
            on_output: None,
            on_error: None,
            raise_on_error: true,
            continue_on_pre_run_error: false,
            continue_on_post_run_error: true,
            log_execution: true,
            log_level: LOG_LEVEL_INFO,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{ExecutionResult, WrapperConfig, LOG_LEVEL_INFO};

    #[test]
    fn execution_result_success_is_true_for_zero_exit_without_timeout() {
        let result = ExecutionResult::new(0);

        assert!(result.success());
    }

    #[test]
    fn execution_result_success_is_false_for_non_zero_exit() {
        let result = ExecutionResult::new(1);

        assert!(!result.success());
    }

    #[test]
    fn execution_result_success_is_false_for_timeout_even_with_zero_exit() {
        let mut result = ExecutionResult::new(0);
        result.timed_out = true;

        assert!(!result.success());
    }

    #[test]
    fn execution_result_display_matches_success_repr() {
        let mut result = ExecutionResult::new(0);
        result.execution_time = 1.234;
        result.pid = Some(456);

        assert_eq!(
            format!("{result}"),
            "ExecutionResult(SUCCESS, time=1.23s, pid=456)"
        );
    }

    #[test]
    fn execution_result_display_matches_failed_repr() {
        let mut result = ExecutionResult::new(1);
        result.execution_time = 1.234;
        result.pid = Some(456);

        assert_eq!(
            format!("{result}"),
            "ExecutionResult(FAILED (code=1), time=1.23s, pid=456)"
        );
    }

    #[test]
    fn wrapper_config_new_uses_python_defaults() {
        let config = WrapperConfig::new("envoy.exe");

        assert_eq!(config.executable, "envoy.exe");
        assert!(config.args.is_empty());
        assert!(config.env.is_none());
        assert!(config.env_files.is_none());
        assert!(!config.inherit_env);
        assert!(config.env_allowlist.is_none());
        assert!(config.cwd.is_none());
        assert!(!config.capture_output);
        assert!(config.stream_output);
        assert!(config.timeout.is_none());
        assert!(!config.shell);
        assert!(config.pre_run.is_none());
        assert!(config.post_run.is_none());
        assert!(config.on_start.is_none());
        assert!(config.on_output.is_none());
        assert!(config.on_error.is_none());
        assert!(config.raise_on_error);
        assert!(!config.continue_on_pre_run_error);
        assert!(config.continue_on_post_run_error);
        assert!(config.log_execution);
        assert_eq!(config.log_level, LOG_LEVEL_INFO);
    }
}
