//! PyO3 bindings exposing `envoy-core` as the `envoy` Python package.
//!
//! This crate is built with `maturin` into a compiled extension module
//! that is imported as `envoy._envoy` by `python/envoy/__init__.py` (mixed
//! maturin project layout), preserving the existing `import envoy`,
//! `envoy.proc`, `envoy.testing` surface for consumers such as
//! `gt/globals/py/gt/vscode/wrapper`, `gt/devtools/py/cleanup_branches.py`,
//! and the `gt/krita` / `gt/unreal` wrapper packages.
//!
//! This is currently a Phase 0 scaffolding placeholder. The real bindings
//! (Bundle, BundleConfig, ApplicationWrapper, proc.call/checkCall/
//! checkOutput/spawn, exception classes, etc.) land in Phases 2-4 of the
//! migration plan as envoy-core gains the corresponding modules.

use pyo3::prelude::*;

/// Returns the `envoy-core` crate version this extension was built against.
/// Placeholder export to prove the PyO3 build/import path works end-to-end;
/// removed once real bindings are ported.
#[pyfunction]
fn _core_version() -> &'static str {
    env!("CARGO_PKG_VERSION")
}

#[pymodule]
fn _envoy(m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add_function(wrap_pyfunction!(_core_version, m)?)?;
    Ok(())
}
