#![allow(clippy::useless_conversion)]

//! Top-level `envoy` API bindings ported from `py/envoy/_api.py`.
//!
//! This module exposes the small convenience surface that historically lived
//! at `envoy.getEnvironment`, `envoy.getAllowlist`, `envoy.traceEnvironment`,
//! `envoy.setApiVerbosity`, `envoy.loadUserConfig`, and
//! `envoy.getCurrentBundleConfig`.

use std::cell::RefCell;
use std::collections::{BTreeSet, HashMap};
use std::path::{Path, PathBuf};

use envoy_core::discovery::{Bundle as CoreBundle, BundleConfig as CoreBundleConfig};
use envoy_core::environment::{
    core_env_vars, envoy_env_vars, EnvironmentManager,
    TraceAllowlistEvent as CoreTraceAllowlistEvent, TraceEvent as CoreTraceEvent,
    TraceStepEvent as CoreTraceStepEvent,
};
use envoy_core::error::EnvoyError;
use envoy_core::runtime::{collect_env_files, is_raw_path, load_registry, prepare_env};
use envoy_core::user_config::UserConfig as CoreUserConfig;
use pyo3::exceptions::{PyException, PyOSError, PyTypeError, PyValueError};
use pyo3::prelude::*;
use pyo3::types::{PyAny, PyDict, PyModule, PyTuple, PyType};

const SUPPORTED_OPERATING_SYSTEMS: &[&str] = &["Windows", "Linux", "Darwin"];

#[pyclass(module = "envoy")]
struct TraceAllowlistEvent {
    inner: CoreTraceAllowlistEvent,
}

#[pymethods]
impl TraceAllowlistEvent {
    #[getter]
    fn file_path(&self, py: Python<'_>) -> PyResult<PyObject> {
        path_to_py_path(py, &self.inner.file_path)
    }

    #[getter]
    fn var_name(&self) -> String {
        self.inner.var_name.clone()
    }

    #[getter]
    fn seeded(&self) -> bool {
        self.inner.seeded
    }

    #[getter]
    fn os_value(&self) -> String {
        self.inner.os_value.clone()
    }

    #[getter]
    fn already_set(&self) -> bool {
        self.inner.already_set
    }

    fn __repr__(&self) -> String {
        format!(
            "TraceAllowlistEvent(file_path='{}', var_name='{}', seeded={}, os_value='{}', \
already_set={})",
            self.inner.file_path.display(),
            self.inner.var_name,
            self.inner.seeded,
            self.inner.os_value,
            self.inner.already_set
        )
    }

    fn __str__(&self) -> String {
        self.__repr__()
    }
}

impl TraceAllowlistEvent {
    fn from_inner(inner: CoreTraceAllowlistEvent) -> Self {
        Self { inner }
    }
}

#[pyclass(module = "envoy")]
struct TraceStepEvent {
    inner: CoreTraceStepEvent,
}

#[pymethods]
impl TraceStepEvent {
    #[getter]
    fn file_path(&self, py: Python<'_>) -> PyResult<PyObject> {
        path_to_py_path(py, &self.inner.file_path)
    }

    #[getter]
    fn var_name(&self) -> String {
        self.inner.var_name.clone()
    }

    #[getter]
    fn operator(&self) -> String {
        self.inner.operator.clone()
    }

    #[getter]
    fn raw_value(&self) -> String {
        self.inner.raw_value.clone()
    }

    #[getter]
    fn expanded_value(&self) -> String {
        self.inner.expanded_value.clone()
    }

    #[getter]
    fn value_before(&self) -> String {
        self.inner.value_before.clone()
    }

    #[getter]
    fn value_after(&self) -> String {
        self.inner.value_after.clone()
    }

    #[getter]
    fn was_applied(&self) -> bool {
        self.inner.was_applied
    }

    fn __repr__(&self) -> String {
        format!(
            "TraceStepEvent(file_path='{}', var_name='{}', operator='{}', raw_value='{}', \
expanded_value='{}', value_before='{}', value_after='{}', was_applied={})",
            self.inner.file_path.display(),
            self.inner.var_name,
            self.inner.operator,
            self.inner.raw_value,
            self.inner.expanded_value,
            self.inner.value_before,
            self.inner.value_after,
            self.inner.was_applied
        )
    }

