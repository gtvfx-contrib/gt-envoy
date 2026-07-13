#![allow(clippy::useless_conversion)]

//! PyO3 bindings for envoy command-definition loading and resolution.
//!
//! This module ports the public surface from `py/envoy/_commands.py`:
//! - `CommandDefinition`
//! - `CommandRegistry`
//! - `findCommandsFile()`
//! - the historical `Command = CommandDefinition` alias
//!
//! The underlying behavior lives in `envoy-core::commands`; these bindings
//! preserve the legacy Python API shape while forwarding the real work to the
//! Rust core implementation.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use crate::exceptions::envoy_error_to_pyerr;
use envoy_core::commands::{
    find_commands_file as core_find_commands_file, BundleLike as CoreBundleLike,
    CommandDefinition as CoreCommandDefinition, CommandRegistry as CoreCommandRegistry,
};
use pyo3::exceptions::PyTypeError;
use pyo3::prelude::*;
use pyo3::types::{PyAny, PyModule};

#[derive(Clone, Debug, Eq, PartialEq)]
struct PythonBundleLike {
    bndlid: String,
    envoy_env: PathBuf,
    name: String,
}

impl CoreBundleLike for PythonBundleLike {
    fn bndlid(&self) -> String {
        self.bndlid.clone()
    }

    fn envoy_env(&self) -> &Path {
        &self.envoy_env
    }

    fn name(&self) -> &str {
        &self.name
    }
}

/// Represents one command entry loaded from ``commands.json``.
///
/// Instances expose the same public attributes as the original Python
/// ``envoy._commands.CommandDefinition`` type. A command stores its declared
/// environment-file list, optional alias argv, originating bundle, and the
/// source paths needed to expand bundle-relative alias tokens.
#[pyclass(module = "envoy")]
struct CommandDefinition {
    inner: CoreCommandDefinition,
}

#[pymethods]
impl CommandDefinition {
    /// Initialize a command definition.
    ///
    /// Args:
    ///     name: Command name, matching the JSON key in ``commands.json``.
    ///     environment: Ordered list of environment file names or command
    ///         references.
    ///     alias: Optional argv vector that replaces ``name`` when launching
    ///         the command.
    ///     bundle: Optional bundle identifier (for example
    ///         ``'gt:pythoncore'``) describing where the command came from.
    ///     envoy_env_dir: Optional ``.envoy`` directory that owns the
    ///         referenced environment files.
    ///     source_file: Optional path to the ``commands.json`` file this
    ///         definition was loaded from.
    #[new]
    #[pyo3(signature = (name, environment, alias=None, bundle=None, envoy_env_dir=None, source_file=None))]
    fn new(
        name: String,
        environment: Vec<String>,
        alias: Option<Vec<String>>,
        bundle: Option<String>,
        envoy_env_dir: Option<&Bound<'_, PyAny>>,
        source_file: Option<&Bound<'_, PyAny>>,
    ) -> PyResult<Self> {
        Ok(Self {
            inner: CoreCommandDefinition {
                name,
                environment,
                alias,
                bundle,
                envoy_env_dir: resolve_optional_path(envoy_env_dir)?,
                source_file: resolve_optional_path(source_file)?,
            },
        })
    }

    /// Return the command name.
    #[getter]
    fn name(&self) -> String {
        self.inner.name.clone()
    }

    /// Return the declared environment list.
    ///
    /// Entries whose basename contains no dot are treated by
    /// :meth:`CommandRegistry.resolveEnvironment` as references to another
    /// command rather than as plain file names.
    #[getter]
    fn environment(&self) -> Vec<String> {
        self.inner.environment.clone()
    }

    /// Return the optional alias argv vector.
    ///
    /// When present, the first element becomes :attr:`executable` and the
    /// remaining elements become :attr:`base_args`.
    #[getter]
    fn alias(&self) -> Option<Vec<String>> {
        self.inner.alias.clone()
    }

    /// Return the optional bundle identifier that supplied this command.
    #[getter]
    fn bundle(&self) -> Option<String> {
        self.inner.bundle.clone()
    }

    /// Return the owning ``.envoy`` directory, when known.
    #[getter]
    fn envoy_env_dir(&self, py: Python<'_>) -> PyResult<PyObject> {
        optional_path_to_pyobject(py, self.inner.envoy_env_dir.as_deref())
    }

    /// Return the source ``commands.json`` file, when known.
    #[getter]
    fn source_file(&self, py: Python<'_>) -> PyResult<PyObject> {
        optional_path_to_pyobject(py, self.inner.source_file.as_deref())
    }

