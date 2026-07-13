#![allow(clippy::useless_conversion)]

//! PyO3 bindings for envoy's named-config registry and user-config metadata.
//!
//! This module ports the public top-level surface historically re-exported
//! from `py/envoy/_config_registry.py` and `py/envoy/_user_config.py`:
//! `NamedConfigEntry`, `CFG_ROOTS_VAR`, `USER_CONFIG_PATH`,
//! `KNOWN_SETTINGS`, and the named-config helper functions.

use std::path::{Path, PathBuf};

use crate::exceptions::envoy_error_to_pyerr;
use envoy_core::config_registry::{
    is_config_name as core_is_config_name, list_config_versions as core_list_config_versions,
    list_named_configs as core_list_named_configs, publish_config as core_publish_config,
    resolve_named_config as core_resolve_named_config, NamedConfigEntry as CoreNamedConfigEntry,
    CFG_ROOTS_VAR,
};
use envoy_core::user_config::{known_settings, user_config_path};
use pyo3::exceptions::PyTypeError;
use pyo3::prelude::*;
use pyo3::types::{PyAny, PyDict, PyList, PyModule};

/// A single named config entry discovered from ``ENVOY_CFG_ROOTS``.
///
/// Attributes:
///     name: The config name (for example ``'studio'``).
///     version: The version timestamp string without the ``.json`` suffix.
///     path: Absolute path to the resolved config JSON file.
///     cfg_root: Absolute path to the config root directory that owns the
///         entry.
#[pyclass(module = "envoy")]
struct NamedConfigEntry {
    inner: CoreNamedConfigEntry,
}

#[pymethods]
impl NamedConfigEntry {
    #[getter]
    fn name(&self) -> String {
        self.inner.name.clone()
    }

    #[getter]
    fn version(&self) -> String {
        self.inner.version.clone()
    }

    #[getter]
    fn path(&self, py: Python<'_>) -> PyResult<PyObject> {
        path_to_py_path(py, &self.inner.path)
    }

    #[getter]
    fn cfg_root(&self, py: Python<'_>) -> PyResult<PyObject> {
        path_to_py_path(py, &self.inner.cfg_root)
    }

    fn __repr__(&self) -> String {
        format!(
            "NamedConfigEntry(name='{}', version='{}', path={}, cfg_root={})",
            self.inner.name,
            self.inner.version,
            python_path_string(&self.inner.path),
            python_path_string(&self.inner.cfg_root)
        )
    }

    fn __str__(&self) -> String {
        self.__repr__()
    }
}

impl NamedConfigEntry {
    fn from_inner(inner: CoreNamedConfigEntry) -> Self {
        Self { inner }
    }
}

/// Return ``True`` if ``value`` looks like a named config rather than a path.
///
/// A value is treated as a name when it contains no path separator characters
/// (``/``, ``\\``, ``:``) and does not start with a dot. Everything else is
/// treated as a filesystem path.
///
/// Args:
///     value: The raw string from ``bundles_config`` or ``--bundles-config``.
///
/// Returns:
///     ``True`` if ``value`` is a config name; ``False`` if it looks like a
///     path.
///
/// Examples:
/// - Basic usage::
///     ```python
///     envoy.isConfigName('studio')          # True
///     envoy.isConfigName('my-config')       # True
///     envoy.isConfigName('/path/to/f.json') # False
///     envoy.isConfigName('R:/configs.json') # False
///     envoy.isConfigName('./relative.json') # False
///     ```
#[pyfunction(name = "isConfigName")]
fn is_config_name(value: &str) -> bool {
    core_is_config_name(value)
}

/// Resolve a named config to the path of its latest version.
///
/// Searches each directory in ``ENVOY_CFG_ROOTS`` for a subdirectory named
/// ``name`` that contains a ``latest`` pointer file. Returns the first match.
///
/// Args:
///     name: Config name to resolve (for example ``'studio'``).
///
/// Returns:
///     Absolute path to the latest config JSON file, or ``None`` if not found.
#[pyfunction(name = "resolveNamedConfig")]
fn resolve_named_config(py: Python<'_>, name: &str) -> PyResult<Option<PyObject>> {
    core_resolve_named_config(name)
        .map(|path| path_to_py_path(py, &path))
        .transpose()
}

