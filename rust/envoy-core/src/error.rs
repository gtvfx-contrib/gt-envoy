//! Error hierarchy for `envoy-core`, ported from `py/envoy/_exceptions.py`.
//!
//! The Python hierarchy is:
//!
//! ```text
//! EnvoyError
//! ├── WrapperError          (back-compat alias for EnvoyError)
//! │   ├── PreRunError
//! │   ├── PostRunError
//! │   └── ExecutionError
//! ├── CalledProcessError    (also inherits subprocess.CalledProcessError)
//! ├── EnvironmentBuildError
//! ├── CommandNotFoundError
//! └── ValidationError
//! ```
//!
//! Rust has no exception hierarchy, so this is represented as a single
//! `EnvoyError` enum with one variant per Python exception class. The
//! `envoy-py` crate is responsible for mapping each variant to a distinct
//! Python exception type (via `pyo3::create_exception!`) so that
//! `except envoy.CommandNotFoundError` and friends keep working for
//! existing consumers exactly as they do today.

use std::path::PathBuf;

use thiserror::Error;

/// Convenience alias used throughout `envoy-core`.
pub type Result<T> = std::result::Result<T, EnvoyError>;

/// Root error type for all fallible `envoy-core` operations.
///
/// Every variant corresponds 1:1 to a Python exception class in
/// `_exceptions.py`; see the module docs for the mapping.
#[derive(Debug, Error)]
pub enum EnvoyError {
    /// Corresponds to `PreRunError` -- failure during pre-run operations.
    #[error("pre-run error: {0}")]
    PreRun(String),

    /// Corresponds to `PostRunError` -- failure during post-run operations.
    #[error("post-run error: {0}")]
    PostRun(String),

    /// Corresponds to `ExecutionError` -- failure during application execution.
    #[error("execution error: {0}")]
    Execution(String),

    /// Corresponds to `CalledProcessError` -- a checked subprocess call
    /// exited with a non-zero return code.
    #[error("command '{cmd}' returned non-zero exit status {returncode}")]
    CalledProcess {
        returncode: i32,
        cmd: String,
        output: Option<Vec<u8>>,
        stderr: Option<Vec<u8>>,
    },

    /// Corresponds to `EnvironmentBuildError` -- failed to construct the
    /// subprocess environment for a command (missing/invalid env files,
    /// expansion or path-resolution failure).
    #[error("failed to build environment: {0}")]
    EnvironmentBuild(String),

    /// Corresponds to `CommandNotFoundError` -- named command not present
    /// in the loaded registry.
    #[error("command not found: {0}")]
    CommandNotFound(String),

    /// Corresponds to `ValidationError` -- a value provided to an envoy API
    /// failed validation.
    #[error("validation error: {0}")]
    Validation(String),

    /// I/O failure reading/writing a file (env files, config files, etc.).
    /// Not a distinct Python exception class; mapped to `EnvironmentBuildError`
    /// or `ValidationError` at the PyO3 boundary depending on context.
    #[error("I/O error at {path}: {source}")]
    Io {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },

    /// JSON parse failure for an env/commands/config file. Mapped to
    /// `EnvironmentBuildError` at the PyO3 boundary.
    #[error("invalid JSON in {path}: {source}")]
    Json {
        path: PathBuf,
        #[source]
        source: serde_json::Error,
    },
}

impl EnvoyError {
    /// Construct a [`EnvoyError::CalledProcess`] with no captured output,
    /// mirroring the common case in `proc.checkCall`.
    pub fn called_process(returncode: i32, cmd: impl Into<String>) -> Self {
        EnvoyError::CalledProcess {
            returncode,
            cmd: cmd.into(),
            output: None,
            stderr: None,
        }
    }
}
