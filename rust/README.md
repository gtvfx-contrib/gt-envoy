# envoy / engit — Rust workspace

Rust re-implementation of `envoy` and `engit`, migrated incrementally
(module-by-module) from `py/envoy` and `py/engit`. This file documents the
workspace layout for contributors; it will grow into the canonical reference
as the migration progresses.

## Crates

| Crate         | Kind                 | Replaces (Python)      | Notes |
|---------------|----------------------|-------------------------|-------|
| `envoy-core`  | lib                  | `py/envoy/_*.py` (core) | Framework-agnostic: discovery, environment, commands, executor, wrapper, config registry, user config. No Python or CLI dependency. |
| `envoy-py`    | cdylib (PyO3)        | `py/envoy/__init__.py`, `proc.py`, `_api.py` | Built with `maturin`. Preserves `import envoy`, `envoy.proc`, `envoy.testing` for existing consumers. |
| `envoy-cli`   | bin (`envoy`)        | `py/envoy/_cli.py`      | Native binary, no Python runtime dependency. Replaces the PyInstaller-built `dist/envoy.exe`. |
| `engit-core`  | lib                  | `py/engit/_*.py`        | Git/GitHub tooling logic. Depends on `envoy-core` for bundle discovery / named-config resolution. |
| `engit-cli`   | bin (`engit`)        | `py/engit/_cli.py`      | Native binary. No Python API — `engit` is CLI-only. |

## Building

```powershell
# Native binaries + core/engit libs
cargo build --workspace --exclude envoy-py --release

# Python extension wheel (envoy-py), from its own directory
cd envoy-py
python -m maturin build --release
# or, for local development against a venv:
python -m maturin develop
```

## Testing / linting

```powershell
cargo test --workspace --exclude envoy-py
cargo clippy --workspace --exclude envoy-py -- -D warnings
cargo fmt --check
```

`envoy-py` is excluded from the plain `cargo build`/`test`/`clippy` workspace
commands above because it requires linking against a Python interpreter
(via `pyo3-build-config`); build/test it with `maturin` as shown above, or
`cargo check -p envoy-py` if a Python dev environment is available.

## Migration status

`envoy-core`, `envoy-cli`, `engit-core`, and `engit-cli` are functionally
complete (full test coverage, ported module-for-module from `py/envoy`'s
core and `py/engit`). `envoy-py` (the PyO3 bindings preserving `import
envoy`) currently exposes `envoy.proc`, `envoy.testing`, `envoy.exceptions`,
and a subset of top-level functions (`getEnvironment`, `getAllowlist`,
`traceEnvironment`, `setApiVerbosity`, `loadUserConfig`,
`getCurrentBundleConfig`) — but **not yet** the full `py/envoy/__init__.py`
surface (e.g. `Bundle`, `BundleConfig` full construction, `ApplicationWrapper`,
`CommandRegistry`, `discoverBundlesAuto()`, etc., which
`gt/globals/py/gt/vscode/wrapper` calls directly today).

**Distribution status:**
- `envoy`/`engit` **native binaries** (`bin/envoy.bat`, `bin/engit.bat`) now
  prefer the Rust builds (`rust/target/release/*.exe`, falling back to
  `dist/*.exe` in published bundles, falling back to `python -m envoy`/
  `python -m engit` from `py/` in dev checkouts). `.github/workflows/
  build-release.yml` builds these via `cargo build --release`, replacing
  the old PyInstaller (`envoy.spec`) step.
- The **`envoy` Python package** (`pip install envoy`, used as a library by
  `gt/globals`, `gt/devtools`, `gt/krita`, `gt/unreal`) is still built from
  `py/envoy` (pure Python, `hatchling` backend) — **not** yet cut over to
  the `envoy-py` PyO3 wheel — because `envoy-py` doesn't have full parity
  with `py/envoy`'s public API yet. CI builds the PyO3 wheel too (via
  `maturin`) as a build-correctness check, but does not publish it as a
  release asset until parity is verified against real consumers.
- Cross-platform (Linux/macOS) builds of the native binaries are verified
  in CI (`build-native-cross-platform` job) but not yet distributed as
  release assets, since Linux/macOS distribution isn't an established
  workflow for this project yet (bin/*.bat, the bundle-publish zip layout,
  etc. are still Windows-oriented).

See the `todos` tracked for this effort for granular per-module status.
