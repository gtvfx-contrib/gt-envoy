# Envoy

**Environment orchestration for applications** — A CLI-first tool for managing complex application environments with JSON-based configuration and multi-package support.

## Overview

Envoy simplifies the execution of applications that require specific environment setups. Define your commands once in JSON, specify environment variables and paths, and run them anywhere with a simple CLI interface.

### Key Features

- 🚀 **CLI-First Design** - Simple command-line interface for running applications
- 📦 **Multi-Package Support** - Aggregate commands from multiple Git repositories
- 🔧 **Environment Management** - JSON-based environment variable configuration
- 🔍 **Auto-Discovery** - Automatic package detection via `ENVOY_PKG_ROOTS`
- 🎯 **Command Aliases** - Map friendly names to complex command invocations
- 🔗 **Path Normalization** - Automatic Unix-style path handling across platforms
- 📝 **Special Variables** - Built-in package-relative path variables
- 🐛 **Debug Mode** - Verbose logging with `--verbose` flag

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

### 1. Packages

A **package** is a Git repository containing an `envoy_env/` directory. Each package can define:
- Commands in `envoy_env/commands.json`
- Environment files (JSON) in `envoy_env/`

Example package structure:
```
my-app/
├── .git/
├── envoy_env/
│   ├── commands.json
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

Environment files define variable modifications using JSON:

```json
{
  "PYTHONPATH": "+=PATH:{$__PACKAGE__}/src",
  "MY_APP_ROOT": "{$__PACKAGE__}",
  "DEBUG": "1"
}
```

**Variable Operators:**
- `"VAR": "value"` — Replace/set variable
- `"+=PATH:VAR": "value"` — Prepend to path variable (OS-appropriate separator)
- `"+=PATHLIST:VAR": "value"` — Prepend to path list (semicolon-separated)
- `"+=VAR": "value"` — Append to variable

**Special Variables:**
- `{$__PACKAGE__}` — Package root directory (parent of `envoy_env/`)
- `{$__PACKAGE_ENV__}` — The `envoy_env/` directory
- `{$__PACKAGE_NAME__}` — Package directory name
- `{$__FILE__}` — Current environment JSON file path
- `{$VARNAME}` — Reference existing environment variables

See [ENV_FILES_README.md](py/gt/envoy/examples/envoy_env/ENV_FILES_README.md) for detailed documentation.

## Package Discovery

Envoy discovers packages in two ways:

### Auto-Discovery (Recommended)

Set the `ENVOY_PKG_ROOTS` environment variable to semicolon-separated root directories:

```bash
# Windows
set ENVOY_PKG_ROOTS=R:/repo/packages;C:/tools

# PowerShell
$env:ENVOY_PKG_ROOTS="R:/repo/packages;C:/tools"

# Unix/Linux
export ENVOY_PKG_ROOTS=/repo/packages:/tools
```

Envoy will:
1. Search for Git repositories under each root
2. Validate each has an `envoy_env/` directory
3. Load commands from `envoy_env/commands.json`

### Config File (Alternative)

Create a `packages.json` file:

```json
{
  "packages": [
    {
      "root": "R:/repo/my-app",
      "name": "my-app"
    },
    {
      "root": "C:/tools/build-tools",
      "name": "build-tools"
    }
  ]
}
```

Use with:
```bash
envoy --packages-config packages.json --list
```

See [PACKAGE_DISCOVERY.md](py/gt/envoy/PACKAGE_DISCOVERY.md) for detailed information.

## Real-World Examples

### Example 1: Python Development Environment

**envoy_env/commands.json:**
```json
{
  "python_dev": {
    "environment": ["python_env.json"],
    "alias": ["python", "-X", "dev"]
  }
}
```

**envoy_env/python_env.json:**
```json
{
  "PYTHONPATH": "+=PATH:{$__PACKAGE__}/src",
  "PYTHONDONTWRITEBYTECODE": "1",
  "PYTHONUTF8": "1"
}
```

**Usage:**
```bash
envoy python_dev script.py
```

### Example 2: Unreal Engine Tools

**envoy_env/commands.json:**
```json
{
  "unreal": {
    "environment": ["unreal_env.json"]
  }
}
```

**envoy_env/unreal_env.json:**
```json
{
  "UE_ROOT": "{$__PACKAGE__}/UnrealEngine",
  "PATH": "+=PATH:{$UE_ROOT}/Engine/Binaries/Win64"
}
```

**Usage:**
```bash
envoy unreal -project MyGame.uproject
```

### Example 3: Multi-Package Build System

With `ENVOY_PKG_ROOTS=R:/repo` containing:
- `R:/repo/build-tools/envoy_env/commands.json` — Defines `build`, `test`
- `R:/repo/deploy-tools/envoy_env/commands.json` — Defines `deploy`, `package`

```bash
# List all commands from both packages
envoy --list

