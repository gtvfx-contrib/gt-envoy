# Envoy

**Platform-agnostic environment orchestration for applications** — A CLI-first
tool for managing complex application environments with multi-bundle runtime
Stacks on Windows, Linux, and macOS.

## Key Features

- **CLI-First** — Simple command-line interface via `en` and `envoy`
- **Cross-Platform** — Native CLIs and Python wheels for Windows, Linux, and macOS
- **Portable Commands** — Optional OS and CPU overrides in one bundle configuration
- **Multi-Bundle** — Aggregate commands from multiple Git repositories
- **JSON Configuration** — Define environments with operators (`=`, `+=`, `^=`, `?=`), with `//`, `/* */`, and `#` comment support
- **Closed Environment** — Subprocesses receive only what you define, not the full system environment
- **Auto-Discovery** — Automatic, parallelized bundle detection via `ENVOY_BNDL_ROOTS`, with an on-disk discovery cache for fast repeat invocations
- **Path Normalization** — Automatic OS-native path handling
- **Null-Safe** — Undefined variables in env files warn and skip rather than propagate empty values
- **Bundle Caching** — Local, content-addressed caching for production bundles, resolved automatically alongside bundle discovery
- **Runtime Stacks** — Strict YAML `.estack` runtime containers with direct, named, and context-aware resolution
- **VCS Integration** — Auto-detects Git, Perforce, or [Lore](https://github.com/EpicGames/lore) working copies for status/change queries
- **Opt-In Telemetry** — Disabled by default; when enabled, exports usage events via OpenTelemetry/OTLP to any compatible collector
- **Diagnostics** — `envoy --diagnose [COMMAND]` reports discovered bundles, team/Stack context, cache and VCS status, and full environment resolution in one place

## Quick Start

```console
# List available commands
en --list

# Run a command
en python script.py

# Show command and target details
en --info python

# Diagnose the target, bundles, Stack, cache, VCS, and environment
en --diagnose python
```

## Documentation

Full documentation is available at **[gtvfx-envoy.github.io/envoy](https://gtvfx-envoy.github.io/envoy/)**.

| Topic | Description |
|---|---|
| [Installation](https://gtvfx-envoy.github.io/envoy/installation/) | Download and setup options |
| [Core Concepts](https://gtvfx-envoy.github.io/envoy/concepts/) | Bundles, commands, and env files |
| [Environment Files](https://gtvfx-envoy.github.io/envoy/env-files/) | JSON format reference |
| [Bundle Discovery](https://gtvfx-envoy.github.io/envoy/bundle-discovery/) | Auto-discovery and Stack files |
| [CLI Reference](https://gtvfx-envoy.github.io/envoy/cli-reference/envoy/) | `envoy` commands |
| [Python API](https://gtvfx-envoy.github.io/envoy/reference/envoy/) | Scripting and Stack integration |
| [Troubleshooting](https://gtvfx-envoy.github.io/envoy/troubleshooting/) | Common issues and fixes |

## Contributing

Envoy is part of the GT Tools collection. See `LICENSE` for details.
