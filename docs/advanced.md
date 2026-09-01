# Advanced Topics

## Environment File Chaining

Environment files are loaded in order; later files can reference variables set by earlier ones. This enables layered configuration where a base file sets roots and a dev file builds on them:

```mermaid
sequenceDiagram
    participant L as Loader
    participant B as base_env.json
    participant D as dev_env.json
    participant E as Environment

    L->>B: load
    B-->>E: APP_ROOT = "${__BUNDLE__}"
    B-->>E: LOG_LEVEL = "WARNING"
    L->>D: load
    D-->>E: APP_CONFIG = "${APP_ROOT}/config/dev.json"
    Note over E: APP_ROOT already set — expands correctly
    D-->>E: LOG_LEVEL = "DEBUG" (overrides WARNING)
```

**`base_env.json`:**
```json
{
    "APP_ROOT": "${__BUNDLE__}"
}
```

**`dev_env.json`:**
```json
{
    "APP_CONFIG": "${APP_ROOT}/config/dev.json",
    "LOG_LEVEL": "DEBUG"
}
```

## Command Conflicts

When multiple bundles define the same command name, the **last discovered bundle wins**. Bundle discovery order is:

1. Order of entries in `--stack` / `-s` (top-to-bottom)
2. Order of roots in `ENVOY_BNDL_ROOTS` (left-to-right), then alphabetical within each root

```
WARNING - Command 'python' from gt:bundle-b overrides existing command from gt:bundle-a
```

Run `en --verbose --list` to see these warnings and identify the conflict source.

## Local Fallback

If no bundles are found through auto-discovery or a config file, Envoy walks up from the current working directory looking for `.envoy/commands.json`:

```mermaid
flowchart TD
    CWD["Current directory\nR:/project/src/module"] --> C1{".envoy/\ncommands.json?"}
    C1 -- No --> P1["R:/project/src"]
    P1 --> C2{".envoy/\ncommands.json?"}
    C2 -- No --> P2["R:/project"]
    P2 --> C3{".envoy/\ncommands.json?"}
    C3 -- Yes --> FOUND["Commands loaded\n(single-bundle mode)"]
    C3 -- No --> P3["... continues to root"]
```

This allows running envoy from inside a project without any environment setup.

## `--trace` — Variable Mutation Debugging

Use `--trace VAR` to see exactly how a variable is built through each env file:

```powershell
en --trace PYTHONPATH python
```

Shows each assignment/append/prepend as it happens across `global_env.json` and the command's env files — useful when a path is missing or appearing in the wrong position.

## Path Normalization

All values produced by env file processing are normalized to OS-native path separators before being applied to the subprocess environment. On Windows, forward slashes are converted to backslashes.

You can write paths with either separator in JSON files:

```json
{
    "+=PYTHONPATH": "R:/repo/my-bundle/py"
}
```

The subprocess will see `R:\repo\my-bundle\py` on Windows.

## Python API Initialization Hooks (`ENVOY_PYINIT`)

Set `ENVOY_PYINIT` to a platform-separated (`;` on Windows, `:` elsewhere)
list of directories to have `import envoy` run every `*.py` file in each
directory (non-recursive, sorted by filename) once the module has finished
initializing:

```powershell
$env:ENVOY_PYINIT = "R:/studio/envoy_pyinit;C:/dev/my_pyinit_scripts"
python -c "import envoy"
```

Scripts run after the full public API is available, so they can freely
`import envoy` themselves. A script that raises is reported to stderr and
does not prevent the remaining scripts (or the `import envoy` itself) from
completing -- treat this the same as any other best-effort extension point
in envoy (e.g. bundle cache warmup). Unset or empty, `ENVOY_PYINIT` is a
no-op.

## Bundle Publishing

The `engit publish bundle` command produces a clean, deployment-ready copy of any bundle:

```mermaid
flowchart LR
    SRC["Bundle root\n(live checkout)"]
    PUB["engit publish bundle"]
    FOLD["output/\nbundle/v1.2.3/\n  .envoy/\n  py/\n  LICENSE"]
    ZIP["bundle-v1.2.3.zip\n(internal: bundle/v1.2.3/...)"]

    SRC --> PUB
    PUB -- "default" --> FOLD
    PUB -- "--zip" --> ZIP
```

The default exclusions strip git artifacts, build outputs, and caches automatically. See
[`engit publish bundle`](https://github.com/gtvfx-envoy/envoy_utils/blob/main/docs/cli-reference/engit.md#engit-publish-bundle)
for full details.

## GitHub Actions — Reusable Bundle Publish Workflow

Simple bundles can adopt a one-file release workflow by calling the versioned
workflow from the Envoy Utils repository:

**`.github/workflows/build-release.yml`** in any bundle repo:
```yaml
name: Release

on:
  push:
    tags:
      - 'v*'

jobs:
  publish:
    uses: gtvfx-envoy/envoy_utils/.github/workflows/bundle-publish.yml@v0.1.0
    with:
      extra_excludes: 'scripts pyproject.toml'  # optional
    secrets: inherit
    permissions:
      contents: write
```

When a `v*` tag is pushed, GitHub runs `engit publish bundle --zip --output dist`
and uploads the resulting zip to the release assets automatically.