# Available commands:
#   build    [build-tools]
#   test     [build-tools]
#   deploy   [deploy-tools]
#   package  [deploy-tools]

# Run command from any package
envoy build --target Release
envoy deploy --env production
```

## CLI Reference

```
usage: envoy [-h] [--list] [--info COMMAND] [--which COMMAND]
             [--commands-file COMMANDS_FILE]
             [--packages-config PACKAGES_CONFIG] [--verbose]
             [command] [args ...]

Options:
  -h, --help            Show help message
  --list                List all available commands
  --info COMMAND        Show detailed information about a command
  --which COMMAND       Show the resolved executable path for a command
  --commands-file PATH  Path to commands.json (auto-detected by default)
  --packages-config PATH
                        Path to packages config file
  --verbose, -v         Enable verbose logging

Arguments:
  command               Command to execute
  args                  Arguments to pass to the command
```

See [CLI_USAGE.md](examples/CLI_USAGE.md) for detailed CLI documentation.

## Advanced Topics

### Command Conflicts

When multiple packages define the same command name, the last discovered package wins. Use `--verbose` to see conflict warnings:

```bash
envoy --verbose --list
# WARNING: Command 'build' from package-b overrides existing command from package-a
```

### Environment File Chaining

Environment files are loaded in order, with later files able to reference earlier variables:

```json
// base_env.json
{
  "APP_ROOT": "{$__PACKAGE__}"
}

// dev_env.json
{
  "APP_CONFIG": "{$APP_ROOT}/config/dev.json",
  "LOG_LEVEL": "DEBUG"
}
```

### Local Fallback

If no packages are discovered, Envoy searches for `envoy_env/commands.json` in the current directory and parent directories, allowing per-project command definitions.

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
│           ├── _discovery.py   # Package discovery
│           ├── _environment.py # Environment processing
│           ├── _wrapper.py     # Application wrapper
│           └── examples/
│               └── envoy_env/  # Example configurations
├── examples/
│   ├── CLI_USAGE.md
│   ├── packages.json           # Example package config
│   └── python_dev.bat          # Example wrapper script
└── README.md                   # This file
```

## Documentation

- **[CLI_USAGE.md](examples/CLI_USAGE.md)** — Detailed CLI usage guide
- **[PACKAGE_DISCOVERY.md](py/gt/envoy/PACKAGE_DISCOVERY.md)** — Package discovery system
- **[ENV_FILES_README.md](py/gt/envoy/examples/envoy_env/ENV_FILES_README.md)** — Environment file format
- **[CLI_IMPLEMENTATION_SUMMARY.md](py/gt/envoy/CLI_IMPLEMENTATION_SUMMARY.md)** — Implementation details

## Contributing

Envoy is part of the GT Tools collection. See `LICENSE` for details.

## Troubleshooting

**"Error: Could not find commands.json"**
- Ensure you're in a directory with `envoy_env/commands.json`, or
- Set `ENVOY_PKG_ROOTS` to point to package root directories, or
- Use `--packages-config` to specify a package configuration file

**Commands not appearing in --list**
- Check that packages have `envoy_env/` directories
- Use `--verbose` to see discovery debug information
- Verify Git repositories are valid (have `.git/` directory)

**Environment variables not applying**
- Check JSON syntax in environment files
- Use `--verbose` to see environment processing
- Verify paths use forward slashes: `{$__PACKAGE__}/src`

**Need help?**
Run with `--verbose` to see detailed logging of package discovery, command loading, and environment processing.
