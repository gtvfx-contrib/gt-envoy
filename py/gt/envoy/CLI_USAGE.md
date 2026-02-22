# CLI Usage Guide

## Overview

Envoy is invoked via the `envoy` command (provided by `bin/envoy.bat`). Commands are discovered automatically from Git repositories on `ENVOY_BNDL_ROOTS`, or loaded from a local `envoy_env/commands.json`.

## Invocation

```powershell
# With bin/ on PATH
envoy [options] [command] [args ...]

# Or directly
R:\path\to\envoy\bin\envoy.bat [options] [command] [args ...]
```

## Options Reference

| Flag | Short | Description |
|---|---|---|
| `--list` | | List all available commands |
| `--info COMMAND` | | Show detailed information about a command |
| `--which COMMAND` | | Resolve the executable path for a command |
| `--commands-file PATH` | `-cf` | Path to a specific `commands.json` |
| `--bundles-config PATH` | `-bc` | Path to a bundles config file |
| `--inherit-env` | `-ie` | Inherit the full system environment (overrides closed mode) |
| `--verbose` | `-v` | Enable verbose logging |
| `--help` | `-h` | Show help message |

## Command Definition

Commands are defined in `envoy_env/commands.json`:

```json
{
    "command_name": {
        "environment": ["env1.json", "env2.json"],
        "alias": ["executable", "arg1", "arg2"]
    }
}
```

### Fields

- **`environment`** — List of JSON environment files to load (relative to `envoy_env/`)
- **`alias`** (optional) — Executable and base arguments to run
  - `alias[0]` is the executable, `alias[1:]` are prepended arguments
  - If omitted, `command_name` is used as the executable (must be on the subprocess `PATH`)

### Example

```json
{
    "python_dev": {
        "environment": ["python_env.json"],
        "alias": ["python", "-X", "dev"]
    },
    "unreal": {
        "environment": ["unreal_env.json"]
    },
    "vscode": {
        "environment": ["vscode_env.json"],
        "alias": ["C:/Users/me/AppData/Local/Programs/Microsoft VS Code/Code.exe", "--wait"]
    }
}
```

## Common Usage

### List available commands

```powershell
envoy --list
```

Output:
```
Available commands:

  python_dev           → python -X dev  [pythoncore]
  unreal               (executable on PATH)  [unreal]
  vscode               → C:/.../Code.exe --wait  [globals]
```

### Show command details

```powershell
envoy --info unreal
```

### Resolve executable path

```powershell
envoy --which unreal
```

Resolves the executable using the subprocess `PATH` built from the command's env files — the same PATH the child process will actually see.

### Run a command

```powershell
envoy python_dev script.py --arg value
envoy unreal
envoy vscode .
```

### Run with verbose logging

```powershell
envoy --verbose python_dev script.py
```

Logs bundle discovery, env file loading, environment contents, and executable resolution.

### Run with inherit-env (inherit system env)

```powershell
envoy --inherit-env python_dev script.py
envoy -ie python_dev script.py
```

## Bundle Discovery

### Auto-discovery via `ENVOY_BNDL_ROOTS`

```powershell
$env:ENVOY_BNDL_ROOTS = "R:/repo/gtvfx-contrib;R:/repo/gtvfx"
envoy --list
```

Envoy scans each root for Git repositories containing `envoy_env/` and loads their `commands.json`.

### Config file

```powershell
envoy --bundles-config R:/repo/bundles.json --list
envoy -bc R:/repo/bundles.json --list
```

**`bundles.json`:**
```json
{
    "bundles": [
        "R:/repo/gtvfx-contrib/gt/unreal/wrapper",
        "R:/repo/gtvfx-contrib/gt/globals"
    ]
}
```

### Explicit commands file

```powershell
envoy --commands-file R:/repo/my-tool/envoy_env/commands.json --list
envoy -cf R:/repo/my-tool/envoy_env/commands.json my_command
```

### Local fallback

If no bundles are found and no flags are given, Envoy searches for `envoy_env/commands.json` starting from the current directory and walking up to the filesystem root.

## Environment Modes

### Closed mode (default)

The subprocess receives only:

1. **Core OS variables** — `USERPROFILE`, `APPDATA`, `LOCALAPPDATA`, `TEMP`, `SystemRoot`, `COMPUTERNAME`, `HOME`, `LANG`, etc.
2. **User allowlist** — variables named in `ENVOY_ALLOWLIST`
3. **Bundle env files** — `global_env.json` (if present) and command-specific env files

### Inherit-env mode

```powershell
envoy --inherit-env my_command
envoy -ie my_command
```

The full system environment is inherited, with bundle env files layered on top.

### Allowlist

```powershell
$env:ENVOY_ALLOWLIST = "MY_STUDIO_VAR;ANOTHER_VAR"
envoy my_command
```

Lets specific system variables through in closed mode without full inherit-env. Supports `;` and `,` as separators.

## `global_env.json`

Any bundle can place a `global_env.json` in its `envoy_env/` directory. It is loaded automatically before command-specific env files for every command sourced from that bundle:

```
my-bundle/
└── envoy_env/
    ├── commands.json
    ├── global_env.json     ← always loaded first
    └── my_tool_env.json
```

## Error Reference

**"Could not find commands.json"**
Set `ENVOY_BNDL_ROOTS`, use `--bundles-config`/`-bc`, use `--commands-file`/`-cf`, or run from inside a project that has `envoy_env/commands.json`.

**"Command 'x' not found"**
Run `envoy --list` to see what is available.

**"Executable 'x' not found in PATH"**
In closed mode the subprocess `PATH` is built entirely from bundle env files. Ensure the bundle's env files set `+=PATH` to point at the executable's directory. Use `--which` to inspect resolution, or `--inherit-env` to temporarily use the system `PATH`.
