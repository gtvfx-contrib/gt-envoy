#![allow(clippy::useless_conversion)]

//! PyO3 binding for `envoy.cli_main`, ported from `py/envoy/_cli.py`'s
//! `main()` entry point.
//!
//! Delegates to `envoy-cli`'s extracted library function (`envoy_cli::run`)
//! so the native `envoy` binary and this Python binding share exactly the
//! same CLI dispatch logic (argument parsing, raw-path detection, `-e`
//! environment overrides, `=`-operator short-flag normalization, etc.)
//! without duplicating it.

use pyo3::prelude::*;
use pyo3::types::PyModule;

/// Runs the envoy CLI dispatcher, mirroring `py/envoy/_cli.py`'s `main()`.
///
/// Args:
///     argv: Command-line arguments (defaults to `sys.argv[1:]`).
///
/// Returns:
///     Exit code.
#[pyfunction]
#[pyo3(signature = (argv=None))]
fn cli_main(py: Python<'_>, argv: Option<Vec<String>>) -> PyResult<i32> {
    let argv = match argv {
        Some(values) => values,
        None => {
            let sys = PyModule::import_bound(py, "sys")?;
            let sys_argv: Vec<String> = sys.getattr("argv")?.extract()?;
            sys_argv.into_iter().skip(1).collect()
        }
    };

    Ok(envoy_cli::run(&argv))
}

pub fn register_cli_bindings(_py: Python<'_>, m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add_function(wrap_pyfunction!(cli_main, m)?)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn with_python<T>(test_fn: impl FnOnce(Python<'_>) -> T) -> T {
        pyo3::prepare_freethreaded_python();
        Python::with_gil(test_fn)
    }

    #[test]
    fn register_cli_bindings_adds_cli_main() {
        with_python(|py| {
            let module = PyModule::new_bound(py, "envoy").expect("module should be created");
            register_cli_bindings(py, &module).expect("cli bindings should register");
            assert!(module.getattr("cli_main").is_ok());
        });
    }

    #[test]
    fn cli_main_returns_success_for_help_flag() {
        with_python(|py| {
            let exit_code = cli_main(py, Some(vec!["--help".to_string()])).expect("should run");
            assert_eq!(exit_code, 0);
        });
    }

    #[test]
    fn cli_main_returns_success_for_version_flag() {
        with_python(|py| {
            let exit_code = cli_main(py, Some(vec!["--version".to_string()])).expect("should run");
            assert_eq!(exit_code, 0);
        });
    }

    #[test]
    fn cli_main_defaults_to_sys_argv_when_none() {
        with_python(|py| {
            let sys = PyModule::import_bound(py, "sys").expect("sys should import");
            sys.setattr("argv", vec!["envoy", "--help"])
                .expect("argv should be settable");
            let exit_code = cli_main(py, None).expect("should run");
            assert_eq!(exit_code, 0);
        });
    }
}