    fn __str__(&self) -> String {
        self.__repr__()
    }
}

impl TraceStepEvent {
    fn from_inner(inner: CoreTraceStepEvent) -> Self {
        Self { inner }
    }
}

#[pyclass(module = "envoy")]
struct UserConfig {
    inner: RefCell<CoreUserConfig>,
}

#[pymethods]
impl UserConfig {
    #[classmethod]
    #[pyo3(signature = (path=None))]
    fn load(_cls: &Bound<'_, PyType>, path: Option<&Bound<'_, PyAny>>) -> PyResult<Self> {
        Ok(Self {
            inner: RefCell::new(CoreUserConfig::load(resolve_optional_path(path)?)),
        })
    }

    #[getter]
    fn path(&self, py: Python<'_>) -> PyResult<PyObject> {
        path_to_py_path(py, &self.inner.borrow().path)
    }

    fn save(&self) -> PyResult<()> {
        self.inner
            .borrow()
            .save()
            .map_err(user_config_save_to_pyerr)
    }

    fn get(&self, key: &str) -> Option<String> {
        self.inner.borrow().get(key).map(ToOwned::to_owned)
    }

    fn set(&self, key: &str, value: &str) -> PyResult<()> {
        self.inner
            .borrow_mut()
            .set(key, value)
            .map_err(bundle_config_to_pyerr)
    }

    fn unset(&self, key: &str) -> bool {
        self.inner.borrow_mut().unset(key)
    }

    fn items(&self) -> HashMap<String, String> {
        self.inner.borrow().items()
    }

    fn __bool__(&self) -> bool {
        !self.inner.borrow().is_empty()
    }

    fn __repr__(&self) -> String {
        self.inner.borrow().to_string()
    }

    fn __str__(&self) -> String {
        self.__repr__()
    }
}

impl UserConfig {
    fn from_inner(inner: CoreUserConfig) -> Self {
        Self {
            inner: RefCell::new(inner),
        }
    }
}

#[pyclass(module = "envoy._envoy", name = "Bundle")]
struct Bundle {
    inner: CoreBundle,
}

#[pymethods]
impl Bundle {
    #[getter]
    fn name(&self) -> String {
        self.inner.name().to_string()
    }

    #[getter]
    fn namespace(&self) -> String {
        self.inner.namespace().to_string()
    }

    #[getter]
    fn bndlid(&self) -> String {
        self.inner.bndlid()
    }

    #[getter]
    fn version(&self) -> String {
        self.inner.version()
    }

    #[getter]
    fn is_production(&self) -> bool {
        self.inner.is_production()
    }

    #[getter]
    fn is_checkout(&self) -> bool {
        self.inner.is_checkout()
    }

    #[getter]
    fn path(&self, py: Python<'_>) -> PyResult<PyObject> {
        path_to_py_path(py, self.inner.path())
    }

    #[getter]
    fn envoy_env(&self, py: Python<'_>) -> PyResult<PyObject> {
        path_to_py_path(py, self.inner.envoy_env())
    }

    #[getter]
    fn env_files(&self, py: Python<'_>) -> PyResult<PyObject> {
        let dict = PyDict::new_bound(py);
        for (key, value) in self.inner.env_files() {
            dict.set_item(key, path_to_py_path(py, &value)?)?;
        }
        Ok(dict.into_any().unbind())
    }

    #[getter]
    fn commands(&self) -> Vec<String> {
        self.inner.commands()
    }

    fn __repr__(&self) -> String {
        format!(
            "Bundle(bndlid='{}', path={})",
            self.inner.bndlid(),
            self.inner.path().display()
        )
    }

    fn __str__(&self) -> String {
        self.__repr__()
    }
}

impl Bundle {
    fn from_inner(inner: CoreBundle) -> Self {
        Self { inner }
    }
}

#[pyclass(module = "envoy")]
struct BundleConfig {
    inner: CoreBundleConfig,
}

#[pymethods]
impl BundleConfig {
    #[new]
    fn new(path: &Bound<'_, PyAny>) -> PyResult<Self> {
        Ok(Self {
            inner: CoreBundleConfig::new(path_like_to_pathbuf(path)?)
                .map_err(bundle_config_to_pyerr)?,
        })
    }

