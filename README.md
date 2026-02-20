# Envoy

**Environment orchestration for applications** — A CLI-first tool for managing complex application environments with JSON-based configuration and multi-bundle support.

## Overview

Envoy simplifies the execution of applications that require specific environment setups. Define your commands once in JSON, specify environment variables and paths, and run them anywhere with a simple CLI interface.

### Key Features

- 🚀 **CLI-First Design** — Simple command-line interface for running applications
- 📦 **Multi-Bundle Support** — Aggregate commands from multiple Git repositories
- 🔧 **Environment Management** — JSON-based environment variable configuration
- 🔒 **Closed Environment Mode** — Subprocesses receive only what you define, not the full system environment
- 🌍 **Global Env Files** — `global_env.json` auto-loaded from every bundle before command-specific files
- 🔍 **Auto-Discovery** — Automatic bundle detection via `ENVOY_BNDL_ROOTS`
- 🎯 **Command Aliases** — Map friendly names to complex command invocations
- 🔗 **Path Normalization** — Automatic Unix-style path handling across platforms
- 📝 **Special Variables** — Built-in bundle-relative path variables
- 🐛 **Debug Mode** — Verbose logging with `--verbose`/`-v`

## Quick Start

### Installation

1. Clone the repository:
```bash
git clone <repo-url> envoy
```

2. Add the `bin` directory to your PATH, or use the full path to `envoy.bat`:
```bash
# Windows
set PATH=%PATH%;C:\path\to\envoy\bin

# Or use directly
C:\path\to\envoy\bin\envoy.bat --help
```

### Basic Usage

```bash
# List all available commands
envoy --list

# Show detailed information about a command
envoy --info python_dev

# Show where a command's executable is located
envoy --which python_dev

# Run a command
envoy python_dev script.py --arg value

# Enable verbose logging
envoy --verbose python_dev script.py
```

## Core Concepts

### 1. Bundles

A **bundle** is a Git repository containing an `envoy_env/` directory. Each bundle can define:
- Commands in `envoy_env/commands.json`
- Environment files (JSON) in `envoy_env/`

Example bundle structure:
```
my-app/
├── .git/
├── envoy_env/
│   ├── commands.json
│   ├── global_env.json     ← loaded automatically before any command env files
│   ├── base_env.json
│   └── dev_env.json
└── src/
```

### 2. Commands

Commands are defined in `envoy_env/commands.json`:

```json
{
  "python_dev": {
    "environment": ["base_env.json", "dev_env.json"],
    "alias": ["python", "-X", "dev"]
  },
  "build": {
    "environment": ["build_env.json"]
  }
}
```

- **environment**: List of environment JSON files to load (relative to `envoy_env/`)
- **alias** (optional): Command to execute. If omitted, uses the command name as executable

### 3. Environment Files

Environment files define variable assignments using JSON. Keys may carry an operator prefix to control how the value is applied.

**Operators:**

| Key syntax | Effect |
|---|---|
| `"VAR": "value"` | Assign / replace |
| `"+=VAR": "value"` | Append `value` to existing, separated by the OS path separator (`;` / `:`) |
| `"^=VAR": "value"` | Prepend `value` to existing, separated by the OS path separator |

**List values** — A JSON array is joined with the OS path separator:

```json
{
    "PYTHONPATH": [
        "{$__BUNDLE__}/py",
        "{$__BUNDLE__}/vendor"
    ]
}
```

**Variable expansion** — Use `{$VARNAME}` to reference variables already in scope. References resolve against the environment being built, not the raw system environment:

```json
{
    "+=PYTHONPATH": "{$__BUNDLE__}/src",
    "MY_APP_ROOT": "{$__BUNDLE__}",
    "DEBUG": "1"
}
```

**Special Variables:**

| Variable | Value |
|---|---|
| `{$__BUNDLE__}` | Bundle root directory (parent of `envoy_env/`) |
| `{$__BUNDLE_ENV__}` | The `envoy_env/` directory |
| `{$__BUNDLE_NAME__}` | Bundle directory name |
| `{$__FILE__}` | Full path of the current JSON file being loaded |

See [ENV_FILES_README.md](py/gt/envoy/examples/envoy_env/ENV_FILES_README.md) for detailed documentation.

## Bundle Discovery

Envoy discovers bundles in two ways:

### Auto-Discovery (Recommended)

