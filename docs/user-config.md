# User Configuration

Envoy stores per-user preferences in a persistent config file so you don't have
to repeat flags or paths on every invocation.

## Config file location

| OS | Path |
|----|------|
| Windows | `%APPDATA%\envoy\user_config.json` |
| macOS / Linux | `~/.config/envoy/user_config.json` |

The directory is created automatically the first time a setting is saved.
You can override the path for testing by setting the `ENVOY_USER_CONFIG`
environment variable.

## Managing settings

### Set a value

```powershell
envoy --set-config bundles_config=R:/studio/envoy/studio_bundles.json
```

### Clear a value

```powershell
envoy --set-config bundles_config=
```

(Empty value after `=` removes the setting.)

### View current settings

```powershell
# All settings
envoy --get-config

# One specific setting
envoy --get-config bundles_config
```

### List all configurable settings

```powershell
envoy --list-configs
```

This prints each setting name, its current value (or `not set`), a description,
and the allowed choices where applicable.  When `ENVOY_CFG_ROOTS` is set, it
also lists all available named configs.

### Bypass the config for one run

```powershell
envoy --ignore-config python_dev script.py
```

`--ignore-config` (`-ic`) skips the user config entirely for that invocation,
reverting to `ENVOY_BNDL_ROOTS` auto-discovery as if no config is set.

## Available settings

### `bundles_config`

Path **or name** of the default bundles config.

```powershell
# Point at a specific file
envoy --set-config bundles_config=R:/studio/envoy/bundles.json

# Reference a named config (resolved via ENVOY_CFG_ROOTS)
envoy --set-config bundles_config=studio
```

Once set, this is used automatically whenever `--bundles-config` is not
supplied on the command line.  See [Named configs](#named-configs) below.

### `verbosity`

Default verbosity level.  One of `quiet`, `normal`, or `verbose`.

```powershell
envoy --set-config verbosity=verbose
```

The `--verbose` (`-v`) flag always overrides this for the current run.

## Resolution priority

When envoy determines which bundles to load it follows this priority order:

```mermaid
flowchart TD
    A([envoy starts]) --> B{--bundles-config\nflag?}
    B -- Yes --> R1[Resolve path or named config]
    B -- No --> C{--ignore-config\nflag?}
    C -- Yes --> E[Auto-discover via\nENVOY_BNDL_ROOTS]
    C -- No --> D{User config\nbundles_config set?}
    D -- Yes --> R2[Resolve path or named config]
    D -- No --> E
    R1 --> F[Use that config file]
    R2 --> F
    E --> G[Local fallback\nenvoy_env/commands.json]
    F --> H([Commands ready])
    G --> H
```

## Named configs

A named config (e.g. `studio`) is a config slot stored under
`ENVOY_CFG_ROOTS` with versioned history and a `latest` pointer.  This lets
teams publish and update a shared bundles config that all users pull
automatically by name, without distributing a specific file path.

### How envoy tells names from paths

A value is treated as a **name** when it contains no path separator characters
(`/`, `\`, `:`) and does not start with a dot.  Everything else is a path.

```powershell
envoy --set-config bundles_config=studio          # name → resolved via ENVOY_CFG_ROOTS
envoy --set-config bundles_config=R:/my/f.json   # path → used directly
```

### `ENVOY_CFG_ROOTS`

Set this to one or more config root directories (semicolon-separated on Windows,
colon-separated on Unix).  Envoy scans each root for named config slots.

=== "PowerShell"

    ```powershell
    $env:ENVOY_CFG_ROOTS = "R:/studio/envoy/configs"
    ```

=== "cmd"

    ```batch
    set ENVOY_CFG_ROOTS=R:/studio/envoy/configs
    ```

=== "Unix/macOS"

    ```bash
    export ENVOY_CFG_ROOTS=/studio/envoy/configs
    ```

### Named config directory structure

```
R:\studio\envoy\configs\
└── studio\
    ├── 2026-06-21T10-13-00.json   ← versioned config
    ├── 2026-06-22T09-00-00.json   ← newer version
    └── latest                     ← text file: "2026-06-22T09-00-00.json"
```

Each version is a timestamped copy of a bundles-config JSON file.  The `latest`
file contains just the filename of the most recently published version.

### Publishing a named config

Use `engit publish-config` to publish a new version and update `latest`:

```powershell
engit publish-config studio R:/my/bundles.json
```

With an explicit config root (instead of using `ENVOY_CFG_ROOTS`):

```powershell
engit publish-config studio R:/my/bundles.json --cfg-root R:/studio/envoy/configs
```

Dry-run to preview without writing:

```powershell
engit publish-config studio R:/my/bundles.json --dry-run
```

### Listing available named configs

```powershell
envoy --list-configs
```

When `ENVOY_CFG_ROOTS` is set, `--list-configs` appends a table of all
available named configs with their current version and resolved path.

## Config file format

The user config file is plain JSON:

```json
{
  "bundles_config": "studio",
  "verbosity": "verbose"
}
```

You can edit it directly in any text editor — envoy reads it on every invocation.