    #[classmethod]
    #[pyo3(signature = (*, ignore_user_config=false))]
    fn current(_cls: &Bound<'_, PyType>, ignore_user_config: bool) -> PyResult<Option<Self>> {
        CoreBundleConfig::current(ignore_user_config)
            .map(|value| value.map(Self::from_inner))
            .map_err(bundle_config_to_pyerr)
    }

    #[getter]
    fn path(&self, py: Python<'_>) -> PyResult<PyObject> {
        path_to_py_path(py, self.inner.path())
    }

    #[getter]
    fn name(&self) -> Option<String> {
        self.inner.name().map(ToOwned::to_owned)
    }

    #[getter]
    fn cfg_version(&self) -> Option<String> {
        self.inner.cfg_version().map(ToOwned::to_owned)
    }

    #[getter]
    fn bundles(&self, py: Python<'_>) -> PyResult<Vec<Py<Bundle>>> {
        self.inner
            .bundles()
            .map_err(bundle_config_to_pyerr)?
            .into_iter()
            .map(|bundle| Py::new(py, Bundle::from_inner(bundle)))
            .collect()
    }

    #[getter]
    fn commands(&self) -> PyResult<Vec<String>> {
        self.inner.commands().map_err(bundle_config_to_pyerr)
    }

    fn __repr__(&self) -> String {
        self.inner.to_string()
    }

    fn __str__(&self) -> String {
        self.__repr__()
    }
}

impl BundleConfig {
    fn from_inner(inner: CoreBundleConfig) -> Self {
        Self { inner }
    }
}

/// Build and return the subprocess environment dict for `command`.
#[pyfunction(name = "getEnvironment")]
#[pyo3(signature = (command, *, inherit_env=false, allowlist=None, bundle_roots=None, commands_file=None))]
fn get_environment(
    command: &str,
    inherit_env: bool,
    allowlist: Option<Vec<String>>,
    bundle_roots: Option<Vec<String>>,
    commands_file: Option<&Bound<'_, PyAny>>,
) -> PyResult<HashMap<String, String>> {
    if is_raw_path(command) {
        let allowlist_set = allowlist_to_hashset(allowlist);
        return EnvironmentManager::new(inherit_env, allowlist_set)
            .prepare_environment(&[], None, None, None)
            .map_err(top_level_envoy_error_to_pyerr);
    }

    let commands_file = resolve_optional_path(commands_file)?;
    let (registry, bundles) = load_registry(bundle_roots.as_deref(), commands_file.as_deref())
        .map_err(top_level_envoy_error_to_pyerr)?;
    let (env, _) = prepare_env(
        command,
        &registry,
        bundles.as_deref(),
        inherit_env,
        allowlist.as_deref(),
        None,
    )
    .map_err(top_level_envoy_error_to_pyerr)?;

    Ok(env)
}

/// Return the default set of system variable names envoy seeds in closed mode.
#[pyfunction(name = "getAllowlist", signature = (extra=None))]
fn get_allowlist(py: Python<'_>, extra: Option<Vec<String>>) -> PyResult<PyObject> {
    let values = build_allowlist(extra);
    let builtins = PyModule::import_bound(py, "builtins")?;
    let frozenset = builtins.getattr("frozenset")?;

    Ok(frozenset.call1((values,))?.into_any().unbind())
}

/// Build the environment for `command` and return a trace of how `var` mutated.
#[pyfunction(name = "traceEnvironment")]
#[pyo3(signature = (command, var, *, inherit_env=false, allowlist=None, bundle_roots=None, commands_file=None))]
fn trace_environment(
    py: Python<'_>,
    command: &str,
    var: &str,
    inherit_env: bool,
    allowlist: Option<Vec<String>>,
    bundle_roots: Option<Vec<String>>,
    commands_file: Option<&Bound<'_, PyAny>>,
) -> PyResult<(HashMap<String, String>, Vec<PyObject>)> {
    let allowlist_set = allowlist_to_hashset(allowlist.clone());
    let env_manager = EnvironmentManager::new(inherit_env, allowlist_set);
    let mut trace_events = Vec::new();

    let final_env = if is_raw_path(command) {
        env_manager
            .prepare_environment(&[], None, Some(var), Some(&mut trace_events))
            .map_err(top_level_envoy_error_to_pyerr)?
    } else {
        let commands_file = resolve_optional_path(commands_file)?;
        let (registry, bundles) = load_registry(bundle_roots.as_deref(), commands_file.as_deref())
            .map_err(top_level_envoy_error_to_pyerr)?;
        let env_files = collect_env_files(command, &registry, bundles.as_deref())
            .map_err(top_level_envoy_error_to_pyerr)?;
        env_manager
            .prepare_environment(&env_files, None, Some(var), Some(&mut trace_events))
            .map_err(top_level_envoy_error_to_pyerr)?
    };

    let trace_out = trace_events
        .into_iter()
        .map(|event| trace_event_to_pyobject(py, event))
        .collect::<PyResult<Vec<_>>>()?;

    Ok((final_env, trace_out))
}

