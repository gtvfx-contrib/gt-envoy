# `envoy-py` Python contract tests

These files run the subset of `py/envoy/test_bundle`'s pytest suite that
exercises only the **public** `py/envoy/__init__.py` API surface, against the
compiled `envoy-py` (PyO3/maturin) wheel instead of the pure-Python
`py/envoy` source. They exist to prove behavioral parity for the API surface
that real consumers (`gt/globals`, `gt/devtools`, `gt/krita`, `gt/unreal`)
actually depend on.

## Running

```powershell
cd rust/envoy-py
python -m venv .venv-wheel-test
.venv-wheel-test\Scripts\python.exe -m pip install maturin pytest
# python3XX.dll's directory must be on PATH for the PyO3 extension to load.
maturin develop --release  # builds + installs the wheel into the active venv
.venv-wheel-test\Scripts\python.exe -m pytest tests/python_contract -v
```

## Scope decision: private-internal tests are out of scope

A large portion of `py/envoy/test_bundle`'s "real" pytest files (per the
migration plan: `test_commands.py`, `test_default_operator.py`,
`test_null_values.py`, `test_environment_allowlist.py`, `test_cli.py`,
`test_discovery.py`, `test_wrapper.py`, `test_proc.py`) test **private
implementation internals** directly — e.g. `envoy._commands.CommandRegistry`'s
private `_commands` dict, `envoy._cli._normalizeArgv`/`runCommand`/
`showCommandInfo`/`showWhich`, `envoy._environment.EnvironmentManager`,
`envoy.proc._collectEnvFiles`/`_loadRegistry`/`_resolveEnvoyExe`. These
modules/functions have no equivalent in the compiled PyO3 wheel by design —
only the public surface listed in `py/envoy/__init__.py.__all__` was ported
(see `rust/envoy-py/src/lib.rs`'s module doc comment).

Per an explicit scope decision, this directory **only** adapts and runs the
subset of each file that exercises the public contract; tests requiring
private internals are treated as **not applicable** to the wheel. Equivalent
behavior for those internals is already covered by `envoy-core`/`envoy-py`'s
own Rust unit tests (`cargo test`), many of which are named/documented to
assert parity with the original Python behavior (e.g.
`config_registry::tests::known_settings_matches_python_shape`,
`exceptions::tests::exception_hierarchy_matches_python_shape`).

| Source file | Status here |
| --- | --- |
| `test_wrapper.py` | Copied unmodified — 100% public API (`ApplicationWrapper`, `WrapperConfig`, `ExecutionResult`, `ExecutionError`, `createWrapper`). |
| `test_proc.py` | Adapted — kept `Environment`/`proc.*` free-function tests (public); dropped `TestLoadRegistry`, `TestCollectEnvFiles` (private `_loadRegistry`/`_collectEnvFiles`) and `test_resolve_envoy_exe_returns_list` (private `_resolveEnvoyExe`). |
| `test_discovery.py` | Rewritten with self-contained `tmp_path` fixtures using public `loadBundlesFromConfig`/`getBundles`; dropped `test_validation` (private `validateBundle`, no public equivalent). |
| `test_default_operator.py` | Kept only `test_default_operator_via_wrapper` (public `ApplicationWrapper`/`WrapperConfig` end-to-end); dropped 5 tests using private `EnvironmentManager` directly. |
| `test_commands.py` | N/A — entirely private (`CommandRegistry._commands`, `resolveEnvironment` internals). |
| `test_cli.py` | N/A — entirely private (`_normalizeArgv`, `runCommand`, `showCommandInfo`, `showWhich`); the public `cli_main()` binding is instead covered by Rust tests in `rust/envoy-py/src/cli.rs`. |
| `test_null_values.py` | N/A — entirely private (`EnvironmentManager.loadEnvFromFiles`). |
| `test_environment_allowlist.py` | N/A — entirely private (`EnvironmentManager`). |
| `test_expansion.py`, `test_list_paths.py`, `test_operators.py`, `test_special_vars.py` | N/A — standalone demo scripts, not pytest tests (no `def test_*` functions), per the original migration plan. |

## Known pre-existing failures

`test_wrapper.py::test_timeout` (and related timeout/process-termination
tests) may fail with `WinError 6` in some environments — this is a
pre-existing, environment-specific issue confirmed present against
`py/envoy` itself (unrelated to the Rust port). Do not treat as a new
regression.

## Real parity bugs found and fixed by this test suite

Running these tests against the freshly built wheel surfaced two genuine
`envoy-py` regressions (not environment-specific), both since fixed:

1. **`WrapperConfig` had no Python-facing getters/setters at all.** The
   original `py/envoy/_models.py` `WrapperConfig` is a `@dataclass`, so every
   field is freely readable/writable after construction (e.g.
   `config.raise_on_error = True`). The PyO3 `WrapperConfig` pyclass exposed
   none of that — not even read access. Fixed in `rust/envoy-py/src/wrapper.rs`
   by adding `#[pyo3(get, set)]` to scalar fields and explicit
   `#[getter]`/`#[setter]` pairs (with callable validation preserved) for
   `executable`, `env_files`, `cwd`, and the `preRun`/`postRun`/`onStart`/
   `onOutput`/`onError` callbacks (which keep their camelCase Python-facing
   names via `#[getter(preRun)]`/`#[setter(preRun)]` etc.).
2. **`Environment.build()` was not actually idempotent.** `py/envoy/proc.py`'s
   `Environment.build()` caches `self._env` and returns the *same* dict
   object on repeated calls. The Rust port cached the underlying Rust
   `HashMap` but constructed a brand-new Python `dict` on every `build()`
   call, breaking object identity (`first is second`). Fixed in
   `rust/envoy-py/src/proc.rs` by adding a `built_dict: Mutex<Option<Py<PyDict>>>`
   cache to `Environment`, populated once and returned by reference thereafter.

Both fixes are also covered by new Rust unit tests
(`wrapper::tests::wrapper_config_fields_are_mutable_from_python`,
`proc::tests::build_returns_same_python_dict_object_on_repeated_calls`).

## Accepted, documented differences (not bugs)

`envoy.proc.spawn()`/`Environment.spawn()` return `envoy.proc.PyPopen`, a
Popen-*compatible* (duck-typed: `.wait()`, `.communicate()`, `.pid`,
`.returncode`, `.stdin`/`.stdout`/`.stderr`) object — not a real
`subprocess.Popen` instance, so `isinstance(p, subprocess.Popen)` (as the
original `py/envoy` test asserted, since its `spawn()` really did return a
stdlib `Popen`) is `False` against the wheel. Real consumers (`gt/globals`,
`gt/devtools`, `gt/krita`, `gt/unreal`) were checked and only use the
duck-typed surface, never `isinstance`, so this is accepted as a documented
difference rather than fixed — genuinely subclassing `subprocess.Popen` from
a PyO3 type is fragile/platform-specific (would require bypassing
`Popen.__init__`'s own process-spawning and manually wiring CPython-internal
attributes). The adapted tests here assert the duck-typed contract instead.

