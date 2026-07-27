# envoy / engit — Rust workspace

Rust re-implementation of `envoy` and `engit`, migrated module-by-module
from the retired `py/envoy` and `py/engit` packages (both fully removed —
see git history for the original Python sources). This file documents the
workspace layout for contributors.

## Crates

| Crate         | Kind                 | Replaced (Python)       | Notes |
|---------------|----------------------|--------------------------|-------|
| `envoy-core`  | lib                  | `py/envoy/_*.py` (core)  | Framework-agnostic: discovery, environment, commands, executor, wrapper, Stack registry, user config. No Python or CLI dependency. |
| `envoy-py`    | cdylib (PyO3)        | `py/envoy/__init__.py`, `proc.py`, `_api.py`, `_cli.py` | Built with `maturin`. This is now the distributed `envoy` Python package (`pip install envoy`) — preserves `import envoy`, `envoy.proc`, `envoy.testing`, `envoy.cli_main` for existing consumers. |
| `envoy-cli`   | bin (`envoy`)        | `py/envoy/_cli.py`       | Native binary, no Python runtime dependency. Replaces the PyInstaller-built `dist/envoy.exe`. `envoy-py`'s `cli_main()` binding calls into this crate's library function, so both share the same CLI dispatch logic. |
| `engit-core`  | lib                  | `py/engit/_*.py`         | Git/GitHub tooling logic. Depends on `envoy-core` for bundle discovery and named-Stack resolution. |
| `engit-cli`   | bin (`engit`)        | `py/engit/_cli.py`       | Native binary. No Python API — `engit` is CLI-only. |

## Versioning

`envoy-cli`/`engit-cli`'s `--version` output and `envoy.__version__` (in
`envoy-py`) are derived from `git describe --tags --always --dirty` at
*build time* via each crate's `build.rs`, falling back to the static
`Cargo.toml` version if `git` is unavailable (e.g. building from a
source-only tarball with no `.git/`). This mirrors `py/envoy`'s former
`hatch-vcs`-derived versioning without requiring `Cargo.toml`'s
`[workspace.package] version` (which must stay a fixed placeholder, since
Cargo requires a static valid semver there) to be hand-maintained per
release. The wheel's own static metadata version (in `pyproject.toml`
/ `rust/envoy-py/pyproject.toml`) stays `0.0.0` for the same reason — it's
`envoy.__version__` at runtime that carries the real git-derived version.

## Building

### Prerequisites

- **Rust toolchain** (`rustup` with the `x86_64-pc-windows-msvc` target)
- **Visual Studio Build Tools** (for `link.exe`)
- **Python 3.10+** — a virtual environment is recommended:
  ```powershell
  python -m venv .venv
  .venv\Scripts\Activate.ps1
  pip install maturin
  ```

### Quick build (development)

```powershell
# From the repo root:
cd rust/envoy-py
maturin develop --release
```

This compiles `envoy-py` and installs the `_envoy.pyd` extension module into
the active Python environment. Useful for iterating on Rust code without
building native binaries or publishing a wheel.

### Full build (native binaries + wheel)

```powershell
# Native binaries (envoy, engit) + core/engit libs
cargo build --workspace --exclude envoy-py --release

# Python extension wheel (the distributed `envoy` package)
cd rust/envoy-py
maturin develop --release
# or for a distributable wheel:
maturin build --release
```

The full build script `scripts\build_native.bat` runs both steps in sequence.

## Testing / linting

```powershell
cargo test --workspace --exclude envoy-py
cargo clippy --workspace --exclude envoy-py -- -D warnings
cargo fmt --check

# envoy-py itself (requires linking against a Python interpreter):
cd rust/envoy-py
cargo test --lib
cargo clippy --all-targets -- -D warnings

# Python-facing contract/consumer tests against a built wheel -- see
# rust/envoy-py/tests/python_contract/README.md and
# rust/envoy-py/tests/consumer_smoke/README.md
maturin develop --release
python -m pytest tests
```

`envoy-py` is excluded from the plain `cargo build`/`test`/`clippy` workspace
commands above because it requires linking against a Python interpreter
(via `pyo3-build-config`); build/test it separately as shown above, or
`cargo check -p envoy-py` if a Python dev environment is available.

## Migration status: complete

All modules have been ported and `envoy-py` now exposes the full
`py/envoy/__init__.py` public surface: `envoy.proc`, `envoy.testing`,
`envoy.exceptions`, the top-level `_api.py` functions, `Bundle`/
`BundleInfo`/`Stack` discovery, `CommandDefinition`/`CommandRegistry`,
the named-Stack registry, `ApplicationWrapper`/`WrapperConfig` (including
real Python callback support), and `cli_main()`. `py/envoy` and `py/engit`
have both been deleted — the root `pyproject.toml` now builds `envoy` via
the `maturin` backend from this workspace, and `engit` is native-only (no
Python package).

- Native binaries (`bin/envoy.bat`, `bin/engit.bat`) prefer the Rust builds
  (`rust/target/release/*.exe`, falling back to `dist/*.exe` in published
  bundles). `.github/workflows/build-release.yml` builds these via `cargo
  build --release` and builds the `envoy` wheel via `maturin`, both as
  release assets.
- Full parity was verified via `rust/envoy-py/tests/python_contract` (the
  subset of `py/envoy`'s original pytest suite that exercises the public
  API, run against the compiled wheel) and
  `rust/envoy-py/tests/consumer_smoke` (real `gt/globals`, `gt/devtools`,
  `gt/krita`, `gt/unreal` call patterns run against the compiled wheel).
- Cross-platform (Linux/macOS) builds of the native binaries are verified
  in CI (`build-native-cross-platform` job) but not yet distributed as
  release assets, since Linux/macOS distribution isn't an established
  workflow for this project yet (bin/*.bat, the bundle-publish zip layout,
  etc. are still Windows-oriented).

