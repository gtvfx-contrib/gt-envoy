//! Canonical PyO3 exception hierarchy for `envoy`.
//!
//! The authoritative Python shape lives in `py/envoy/_exceptions.py`. This
//! module defines the Rust/PyO3 equivalents once and registers the same class
//! objects on `envoy`, `envoy.exceptions`, and selected aliases such as
//! `envoy.proc.CalledProcessError`.

use envoy_core::error::EnvoyError as CoreEnvoyError;
use pyo3::create_exception;
use pyo3::exceptions::PyException;
use pyo3::prelude::*;
use pyo3::types::{PyAny, PyBytes, PyModule};

const EXCEPTIONS_MODULE_DOC: &str = r#"envoy.exceptions -- Public exception module.

All envoy exceptions are accessible here. `CalledProcessError` is also
re-exported on `envoy.proc` and is the same class object in both places.
"#;

create_exception!(envoy, EnvoyError, PyException);
create_exception!(envoy, WrapperError, EnvoyError);
create_exception!(envoy, PreRunError, WrapperError);
create_exception!(envoy, PostRunError, WrapperError);
create_exception!(envoy, ExecutionError, WrapperError);
create_exception!(envoy, CalledProcessError, EnvoyError);
create_exception!(envoy, EnvironmentBuildError, EnvoyError);
create_exception!(envoy, CommandNotFoundError, EnvoyError);
create_exception!(envoy, ValidationError, EnvoyError);

pub fn register_exception_bindings(py: Python<'_>, parent: &Bound<'_, PyModule>) -> PyResult<()> {
    add_exception_types(py, parent)?;

    let module = PyModule::new_bound(py, "envoy.exceptions")?;
    module.add("__doc__", EXCEPTIONS_MODULE_DOC)?;
    add_exception_types(py, &module)?;
    parent.add("exceptions", module.clone())?;
    parent.add_submodule(&module)?;
    Ok(())
}

pub fn called_process_error(
    py: Python<'_>,
    returncode: i32,
    cmd: String,
    output: Option<Vec<u8>>,
    stderr: Option<Vec<u8>>,
) -> PyErr {
    let message = format!("command '{cmd}' returned non-zero exit status {returncode}");
    let instance = py
        .get_type_bound::<CalledProcessError>()
        .call1((message,))
        .expect("CalledProcessError should be instantiable");

    instance
        .setattr("returncode", returncode)
        .expect("CalledProcessError.returncode should be assignable");
    instance
        .setattr("cmd", cmd)
        .expect("CalledProcessError.cmd should be assignable");
    set_optional_bytes_attr(py, &instance, "output", output)
        .expect("CalledProcessError.output should be assignable");
    set_optional_bytes_attr(py, &instance, "stderr", stderr)
        .expect("CalledProcessError.stderr should be assignable");

    PyErr::from_value_bound(instance.into_any())
}

pub fn envoy_error_to_pyerr(error: CoreEnvoyError) -> PyErr {
    match error {
        CoreEnvoyError::PreRun(message) => PreRunError::new_err(message),
        CoreEnvoyError::PostRun(message) => PostRunError::new_err(message),
        CoreEnvoyError::Execution(message) => ExecutionError::new_err(message),
        CoreEnvoyError::CalledProcess {
            returncode,
            cmd,
            output,
            stderr,
        } => Python::with_gil(|py| called_process_error(py, returncode, cmd, output, stderr)),
        CoreEnvoyError::EnvironmentBuild(message) => EnvironmentBuildError::new_err(message),
        CoreEnvoyError::CommandNotFound(message) => CommandNotFoundError::new_err(message),
        CoreEnvoyError::Validation(message) => ValidationError::new_err(message),
        CoreEnvoyError::Io { .. } | CoreEnvoyError::Json { .. } => {
            EnvironmentBuildError::new_err(error.to_string())
        }
    }
}

fn add_exception_types(py: Python<'_>, module: &Bound<'_, PyModule>) -> PyResult<()> {
    module.add("EnvoyError", py.get_type_bound::<EnvoyError>())?;
    module.add("WrapperError", py.get_type_bound::<WrapperError>())?;
    module.add("PreRunError", py.get_type_bound::<PreRunError>())?;
    module.add("PostRunError", py.get_type_bound::<PostRunError>())?;
    module.add("ExecutionError", py.get_type_bound::<ExecutionError>())?;
    module.add(
        "CalledProcessError",
        py.get_type_bound::<CalledProcessError>(),
    )?;
    module.add(
        "EnvironmentBuildError",
        py.get_type_bound::<EnvironmentBuildError>(),
    )?;
    module.add(
        "CommandNotFoundError",
        py.get_type_bound::<CommandNotFoundError>(),
    )?;
    module.add("ValidationError", py.get_type_bound::<ValidationError>())?;
    Ok(())
}

fn set_optional_bytes_attr(
    py: Python<'_>,
    instance: &Bound<'_, PyAny>,
    attr: &str,
    value: Option<Vec<u8>>,
) -> PyResult<()> {
    match value {
        Some(bytes) => instance.setattr(attr, PyBytes::new_bound(py, &bytes)),
        None => instance.setattr(attr, py.None()),
    }
}

#[cfg(test)]
mod tests {
    use super::{
        called_process_error, envoy_error_to_pyerr, CalledProcessError, CommandNotFoundError,
        EnvironmentBuildError, EnvoyError, ExecutionError, PostRunError, PreRunError,
        ValidationError, WrapperError,
    };
    use envoy_core::error::EnvoyError as CoreEnvoyError;
    use pyo3::prelude::*;
    use pyo3::types::{PyBytes, PyModule, PyType};