/// Set the logging verbosity for the `envoy` logger.
#[pyfunction(name = "setApiVerbosity")]
fn set_api_verbosity(level: &Bound<'_, PyAny>) -> PyResult<()> {
    let py = level.py();
    let logging = PyModule::import_bound(py, "logging")?;
    let logger = logging.getattr("getLogger")?.call1(("envoy",))?;
    logger.call_method1("setLevel", (level,))?;
    Ok(())
}

/// Load the persistent user config from disk.
#[pyfunction(name = "loadUserConfig", signature = (path=None))]
fn load_user_config(py: Python<'_>, path: Option<&Bound<'_, PyAny>>) -> PyResult<Py<UserConfig>> {
    Py::new(
        py,
        UserConfig::from_inner(CoreUserConfig::load(resolve_optional_path(path)?)),
    )
}

/// Return the active bundle config as configured by the user.
#[pyfunction(name = "getCurrentBundleConfig")]
#[pyo3(signature = (*, ignore_user_config=false))]
fn get_current_bundle_config(
    py: Python<'_>,
    ignore_user_config: bool,
) -> PyResult<Option<Py<BundleConfig>>> {
    let config = CoreBundleConfig::current(ignore_user_config)
        .map_err(bundle_config_to_pyerr)?
        .map(BundleConfig::from_inner);

    config.map(|value| Py::new(py, value)).transpose()
}

pub fn register_api_bindings(py: Python<'_>, m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add("OPERATING_SYSTEM", current_operating_system())?;
    m.add(
        "SUPPORTED_OPERATING_SYSTEMS",
        PyTuple::new_bound(py, SUPPORTED_OPERATING_SYSTEMS),
    )?;
    m.add_class::<TraceAllowlistEvent>()?;
    m.add_class::<TraceStepEvent>()?;
    m.add_class::<UserConfig>()?;
    m.add_class::<Bundle>()?;
    m.add_class::<BundleConfig>()?;
    m.add_function(wrap_pyfunction!(get_environment, m)?)?;
    m.add_function(wrap_pyfunction!(get_allowlist, m)?)?;
    m.add_function(wrap_pyfunction!(trace_environment, m)?)?;
    m.add_function(wrap_pyfunction!(set_api_verbosity, m)?)?;
    m.add_function(wrap_pyfunction!(load_user_config, m)?)?;
    m.add_function(wrap_pyfunction!(get_current_bundle_config, m)?)?;
    Ok(())
}

fn current_operating_system() -> &'static str {
    map_operating_system_name(std::env::consts::OS)
}

fn map_operating_system_name(name: &str) -> &str {
    match name {
        "windows" => "Windows",
        "linux" => "Linux",
        "macos" => "Darwin",
        other => other,
    }
}

fn build_allowlist(extra: Option<Vec<String>>) -> Vec<String> {
    let mut values = core_env_vars()
        .iter()
        .chain(envoy_env_vars().iter())
        .map(|value| (*value).to_string())
        .collect::<BTreeSet<_>>();

    if let Some(extra) = extra {
        values.extend(extra);
    }

    values.into_iter().collect()
}

fn allowlist_to_hashset(
    allowlist: Option<Vec<String>>,
) -> Option<std::collections::HashSet<String>> {
    allowlist.map(|values| values.into_iter().collect())
}

