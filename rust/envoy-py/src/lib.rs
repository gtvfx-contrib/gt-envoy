//! PyO3 bindings exposing `envoy-core` as the `envoy` Python package.
//!
//! This crate is built with `maturin` into a compiled extension module
//! that is imported as `envoy._envoy` by `python/envoy/__init__.py` (mixed
//! maturin project layout), preserving the existing `import envoy`,
//! `envoy.proc`, `envoy.testing` surface for consumers such as
//! `gt/globals/py/gt/vscode/wrapper`, `gt/devtools/py/cleanup_branches.py`,
//! and the `gt/krita` / `gt/unreal` wrapper packages.
//!
//! Current bindings include the `envoy.proc` subprocess surface plus the
//! top-level `_api.py` convenience functions and supporting wrapper types
//! (`UserConfig`, `BundleConfig`, and trace-event records). Additional
//! bundle/discovery/wrapper APIs land in later migration phases.

use pyo3::prelude::*;

mod api;
mod commands;
mod config_registry;
mod exceptions;
mod proc;
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
    commands::register_command_bindings(py, m)?;
    config_registry::register_config_registry_bindings(py, m)?;
    proc::register_proc_module(py, m)?;
    wrapper::register_wrapper_bindings(py, m)?;
    Ok(())
}
