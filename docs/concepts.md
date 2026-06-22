# Core Concepts

## Bundles

A **bundle** is a Git repository containing an `.envoy/` directory. Each bundle can define commands and environment files.

```
my-bundle/
├── .git/
├── .envoy/
│   ├── commands.json       ← command definitions
│   ├── global_env.json     ← loaded before every command's env files
│   └── tool_env.json       ← per-tool environment
└── src/
```

Bundles are the unit of distribution in the envoy ecosystem. A developer checks out one or more bundles under a shared root, and envoy discovers all of them automatically via `ENVOY_BNDL_ROOTS`.

### Bundle ID (bndlid)

Each bundle has a **bundle ID** (`bndlid`) of the form `<namespace>:<name>`. The namespace is inferred from the parent directory name:

```mermaid
graph TD
    ROOT["R:\\repo\\"]
    NS["gtvfx-contrib\\ ← namespace: gt"]
    GT["gt\\"]
    G["globals\\ → gt:globals"]
    E["envoy\\  → gt:envoy"]
    P["pythoncore\\ → gt:pythoncore"]

    ROOT --> NS --> GT --> G
    GT --> E
    GT --> P
```

The default namespace is `gt`. It can be overridden via the Python API.

## Commands

Commands are defined in `.envoy/commands.json`:

```json
{
    "python_dev": {
        "environment": ["python_env.json"],
        "alias": ["python", "-X", "dev"]
    },
    "unreal": {
        "environment": ["unreal_env.json"]
    },
    "build": {
        "environment": ["build_env.json"],
        "alias": ["make", "build"]
    }
}
```

| Field | Required | Description |
|---|---|---|
| `environment` | Yes | List of env JSON files to load (relative to `.envoy/`) |
| `alias` | No | Executable + base args. `alias[0]` is the exe; `alias[1:]` are prepended args. If omitted, the command name is used as the executable. |

### Running Commands

```powershell
en python_dev script.py --arg value
en unreal MyGame.uproject
en build --target Release
```

Anything after the command name is passed through verbatim to the child process.

## Environment Files

Environment files are JSON files that define variable assignments. They use **operator prefixes** on keys to control how values are applied:

| Key syntax | Effect |
|---|---|
| `"VAR": "value"` | Assign / replace |
| `"+=VAR": "value"` | Append to existing, separated by the OS path separator |
| `"^=VAR": "value"` | Prepend to existing, separated by the OS path separator |
| `"?=VAR": "value"` | Set only if `VAR` is not already defined (default / fallback) |

**List values** — A JSON array is joined with the OS path separator:

```json
{
    "PYTHONPATH": [
        "${__BUNDLE__}/py",
        "${__BUNDLE__}/vendor"
    ]
}
```

**Variable expansion** — Use `${VARNAME}` to reference variables already in scope:

```json
{
    "APP_ROOT": "${__BUNDLE__}",
    "APP_CONFIG": "${APP_ROOT}/config/dev.json",
    "+=PYTHONPATH": "${__BUNDLE__}/src"
}
```

See [Environment Files](env-files.md) for the full reference.