    fn with_python<T>(test_fn: impl FnOnce(Python<'_>) -> T) -> T {
        pyo3::prepare_freethreaded_python();
        Python::with_gil(test_fn)
    }

    fn assert_issubclass(py: Python<'_>, child: &Bound<'_, PyType>, parent: &Bound<'_, PyType>) {
        let builtins = PyModule::import_bound(py, "builtins")
            .expect("builtins should be importable for hierarchy tests");
        let is_subclass: bool = builtins
            .getattr("issubclass")
            .expect("issubclass should exist")
            .call1((child, parent))
            .expect("issubclass call should succeed")
            .extract()
            .expect("issubclass result should be bool");

        assert!(
            is_subclass,
            "{} should inherit from {}",
            child.name().expect("child name should resolve"),
            parent.name().expect("parent name should resolve")
        );
    }

    fn assert_maps_to(py: Python<'_>, error: CoreEnvoyError, expected_type: &Bound<'_, PyType>) {
        let pyerr = envoy_error_to_pyerr(error);
        assert!(pyerr.get_type_bound(py).is(expected_type));
    }

    #[test]
    fn exception_hierarchy_matches_python_shape() {
        with_python(|py| {
            assert_issubclass(
                py,
                &py.get_type_bound::<WrapperError>(),
                &py.get_type_bound::<EnvoyError>(),
            );
            assert_issubclass(
                py,
                &py.get_type_bound::<PreRunError>(),
                &py.get_type_bound::<WrapperError>(),
            );
            assert_issubclass(
                py,
                &py.get_type_bound::<PostRunError>(),
                &py.get_type_bound::<WrapperError>(),
            );
            assert_issubclass(
                py,
                &py.get_type_bound::<ExecutionError>(),
                &py.get_type_bound::<WrapperError>(),
            );
            assert_issubclass(
                py,
                &py.get_type_bound::<CalledProcessError>(),
                &py.get_type_bound::<EnvoyError>(),
            );
            assert_issubclass(
                py,
                &py.get_type_bound::<CommandNotFoundError>(),
                &py.get_type_bound::<EnvoyError>(),
            );
            assert_issubclass(
                py,
                &py.get_type_bound::<EnvironmentBuildError>(),
                &py.get_type_bound::<EnvoyError>(),
            );
            assert_issubclass(
                py,
                &py.get_type_bound::<ValidationError>(),
                &py.get_type_bound::<EnvoyError>(),
            );
        });
    }

    #[test]
    fn envoy_error_variants_map_to_expected_exception_types() {
        with_python(|py| {
            assert_maps_to(
                py,
                CoreEnvoyError::PreRun(String::from("pre-run failure")),
                &py.get_type_bound::<PreRunError>(),
            );
            assert_maps_to(
                py,
                CoreEnvoyError::PostRun(String::from("post-run failure")),
                &py.get_type_bound::<PostRunError>(),
            );
            assert_maps_to(
                py,
                CoreEnvoyError::Execution(String::from("execution failure")),
                &py.get_type_bound::<ExecutionError>(),
            );
            assert_maps_to(
                py,
                CoreEnvoyError::EnvironmentBuild(String::from("env failure")),
                &py.get_type_bound::<EnvironmentBuildError>(),
            );
            assert_maps_to(
                py,
                CoreEnvoyError::CommandNotFound(String::from("missing command")),
                &py.get_type_bound::<CommandNotFoundError>(),
            );
            assert_maps_to(
                py,
                CoreEnvoyError::Validation(String::from("bad value")),
                &py.get_type_bound::<ValidationError>(),
            );
            assert_maps_to(
                py,
                CoreEnvoyError::Io {
                    path: "config.json".into(),
                    source: std::io::Error::other("read failed"),
                },
                &py.get_type_bound::<EnvironmentBuildError>(),
            );
            assert_maps_to(
                py,
                CoreEnvoyError::Json {
                    path: "config.json".into(),
                    source: serde_json::from_str::<serde_json::Value>("not-json")
                        .expect_err("fixture should be invalid JSON"),
                },
                &py.get_type_bound::<EnvironmentBuildError>(),
            );
        });
    }

    #[test]
    fn called_process_errors_expose_process_attributes() {
        with_python(|py| {
            let pyerr = called_process_error(
                py,
                5,
                String::from("cmd.exe"),
                Some(vec![1_u8, 2_u8]),
                Some(vec![3_u8, 4_u8]),
            );
            let value = pyerr.value_bound(py);

            assert!(pyerr
                .get_type_bound(py)
                .is(&py.get_type_bound::<CalledProcessError>()));
            assert_eq!(
                value
                    .getattr("returncode")
                    .expect("returncode attr should exist")
                    .extract::<i32>()
                    .expect("returncode should be int"),
                5
            );
            assert_eq!(
                value
                    .getattr("cmd")
                    .expect("cmd attr should exist")
                    .extract::<String>()
                    .expect("cmd should be str"),
                "cmd.exe"
            );
            assert_eq!(
                value
                    .getattr("output")
                    .expect("output attr should exist")
                    .downcast::<PyBytes>()
                    .expect("output should be bytes")
                    .as_bytes(),
                &[1_u8, 2_u8]
            );
            assert_eq!(
                value
                    .getattr("stderr")
                    .expect("stderr attr should exist")
                    .downcast::<PyBytes>()
                    .expect("stderr should be bytes")
                    .as_bytes(),
                &[3_u8, 4_u8]
            );
        });
    }
}