/// List all available named configs across all ``ENVOY_CFG_ROOTS`` roots.
///
/// Scans each config root for named subdirectories that have a ``latest``
/// pointer file. Deduplicates by name — the first root that defines a given
/// name wins, matching :func:`resolveNamedConfig`.
///
/// Returns:
///     List of :class:`NamedConfigEntry` objects, sorted by name.
#[pyfunction(name = "listNamedConfigs")]
fn list_named_configs(py: Python<'_>) -> PyResult<Vec<Py<NamedConfigEntry>>> {
    core_list_named_configs()
        .into_iter()
        .map(|entry| Py::new(py, NamedConfigEntry::from_inner(entry)))
        .collect()
}

/// List all published versions of a named config, newest first.
///
/// Args:
///     name: Config name (for example ``'studio'``).
///
/// Returns:
///     List of ``(version_string, absolute_path)`` tuples, newest first.
///     Returns an empty list if the name is not found in any root.
#[pyfunction(name = "listConfigVersions")]
fn list_config_versions(py: Python<'_>, name: &str) -> PyResult<Vec<(String, PyObject)>> {
    core_list_config_versions(name)
        .into_iter()
        .map(|(version, path)| Ok((version, path_to_py_path(py, &path)?)))
        .collect()
}

/// Publish a new version of a named config.
///
/// Copies ``source_path`` into ``<cfg_root>/<name>/<timestamp>.json`` and
/// updates the ``<cfg_root>/<name>/latest`` pointer file.
///
/// Args:
///     cfg_root: Root directory for config storage.
///     name: Config name (for example ``'studio'``).
///     source_path: Path to the source bundles-config JSON file.
///     dry_run: If ``True``, print what would happen and return without
///         writing.
///
/// Returns:
///     The absolute path of the newly written config file.
///
/// Raises:
///     ValidationError: If ``source_path`` does not exist or is not a file.
///     EnvironmentBuildError: If the destination directory or files cannot be
///         written.
#[pyfunction(name = "publishConfig", signature = (cfg_root, name, source_path, *, dry_run=false))]
fn publish_config(
    py: Python<'_>,
    cfg_root: &Bound<'_, PyAny>,
    name: &str,
    source_path: &Bound<'_, PyAny>,
    dry_run: bool,
) -> PyResult<PyObject> {
    let cfg_root = path_like_to_pathbuf(cfg_root)?;
    let source_path = path_like_to_pathbuf(source_path)?;
    let published_path = core_publish_config(&cfg_root, name, &source_path, dry_run)
        .map_err(envoy_error_to_pyerr)?;

    path_to_py_path(py, &published_path)
}

pub fn register_config_registry_bindings(py: Python<'_>, m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add_class::<NamedConfigEntry>()?;
    m.add("CFG_ROOTS_VAR", CFG_ROOTS_VAR)?;
    m.add(
        "USER_CONFIG_PATH",
        path_to_py_path(py, &user_config_path())?,
    )?;
    m.add("KNOWN_SETTINGS", build_known_settings_dict(py)?)?;
    m.add_function(wrap_pyfunction!(is_config_name, m)?)?;
    m.add_function(wrap_pyfunction!(resolve_named_config, m)?)?;
    m.add_function(wrap_pyfunction!(list_named_configs, m)?)?;
    m.add_function(wrap_pyfunction!(list_config_versions, m)?)?;
    m.add_function(wrap_pyfunction!(publish_config, m)?)?;
    Ok(())
}

fn build_known_settings_dict(py: Python<'_>) -> PyResult<Bound<'_, PyDict>> {
    let settings = PyDict::new_bound(py);

    for (name, setting) in known_settings() {
        let setting_dict = PyDict::new_bound(py);
        setting_dict.set_item("description", setting.description)?;

        match setting.choices {
            Some(choices) => setting_dict.set_item("choices", PyList::new_bound(py, choices))?,
            None => setting_dict.set_item("choices", py.None())?,
        }

        settings.set_item(name, setting_dict)?;
    }

    Ok(settings)
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
        .call1((python_path_string(path),))?
        .into_any()
        .unbind())
}