Set `ENVOY_BNDL_ROOTS` to a semicolon-separated list of root directories:

```powershell
# PowerShell
$env:ENVOY_BNDL_ROOTS = "R:/repo/gtvfx-contrib;R:/repo/gtvfx"

# cmd
set ENVOY_BNDL_ROOTS=R:/repo/gtvfx-contrib;R:/repo/gtvfx
```

Envoy will:
1. Search for Git repositories under each root
2. Validate each has an `envoy_env/` directory
3. Load commands from `envoy_env/commands.json`

### Config File (Alternative)

Create a `bundles.json` file:

```json
{
  "bundles": [
    "R:/repo/my-app",
    "R:/repo/build-tools",
    "C:/tools/deploy-tools"
  ]
}
```

Or as a direct array:

```json
[
  "R:/repo/my-app",
  "R:/repo/build-tools",
  "C:/tools/deploy-tools"
]
```

Use with:
```bash
envoy --bundles-config bundles.json --list
envoy -bc bundles.json --list
```

See [BUNDLE_DISCOVERY.md](py/gt/envoy/BUNDLE_DISCOVERY.md) for more.

## Environment Modes

### Closed Mode (Default)

By default Envoy runs in **closed mode**: the subprocess environment contains only:

1. **Core OS variables** — always present: `USERPROFILE`, `APPDATA`, `LOCALAPPDATA`, `TEMP`, `TMP`, `SystemRoot`, `COMPUTERNAME`, `HOME`, `LANG`, and similar identity/system variables
2. **User allowlist** — additional variables named in `ENVOY_ALLOWLIST` (see below)
3. **Bundle env files** — everything defined in `global_env.json` and the command's env files

This prevents accidental dependency on the developer's machine state and makes environments fully reproducible.

### Passthrough Mode

Pass `--passthrough` (or `-pt`) to inherit the full system environment, with bundle env files layered on top:

```bash
envoy --passthrough python_dev script.py
envoy -pt python_dev script.py
```

### Allowlist

To let specific system variables through in closed mode without full passthrough, set `ENVOY_ALLOWLIST`:

```powershell
$env:ENVOY_ALLOWLIST = "MY_STUDIO_VAR;ANOTHER_VAR"
```

Supports both `;` and `,` as separators. These are merged on top of the built-in core OS variables.

## Real-World Examples

### Example 1: Python Development Environment

**`envoy_env/commands.json`:**
```json
{
    "python_dev": {
        "environment": ["python_env.json"],
        "alias": ["python", "-X", "dev"]
    }
}
```

**`envoy_env/python_env.json`:**
```json
{
    "+=PYTHONPATH": "{$__BUNDLE__}/src",
    "PYTHONDONTWRITEBYTECODE": "1",
    "PYTHONUTF8": "1"
}
```

**Usage:**
```bash
envoy python_dev script.py
```

### Example 2: Unreal Engine

**`envoy_env/commands.json`:**
```json
{
    "unreal": {
        "environment": ["unreal_env.json"]
    }
}
```

**`envoy_env/unreal_env.json`:**
```json
{
    "+=PYTHONPATH": "{$__BUNDLE__}/py",
    "+=PATH": "{$__BUNDLE__}/bin",
    "UE_BIN": "D:/Epic Games/UE_5.7/Engine/Binaries/Win64/UnrealEditor.exe"
}
```

**Usage:**
```bash
envoy unreal
envoy unreal MyGame.uproject
```

### Example 3: Multi-Bundle Setup

With `ENVOY_BNDL_ROOTS=R:/repo` and two bundles:
- `R:/repo/build-tools/envoy_env/commands.json` — defines `build`, `test`
- `R:/repo/deploy-tools/envoy_env/commands.json` — defines `deploy`, `package`

```bash
envoy --list
#   build                [build-tools]
#   test                 [build-tools]
#   deploy               [deploy-tools]
#   package              [deploy-tools]

envoy build --target Release
envoy deploy --env production
```

### Example 4: Shared Baseline via `global_env.json`

Any bundle can place a `global_env.json` in its `envoy_env/` directory. It is loaded automatically before command-specific env files for every command sourced from that bundle:

**`envoy_env/global_env.json`:**
```json
{
    "PYTHONDONTWRITEBYTECODE": "1",
    "STUDIO": "gtvfx"
}
```

