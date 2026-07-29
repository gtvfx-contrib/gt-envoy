# Envoy

**Platform-agnostic environment orchestration for applications.** Envoy is a
CLI-first tool for managing complex application environments and multi-bundle
runtime Stacks on Windows, Linux, and macOS.

## Key Features

- **Cross-Platform** — Native `envoy` and `engit` CLIs plus the `envoy` Python API
- **Portable Commands** — Base definitions with optional OS and CPU overrides
- **Multi-Bundle** — Aggregate commands from multiple Git repositories
- **JSON Configuration** — Environment operators (`=`, `+=`, `^=`, `?=`) and comments
- **Closed Environment** — Subprocesses receive only explicitly selected variables
- **Auto-Discovery** — Parallel bundle detection with a persistent discovery cache
- **Native Paths** — Platform-aware separators, executable lookup, and config locations
- **Runtime Stacks** — Strict YAML `.estack` containers with contextual resolution
- **VCS Integration** — Git, Perforce, and [Lore](https://github.com/EpicGames/lore)
- **Diagnostics** — Inspect the target, bundles, Stack, cache, VCS, and resolved environment
- **Opt-In Telemetry** — Disabled by default; exports through OpenTelemetry/OTLP

## Quick Start

After installing the archive for your platform and adding its `bin` directory
to `PATH`:

```console
envoy --list
envoy --info python
envoy --diagnose python
envoy python script.py
```

The short `en` alias can be used anywhere `envoy` appears above.

## Supported Targets

| Surface | Targets |
|---|---|
| Native CLIs | Windows x64, Linux x64 (musl), macOS x64, macOS arm64 |
| Python wheel | Windows x64, manylinux2014 x64, macOS x64, macOS arm64 |

Release artifacts are checksummed but currently unsigned. See the
[installation guide](https://gtvfx-contrib.github.io/gt-envoy/installation/)
for platform-specific setup and security guidance.

## Documentation

Full documentation: **[gtvfx-contrib.github.io/gt-envoy](https://gtvfx-contrib.github.io/gt-envoy/)**

| Topic | Description |
|---|---|
| [Installation](https://gtvfx-contrib.github.io/gt-envoy/installation/) | Platform archives and source builds |
| [Core Concepts](https://gtvfx-contrib.github.io/gt-envoy/concepts/) | Bundles, commands, and platform overrides |
| [Environment Files](https://gtvfx-contrib.github.io/gt-envoy/env-files/) | JSON environment format |
| [Bundle Discovery](https://gtvfx-contrib.github.io/gt-envoy/bundle-discovery/) | Auto-discovery and Stack files |
| [CLI Reference](https://gtvfx-contrib.github.io/gt-envoy/cli-reference/envoy/) | `envoy` and `engit` commands |
| [Python API](https://gtvfx-contrib.github.io/gt-envoy/reference/envoy/) | Scripting and Stack integration |
| [Troubleshooting](https://gtvfx-contrib.github.io/gt-envoy/troubleshooting/) | Diagnostics and common issues |

## Contributing

Envoy is part of the GT Tools collection. See `LICENSE` for details.