fn python_path_string(path: &Path) -> String {
    #[cfg(windows)]
    {
        let text = path.to_string_lossy();
        if let Some(stripped) = text.strip_prefix(r"\\?\UNC\") {
            return format!(r"\\{stripped}");
        }
        if let Some(stripped) = text.strip_prefix(r"\\?\") {
            return stripped.to_string();
        }

        text.into_owned()
    }

    #[cfg(not(windows))]
    {
        path.to_string_lossy().into_owned()
    }
}

#[cfg(test)]
mod tests {
    use super::{register_config_registry_bindings, CFG_ROOTS_VAR};
    use pyo3::prelude::*;
    use pyo3::types::PyDict;
    use std::env;
    use std::ffi::OsString;
    use std::sync::{LazyLock, Mutex};

    static ENV_LOCK: LazyLock<Mutex<()>> = LazyLock::new(|| Mutex::new(()));

    struct EnvVarGuard {
        name: &'static str,
        previous: Option<OsString>,
    }

    impl EnvVarGuard {
        fn set(name: &'static str, value: &str) -> Self {
            let previous = env::var_os(name);
            env::set_var(name, value);

            Self { name, previous }
        }
    }

    impl Drop for EnvVarGuard {
        fn drop(&mut self) {
            match &self.previous {
                Some(value) => env::set_var(self.name, value),
                None => env::remove_var(self.name),
            }
        }
    }

    fn with_python<T>(test_fn: impl FnOnce(Python<'_>) -> T) -> T {
        pyo3::prepare_freethreaded_python();
        Python::with_gil(test_fn)
    }

    #[test]
    fn known_settings_matches_python_shape() {
        with_python(|py| {
            let module = PyModule::new_bound(py, "envoy._envoy_test")
                .expect("test module should be created");
            register_config_registry_bindings(py, &module)
                .expect("config registry bindings should register");

            let settings_obj = module
                .getattr("KNOWN_SETTINGS")
                .expect("KNOWN_SETTINGS should exist");
            let settings = settings_obj
                .downcast::<PyDict>()
                .expect("KNOWN_SETTINGS should be a dict");

            let bundles_config_obj = settings
                .get_item("bundles_config")
                .expect("dict lookup should succeed")
                .expect("bundles_config should exist");
            let bundles_config = bundles_config_obj
                .downcast::<PyDict>()
                .expect("bundles_config entry should be a dict");
            let verbosity_obj = settings
                .get_item("verbosity")
                .expect("dict lookup should succeed")
                .expect("verbosity should exist");
            let verbosity = verbosity_obj
                .downcast::<PyDict>()
                .expect("verbosity entry should be a dict");

            let bundles_choices = bundles_config
                .get_item("choices")
                .expect("dict lookup should succeed")
                .expect("choices should exist");
            assert!(bundles_choices.is_none());
            assert_eq!(
                verbosity
                    .get_item("choices")
                    .expect("dict lookup should succeed")
                    .expect("choices should exist")
                    .extract::<Vec<String>>()
                    .expect("choices should be a string list"),
                vec!["quiet", "normal", "verbose"]
            );
        });
    }

    #[test]
    fn user_config_path_is_frozen_when_bindings_register() {
        let _lock = ENV_LOCK.lock().unwrap_or_else(|poison| poison.into_inner());
        let _guard = EnvVarGuard::set(
            "ENVOY_USER_CONFIG",
            r"C:\envoy-tests\first\user_config.json",
        );

        with_python(|py| {
            let module = PyModule::new_bound(py, "envoy._envoy_test")
                .expect("test module should be created");
            register_config_registry_bindings(py, &module)
                .expect("config registry bindings should register");

            env::set_var(
                "ENVOY_USER_CONFIG",
                r"C:\envoy-tests\second\user_config.json",
            );

            let frozen_path: String = module
                .getattr("USER_CONFIG_PATH")
                .expect("USER_CONFIG_PATH should exist")
                .call_method0("__fspath__")
                .expect("__fspath__ should exist")
                .extract()
                .expect("USER_CONFIG_PATH should be path-like");
            let cfg_roots_var: String = module
                .getattr("CFG_ROOTS_VAR")
                .expect("CFG_ROOTS_VAR should exist")
                .extract()
                .expect("CFG_ROOTS_VAR should be str");

            assert_eq!(cfg_roots_var, CFG_ROOTS_VAR);
            assert_eq!(frozen_path, r"C:\envoy-tests\first\user_config.json");
        });
    }
}
