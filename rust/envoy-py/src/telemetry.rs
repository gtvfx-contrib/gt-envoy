//! `envoy.telemetry` submodule and top-level `envoy.enable_telemetry` /
//! `envoy.disable_telemetry` / `envoy.is_telemetry_enabled` bindings.
//!
//! Wraps `envoy_core::telemetry`'s pluggable `TelemetrySink` so Python
//! callers can opt in to anonymous usage tracking (disabled by default,
//! matching envoy's closed-by-default philosophy elsewhere, e.g. the closed
//! subprocess environment) and emit custom events:
//!
//! ```python
//! envoy.enable_telemetry("http://localhost:4318/v1/traces")
//! envoy.telemetry.track("command_run", {
//!     "command": "unreal",
//!     "duration_ms": 1500,
//!     "success": True,
//! })
//! envoy.disable_telemetry()
//! ```

use std::collections::HashMap;

use envoy_core::telemetry::{self, OtlpSink, TelemetryValue};
use pyo3::exceptions::{PyTypeError, PyValueError};
use pyo3::prelude::*;
use pyo3::types::PyDict;

/// Enable telemetry export to the given OTLP/HTTP endpoint (e.g.
/// `"http://localhost:4318/v1/traces"`, the default path for an
/// OpenTelemetry Collector).
///
/// Telemetry is disabled by default; calling this installs a batched OTLP
/// sink. Call `disable_telemetry` to revert to the no-op default and stop
/// exporting.
#[pyfunction(name = "enable_telemetry")]
fn enable_telemetry(endpoint: &str) -> PyResult<()> {
    let sink = OtlpSink::new(endpoint).map_err(|error| PyValueError::new_err(error.to_string()))?;
    telemetry::enable(Box::new(sink));
    Ok(())
}

/// Disable telemetry export, reverting to the no-op default sink.
#[pyfunction(name = "disable_telemetry")]
fn disable_telemetry() {
    telemetry::disable_and_clear_flag();
}

/// Return whether telemetry export is currently enabled.
#[pyfunction(name = "is_telemetry_enabled")]
fn is_telemetry_enabled() -> bool {
    telemetry::is_enabled()
}

/// Record a custom telemetry event with the given name and attributes.
///
/// `attributes` values must be `str`, `bool`, `int`, or `float`. Silently
/// discarded when telemetry is disabled (the default).
#[pyfunction(name = "track", signature = (name, attributes=None))]
fn track(name: &str, attributes: Option<&Bound<'_, PyDict>>) -> PyResult<()> {
    let mut attrs = HashMap::new();
    if let Some(dict) = attributes {
        for (key, value) in dict.iter() {
            let key: String = key.extract()?;
            attrs.insert(key, python_value_to_telemetry_value(&value)?);
        }
    }
    telemetry::track(name, attrs);
    Ok(())
}

/// Convert a Python attribute value to a [`TelemetryValue`].
///
/// Checks `bool` before `int` since Python `bool` is a subtype of `int`
/// (`isinstance(True, int)` is `True`), so extracting as `i64` first would
/// silently turn every `True`/`False` attribute into `1`/`0`.
fn python_value_to_telemetry_value(value: &Bound<'_, PyAny>) -> PyResult<TelemetryValue> {
    if let Ok(flag) = value.extract::<bool>() {
        return Ok(TelemetryValue::Bool(flag));
    }
    if let Ok(number) = value.extract::<i64>() {
        return Ok(TelemetryValue::Int(number));
    }
    if let Ok(number) = value.extract::<f64>() {
        return Ok(TelemetryValue::Float(number));
    }
    if let Ok(text) = value.extract::<String>() {
        return Ok(TelemetryValue::Str(text));
    }

    Err(PyTypeError::new_err(
        "telemetry attribute values must be str, bool, int, or float",
    ))
}

/// Register the top-level telemetry functions and the `envoy.telemetry`
/// submodule.
pub fn register_telemetry_module(py: Python<'_>, parent: &Bound<'_, PyModule>) -> PyResult<()> {
    parent.add_function(wrap_pyfunction!(enable_telemetry, parent)?)?;
    parent.add_function(wrap_pyfunction!(disable_telemetry, parent)?)?;
    parent.add_function(wrap_pyfunction!(is_telemetry_enabled, parent)?)?;

    let module = PyModule::new_bound(py, "envoy.telemetry")?;
    module.add_function(wrap_pyfunction!(track, &module)?)?;
    parent.add("telemetry", module.clone())?;
    parent.add_submodule(&module)?;
    Ok(())
}
