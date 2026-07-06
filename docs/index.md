# Envoy

**Environment orchestration for applications** — A CLI-first tool for managing complex application environments with JSON-based configuration and multi-bundle support.

## Key Features

- **CLI-First** — Simple command-line interface via `en`, `envoy`, and `engit`
- **Multi-Bundle** — Aggregate commands from multiple Git repositories
- **JSON Configuration** — Define environments with operators (`=`, `+=`, `^=`, `?=`)
- **Closed Environment** — Subprocesses receive only what you define, not the full system environment
- **Auto-Discovery** — Automatic bundle detection via `ENVOY_BNDL_ROOTS`
- **Path Normalization** — Automatic OS-native path handling
- **Null-Safe** — Undefined variables in env files warn and skip rather than propagate empty values

## Quick Start

```powershell
# 1. Set your bundle roots
$env:ENVOY_BNDL_ROOTS = "R:/repo/gtvfx-contrib"

# 2. List available commands
en --list

# 3. Run a command
en python_dev script.py

# 4. Show command details
en --info python_dev
```

## Documentation

Full documentation is available at **[gtvfx-contrib.github.io/gt-envoy](https://gtvfx-contrib.github.io/gt-envoy/)**.

| Topic | Description |
|---|---|
| [Installation](https://gtvfx-contrib.github.io/gt-envoy/installation/) | Download and setup options |
| [Core Concepts](https://gtvfx-contrib.github.io/gt-envoy/concepts/) | Bundles, commands, and env files |
| [Environment Files](https://gtvfx-contrib.github.io/gt-envoy/env-files/) | JSON format reference |
| [Bundle Discovery](https://gtvfx-contrib.github.io/gt-envoy/bundle-discovery/) | Auto-discovery and config files |
| [CLI Reference](https://gtvfx-contrib.github.io/gt-envoy/cli-reference/envoy/) | `envoy` and `engit` commands |
| [Python API](https://gtvfx-contrib.github.io/gt-envoy/reference/envoy/) | Scripting and pipeline integration |
| [Troubleshooting](https://gtvfx-contrib.github.io/gt-envoy/troubleshooting/) | Common issues and fixes |

## Contributing

Envoy is part of the GT Tools collection. See `LICENSE` for details.