fn trace_event_to_pyobject(py: Python<'_>, event: CoreTraceEvent) -> PyResult<PyObject> {
    match event {
        CoreTraceEvent::Allowlist(event) => {
            Ok(Py::new(py, TraceAllowlistEvent::from_inner(event))?
                .into_bound(py)
                .into_any()
                .unbind())
        }
        CoreTraceEvent::Step(event) => Ok(Py::new(py, TraceStepEvent::from_inner(event))?
            .into_bound(py)
            .into_any()
            .unbind()),
    }
}

fn resolve_optional_path(value: Option<&Bound<'_, PyAny>>) -> PyResult<Option<PathBuf>> {
    value.map(path_like_to_pathbuf).transpose()
}

fn path_like_to_pathbuf(value: &Bound<'_, PyAny>) -> PyResult<PathBuf> {
    let py = value.py();
    let os = PyModule::import_bound(py, "os")?;
    let path_value = os.getattr("fspath")?.call1((value,))?;
    if let Ok(text) = path_value.extract::<String>() {
        return Ok(PathBuf::from(text));
    }
    if let Ok(bytes) = path_value.extract::<Vec<u8>>() {
        return Ok(PathBuf::from(String::from_utf8_lossy(&bytes).into_owned()));
    }

    Err(PyTypeError::new_err("Expected a path-like object"))
}

fn path_to_py_path(py: Python<'_>, path: &Path) -> PyResult<PyObject> {
    let pathlib = PyModule::import_bound(py, "pathlib")?;
    Ok(pathlib
        .getattr("Path")?
        .call1((path.to_string_lossy().into_owned(),))?
        .into_any()
        .unbind())
}

fn proc_exception(py: Python<'_>, class_name: &str, message: String) -> PyErr {
    let result = (|| -> PyResult<PyErr> {
        let module = PyModule::import_bound(py, "envoy._envoy")?;
        let proc = module.getattr("proc")?;
        let exc_type = proc.getattr(class_name)?;
        let instance = exc_type.call1((message.clone(),))?;
        Ok(PyErr::from_value_bound(instance.into_any()))
    })();

    result.unwrap_or_else(|_| PyException::new_err(message))
}

fn top_level_envoy_error_to_pyerr(error: EnvoyError) -> PyErr {
    Python::with_gil(|py| match error {
        EnvoyError::CommandNotFound(message) => proc_exception(py, "CommandNotFoundError", message),
        EnvoyError::EnvironmentBuild(message) => {
            proc_exception(py, "EnvironmentBuildError", message)
        }
        other @ (EnvoyError::Io { .. }
        | EnvoyError::Json { .. }
        | EnvoyError::PreRun(_)
        | EnvoyError::PostRun(_)
        | EnvoyError::Execution(_)
        | EnvoyError::CalledProcess { .. }) => {
            proc_exception(py, "EnvironmentBuildError", other.to_string())
        }
        EnvoyError::Validation(message) => PyValueError::new_err(message),
    })
}

fn user_config_save_to_pyerr(error: EnvoyError) -> PyErr {
    match error {
        EnvoyError::Validation(message) => PyValueError::new_err(message),
        other @ (EnvoyError::Io { .. } | EnvoyError::Json { .. }) => {
            PyOSError::new_err(other.to_string())
        }
        _ => top_level_envoy_error_to_pyerr(error),
    }
}

fn bundle_config_to_pyerr(error: EnvoyError) -> PyErr {
    match error {
        EnvoyError::Validation(message) => PyValueError::new_err(message),
        _ => top_level_envoy_error_to_pyerr(error),
    }
}

#[cfg(test)]
mod tests {
    use super::{build_allowlist, map_operating_system_name};

    #[test]
    fn os_name_mapping_matches_python_platform_system_names() {
        assert_eq!(map_operating_system_name("windows"), "Windows");
        assert_eq!(map_operating_system_name("linux"), "Linux");
        assert_eq!(map_operating_system_name("macos"), "Darwin");
    }

    #[test]
    fn unknown_os_names_pass_through() {
        assert_eq!(map_operating_system_name("freebsd"), "freebsd");
    }

    #[test]
    fn allowlist_contains_envoy_roots_and_extra_values() {
        let allowlist = build_allowlist(Some(vec![String::from("EXTRA_VAR")]));

        assert!(allowlist.contains(&String::from("ENVOY_BNDL_ROOTS")));
        assert!(allowlist.contains(&String::from("EXTRA_VAR")));
    }
}