## CLI Reference

```
usage: envoy [-h] [--list] [--info COMMAND] [--which COMMAND]
             [--commands-file PATH] [-cf PATH]
             [--bundles-config PATH] [-bc PATH]
             [--passthrough] [-pt]
             [--verbose] [-v]
             [command] [args ...]

Options:
  -h, --help                    Show this help message
  --list                        List all available commands
  --info COMMAND                Show detailed information about a command
  --which COMMAND               Show the resolved executable path for a command
  --commands-file, -cf PATH     Path to commands.json (auto-detected by default)
  --bundles-config, -bc PATH    Path to bundles config file
  --passthrough, -pt            Inherit the full system environment (overrides closed mode)
  --verbose, -v                 Enable verbose logging

Arguments:
  command                       Command to execute
  args                          Arguments passed through to the command
```

## Environment Variables

| Variable | Purpose |
|---|---|
| `ENVOY_BNDL_ROOTS` | Semicolon-separated root directories for bundle auto-discovery |
| `ENVOY_ALLOWLIST` | Semicolon- or comma-separated system variable names to carry through in closed mode |

## Advanced Topics

### Command Conflicts

When multiple bundles define the same command name, the last discovered bundle wins. Use `--verbose` to see conflict warnings:

```bash
envoy --verbose --list
# WARNING: Command 'build' from bundle-b overrides existing command from bundle-a
```

### Environment File Chaining

Environment files are loaded in order; later files can reference variables set by earlier ones:

**`base_env.json`:**
```json
{
    "APP_ROOT": "{$__BUNDLE__}"
}
```

**`dev_env.json`:**
```json
{
    "APP_CONFIG": "{$APP_ROOT}/config/dev.json",
    "LOG_LEVEL": "DEBUG"
}
```

### Local Fallback

If no bundles are discovered, Envoy searches for `envoy_env/commands.json` in the current directory and parent directories, allowing per-project command definitions.

## Project Structure

```
envoy/
├── bin/
│   └── envoy.bat              # CLI entry point
├── py/
│   └── gt/
│       └── envoy/
│           ├── __main__.py     # Module entry point
│           ├── _cli.py         # CLI implementation
│           ├── _commands.py    # Command registry
│           ├── _discovery.py   # Bundle discovery
│           ├── _environment.py # Environment processing
│           ├── _executor.py    # Process execution
│           ├── _wrapper.py     # Application wrapper
│           ├── _models.py      # Data models
│           ├── _exceptions.py  # Exception types
│           └── examples/
│               └── envoy_env/  # Example configurations
└── README.md
```

## Documentation

- **[CLI_USAGE.md](py/gt/envoy/CLI_USAGE.md)** — Detailed CLI usage guide
- **[BUNDLE_DISCOVERY.md](py/gt/envoy/BUNDLE_DISCOVERY.md)** — Bundle discovery system
- **[ENV_FILES_README.md](py/gt/envoy/examples/envoy_env/ENV_FILES_README.md)** — Environment file format reference

## Contributing

Envoy is part of the GT Tools collection. See `LICENSE` for details.

## Troubleshooting

**"Error: Could not find commands.json"**
- Ensure you're in a directory with `envoy_env/commands.json`, or
- Set `ENVOY_BNDL_ROOTS` to point to bundle root directories, or
- Use `--bundles-config` to specify a bundle configuration file

**Commands not appearing in --list**
- Check that bundles have `envoy_env/` directories
- Use `--verbose` to see discovery debug information
- Verify Git repositories are valid (have `.git/` directory)

**Executable not found**
- In closed mode the subprocess `PATH` comes entirely from bundle env files — ensure the bundle defines `+=PATH` pointing to the executable's directory
- Use `--which <command>` to check what path the executable resolves to against the subprocess `PATH`
- Use `--passthrough`/`-pt` temporarily to confirm the executable is found when the system `PATH` is inherited

**Environment variables not applying**
- Check JSON syntax in environment files
- Use `--verbose` to see environment loading detail
- In closed mode, only core OS vars, the allowlist, and bundle env file vars are present — `{$VARNAME}` references to unlisted system vars expand to empty string
- Use `ENVOY_ALLOWLIST` to explicitly carry through additional system variables

**Run with `--verbose` for detailed logging** of bundle discovery, command loading, environment processing, and executable resolution.
