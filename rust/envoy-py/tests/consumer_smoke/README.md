# Consumer smoke tests

Exercises the real `envoy`-calling code paths from the 4 known real
consumers of the `envoy` Python library API (not the `envoy`/`engit` CLI
binaries), against the compiled `envoy-py` wheel:

- `gt/globals/py/gt/vscode/wrapper/_wrapper.py`
- `gt/devtools/py/cleanup_branches.py`
- `gt/krita/wrapper/py/gt/krita/wrapper/__main__.py`
- `gt/unreal/wrapper/py/gt/unreal/wrapper/__main__.py`

## Running

Same setup as `tests/python_contract` (build+install the wheel into a venv
first), then:

```powershell
.venv-wheel-test\Scripts\python.exe -m pytest tests/consumer_smoke -v
```

## Findings

- **`gt/globals` vscode wrapper**: fully testable and exercised for real —
  `writeLocalStack()` calls `envoy.discoverBundlesAuto()`, writes a validated
  `.estack`, and the launch test verifies that `ENVOY_STACK` is injected into
  the VS Code child environment without mutating the wrapper process.
- **`gt/devtools/cleanup_branches.py`**: its `envoy.proc.spawn(cmd,
  pipeline='build', inheritenv=False, ...)` call passes two kwargs
  (`pipeline`, `inheritenv`) that are not real `subprocess.Popen` parameters.
  Since the free `spawn()` function forwards `**kwargs` straight to
  `subprocess.Popen` in *both* `py/envoy` and this wheel, this call raises
  `TypeError` identically in both implementations — confirmed as a
  **pre-existing bug in the consumer script**, unrelated to and unaffected
  by this migration. Full end-to-end execution of `cleanup_branches.py`
  isn't otherwise possible in this checkout anyway: it imports
  `gt.gitutils` and `gt.repl`, neither of which exist in this repository.
- **`gt/krita` / `gt/unreal` wrappers**: their `__main__.py` modules can't be
  imported standalone in this checkout — `gt/krita/wrapper/_initialize.py`
  imports `gt.pycore`, and `gt/unreal/wrapper/_initialize.py` imports
  `gt.winreg`/`gt.win32`, none of which exist in this repository (unrelated
  to this migration; likely internal packages not checked out here, or only
  available from inside the real Krita/Unreal Python environment). Instead,
  this directory reproduces their exact `envoy.proc` call patterns directly
  (`spawn(cmd, env_override=..., stdout=PIPE, stderr=PIPE, creationflags=0)`
  for Krita; `Environment(cmd, env_override=...).build()` then
  `.spawn(args, stdout=PIPE, stderr=PIPE, creationflags=0)` for Unreal) using
  a real Python subprocess as a stand-in for the actual Krita/Unreal Editor
  binary (neither of which is installed in this environment), and verifies
  `.wait()`/`.returncode`/`.stdout`/`.stderr` streaming all work exactly as
  those wrappers depend on.
- The known `krita`/`unreal` `_initialize.py` bug where
  `envoy.get_environment(...)` (snake_case, doesn't exist) is called instead
  of `getEnvironment` (camelCase, the real public API) was confirmed
  out-of-scope in an earlier planning session — it's a pre-existing bug in
  those consumers unrelated to this migration, not fixed or worked around
  here.

All consumer smoke tests pass against the compiled wheel with no wheel-specific
regressions found.
