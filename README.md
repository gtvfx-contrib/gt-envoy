# Envoy

**Platform-agnostic environment orchestration for applications.** Envoy is a
CLI-first tool for managing complex application environments and multi-bundle
runtime Stacks on Windows, Linux, and macOS.

## Key Features

- **Cross-Platform** — Native `envoy` CLI plus the `envoy` Python API
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
[installation guide](https://gtvfx-envoy.github.io/envoy/installation/)
for platform-specific setup and security guidance.

## Documentation

Full documentation: **[gtvfx-envoy.github.io/envoy](https://gtvfx-envoy.github.io/envoy/)**

| Topic | Description |
|---|---|
| [Installation](https://gtvfx-envoy.github.io/envoy/installation/) | Platform archives and source builds |
| [Core Concepts](https://gtvfx-envoy.github.io/envoy/concepts/) | Bundles, commands, and platform overrides |
| [Environment Files](https://gtvfx-envoy.github.io/envoy/env-files/) | JSON environment format |
| [Bundle Discovery](https://gtvfx-envoy.github.io/envoy/bundle-discovery/) | Auto-discovery and Stack files |
| [CLI Reference](https://gtvfx-envoy.github.io/envoy/cli-reference/envoy/) | `envoy` commands |
| [Python API](https://gtvfx-envoy.github.io/envoy/reference/envoy/) | Scripting and Stack integration |
| [Troubleshooting](https://gtvfx-envoy.github.io/envoy/troubleshooting/) | Diagnostics and common issues |

Repository, release, and bundle-publishing commands are provided separately by
[Envoy Utils](https://github.com/gtvfx-envoy/envoy_utils).

## Contributing

Envoy is part of the GT Tools collection. See `LICENSE` for details.
