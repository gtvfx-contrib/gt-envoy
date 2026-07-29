//! PyO3 bindings exposing `envoy-core` as the `envoy` Python package.
//!
//! This crate is built with `maturin` into a compiled extension module
//! that is imported as `envoy._envoy` by `python/envoy/__init__.py` (mixed
//! maturin project layout), preserving the existing `import envoy`,
//! `envoy.proc`, `envoy.testing` surface for consumers such as
//! `gt/globals/py/gt/vscode/wrapper`, `gt/devtools/py/cleanup_branches.py`,
//! and the `gt/krita` / `gt/unreal` wrapper packages.
//!
//! Current bindings expose the full `py/envoy/__init__.py` public surface:
//! the `envoy.proc` subprocess module, top-level `_api.py` convenience
//! functions, `CommandDefinition`/`CommandRegistry`, the named-stack
//! registry, `Bundle`/`BundleInfo`/`Stack` discovery, `ApplicationWrapper`/
//! `WrapperConfig`, the PyO3 exception hierarchy, and `cli_main`.

// pyo3's `#[pyfunction]`/`#[pymethods]` macros generate return-value
// conversion code inside a `quote_spanned!` block that reuses the spans of
// the annotated function's own tokens (e.g. its return type), so clippy
// attributes the resulting `.into()` call to our source instead of to
// pyo3's generated glue. This is a known upstream false positive tracked
// at https://github.com/PyO3/pyo3/pull/4944 (open/unreleased as of pyo3
// 0.22) -- suppressed crate-wide rather than annotating every binding.
#![allow(clippy::useless_conversion)]

use pyo3::prelude::*;

mod api;
mod cli;
mod commands;
mod environment;
mod exceptions;
mod proc;
mod stack_registry;
mod telemetry;
mod wrapper;

/// Returns the `envoy-core` crate version this extension was built against.
/// Placeholder export to prove the PyO3 build/import path works end-to-end;
/// removed once real bindings are ported.
#[pyfunction]
fn _core_version() -> &'static str {
    env!("CARGO_PKG_VERSION")
}

/// Git-tag-derived version string (see `build.rs`), exposed as
/// `envoy.__version__` via `python/envoy/__init__.py`, mirroring
/// `py/envoy/__init__.py`'s `hatch-vcs`-derived `__version__`.
#[pyfunction]
fn _git_version() -> &'static str {
    env!("ENVOY_PY_VERSION")
}

#[pymodule]
fn _envoy(py: Python<'_>, m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add_function(wrap_pyfunction!(_core_version, m)?)?;
    m.add_function(wrap_pyfunction!(_git_version, m)?)?;
    exceptions::register_exception_bindings(py, m)?;
    api::register_api_bindings(py, m)?;
    cli::register_cli_bindings(py, m)?;
    commands::register_command_bindings(py, m)?;
    stack_registry::register_stack_registry_bindings(py, m)?;
    environment::register_environment_module(py, m)?;
    proc::register_proc_module(py, m)?;
    telemetry::register_telemetry_module(py, m)?;
    wrapper::register_wrapper_bindings(py, m)?;
    Ok(())
}