    /// Return the executable that should be launched for this command.
    ///
    /// This is the first alias element when an alias is defined, otherwise the
    /// command's own :attr:`name`.
    #[getter]
    fn executable(&self) -> String {
        self.inner.executable().to_string()
    }

    /// Return the alias elements that come before user-supplied arguments.
    ///
    /// This is ``alias[1:]`` when an alias exists and is longer than one
    /// element, otherwise an empty list.
    #[getter]
    fn base_args(&self) -> Vec<String> {
        self.inner.base_args().to_vec()
    }

    /// Expand the command alias into a concrete argv vector.
    ///
    /// ``${__BUNDLE__}``, ``${__BUNDLE_ENV__}``, and ``${__BUNDLE_NAME__}``
    /// are resolved from :attr:`envoy_env_dir`. Any remaining ``${VAR}``
    /// references are expanded from *env* when provided.
    ///
    /// Args:
    ///     env: Optional ``dict[str, str]`` used to expand non-special
    ///         ``${VAR}`` references.
    ///
    /// Returns:
    ///     A fully expanded argv list. When no alias is defined, the return
    ///     value is ``[self.name]``.
    #[pyo3(name = "expandAlias", signature = (env=None))]
    fn expand_alias(&self, env: Option<HashMap<String, String>>) -> Vec<String> {
        self.inner.expand_alias(env.as_ref())
    }

    fn __repr__(&self) -> String {
        self.inner.to_string()
    }

    fn __str__(&self) -> String {
        self.__repr__()
    }
}

impl CommandDefinition {
    fn from_inner(inner: CoreCommandDefinition) -> Self {
        Self { inner }
    }
}

/// Loads and resolves command definitions from one or more bundles.
///
/// The registry stores commands by name, supports the original
/// ``loadFromFile`` and ``loadFromBundles`` mutation API, and can flatten a
/// command's recursive environment references into the ordered file list envoy
/// should load.
#[pyclass(module = "envoy")]
struct CommandRegistry {
    inner: CoreCommandRegistry,
}

#[pymethods]
impl CommandRegistry {
    /// Initialize the registry and optionally load one ``commands.json`` file.
    ///
    /// Args:
    ///     commands_file: Optional path to a ``commands.json`` file to load
    ///         immediately.
    ///
    /// Raises:
    ///     EnvironmentBuildError: The file could not be read, parsed, or
    ///         validated.
    #[new]
    #[pyo3(signature = (commands_file=None))]
    fn new(commands_file: Option<&Bound<'_, PyAny>>) -> PyResult<Self> {
        let commands_path = resolve_optional_path(commands_file)?;
        Ok(Self {
            inner: CoreCommandRegistry::new(commands_path.as_deref())
                .map_err(envoy_error_to_pyerr)?,
        })
    }

    /// Load commands from a JSON file.
    ///
    /// Args:
    ///     commands_file: Path to a ``commands.json`` file.
    ///     bundle_name: Optional bundle identifier used to populate each
    ///         loaded command's :attr:`CommandDefinition.bundle` attribute.
    ///
    /// Raises:
    ///     EnvironmentBuildError: The file could not be read, parsed, or
    ///         validated.
    #[pyo3(name = "loadFromFile", signature = (commands_file, bundle_name=None))]
    fn load_from_file(
        &mut self,
        commands_file: &Bound<'_, PyAny>,
        bundle_name: Option<&str>,
    ) -> PyResult<()> {
        self.inner
            .load_from_file(&path_like_to_pathbuf(commands_file)?, bundle_name)
            .map_err(envoy_error_to_pyerr)
    }

    /// Load commands from multiple discovered bundles.
    ///
    /// Args:
    ///     bundles: Iterable of ``Bundle``/``BundleInfo``-like objects. Each
    ///         entry must expose ``.bndlid`` and ``.envoy_env`` attributes;
    ///         ``.name`` is used when present and otherwise derived.
    ///
    /// Notes:
    ///     This binding intentionally accepts bundle-like Python objects via
    ///     attribute access rather than requiring one concrete wrapper class.
    ///     That keeps the method compatible while other discovery bindings are
    ///     still landing in parallel.
    #[pyo3(name = "loadFromBundles")]
    fn load_from_bundles(&mut self, bundles: &Bound<'_, PyAny>) -> PyResult<()> {
        let bundle_values = bundles
            .iter()?
            .map(|item| item.and_then(|value| extract_bundle_like(&value)))
            .collect::<PyResult<Vec<_>>>()?;
        self.inner.load_from_bundles(&bundle_values);
        Ok(())
    }

