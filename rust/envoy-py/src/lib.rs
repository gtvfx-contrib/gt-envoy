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
mod proc;

/// Returns the `envoy-core` crate version this extension was built against.
/// Placeholder export to prove the PyO3 build/import path works end-to-end;
/// removed once real bindings are ported.
#[pyfunction]
fn _core_version() -> &'static str {
    env!("CARGO_PKG_VERSION")
}

#[pymodule]
fn _envoy(py: Python<'_>, m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add_function(wrap_pyfunction!(_core_version, m)?)?;
    api::register_api_bindings(py, m)?;
    proc::register_proc_module(py, m)?;
    Ok(())
}
