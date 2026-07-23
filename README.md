# Envoy

**Environment orchestration for applications** — A CLI-first tool for managing complex application environments with JSON-based configuration and multi-bundle support.

## Key Features

- **CLI-First** — Simple command-line interface via `en`, `envoy`, and `engit`
- **Multi-Bundle** — Aggregate commands from multiple Git repositories
- **JSON Configuration** — Define environments with operators (`=`, `+=`, `^=`, `?=`), with `//`, `/* */`, and `#` comment support
- **Closed Environment** — Subprocesses receive only what you define, not the full system environment
- **Auto-Discovery** — Automatic, parallelized bundle detection via `ENVOY_BNDL_ROOTS`, with an on-disk discovery cache for fast repeat invocations
- **Path Normalization** — Automatic OS-native path handling
- **Null-Safe** — Undefined variables warn and skip rather than propagate empty values
- **Package Caching** — Local, content-addressed caching for production packages, resolved automatically alongside bundle discovery; on a cache miss, envoy fetches the package from the team's configured production package root and populates the cache automatically
- **Team & Pipeline Context** — Automatic `.envoy/team.json` and `.envoy/pipeline.json` resolution for team-scoped settings and context-aware pipelines
- **VCS Integration** — Auto-detects Git, Perforce, or [Lore](https://github.com/EpicGames/lore) working copies for status/change queries
- **Opt-In Telemetry** — Disabled by default; when enabled, exports usage events via OpenTelemetry/OTLP to any compatible collector
- **Diagnostics** — `envoy --diagnose [COMMAND]` reports discovered bundles, team/pipeline context, cache and VCS status, and full environment resolution in one place

## Quick Start

```powershell
# 1. Set your bundle roots
$env:ENVOY_BNDL_ROOTS = "R:/repo/gtvfx-contrib"

# 2. List available commands
envoy --list

# 3. Run a command
envoy unreal

# 4. Show command details
envoy --info unreal

# 5. Diagnose your environment (bundles, team/pipeline, cache, VCS, and more)
envoy --diagnose unreal
```

## Documentation

Full documentation: **[gtvfx-contrib.github.io/gt-envoy](https://gtvfx-contrib.github.io/gt-envoy/)**

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