    /// Return one command definition by name.
    ///
    /// Args:
    ///     command_name: Name of the command to look up.
    ///
    /// Returns:
    ///     The matching :class:`CommandDefinition`, or ``None`` when the
    ///     command is not registered.
    fn get(&self, py: Python<'_>, command_name: &str) -> PyResult<Option<Py<CommandDefinition>>> {
        self.inner
            .get(command_name)
            .cloned()
            .map(CommandDefinition::from_inner)
            .map(|command| Py::new(py, command))
            .transpose()
    }

    /// Return all registered command names in sorted order.
    #[pyo3(name = "listCommands")]
    fn list_commands(&self) -> Vec<String> {
        self.inner.list_commands()
    }

    /// Resolve and flatten a command's environment references.
    ///
    /// Entries without a dot in their basename are treated as references to
    /// other commands and recursively expanded in place. Entries with a dot
    /// are treated as plain file names.
    ///
    /// Args:
    ///     command_name: Command whose environment should be flattened.
    ///
    /// Returns:
    ///     A list of ``(filename, envoy_env_dir)`` pairs in load order.
    ///     ``envoy_env_dir`` is a ``pathlib.Path`` when known, otherwise
    ///     ``None``.
    ///
    /// Raises:
    ///     EnvironmentBuildError: A referenced command was missing or a cycle
    ///         was detected.
    #[pyo3(name = "resolveEnvironment")]
    fn resolve_environment(
        &self,
        py: Python<'_>,
        command_name: &str,
    ) -> PyResult<Vec<(String, PyObject)>> {
        self.inner
            .resolve_environment(command_name)
            .map_err(envoy_error_to_pyerr)?
            .into_iter()
            .map(|(file_name, envoy_env_dir)| {
                Ok((
                    file_name,
                    optional_path_to_pyobject(py, envoy_env_dir.as_deref())?,
                ))
            })
            .collect()
    }

    /// Return ``True`` when the registry contains ``command_name``.
    fn __contains__(&self, command_name: &str) -> bool {
        self.inner.contains(command_name)
    }

    /// Return the number of registered commands.
    fn __len__(&self) -> usize {
        self.inner.len()
    }
}

/// Find ``commands.json`` by checking the env-var override and then walking
/// parent directories for ``.envoy/commands.json``.
///
/// Resolution order matches the historical Python implementation:
/// 1. ``ENVOY_COMMANDS_FILE`` environment-variable override
/// 2. upward search from *start_path* or the current working directory
///
/// Args:
///     start_path: Optional starting directory for the upward search.
///
/// Returns:
///     A ``pathlib.Path`` to the discovered file, or ``None`` when no
///     candidate was found.
///
/// Raises:
///     EnvironmentBuildError: ``ENVOY_COMMANDS_FILE`` was set to an existing
///         path that is not a file, or the search could not resolve the
///         current working directory.
#[pyfunction(name = "findCommandsFile", signature = (start_path=None))]
fn find_commands_file(
    py: Python<'_>,
    start_path: Option<&Bound<'_, PyAny>>,
) -> PyResult<Option<PyObject>> {
    let start_path = resolve_optional_path(start_path)?;
    core_find_commands_file(start_path.as_deref())
        .map_err(envoy_error_to_pyerr)?
        .map(|path| path_to_py_path(py, &path))
        .transpose()
}

pub fn register_command_bindings(py: Python<'_>, m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add_class::<CommandDefinition>()?;
    m.add_class::<CommandRegistry>()?;
    m.add("Command", py.get_type_bound::<CommandDefinition>())?;
    m.add_function(wrap_pyfunction!(find_commands_file, m)?)?;
    Ok(())
}

fn extract_bundle_like(value: &Bound<'_, PyAny>) -> PyResult<PythonBundleLike> {
    let bndlid = value
        .getattr("bndlid")
        .and_then(|attr| attr.extract::<String>())
        .map_err(|_| PyTypeError::new_err("Each bundle must expose a string 'bndlid' attribute"))?;
    let envoy_env = value
        .getattr("envoy_env")
        .and_then(|attr| path_like_to_pathbuf(&attr))
        .map_err(|_| {
            PyTypeError::new_err("Each bundle must expose a path-like 'envoy_env' attribute")
        })?;
    let bundle_name = value
        .getattr("name")
        .ok()
        .and_then(|attr| attr.extract::<String>().ok())
        .filter(|text| !text.is_empty())
        .unwrap_or_else(|| derive_bundle_name(&bndlid, &envoy_env));

    Ok(PythonBundleLike {
        bndlid,
        envoy_env,
        name: bundle_name,
    })
}

