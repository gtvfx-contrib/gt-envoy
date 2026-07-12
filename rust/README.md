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

This workspace is being built out incrementally, module-by-module, per the
migration plan. See the `todos` tracked for this effort for current status.
Until the migration completes, `py/envoy` and `py/engit` remain the
source of truth / what's actually shipped.