fn derive_bundle_name(bndlid: &str, envoy_env: &Path) -> String {
    bndlid
        .rsplit(':')
        .next()
        .filter(|name| !name.is_empty())
        .map(ToOwned::to_owned)
        .or_else(|| {
            envoy_env
                .parent()
                .and_then(Path::file_name)
                .map(|name| name.to_string_lossy().into_owned())
        })
        .unwrap_or_default()
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

fn optional_path_to_pyobject(py: Python<'_>, path: Option<&Path>) -> PyResult<PyObject> {
    match path {
        Some(path) => path_to_py_path(py, path),
        None => Ok(py.None()),
    }
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;
    use std::env;
    use std::ffi::{OsStr, OsString};
    use std::fs;
    use std::path::{Path, PathBuf};
    use std::sync::Mutex;
    use std::time::{SystemTime, UNIX_EPOCH};

    use pyo3::types::{PyAnyMethods, PyModule, PyStringMethods};
    use pyo3::Python;
    use serde_json::json;

    use super::{
        derive_bundle_name, find_commands_file, path_to_py_path, register_command_bindings,
        CommandDefinition, CommandRegistry,
    };

    static ENV_MUTEX: Mutex<()> = Mutex::new(());

    struct EnvVarGuard {
        key: &'static str,
        previous: Option<OsString>,
    }

    impl EnvVarGuard {
        fn set(key: &'static str, value: Option<&OsStr>) -> Self {
            let previous = env::var_os(key);

            match value {
                Some(value) => env::set_var(key, value),
                None => env::remove_var(key),
            }

            Self { key, previous }
        }
    }

    impl Drop for EnvVarGuard {
        fn drop(&mut self) {
            match &self.previous {
                Some(value) => env::set_var(self.key, value),
                None => env::remove_var(self.key),
            }
        }
    }

    fn with_python<T>(test_fn: impl FnOnce(Python<'_>) -> T) -> T {
        pyo3::prepare_freethreaded_python();
        Python::with_gil(test_fn)
    }

    struct TestDir {
        path: PathBuf,
    }

    impl TestDir {
        fn path(&self) -> &Path {
            &self.path
        }
    }

    impl Drop for TestDir {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.path);
        }
    }

    fn create_test_dir(label: &str) -> TestDir {
        let timestamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system time should be after epoch")
            .as_nanos();
        let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("target")
            .join("envoy-py-tests")
            .join(format!("{label}-{}-{timestamp}", std::process::id()));
        fs::create_dir_all(&root).expect("test directory should be created");
        TestDir { path: root }
    }

    #[test]
    fn register_command_bindings_adds_expected_symbols() {
        with_python(|py| {
            let module = PyModule::new_bound(py, "envoy").expect("module should be created");

            register_command_bindings(py, &module).expect("command bindings should register");

            assert!(module.getattr("CommandDefinition").is_ok());
            assert!(module.getattr("CommandRegistry").is_ok());
            assert!(module.getattr("findCommandsFile").is_ok());

            let command_alias = module
                .getattr("Command")
                .expect("Command alias should exist");
            let command_definition = module
                .getattr("CommandDefinition")
                .expect("CommandDefinition should exist");
            assert!(command_alias.is(&command_definition));
        });
    }

    #[test]
    fn command_registry_round_trips_python_visible_objects() {
        with_python(|py| {
            let temp_dir = create_test_dir("command-registry-round-trip");
            let envoy_env_dir = temp_dir.path().join(".envoy");
            fs::create_dir_all(&envoy_env_dir).expect(".envoy should be created");
            let commands_file = envoy_env_dir.join("commands.json");
            fs::write(
                &commands_file,
                serde_json::to_string_pretty(&json!({
                    "python": {
                        "environment": ["base_env.json"],
                        "alias": ["python.exe", "-m", "pip"]
                    }
                }))
                .expect("fixture json should serialize"),
            )
            .expect("commands fixture should be written");

            let commands_path =
                path_to_py_path(py, &commands_file).expect("commands path should convert");
            let registry =
                CommandRegistry::new(Some(commands_path.bind(py))).expect("registry should load");

            assert_eq!(registry.__len__(), 1);
            assert!(registry.__contains__("python"));
            assert_eq!(registry.list_commands(), vec![String::from("python")]);

            let command = registry
                .get(py, "python")
                .expect("get should succeed")
                .expect("python command should exist");
            let command_ref = command.bind(py).borrow();

            assert_eq!(command_ref.executable(), String::from("python.exe"));
            assert_eq!(
                command_ref.base_args(),
                vec![String::from("-m"), String::from("pip")]
            );
        });
    }

    #[test]
    fn command_definition_expand_alias_uses_special_vars_and_env_values() {
        with_python(|py| {
            let temp_dir = create_test_dir("expand-alias");
            let bundle_root = temp_dir.path().join("gt").join("pythoncore");
            let envoy_env_dir = bundle_root.join(".envoy");
            fs::create_dir_all(&envoy_env_dir).expect(".envoy should be created");
            let envoy_env_path =
                path_to_py_path(py, &envoy_env_dir).expect("envoy env path should convert");

            let command = CommandDefinition::new(
                String::from("python"),
                Vec::new(),
                Some(vec![
                    String::from("${__BUNDLE__}/bin/python.exe"),
                    String::from("${__BUNDLE_ENV__}/python_env.json"),
                    String::from("${HOME}/tools"),
                ]),
                Some(String::from("gt:pythoncore")),
                Some(envoy_env_path.bind(py)),
                None,
            )
            .expect("command should construct");

            let env_values = HashMap::from([(String::from("HOME"), String::from("C:/Users/Test"))]);
            let expanded = command.expand_alias(Some(env_values));

            assert_eq!(
                expanded,
                vec![
                    bundle_root
                        .join("bin")
                        .join("python.exe")
                        .display()
                        .to_string(),
                    envoy_env_dir.join("python_env.json").display().to_string(),
                    String::from("C:\\Users\\Test\\tools"),
                ]
            );
        });
    }

    #[test]
    fn find_commands_file_honours_override_and_upward_search() {
        let _lock = ENV_MUTEX
            .lock()
            .unwrap_or_else(|poison| poison.into_inner());

        with_python(|py| {
            let temp_dir = create_test_dir("find-commands-override");
            let commands_file = temp_dir.path().join("commands.json");
            fs::write(&commands_file, "{}").expect("commands file should be written");
            let _override_guard =
                EnvVarGuard::set("ENVOY_COMMANDS_FILE", Some(commands_file.as_os_str()));

            let search_path =
                path_to_py_path(py, temp_dir.path()).expect("search path should convert");
            let found = find_commands_file(py, Some(search_path.bind(py)))
                .expect("override lookup should succeed")
                .expect("override file should be found");
            let found_path = found
                .bind(py)
                .str()
                .expect("path should stringify")
                .to_str()
                .expect("path string should be valid utf-8")
                .to_string();
            assert_eq!(found_path, commands_file.display().to_string());
        });

        with_python(|py| {
            let temp_dir = create_test_dir("find-commands-upward");
            let project_root = temp_dir.path().join("project");
            let nested = project_root.join("a").join("b").join("c");
            let envoy_env_dir = project_root.join(".envoy");
            let commands_file = envoy_env_dir.join("commands.json");
            fs::create_dir_all(&nested).expect("nested directories should be created");
            fs::create_dir_all(&envoy_env_dir).expect(".envoy should be created");
            fs::write(&commands_file, "{}").expect("commands file should be written");
            let _override_guard = EnvVarGuard::set("ENVOY_COMMANDS_FILE", None);

            let search_path = path_to_py_path(py, &nested).expect("search path should convert");
            let found = find_commands_file(py, Some(search_path.bind(py)))
                .expect("upward lookup should succeed")
                .expect("commands file should be found");
            let found_path = found
                .bind(py)
                .str()
                .expect("path should stringify")
                .to_str()
                .expect("path string should be valid utf-8")
                .to_string();
            assert_eq!(found_path, commands_file.display().to_string());
        });
    }

    #[test]
    fn derive_bundle_name_falls_back_to_envoy_env_parent() {
        assert_eq!(
            derive_bundle_name(
                "gt:pythoncore",
                Path::new("C:\\repo\\gt\\pythoncore\\.envoy")
            ),
            String::from("pythoncore")
        );
        assert_eq!(
            derive_bundle_name("pythoncore", Path::new("C:\\repo\\gt\\pythoncore\\.envoy")),
            String::from("pythoncore")
        );
    }
}
